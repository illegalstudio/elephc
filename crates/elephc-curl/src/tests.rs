//! Purpose:
//! Crate unit tests for `elephc-curl`. Real libcurl-touching tests live
//! behind `#[cfg(elephc_curl_native)]`, emitted by `build.rs` only when
//! `ELEPHC_CURL_LIB_DIR` (plus the sibling `ELEPHC_CURL_OPENSSL_LIB_DIR` /
//! `ELEPHC_CURL_ZLIB_LIB_DIR`) point at a real `elephc native`-installed
//! curl/openssl/zlib package. Without the cfg, nothing in this test binary
//! calls the real ABI, so the never-resolved libcurl `extern "C"` symbols it
//! declares get dropped before the link step needs them, and `cargo test -p
//! elephc-curl` still runs cleanly, printing a clear skip message.
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
//!   HTTP server, keeping these unit tests off the public internet without
//!   needing a local HTTP fixture server here — `file://` still drives the
//!   exact same write-callback path HTTP does. (`tests/codegen/curl/*`
//!   integration fixtures cover real HTTP/HTTPS separately.)

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
        option_kind, KIND_CALLBACK, KIND_INVALID, KIND_LONG, KIND_OFF_T, KIND_PHP_LAYER,
        KIND_SHARE, KIND_SLIST, KIND_STREAM, KIND_STRING, KIND_UNSUPPORTED, OPTION_KINDS,
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

    /// EVERY `CURLOPT_*` PHP exposes has a classification, and it is one of the TEN
    /// kinds `crate::options` defines — never the `KIND_INVALID`
    /// that would make `curl_setopt()` raise `ValueError` for a real PHP option.
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
                (KIND_INVALID..=KIND_STREAM).contains(&kind),
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
            // The four PHP-stream options. The frozen surface buckets three of them
            // `php_layer` and `CURLOPT_READDATA` — which is option 10009 under its other
            // PHP name — `file`; all four are `KIND_STREAM` here, serviced by the curl
            // prelude's internal callbacks rather than by anything the bridge forwards.
            ("CURLOPT_FILE", KIND_STREAM, "php_layer"),
            ("CURLOPT_INFILE", KIND_STREAM, "php_layer"),
            ("CURLOPT_READDATA", KIND_STREAM, "file"),
            ("CURLOPT_WRITEHEADER", KIND_STREAM, "php_layer"),
            ("CURLOPT_STDERR", KIND_STREAM, "php_layer"),
            ("CURLOPT_PRIVATE", KIND_PHP_LAYER, "file"),
            ("CURLOPT_SHARE", KIND_SHARE, "file"),
        ];

        /// The callback options this build carries as real PHP callables.
        /// Every other `"callback"`-bucketed option stays `KIND_UNSUPPORTED`.
        const IMPLEMENTED_CALLBACKS: &[&str] = &[
            "CURLOPT_WRITEFUNCTION",
            "CURLOPT_HEADERFUNCTION",
            "CURLOPT_READFUNCTION",
            "CURLOPT_PROGRESSFUNCTION",
            "CURLOPT_XFERINFOFUNCTION",
            "CURLOPT_DEBUGFUNCTION",
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
                // The frozen surface has ONE "callback" bucket; this build implements the
                // first wave of it and still rejects the rest, so the bucket alone no
                // longer decides the kind. `IMPLEMENTED_CALLBACKS` is the explicit list —
                // adding a row there is the deliberate act of shipping that option, and
                // anything not on it must still answer `false` + PHP's warning (locked
                // decision 7), which is what keeps this half of the ratchet meaningful.
                "callback" if IMPLEMENTED_CALLBACKS.contains(&name.as_str()) => KIND_CALLBACK,
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

    /// THE AUDIT ARTIFACT: the exact set of `curl_setopt()` options this build
    /// rejects, pinned by NAME.
    ///
    /// `docs/php/curl.md` publishes this list to users as the answer to "what does
    /// elephc's curl not do?", so it has to be the code's list rather than a prose
    /// approximation of it. Its two siblings above catch a *missing* or *wrong*
    /// classification; this one catches the case they cannot — an option quietly
    /// gaining or losing support without the user-facing table following it. Shipping
    /// a rejected option is a one-line edit here plus a one-line edit in the doc.
    ///
    /// `CURLOPT_INFILE` and `CURLOPT_READDATA` are the same option number (10009) under
    /// two PHP names, so 16 names cover 15 distinct numbers.
    #[test]
    fn the_documented_rejection_set_is_exactly_this() {
        const DOCUMENTED_REJECTIONS: &[&str] = &[
            // Certificate/key material passed as an in-memory blob. libcurl's
            // `struct curl_blob` is a pointer-plus-length shape no current elephc-curl
            // entry point carries; the file-path forms (`CURLOPT_SSLCERT`, …) work.
            "CURLOPT_CAINFO_BLOB",
            "CURLOPT_ISSUERCERT_BLOB",
            "CURLOPT_PROXY_CAINFO_BLOB",
            "CURLOPT_PROXY_ISSUERCERT_BLOB",
            "CURLOPT_PROXY_SSLCERT_BLOB",
            "CURLOPT_PROXY_SSLKEY_BLOB",
            "CURLOPT_SSLCERT_BLOB",
            "CURLOPT_SSLKEY_BLOB",
            // Callbacks outside the six this build invokes.
            "CURLOPT_FNMATCH_FUNCTION",
            "CURLOPT_PREREQFUNCTION",
            "CURLOPT_SSH_HOSTKEYFUNCTION",
            // NOTE: the five PHP-STREAM options (`CURLOPT_FILE`, `CURLOPT_INFILE`/
            // `CURLOPT_READDATA`, `CURLOPT_WRITEHEADER`, `CURLOPT_STDERR`) used to be
            // here. WP-C implements them at the PHP layer on top of the callback slots,
            // so they are `KIND_STREAM` now and this list is five names shorter.
        ];

        let surface = frozen_surface();
        let constants = surface["constants"].as_object().expect("constants map");
        let mut rejected: Vec<&str> = Vec::new();
        for (name, value) in constants {
            if !name.starts_with("CURLOPT_") {
                continue;
            }
            let number =
                i32::try_from(value.as_i64().expect("integer option value")).expect("fits i32");
            if option_kind(number) == KIND_UNSUPPORTED {
                rejected.push(name.as_str());
            }
        }
        rejected.sort_unstable();
        let mut expected = DOCUMENTED_REJECTIONS.to_vec();
        expected.sort_unstable();
        assert_eq!(
            rejected, expected,
            "the set of rejected curl_setopt() options changed; update the table in \
             docs/php/curl.md to match before changing this test"
        );

        // The headline the doc quotes: 271 PHP `CURLOPT_*` names, 11 rejected.
        let total = constants.keys().filter(|n| n.starts_with("CURLOPT_")).count();
        assert_eq!(total, 271, "the frozen CURLOPT_* name count changed");
        assert_eq!(rejected.len(), 11, "the rejected CURLOPT_* name count changed");
    }
}

/// Purpose:
/// The `curl_multi_setopt()` half of the option ratchet: every `CURLMOPT_*` in the frozen
/// PHP surface is classified by `crate::multi`, and every classification matches the
/// frozen bucket. A new `CURLMOPT_*` constant nobody classified fails here.
///
/// Called from:
/// - `cargo test -p elephc-curl` through Rust's test harness.
///
/// Key details:
/// - Runs WITHOUT native libcurl, like its easy sibling: the table is pure data.
/// - `CURLMOPT_PUSHFUNCTION` is the one documented divergence — the frozen surface buckets
///   it as `callback`, which php-src supports and this build cannot (it is an HTTP/2
///   server-push hook, and HTTP/2 is not built in), so it is classified `unsupported`
///   (`false` + PHP's warning).
#[cfg(test)]
mod multi_option_table {
    use crate::multi::{
        multi_option_kind, MULTI_OPTION_INVALID, MULTI_OPTION_LONG, MULTI_OPTION_OFF_T,
        MULTI_OPTION_UNSUPPORTED,
    };

