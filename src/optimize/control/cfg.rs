use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BasicBlockSuccessor {
    Block(usize),
    FallsThrough,
    Breaks,
    Exits,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BasicBlock {
    pub(crate) successors: Vec<BasicBlockSuccessor>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SwitchCfg {
    pub(crate) case_entries: Vec<usize>,
    pub(crate) default_entry: Option<usize>,
    pub(crate) blocks: Vec<BasicBlock>,
}

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

pub(crate) fn classify_switch_cfg_paths(cfg: &SwitchCfg) -> Vec<BasicBlockSuccessor> {
    cfg.case_entries
        .iter()
        .map(|&entry| classify_cfg_successor(&cfg.blocks, BasicBlockSuccessor::Block(entry)))
        .collect()
}

pub(crate) fn classify_cfg_successor(
    blocks: &[BasicBlock],
    successor: BasicBlockSuccessor,
) -> BasicBlockSuccessor {
    classify_cfg_successor_with_visited(blocks, successor, &mut Vec::new())
}

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
