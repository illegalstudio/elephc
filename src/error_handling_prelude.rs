//! Purpose:
//! The part of PHP's error-handling surface that is built as AST and shared by EVERY SAPI: the
//! `error_get_last()` / `error_clear_last()` pair over the request's last-diagnostic record,
//! `register_shutdown_function()` and its drain, and `error_log()`. `error_reporting()`, the
//! error/exception handler stacks and `trigger_error()` are native builtins with runtime state.
//!
//! - THE LAST-ERROR RECORD IS NATIVE (`codegen_support::runtime::last_error`), written by the
//!   two native producers -- the warning dispatcher and `trigger_error()` -- and read here
//!   through the internal `__elephc_last_error_int` / `__elephc_last_error_str` builtins. It is
//!   per-request state: `web_prelude`'s `bootstrap 43c` clears it at the head of every request
//!   beside the `43a` / `43b` resets.
//!
//! Called from:
//! - `crate::web_prelude::build::web_declarations`, which splices [`declarations`] into the
//!   `--web` surface where these declarations used to sit inline.
//! - `crate::pipeline::compile` through [`inject_if_used`], which gives a NON-`--web` build
//!   the same surface when the program mentions it.
//!
//! Key details:
//! - ONE implementation, two injection sites. These functions were `--web`-only until a plain
//!   CLI build turned out to have none of them, and PHP's error rule is not something two
//!   copies may each hold: the bodies live here and both preludes hand out the same `Stmt`s.
//! - Built as AST (`function(...).param(...).body(vec![...])`), never parsed from PHP source,
//!   which is the standing rule for every compiler-provided prelude outside tests.
//! - Pay-for-use off `--web`: [`inject_if_used`] injects nothing unless the program names one
//!   of [`SURFACE`], so a program that never mentions them does not grow. Under `--web` the
//!   whole prelude is injected unconditionally and pruned later, which is why
//!   [`inject_if_used`] returns early for a `--web` compile — the declarations are already
//!   there and a second copy would be a redeclaration.
//! - `register_shutdown_function()` IS here now, and the constraint that used to keep it out is
//!   worth stating because it is what the shape below answers. It was: "its registry only means
//!   something if something drains it, and the only drain that exists is the `--web` request
//!   wrapper's `finally`; a CLI copy would register callbacks that never run." That was right —
//!   and a `finally` is not a drain a CLI build may rely on, because `exit()` does not run
//!   `finally` blocks. Measured on php 8.5.10 AND on elephc, on the same source
//!   (`try { exit(3); } finally { echo "FINALLY"; }` prints nothing but the body in both, rc=3):
//!   a wrapper copied from `--web` would cover the normal end and miss the exit, which is the
//!   path a console application takes.
//!
//!   So the drain is not a `finally` at all. [`decl_fn_elephc_shutdown_run`] is a nullary `void`
//!   entry, and CODEGEN calls it by symbol at the process-exit sites it owns:
//!   `codegen::lower_inst::builtins::system::lower_exit` (every `exit()` / `die()`, `--web`
//!   included) and `codegen::frame::emit_main_epilogue` (the normal end of the top level). That
//!   covers php's normal-end and `exit()` paths in one mechanism with no control-flow rewrite.
//!
//!   WHAT IT STILL DOES NOT COVER: an uncaught exception. php runs shutdown functions after
//!   printing the fatal report; elephc's report is `__rt_report_uncaught_exception`, a SHARED
//!   RUNTIME symbol in a separate object that cannot name this program's functions, entered by
//!   a tail jump with no frame. Reaching the drain from there needs the `RuntimeFeatures`
//!   gating that `diag_user_handler` already demonstrates, and that is where the fix belongs.

use std::path::Path;

use crate::parser::ast::{BinOp, CastType, Program, Stmt, StmtKind, TypeExpr};
use crate::prelude_prune::usage;
use crate::synthetic_class::{
    e_array, e_array_assoc, e_binop, e_bool, e_call, e_cast, e_const, e_index, e_int, e_null,
    e_str, e_var, function, php_semantics_declarations, s_array_push, s_assign, s_expr, s_if,
    s_return, s_static, s_while, t_array, t_class, t_mixed, t_nullable,
};

