//! Purpose:
//! Rewrites one label scope according to its jump plan: wraps each routing statement list in a
//! dispatch loop, turns every `goto` into a state store plus `continue N`, drops the labels, and
//! renumbers the `break`/`continue` levels the new loops would otherwise capture.
//!
//! Called from:
//! - `crate::goto_lowering::GotoLowering::lower_scope()`.
//!
//! Key details:
//! - A dispatch loop wraps only the statements between the first and the last one involved in
//!   its jumps, so code before and after it keeps its straight-line shape. A block has one
//!   loop per group of jumps whose spans touch, so every statement inside a loop other than the
//!   ones holding its jumps and labels is skipped or repeated by a jump.
//! - A loop body is its segments in source order, each but the last guarded by
//!   `if ($state === 0 || $state === <its labels>)`. Arriving at a label zeroes the state, so
//!   every later guard passes and execution falls through the remaining segments exactly as the
//!   original code did. Plain `if` joins are what the checker and EIR lowering model best: a
//!   local assigned in a skipped segment is correctly "maybe unset" at the label.
//! - The last segment is unguarded and the loop exits from inside it. By then the state is
//!   zero or one of that segment's own labels, and termination analysis sees a function whose
//!   last segment returns as returning on every path.
//! - A jump is pending from its state store until it arrives at its label. Only `finally`
//!   bodies and destructors run in between, and one that throws cancels the jump, as in PHP.
//!   Every `catch` body therefore starts by zeroing the state: the exception it caught may have
//!   left a cancelled jump's label number behind, which would make later guards skip code.
//! - `frames` mirrors the `break`/`continue` levels open at the statement being rewritten, the
//!   program's own and the synthesized ones, so every level count is computed, never guessed.
//! - Function and class declarations are moved out in front of their loop: this compiler does
//!   not discover a declaration inside a loop body, and PHP binds them at compile time
//!   wherever a jump goes. A `namespace`, `use` or `const` declaration inside a loop is
//!   refused instead: see `plan::Unsupported`.

use std::collections::HashMap;

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::span::Span;
use crate::termination::{block_terminal_effect, TerminalEffect};

use super::plan::{
    child_blocks_mut, opens_loop_level, BlockId, Dispatch, Plan, Unsupported, Via,
};
use super::build::{resume_guard, Origin};

/// One `break`/`continue` level open around the statement being rewritten.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Frame {
    /// A loop or `switch` the program wrote.
    Original,
    /// The `while (true)` of a block's dispatch loop.
    DispatchLoop(BlockId),
}

/// One dispatch loop being assembled while the statements it wraps are rewritten.
struct OpenLoop {
    /// Source profile of the first wrapped statement, inherited by the synthesized nodes.
    origin: Origin,
    /// Declarations moved out in front of the loop.
    hoisted: Vec<Stmt>,
    /// Segments in source order with the label numbers that resume at each one. The first is
    /// the natural entry and has none.
    segments: Vec<(Vec<i64>, Vec<Stmt>)>,
}

impl OpenLoop {
    /// Starts a loop with its empty natural-entry segment.
    fn new(origin: Origin) -> Self {
        Self {
            origin,
            hoisted: Vec::new(),
            segments: vec![(Vec::new(), Vec::new())],
        }
    }
}

/// Rewrites the body of one label scope. Returns the new body and any diagnostics.
pub(super) fn rewrite_scope(
    body: Vec<Stmt>,
    plan: &Plan,
    fresh_state_local: &dyn Fn() -> String,
) -> (Vec<Stmt>, Vec<CompileError>) {
    let mut rewriter = Rewriter {
        plan,
        fresh_state_local,
        frames: Vec::new(),
        state_locals: HashMap::new(),
        errors: Vec::new(),
    };
    let body = rewriter.block(0, body);
    (body, rewriter.errors)
}

/// Rewrite state for one label scope.
struct Rewriter<'a> {
    plan: &'a Plan,
    fresh_state_local: &'a dyn Fn() -> String,
    frames: Vec<Frame>,
    /// State local of each jump island, minted on first use.
    state_locals: HashMap<usize, String>,
    errors: Vec<CompileError>,
}

