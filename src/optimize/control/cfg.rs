//! Purpose:
//! Implements optimizer control-flow cfg logic.
//! Supports normalization, reachability, path analysis, and structural rewrites used by pruning and DCE.
//!
//! Called from:
//! - `crate::optimize::control`
//!
//! Key details:
//! - Control-flow helpers must treat terminal effects, switch fallthrough, and exception paths conservatively.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Represents the possible successors of a basic block in the control-flow graph.
/// - `Block(n)`: Control flows to block index `n`
/// - `FallsThrough`: Control flows to the next sequential block
/// - `Breaks`: Control exits a loop or switch (used for `break`)
/// - `Exits`: Control exits the current function or program (used for `return`, `exit`)
/// - `Unknown`: Control flow cannot be statically determined
pub(crate) enum BasicBlockSuccessor {
    Block(usize),
    FallsThrough,
    Breaks,
    Exits,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// A single basic block in the control-flow graph.
/// Each block contains a list of possible successors representing where control
/// can flow after this block completes.
pub(crate) struct BasicBlock {
    pub(crate) successors: Vec<BasicBlockSuccessor>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Control-flow graph for a PHP `switch` statement.
/// - `case_entries`: Block indices for each case's condition body
/// - `default_entry`: Block index of the default case, if present
/// - `blocks`: All basic blocks in the switch CFG (case bodies + default)
pub(crate) struct SwitchCfg {
    pub(crate) case_entries: Vec<usize>,
    pub(crate) default_entry: Option<usize>,
    pub(crate) blocks: Vec<BasicBlock>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Control-flow graph for a PHP `if`/`elseif`/`else` statement.
/// - `body_entries`: Block indices for each branch's body (then, elseif bodies)
/// - `else_entry`: Block index of the else body, if present
/// - `implicit_else_successor`: Successor when no explicit else branch exists
/// - `blocks`: All basic blocks in the if CFG (condition blocks + branch bodies + else)
pub(crate) struct IfCfg {
    pub(crate) body_entries: Vec<usize>,
    pub(crate) else_entry: Option<usize>,
    pub(crate) implicit_else_successor: BasicBlockSuccessor,
    pub(crate) blocks: Vec<BasicBlock>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Control-flow graph for a PHP `try`/`catch`/`finally` statement.
/// - `try_entry`: Block index of the try body
/// - `catch_entries`: Block indices for each catch clause body
/// - `finally_entry`: Block index of the finally body, if present
/// - `blocks`: All basic blocks (try body + catch bodies + finally)
pub(crate) struct TryCfg {
    pub(crate) try_entry: usize,
    pub(crate) catch_entries: Vec<usize>,
    pub(crate) finally_entry: Option<usize>,
    pub(crate) blocks: Vec<BasicBlock>,
}

/// Constructs a control-flow graph for a PHP `if`/`elseif`/`else` statement.
/// Takes the then-body, elseif clauses (condition + body pairs), and optional else-body.
/// Returns an `IfCfg` with condition blocks and branch body blocks, each with successors
/// determined by `successor_for_effect` applied to the block's terminal effect.
pub(crate) fn build_if_cfg(
    then_body: &[Stmt],
    elseif_clauses: &[(Expr, Vec<Stmt>)],
    else_body: &Option<Vec<Stmt>>,
) -> IfCfg {
    let branch_bodies: Vec<&[Stmt]> = std::iter::once(then_body)
        .chain(elseif_clauses.iter().map(|(_, body)| body.as_slice()))
        .collect();
    let branch_count = branch_bodies.len();
    let condition_count = branch_count;
    let body_entries: Vec<usize> = (condition_count..condition_count + branch_count).collect();
    let else_entry = else_body.as_ref().map(|_| condition_count + branch_count);
    let implicit_else_successor = if else_entry.is_some() {
        BasicBlockSuccessor::Unknown
    } else {
        BasicBlockSuccessor::FallsThrough
    };

    let mut blocks = Vec::with_capacity(condition_count + branch_count + usize::from(else_body.is_some()));

    for condition_index in 0..condition_count {
        let false_successor = if condition_index + 1 < condition_count {
            BasicBlockSuccessor::Block(condition_index + 1)
        } else if let Some(else_entry) = else_entry {
            BasicBlockSuccessor::Block(else_entry)
        } else {
            BasicBlockSuccessor::FallsThrough
        };
        blocks.push(BasicBlock {
            successors: vec![
                BasicBlockSuccessor::Block(body_entries[condition_index]),
                false_successor,
            ],
        });
    }

    for body in branch_bodies {
        blocks.push(BasicBlock {
            successors: vec![successor_for_effect(
                block_terminal_effect(body),
                BasicBlockSuccessor::FallsThrough,
            )],
        });
    }

    if let Some(else_body) = else_body.as_ref() {
        blocks.push(BasicBlock {
            successors: vec![successor_for_effect(
                block_terminal_effect(else_body),
                BasicBlockSuccessor::FallsThrough,
            )],
        });
    }

    IfCfg {
        body_entries,
        else_entry,
        implicit_else_successor,
        blocks,
    }
}

/// Classifies the paths through each branch body of an if CFG.
/// For each body entry, traces its successors to determine whether the branch
/// ultimately falls through, breaks, exits, or has unknown control flow.
/// Returns a vector of `BasicBlockSuccessor` values, one per branch body.
pub(crate) fn classify_if_cfg_paths(cfg: &IfCfg) -> Vec<BasicBlockSuccessor> {
    cfg.body_entries
        .iter()
        .map(|&entry| classify_cfg_successor(&cfg.blocks, BasicBlockSuccessor::Block(entry)))
        .collect()
}

/// Constructs a control-flow graph for a PHP `try`/`catch`/`finally` statement.
/// Takes the try body, catch clauses, and optional finally body.
/// Returns a `TryCfg` with blocks for try, each catch, and optionally finally,
/// with successors routing to the next catch or finally via `successor_for_effect`.
pub(crate) fn build_try_cfg(
    try_body: &[Stmt],
    catches: &[crate::parser::ast::CatchClause],
    finally_body: &Option<Vec<Stmt>>,
) -> TryCfg {
    let try_entry = 0;
    let catch_entries: Vec<usize> = (1..=catches.len()).collect();
    let finally_entry = finally_body.as_ref().map(|_| catches.len() + 1);
    let mut blocks = Vec::with_capacity(1 + catches.len() + usize::from(finally_body.is_some()));

    let tail_successor = if let Some(finally_entry) = finally_entry {
        BasicBlockSuccessor::Block(finally_entry)
    } else {
        BasicBlockSuccessor::FallsThrough
    };

    blocks.push(BasicBlock {
        successors: vec![successor_for_effect(block_terminal_effect(try_body), tail_successor)],
    });

    for catch in catches {
        blocks.push(BasicBlock {
            successors: vec![successor_for_effect(
                block_terminal_effect(&catch.body),
                tail_successor,
            )],
        });
    }

    if let Some(finally_body) = finally_body.as_ref() {
        blocks.push(BasicBlock {
            successors: vec![successor_for_effect(
                block_terminal_effect(finally_body),
                BasicBlockSuccessor::FallsThrough,
            )],
        });
    }

    TryCfg {
        try_entry,
        catch_entries,
        finally_entry,
        blocks,
    }
}

/// Classifies the paths through the try, catch, and optional finally blocks.
/// Traces each entry point (try and each catch) through the CFG to determine
/// whether it ultimately falls through, breaks, exits, or has unknown control flow.
pub(crate) fn classify_try_cfg_paths(cfg: &TryCfg) -> Vec<BasicBlockSuccessor> {
    std::iter::once(cfg.try_entry)
        .chain(cfg.catch_entries.iter().copied())
        .map(|entry| classify_cfg_successor(&cfg.blocks, BasicBlockSuccessor::Block(entry)))
        .collect()
}

/// Constructs a control-flow graph for a PHP `switch` statement.
/// Takes the cases (each with match expressions and body statements) and optional default body.
/// Returns a `SwitchCfg` with blocks for each case body and optionally the default,
/// with fallthrough successors determined by case order and `successor_for_effect`.
pub(crate) fn build_switch_cfg(
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: &Option<Vec<Stmt>>,
) -> SwitchCfg {
    let default_entry = default.as_ref().map(|_| cases.len());
    let mut blocks = Vec::with_capacity(cases.len() + usize::from(default.is_some()));

    for (index, (_, body)) in cases.iter().enumerate() {
        let next_successor = if index + 1 < cases.len() {
            BasicBlockSuccessor::Block(index + 1)
        } else if let Some(default_entry) = default_entry {
            BasicBlockSuccessor::Block(default_entry)
        } else {
            BasicBlockSuccessor::FallsThrough
        };
        blocks.push(BasicBlock {
            successors: vec![successor_for_effect(block_terminal_effect(body), next_successor)],
        });
    }

    if let Some(default_body) = default.as_ref() {
        blocks.push(BasicBlock {
            successors: vec![successor_for_effect(
                block_terminal_effect(default_body),
                BasicBlockSuccessor::FallsThrough,
            )],
        });
    }

    SwitchCfg {
        case_entries: (0..cases.len()).collect(),
        default_entry,
        blocks,
    }
}

/// Classifies the paths through each case body of a switch CFG.
/// For each case entry, traces its successors to determine whether the case
/// ultimately falls through, breaks, exits, or has unknown control flow.
pub(crate) fn classify_switch_cfg_paths(cfg: &SwitchCfg) -> Vec<BasicBlockSuccessor> {
    cfg.case_entries
        .iter()
        .map(|&entry| classify_cfg_successor(&cfg.blocks, BasicBlockSuccessor::Block(entry)))
        .collect()
}

/// Classifies the ultimate successor of a given immediate successor by tracing through
/// the CFG until a terminal or non-Block successor is reached. Uses `classify_cfg_successor_with_visited`
/// internally with a fresh empty visited set; returns `Unknown` if a cycle is detected.
pub(crate) fn classify_cfg_successor(
    blocks: &[BasicBlock],
    successor: BasicBlockSuccessor,
) -> BasicBlockSuccessor {
    classify_cfg_successor_with_visited(blocks, successor, &mut Vec::new())
}

/// Computes the set of reachable basic blocks from the given entry block indices.
/// Uses an iterative worklist algorithm: starts with entry blocks, then propagates
/// reachability through each block's successors. Returns a vector of booleans where
/// `reachable[n]` is true iff block `n` is reachable from some entry block.
pub(crate) fn collect_reachable_cfg_blocks(blocks: &[BasicBlock], entry_blocks: &[usize]) -> Vec<bool> {
    let mut reachable = vec![false; blocks.len()];
    let mut stack: Vec<usize> = entry_blocks
        .iter()
        .copied()
        .filter(|entry| *entry < blocks.len())
        .collect();

    while let Some(index) = stack.pop() {
        if reachable[index] {
            continue;
        }
        reachable[index] = true;

        for successor in &blocks[index].successors {
            if let BasicBlockSuccessor::Block(next) = successor {
                if *next < blocks.len() {
                    stack.push(*next);
                }
            }
        }
    }

    reachable
}

/// Recursive helper for `classify_cfg_successor`. Tracks visited block indices to detect cycles;
/// returns `Unknown` if a cycle is encountered. For `Block(index)` successors, recursively
/// merges the successors of that block. Non-Block successors (FallsThrough, Breaks, Exits, Unknown)
/// are returned as-is once encountered.
fn classify_cfg_successor_with_visited(
    blocks: &[BasicBlock],
    successor: BasicBlockSuccessor,
    visited: &mut Vec<usize>,
) -> BasicBlockSuccessor {
    match successor {
        BasicBlockSuccessor::Block(index) => {
            if visited.contains(&index) {
                return BasicBlockSuccessor::Unknown;
            }
            visited.push(index);
            let merged = merge_cfg_successors(
                blocks[index]
                    .successors
                    .iter()
                    .copied()
                    .map(|successor| classify_cfg_successor_with_visited(blocks, successor, visited)),
            );
            visited.pop();
            merged
        }
        terminal => terminal,
    }
}

/// Merges a sequence of `BasicBlockSuccessor` values into a single successor.
/// If all successors are identical and non-Block, returns that successor;
/// otherwise returns `Unknown`. Block successors are treated as `Unknown` since
/// they represent dynamic control flow targets.
fn merge_cfg_successors(successors: impl Iterator<Item = BasicBlockSuccessor>) -> BasicBlockSuccessor {
    let mut merged: Option<BasicBlockSuccessor> = None;
    for successor in successors {
        let successor = match successor {
            BasicBlockSuccessor::Block(_) => BasicBlockSuccessor::Unknown,
            terminal => terminal,
        };
        if let Some(current) = merged {
            if current != successor {
                return BasicBlockSuccessor::Unknown;
            }
        } else {
            merged = Some(successor);
        }
    }
    merged.unwrap_or(BasicBlockSuccessor::Unknown)
}

/// Converts a `TerminalEffect` into a `BasicBlockSuccessor`.
/// - `FallsThrough` maps to the provided `fallthrough_successor`
/// - `Breaks` maps to `BasicBlockSuccessor::Breaks`
/// - `ExitsCurrentBlock` maps to `BasicBlockSuccessor::Exits`
/// - `TerminatesMixed` maps to `BasicBlockSuccessor::Unknown` (conservative; control flow is indeterminate)
fn successor_for_effect(
    effect: TerminalEffect,
    fallthrough_successor: BasicBlockSuccessor,
) -> BasicBlockSuccessor {
    match effect {
        TerminalEffect::FallsThrough => fallthrough_successor,
        TerminalEffect::Breaks => BasicBlockSuccessor::Breaks,
        TerminalEffect::ExitsCurrentBlock => BasicBlockSuccessor::Exits,
        TerminalEffect::TerminatesMixed => BasicBlockSuccessor::Unknown,
    }
}
