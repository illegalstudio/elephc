//! Purpose:
//! Defines the jump plan of one label scope: the tree of statement lists ("blocks"), the labels
//! and resolved jumps found in it, and the dispatch loop planned for each routing block.
//!
//! Called from:
//! - `crate::goto_lowering::analysis`, which builds the plan.
//! - `crate::goto_lowering::rewrite`, which executes it.
//!
//! Key details:
//! - A "block" is one statement list: the scope body, a loop body, an `if` branch, a `try`,
//!   `catch` or `finally` body, a `switch` case, a braced `namespace` body. Blocks form a tree; a jump is executed at the
//!   lowest block that holds both the `goto` and the label (their LCA).
//! - `child_blocks` and `child_blocks_mut` define the one child order both the analysis and the
//!   rewrite number blocks by. They must list the same bodies in the same order.

use std::collections::{BTreeMap, HashMap};

use crate::parser::ast::{Stmt, StmtKind};

/// Index of a block in `Plan::blocks`. The scope body is block 0.
pub(super) type BlockId = usize;

/// How a block hangs off the statement that owns it, which decides whether a jump may enter it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Via {
    /// The scope body itself.
    Root,
    /// An `if`/`elseif`/`else` branch, a `try` body, or a statement splice: a jump from outside
    /// may land inside.
    Enterable,
    /// A `while`, `do`-`while`, `for` or `foreach` body. PHP forbids jumping in.
    Loop,
    /// One `case` or `default` body of a `switch`. PHP forbids jumping in from outside the
    /// `switch`.
    SwitchCase,
    /// A `catch` body. PHP allows jumping in; this compiler does not (see [`Unsupported`]).
    Catch,
    /// A `finally` body. PHP forbids jumping in or out, so it is a jump island of its own.
    Finally,
    /// The body of a braced `namespace` block. It shares the file's label scope, but a jump may
    /// not cross its braces: name resolution only knows the block as a top-level statement.
    Namespace,
    /// A branch of the compile-time `ifdef` extension, present only where conditional
    /// compilation has not run. It has no run-time condition to route a resumed jump through.
    IfDef,
}

/// The jump shapes PHP accepts and this compiler refuses with an explicit diagnostic.
///
/// The first three would need the statement that holds the label to be rebuilt around the jump,
/// not just entered: a `catch` body cannot be reached without a thrown exception, and the bodies
/// of one `switch` do not form a single statement list. The `Across` ones are declarations
/// written between a `goto` and its label. Name resolution applies a `namespace` or a `use` to
/// the statements that follow it in one flat list, which a dispatch loop's nested segments are
/// not; and PHP defines a `const` when its statement runs, while this compiler binds every
/// constant at compile time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Unsupported {
    /// A jump into a `catch` block from outside that block.
    IntoCatch,
    /// A jump from one `case` body to a label in another `case` body of the same `switch`.
    AcrossSwitchCases,
    /// A jump into an unresolved `ifdef` branch.
    IntoIfDef,
    /// A jump that skips or repeats a `namespace` declaration, or enters or leaves a braced
    /// `namespace` block.
    AcrossNamespace,
    /// A jump that skips or repeats a `use` declaration.
    AcrossUse,
    /// A jump that skips or repeats a `const` declaration.
    AcrossConst,
}

impl Unsupported {
    /// Returns the diagnostic text for this refused jump shape.
    pub(super) fn message(self) -> &'static str {
        match self {
            Unsupported::IntoCatch => {
                "'goto' into a catch block from outside that block is not supported; \
                 move the label out of the catch block"
            }
            Unsupported::AcrossSwitchCases => {
                "'goto' from one case of a switch statement to a label in another case is not \
                 supported; move the label out of the switch statement"
            }
            Unsupported::IntoIfDef => {
                "'goto' into an ifdef block from outside that block is not supported"
            }
            Unsupported::AcrossNamespace => "'goto' across a namespace declaration is not supported",
            Unsupported::AcrossUse => {
                "'goto' across a use declaration is not supported; \
                 move the declaration above the code the jump covers"
            }
            Unsupported::AcrossConst => {
                "'goto' across a const declaration is not supported; \
                 move the declaration out of the range the jump covers"
            }
        }
    }
}

/// One statement list of the scope and its place in the block tree.
#[derive(Debug)]
pub(super) struct Block {
    /// The block whose statement owns this one, `None` for the scope body.
    pub parent: Option<BlockId>,
    /// How this block hangs off its owning statement.
    pub via: Via,
    /// Index, in the parent's statement list, of the statement that owns this block.
    pub stmt_index: usize,
    /// Distance from the scope body, which has depth 0.
    pub depth: usize,
    /// Jump island: the scope body's or the innermost enclosing `finally` body's. Each island
    /// has its own state local, because a `finally` runs while an outer jump is pending.
    pub island: usize,
    /// The dispatch loop this block becomes, when it routes at least one label.
    pub dispatch: Option<Dispatch>,
    /// Labels routed into this block from a block above it, by label number.
    pub incoming: Vec<i64>,
}

/// The dispatch loops planned for one block.
#[derive(Debug, Default)]
pub(super) struct Dispatch {
    /// Entry points by statement index: a label of this block, or a statement holding labels
    /// further down, each with the label numbers that resume there.
    pub entries: BTreeMap<usize, Vec<i64>>,
    /// Whether a label below is routed in from a block above, so entering this block may be a
    /// resumed jump and its one loop has to start at the block's first statement.
    pub entered_from_above: bool,
    /// The loops of the block, in statement order and never overlapping. Jumps whose statement
    /// spans do not touch get loops of their own, so a statement between two independent jumps
    /// stays where it was written.
    pub loops: Vec<LoopRange>,
}

