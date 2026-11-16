//! Purpose:
//! Downloads bounded HTTPS source bodies and verifies their catalog identity.
//!
//! Called from:
//! - HttpsDownloader during explicit native source installation.
//!
//! Key details:
//! - Request and sleep injection exercise production retry and streaming paths without a network.

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

use sha2::{Digest, Sha256};
use ureq::{Body, Error as UreqError, ResponseExt};

use super::super::catalog::SourceArchive;
use super::super::error::{NativeError, NativeErrorKind};

mod github;

/// Response facts consumed by both source streaming and bounded metadata parsing.
pub(super) struct HttpResponse {
    uri: ureq::http::Uri,
    content_length: Option<u64>,
    body: Body,
}

impl HttpResponse {
    /// Preserves the final URI and declared length from the HTTPS transport.
    pub(super) fn from_response(response: ureq::http::Response<Body>) -> Self {
        let uri = response.get_uri().clone();
        let content_length = response.headers().get("content-length")
            .and_then(|value| value.to_str().ok()).and_then(|value| value.parse().ok());
        Self { uri, content_length, body: response.into_body() }
    }
}

/// Retries source requests, then streams one response under the unchanged catalog constraints.
pub(super) fn download(
    source: &SourceArchive,
    destination: &Path,
    mut get: impl FnMut(&str, Option<&str>) -> Result<HttpResponse, UreqError>,
    mut sleep: impl FnMut(Duration),
) -> Result<(), NativeError> {
    if !source.https_url.starts_with("https://") {
        return Err(NativeError::new(NativeErrorKind::Network, "catalog source URL is not HTTPS"));
    }
    let mut attempt = 1_u32;
    let mut response = loop {
        match get(source.https_url, None) {
            Ok(response) => break response,
            Err(error) if attempt < 4 && retryable_download_error(&error) => {
                let delay = Duration::from_secs(1_u64 << (attempt - 1));
                eprintln!(
                    "native network warning: download attempt {attempt} failed for trusted source '{}': {error}; retrying in {}s",
                    source.https_url,
                    delay.as_secs(),
                );
                sleep(delay);
                attempt += 1;
            }
            Err(error) => {
                if matches!(error, UreqError::StatusCode(502 | 503 | 504)) {
                    if let Some(asset) = github::ReleaseAsset::from_source(source.https_url) {
                        eprintln!(
                            "native network warning: trusted source '{}' still failed after {attempt} attempts: {error}; trying the same GitHub release asset through the API",
                            source.https_url,
                        );
                        break asset.download(source, &mut get).map_err(|fallback| NativeError::new(
                            fallback.kind,
                            format!("download failed for trusted source '{}': {error}; GitHub fallback failed: {fallback}", source.https_url),
                        ))?;
                    }
                }
                return Err(NativeError::new(
                    NativeErrorKind::Network,
                    format!("download failed for trusted source '{}': {error}", source.https_url),
                ));
            }
        }
    };
    if response.uri.scheme_str() != Some("https") {
        return Err(NativeError::new(NativeErrorKind::Network, "native download ended at a non-HTTPS URL"));
    }
    if response.content_length.is_some_and(|length| length > source.body_limit) {
        return Err(NativeError::new(NativeErrorKind::Integrity, format!("native source body exceeds {} byte limit", source.body_limit)));
    }
    let mut output = OpenOptions::new().write(true).create_new(true).open(destination)
        .map_err(|error| NativeError::io("create unique native download temporary file", destination, error))?;
    let mut reader = response.body.as_reader();
    let mut total = 0_u64;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer).map_err(|error| NativeError::new(NativeErrorKind::Network, format!("read native HTTPS response: {error}")))?;
        if read == 0 { break; }
        total = total.checked_add(read as u64).ok_or_else(|| NativeError::new(NativeErrorKind::Integrity, "native source length overflow"))?;
        if total > source.body_limit {
            return Err(NativeError::new(NativeErrorKind::Integrity, format!("native source body exceeds {} byte limit", source.body_limit)));
        }
        digest.update(&buffer[..read]);
        output.write_all(&buffer[..read]).map_err(|error| NativeError::io("write native download temporary file", destination, error))?;
    }
    let sha256 = format!("{:x}", digest.finalize());
    if total != source.exact_size || sha256 != source.sha256 {
        return Err(NativeError::new(
            NativeErrorKind::Integrity,
            format!("native source stream checksum/size mismatch: expected {} bytes and {}, got {} bytes and {}", source.exact_size, source.sha256, total, sha256),
        ));
    }
    output.sync_all().map_err(|error| NativeError::io("flush native download temporary file", destination, error))
}

/// Returns whether another request can safely recover from a transient transport or server error.
pub(super) fn retryable_download_error(error: &UreqError) -> bool {
    match error {
        // Public artifact CDNs can answer 403 for a transient edge or rate-limit denial.
        // Catalog URLs are fixed and trusted, so the existing bounded retry remains safe.
        UreqError::StatusCode(status) => {
            *status == 403 || *status == 408 || *status == 429 || (500..=599).contains(status)
        }
        UreqError::Protocol(_)
        | UreqError::Io(_)
        | UreqError::Timeout(_)
        | UreqError::HostNotFound
        | UreqError::ConnectionFailed => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests;
