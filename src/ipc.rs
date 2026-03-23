use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
struct ClientMessage {
    scrollable: bool,
}

#[derive(Debug, Serialize)]
struct ServerMessage {
    event: String,
}

struct Client {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
}

pub struct IpcServer {
    listener: UnixListener,
    clients: Vec<Client>,
    socket_path: PathBuf,
    pub scrollable: bool,
}

impl IpcServer {
    pub fn new() -> Self {
        let socket_path = socket_path();

        // Clean up stale socket from previous run
        let _ = std::fs::remove_file(&socket_path);

        let listener = UnixListener::bind(&socket_path)
            .unwrap_or_else(|e| panic!("Failed to bind socket at {}: {e}", socket_path.display()));
        listener.set_nonblocking(true).expect("Failed to set listener non-blocking");

        log::info!("IPC server listening on {}", socket_path.display());

        Self {
            listener,
            clients: Vec::new(),
            socket_path,
            scrollable: false,
        }
    }

    /// Non-blocking poll for new connections and incoming messages.
    /// Returns true if scrollable state changed.
    pub fn poll(&mut self) -> bool {
        self.accept_new_clients();
        self.read_messages()
    }

    /// Send a message to all connected clients.
    pub fn broadcast_close(&mut self) {
        let msg = serde_json::to_string(&ServerMessage {
            event: "close".into(),
        })
        .expect("Failed to serialize close message");

        let line = format!("{msg}\n");
        log::info!("Broadcasting close to {} client(s)", self.clients.len());
        self.clients.retain_mut(|client| {
            match client.writer.write_all(line.as_bytes()).and_then(|_| client.writer.flush()) {
                Ok(_) => true,
                Err(e) => {
                    log::debug!("Client disconnected on write: {e}");
                    false
                }
            }
        });
    }

    fn accept_new_clients(&mut self) {
        loop {
            match self.listener.accept() {
                Ok((stream, _addr)) => {
                    log::info!("IPC client connected");
                    let writer = stream.try_clone().expect("Failed to clone stream");
                    stream.set_nonblocking(true).expect("Failed to set stream non-blocking");
                    self.clients.push(Client {
                        reader: BufReader::new(stream),
                        writer,
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => {
                    log::error!("Failed to accept IPC client: {e}");
                    break;
                }
            }
        }
    }

    /// Read messages from all clients. Returns true if scrollable changed.
    fn read_messages(&mut self) -> bool {
        let mut changed = false;
        let mut disconnected = Vec::new();

        for (i, client) in self.clients.iter_mut().enumerate() {
            let mut line = String::new();
            loop {
                line.clear();
                match client.reader.read_line(&mut line) {
                    Ok(0) => {
                        log::info!("IPC client disconnected");
                        disconnected.push(i);
                        break;
                    }
                    Ok(_) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        match serde_json::from_str::<ClientMessage>(trimmed) {
                            Ok(msg) => {
                                if self.scrollable != msg.scrollable {
                                    log::debug!("Scrollable state changed: {}", msg.scrollable);
                                    self.scrollable = msg.scrollable;
                                    changed = true;
                                }
                            }
                            Err(e) => {
                                log::warn!("Invalid IPC message: {e}: {trimmed}");
                            }
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(e) => {
                        log::error!("Error reading from IPC client: {e}");
                        disconnected.push(i);
                        break;
                    }
                }
            }
        }

        // Remove disconnected clients in reverse order to maintain indices
        for i in disconnected.into_iter().rev() {
            self.clients.swap_remove(i);
        }

        changed
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

fn socket_path() -> PathBuf {
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        PathBuf::from(runtime_dir).join("edgeswipe.sock")
    } else {
        PathBuf::from("/tmp/edgeswipe.sock")
    }
}
