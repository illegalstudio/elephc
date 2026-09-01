//! Purpose:
//! Tests core templates, injection gates, JIT status, and file functions.
//!
//! Called from:
//! - cargo test through Rust's test harness.
//!
//! Key details:
//! - Shared fixtures are imported through the parent OPcache prelude test facade.

use super::*;

    /// Parses source the way `inject_if_used` sees it.
pub(super) fn parse(source: &str) -> Program {
        let tokens = crate::lexer::tokenize(source).expect("test source must tokenize");
        crate::parser::parse(&tokens).expect("test source must parse")
    }

    /// Renders one built declaration back to PHP.
    ///
    /// The assertions below were written against the rendered PHP these functions used to
    /// splice, and they state reference-PHP facts (a key's presence, a baked figure, a key
    /// ORDER) that are properties of the declaration, not of any particular formatting. Reading
    /// them off the printed form keeps every one of those facts under test without a second
    /// source of truth: `synthetic_class::print` is test-only and pins its own faithfulness
    /// with a round trip.
pub(super) fn rendered(declaration: Stmt) -> String {
        crate::synthetic_class::print::print_program(&vec![declaration])
    }

    /// Renders a whole built declaration block back to PHP.
pub(super) fn rendered_block(declarations: Program) -> String {
        crate::synthetic_class::print::print_program(&declarations)
    }

    /// Renders one baked literal (a configuration array, a manifest path list) back to PHP.
pub(super) fn rendered_expr(value: &Expr) -> String {
        crate::synthetic_class::print::print_expr(value)
    }

    /// Parses compiler-generated source with the internal source profile.
pub(super) fn parse_internal(source: &str) -> Program {
        let tokens = crate::lexer::tokenize(source).expect("internal test source must tokenize");
        crate::parser::parse_internal(&tokens).expect("internal test source must parse")
    }

    /// The rendered 8.5 literal tokenizes/parses and carries the 8.5 markers.
    ///
    /// The EXCLUDED directives (`opcache.jit`, `opcache.memory_consumption`, …) are asserted as
    /// PLAIN LITERALS — that is the runtime-override scope rule showing up in the rendered text —
    /// while a reporting-only directive carries its env-override call with the compile-time value
    /// as the `$def` argument.
    #[test]
pub(super) fn renders_parsable_php85_literal() {
        let literal = rendered_expr(&configuration_expr(PhpVersion::Php85, &[]));
        assert!(literal.contains("'opcache.jit' => 'disable'"));
        assert!(literal.contains("'opcache.memory_consumption' => 134217728"));
        assert!(literal.contains(
            "'opcache.max_wasted_percentage' => __elephc_opcache_env_pct('ELEPHC_INI_opcache__max_wasted_percentage', 'ELEPHC_INI_opcache.max_wasted_percentage', 0.05)"
        ));
        assert!(literal.contains(
            "'opcache.file_cache_read_only' => __elephc_opcache_env_bool('ELEPHC_INI_opcache__file_cache_read_only', 'ELEPHC_INI_opcache.file_cache_read_only', false)"
        ));
        assert!(literal.contains("'version' => '8.5.10-dev'"));
        assert!(literal.contains("'opcache_product_name' => 'Zend OPcache'"));
        // The literal must parse as a standalone expression statement.
        let _ = parse(&format!("<?php $c = {literal};"));
    }

    /// The 8.2 literal flips the JIT defaults and drops the 8.5-only directive.
    #[test]
pub(super) fn renders_php82_deltas() {
        let literal = rendered_expr(&configuration_expr(PhpVersion::Php82, &[]));
        assert!(literal.contains("'opcache.jit' => 'tracing'"));
        assert!(literal.contains("'opcache.jit_buffer_size' => 0"));
        // 8.2-only, and reporting-only ⇒ it carries the runtime env-override call.
        assert!(literal.contains(
            "'opcache.consistency_checks' => __elephc_opcache_env_int('ELEPHC_INI_opcache__consistency_checks', 'ELEPHC_INI_opcache.consistency_checks', 0)"
        ));
        assert!(!literal.contains("file_cache_read_only"));
        assert!(literal.contains("'version' => '8.2.0'"));
    }

    /// Injection is skipped for a program that never references either function.
    #[test]