/// The statements one dispatch loop wraps.
#[derive(Debug)]
pub(super) struct LoopRange {
    /// First statement index the loop wraps.
    pub lo: usize,
    /// Last statement index the loop wraps.
    pub hi: usize,
    /// Whether the loop reads the state local, which then has to be zeroed before the loop.
    pub needs_init: bool,
}

/// One label of the scope.
#[derive(Debug)]
pub(super) struct Label {
    /// Number stored in the state local to select this label. Zero means "no jump pending".
    pub id: i64,
    /// The block whose statement list holds the label.
    pub block: BlockId,
    /// Index of the label statement in that list.
    pub index: usize,
    /// Outermost block that routes this label, `None` when no `goto` targets it.
    pub top: Option<BlockId>,
    /// Whether reaching the label goes through the state local. False for a label whose only
    /// router is a single-entry loop: its jumps are a bare `continue`.
    pub uses_state: bool,
}

/// One `goto` of the scope, resolved.
#[derive(Debug)]
pub(super) struct Jump {
    /// Index into `Plan::labels` of the target.
    pub label: usize,
    /// The block whose dispatch loop executes the jump.
    pub lca: BlockId,
}

/// Everything the rewrite needs to know about one label scope.
#[derive(Debug, Default)]
pub(super) struct Plan {
    pub blocks: Vec<Block>,
    pub labels: Vec<Label>,
    /// Child blocks by (parent block, owning statement index, slot in `child_blocks` order).
    pub children: HashMap<(BlockId, usize, usize), BlockId>,
    /// Labels by (block, statement index).
    pub label_at: HashMap<(BlockId, usize), usize>,
    /// Jumps by (block, statement index) of the `goto` statement.
    pub jump_at: HashMap<(BlockId, usize), Jump>,
    /// For each block, the statement index pair (lower, upper) every jump it executes runs
    /// between: the statement holding the `goto` and the one holding the label. A statement
    /// strictly between the two is skipped by a forward jump or repeated by a backward one.
    pub crossings: HashMap<BlockId, Vec<(usize, usize)>>,
}

/// Lists the statement lists a statement owns in this label scope, in slot order.
///
/// Function and class bodies are other scopes and are not listed. The `init` and `update` of
/// a `for` are single parser-built statements that cannot hold a label or a jump.
pub(super) fn child_blocks(kind: &StmtKind) -> Vec<(Via, &Vec<Stmt>)> {
    match kind {
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            let mut blocks = vec![(Via::Enterable, then_body)];
            blocks.extend(elseif_clauses.iter().map(|(_, body)| (Via::Enterable, body)));
            blocks.extend(else_body.iter().map(|body| (Via::Enterable, body)));
            blocks
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            let mut blocks = vec![(Via::IfDef, then_body)];
            blocks.extend(else_body.iter().map(|body| (Via::IfDef, body)));
            blocks
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::For { body, .. }
        | StmtKind::Foreach { body, .. } => vec![(Via::Loop, body)],
        StmtKind::Switch { cases, default, .. } => {
            let mut blocks: Vec<_> = cases.iter().map(|(_, body)| (Via::SwitchCase, body)).collect();
            blocks.extend(default.iter().map(|body| (Via::SwitchCase, body)));
            blocks
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            let mut blocks = vec![(Via::Enterable, try_body)];
            blocks.extend(catches.iter().map(|clause| (Via::Catch, &clause.body)));
            blocks.extend(finally_body.iter().map(|body| (Via::Finally, body)));
            blocks
        }
        StmtKind::Synthetic(body) => vec![(Via::Enterable, body)],
        StmtKind::NamespaceBlock { body, .. } => vec![(Via::Namespace, body)],
        _ => Vec::new(),
    }
}

/// Mutable twin of [`child_blocks`]: the same bodies in the same slot order.
pub(super) fn child_blocks_mut(kind: &mut StmtKind) -> Vec<&mut Vec<Stmt>> {
    match kind {
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            let mut blocks = vec![then_body];
            blocks.extend(elseif_clauses.iter_mut().map(|(_, body)| body));
            blocks.extend(else_body.iter_mut());
            blocks
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            let mut blocks = vec![then_body];
            blocks.extend(else_body.iter_mut());
            blocks
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::For { body, .. }
        | StmtKind::Foreach { body, .. } => vec![body],
        StmtKind::Switch { cases, default, .. } => {
            let mut blocks: Vec<_> = cases.iter_mut().map(|(_, body)| body).collect();
            blocks.extend(default.iter_mut());
            blocks
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            let mut blocks = vec![try_body];
            blocks.extend(catches.iter_mut().map(|clause| &mut clause.body));
            blocks.extend(finally_body.iter_mut());
            blocks
        }
        StmtKind::Synthetic(body) => vec![body],
        StmtKind::NamespaceBlock { body, .. } => vec![body],
        _ => Vec::new(),
    }
}

/// Returns whether a statement opens a `break`/`continue` level of its own.
pub(super) fn opens_loop_level(kind: &StmtKind) -> bool {
    matches!(
        kind,
        StmtKind::While { .. }
            | StmtKind::DoWhile { .. }
            | StmtKind::For { .. }
            | StmtKind::Foreach { .. }
            | StmtKind::Switch { .. }
    )
}
