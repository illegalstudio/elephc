//! Purpose:
//! Exercises native HTTP recovery and integrity checks with scripted responses.
//!
//! Called from:
//! - The native downloader unit tests through Rust's test harness.
//!
//! Key details:
//! - Runs production request, streaming and cache publication code without external network access.

use super::*;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;

use super::super::{Downloader, ensure_source};
use crate::native_deps::util::unique_sibling;

const SOURCE_URL: &str = "https://github.com/kkos/oniguruma/releases/download/v6.9.10/onig-6.9.10.tar.gz";
const RELEASE_URL: &str = "https://api.github.com/repos/kkos/oniguruma/releases/tags/v6.9.10";
const ASSET_URL: &str = "https://api.github.com/repos/kkos/oniguruma/releases/assets/217074598";

/// Records each production request and provides the next scripted transport result.
struct ScriptedDownloader {
    responses: RefCell<VecDeque<Result<HttpResponse, UreqError>>>,
    requests: RefCell<Vec<(String, Option<String>)>>,
    sleeps: RefCell<Vec<Duration>>,
}

impl ScriptedDownloader {
    /// Initializes the response script with observable requests and retry delays.
    fn new(responses: Vec<Result<HttpResponse, UreqError>>) -> Self {
        Self { responses: RefCell::new(responses.into()), requests: RefCell::new(Vec::new()), sleeps: RefCell::new(Vec::new()) }
    }
}

impl Downloader for ScriptedDownloader {
    /// Uses the actual downloader while replacing only network I/O and elapsed sleep.
    fn download_to(&self, source: &SourceArchive, destination: &Path) -> Result<(), NativeError> {
        download(source, destination, |url, accept| {
            self.requests.borrow_mut().push((url.into(), accept.map(str::to_owned)));
            self.responses.borrow_mut().pop_front().expect("unexpected extra HTTP request")
        }, |delay| self.sleeps.borrow_mut().push(delay))
    }
}

/// Owns one isolated cache directory and removes it even when an assertion panics.
struct CacheFixture(PathBuf);

impl CacheFixture {
    /// Creates a unique directory for cache and publication temporary files.
    fn new() -> Self {
        let directory = unique_sibling(&std::env::temp_dir().join("elephc-http-test"), "fixture");
        fs::create_dir(&directory).unwrap();
        Self(directory)
    }

    /// Returns the exact unpublished source cache path.
    fn cached(&self) -> PathBuf { self.0.join("source.tar.gz") }
}

impl Drop for CacheFixture {
    /// Removes only this test's owned directory on success or failure.
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

/// Pins the fixture bytes with the same source identity checks used for real archives.
fn source() -> SourceArchive {
    SourceArchive {
        https_url: SOURCE_URL,
        sha256: "f16d05ec6b29248d2c61adb1e9263f78e4f7bace1b955014a2d17872cfe4064d",
        exact_size: 7,
        body_limit: 1024,
    }
}

/// Builds a response whose final URI can represent a followed HTTPS CDN redirect.
fn response(uri: &str, bytes: impl Into<Vec<u8>>) -> Result<HttpResponse, UreqError> {
    Ok(HttpResponse { uri: uri.parse().unwrap(), content_length: None, body: Body::builder().data(bytes) })
}

/// Describes the exact pinned release asset while including an unrelated asset first.
fn metadata() -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "tag_name": "v6.9.10", "draft": false,
        "assets": [
            {"id": 1, "name": "other.tar.gz", "size": 7, "state": "uploaded", "browser_download_url": "https://example.invalid/other"},
            {"id": 217074598, "name": "onig-6.9.10.tar.gz", "size": 7, "state": "uploaded", "browser_download_url": SOURCE_URL}
        ]
    })).unwrap()
}

/// Proves persistent gateway failures recover the same verified source via the asset API.
#[test]
fn github_gateway_failure_recovers_verified_source() {
    for status in [502, 503, 504] { assert_gateway_recovery(status); }
}

/// Checks the complete fallback request sequence for one supported gateway status.
fn assert_gateway_recovery(status: u16) {
    let fixture = CacheFixture::new();
    let downloader = ScriptedDownloader::new(vec![
        Err(UreqError::StatusCode(status)), Err(UreqError::StatusCode(status)),
        Err(UreqError::StatusCode(status)), Err(UreqError::StatusCode(status)),
        response(RELEASE_URL, metadata()), response("https://release-assets.githubusercontent.com/fixture", b"fixture"),
    ]);
    ensure_source(&fixture.cached(), &source(), false, &downloader).unwrap();
    assert_eq!(fs::read(fixture.cached()).unwrap(), b"fixture");
    let requests = downloader.requests.borrow();
    assert_eq!(requests.len(), 6);
    assert!(requests[..4].iter().all(|request| request == &(SOURCE_URL.to_owned(), None)));
    assert_eq!(requests[4], (RELEASE_URL.into(), Some("application/vnd.github+json".into())));
    assert_eq!(requests[5], (ASSET_URL.into(), Some("application/octet-stream".into())));
    assert_eq!(*downloader.sleeps.borrow(), [Duration::from_secs(1), Duration::from_secs(2), Duration::from_secs(4)]);
    assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 1);
}

