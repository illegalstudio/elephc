//! Purpose:
//! Defines elephc IR (EIR), a CFG-based SSA-lite intermediate representation
//! used between AST-level optimization and assembly emission.
//!
//! Called from:
//! - Phase 03 AST-to-EIR lowering and Phase 04 EIR-to-ASM codegen.
//!
//! Key details:
//! - Block parameters replace SSA phi nodes; ownership is explicit; effects
//!   are immutable metadata assigned when instructions are built.

mod block;
mod builder;
mod effects;
mod function;
pub(crate) mod function_variants;
mod instr;
mod module;
mod print;
mod types;
mod validator;
mod value;

#[cfg(test)]
mod tests;

pub use block::{BasicBlock, BlockId, SwitchCase, Terminator};
pub use builder::Builder;
pub use effects::Effects;
pub use function::{
    Function, FunctionFlags, FunctionId, FunctionParam, GeneratorSource, LocalKind, LocalSlot,
    LocalSlotId,
};
pub use instr::{
    BuiltinId, CmpPredicate, Immediate, InstId, Instruction, MixedNumericOp, Op, RuntimeId,
};
pub use module::{
    ClassTable, DataId, DataPool, EnumTable, ExternDecl, ExternParamDecl, InterfaceTable,
    Module, PackedLayoutTable,
};
pub use print::{print_function, print_module};
pub use types::{IrHeapKind, IrType};
pub use validator::{validate_function, validate_module, ValidationError};
pub use function_variants::{
    collect_dispatch_groups, parse_variant_label, resolve_variant_callee,
    resolve_variant_callee_name, FunctionVariantLabel,
};
pub use value::{Ownership, Value, ValueDef, ValueId};