impl Rewriter<'_> {
    /// Returns the state local of the jump island `block` belongs to.
    fn state_local(&mut self, block: BlockId) -> String {
        let island = self.plan.blocks[block].island;
        let fresh = self.fresh_state_local;
        self.state_locals.entry(island).or_insert_with(fresh).clone()
    }

    /// Returns whether any jump of the island `block` belongs to goes through the state local.
    fn island_uses_state(&self, block: BlockId) -> bool {
        let island = self.plan.blocks[block].island;
        self.plan.labels.iter().any(|label| {
            label.top.is_some() && label.uses_state && self.plan.blocks[label.block].island == island
        })
    }

    /// Rewrites one statement list.
    fn block(&mut self, id: BlockId, stmts: Vec<Stmt>) -> Vec<Stmt> {
        match &self.plan.blocks[id].dispatch {
            Some(dispatch) => self.dispatch_block(id, stmts, dispatch),
            None => stmts
                .into_iter()
                .enumerate()
                .flat_map(|(index, stmt)| self.stmt(id, index, stmt))
                .collect(),
        }
    }

    /// Rewrites a statement list that routes labels, wrapping each planned range in a loop.
    fn dispatch_block(&mut self, id: BlockId, stmts: Vec<Stmt>, dispatch: &Dispatch) -> Vec<Stmt> {
        let mut out = Vec::new();
        let mut ranges = dispatch.loops.iter();
        let mut next = ranges.next();
        let mut open: Option<OpenLoop> = None;

        for (index, stmt) in stmts.into_iter().enumerate() {
            let Some(range) = next.filter(|range| range.lo <= index) else {
                out.extend(self.stmt(id, index, stmt));
                continue;
            };
            if index == range.lo {
                self.frames.push(Frame::DispatchLoop(id));
                open = Some(OpenLoop::new(Origin::of(&stmt)));
            }
            let current = open.as_mut().expect("opened at the first statement of the range");
            if hoists_out_of_dispatch(&stmt.kind) {
                current.hoisted.push(stmt);
            } else {
                if let Some(refused) = refused_inside_dispatch(&stmt.kind) {
                    self.errors.push(CompileError::new(stmt.span, refused.message()));
                }
                if let Some(ids) = dispatch.entries.get(&index) {
                    current.segments.push((ids.clone(), Vec::new()));
                }
                let rewritten = self.stmt(id, index, stmt);
                current
                    .segments
                    .last_mut()
                    .expect("the natural-entry segment always exists")
                    .1
                    .extend(rewritten);
            }
            if index == range.hi {
                self.frames.pop();
                let finished = open.take().expect("opened at the first statement of the range");
                out.extend(self.close_loop(id, range.needs_init, finished));
                next = ranges.next();
            }
        }
        out
    }

    /// Builds the statements that replace one wrapped range: its hoisted declarations, the
    /// state reset when the loop reads the state, and the loop itself.
    fn close_loop(&mut self, id: BlockId, needs_init: bool, open: OpenLoop) -> Vec<Stmt> {
        let OpenLoop {
            origin,
            hoisted,
            mut segments,
        } = open;
        let mut out = hoisted;
        if needs_init {
            let state = self.state_local(id);
            out.push(origin.assign_state(&state, 0));
        }
        let (_, mut last) = segments.pop().expect("the natural-entry segment always exists");
        if block_terminal_effect(&last) == TerminalEffect::FallsThrough {
            last.push(origin.stmt(StmtKind::Break(1)));
        }
        let mut loop_body = Vec::new();
        for (ids, body) in segments {
            // An empty segment has nothing to skip: the natural entry when the range starts
            // at a label, or a label whose jumps need no state.
            if !body.is_empty() {
                let state = self.state_local(id);
                loop_body.push(origin.guarded_segment(&state, &ids, body));
            }
        }
        loop_body.extend(last);
        out.push(origin.dispatch_loop(loop_body));
        out
    }

    /// Rewrites one statement of `block`, returning the statements that replace it.
    fn stmt(&mut self, block: BlockId, index: usize, mut stmt: Stmt) -> Vec<Stmt> {
        let origin = Origin::of(&stmt);
        match &stmt.kind {
            StmtKind::GotoLabel(_) => {
                let routed = self.plan.label_at.get(&(block, index)).is_some_and(|&label| {
                    let label = &self.plan.labels[label];
                    label.top.is_some() && label.uses_state
                });
                if routed {
                    // Arriving at the label completes the jump.
                    let state = self.state_local(block);
                    vec![origin.assign_state(&state, 0)]
                } else {
                    Vec::new()
                }
            }
            StmtKind::Goto(_) => {
                let jump = self
                    .plan
                    .jump_at
                    .get(&(block, index))
                    .expect("every goto of a valid scope was resolved");
                let label = &self.plan.labels[jump.label];
                let loop_frame = self
                    .frames
                    .iter()
                    .rposition(|frame| *frame == Frame::DispatchLoop(jump.lca))
                    .expect("a goto sits inside the dispatch loop that executes it");
                let levels = self.frames.len() - loop_frame;
                let mut replacement = Vec::new();
                if label.uses_state {
                    let state = self.state_local(block);
                    replacement.push(origin.assign_state(&state, label.id));
                }
                replacement.push(origin.stmt_at(StmtKind::Continue(levels), stmt.span));
                replacement
            }
            StmtKind::Break(levels) => {
                let levels = self.renumber("break", *levels, stmt.span);
                stmt.kind = StmtKind::Break(levels);
                vec![stmt]
            }
            StmtKind::Continue(levels) => {
                let levels = self.renumber("continue", *levels, stmt.span);
                stmt.kind = StmtKind::Continue(levels);
                vec![stmt]
            }
            kind => {
                let opens_level = opens_loop_level(kind);
                if opens_level {
                    self.frames.push(Frame::Original);
                }
                for (slot, child) in child_blocks_mut(&mut stmt.kind).into_iter().enumerate() {
                    let child_id = *self
                        .plan
                        .children
                        .get(&(block, index, slot))
                        .expect("the scan recorded every child block");
                    let mut body = self.block(child_id, std::mem::take(child));
                    if self.plan.blocks[child_id].via == Via::Catch && self.island_uses_state(child_id) {
                        // A caught exception cancelled whatever jump was on its way.
                        let state = self.state_local(child_id);
                        body.insert(0, origin.assign_state(&state, 0));
                    }
                    *child = body;
                }
                if opens_level {
                    self.frames.pop();
                }
                self.guard_resumed_if(block, index, &mut stmt);
                vec![stmt]
            }
        }
    }

    /// Maps a `break`/`continue` level the program wrote onto the levels now open.
    ///
    /// The statement keeps its target: the `levels`-th loop or `switch` of the program around
    /// it. A level that reaches past every one of them was already out of range. It is pushed
    /// past the synthesized levels too, so it cannot leave a dispatch loop by accident, and it
    /// is reported here with the level the program wrote and the type checker's wording: the
    /// checker would only see the renumbered level.
    fn renumber(&mut self, keyword: &str, levels: usize, span: Span) -> usize {
        let mut originals = 0;
        for (distance, frame) in self.frames.iter().rev().enumerate() {
            if *frame == Frame::Original {
                originals += 1;
                if originals == levels {
                    return distance + 1;
                }
            }
        }
        let synthesized = self.frames.len() - originals;
        if synthesized > 0 {
            self.errors.push(CompileError::new(
                span,
                &format!("Cannot '{}' {} levels", keyword, levels),
            ));
        }
        levels + synthesized
    }

    /// Guards the conditions of an `if` that a resumed jump has to enter.
    fn guard_resumed_if(&mut self, block: BlockId, index: usize, stmt: &mut Stmt) {
        let is_entry = self.plan.blocks[block]
            .dispatch
            .as_ref()
            .is_some_and(|dispatch| dispatch.entries.contains_key(&index));
        if !is_entry {
            return;
        }
        let StmtKind::If {
            condition,
            elseif_clauses,
            ..
        } = &mut stmt.kind
        else {
            return;
        };
        // Labels resumed in each branch, by slot: `then`, each `elseif`, then `else`.
        let incoming = |slot: usize| -> Vec<i64> {
            self.plan
                .children
                .get(&(block, index, slot))
                .map(|&child| self.plan.blocks[child].incoming.clone())
                .unwrap_or_default()
        };
        let slots = elseif_clauses.len() + 2;
        let ids: Vec<Vec<i64>> = (0..slots).map(incoming).collect();
        let Some(last) = ids.iter().rposition(|ids| !ids.is_empty()) else {
            return;
        };
        let state = self.state_local(block);
        let conditions = std::iter::once(condition).chain(elseif_clauses.iter_mut().map(|(c, _)| c));
        for (slot, condition) in conditions.enumerate().take(last + 1) {
            let written = std::mem::replace(condition, Expr::new(ExprKind::Null, condition.span));
            *condition = resume_guard(&state, &ids[slot], slot < last, written);
        }
    }
}

