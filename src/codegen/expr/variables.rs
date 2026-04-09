use crate::codegen::platform::Arch;

use super::super::abi;
use super::super::context::{Context, HeapOwnership};
use super::super::data_section::DataSection;
use super::super::emit::Emitter;
use super::{expr_result_heap_ownership, Expr, PhpType};

pub(super) fn emit_variable(name: &str, emitter: &mut Emitter, ctx: &mut Context) -> PhpType {
    if let Some(ty) = ctx.extern_globals.get(name).cloned() {
        emitter.comment(&format!("load extern global ${}", name));
        match &ty {
            PhpType::Bool
            | PhpType::Int
            | PhpType::Pointer(_)
            | PhpType::Buffer(_)
            | PhpType::Packed(_)
            | PhpType::Callable => {
                let sym = emitter.target.extern_symbol(name);
                emitter.adrp_got("x9", &sym);                                       // load page of extern global GOT entry
                emitter.ldr_got_lo12("x9", "x9", &sym);                             // resolve extern global address
                emitter.instruction("ldr x0, [x9]");                                // load extern integer or pointer value
            }
            PhpType::Float => {
                let sym = emitter.target.extern_symbol(name);
                emitter.adrp_got("x9", &sym);                                       // load page of extern global GOT entry
                emitter.ldr_got_lo12("x9", "x9", &sym);                             // resolve extern global address
                emitter.instruction("ldr d0, [x9]");                                // load extern float value
            }
            PhpType::Str => {
                let sym = emitter.target.extern_symbol(name);
                emitter.adrp_got("x9", &sym);                                       // load page of extern global GOT entry
                emitter.ldr_got_lo12("x9", "x9", &sym);                             // resolve extern global address
                emitter.instruction("ldr x0, [x9]");                                // load char* from the extern global
                abi::emit_call_label(emitter, "__rt_cstr_to_str");                  // convert the C string into the elephc string result convention
            }
            PhpType::Void
            | PhpType::Mixed
            | PhpType::Union(_)
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Object(_) => {
                emitter.comment(&format!(
                    "WARNING: unsupported extern global type for ${}",
                    name
                ));
                return PhpType::Int;
            }
        }
        return ty;
    }

    if ctx.global_vars.contains(name) || (ctx.in_main && ctx.all_global_var_names.contains(name)) {
        let Some(var) = ctx.variables.get(name) else {
            emitter.comment(&format!("WARNING: undefined variable ${}", name));
            return PhpType::Int;
        };
        let ty = var.ty.clone();
        super::super::stmt::emit_global_load(emitter, ctx, name, &ty);
        return ty;
    }

    if ctx.ref_params.contains(name) {
        return emit_ref_variable(name, emitter, ctx);
    }

    let Some(var) = ctx.variables.get(name) else {
        emitter.comment(&format!("WARNING: undefined variable ${}", name));
        return PhpType::Int;
    };
    let offset = var.stack_offset;
    let ty = var.ty.clone();
    emitter.comment(&format!("load ${}", name));
    abi::emit_load(emitter, &ty, offset);
    ty
}

pub(super) fn emit_throw(
    inner: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let thrown_ty = super::emit_expr(inner, emitter, ctx, data);
    if thrown_ty.is_refcounted() && expr_result_heap_ownership(inner) != HeapOwnership::Owned {
        abi::emit_incref_if_refcounted(emitter, &thrown_ty);                        // retain borrowed heap values before publishing them as the active exception
    }
    abi::emit_store_reg_to_symbol(
        emitter,
        abi::int_result_reg(emitter),
        "_exc_value",
        0,
    );
    abi::emit_call_label(emitter, "__rt_throw_current");                            // unwind to the nearest active exception handler
    PhpType::Void
}

