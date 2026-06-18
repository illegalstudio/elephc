//! Purpose:
//! Injects PHP builtin enum metadata into the checker.
//! Provides always-available enum declarations introduced by newer PHP versions.
//!
//! Called from:
//! - `crate::types::checker::driver::check_types_impl()`
//!
//! Key details:
//! - Builtin enum names share PHP's class-like namespace and reject userland redeclarations.

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{Program, Stmt, StmtKind};
use crate::types::EnumCaseInfo;

use super::schema::insert_enum_metadata;
use super::Checker;

const SORT_DIRECTION: &str = "SortDirection";

/// Injects all builtin enum declarations into the checker.
///
/// Returns a redeclaration diagnostic if user code declares a class-like symbol
/// with the same PHP case-insensitive name as a builtin enum.
pub(crate) fn inject_builtin_enums(
    program: &Program,
    checker: &mut Checker,
    next_class_id: &mut u64,
) -> Result<(), CompileError> {
    ensure_builtin_enum_name_available(program, checker, SORT_DIRECTION)?;
    insert_enum_metadata(
        SORT_DIRECTION,
        None,
        vec![
            EnumCaseInfo {
                name: "Ascending".to_string(),
                value: None,
            },
            EnumCaseInfo {
                name: "Descending".to_string(),
                value: None,
            },
        ],
        &[],
        &[],
        &[],
        checker,
        next_class_id,
    )
}

/// Ensures a builtin enum name does not collide with any user-visible class-like symbol.
fn ensure_builtin_enum_name_available(
    program: &Program,
    checker: &Checker,
    builtin_name: &str,
) -> Result<(), CompileError> {
    let builtin_key = php_symbol_key(builtin_name);
    let checker_collision = checker
        .classes
        .keys()
        .chain(checker.interfaces.keys())
        .chain(checker.enums.keys())
        .any(|name| php_symbol_key(name) == builtin_key);
    if checker_collision || program_declares_class_like(program, &builtin_key) {
        return Err(CompileError::new(
            crate::span::Span::dummy(),
            &format!("Cannot redeclare built-in type: {}", builtin_name),
        ));
    }
    Ok(())
}

/// Returns true when the program declares a class-like symbol matching `builtin_key`.
fn program_declares_class_like(program: &Program, builtin_key: &str) -> bool {
    program
        .iter()
        .any(|stmt| stmt_declares_class_like(stmt, builtin_key))
}

/// Recursively checks statement forms that can contain class-like declarations.
fn stmt_declares_class_like(stmt: &Stmt, builtin_key: &str) -> bool {
    match &stmt.kind {
        StmtKind::ClassDecl { name, .. }
        | StmtKind::EnumDecl { name, .. }
        | StmtKind::PackedClassDecl { name, .. }
        | StmtKind::InterfaceDecl { name, .. }
        | StmtKind::TraitDecl { name, .. }
        | StmtKind::ExternClassDecl { name, .. } => php_symbol_key(name) == builtin_key,
        StmtKind::NamespaceBlock { body, .. }
        | StmtKind::Synthetic(body)
        | StmtKind::IncludeOnceGuard { body, .. } => program_declares_class_like(body, builtin_key),
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            program_declares_class_like(then_body, builtin_key)
                || else_body
                    .as_ref()
                    .is_some_and(|body| program_declares_class_like(body, builtin_key))
        }
        _ => false,
    }
}
