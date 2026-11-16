//! Purpose:
//! Injects PHP builtin enum metadata into the checker.
//! Provides always-available enum declarations introduced by newer PHP versions.
//!
//! Called from:
//! - `crate::types::checker::driver::check_types_impl()`
//! - `crate::types::checker::type_compat::pointers` (`is_builtin_enum_name`, for `ptr<T>` targets
//!   resolved before the builtin enums are injected)
//!
//! Key details:
//! - Builtin enum names share PHP's class-like namespace and reject userland redeclarations.
//! - `Pcntl\QosClass` is injected only for macOS targets, matching php-src's platform guard.

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{Program, Stmt, StmtKind};
use crate::types::{EnumCaseInfo, EnumCaseValue, PhpType};

use super::schema::insert_enum_metadata;
use super::Checker;

const SORT_DIRECTION: &str = "SortDirection";
const PROPERTY_HOOK_TYPE: &str = "PropertyHookType";
const PCNTL_QOS_CLASS: &str = "Pcntl\\QosClass";

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
                attribute_names: Vec::new(),
                attribute_args: Vec::new(),
            },
            EnumCaseInfo {
                name: "Descending".to_string(),
                value: None,
                attribute_names: Vec::new(),
                attribute_args: Vec::new(),
            },
        ],
        &[],
        &[],
        &[],
        &[],
        &[],
        crate::span::Span::dummy(),
        true,
        checker,
        next_class_id,
    )?;

    ensure_builtin_enum_name_available(program, checker, PROPERTY_HOOK_TYPE)?;
    insert_enum_metadata(
        PROPERTY_HOOK_TYPE,
        Some(PhpType::Str),
        vec![
            EnumCaseInfo {
                name: "Get".to_string(),
                value: Some(EnumCaseValue::Str("get".to_string())),
                attribute_names: Vec::new(),
                attribute_args: Vec::new(),
            },
            EnumCaseInfo {
                name: "Set".to_string(),
                value: Some(EnumCaseValue::Str("set".to_string())),
                attribute_names: Vec::new(),
                attribute_args: Vec::new(),
            },
        ],
        &[],
        &[],
        &[],
        &[],
        &[],
        crate::span::Span::dummy(),
        true,
        checker,
        next_class_id,
    )?;

    if pcntl_qos_class_is_builtin(checker) {
        ensure_builtin_enum_name_available(program, checker, PCNTL_QOS_CLASS)?;
        insert_enum_metadata(
            PCNTL_QOS_CLASS,
            None,
            [
                "UserInteractive",
                "UserInitiated",
                "Default",
                "Utility",
                "Background",
            ]
            .into_iter()
            .map(|name| EnumCaseInfo {
                name: name.to_string(),
                value: None,
                attribute_names: Vec::new(),
                attribute_args: Vec::new(),
            })
            .collect(),
            &[],
            &[],
            &[],
            &[],
            &[],
            crate::span::Span::dummy(),
            true,
            checker,
            next_class_id,
        )?;
    }

    Ok(())
}

/// Returns whether `name` is a builtin enum that `inject_builtin_enums` registers for the
/// checker's target.
///
/// The packed and extern prescan resolves `ptr<T>` targets before the builtin enums are
/// injected, so it consults this list where it would otherwise find the enum in `classes`. The
/// match is exact, like that `classes` lookup. The names are deliberately not pre-declared in
/// `declared_classes`: the prescan's duplicate check reads that set, and a user packed or extern
/// class named like a builtin enum must keep the builtin redeclaration diagnostic.
pub(crate) fn is_builtin_enum_name(checker: &Checker, name: &str) -> bool {
    name == SORT_DIRECTION
        || name == PROPERTY_HOOK_TYPE
        || (name == PCNTL_QOS_CLASS && pcntl_qos_class_is_builtin(checker))
}

/// Returns whether the target registers `Pcntl\QosClass`, matching php-src's macOS-only guard.
fn pcntl_qos_class_is_builtin(checker: &Checker) -> bool {
    checker.target.platform == crate::codegen::platform::Platform::MacOS
        && !checker.target.is_ios()
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