/// Adds exhausted gateway failures before the supplied API responses.
fn gateway_failure_then(responses: Vec<Result<HttpResponse, UreqError>>) -> ScriptedDownloader {
    let mut script: Vec<_> = (0..4).map(|_| Err(UreqError::StatusCode(504))).collect();
    script.extend(responses);
    ScriptedDownloader::new(script)
}

/// Asserts rejected responses leave neither a published cache entry nor temporary files.
fn assert_rejected(downloader: &ScriptedDownloader, expected: &str) -> NativeError {
    let fixture = CacheFixture::new();
    let error = ensure_source(&fixture.cached(), &source(), false, downloader).unwrap_err();
    assert!(error.to_string().contains(expected), "{error}");
    assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 0);
    error
}

/// Successful direct downloads and ordinary retries never consult the API.
#[test]
fn direct_source_success_avoids_github_api() {
    for retry in [false, true] {
        let fixture = CacheFixture::new();
        let mut responses = Vec::new();
        if retry { responses.push(Err(UreqError::StatusCode(504))); }
        responses.push(response(SOURCE_URL, b"fixture"));
        let downloader = ScriptedDownloader::new(responses);
        ensure_source(&fixture.cached(), &source(), false, &downloader).unwrap();
        assert_eq!(downloader.requests.borrow().len(), if retry { 2 } else { 1 });
        assert!(downloader.requests.borrow().iter().all(|(url, accept)| url == SOURCE_URL && accept.is_none()));
    }
}

/// Permanent errors, throttling and unrelated server errors retain their existing retry policy.
#[test]
fn other_http_failures_do_not_use_github_fallback() {
    for (status, attempts) in [(400, 1), (404, 1), (403, 4), (408, 4), (429, 4), (500, 4), (501, 4)] {
        let downloader = ScriptedDownloader::new((0..attempts).map(|_| Err(UreqError::StatusCode(status))).collect());
        assert_rejected(&downloader, "download failed for trusted source");
        assert_eq!(downloader.requests.borrow().len(), attempts);
        assert_eq!(downloader.sleeps.borrow().len(), attempts - 1);
    }
}

/// Unsupported URL shapes never construct a fallback API request, even after gateway failures.
#[test]
fn github_fallback_rejects_ambiguous_source_urls() {
    for url in [
        "https://example.com/kkos/oniguruma/releases/download/v6.9.10/onig-6.9.10.tar.gz",
        "https://github.com.evil.invalid/kkos/oniguruma/releases/download/v6.9.10/onig.tar.gz",
        "https://github.com@evil.invalid/kkos/oniguruma/releases/download/v6.9.10/onig.tar.gz",
        "https://github.com/kkos/oniguruma/archive/refs/tags/v6.9.10.tar.gz",
        "https://github.com/kkos/oniguruma/releases/download/../onig.tar.gz",
        "https://github.com/kkos/oniguruma/releases/download/v6.9.10/onig.tar.gz?other=1",
        "https://github.com/kkos/oniguruma/releases/download/v6.9.10/onig.tar.gz#fragment",
        "https://github.com/kkos/oniguruma/releases/download/v6.9.10/onig%2F.tar.gz",
        "https://github.com//oniguruma/releases/download/v6.9.10/onig.tar.gz",
    ] {
        let fixture = CacheFixture::new();
        let downloader = gateway_failure_then(Vec::new());
        let source = SourceArchive { https_url: url, ..source() };
        assert!(ensure_source(&fixture.cached(), &source, false, &downloader).is_err());
        assert_eq!(downloader.requests.borrow().len(), 4);
        assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 0);
    }
}

/// Every GitHub source currently pinned in the catalog can use the narrow URL resolver.
#[test]
fn catalog_github_release_sources_support_fallback() {
    let mut count = 0;
    for package in crate::native_deps::catalog::packages() {
        for version in package.versions {
            if version.source.https_url.starts_with("https://github.com/") {
                assert!(github::ReleaseAsset::from_source(version.source.https_url).is_some(), "{}", version.source.https_url);
                count += 1;
            }
        }
    }
    assert!(count > 0);
}

