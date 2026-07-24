//! Purpose:
//! Opens TCP listeners/streams, accepted connections, and Unix socket pairs for
//! eval stream builtins.
//!
//! Called from:
//! - Socket-related filesystem and network builtins through `EvalStreamResources`.
//!
//! Key details:
//! - Socket names are captured when handles enter the shared resource table.

use super::*;

impl EvalStreamResources {

    /// Opens a TCP listener resource for `stream_socket_server()`.
    pub(crate) fn open_tcp_listener(&mut self, address: &str) -> Option<i64> {
        let listener = TcpListener::bind(eval_tcp_address(address)).ok()?;
        let local = listener.local_addr().ok()?.to_string();
        let id = self.next_id;
        self.next_id += 1;
        self.socket_names.insert(
            id,
            EvalSocketNames {
                local,
                peer: None,
            },
        );
        self.socket_listeners.insert(id, listener);
        Some(id)
    }

    /// Opens a connected TCP stream resource.
    pub(crate) fn open_tcp_stream(&mut self, address: &str) -> Option<i64> {
        self.open_tcp_stream_result(address).ok()
    }

    /// Opens a connected TCP stream resource and preserves the host I/O error on failure.
    pub(crate) fn open_tcp_stream_result(&mut self, address: &str) -> io::Result<i64> {
        let stream = TcpStream::connect(eval_tcp_address(address))?;
        self.insert_tcp_stream(stream).ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "failed to track eval TCP stream")
        })
    }

    /// Opens a connected TCP stream from separate host and port arguments.
    pub(crate) fn open_tcp_stream_host_port(&mut self, host: &str, port: i64) -> Option<i64> {
        self.open_tcp_stream_host_port_result(host, port).ok()
    }

    /// Opens a connected TCP stream from host and port while preserving I/O errors.
    pub(crate) fn open_tcp_stream_host_port_result(
        &mut self,
        host: &str,
        port: i64,
    ) -> io::Result<i64> {
        let host = host
            .strip_prefix("tcp://")
            .or_else(|| host.strip_prefix("ssl://"))
            .or_else(|| host.strip_prefix("tls://"))
            .unwrap_or(host);
        self.open_tcp_stream_result(&format!("{host}:{port}"))
    }

    /// Accepts one TCP connection from a listener resource.
    pub(crate) fn accept_tcp(&mut self, id: i64) -> Option<i64> {
        let listener = self.socket_listeners.get(&id)?;
        let (stream, _) = listener.accept().ok()?;
        self.insert_tcp_stream(stream)
    }

    /// Opens a pair of connected local stream resources.
    pub(crate) fn open_socket_pair(&mut self) -> Option<(i64, i64)> {
        #[cfg(unix)]
        {
            let (left, right) = UnixStream::pair().ok()?;
            let left = unsafe {
                // The UnixStream endpoint is moved into the File-backed eval stream.
                File::from_raw_fd(left.into_raw_fd())
            };
            let right = unsafe {
                // The UnixStream endpoint is moved into the File-backed eval stream.
                File::from_raw_fd(right.into_raw_fd())
            };
            let left_id = self.insert(EvalFileStream::new(
                left,
                "socketpair".to_string(),
                "r+".to_string(),
            ));
            let right_id = self.insert(EvalFileStream::new(
                right,
                "socketpair".to_string(),
                "r+".to_string(),
            ));
            self.socket_names.insert(
                left_id,
                EvalSocketNames {
                    local: "socketpair".to_string(),
                    peer: Some("socketpair".to_string()),
                },
            );
            self.socket_names.insert(
                right_id,
                EvalSocketNames {
                    local: "socketpair".to_string(),
                    peer: Some("socketpair".to_string()),
                },
            );
            Some((left_id, right_id))
        }
        #[cfg(windows)]
        {
            let listener = TcpListener::bind("127.0.0.1:0").ok()?;
            let address = listener.local_addr().ok()?;
            let left = TcpStream::connect(address).ok()?;
            let (right, _) = listener.accept().ok()?;
            let left_id = self.insert_tcp_stream(left)?;
            let right_id = self.insert_tcp_stream(right)?;
            Some((left_id, right_id))
        }
    }

}
