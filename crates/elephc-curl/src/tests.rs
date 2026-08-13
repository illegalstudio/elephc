//! Purpose:
//! Crate unit tests for `elephc-curl`. Real libcurl-touching tests live
//! behind `#[cfg(elephc_curl_native)]`, emitted by `build.rs` only when
//! `ELEPHC_CURL_LIB_DIR` (plus the sibling `ELEPHC_CURL_OPENSSL_LIB_DIR` /
//! `ELEPHC_CURL_ZLIB_LIB_DIR`) point at a real `elephc native`-installed
//! curl/openssl/zlib package. Without the cfg, nothing in this test binary
//! calls the real ABI, so the never-resolved libcurl `extern "C"` symbols it
//! declares get dropped before the link step needs them, and `cargo test -p
//! elephc-curl` still runs cleanly, printing a clear skip message (verified
//! empirically; see task-3-report.md for the experiment and the RED/GREEN
//! evidence).
//!
//! Called from:
//! - `cargo test -p elephc-curl` through Rust's test harness.
//!
//! Key details:
//! - Real run:
//!   `ELEPHC_CURL_LIB_DIR=<curl prefix>/lib \`
//!   `ELEPHC_CURL_OPENSSL_LIB_DIR=<openssl prefix>/lib \`
//!   `ELEPHC_CURL_ZLIB_LIB_DIR=<zlib prefix>/lib \`
//!   `cargo test -p elephc-curl`
//!   using this machine's `elephc native`-installed curl 8.21.0/openssl
//!   3.5.7/zlib 1.3.2 artifact prefixes.
//! - Handle-table assertions check presence/absence of the specific ids each
//!   test allocated rather than the table's aggregate length, since
//!   `cargo test` runs tests in parallel threads sharing the one global
//!   table.
//! - The RETURNTRANSFER smoke uses a `file://` fixture instead of a live
//!   HTTP server, matching the "tests never hit the public internet" rule
//!   (`.superpowers/sdd/php-curl-family/global-constraints.md`) without
//!   needing Task 7's HTTP fixture pattern yet — `file://` still drives the
//!   exact same write-callback path HTTP does.

#[cfg(not(elephc_curl_native))]
mod skipped {
    /// Reports why the real libcurl-linked tests below did not run, instead
    /// of silently reporting a green suite that skipped everything.
    #[test]
    fn native_curl_tests_skipped_without_lib_dir() {
        eprintln!(
            "SKIP: ELEPHC_CURL_LIB_DIR (and ELEPHC_CURL_OPENSSL_LIB_DIR / \
             ELEPHC_CURL_ZLIB_LIB_DIR) are not set, so this run cannot link \
             real libcurl into the elephc-curl test binary. Set them to an \
             `elephc native`-installed curl/openssl/zlib package's lib/ \
             directories to run the real handle-table/perform/global_info \
             tests."
        );
    }
}

#[cfg(elephc_curl_native)]
mod native {
    use crate::abi::{
        elephc_curl_easy_error, elephc_curl_easy_errno, elephc_curl_easy_free,
        elephc_curl_easy_getinfo_long, elephc_curl_easy_init, elephc_curl_easy_perform,
        elephc_curl_easy_set_url, elephc_curl_easy_setopt_long, elephc_curl_easy_take_body,
        elephc_curl_global_info, elephc_curl_version_abi,
    };
    use crate::handles;
    use crate::php_layer::CURLOPT_RETURNTRANSFER;

    /// Whether `id` is currently present in the shared handle table. Used
    /// instead of the table's aggregate length so these assertions stay
    /// correct when `cargo test` runs other tests concurrently against the
    /// same process-wide table.
    fn handle_exists(id: i64) -> bool {
        handles::lock_recover(handles::handles()).contains_key(&id)
    }

    /// TDD Step 1: `elephc_curl_easy_init`/`elephc_curl_easy_free` leave the
    /// handle table balanced, hand out unique, monotonically increasing ids,
    /// and never reuse a freed id. Written before `abi.rs`/`handles.rs`
    /// existed; task-3-report.md records the RED run against the
    /// not-yet-implemented crate and a second RED run against a
    /// deliberately reintroduced bug, both followed by GREEN.
    #[test]
    fn init_and_free_balance_the_handle_table() {
        let a = elephc_curl_easy_init();
        let b = elephc_curl_easy_init();
        assert_ne!(a, 0, "curl_easy_init should succeed with real libcurl linked");
        assert_ne!(b, 0);
        assert_ne!(a, b, "ids must be unique");
        assert!(b > a, "ids must be monotonically increasing");
        assert!(handle_exists(a));
        assert!(handle_exists(b));

        elephc_curl_easy_free(a);
        assert!(!handle_exists(a), "free must remove exactly the freed handle");
        assert!(handle_exists(b), "freeing one handle must not affect another");

        elephc_curl_easy_free(b);
        assert!(!handle_exists(b), "table must be balanced after every handle is freed");

        // Ids are never reused: a third init must not hand back `a` or `b`.
        let c = elephc_curl_easy_init();
        assert_ne!(c, a);
        assert_ne!(c, b);
        assert!(c > b);
        elephc_curl_easy_free(c);
        assert!(!handle_exists(c));
    }

