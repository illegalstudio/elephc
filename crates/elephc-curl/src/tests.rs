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

/// Purpose:
/// The wave-completeness ratchet for `curl_setopt()`: every `CURLOPT_*` in the frozen
/// PHP surface is classified by `crate::options`, and every classification is one this
/// build actually implements or one whose "unsupported" answer is documented. A new
/// constant added to `scripts/docs/curl_surface.json` that nobody classified fails here.
///
/// Called from:
/// - `cargo test -p elephc-curl` through Rust's test harness.
///
/// Key details:
/// - THESE RUN WITHOUT NATIVE libcurl, unlike everything in `native` below: the option
///   table is pure data, so the contract it encodes is checkable on any machine and in
///   any CI job, which is exactly what makes it usable as a ratchet.
/// - The frozen JSON is read from `CARGO_MANIFEST_DIR/../../scripts/docs/curl_surface.json`
///   at test time rather than baked in with `include_str!`, so the test reports a missing
///   file as a clear failure instead of failing to compile.
#[cfg(test)]
mod option_table {
    use crate::options::{
        option_kind, KIND_INVALID, KIND_LONG, KIND_OFF_T, KIND_PHP_LAYER, KIND_SLIST,
        KIND_STRING, KIND_UNSUPPORTED, OPTION_KINDS,
    };

    /// Loads the frozen curl surface the whole feature is generated from.
    fn frozen_surface() -> serde_json::Value {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/docs/curl_surface.json");
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("frozen curl surface at {}: {e}", path.display()));
        serde_json::from_slice(&bytes).expect("frozen curl surface must be valid JSON")
    }

    /// Binary search needs a sorted table, and two rows for the same option number would
    /// make the classification depend on which one search happened to land on.
    #[test]
    fn option_table_is_sorted_and_unique() {
        for window in OPTION_KINDS.windows(2) {
            assert!(
                window[0].0 < window[1].0,
                "OPTION_KINDS must be strictly sorted by option number: {} then {}",
                window[0].0,
                window[1].0
            );
        }
    }

    /// EVERY `CURLOPT_*` PHP exposes has a classification, and it is one of the seven
    /// kinds — never the `KIND_INVALID` that would make `curl_setopt()` raise
    /// `ValueError` for a real PHP option.
    #[test]
    fn every_frozen_curlopt_is_classified() {
        let surface = frozen_surface();
        let constants = surface["constants"]
            .as_object()
            .expect("frozen surface must carry a constants map");
        let mut unclassified = Vec::new();
        for (name, value) in constants {
            if !name.starts_with("CURLOPT_") {
                continue;
            }
            let number = value.as_i64().expect("option values are integers");
            let number = i32::try_from(number).expect("option numbers fit in an i32");
            let kind = option_kind(number);
            if kind == KIND_INVALID {
                unclassified.push(format!("{name} ({number})"));
            }
            assert!(
                (KIND_INVALID..=KIND_UNSUPPORTED).contains(&kind),
                "{name} ({number}) has an out-of-range kind {kind}"
            );
        }
        assert!(
            unclassified.is_empty(),
            "these frozen CURLOPT_* constants are neither implemented nor documented as \
             unsupported — add them to crate::options: {unclassified:?}"
        );
    }

    /// The classification MATCHES the frozen `option_kinds` bucket for every option,
    /// except for the six rows `crate::options` documents as deliberate divergences. This
    /// is the half of the ratchet that catches a *wrong* classification rather than a
    /// missing one: silently marking a slist option as a string is the exact bug the
    /// table exists to prevent, and it would pass the previous test.
    #[test]
    fn option_table_matches_the_frozen_surface() {
        /// (constant name, expected kind here, the frozen bucket it diverges from), each
        /// explained in `crate::options`' module doc. Keyed by NAME rather than number
        /// because `CURLOPT_INFILE` and `CURLOPT_READDATA` share option 10009 while
        /// sitting in different frozen buckets.
        const DOCUMENTED_DIVERGENCES: &[(&str, i32, &str)] = &[
            ("CURLOPT_HEADER", KIND_LONG, "php_layer"),
            ("CURLOPT_INFILESIZE", KIND_LONG, "php_layer"),
            ("CURLOPT_FILE", KIND_UNSUPPORTED, "php_layer"),
            ("CURLOPT_INFILE", KIND_UNSUPPORTED, "php_layer"),
            ("CURLOPT_WRITEHEADER", KIND_UNSUPPORTED, "php_layer"),
            ("CURLOPT_STDERR", KIND_UNSUPPORTED, "php_layer"),
            ("CURLOPT_PRIVATE", KIND_PHP_LAYER, "file"),
            ("CURLOPT_SHARE", KIND_UNSUPPORTED, "file"),
        ];

        let surface = frozen_surface();
        let constants = surface["constants"].as_object().expect("constants map");
        let buckets = surface["option_kinds"]
            .as_object()
            .expect("frozen surface must carry an option_kinds map");
        for (name, value) in constants {
            if !name.starts_with("CURLOPT_") {
                continue;
            }
            let number =
                i32::try_from(value.as_i64().expect("integer option value")).expect("fits i32");
            let bucket = buckets[name].as_str().expect("option_kinds values are strings");
            let actual = option_kind(number);
            if let Some(&(_, expected, from)) = DOCUMENTED_DIVERGENCES
                .iter()
                .find(|&&(divergent, _, _)| divergent == name.as_str())
            {
                assert_eq!(
                    bucket, from,
                    "{name} ({number}) is recorded as a documented divergence from {from:?}, \
                     but the frozen surface now buckets it as {bucket:?}"
                );
                assert_eq!(actual, expected, "{name} ({number}) diverges to a different kind");
                continue;
            }
            let expected = match bucket {
                "long" => KIND_LONG,
                "string" => KIND_STRING,
                "slist" => KIND_SLIST,
                "off_t" => KIND_OFF_T,
                "php_layer" => KIND_PHP_LAYER,
                "blob" | "callback" | "file" => KIND_UNSUPPORTED,
                other => panic!("{name}: unknown frozen option kind {other:?}"),
            };
            assert_eq!(
                actual, expected,
                "{name} ({number}) is bucketed {bucket:?} in the frozen surface but \
                 classified {actual} by crate::options"
            );
        }
    }

    /// An option number that is not a cURL option at all — including one that would
    /// TRUNCATE onto a real option in a 32-bit parameter — classifies as `KIND_INVALID`,
    /// which is what makes `curl_setopt()` raise php-src's `ValueError` for it.
    #[test]
    fn unknown_option_numbers_are_invalid() {
        for number in [0, 1, 9998, 12_345, 25_000, 30_005, 40_077, i32::MAX, i32::MIN] {
            assert_eq!(
                option_kind(number),
                KIND_INVALID,
                "{number} must not classify as a real cURL option"
            );
        }
        assert_eq!(
            crate::abi::elephc_curl_option_kind(4_294_967_298),
            KIND_INVALID,
            "a value that truncates onto option 2 must not be classified as option 2"
        );
        assert_eq!(
            crate::abi::elephc_curl_option_kind(2),
            KIND_UNSUPPORTED,
            "CURLINFO_HEADER_OUT is a real curl_setopt option php-src recognizes"
        );
    }
}

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
