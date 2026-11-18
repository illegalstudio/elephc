//! Purpose:
//! Transaction boundaries outside HTTP for a build that carries transaction
//! monitoring (`--with-apm`, `--with-monitoring`): the methods whose every
//! call is a transaction of its own get their body wrapped so the probe
//! counts it, without a line of the application or its vendors changed.
//!
//! Called from:
//! - `crate::pipeline::compile()`, after `autoload::run`: the methods it wraps
//!   are mostly in vendor classes, which are part of the program only from
//!   there on. The declarations it adds are name-resolved on the way in.
//! - `crate::cli` (`check_transaction_target`), for `--apm-transaction`.
//!
//! Key details:
//! - Symfony Messenger: `Worker::handleMessage` is a message's whole life in
//!   a consumer — its handlers, its middleware, its acknowledgement — so each
//!   call is one transaction, named after the message's class
//!   (`MSG App\Message\SendEmail`). The worker catches a failing handler's
//!   exception itself and reports it by building a `WorkerMessageFailedEvent`
//!   during that call, so the event's constructor is what marks the message
//!   failed.
//! - `--apm-transaction Class::method` (or `function`) names any other
//!   boundary: each call is a transaction named `TXN Class::method`, failed
//!   when an exception leaves it.
//! - The wrap is `begin; try { body } catch (\Throwable) { fail; rethrow }
//!   finally { end }`: the probe counts nothing unless the service was asked
//!   to count, and a transaction opened inside another counts nothing of its
//!   own (`elephc_probe_txn_begin`), so a message dispatched synchronously
//!   during a web request stays part of the request.

use crate::errors::CompileError;
use crate::parser::ast::{CType, Expr, Program, Stmt, StmtKind};
use crate::synthetic_class::{
    e_call, e_concat, e_int, e_method_call, e_str, e_var, extern_fn_unbound, internal_declarations,
    s_assign, s_expr, s_throw, s_try,
};

/// The Messenger class whose `handleMessage` handles one message.
const MESSENGER_WORKER: &str = "Symfony\\Component\\Messenger\\Worker";
/// The method of `MESSENGER_WORKER` each message goes through.
const MESSENGER_HANDLE: &str = "handleMessage";
/// The event the worker builds for a message whose handling failed.
const MESSENGER_FAILED_EVENT: &str = "Symfony\\Component\\Messenger\\Event\\WorkerMessageFailedEvent";

/// The local holding the open transaction's token in a wrapped body.
const TOKEN_VAR: &str = "__elephc_apm_txn";
/// The local the rethrown exception is caught into.
const ERROR_VAR: &str = "__elephc_apm_error";

/// What a transaction outside HTTP is named after, by kind: a message, a
/// named boundary. The page tells the kinds apart by these.
pub(crate) const MESSAGE_PREFIX: &str = "MSG ";
/// The prefix of a `--apm-transaction` boundary's name.
pub(crate) const BOUNDARY_PREFIX: &str = "TXN ";

/// Checks a `--apm-transaction` target: `Class::method`, or a function name.
pub(crate) fn check_transaction_target(target: &str) -> Result<(), String> {
    let valid = |name: &str| {
        let name = name.trim_start_matches('\\');
        !name.is_empty()
            && name.split('\\').all(|part| {
                part.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_')
                    && part.chars().all(|c| c.is_alphanumeric() || c == '_')
            })
    };
    match target.split_once("::") {
        Some((class, method)) if valid(class) && valid(method) && !method.contains('\\') => Ok(()),
        None if valid(target) => Ok(()),
        _ => Err("expected Class::method or a function name".to_string()),
    }
}

/// Wraps the transaction boundaries of `program`, when the build carries
/// transaction monitoring (`enabled`): Messenger's, and the `targets` the
/// command line named. Declares the probe entries the wraps call, only when
/// something was wrapped.
pub fn inject(program: Program, targets: &[String], enabled: bool) -> Result<Program, CompileError> {
    if !enabled {
        return Ok(program);
    }
    crate::compiler_stack::with_compiler_stack(|| inject_on_compiler_stack(program, targets))
}

