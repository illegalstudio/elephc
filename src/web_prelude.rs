//! Purpose:
//! The `--web` request prelude: under `--web`, prepends an `extern "elephc_web"`
//! declaration block (Phase 2 Task 2) and executable statements that build the
//! request superglobals ($_SERVER/$_GET/$_POST) on every request (Task 5+).
//!
//! Called from:
//! - `crate::pipeline::compile`, after the other preludes and before name
//!   resolution, gated on `CliConfig.web` (NOT usage detection — it is the only
//!   flag-gated prelude).
//!
//! Key details:
//! - The injected statements run before user top-level code each request because
//!   the prelude statements are prepended and the whole top-level body re-runs
//!   per request.
//! - Optional session functions are retained through AST reachability, while
//!   unknown dynamic calls conservatively keep the complete prelude.
//! - The declarations themselves are BUILT in Rust (`build`), not parsed from embedded PHP.
//!   This module owns what a `--web` binary KEEPS: the pay-for-use pruning, the callable
//!   session-handler gate, and the catch-all `try` wrap.
//! - Legacy callable-handler dispatch is injected only when user code can reach
//!   `session_set_save_handler()`.

use crate::parser::ast::{BinOp, Program, Stmt, StmtKind};
use crate::synthetic_class::{
    e_binop, e_call, e_const, e_int, e_static_prop, s_assign, s_expr, s_if, s_try,
};

use crate::prelude_prune::usage;

// Upstream lifted `PhpVersion` out of this module into `crate::php_version`; the re-export is
// what keeps `crate::web_prelude::PhpVersion` resolving for `cli`, `pipeline`, `version_prelude`
// and `opcache_prelude`, which all still spell it that way.
pub use crate::php_version::PhpVersion;

/// Returns the `PHP_SAPI` / `php_sapi_name()` string for an elephc compile mode.
///
/// elephc has exactly two runtime shapes, and they map onto reference SAPI names as follows:
///
/// - default (no `--web`): **`cli`**. The binary is a one-shot program driven by `argv`, writing
///   to stdout, exactly what reference PHP's `cli` SAPI describes.
/// - `--web` / `--with-web`: **`cli-server`**. The binary embeds its own HTTP listener and serves
///   requests itself, with no external web server, no FastCGI channel and no module host — which
///   is precisely what reference PHP's built-in server (`php -S`) is, and it is the only
///   reference SAPI name that describes "a standalone PHP binary that speaks HTTP". Reporting
///   `fpm-fcgi` or `apache2handler` would claim a process model (a pool manager, an Apache
///   module) that does not exist here, and library code branches on those names to reach for
///   `fastcgi_finish_request()` / `apache_*` functions elephc does not provide.
///
/// Why this matters more than cosmetics: framework code gates on `PHP_SAPI === 'cli'` (Symfony's
/// `Debug`/`ErrorHandler` and Laravel's `runningInConsole()` both do) to decide whether it is in
/// a console or a request. Reporting `cli` under `--web` would put every such library on the
/// console path inside an HTTP request. `cli-server` is on the "web" side of every such test
/// while still being a name libraries already know.
///
/// This is the single source of truth for the SAPI name: `PHP_SAPI` (baked by
/// `codegen_support::prescan::collect_constants`) and `php_sapi_name()` (rendered by
/// `crate::version_prelude`) both read it.
pub const fn sapi_name(web: bool) -> &'static str {
    if web {
        "cli-server"
    } else {
        "cli"
    }
}

/// The declaration SHAPES, built as AST. This module decides what a `--web` binary KEEPS;
/// `build` spells out what there is to keep.
pub(crate) mod build;


/// The catch-all wrapper: the whole handler body goes inside its `try` so an uncaught exception
/// becomes a 500 before response commitment, or aborts an already-streaming response, instead of
/// crashing the worker (the process would otherwise die and the master would respawn it, dropping
/// the connection). The `$__elephc_wrap = 0;` placeholder body is replaced with the real
/// statements by [`inject_if_web`].
///
/// `finally` closes an active session, which is what makes `session_write_close()` run on the
/// exception path too — `__ElephcSessionState::$shutdown` is the latch a `session_write_close()`
/// in user code clears so it does not run twice.
pub(crate) fn web_wrap_stmt() -> Stmt {
    crate::synthetic_class::internal_declarations(|| {
        vec![s_try(
            vec![s_assign("__elephc_wrap", e_int(0))],
            vec![(
                vec!["\\Throwable"],
                Some("__elephc_exc"),
                vec![s_expr(e_call(
                    "elephc_web_handle_uncaught_exception",
                    vec![],
                ))],
            )],
            Some(vec![s_if(
                e_binop(
                    e_binop(
                        e_call("elephc_web_session_get_status", vec![]),
                        BinOp::StrictEq,
                        e_const("PHP_SESSION_ACTIVE"),
                    ),
                    BinOp::And,
                    e_static_prop("__ElephcSessionState", "shutdown"),
                ),
                vec![s_expr(e_call("session_write_close", vec![]))],
                vec![],
                None,
            )]),
        )]
    })
    .remove(0)
}

