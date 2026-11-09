//! Purpose:
//! Owns the closed-world build mode, "link no interpreter", which is how a program is compiled
//! unless it needs the interpreter.
//!
//! Called from:
//! - `crate::cli` (`--with-eval`, `ELEPHC_CLOSED_WORLD`), `crate::pipeline::compile()` (sets the
//!   mode), the lowering of `eval()` and runtime `include`/`require`, and the final bridge check.
//!
//! Key details:
//! - The state is thread-local like `crate::strict_php`: the front end and lowering run on the one
//!   compile thread, and codegen reads the copy the lowered module carries (`Module::closed_world`).
//! - In this mode a construct that would need the eval bridge either lowers to a native
//!   equivalent (a runtime include over the compiled-source table) or to a runtime error
//!   (`eval()` of a non-literal string in a dependency).
//! - [`Mode::Auto`] (the default) leaves the mode when the project's own code evaluates a computed
//!   string, and otherwise re-runs the compiler with the interpreter linked when the build still
//!   needs it, saying why; [`Mode::Strict`] fails the build with the reasons instead.

use std::cell::Cell;

thread_local! {
    static CLOSED_WORLD: Cell<bool> = const { Cell::new(false) };
}

/// Enables or disables the closed-world mode for the compilation running on this thread.
pub fn set_enabled(enabled: bool) {
    CLOSED_WORLD.with(|cell| cell.set(enabled));
}

/// Whether the compilation running on this thread links no interpreter.
pub fn is_enabled() -> bool {
    CLOSED_WORLD.with(|cell| cell.get())
}

/// Whether a closed-world build can never see class-like `name`: it is neither declared in the
/// program (`declared`) nor a builtin class contract. With no autoloader or interpreter to supply
/// it, any run-time use is php's `Error: Class "X" not found`, which a guarded optional
/// dependency (`if (class_exists(...)) { return X::m(...); }`) never reaches.
pub fn class_is_absent(name: &str, declared: impl Fn(&str) -> bool) -> bool {
    let name = name.trim_start_matches('\\');
    is_enabled()
        && !name.is_empty()
        && !declared(name)
        && elephc_builtin_contract::lookup_class(name).is_none()
}

/// How a compilation chooses between a closed world and a linked interpreter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Link the interpreter (`--with-eval`, the REPL, or the re-run [`Mode::Auto`] asks for).
    Off,
    /// The default: a closed world, unless the program needs the interpreter.
    Auto,
    /// A closed world, and a build that still needs the interpreter fails with the reasons.
    Strict,
}

/// Environment variable that overrides the mode: `0`/`off` links the interpreter, `1`/`on`/`strict`
/// asks for [`Mode::Strict`]. The [`Mode::Auto`] re-run sets it to `0`.
pub const MODE_ENV: &str = "ELEPHC_CLOSED_WORLD";

/// The mode `ELEPHC_CLOSED_WORLD` asks for; [`Mode::Auto`] when unset or unrecognised.
pub fn mode_from_environment() -> Mode {
    match std::env::var(MODE_ENV).as_deref() {
        Ok("0") | Ok("false") | Ok("off") => Mode::Off,
        Ok("1") | Ok("true") | Ok("on") | Ok("strict") => Mode::Strict,
        _ => Mode::Auto,
    }
}

/// Whether a source path belongs to a dependency (a `vendor` directory) rather than the project.
fn is_dependency_source(path: &str) -> bool {
    std::path::Path::new(path).components().any(|part| part.as_os_str() == "vendor")
}

/// The first `eval()` of a computed string in the project's own code, as `file:line`.
///
/// A dependency's computed eval (Symfony's VarExporter, a proxy generator) does not count: in a
/// closed world it raises php's `Error`, which those libraries never reach on the paths an
/// application runs. The project's own one does, so it keeps the interpreter.
pub fn project_computed_eval(
    program: &[crate::parser::ast::Stmt],
    preload_dirs: &[std::path::PathBuf],
) -> Option<String> {
    use crate::parser::ast::visit::{walk_nodes, Node};
    use crate::parser::ast::ExprKind;
    let mut found = None;
    walk_nodes(program, &mut |node| {
        if found.is_some() {
            return;
        }
        let Node::Expr(expr) = node else { return };
        let ExprKind::FunctionCall { name, args } = &expr.kind else { return };
        if crate::names::php_symbol_key(name.as_str().trim_start_matches('\\')) != "eval"
            || matches!(args.first().map(|arg| &arg.kind), Some(ExprKind::StringLiteral(_)))
        {
            return;
        }
        let path = expr.span.source_path();
        if path.as_deref().is_some_and(|path| is_dependency_source(path) || is_preloaded_source(path, preload_dirs)) {
            return;
        }
        found = Some(format!("{}:{}", path.unwrap_or_else(|| "<entry>".to_string()), expr.span.source_line()));
    });
    found
}