    /// Freeing an unknown/already-freed/negative/zero id is a documented
    /// no-op, not a crash or a double `curl_easy_cleanup` of a real handle.
    #[test]
    fn free_is_idempotent_and_tolerates_unknown_ids() {
        let id = elephc_curl_easy_init();
        assert_ne!(id, 0);
        elephc_curl_easy_free(id);
        elephc_curl_easy_free(id); // must not double-free the underlying CURL*
        elephc_curl_easy_free(-1);
        elephc_curl_easy_free(0);
        assert!(!handle_exists(id));
    }

    /// The ABI version marker is fixed and independent of libcurl linkage;
    /// exercised here alongside the real tests for completeness.
    #[test]
    fn version_abi_is_one() {
        assert_eq!(elephc_curl_version_abi(), 1);
    }

    /// A real `curl_version_info` smoke: `elephc_curl_global_info` must
    /// report our pinned libcurl 8.21.0 / OpenSSL 3.5.7 / zlib 1.3.2, and the
    /// required HTTP/HTTPS/FILE/FTP/FTPS protocol matrix
    /// (global-constraints.md's protocol matrix).
    #[test]
    fn global_info_reports_the_pinned_libcurl_build() {
        let mut cap = 64usize;
        let mut buf: Vec<u8>;
        let mut len = 0usize;
        loop {
            buf = vec![0u8; cap];
            let ok = unsafe { elephc_curl_global_info(buf.as_mut_ptr(), buf.len(), &mut len) };
            if ok == 1 {
                break;
            }
            assert!(len > cap, "a 0 return must always grow the required length");
            cap = len;
        }
        buf.truncate(len);
        let json: serde_json::Value =
            serde_json::from_slice(&buf).expect("global_info must produce valid JSON");
        assert_eq!(json["version"], "8.21.0");
        assert_eq!(json["ssl_version"], "OpenSSL/3.5.7");
        assert_eq!(json["libz_version"], "1.3.2");
        let protocols: Vec<String> = json["protocols"]
            .as_array()
            .expect("protocols must be a JSON array")
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        for required in ["file", "http", "https", "ftp", "ftps"] {
            assert!(
                protocols.iter().any(|p| p == required),
                "missing required protocol {required} in {protocols:?}"
            );
        }
    }