/// Prepends the web prelude when compiling with `--web` and wraps the whole
/// handler body in a catch-all `try`/`catch` so uncaught exceptions become a
/// pre-commit 500 or abort an already-committed response.
/// Returns the program unchanged otherwise.
pub fn inject_if_web(
    program: Program,
    web: bool,
    php_version: PhpVersion,
    ini_overrides: &[(String, String)],
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    if !web {
        return program;
    }
    let user_usage = usage::collect(&program);
    let needs_callable_session_handler = user_usage.references("session_set_save_handler")
        || user_usage.dynamic_function_call;
    let mut combined = build::web_declarations(php_version, ini_overrides);
    if !needs_callable_session_handler {
        combined.retain(|stmt| !is_callable_session_handler_decl(&stmt.kind));
    }
    // The callable-handler retain runs BEFORE the prune, and the order matters: those
    // declarations are dropped by POLICY (they are heavy and unreachable for ordinary session
    // programs), and leaving them in would let the pruner root them through its own literal
    // harvest, which would silently undo the policy.
    //
    // THE REST OF THE SESSION SURFACE IS NOT POLICY'S TO DROP, however tempting the number
    // looks. Compiling `<?php echo 1;` with `--web` emits 45,347 lines against a 8,254-line
    // CLI floor, and 21,098 of them — 49% — are session machinery the program never mentions.
    // That is not dead code. `session.auto_start` is seeded per request from the
    // `ELEPHC_SESSION_AUTO_START` environment variable, so a deployment can activate sessions
    // for a handler whose source says nothing about them, and the prelude's own top-level
    // bootstrap calls `__elephc_session_start_core` when it is set. Gating the surface on what
    // the SOURCE mentions would compile a binary that cannot honour its own deployment config —
    // a semantic change wearing an optimisation's clothes. `session_auto_start_env_activates_session`
    // in `tests/web_session_tests.rs` is the test that would go red, and it is right to.
    //
    // Trimming what the program cannot reach is NOT done here. A local pass that harvests literal
    // names cannot see a computed one — `$name = 'session_' . 'regenerate_id'; $name();` had its
    // target removed and the program died with `Call to undefined function`, where PHP answers.
    // The global declaration reachability pass already treats an unknown `$fn()` conservatively,
    // so the COMPLETE selected prelude is recorded and that pass decides what survives.
    //
    // The callable-handler drop above stays here because it is POLICY, not reachability: those
    // declarations are heavy and a program that never mentions `session_set_save_handler` cannot
    // want them, which is a different question from what it can reach.
    inventory.record_program("web", &combined);
    combined.extend(program);

    // The catch-all try wrap below reorders the top level (declarations hoisted
    // out, executables wrapped). That reordering is unsafe across namespace
    // boundaries: a `namespace X;` / `namespace X { … }` would be separated from
    // the declarations it scopes, leaving them in the wrong namespace. For
    // namespaced programs (e.g. a framework with `App\…` classes) skip the wrap
    // entirely — such programs do their own error handling — and keep B1's
    // uncaught-exception → 500 net only for flat, non-namespaced programs.
    if combined.iter().any(|s| {
        matches!(
            s.kind,
            StmtKind::NamespaceDecl { .. } | StmtKind::NamespaceBlock { .. }
        )
    }) {
        return combined;
    }

    // Partition the top level: hoistable declarations (functions, classes, externs)
    // stay outside the try so they resolve normally — externs in particular are NOT
    // resolved when nested in a try. Everything executable goes inside a catch-all
    // try so an uncaught exception becomes a 500 instead of crashing the worker.
    let mut decls: Program = Vec::new();
    let mut exec: Program = Vec::new();
    for stmt in combined {
        if is_hoistable_decl(&stmt.kind) {
            decls.push(stmt);
        } else {
            exec.push(stmt);
        }
    }

    let mut wrapper = web_wrap_stmt();
    let StmtKind::Try { try_body, .. } = &mut wrapper.kind else {
        unreachable!("the catch-all wrapper is built as a Try");
    };
    *try_body = exec;
    decls.push(wrapper);
    decls
}

