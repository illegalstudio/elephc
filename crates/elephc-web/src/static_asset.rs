//! Purpose:
//! Static-asset fast path for the `--web` server. With `--static-dir DIR`, each
//! worker serves files under DIR from a URL prefix (`--static-prefix`, default
//! `/assets`) directly on the I/O thread WITHOUT invoking the PHP handler —
//! raising effective concurrency under N=1 (a static request does not wait
//! behind a slow in-flight PHP request). Files are cached per worker in a
//! bounded LRU; misses load from disk via `tokio::task::spawn_blocking` so the
//! I/O thread never blocks on a syscall.
//!
//! Called from:
//! - `crate::worker::serve` classic `--web` service_fn (the intercept + cache
//!   lookup are mirrored in `crate::worker_mode::enter_worker_loop`).
//!
//! Key details:
//! - The cache + lookup live on the I/O thread (single-threaded access →
//!   `RefCell`, no atomics). The `spawn_blocking` closure does ONLY the disk
//!   read + traversal check and returns owned `Send` data; it CANNOT capture
//!   `Rc` (not `Send`).
//! - The traversal guard canonicalizes `root + rel` and requires the canonical
//!   path to be `root` itself or a descendant, so a `..` in `rel` that escapes
//!   the root (and a symlink pointing outside) yields `NotFound` (404, not
//!   403, so a traversal attempt is indistinguishable from a missing file).
//! - Static responses ARE recorded as requests via `metrics::record_request`
//!   (status 200/304/403/404/500), so `/_status` reflects static traffic.

use std::collections::{HashMap, VecDeque};
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::cell::RefCell;
use std::time::SystemTime;

use http_body_util::Full;
use hyper::body::Bytes;
use hyper::Response;

/// One cached static asset: its bytes, MIME type, and ETag. Stored in the
/// per-worker LRU. `mime` is a `&'static str` from the built-in extension map.
/// `etag` is `"<size>-<mtime_secs>"` (cheap, no hashing). `Clone` so a cache
/// hit can hand the serve path an owned copy for building the response while
/// the cache keeps its copy (the clone cost on a hit is the file size —
/// acceptable since the disk read on a miss would be slower).
#[derive(Clone)]
pub(crate) struct StaticAsset {
    /// The raw file bytes.
    pub bytes: Vec<u8>,
    /// The MIME type from the built-in extension map (`&'static str`).
    pub mime: &'static str,
    /// The ETag, formatted as `"<size>-<mtime_secs>"`.
    pub etag: String,
}

/// Outcome of an on-demand disk load for a static asset. `Ok` carries the
/// asset; `Err` carries a stable status code so the caller can record + respond
/// without re-classifying.
#[derive(Debug)]
pub(crate) enum StaticLoadError {
    /// 404: file does not exist, is outside the root, or is a directory.
    NotFound,
    /// 403: file exceeds `--static-max-file-size` (defense against huge files).
    TooBig,
    /// 500: transient read error (treat as 500 so the operator notices).
    Io,
}

/// Maps a file extension (lowercased) to a `&'static str` MIME type. The map is
/// intentionally small — it covers the common web-asset types; add entries as
/// needed. Unknown extensions fall back to `application/octet-stream`.
pub(crate) fn mime_for_extension(ext: &str) -> &'static str {
    let lower = ext.to_ascii_lowercase();
    match lower.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "application/javascript; charset=utf-8",
        "mjs" => "application/javascript; charset=utf-8",
        "json" => "application/json",
        "txt" => "text/plain; charset=utf-8",
        "xml" => "application/xml",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "eot" => "application/vnd.ms-fontobject",
        "wasm" => "application/wasm",
        "map" => "application/json",
        _ => "application/octet-stream",
    }
}