/// The body of [`inject`], run on the stack `with_compiler_stack` sized.
fn inject_on_compiler_stack(mut program: Program, targets: &[String]) -> Result<Program, CompileError> {
    // The command line checked them; an embedder passing its own gets the same rule.
    for target in targets {
        if let Err(error) = check_transaction_target(target) {
            return Err(CompileError::new(crate::span::Span::dummy(), &format!("--apm-transaction {target}: {error}")));
        }
    }
    let mut found = vec![false; targets.len()];
    let wrapped = rewrite(&mut program, targets, &mut found);
    for (target, found) in targets.iter().zip(&found) {
        if !found {
            eprintln!(
                "warning: --apm-transaction {target}: no such method or function is declared in this program \
                 (a method a class takes from a trait is named by its trait: Trait::method)"
            );
        }
    }
    if !wrapped {
        return Ok(program);
    }
    let mut combined = crate::name_resolver::resolve(declarations())?;
    combined.extend(program);
    Ok(combined)
}

/// The probe entries a wrapped body calls (the probe archive is linked by
/// the build that enabled this pass).
fn declarations() -> Program {
    internal_declarations(|| {
        vec![
            extern_fn_unbound("elephc_probe_txn_begin").param("name", CType::Str).returns(CType::Int).build(),
            extern_fn_unbound("elephc_probe_txn_fail").param("token", CType::Int).build(),
            extern_fn_unbound("elephc_probe_txn_fail_current").build(),
            extern_fn_unbound("elephc_probe_txn_end")
                .param("token", CType::Int)
                .param("failed", CType::Int)
                .build(),
        ]
    })
}

/// Wraps every boundary in `statements`, namespace blocks included; true
/// when anything was wrapped.
fn rewrite(statements: &mut [Stmt], targets: &[String], found: &mut [bool]) -> bool {
    let mut wrapped = false;
    for stmt in statements.iter_mut() {
        match &mut stmt.kind {
            StmtKind::NamespaceBlock { body, .. } => wrapped |= rewrite(body, targets, found),
            StmtKind::ClassDecl { name, methods, .. } => {
                let class = name.trim_start_matches('\\').to_string();
                for method in methods.iter_mut().filter(|method| method.has_body) {
                    // Once per method: a target naming Messenger's own boundary, or
                    // named twice, would nest two wraps whose inner begin opens
                    // nothing (review).
                    let mut boundary_wrapped = false;
                    if same_name(&class, MESSENGER_WORKER) && same_name(&method.name, MESSENGER_HANDLE) {
                        if let Some((envelope, ..)) = method.params.first() {
                            let message_class = e_call(
                                "\\get_class",
                                vec![e_method_call(e_var(envelope), "getMessage", vec![])],
                            );
                            let name = e_concat(e_str(MESSAGE_PREFIX), message_class);
                            method.body = wrap(std::mem::take(&mut method.body), name);
                            boundary_wrapped = true;
                            wrapped = true;
                        }
                    } else if same_name(&class, MESSENGER_FAILED_EVENT) && same_name(&method.name, "__construct") {
                        method.body.insert(0, s_expr(e_call("\\elephc_probe_txn_fail_current", vec![])));
                        wrapped = true;
                    }
                    wrapped |= wrap_named_method(&class, method, targets, found, boundary_wrapped);
                }
            }
            // An enum's methods are named boundaries too, and so are a trait's,
            // under the trait's name: its body is what every class using it
            // runs (review).
            StmtKind::EnumDecl { name, methods, .. } | StmtKind::TraitDecl { name, methods, .. } => {
                let owner = name.trim_start_matches('\\').to_string();
                for method in methods.iter_mut().filter(|method| method.has_body) {
                    wrapped |= wrap_named_method(&owner, method, targets, found, false);
                }
            }
            StmtKind::FunctionDecl { name, body, .. } => {
                let function = name.trim_start_matches('\\').to_string();
                let mut boundary_wrapped = false;
                for (index, target) in targets.iter().enumerate() {
                    if !target.contains("::") && same_name(&function, target.trim_start_matches('\\')) {
                        found[index] = true;
                        if !boundary_wrapped {
                            let label = format!("{BOUNDARY_PREFIX}{function}");
                            *body = wrap(std::mem::take(body), e_str(&label));
                            boundary_wrapped = true;
                            wrapped = true;
                        }
                    }
                }
            }
            // A declaration inside a block is declared when the block runs:
            // `if (!function_exists('f')) { function f() {} }`, a polyfill's
            // shape. Its boundaries are looked for there too (review: they
            // were reported missing and never counted).
            kind => {
                for body in nested_bodies(kind) {
                    wrapped |= rewrite(body, targets, found);
                }
            }
        }
    }
    wrapped
}

