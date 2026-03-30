use crate::codegen::emit::Emitter;
use crate::types::PhpType;

pub(crate) fn emit_ensure_unique_arg(emitter: &mut Emitter, ty: &PhpType) {
    match ty {
        PhpType::Array(_) => {
            emitter.instruction("bl __rt_array_ensure_unique");                 // split shared indexed arrays before a mutating builtin runs
        }
        PhpType::AssocArray { .. } => {
            emitter.instruction("bl __rt_hash_ensure_unique");                  // split shared associative arrays before a mutating builtin runs
        }
        _ => {}
    }
}
