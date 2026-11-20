//! Purpose:
//! Analyses the jumps of one label scope: scans its statement lists, resolves every `goto` to
//! its label, enforces PHP's jump rules, and plans which statement lists become dispatch loops
//! and which labels each of them routes.
//!
//! Called from:
//! - `crate::goto_lowering::GotoLowering::lower_scope()`.
//!
//! Key details:
//! - A label is "routed" by every block from the outermost LCA of its jumps down to the block
//!   that holds it. Each routing block gets a dispatch loop; the ones below the top are
//!   entered in resume mode and must skip everything before the label.
//! - A block that is not entered in resume mode gets one loop per group of jumps whose
//!   statement spans touch. Independent jumps therefore never share a loop, and a declaration
//!   written between them is not swept into one.
//! - Entering a block on the way down to a label is where PHP's rules apply: a loop, a `switch`
//!   or a `finally` body may not be entered, and neither may the two shapes listed in
//!   `plan::Unsupported`.

use std::collections::HashMap;

use crate::errors::CompileError;
use crate::parser::ast::{Stmt, StmtKind};
use crate::span::Span;

use super::plan::{
    child_blocks, child_blocks_mut, Block, BlockId, Dispatch, Jump, Label, LoopRange, Plan,
    Unsupported, Via,
};

/// Splits `try { T } catch … finally { F }` into `try { try { T } catch … } finally { F }`
/// wherever `T` or a `catch` body holds a label.
///
/// PHP does not run `finally` for a jump that stays inside the statement, such as a `catch`
/// that jumps back to a label in its `try` body. Lowered as written, that jump would leave the
/// `try` statement to reach the dispatch loop around it and run `finally` on the way. With the
/// handlers nested, the same jump stays inside the outer `try` body and `finally` still runs
/// exactly once, when control really leaves. The two spellings are equivalent in PHP.
pub(super) fn isolate_finally_of_labelled_tries(body: Vec<Stmt>) -> Vec<Stmt> {
    body.into_iter()
        .map(|mut stmt| {
            for child in child_blocks_mut(&mut stmt.kind) {
                *child = isolate_finally_of_labelled_tries(std::mem::take(child));
            }
            let StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } = &stmt.kind
            else {
                return stmt;
            };
            let holds_label = holds_label(try_body) || catches.iter().any(|c| holds_label(&c.body));
            if catches.is_empty() || finally_body.is_none() || !holds_label {
                return stmt;
            }
            let StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } = stmt.kind
            else {
                unreachable!("matched as a try statement above");
            };
            let inner = Stmt {
                kind: StmtKind::Try {
                    try_body,
                    catches,
                    finally_body: None,
                },
                span: stmt.span,
                source_mode: stmt.source_mode,
                strict_types: stmt.strict_types,
                attributes: Vec::new(),
            };
            Stmt {
                kind: StmtKind::Try {
                    try_body: vec![inner],
                    catches: Vec::new(),
                    finally_body,
                },
                span: Span::synthetic(),
                source_mode: stmt.source_mode,
                strict_types: stmt.strict_types,
                attributes: stmt.attributes,
            }
        })
        .collect()
}

/// Returns whether `stmts` holds a label of this scope at any statement depth.
fn holds_label(stmts: &[Stmt]) -> bool {
    stmts.iter().any(|stmt| match &stmt.kind {
        StmtKind::GotoLabel(_) => true,
        kind => child_blocks(kind).into_iter().any(|(_, body)| holds_label(body)),
    })
}

/// A `goto` found by the scan, before it is resolved.
struct RawJump {
    target: String,
    block: BlockId,
    index: usize,
    span: Span,
}

/// Scan state: the block tree and the labels and jumps found so far.
#[derive(Default)]
struct Scan {
    plan: Plan,
    names: HashMap<String, usize>,
    jumps: Vec<RawJump>,
    islands: usize,
    errors: Vec<CompileError>,
}

impl Scan {
    /// Records one block and returns its id.
    fn add_block(&mut self, parent: Option<(BlockId, usize, usize)>, via: Via) -> BlockId {
        let id = self.plan.blocks.len();
        let (parent_id, stmt_index, depth, parent_island) = match parent {
            Some((parent_id, stmt_index, slot)) => {
                self.plan.children.insert((parent_id, stmt_index, slot), id);
                let parent = &self.plan.blocks[parent_id];
                (Some(parent_id), stmt_index, parent.depth + 1, parent.island)
            }
            None => (None, 0, 0, 0),
        };
        let island = if via == Via::Finally {
            self.islands += 1;
            self.islands
        } else {
            parent_island
        };
        self.plan.blocks.push(Block {
            parent: parent_id,
            via,
            stmt_index,
            depth,
            island,
            dispatch: None,
            incoming: Vec::new(),
        });
        id
    }

