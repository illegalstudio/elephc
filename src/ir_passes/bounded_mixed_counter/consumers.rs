//! Purpose:
//! Preserves PHP comparison dispatch when a proven boxed counter becomes an integer.
//!
//! Called from:
//! - The shared counter proof for storage replay and committed EIR copies.
//!
//! Key details:
//! - Safe integer ranges do not imply equivalent coercion or identity rules.
//! - Jointly proven scalar operands are checked after closing the complete counter graph.

use std::collections::HashSet;
use crate::ir::{Function, Instruction, Op, ValueId};
use crate::types::PhpType;

/// Requires every external comparison partner to retain the same PHP rule after scalarization.
pub(super) fn comparison_preserves_integer(function: &Function, inst: &Instruction, scalar_values: &HashSet<ValueId>) -> bool {
    inst.operands.iter().all(|value| {
        scalar_values.contains(value) || function.value(*value).is_some_and(|metadata| {
            let partner = metadata.php_type.codegen_repr();
            match inst.op {
                // Typed integer ordering with bool/null uses raw payloads, unlike boxed PHP ordering.
                Op::Spaceship => matches!(partner, PhpType::Int | PhpType::Float
                    | PhpType::Mixed | PhpType::TaggedScalar),
                // Mixed boxing turns raw address-like values into integers; typed identity differs.
                Op::StrictEq | Op::StrictNotEq => !matches!(partner, PhpType::Pointer(_)
                    | PhpType::Buffer(_) | PhpType::Packed(_)),
                // Int/Mixed numeric dispatch does not implement the full object comparison table.
                Op::LooseEq | Op::LooseNotEq => partner != PhpType::Mixed,
                _ => true,
            }
        })
    })
}
