//! Purpose:
//! The canonical set of HTTP-request superglobals exposed under `--web`, the shared
//! PhpType for them, and the pass that creates the ones PHP's CLI SAPI would already
//! have created. Single source of truth consumed by the type checker, the IR lowering
//! global-storage path, and `__rt_web_reset`.
//!
//! Called from:
//! - `crate::types::checker` (seeding), `crate::ir_lower::context` (global
//!   storage), `crate::codegen::web` (per-request reset), `crate::optimize`
//!   (`seed_cli_populated_superglobals`), `crate::pipeline` (the `--web` flag).
//!
//! Key details:
//! - These names use `_eir_global_*` symbol storage in EVERY scope (true
//!   superglobals), unlike `$argc`/`$argv` which are top-level only.
//! - `SUPERGLOBALS` (what `--web` exposes) and `CLI_POPULATED_SUPERGLOBALS` (what a
//!   CLI program finds already there) are DIFFERENT sets, both measured against
//!   reference PHP rather than derived from each other.

use crate::ir::{Immediate, Module, Op};
use crate::types::PhpType;

thread_local! {
    /// Whether this compile targets `--web`, for the passes that cannot be handed the flag
    /// through their signature.
    ///
    /// `seed_cli_populated_superglobals` runs from `optimize::fold_constants`, and that has
    /// THIRTEEN call sites: the driver plus twelve test harnesses that each rebuild the phase
    /// order by hand (`tests/codegen/support/compiler.rs` says so in a comment: "Mirrors
    /// `pipeline::compile`"). A pass that changes what a program MEANS must not be one of the
    /// things those harnesses can forget to mirror, or the suites keep pinning the behaviour
    /// the driver no longer has. Riding along inside a call they all already make is what
    /// makes it unforgettable; this flag is the price.
    ///
    /// THREAD-LOCAL, not a process-wide static: one compilation is single-threaded, but a test
    /// binary runs many of them in ONE process, on several threads. A shared flag would let a
    /// `--web` unit test blank the flag another thread is compiling CLI code under, and a panic
    /// between set and reset would leave every later compile in that process in web mode. The
    /// lifetime of this fact is "this compile", and a thread-local says exactly that;
    /// `optimize.rs` already uses `thread_local!` for the same reason.
    static COMPILING_FOR_WEB: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Records whether the compile in progress is a `--web` build.
pub fn set_compiling_for_web(web: bool) {
    COMPILING_FOR_WEB.with(|cell| cell.set(web));
}

/// Returns true when the compile in progress is a `--web` build.
pub fn compiling_for_web() -> bool {
    COMPILING_FOR_WEB.with(std::cell::Cell::get)
}

thread_local! {
    /// The entry source this compile was asked to build, for the `$_SERVER` path keys.
    ///
    /// A thread-local for the same reason as the flag above: the fact lives exactly as long as
    /// this compile. It cannot be a parameter -- the seeding rides on `optimize::fold_constants`,
    /// which thirteen hand-rolled pipelines call, and threading a path through all of them to
    /// reach one statement is worse than saying where the fact lives.
    static ENTRY_SCRIPT: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// Records the entry source path for the compile in progress.
pub fn set_entry_script(path: &str) {
    ENTRY_SCRIPT.with(|cell| *cell.borrow_mut() = path.to_string());
}

/// Returns the entry source path, or an empty string when none was recorded.
pub(crate) fn entry_script() -> String {
    ENTRY_SCRIPT.with(|cell| cell.borrow().clone())
}

/// PHP request superglobals visible in every scope under `--web`.
pub const SUPERGLOBALS: &[&str] =
    &["_SERVER", "_GET", "_POST", "_COOKIE", "_REQUEST", "_ENV", "_FILES", "_SESSION"];

/// Returns true when `name` (without leading `$`) is a request superglobal.
pub fn is_superglobal(name: &str) -> bool {
    SUPERGLOBALS.contains(&name)
}

/// The superglobals PHP's CLI SAPI has already populated when the script starts.
///
/// Measured, not assumed — and the FIRST measurement was wrong, which is worth stating
/// because the trap is easy to fall into twice. Probing with `isset($GLOBALS["_ENV"])`
/// answers false, so the set looked like five. PHP materializes an auto-global when the
/// script MENTIONS IT BY NAME; a string subscript of `$GLOBALS` is not a mention. Probing
/// with `isset($_ENV)` answers true, and the set is SEVEN:
///
/// ```text
/// php -n -r 'var_dump(isset($_ENV), isset($_REQUEST), isset($_SESSION));'
/// bool(true)  bool(true)  bool(false)
/// ```
///
/// Only `$_SESSION` is genuinely absent until `session_start()`.
///
/// The distinction is observable and both halves are pinned by tests: a CLI
/// `count($_SERVER)` must answer a number rather than raise `count(): Argument #1
/// ($value) must be of type Countable|array, null given`, while `isset($_SESSION)`
/// must stay false. Off-web these names are ordinary top-level locals — nothing
/// pre-initializes the shared `_eir_global_*` storage — so a program that never
/// mentions one pays nothing for it.
pub const CLI_POPULATED_SUPERGLOBALS: &[&str] =
    &["_SERVER", "_GET", "_POST", "_COOKIE", "_FILES", "_ENV", "_REQUEST"];

/// The shared type of every request superglobal: a string-keyed associative
/// array of heterogeneous (Mixed) values.
pub fn superglobal_type() -> PhpType {
    PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    }
}

/// Returns every global that needs shared reference-cell storage, in ONE walk of the module.
///
/// `uses_shared_ref_cell` answers for one name and walks every instruction of every body to do
/// it, and codegen asks it per scope entry at every `eval` site. A sample of the codegen phase
/// caught that chain — `lower_dynamic_include` -> `reload_eval_global_scope` ->
/// `store_missing_scope_entry_to_global` -> here — holding the single heaviest iterator in the
/// phase. Callers that hold a `SharedCodegenState` ask it instead, and it computes this set once.
pub(crate) fn shared_ref_cell_globals(module: &Module) -> crate::fast_hash::FastSet<String> {
    let mut names = crate::fast_hash::FastSet::default();
    for function in module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
    {
        for inst in &function.instructions {
            if !matches!(inst.op, Op::InvokerRefArg | Op::GlobalRefCell) {
                continue;
            }
            let Some(Immediate::GlobalName(data)) = inst.immediate else {
                continue;
            };
            if let Some(candidate) = module.data.global_names.get(data.as_raw() as usize) {
                names.insert(candidate.clone());
            }
        }
    }
    names
}

/// Returns true when any EIR body requires shared reference-cell storage for this global.
///
/// Only those symbols change from direct hash storage to shared ref-cell storage, keeping the
/// ordinary fast path and eval/pointer ABI unchanged for every other request superglobal.
pub(crate) fn uses_shared_ref_cell(module: &Module, name: &str) -> bool {
    // One walk per module: each superglobal used to walk every instruction of the program.
    module.shared_ref_cell_globals(|| ref_cell_bound_globals(module)).contains(name)
}

/// Walks every body for the globals [`uses_shared_ref_cell`] answers for.
fn ref_cell_bound_globals(module: &Module) -> crate::fast_hash::FastSet<String> {
    module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
        .flat_map(|function| function.instructions.iter())
        .filter_map(|inst| {
            if !matches!(inst.op, Op::InvokerRefArg | Op::GlobalRefCell) {
                return None;
            }
            let Some(Immediate::GlobalName(data)) = inst.immediate else {
                return None;
            };
            module.data.global_names.get(data.as_raw() as usize).cloned()
        })
        .collect()
}

/// Gives a CLI program the superglobals PHP's CLI SAPI would already have created,
/// with the CONTENTS PHP puts in them, for every one the source SPELLS.
///
/// Off-web these names are ordinary top-level locals: nothing pre-initializes the
/// shared `_eir_global_*` storage, so a bare read used to yield `null` and every
/// consumer inherited that — silently for `$_SERVER['x']`, and loudly once
/// `count()` started raising PHP's TypeError for a non-countable argument.
///
/// They were seeded EMPTY at first, which fixed the null but left a second,
/// quieter divergence: a PHP program reading `$_ENV['HOME']` or `$_SERVER['argv']`
/// got nothing rather than the wrong thing, and nothing looks like "not set".
/// Measured against `php -n` on a script file, the CLI SAPI creates:
///
/// - `$_ENV` — exactly `getenv()`, entry for entry.
/// - `$_SERVER` — the same environment, plus nine of its own: `argv`, `argc`,
///   `PHP_SELF`, `SCRIPT_NAME`, `SCRIPT_FILENAME`, `PATH_TRANSLATED`,
///   `DOCUMENT_ROOT`, `REQUEST_TIME` and `REQUEST_TIME_FLOAT`.
/// - `$_GET`, `$_POST`, `$_COOKIE`, `$_FILES`, `$_REQUEST` — empty arrays, which
///   is what they already were.
///
/// The four path-shaped keys are `$argv[0]`. PHP names the script it was asked to
/// run; a compiled program has no script at run time, and the thing that WAS
/// invoked is the closest true answer rather than a fabricated path.
/// `DOCUMENT_ROOT` is empty because PHP leaves it empty off-web.
///
/// Seeding is driven by what the program mentions, so a source that never names
/// one emits nothing for it and the binary does not grow — which is also what
/// PHP's own `auto_globals_jit` does, for the same reason.
///
/// Only `$_SESSION` is deliberately absent from the seeded set — it does not
/// exist until `session_start()` — see `CLI_POPULATED_SUPERGLOBALS`.
pub fn seed_cli_populated_superglobals(
    program: crate::parser::ast::Program,
) -> crate::parser::ast::Program {
    use crate::parser::ast::{Stmt, StmtKind};

    // Under `--web` the request prelude OWNS these names: it stores the real request
    // data into `_eir_global_*` before the script body runs. Seeding there would
    // wipe the request.
    if compiling_for_web() {
        return program;
    }
    let spelled = crate::prelude_prune::usage::collect(&program).variables;
    let mut seeds: Vec<Stmt> = Vec::new();
    for name in CLI_POPULATED_SUPERGLOBALS.iter().filter(|name| spelled.contains(**name)) {
        seeds.extend(seed_for(name));
    }
    if seeds.is_empty() {
        return program;
    }
    // One `Synthetic` group rather than loose statements, so nothing that reasons
    // about the first top-level statement (a `declare`, an include-once guard) sees
    // a different one than the source wrote.
    let mut seeded = Vec::with_capacity(program.len() + 1);
    seeded.push(Stmt::new(StmtKind::Synthetic(seeds), crate::span::Span::synthetic()));
    seeded.extend(program);
    seeded
}

/// Builds the statements that give ONE superglobal the contents PHP gives it.
///
/// `$_ENV` and `$_SERVER` carry the environment; the rest are empty arrays, which
/// is what PHP's CLI SAPI leaves them as. Emitted as ordinary PHP statements
/// rather than a runtime call, so the same optimizer, checker and ownership
/// passes see them as they would see the program's own code.
fn seed_for(name: &str) -> Vec<crate::parser::ast::Stmt> {
    use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
    use crate::synthetic_class::{
        e_bool, e_call, e_str, e_var, s_array_assign, s_assign,
    };

    let span = crate::span::Span::synthetic();
    let empty = || Expr::new(ExprKind::ArrayLiteral(Vec::new()), span);

    match name {
        // `$_ENV` is `getenv()`: PHP populates it from the same environment, and
        // the two compare equal entry for entry.
        "_ENV" => vec![s_assign("_ENV", e_call("getenv", Vec::new()))],
        "_SERVER" => {
            // The environment first, then PHP's own keys on top — the order
            // matters only if a variable is literally named `argv`, in which case
            // PHP's key wins, as it does here.
            let mut out = vec![s_assign("_SERVER", e_call("getenv", Vec::new()))];
            // The SCRIPT, not the executable. php reports the file it was asked to run, and a
            // compiled program HAS that file -- it is the entry these statements are prepended
            // to, so `__FILE__` names it. `$argv[0]` was the closest true answer only if you
            // grant that a compiled program has no script, and PHP that re-reads its own entry
            // shows why that does not hold: `vendor/autoload_runtime.php` does
            // `$app = require $_SERVER['SCRIPT_FILENAME']` to collect the closure the entry
            // returns, and pointed at the executable it tries to PARSE 108 MB of Mach-O.
            // `--web` already answers `__FILE__` here (`web_prelude::build`, bootstrap 6a),
            // which is why the same entry works there and not in the CLI SAPI.
            // A LITERAL, not `__FILE__`: this seeding is prepended at `fold_constants`, and
            // magic constants are lowered before that, so one reaching the optimizer panics.
            let entry = entry_script();
            let script = || e_str(&entry);
            for key in ["PHP_SELF", "SCRIPT_NAME", "SCRIPT_FILENAME", "PATH_TRANSLATED"] {
                out.push(s_array_assign("_SERVER", e_str(key), script()));
            }
            out.push(s_array_assign("_SERVER", e_str("DOCUMENT_ROOT"), e_str("")));
            out.push(s_array_assign("_SERVER", e_str("REQUEST_TIME"), e_call("time", Vec::new())));
            out.push(s_array_assign(
                "_SERVER",
                e_str("REQUEST_TIME_FLOAT"),
                e_call("microtime", vec![e_bool(true)]),
            ));
            out.push(s_array_assign("_SERVER", e_str("argv"), e_var("argv")));
            out.push(s_array_assign("_SERVER", e_str("argc"), e_var("argc")));
            out
        }
        _ => vec![Stmt::new(
            StmtKind::Assign { name: name.to_string(), value: empty() },
            span,
        )],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::StmtKind;

    /// Parses PHP and returns the names the seeding pass prepends, in order.
    fn seeded_names(source: &str) -> Vec<String> {
        let tokens = crate::lexer::tokenize(source).expect("tokenize");
        let program = crate::parser::parse(&tokens).expect("parse");
        let seeded = seed_cli_populated_superglobals(program);
        match seeded.first().map(|stmt| &stmt.kind) {
            Some(StmtKind::Synthetic(seeds)) => seeds
                .iter()
                .filter_map(|stmt| match &stmt.kind {
                    StmtKind::Assign { name, .. } => Some(name.clone()),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Pay-for-use is what lets this land on a branch about binary size: a program
    /// that never spells a superglobal must emit nothing for one. The end-to-end
    /// tests cannot see this — they only observe the names that ARE used.
    #[test]
    fn a_program_that_names_no_superglobal_is_left_alone() {
        assert!(seeded_names("<?php echo 1;").is_empty());
    }

    /// Only the name the source spells is seeded, not the whole set.
    #[test]
    fn only_the_superglobals_the_source_spells_are_seeded() {
        assert_eq!(seeded_names("<?php echo count($_GET);"), vec!["_GET".to_string()]);
    }

    /// `$_SESSION` is never seeded even when spelled: PHP's CLI SAPI does not create it
    /// until `session_start()`, so `isset($_SESSION)` must stay false. `$_ENV` and
    /// `$_REQUEST` ARE created and are seeded alongside it here, which is what separates
    /// "not in the set" from "not mentioned".
    #[test]
    fn the_session_superglobal_is_never_seeded() {
        let source = "<?php echo isset($_SESSION), isset($_ENV), isset($_REQUEST);";
        assert_eq!(
            seeded_names(source),
            vec!["_ENV".to_string(), "_REQUEST".to_string()],
        );
    }

    /// A `--web` compile must be left completely alone: the request prelude owns
    /// these names there, and an empty literal would wipe the request.
    #[test]
    fn a_web_compile_is_left_alone() {
        set_compiling_for_web(true);
        let seeded = seeded_names("<?php echo count($_GET);");
        set_compiling_for_web(false);
        assert!(seeded.is_empty());
    }
}