/// Reads one static asset from disk relative to `root`, enforcing the
/// traversal guard and the per-file size cap. Meant to run on a
/// `spawn_blocking` thread: it performs synchronous `std::fs` IO and returns
/// owned Send data. `root` is the already-canonicalized root (canonicalized
/// ONCE at worker startup); `rel` is the URL-decoded path after the prefix.
/// Returns `Ok(StaticAsset)` on success or a `StaticLoadError` (`NotFound` if
/// the resolved path is outside `root`, missing, or a directory; `TooBig` if
/// over the cap; `Io` on a transient read error).
///
/// Traversal guard: joins `root` + `rel`, canonicalizes the result, and
/// requires the canonical path to be `root` itself or start with `root` + a
/// path separator. A `..` in `rel` that escapes the root yields a path outside
/// `root` → `NotFound` (reported as 404, not 403, so the traversal attempt is
/// not distinguishable from a missing file — avoids leaking the layout).
/// Symlinks: `canonicalize` resolves them; a symlink inside `root` pointing
/// outside → path outside `root` → `NotFound`.
pub(crate) fn load_asset(
    root: &Path,
    rel: &str,
    max_file_size: u64,
) -> Result<StaticAsset, StaticLoadError> {
    let full = root.join(rel);
    let canonical = match std::fs::canonicalize(&full) {
        Ok(c) => c,
        Err(e) => {
            return Err(if e.kind() == io::ErrorKind::NotFound {
                StaticLoadError::NotFound
            } else {
                StaticLoadError::Io
            });
        }
    };
    // Membership test: canonical must be root or a descendant. `strip_prefix`
    // succeeds iff canonical == root or canonical starts with root + separator.
    if canonical.strip_prefix(root).is_err() {
        return Err(StaticLoadError::NotFound);
    }
    // Reject a request for the directory itself.
    if canonical == *root {
        return Err(StaticLoadError::NotFound);
    }
    let metadata = match std::fs::metadata(&canonical) {
        Ok(m) => m,
        Err(_) => return Err(StaticLoadError::Io),
    };
    if !metadata.is_file() {
        return Err(StaticLoadError::NotFound);
    }
    let size = metadata.len();
    if max_file_size != 0 && size > max_file_size {
        return Err(StaticLoadError::TooBig);
    }
    let bytes = match std::fs::read(&canonical) {
        Ok(b) => b,
        Err(_) => return Err(StaticLoadError::Io),
    };
    let mtime_secs = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // RFC 7232 §2.3: an entity-tag is a double-quoted opaque string. Quote the
    // size-mtime payload so strict intermediaries (CDN/proxy) do not strip it.
    let etag = format!("\"{}-{}\"", size, mtime_secs);
    let mime = mime_for_extension(
        Path::new(rel)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or(""),
    );
    Ok(StaticAsset { bytes, mime, etag })
}

/// Bounded per-worker LRU of recently-served static assets. `Rc<RefCell<...>>`
/// so the service_fn closures can `.clone()` an `Rc` handle cheaply.
/// Single-threaded access (I/O thread only) → `RefCell`, no atomics. The byte
/// cap is `--static-cache-size` MiB (default 64); insertion evicts oldest
/// entries (by insertion order — a `VecDeque<String>` of keys + a
/// `HashMap<String, (StaticAsset, usize)>`) until under the cap. Cheap,
/// deterministic, no dep.
pub(crate) struct StaticCache {
    /// key = rel path (the stripped prefix) → (asset, byte size).
    map: HashMap<String, (StaticAsset, usize)>,
    /// LRU order: front = oldest, back = most-recent.
    order: VecDeque<String>,
    /// Current total bytes cached.
    total_bytes: usize,
    /// Max bytes the cache will hold before evicting.
    cap_bytes: usize,
}

impl StaticCache {
    /// Builds a new cache with the given byte cap.
    pub(crate) fn new(cap_bytes: usize) -> Self {
        StaticCache {
            map: HashMap::new(),
            order: VecDeque::new(),
            total_bytes: 0,
            cap_bytes,
        }
    }

    /// Looks up `key`, promoting it to most-recent on a hit and returning a
    /// clone of the asset. Returns `None` on a miss. The clone is the file
    /// size — acceptable for a hit (disk would be slower); the cache keeps its
    /// copy so the serve path owns the clone for building the response.
    pub(crate) fn get(&mut self, key: &str) -> Option<StaticAsset> {
        if self.map.contains_key(key) {
            self.order.retain(|k| k != key);
            self.order.push_back(key.to_string());
            return self.map.get(key).map(|(a, _)| a.clone());
        }
        None
    }