pub(super) fn emit_pre_increment(name: &str, emitter: &mut Emitter, ctx: &mut Context) -> PhpType {
    let Some(var) = ctx.variables.get(name) else {
        emitter.comment(&format!("WARNING: undefined variable ${}", name));
        return PhpType::Int;
    };

    if ctx.global_vars.contains(name) {
        let ty = var.ty.clone();
        emitter.comment(&format!("++${} (global)", name));
        super::super::stmt::emit_global_load(emitter, ctx, name, &ty);
        emit_add_one(emitter, abi::int_result_reg(emitter));
        emit_global_store_inline(emitter, name, abi::int_result_reg(emitter));
        return PhpType::Int;
    }

    if ctx.ref_params.contains(name) {
        let offset = var.stack_offset;
        let pointer_reg = abi::symbol_scratch_reg(emitter);
        emitter.comment(&format!("++${} (ref)", name));
        abi::load_at_offset(emitter, pointer_reg, offset);
        abi::emit_load_from_address(emitter, abi::int_result_reg(emitter), pointer_reg, 0);
        emit_add_one(emitter, abi::int_result_reg(emitter));
        abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), pointer_reg, 0);
        return PhpType::Int;
    }

    let offset = var.stack_offset;
    emitter.comment(&format!("++${}", name));
    abi::load_at_offset(emitter, abi::int_result_reg(emitter), offset);
    emit_add_one(emitter, abi::int_result_reg(emitter));
    abi::store_at_offset(emitter, abi::int_result_reg(emitter), offset);
    if ctx.in_main && ctx.all_global_var_names.contains(name) {
        emit_global_store_inline(emitter, name, abi::int_result_reg(emitter));
    }
    PhpType::Int
}

pub(super) fn emit_post_increment(
    name: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> PhpType {
    let Some(var) = ctx.variables.get(name) else {
        emitter.comment(&format!("WARNING: undefined variable ${}", name));
        return PhpType::Int;
    };

    let result_reg = abi::int_result_reg(emitter);
    let scratch_reg = abi::temp_int_reg(emitter.target);

    if ctx.global_vars.contains(name) {
        let ty = var.ty.clone();
        emitter.comment(&format!("${}++ (global)", name));
        super::super::stmt::emit_global_load(emitter, ctx, name, &ty);
        emit_copy_int_reg(emitter, scratch_reg, result_reg);
        emit_add_one(emitter, scratch_reg);
        emit_global_store_inline(emitter, name, scratch_reg);
        return PhpType::Int;
    }

    if ctx.ref_params.contains(name) {
        let offset = var.stack_offset;
        let pointer_reg = abi::symbol_scratch_reg(emitter);
        emitter.comment(&format!("${}++ (ref)", name));
        abi::load_at_offset(emitter, pointer_reg, offset);
        abi::emit_load_from_address(emitter, result_reg, pointer_reg, 0);
        emit_copy_int_reg(emitter, scratch_reg, result_reg);
        emit_add_one(emitter, scratch_reg);
        abi::emit_store_to_address(emitter, scratch_reg, pointer_reg, 0);
        return PhpType::Int;
    }

    let offset = var.stack_offset;
    emitter.comment(&format!("${}++", name));
    abi::load_at_offset(emitter, result_reg, offset);
    emit_copy_int_reg(emitter, scratch_reg, result_reg);
    emit_add_one(emitter, scratch_reg);
    abi::store_at_offset(emitter, scratch_reg, offset);
    if ctx.in_main && ctx.all_global_var_names.contains(name) {
        emit_global_store_inline(emitter, name, scratch_reg);
    }
    PhpType::Int
}

pub(super) fn emit_pre_decrement(name: &str, emitter: &mut Emitter, ctx: &mut Context) -> PhpType {
    let Some(var) = ctx.variables.get(name) else {
        emitter.comment(&format!("WARNING: undefined variable ${}", name));
        return PhpType::Int;
    };

    if ctx.global_vars.contains(name) {
        let ty = var.ty.clone();
        emitter.comment(&format!("--${} (global)", name));
        super::super::stmt::emit_global_load(emitter, ctx, name, &ty);
        emit_sub_one(emitter, abi::int_result_reg(emitter));
        emit_global_store_inline(emitter, name, abi::int_result_reg(emitter));
        return PhpType::Int;
    }

    if ctx.ref_params.contains(name) {
        let offset = var.stack_offset;
        let pointer_reg = abi::symbol_scratch_reg(emitter);
        emitter.comment(&format!("--${} (ref)", name));
        abi::load_at_offset(emitter, pointer_reg, offset);
        abi::emit_load_from_address(emitter, abi::int_result_reg(emitter), pointer_reg, 0);
        emit_sub_one(emitter, abi::int_result_reg(emitter));
        abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), pointer_reg, 0);
        return PhpType::Int;
    }

    let offset = var.stack_offset;
    emitter.comment(&format!("--${}", name));
    abi::load_at_offset(emitter, abi::int_result_reg(emitter), offset);
    emit_sub_one(emitter, abi::int_result_reg(emitter));
    abi::store_at_offset(emitter, abi::int_result_reg(emitter), offset);
    PhpType::Int
}