/// The first `include`/`require` of a computed path in the project's own code, as `file:line`.
///
/// An include runs its file in the caller's scope -- its variables, `$this`, the declarations it
/// makes -- so a closed world answers one only for the files it compiled as units of their own
/// (data files, the preload's, literal paths). A dependency's computed include (a framework's
/// cache or configuration loader) reaches those; the project's own one can reach any file, so it
/// keeps the interpreter, as a computed `eval()` does.
pub fn project_computed_include(
    program: &[crate::parser::ast::Stmt],
    preload_dirs: &[std::path::PathBuf],
) -> Option<String> {
    use crate::parser::ast::visit::{walk_nodes, Node};
    use crate::parser::ast::{ExprKind, StmtKind};
    let mut found = None;
    let mut consider = |path: &crate::parser::ast::Expr, span: crate::span::Span| {
        if found.is_some() || crate::resolver::include_path_is_static(path) {
            return;
        }
        let source = span.source_path();
        if source.as_deref().is_some_and(|path| is_dependency_source(path) || is_preloaded_source(path, preload_dirs)) {
            return;
        }
        found = Some(format!("{}:{}", source.unwrap_or_else(|| "<entry>".to_string()), span.source_line()));
    };
    walk_nodes(program, &mut |node| match node {
        Node::Expr(expr) => match &expr.kind {
            ExprKind::IncludeValue { path, .. } => consider(path, expr.span),
            ExprKind::FunctionCall { name, args }
                if name.as_str().trim_start_matches('\\') == crate::names::DYNAMIC_INCLUDE_FUNCTION =>
            {
                if let Some(path) = args.first() {
                    consider(path, expr.span);
                }
            }
            _ => {}
        },
        Node::Stmt(stmt) => {
            if let StmtKind::Include { path, .. } = &stmt.kind {
                consider(path, stmt.span);
            }
        }
    });
    found
}

/// Whether a site's source file lies under the directory of an `opcache.preload` script, whose
/// files the build compiles as the preload's closure
/// (`opcache_preload_sources::preload_directory_closure`).
///
/// What sits there is what the framework generated at deploy time (Symfony's dumped container
/// and its factories, which load each other with `require $this->containerDir.'/'.$file`), not
/// the application's own code, and the closed world answers those includes from the units it
/// compiled for that directory. Counted as the project's computed includes, they linked the
/// interpreter into every Symfony build.
fn is_preloaded_source(path: &str, preload_dirs: &[std::path::PathBuf]) -> bool {
    !preload_dirs.is_empty() && {
        let path = crate::source::canonical(path).unwrap_or_else(|_| path.into());
        preload_dirs.iter().any(|dir| path.starts_with(dir))
    }
}

/// Re-runs this compiler with the interpreter linked, after saying why; returns only when the
/// re-run could not start.
pub fn rerun_with_interpreter(reasons: &[String]) {
    crate::progress::clear();
    eprintln!(
        "note: the program needs the interpreter ({} reason{}); it is compiled with it linked:",
        reasons.len(),
        if reasons.len() == 1 { "" } else { "s" }
    );
    for reason in reasons {
        eprintln!("  - {reason}");
    }
    let Ok(executable) = std::env::current_exe() else {
        return;
    };
    let mut command = std::process::Command::new(executable);
    command.args(std::env::args_os().skip(1)).env(MODE_ENV, "0");
    // Replaces this process, as the PHPDoc erasure re-run does, so two compilations are never
    // resident at once.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let _ = command.exec();
    }
    if let Ok(status) = command.status() {
        std::process::exit(status.code().unwrap_or(1));
    }
}

/// The message a non-literal `eval()` raises in a closed-world program.
pub const EVAL_UNAVAILABLE_MESSAGE: &str = "eval() is not available in a closed-world build";

/// Builds the table a closed-world runtime include consults: one record per catalog source.
///
/// - a source the compiler already carries whole (autoloaded or preloaded declaration files)
///   is [`ClosedIncludeKind::Declarations`]: including it again runs nothing;
/// - a source with a compiled unit function runs that function ([`ClosedIncludeKind::Unit`]);
/// - every other source is part of the world but cannot be replayed from an include
///   ([`ClosedIncludeKind::NotCompiled`]); its `_once` guard still answers `true` after the
///   compiler's own inclusion ran it.
pub fn include_table(module: &crate::ir::Module) -> Vec<crate::ir::ClosedInclude> {
    use crate::ir::{ClosedInclude, ClosedIncludeKind};
    let Some(catalog) = module.source_catalog() else {
        return Vec::new();
    };
    let units: std::collections::HashMap<&std::path::Path, &str> = module
        .include_units
        .iter()
        .filter(|unit| {
            module
                .functions
                .iter()
                .any(|function| function.name.eq_ignore_ascii_case(&unit.function))
        })
        .map(|unit| (unit.canonical_path.as_path(), unit.function.as_str()))
        .collect();
    let mut table: Vec<ClosedInclude> = catalog
        .iter()
        .map(|(_, source)| {
            let kind = if let Some(function) = units.get(source.canonical_path.as_path()) {
                ClosedIncludeKind::Unit((*function).to_string())
            } else if module.preincluded_sources.contains(&source.canonical_path) {
                ClosedIncludeKind::Declarations
            } else {
                ClosedIncludeKind::NotCompiled
            };
            ClosedInclude { canonical_path: source.canonical_path.clone(), kind }
        })
        .collect();
    // A file left out of the program has no catalog entry, yet its unit answers its include: a
    // class file whose parent exists nowhere fails the way php's load of it does.
    let mut unlisted: Vec<(&std::path::Path, &str)> = units
        .iter()
        .filter(|(path, _)| !table.iter().any(|entry| entry.canonical_path == **path))
        .map(|(path, function)| (*path, *function))
        .collect();
    unlisted.sort();
    table.extend(unlisted.into_iter().map(|(path, function)| ClosedInclude {
        canonical_path: path.to_path_buf(),
        kind: ClosedIncludeKind::Unit(function.to_string()),
    }));
    table
}