    /// Inserts `asset` under `key`, evicting oldest entries until the cache is
    /// under the byte cap. If `key` already exists, the old entry is removed
    /// first (map + order, size subtracted). Guard: if a single asset is
    /// larger than `cap_bytes`, inserting it will immediately evict itself
    /// (the while loop pops it back out) — a too-big-for-cache file is served
    /// fresh each time and never cached; that is intentional and documented.
    pub(crate) fn insert(&mut self, key: String, asset: StaticAsset) {
        if let Some((_, oldsize)) = self.map.remove(&key) {
            self.order.retain(|k| k != &key);
            self.total_bytes -= oldsize;
        }
        let size = asset.bytes.len();
        self.map.insert(key.clone(), (asset, size));
        self.order.push_back(key);
        self.total_bytes += size;
        while self.total_bytes > self.cap_bytes {
            match self.order.pop_front() {
                Some(old) => {
                    if let Some((_, oldsize)) = self.map.remove(&old) {
                        self.total_bytes -= oldsize;
                    }
                }
                None => break,
            }
        }
    }
}

/// URL-decode a relative path segment (`%XX` → byte, else literal). Hand-rolled
/// (no dep). Invalid `%` sequences (not followed by two hex digits) are left
/// as the literal `%` byte. Lowercase and uppercase hex both accepted. Uses
/// `String::from_utf8_lossy` so invalid UTF-8 in a path does not panic (lossy:
/// invalid bytes become `U+FFFD`).
pub(crate) fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex_digit(bytes[i + 1]), hex_digit(bytes[i + 2])) {
                out.push((h << 4) | l);
                i += 3;
                continue;
            }
        }
        out.push(b);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Returns the numeric value of a single hex digit byte, or `None` if not hex.
fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Serves one static request from the per-worker cache, loading from disk on
/// a miss via `spawn_blocking`. Returns the `Response` (200 with body for GET,
/// empty for HEAD, 304 for a matching `If-None-Match`, 404/403/500 on error)
/// AND the status code, so the caller records the request via
/// `metrics::record_request`. `method` is `"GET"` or `"HEAD"`. `rel` is the
/// URL-decoded path after the prefix. `if_none_match` is the request's
/// `If-None-Match` header value (for 304 revalidation). This fn is async
/// because the miss path awaits `spawn_blocking`.
pub(crate) async fn serve_static(
    cache: Rc<RefCell<StaticCache>>,
    root: PathBuf,
    rel: String,
    method: &str,
    if_none_match: Option<&str>,
    max_file_size: u64,
    max_age_secs: u32,
) -> (Response<Full<Bytes>>, u16) {
    // Cache hit path.
    if let Some(asset) = cache.borrow_mut().get(&rel) {
        if if_none_match == Some(asset.etag.as_str()) {
            return (build_304(&asset.etag, max_age_secs), 304);
        }
        return (build_200(asset, method, max_age_secs), 200);
    }
    // Cache miss: load from disk on a blocking thread.
    let root2 = root.clone();
    let rel2 = rel.clone();
    let loaded = tokio::task::spawn_blocking(move || {
        load_asset(&root2, &rel2, max_file_size)
    })
    .await;
    match loaded {
        Ok(Ok(asset)) => {
            // Insert into the cache first (so the next request is a hit), then
            // build the response from the local asset. A clone goes into the
            // cache; the local one is used for the response.
            cache.borrow_mut().insert(rel.clone(), asset.clone());
            if if_none_match == Some(asset.etag.as_str()) {
                return (build_304(&asset.etag, max_age_secs), 304);
            }
            (build_200(asset, method, max_age_secs), 200)
        }
        Ok(Err(StaticLoadError::NotFound)) => {
            (build_error(404, "Not Found", method), 404)
        }
        Ok(Err(StaticLoadError::TooBig)) => {
            (build_error(403, "Forbidden", method), 403)
        }
        Ok(Err(StaticLoadError::Io)) => {
            (build_error(500, "Internal Server Error", method), 500)
        }
        Err(_join_err) => {
            (build_error(500, "Internal Server Error", method), 500)
        }
    }
}