/// The statement lists nested in a control statement, whose declarations run
/// when it runs it.
fn nested_bodies(kind: &mut StmtKind) -> Vec<&mut Vec<Stmt>> {
    match kind {
        StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
            let mut bodies = vec![then_body];
            bodies.extend(elseif_clauses.iter_mut().map(|(_, body)| body));
            bodies.extend(else_body.as_mut());
            bodies
        }
        StmtKind::IfDef { then_body, else_body, .. } => {
            let mut bodies = vec![then_body];
            bodies.extend(else_body.as_mut());
            bodies
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::For { body, .. }
        | StmtKind::Foreach { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body) => vec![body],
        StmtKind::Switch { cases, default, .. } => {
            let mut bodies: Vec<&mut Vec<Stmt>> = cases.iter_mut().map(|(_, body)| body).collect();
            bodies.extend(default.as_mut());
            bodies
        }
        StmtKind::Try { try_body, catches, finally_body } => {
            let mut bodies = vec![try_body];
            bodies.extend(catches.iter_mut().map(|clause| &mut clause.body));
            bodies.extend(finally_body.as_mut());
            bodies
        }
        _ => Vec::new(),
    }
}

/// Wraps `method` of `owner` (a class or an enum) when a `--apm-transaction`
/// target names it, once however many do: not when it is already a boundary
/// (`already`). True when it was wrapped.
fn wrap_named_method(
    owner: &str,
    method: &mut crate::parser::ast::ClassMethod,
    targets: &[String],
    found: &mut [bool],
    already: bool,
) -> bool {
    let mut wrapped = already;
    for (index, target) in targets.iter().enumerate() {
        let Some((target_owner, target_method)) = target.split_once("::") else {
            continue;
        };
        if same_name(owner, target_owner.trim_start_matches('\\')) && same_name(&method.name, target_method) {
            found[index] = true;
            if !wrapped {
                let name = format!("{BOUNDARY_PREFIX}{owner}::{}", method.name);
                method.body = wrap(std::mem::take(&mut method.body), e_str(&name));
                wrapped = true;
            }
        }
    }
    wrapped && !already
}