pub(super) fn emit_post_decrement(
    name: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> PhpType {
    let Some(var) = ctx.variables.get(name) else {
        emitter.comment(&format!("WARNING: undefined variable ${}", name));
        return PhpType::Int;
    };

    let result_reg = abi::int_result_reg(emitter);
    let scratch_reg = abi::temp_int_reg(emitter.target);

    if ctx.ref_params.contains(name) {
        let offset = var.stack_offset;
        let pointer_reg = abi::symbol_scratch_reg(emitter);
        emitter.comment(&format!("${}-- (ref)", name));
        abi::load_at_offset(emitter, pointer_reg, offset);
        abi::emit_load_from_address(emitter, result_reg, pointer_reg, 0);
        emit_copy_int_reg(emitter, scratch_reg, result_reg);
        emit_sub_one(emitter, scratch_reg);
        abi::emit_store_to_address(emitter, scratch_reg, pointer_reg, 0);
        return PhpType::Int;
    }

    let offset = var.stack_offset;
    emitter.comment(&format!("${}--", name));
    abi::load_at_offset(emitter, result_reg, offset);
    emit_copy_int_reg(emitter, scratch_reg, result_reg);
    emit_sub_one(emitter, scratch_reg);
    abi::store_at_offset(emitter, scratch_reg, offset);
    PhpType::Int
}

pub(super) fn emit_this(emitter: &mut Emitter, ctx: &mut Context) -> PhpType {
    emitter.comment("$this");
    let var = match ctx.variables.get("this") {
        Some(v) => v,
        None => {
            emitter.comment("WARNING: $this used outside class scope");
            return PhpType::Int;
        }
    };
    let offset = var.stack_offset;
    abi::load_at_offset(emitter, abi::int_result_reg(emitter), offset);
    let class_name = ctx.current_class.clone().unwrap_or_default();
    PhpType::Object(class_name)
}

fn emit_ref_variable(name: &str, emitter: &mut Emitter, ctx: &mut Context) -> PhpType {
    let Some(var) = ctx.variables.get(name) else {
        emitter.comment(&format!("WARNING: undefined variable ${}", name));
        return PhpType::Int;
    };
    let offset = var.stack_offset;
    let ty = var.ty.clone();
    let pointer_reg = abi::symbol_scratch_reg(emitter);
    emitter.comment(&format!("load ref ${}", name));
    abi::load_at_offset(emitter, pointer_reg, offset);
    match &ty {
        PhpType::Bool | PhpType::Int => {
            abi::emit_load_from_address(emitter, abi::int_result_reg(emitter), pointer_reg, 0);
        }
        PhpType::Float => {
            abi::emit_load_from_address(emitter, abi::float_result_reg(emitter), pointer_reg, 0);
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_load_from_address(emitter, ptr_reg, pointer_reg, 0);
            abi::emit_load_from_address(emitter, len_reg, pointer_reg, 8);
        }
        _ => {
            abi::emit_load_from_address(emitter, abi::int_result_reg(emitter), pointer_reg, 0);
        }
    }
    ty
}

fn emit_global_store_inline(emitter: &mut Emitter, name: &str, reg: &str) {
    let label = format!("_gvar_{}", name);
    abi::emit_store_reg_to_symbol(emitter, reg, &label, 0);
}

fn emit_copy_int_reg(emitter: &mut Emitter, dst: &str, src: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov {}, {}", dst, src));                    // copy the integer result into a scratch register before mutating it
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {}, {}", dst, src));                    // copy the integer result into a scratch register before mutating it
        }
    }
}