/// Builds a 200 response with the asset body (empty for HEAD). Sets
/// `Content-Type`, `Cache-Control: public, max-age=<max_age>`, and `ETag`.
/// Takes the asset BY VALUE so the body bytes move into the response with no
/// redundant clone (the cache already holds its own copy from `StaticCache::get`).
fn build_200(asset: StaticAsset, method: &str, max_age: u32) -> Response<Full<Bytes>> {
    let body = if method == "HEAD" {
        Full::new(Bytes::new())
    } else {
        Full::new(Bytes::from(asset.bytes))
    };
    Response::builder()
        .status(200)
        .header("content-type", asset.mime)
        .header("cache-control", format!("public, max-age={}", max_age))
        .header("etag", asset.etag.clone())
        .body(body)
        .unwrap_or_else(|_| Response::new(Full::new(Bytes::new())))
}

/// Builds a 304 response with the `ETag` and `Cache-Control` headers and an
/// empty body.
fn build_304(etag: &str, max_age: u32) -> Response<Full<Bytes>> {
    Response::builder()
        .status(304)
        .header("etag", etag)
        .header("cache-control", format!("public, max-age={}", max_age))
        .body(Full::new(Bytes::new()))
        .unwrap_or_else(|_| Response::new(Full::new(Bytes::new())))
}

/// Builds an error response with a plain-text body (empty for HEAD).
fn build_error(status: u16, msg: &str, method: &str) -> Response<Full<Bytes>> {
    let body = if method == "HEAD" {
        Full::new(Bytes::new())
    } else {
        Full::new(Bytes::from(msg.as_bytes().to_vec()))
    };
    Response::builder()
        .status(status)
        .header("content-type", "text/plain; charset=utf-8")
        .body(body)
        .unwrap_or_else(|_| Response::new(Full::new(Bytes::new())))
}

