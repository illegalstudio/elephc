//! Purpose:
//! Tests environment overrides, injection uniqueness, and restrict_api behavior.
//!
//! Called from:
//! - cargo test through Rust's test harness.
//!
//! Key details:
//! - Shared fixtures are imported through the parent OPcache prelude test facade.

use super::*;

    /// Counts top-level declarations of `name` in a program (test helper for the
    /// inject-exactly-once rules below).
pub(super) fn declarations_of(program: &Program, name: &str) -> usize {
        program
            .iter()
            .filter(|stmt| {
                matches!(&stmt.kind, StmtKind::FunctionDecl { name: decl, .. } if decl == name)
            })
            .count()
    }

    /// The runtime env-override block is a self-contained, parsable set of PHP functions covering
    /// the lookup, the per-type normalizers, and both consumer surfaces.
    #[test]
pub(super) fn renders_parsable_env_override_helpers() {
        let helpers = rendered_block(env_override_declarations());
        // The lookup consults the `__` spelling first and the dotted one only as a fallback.
        assert!(helpers.contains("function __elephc_opcache_env(string $u, string $d): string"));
        assert!(helpers.contains("$v = (string) getenv($u);"));
        assert!(helpers.contains("return (string) getenv($d);"));
        // The typed surface (opcache_get_configuration) — one helper per type code.
        for typed in [
            "function __elephc_opcache_env_bool(string $u, string $d, bool $def): bool",
            "function __elephc_opcache_env_int(string $u, string $d, int $def): int",
            "function __elephc_opcache_env_float(string $u, string $d, float $def): float",
            "function __elephc_opcache_env_pct(string $u, string $d, float $def): float",
            "function __elephc_opcache_env_str(string $u, string $d, string $def): string",
        ] {
            assert!(helpers.contains(typed), "missing {typed}");
        }
        // The raw-string surface (ini_get / ini_get_all).
        assert!(helpers.contains(
            "function __elephc_opcache_env_raw(string $u, string $d, string $t, string $def): string"
        ));
        // The scanner and the normalizers mirroring `ini_scanner_value` / `parse_ini_override`.
        // `_bool` and `_int` carry NO `_ok` predicate: the reference handlers they mirror
        // (`zend_ini_parse_bool`, `zend_ini_parse_quantity`) cannot fail, so `_pct` is the only
        // type left with a rejection path.
        for normalizer in [
            "__elephc_ini_scan",
            "__elephc_ini_bool_val",
            "__elephc_ini_isspace",
            "__elephc_ini_digit",
            "__elephc_ini_quantity",
            "__elephc_ini_atoi",
            "__elephc_ini_pct_ok",
            "__elephc_ini_pct_val",
        ] {
            assert!(helpers.contains(normalizer), "missing {normalizer}");
        }
        let _ = parse(&format!("<?php {helpers}"));
    }

    /// `render_directive_value_expr` IS the scope rule in rendered form: an excluded directive
    /// stays a plain literal, a reporting-only one becomes an env-override call whose `$def` is
    /// that same literal.
    #[test]
pub(super) fn directive_value_expr_honors_the_override_scope() {
        // Excluded — the ten directives elephc derives compiled-in behavior from.
        for (name, value, expected) in [
            ("opcache.enable_cli", DirectiveValue::Bool(false), "false"),
            ("opcache.memory_consumption", DirectiveValue::Int(134_217_728), "134217728"),
            ("opcache.jit", DirectiveValue::Str("disable"), "'disable'"),
            ("opcache.preload", DirectiveValue::Str(""), "''"),
        ] {
            assert_eq!(rendered_expr(&directive_runtime_value_expr(name, &value)), expected);
        }
        // Reporting-only — the literal becomes the call's default argument.
        assert_eq!(
            rendered_expr(&directive_runtime_value_expr("opcache.save_comments", &DirectiveValue::Bool(true))),
            "__elephc_opcache_env_bool('ELEPHC_INI_opcache__save_comments', \
             'ELEPHC_INI_opcache.save_comments', true)"
        );
        assert_eq!(
            rendered_expr(&directive_runtime_value_expr("opcache.lockfile_path", &DirectiveValue::Str("/tmp"))),
            "__elephc_opcache_env_str('ELEPHC_INI_opcache__lockfile_path', \
             'ELEPHC_INI_opcache.lockfile_path', '/tmp')"
        );
    }

    /// The env-override block is injected EXACTLY ONCE on CLI — a second copy would be a
    /// redeclaration — whether it is pulled in by `opcache_get_configuration`, by the `opcache.*`
    /// INI dispatcher, or by both at the same time. Under `--web` it is never injected here: the
    /// web prelude owns it (see `render_opcache_env_helpers`).
    #[test]