fn emit_add_one(emitter: &mut Emitter, reg: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("add {}, {}, #1", reg, reg));                // increment the integer value in place
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("add {}, 1", reg));                          // increment the integer value in place
        }
    }
}

fn emit_sub_one(emitter: &mut Emitter, reg: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("sub {}, {}, #1", reg, reg));                // decrement the integer value in place
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("sub {}, 1", reg));                          // decrement the integer value in place
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::context::Context;
    use crate::codegen::platform::{Arch, Platform, Target};
    use crate::parser::ast::{Expr, ExprKind};

    fn test_emitter_x86() -> Emitter {
        Emitter::new(Target::new(Platform::Linux, Arch::X86_64))
    }

    #[test]
    fn test_emit_ref_variable_linux_x86_64_uses_native_indirect_loads() {
        let mut emitter = test_emitter_x86();
        let mut ctx = Context::new();
        ctx.alloc_var("value", PhpType::Int);
        ctx.ref_params.insert("value".into());

        let ty = emit_variable("value", &mut emitter, &mut ctx);
        let out = emitter.output();

        assert_eq!(ty, PhpType::Int);
        assert!(out.contains("    mov r11, QWORD PTR [rbp - 8]\n"));
        assert!(out.contains("    mov rax, QWORD PTR [r11]\n"));
    }

    #[test]
    fn test_emit_local_pre_and_post_increment_linux_x86_64_use_native_registers() {
        let mut emitter = test_emitter_x86();
        let mut ctx = Context::new();
        ctx.alloc_var("value", PhpType::Int);

        emit_pre_increment("value", &mut emitter, &mut ctx);
        emit_post_increment("value", &mut emitter, &mut ctx);

        let out = emitter.output();
        assert!(out.contains("    mov rax, QWORD PTR [rbp - 8]\n"));
        assert!(out.contains("    add rax, 1\n"));
        assert!(out.contains("    mov QWORD PTR [rbp - 8], rax\n"));
        assert!(out.contains("    mov r10, rax\n"));
        assert!(out.contains("    add r10, 1\n"));
        assert!(out.contains("    mov QWORD PTR [rbp - 8], r10\n"));
    }

    #[test]
    fn test_emit_throw_linux_x86_64_uses_native_result_register() {
        let mut emitter = test_emitter_x86();
        let mut ctx = Context::new();
        let mut data = DataSection::new();
        let expr = Expr::new(ExprKind::Throw(Box::new(Expr::int_lit(7))), crate::span::Span::dummy());

        let ty = emit_throw(
            match &expr.kind {
                ExprKind::Throw(inner) => inner,
                _ => unreachable!(),
            },
            &mut emitter,
            &mut ctx,
            &mut data,
        );

        let out = emitter.output();
        assert_eq!(ty, PhpType::Void);
        assert!(out.contains("    mov rax, 7\n"));
        assert!(out.contains("    mov QWORD PTR [rip + _exc_value], rax\n"));
        assert!(out.contains("    call __rt_throw_current\n"));
    }
}
