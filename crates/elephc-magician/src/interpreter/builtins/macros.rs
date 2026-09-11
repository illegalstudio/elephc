//! Purpose:
//! Declarative helpers for joining Magician dispatch hooks to shared builtin
//! contracts without repeating PHP-visible catalog metadata.
//!
//! Called from:
//! - `crate::interpreter::builtins::<area>::<builtin>` home files.
//!
//! Key details:
//! - Macro expansion submits only a stable shared-contract ID and eval hooks.
//! - Dispatch hooks remain Magician-specific enums so handlers stay generic
//!   over `RuntimeValueOps`.

macro_rules! eval_builtin {
    (
        contract: $contract:literal,
        area: $area:ident,
        $(source_arguments: $source_arguments:tt,)?
        direct: $direct:tt,
        values: $values:tt $(,)?
    ) => {
        inventory::submit! {
            $crate::interpreter::builtins::spec::EvalBuiltinBinding {
                id: elephc_builtin_contract::BuiltinId::from_canonical_name($contract),
                area: $crate::interpreter::builtins::spec::EvalArea::$area,
                source_arguments: eval_builtin!(@source_arguments $($source_arguments)?),
                direct: eval_builtin!(@direct $direct),
                values: eval_builtin!(@values $values),
                home_file: file!(),
            }
        }
    };

    (@source_arguments true) => {
        true
    };

    (@source_arguments) => {
        false
    };

    (@direct none) => {
        None
    };

    (@direct $direct:ident) => {
        Some($crate::interpreter::builtins::spec::EvalDirectHook::$direct)
    };

    (@values none) => {
        None
    };

    (@values $values:ident) => {
        Some($crate::interpreter::builtins::spec::EvalValuesHook::$values)
    };
}