/// Missing, duplicate, malformed and mismatched metadata cannot select an archive.
#[test]
fn github_fallback_requires_exact_release_asset_metadata() {
    for case in 0..9 {
        let mut release: serde_json::Value = serde_json::from_slice(&metadata()).unwrap();
        match case {
            0 => release["tag_name"] = "v9.9.9".into(),
            1 => release["draft"] = true.into(),
            2 => release["assets"][1]["name"] = "another.tar.gz".into(),
            3 => release["assets"][1]["size"] = 8.into(),
            4 => release["assets"][1]["browser_download_url"] = "https://evil.invalid/source".into(),
            5 => release["assets"][1]["state"] = "new".into(),
            6 => release["assets"][1]["id"] = 0.into(),
            7 => release["assets"].as_array_mut().unwrap().push(serde_json::from_slice::<serde_json::Value>(&metadata()).unwrap()["assets"][1].clone()),
            8 => release["assets"][1]["id"] = "not-an-id".into(),
            _ => unreachable!(),
        }
        let downloader = gateway_failure_then(vec![response(RELEASE_URL, serde_json::to_vec(&release).unwrap())]);
        assert_rejected(&downloader, "GitHub fallback failed");
        assert_eq!(downloader.requests.borrow().len(), 5);
    }
    let downloader = gateway_failure_then(vec![response(RELEASE_URL, b"not json")]);
    assert_rejected(&downloader, "invalid GitHub release metadata");
}

/// Metadata reads reject HTTPS downgrades and bound both advertised and actual response size.
#[test]
fn github_fallback_bounds_metadata_and_requires_https() {
    let mut oversized_header = response(RELEASE_URL, b"").unwrap();
    oversized_header.content_length = Some((1024 * 1024_u64) + 1);
    for (metadata, expected) in [
        (response("http://api.github.com/fixture", self::metadata()), "non-HTTPS"),
        (Ok(oversized_header), "exceeds 1 MiB"),
        (response(RELEASE_URL, vec![b' '; ((1024 * 1024_u64) + 1) as usize]), "exceeds 1 MiB"),
    ] {
        let downloader = gateway_failure_then(vec![metadata]);
        assert_rejected(&downloader, expected);
        assert_eq!(downloader.requests.borrow().len(), 5);
    }
}

/// A secondary API failure is final, including rate limits, with the original source error retained.
#[test]
fn github_fallback_api_failures_are_bounded() {
    for status in [403, 404, 429, 502, 503, 504] {
        for asset_failure in [false, true] {
            let mut responses = Vec::new();
            if asset_failure { responses.push(response(RELEASE_URL, metadata())); }
            responses.push(Err(UreqError::StatusCode(status)));
            let downloader = gateway_failure_then(responses);
            let error = assert_rejected(&downloader, "GitHub release asset API request failed");
            assert!(error.to_string().contains(SOURCE_URL));
            assert!(error.to_string().contains("504"));
            if matches!(status, 403 | 429) { assert!(error.to_string().contains("rate limited")); }
            assert_eq!(downloader.requests.borrow().len(), if asset_failure { 6 } else { 5 });
            assert_eq!(downloader.sleeps.borrow().len(), 3);
        }
    }
}

/// Fallback bodies keep the exact checksum, exact size, streaming cap and HTTPS requirements.
#[test]
fn github_fallback_rejects_invalid_archive_bodies() {
    let mut oversized_header = response(ASSET_URL, b"fixture").unwrap();
    oversized_header.content_length = Some(source().body_limit + 1);
    for (body, expected) in [
        (response(ASSET_URL, b"altered"), "checksum/size mismatch"),
        (response(ASSET_URL, b"fixtur"), "checksum/size mismatch"),
        (response(ASSET_URL, vec![b'x'; 1025]), "exceeds 1024 byte limit"),
        (Ok(oversized_header), "exceeds 1024 byte limit"),
        (response("http://example.invalid/source", b"fixture"), "non-HTTPS"),
    ] {
        let downloader = gateway_failure_then(vec![response(RELEASE_URL, metadata()), body]);
        assert_rejected(&downloader, expected);
        assert_eq!(downloader.requests.borrow().len(), 6);
    }
}

/// Offline misses and verified cache hits cannot reach either network endpoint.
#[test]
fn github_source_offline_and_cache_hits_avoid_requests() {
    let fixture = CacheFixture::new();
    let downloader = ScriptedDownloader::new(Vec::new());
    assert!(ensure_source(&fixture.cached(), &source(), true, &downloader).is_err());
    fs::write(fixture.cached(), b"fixture").unwrap();
    for offline in [true, false] {
        ensure_source(&fixture.cached(), &source(), offline, &downloader).unwrap();
    }
    assert!(downloader.requests.borrow().is_empty());
}
