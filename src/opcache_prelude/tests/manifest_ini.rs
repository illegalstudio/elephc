//! Purpose:
//! Tests manifest rendering and CLI INI helper output.
//!
//! Called from:
//! - cargo test through Rust's test harness.
//!
//! Key details:
//! - Shared fixtures are imported through the parent OPcache prelude test facade.

use super::*;

    /// A two-entry manifest sample used by the manifest-rendering tests.
pub(super) fn sample_manifest() -> Vec<ScriptEntry> {
        vec![
            ScriptEntry {
                path: "/srv/app/index.php".to_string(),
                timestamp: 1_700_000_000,
                memory_consumption: 12_345,
            },
            ScriptEntry {
                path: "/srv/app/vendor/autoload_files/helpers.php".to_string(),
                timestamp: 1_699_999_000,
                memory_consumption: 678,
            },
        ]
    }

    /// The flat manifest-paths literal is a parsable PHP array of the canonical paths, and
    /// an empty manifest renders `[]`.
    #[test]
pub(super) fn renders_manifest_paths_literal() {
        let literal = rendered_expr(&manifest_paths_expr(&sample_manifest()));
        assert_eq!(
            literal,
            "['/srv/app/index.php', '/srv/app/vendor/autoload_files/helpers.php']"
        );
        let _ = parse(&format!("<?php $h = {literal};"));

        assert_eq!(rendered_expr(&manifest_paths_expr(&[])), "[]");
        let _ = parse(&format!("<?php $h = {};", rendered_expr(&manifest_paths_expr(&[]))));
    }

    /// The `scripts` map is keyed by full_path and each entry carries the exact 7-key shape,
    /// reading its two REQUEST-clock fields off `opcache_get_status`'s memoized `static` and its
    /// `timestamp` off the file mtime through the discard-aware reader. Reference PHP 8.5.6
    /// (VERIFIED): `last_used_timestamp == time()`, `revalidate == last_used_timestamp +
    /// opcache.revalidate_freq`, `timestamp == filemtime()`, `last_used ==
    /// asctime(localtime(last_used))`.
    #[test]
pub(super) fn renders_scripts_map_literal() {
        // revalidate_freq = 2 (the 8.5 directive default).
        let map = rendered_expr(&scripts_map_expr(&sample_manifest(), 2, 80500));
        // Keyed by full_path.
        assert!(map.contains("'/srv/app/index.php' => ["));
        assert!(map.contains("'full_path' => '/srv/app/index.php'"));
        // All 7 keys present with integer/int-derived values.
        assert!(map.contains("'hits' => 0"));
        assert!(map.contains("'memory_consumption' => 12345"));
        // The two REQUEST-clock fields read `opcache_get_status`'s memoized `static`, not the
        // mtime — see `render_scripts_map_literal` for the verified reference transcript.
        assert!(map.contains("'last_used' => __elephc_opcache_asctime($__elephc_opcache_start_time)"));
        assert!(map.contains("'last_used_timestamp' => $__elephc_opcache_start_time"));
        // `timestamp` stays the FILE mtime, routed through the discard-aware reader so a
        // force-invalidated entry reports 0 (php-src `zend_accel_discard_script`).
        assert!(map.contains(
            "'timestamp' => __elephc_opcache_script_timestamp('/srv/app/index.php', 1700000000)"
        ));
        // revalidate = the request clock + opcache.revalidate_freq (NOT the mtime + freq).
        assert!(map.contains("'revalidate' => $__elephc_opcache_start_time + 2"));
        // The whole map parses as a PHP expression.
        let _ = parse(&format!("<?php $s = {map};"));

        // Empty manifest → empty map.
        assert_eq!(rendered_expr(&scripts_map_expr(&[], 2, 80500)), "[]");
    }

    /// `opcache_get_status` bakes the manifest count into `num_cached_scripts` /
    /// `num_cached_keys`, splices the scripts map, and grows `used_memory` by the sum of the
    /// per-script memory (keeping `free = total - used - wasted`). The body still parses.
    #[test]