pub(super) fn skips_injection_when_unused() {
        let program = parse("<?php echo 1;");
        let injected = inject_for_test(program.clone(), PhpVersion::Php85, false, None, &[], &[], None, false).0;
        assert_eq!(injected.len(), program.len());
    }

    /// Injection fires when `opcache_get_configuration` is called.
    #[test]
pub(super) fn injects_when_called() {
        let program = parse("<?php $c = opcache_get_configuration();");
        let injected = inject_for_test(program.clone(), PhpVersion::Php85, false, None, &[], &[], None, false).0;
        assert!(injected.len() > program.len());
    }

    /// The baked cache-enabled gate follows the compile-time SAPI: CLI disabled, web enabled,
    /// for every maintained version.
    #[test]
pub(super) fn reset_body_follows_sapi() {
        for version in [
            PhpVersion::Php82,
            PhpVersion::Php83,
            PhpVersion::Php84,
            PhpVersion::Php85,
        ] {
            assert!(!cache_enabled(version, false, &[]));
            assert!(cache_enabled(version, true, &[]));
        }
    }

    /// A default CLI binary that calls `opcache_reset()` injects a body returning
    /// `false`; the same program compiled `--web` returns `true`.
    #[test]
pub(super) fn injects_reset_with_sapi_gated_constant() {
        let program = parse("<?php var_dump(opcache_reset());");

        let cli = inject_for_test(program.clone(), PhpVersion::Php85, false, None, &[], &[], None, false).0;
        assert!(cli.len() > program.len());

        let web = inject_for_test(program.clone(), PhpVersion::Php85, true, None, &[], &[], None, false).0;
        assert!(web.len() > program.len());
    }

    /// The number of declarations [`OPCACHE_STATE_HELPERS`] contributes when any state-touching
    /// OPcache function is injected: the restart latch, the discard set, the discard-aware
    /// `timestamp` reader, the system-timezone resolver and the `asctime` formatter.
    const STATE_HELPER_DECLS: usize = 5;

    /// A program that references only `opcache_reset` does not inject
    /// `opcache_get_configuration`, and vice versa (pay-for-use per function).
    ///
    /// `opcache_reset` reads the in-process restart latch, so it also pulls in the shared state
    /// block — which is emitted ONCE however many of the five state-touching functions are used.
    #[test]
pub(super) fn injection_is_per_function() {
        let reset_only = parse("<?php opcache_reset();");
        let injected = inject_for_test(reset_only.clone(), PhpVersion::Php85, false, None, &[], &[], None, false).0;
        // Exactly one OPcache function plus the one shared state block.
        assert_eq!(injected.len(), reset_only.len() + 1 + STATE_HELPER_DECLS);
    }

    /// The rendered 8.5 `opcache_get_status` body parses and carries the enabled-cache
    /// literals with the class-B invariants intact (memory/interned free = total - used,
    /// the derived `max_cached_keys`, and the default disabled-JIT sub-array).
    #[test]