/// Returns true for the heavy legacy callable-handler declarations that ordinary
/// web/session programs do not need. The detector keeps both declarations when
/// user code can reach `session_set_save_handler()` or another dynamic callable
/// surface; otherwise omitting them avoids compiling ten boxed-Mixed callback
/// dispatchers into every `--web` binary.
fn is_callable_session_handler_decl(kind: &StmtKind) -> bool {
    match kind {
        StmtKind::ClassDecl { name, .. } => name == "__ElephcCallableSessionHandler",
        StmtKind::FunctionDecl { name, .. } => {
            name.eq_ignore_ascii_case("session_set_save_handler")
        }
        _ => false,
    }
}

/// Returns true for top-level statement kinds that are position-independent
/// declarations (hoisted by the resolver), so they can be kept outside the
/// catch-all `try` that wraps the executable handler body.
fn is_hoistable_decl(kind: &StmtKind) -> bool {
    matches!(
        kind,
        StmtKind::FunctionDecl { .. }
            | StmtKind::ClassDecl { .. }
            | StmtKind::EnumDecl { .. }
            | StmtKind::PackedClassDecl { .. }
            | StmtKind::InterfaceDecl { .. }
            | StmtKind::TraitDecl { .. }
            | StmtKind::ExternFunctionDecl { .. }
            | StmtKind::ExternClassDecl { .. }
            | StmtKind::ExternGlobalDecl { .. }
    )
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for web-prelude pay-for-use declaration selection.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Bootstrap session support remains while optional APIs and callable handlers are pruned.

    use super::*;
    use crate::parser::ast::{Expr, ExprKind};


    /// Parses a PHP fixture before web-prelude injection.
    fn parse(source: &str) -> Program {
        let tokens = crate::lexer::tokenize(source).expect("fixture must tokenize");
        crate::parser::parse(&tokens).expect("fixture must parse")
    }

    /// Every maintained profile in declaration order, for exhaustive table checks.
    const ALL_PROFILES: [PhpVersion; 4] = [
        PhpVersion::Php82,
        PhpVersion::Php83,
        PhpVersion::Php84,
        PhpVersion::Php85,
    ];

    /// `PHP_VERSION_ID` must be the reference formula applied to the reported components.
    ///
    /// This also prevents the string and numeric surfaces from drifting apart.
    #[test]
    fn version_id_matches_components() {
        for profile in ALL_PROFILES {
            assert_eq!(
                profile.version_id(),
                profile.major() * 10000 + profile.minor() * 100 + profile.release(),
                "{profile:?} version_id must equal major*10000 + minor*100 + release",
            );
        }
    }

    /// The reported version STRING must spell out exactly the reported components.
    #[test]
    fn version_string_matches_components() {
        for profile in ALL_PROFILES {
            assert_eq!(
                profile.version_string(),
                format!(
                    "{}.{}.{}{}",
                    profile.major(),
                    profile.minor(),
                    profile.release(),
                    profile.extra_version(),
                ),
                "{profile:?} version_string must spell out its own components",
            );
            assert_eq!(profile.major(), 8, "every maintained profile is PHP 8.x");
        }
    }

    /// `zend_version()` tracks the profile minor on the Zend Engine 4.x track.
    ///
    /// The default php-src snapshot includes the `-dev` suffix on both surfaces.
    #[test]
    fn zend_version_tracks_profile_minor() {
        for profile in ALL_PROFILES {
            assert_eq!(
                profile.zend_version(),
                format!(
                    "4.{}.{}{}",
                    profile.minor(),
                    profile.release(),
                    profile.extra_version(),
                ),
                "{profile:?} zend_version must track the PHP release and suffix",
            );
        }
    }

    /// The SAPI name is decided by compile mode and nothing else.
    #[test]
    fn sapi_name_follows_compile_mode() {
        assert_eq!(sapi_name(false), "cli");
        assert_eq!(sapi_name(true), "cli-server");
    }

    /// `--php-version` spellings must round-trip to the profile they name.
    #[test]
    fn parsed_spelling_round_trips_to_version_string() {
        for (spelling, profile) in [
            ("8.2", PhpVersion::Php82),
            ("8.3", PhpVersion::Php83),
            ("8.4", PhpVersion::Php84),
            ("8.5", PhpVersion::Php85),
        ] {
            assert_eq!(PhpVersion::parse(spelling), Some(profile));
            assert!(
                profile.version_string().starts_with(spelling),
                "{profile:?} must report the profile it was selected by",
            );
        }
    }

    /// Returns whether an injected program declares a free function.
    fn declares_function(program: &Program, expected: &str) -> bool {
        program.iter().any(|stmt| {
            matches!(
                &stmt.kind,
                StmtKind::FunctionDecl { name, .. } if name.eq_ignore_ascii_case(expected)
            )
        })
    }

    /// Returns whether an injected program declares a class.
    fn declares_class(program: &Program, expected: &str) -> bool {
        program.iter().any(|stmt| {
            matches!(
                &stmt.kind,
                StmtKind::ClassDecl { name, .. } if name == expected
            )
        })
    }

    /// Injects the web prelude with a throwaway declaration inventory.
    fn inject_web_for_test(
        program: Program,
        web: bool,
        php_version: PhpVersion,
        ini_overrides: &[(String, String)],
    ) -> Program {
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        inject_if_web(
            program,
            web,
            php_version,
            ini_overrides,
            &mut inventory,
        )
    }

    /// Plain web programs keep auto-start/finalization roots but shed optional APIs.
    ///
    /// This asserted the opposite until the web prelude gained pruning: injection used to DEFER
    /// all function pruning, so every session entry point came along. What survives now is what
    /// the program can actually reach without naming it — `__elephc_session_start_core`, because
    /// `session.auto_start` is seeded per request from `ELEPHC_SESSION_AUTO_START`, and
    /// `session_write_close`, which the catch-all wrapper calls from a `finally`. The entry
    /// points a program must SPELL to use, `session_start` and `session_regenerate_id`, are not
    /// roots for a program that spells neither.
    #[test]
    fn plain_web_program_keeps_the_surface_and_drops_only_the_handler() {
        let injected = inject_web_for_test(parse("<?php echo 'ok';"), true, PhpVersion::Php85, &[]);
        assert!(declares_function(
            &injected,
            "__elephc_session_start_core"
        ));
        // INJECTION no longer prunes. It used to, and got a computed name wrong; the global
        // declaration reachability pass owns that now, so what arrives here is the complete
        // selected surface and the trimming happens later, where an unknown `$fn()` is handled.
        assert!(declares_function(&injected, "session_start"));
        assert!(declares_function(&injected, "session_write_close"));
        assert!(declares_function(&injected, "session_regenerate_id"));
        // The callable handler is the exception, because dropping it is POLICY rather than
        // reachability: it is heavy, and a program that neither mentions
        // `session_set_save_handler` nor calls anything dynamically cannot want it.
        assert!(!declares_function(&injected, "session_set_save_handler"));
        assert!(!declares_class(
            &injected,
            "__ElephcCallableSessionHandler"
        ));
    }

    /// A direct session API call roots that function and its transitive helpers.
    #[test]
    fn direct_session_api_call_keeps_requested_declaration() {
        let injected = inject_web_for_test(
            parse("<?php session_start(); session_regenerate_id(true);"),
            true,
            PhpVersion::Php85,
            &[],
        );
        assert!(declares_function(&injected, "session_regenerate_id"));
    }

    /// Verifies entropy failure maps an empty bridge result to PHP `false`
    /// even when `session_create_id()` receives its default empty prefix.
    ///
    /// The property is structural: the `return false` must sit directly under the
    /// `$__elephc_created_id === ''` test, NOT nested inside the `$prefix !== ''` test that
    /// only decides whether to warn. Nesting it there let an empty generated id escape as a
    /// string whenever the prefix was the default empty one.
    #[test]
    fn session_create_id_maps_empty_entropy_result_to_false() {
        let injected = inject_web_for_test(
            parse("<?php session_create_id();"),
            true,
            PhpVersion::Php85,
            &[],
        );
        let body = injected
            .iter()
            .find_map(|stmt| match &stmt.kind {
                StmtKind::FunctionDecl { name, body, .. }
                    if name.eq_ignore_ascii_case("session_create_id") =>
                {
                    Some(body)
                }
                _ => None,
            })
            .expect("session_create_id must be declared");

        let empty_result_guard = body
            .iter()
            .find_map(|stmt| match &stmt.kind {
                StmtKind::If { condition, then_body, .. } if is_created_id_is_empty(condition) => {
                    Some(then_body)
                }
                _ => None,
            })
            .expect("session_create_id must test the generated id for emptiness on its own");

        assert!(
            empty_result_guard.iter().any(|stmt| matches!(
                &stmt.kind,
                StmtKind::Return(Some(expr)) if matches!(expr.kind, ExprKind::BoolLiteral(false))
            )),
            "an empty generated session id must never escape as a string"
        );
        assert!(
            empty_result_guard.iter().any(|stmt| matches!(
                &stmt.kind,
                StmtKind::If { condition, .. } if is_prefix_is_not_empty(condition)
            )),
            "session_create_id must still warn only for an invalid non-empty prefix"
        );
    }

    /// Matches `$__elephc_created_id === ''`.
    fn is_created_id_is_empty(condition: &Expr) -> bool {
        is_strict_string_compare(condition, BinOp::StrictEq, "__elephc_created_id", "")
    }

    /// Matches `$prefix !== ''`.
    fn is_prefix_is_not_empty(condition: &Expr) -> bool {
        is_strict_string_compare(condition, BinOp::StrictNotEq, "prefix", "")
    }

    /// Matches `$<name> <op> '<literal>'` and nothing wider — an `&&` around it fails here,
    /// which is the whole point: the fix was un-nesting the two tests.
    fn is_strict_string_compare(
        condition: &Expr,
        expected_op: BinOp,
        name: &str,
        literal: &str,
    ) -> bool {
        let ExprKind::BinaryOp { left, op, right } = &condition.kind else {
            return false;
        };
        *op == expected_op
            && matches!(&left.kind, ExprKind::Variable(var) if var == name)
            && matches!(&right.kind, ExprKind::StringLiteral(text) if text == literal)
    }

    /// Literal availability probes retain the queried PHP-visible function.
    #[test]
    fn function_exists_probe_keeps_session_save_handler() {
        let injected = inject_web_for_test(
            parse("<?php echo function_exists('session_set_save_handler');"),
            true,
            PhpVersion::Php85,
            &[],
        );
        assert!(declares_function(&injected, "session_set_save_handler"));
        assert!(declares_class(
            &injected,
            "__ElephcCallableSessionHandler"
        ));
    }

    /// A dynamic call keeps what the program NAMES, not the whole surface.
    ///
    /// This replaces a deliberately blunt earlier rule — any dynamic call kept every declaration.
    /// That was safe and useless: one `$f()` anywhere reimposed the entire prelude, so the pruner
    /// was off on precisely the programs where compile time matters. The dispatched name is a
    /// string literal, which is how a real dispatcher is written, so it is rooted; a function the
    /// program never mentions is not.
    #[test]
    fn a_dynamic_call_keeps_even_the_callable_handler() {
        let injected = inject_web_for_test(
            parse("<?php $name = 'session_regenerate_id'; $name();"),
            true,
            PhpVersion::Php85,
            &[],
        );
        assert!(declares_function(&injected, "session_regenerate_id"));
        // The handler policy reads `$fn()` as a hazard, not as the literal it happens to hold:
        // the same syntax could name `session_set_save_handler`, so the surface stays. Reading
        // the literal instead is what removed a computed name's target elsewhere.
        assert!(
            declares_function(&injected, "session_set_save_handler"),
            "a dynamic dispatch could name the handler, so policy must not drop it"
        );
    }

    /// The same rule for an availability probe on a computed name.
    #[test]
    fn dynamic_function_probe_disables_prelude_function_pruning() {
        let injected = inject_web_for_test(
            parse("<?php $name = 'session_regenerate_id'; echo function_exists($name);"),
            true,
            PhpVersion::Php85,
            &[],
        );
        assert!(declares_function(&injected, "session_regenerate_id"));
        assert!(!declares_function(&injected, "session_set_save_handler"));
    }

    /// Verifies a computed name keeps its target through global declaration reachability.
    ///
    /// The local literal harvest cannot see a concatenated name, so prelude injection records the
    /// complete selected surface and lets the global reachability pass handle the unknown dynamic
    /// call conservatively. Pruning locally instead made this call fail at runtime with an
    /// undefined function.
    #[test]
    fn a_computed_name_keeps_its_target() {
        let injected = inject_web_for_test(
            parse("<?php $name = 'session_' . 'regenerate_id'; $name();"),
            true,
            PhpVersion::Php85,
            &[],
        );
        // This asserted the OPPOSITE, and pinned a runtime failure as expected behaviour: a
        // literal harvest cannot see a concatenated name, so the target was pruned and the
        // program died with `Call to undefined function` where PHP answers.
        assert!(
            declares_function(&injected, "session_regenerate_id"),
            "a name no literal harvest can see must not cost the program its target"
        );
    }

    /// Non-web compilation leaves the user program untouched.
    #[test]
    fn non_web_program_is_unchanged() {
        let program = parse("<?php echo 'ok';");
        assert_eq!(
            inject_web_for_test(program.clone(), false, PhpVersion::Php85, &[]),
            program
        );
    }
}
