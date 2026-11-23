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
//!   when an exception leaves it. A generator is refused: a call only creates
//!   it, and its body runs while the caller iterates it. A method a class
//!   inherits is named by the class or trait that declares it, which the
//!   warning for an undeclared target names.
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

/// The HttpKernel class that turns an exception into an error response.
const HTTP_KERNEL: &str = "Symfony\\Component\\HttpKernel\\HttpKernel";
/// The method of `HTTP_KERNEL` every exception a request throws goes through.
const HTTP_KERNEL_HANDLE_THROWABLE: &str = "handleThrowable";

/// The variable the `--web` handler's catch-all catches into (`crate::web_prelude`), and the
/// one a catch-all that binds none is given: its catch notes the error of a request that
/// ends with an uncaught exception.
pub(crate) const WEB_CATCH_VAR: &str = "__elephc_exc";

/// The stem of the local holding the open transaction's token in a wrapped
/// body; the local itself is `generated_local_name`d, so PHP code cannot see it.
const TOKEN_VAR: &str = "__elephc_apm_txn";
/// The stem of the local the rethrown exception is caught into.
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
    let mut search = Search { targets, found: vec![false; targets.len()], refused: None, owners: Default::default() };
    let mut wrapped = rewrite(&mut program, &mut search);
    if let Some(refusal) = search.refused {
        return Err(CompileError::new(crate::span::Span::dummy(), &refusal));
    }
    wrapped |= note_web_catches(&mut program);
    for (target, found) in targets.iter().zip(&search.found) {
        if !found {
            eprintln!("warning: --apm-transaction {target}: {}", search.not_found(target));
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
            extern_fn_unbound("elephc_probe_note_error")
                .param("class", CType::Str)
                .param("file", CType::Str)
                .param("line", CType::Int)
                .build(),
        ]
    })
}

/// What declares a method: a class, an enum or a trait, with what it inherits
/// from (`Search::declaring_owner`).
struct Owner {
    /// The declared name, without a leading `\`.
    name: String,
    /// The class it extends, lowercased.
    parent: Option<String>,
    /// The traits it uses, lowercased.
    traits: Vec<String>,
    /// The methods it declares with a body, which a target can wrap.
    methods: Vec<String>,
}

/// The `--apm-transaction` targets looked for in one program, and what the
/// walk found of them.
struct Search<'a> {
    /// The targets, `Class::method` or a function name.
    targets: &'a [String],
    /// Per target, whether a declaration it names was found.
    found: Vec<bool>,
    /// The first target refused, with why: a generator.
    refused: Option<String>,
    /// Every class, enum and trait seen, by lowercased name, for the warning
    /// naming where an inherited method is declared.
    owners: std::collections::HashMap<String, Owner>,
}

