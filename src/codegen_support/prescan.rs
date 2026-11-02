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

use crate::codegen_support::platform::{Platform, Target};
use crate::parser::ast::{ExprKind, Program, Stmt, StmtKind};
use crate::types::iconv_constants::{iconv_impl, ICONV_VERSION};
use crate::types::pcntl_constants::pcntl_int_constants;
use crate::types::predefined_constants::{
    constant_defined_on_target, literal_of, php_type_of, registered_constants,
};
use crate::types::token_constants::{token_int_constant_value, TOKEN_INT_CONSTANTS};
use crate::types::PhpType;

/// Seeds the constant map with built-in PHP constants and user-defined constants.
///
/// Every builtin constant comes from the shared catalog (`elephc_builtin_contract::constants()`,
/// read through `types::predefined_constants`): fixed values are materialized as literals
/// straight from their contract, while the TARGET- AND PROFILE-DEPENDENT ones
/// (`ConstValue::TargetDependent`: `PHP_OS`, the `PHP_VERSION*` / `PHP_SAPI` version surface,
/// `DIRECTORY_SEPARATOR`, `E_ALL`, the platform-specific `FNM_*` / `LC_*` families, `PHP_MAXPATHLEN`,
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
) -> HashMap<String, (ExprKind, PhpType)> {
    let target_platform = target.platform;
    let mut constants = HashMap::new();
    for constant in registered_constants() {
        // A name php does not define on this target gets no value: `defined()` then answers
        // false, and a read throws `Undefined constant` (see `constant_defined_on_target`).
        if !constant_defined_on_target(constant, target) {
            continue;
        }
        let value = elephc_builtin_contract::core_environment_constant(
            constant.name, target.platform == Platform::MacOS,
        ).unwrap_or(constant.value);
        if let Some(literal) = literal_of(value) {
            constants.insert(
                constant.name.to_string(),
                (literal, php_type_of(value)),
            );
        }
    }
    // The runtime materializer replaces this metadata slot at every observable fetch.
    constants.insert("PHP_BINARY".to_string(), (ExprKind::StringLiteral(String::new()), PhpType::Str));
    let php_version = crate::codegen_support::compile_php_version();
    for (name, _values) in TOKEN_INT_CONSTANTS {
        let value = token_int_constant_value(name, php_version)
            .expect("registered tokenizer constant must have a profile value");
        constants.insert(
            (*name).to_string(),
            (ExprKind::IntLiteral(value), PhpType::Int),
        );
    }
    let (fnm_noescape, fnm_pathname) = match target_platform {
        Platform::MacOS => (1, 2),
        Platform::Linux => (2, 1),
        Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
    };
    let str_const = |value: String| (ExprKind::StringLiteral(value), PhpType::Str);
    let int_const = |value: i64| (ExprKind::IntLiteral(value), PhpType::Int);
    // `setlocale()`'s categories are plain C-library enumerators, and the two libcs number them
    // differently: Darwin orders them alphabetically from `LC_ALL = 0`, glibc puts `LC_CTYPE`
    // first and `LC_ALL` last. A program that passes `LC_NUMERIC` to `setlocale()` would select
    // `LC_TIME` on Linux if the value were baked from the host's headers, so the whole family is
    // target-dependent. `Platform::lc_ctype` answers for the one category the regex helpers
    // select themselves, and agrees with the value baked here.
    let per_platform = |macos: i64, linux: i64| {
        int_const(match target_platform {
            Platform::MacOS => macos,
            Platform::Linux => linux,
            Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
        })
    };
    let computed = [
        ("PHP_OS", str_const(target_platform.php_os_name().to_string())),
        ("PHP_OS_FAMILY", str_const(target_platform.php_os_family_name().to_string())),
        ("PHP_VERSION", str_const(php_version.reported_version_string())),
        ("PHP_VERSION_ID", int_const(i64::from(php_version.reported_version_id()))),
        ("PHP_MAJOR_VERSION", int_const(i64::from(php_version.major()))),
        ("PHP_MINOR_VERSION", int_const(i64::from(php_version.minor()))),
        ("PHP_RELEASE_VERSION", int_const(i64::from(php_version.reported_release()))),
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
        // The `include_path` list separator: `;` where the directory separator is `\\`.
        (
            "PATH_SEPARATOR",
            str_const(if std::path::MAIN_SEPARATOR == '\\' { ";" } else { ":" }.to_string()),
        ),
        ("FNM_NOESCAPE", int_const(fnm_noescape)),
        ("FNM_PATHNAME", int_const(fnm_pathname)),
        ("ICONV_IMPL", str_const(iconv_impl(target_platform == Platform::MacOS).to_string())),
        ("ICONV_VERSION", str_const(ICONV_VERSION.to_string())),
        ("LC_ALL", per_platform(0, 6)),
        ("LC_COLLATE", per_platform(1, 3)),
        ("LC_CTYPE", per_platform(2, 0)),
        ("LC_MESSAGES", per_platform(6, 5)),
        ("LC_MONETARY", per_platform(3, 4)),
        ("LC_NUMERIC", per_platform(4, 1)),
        ("LC_TIME", per_platform(5, 2)),
        // php-src exposes `MAXPATHLEN` from the target's own headers: `PATH_MAX` is 1024 on
        // Darwin and 4096 on Linux.
        ("PHP_MAXPATHLEN", per_platform(1024, 4096)),
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

    /// Verifies the locale-category and path-length constants follow the target's own C library.
    ///
    /// Darwin numbers `setlocale()`'s categories alphabetically from `LC_ALL = 0`; glibc puts
    /// `LC_CTYPE` first and `LC_ALL` last, so a program passing `LC_NUMERIC` would select
    /// `LC_TIME` on Linux if one table were baked for both. `php -n` 8.5.10 on macOS prints the
    /// Darwin column; `PATH_MAX` is 1024 there and 4096 on Linux.
    #[test]
    fn test_locale_and_path_constants_follow_target_platform() {
        use crate::codegen_support::platform::Arch;

        let mac = collect_constants(&vec![], Target::new(Platform::MacOS, Arch::AArch64));
        assert_eq!(int_constant(&mac, "LC_ALL"), 0);
        assert_eq!(int_constant(&mac, "LC_COLLATE"), 1);
        assert_eq!(int_constant(&mac, "LC_CTYPE"), 2);
        assert_eq!(int_constant(&mac, "LC_MONETARY"), 3);
        assert_eq!(int_constant(&mac, "LC_NUMERIC"), 4);
        assert_eq!(int_constant(&mac, "LC_TIME"), 5);
        assert_eq!(int_constant(&mac, "LC_MESSAGES"), 6);
        assert_eq!(int_constant(&mac, "PHP_MAXPATHLEN"), 1024);

        let linux = collect_constants(&vec![], Target::new(Platform::Linux, Arch::AArch64));
        assert_eq!(int_constant(&linux, "LC_CTYPE"), 0);
        assert_eq!(int_constant(&linux, "LC_NUMERIC"), 1);
        assert_eq!(int_constant(&linux, "LC_TIME"), 2);
        assert_eq!(int_constant(&linux, "LC_COLLATE"), 3);
        assert_eq!(int_constant(&linux, "LC_MONETARY"), 4);
        assert_eq!(int_constant(&linux, "LC_MESSAGES"), 5);
        assert_eq!(int_constant(&linux, "LC_ALL"), 6);
        assert_eq!(int_constant(&linux, "PHP_MAXPATHLEN"), 4096);

        // The runtime's own `setlocale()` call sites read the same category from `Platform`.
        assert_eq!(i64::from(Platform::MacOS.lc_ctype()), int_constant(&mac, "LC_CTYPE"));
        assert_eq!(i64::from(Platform::Linux.lc_ctype()), int_constant(&linux, "LC_CTYPE"));
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
            let target = Target::new(platform, crate::codegen_support::platform::Arch::AArch64);
            for constant in registered_constants() {
                if !constant_defined_on_target(constant, target) {
                    assert!(
                        !constants.contains_key(constant.name),
                        "{} is not defined on {platform:?} but has a value",
                        constant.name
                    );
                    continue;
                }
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
}