pub(super) fn get_status_bakes_manifest_counts_and_scripts() {
        let manifest = sample_manifest();
        let body = rendered(get_status_declaration(PhpVersion::Php85, true, &manifest, &[], false, None));
        // Two cached scripts / keys.
        assert!(body.contains("'num_cached_scripts' => 2"));
        assert!(body.contains("'num_cached_keys' => 2"));
        // The scripts map is spliced (not the empty literal).
        assert!(body.contains("'full_path' => '/srv/app/index.php'"));
        assert!(body.contains("'full_path' => '/srv/app/vendor/autoload_files/helpers.php'"));
        // used_memory = 6291456 baseline + (12345 + 678) = 6304479.
        assert!(body.contains("'used_memory' => 6304479"));
        // free_memory = 134217728 - 6304479 = 127913249.
        assert!(body.contains("'free_memory' => 127913249"));
        // scripts precedes jit.
        let scripts_at = body.find("$status['scripts']").expect("scripts insert");
        let jit_at = body.find("$status['jit']").expect("jit insert");
        assert!(scripts_at < jit_at);
        let _ = parse(&format!("<?php {body}"));
    }

    /// An empty manifest still renders a valid `opcache_get_status` body: zero counts, an
    /// empty scripts map, and the untouched baseline memory figures.
    #[test]
pub(super) fn get_status_empty_manifest_is_valid() {
        let body = rendered(get_status_declaration(PhpVersion::Php85, true, &[], &[], false, None));
        assert!(body.contains("'num_cached_scripts' => 0"));
        assert!(body.contains("'num_cached_keys' => 0"));
        assert!(body.contains("$status['scripts'] = [];"));
        // Baseline used_memory unchanged when no scripts contribute memory.
        assert!(body.contains("'used_memory' => 6291456"));
        let _ = parse(&format!("<?php {body}"));
    }

    /// The rendered opcache INI helpers parse and carry the raw-string projection: booleans
    /// as "1"/"0", the four non-derivable overrides, an empty string, and the access sets.
    ///
    /// The compile-time raw string is the same in every arm as before; what the runtime
    /// env-override adds is that a REPORTING-ONLY arm wraps it in an
    /// `__elephc_opcache_env_raw(<under>, <dotted>, <type code>, <compile-time raw>)` call while an
    /// EXCLUDED arm (`opcache.enable`, `opcache.memory_consumption`, `opcache.jit`,
    /// `opcache.jit_buffer_size`, `opcache.preload`, …) still returns the bare literal.
    #[test]