impl Search<'_> {
    /// Records `name` (a class, enum or trait) with what it inherits from and
    /// the methods it declares with a body.
    fn note_owner(&mut self, name: &str, parent: Option<&crate::names::Name>, uses: &[crate::parser::ast::TraitUse], methods: &[crate::parser::ast::ClassMethod]) {
        let name = name.trim_start_matches('\\').to_string();
        let owner = Owner {
            parent: parent.map(|parent| parent.as_str().trim_start_matches('\\').to_ascii_lowercase()),
            traits: uses
                .iter()
                .flat_map(|used| &used.trait_names)
                .map(|used| used.as_str().trim_start_matches('\\').to_ascii_lowercase())
                .collect(),
            methods: methods.iter().filter(|method| method.has_body).map(|method| method.name.clone()).collect(),
            name,
        };
        self.owners.insert(owner.name.to_ascii_lowercase(), owner);
    }

    /// The class or trait that declares `method` for `class` when `class`
    /// inherits it, its traits first and then its parents, as PHP resolves it.
    fn declaring_owner(&self, class: &str, method: &str) -> Option<&str> {
        let mut seen = std::collections::HashSet::new();
        let start = class.trim_start_matches('\\').to_ascii_lowercase();
        let mut pending = vec![start.clone()];
        while let Some(key) = pending.pop() {
            if !seen.insert(key.clone()) {
                continue;
            }
            let Some(owner) = self.owners.get(&key) else {
                continue;
            };
            if key != start && owner.methods.iter().any(|declared| same_name(declared, method)) {
                return Some(&owner.name);
            }
            // Popped last-in first: the traits are searched before the parent.
            pending.extend(owner.parent.iter().cloned());
            pending.extend(owner.traits.iter().rev().cloned());
        }
        None
    }

    /// Why `target` matched nothing: an inherited method names the class or
    /// trait to give instead; anything else is not declared.
    fn not_found(&self, target: &str) -> String {
        if let Some((class, method)) = target.split_once("::") {
            if let Some(owner) = self.declaring_owner(class, method) {
                return format!(
                    "{class} inherits {method} from {owner}, whose body is the one that runs: name {owner}::{method} \
                     (it then counts the calls on every class that inherits it)"
                );
            }
        }
        "no such method or function is declared in this program \
         (a method a class takes from a trait is named by its trait: Trait::method)"
            .to_string()
    }

    /// Refuses `target` (the first refusal is reported) for naming `name`, a
    /// generator: a call only creates it, so there is no call to time.
    fn refuse_generator(&mut self, target: &str, name: &str) {
        self.refused.get_or_insert_with(|| {
            format!(
                "--apm-transaction {target}: {name} is a generator. A call only creates the generator, and its body \
                 runs while the caller iterates it, so the call has no duration of its own to count. Name the method \
                 or function that iterates it instead"
            )
        });
    }
}

