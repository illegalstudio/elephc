//! Purpose:
//! Defines EIR basic blocks, block identifiers, switch cases, and terminators.
//!
//! Called from:
//! - `crate::ir::function`, `crate::ir::builder`, `crate::ir::validator`, and
//!   the textual printer.
//!
//! Key details:
//! - Blocks have explicit parameters instead of phi nodes. A block is invalid
//!   until it has exactly one terminator.

use crate::ir::instr::InstId;
use crate::ir::module::DataId;
use crate::ir::value::ValueId;

/// Function-local identifier for a basic block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockId(u32);

impl BlockId {
    /// Creates a block identifier from its raw zero-based table index.
    pub fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the raw zero-based table index represented by this identifier.
    pub fn as_raw(self) -> u32 {
        self.0
    }
}

/// Basic block with block parameters, instruction references, and a terminator.
#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub id: BlockId,
    pub name: String,
    pub params: Vec<ValueId>,
    pub instructions: Vec<InstId>,
    pub terminator: Option<Terminator>,
}

impl BasicBlock {
    /// Creates an unterminated block with the given identifier, name, and parameters.
    pub fn new(id: BlockId, name: String, params: Vec<ValueId>) -> Self {
        Self {
            id,
            name,
            params,
            instructions: Vec::new(),
            terminator: None,
        }
    }
}

/// One switch case edge with its destination arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitchCase {
    pub value: i64,
    pub target: BlockId,
    pub args: Vec<ValueId>,
}

/// Control-flow terminator for an EIR basic block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Terminator {
    Br {
        target: BlockId,
        args: Vec<ValueId>,
    },
    CondBr {
        cond: ValueId,
        then_target: BlockId,
        then_args: Vec<ValueId>,
        else_target: BlockId,
        else_args: Vec<ValueId>,
    },
    Switch {
        scrutinee: ValueId,
        cases: Vec<SwitchCase>,
        default: BlockId,
        default_args: Vec<ValueId>,
    },
    Return {
        value: Option<ValueId>,
    },
    Throw {
        value: ValueId,
    },
    Fatal {
        message: DataId,
    },
    GeneratorSuspend {
        key: Option<ValueId>,
        value: Option<ValueId>,
        resume: BlockId,
        resume_args: Vec<ValueId>,
    },
    Unreachable,
}

impl Terminator {
    /// Returns all value uses, including branch arguments and suspension operands.
    pub(crate) fn used_values(&self) -> Vec<ValueId> {
        match self {
            Terminator::Br { args, .. } => args.clone(),
            Terminator::CondBr {
                cond,
                then_args,
                else_args,
                ..
            } => {
                let mut uses = vec![*cond];
                uses.extend(then_args.iter().copied());
                uses.extend(else_args.iter().copied());
                uses
            }
            Terminator::Switch {
                scrutinee,
                cases,
                default_args,
                ..
            } => {
                let mut uses = vec![*scrutinee];
                for case in cases {
                    uses.extend(case.args.iter().copied());
                }
                uses.extend(default_args.iter().copied());
                uses
            }
            Terminator::Return { value } => value.iter().copied().collect(),
            Terminator::Throw { value } => vec![*value],
            Terminator::GeneratorSuspend {
                key,
                value,
                resume_args,
                ..
            } => {
                let mut uses: Vec<ValueId> = key.iter().copied().collect();
                uses.extend(value.iter().copied());
                uses.extend(resume_args.iter().copied());
                uses
            }
            Terminator::Fatal { .. } | Terminator::Unreachable => Vec::new(),
        }
    }
}