#[cfg(test)]
mod tests {
    // Purpose:
    // Unit tests for the static-asset fast path: MIME map, LRU cache
    // eviction/promotion, on-disk load (traversal guard, missing-file,
    // size cap, ETag format), and percent-decoding.
    //
    // Called from:
    // - `cargo test` through Rust's test harness.
    //
    // Key details:
    // - Temp files live under a unique subdir per test (an `AtomicU64`
    //   counter for deterministic uniqueness, not time). Each test cleans
    //   up its temp dir with `remove_dir_all` (best effort).

    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    /// Makes a unique temp dir for one test and returns its canonicalized path.
    fn unique_temp_dir(label: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("elephc_static_{}_{}", label, n));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        std::fs::canonicalize(&dir).unwrap_or(dir)
    }

    /// Verifies the MIME map covers common extensions and falls back to
    /// `application/octet-stream` for unknown ones, case-insensitively.
    #[test]
    fn mime_for_extension_known() {
        assert!(mime_for_extension("css").contains("text/css"));
        assert!(mime_for_extension("JS").contains("application/javascript"));
        assert!(mime_for_extension("wasm").contains("application/wasm"));
        assert_eq!(mime_for_extension("xyz"), "application/octet-stream");
        assert_eq!(mime_for_extension(""), "application/octet-stream");
    }

    /// Verifies the LRU evicts the oldest entry when over the byte cap.
    #[test]
    fn static_cache_insert_evicts_oldest() {
        let mut cache = StaticCache::new(100);
        let mk = |c: u8| StaticAsset {
            bytes: vec![c; 40],
            mime: "application/octet-stream",
            etag: "1-1".to_string(),
        };
        cache.insert("a".into(), mk(b'a'));
        cache.insert("b".into(), mk(b'b'));
        cache.insert("c".into(), mk(b'c'));
        // total 120 > 100 → "a" evicted.
        assert!(cache.get("a").is_none(), "a must be evicted");
        assert!(cache.get("b").is_some(), "b must be present");
        assert!(cache.get("c").is_some(), "c must be present");
    }

    /// Verifies a cache hit promotes the key to most-recent, so a subsequent
    /// eviction skips the promoted key and removes the actual oldest.
    #[test]
    fn static_cache_get_promotes_lru() {
        let mut cache = StaticCache::new(200);
        let mk = |c: u8| StaticAsset {
            bytes: vec![c; 40],
            mime: "application/octet-stream",
            etag: "1-1".to_string(),
        };
        cache.insert("a".into(), mk(b'a'));
        cache.insert("b".into(), mk(b'b'));
        cache.insert("c".into(), mk(b'c'));
        // Promote "a" to most-recent.
        let _ = cache.get("a").expect("a present before promote");
        cache.insert("d".into(), mk(b'd'));
        cache.insert("e".into(), mk(b'e'));
        cache.insert("f".into(), mk(b'f'));
        // total 240 > 200 → evicts oldest = "b" (a was promoted).
        assert!(cache.get("b").is_none(), "b must be evicted (a was promoted)");
        assert!(cache.get("a").is_some(), "a present");
        assert!(cache.get("c").is_some(), "c present");
        assert!(cache.get("d").is_some(), "d present");
        assert!(cache.get("e").is_some(), "e present");
        assert!(cache.get("f").is_some(), "f present");
    }

    /// Verifies a `..` traversal in `rel` is rejected as `NotFound` (the
    /// resolved canonical path lies outside the root), while a file inside the
    /// root loads fine.
    #[test]
    fn load_asset_traversal_rejects_dotdot() {
        let root = unique_temp_dir("traversal");
        std::fs::write(root.join("sub.txt"), "inside").unwrap();
        let parent = root.parent().unwrap().to_path_buf();
        std::fs::write(parent.join("elephc_outside.txt"), "outside").unwrap();
        let big_cap: u64 = 10 * 1024 * 1024;
        match load_asset(&root, "../elephc_outside.txt", big_cap) {
            Err(StaticLoadError::NotFound) => {}
            other => panic!("expected NotFound, got {:?}", other.map(|a| a.etag)),
        }
        assert!(load_asset(&root, "sub.txt", big_cap).is_ok());
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_file(parent.join("elephc_outside.txt"));
    }

    /// Verifies a missing file returns `NotFound`.
    #[test]
    fn load_asset_missing_is_notfound() {
        let root = unique_temp_dir("missing");
        match load_asset(&root, "nope.txt", 1024) {
            Err(StaticLoadError::NotFound) => {}
            other => panic!("expected NotFound, got {:?}", other.map(|a| a.etag)),
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Verifies a file over the size cap returns `TooBig`.
    #[test]
    fn load_asset_too_big() {
        let root = unique_temp_dir("toobig");
        std::fs::write(root.join("big.bin"), vec![b'x'; 100]).unwrap();
        match load_asset(&root, "big.bin", 50) {
            Err(StaticLoadError::TooBig) => {}
            other => panic!("expected TooBig, got {:?}", other.map(|a| a.etag)),
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Verifies the ETag is a double-quoted opaque string (RFC 7232 §2.3)
    /// whose inner payload is `<size>-<mtime_secs>` (a `-` with parseable u64s
    /// on both sides).
    #[test]
    fn load_asset_etag_format() {
        let root = unique_temp_dir("etag");
        std::fs::write(root.join("f.txt"), "hello").unwrap();
        let asset = load_asset(&root, "f.txt", 1024).expect("ok");
        // RFC 7232: ETag is a double-quoted opaque string.
        assert!(asset.etag.starts_with('"') && asset.etag.ends_with('"'), "quoted: {}", asset.etag);
        let inner = asset.etag.trim_matches('"');
        let parts: Vec<&str> = inner.splitn(2, '-').collect();
        assert_eq!(parts.len(), 2, "etag inner has a dash: {}", inner);
        assert!(parts[0].parse::<u64>().is_ok(), "size side is u64: {}", parts[0]);
        assert!(parts[1].parse::<u64>().is_ok(), "mtime side is u64: {}", parts[1]);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Verifies percent-decoding of `%XX` sequences and that invalid sequences
    /// are left as the literal `%` byte.
    #[test]
    fn percent_decode_basic() {
        assert_eq!(percent_decode("a%20b"), "a b");
        assert_eq!(percent_decode("%2e%2e"), "..");
        assert_eq!(percent_decode("plain"), "plain");
        assert_eq!(percent_decode("a%2"), "a%2");
        assert_eq!(percent_decode("%41"), "A");
    }
}