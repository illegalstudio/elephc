//! Purpose:
//! Defines the narrow intermediate representation used by generator codegen:
//! `ResumeNode`, `BodyStmt`, `IntSource`/`MixedSource`, slot types, and the
//! state numberer.
//!
//! Called from:
//!  - `crate::codegen::functions::generator::build` (produces the IR).
//!  - `crate::codegen::functions::generator::emit` (consumes the IR).
//!
//! Key details:
//!  - The IR is deliberately narrow — anything outside the v1 generator
//!    grammar lowers to `ResumeNode::Bail` and short-circuits to the resume
//!    function's terminator at compile time.
//!  - State indices are assigned depth-first in source order so resume-label
//!    emission and runtime state dispatch stay in lockstep.

pub(super) enum ResumeNode {
    Stmt(BodyStmt),
    Yield(YieldEntry, u32),
    /// `$local = yield <expr>;` — emits the yield, then on resume reads
    /// the sent_value Mixed pointer (boxed by `Generator::send($v)` at
    /// the type-check call site). For Int-typed LHS, unboxes via
    /// `__rt_mixed_unbox` and stores the int. For Mixed-typed LHS,
    /// refcount-replaces the slot with the sent Mixed pointer (incref
    /// to share with whoever else holds it). `next()` (no send) leaves
    /// sent_value NULL — Int slot receives 0, Mixed slot stays NULL.
    YieldAssign {
        local_idx: usize,
        local_ty: SlotType,
        yield_entry: YieldEntry,
        state_idx: u32,
    },
    If {
        cond: BoolExpr,
        then_body: Vec<ResumeNode>,
        else_body: Vec<ResumeNode>,
    },
    While {
        cond: BoolExpr,
        body: Vec<ResumeNode>,
    },
    DoWhile {
        cond: BoolExpr,
        body: Vec<ResumeNode>,
    },
    For {
        init: Vec<ResumeNode>,
        cond: BoolExpr,
        update: Vec<ResumeNode>,
        body: Vec<ResumeNode>,
    },
    Break,
    Continue,
    /// `switch (subject) { case <int>: ...; default: ... }` — case values
    /// must be integer literals in v1; cases fall through unless they
    /// `break`. The switch end label is pushed onto the loop stack so
    /// `break` inside cases jumps to switch end.
    Switch {
        subject: IntSource,
        cases: Vec<(Vec<i64>, Vec<ResumeNode>)>,
        default: Vec<ResumeNode>,
    },
    /// `try { ... } finally { ... }` inside a generator body. The v1
    /// generator IR preserves the normal no-exception path by running the
    /// translated try body followed by the translated finally body. Catch
    /// dispatch remains outside this narrow IR.
    Try {
        try_body: Vec<ResumeNode>,
        finally_body: Vec<ResumeNode>,
    },
    /// `yield from <expr>` — runtime delegation. `source` describes how
    /// to materialise the inner Generator pointer. The single state index
    /// is reused on every resume call so successive `next()` invocations
    /// advance the inner generator one step at a time. `result`
    /// describes whether the delegated terminal return value is
    /// discarded, stored in a local, or returned by the outer generator.
    YieldFromGenerator {
        source: YieldFromSource,
        state_idx: u32,
        result: YieldFromResult,
    },
    /// `return <expr>;` inside a generator body — boxes the value into
    /// the frame's `return_value` slot and terminates the generator.
    /// `return;` (no expression) terminates without writing a value.
    Return(Option<MixedSource>),
    /// Sequence of nodes treated as a single unit. Used when one source
    /// statement desugars to multiple `ResumeNode`s (e.g. `yield from
    /// [a, b, c]` expands to several `Yield` nodes).
    Block {
        stmts: Vec<ResumeNode>,
    },
    /// Pseudo-node emitted whenever we hit something the v1 grammar
    /// doesn't translate; the emitter routes it straight to the
    /// terminator label so the rest of the body has no effect.
    Bail,
}

#[derive(Clone)]
/// Statements inside a generator body that represent simple imperative actions:
/// integer/mixed assignment, post-increment, and post-decrement. These are
///terminal actions within a basic block, as opposed to control-flow nodes
/// (If, While, etc.) which branch into sub-trees of ResumeNodes.
pub(super) enum BodyStmt {
    AssignInt(usize, IntSource),
    /// `$local = <mixed_expr>` where `$local` is a Mixed-typed slot. The
    /// emitter follows the standard refcount-replace pattern: park the
    /// previous Mixed pointer in x20, materialize the new boxed Mixed
    /// pointer in x0, store it into the slot, then decref the previous.
    AssignMixed(usize, MixedSource),
    /// `echo <mixed_expr>` inside a generator body. The expression is boxed as
    /// a Mixed cell, written with PHP echo semantics, then released.
    EchoMixed(MixedSource),
    /// `var_dump(<mixed_expr>)` inside a generator body. The expression is
    /// boxed as a Mixed cell, formatted with PHP diagnostic semantics, then
    /// released.
    VarDumpMixed(MixedSource),
    PostIncrement(usize),
    PostDecrement(usize),
}

/// Tracks the type of each slot in the unified params+locals table used by
/// generator resume frames. Determines how the emitter materialises a value
/// (integer register vs. boxed Mixed pointer) and whether cleanup is needed.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum SlotType {
    Int,
    Mixed,
}