pub(super) fn env_override_helpers_are_injected_exactly_once() {
        let configuration_only = parse("<?php $c = opcache_get_configuration();");
        let ini_only = parse("<?php echo ini_get('opcache.enable');");
        let both = parse("<?php $c = opcache_get_configuration(); echo ini_get('opcache.enable');");

        for program in [&configuration_only, &ini_only, &both] {
            let cli =
                inject_for_test(program.clone(), PhpVersion::Php85, false, None, &[], &[], None, false).0;
            assert_eq!(
                declarations_of(&cli, "__elephc_opcache_env"),
                1,
                "the env-override block must be injected exactly once on CLI"
            );
            assert_eq!(declarations_of(&cli, "__elephc_opcache_env_raw"), 1);
            // web = true never emits it here; the web prelude bakes it instead.
            let web =
                inject_for_test(program.clone(), PhpVersion::Php85, true, None, &[], &[], None, false).0;
            assert_eq!(declarations_of(&web, "__elephc_opcache_env"), 0);
        }

        // A program that uses neither surface pays nothing.
        let unrelated = parse("<?php echo 1;");
        let none =
            inject_for_test(unrelated.clone(), PhpVersion::Php85, false, None, &[], &[], None, false).0;
        assert_eq!(declarations_of(&none, "__elephc_opcache_env"), 0);
    }

    /// The RESTRICTED `opcache_get_configuration` keeps its dead array exit, which still names the
    /// typed env helpers — so the block has to be injected for it too or the body would not
    /// name-resolve.
    #[test]
pub(super) fn restricted_configuration_still_injects_the_env_helpers() {
        let program = parse("<?php $c = opcache_get_configuration();");
        let overrides = vec![("opcache.restrict_api".to_string(), "/nowhere".to_string())];
        let injected = inject_for_test(
            program,
            PhpVersion::Php85,
            false,
            Some("/tmp/app.php"),
            &[],
            &overrides,
            None,
            false,
        )
        .0;
        assert_eq!(declarations_of(&injected, "__elephc_opcache_env"), 1);
        assert_eq!(declarations_of(&injected, "__elephc_opcache_env_bool"), 1);
    }

    /// A user-declared `ini_get` wins: the CLI wrapper is not injected (no redeclaration).
    #[test]
pub(super) fn cli_ini_get_respects_user_declaration() {
        let program = parse("<?php function ini_get($o): string|false { return 'x'; } echo ini_get('a');");
        let injected = inject_for_test(program.clone(), PhpVersion::Php85, false, None, &[], &[], None, false).0;
        assert_eq!(injected.len(), program.len());
    }

    /// Builds a `--ini opcache.restrict_api=<prefix>` override list.
pub(super) fn restrict_api_override(prefix: &str) -> Vec<(String, String)> {
        vec![(RESTRICT_API_DIRECTIVE.to_string(), prefix.to_string())]
    }

    /// `restrict_api_denies` reproduces php-src's `validate_api_restriction()` byte-compare.
    /// Every case below is PINNED FROM REFERENCE PHP 8.5.6 (see the doc comment on the function
    /// for the exact `php -d` invocations that produced them).
    #[test]