pub(super) fn renders_parsable_opcache_ini_helpers() {
        let helpers = rendered_block(ini_helper_declarations(PhpVersion::Php85, &[]));
        // EXCLUDED directives keep the bare raw-string literal.
        assert!(helpers.contains("if ($option === 'opcache.enable') { return '1'; }"));
        assert!(helpers.contains("if ($option === 'opcache.memory_consumption') { return '128'; }"));
        assert!(helpers.contains("if ($option === 'opcache.jit_buffer_size') { return '64M'; }"));
        assert!(helpers.contains("if ($option === 'opcache.jit') { return 'disable'; }"));
        assert!(helpers.contains("if ($option === 'opcache.preload') { return ''; }"));
        // Reporting-only directives: raw strings (not the normalized configuration values) behind
        // the runtime env-override call, one type code each.
        assert!(helpers.contains(
            "if ($option === 'opcache.protect_memory') { return __elephc_opcache_env_raw('ELEPHC_INI_opcache__protect_memory', 'ELEPHC_INI_opcache.protect_memory', 'b', '0'); }"
        ));
        assert!(helpers.contains(
            "if ($option === 'opcache.max_wasted_percentage') { return __elephc_opcache_env_raw('ELEPHC_INI_opcache__max_wasted_percentage', 'ELEPHC_INI_opcache.max_wasted_percentage', 'p', '5'); }"
        ));
        assert!(helpers.contains(
            "if ($option === 'opcache.optimization_level') { return __elephc_opcache_env_raw('ELEPHC_INI_opcache__optimization_level', 'ELEPHC_INI_opcache.optimization_level', 'i', '0x7FFEBFFF'); }"
        ));
        assert!(helpers.contains(
            "if ($option === 'opcache.jit_prof_threshold') { return __elephc_opcache_env_raw('ELEPHC_INI_opcache__jit_prof_threshold', 'ELEPHC_INI_opcache.jit_prof_threshold', 'f', '0.005'); }"
        ));
        assert!(helpers.contains(
            "if ($option === 'opcache.error_log') { return __elephc_opcache_env_raw('ELEPHC_INI_opcache__error_log', 'ELEPHC_INI_opcache.error_log', 's', ''); }"
        ));
        // The helper functions are present and the whole block parses.
        assert!(helpers.contains("function __elephc_opcache_ini_string(string $option): string|false"));
        assert!(helpers.contains("function __elephc_opcache_ini_access(string $option): int"));
        assert!(helpers.contains("function __elephc_opcache_ini_keys(): array"));
        assert!(helpers.contains("function __elephc_opcache_ini_all_details(): array"));
        assert!(helpers.contains("function __elephc_opcache_ini_all_plain(): array"));
        let _ = parse(&format!("<?php {helpers}"));
    }

    /// Extracts the PHP string literals from the rendered `__elephc_opcache_ini_keys()` body.
pub(super) fn rendered_ini_keys(helpers: &str) -> Vec<String> {
        let body = helpers
            .split("function __elephc_opcache_ini_keys(): array {")
            .nth(1)
            .expect("keys helper must be rendered");
        let literal = body
            .split_once('[')
            .and_then(|(_, rest)| rest.split_once(']'))
            .expect("keys helper must render a list literal")
            .0;
        literal
            .split(", ")
            .map(|entry| entry.trim().trim_matches('\'').to_string())
            .collect()
    }

    /// `__elephc_opcache_ini_keys()` renders the directive names SORTED ASCENDING, matching
    /// reference PHP's `ini_get_all` key order, while `opcache_directives()` itself keeps
    /// REGISTRATION order (what `opcache_get_configuration()['directives']` reports). The two
    /// orders must differ — if they ever coincide this test still passes, so the registration
    /// list is asserted to be un-sorted on 8.5 to prove the sort is doing real work.
    #[test]
pub(super) fn ini_keys_are_sorted_but_directive_table_is_not() {
        let helpers = rendered_block(ini_helper_declarations(PhpVersion::Php85, &[]));
        let keys = rendered_ini_keys(&helpers);

        let registration: Vec<String> = opcache_directives(80500)
            .iter()
            .map(|(name, _)| (*name).to_string())
            .collect();
        assert_eq!(
            keys.len(),
            registration.len(),
            "sorting must not add or drop directives"
        );

        let mut expected = registration.clone();
        expected.sort();
        assert_eq!(keys, expected, "rendered ini_get_all keys must be sorted");
        assert_eq!(keys[0], "opcache.blacklist_filename");
        assert_eq!(keys[keys.len() - 1], "opcache.validate_timestamps");

        // The registration order is genuinely different, so the sort is load-bearing and
        // opcache_directives() was left untouched.
        assert_ne!(
            registration, expected,
            "registration order must stay unsorted (opcache_get_configuration relies on it)"
        );
        assert_eq!(
            registration[0], "opcache.enable",
            "registration order still starts at opcache.enable"
        );
    }

    /// `render_ini_module_known` renders the known-module predicate from
    /// `CORE_LOADED_EXTENSIONS`, LOWERCASED so the comparison is verbatim against php-src's
    /// lowercase registry keys, and adds `'session'` only for the web SAPI. Every core
    /// extension must appear, and the canonical mixed-case spellings must NOT.
    #[test]