/// How to materialise the inner Generator pointer for a `yield from`.
#[derive(Clone)]
pub(super) enum YieldFromSource {
    /// `yield from <fn_name>(args)` — call the function, get the
    /// Generator pointer in `x0`.
    Call { fn_name: String, args: Vec<IntSource> },
    /// `yield from $local` where `$local` is an Int-typed slot holding
    /// the raw Generator pointer (typically the result of a previous
    /// generator-function call).
    IntSlot(usize),
    /// `yield from $local` where `$local` is a Mixed-typed slot whose
    /// boxed Mixed cell wraps an Object payload (a Generator or other
    /// Iterator). We `__rt_mixed_unbox` to recover the raw object
    /// pointer before driving the delegation loop.
    MixedSlot(usize),
}

#[derive(Clone, Copy)]
/// Where the result of a `yield from` delegation is sent: discarded,
/// stored in a local slot, or propagated as the outer generator's return
/// value. Controls whether and how the runtime result is captured.
pub(super) enum YieldFromResult {
    Discard,
    /// `$local = yield from ...` stores the delegated return in a
    /// Mixed-typed local slot.
    Local(usize),
    /// `return yield from ...` stores the delegated return in the outer
    /// frame's return_value slot, then terminates the outer generator.
    Return,
}

#[derive(Clone)]
/// Captures the key and value expressions of a `yield` or `yield from`
/// expression. `key` is `None` for plain `yield <value>` (auto-incremented
/// numeric key); `key` is `Some` for `yield <key> => <value>`. Both `key`
/// and `value` are MixedSource so they can be integers, strings, or slot
/// reads boxed at yield time.
pub(super) struct YieldEntry {
    /// `None` means use the auto-incrementing counter.
    pub key: Option<MixedSource>,
    pub value: MixedSource,
}

/// Source of a Mixed-cell payload. v1 covers integer expressions, string
/// literals, homogeneous int-array literals, and reads of Mixed-typed
/// slots (Mixed locals); the first three are boxed at yield time via
/// `__rt_mixed_from_value`, while reads incref the existing boxed
/// pointer to share the cell with the slot.
#[derive(Clone)]
pub(super) enum MixedSource {
    Null,
    Int(IntSource),
    Str { label: String, len: usize },
    /// Homogeneous int-array literal `[1, 2, 3]`. Allocated on the heap
    /// at yield time and boxed as a Mixed cell with the array tag.
    IntArrayLit(Vec<i64>),
    /// Read of a Mixed-typed slot. The emitter loads the boxed Mixed
    /// pointer from the slot and `__rt_incref`s it so the slot keeps its
    /// own reference.
    MixedSlot(usize),
}

/// Source of an integer value in generator codegen. Covers literal constants,
/// reads of Int-typed slots, binary operations on two IntSources, and function
/// calls whose return value is assumed to be an integer (v1 does not typecheck
/// this assumption). Arguments to IntSource calls are evaluated left-to-right
/// into a stack area before being loaded into x0..x7 for the branch.
#[derive(Clone)]
pub(super) enum IntSource {
    Literal(i64),
    /// Index into the unified params+locals table — only valid for slots
    /// whose `SlotType` is `Int`.
    Slot(usize),
    BinaryOp(Box<IntSource>, IntBinOp, Box<IntSource>),
    /// `funcname($a, $b, ...)` where each argument is itself an
    /// `IntSource`. Args are evaluated left-to-right into a stack stash
    /// then popped into x0..x7 just before the `bl`. The return value
    /// (assumed int — v1 doesn't typecheck this, garbage otherwise)
    /// arrives in x0.
    Call {
        fn_name: String,
        args: Vec<IntSource>,
    },
}

#[derive(Clone, Copy)]
/// Integer binary operations usable inside a BoolExpr or as part of an
/// IntSource. Covers the four basic arithmetic operators; no modulo,
/// bitwise, or comparison operators — those live in CmpOp/BoolExpr.
pub(super) enum IntBinOp {
    Add,
    Sub,
    Mul,
    Div,
}

/// A boolean expression used in generator control-flow and ternary nodes.
/// Covers integer comparisons and null checks against Mixed slots.
#[derive(Clone)]
pub(super) enum BoolExpr {
    /// Compares two int-classifiable values with a comparison operator.
    IntCompare {
        left: IntSource,
        op: CmpOp,
        right: IntSource,
    },
    /// Compares a Mixed-typed slot with PHP null.
    MixedSlotNull {
        slot_idx: usize,
        is_equal: bool,
    },
}

#[derive(Clone, Copy)]
/// Comparison operators used in BoolExpr. All variants compare two IntSources.
/// For integer operands: Lt (<), Le (<=), Gt (>), Ge (>=), Eq (==), Ne (!=).
/// PHP loosely-typed comparison is NOT supported — both operands must be
/// integer-typed IntSources.
pub(super) enum CmpOp {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

/// Assigns sequential state indices to yield sites in a generator resume
/// function. State 0 is reserved for the body entry point; yield sites
/// receive 1, 2, ... in depth-first source order. This keeps the emit
/// order and runtime dispatch table in lockstep.
pub(super) struct StateNumberer {
    pub next_state: u32,
}

impl StateNumberer {
    /// Creates a new numberer. State 0 is reserved for the body entry;
    /// the first yield site receives state 1.
    pub fn new() -> Self {
        Self { next_state: 1 }
    }

    /// Returns the next available state index and increments the counter.
    /// Callers must use the returned index as the state number for a yield
    /// site before requesting the next one.
    pub fn next(&mut self) -> u32 {
        let s = self.next_state;
        self.next_state += 1;
        s
    }
}