pub(super) fn restrict_api_prefix_rule_matches_reference() {
        let entry = "/private/tmp/ra/foobar/x.php";
        let deny = |prefix: &str| {
            restrict_api_denies(Some(entry), 80500, &restrict_api_override(prefix))
        };

        // Empty prefix disables the restriction entirely — today's default behavior.
        assert!(!deny(""), "empty restrict_api must allow");
        assert!(
            !restrict_api_denies(Some(entry), 80500, &[]),
            "no --ini override at all must allow"
        );

        // Exact directory prefix, and the whole path as the prefix, both allow.
        assert!(!deny("/private/tmp/ra/foobar"));
        assert!(!deny("/private/tmp/ra/foobar/x.php"));
        assert!(!deny("/"), "root prefix allows every absolute entry");

        // PLAIN BYTE PREFIX, not a path-component match: `/private/tmp/ra/foo` ALLOWS an entry
        // under `/private/tmp/ra/foobar/`. Verified on reference PHP, which uses memcmp.
        assert!(
            !deny("/private/tmp/ra/foo"),
            "a partial path component still matches (memcmp, not component-wise)"
        );

        // CASE-SENSITIVE even on a case-insensitive filesystem (memcmp, not a fs lookup).
        assert!(deny("/private/tmp/ra/Foobar"));

        // A prefix LONGER than the entry path can never match.
        assert!(deny("/private/tmp/ra/foobar/x.php/deeper"));

        // A wholly unrelated prefix denies.
        assert!(deny("/nonexistent"));

        // Reference compares the RESOLVED path, so an unresolved spelling of the same file
        // denies (macOS: /tmp is a symlink to /private/tmp).
        assert!(deny("/tmp/ra"));

        // No entry path at all mirrors php-src's null `path_translated` arm: deny — but ONLY
        // when a non-empty prefix is configured.
        assert!(restrict_api_denies(None, 80500, &restrict_api_override("/srv")));
        assert!(!restrict_api_denies(None, 80500, &restrict_api_override("")));
        assert!(!restrict_api_denies(None, 80500, &[]));
    }

    /// The restriction is entry-script-relative, and the directive is version-independent
    /// (`opcache.restrict_api` is registered by every maintained version).
    #[test]
pub(super) fn restrict_api_applies_to_every_version() {
        for version in [
            PhpVersion::Php82,
            PhpVersion::Php83,
            PhpVersion::Php84,
            PhpVersion::Php85,
        ] {
            let id = version.version_id();
            assert!(restrict_api_denies(
                Some("/srv/app/index.php"),
                id,
                &restrict_api_override("/other")
            ));
            assert!(!restrict_api_denies(
                Some("/srv/app/index.php"),
                id,
                &restrict_api_override("/srv/app")
            ));
        }
    }

    /// With the default (empty) `restrict_api` every rendered body is BYTE-IDENTICAL to the
    /// unrestricted rendering — the warning slot is removed whole, newline included. This is the
    /// regression guard for "the default path is untouched".
    #[test]
pub(super) fn default_restrict_api_renders_byte_identical_bodies() {
        let manifest = sample_manifest();
        // An unrelated --ini override must not disturb the slot removal either.
        let unrelated = vec![("opcache.enable_cli".to_string(), "1".to_string())];

        for overrides in [Vec::new(), unrelated, restrict_api_override("")] {
            assert!(!restrict_api_denies(
                Some("/srv/app/index.php"),
                80500,
                &overrides
            ));
            let status = rendered(get_status_declaration(PhpVersion::Php85, true, &manifest, &overrides, false, None));
            // No placeholder survives and no warning leaks into the default body.
            assert!(!status.contains("__RESTRICT_API_WARNING__"));
            assert!(!status.contains("restricted by"));
            // The gate's body is EXACTLY the early return: the warning is not merely silent,
            // it is not in the declaration at all.
            assert!(
                status.contains("if (true === false) { return false; }"),
                "default status gate must carry nothing but the early return: {status}"
            );
            let _ = parse(&format!("<?php {status}"));
        }
    }

    /// A denying `restrict_api` renders the five RESTRICTED bodies: each emits the verbatim
    /// reference warning to STDERR and returns `false`. `opcache_compile_file` is UNTOUCHED —
    /// reference PHP does not guard it (verified: it still returns `true` with no warning under
    /// `restrict_api=/nonexistent`).
    #[test]
