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
//! - [`unrelated_ca_bundle`] is the OTHER half of keeping these tests hermetic: a
//!   verification-failure test that lets libcurl fall back to its baked-in default CA path
//!   answers a different `CURLcode` depending on whether that path exists on the machine.
//!   Handing libcurl a real bundle that simply does not contain the fixture's certificate
//!   removes the build machine from the question.
//! - NO SYSTEM CA STORE IS INVOLVED ON EITHER SIDE. The success case turns verification OFF and
//!   the failure case leaves it ON and expects rejection, so these fixtures run
//!   identically on a machine with no system trust store — which is what keeps them
//!   independent of both the managed curl build's build-time CA autodetection and the
//!   bridge's runtime CA discovery (`crates/elephc-curl/src/ca.rs`). `easy_ca.rs` is the
//!   file that DOES depend on those, and it reuses this same server and bundle.

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

/// A self-signed certificate that is a perfectly VALID trust anchor and has nothing to do
/// with [`LocalHttpsServer`]'s certificate. Only the certificate is kept — a trust anchor
/// needs no private key — so this cannot accidentally become a second server identity.
///
/// `CN=elephc-unrelated-test-ca`, valid to 2046.
const UNRELATED_CA_PEM: &str = "\
-----BEGIN CERTIFICATE-----
MIIDJzCCAg+gAwIBAgIUB1It8X7fDToZ4Ows+a8anwso3kUwDQYJKoZIhvcNAQEL
BQAwIzEhMB8GA1UEAwwYZWxlcGhjLXVucmVsYXRlZC10ZXN0LWNhMB4XDTI2MDgx
MzEyNTkzMloXDTQ2MDgwODEyNTkzMlowIzEhMB8GA1UEAwwYZWxlcGhjLXVucmVs
YXRlZC10ZXN0LWNhMIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEA3NKm
ZdpQJk0yv5EBAtRgsRHGrRWraS7H4JAWixsHvlDVAHB8uxGcGIpWjSPM+HurdBQy
ZnxScj7Mt0YCOSLbduSRko4Iyzm+wC5vdTceDCRO0Escc8tDkOYTS2HMvhJfrgSu
1pdTTnYlXILzDhvqGS8zJZRaCole5PVc17VfhaOhWvb5+UDleNRMMiR3YjdcZjMg
Ag4Ly678VMWKt4d7ONBEd/biIiFh5z+wrS2GkEQrtzsyUpyITBTMLmyT7YdbHV08
TVh53lNf59jAlvDxKoQ817MQwceXiA6i/6fTWWvTJc+YuHnpT7ggWHp1HA6geNT+
eeSp5CctaUzNRWZXeQIDAQABo1MwUTAdBgNVHQ4EFgQUh7M+SH+eMh6xYl5vMnyz
/oq1vqcwHwYDVR0jBBgwFoAUh7M+SH+eMh6xYl5vMnyz/oq1vqcwDwYDVR0TAQH/
BAUwAwEB/zANBgkqhkiG9w0BAQsFAAOCAQEAOKW1N2xBy0vmvWT3/glGPPssdJ3s
QXKk9n64U0EeAEAZjyv2PaPt47Q/zzUbI9EnaF9W7nwCtToaoa52FPV5v78uOEK0
VBUxrxQdDtydUlur2OKomjx6Aj21YW+Ip0IvmEgifZmlpm42aILg6aTtou3NGHpY
a0+mQZjVQI4hhqb5WHOnya+Gq3csYSx5oXGo9I2Le+m7rA6xwI19PgFsfGXn+pz3
D2+RSrABs5YpGKPEG8l3Ca5igI3tas/yUNslJXni7N1blKg2nf2nmDumHRV56lsC
VnNfCxoR32CQNuilos6yhoJZRf4jjSdEllMvBQrq2Rixx1N8EZcMgR9QlQ==
-----END CERTIFICATE-----
";

/// Writes [`UNRELATED_CA_PEM`] to a temp file once per process and returns its path, for
/// `CURLOPT_CAINFO`.
///
/// This is what makes the verification-FAILURE test deterministic. Without it libcurl
/// falls back to whatever CA path was compiled into the managed build, and the resulting
/// `CURLcode` depends on whether that path exists on THIS machine: `60`
/// (`CURLE_PEER_FAILED_VERIFICATION`) when the bundle is readable and simply does not
/// contain the fixture's certificate, `77` (`CURLE_SSL_CACERT_BADFILE`) when it is not
/// there at all. A bundle the test itself supplies is always readable and never contains
/// the fixture's certificate, so the answer is always 60.
pub(crate) fn unrelated_ca_bundle() -> std::path::PathBuf {
    use std::sync::OnceLock;
    static PATH: OnceLock<std::path::PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let path = std::env::temp_dir().join(format!(
            "elephc-curl-unrelated-ca-{}.pem",
            std::process::id()
        ));
        std::fs::write(&path, UNRELATED_CA_PEM).expect("curl tls fixture: write CA bundle");
        path
    })
    .clone()
}
