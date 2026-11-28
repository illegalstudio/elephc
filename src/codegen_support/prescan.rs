//! Purpose:
//! Scans the program for compile-time constants used by lowering and codegen.
//! Seeds the constant map with builtin and user-defined constant values.
//!
//! Called from:
//! - `crate::ir_lower::program` through `crate::codegen::collect_constants`.
//!
//! Key details:
//! - The scan must not evaluate PHP side effects; it only recognizes declarations and literal `define()` calls.
//! - The `--web` catch-all `try` around the top level is transparent to the scan.
//! - AST tail sinking keeps the statements `stmt_declares_scanned_constant` names singular, so
//!   an optimized top level still holds them.

use std::collections::HashMap;

use elephc_builtin_contract::{lookup_constant, ConstValue};

use crate::codegen_support::platform::{Platform, Target};
use crate::parser::ast::{Expr, ExprKind, Program, Stmt, StmtKind};
use crate::types::iconv_constants::{iconv_impl, ICONV_VERSION};
use crate::types::pcntl_constants::pcntl_int_constants;
use crate::types::predefined_constants::{literal_of, php_type_of, registered_constants};
use crate::types::PhpType;

/// Seeds the constant map with built-in PHP constants and user-defined constants.
///
/// Every builtin constant comes from the shared catalog (`elephc_builtin_contract::constants()`,
/// read through `types::predefined_constants`): fixed values are materialized as literals
/// straight from their contract, while the TARGET- AND PROFILE-DEPENDENT ones
/// (`ConstValue::TargetDependent`: `PHP_OS`, the `PHP_VERSION*` / `PHP_SAPI` version surface,
/// `DIRECTORY_SEPARATOR`, `E_ALL`, the platform-specific `FNM_*` flags,
/// `ICONV_IMPL` / `ICONV_VERSION`)
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
    target: Target,
) -> crate::types::GlobalConstants {
    let target_platform = target.platform;
    let mut constants = HashMap::new();
    for constant in registered_constants() {
        let value = elephc_builtin_contract::core_environment_constant(
            constant.name, target.platform == Platform::MacOS,
        ).unwrap_or(constant.value);
        if let Some(literal) = literal_of(value) {
            constants.insert(
                constant.name.to_string(),
                builtin_value(literal, php_type_of(value)),
            );
        }
    }
    // The runtime materializer replaces this metadata slot at every observable fetch.
    constants.insert("PHP_BINARY".to_string(), builtin_value(ExprKind::StringLiteral(String::new()), PhpType::Str));
    let php_version = crate::codegen_support::compile_php_version();
    let (fnm_noescape, fnm_pathname) = match target_platform {
        Platform::MacOS => (1, 2),
        Platform::Linux => (2, 1),
        Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
    };
    let str_const = |value: String| builtin_value(ExprKind::StringLiteral(value), PhpType::Str);
    let int_const = |value: i64| builtin_value(ExprKind::IntLiteral(value), PhpType::Int);
    let computed = [
        ("PHP_OS", str_const(target_platform.php_os_name().to_string())),
        ("PHP_OS_FAMILY", str_const(target_platform.php_os_family_name().to_string())),
        ("PHP_VERSION", str_const(php_version.version_string().to_string())),
        ("PHP_VERSION_ID", int_const(i64::from(php_version.version_id()))),
        ("PHP_MAJOR_VERSION", int_const(i64::from(php_version.major()))),
        ("PHP_MINOR_VERSION", int_const(i64::from(php_version.minor()))),
        ("PHP_RELEASE_VERSION", int_const(i64::from(php_version.release()))),
        ("PHP_EXTRA_VERSION", str_const(php_version.extra_version().to_string())),
        ("E_ALL", int_const(php_version.error_reporting_mask())),
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
        ("CODESET", int_const(match target_platform { Platform::MacOS => 0, Platform::Linux => 14, Platform::Windows => panic!("Windows target is not yet supported (see issue #379)") })),
        ("LC_ALL", int_const(i64::from(target_platform.lc_all()))),
        ("LC_COLLATE", int_const(match target_platform { Platform::MacOS => 1, Platform::Linux => 3, Platform::Windows => panic!("Windows target is not yet supported (see issue #379)") })),
        ("LC_CTYPE", int_const(i64::from(target_platform.lc_ctype()))),
        ("LC_MESSAGES", int_const(match target_platform { Platform::MacOS => 6, Platform::Linux => 5, Platform::Windows => panic!("Windows target is not yet supported (see issue #379)") })),
        ("LC_MONETARY", int_const(match target_platform { Platform::MacOS => 3, Platform::Linux => 4, Platform::Windows => panic!("Windows target is not yet supported (see issue #379)") })),
        ("LC_NUMERIC", int_const(match target_platform { Platform::MacOS => 4, Platform::Linux => 1, Platform::Windows => panic!("Windows target is not yet supported (see issue #379)") })),
        ("LC_TIME", int_const(match target_platform { Platform::MacOS => 5, Platform::Linux => 2, Platform::Windows => panic!("Windows target is not yet supported (see issue #379)") })),
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
    for (name, value) in pcntl_int_constants(target) {
        constants.insert(
            (*name).to_string(),
            builtin_value(ExprKind::IntLiteral(*value), PhpType::Int),
        );
    }
    collect_constant_decls(program, &mut constants);
    constants
}

/// Gives builtin values an explicit trusted producer instead of inheriting a calling parse scope.
fn builtin_value(kind: ExprKind, ty: PhpType) -> (Expr, PhpType) {
    let value = crate::source::with_parse_mode(crate::source::SourceProfile::new(crate::source::SourceMode::Internal),
        || Expr::new(kind, crate::span::Span::dummy()));
    (crate::types::constant_binding::bind_value(value, &PhpType::Mixed), ty)
}

/// Recursively scans statements for user-defined constant declarations.
///
/// Visits `const` declarations and `define()` function calls, inserting each
/// constant's name, expression, and inferred type into `constants`. Skips nested
/// functions/classes; only processes statement bodies at the top level and within
/// `IncludeOnceGuard`, synthetic bodies, or the `--web` catch-all `try`.
fn collect_constant_decls(
    stmts: &[Stmt],
    constants: &mut crate::types::GlobalConstants,
) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::ConstDecl { name, value } => {
                // A user constant whose initializer names an already-known constant keeps that
                // constant's value: `const X = MT_RAND_PHP;` then reads as the literal `1`
                // instead of re-evaluating the (deprecated) read at every use.
                //
                // Folding only feeds this READ map: the declaration statement still lowers its
                // original `ConstRef(MT_RAND_PHP)` and fires the one deprecation there, and an
                // alias chain (`const X = Y;` where `Y = MT_RAND_PHP`) inherits the folded
                // value without re-deprecating. Runtime-materialized observables such as
                // `PHP_BINARY` are excluded below because their value is not known yet.
                let resolved = match &value.kind {
                    // `PHP_BINARY` is only a placeholder until the runtime materializer
                    // resolves it per fetch; folding an alias to that placeholder would
                    // replace the executable path with an empty string.
                    ExprKind::ConstRef(reference)
                        if reference.as_str().trim_start_matches('\\') != "PHP_BINARY" =>
                    {
                        constants
                            .get(reference.as_str().trim_start_matches('\\'))
                            .map(|(resolved, ty)| {
                                alias_value(resolved.clone(), value, ty.clone())
                            })
                            .unwrap_or_else(|| {
                                (crate::types::constant_binding::bind_value(value.clone(), &PhpType::Mixed), constant_expr_type(&value.kind))
                            })
                    }
                    _ => (crate::types::constant_binding::bind_value(value.clone(), &PhpType::Mixed), constant_expr_type(&value.kind)),
                };
                constants.entry(name.clone()).or_insert(resolved);
            }
            StmtKind::ExprStmt(_) => {
                if let Some((const_name, value)) = literal_define(stmt) {
                    constants.entry(const_name.to_string()).or_insert((
                        crate::types::constant_binding::bind_value(value.clone(), &PhpType::Mixed),
                        constant_expr_type(&value.kind),
                    ));
                }
            }
            StmtKind::IncludeOnceGuard { body, .. } | StmtKind::Synthetic(body) => {
                collect_constant_decls(body, constants);
            }
            // `--web` runs the whole top level inside this catch-all, which must not hide its
            // `define()` calls: they stay top-level statements, exactly as without `--web`.
            StmtKind::Try { try_body, catches, .. }
                if catches.iter().any(|clause| crate::web_prelude::answers_uncaught(&clause.body)) =>
            {
                collect_constant_decls(try_body, constants);
            }
            _ => {}
        }
    }
}

