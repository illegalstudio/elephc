//! Purpose:
//! Tests preload verdicts, symbols, statistics, and status insertion.
//!
//! Called from:
//! - cargo test through Rust's test harness.
//!
//! Key details:
//! - Shared fixtures are imported through the parent OPcache prelude test facade.

use super::*;

    // ---------------------------------------------------------------------------------------
    // `opcache.preload` — the compile-time verdict and the rendered `preload_statistics` block.
    // Every expectation is pinned to reference PHP 8.5.6 (Homebrew, `Zend OPcache` loaded); the
    // probe commands are recorded on `PreloadVerdict` / `PreloadStatistics`.
    // ---------------------------------------------------------------------------------------

    /// Builds an `--ini` override list for `opcache.preload`.
pub(super) fn preload_override(path: &str) -> Vec<(String, String)> {
        vec![("opcache.preload".to_string(), path.to_string())]
    }

    /// Creates a temp dir holding a real file, and returns `(dir, canonical file path)`.
pub(super) fn temp_preload_file(tag: &str) -> (PathBuf, String) {
        use std::io::Write;

        let dir = std::env::temp_dir().join(format!(
            "elephc_opcache_preload_{tag}_{}_{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("pre.php");
        let mut handle = std::fs::File::create(&file).expect("create temp preload file");
        handle.write_all(b"<?php\n").expect("write temp preload file");
        drop(handle);
        let canonical = file.canonicalize().expect("canonicalize temp preload file");
        (dir, canonical.display().to_string())
    }

    /// THE DEFAULT: an empty `opcache.preload` never preloads, whatever the SAPI — so no
    /// `preload_statistics` key is ever emitted on a stock build. This is the row the whole
    /// feature must not disturb.
    #[test]
pub(super) fn empty_preload_directive_never_preloads() {
        for version in [
            PhpVersion::Php82,
            PhpVersion::Php83,
            PhpVersion::Php84,
            PhpVersion::Php85,
        ] {
            for web in [false, true] {
                assert_eq!(
                    preload_verdict(version, web, &[], &[]),
                    PreloadVerdict::NotPreloading,
                    "the default empty opcache.preload must not preload ({version:?}, web={web})"
                );
            }
        }
        // An explicit empty override is the same thing (verified against reference
        // `-d opcache.preload=`, which reports no `preload_statistics` key).
        assert_eq!(
            preload_verdict(PhpVersion::Php85, true, &preload_override(""), &[]),
            PreloadVerdict::NotPreloading
        );
    }

    /// CACHE DISABLED: a set `opcache.preload` is ignored ENTIRELY — including a path that does
    /// not exist, which must NOT become a compile error. Pinned to reference PHP, where
    /// `opcache.enable_cli=0` with a missing preload path runs cleanly and exits 0.
    #[test]
pub(super) fn disabled_cache_ignores_preload_entirely() {
        let (dir, file) = temp_preload_file("disabled");

        // CLI defaults to `opcache.enable_cli=0` → disabled.
        assert_eq!(
            preload_verdict(PhpVersion::Php85, false, &preload_override(&file), &[]),
            PreloadVerdict::NotPreloading
        );
        // A missing path is not even looked at when the cache is off.
        let missing = dir.join("nope.php").display().to_string();
        let verdict = preload_verdict(PhpVersion::Php85, false, &preload_override(&missing), &[]);
        assert_eq!(verdict, PreloadVerdict::NotPreloading);
        assert!(verdict.compile_error().is_none());
        assert!(verdict.compile_warning().is_none());

        // Explicitly disabling the web cache reaches the same row.
        let mut overrides = preload_override(&missing);
        overrides.push(("opcache.enable".to_string(), "0".to_string()));
        assert_eq!(
            preload_verdict(PhpVersion::Php85, true, &overrides, &[]),
            PreloadVerdict::NotPreloading
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// CACHE ENABLED + UNRESOLVABLE PATH: a compile ERROR naming the directive and the path — the
    /// AOT equivalent of reference PHP's startup fatal `Failed opening required '<path>'`.
    /// A DIRECTORY is unresolvable too (reference cannot `require` one either).
    #[test]
pub(super) fn enabled_cache_with_missing_preload_is_a_compile_error() {
        let (dir, _file) = temp_preload_file("missing");
        let missing = dir.join("nope.php").display().to_string();

        let verdict = preload_verdict(PhpVersion::Php85, true, &preload_override(&missing), &[]);
        assert_eq!(
            verdict,
            PreloadVerdict::Unresolvable {
                requested: missing.clone()
            }
        );
        let message = verdict.compile_error().expect("an unresolvable path must error");
        assert!(message.contains("opcache.preload"), "{message}");
        assert!(message.contains(&missing), "the message must name the path: {message}");
        assert!(
            message.contains("failed opening required"),
            "the message must echo reference's fatal wording: {message}"
        );
        // An error, not a warning.
        assert!(verdict.compile_warning().is_none());

        // A directory does not resolve to a preloadable file.
        let dir_verdict = preload_verdict(
            PhpVersion::Php85,
            true,
            &preload_override(&dir.display().to_string()),
            &[],
        );
        assert!(matches!(dir_verdict, PreloadVerdict::Unresolvable { .. }));

        // `--ini opcache.enable_cli=1` reaches the same row on a CLI target.
        let mut overrides = preload_override(&missing);
        overrides.push(("opcache.enable_cli".to_string(), "1".to_string()));
        assert!(matches!(
            preload_verdict(PhpVersion::Php85, false, &overrides, &[]),
            PreloadVerdict::Unresolvable { .. }
        ));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// CACHE ENABLED + RESOLVABLE PATH: preloading, with `in_manifest` deciding the warning.
    /// A manifest member is silent; a resolvable file outside the manifest warns but still
    /// compiles (preloading a file this program never compiles in is legitimate).
    #[test]
pub(super) fn enabled_cache_with_resolvable_preload_preloads() {
        let (dir, file) = temp_preload_file("resolvable");

        // Outside the manifest → warning, but no error.
        let outside = preload_verdict(PhpVersion::Php85, true, &preload_override(&file), &[]);
        assert_eq!(
            outside,
            PreloadVerdict::Preloading {
                resolved: file.clone(),
                in_manifest: false
            }
        );
        assert!(outside.compile_error().is_none(), "a resolvable path must never error");
        let warning = outside
            .compile_warning()
            .expect("a preload file outside the manifest must warn");
        assert!(warning.contains("opcache.preload"), "{warning}");
        assert!(warning.contains(&file), "the warning must name the path: {warning}");

        // In the manifest → completely silent.
        let manifest = [ScriptEntry {
            path: file.clone(),
            timestamp: 1_700_000_000,
            memory_consumption: 6,
        }];
        let inside = preload_verdict(PhpVersion::Php85, true, &preload_override(&file), &manifest);
        assert_eq!(
            inside,
            PreloadVerdict::Preloading {
                resolved: file.clone(),
                in_manifest: true
            }
        );
        assert!(inside.compile_error().is_none());
        assert!(inside.compile_warning().is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `collect_preload_symbols` reports FULLY-QUALIFIED user names in ORIGINAL CASE with no
    /// leading `\` — the reference spelling (VERIFIED: `My\Space\MixedCaseFn`). Interfaces,
    /// traits and enums all land under `classes`, as reference PHP does. Both the statement and
    /// the brace form of `namespace` are handled, and duplicates are dropped case-insensitively.
    #[test]
pub(super) fn collect_preload_symbols_qualifies_and_dedupes() {
        let program = parse(
            "<?php\n\
             function GlobalFn() {}\n\
             class GlobalClass {}\n\
             namespace My\\Space;\n\
             function MixedCaseFn() {}\n\
             class MixedCaseClass {}\n\
             interface MyIface {}\n\
             trait MyTrait {}\n\
             enum MyEnum {}\n",
        );
        let symbols = collect_preload_symbols(&program);
        assert_eq!(
            symbols.functions,
            vec!["GlobalFn".to_string(), "My\\Space\\MixedCaseFn".to_string()]
        );
        assert_eq!(
            symbols.classes,
            vec![
                "GlobalClass".to_string(),
                "My\\Space\\MixedCaseClass".to_string(),
                "My\\Space\\MyIface".to_string(),
                "My\\Space\\MyTrait".to_string(),
                "My\\Space\\MyEnum".to_string(),
            ]
        );

        // The brace form scopes the namespace to its own body only.
        let braced = parse(
            "<?php\n\
             namespace A { function InA() {} }\n\
             namespace B { function InB() {} }\n",
        );
        let braced = collect_preload_symbols(&braced);
        assert_eq!(
            braced.functions,
            vec!["A\\InA".to_string(), "B\\InB".to_string()]
        );

        // A program that declares nothing reports nothing.
        assert!(collect_preload_symbols(&parse("<?php echo 1;"))
            .functions
            .is_empty());
    }

    /// `preload_statistics` is `None` for every non-preloading verdict, and derives its four
    /// fields from the manifest plus the collected symbols when preloading.
    #[test]
pub(super) fn preload_statistics_derives_from_manifest_and_symbols() {
        let symbols = collect_preload_symbols(&parse("<?php function f() {} class C {}"));
        let manifest = sample_manifest();

        assert!(preload_statistics(&PreloadVerdict::NotPreloading, &manifest, &symbols).is_none());
        assert!(preload_statistics(
            &PreloadVerdict::Unresolvable {
                requested: "/nope".to_string()
            },
            &manifest,
            &symbols
        )
        .is_none());

        let stats = preload_statistics(
            &PreloadVerdict::Preloading {
                resolved: "/srv/app/index.php".to_string(),
                in_manifest: true,
            },
            &manifest,
            &symbols,
        )
        .expect("a preloading verdict must produce statistics");
        // Σ of the sample manifest's per-script memory: 12345 + 678.
        assert_eq!(stats.memory_consumption, 13_023);
        assert_eq!(stats.functions, vec!["f".to_string()]);
        assert_eq!(stats.classes, vec!["C".to_string()]);
        assert_eq!(
            stats.scripts,
            vec![
                "/srv/app/index.php".to_string(),
                "/srv/app/vendor/autoload_files/helpers.php".to_string(),
            ]
        );
    }

    /// The RENDERED `preload_statistics` literal: the VERIFIED reference key ORDER
    /// (`memory_consumption`, `functions`, `classes`, `scripts`), inserted BETWEEN
    /// `opcache_statistics` and `scripts`, and BEFORE `jit`.
    #[test]
pub(super) fn renders_preload_statistics_in_reference_key_order() {
        let symbols = collect_preload_symbols(&parse(
            "<?php namespace App; function helper() {} class Widget {}",
        ));
        let manifest = sample_manifest();
        let stats = preload_statistics(
            &PreloadVerdict::Preloading {
                resolved: "/srv/app/index.php".to_string(),
                in_manifest: true,
            },
            &manifest,
            &symbols,
        )
        .expect("statistics");

        let body =
            rendered(get_status_declaration(PhpVersion::Php85, true, &manifest, &[], false, Some(&stats)));

        assert!(body.contains("$status['preload_statistics'] = ["), "{body}");
        assert!(body.contains("'memory_consumption' => 13023,"), "{body}");
        assert!(body.contains("'functions' => ['App\\\\helper'],"), "{body}");
        assert!(body.contains("'classes' => ['App\\\\Widget'],"), "{body}");
        assert!(
            body.contains(
                "'scripts' => ['/srv/app/index.php', '/srv/app/vendor/autoload_files/helpers.php']"
            ),
            "{body}"
        );

        // Key ORDER inside the block, and the block's position in the status array.
        let block_at = body.find("$status['preload_statistics']").expect("block");
        let mem_at = body[block_at..].find("'memory_consumption'").expect("mem");
        let fns_at = body[block_at..].find("'functions'").expect("functions");
        let cls_at = body[block_at..].find("'classes'").expect("classes");
        let scr_at = body[block_at..].find("'scripts'").expect("scripts");
        assert!(mem_at < fns_at && fns_at < cls_at && cls_at < scr_at, "{body}");

        let stats_map_at = body.find("'opcache_statistics' =>").expect("opcache_statistics");
        let scripts_at = body.find("$status['scripts']").expect("scripts insert");
        let jit_at = body.find("$status['jit']").expect("jit insert");
        assert!(
            stats_map_at < block_at && block_at < scripts_at && scripts_at < jit_at,
            "preload_statistics must sit between opcache_statistics and scripts: {body}"
        );

        // The whole function still tokenizes and parses.
        let _ = parse(&format!("<?php {body}"));
    }

    /// A program with NO user functions/classes renders the block WITHOUT the `functions` and
    /// `classes` keys — reference PHP omits them entirely when empty rather than reporting empty
    /// arrays (VERIFIED by preloading a file containing only `<?php`).
    #[test]
pub(super) fn renders_preload_statistics_omitting_empty_symbol_lists() {
        let symbols = collect_preload_symbols(&parse("<?php echo 1;"));
        let manifest = sample_manifest();
        let stats = preload_statistics(
            &PreloadVerdict::Preloading {
                resolved: "/srv/app/index.php".to_string(),
                in_manifest: true,
            },
            &manifest,
            &symbols,
        )
        .expect("statistics");

        let rendered = rendered_expr(&preload_statistics_expr(&stats));
        assert!(rendered.contains("'memory_consumption' => 13023,"), "{rendered}");
        assert!(rendered.contains("'scripts' => ["), "{rendered}");
        assert!(
            !rendered.contains("'functions'"),
            "an empty functions list must be OMITTED, not reported as []: {rendered}"
        );
        assert!(
            !rendered.contains("'classes'"),
            "an empty classes list must be OMITTED, not reported as []: {rendered}"
        );

        // Only `classes` empty → `functions` present, `classes` absent.
        let fns_only = collect_preload_symbols(&parse("<?php function f() {}"));
        let stats = preload_statistics(
            &PreloadVerdict::Preloading {
                resolved: "/srv/app/index.php".to_string(),
                in_manifest: true,
            },
            &manifest,
            &fns_only,
        )
        .expect("statistics");
        let rendered = rendered_expr(&preload_statistics_expr(&stats));
        assert!(rendered.contains("'functions' => ['f'],"), "{rendered}");
        assert!(!rendered.contains("'classes'"), "{rendered}");
    }

    /// THE BASELINE: with no preloading, `opcache_get_status` renders BYTE-IDENTICALLY to the
    /// template — the `__PRELOAD_STATISTICS__` slot is removed WHOLE (newline included), so the
    /// default build carries not even a whitespace diff. Mirrors
    /// `default_restrict_api_renders_byte_identical_bodies`.
    #[test]
pub(super) fn absent_preload_renders_byte_identical_status_body() {
        let manifest = sample_manifest();
        let body =
            rendered(get_status_declaration(PhpVersion::Php85, true, &manifest, &[], false, None));
        assert!(
            !body.contains("preload_statistics"),
            "no preload key may appear on the default path: {body}"
        );
        // The statement right after the status literal is the `$include_scripts` guard:
        // nothing sits between them.
        assert!(body.contains("];\n    if ($include_scripts) {"), "{body}");
        let _ = parse(&format!("<?php {body}"));
    }
