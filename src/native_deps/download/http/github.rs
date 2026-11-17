//! Purpose:
//! Resolves an exact catalogued GitHub release asset after exhausted gateway failures.
//!
//! Called from:
//! - The native HTTP downloader after its ordinary source retries.
//!
//! Key details:
//! - Two anonymous, bounded API requests; API URLs are constructed locally from trusted identity.

use std::io::Read;

use serde::Deserialize;

use super::{HttpResponse, NativeError, NativeErrorKind, SourceArchive, UreqError};

const METADATA_LIMIT: u64 = 1024 * 1024;

/// Catalog identity supported by the GitHub release-asset fallback.
pub(super) struct ReleaseAsset<'a> {
    owner: &'a str,
    repo: &'a str,
    tag: &'a str,
    name: &'a str,
}

/// Only the release identity and asset fields needed to select the exact source.
#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    assets: Vec<Asset>,
}

/// Untrusted metadata is matched against the catalog before constructing a download URL.
#[derive(Deserialize)]
struct Asset {
    id: u64,
    name: String,
    size: u64,
    state: String,
    browser_download_url: String,
}

impl<'a> ReleaseAsset<'a> {
    /// Accepts only literal GitHub release URLs with unambiguous, unreserved path components.
    pub(super) fn from_source(url: &'a str) -> Option<Self> {
        let parts: Vec<_> = url.strip_prefix("https://github.com/")?.split('/').collect();
        let [owner, repo, "releases", "download", tag, name] = parts.as_slice() else { return None; };
        if ![owner, repo, tag, name].into_iter().all(|part| {
            !part.is_empty() && *part != "." && *part != ".."
                && part.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"-._".contains(&byte))
        }) { return None; }
        Some(Self { owner, repo, tag, name })
    }

    /// Resolves the pinned asset anonymously, never following URLs supplied by JSON metadata.
    pub(super) fn download(
        &self,
        source: &SourceArchive,
        get: &mut impl FnMut(&str, Option<&str>) -> Result<HttpResponse, UreqError>,
    ) -> Result<HttpResponse, NativeError> {
        let base = format!("https://api.github.com/repos/{}/{}", self.owner, self.repo);
        let mut metadata = get(&format!("{base}/releases/tags/{}", self.tag), Some("application/vnd.github+json"))
            .map_err(api_error)?;
        if metadata.uri.scheme_str() != Some("https") {
            return Err(network_error("GitHub release metadata ended at a non-HTTPS URL"));
        }
        if metadata.content_length.is_some_and(|size| size > METADATA_LIMIT) {
            return Err(network_error("GitHub release metadata exceeds 1 MiB limit"));
        }
        let mut bytes = Vec::new();
        metadata.body.as_reader().take(METADATA_LIMIT + 1).read_to_end(&mut bytes)
            .map_err(|error| network_error(format!("read GitHub release metadata: {error}")))?;
        if bytes.len() as u64 > METADATA_LIMIT {
            return Err(network_error("GitHub release metadata exceeds 1 MiB limit"));
        }
        let release: Release = serde_json::from_slice(&bytes)
            .map_err(|error| network_error(format!("invalid GitHub release metadata: {error}")))?;
        if release.draft || release.tag_name != self.tag {
            return Err(network_error("GitHub release metadata does not match the catalog tag"));
        }
        let mut assets = release.assets.iter().filter(|asset| asset.name == self.name);
        let asset = assets.next().ok_or_else(|| network_error("catalog source asset is missing from GitHub release"))?;
        if assets.next().is_some() || asset.id == 0 || asset.state != "uploaded"
            || asset.size != source.exact_size || asset.browser_download_url != source.https_url {
            return Err(network_error("GitHub release asset metadata does not match the catalog source"));
        }
        get(&format!("{base}/releases/assets/{}", asset.id), Some("application/octet-stream"))
            .map_err(api_error)
    }
}

/// Reports an API failure without spending additional requests on rate-limited endpoints.
fn api_error(error: UreqError) -> NativeError {
    let hint = if matches!(error, UreqError::StatusCode(403 | 429)) {
        "; anonymous API access may be rate limited, retry later"
    } else { "" };
    network_error(format!("GitHub release asset API request failed: {error}{hint}"))
}

/// Keeps fallback lookup failures in the native network diagnostic category.
fn network_error(message: impl Into<String>) -> NativeError {
    NativeError::new(NativeErrorKind::Network, message)
}
