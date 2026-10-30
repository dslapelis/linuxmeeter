//! Private local control transport. The PipeWire thread remains the state owner.
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use lm_protocol::{ControlRequest, ControlResponse, EngineCommand};

const MAX_MESSAGE: usize = 16 * 1024;

pub struct ControlBridge {
    sender: pipewire::channel::Sender<EngineCommand>,
    next: AtomicU64,
    pending: Mutex<HashMap<u64, mpsc::SyncSender<ControlResponse>>>,
    closed: AtomicBool,
}

impl ControlBridge {
    pub fn new(sender: pipewire::channel::Sender<EngineCommand>) -> Arc<Self> {
        Arc::new(Self {
            sender,
            next: AtomicU64::new(1),
            pending: Mutex::new(HashMap::new()),
            closed: AtomicBool::new(false),
        })
    }

    pub fn request(&self, mut request: ControlRequest) -> ControlResponse {
        let client_id = request.id;
        let key = self.next.fetch_add(1, Ordering::Relaxed);
        request.id = key;
        let (tx, rx) = mpsc::sync_channel(1);
        {
            let mut pending = self.pending.lock().unwrap();
            if self.closed.load(Ordering::Acquire) {
                return failure(client_id, "mixer is stopping");
            }
            pending.insert(key, tx);
        }
        let result = if self.sender.send(EngineCommand::Control(request)).is_err() {
            failure(client_id, "audio engine unavailable")
        } else {
            match rx.recv_timeout(Duration::from_secs(2)) {
                Ok(mut reply) => {
                    reply.id = client_id;
                    reply
                }
                Err(_) => failure(client_id, "audio engine did not acknowledge the request"),
            }
        };
        self.pending.lock().unwrap().remove(&key);
        result
    }

    pub fn complete(&self, response: ControlResponse) {
        if let Some(tx) = self.pending.lock().unwrap().remove(&response.id) {
            let _ = tx.send(response);
        }
    }

    pub fn stop(&self) {
        self.closed.store(true, Ordering::Release);
        self.pending.lock().unwrap().clear();
    }
}

pub fn failure(id: u64, error: impl Into<String>) -> ControlResponse {
    ControlResponse {
        id,
        ok: false,
        state: None,
        error: Some(error.into()),
    }
}

pub fn socket_path() -> Result<PathBuf, String> {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .map(|p| p.join("linuxmeeter/control.sock"))
        .ok_or_else(|| "XDG_RUNTIME_DIR is not set".to_string())
}

pub struct ControlServer {
    path: PathBuf,
    stopping: Arc<AtomicBool>,
    listener: Mutex<Option<std::thread::JoinHandle<()>>>,
    identity: (u64, u64),
}