/// Prelude inventory group holding the surface itself, on the non-`--web` path.
///
/// A separate id from `"web"`, because the two injection sites are mutually exclusive and a
/// shared id would make "which prelude contributed this" unanswerable.
pub(crate) const GROUP: &str = "error_handling";

/// The PHP-visible names this prelude declares — the pay-for-use gate off `--web`.
///
/// The internal `__elephc_*` helpers are deliberately absent: nothing but these functions and
/// the generated runtime reaches them, so a program that names none of these cannot want them.
pub(crate) const SURFACE: &[&str] = &[
    "error_get_last",
    "error_clear_last",
    // `error_log` is the one name here that is NOT undefined off `--web`: the registry builtin
    // answers it. It is gated all the same, because the prelude declaration SHADOWS that builtin
    // and must therefore be pay-for-use — a program that never spells `error_log` keeps the
    // cheaper direct `__rt_error_log` call it has today.
    "error_log",
    "register_shutdown_function",
];

/// Prelude inventory group holding the nullary shutdown drain, on the non-`--web` path.
///
/// `__elephc_shutdown_run` has NO PHP caller off `--web`: `lower_exit` and `emit_main_epilogue`
/// reach it by symbol, and declaration reachability cannot see a call that lives in assembly.
/// Recorded as its own group and forced by `pipeline::compile` — but only when the program
/// SPELLS `register_shutdown_function`, so a program that merely names `error_reporting()`
/// still gets the whole registry pruned and pays for no drain call at its exits.
pub const SHUTDOWN_RUN_GROUP: &str = "shutdown_run";

/// `error_get_last` — the four-key record in php's order, or `null` when nothing is recorded.
///
/// php's signature is `error_get_last(): ?array`, and the nullable array return is what
/// Symfony's `ErrorHandler::handleFatalError` relies on: it does
/// `if ($error && $error['type'] &= E_PARSE|E_ERROR|E_CORE_ERROR|E_COMPILE_ERROR)`, so both
/// `null` and an ordinary `E_WARNING` record answer false there and only a genuine fatal
/// promotes. `DefaultMarshaller` reads `error_get_last()['message']`, so the record is real.
///
/// WHAT php RECORDS, measured on php 8.5.10 -- each of these is a test in
/// `tests/error_handling_surface_tests.rs`:
///
/// ```text
/// php -r 'var_dump(error_get_last());'                        → NULL
/// php -r '@$x = $u; var_dump(error_get_last());'               → array, type 2
/// php -r 'error_reporting(0); $x = $u; var_dump(...);'         → array, type 2
/// php -r 'set_error_handler(fn(...) => true); $x = $u; ...'    → NULL
/// php -r 'set_error_handler(fn(...) => false); $x = $u; ...'   → array, type 2
/// php -r 'trigger_error("boom"); ...'                          → array, type 1024
/// php -r 'trigger_error("bang", E_USER_WARNING); ...'          → array, type 512
/// php -r 'trigger_error("x"); error_clear_last(); ...'         → NULL
/// ```
///
/// SUPPRESSION DOES NOT STOP THE RECORD -- `@` and `error_reporting(0)` silence the display
/// only -- and A HANDLER THAT TAKES THE DIAGNOSTIC LEAVES THE PREVIOUS RECORD STANDING. That is
/// php-src's structure: the record is written by `php_error_cb`, the internal handler a user
/// handler returning anything but `false` pre-empts. The native producers write it on exactly
/// that path; this function only reads it. The level is never `0` for a real diagnostic, so
/// `0` is the "nothing recorded" answer and no separate presence flag is needed.
fn decl_fn_error_get_last() -> Stmt {
    function("error_get_last")
        .returns(t_nullable(t_array()))
        .body(vec![
            s_assign("type", e_call("__elephc_last_error_int", vec![e_int(0)])),
            // AN EARLY `if`, NEVER A TERNARY: a `Mixed`-typed ternary has miscompiled before.
            s_if(
                e_binop(e_var("type"), BinOp::StrictEq, e_int(0)),
                vec![s_return(e_null())],
                vec![],
                None,
            ),
            s_return(e_array_assoc(vec![
                (e_str("type"), e_var("type")),
                (e_str("message"), e_call("__elephc_last_error_str", vec![e_int(0)])),
                (e_str("file"), e_call("__elephc_last_error_str", vec![e_int(1)])),
                (e_str("line"), e_call("__elephc_last_error_int", vec![e_int(1)])),
            ])),
        ])
        .build()
}

