//! Purpose:
//! Lowers variable reads from locals, globals, static storage, and special compiler-managed slots.
//! Loads values into the standard expression result registers for downstream consumers.
//!
//! Called from:
//! - `crate::codegen::expr::emit_expr()`
//!
//! Key details:
//! - Variable reads must respect slot ownership and static/global symbol storage conventions.

use crate::codegen::platform::Arch;

use super::super::abi;
use super::super::context::{Context, HeapOwnership};
use super::super::data_section::DataSection;
use super::super::emit::Emitter;
use super::{expr_result_heap_ownership, Expr, PhpType};

/// Emits code to read a variable by name, dispatching on storage class.
///
/// Checks for FCC deferred closures, extern globals, global vars, ref params,
/// and local stack slots. Loads the value into the standard expression result
/// register(s) and returns the PHP type.
///
/// - **FCC closures**: marks the deferred wrapper as needed before dispatching.
/// - **Extern globals**: delegates to `emit_global_load`.
/// - **Global vars**: delegates to `emit_global_load`.
/// - **Ref params**: delegates to `emit_ref_variable`.
/// - **Local slots**: loads from the stack offset via `abi::emit_load`.
pub(super) fn emit_variable(name: &str, emitter: &mut Emitter, ctx: &mut Context) -> PhpType {
    // Loading the variable's value as an Expr means the FCC pointer escapes the
    // short-circuit path (`emit_closure_call` bypasses this function when it
    // short-circuits, so we don't see those reads here). Mark the wrapper as
    // needed so the dead-wrapper optimisation emits its full body.
    if let Some(label) = ctx.variable_fcc_label.get(name).cloned() {
        if let Some(deferred) = ctx.deferred_closures.iter_mut().find(|d| d.label == label) {
            deferred.needed = true;
        }
    }

    if let Some(ty) = ctx.extern_globals.get(name).cloned() {
        super::super::stmt::emit_global_load(emitter, ctx, name, &ty);
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

/// Emits code to throw an exception.
///
/// Evaluates `inner`, retains borrowed refcounted heap values before publishing
/// them as the active exception, stores the result in `_exc_value`, and calls
/// `__rt_throw_current` to unwind to the nearest handler. Returns `PhpType::Void`.
///
/// - `inner` is evaluated first (source order preserved).
/// - Retains the value if it is refcounted and not already owned.
/// - Uses `abi::int_result_reg(emitter)` as the temporary register for the value.
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

/// Emits code for a pre-increment operation (`++$name`).
///
/// Reads the current value, increments it in place, stores back to the original
/// slot, and returns `PhpType::Int`. Supports global vars, ref params, and local
/// stack slots.
///
/// - **Undefined var**: emits a warning comment and returns `PhpType::Int`.
/// - **Global vars**: loads via `emit_global_load`, increments, stores inline to the global symbol.
/// - **Ref params**: loads the pointer from the stack slot, dereferences, increments, stores back.
/// - **Local slots**: uses `abi::load_at_offset` / `abi::store_at_offset`.
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

/// Emits code for a post-increment operation (`$name++`).
///
/// Copies the current value to the result register, increments a scratch copy,
/// stores back to the original slot, and returns the original value as
/// `PhpType::Int`. Supports global vars, ref params, and local stack slots.
///
/// - **Undefined var**: emits a warning comment and returns `PhpType::Int`.
/// - **Global vars**: loads via `emit_global_load`, copies to scratch, increments scratch, stores inline.
/// - **Ref params**: loads the pointer from the stack slot, dereferences, copies result, increments scratch, stores back.
/// - **Local slots**: uses `abi::load_at_offset` to result reg, copies to scratch, increments scratch, stores back.
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

/// Emits code for a pre-decrement operation (`--$name`).
///
/// Reads the current value, decrements it in place, stores back to the original
/// slot, and returns `PhpType::Int`. Supports global vars, ref params, and local
/// stack slots.
///
/// - **Undefined var**: emits a warning comment and returns `PhpType::Int`.
/// - **Global vars**: loads via `emit_global_load`, decrements, stores inline to the global symbol.
/// - **Ref params**: loads the pointer from the stack slot, dereferences, decrements, stores back.
/// - **Local slots**: uses `abi::load_at_offset` / `abi::store_at_offset`.
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

/// Emits code for a post-decrement operation (`$name--`).
///
/// Copies the current value to the result register, decrements a scratch copy,
/// stores back to the original slot, and returns the original value as
/// `PhpType::Int`. Supports ref params and local stack slots. Note: global vars
/// are not handled for post-decrement (no inline global store path).
///
/// - **Undefined var**: emits a warning comment and returns `PhpType::Int`.
/// - **Ref params**: loads the pointer from the stack slot, dereferences, copies result, decrements scratch, stores back.
/// - **Local slots**: uses `abi::load_at_offset` to result reg, copies to scratch, decrements scratch, stores back.
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

/// Emits code to read the `$this` variable.
///
/// Loads `$this` from its stack slot into the integer result register and returns
/// `PhpType::Object(class_name)` where `class_name` is the current class or empty
/// string if outside a class scope.
///
/// - Emits a warning and returns `PhpType::Int` if `$this` is not in scope.
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

/// Emits code to read a variable passed by reference (ref param).
///
/// Loads the pointer stored in the stack slot, then dereferences it and loads
/// the value into the appropriate result register(s) based on type:
/// - `Int`/`Bool` → `abi::int_result_reg`
/// - `Float` → `abi::float_result_reg`
/// - `Str` → `abi::string_result_regs` (pointer in first reg, length in second)
/// - Other types → `abi::int_result_reg`
///
/// Returns the variable's `PhpType`.
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

/// Emits a store of `reg` into the global variable symbol `_gvar_{name}`.
///
/// Uses `abi::emit_store_reg_to_symbol` with offset 0.
fn emit_global_store_inline(emitter: &mut Emitter, name: &str, reg: &str) {
    let label = format!("_gvar_{}", name);
    abi::emit_store_reg_to_symbol(emitter, reg, &label, 0);
}

/// Copies the integer value from `src` to `dst` using a target-specific mov instruction.
fn emit_copy_int_reg(emitter: &mut Emitter, dst: &str, src: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov {}, {}", dst, src));              // copy the integer result into a scratch register before mutating it
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {}, {}", dst, src));              // copy the integer result into a scratch register before mutating it
        }
    }
}

/// Emits an in-place increment of `reg` by 1 using a target-specific add instruction.
fn emit_add_one(emitter: &mut Emitter, reg: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("add {}, {}, #1", reg, reg));          // increment the integer value in place
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("add {}, 1", reg));                    // increment the integer value in place
        }
    }
}

/// Emits an in-place decrement of `reg` by 1 using a target-specific sub instruction.
fn emit_sub_one(emitter: &mut Emitter, reg: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("sub {}, {}, #1", reg, reg));          // decrement the integer value in place
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("sub {}, 1", reg));                    // decrement the integer value in place
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::context::Context;
    use crate::codegen::platform::{Arch, Platform, Target};
    use crate::parser::ast::{Expr, ExprKind};

    /// Verifies emitter x86.
    fn test_emitter_x86() -> Emitter {
        Emitter::new(Target::new(Platform::Linux, Arch::X86_64))
    }

    /// Verifies emit ref variable linux x86_64 uses native indirect loads.
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

    /// Verifies emit local pre and post increment linux x86_64 use native registers.
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

    /// Verifies emit throw linux x86_64 uses native result register.
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