impl ControlServer {
    pub fn start<F>(path: PathBuf, handler: F) -> Result<Self, String>
    where
        F: Fn(ControlRequest) -> ControlResponse + Send + Sync + 'static,
    {
        let parent = path.parent().ok_or("missing socket directory")?;
        let runtime = parent.parent().ok_or("missing runtime directory")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let meta = fs::symlink_metadata(parent).map_err(|e| e.to_string())?;
        let owner = fs::metadata(runtime).map_err(|e| e.to_string())?.uid();
        if !meta.is_dir() || meta.uid() != owner {
            return Err("control directory ownership mismatch".into());
        }
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
        if let Ok(meta) = fs::symlink_metadata(&path) {
            if !meta.file_type().is_socket() || meta.uid() != owner {
                return Err("refusing to replace control socket path".into());
            }
            match UnixStream::connect(&path) {
                Ok(_) => return Err("another linuxmeeter control server is running".into()),
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound
                    ) => {}
                Err(e) => return Err(format!("cannot check existing control socket: {e}")),
            }
            fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        let listener = UnixListener::bind(&path).map_err(|e| e.to_string())?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        let identity = (meta.dev(), meta.ino());
        let stopping = Arc::new(AtomicBool::new(false));
        let stop = stopping.clone();
        let handler = Arc::new(handler);
        let active = Arc::new(AtomicUsize::new(0));
        let listener_thread = std::thread::Builder::new()
            .name("lm-control-listen".into())
            .spawn(move || {
                while !stop.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            if active.fetch_add(1, Ordering::AcqRel) >= 8 {
                                active.fetch_sub(1, Ordering::AcqRel);
                                let _ = stream.set_write_timeout(Some(Duration::from_millis(100)));
                                let _ =
                                    write_response(&mut stream, &failure(0, "control server busy"));
                                continue;
                            }
                            let handler = handler.clone();
                            let count = active.clone();
                            let spawned = std::thread::Builder::new()
                                .name("lm-control-client".into())
                                .spawn(move || {
                                    let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
                                    let response = match read_request(&stream) {
                                        Ok(request) => handler(request),
                                        Err(error) => failure(0, error),
                                    };
                                    let _ = write_response(&mut stream, &response);
                                    count.fetch_sub(1, Ordering::AcqRel);
                                });
                            if spawned.is_err() {
                                active.fetch_sub(1, Ordering::AcqRel);
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(5))
                        }
                        Err(e) => {
                            tracing::warn!("control accept failed: {e}");
                            break;
                        }
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            path,
            stopping,
            listener: Mutex::new(Some(listener_thread)),
            identity,
        })
    }

    pub fn stop(&self) {
        if self.stopping.swap(true, Ordering::AcqRel) {
            return;
        }
        if let Some(thread) = self.listener.lock().unwrap().take() {
            let _ = thread.join();
        }
        if fs::symlink_metadata(&self.path)
            .is_ok_and(|meta| (meta.dev(), meta.ino()) == self.identity)
        {
            let _ = fs::remove_file(&self.path);
        }
    }
}

impl Drop for ControlServer {
    fn drop(&mut self) {
        self.stop();
    }
}

fn read_request(stream: &UnixStream) -> Result<ControlRequest, String> {
    let mut reader = BufReader::new(stream);
    let mut bytes = Vec::new();
    let deadline = Instant::now() + Duration::from_millis(500);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("request timed out".into());
        }
        stream
            .set_read_timeout(Some(remaining))
            .map_err(|e| e.to_string())?;
        let available = reader.fill_buf().map_err(|e| e.to_string())?;
        if available.is_empty() {
            return Err("request needs a terminating newline".into());
        }
        let end = available.iter().position(|b| *b == b'\n').map(|i| i + 1);
        let length = end.unwrap_or(available.len());
        if bytes.len() + length > MAX_MESSAGE {
            return Err("request exceeds 16 KiB".into());
        }
        bytes.extend_from_slice(&available[..length]);
        reader.consume(length);
        if end.is_some() {
            break;
        }
    }
    serde_json::from_slice(&bytes).map_err(|e| format!("invalid control request: {e}"))
}