/// Reuses the prepared value producer while reporting failures at the alias declaration.
///
/// The alias span is observable diagnostic context, but its source profile is not a new
/// evaluation policy. PHP stores the already evaluated constant value, so replaying a trusted
/// producer for an alias written in user source must retain that producer's semantics.
fn alias_value(resolved: Expr, alias: &Expr, ty: PhpType) -> (Expr, PhpType) {
    let mut resolved = crate::optimize::fold_constant_expression(resolved);
    resolved.span = alias.span;
    (resolved, ty)
}

/// Returns the name and value of a `define("NAME", value)` statement the scan registers.
fn literal_define(stmt: &Stmt) -> Option<(&str, &Expr)> {
    let StmtKind::ExprStmt(expr) = &stmt.kind else { return None };
    let ExprKind::FunctionCall { name, args } = &expr.kind else { return None };
    if name.as_str() != "define" || args.len() != 2 {
        return None;
    }
    let ExprKind::StringLiteral(const_name) = &args[0].kind else { return None };
    Some((const_name, &args[1]))
}

/// Returns whether `stmt` declares a constant the scan reads only where it stands.
///
/// A `const` declaration or a literal `define()` call is registered at the top level and in
/// the bodies the scan enters. A pass that moves one into an `if` branch, as AST tail sinking
/// would, hides it: reads in functions then lower without its value or type.
pub(crate) fn stmt_declares_scanned_constant(stmt: &Stmt) -> bool {
    matches!(stmt.kind, StmtKind::ConstDecl { .. }) || literal_define(stmt).is_some()
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
    fn int_constant(constants: &crate::types::GlobalConstants, name: &str) -> i64 {
        match &constants[name].0.kind {
            ExprKind::IntLiteral(value) => *value,
            _ => panic!("{name} is not an integer constant"),
        }
    }

    /// Verifies fnmatch constants follow target platform.
    #[test]
    fn test_fnmatch_constants_follow_target_platform() {
        use crate::codegen_support::platform::Arch;

        let mac = collect_constants(&vec![], Target::new(Platform::MacOS, Arch::AArch64));
        assert_eq!(int_constant(&mac, "FNM_NOESCAPE"), 1);
        assert_eq!(int_constant(&mac, "FNM_PATHNAME"), 2);
        assert_eq!(int_constant(&mac, "FNM_PERIOD"), 4);
        assert_eq!(int_constant(&mac, "FNM_CASEFOLD"), 16);

        let linux = collect_constants(&vec![], Target::new(Platform::Linux, Arch::AArch64));
        assert_eq!(int_constant(&linux, "FNM_NOESCAPE"), 2);
        assert_eq!(int_constant(&linux, "FNM_PATHNAME"), 1);
        assert_eq!(int_constant(&linux, "FNM_PERIOD"), 4);
        assert_eq!(int_constant(&linux, "FNM_CASEFOLD"), 16);
    }

    /// Verifies the prescanned `E_ALL` literal follows the selected PHP profile.
    #[test]
    fn test_error_all_follows_php_profile() {
        let default_profile = collect_constants(
            &vec![],
            Target::new(Platform::Linux, crate::codegen_support::platform::Arch::X86_64),
        );
        assert_eq!(int_constant(&default_profile, "E_ALL"), 30_719);
    }

    /// Verifies every catalogued constant the compiler registers gets a value here, on every
    /// target, with the type the catalog declares — including each `TargetDependent` one.
    #[test]
    fn every_target_dependent_constant_is_computed() {
        for platform in [Platform::MacOS, Platform::Linux] {
            let constants = collect_constants(
                &vec![],
                Target::new(platform, crate::codegen_support::platform::Arch::AArch64),
            );
            for constant in registered_constants() {
                let (_, ty) = constants
                    .get(constant.name)
                    .unwrap_or_else(|| panic!("{} has no value on {platform:?}", constant.name));
                assert_eq!(*ty, php_type_of(constant.value), "{} type", constant.name);
            }
        }
    }

    /// Verifies PCNTL constants are seeded with target-specific values and availability.
    #[test]
    fn test_pcntl_constants_follow_target_platform() {
        use crate::codegen_support::platform::{AppleVariant, Arch};

        let mac = collect_constants(&vec![], Target::new(Platform::MacOS, Arch::AArch64));
        assert_eq!(int_constant(&mac, "SIGCHLD"), 20);
        assert_eq!(int_constant(&mac, "PCNTL_EAGAIN"), 35);
        assert!(mac.contains_key("PRIO_DARWIN_BG"));
        assert!(!mac.contains_key("CLONE_NEWNS"));

        let linux = collect_constants(&vec![], Target::new(Platform::Linux, Arch::AArch64));
        assert_eq!(int_constant(&linux, "SIGCHLD"), 17);
        assert_eq!(int_constant(&linux, "PCNTL_EAGAIN"), 11);
        assert!(linux.contains_key("CLONE_NEWNS"));
        assert!(!linux.contains_key("PRIO_DARWIN_BG"));

        let ios = collect_constants(
            &vec![],
            Target::new_apple(Arch::AArch64, AppleVariant::IOS),
        );
        assert!(!ios.contains_key("SIGCHLD"));
        assert!(!ios.contains_key("PCNTL_EAGAIN"));
    }

    /// Verifies the `--web` catch-all keeps top-level `define()` calls visible to the scan,
    /// while an ordinary `try` still hides the ones it guards.
    #[test]
    fn test_web_catch_all_keeps_top_level_defines() {
        use crate::codegen_support::platform::Arch;
        use crate::synthetic_class::{e_call, e_str, s_expr};

        let define = |name: &str, value: &str| s_expr(e_call("define", vec![e_str(name), e_str(value)]));
        let target = Target::new(Platform::Linux, Arch::X86_64);
        let web = crate::web_prelude::wrap_resolved_catch_all(vec![define("WEB_PLUGIN_DIR", "/x/plugins")]);
        assert!(matches!(web.first().map(|stmt| &stmt.kind), Some(StmtKind::Try { .. })));
        let constants = collect_constants(&web, target);
        assert!(matches!(&constants["WEB_PLUGIN_DIR"], (Expr { kind: ExprKind::StringLiteral(value), .. }, PhpType::Str)
            if value == "/x/plugins"));

        let user_try = vec![Stmt::new(
            StmtKind::Try { try_body: vec![define("GUARDED_DIR", "/y")], catches: Vec::new(),
                finally_body: Some(Vec::new()) },
            crate::span::Span::dummy(),
        )];
        assert!(!collect_constants(&user_try, target).contains_key("GUARDED_DIR"));
    }
}
