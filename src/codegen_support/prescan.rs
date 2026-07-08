//! Purpose:
//! Scans the program for compile-time constants used by lowering and codegen.
//! Seeds the constant map with builtin and user-defined constant values.
//!
//! Called from:
//! - `crate::ir_lower::program` through `crate::codegen::collect_constants`.
//!
//! Key details:
//! - The scan must not evaluate PHP side effects; it only recognizes declarations and literal `define()` calls.

use std::collections::HashMap;

use crate::codegen_support::platform::Platform;
use crate::parser::ast::{ExprKind, Program, Stmt, StmtKind};
use crate::types::array_constants::ARRAY_INT_CONSTANTS;
use crate::types::date_constants::DATE_INT_CONSTANTS;
use crate::types::json_constants::JSON_INT_CONSTANTS;
use crate::types::stream_constants::STREAM_INT_CONSTANTS;
use crate::types::preg_constants::PREG_INT_CONSTANTS;
use crate::types::PhpType;

/// Seeds the constant map with built-in PHP constants and user-defined constants.
///
/// Built-in constants include platform-specific values (e.g., `FNM_*` flags differ
/// between macOS and Linux), `PATHINFO_*` bitmask values, stream handles (`STDIN`/`STDOUT`/`STDERR`),
/// `LOCK_*` values, array callback-mode constants, `JSON_*` integer constants, and
/// `PREG_*` integer constants. User constants come from `const` declarations and
/// `define()` calls discovered by `collect_constant_decls`.
pub(crate) fn collect_constants(
    program: &Program,
    target_platform: Platform,
) -> HashMap<String, (ExprKind, PhpType)> {
    let mut constants = HashMap::new();
    constants.insert(
        "PHP_OS".to_string(),
        (
            ExprKind::StringLiteral(target_platform.php_os_name().to_string()),
            PhpType::Str,
        ),
    );
    constants.insert(
        "PATHINFO_DIRNAME".to_string(),
        (ExprKind::IntLiteral(1), PhpType::Int),
    );
    constants.insert(
        "PATHINFO_BASENAME".to_string(),
        (ExprKind::IntLiteral(2), PhpType::Int),
    );
    constants.insert(
        "PATHINFO_EXTENSION".to_string(),
        (ExprKind::IntLiteral(4), PhpType::Int),
    );
    constants.insert(
        "PATHINFO_FILENAME".to_string(),
        (ExprKind::IntLiteral(8), PhpType::Int),
    );
    constants.insert(
        "PATHINFO_ALL".to_string(),
        (ExprKind::IntLiteral(15), PhpType::Int),
    );
    let (fnm_noescape, fnm_pathname) = match target_platform {
        Platform::MacOS => (1, 2),
        Platform::Linux => (2, 1),
        Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
    };
    constants.insert(
        "FNM_NOESCAPE".to_string(),
        (ExprKind::IntLiteral(fnm_noescape), PhpType::Int),
    );
    constants.insert(
        "FNM_PATHNAME".to_string(),
        (ExprKind::IntLiteral(fnm_pathname), PhpType::Int),
    );
    constants.insert(
        "FNM_PERIOD".to_string(),
        (ExprKind::IntLiteral(4), PhpType::Int),
    );
    constants.insert(
        "FNM_CASEFOLD".to_string(),
        (ExprKind::IntLiteral(16), PhpType::Int),
    );
    constants.insert(
        "STDIN".to_string(),
        (ExprKind::IntLiteral(0), PhpType::stream_resource()),
    );
    constants.insert(
        "STDOUT".to_string(),
        (ExprKind::IntLiteral(1), PhpType::stream_resource()),
    );
    constants.insert(
        "STDERR".to_string(),
        (ExprKind::IntLiteral(2), PhpType::stream_resource()),
    );
    constants.insert(
        "LOCK_SH".to_string(),
        (ExprKind::IntLiteral(1), PhpType::Int),
    );
    constants.insert(
        "LOCK_EX".to_string(),
        (ExprKind::IntLiteral(2), PhpType::Int),
    );
    constants.insert(
        "LOCK_UN".to_string(),
        (ExprKind::IntLiteral(3), PhpType::Int),
    );
    constants.insert(
        "LOCK_NB".to_string(),
        (ExprKind::IntLiteral(4), PhpType::Int),
    );
    for (name, value) in ARRAY_INT_CONSTANTS {
        constants.insert(
            (*name).to_string(),
            (ExprKind::IntLiteral(*value), PhpType::Int),
        );
    }
    for (name, value) in JSON_INT_CONSTANTS {
        constants.insert(
            (*name).to_string(),
            (ExprKind::IntLiteral(*value), PhpType::Int),
        );
    }
    for (name, value) in STREAM_INT_CONSTANTS {
        constants.insert(
            (*name).to_string(),
            (ExprKind::IntLiteral(*value), PhpType::Int),
        );
    }
    for (name, value) in PREG_INT_CONSTANTS {
        constants.insert(
            (*name).to_string(),
            (ExprKind::IntLiteral(*value), PhpType::Int),
        );
    }
    for (name, value) in DATE_INT_CONSTANTS {
        constants.insert(
            (*name).to_string(),
            (ExprKind::IntLiteral(*value), PhpType::Int),
        );
    }
    collect_constant_decls(program, &mut constants);
    constants
}

