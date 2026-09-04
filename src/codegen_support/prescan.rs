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

use elephc_builtin_contract::{lookup_constant, ConstValue};

use crate::codegen_support::platform::Platform;
use crate::parser::ast::{ExprKind, Program, Stmt, StmtKind};
use crate::types::iconv_constants::{iconv_impl, ICONV_VERSION};
use crate::types::predefined_constants::{literal_of, php_type_of, registered_constants};
use crate::types::PhpType;

/// Seeds the constant map with built-in PHP constants and user-defined constants.
///
/// Every builtin constant comes from the shared catalog (`elephc_builtin_contract::constants()`,
/// read through `types::predefined_constants`): fixed values are materialized as literals
/// straight from their contract, while the TARGET- AND PROFILE-DEPENDENT ones
/// (`ConstValue::TargetDependent`: `PHP_OS`, the `PHP_VERSION*` / `PHP_SAPI` version surface,
/// `DIRECTORY_SEPARATOR`, the platform-specific `FNM_*` flags, `ICONV_IMPL` / `ICONV_VERSION`)
/// are computed here under their catalogued names. The version surface reads the compilation's
/// `--php-version` profile and `--web` mode from the codegen thread-local pair
/// (`compile_php_version` / `compile_is_web_sapi`) rather than from a parameter, because this
/// function sits under `ir_lower::lower` which does not carry the profile. See
/// `web_prelude::PhpVersion::version_string` for the version rule and `web_prelude::sapi_name`
/// for the SAPI mapping. `every_target_dependent_constant_is_computed` pins this function
/// against the catalog so no such name can be left without a value.
///
/// User constants come from `const` declarations and `define()` calls discovered by
/// `collect_constant_decls`; a builtin name always wins over a user declaration of it.
pub(crate) fn collect_constants(
    program: &Program,
    target_platform: Platform,
) -> HashMap<String, (ExprKind, PhpType)> {
    let mut constants = HashMap::new();
    for constant in registered_constants() {
        if let Some(literal) = literal_of(constant.value) {
            constants.insert(
                constant.name.to_string(),
                (literal, php_type_of(constant.value)),
            );
        }
    }

    let php_version = crate::codegen_support::compile_php_version();
    let (fnm_noescape, fnm_pathname) = match target_platform {
        Platform::MacOS => (1, 2),
        Platform::Linux => (2, 1),
        Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
    };
    let str_const = |value: String| (ExprKind::StringLiteral(value), PhpType::Str);
    let int_const = |value: i64| (ExprKind::IntLiteral(value), PhpType::Int);
    let computed = [
        ("PHP_OS", str_const(target_platform.php_os_name().to_string())),
        ("PHP_VERSION", str_const(php_version.version_string().to_string())),
        ("PHP_VERSION_ID", int_const(i64::from(php_version.version_id()))),
        ("PHP_MAJOR_VERSION", int_const(i64::from(php_version.major()))),
        ("PHP_MINOR_VERSION", int_const(i64::from(php_version.minor()))),
        ("PHP_RELEASE_VERSION", int_const(i64::from(php_version.release()))),
        ("PHP_EXTRA_VERSION", str_const(php_version.extra_version().to_string())),
        (
            "PHP_SAPI",
            str_const(
                crate::web_prelude::sapi_name(crate::codegen_support::compile_is_web_sapi())
                    .to_string(),
            ),
        ),
        ("DIRECTORY_SEPARATOR", str_const(std::path::MAIN_SEPARATOR.to_string())),
        ("FNM_NOESCAPE", int_const(fnm_noescape)),
        ("FNM_PATHNAME", int_const(fnm_pathname)),
        ("ICONV_IMPL", str_const(iconv_impl(target_platform == Platform::MacOS).to_string())),
        ("ICONV_VERSION", str_const(ICONV_VERSION.to_string())),
    ];
    for (name, value) in computed {
        debug_assert!(
            matches!(
                lookup_constant(name).map(|constant| constant.value),
                Some(ConstValue::TargetDependent(_))
            ),
            "{name} is computed by prescan but the catalog does not mark it TargetDependent"
        );
        constants.insert(name.to_string(), value);
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

    /// Verifies every catalogued constant the compiler registers gets a value here, on every
    /// target, with the type the catalog declares — including each `TargetDependent` one.
    #[test]
    fn every_target_dependent_constant_is_computed() {
        for platform in [Platform::MacOS, Platform::Linux] {
            let constants = collect_constants(&vec![], platform);
            for constant in registered_constants() {
                let (_, ty) = constants
                    .get(constant.name)
                    .unwrap_or_else(|| panic!("{} has no value on {platform:?}", constant.name));
                assert_eq!(*ty, php_type_of(constant.value), "{} type", constant.name);
            }
        }
    }
}