/// `error_clear_last` — transcribed from the PHP form.
///
/// php's `error_clear_last(): void`. Nothing in the `--web` corpus calls it — the Symfony
/// vendor tree has `error_get_last` in five places and `error_clear_last` in none — but it is
/// half of a two-function surface in php and a program that clears cannot be made to work by
/// adding only the reader. It costs one declaration behind the same pay-for-use gate.
fn decl_fn_error_clear_last() -> Stmt {
    function("error_clear_last")
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_call("__elephc_last_error_int", vec![e_int(2)])),
        ])
        .build()
}

/// `error_log` — MOVED VERBATIM out of `web_prelude::build`, with one node changed (below).
///
/// PHP declares `error_log()` in every SAPI and elephc did too — but off `--web` the answer was
/// the REGISTRY BUILTIN (`src/builtins/system/error_log.rs` + `__rt_error_log`), which writes the
/// message to stderr and ignores `$message_type` and `$destination` entirely. So
/// `error_log($m, 3, $file)` on a CLI binary logged to the console and returned `true`, and
/// nothing said so. This declaration is `SourceMode::Internal`, which is what lets it shadow the
/// registry builtin (see `types::checker::driver::functions::collect_function_decls`) — the same
/// shadowing `--web` already relied on, now available to both SAPIs.
///
/// THE ONE CHANGED NODE is the `message_type === 0` newline. The `--web` form appended `"\n"`
/// only when the message did not already end in one, which is NOT what php does: php appends
/// unconditionally (`php -r 'error_log("b\n");'` writes `b\n\n` — verified on 8.5.10, and
/// `__rt_error_log` matches it). Transcribing that branch as it stood would have made the moved
/// declaration a REGRESSION on CLI, losing a newline the registry builtin gets right, so the
/// condition is gone and the concat is unconditional. This also fixes the same divergence under
/// `--web`.
///
/// The `message_type === 1` (mail) message lost its "under --web" wording for the same reason:
/// the branch is now reachable from a CLI binary, where that sentence would be a lie. Mail
/// delivery is still unimplemented in both SAPIs.
fn decl_fn_error_log() -> Stmt {
    function("error_log")
        .param("message", TypeExpr::Str)
        .param_default("message_type", TypeExpr::Int, e_int(0))
        .param_default("destination", t_nullable(TypeExpr::Str), e_null())
        .param_default("additional_headers", t_nullable(TypeExpr::Str), e_null())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_var("message_type"), BinOp::StrictEq, e_int(3)),
                vec![
                    s_if(
                        e_binop(e_var("destination"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("__elephc_el_fh", e_call("fopen", vec![e_cast(CastType::String, e_var("destination")), e_str("a")])),
                    s_if(
                        e_binop(e_var("__elephc_el_fh"), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_call("fwrite", vec![e_var("__elephc_el_fh"), e_var("message")])),
                    s_expr(e_call("fclose", vec![e_var("__elephc_el_fh")])),
                    s_return(e_bool(true)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("message_type"), BinOp::StrictEq, e_int(0)),
                vec![
                    s_expr(e_call("fwrite", vec![e_const("STDERR"), e_binop(e_var("message"), BinOp::Concat, e_str("\n"))])),
                    s_return(e_bool(true)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("message_type"), BinOp::StrictEq, e_int(1)),
                vec![
                    s_expr(e_call("fwrite", vec![e_const("STDERR"), e_binop(e_binop(e_binop(e_binop(e_binop(e_str("error_log(): mail delivery (type 1) is not supported by elephc"), BinOp::Concat, e_str(" [to=")), BinOp::Concat, e_cast(CastType::String, e_var("destination"))), BinOp::Concat, e_str(", headers=")), BinOp::Concat, e_cast(CastType::String, e_var("additional_headers"))), BinOp::Concat, e_str("]\n"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_bool(false)),
        ])
        .build()
}

/// `__elephc_shutdown_function_state` — transcribed from the PHP form.
///
/// The shutdown callback registry, dispatched on `$operation`: 1 registers, 2 drains
/// ([`decl_fn_elephc_shutdown_run`] is the only caller of mode 2 now), 3 clears. The reset is
/// not optional housekeeping — the registry is a STATIC function local, so without
/// `bootstrap 43a` a reused `--web` worker would run the previous request's callbacks again.
///
/// The drain is an ordinary `array_shift()`-until-empty loop. It was written with a CURSOR for a
/// while, because `array_shift()` on a `static` array returned `NULL` for anything an earlier
/// call had pushed — and the note left here described that as specific to a static array whose
/// ELEMENTS ARE ARRAYS, with flat scalars reported as correct. THAT WAS WRONG: the element type
/// never mattered. `array_shift()` on a `static` array of plain strings returned `NULL` just the
/// same, and once the array outgrew its first allocation the static lost its contents entirely,
/// appends included. Both causes are fixed (an empty-array `static` is typed `array<mixed>`
/// rather than `array<never>`, and `ReceiverPlace` now knows `Op::LoadStaticLocal`), and
/// `tests/static_local_array_tests.rs` holds the repro with its controls.
///
/// This loop keeps php's semantics exactly. Registration order is the array's own order.
/// `register_shutdown_function()` called from INSIDE a shutdown callback appends to the same
/// registry, and the loop re-reads `count()` each turn, so it reaches it — which is what php
/// does: `php -r 'register_shutdown_function(function(){ echo "outer\n";
/// register_shutdown_function(function(){ echo "inner\n"; }); });'` prints both. And a second
/// drain is a no-op because the registry is empty, which is what the exit path relies on.
///
/// `$draining` is the RE-ENTRANCY LATCH, and it is the one thing the `--web` form did not need.
/// A shutdown callback may call `exit()`, and php then terminates at once WITHOUT running the
/// callbacks still queued behind it — measured:
///
/// ```text
/// php -r 'register_shutdown_function(function(){ echo "one\n"; exit(7); });
///         register_shutdown_function(function(){ echo "two\n"; }); echo "body\n"; exit(3);'
/// body
/// one
/// rc=7
/// ```
///
/// `two` never prints and the callback's status wins. That `exit()` re-enters this helper
/// through `lower_exit`'s drain call — in BOTH SAPIs, since that arm is shared — and without the
/// latch the loop would resume and print `two`. The latch is cleared by operation 3 as well as
/// by a completed drain, because a reused `--web` worker resets the registry between requests
/// and must reset this with it.
fn decl_fn_elephc_shutdown_function_state() -> Stmt {
    function("__elephc_shutdown_function_state")
        .param_default("callback", t_mixed(), e_null())
        .param_default("args", t_mixed(), e_array(vec![]))
        .param_default("operation", TypeExpr::Int, e_int(0))
        .returns(t_mixed())
        .body(vec![
            s_static("callbacks", e_array(vec![])),
            s_static("draining", e_bool(false)),
            s_if(
                e_binop(e_var("operation"), BinOp::StrictEq, e_int(1)),
                vec![
                    s_array_push("callbacks", e_array(vec![e_var("callback"), e_var("args")])),
                    s_return(e_null()),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("operation"), BinOp::StrictEq, e_int(2)),
                vec![
                    s_if(
                        e_var("draining"),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("draining", e_bool(true)),
                    s_while(e_binop(e_call("count", vec![e_var("callbacks")]), BinOp::Gt, e_int(0)), vec![
                        s_assign("__elephc_shutdown_entry", e_cast(CastType::Array, e_call("array_shift", vec![e_var("callbacks")]))),
                        s_expr(e_call("call_user_func_array", vec![e_index(e_var("__elephc_shutdown_entry"), e_int(0)), e_index(e_var("__elephc_shutdown_entry"), e_int(1))])),
                    ]),
                    s_assign("draining", e_bool(false)),
                    s_return(e_null()),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("operation"), BinOp::StrictEq, e_int(3)),
                vec![
                    s_assign("callbacks", e_array(vec![])),
                    s_assign("draining", e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_return(e_null()),
        ])
        .build()
}

/// `__elephc_shutdown_run` — the zero-argument drain entry, and the ONLY one codegen calls.
///
/// It exists purely so the drain has a symbol that can be reached from emitted assembly with no
/// argument marshalling. `__elephc_shutdown_function_state(null, [], 2)` would mean building a
/// Mixed null and an empty PHP array at every process-exit site; a nullary `void` function is a
/// bare `bl` / `call`, which is what `lower_exit` and `emit_main_epilogue` emit.
///
/// It has NO PHP caller off `--web`, so declaration reachability cannot see it and would delete
/// it — which is the reason [`SHUTDOWN_RUN_GROUP`] is
/// forced by `pipeline::compile` whenever the program asks for this surface. Under `--web` the
/// request wrapper's `finally` calls it, so it is reachable there by ordinary means.
fn decl_fn_elephc_shutdown_run() -> Stmt {
    function(crate::names::SHUTDOWN_RUN_FUNCTION)
        .returns(TypeExpr::Void)
        .body(vec![s_expr(e_call(
            "__elephc_shutdown_function_state",
            vec![e_null(), e_array(vec![]), e_int(2)],
        ))])
        .build()
}

/// `register_shutdown_function` — transcribed from the PHP form.
fn decl_fn_register_shutdown_function() -> Stmt {
    function("register_shutdown_function")
        .param("callback", t_class("callable"))
        .variadic("args", Some(t_mixed()))
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_call("__elephc_shutdown_function_state", vec![e_var("callback"), e_var("args"), e_int(1)])),
        ])
        .build()
}

/// The whole error/exception surface, in the order the `--web` prelude's PHP source had.
///
/// Declarations are hoisted, so the order is documentation rather than semantics — but it is
/// the order `web_declarations` shipped, and keeping it makes the two surfaces diffable.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| {
        vec![
            decl_fn_error_get_last(),
            decl_fn_error_clear_last(),
            decl_fn_error_log(),
            decl_fn_elephc_shutdown_function_state(),
            decl_fn_elephc_shutdown_run(),
            decl_fn_register_shutdown_function(),
        ]
    })
}

/// The function names [`declarations`] contributes, normalized.
///
/// Derived from the declarations themselves rather than listed a second time, so a helper
/// added above cannot fall out of the shadowing filter by being forgotten here.
fn declared_function_names() -> std::collections::HashSet<String> {
    declarations()
        .iter()
        .filter_map(|stmt| match &stmt.kind {
            StmtKind::FunctionDecl { name, .. } => Some(crate::names::php_symbol_key(name)),
            _ => None,
        })
        .collect()
}

/// Removes from the PROGRAM every function declaration whose name this prelude owns, so the
/// built-in wins — which is what php does and what elephc already does for a registry builtin.
///
/// THE SHAPE THIS EXISTS FOR is the polyfill guard, which is ordinary library code:
///
/// ```php
/// if (!function_exists('trigger_error')) { function trigger_error(…) { … } }
/// ```
///
/// php never takes that branch, because `trigger_error` is built in. elephc emits the guarded
/// body as a symbol anyway — the registration is conditional, the EMISSION is not — so a
/// prelude that declares the same name collides at assembly time:
/// `error: symbol '_fn_trigger_u_error' is already defined`, with no source position. The same
/// source failed a `--web` LINK the same way before this filter existed, so the defect is the
/// prelude's, not the CLI injection's; fixing it here fixes both.
///
/// DROPPING THE PRELUDE'S COPY INSTEAD DOES NOT WORK, and the reason is worth recording. Left
/// to the program's own declaration, the guard evaluates `function_exists('trigger_error')`,
/// which is TRUE (the name is in the closed world), so the branch never runs, so the
/// declaration is never registered, and the call one line later dies with `Call to undefined
/// function trigger_error()`. Measured. The built-in has to win, and for it to win the
/// shadowing declaration has to go.
///
/// A program that declares one of these names UNCONDITIONALLY is rejected outright by php
/// (`Cannot redeclare trigger_error()`), so nothing correct is lost by treating it the same
/// way. Function BODIES are not walked: a declaration nested inside one is a different
/// declaration and is left alone.
pub(crate) fn without_shadowing_declarations(program: Program) -> Program {
    without_shadowing_names(program, &declared_function_names())
}

/// The same filter against an EXPLICIT owned set, for a caller that injects only part of
/// [`declarations`].
///
/// The two have to agree or the program loses a declaration nothing replaces: off `--web` the
/// shutdown trio is dropped for a program that never spells `register_shutdown_function`, and a
/// filter keyed on the FULL set would still strip that program's own `function
/// register_shutdown_function()` — leaving an undefined function where there had been a working
/// one. Passing the set that was actually injected is what keeps the two sides one decision.
fn without_shadowing_names(
    program: Program,
    owned: &std::collections::HashSet<String>,
) -> Program {
    fn filter(body: Vec<Stmt>, owned: &std::collections::HashSet<String>) -> Vec<Stmt> {
        body.into_iter()
            .filter(|stmt| match &stmt.kind {
                StmtKind::FunctionDecl { name, .. } => {
                    !owned.contains(&crate::names::php_symbol_key(name))
                }
                _ => true,
            })
            .map(|mut stmt| {
                stmt.kind = match stmt.kind {
                    StmtKind::NamespaceBlock { name, body } => StmtKind::NamespaceBlock {
                        name,
                        body: filter(body, owned),
                    },
                    StmtKind::Synthetic(body) => StmtKind::Synthetic(filter(body, owned)),
                    StmtKind::IncludeOnceGuard { source_path, required, body } => {
                        StmtKind::IncludeOnceGuard {
                            source_path,
                            required,
                            body: filter(body, owned),
                        }
                    }
                    StmtKind::If {
                        condition,
                        then_body,
                        elseif_clauses,
                        else_body,
                    } => StmtKind::If {
                        condition,
                        then_body: filter(then_body, owned),
                        elseif_clauses: elseif_clauses
                            .into_iter()
                            .map(|(guard, arm)| (guard, filter(arm, owned)))
                            .collect(),
                        else_body: else_body.map(|arm| filter(arm, owned)),
                    },
                    other => other,
                };
                stmt
            })
            .collect()
    }
    filter(program, owned)
}

/// Returns whether the program names any function this prelude declares.
///
/// A literal counts as well as a call: `function_exists('set_error_handler')` and
/// `array_map('trigger_error', …)` already land in `usage.functions`, and the `literals` pool
/// catches the remaining string forms. What this cannot see is a name the program COMPUTES
/// (`$f = 'trigger' . '_error'; $f();`), which stays an undefined function — the same
/// limitation `superglobals::seed_cli_populated_superglobals` has, and for the same reason: a
/// pay-for-use gate reads what the source SPELLS.
fn program_names_the_surface(program: &[Stmt]) -> bool {
    let used = usage::collect(program);
    // A `define()` with a computed name goes through `crate::dynamic_define_prelude`, whose
    // "already defined" warning is raised with `trigger_error()`.
    SURFACE.iter().any(|name| program_names(&used, name))
        || program
            .iter()
            .any(|stmt| crate::ast_usage::collect_stmt(stmt).dynamic_define)
}

/// Returns true for the three declarations that make up the shutdown registry.
///
/// Derived by NAME rather than by position so that reordering `declarations()` cannot silently
/// change what the gate drops.
fn is_shutdown_declaration(kind: &StmtKind) -> bool {
    let StmtKind::FunctionDecl { name, .. } = kind else {
        return false;
    };
    matches!(
        crate::names::php_symbol_key(name).as_str(),
        "register_shutdown_function"
            | "__elephc_shutdown_function_state"
            | "__elephc_shutdown_run"
    )
}

/// Returns whether the program names `register_shutdown_function` specifically.
///
/// Separate from [`program_names_the_surface`] because the two questions have different
/// answers and different costs: the surface gate decides whether PHP's error rule is compiled
/// in at all, while THIS one decides whether every `exit()` in the binary grows a drain call.
/// A program that only calls `error_reporting()` must answer false here.
fn program_names_shutdown_registration(program: &[Stmt]) -> bool {
    program_names(&usage::collect(program), "register_shutdown_function")
}

/// Returns whether one name is spelled by the program, as a call or as a string literal.
fn program_names(used: &usage::Usage, name: &str) -> bool {
    used.references(name) || used.literals.contains(&crate::names::php_symbol_key(name))
}

/// Gives a NON-`--web` build the error-handling surface PHP's every SAPI has, when the program
/// mentions it.
///
/// Returns the program untouched under `--web`: `web_prelude::inject_if_web` has already
/// injected the same declarations (through [`declarations`]), and a second copy would be a
/// redeclaration.
///
/// `entry_path` resolves the `__FILE__` in `trigger_error()`'s handler call. The magic-constant
/// pass has already run per file by the time a prelude is injected, so nothing later would
/// resolve it — exactly the reason `web_prelude::inject_if_web` substitutes there too.
pub fn inject_if_used(
    program: Program,
    web: bool,
    entry_path: &Path,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    if web || !program_names_the_surface(&program) {
        return program;
    }
    // PAY-FOR-USE WITHIN THE SURFACE. The shutdown trio rides in the same `declarations()` list
    // as the error functions, and leaving it there for every caller is not free: reachability
    // keeps the WHOLE injected prelude once any of it is in (the dispatch helper's
    // `call_user_func` is a dynamic call, which the pass treats conservatively), so a program
    // whose only mention is `error_reporting()` would compile the registry AND grow a drain call
    // at every one of its `exit()` sites. Dropping the declarations here is the only gate fine
    // enough to stop that, and it is the same POLICY drop `web_prelude::inject_if_web` performs
    // for the callable session handler — before `record_program`, so the pruner's literal
    // harvest cannot re-root what policy removed.
    let mut declarations = declarations();
    if !program_names_shutdown_registration(&program) {
        declarations.retain(|stmt| !is_shutdown_declaration(&stmt.kind));
    }
    // The shadowing filter is keyed on what was ACTUALLY injected, not on the full surface: a
    // program that keeps its own `register_shutdown_function()` polyfill must keep it when the
    // prelude declined to declare one.
    let owned: std::collections::HashSet<String> = declarations
        .iter()
        .filter_map(|stmt| match &stmt.kind {
            StmtKind::FunctionDecl { name, .. } => Some(crate::names::php_symbol_key(name)),
            _ => None,
        })
        .collect();
    let program = without_shadowing_names(program, &owned);
    let mut combined =
        crate::magic_constants::substitute_file_constants(declarations, entry_path);
    inventory.record_program(GROUP, &combined);
    record_internal_callable_drain(inventory, GROUP);
    // See `SHUTDOWN_RUN_GROUP`: the drain entry is reached from EMITTED CODE, not from PHP, so
    // reachability would delete it and `lower_exit` would call a symbol nobody defined. Forced
    // only for a program that spells `register_shutdown_function` — one that merely names
    // `error_reporting()` keeps the registry pruned and pays nothing at its exit sites.
    if program_names_shutdown_registration(&program) {
        inventory
            .group_mut(SHUTDOWN_RUN_GROUP)
            .functions
            .insert(crate::names::php_symbol_key(
                crate::names::SHUTDOWN_RUN_FUNCTION,
            ));
    }
    combined.extend(program);
    combined
}

/// Marks the shutdown registry's drain as an internal callable function of `group`.
///
/// Its `call_user_func_array()` only runs what `register_shutdown_function()` registered, and
/// that function's `callable` parameter already makes reachability scan every callback at the
/// registration site. Counted as a dynamic call, the drain -- reached from every `--web` request
/// wrapper -- kept every declared function and method of the program and its preludes alive.
pub(crate) fn record_internal_callable_drain(
    inventory: &mut crate::optimize::reachability::PreludeInventory,
    group: &str,
) {
    let drain = crate::names::php_symbol_key("__elephc_shutdown_function_state");
    if inventory
        .groups
        .get(group)
        .is_some_and(|recorded| recorded.functions.contains(&drain))
    {
        inventory.record_internal_callable_function(group, &drain);
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the shared error-handling prelude: that the gate and the declared set
    //! agree, that a program naming nothing is untouched, that a `--web` compile is left to
    //! the web prelude, and that one mention forces the runtime's dispatch pair.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Tests parse raw source, which is the stage `inject_if_used` runs at.

    use super::*;
    use crate::parser::ast::StmtKind;

    /// Parses PHP the way `inject_if_used` sees it: tokenize then parse.
    fn parse(source: &str) -> Program {
        let tokens = crate::lexer::tokenize(source).expect("test source must tokenize");
        crate::parser::parse(&tokens).expect("test source must parse")
    }

    /// The names the prelude declares, in order.
    fn declared_names() -> Vec<String> {
        declarations()
            .iter()
            .filter_map(|stmt| match &stmt.kind {
                StmtKind::FunctionDecl { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect()
    }

    /// Every PHP-visible name in [`SURFACE`] must actually be declared, or the gate admits a
    /// program to a surface that does not contain what it asked for.
    #[test]
    fn every_gated_name_is_declared() {
        let declared = declared_names();
        for name in SURFACE {
            assert!(
                declared.iter().any(|candidate| candidate == name),
                "{name} is gated on but not declared"
            );
        }
    }

    /// Pay-for-use: a program that names none of the surface must be handed back unchanged.
    #[test]
    fn a_program_that_names_nothing_is_left_alone() {
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let program = parse("<?php echo 1;");
        let before = program.len();
        let after = inject_if_used(program, false, Path::new("entry.php"), &mut inventory);
        assert_eq!(after.len(), before);
        assert!(inventory.groups.is_empty());
    }

    /// A `--web` compile must be left alone: the web prelude already injected these.
    #[test]
    fn a_web_compile_is_left_alone() {
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let program = parse("<?php set_error_handler('h');");
        let before = program.len();
        let after = inject_if_used(program, true, Path::new("entry.php"), &mut inventory);
        assert_eq!(after.len(), before);
        assert!(inventory.groups.is_empty());
    }

    /// One mention is enough to inject the surface.
    #[test]
    fn one_mention_injects_the_surface() {
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let injected = inject_if_used(
            parse("<?php var_dump(error_get_last());"),
            false,
            Path::new("entry.php"),
            &mut inventory,
        );
        assert!(injected.len() > 1);
        assert!(inventory.groups.contains_key(GROUP));
    }

    /// A `function_exists('error_get_last')` guard is a mention: its subject must exist, or the
    /// guard silently takes its else branch.
    #[test]
    fn a_function_exists_probe_is_a_mention() {
        assert!(program_names_the_surface(&parse(
            "<?php if (function_exists('error_get_last')) { echo 1; }"
        )));
    }

    /// The polyfill guard: a declaration nested inside the `if` must go, because elephc emits
    /// its body as a symbol whether or not the branch runs, and the prelude declares the same
    /// name. The end-to-end proof is `a_function_exists_guarded_polyfill_still_builds` in
    /// `tests/error_handling_surface_tests.rs`; this pins the walk that finds it.
    #[test]
    fn a_guarded_polyfill_declaration_is_dropped() {
        let program = without_shadowing_declarations(parse(
            "<?php if (!function_exists('error_get_last')) { function error_get_last() { return null; } } echo 1;",
        ));
        let StmtKind::If { then_body, .. } = &program[0].kind else {
            panic!("expected the guard to survive, only its declaration to go");
        };
        assert!(then_body.is_empty(), "the shadowing declaration must be dropped");
        assert_eq!(program.len(), 2, "nothing else may be removed");
    }

    /// A declaration this prelude does not own is left where it is, however it is nested.
    #[test]
    fn an_unrelated_declaration_is_left_alone() {
        let program = without_shadowing_declarations(parse(
            "<?php if (!function_exists('str_contains')) { function str_contains($h, $n) { return false; } }",
        ));
        let StmtKind::If { then_body, .. } = &program[0].kind else {
            panic!("expected an if");
        };
        assert_eq!(then_body.len(), 1, "an unrelated polyfill must survive");
    }
}