/// Wraps every boundary in `statements`, namespace blocks included; true
/// when anything was wrapped.
fn rewrite(statements: &mut [Stmt], search: &mut Search) -> bool {
    let mut wrapped = false;
    for stmt in statements.iter_mut() {
        match &mut stmt.kind {
            StmtKind::NamespaceBlock { body, .. } => wrapped |= rewrite(body, search),
            StmtKind::ClassDecl { name, extends, trait_uses, methods, .. } => {
                search.note_owner(name, extends.as_ref(), trait_uses, methods);
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
                        // `(Envelope $envelope, string $receiverName, \Throwable $error)`: the
                        // handler's exception is the message's error.
                        if let Some((error, ..)) = method.params.get(2) {
                            method.body.insert(0, note_error(error));
                        }
                        wrapped = true;
                    } else if same_name(&class, HTTP_KERNEL) && same_name(&method.name, HTTP_KERNEL_HANDLE_THROWABLE) {
                        // Symfony answers a request's exception itself, with an error page: the
                        // probe counts it if that answer is a 5xx, not a 404.
                        if let Some((error, ..)) = method.params.first() {
                            method.body.insert(0, note_error(error));
                            wrapped = true;
                        }
                    }
                    wrapped |= wrap_named_method(&class, method, search, boundary_wrapped);
                }
            }
            // An enum's methods are named boundaries too, and so are a trait's,
            // under the trait's name: its body is what every class using it
            // runs (review).
            StmtKind::EnumDecl { name, trait_uses, methods, .. } | StmtKind::TraitDecl { name, trait_uses, methods, .. } => {
                search.note_owner(name, None, trait_uses, methods);
                let owner = name.trim_start_matches('\\').to_string();
                for method in methods.iter_mut().filter(|method| method.has_body) {
                    wrapped |= wrap_named_method(&owner, method, search, false);
                }
            }
            StmtKind::FunctionDecl { name, body, .. } => {
                let function = name.trim_start_matches('\\').to_string();
                let mut boundary_wrapped = false;
                let targets = search.targets;
                for (index, target) in targets.iter().enumerate() {
                    if !target.contains("::") && same_name(&function, target.trim_start_matches('\\')) {
                        search.found[index] = true;
                        if crate::types::checker::yield_validation::body_contains_yield(body) {
                            search.refuse_generator(target, &function);
                        } else if !boundary_wrapped {
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
                    wrapped |= rewrite(body, search);
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

/// Wraps `method` of `owner` (a class, an enum or a trait) when a
/// `--apm-transaction` target names it, once however many do: not when it is
/// already a boundary (`already`), nor when it is a generator, which refuses
/// the target. True when it was wrapped.
fn wrap_named_method(
    owner: &str,
    method: &mut crate::parser::ast::ClassMethod,
    search: &mut Search,
    already: bool,
) -> bool {
    let mut wrapped = already;
    let targets = search.targets;
    for (index, target) in targets.iter().enumerate() {
        let Some((target_owner, target_method)) = target.split_once("::") else {
            continue;
        };
        if same_name(owner, target_owner.trim_start_matches('\\')) && same_name(&method.name, target_method) {
            search.found[index] = true;
            if crate::types::checker::yield_validation::body_contains_yield(&method.body) {
                search.refuse_generator(target, &format!("{owner}::{}", method.name));
            } else if !wrapped {
                let name = format!("{BOUNDARY_PREFIX}{owner}::{}", method.name);
                method.body = wrap(std::mem::take(&mut method.body), e_str(&name));
                wrapped = true;
            }
        }
    }
    wrapped && !already
}

/// `\elephc_probe_note_error(\get_class($var), $var->getFile(), $var->getLine());`: notes the
/// exception in `$var` as the error of the transaction it is failing.
fn note_error(var: &str) -> Stmt {
    s_expr(e_call(
        "\\elephc_probe_note_error",
        vec![
            e_call("\\get_class", vec![e_var(var)]),
            e_method_call(e_var(var), "getFile", vec![]),
            e_method_call(e_var(var), "getLine", vec![]),
        ],
    ))
}

/// Notes the error in every catch of a `--web` catch-all, wherever the web prelude put it:
/// the program's top level, or a function of the application adapter. Found by the call
/// they make (`web_prelude::answers_uncaught`), not by how their variable is named. True
/// when one was found.
fn note_web_catches(statements: &mut [Stmt]) -> bool {
    let mut noted = false;
    for stmt in statements.iter_mut() {
        if let StmtKind::Try { catches, .. } = &mut stmt.kind {
            for clause in catches.iter_mut().filter(|clause| crate::web_prelude::answers_uncaught(&clause.body)) {
                // The application adapter's catch binds nothing: the exception gets a name here.
                let var = clause.variable.get_or_insert_with(|| WEB_CATCH_VAR.to_string()).clone();
                clause.body.insert(0, note_error(&var));
                noted = true;
            }
        }
        match &mut stmt.kind {
            StmtKind::NamespaceBlock { body, .. } | StmtKind::FunctionDecl { body, .. } => noted |= note_web_catches(body),
            kind => {
                for body in nested_bodies(kind) {
                    noted |= note_web_catches(body);
                }
            }
        }
    }
    noted
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
    let token = crate::names::generated_local_name(TOKEN_VAR);
    let error = crate::names::generated_local_name(ERROR_VAR);
    vec![
        s_assign(&token, e_call("\\elephc_probe_txn_begin", vec![name])),
        s_try(
            body,
            vec![(
                vec!["\\Throwable"],
                Some(&error),
                vec![
                    note_error(&error),
                    s_expr(e_call("\\elephc_probe_txn_fail", vec![e_var(&token)])),
                    s_throw(e_var(&error)),
                ],
            )],
            Some(vec![s_expr(e_call("\\elephc_probe_txn_end", vec![e_var(&token), e_int(0)]))]),
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

    /// Where a request's exception is answered — the `--web` catch-all, Symfony's
    /// `HttpKernel::handleThrowable` — it is noted as the request's error, and the probe
    /// entry is declared.
    #[test]
    fn a_requests_exception_is_noted_where_it_is_answered() {
        let source = "<?php namespace Symfony\\Component\\HttpKernel { class HttpKernel { private function handleThrowable(\\Throwable $e, $request, int $type) { return 1; } } }
            namespace {
                function handle() { try { echo 1; } catch (\\Throwable $__elephc_exc) { elephc_web_handle_uncaught_exception(); } }
                function adapter() { try { echo 1; } catch (\\Throwable) { elephc_web_handle_uncaught_exception(); } }
            }";
        let program = inject(resolved(source), &[], true).expect("no error");
        let text = calls(&program);
        assert_eq!(text.matches("ExternFunctionDecl { name: \"elephc_probe_note_error\"").count(), 1, "declared: {text}");
        assert_eq!(text.matches("text: \"elephc_probe_note_error\"").count(), 3, "called thrice: {text}");
        assert_eq!(text.matches("variable: Some(\"__elephc_exc\")").count(), 2, "the adapter's catch is given a variable: {text}");
        let untouched = "<?php function f() { try { echo 1; } catch (\\Throwable $e) { echo 2; } }";
        assert!(!calls(&inject(resolved(untouched), &[], true).expect("no error")).contains("elephc_probe_note_error"));
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

    /// A generator function or method named as a transaction is refused with
    /// why: a call only creates the generator, so the wrap would time the
    /// iteration, not the call (#2400).
    #[test]
    fn a_generator_target_is_refused() {
        let source = "<?php namespace App; function rows() { yield 1; } class Feed { public function items() { yield from [1, 2]; } public function count(): int { return 2; } }";
        for (target, name) in [("App\\rows", "App\\rows"), ("App\\Feed::items", "App\\Feed::items")] {
            let error = inject(resolved(source), &[target.to_string()], true).expect_err(target);
            assert!(error.message.contains(&format!("{name} is a generator")), "{}", error.message);
        }
        // A generator's sibling stays a boundary; a closure's yield is its own generator.
        let closure = "<?php function make() { return function () { yield 1; }; }";
        let after = calls(&inject(resolved(closure), &["make".to_string()], true).expect("make is no generator"));
        assert!(after.contains("TXN make"), "{after}");
        let after = calls(&inject(resolved(source), &["App\\Feed::count".to_string()], true).expect("no error"));
        assert!(after.contains("TXN App\\\\Feed::count"), "{after}");
    }

    /// A target naming a method through a class that inherits it, from a
    /// parent or a trait, is reported with the class or trait that declares
    /// it, which is the name to give (#2398). A method nobody declares keeps
    /// the plain warning.
    #[test]
    fn an_inherited_target_names_where_its_method_is_declared() {
        let source = "<?php namespace App;
            trait Retries { public function retry(): int { return 1; } }
            abstract class Base { use Retries; public function run(): int { return 1; } abstract public function name(): string; }
            class Child extends Base { public function name(): string { return 'c'; } }";
        let targets: Vec<String> =
            ["App\\Child::run", "App\\Child::retry", "App\\Child::missing", "App\\Base::name"].map(str::to_string).to_vec();
        let mut program = resolved(source);
        let mut search = Search { targets: &targets, found: vec![false; targets.len()], refused: None, owners: Default::default() };
        assert!(!rewrite(&mut program, &mut search), "nothing is wrapped");
        assert_eq!(search.found, vec![false; 4]);
        let run = search.not_found(&targets[0]);
        assert!(run.contains("App\\Child inherits run from App\\Base") && run.contains("name App\\Base::run"), "{run}");
        let retry = search.not_found(&targets[1]);
        assert!(retry.contains("inherits retry from App\\Retries") && retry.contains("App\\Retries::retry"), "{retry}");
        for plain in [&targets[2], &targets[3]] {
            assert!(search.not_found(plain).starts_with("no such method or function"), "{plain}");
        }
    }

    /// The wrap's locals are compiler-generated, so `get_defined_vars()`,
    /// `compact()` and eval cannot see them (#2397).
    #[test]
    fn wrap_locals_are_hidden_from_php() {
        let body = wrap(Vec::new(), e_str("TXN job"));
        let StmtKind::Assign { name, .. } = &body[0].kind else {
            panic!("the wrap opens with the token assignment: {body:?}");
        };
        assert!(crate::names::is_generated_local_name(name), "{name}");
        let StmtKind::Try { catches, .. } = &body[1].kind else {
            panic!("the wrap's second statement is its try: {body:?}");
        };
        let error = catches[0].variable.as_deref().expect("the catch binds the error");
        assert!(crate::names::is_generated_local_name(error), "{error}");
    }
}
