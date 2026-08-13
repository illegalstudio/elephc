//! Purpose:
//! A loopback HTTPS server for the Wave E `ext/curl` fixtures: the same self-signed
//! certificate `tests/codegen/io/streams.rs` presents, served over rustls on an ephemeral
//! port, answering `GET /secure` with `hello-tls`.
//!
//! Called from:
//! - `tests/codegen/curl/easy_tls.rs`.
//!
//! Key details:
//! - IT REUSES `streams.rs`'s CERTIFICATE, imported rather than copied. A second
//!   self-signed certificate would be a second expiry date to renew and a second thing to
//!   get wrong; there is exactly one local-TLS identity in this test binary.
//! - IT IS DELIBERATELY A LOOP, not the one-shot `spawn_https_server` next to that
//!   certificate: Wave E's tests make SEVERAL connections against one server — a
//!   successful one and one or more that fail during the handshake — and a one-shot
//!   listener would leave the later ones connecting to a closed port, which is a different
//!   error from the certificate rejection being tested.
//! - EVERY ERROR IS SWALLOWED. A `CURLOPT_SSL_VERIFYPEER=true` client aborts the handshake
//!   partway through, which surfaces here as a failed `read`/`write`. That is the expected
//!   outcome of a test, not a fixture fault, so the accept loop logs nothing and simply
//!   moves on to the next connection.
//! - NO CA STORE IS INVOLVED ON EITHER SIDE. The success case turns verification OFF and
//!   the failure case leaves it ON and expects rejection, so these fixtures run
//!   identically on a machine with no system trust store — which is what keeps them
//!   independent of the managed curl build's (currently non-hermetic) CA autodetection.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;

use crate::codegen::io::streams::{TEST_HTTPS_CERT_PEM, TEST_HTTPS_KEY_PEM};

/// A running loopback HTTPS server plus the port it bound.
pub(crate) struct LocalHttpsServer {
    port: u16,
}

impl LocalHttpsServer {
    /// Binds `127.0.0.1:0` and serves `GET /secure` over TLS on a background thread.
    pub(crate) fn spawn() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("curl tls fixture: bind port");
        let port = listener
            .local_addr()
            .expect("curl tls fixture: local addr")
            .port();

        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut cert_reader = TEST_HTTPS_CERT_PEM.as_bytes();
        let certs = rustls_pemfile::certs(&mut cert_reader)
            .collect::<Result<Vec<_>, _>>()
            .expect("curl tls fixture: parse cert");
        let mut key_reader = TEST_HTTPS_KEY_PEM.as_bytes();
        let key = rustls_pemfile::private_key(&mut key_reader)
            .expect("curl tls fixture: parse private key")
            .expect("curl tls fixture: private key present");
        let config = Arc::new(
            rustls::ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(certs, key)
                .expect("curl tls fixture: build server config"),
        );

        std::thread::spawn(move || loop {
            let Ok((tcp, _)) = listener.accept() else {
                return;
            };
            let config = Arc::clone(&config);
            std::thread::spawn(move || {
                let _ = tcp.set_read_timeout(Some(std::time::Duration::from_secs(5)));
                let Ok(conn) = rustls::ServerConnection::new(config) else {
                    return;
                };
                let mut tls = rustls::StreamOwned::new(conn, tcp);
                let mut request = [0u8; 2048];
                // A rejected-certificate client aborts here; that is a passing test, not a
                // fixture failure, so the result is discarded either way.
                if tls.read(&mut request).is_err() {
                    return;
                }
                let body = b"hello-tls";
                let headers = format!(
                    "HTTP/1.0 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n",
                    body.len()
                );
                let _ = tls.write_all(headers.as_bytes());
                let _ = tls.write_all(body);
                let _ = tls.flush();
            });
        });
        Self { port }
    }

    /// Builds `https://127.0.0.1:<port><path>` for this server.
    pub(crate) fn url(&self, path: &str) -> String {
        format!("https://127.0.0.1:{}{}", self.port, path)
    }
}
