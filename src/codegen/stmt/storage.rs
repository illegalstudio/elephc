use super::super::abi;
use super::super::context::Context;
use super::super::emit::Emitter;
use super::{static_storage_label, PhpType};

pub(super) fn emit_static_store(
    emitter: &mut Emitter,
    ctx: &Context,
    name: &str,
    ty: &PhpType,
) {
    let data_label = static_storage_label(ctx, name);
    emitter.comment(&format!("store to static ${}", name));
    abi::emit_store_result_to_symbol(emitter, &data_label, ty, true);
}

pub(super) fn emit_global_store(
    emitter: &mut Emitter,
    _ctx: &mut Context,
    name: &str,
    ty: &PhpType,
) {
    let label = format!("_gvar_{}", name);
    emitter.comment(&format!("store to global ${}", name));
    abi::emit_store_result_to_symbol(emitter, &label, ty, true);
}

pub(super) fn emit_global_load(
    emitter: &mut Emitter,
    ctx: &mut Context,
    name: &str,
    ty: &PhpType,
) {
    if ctx.extern_globals.contains_key(name) {
        emit_extern_global_load(emitter, name, ty);
        return;
    }
    let label = format!("_gvar_{}", name);
    emitter.comment(&format!("load from global ${}", name));
    abi::emit_load_symbol_to_result(emitter, &label, ty);
}

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
