//! The PHP CLI session extension surface, backed by the same session implementation as `--web`.
//!
//! Called from `crate::pipeline::compile` after include/autoload expansion so the CLI program
//! and its dynamically loaded declarations see the standard `session_*` functions.
//!
//! Key details:
//! - This is the PHP session implementation built by `web_prelude::build`, not a synthetic
//!   success stub. Both SAPIs share the PHP-level implementation and state/file-I/O bridge.
//! - Only session declarations are injected here; HTTP request initialization and the web entry
//!   point remain exclusive to `--web`.

use std::path::Path;

use crate::optimize::reachability::PreludeInventory;
use crate::parser::ast::Program;
use crate::web_prelude::PhpVersion;

/// Reachability group retained because the PHP session extension is present in the CLI profile,
/// even when session calls are made only from source the compiler loads dynamically.
pub const SESSION_PRELUDE_GROUP: &str = "session";

/// Injects the standard session surface into the CLI profile; `--web` already owns it.
pub fn inject_cli_session_surface(
    program: Program,
    web: bool,
    php_version: PhpVersion,
    ini_overrides: &[(String, String)],
    entry_path: &Path,
    inventory: &mut PreludeInventory,
    autoloaded_save_handler: bool,
) -> Program {
    if web {
        return program;
    }

    let mut declarations =
        crate::web_prelude::build::session_declarations(php_version, ini_overrides);
    // The same policy `--web` applies: the callable save-handler adapter exists for
    // `session_set_save_handler()`, and its `call_user_func()` dispatch is a dynamic-call hazard
    // that, with this group forced, kept every declared function and method of a CLI program
    // alive (`<?php echo 'ok';` lowered 183 functions). Source the compiler cannot see -- eval,
    // a runtime include, a dynamic call -- may still register a handler, so those keep it.
    let user_usage = user_source_usage(&program, inventory);
    let needs_callable_session_handler = autoloaded_save_handler
        || user_usage.references("session_set_save_handler")
        || unseen_source_may_call(&user_usage);
    if !needs_callable_session_handler {
        declarations.retain(|stmt| !crate::web_prelude::is_callable_session_handler_decl(&stmt.kind));
    }
    // A program nothing of which can reach the session surface does not get it at all. Its bodies
    // are pruned whole when unreached, but until then every pass that decides what the program
    // uses reads them: `session_decode()` calls `unserialize()`, which registered the SPL and
    // date/time builtin classes (some ninety class thunks) in `<?php echo 1;`, and `max()` in the
    // cookie code declared a forced backend helper and the throwable machinery behind it.
    if !autoloaded_save_handler
        && !unseen_source_may_call(&user_usage)
        && !names_a_declaration(&user_usage, &declarations) {
        return program;
    }
    let declarations = crate::magic_constants::substitute_file_constants(declarations, entry_path);
    inventory.record_program(SESSION_PRELUDE_GROUP, &declarations);

    let mut combined = declarations;
    combined.extend(program);
    combined
}

/// Returns whether code the compiler cannot see -- eval, a runtime include, a dynamic call --
/// may call into the session surface, which is when the whole group must stay compiled.
///
/// A program with none of them reaches only the session functions it names, and declaration
/// reachability keeps exactly those; forcing the group regardless compiled ~150 session
/// functions into `<?php echo 'ok';`.
pub fn unseen_source_may_call_sessions(
    program: &Program,
    inventory: &PreludeInventory,
) -> bool {
    unseen_source_may_call(&user_source_usage(program, inventory))
}

/// Collects usage over the user's own statements only.
///
/// Earlier preludes (PDO, curl, ...) are already spliced into `program` by now, and their bodies
/// legitimately contain dynamic calls. Scanning them made every program that merely named
/// `PDO` look like it reached unseen source, which forced the whole session group and kept
/// every declaration of the binary alive.
pub(crate) fn user_source_usage(
    program: &Program,
    inventory: &PreludeInventory,
) -> crate::prelude_prune::usage::Usage {
    let user: Program = program
        .iter()
        .filter(|stmt| !inventory.owns_top_level_declaration(stmt))
        .cloned()
        .collect();
    crate::prelude_prune::usage::collect(&user)
}

/// Returns whether `program` (the autoload closure) names `session_set_save_handler`, so the
/// callable save-handler adapter must survive even though the entry file never references it.
pub(crate) fn program_registers_save_handler(program: &[crate::parser::ast::Stmt]) -> bool {
    crate::prelude_prune::usage::collect(program).references("session_set_save_handler")
}

/// The shared test behind [`unseen_source_may_call_sessions`].
///
/// An `eval('…')` of a string literal is not unseen: the usage walk reads its fragment like any
/// other source, so the session functions it names are kept by ordinary reachability. Treating
/// every `eval` as unseen forced the whole session group into `<?php eval('return 1;');`.
fn unseen_source_may_call(usage: &crate::prelude_prune::usage::Usage) -> bool {
    usage.opaque_eval || usage.reaches_unseen_callable() || usage.includes_runtime_php
}

/// Whether the user's own source names a function, class or interface that `declarations` declares,
/// by call, by `new`/type/`extends`, or by a literal a probe or callback spells.
fn names_a_declaration(
    usage: &crate::prelude_prune::usage::Usage,
    declarations: &[crate::parser::ast::Stmt],
) -> bool {
    use crate::parser::ast::StmtKind;
    declarations.iter().any(|stmt| match &stmt.kind {
        StmtKind::FunctionDecl { name, .. } => {
            let key = crate::names::php_symbol_key(name.trim_start_matches('\\'));
            usage.functions.contains(&key) || usage.literals.contains(&key)
        }
        StmtKind::ClassDecl { name, .. } | StmtKind::InterfaceDecl { name, .. } => {
            let key = crate::names::php_symbol_key(name.trim_start_matches('\\'));
            usage.classes.contains(&key) || usage.literals.contains(&key)
        }
        _ => false,
    })
}