/// Recursively scans statements for user-defined constant declarations.
///
/// Visits `const` declarations and `define()` function calls, inserting each
/// constant's name, expression, and inferred type into `constants`. Skips nested
/// functions/classes; only processes statement bodies at the top level and within
/// `IncludeOnceGuard` or synthetic bodies.
fn collect_constant_decls(
    stmts: &[Stmt],
    constants: &mut HashMap<String, (ExprKind, PhpType)>,
) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::ConstDecl { name, value } => {
                constants
                    .entry(name.clone())
                    .or_insert((value.kind.clone(), constant_expr_type(&value.kind)));
            }
            StmtKind::ExprStmt(expr) => {
                if let ExprKind::FunctionCall { name, args } = &expr.kind {
                    if name.as_str() == "define" && args.len() == 2 {
                        if let ExprKind::StringLiteral(const_name) = &args[0].kind {
                            constants.entry(const_name.clone()).or_insert((
                                args[1].kind.clone(),
                                constant_expr_type(&args[1].kind),
                            ));
                        }
                    }
                }
            }
            StmtKind::IncludeOnceGuard { body, .. } | StmtKind::Synthetic(body) => {
                collect_constant_decls(body, constants);
            }
            _ => {}
        }
    }
}

/// Infers the `PhpType` for a constant expression from its `ExprKind` variant.
///
/// Returns `PhpType::Int` as a fallback for unsupported expression kinds.
/// Does not evaluate the expression; only maps literal variants to their types.
fn constant_expr_type(kind: &ExprKind) -> PhpType {
    match kind {
        ExprKind::IntLiteral(_) => PhpType::Int,
        ExprKind::FloatLiteral(_) => PhpType::Float,
        ExprKind::StringLiteral(_) => PhpType::Str,
        ExprKind::BoolLiteral(_) => PhpType::Bool,
        ExprKind::Null => PhpType::Void,
        _ => PhpType::Int,
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    /// Implements the `int_constant` operation for this module.
    fn int_constant(constants: &HashMap<String, (ExprKind, PhpType)>, name: &str) -> i64 {
        match &constants[name].0 {
            ExprKind::IntLiteral(value) => *value,
            _ => panic!("{name} is not an integer constant"),
        }
    }

    /// Verifies fnmatch constants follow target platform.
    #[test]
    fn test_fnmatch_constants_follow_target_platform() {
        let mac = collect_constants(&vec![], Platform::MacOS);
        assert_eq!(int_constant(&mac, "FNM_NOESCAPE"), 1);
        assert_eq!(int_constant(&mac, "FNM_PATHNAME"), 2);
        assert_eq!(int_constant(&mac, "FNM_PERIOD"), 4);
        assert_eq!(int_constant(&mac, "FNM_CASEFOLD"), 16);

        let linux = collect_constants(&vec![], Platform::Linux);
        assert_eq!(int_constant(&linux, "FNM_NOESCAPE"), 2);
        assert_eq!(int_constant(&linux, "FNM_PATHNAME"), 1);
        assert_eq!(int_constant(&linux, "FNM_PERIOD"), 4);
        assert_eq!(int_constant(&linux, "FNM_CASEFOLD"), 16);
    }
}
