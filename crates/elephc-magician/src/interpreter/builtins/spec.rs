//! Purpose:
//! Declarative builtin specifications for the eval interpreter.
//! Each spec owns PHP-visible metadata plus optional direct and evaluated-arg
//! dispatch hooks for one builtin.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry` lookup and metadata helpers.
//! - `eval_builtin!` submissions in per-builtin home files.
//!
//! Key details:
//! - Specs are collected with `inventory` to let builtin files register
//!   themselves without growing a central match.
//! - Hook enums keep calls monomorphized over `RuntimeValueOps`.

use super::super::{
    eval_builtin_count, eval_builtin_gettype, eval_builtin_strlen, eval_builtin_type_predicate,
    eval_count_result, ElephcEvalContext, ElephcEvalScope, EvalExpr, EvalStatus,
    RuntimeCellHandle, RuntimeValueOps,
};
use super::{
    eval_builtin_abs, eval_builtin_cast, eval_builtin_strrev, eval_cast_result,
    eval_gettype_result, eval_type_predicate_result,
};
pub(in crate::interpreter) use super::registry::EvalBuiltinDefaultValue;

/// Broad domain used to group eval builtin home files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::interpreter) enum EvalArea {
    /// Array and collection builtins.
    Array,
    /// Numeric and mathematical builtins.
    Math,
    /// String-processing builtins.
    String,
    /// Scalar conversion and type-related builtins.
    Types,
}

/// Parameter metadata for one eval builtin argument.
#[derive(Clone, Copy)]
pub(in crate::interpreter) struct EvalParamSpec {
    /// PHP-visible parameter name.
    pub(in crate::interpreter) name: &'static str,
    /// Optional PHP default value.
    pub(in crate::interpreter) default: Option<EvalBuiltinDefaultValue>,
    /// Whether this parameter must bind to caller storage.
    pub(in crate::interpreter) by_ref: bool,
}

/// Direct expression-level dispatch hooks for migrated builtins.
#[derive(Clone, Copy)]
pub(in crate::interpreter) enum EvalDirectHook {
    /// Dispatches `abs(...)`.
    Abs,
    /// Dispatches scalar cast builtins.
    Cast,
    /// Dispatches `count(...)`.
    Count,
    /// Dispatches `gettype(...)`.
    Gettype,
    /// Dispatches `strlen(...)`.
    Strlen,
    /// Dispatches `strrev(...)`.
    Strrev,
    /// Dispatches scalar and container type predicates.
    TypePredicate,
}

/// Evaluated-argument dispatch hooks for migrated builtins.
#[derive(Clone, Copy)]
pub(in crate::interpreter) enum EvalValuesHook {
    /// Dispatches `abs(...)`.
    Abs,
    /// Dispatches scalar cast builtins.
    Cast,
    /// Dispatches `count(...)`.
    Count,
    /// Dispatches `gettype(...)`.
    Gettype,
    /// Dispatches `strlen(...)`.
    Strlen,
    /// Dispatches `strrev(...)`.
    Strrev,
    /// Dispatches scalar and container type predicates.
    TypePredicate,
}

/// Static declaration for one PHP-visible eval builtin.
pub(in crate::interpreter) struct EvalBuiltinSpec {
    /// Canonical lowercase PHP builtin name.
    pub(in crate::interpreter) name: &'static str,
    /// Builtin family used by the file layout.
    pub(in crate::interpreter) area: EvalArea,
    /// Parameter names in PHP call order.
    pub(in crate::interpreter) param_names: &'static [&'static str],
    /// Parameter metadata in PHP call order.
    pub(in crate::interpreter) params: &'static [EvalParamSpec],
    /// Variadic parameter name, when supported.
    pub(in crate::interpreter) variadic: Option<&'static str>,
    /// Parameter names that must bind by reference.
    pub(in crate::interpreter) by_ref_params: &'static [&'static str],
    /// Direct expression-level dispatch hook.
    pub(in crate::interpreter) direct: Option<EvalDirectHook>,
    /// Evaluated-argument dispatch hook.
    pub(in crate::interpreter) values: Option<EvalValuesHook>,
}

impl EvalBuiltinSpec {
    /// Returns this builtin's file-layout area.
    pub(in crate::interpreter) fn area(&self) -> EvalArea {
        self.area
    }

    /// Returns the number of required leading parameters.
    pub(in crate::interpreter) fn required_param_count(&self) -> usize {
        self.params
            .iter()
            .take_while(|param| param.default.is_none())
            .count()
    }

    /// Returns the number of parameters that define defaults.
    pub(in crate::interpreter) fn default_param_count(&self) -> usize {
        self.params
            .iter()
            .filter(|param| param.default.is_some())
            .count()
    }

    /// Returns by-reference parameter names, checking they agree with param flags in debug builds.
    pub(in crate::interpreter) fn by_ref_param_names(&self) -> &'static [&'static str] {
        debug_assert!(self
            .params
            .iter()
            .filter(|param| param.by_ref)
            .all(|param| self.by_ref_params.contains(&param.name)));
        self.by_ref_params
    }

    /// Returns the default value for one PHP parameter slot.
    pub(in crate::interpreter) fn default_value(
        &self,
        param_index: usize,
    ) -> Option<EvalBuiltinDefaultValue> {
        self.params.get(param_index).and_then(|param| param.default)
    }
}

impl EvalDirectHook {
    /// Runs a direct expression-level builtin call through the migrated hook.
    pub(in crate::interpreter) fn call(
        self,
        name: &str,
        args: &[EvalExpr],
        context: &mut ElephcEvalContext,
        scope: &mut ElephcEvalScope,
        values: &mut impl RuntimeValueOps,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        match self {
            Self::Abs => eval_builtin_abs(args, context, scope, values),
            Self::Cast => eval_builtin_cast(name, args, context, scope, values),
            Self::Count => eval_builtin_count(args, context, scope, values),
            Self::Gettype => eval_builtin_gettype(args, context, scope, values),
            Self::Strlen => eval_builtin_strlen(args, context, scope, values),
            Self::Strrev => eval_builtin_strrev(args, context, scope, values),
            Self::TypePredicate => eval_builtin_type_predicate(name, args, context, scope, values),
        }
    }
}

impl EvalValuesHook {
    /// Runs an evaluated-argument builtin call through the migrated hook.
    pub(in crate::interpreter) fn call(
        self,
        name: &str,
        evaluated_args: &[RuntimeCellHandle],
        context: &mut ElephcEvalContext,
        values: &mut impl RuntimeValueOps,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        match self {
            Self::Abs => {
                let [value] = evaluated_args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                values.abs(*value)
            }
            Self::Cast => {
                let [value] = evaluated_args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                eval_cast_result(name, *value, context, values)
            }
            Self::Count => match evaluated_args {
                [value] => eval_count_result(*value, None, context, values),
                [value, mode] => eval_count_result(*value, Some(*mode), context, values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::Gettype => {
                let [value] = evaluated_args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                eval_gettype_result(*value, values)
            }
            Self::Strlen => {
                let [value] = evaluated_args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                let bytes = values.string_bytes(*value)?;
                let len = i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?;
                values.int(len)
            }
            Self::Strrev => {
                let [value] = evaluated_args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                values.strrev(*value)
            }
            Self::TypePredicate => {
                let [value] = evaluated_args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                eval_type_predicate_result(name, *value, context, values)
            }
        }
    }
}

inventory::collect!(EvalBuiltinSpec);
