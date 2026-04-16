use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::types::PhpType;

pub(super) fn emit_extern_global_store(emitter: &mut Emitter, name: &str, ty: &PhpType) {
    emitter.comment(&format!("store to extern global ${}", name));
    let sym = emitter.target.extern_symbol(name);
    match ty {
        PhpType::Bool
        | PhpType::Int
        | PhpType::Pointer(_)
        | PhpType::Buffer(_)
        | PhpType::Packed(_)
        | PhpType::Callable => {
            abi::emit_store_reg_to_extern_symbol(emitter, abi::int_result_reg(emitter), &sym, 0); // store integer or pointer payload into extern global storage
        }
        PhpType::Float => {
            abi::emit_store_reg_to_extern_symbol(emitter, abi::float_result_reg(emitter), &sym, 0); // store floating-point payload into extern global storage
        }
        PhpType::Str => {
            abi::emit_call_label(emitter, "__rt_str_to_cstr");                 // allocate a null-terminated copy before publishing it to C-owned global storage
            abi::emit_store_reg_to_extern_symbol(emitter, abi::int_result_reg(emitter), &sym, 0); // store the returned char* into the extern global slot
        }
        PhpType::Void
        | PhpType::Mixed
        | PhpType::Union(_)
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Object(_) => {
            emitter.comment(&format!(
                "WARNING: unsupported extern global store for ${}",
                name
            ));
        }
    }
}

pub(super) fn emit_extern_global_load(emitter: &mut Emitter, name: &str, ty: &PhpType) {
    emitter.comment(&format!("load from extern global ${}", name));
    let sym = emitter.target.extern_symbol(name);
    match ty {
        PhpType::Bool
        | PhpType::Int
        | PhpType::Pointer(_)
        | PhpType::Buffer(_)
        | PhpType::Packed(_)
        | PhpType::Callable => {
            abi::emit_load_extern_symbol_to_reg(emitter, abi::int_result_reg(emitter), &sym, 0); // load integer or pointer payload from extern global storage
        }
        PhpType::Float => {
            abi::emit_load_extern_symbol_to_reg(emitter, abi::float_result_reg(emitter), &sym, 0); // load floating-point payload from extern global storage
        }
        PhpType::Str => {
            abi::emit_load_extern_symbol_to_reg(emitter, abi::int_result_reg(emitter), &sym, 0); // load the borrowed char* from extern global storage
            abi::emit_call_label(emitter, "__rt_cstr_to_str");                 // convert the borrowed C string into the elephc string result convention
        }
        PhpType::Void
        | PhpType::Mixed
        | PhpType::Union(_)
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Object(_) => {
            emitter.comment(&format!(
                "WARNING: unsupported extern global load for ${}",
                name
            ));
        }
    }
}
