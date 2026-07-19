#![cfg(feature = "rustls")]

use std::{
    fs::File,
    io::{self, Read},
    sync::Arc,
};

use bufstream::BufStream;
use rustls::{
    client::{
        danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
        WebPkiServerVerifier,
    },
    pki_types::{CertificateDer, ServerName, UnixTime},
    CertificateError, ClientConfig, Error, RootCertStore, SignatureScheme,
};
use rustls_pemfile::certs;

use crate::{
    error::tls::TlsError,
    io::{Stream, TcpStream},
    Result, SslOpts,
};

impl Stream {
    pub fn make_secure(self, host: url::Host, ssl_opts: SslOpts) -> Result<Stream> {
        if self.is_socket() {
            // won't secure socket connection
            return Ok(self);
        }

        let domain = match host {
            url::Host::Domain(domain) => domain,
            url::Host::Ipv4(ip) => ip.to_string(),
            url::Host::Ipv6(ip) => ip.to_string(),
        };

        let mut root_store = RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().map(|x| x.to_owned()));

        if let Some(root_cert_path) = ssl_opts.root_cert_path() {
            let mut root_cert_data = vec![];
            let mut root_cert_file = File::open(root_cert_path)?;
            root_cert_file.read_to_end(&mut root_cert_data)?;

            let mut root_certs = Vec::new();
            for cert in certs(&mut &*root_cert_data) {
                root_certs.push(cert?);
            }

            if root_certs.is_empty() && !root_cert_data.is_empty() {
                root_certs.push(CertificateDer::from(root_cert_data));
            }

            for cert in &root_certs {
                root_store.add(cert.to_owned())?;
            }
        }

        let mut provider = rustls::crypto::ring::default_provider();
        if let Some(requested) = ssl_opts.cipher_suites() {
            provider.cipher_suites.retain(|suite| {
                let iana = format!("{:?}", suite.suite());
                requested.iter().any(|name| cipher_name_matches(name, &iana))
            });
            if provider.cipher_suites.is_empty() {
                return Err(TlsError::Tls(rustls::Error::General(
                    "no requested MySQL TLS cipher is supported by rustls".into(),
                )).into());
            }
        }
        let config_builder = ClientConfig::builder_with_provider(Arc::new(provider))
            .with_safe_default_protocol_versions()
            .map_err(TlsError::from)?
            .with_root_certificates(root_store.clone());

        let mut config = if let Some(identity) = ssl_opts.client_identity() {
            let (cert_chain, priv_key) = identity.load()?;
            config_builder.with_client_auth_cert(cert_chain, priv_key)?
        } else {
            config_builder.with_no_client_auth()
        };

        let server_name = ServerName::try_from(domain.as_str())
            .map_err(|_| webpki::InvalidDnsNameError)?
            .to_owned();
        let mut dangerous = config.dangerous();
        let web_pki_verifier = WebPkiServerVerifier::builder(Arc::new(root_store))
            .build()
            .map_err(TlsError::from)?;
        let dangerous_verifier = DangerousVerifier::new(
            ssl_opts.accept_invalid_certs(),
            ssl_opts.skip_domain_validation(),
            web_pki_verifier,
        );
        dangerous.set_certificate_verifier(Arc::new(dangerous_verifier));

        match self {
            Stream::TcpStream(tcp_stream) => match tcp_stream {
                TcpStream::Insecure(insecure_stream) => {
                    let inner = insecure_stream
                        .into_inner()
                        .map_err(io::Error::from)
                        .unwrap();
                    let conn =
                        rustls::ClientConnection::new(Arc::new(config), server_name).unwrap();
                    let secure_stream = rustls::StreamOwned::new(conn, inner);
                    Ok(Stream::TcpStream(TcpStream::Secure(BufStream::new(
                        Box::new(secure_stream),
                    ))))
                }
                TcpStream::Secure(_) => Ok(Stream::TcpStream(tcp_stream)),
            },
            _ => unreachable!(),
        }
    }
}

/// Matches IANA rustls suite names and the OpenSSL spellings accepted by MySQL.
fn cipher_name_matches(requested: &str, iana: &str) -> bool {
    let requested = requested.trim().to_ascii_uppercase().replace('-', "_");
    let aliases = match iana {
        "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256" => "ECDHE_RSA_AES128_GCM_SHA256",
        "TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384" => "ECDHE_RSA_AES256_GCM_SHA384",
        "TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256" => "ECDHE_RSA_CHACHA20_POLY1305",
        "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256" => "ECDHE_ECDSA_AES128_GCM_SHA256",
        "TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384" => "ECDHE_ECDSA_AES256_GCM_SHA384",
        "TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256" => "ECDHE_ECDSA_CHACHA20_POLY1305",
        _ => iana,
    };
    requested == iana || requested == aliases
}

#[derive(Debug)]
struct DangerousVerifier {
    accept_invalid_certs: bool,
    skip_domain_validation: bool,
    verifier: Arc<WebPkiServerVerifier>,
}

impl DangerousVerifier {
    fn new(
        accept_invalid_certs: bool,
        skip_domain_validation: bool,
        verifier: Arc<WebPkiServerVerifier>,
    ) -> Self {
        Self {
            accept_invalid_certs,
            skip_domain_validation,
            verifier,
        }
    }
}

impl ServerCertVerifier for DangerousVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        if self.accept_invalid_certs {
            Ok(ServerCertVerified::assertion())
        } else {
            match self.verifier.verify_server_cert(
                end_entity,
                intermediates,
                server_name,
                ocsp_response,
                now,
            ) {
                Ok(assertion) => Ok(assertion),
                Err(Error::InvalidCertificate(CertificateError::NotValidForName))
                    if self.skip_domain_validation =>
                {
                    Ok(ServerCertVerified::assertion())
                }
                Err(e) => Err(e),
            }
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        self.verifier.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        self.verifier.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.verifier.supported_verify_schemes()
    }
}