pub(super) fn renders_parsable_php85_status_web() {
        let body = rendered(get_status_declaration(PhpVersion::Php85, true, &[], &[], false, None));
        // Web SAPI bakes the enabled gate as `true === false` (never returns false).
        assert!(body.contains("if (true === false)"));
        // memory_usage invariant: 134217728 - 6291456 = 127926272.
        assert!(body.contains("'used_memory' => 6291456"));
        assert!(body.contains("'free_memory' => 127926272"));
        assert!(body.contains("'wasted_memory' => 0"));
        // interned_strings_usage invariant: 8388608 - 1048576 = 7340032.
        assert!(body.contains("'buffer_size' => 8388608"));
        assert!(body.contains("'free_memory' => 7340032"));
        // Derived hash capacity for the default max_accelerated_files=10000.
        assert!(body.contains("'max_cached_keys' => 16229"));
        // start_time is a run-time clock reading, not a baked compile-time constant — and it is
        // MEMOIZED in a `static` so repeated calls report the SAME value, as reference PHP does.
        assert!(body.contains("static $__elephc_opcache_start_time = 0;"));
        assert!(body.contains(
            "if ($__elephc_opcache_start_time === 0) { $__elephc_opcache_start_time = time(); }"
        ));
        assert!(body.contains("'start_time' => $__elephc_opcache_start_time,"));
        assert!(
            !body.contains("'start_time' => time()"),
            "the per-call time() read is the bug this replaced"
        );
        // Rates are floats, so `0.0` (not `0`) must be emitted.
        assert!(body.contains("'opcache_hit_rate' => 0.0"));
        // Default JIT (disable) sub-array is entirely zero/false.
        assert!(body.contains("'enabled' => false"));
        assert!(body.contains("'buffer_size' => 0"));
        // scripts precedes jit: the `$status['scripts']` insert is before the jit block.
        let scripts_at = body.find("$status['scripts']").expect("scripts insert");
        let jit_at = body.find("$status['jit']").expect("jit insert");
        assert!(scripts_at < jit_at, "scripts must be inserted before jit");
        // The whole function tokenizes/parses.
        let _ = parse(&format!("<?php {body}"));
    }

    /// The CLI (non-web) 8.5 body bakes the disabled gate `false === false`, so the
    /// function returns `false` before building the array; it still parses.
    #[test]
pub(super) fn renders_php85_status_cli_disabled_gate() {
        let body = rendered(get_status_declaration(PhpVersion::Php85, false, &[], &[], false, None));
        assert!(body.contains("if (false === false)"));
        let _ = parse(&format!("<?php {body}"));
    }

    /// Extracts the rendered `$status['jit'] = [...]` block from a `opcache_get_status()` body,
    /// so a jit assertion cannot accidentally match a key of one of the earlier sub-arrays.
pub(super) fn jit_block(body: &str) -> String {
        let start = body.find("$status['jit']").expect("jit block must be rendered");
        let end = body[start..].find("];").expect("jit block must terminate") + start;
        body[start..end].to_string()
    }

    /// The BASELINE the whole feature must not disturb: on the default 8.5 target
    /// (`opcache.jit = disable`, no `--ini`), the jit sub-array is the all-zero/false array,
    /// byte-identical to what reference PHP 8.5.6 reports for its own default.
    #[test]
pub(super) fn renders_php85_default_jit_all_zero() {
        let body = rendered(get_status_declaration(PhpVersion::Php85, true, &[], &[], false, None));
        assert_eq!(
            jit_block(&body),
            "$status['jit'] = ['enabled' => false, 'on' => false, 'kind' => 0, \
             'opt_level' => 0, 'opt_flags' => 0, 'buffer_size' => 0, 'buffer_free' => 0"
        );
        let _ = parse(&format!("<?php {body}"));
    }

    /// Renders the jit block's seven values for readable assertions.
pub(super) fn jit_values(version: PhpVersion, overrides: &[(String, String)]) -> String {
        let body = rendered(get_status_declaration(version, true, &[], overrides, false, None));
        let block = jit_block(&body);
        let field = |key: &str| {
            let at = block
                .find(&format!("'{key}' => "))
                .unwrap_or_else(|| panic!("jit block must carry {key}"))
                + key.len()
                + 6;
            block[at..]
                .split(',')
                .next()
                .expect("field must terminate")
                .to_string()
        };
        format!(
            "{}/{}/{}/{}/{}/{}/{}",
            field("enabled"),
            field("on"),
            field("kind"),
            field("opt_level"),
            field("opt_flags"),
            field("buffer_size"),
            field("buffer_free"),
        )
    }

    /// The `--ini opcache.jit=<spelling>` overrides render the FULL reference
    /// kind/opt_level/opt_flags mapping while the clamp keeps `enabled`/`on` false and both
    /// buffer figures 0 — reference PHP's own "configured but unavailable" shape.
    #[test]
