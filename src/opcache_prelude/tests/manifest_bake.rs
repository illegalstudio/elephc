//! Purpose:
//! Tests entry canonicalization, manifest collection, and targeted rebaking.
//!
//! Called from:
//! - cargo test through Rust's test harness.
//!
//! Key details:
//! - Shared fixtures are imported through the parent OPcache prelude test facade.

use super::*;

    /// `canonical_entry_path` resolves symlinked spellings the way reference PHP's
    /// `path_translated` does, and yields `None` for a path that does not exist.
    #[test]
pub(super) fn canonical_entry_path_resolves_and_reports_missing() {
        use std::io::Write;

        let dir = std::env::temp_dir().join(format!(
            "elephc_opcache_entry_test_{}_{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let main = dir.join("entry.php");
        let mut file = std::fs::File::create(&main).expect("create temp entry");
        file.write_all(b"<?php echo 1;").expect("write temp entry");
        drop(file);

        let resolved = canonical_entry_path(main.to_str().unwrap()).expect("entry must resolve");
        assert_eq!(
            resolved,
            main.canonicalize().unwrap().display().to_string(),
            "the entry path must be canonicalized like __FILE__ and ScriptEntry::path"
        );
        // The canonical entry is always allowed by a prefix of its own directory.
        let parent = Path::new(&resolved).parent().unwrap().display().to_string();
        assert!(!restrict_api_denies(
            Some(&resolved),
            80500,
            &restrict_api_override(&parent)
        ));

        assert!(canonical_entry_path(dir.join("nope.php").to_str().unwrap()).is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `collect_manifest` canonicalizes and stats the entry file, dedupes it against the
    /// autoloaded list, and skips paths that cannot be stat'd (never fabricated).
    #[test]
pub(super) fn collect_manifest_stats_and_dedupes() {
        use std::io::Write;

        let dir = std::env::temp_dir().join(format!(
            "elephc_opcache_manifest_test_{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let main = dir.join("main.php");
        let mut file = std::fs::File::create(&main).expect("create temp main");
        file.write_all(b"<?php echo 1;").expect("write temp main");
        drop(file);

        let canonical = main.canonicalize().expect("canonicalize temp main");

        // The entry file also appears in the autoloaded list, plus a nonexistent path that
        // must be skipped (not fabricated).
        let missing = dir.join("does_not_exist.php");
        let always = [canonical.clone(), missing];

        let manifest = collect_manifest(main.to_str().unwrap(), &[], &always);

        // Exactly one entry: the deduped entry file; the missing path is skipped.
        assert_eq!(manifest.len(), 1);
        assert_eq!(manifest[0].path, canonical.display().to_string());
        // Size of "<?php echo 1;" is 13 bytes.
        assert_eq!(manifest[0].memory_consumption, 13);
        // A plausible recent mtime (> 2020-01-01).
        assert!(manifest[0].timestamp > 1_577_836_800);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The manifest ORDER is entry file, then includes, then autoloaded — each group in the
    /// order its producing pass hands over (both sort) — with duplicates dropped across groups,
    /// first occurrence winning. Pinned here because the baked `scripts` map key order and the
    /// `preload_statistics.scripts` list both follow it, so it must not drift silently.
    #[test]
pub(super) fn collect_manifest_orders_entry_then_includes_then_autoloaded() {
        use std::io::Write;

        let dir = std::env::temp_dir().join(format!(
            "elephc_opcache_manifest_order_{}_{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let write = |name: &str| {
            let path = dir.join(name);
            let mut file = std::fs::File::create(&path).expect("create temp script");
            file.write_all(b"<?php\n").expect("write temp script");
            path.canonicalize().expect("canonicalize temp script")
        };
        // Deliberately named so alphabetical order differs from argument order.
        let main = write("z_main.php");
        let inc_a = write("a_inc.php");
        let inc_b = write("b_inc.php");
        let auto = write("c_auto.php");

        // `inc_b` is ALSO in the autoloaded group (a required file that is autoloadable too):
        // it must appear once, in the include group.
        let manifest = collect_manifest(
            main.to_str().unwrap(),
            &[inc_a.clone(), inc_b.clone()],
            &[auto.clone(), inc_b.clone()],
        );

        let paths: Vec<&str> = manifest.iter().map(|entry| entry.path.as_str()).collect();
        assert_eq!(
            paths,
            vec![
                main.display().to_string().as_str(),
                inc_a.display().to_string().as_str(),
                inc_b.display().to_string().as_str(),
                auto.display().to_string().as_str(),
            ]
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// THE SOUNDNESS PIN for the split injection/baking mechanism (see `bake_manifest`):
    /// name-resolving a re-rendered manifest function IN ISOLATION produces the byte-identical
    /// AST that name-resolving it as part of the whole program does. That equality is what makes
    /// substituting the baked declaration after `name_resolver::resolve` has already run a pure
    /// substitution rather than a semantic change — and it is checked on the REAL rendered
    /// bodies, including the `date(…)` call inside the `scripts` map and the `in_array` /
    /// `realpath` / `fwrite(STDERR, …)` references in the other two.
    #[test]
pub(super) fn substitutes_a_name_resolution_identical_body() {
        let manifest = sample_manifest();
        let bodies = [
            rendered(get_status_declaration(PhpVersion::Php85, true, &manifest, &[], false, None)),
            rendered(is_script_cached_declaration(PhpVersion::Php85, true, &manifest, &[])),
            rendered(compile_file_declaration(PhpVersion::Php85, true, &manifest, &[])),
        ];
        for body in &bodies {
            // Isolated: exactly what `bake_manifest` substitutes.
            let mut parsed = parse_internal(&format!("<?php\n{body}\n"));
            assert_eq!(parsed.len(), 1, "one declaration per body");
            let isolated = resolve_baked_function(parsed.remove(0));
            // In-program: the same declaration name-resolved alongside a namespaced caller,
            // which is the situation the declaration must survive at the injection point.
            let mut program = parse_internal(&format!("<?php\n{body}\n"));
            program.extend(parse(
                "<?php\nnamespace App;\nfunction caller() { return opcache_get_status(); }\n",
            ));
            let resolved = crate::name_resolver::resolve(program)
                .expect("in-program source must name-resolve");
            let in_program = resolved
                .iter()
                .find(|stmt| matches!(&stmt.kind, StmtKind::FunctionDecl { name, .. }
                    if name.to_ascii_lowercase().starts_with("opcache_")))
                .expect("the opcache declaration must survive name resolution at top level");
            assert_eq!(format!("{:?}", isolated), format!("{:?}", in_program));
        }
    }

    /// `bake_manifest` swaps ONLY the recorded sites, and swaps in the full manifest. A program
    /// that declares its own `opcache_get_status()` records no site, so nothing is touched.
    #[test]
pub(super) fn bake_manifest_replaces_only_recorded_sites() {
        let program = parse("<?php $s = opcache_get_status(); $c = opcache_is_script_cached(__FILE__);");
        let (injected, sites) =
            inject_if_used(program, PhpVersion::Php85, true, None, &[], &[], None, false);
        assert!(!sites.is_empty());

        let manifest = sample_manifest();
        let baked = bake_manifest(injected, &sites, PhpVersion::Php85, true, &manifest, &[], None, false);
        let rendered = format!("{:?}", baked);
        assert!(rendered.contains("/srv/app/index.php"));
        assert!(rendered.contains("/srv/app/vendor/autoload_files/helpers.php"));

        // A user-declared `opcache_get_status` is never a bake site.
        let own = parse("<?php function opcache_get_status($x = true) { return false; } $s = opcache_get_status();");
        let (_, own_sites) = inject_if_used(own, PhpVersion::Php85, true, None, &[], &[], None, false);
        assert!(!own_sites.get_status);
    }
