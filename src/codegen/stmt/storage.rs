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
    emitter.adrp("x9", &format!("{}", data_label));              // load page of static variable storage
    emitter.add_lo12("x9", "x9", &format!("{}", data_label));        // resolve static storage address
    if matches!(ty, PhpType::Str) {
        emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve incoming string value across old-slot release
        emitter.instruction("ldr x0, [x9]");                                    // load previous string pointer from static slot
        emitter.instruction("bl __rt_heap_free_safe");                          // release previous string storage before overwrite
        emitter.instruction("ldp x1, x2, [sp], #16");                           // restore incoming string value after release
        emitter.adrp("x9", &format!("{}", data_label));          // reload page of static variable storage after call clobbers scratch regs
        emitter.add_lo12("x9", "x9", &format!("{}", data_label));    // resolve static storage address again
    } else if ty.is_refcounted() {
        emitter.instruction("str x0, [sp, #-16]!");                             // preserve incoming heap pointer across old-slot decref
        emitter.instruction("ldr x0, [x9]");                                    // load previous heap pointer from static slot
        abi::emit_decref_if_refcounted(emitter, ty);
        emitter.instruction("ldr x0, [sp], #16");                               // restore incoming heap pointer for the store
        emitter.adrp("x9", &format!("{}", data_label));          // reload page of static variable storage after call clobbers scratch regs
        emitter.add_lo12("x9", "x9", &format!("{}", data_label));    // resolve static storage address again
    }
    match ty {
        PhpType::Bool | PhpType::Int => {
            emitter.instruction("str x0, [x9]");                                // store int/bool to static storage
        }
        PhpType::Float => {
            emitter.instruction("str d0, [x9]");                                // store float to static storage
        }
        PhpType::Str => {
            emitter.instruction("str x1, [x9]");                                // store string pointer to static storage
            emitter.instruction("str x2, [x9, #8]");                            // store string length to static storage
        }
        _ => {
            emitter.instruction("str x0, [x9]");                                // store heap/pointer value to static storage
        }
    }
}

pub(super) fn emit_global_store(
    emitter: &mut Emitter,
    _ctx: &mut Context,
    name: &str,
    ty: &PhpType,
) {
    let label = format!("_gvar_{}", name);
    emitter.comment(&format!("store to global ${}", name));
    emitter.adrp("x9", &format!("{}", label));                   // load page of global var storage
    emitter.add_lo12("x9", "x9", &format!("{}", label));             // add page offset
    if matches!(ty, PhpType::Str) {
        emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve incoming string value across old-slot release
        emitter.instruction("ldr x0, [x9]");                                    // load previous string pointer from global slot
        emitter.instruction("bl __rt_heap_free_safe");                          // release previous string storage before overwrite
        emitter.instruction("ldp x1, x2, [sp], #16");                           // restore incoming string value after release
        emitter.adrp("x9", &format!("{}", label));               // reload page of global var storage after call clobbers scratch regs
        emitter.add_lo12("x9", "x9", &format!("{}", label));         // resolve global storage address again
    } else if ty.is_refcounted() {
        emitter.instruction("str x0, [sp, #-16]!");                             // preserve incoming heap pointer across old-slot decref
        emitter.instruction("ldr x0, [x9]");                                    // load previous heap pointer from global slot
        abi::emit_decref_if_refcounted(emitter, ty);
        emitter.instruction("ldr x0, [sp], #16");                               // restore incoming heap pointer for the store
        emitter.adrp("x9", &format!("{}", label));               // reload page of global var storage after call clobbers scratch regs
        emitter.add_lo12("x9", "x9", &format!("{}", label));         // resolve global storage address again
    }
    match ty {
        PhpType::Bool | PhpType::Int => {
            emitter.instruction("str x0, [x9]");                                // store int/bool to global storage
        }
        PhpType::Float => {
            emitter.instruction("str d0, [x9]");                                // store float to global storage
        }
        PhpType::Str => {
            emitter.instruction("str x1, [x9]");                                // store string pointer to global storage
            emitter.instruction("str x2, [x9, #8]");                            // store string length to global storage
        }
        _ => {
            emitter.instruction("str x0, [x9]");                                // store value to global storage
        }
    }
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
    emitter.adrp("x9", &format!("{}", label));                   // load page of global var storage
    emitter.add_lo12("x9", "x9", &format!("{}", label));             // add page offset
    match ty {
        PhpType::Bool | PhpType::Int => {
            emitter.instruction("ldr x0, [x9]");                                // load int/bool from global storage
        }
        PhpType::Float => {
            emitter.instruction("ldr d0, [x9]");                                // load float from global storage
        }
        PhpType::Str => {
            emitter.instruction("ldr x1, [x9]");                                // load string pointer from global storage
            emitter.instruction("ldr x2, [x9, #8]");                            // load string length from global storage
        }
        _ => {
            emitter.instruction("ldr x0, [x9]");                                // load value from global storage
        }
    }
}

pub(super) fn emit_extern_global_store(emitter: &mut Emitter, name: &str, ty: &PhpType) {
    emitter.comment(&format!("store to extern global ${}", name));
    let sym = match emitter.platform {
        crate::codegen::platform::Platform::MacOS => format!("_{}", name),
        crate::codegen::platform::Platform::Linux => name.to_string(),
    };
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
    let sym = match emitter.platform {
        crate::codegen::platform::Platform::MacOS => format!("_{}", name),
        crate::codegen::platform::Platform::Linux => name.to_string(),
    };
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