    /// Records the labels and jumps of one statement list and scans the lists below it.
    fn scan_block(&mut self, block: BlockId, stmts: &[Stmt]) {
        for (index, stmt) in stmts.iter().enumerate() {
            match &stmt.kind {
                StmtKind::GotoLabel(name) => {
                    if self.names.contains_key(name) {
                        self.errors.push(CompileError::new(
                            stmt.span,
                            &format!("Label '{}' already defined", name),
                        ));
                        continue;
                    }
                    let label = self.plan.labels.len();
                    self.names.insert(name.clone(), label);
                    self.plan.label_at.insert((block, index), label);
                    self.plan.labels.push(Label {
                        id: label as i64 + 1,
                        block,
                        index,
                        top: None,
                        uses_state: true,
                    });
                }
                StmtKind::Goto(target) => self.jumps.push(RawJump {
                    target: target.clone(),
                    block,
                    index,
                    span: stmt.span,
                }),
                kind => {
                    for (slot, (via, body)) in child_blocks(kind).into_iter().enumerate() {
                        let child = self.add_block(Some((block, index, slot)), via);
                        self.scan_block(child, body);
                    }
                }
            }
        }
    }
}

/// Analyses one label scope and plans its dispatch loops, or reports every invalid jump.
pub(super) fn plan_scope(body: &[Stmt]) -> Result<Plan, Vec<CompileError>> {
    let mut scan = Scan::default();
    let root = scan.add_block(None, Via::Root);
    scan.scan_block(root, body);
    let Scan {
        mut plan,
        names,
        jumps,
        mut errors,
        ..
    } = scan;

    for jump in jumps {
        let Some(&label) = names.get(&jump.target) else {
            errors.push(CompileError::new(
                jump.span,
                &format!("'goto' to undefined label '{}'", jump.target),
            ));
            continue;
        };
        match resolve_jump(&plan, jump.block, plan.labels[label].block) {
            Ok(resolved) => {
                let site = resolved.goto_side.map_or(jump.index, |child| plan.blocks[child].stmt_index);
                let entry = resolved
                    .label_side
                    .map_or(plan.labels[label].index, |child| plan.blocks[child].stmt_index);
                plan.crossings
                    .entry(resolved.lca)
                    .or_default()
                    .push((site.min(entry), site.max(entry)));
                let top = &mut plan.labels[label].top;
                let lca_depth = plan.blocks[resolved.lca].depth;
                if top.is_none_or(|top| lca_depth < plan.blocks[top].depth) {
                    *top = Some(resolved.lca);
                }
                plan.jump_at.insert(
                    (jump.block, jump.index),
                    Jump {
                        label,
                        lca: resolved.lca,
                    },
                );
            }
            Err(message) => errors.push(CompileError::new(jump.span, message)),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    route_labels(&mut plan);
    finish_dispatches(&mut plan);
    Ok(plan)
}

/// Where a valid jump executes.
struct ResolvedJump {
    /// Lowest block holding both the `goto` and the label.
    lca: BlockId,
    /// The child of `lca` that holds the `goto`, `None` when the `goto` is a statement of
    /// `lca` itself.
    goto_side: Option<BlockId>,
    /// The child of `lca` that holds the label, `None` when the label is a statement of
    /// `lca` itself.
    label_side: Option<BlockId>,
}

/// Finds the block that executes a jump and checks PHP's rules on the way, returning the
/// diagnostic text for a jump PHP or this compiler refuses.
fn resolve_jump(plan: &Plan, from: BlockId, to: BlockId) -> Result<ResolvedJump, &'static str> {
    let blocks = &plan.blocks;
    // Climb both sides to their lowest common ancestor, remembering the blocks left behind.
    let mut goto_path = Vec::new();
    let mut label_path = Vec::new();
    let (mut a, mut b) = (from, to);
    while a != b {
        if blocks[a].depth >= blocks[b].depth {
            goto_path.push(a);
            a = blocks[a].parent.expect("a deeper block has a parent");
        } else {
            label_path.push(b);
            b = blocks[b].parent.expect("a deeper block has a parent");
        }
    }
    let lca = a;
    let goto_side = goto_path.last().copied();
    let label_side = label_path.last().copied();

    // Entering blocks on the label's side, outermost first, like PHP's own checks.
    let mut crosses_namespace_block = false;
    for &entered in label_path.iter().rev() {
        match blocks[entered].via {
            Via::Loop => return Err("'goto' into loop or switch statement is disallowed"),
            Via::SwitchCase => {
                // Both sides inside one `switch` is legal PHP; from outside it is not.
                let same_switch = Some(entered) == label_side
                    && goto_side.is_some_and(|side| {
                        blocks[side].via == Via::SwitchCase
                            && blocks[side].stmt_index == blocks[entered].stmt_index
                    });
                return Err(if same_switch {
                    Unsupported::AcrossSwitchCases.message()
                } else {
                    "'goto' into loop or switch statement is disallowed"
                });
            }
            Via::Finally => return Err("jump into a finally block is disallowed"),
            Via::Catch => return Err(Unsupported::IntoCatch.message()),
            Via::IfDef => return Err(Unsupported::IntoIfDef.message()),
            Via::Namespace => crosses_namespace_block = true,
            Via::Enterable => {}
            Via::Root => unreachable!("the scope body is never below another block"),
        }
    }
    if goto_path.iter().any(|&left| blocks[left].via == Via::Finally) {
        return Err("jump out of a finally block is disallowed");
    }
    // PHP's own rules first, then this compiler's limit: the label is in the file's scope, but
    // the jump would have to enter or leave a braced `namespace` block.
    if crosses_namespace_block || goto_path.iter().any(|&left| blocks[left].via == Via::Namespace) {
        return Err(Unsupported::AcrossNamespace.message());
    }
    Ok(ResolvedJump {
        lca,
        goto_side,
        label_side,
    })
}

/// Records, on every block between a label's top router and the label, the entry it adds.
fn route_labels(plan: &mut Plan) {
    for index in 0..plan.labels.len() {
        let Some(top) = plan.labels[index].top else {
            continue;
        };
        let id = plan.labels[index].id;
        let label_block = plan.labels[index].block;
        let label_index = plan.labels[index].index;

        let mut block = label_block;
        let mut entry_index = label_index;
        loop {
            let dispatch = plan.blocks[block].dispatch.get_or_insert_with(Dispatch::default);
            dispatch.entries.entry(entry_index).or_default().push(id);
            if block == top {
                break;
            }
            dispatch.entered_from_above = true;
            plan.blocks[block].incoming.push(id);
            entry_index = plan.blocks[block].stmt_index;
            block = plan.blocks[block].parent.expect("the top router is an ancestor");
        }
    }
}

/// Fixes the loops of each dispatch block once every entry and jump is known.
fn finish_dispatches(plan: &mut Plan) {
    for id in 0..plan.blocks.len() {
        let Some(dispatch) = plan.blocks[id].dispatch.as_ref() else {
            continue;
        };
        let entered_from_above = dispatch.entered_from_above;
        let spans = plan.crossings.get(&id).map(Vec::as_slice).unwrap_or_default();
        let ranges = if entered_from_above {
            // A resumed entry skips forward from the block's first statement to any entry.
            let last_entry = *dispatch.entries.keys().next_back().expect("a dispatch has an entry");
            vec![(0, spans.iter().fold(last_entry, |hi, span| hi.max(span.1)))]
        } else {
            jump_clusters(spans)
        };
        let mut loops = Vec::new();
        for (lo, hi) in ranges {
            let mut entries = dispatch.entries.range(lo..=hi).map(|(&index, _)| index);
            let single = entries.next() == Some(lo) && entries.next().is_none();
            // A loop whose one entry is its first statement has a single segment. When that
            // entry is a label only this block routes, nothing tests the state local and the
            // label's jumps are a bare `continue`.
            let direct_label = plan.label_at.get(&(id, lo)).copied().filter(|_| single);
            let reads_state = match direct_label {
                Some(label) if plan.labels[label].top == Some(id) => {
                    plan.labels[label].uses_state = false;
                    false
                }
                _ => true,
            };
            loops.push(LoopRange {
                lo,
                hi,
                needs_init: reads_state && !entered_from_above,
            });
        }
        plan.blocks[id].dispatch.as_mut().expect("checked above").loops = loops;
    }
}

/// Groups the statement spans of a block's jumps into its independent loop ranges.
///
/// Spans that overlap or share an end statement are one range. Every statement of a range that
/// is not an end of some span lies strictly inside one, so a jump skips or repeats it.
fn jump_clusters(spans: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut spans = spans.to_vec();
    spans.sort_unstable();
    let mut clusters: Vec<(usize, usize)> = Vec::new();
    for (lower, upper) in spans {
        match clusters.last_mut() {
            Some(last) if lower <= last.1 => last.1 = last.1.max(upper),
            _ => clusters.push((lower, upper)),
        }
    }
    clusters
}