fn write_response(stream: &mut UnixStream, response: &ControlResponse) -> std::io::Result<()> {
    let mut bytes = serde_json::to_vec(response).map_err(std::io::Error::other)?;
    bytes.push(b'\n');
    stream.write_all(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    struct Runtime(PathBuf);

    impl Runtime {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "lm-control-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
            Self(path)
        }

        fn socket(&self) -> PathBuf {
            self.0.join("linuxmeeter/control.sock")
        }
    }

    impl Drop for Runtime {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn test_server(path: PathBuf) -> Result<ControlServer, String> {
        ControlServer::start(path, |request| failure(request.id, "test handler"))
    }

    #[test]
    fn server_is_private_and_shutdown_does_not_remove_a_successor() {
        let runtime = Runtime::new();
        let path = runtime.socket();
        let server = test_server(path.clone()).unwrap();
        assert_eq!(
            fs::metadata(path.parent().unwrap()).unwrap().mode() & 0o777,
            0o700
        );
        assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
        let mut client = UnixStream::connect(&path).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        client
            .write_all(b"{\"id\":42,\"command\":{\"op\":\"get_state\"}}\n")
            .unwrap();
        let mut line = String::new();
        BufReader::new(client).read_line(&mut line).unwrap();
        let reply: ControlResponse = serde_json::from_str(&line).unwrap();
        assert_eq!(reply.id, 42);
        server.stop();
        assert!(!path.exists());
        let successor = test_server(path.clone()).unwrap();
        server.stop();
        drop(server);
        assert!(path.exists());
        successor.stop();
        assert!(!path.exists());
    }

    #[test]
    fn an_active_server_is_not_replaced() {
        let runtime = Runtime::new();
        let server = test_server(runtime.socket()).unwrap();
        assert!(
            matches!(test_server(runtime.socket()), Err(error) if error.contains("already") || error.contains("another"))
        );
        assert!(UnixStream::connect(runtime.socket()).is_ok());
        server.stop();
    }

    #[test]
    fn stale_socket_is_recovered_but_a_regular_file_is_preserved() {
        let runtime = Runtime::new();
        let path = runtime.socket();
        fs::create_dir(path.parent().unwrap()).unwrap();
        drop(UnixListener::bind(&path).unwrap());
        let server = test_server(path.clone()).unwrap();
        server.stop();
        fs::write(&path, b"keep me").unwrap();
        assert!(test_server(path.clone()).is_err());
        assert_eq!(fs::read(path).unwrap(), b"keep me");
    }

    #[test]
    fn symlink_directory_is_rejected_without_changing_its_target() {
        let runtime = Runtime::new();
        let target = runtime.0.join("target");
        fs::create_dir(&target).unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o750)).unwrap();
        std::os::unix::fs::symlink(&target, runtime.0.join("linuxmeeter")).unwrap();
        assert!(test_server(runtime.socket()).is_err());
        assert_eq!(fs::metadata(target).unwrap().mode() & 0o777, 0o750);
    }

    #[test]
    fn incomplete_request_times_out_and_the_server_remains_usable() {
        let runtime = Runtime::new();
        let server = test_server(runtime.socket()).unwrap();
        let mut client = UnixStream::connect(runtime.socket()).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        client.write_all(b"{\"id\":1").unwrap();
        let mut line = String::new();
        BufReader::new(client).read_line(&mut line).unwrap();
        let reply: ControlResponse = serde_json::from_str(&line).unwrap();
        assert!(!reply.ok);
        assert_eq!(reply.id, 0);
        assert!(UnixStream::connect(runtime.socket()).is_ok());
        server.stop();
    }

    #[test]
    fn fragmented_request_is_reassembled() {
        let (mut client, server) = UnixStream::pair().unwrap();
        let thread = std::thread::spawn(move || read_request(&server).unwrap());
        client.write_all(b"{\"id\":9,").unwrap();
        client
            .write_all(b"\"command\":{\"op\":\"get_state\"}}\n")
            .unwrap();
        assert_eq!(thread.join().unwrap().id, 9);
    }

    #[test]
    fn oversized_request_is_rejected_without_waiting_for_newline() {
        let (mut client, server) = UnixStream::pair().unwrap();
        let thread = std::thread::spawn(move || read_request(&server));
        client.write_all(&vec![b'x'; MAX_MESSAGE + 1]).unwrap();
        assert!(thread.join().unwrap().unwrap_err().contains("16 KiB"));
    }

    #[test]
    fn invalid_json_and_operations_return_errors() {
        for bytes in [
            b"not json\n".as_slice(),
            b"{\"id\":1,\"command\":{\"op\":\"shutdown\"}}\n".as_slice(),
        ] {
            let (mut client, server) = UnixStream::pair().unwrap();
            client.write_all(bytes).unwrap();
            assert!(read_request(&server).is_err());
        }
    }

    #[test]
    fn response_is_newline_delimited() {
        let (mut server, mut client) = UnixStream::pair().unwrap();
        write_response(&mut server, &failure(42, "test")).unwrap();
        drop(server);
        let mut bytes = String::new();
        client.read_to_string(&mut bytes).unwrap();
        assert!(bytes.ends_with('\n'));
        let response: ControlResponse = serde_json::from_str(&bytes).unwrap();
        assert_eq!(response.id, 42);
        assert!(!response.ok);
    }
}