    /// End-to-end: init -> set_url (a `file://` fixture, no network) ->
    /// setopt_long(RETURNTRANSFER) -> perform -> take_body must round-trip
    /// the fixture's exact bytes, and errno/error must report success.
    #[test]
    fn perform_with_returntransfer_captures_a_file_url_body() {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let suffix = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("elephc-curl-test-{}-{suffix}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fixture_path = dir.join("body.txt");
        std::fs::write(&fixture_path, b"elephc curl fixture body\n").unwrap();
        let url = format!("file://{}", fixture_path.display());

        let id = elephc_curl_easy_init();
        assert_ne!(id, 0);
        let set_ok = unsafe { elephc_curl_easy_set_url(id, url.as_ptr(), url.len()) };
        assert_eq!(set_ok, 1, "set_url should accept a file:// URL");

        let opt_ok = elephc_curl_easy_setopt_long(id, CURLOPT_RETURNTRANSFER, 1);
        assert_eq!(opt_ok, 1);

        let perform_ok = elephc_curl_easy_perform(id);
        assert_eq!(perform_ok, 1, "perform should succeed against a local file:// fixture");
        assert_eq!(elephc_curl_easy_errno(id), 0);

        let mut error_buf = vec![0u8; 256];
        let mut error_len = 0usize;
        let error_ok = unsafe {
            elephc_curl_easy_error(id, error_buf.as_mut_ptr(), error_buf.len(), &mut error_len)
        };
        assert_eq!(error_ok, 1);
        assert_eq!(error_len, 0, "no error message after a successful transfer");

        let mut body_ptr: *mut u8 = std::ptr::null_mut();
        let mut body_len = 0usize;
        let take_ok = unsafe { elephc_curl_easy_take_body(id, &mut body_ptr, &mut body_len) };
        assert_eq!(take_ok, 1);
        let body = unsafe { std::slice::from_raw_parts(body_ptr, body_len) }.to_vec();
        assert_eq!(body, b"elephc curl fixture body\n");

        elephc_curl_easy_free(id);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Without `RETURNTRANSFER`, the write callback streams to fd 1 instead
    /// of capturing: `take_body` must report an empty body even after a
    /// successful transfer.
    #[test]
    fn perform_without_returntransfer_leaves_body_empty() {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let suffix = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir()
            .join(format!("elephc-curl-test-nrt-{}-{suffix}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fixture_path = dir.join("body.txt");
        std::fs::write(&fixture_path, b"not captured\n").unwrap();
        let url = format!("file://{}", fixture_path.display());

        let id = elephc_curl_easy_init();
        let set_ok = unsafe { elephc_curl_easy_set_url(id, url.as_ptr(), url.len()) };
        assert_eq!(set_ok, 1);

        let perform_ok = elephc_curl_easy_perform(id);
        assert_eq!(perform_ok, 1);

        let mut body_ptr: *mut u8 = std::ptr::null_mut();
        let mut body_len = 1usize; // start nonzero so we can observe it reset to 0
        let take_ok = unsafe { elephc_curl_easy_take_body(id, &mut body_ptr, &mut body_len) };
        assert_eq!(take_ok, 1);
        assert_eq!(body_len, 0, "body must be empty when RETURNTRANSFER was never set");

        elephc_curl_easy_free(id);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `CURLINFO_RESPONSE_CODE` (2097154, PHP's `CURLINFO_HTTP_CODE`) on a handle that
    /// never performed a transfer reports success with value `0` — matching libcurl's own
    /// documented behavior (the call succeeds even before any response code exists) and
    /// `curl_getinfo()`'s expectation that a fresh handle answers `0`, not `false`.
    #[test]
    fn getinfo_long_reports_zero_before_any_transfer() {
        const CURLINFO_RESPONSE_CODE: i32 = 2_097_154;
        let id = elephc_curl_easy_init();
        assert_ne!(id, 0);

        let mut value: i64 = -1;
        let ok = unsafe { elephc_curl_easy_getinfo_long(id, CURLINFO_RESPONSE_CODE, &mut value) };
        assert_eq!(ok, 1, "getinfo must succeed on a fresh handle");
        assert_eq!(value, 0, "no response code has been received yet");

        elephc_curl_easy_free(id);
    }

    /// End-to-end: a `file://` transfer (no network, matching every other perform test in
    /// this file) still reports `CURLINFO_RESPONSE_CODE` as a real libcurl success with
    /// value `0` — `file://` has no HTTP status line, so this is the same "succeeds, value
    /// zero" shape as the fresh-handle case, but reached through a completed transfer
    /// instead of an untouched handle.
    #[test]
    fn getinfo_long_after_file_transfer_reports_zero() {
        const CURLINFO_RESPONSE_CODE: i32 = 2_097_154;
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let suffix = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir()
            .join(format!("elephc-curl-test-getinfo-{}-{suffix}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fixture_path = dir.join("body.txt");
        std::fs::write(&fixture_path, b"getinfo fixture body\n").unwrap();
        let url = format!("file://{}", fixture_path.display());

        let id = elephc_curl_easy_init();
        let set_ok = unsafe { elephc_curl_easy_set_url(id, url.as_ptr(), url.len()) };
        assert_eq!(set_ok, 1);
        let perform_ok = elephc_curl_easy_perform(id);
        assert_eq!(perform_ok, 1);

        let mut value: i64 = -1;
        let ok = unsafe { elephc_curl_easy_getinfo_long(id, CURLINFO_RESPONSE_CODE, &mut value) };
        assert_eq!(ok, 1);
        assert_eq!(value, 0, "file:// transfers carry no HTTP status line");

        elephc_curl_easy_free(id);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// An `info` value outside libcurl's `CURLINFO_LONG` type range (here
    /// `CURLINFO_EFFECTIVE_URL`, a STRING-typed field) is rejected before libcurl is ever
    /// asked to write through the `long`-shaped out-parameter — the read-side mirror of
    /// `curl_setopt()`'s pointer-range validation, and the reason `getinfo_long` checks the
    /// type mask at all rather than forwarding any option number blindly.
    #[test]
    fn getinfo_long_rejects_a_non_long_info_type() {
        const CURLINFO_EFFECTIVE_URL: i32 = 0x10_0001;
        let id = elephc_curl_easy_init();
        assert_ne!(id, 0);

        let mut value: i64 = -1;
        let ok = unsafe { elephc_curl_easy_getinfo_long(id, CURLINFO_EFFECTIVE_URL, &mut value) };
        assert_eq!(ok, 0, "a non-long info type must be rejected, not forwarded");
        assert_eq!(value, -1, "out must be left untouched on rejection");

        elephc_curl_easy_free(id);
    }

    /// An unknown/already-freed id is a documented `0` failure, matching every other
    /// per-handle entry point in this ABI.
    #[test]
    fn getinfo_long_reports_zero_for_unknown_id() {
        const CURLINFO_RESPONSE_CODE: i32 = 2_097_154;
        let mut value: i64 = -1;
        let ok = unsafe { elephc_curl_easy_getinfo_long(-999, CURLINFO_RESPONSE_CODE, &mut value) };
        assert_eq!(ok, 0);
        assert_eq!(value, -1);
    }
}
