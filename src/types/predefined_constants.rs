//! Purpose:
//! The compiler's single view over the shared global-constant catalog
//! (`elephc_builtin_contract::constants()`): which names are registered unconditionally,
//! their PHP types, and their literal values.
//!
//! Called from:
//! - `crate::types::checker::driver::init` when seeding the checker's constant types.
//! - `crate::codegen_support::prescan` when materializing constant literal values.
//! - `crate::name_resolver::names` when deciding whether a bare name is a builtin constant.
//! - Builtin checkers that fold a `JSON_*` / `ARRAY_*` flag from its name.
//!
//! Key details:
//! - There is no compiler-side constant table any more: adding a PHP constant means adding
//!   its contract (name, module, value) to the shared catalog, and every consumer above sees
//!   it. Target- and profile-dependent values (`PHP_OS`, `FNM_*`, `ICONV_IMPL`, ...) are
//!   `ConstValue::TargetDependent` in the catalog and computed by `prescan`, which a test
//!   pins against the catalog so no such name can be silently left without a value.

use elephc_builtin_contract::{
    constants, lookup_constant, ConstType, ConstValue, ConstantContract, ConstantRoute,
};

use crate::parser::ast::ExprKind;
use crate::types::PhpType;

/// Returns every constant the compiler registers for every program: the predefined ones and
/// the runtime-defined `Dynamic` ones (`SID`), in catalog order. Prelude-declared constants
/// (`MYSQLI_*`, `IMG_*`) are excluded: they arrive with their prelude's declarations.
pub(crate) fn registered_constants() -> impl Iterator<Item = &'static ConstantContract> {
    constants()
        .iter()
        .filter(|constant| is_registered_route(constant.route))
}

/// Returns whether `name` (case-sensitive, optional leading `\`) is a constant the compiler
/// registers for every program.
pub(crate) fn is_registered_constant(name: &str) -> bool {
    lookup_constant(name).is_some_and(|constant| is_registered_route(constant.route))
}

/// Returns the fixed integer value of a predefined constant, if `name` is one.
pub(crate) fn int_constant_value(name: &str) -> Option<i64> {
    match lookup_constant(name)?.value {
        ConstValue::Int(value) => Some(value),
        _ => None,
    }
}

/// Returns the checker type of a catalogued constant value.
pub(crate) fn php_type_of(value: ConstValue) -> PhpType {
    match value {
        ConstValue::StreamResource(_) => PhpType::stream_resource(),
        ConstValue::Null => PhpType::Void,
        other => match other.php_type() {
            ConstType::Int => PhpType::Int,
            ConstType::Float => PhpType::Float,
            ConstType::Str => PhpType::Str,
            ConstType::Bool => PhpType::Bool,
        },
    }
}

/// Returns the literal expression a fixed catalogued value folds to, or `None` for a
/// target-dependent value the caller must compute itself.
pub(crate) fn literal_of(value: ConstValue) -> Option<ExprKind> {
    Some(match value {
        ConstValue::Int(value) | ConstValue::StreamResource(value) => ExprKind::IntLiteral(value),
        ConstValue::Float(value) => ExprKind::FloatLiteral(value),
        ConstValue::Str(value) => ExprKind::StringLiteral(value.to_string()),
        ConstValue::Bool(value) => ExprKind::BoolLiteral(value),
        ConstValue::Null => ExprKind::Null,
        ConstValue::TargetDependent(_) => return None,
    })
}

fn is_registered_route(route: ConstantRoute) -> bool {
    matches!(route, ConstantRoute::Predefined | ConstantRoute::Dynamic)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies representative names resolve with the values PHP documents.
    #[test]
    fn catalog_values_match_php() {
        assert_eq!(int_constant_value("JSON_PRETTY_PRINT"), Some(128));
        assert_eq!(int_constant_value("ARRAY_FILTER_USE_KEY"), Some(2));
        assert_eq!(int_constant_value("E_ALL"), Some(32767));
        assert_eq!(int_constant_value("CURLOPT_URL"), Some(10002));
        assert!(is_registered_constant("\\PHP_EOL"));
        assert!(is_registered_constant("SID"));
        assert!(!is_registered_constant("MYSQLI_ASSOC"), "prelude constants are not predefined");
        assert!(!is_registered_constant("json_pretty_print"), "constants are case-sensitive");
        assert_eq!(php_type_of(ConstValue::StreamResource(0)), PhpType::stream_resource());
    }
}
