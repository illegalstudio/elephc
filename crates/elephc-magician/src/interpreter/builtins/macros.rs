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
        params: [$($param:ident $(: $mode:ident)? $(= $default:expr)?),* $(,)?],
        by_ref: [$($by_ref:ident),* $(,)?],
        direct: none,
        values: $values:ident $(,)?
    ) => {
        inventory::submit! {
            $crate::interpreter::builtins::spec::EvalBuiltinSpec {
                name: $name,
                area: $crate::interpreter::builtins::spec::EvalArea::$area,
                param_names: &[$(eval_builtin!(@name_str $param)),*],
                params: &[
                    $(
                        $crate::interpreter::builtins::spec::EvalParamSpec {
                            name: eval_builtin!(@name_str $param),
                            default: eval_builtin!(@default $($default)?),
                            by_ref: eval_builtin!(@param_by_ref $($mode)?),
                        },
                    )*
                ],
                variadic: None,
                by_ref_params: &[$(eval_builtin!(@name_str $by_ref)),*],
                required_param_count: None,
                direct: None,
                values: Some($crate::interpreter::builtins::spec::EvalValuesHook::$values),
            }
        }
    };

    (
        name: $name:literal,
        area: $area:ident,
        params: [$($param:ident $(: $mode:ident)? $(= $default:expr)?),* $(,)?],
        by_ref: [$($by_ref:ident),* $(,)?],
        direct: $direct:ident,
        values: $values:ident $(,)?
    ) => {
        inventory::submit! {
            $crate::interpreter::builtins::spec::EvalBuiltinSpec {
                name: $name,
                area: $crate::interpreter::builtins::spec::EvalArea::$area,
                param_names: &[$(eval_builtin!(@name_str $param)),*],
                params: &[
                    $(
                        $crate::interpreter::builtins::spec::EvalParamSpec {
                            name: eval_builtin!(@name_str $param),
                            default: eval_builtin!(@default $($default)?),
                            by_ref: eval_builtin!(@param_by_ref $($mode)?),
                        },
                    )*
                ],
                variadic: None,
                by_ref_params: &[$(eval_builtin!(@name_str $by_ref)),*],
                required_param_count: None,
                direct: Some($crate::interpreter::builtins::spec::EvalDirectHook::$direct),
                values: Some($crate::interpreter::builtins::spec::EvalValuesHook::$values),
            }
        }
    };

    (
        name: $name:literal,
        area: $area:ident,
        params: [$($param:ident $(= $default:expr)?),* $(,)?],
        variadic: $variadic:ident,
        direct: $direct:ident,
        values: $values:ident $(,)?
    ) => {
        inventory::submit! {
            $crate::interpreter::builtins::spec::EvalBuiltinSpec {
                name: $name,
                area: $crate::interpreter::builtins::spec::EvalArea::$area,
                param_names: &[$(eval_builtin!(@name_str $param),)* eval_builtin!(@name_str $variadic)],
                params: &[
                    $(
                        $crate::interpreter::builtins::spec::EvalParamSpec {
                            name: eval_builtin!(@name_str $param),
                            default: eval_builtin!(@default $($default)?),
                            by_ref: false,
                        },
                    )*
                ],
                variadic: Some(eval_builtin!(@name_str $variadic)),
                by_ref_params: &[],
                required_param_count: None,
                direct: Some($crate::interpreter::builtins::spec::EvalDirectHook::$direct),
                values: Some($crate::interpreter::builtins::spec::EvalValuesHook::$values),
            }
        }
    };

    (
        name: $name:literal,
        area: $area:ident,
        params: [$($param:ident $(= $default:expr)?),* $(,)?],
        required: $required:expr,
        direct: $direct:ident,
        values: $values:ident $(,)?
    ) => {
        inventory::submit! {
            $crate::interpreter::builtins::spec::EvalBuiltinSpec {
                name: $name,
                area: $crate::interpreter::builtins::spec::EvalArea::$area,
                param_names: &[$(eval_builtin!(@name_str $param)),*],
                params: &[
                    $(
                        $crate::interpreter::builtins::spec::EvalParamSpec {
                            name: eval_builtin!(@name_str $param),
                            default: eval_builtin!(@default $($default)?),
                            by_ref: false,
                        },
                    )*
                ],
                variadic: None,
                by_ref_params: &[],
                required_param_count: Some($required),
                direct: Some($crate::interpreter::builtins::spec::EvalDirectHook::$direct),
                values: Some($crate::interpreter::builtins::spec::EvalValuesHook::$values),
            }
        }
    };

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
                param_names: &[$(eval_builtin!(@name_str $param)),*],
                params: &[
                    $(
                        $crate::interpreter::builtins::spec::EvalParamSpec {
                            name: eval_builtin!(@name_str $param),
                            default: eval_builtin!(@default $($default)?),
                            by_ref: false,
                        },
                    )*
                ],
                variadic: None,
                by_ref_params: &[],
                required_param_count: None,
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

    (@param_by_ref) => {
        false
    };

    (@param_by_ref by_ref) => {
        true
    };

    (@name_str r#break) => {
        "break"
    };

    (@name_str r#type) => {
        "type"
    };

    (@name_str $name:ident) => {
        stringify!($name)
    };
}