    /// Loads the frozen curl surface the whole feature is generated from.
    fn frozen_surface() -> serde_json::Value {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/docs/curl_surface.json");
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("frozen curl surface at {}: {e}", path.display()));
        serde_json::from_slice(&bytes).expect("frozen curl surface must be valid JSON")
    }

    /// Every `CURLMOPT_*` PHP exposes is classified, and as the kind the frozen bucket
    /// names — with `PUSHFUNCTION`'s documented divergence spelled out.
    #[test]
    fn multi_option_table_matches_the_frozen_surface() {
        let surface = frozen_surface();
        let constants = surface["constants"].as_object().expect("constants map");
        let buckets = surface["option_kinds"]
            .as_object()
            .expect("frozen surface must carry an option_kinds map");
        let mut seen = 0;
        for (name, value) in constants {
            if !name.starts_with("CURLMOPT_") {
                continue;
            }
            seen += 1;
            let number = value.as_i64().expect("integer option value");
            let bucket = buckets[name].as_str().expect("option_kinds values are strings");
            let expected = match bucket {
                "long" => MULTI_OPTION_LONG,
                "off_t" => MULTI_OPTION_OFF_T,
                // A real php-src option this build cannot carry (`CURLMOPT_PUSHFUNCTION` is
                // an HTTP/2 server-push hook, and HTTP/2 is not built in), so it answers
                // `false` plus PHP's warning.
                "callback" => MULTI_OPTION_UNSUPPORTED,
                other => panic!("{name} has unclassified frozen bucket {other:?}"),
            };
            assert_eq!(
                multi_option_kind(number),
                expected,
                "{name} ({number}) must be classified as the frozen surface's {bucket:?}"
            );
        }
        assert!(seen >= 9, "the frozen surface must still carry the CURLMOPT_* family");
    }

    /// A number that is not a `CURLMOPT_*` at all is INVALID, which is what makes the
    /// prelude raise php-src's `ValueError` instead of forwarding it to a variadic
    /// `curl_multi_setopt` that would read it as whatever its range implies.
    #[test]
    fn unknown_multi_options_are_invalid() {
        for opt in [0, 1, 2, 4, 5, 9, 999_999, -1, i64::from(i32::MAX) + 1] {
            assert_eq!(
                multi_option_kind(opt),
                MULTI_OPTION_INVALID,
                "{opt} is not a cURL multi option"
            );
        }
    }
}

/// Purpose:
/// The `curl_share_setopt()` half of the option ratchet.
///
/// Called from:
/// - `cargo test -p elephc-curl` through Rust's test harness.
///
/// Key details:
/// - UNLIKE `option_table`/`multi_option_table`, there is no per-constant frozen
///   `option_kinds` bucket to walk here: `CURLSHOPT_*` is a plain `typedef enum`
///   (`scripts/docs/curl_surface.json`'s `constant_verification.method` says so
///   explicitly), and PHP exposes only THREE members of it as constants at all —
///   `CURLSHOPT_NONE`/`SHARE`/`UNSHARE` — so the ratchet instead cross-checks those three
///   frozen NUMBERS directly against `crate::share::share_option_kind`.
#[cfg(test)]
mod share_option_table {
    use crate::share::{share_option_kind, SHARE_OPTION_INVALID, SHARE_OPTION_LONG};