pub(super) fn renders_overridden_jit_modes_with_unavailable_clamp() {
        let ini = |raw: &str| vec![("opcache.jit".to_string(), raw.to_string())];
        // tracing (= 1254): kind 5, opt_level 4, opt_flags 6.
        assert_eq!(
            jit_values(PhpVersion::Php85, &ini("tracing")),
            "false/false/5/4/6/0/0"
        );
        assert_eq!(
            jit_values(PhpVersion::Php85, &ini("1254")),
            jit_values(PhpVersion::Php85, &ini("tracing"))
        );
        // function (= 1205): kind 0, opt_level 5, opt_flags 6.
        assert_eq!(
            jit_values(PhpVersion::Php85, &ini("function")),
            "false/false/0/5/6/0/0"
        );
        // A hand-written CRTO form decodes digit by digit.
        assert_eq!(
            jit_values(PhpVersion::Php85, &ini("1111")),
            "false/false/1/1/5/0/0"
        );
        // The switched-off spellings stay all-zero, indistinguishable from `disable` under the
        // clamp — which is exactly what reference PHP reports when the JIT is unavailable.
        assert_eq!(
            jit_values(PhpVersion::Php85, &ini("off")),
            "false/false/0/0/0/0/0"
        );
        // `opcache.jit_buffer_size` cannot lift the buffer clamp.
        let tracing_64m = vec![
            ("opcache.jit".to_string(), "tracing".to_string()),
            ("opcache.jit_buffer_size".to_string(), "64M".to_string()),
        ];
        assert_eq!(
            jit_values(PhpVersion::Php85, &tracing_64m),
            "false/false/5/4/6/0/0"
        );
    }

    /// The 8.2/8.3 targets default to `opcache.jit = tracing`, so their DEFAULT jit sub-array
    /// carries the tracing triple under the clamp. Pinned to reference PHP 8.2.31 with Xdebug
    /// loaded and its stock `opcache.jit` default, which reports exactly this array.
    /// An INVALID override on 8.2 is masked by re-applying that default (php-src's INI
    /// two-pass), where the same override on 8.5 would leave its partial residue.
    #[test]
pub(super) fn renders_php82_default_jit_tracing_under_clamp() {
        assert_eq!(jit_values(PhpVersion::Php82, &[]), "false/false/5/4/6/0/0");
        assert_eq!(jit_values(PhpVersion::Php83, &[]), "false/false/5/4/6/0/0");
        // 8.4 flipped the default to `disable`.
        assert_eq!(jit_values(PhpVersion::Php84, &[]), "false/false/0/0/0/0/0");

        let bad = vec![("opcache.jit".to_string(), "1355".to_string())];
        assert_eq!(
            jit_values(PhpVersion::Php82, &bad),
            "false/false/5/4/6/0/0",
            "the tracing default overwrites the rejected value's residue"
        );
        assert_eq!(
            jit_values(PhpVersion::Php85, &bad),
            "false/false/5/5/0/0/0",
            "the disable default leaves the rejected value's residue visible"
        );
        let body = rendered(get_status_declaration(PhpVersion::Php82, true, &[], &[], false, None));
        let _ = parse(&format!("<?php {body}"));
    }

    /// Injection fires for `opcache_get_status`, independently of the other two prelude
    /// functions (pay-for-use per function).
    #[test]
pub(super) fn injects_get_status_per_function() {
        let status_only = parse("<?php var_dump(opcache_get_status());");
        let injected = inject_for_test(status_only.clone(), PhpVersion::Php85, false, None, &[], &[], None, false).0;
        // Exactly one OPcache function plus the one shared state block (`opcache_get_status`
        // reads the restart latch, the discard-aware `timestamp`, and the `asctime` formatter).
        assert_eq!(injected.len(), status_only.len() + 1 + STATE_HELPER_DECLS);
    }

    /// `opcache_is_script_cached` bakes the SAPI gate and the manifest: CLI disabled short-
    /// circuits to `false`; web enabled `realpath`-normalizes `$filename` and tests membership
    /// in the baked manifest. Both bodies parse; the empty manifest renders `[]`.
    #[test]
