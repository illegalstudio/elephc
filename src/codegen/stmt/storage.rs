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
    emitter.adrp_got("x9", &format!("{}", sym));                // load page of extern global GOT entry
    emitter.ldr_got_lo12("x9", "x9", &format!("{}", sym));        // resolve extern global address
    match ty {
        PhpType::Bool
        | PhpType::Int
        | PhpType::Pointer(_)
        | PhpType::Buffer(_)
        | PhpType::Packed(_)
        | PhpType::Callable => {
            emitter.instruction("str x0, [x9]");                                // store integer/pointer into extern global
        }
        PhpType::Float => {
            emitter.instruction("str d0, [x9]");                                // store float into extern global
        }
        PhpType::Str => {
            emitter.instruction("bl __rt_str_to_cstr");                         // allocate null-terminated copy for C global
            emitter.instruction("str x0, [x9]");                                // store char* into extern global
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
    emitter.adrp_got("x9", &format!("{}", sym));                // load page of extern global GOT entry
    emitter.ldr_got_lo12("x9", "x9", &format!("{}", sym));        // resolve extern global address
    match ty {
        PhpType::Bool
        | PhpType::Int
        | PhpType::Pointer(_)
        | PhpType::Buffer(_)
        | PhpType::Packed(_)
        | PhpType::Callable => {
            emitter.instruction("ldr x0, [x9]");                                // load integer/pointer from extern global
        }
        PhpType::Float => {
            emitter.instruction("ldr d0, [x9]");                                // load float from extern global
        }
        PhpType::Str => {
            emitter.instruction("ldr x0, [x9]");                                // load char* from extern global
            emitter.instruction("bl __rt_cstr_to_str");                         // convert C string to elephc string
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