/// Returns whether a statement inside a dispatch range is moved out in front of the loop.
///
/// These are the declarations PHP binds when it compiles the file, wherever they are written
/// and whichever way a jump goes around them.
fn hoists_out_of_dispatch(kind: &StmtKind) -> bool {
    matches!(
        kind,
        StmtKind::FunctionDecl { .. }
            | StmtKind::ClassDecl { .. }
            | StmtKind::EnumDecl { .. }
            | StmtKind::PackedClassDecl { .. }
            | StmtKind::InterfaceDecl { .. }
            | StmtKind::TraitDecl { .. }
            | StmtKind::ExternFunctionDecl { .. }
            | StmtKind::ExternClassDecl { .. }
            | StmtKind::ExternGlobalDecl { .. }
    )
}

/// Returns the refusal for a declaration that a jump skips or repeats, which every statement
/// inside a dispatch range other than the ones holding its jumps and labels is.
fn refused_inside_dispatch(kind: &StmtKind) -> Option<Unsupported> {
    match kind {
        StmtKind::NamespaceDecl { .. } | StmtKind::NamespaceBlock { .. } => {
            Some(Unsupported::AcrossNamespace)
        }
        StmtKind::UseDecl { .. } => Some(Unsupported::AcrossUse),
        StmtKind::ConstDecl { .. } => Some(Unsupported::AcrossConst),
        _ => None,
    }
}