pub(super) fn is_script_cached_bakes_gate_and_manifest() {
        let cli = rendered(is_script_cached_declaration(PhpVersion::Php85, false, &[], &[]));
        assert!(cli.contains("if (false === false)"));
        assert!(cli.contains("in_array($path, [], true)"));
        let _ = parse(&format!("<?php {cli}"));

        let entries = [ScriptEntry {
            path: "/srv/app/main.php".to_string(),
            timestamp: 1_700_000_000,
            memory_consumption: 4_096,
        }];
        let web = rendered(is_script_cached_declaration(PhpVersion::Php85, true, &entries, &[]));
        // Web enabled → the gate never fires, realpath membership is reached.
        assert!(web.contains("if (true === false)"));
        assert!(web.contains("$rp = realpath($filename)"));
        assert!(web.contains("in_array($path, ['/srv/app/main.php'], true)"));
        let _ = parse(&format!("<?php {web}"));
    }

    /// `opcache_invalidate` bakes the SAPI gate AND the manifest: CLI disabled returns `false`,
    /// web enabled resolves the path (with the empty path going through `getcwd()`, which is what
    /// PHP's `realpath('')` resolves to) and records a discard for a FORCED call on a manifest
    /// member. Both bodies parse.
    #[test]
pub(super) fn invalidate_bakes_sapi_gate_and_manifest() {
        let entries = [ScriptEntry {
            path: "/srv/app/main.php".to_string(),
            timestamp: 1_700_000_000,
            memory_consumption: 4096,
        }];

        let cli = rendered(invalidate_declaration(PhpVersion::Php85, false, &entries, &[], false));
        assert!(cli.contains("if (false === false)"));
        assert!(cli.contains("$rp = realpath($filename)"));
        let _ = parse(&format!("<?php {cli}"));

        let web = rendered(invalidate_declaration(PhpVersion::Php85, true, &entries, &[], false));
        // Web enabled → the gate never fires, the path resolution is reached.
        assert!(web.contains("if (true === false)"));
        assert!(web.contains("$rp = realpath($filename)"));
        // The empty path resolves through `getcwd()`, not `realpath('')` (see INVALIDATE_TEMPLATE).
        assert!(web.contains("if ($filename === '') {"));
        assert!(web.contains("$cwd = getcwd();"));
        // A forced call on a manifest member records the discard.
        assert!(web.contains("if ($force && in_array($path, ['/srv/app/main.php'], true)) {"));
        assert!(web.contains("__elephc_opcache_invalidate_state($path, 1);"));
        let _ = parse(&format!("<?php {web}"));
    }

    /// `opcache_compile_file` bakes the SAPI gate and emits the exact php-src notice text
    /// to STDERR on the disabled path; the enabled path tests `realpath` membership in the
    /// baked manifest (a member → `true`). Both bodies parse.
    #[test]
pub(super) fn compile_file_bakes_sapi_gate_and_notice() {
        let cli = rendered(compile_file_declaration(PhpVersion::Php85, false, &[], &[]));
        assert!(cli.contains("if (false === false)"));
        assert!(cli.contains(
            "Notice: Zend OPcache has not been properly started, can't compile file"
        ));
        assert!(cli.contains("fwrite(STDERR,"));
        let _ = parse(&format!("<?php {cli}"));

        let entries = [ScriptEntry {
            path: "/srv/app/main.php".to_string(),
            timestamp: 1_700_000_000,
            memory_consumption: 4_096,
        }];
        let web = rendered(compile_file_declaration(PhpVersion::Php85, true, &entries, &[]));
        assert!(web.contains("if (true === false)"));
        assert!(web.contains("$rp = realpath($filename)"));
        assert!(web.contains("in_array($path, ['/srv/app/main.php'], true)"));
        let _ = parse(&format!("<?php {web}"));
    }

    /// The three file functions are injected pay-for-use, one per reference, and only when
    /// referenced. Each also pulls in the ONE shared in-process state block (they all consult the
    /// discard set — see `OPCACHE_STATE_HELPERS`).
    #[test]
pub(super) fn injects_file_functions_per_function() {
        for source in [
            "<?php var_dump(opcache_is_script_cached(__FILE__));",
            "<?php var_dump(opcache_invalidate(__FILE__));",
            "<?php var_dump(opcache_compile_file(__FILE__));",
        ] {
            let program = parse(source);
            let injected = inject_for_test(program.clone(), PhpVersion::Php85, false, None, &[], &[], None, false).0;
            // Exactly one OPcache function per referenced name, plus the shared state block.
            assert_eq!(injected.len(), program.len() + 1 + STATE_HELPER_DECLS);
        }
    }
