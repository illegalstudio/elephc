//! Purpose:
//! Shared array metadata emitters used by EIR codegen, runtime helpers, and legacy support.
//! Keeps packed array header stamping outside AST expression lowering.
//!
//! Called from:
//! - `crate::codegen` EIR lowerers and `crate::codegen_support` helper emitters.
//!
//! Key details:
//! - The packed kind word layout must match runtime array allocation and COW metadata.

use crate::codegen_support::emit::Emitter;
use crate::types::PhpType;
pub(crate) use elephc_memoir::values::array_header::emit_array_value_type_inherit;

/// Writes the runtime value_type tag into the array header's packed kind word.
pub(crate) fn emit_array_value_type_stamp(
    emitter: &mut Emitter,
    array_reg: &str,
    elem_ty: &PhpType,
) {
    let value_type_tag = match elem_ty {
        PhpType::Float => 2,
        PhpType::Bool => 3,
        PhpType::Str => 1,
        PhpType::Array(_) => 4,
        PhpType::AssocArray { .. } => 5,
        PhpType::Object(_) => 6,
        PhpType::Mixed => 7,
        PhpType::Union(_) => 7,
        PhpType::Void => 8,
        PhpType::Callable => 10,
        // A `?ptr<T>` word: dynamic readers show 0 as PHP null and any other word as an integer.
        PhpType::Pointer(_) => elephc_memoir::layout::POINTER_ARRAY_VALUE_TYPE as u8,
        _ => return,
    };
    elephc_memoir::values::array_header::emit_array_value_type_stamp(
        emitter, array_reg, elephc_memoir::ValueTag::new(value_type_tag),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::abi;
    use crate::codegen_support::platform::Target;

    /// Packed callable arrays must advertise descriptors to boxed reads and deep cleanup on every ABI.
    #[test]
    fn callable_array_headers_use_the_descriptor_value_tag_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            let result = abi::int_result_reg(&emitter);
            emit_array_value_type_stamp(&mut emitter, result, &PhpType::Callable);
            let assembly = emitter.output();
            let expected = if name == "linux-x86_64" { "mov r12, 10" } else { "mov x11, #10" };
            assert!(assembly.contains(expected), "{name}: {assembly}");
        }
    }
}
