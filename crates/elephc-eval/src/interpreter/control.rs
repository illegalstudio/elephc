//! Purpose:
//! Holds small interpreter-local control and call-shape types shared across eval execution modules.
//! These types describe control-flow escape values, evaluated call arguments, and parsed builtin state.
//!
//! Called from:
//! - `crate::interpreter` execution, builtin, and call-dispatch helpers.
//!
//! Key details:
//! - Runtime cells are opaque handles; these types do not own or release values by themselves.

use crate::value::RuntimeCellHandle;

/// Internal statement-control result used to propagate eval returns and loops.
pub(super) enum EvalControl {
    None,
    Return(RuntimeCellHandle),
    Throw(RuntimeCellHandle),
    Break,
    Continue,
}

/// Final result of executing a parsed eval program.
pub enum EvalOutcome {
    Value(RuntimeCellHandle),
    Throwable(RuntimeCellHandle),
}

/// One already evaluated function-like call argument.
#[derive(Clone)]
pub(super) struct EvaluatedCallArg {
    pub(super) name: Option<String>,
    pub(super) value: RuntimeCellHandle,
}

/// One already evaluated PHP callback supported by the eval dispatcher.
pub(super) enum EvaluatedCallable {
    Named(String),
    ObjectMethod {
        object: RuntimeCellHandle,
        method: String,
    },
}

/// Bound argument tuple for direct `array_splice()` calls.
pub(super) type EvalArraySpliceDirectArgs = (
    String,
    RuntimeCellHandle,
    Option<RuntimeCellHandle>,
    Option<RuntimeCellHandle>,
);

/// Parsed flags for one eval `sprintf()` conversion specifier.
#[derive(Clone, Copy)]
pub(super) struct EvalSprintfSpec {
    pub(super) left_align: bool,
    pub(super) force_sign: bool,
    pub(super) space_sign: bool,
    pub(super) zero_pad: bool,
    pub(super) alternate: bool,
    pub(super) width: Option<usize>,
    pub(super) precision: Option<usize>,
    pub(super) specifier: u8,
}

/// Eval-visible predefined constant payloads that are not stored in the dynamic context.
pub(super) enum EvalPredefinedConstant {
    Int(i64),
    Float(f64),
    String(&'static str),
}