    fn frozen_surface() -> serde_json::Value {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/docs/curl_surface.json");
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("frozen curl surface at {}: {e}", path.display()));
        serde_json::from_slice(&bytes).expect("frozen curl surface must be valid JSON")
    }

    /// `CURLSHOPT_SHARE`/`CURLSHOPT_UNSHARE`, at whatever numbers the frozen surface
    /// freezes them at, classify as the one real kind; `CURLSHOPT_NONE` (0, never a
    /// meaningful `curl_share_setopt()` argument) does not.
    #[test]
    fn share_and_unshare_match_the_frozen_numbers() {
        let surface = frozen_surface();
        let constants = surface["constants"].as_object().expect("constants map");
        for name in ["CURLSHOPT_SHARE", "CURLSHOPT_UNSHARE"] {
            let number = constants[name].as_i64().expect("integer option value");
            assert_eq!(
                share_option_kind(number),
                SHARE_OPTION_LONG,
                "{name} ({number}) must classify as a real cURL share option"
            );
        }
        let none = constants["CURLSHOPT_NONE"].as_i64().expect("integer option value");
        assert_eq!(share_option_kind(none), SHARE_OPTION_INVALID);
    }

    /// No other number — including the three locking-hook values PHP does not even
    /// expose as constants (see this module's header) — is a valid share option.
    #[test]
    fn unknown_share_options_are_invalid() {
        for opt in [-1, 3, 4, 5, 6, 999_999, i64::from(i32::MAX) + 1] {
            assert_eq!(
                share_option_kind(opt),
                SHARE_OPTION_INVALID,
                "{opt} is not a cURL share option"
            );
        }
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
        elephc_curl_easy_duphandle, elephc_curl_easy_errno, elephc_curl_easy_error,
        elephc_curl_easy_free, elephc_curl_easy_getinfo_long, elephc_curl_easy_init,
        elephc_curl_easy_perform, elephc_curl_easy_set_url, elephc_curl_easy_setopt_long,
        elephc_curl_easy_setopt_slist, elephc_curl_easy_setopt_str, elephc_curl_easy_take_body,
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

    /// `elephc_curl_easy_init`/`elephc_curl_easy_free` leave the
    /// handle table balanced, hand out unique, monotonically increasing ids,
    /// and never reuse a freed id. Written before `abi.rs`/`handles.rs`
    /// existed, as a RED test against the not-yet-implemented crate, then kept
    /// as a regression guard.
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

    /// PUNCH-LIST ITEM 4: `feature_list` is php-src's ASSOCIATIVE `name => bool`
    /// map over the `features` bitmask, not the list of strings libcurl's own
    /// `feature_names` array would give (measured against PHP 8.4.20:
    /// `var_dump(curl_version()["feature_list"])` prints 29 `string => bool`
    /// pairs, `"AsynchDNS" => bool(true)` first). Every name php publishes is
    /// present — including the ones this build lacks, reported `false` rather
    /// than omitted — and every value agrees with the bit it stands for, so a
    /// mis-transcribed bit in the table cannot pass.
    #[test]
    fn global_info_feature_list_is_php_s_name_to_bool_map() {
        let json = global_info_json();
        let features = json["features"]
            .as_i64()
            .expect("features must be an integer bitmask");
        let list = json["feature_list"]
            .as_object()
            .expect("feature_list must be a JSON object (PHP assoc array), not an array");
        assert_eq!(
            list.len(),
            crate::abi::PHP_FEATURE_LIST.len(),
            "feature_list must carry exactly php-src's own name set"
        );
        for (name, bit) in crate::abi::PHP_FEATURE_LIST {
            let expected = features & i64::from(*bit) != 0;
            assert_eq!(
                list.get(*name).and_then(serde_json::Value::as_bool),
                Some(expected),
                "feature_list[{name}] must be a bool matching bit {bit:#x} of {features}"
            );
        }
        // Pinned-build sanity: this libcurl is built against OpenSSL and zlib,
        // and nothing has shipped Kerberos V4 in two decades.
        assert_eq!(list["SSL"], serde_json::Value::Bool(true));
        assert_eq!(list["libz"], serde_json::Value::Bool(true));
        assert_eq!(list["krb4"], serde_json::Value::Bool(false));

        // A PHP ARRAY IS ORDERED, so the key ORDER is part of the shape, not an
        // implementation detail: php-src emits its table's declaration order
        // (`AsynchDNS` first, `GSASL` last), which `serde_json`'s `preserve_order`
        // feature is what keeps. Byte-sorted output would start at `ALTSVC`.
        let names: Vec<&str> = list.keys().map(String::as_str).collect();
        let expected: Vec<&str> = crate::abi::PHP_FEATURE_LIST
            .iter()
            .map(|(name, _)| *name)
            .collect();
        assert_eq!(names, expected, "feature_list must keep php-src's own order");
    }

    /// The `curl_version()` array's own KEY ORDER is php-src's, measured on PHP
    /// 8.4.20 (`array_keys(curl_version())`). PHP arrays are ordered, so a
    /// byte-sorted blob would make `foreach`/`array_keys()`/`json_encode()` report
    /// an order php never produces (`age` first, `version_number` last).
    /// `ssl_version`/`libz_version` are in the list unconditionally: php-src adds
    /// every string field through `CAAS`, which substitutes `""` for a null
    /// pointer rather than dropping the key.
    #[test]
    fn global_info_keys_are_in_php_s_order() {
        let json = global_info_json();
        let keys: Vec<&str> = json
            .as_object()
            .expect("global_info must be a JSON object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            vec![
                "version_number",
                "age",
                "features",
                "feature_list",
                "ssl_version_number",
                "version",
                "host",
                "ssl_version",
                "libz_version",
                "protocols",
                "ares",
                "ares_num",
                "libidn",
                "iconv_ver_num",
                "libssh_version",
                "brotli_ver_num",
                "brotli_version",
            ]
        );
    }

    /// PUNCH-LIST ITEM 5: the sub-library keys are gated on the struct's `age`,
    /// php-src's own rule, so `iconv_ver_num` is present on every build recent
    /// enough to have the field — it used to hang off `libssh_version`'s NULL
    /// pointer and therefore never appeared at all here. Measured on PHP 8.4.20
    /// (no c-ares, no libidn, no libssh): all five keys are present, with `""`
    /// for the libraries that are missing.
    #[test]
    fn global_info_reports_the_age_gated_sublibrary_keys() {
        let json = global_info_json();
        let age = json["age"].as_i64().expect("age must be an integer");
        assert!(age >= 4, "pinned libcurl 8.21.0 reports CURLVERSION_TWELFTH");
        for key in ["ares", "libidn", "libssh_version", "brotli_version"] {
            assert!(
                json.get(key).map(serde_json::Value::is_string) == Some(true),
                "{key} must be present as a string (empty when unavailable): {json}"
            );
        }
        for key in ["ares_num", "iconv_ver_num", "brotli_ver_num"] {
            assert!(
                json.get(key).map(serde_json::Value::is_i64) == Some(true),
                "{key} must be present as an integer: {json}"
            );
        }
    }

    /// Reads `elephc_curl_global_info`'s JSON blob through its grow-the-buffer
    /// protocol, the same loop the version smoke above documents.
    fn global_info_json() -> serde_json::Value {
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
        serde_json::from_slice(&buf).expect("global_info must produce valid JSON")
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

    /// PUNCH-LIST ITEM 2: a duplicate starts with a CLEAN transfer record — no
    /// captured body, `curl_errno() == 0`, an empty `curl_error()` — however the
    /// SOURCE's last transfer ended. Both halves are checked here because the two
    /// used to be copied together: a successful transfer (whose body was carried
    /// onto the copy, so `curl_multi_getcontent($copy)` answered the original's
    /// bytes before the copy had performed anything) and a failed one (whose
    /// `CURLcode`/message were carried too). Measured against PHP 8.4.20 — see
    /// `crate::abi::elephc_curl_easy_duphandle`'s doc comment for the transcript.
    #[test]
    fn copy_handle_starts_with_a_clean_transfer_record() {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let suffix = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "elephc-curl-copy-{}-{suffix}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let fixture_path = dir.join("body.txt");
        std::fs::write(&fixture_path, b"copied fixture body\n").unwrap();
        let url = format!("file://{}", fixture_path.display());

        // 1. A SUCCESSFUL transfer's captured body must not travel.
        let id = elephc_curl_easy_init();
        assert_ne!(id, 0);
        assert_eq!(unsafe { elephc_curl_easy_set_url(id, url.as_ptr(), url.len()) }, 1);
        assert_eq!(elephc_curl_easy_setopt_long(id, CURLOPT_RETURNTRANSFER, 1), 1);
        assert_eq!(elephc_curl_easy_perform(id), 1);
        assert_eq!(take_body(id), b"copied fixture body\n");

        let copy = elephc_curl_easy_duphandle(id);
        assert_ne!(copy, 0);
        assert!(
            take_body(copy).is_empty(),
            "the copy must have no captured body before it performs anything"
        );
        assert_eq!(elephc_curl_easy_errno(copy), 0);
        assert!(read_error(copy).is_empty());
        // The OPTION did travel, which is the half that must not regress: the copy
        // still captures rather than streaming to stdout.
        assert_eq!(elephc_curl_easy_perform(copy), 1);
        assert_eq!(take_body(copy), b"copied fixture body\n");
        elephc_curl_easy_free(copy);
        elephc_curl_easy_free(id);

        // 2. A FAILED transfer's errno/message must not travel either.
        let failed = elephc_curl_easy_init();
        let bad = "xyzzy://not-a-protocol";
        assert_eq!(unsafe { elephc_curl_easy_set_url(failed, bad.as_ptr(), bad.len()) }, 1);
        assert_eq!(elephc_curl_easy_perform(failed), 0);
        assert_ne!(
            elephc_curl_easy_errno(failed),
            0,
            "an unsupported protocol must leave a real CURLcode on the source"
        );
        assert!(!read_error(failed).is_empty());

        let copy = elephc_curl_easy_duphandle(failed);
        assert_ne!(copy, 0);
        assert_eq!(
            elephc_curl_easy_errno(copy),
            0,
            "curl_errno() on a fresh copy must be 0, not the source's last CURLcode"
        );
        assert!(
            read_error(copy).is_empty(),
            "curl_error() on a fresh copy must be empty"
        );
        elephc_curl_easy_free(copy);
        elephc_curl_easy_free(failed);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `elephc_curl_easy_take_body`'s two-out-parameter protocol as an owned `Vec`.
    fn take_body(id: i64) -> Vec<u8> {
        let mut ptr: *mut u8 = std::ptr::null_mut();
        let mut len = 0usize;
        assert_eq!(unsafe { elephc_curl_easy_take_body(id, &mut ptr, &mut len) }, 1);
        if len == 0 {
            return Vec::new();
        }
        unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec()
    }

    /// `elephc_curl_easy_error`'s message for `id` as owned bytes.
    fn read_error(id: i64) -> Vec<u8> {
        let mut buf = vec![0u8; 512];
        let mut len = 0usize;
        assert_eq!(
            unsafe { elephc_curl_easy_error(id, buf.as_mut_ptr(), buf.len(), &mut len) },
            1
        );
        buf.truncate(len);
        buf
    }

    /// PUNCH-LIST ITEM 17: each setter verifies the OPTION'S KIND itself before it
    /// calls libcurl, so a caller that picks the wrong setter gets a plain `0` —
    /// never a `long` read as a `char *`, a string read as a `long`, or an integer
    /// installed as a function pointer. The prelude and the eval interpreter both
    /// classify correctly today; this pins the BOUNDARY rather than their good
    /// behavior.
    ///
    /// BOTH FAILURE MODES WERE MEASURED against this pinned libcurl 8.21.0, with a
    /// throwaway probe calling `crate::easy`'s raw setters directly:
    /// `setopt_str(CURLOPT_TIMEOUT, ptr)` and `setopt_str(CURLOPT_HTTPHEADER, ptr)`
    /// both answered `CURLE_OK` (0) — a pointer silently accepted as a timeout, and
    /// as a list libcurl will walk — and `setopt_long(CURLOPT_URL, 42)` did not
    /// return at all: the test process died with SIGSEGV as libcurl dereferenced
    /// `42` as a `char *`. That is the difference this check makes.
    #[test]
    fn setters_refuse_options_of_another_kind_before_calling_libcurl() {
        const CURLOPT_TIMEOUT: i32 = 13; // KIND_LONG
        const CURLOPT_URL: i32 = 10_002; // KIND_STRING
        const CURLOPT_HTTPHEADER: i32 = 10_023; // KIND_SLIST
        const CURLOPT_MAXFILESIZE_LARGE: i32 = 30_117; // KIND_OFF_T
        const CURLOPT_WRITEFUNCTION: i32 = 20_011; // KIND_CALLBACK
        const CURLOPT_BINARYTRANSFER: i32 = 19_914; // KIND_PHP_LAYER, char * range
        const CURLOPT_SHARE: i32 = 10_100; // KIND_SHARE
        let url = b"file:///dev/null";
        let header = b"X-Kind: check\0";

        let id = elephc_curl_easy_init();
        assert_ne!(id, 0);

        // The RIGHT setter for each kind still applies.
        assert_eq!(elephc_curl_easy_setopt_long(id, CURLOPT_TIMEOUT, 30), 1);
        assert_eq!(elephc_curl_easy_setopt_long(id, CURLOPT_MAXFILESIZE_LARGE, 4096), 1);
        assert_eq!(
            elephc_curl_easy_setopt_long(id, CURLOPT_RETURNTRANSFER, 1),
            1,
            "the one PHP-layer option the prelude forwards to the long setter"
        );
        assert_eq!(
            unsafe { elephc_curl_easy_setopt_str(id, CURLOPT_URL, url.as_ptr(), url.len()) },
            1
        );
        assert_eq!(
            unsafe {
                elephc_curl_easy_setopt_slist(
                    id,
                    CURLOPT_HTTPHEADER,
                    header.as_ptr(),
                    header.len(),
                )
            },
            1
        );

        // The WRONG setter is refused, in every direction.
        assert_eq!(
            elephc_curl_easy_setopt_long(id, CURLOPT_URL, 42),
            0,
            "an integer must never reach a char * option"
        );
        assert_eq!(
            elephc_curl_easy_setopt_long(id, CURLOPT_HTTPHEADER, 42),
            0,
            "an integer must never reach a slist option"
        );
        assert_eq!(
            elephc_curl_easy_setopt_long(id, CURLOPT_WRITEFUNCTION, 1),
            0,
            "an integer must never be installed as a function pointer"
        );
        assert_eq!(
            elephc_curl_easy_setopt_long(id, CURLOPT_BINARYTRANSFER, 1),
            0,
            "a PHP-layer option in libcurl's char * range must not be forwarded"
        );
        assert_eq!(
            elephc_curl_easy_setopt_long(id, CURLOPT_SHARE, 1),
            0,
            "a share option takes a CURLSH *, never a long"
        );
        assert_eq!(
            unsafe { elephc_curl_easy_setopt_str(id, CURLOPT_TIMEOUT, url.as_ptr(), url.len()) },
            0,
            "a char * must never reach a long option"
        );
        assert_eq!(
            unsafe {
                elephc_curl_easy_setopt_str(id, CURLOPT_HTTPHEADER, url.as_ptr(), url.len())
            },
            0,
            "a char * must never reach a slist option"
        );
        assert_eq!(
            unsafe {
                elephc_curl_easy_setopt_slist(id, CURLOPT_TIMEOUT, header.as_ptr(), header.len())
            },
            0,
            "a slist must never reach a long option"
        );
        assert_eq!(
            unsafe { elephc_curl_easy_setopt_slist(id, CURLOPT_URL, header.as_ptr(), header.len()) },
            0,
            "a slist must never reach a char * option"
        );

        // An option number that is not in the table at all is refused by all three,
        // the same answer the prelude's `ValueError` path already gives it.
        assert_eq!(elephc_curl_easy_setopt_long(id, 999_999, 1), 0);
        assert_eq!(
            unsafe { elephc_curl_easy_setopt_str(id, 999_999, url.as_ptr(), url.len()) },
            0
        );
        assert_eq!(
            unsafe {
                elephc_curl_easy_setopt_slist(id, 999_999, header.as_ptr(), header.len())
            },
            0
        );

        // The handle is still fully usable after every refusal: nothing was
        // half-applied and no list was left dangling behind a rejected call.
        assert_eq!(elephc_curl_easy_perform(id), 1);
        elephc_curl_easy_free(id);
    }
}

/// Purpose:
/// The `curl_mime` builder ABI: the `elephc_curl_mime_new`/`_add_part`/`_part_field`/
/// `_post`/`_abort` state machine, exercised directly (no PHP program involved) against
/// real libcurl.
///
/// Called from:
/// - `cargo test -p elephc-curl` through Rust's test harness, when `ELEPHC_CURL_LIB_DIR` is
///   set (see this module's header).
///
/// Key details:
/// - End-to-end wire verification (does a real transfer really carry `multipart/form-data`
///   with the right field/filename/body) lives at the elephc compiler level
///   (`tests/codegen/curl/`), which has the loopback HTTP fixture this crate does not. These
///   tests instead pin the ABI's own state machine: which calls succeed, which fail without
///   a live builder, and what `elephc_curl_mime_new` replacing an already-attached mime does
///   to `EasyEntry::mime`/`pending_mime`.
#[cfg(elephc_curl_native)]
mod native_mime {
    use crate::abi::{
        elephc_curl_easy_free, elephc_curl_easy_init, elephc_curl_mime_abort,
        elephc_curl_mime_add_part, elephc_curl_mime_new, elephc_curl_mime_part_field,
        elephc_curl_mime_post,
    };
    use crate::handles;
    use crate::mime::{FIELD_DATA, FIELD_FILEDATA, FIELD_FILENAME, FIELD_NAME, FIELD_TYPE};

    /// Whether `id`'s entry currently has an ATTACHED mime (survived a successful `post`).
    fn has_attached_mime(id: i64) -> bool {
        handles::lock_recover(handles::handles())
            .get(&id)
            .is_some_and(|entry| entry.mime.is_some())
    }

    /// Whether `id`'s entry currently has a PENDING (not yet posted) mime builder.
    fn has_pending_mime(id: i64) -> bool {
        handles::lock_recover(handles::handles())
            .get(&id)
            .is_some_and(|entry| entry.pending_mime.is_some())
    }

    /// The happy path: `new` -> `add_part` -> two fields -> `post` succeeds end to end, and
    /// the pending builder becomes the attached one.
    #[test]
    fn happy_path_new_add_part_field_post_attaches() {
        let id = elephc_curl_easy_init();
        assert_ne!(id, 0);

        assert_eq!(elephc_curl_mime_new(id), 1);
        assert!(has_pending_mime(id));
        assert!(!has_attached_mime(id));

        assert_eq!(elephc_curl_mime_add_part(id), 1);
        let name = b"f";
        assert_eq!(
            unsafe { elephc_curl_mime_part_field(id, FIELD_NAME, name.as_ptr(), name.len()) },
            1
        );
        let data = b"hello world";
        assert_eq!(
            unsafe { elephc_curl_mime_part_field(id, FIELD_DATA, data.as_ptr(), data.len()) },
            1
        );

        assert_eq!(elephc_curl_mime_post(id), 1);
        assert!(has_attached_mime(id));
        assert!(!has_pending_mime(id), "post must clear the pending slot");

        elephc_curl_easy_free(id);
    }

    /// A SECOND successful build replaces the first ATTACHED mime rather than leaking it or
    /// leaving two live structures — the same "free the old one only after the new one is
    /// live" contract `elephc_curl_easy_setopt_slist` has for `CURLOPT_HTTPHEADER`. This is
    /// the shape `curl_setopt($ch, CURLOPT_POSTFIELDS, $array)` takes every time it runs
    /// against a handle that already posted a multipart body once.
    #[test]
    fn second_build_replaces_the_first_attached_mime() {
        let id = elephc_curl_easy_init();
        assert_ne!(id, 0);

        assert_eq!(elephc_curl_mime_new(id), 1);
        assert_eq!(elephc_curl_mime_add_part(id), 1);
        let name = b"a";
        unsafe { elephc_curl_mime_part_field(id, FIELD_NAME, name.as_ptr(), name.len()) };
        assert_eq!(elephc_curl_mime_post(id), 1);
        assert!(has_attached_mime(id));

        assert_eq!(elephc_curl_mime_new(id), 1);
        assert_eq!(elephc_curl_mime_add_part(id), 1);
        let name2 = b"b";
        unsafe { elephc_curl_mime_part_field(id, FIELD_NAME, name2.as_ptr(), name2.len()) };
        assert_eq!(elephc_curl_mime_post(id), 1);
        assert!(has_attached_mime(id), "the replacement must still be attached");

        elephc_curl_easy_free(id);
    }

    /// `elephc_curl_mime_part_field` before `elephc_curl_mime_add_part` fails closed: there
    /// is no current part to write to.
    #[test]
    fn field_without_add_part_fails() {
        let id = elephc_curl_easy_init();
        assert_ne!(id, 0);
        assert_eq!(elephc_curl_mime_new(id), 1);
        let name = b"f";
        assert_eq!(
            unsafe { elephc_curl_mime_part_field(id, FIELD_NAME, name.as_ptr(), name.len()) },
            0
        );
        elephc_curl_easy_free(id);
    }

    /// `elephc_curl_mime_add_part` before `elephc_curl_mime_new` fails closed: there is no
    /// pending builder to append to.
    #[test]
    fn add_part_without_new_fails() {
        let id = elephc_curl_easy_init();
        assert_ne!(id, 0);
        assert_eq!(elephc_curl_mime_add_part(id), 0);
        elephc_curl_easy_free(id);
    }

    /// `elephc_curl_mime_post` before `elephc_curl_mime_new` fails closed: there is no
    /// pending builder to attach.
    #[test]
    fn post_without_new_fails() {
        let id = elephc_curl_easy_init();
        assert_ne!(id, 0);
        assert_eq!(elephc_curl_mime_post(id), 0);
        elephc_curl_easy_free(id);
    }

    /// `elephc_curl_mime_abort` discards a half-built PENDING mime without touching a mime
    /// already ATTACHED from an earlier successful build — the exact shape a
    /// `curl_setopt(..., CURLOPT_POSTFIELDS, $array)` call takes when the SECOND call's array
    /// contains something this build refuses partway through the walk (a nested array, an
    /// unrecognized object). The first, successful multipart body must stay attached and
    /// usable.
    #[test]
    fn abort_discards_only_the_pending_builder() {
        let id = elephc_curl_easy_init();
        assert_ne!(id, 0);

        // First, successful build.
        assert_eq!(elephc_curl_mime_new(id), 1);
        assert_eq!(elephc_curl_mime_add_part(id), 1);
        let name = b"a";
        unsafe { elephc_curl_mime_part_field(id, FIELD_NAME, name.as_ptr(), name.len()) };
        assert_eq!(elephc_curl_mime_post(id), 1);
        assert!(has_attached_mime(id));

        // Second build, abandoned partway through (mirrors a nested-array rejection).
        assert_eq!(elephc_curl_mime_new(id), 1);
        assert!(has_pending_mime(id));
        assert_eq!(elephc_curl_mime_abort(id), 1);
        assert!(!has_pending_mime(id), "abort must clear the pending builder");
        assert!(
            has_attached_mime(id),
            "abort must not touch the already-attached mime from the first build"
        );

        elephc_curl_easy_free(id);
    }

    /// `elephc_curl_mime_abort` on an id with no pending builder at all (nothing to build,
    /// or already posted) is a harmless, always-successful no-op — this is a cleanup call,
    /// not a status query, matching every other "nothing to do" shape in this ABI.
    #[test]
    fn abort_is_idempotent_with_no_pending_builder() {
        let id = elephc_curl_easy_init();
        assert_ne!(id, 0);
        assert_eq!(elephc_curl_mime_abort(id), 1);
        assert_eq!(elephc_curl_mime_abort(id), 1);
        elephc_curl_easy_free(id);
    }

    /// Every entry point tolerates an unknown id the same way the rest of this ABI does:
    /// `0` for anything that answers a status, never a crash.
    #[test]
    fn unknown_id_reports_zero_or_tolerates_free() {
        assert_eq!(elephc_curl_mime_new(-999), 0);
        assert_eq!(elephc_curl_mime_add_part(-999), 0);
        let name = b"f";
        assert_eq!(
            unsafe { elephc_curl_mime_part_field(-999, FIELD_NAME, name.as_ptr(), name.len()) },
            0
        );
        assert_eq!(elephc_curl_mime_post(-999), 0);
        assert_eq!(elephc_curl_mime_abort(-999), 1);
    }

    /// Builds a full `CURLFile`-shaped part (name + local file path + explicit type +
    /// posted filename) against a REAL temp file, pinning the field-kind wiring end to end
    /// at the ABI level (the wire-level `multipart/form-data` assertion is the elephc
    /// codegen test's job).
    #[test]
    fn curlfile_shaped_part_with_a_real_file_succeeds() {
        let mut path = std::env::temp_dir();
        path.push(format!("elephc_curl_mime_test_{}.txt", std::process::id()));
        std::fs::write(&path, b"file contents").expect("write temp fixture file");
        let path_bytes = path.to_string_lossy().into_owned().into_bytes();

        let id = elephc_curl_easy_init();
        assert_ne!(id, 0);
        assert_eq!(elephc_curl_mime_new(id), 1);
        assert_eq!(elephc_curl_mime_add_part(id), 1);
        let name = b"f";
        assert_eq!(
            unsafe { elephc_curl_mime_part_field(id, FIELD_NAME, name.as_ptr(), name.len()) },
            1
        );
        assert_eq!(
            unsafe {
                elephc_curl_mime_part_field(
                    id,
                    FIELD_FILEDATA,
                    path_bytes.as_ptr(),
                    path_bytes.len(),
                )
            },
            1,
            "curl_mime_filedata must succeed for a file that really exists"
        );
        let mime_type = b"text/plain";
        assert_eq!(
            unsafe {
                elephc_curl_mime_part_field(id, FIELD_TYPE, mime_type.as_ptr(), mime_type.len())
            },
            1
        );
        let postname = b"hello.txt";
        assert_eq!(
            unsafe {
                elephc_curl_mime_part_field(
                    id,
                    FIELD_FILENAME,
                    postname.as_ptr(),
                    postname.len(),
                )
            },
            1
        );
        assert_eq!(elephc_curl_mime_post(id), 1);
        assert!(has_attached_mime(id));

        elephc_curl_easy_free(id);
        let _ = std::fs::remove_file(&path);
    }

    /// ANSWERS THE OPEN QUESTION `crate::mime`'s module doc leaves to observation: does
    /// `curl_mime_filedata` validate the file's existence EAGERLY (at this call, mirroring
    /// php-src's own `open_basedir`/`stat` check at `curl_setopt()` time) or LAZILY (only
    /// when the transfer actually tries to read it, surfacing as `CURLE_READ_ERROR` from
    /// `curl_easy_perform`)? Whichever this pinned libcurl 8.21.0 build does is recorded
    /// here as a fact, not assumed — see the task report for what this measured.
    #[test]
    fn filedata_missing_file_behavior_is_observed_and_pinned() {
        let mut missing = std::env::temp_dir();
        missing.push(format!(
            "elephc_curl_mime_missing_{}_does_not_exist.txt",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&missing); // ensure it really is absent
        let path_bytes = missing.to_string_lossy().into_owned().into_bytes();

        let id = elephc_curl_easy_init();
        assert_ne!(id, 0);
        assert_eq!(elephc_curl_mime_new(id), 1);
        assert_eq!(elephc_curl_mime_add_part(id), 1);
        let name = b"f";
        unsafe { elephc_curl_mime_part_field(id, FIELD_NAME, name.as_ptr(), name.len()) };

        let filedata_result = unsafe {
            elephc_curl_mime_part_field(id, FIELD_FILEDATA, path_bytes.as_ptr(), path_bytes.len())
        };
        // MEASURED, not assumed: this pinned libcurl 8.21.0 build's `curl_mime_filedata`
        // DOES validate the file EAGERLY — it fails right here (a non-`CURLE_OK` return,
        // this field-setter answering `0`) for a path that does not exist, presumably
        // because it stats the file immediately to compute the part's size.
        //
        // THIS **IS** A REAL DIVERGENCE FROM PHP, corrected here after review — an earlier
        // version of this comment claimed otherwise and was wrong. php-src does NOT use
        // `curl_mime_filedata()` at all: `ext/curl/interface.c`'s `build_mime_structure_
        // from_hash` reads a `CURLFile`'s local file through `curl_mime_data_cb()`, a
        // read/seek/free CALLBACK triple, so the failure for a missing file surfaces
        // LAZILY, from inside that callback at `curl_exec()` time
        // (`CURLE_ABORTED_BY_CALLBACK`, measured directly against a real PHP 8.4.20 —
        // `curl_setopt()` itself answers `true`). `php_check_open_basedir()` runs during
        // that same construction, but it is a NO-OP whenever `open_basedir` is unset (the
        // common case, including every fixture in this tree), so it does not perform an
        // eager existence check either. elephc's simpler `curl_mime_filedata()`-based
        // design (no custom callback — `crate::callbacks`' machinery is not involved here)
        // fails EAGER, at `curl_setopt()` time, which is what this test pins: an honest,
        // defensible, but DIFFERENT answer to "the file does not exist" than php-src's own.
        // See `src/curl_prelude.rs`'s `__elephc_curl_build_multipart()` doc comment and
        // `tests/codegen/curl/multipart.rs::missing_file_makes_curl_setopt_fail_eagerly`
        // for where this divergence is stated for PHP-facing readers.
        assert_eq!(
            filedata_result, 0,
            "curl_mime_filedata must reject a path that does not exist"
        );

        elephc_curl_easy_free(id);
    }
}

/// Purpose:
/// The MULTI interface's real-libcurl tests: lifecycle, attach/detach, a `file://` transfer
/// driven to completion through `curl_multi_perform`, the completion queue's parked fields,
/// and the error surface.
///
/// Called from:
/// - `cargo test -p elephc-curl` through Rust's test harness, when `ELEPHC_CURL_LIB_DIR` is
///   set (see this module's header).
///
/// Key details:
/// - `file://` again, for the same reason the easy tests use it: no network, and the same
///   write-callback path an HTTP transfer takes.
/// - The completion queue is read through the same one-field-per-call protocol the prelude
///   uses, so these tests pin the ABI shape the compiled PHP actually calls.
#[cfg(elephc_curl_native)]
mod native_multi {
    use crate::abi::{
        elephc_curl_easy_free, elephc_curl_easy_init, elephc_curl_easy_set_url,
        elephc_curl_easy_setopt_long, elephc_curl_easy_take_body,
    };
    use crate::multi::{
        elephc_curl_multi_add, elephc_curl_multi_errno, elephc_curl_multi_free,
        elephc_curl_multi_info_read, elephc_curl_multi_init, elephc_curl_multi_perform,
        elephc_curl_multi_remove, elephc_curl_multi_select, elephc_curl_multi_setopt,
        elephc_curl_multi_strerror, INFO_FIELD_ADVANCE, INFO_FIELD_EASY_ID, INFO_FIELD_MSG,
        INFO_FIELD_QUEUED, INFO_FIELD_RESULT, MULTI_SETOPT_APPLIED, MULTI_SETOPT_INVALID,
        MULTI_SETOPT_UNSUPPORTED,
    };
    use crate::php_layer::CURLOPT_RETURNTRANSFER;

    /// Writes a `file://` fixture and returns (directory, url), for a transfer that needs
    /// no network.
    fn file_fixture(tag: &str, body: &[u8]) -> (std::path::PathBuf, String) {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let suffix = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "elephc-curl-multi-{tag}-{}-{suffix}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("body.txt");
        std::fs::write(&path, body).unwrap();
        let url = format!("file://{}", path.display());
        (dir, url)
    }

    /// Multi ids are their OWN id space: allocated, positive, monotonic, and never reused,
    /// exactly like the easy table's but independent of it.
    #[test]
    fn multi_init_and_free_allocate_independent_ids() {
        let a = elephc_curl_multi_init();
        let b = elephc_curl_multi_init();
        assert_ne!(a, 0, "curl_multi_init should succeed with real libcurl linked");
        assert_ne!(b, 0);
        assert!(b > a, "multi ids are monotonic");
        assert_eq!(elephc_curl_multi_errno(a), 0, "a fresh multi handle reports CURLM_OK");
        elephc_curl_multi_free(a);
        elephc_curl_multi_free(b);
        // Freeing twice is a no-op rather than a double `curl_multi_cleanup`: ids are
        // never reused, so a stale id can only ever miss the table.
        elephc_curl_multi_free(a);
        assert_eq!(elephc_curl_multi_errno(a), 1, "an unknown id reports CURLM_BAD_HANDLE");
    }

    /// Attach, drive to completion, read the queue, detach: the whole multi cycle against a
    /// `file://` fixture, through exactly the calls the prelude makes.
    #[test]
    fn multi_perform_runs_a_transfer_and_queues_its_completion() {
        let (dir, url) = file_fixture("perform", b"multi fixture body\n");
        let multi = elephc_curl_multi_init();
        let easy = elephc_curl_easy_init();
        assert_ne!(multi, 0);
        assert_ne!(easy, 0);
        assert_eq!(unsafe { elephc_curl_easy_set_url(easy, url.as_ptr(), url.len()) }, 1);
        assert_eq!(elephc_curl_easy_setopt_long(easy, CURLOPT_RETURNTRANSFER, 1), 1);

        assert_eq!(elephc_curl_multi_add(multi, easy), 0, "add must report CURLM_OK");
        // A second add of the same handle is CURLM_ADDED_ALREADY (7), not a silent success.
        assert_eq!(elephc_curl_multi_add(multi, easy), 7);
        assert_eq!(elephc_curl_multi_errno(multi), 7, "errno tracks the last operation");

        let mut running = 1;
        let mut guard = 0;
        while running > 0 && guard < 1000 {
            let packed = elephc_curl_multi_perform(multi);
            let code = (packed & 0xFFFF_FFFF) as u32 as i32;
            assert_eq!(code, 0, "perform must report CURLM_OK");
            running = packed >> 32;
            if running > 0 {
                elephc_curl_multi_select(multi, 50);
            }
            guard += 1;
        }
        assert_eq!(running, 0, "the transfer must finish");

        assert_eq!(
            elephc_curl_multi_info_read(multi, INFO_FIELD_ADVANCE),
            1,
            "a completed transfer must queue one message"
        );
        assert_eq!(elephc_curl_multi_info_read(multi, INFO_FIELD_MSG), 1, "CURLMSG_DONE");
        assert_eq!(elephc_curl_multi_info_read(multi, INFO_FIELD_RESULT), 0, "CURLE_OK");
        assert_eq!(
            elephc_curl_multi_info_read(multi, INFO_FIELD_EASY_ID),
            easy,
            "the message must name the easy handle's own bridge id"
        );
        assert_eq!(elephc_curl_multi_info_read(multi, INFO_FIELD_QUEUED), 0);
        assert_eq!(
            elephc_curl_multi_info_read(multi, INFO_FIELD_ADVANCE),
            0,
            "the queue is drained after one message"
        );

        let mut body_ptr: *mut u8 = std::ptr::null_mut();
        let mut body_len = 0usize;
        assert_eq!(unsafe { elephc_curl_easy_take_body(easy, &mut body_ptr, &mut body_len) }, 1);
        let body = unsafe { std::slice::from_raw_parts(body_ptr, body_len) }.to_vec();
        assert_eq!(body, b"multi fixture body\n");
        // READING THE BODY DOES NOT CONSUME IT (php-src's `RETURN_STR_COPY`), which is what
        // makes a second `curl_multi_getcontent()` answer the same bytes.
        let mut second_ptr: *mut u8 = std::ptr::null_mut();
        let mut second_len = 0usize;
        assert_eq!(unsafe { elephc_curl_easy_take_body(easy, &mut second_ptr, &mut second_len) }, 1);
        assert_eq!(second_len, body_len, "the capture buffer survives being read");

        assert_eq!(elephc_curl_multi_remove(multi, easy), 0);
        elephc_curl_multi_free(multi);
        elephc_curl_easy_free(easy);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `curl_multi_add_handle` CLEARS the easy handle's capture buffer, mirroring php-src's
    /// `_php_curl_cleanup_handle`: without it a handle reused across two multi runs would
    /// report both bodies concatenated.
    #[test]
    fn multi_add_resets_the_capture_buffer() {
        let (dir, url) = file_fixture("reset", b"first\n");
        let multi = elephc_curl_multi_init();
        let easy = elephc_curl_easy_init();
        assert_eq!(unsafe { elephc_curl_easy_set_url(easy, url.as_ptr(), url.len()) }, 1);
        assert_eq!(elephc_curl_easy_setopt_long(easy, CURLOPT_RETURNTRANSFER, 1), 1);
        assert_eq!(elephc_curl_multi_add(multi, easy), 0);
        let mut running = 1;
        let mut guard = 0;
        while running > 0 && guard < 1000 {
            running = elephc_curl_multi_perform(multi) >> 32;
            guard += 1;
        }
        let mut ptr: *mut u8 = std::ptr::null_mut();
        let mut len = 0usize;
        assert_eq!(unsafe { elephc_curl_easy_take_body(easy, &mut ptr, &mut len) }, 1);
        assert_eq!(len, b"first\n".len());

        assert_eq!(elephc_curl_multi_remove(multi, easy), 0);
        assert_eq!(elephc_curl_multi_add(multi, easy), 0, "re-adding must succeed");
        let mut ptr2: *mut u8 = std::ptr::null_mut();
        let mut len2 = 1usize;
        assert_eq!(unsafe { elephc_curl_easy_take_body(easy, &mut ptr2, &mut len2) }, 1);
        assert_eq!(len2, 0, "add_handle must clear the previous run's captured body");

        elephc_curl_multi_free(multi);
        elephc_curl_easy_free(easy);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `curl_multi_setopt`'s three-way answer, and the `CURLMcode` message space (which is
    /// NOT `CURLcode`'s).
    #[test]
    fn multi_setopt_and_strerror_report_the_multi_space() {
        let multi = elephc_curl_multi_init();
        assert_eq!(
            elephc_curl_multi_setopt(multi, 6, 4),
            MULTI_SETOPT_APPLIED,
            "CURLMOPT_MAXCONNECTS is a real long option"
        );
        assert_eq!(
            elephc_curl_multi_setopt(multi, 30_009, 1024),
            MULTI_SETOPT_APPLIED,
            "CURLMOPT_CONTENT_LENGTH_PENALTY_SIZE is a real off_t option"
        );
        assert_eq!(
            elephc_curl_multi_setopt(multi, 20_014, 1),
            MULTI_SETOPT_UNSUPPORTED,
            "CURLMOPT_PUSHFUNCTION is a callback this build cannot carry"
        );
        assert_eq!(
            elephc_curl_multi_setopt(multi, 999_999, 1),
            MULTI_SETOPT_INVALID,
            "an unknown option is not a cURL multi option at all"
        );

        let mut buf = vec![0u8; 256];
        let mut len = 0usize;
        assert_eq!(
            unsafe { elephc_curl_multi_strerror(7, buf.as_mut_ptr(), buf.len(), &mut len) },
            1
        );
        let message = String::from_utf8_lossy(&buf[..len]).into_owned();
        assert!(
            message.contains("already added"),
            "CURLM_ADDED_ALREADY's own message, not CURLcode 7's: {message}"
        );
        elephc_curl_multi_free(multi);
    }

    /// Every per-handle multi entry point answers its documented failure value for an
    /// unknown id — never a value a caller could read as success.
    #[test]
    fn unknown_multi_ids_fail_closed() {
        const UNKNOWN: i64 = -999;
        assert_eq!(elephc_curl_multi_add(UNKNOWN, -998), 1, "CURLM_BAD_HANDLE");
        assert_eq!(elephc_curl_multi_remove(UNKNOWN, -998), 1);
        assert_eq!(elephc_curl_multi_perform(UNKNOWN) & 0xFFFF_FFFF, 1);
        assert_eq!(elephc_curl_multi_select(UNKNOWN, 0), -1);
        assert_eq!(elephc_curl_multi_info_read(UNKNOWN, INFO_FIELD_ADVANCE), 0);
        assert_eq!(elephc_curl_multi_errno(UNKNOWN), 1);
        assert_eq!(
            elephc_curl_multi_setopt(UNKNOWN, 6, 1),
            MULTI_SETOPT_UNSUPPORTED,
            "a real option on an unknown handle is a failed apply, not a ValueError"
        );
    }
}

/// Purpose:
/// The SHARE interface's real-libcurl tests: lifecycle, `curl_share_setopt()`'s three-way
/// answer, the error surface, `CURLOPT_SHARE` attach/detach, and — the discriminating
/// ones — that freeing a share while it is still attached DEFERS the real
/// `curl_share_cleanup()` call rather than either forcing it early (a permanent leak once
/// libcurl refuses with `CURLSHE_IN_USE`) or forcing an unlink first (unsafe against an
/// in-flight multi-driven transfer). See `crate::share`'s module doc for the full argument
/// libcurl 8.21.0 supports: `CURLOPT_SHARE` REFCOUNTS, so an in-use share is never
/// corrupted by an early `curl_share_cleanup()` — it is simply, silently, permanently
/// leaked, which is the actual failure mode these tests observe via
/// `crate::share::share_cleanup_result`.
///
/// Called from:
/// - `cargo test -p elephc-curl` through Rust's test harness, when `ELEPHC_CURL_LIB_DIR`
///   is set (see this module's header).
#[cfg(elephc_curl_native)]
mod native_share {
    use crate::abi::{
        elephc_curl_easy_free, elephc_curl_easy_init, elephc_curl_easy_reset,
        elephc_curl_easy_set_url, elephc_curl_easy_setopt_long,
    };
    use crate::multi::{
        elephc_curl_multi_add, elephc_curl_multi_free, elephc_curl_multi_init,
        elephc_curl_multi_perform,
    };
    use crate::php_layer::CURLOPT_RETURNTRANSFER;
    use crate::share::{
        elephc_curl_easy_set_share, elephc_curl_share_errno, elephc_curl_share_free,
        elephc_curl_share_init, elephc_curl_share_persistent_init, elephc_curl_share_setopt,
        share_cleanup_result, SHARE_SETOPT_APPLIED, SHARE_SETOPT_INVALID, SHARE_SETOPT_REFUSED,
    };

    /// `CURL_LOCK_DATA_DNS`, frozen at 3 in `scripts/docs/curl_surface.json`.
    const CURL_LOCK_DATA_DNS: i64 = 3;
    /// `CURLSHOPT_SHARE`, frozen at 1.
    const CURLSHOPT_SHARE: i64 = 1;
    /// `CURLSHE_OK`, libcurl's success code for the share family.
    const CURLSHE_OK: i32 = 0;

    /// Writes a `file://` fixture and returns (directory, url), for a transfer that needs
    /// no network — mirrors `native_multi`'s own helper of the same shape.
    fn file_fixture(tag: &str, body: &[u8]) -> (std::path::PathBuf, String) {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let suffix = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "elephc-curl-share-{tag}-{}-{suffix}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("body.txt");
        std::fs::write(&path, body).unwrap();
        let url = format!("file://{}", path.display());
        (dir, url)
    }

    /// Share ids are their own id space: allocated, positive, monotonic, never reused —
    /// exactly like the multi table's, independent of both it and the easy table.
    #[test]
    fn share_init_and_free_allocate_independent_ids() {
        let a = elephc_curl_share_init();
        let b = elephc_curl_share_init();
        assert_ne!(a, 0, "curl_share_init should succeed with real libcurl linked");
        assert_ne!(b, 0);
        assert!(b > a, "share ids are monotonic");
        elephc_curl_share_free(a);
        elephc_curl_share_free(b);
        // Freeing twice is a no-op, ids are never reused.
        elephc_curl_share_free(a);
    }

    /// `curl_share_setopt()`'s three-way answer: a real `CURLSHOPT_SHARE`/`CURL_LOCK_DATA_*`
    /// pair applies; an unknown option number is `SHARE_SETOPT_INVALID`, matching php-src's
    /// own two-case switch (see `crate::share::share_option_kind`'s doc comment).
    #[test]
    fn share_setopt_applies_and_rejects_honestly() {
        let share = elephc_curl_share_init();
        assert_eq!(
            elephc_curl_share_setopt(share, CURLSHOPT_SHARE, CURL_LOCK_DATA_DNS),
            SHARE_SETOPT_APPLIED
        );
        assert_eq!(elephc_curl_share_errno(share), 0, "CURLSHE_OK");
        assert_eq!(
            elephc_curl_share_setopt(share, 999_999, 1),
            SHARE_SETOPT_INVALID,
            "not a cURL share option at all"
        );
        // CURLSHOPT_LOCKFUNC (3): a real libcurl CURLSHOPT_* number, but not one PHP
        // exposes as a constant, so it is INVALID here too, not merely REFUSED — see
        // this module's header for why there is no "recognized but unsupported" bucket.
        assert_eq!(elephc_curl_share_setopt(share, 3, 1), SHARE_SETOPT_INVALID);
        elephc_curl_share_free(share);
    }

    /// An unrecognized `CURL_LOCK_DATA_*` VALUE (as opposed to option NUMBER) is a real
    /// libcurl-level refusal (`CURLSHE_BAD_OPTION`), reported as `SHARE_SETOPT_REFUSED`
    /// with the code retrievable through `curl_share_errno()` — never a fabricated
    /// warning (this module's header explains why that split matters).
    #[test]
    fn share_setopt_refuses_an_unrecognized_lock_data_value() {
        let share = elephc_curl_share_init();
        let result = elephc_curl_share_setopt(share, CURLSHOPT_SHARE, 999_999);
        assert_eq!(result, SHARE_SETOPT_REFUSED);
        assert_ne!(elephc_curl_share_errno(share), 0, "the real CURLSHcode stays retrievable");
        elephc_curl_share_free(share);
    }

    /// THE DISCRIMINATING TEST for the deferred-free design (Important 3). A share is
    /// attached to an easy handle, then the SHARE is freed FIRST — while the easy handle
    /// is still live and never told to stop using it — and the ACTUAL OUTCOME is observed
    /// through `share_cleanup_result`, not merely inferred from the absence of a crash:
    ///
    /// - Immediately after `elephc_curl_share_free`, the real `curl_share_cleanup()` must
    ///   NOT have run yet (`None`) — this is what distinguishes "deferred" from "forced
    ///   early", and would fail if the old forced-cleanup-regardless-of-attachment code
    ///   ever came back.
    /// - Only once the LAST attachment drains (`elephc_curl_easy_free`, which detaches
    ///   AFTER libcurl's own `curl_easy_cleanup()` has released its reference) does the
    ///   deferred cleanup finally run, and it must succeed (`CURLSHE_OK`) — this is what
    ///   would fail if the bridge's bookkeeping ever desynced from libcurl's real refcount
    ///   (exactly what `elephc_curl_easy_reset`'s pre-fix bug did — see
    ///   `reset_then_free_defers_cleanup_until_the_easy_detaches_or_frees` below for that
    ///   scenario specifically).
    #[test]
    fn freeing_the_share_before_the_easy_handle_defers_cleanup_until_it_frees() {
        let share = elephc_curl_share_init();
        assert_eq!(
            elephc_curl_share_setopt(share, CURLSHOPT_SHARE, CURL_LOCK_DATA_DNS),
            SHARE_SETOPT_APPLIED
        );
        let easy = elephc_curl_easy_init();
        assert_ne!(easy, 0);
        assert_eq!(elephc_curl_easy_set_share(easy, share), 1, "attach must succeed");

        // Free the share WHILE the easy handle still has CURLOPT_SHARE pointed at it.
        elephc_curl_share_free(share);
        assert_eq!(
            share_cleanup_result(share),
            None,
            "curl_share_cleanup must NOT run while an easy handle is still attached"
        );

        // The easy handle must still be perfectly usable — no forced unlink ever touched
        // its CURLOPT_SHARE, so this is an entirely ordinary free from libcurl's point of
        // view, exactly as it would be for a handle that was never shared.
        elephc_curl_easy_free(easy);
        assert_eq!(
            share_cleanup_result(share),
            Some(CURLSHE_OK),
            "freeing the last attached easy handle must finally run the deferred \
             curl_share_cleanup(), and it must succeed now that libcurl's own refcount is \
             genuinely back to zero"
        );
    }

    /// THE CRITICAL-1 REGRESSION TEST. `curl_easy_reset()` does not touch `data->share` at
    /// all (pinned `lib/easy.c:1089`; `crate::share`'s module doc). This test proves the
    /// bridge's OWN bookkeeping agrees: reset must NOT detach the share, so a share freed
    /// afterward is STILL deferred (not cleaned up immediately with libcurl's real
    /// reference still live), and only the easy handle's own eventual free finally
    /// completes it, successfully.
    ///
    /// The bug this pins: an earlier version of `elephc_curl_easy_reset` called
    /// `crate::share::detach_easy` here, believing (wrongly) that reset cleared
    /// `CURLOPT_SHARE` at the libcurl level. That desynced the bridge's `attached` list
    /// from libcurl's real internal refcount, so `elephc_curl_share_free` (finding
    /// `attached` already — wrongly — empty) called `curl_share_cleanup()` immediately,
    /// while the easy handle's real reference was still live: libcurl answered
    /// `CURLSHE_IN_USE`, which the OLD code silently discarded (Critical 2) — a permanent,
    /// invisible leak of the share for the rest of the process. Flip the assertions below
    /// and you reproduce exactly that: `share_cleanup_result(share)` would already be
    /// `Some(1)` (`CURLSHE_IN_USE`) right after `elephc_curl_share_free`, not `None`.
    #[test]
    fn reset_then_free_defers_cleanup_until_the_easy_detaches_or_frees() {
        let (dir, url) = file_fixture("reset-then-free", b"share reset fixture\n");
        let share = elephc_curl_share_init();
        assert_eq!(
            elephc_curl_share_setopt(share, CURLSHOPT_SHARE, CURL_LOCK_DATA_DNS),
            SHARE_SETOPT_APPLIED
        );
        let easy = elephc_curl_easy_init();
        assert_eq!(unsafe { elephc_curl_easy_set_url(easy, url.as_ptr(), url.len()) }, 1);
        assert_eq!(elephc_curl_easy_setopt_long(easy, CURLOPT_RETURNTRANSFER, 1), 1);
        assert_eq!(elephc_curl_easy_set_share(easy, share), 1);

        // curl_reset() must leave the attachment exactly as it was: no bridge-level
        // detach, matching libcurl's own untouched `data->share`. (It DOES reset
        // CURLOPT_URL/RETURNTRANSFER along with every other real libcurl option — those
        // are re-applied below, the same way a real PHP program re-configures a handle
        // after `curl_reset()`; only CURLOPT_SHARE is special in never needing that.)
        assert_eq!(elephc_curl_easy_reset(easy), 1);

        // Freeing the share now must STILL defer: the easy handle is (correctly) still
        // recorded as attached.
        elephc_curl_share_free(share);
        assert_eq!(
            share_cleanup_result(share),
            None,
            "curl_reset() must not have desynced the bridge from libcurl's real attachment; \
             a non-None result here means curl_share_cleanup ran while the easy handle was \
             still genuinely attached (either CURLSHE_IN_USE — the leak Critical 1 reported \
             — or, if it somehow answered CURLSHE_OK, a worse bookkeeping-vs-libcurl mismatch)"
        );

        // The reset handle must still transfer normally once its ordinary options are
        // reapplied — it never lost its SHARE attachment, which is the one thing this
        // fixture is actually probing.
        assert_eq!(unsafe { elephc_curl_easy_set_url(easy, url.as_ptr(), url.len()) }, 1);
        assert_eq!(elephc_curl_easy_setopt_long(easy, CURLOPT_RETURNTRANSFER, 1), 1);
        assert!(elephc_curl_easy_perform_ok(easy));

        // Only NOW — the easy handle's own free — does the deferred cleanup complete.
        elephc_curl_easy_free(easy);
        assert_eq!(
            share_cleanup_result(share),
            Some(CURLSHE_OK),
            "the deferred cleanup must finally run, successfully, once the easy handle \
             that survived curl_reset() is itself freed"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// THE IMPORTANT-1 REGRESSION TEST. `curl_multi_exec()` can leave a transfer genuinely
    /// in flight between PHP statements (it is explicitly non-blocking), so freeing a
    /// share attached to a multi-driven easy handle can happen while that transfer has not
    /// finished. The OLD design force-unlinked `CURLOPT_SHARE` on every attached easy
    /// handle from inside `elephc_curl_share_free`, regardless of whether it was mid-
    /// transfer — a real hazard against `Curl_cpool`/connection-cache internals a live
    /// non-blocking transfer could still be using. The deferred design never touches a
    /// live easy handle's `CURLOPT_SHARE` at all, so this scenario is safe by construction
    /// now: this test proves BOTH that the transfer still completes AND that the share is
    /// only actually destroyed once the easy handle (and the multi handle, for good
    /// measure) are freed afterward.
    #[test]
    fn share_freed_while_attached_via_multi_defers_cleanup_until_the_easy_is_freed() {
        let (dir, url) = file_fixture("multi-in-flight", b"share multi fixture\n");
        let share = elephc_curl_share_init();
        assert_eq!(
            elephc_curl_share_setopt(share, CURLSHOPT_SHARE, CURL_LOCK_DATA_DNS),
            SHARE_SETOPT_APPLIED
        );
        let multi = elephc_curl_multi_init();
        let easy = elephc_curl_easy_init();
        assert_eq!(unsafe { elephc_curl_easy_set_url(easy, url.as_ptr(), url.len()) }, 1);
        assert_eq!(elephc_curl_easy_setopt_long(easy, CURLOPT_RETURNTRANSFER, 1), 1);
        assert_eq!(elephc_curl_easy_set_share(easy, share), 1);
        assert_eq!(elephc_curl_multi_add(multi, easy), 0, "add must report CURLM_OK");

        // Free the share BEFORE the transfer has been driven to completion at all — the
        // multi handle has not even had `curl_multi_perform` called on it yet, so if a
        // transfer were ever going to be genuinely "in flight" when a share is freed
        // out from under it, an attached-but-not-yet-run easy handle is the earliest and
        // least ambiguous point to prove the new design never reaches into it.
        elephc_curl_share_free(share);
        assert_eq!(share_cleanup_result(share), None, "must defer while multi-attached");

        // Drive the transfer to completion. If the removed force-unlink code path were
        // still here, THIS is where a live `CURLOPT_SHARE`-clearing setopt would have run
        // underneath an active (even if not literally mid-read) multi-managed transfer.
        let mut running = 1;
        let mut guard = 0;
        while running > 0 && guard < 1000 {
            running = elephc_curl_multi_perform(multi) >> 32;
            guard += 1;
        }
        assert_eq!(running, 0, "the transfer must finish even though its share was already 'freed'");
        assert_eq!(
            share_cleanup_result(share),
            None,
            "the share must still not be cleaned up: curl_multi_remove/free never touch \
             CURLOPT_SHARE, matching real php-src, which only unshares via curl_setopt() or \
             the easy handle's own destruction"
        );

        elephc_curl_multi_free(multi);
        elephc_curl_easy_free(easy);
        assert_eq!(
            share_cleanup_result(share),
            Some(CURLSHE_OK),
            "freeing the last attached easy handle must finally run the deferred cleanup, \
             successfully"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Small helper so the reset-then-free fixture above reads as one assertion rather
    /// than a bare `elephc_curl_easy_perform` call whose return convention a reader would
    /// have to look up.
    fn elephc_curl_easy_perform_ok(id: i64) -> bool {
        crate::abi::elephc_curl_easy_perform(id) == 1
    }

    /// Repeated calls with an EQUIVALENT (sorted/deduplicated) `CURL_LOCK_DATA_*` set
    /// return the SAME underlying share id — php-src's own semantics — and the id is
    /// never freed by `elephc_curl_share_free` (process-lifetime).
    #[test]
    fn persistent_shares_are_keyed_by_the_sorted_option_set() {
        let a = unsafe {
            let csv = b"3,2";
            elephc_curl_share_persistent_init(csv.as_ptr(), csv.len())
        };
        let b = unsafe {
            // Same two values, reversed order plus a duplicate: must resolve to the
            // identical share id.
            let csv = b"2,3,2";
            elephc_curl_share_persistent_init(csv.as_ptr(), csv.len())
        };
        assert_ne!(a, 0);
        assert_eq!(a, b, "an equivalent option set must return the same persistent share");

        let different = unsafe {
            let csv = b"4";
            elephc_curl_share_persistent_init(csv.as_ptr(), csv.len())
        };
        assert_ne!(different, a, "a different option set must mint a different share");

        // Documented no-op: a persistent share is never actually freed.
        elephc_curl_share_free(a);
        assert_eq!(
            elephc_curl_share_setopt(a, CURLSHOPT_SHARE, CURL_LOCK_DATA_DNS),
            SHARE_SETOPT_APPLIED,
            "the persistent share must still be alive and usable after 'freeing' it"
        );
    }

    /// Every per-handle share entry point answers its documented failure value for an
    /// unknown id — never a value a caller could read as success.
    #[test]
    fn unknown_share_ids_fail_closed() {
        const UNKNOWN: i64 = -999;
        assert_eq!(elephc_curl_share_errno(UNKNOWN), 0, "CURLSHE_OK, nothing happened");
        assert_eq!(
            elephc_curl_share_setopt(UNKNOWN, CURLSHOPT_SHARE, CURL_LOCK_DATA_DNS),
            SHARE_SETOPT_REFUSED
        );
        let easy = elephc_curl_easy_init();
        assert_eq!(elephc_curl_easy_set_share(easy, UNKNOWN), 0);
        elephc_curl_easy_free(easy);
        // A no-op, not a crash.
        elephc_curl_share_free(UNKNOWN);
    }

    /// A PENDING-FREE share id (no live PHP object could legitimately name one — see
    /// `crate::share`'s module doc) is refused by every PHP-facing entry point exactly like
    /// an unknown id, defensively. Not reachable from real PHP source, but cheap to pin
    /// directly at the ABI level.
    #[test]
    fn pending_free_share_ids_are_treated_as_unknown() {
        let share = elephc_curl_share_init();
        assert_eq!(
            elephc_curl_share_setopt(share, CURLSHOPT_SHARE, CURL_LOCK_DATA_DNS),
            SHARE_SETOPT_APPLIED
        );
        let attached = elephc_curl_easy_init();
        assert_eq!(elephc_curl_easy_set_share(attached, share), 1);

        // One attachment still live -> this must defer, leaving the id "pending" rather
        // than removing it from the table outright.
        elephc_curl_share_free(share);
        assert_eq!(share_cleanup_result(share), None);

        assert_eq!(
            elephc_curl_share_setopt(share, CURLSHOPT_SHARE, CURL_LOCK_DATA_DNS),
            SHARE_SETOPT_REFUSED,
            "a pending-free share must not accept further setopt calls"
        );
        assert_eq!(
            elephc_curl_share_errno(share),
            CURLSHE_OK,
            "a pending-free share answers CURLSHE_OK (unknown), not its stale last_errno"
        );
        let other = elephc_curl_easy_init();
        assert_eq!(
            elephc_curl_easy_set_share(other, share),
            0,
            "a pending-free share must refuse a NEW attachment"
        );
        elephc_curl_easy_free(other);

        // Draining the real attachment must still complete the deferred cleanup normally.
        elephc_curl_easy_free(attached);
        assert_eq!(share_cleanup_result(share), Some(CURLSHE_OK));
    }
}
