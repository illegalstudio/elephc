//! Purpose:
//! Flat executable image compiled from a narrow EvalIR slice.
//! The tree walker stays the consumer for every construct this image cannot name.
//!
//! Called from:
//! - `crate::interpreter::bytecode` when it compiles or runs a fragment.
//! - `crate::eval_ir::EvalFunction` which holds one body after a function is admitted.
//!
//! Key details:
//! - Opcodes name operations. `RuntimeValueOps` still owns PHP values, arithmetic, and casts.
//! - A frame's instruction index is the resume point. Line numbers and exception
//!   regions are omitted until EvalIR carries the spans those tables would index.
//! - `CompiledBody` does not embed `EvalFunction`, so the `Arc` on a function cannot cycle.

use super::{EvalBinOp, EvalCallArg, EvalSourceLocation, EvalStmt, EvalUnaryOp};
use std::sync::Arc;

/// One operation in a compiled eval body.
///
/// Jumps store an absolute instruction index. `PrepareCall` selects the callee
/// before argument effects; `Call` carries the exact positional argument count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Op {
    /// Runs async signal handlers at the same source boundary as the tree walker.
    StatementBoundary,
    /// Pushes null.
    Null,
    /// Pushes a bool.
    Bool(bool),
    /// Pushes an int.
    Int(i64),
    /// Pushes a float from its raw bits.
    Float(u64),
    /// Pushes a string from this body's string table.
    String(u16),
    /// Pushes a retained copy of a local, or null after an undefined-variable warning.
    Load(u16),
    /// Pops a value into a local.
    Store(u16),
    /// Pops and releases a discarded value.
    Pop,
    /// Warns for an ignored direct call result, then releases it even when the warning throws.
    PopDiscardedCall(u16),
    /// Pushes a second owner of the current top value.
    Dup,
    /// Pops the right operand, then the left, and pushes the operator result.
    Binary(EvalBinOp),
    /// Pops one operand and pushes the unary result.
    Unary(EvalUnaryOp),
    /// Pops a value and pushes its int cast.
    CastInt,
    /// Pops a value and pushes its float cast.
    CastFloat,
    /// Pops a value and pushes its string cast.
    CastString,
    /// Pops a value and pushes its bool cast.
    CastBool,
    /// Sets the instruction index.
    Jump(u32),
    /// Pops a condition and jumps when it is not truthy.
    JumpIfFalse(u32),
    /// Pops a value, echoes it, and releases it.
    Echo,
    /// Pops a value, echoes it, and pushes int 1.
    Print,
    /// Pops the return value and leaves the current frame.
    Return,
    /// Leaves the current frame with no value.
    ReturnVoid,
    /// Selects `calls[index]`, or runs the source call and jumps past its argument opcodes.
    PrepareCall(u16, u32),
    /// Calls the selected compiled body with an exact positional arity.
    Call(u16),
    /// Defines `defined[index]` in the current context when this instruction runs.
    DefineFn(u16),
    /// Increments or decrements a local using PHP's null-decrement rule.
    IncDec(u16, bool),
}

/// Source arguments retained for context-sensitive calls that cannot use the positional fast path.
#[derive(Debug)]
pub(crate) struct CompiledCall {
    /// Original spelling used by the shared source dispatcher and its diagnostics.
    pub(crate) name: String,
    /// Unevaluated arguments preserve reference targets and source-sensitive builtin behavior.
    pub(crate) args: Vec<EvalCallArg>,
}

/// One compiled function or program body.
#[derive(Debug)]
pub(crate) struct CompiledBody {
    /// Source spelling. Empty for the program body, which is not a PHP function.
    pub(crate) name: String,
    /// Source-order parameter names. Empty for the program body.
    pub(crate) params: Vec<String>,
    /// Local names. Parameters occupy the leading slots.
    pub(crate) locals: Vec<String>,
    /// String literals referenced by opcodes.
    pub(crate) strings: Vec<String>,
    /// Source call sites addressed by `PrepareCall`.
    pub(crate) calls: Vec<CompiledCall>,
    /// Instructions, always ending in `Return` or `ReturnVoid`.
    pub(crate) ops: Vec<Op>,
    /// True when a positional call may bind arguments without the tree-walker binder.
    pub(crate) fast_positional: bool,
    /// Functions this body defines when their declarations are reached.
    pub(crate) defined: Vec<Arc<CompiledBody>>,
    /// Original body statements, used to rebuild the `EvalFunction` at `DefineFn`.
    pub(crate) source_body: Vec<EvalStmt>,
    /// Source range copied onto the function defined from this body.
    pub(crate) source_location: Option<EvalSourceLocation>,
}

/// Compiled form of one eval program. `main` is the fragment, not a PHP function.
#[derive(Debug, Clone)]
pub(crate) struct CompiledModule {
    /// Program instructions, including `DefineFn` for each admitted top-level function.
    pub(crate) main: Arc<CompiledBody>,
}