pub(super) fn denying_restrict_api_renders_restricted_bodies() {
        let overrides = restrict_api_override("/nonexistent");
        let entry = Some("/srv/app/index.php");
        assert!(restrict_api_denies(entry, 80500, &overrides));

        let program = parse(
            "<?php opcache_get_status(); opcache_get_configuration(); opcache_reset(); \
             opcache_is_script_cached(__FILE__); opcache_invalidate(__FILE__); \
             opcache_compile_file(__FILE__);",
        );
        let injected = inject_for_test(program, PhpVersion::Php85, false, entry, &[], &overrides, None, false).0;
        // NOT `rendered`: this module already has a `rendered()` helper, and a binding of that
        // name shadows it for the rest of the function.
        let debug = format!("{injected:?}");

        // The warning text appears once per restricted function, and never a sixth time.
        // Counted on a QUOTE-FREE slice of the message: the AST's `Debug` rendering escapes the
        // embedded `"restrict_api"` quotes, so the full const would never match here.
        let hits = debug.matches("API is restricted by").count();
        assert_eq!(
            hits, 5,
            "exactly the five restricted functions carry the warning (compile_file must not)"
        );

        // The two array-returning functions keep their dead array exit, so `array|false`
        // narrowing still works for callers.
        let status = rendered(get_status_declaration(PhpVersion::Php85, true, &[], &overrides, true, None));
        assert!(
            status.contains("if (false === false)"),
            "restricted status forces the always-taken gate regardless of SAPI"
        );
        assert!(
            status.contains("return $status;"),
            "the array exit must survive so the signature stays array|false"
        );
        assert!(status.contains(RESTRICT_API_WARNING_TEXT));
        let _ = parse(&format!("<?php {status}"));

        let config = rendered(build::restricted_get_configuration_decl(
            configuration_expr(PhpVersion::Php85, &overrides),
            restrict_api_warning(true).unwrap(),
        ));
        assert!(config.contains("function opcache_get_configuration() {"));
        assert!(config.contains("if (false === false)"));
        assert!(config.contains("'opcache_product_name' => 'Zend OPcache'"));
        let _ = parse(&format!("<?php {config}"));

        // The three bool-returning functions are single-exit: reference type is already `bool`.
        for declaration in [
            build::restricted_reset_decl(restrict_api_warning(true).unwrap()),
            build::restricted_is_script_cached_decl(restrict_api_warning(true).unwrap()),
            build::restricted_invalidate_decl(restrict_api_warning(true).unwrap()),
        ] {
            let body = rendered(declaration);
            assert!(body.contains("): bool {"));
            assert!(body.contains(RESTRICT_API_WARNING_TEXT));
            assert!(body.contains("return false;"));
            let _ = parse(&format!("<?php {body}"));
        }
    }

    /// The warning statement is the verbatim reference text with PHP's `Warning: `
    /// prefix — byte-identical to what the `--web` prelude's `trigger_error(..., E_WARNING)`
    /// would write, so one form serves both SAPIs.
    #[test]
pub(super) fn restrict_api_warning_statement_is_verbatim() {
        let warning = crate::synthetic_class::print::print_program(&vec![restrict_api_warning(
            true,
        )
        .expect("a restricted binary always carries the warning")]);
        // php puts this on STDOUT, with the blank line it opens every diagnostic with.
        assert_eq!(
            warning.trim_end(),
            format!("fwrite(STDOUT, \"\\n\" . 'Warning: {RESTRICT_API_WARNING_TEXT}' . \"\\n\");")
        );
        // Pin the message itself, so a typo in the const cannot pass by matching itself.
        assert_eq!(
            RESTRICT_API_WARNING_TEXT,
            "Zend OPcache API is restricted by \"restrict_api\" configuration directive"
        );
        // Double quotes around restrict_api survive single-quoting unescaped.
        assert!(warning.contains("\"restrict_api\""));
    }