pub(super) fn module_known_list_is_lowercased_core_extensions() {
        let cli = rendered(ini_module_known_declaration(false));
        let web = rendered(ini_module_known_declaration(true));

        for name in crate::codegen::lower_inst::builtins::CORE_LOADED_EXTENSIONS {
            let lowered = name.to_ascii_lowercase();
            assert!(
                cli.contains(&format!("$m === '{lowered}'")),
                "CLI known-module list must contain {lowered}"
            );
            assert!(
                web.contains(&format!("$m === '{lowered}'")),
                "web known-module list must contain {lowered}"
            );
        }
        // Verbatim, not case-folded: the mixed-case spellings never appear as literals.
        assert!(!cli.contains("'Zend OPcache'"));
        assert!(!cli.contains("'Core'"));
        assert!(!cli.contains("'SPL'"));
        // 'session' is a --web-only module.
        assert!(!cli.contains("$m === 'session'"));
        assert!(web.contains("$m === 'session'"));
        // The parameter is nullable so `$extension !== null` need not narrow ?string to Str.
        assert!(cli.contains("function __elephc_ini_module_known(?string $m): bool"));
        let _ = parse(&format!("<?php {cli}"));
        let _ = parse(&format!("<?php {web}"));
    }

    /// The CLI `ini_get_all` wrapper is injected with its known-module predicate, drops the
    /// return type hint (reference PHP is `array|false`; the hint is omitted so ordinary union
    /// return inference handles the exits), and dispatches to the two single-shape helpers
    /// rather than branching on `$details` inside a loop.
    #[test]
pub(super) fn cli_ini_get_all_renders_filter_dispatch() {
        let program = parse("<?php var_dump(ini_get_all(null, false));");
        let injected = inject_if_used(program.clone(), PhpVersion::Php85, false, None, &[], &[], None, false).0;
        let rendered = format!("{injected:?}");
        assert!(injected.len() > program.len(), "ini_get_all must be injected");
        // The predicate is injected alongside the wrapper.
        assert!(rendered.contains("__elephc_ini_module_known"));
        assert!(rendered.contains("__elephc_opcache_ini_all_details"));
        assert!(rendered.contains("__elephc_opcache_ini_all_plain"));
    }

    /// The 8.2 helpers flip the version-dependent raw strings (jit tracing, buffer 0).
    #[test]
pub(super) fn opcache_ini_helpers_follow_version() {
        let helpers = rendered_block(ini_helper_declarations(PhpVersion::Php82, &[]));
        assert!(helpers.contains("if ($option === 'opcache.jit') { return 'tracing'; }"));
        assert!(helpers.contains("if ($option === 'opcache.jit_buffer_size') { return '0'; }"));
        // 8.2-only directive is present in the dispatch, and is reporting-only ⇒ overridable.
        assert!(helpers.contains(
            "if ($option === 'opcache.consistency_checks') { return __elephc_opcache_env_raw('ELEPHC_INI_opcache__consistency_checks', 'ELEPHC_INI_opcache.consistency_checks', 'i', '0'); }"
        ));
        let _ = parse(&format!("<?php {helpers}"));
    }

    /// CLI (`web = false`) injects the opcache ini_get wrapper plus the shared helpers when
    /// `ini_get` is referenced; `--web` does not (web_prelude owns the ini surface there).
    #[test]
pub(super) fn cli_injects_ini_get_opcache_wrapper() {
        let program = parse("<?php echo ini_get('opcache.enable');");

        let cli = inject_if_used(program.clone(), PhpVersion::Php85, false, None, &[], &[], None, false).0;
        assert!(cli.len() > program.len());
        // web = true must not inject the CLI wrappers (would redeclare web_prelude's ini_get).
        let web = inject_if_used(program.clone(), PhpVersion::Php85, true, None, &[], &[], None, false).0;
        assert_eq!(web.len(), program.len());
    }
