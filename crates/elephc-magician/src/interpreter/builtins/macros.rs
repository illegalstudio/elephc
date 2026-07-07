//! Purpose:
//! Declarative helpers for registering eval-side PHP builtins.
//! The macro keeps per-builtin files compact while preserving the interpreter
//! registry as the runtime lookup surface.
//!
//! Called from:
//! - `crate::interpreter::builtins::<area>::<builtin>` home files.
//!
//! Key details:
//! - Macro expansion submits static metadata to `inventory`.
//! - Dispatch hooks are magician-specific enums so handlers can stay generic
//!   over `RuntimeValueOps`.

macro_rules! eval_builtin {
    (
        name: $name:literal,
        area: $area:ident,
        params: [$($param:ident $(= $default:expr)?),* $(,)?],
        direct: $direct:ident,
        values: $values:ident $(,)?
    ) => {
        inventory::submit! {
            $crate::interpreter::builtins::spec::EvalBuiltinSpec {
                name: $name,
                area: $crate::interpreter::builtins::spec::EvalArea::$area,
                param_names: &[$(stringify!($param)),*],
                params: &[
                    $(
                        $crate::interpreter::builtins::spec::EvalParamSpec {
                            name: stringify!($param),
                            default: eval_builtin!(@default $($default)?),
                            by_ref: false,
                        },
                    )*
                ],
                variadic: None,
                by_ref_params: &[],
                direct: Some($crate::interpreter::builtins::spec::EvalDirectHook::$direct),
                values: Some($crate::interpreter::builtins::spec::EvalValuesHook::$values),
            }
        }
    };

    (@default) => {
        None
    };

    (@default $default:expr) => {
        Some($default)
    };
}