/// PHP class, method and function names compare without regard to case.
fn same_name(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// The body counted as one transaction named `name`:
///
/// ```php
/// $__elephc_apm_txn = \elephc_probe_txn_begin(<name>);
/// try { <body> }
/// catch (\Throwable $__elephc_apm_error) { \elephc_probe_txn_fail($__elephc_apm_txn); throw $__elephc_apm_error; }
/// finally { \elephc_probe_txn_end($__elephc_apm_txn, 0); }
/// ```
fn wrap(body: Vec<Stmt>, name: Expr) -> Vec<Stmt> {
    vec![
        s_assign(TOKEN_VAR, e_call("\\elephc_probe_txn_begin", vec![name])),
        s_try(
            body,
            vec![(
                vec!["\\Throwable"],
                Some(ERROR_VAR),
                vec![
                    s_expr(e_call("\\elephc_probe_txn_fail", vec![e_var(TOKEN_VAR)])),
                    s_throw(e_var(ERROR_VAR)),
                ],
            )],
            Some(vec![s_expr(e_call("\\elephc_probe_txn_end", vec![e_var(TOKEN_VAR), e_int(0)]))]),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses and name-resolves a fixture the way the pipeline does.
    fn resolved(source: &str) -> Program {
        let tokens = crate::lexer::tokenize(source).expect("test source must tokenize");
        let program = crate::parser::parse(&tokens).expect("test source must parse");
        crate::name_resolver::resolve(program).expect("test source must resolve")
    }

    /// The names of the functions a program calls, in order of appearance.
    fn calls(program: &Program) -> String {
        format!("{program:?}")
    }

    #[test]
    fn transaction_targets_are_validated() {
        for good in ["App\\Jobs\\Import::run", "\\App\\Foo::handle", "process_batch", "Ns\\process"] {
            assert!(check_transaction_target(good).is_ok(), "{good}");
        }
        for bad in ["", "::run", "App\\Foo::", "App\\Foo::Ns\\run", "1abc", "a b", "Foo::bar::baz"] {
            assert!(check_transaction_target(bad).is_err(), "{bad}");
        }
    }

    /// A build without transaction monitoring is left exactly as it was.
    #[test]
    fn nothing_changes_without_monitoring() {
        let source = "<?php namespace Symfony\\Component\\Messenger; class Worker { private function handleMessage($envelope, string $t): void { echo 1; } }";
        let program = resolved(source);
        let before = calls(&program);
        let after = inject(program, &[], false).expect("no error");
        assert_eq!(calls(&after), before);
    }

    /// Messenger's worker counts each message under its class, and the
    /// failure event marks it failed; the probe entries are declared.
    #[test]
    fn messenger_messages_are_transactions() {
        let source = "<?php namespace Symfony\\Component\\Messenger\\Event { class WorkerMessageFailedEvent { public function __construct($e) {} } }
            namespace Symfony\\Component\\Messenger { class Worker { private function handleMessage($envelope, string $t): void { echo 1; } } }";
        let after = calls(&inject(resolved(source), &[], true).expect("no error"));
        assert!(after.contains("elephc_probe_txn_begin"), "{after}");
        assert!(after.contains("getMessage"), "{after}");
        assert!(after.contains("elephc_probe_txn_fail_current"), "{after}");
        assert!(after.contains("ExternFunctionDecl"), "{after}");
    }

    /// A program without a boundary declares nothing.
    #[test]
    fn a_program_without_boundaries_declares_nothing() {
        let after = inject(resolved("<?php function f() { return 1; } echo f();"), &[], true).expect("no error");
        assert!(!calls(&after).contains("elephc_probe_txn"));
    }

    /// An enum's method is a named boundary like a class's (review).
    #[test]
    fn an_enum_method_is_a_named_boundary() {
        let source = "<?php namespace App; enum Job { case Import; public function run(): int { return 1; } }";
        let targets = vec!["App\\Job::run".to_string()];
        let after = calls(&inject(resolved(source), &targets, true).expect("no error"));
        assert!(after.contains("TXN App\\\\Job::run"), "{after}");
        assert_eq!(after.matches("parts: [\"elephc_probe_txn_begin\"]").count(), 1, "{after}");
    }

    /// A function or class declared inside a block — a polyfill's
    /// `if (!function_exists(...))` — is a named boundary too (review).
    #[test]
    fn a_conditional_declaration_is_a_named_boundary() {
        let source = "<?php if (!function_exists('job')) { function job(): int { return 1; } } try { if (true) { class Runner { public function run(): int { return 2; } } } } finally {}";
        let targets = vec!["job".to_string(), "Runner::run".to_string()];
        let after = calls(&inject(resolved(source), &targets, true).expect("no error"));
        assert!(after.contains("TXN job") && after.contains("TXN Runner::run"), "{after}");
        assert_eq!(after.matches("parts: [\"elephc_probe_txn_begin\"]").count(), 2, "{after}");
    }

    /// A trait's method is a named boundary under the trait's name: its body
    /// is what every class using it runs (review).
    #[test]
    fn a_trait_method_is_a_named_boundary() {
        let source = "<?php namespace App; trait Imports { public function importBatch(): int { return 1; } } class Job { use Imports; }";
        let targets = vec!["App\\Imports::importBatch".to_string()];
        let after = calls(&inject(resolved(source), &targets, true).expect("no error"));
        assert!(after.contains("TXN App\\\\Imports::importBatch"), "{after}");
        assert_eq!(after.matches("parts: [\"elephc_probe_txn_begin\"]").count(), 1, "{after}");
    }

    /// A boundary is wrapped once, however often it is named: Messenger's
    /// own, named again, and a target given twice (review).
    #[test]
    fn a_boundary_is_wrapped_once() {
        let source = "<?php namespace Symfony\\Component\\Messenger { class Worker { private function handleMessage($envelope, string $t): void { echo 1; } } }
            namespace { function job() {} }";
        let targets = vec![
            "Symfony\\Component\\Messenger\\Worker::handleMessage".to_string(),
            "job".to_string(),
            "job".to_string(),
        ];
        let after = calls(&inject(resolved(source), &targets, true).expect("no error"));
        // One call per boundary: the worker's and the job's.
        assert_eq!(after.matches("parts: [\"elephc_probe_txn_begin\"]").count(), 2, "{after}");
        assert!(!after.contains("TXN Symfony"), "Messenger's own name wins: {after}");
    }

    /// `--apm-transaction` wraps the named method and function, whatever
    /// the case they are written in.
    #[test]
    fn named_boundaries_are_wrapped() {
        let source = "<?php namespace App\\Jobs; class Import { public function run() { return 1; } } function process_batch() {}";
        let targets = vec!["app\\jobs\\IMPORT::Run".to_string(), "App\\Jobs\\process_batch".to_string()];
        let after = calls(&inject(resolved(source), &targets, true).expect("no error"));
        assert!(after.contains("TXN App\\\\Jobs\\\\Import::run"), "{after}");
        assert!(after.contains("TXN App\\\\Jobs\\\\process_batch"), "{after}");
    }
}
