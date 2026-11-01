//! Purpose:
//! Enforces a closure's RUNTIME-bound scope on property access: the visibility check php applies
//! when `Closure::bind($f, $t, $scope)` names a scope that only exists at run time.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()`, immediately before a property opcode.
//!
//! Key details:
//! - Design. A closure whose body addresses properties by a runtime name (`$o->$name`) captures one
//!   hidden `Int` (`crate::names::CLOSURE_SCOPE_LOCAL`): `0` until `__rt_closure_set_scope` stamps
//!   the bound class id plus one into a freshly bound descriptor. Every invocation path already
//!   transports captures, so the scope reaches the body with no thread-local push/pop and no change
//!   to any call path. Closures without runtime-name property access have no such capture, take no
//!   gate, and keep their statically resolved fast path unchanged.
//! - Resolution inside such a body is PERMISSIVE: `resolve_property_name_in_current_scope` answers
//!   as the property's declaring class would, so the access always finds a slot. This module is the
//!   enforcement half: before the access it asks `__rt_prop_scope_denied` whether the EFFECTIVE
//!   scope can see the member on the receiver's runtime class, and raises php's verbatim
//!   `Cannot access private property D::$a` / `Cannot access protected property D::$a` if not.
//! - The effective scope is the stamped one when set; otherwise the closure's lexical class, or
//!   global scope (-1) for a closure written outside any class (php keeps the scope when `bind`
//!   omits it).
//! - A scope class only the interpreter declared has no class id. Its stamp is instead the
//!   address of an interned `[name_ptr, name_len]` record with bit 63 set (see
//!   `crate::codegen::lower_inst::builtins::eval::scope_context::emit_effective_scope_name`), so
//!   the stamp is NEGATIVE: the gate treats it as global scope, since no compiled class sits
//!   under an interpreted one, and the eval bridges hand the interpreter the name itself.
//! - Silent probes (`isset`, `empty`, `??`) are not gated: php answers false/null for an invisible
//!   member, which the permissive resolution would instead answer from the real slot. A private
//!   property declared by a STRICT ancestor of the receiver is likewise never refused here, because
//!   php resolves that name dynamically; see `__rt_prop_scope_denied`.
//! - The gate leaves the stack and every value slot untouched on the allowed path.

use super::*;

use crate::ir::Op as IrOp;

/// Where the gated property name comes from.
enum GateName {
    /// A compile-time name (`$o->prop`).
    Static(String),
    /// A runtime string value (`$o->$name`).
    Runtime(ValueId),
}

/// Emits the runtime scope check before a property opcode of a runtime-scoped closure body.
///
/// A no-op unless the function carries the hidden scope capture and the instruction is a
/// non-silent property access on an object receiver.
pub(in crate::codegen::lower_inst) fn emit_closure_scope_gate(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let Some(scope_slot) = ctx.local_slot_by_name(crate::names::CLOSURE_SCOPE_LOCAL) else {
        return Ok(());
    };
    let name = match inst.op {
        IrOp::PropGet | IrOp::PropSet | IrOp::PropUnset | IrOp::PropGetForWrite => {
            if !matches!(inst.immediate, Some(Immediate::Data(_))) {
                return Ok(());
            }
            if inst.op == IrOp::PropGet && !property_fetch_mode(inst).is_read() {
                return Ok(());
            }
            GateName::Static(property_name_immediate(ctx, inst)?.to_string())
        }
        IrOp::DynamicPropGet | IrOp::DynamicPropSet | IrOp::DynamicPropUnset => {
            if inst.op == IrOp::DynamicPropGet && !property_fetch_mode(inst).is_read() {
                return Ok(());
            }
            let name = expect_operand(inst, 1)?;
            if ctx.value_php_type(name)?.codegen_repr() != PhpType::Str {
                return Ok(());
            }
            match const_string_operand(ctx, name)? {
                Some(literal) => GateName::Static(literal.to_string()),
                None => GateName::Runtime(name),
            }
        }
        _ => return Ok(()),
    };
    // No stamped scope means php keeps the closure's own: its lexical class, or global scope (-1)
    // for a closure written outside any class, even when it has a bound `$this`.
    let fallback_scope = Some(ctx.lexical_class_id());
    let object = expect_operand(inst, 0)?;
    let object_type = ctx.value_php_type(object)?.codegen_repr();
    if !matches!(
        object_type,
        PhpType::Object(_) | PhpType::Mixed | PhpType::Union(_)
    ) {
        return Ok(());
    }
    let skip_label = ctx.next_label("scope_gate_skip");
    let ok_label = ctx.next_label("scope_gate_ok");
    let result_reg = abi::int_result_reg(ctx.emitter);

    // -- receiver object pointer in the result register, or skip --
    ctx.load_value_to_reg(object, result_reg)?;
    if matches!(object_type, PhpType::Object(_)) {
        let scratch = abi::secondary_scratch_reg(ctx.emitter);
        crate::codegen::sentinels::emit_branch_if_null_container(
            ctx.emitter,
            result_reg,
            scratch,
            &skip_label,
        );
    } else {
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("cmp x0, #6");                          // runtime tag 6 = object; anything else has no members to gate
                ctx.emitter.instruction(&format!("b.ne {}", skip_label));       // non-object receivers fail elsewhere
                ctx.emitter.instruction("mov x0, x1");                          // the unboxed object pointer becomes the result
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("cmp rax, 6");                          // runtime tag 6 = object; anything else has no members to gate
                ctx.emitter.instruction(&format!("jne {}", skip_label));        // non-object receivers fail elsewhere
                ctx.emitter.instruction("mov rax, rdi");                        // the unboxed object pointer becomes the result
            }
        }
    }

    // -- stage the object and the property name on the temporary stack --
    abi::emit_push_reg(ctx.emitter, result_reg);
    let (name_ptr, name_len) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x1", "x2"),
        Arch::X86_64 => ("rsi", "rdx"),
    };
    match &name {
        GateName::Static(text) => {
            let (label, len) = ctx.data.add_string(text.as_bytes());
            abi::emit_symbol_address(ctx.emitter, name_ptr, &label);
            abi::emit_load_int_immediate(ctx.emitter, name_len, len as i64);
        }
        GateName::Runtime(value) => ctx.load_string_value_to_regs(*value, name_ptr, name_len)?,
    }
    abi::emit_push_reg_pair(ctx.emitter, name_ptr, name_len);

    // -- effective scope id: the stamped one (minus the +1 encoding), else the fallback --
    let scope_offset = ctx.local_offset(scope_slot)?;
    let scope_reg = match ctx.emitter.target.arch {
        Arch::AArch64 => "x3",
        Arch::X86_64 => "rcx",
    };
    let have_scope = ctx.next_label("scope_gate_have_scope");
    abi::load_at_offset(ctx.emitter, scope_reg, scope_offset);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {scope_reg}, #0"));           // 0 = no run-time scope was stamped
            ctx.emitter.instruction(&format!("b.eq {}_fallback", have_scope));  // fall back to the lexical scope
            ctx.emitter.instruction(&format!("b.lt {}_interpreted", have_scope)); // an interpreted class's name token
            ctx.emitter.instruction(&format!("sub {scope_reg}, {scope_reg}, #1")); // undo the +1 encoding: the bound class id
            ctx.emitter.instruction(&format!("b {}", have_scope));              // scope ready
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {scope_reg}, {scope_reg}")); // 0 = no run-time scope was stamped
            ctx.emitter.instruction(&format!("je {}_fallback", have_scope));    // fall back to the lexical scope
            ctx.emitter.instruction(&format!("js {}_interpreted", have_scope));  // an interpreted class's name token
            ctx.emitter.instruction(&format!("sub {scope_reg}, 1"));            // undo the +1 encoding: the bound class id
            ctx.emitter.instruction(&format!("jmp {}", have_scope));            // scope ready
        }
    }
    // A scope only the interpreter declared has no class id, so no compiled class lies under it:
    // compiled members see it as global scope.
    ctx.emitter.label(&format!("{}_interpreted", have_scope));
    abi::emit_load_int_immediate(ctx.emitter, scope_reg, -1);
    abi::emit_jump(ctx.emitter, &have_scope);
    ctx.emitter.label(&format!("{}_fallback", have_scope));
    match fallback_scope {
        Some(scope) => abi::emit_load_int_immediate(ctx.emitter, scope_reg, scope),
        None => {
            // Keep the receiver-class approximation: nothing to enforce.
            abi::emit_release_temporary_stack(ctx.emitter, 32);
            abi::emit_jump(ctx.emitter, &skip_label);
        }
    }
    ctx.emitter.label(&have_scope);

    // -- ask the runtime: class id of the receiver, name, scope --
    let class_reg = match ctx.emitter.target.arch {
        Arch::AArch64 => "x0",
        Arch::X86_64 => "rdi",
    };
    abi::emit_load_temporary_stack_slot(ctx.emitter, class_reg, 16);
    abi::emit_load_from_address(ctx.emitter, class_reg, class_reg, 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, name_ptr, 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, name_len, 8);
    abi::emit_call_label(ctx.emitter, "__rt_prop_scope_denied");
    let private_label = ctx.next_label("scope_gate_private");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {}", ok_label));          // 0 = visible from this scope
            ctx.emitter.instruction("cmp x0, #1");                              // 1 = private, 2 = protected
            ctx.emitter.instruction(&format!("b.eq {}", private_label));        // private wording
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // 0 = visible from this scope
            ctx.emitter.instruction(&format!("jz {}", ok_label));               // proceed
            ctx.emitter.instruction("cmp rax, 1");                              // 1 = private, 2 = protected
            ctx.emitter.instruction(&format!("je {}", private_label));          // private wording
        }
    }
    emit_scope_refusal(ctx, "protected");
    ctx.emitter.label(&private_label);
    emit_scope_refusal(ctx, "private");
    ctx.emitter.label(&ok_label);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    ctx.emitter.label(&skip_label);
    Ok(())
}

/// Throws `Cannot access <visibility> property <RuntimeClass>::$<name>` and never returns.
///
/// Expects the 32-byte staging block `emit_closure_scope_gate` pushed: the name pair on top and
/// the receiver object below it.
pub(super) fn emit_scope_refusal(ctx: &mut FunctionContext<'_>, visibility: &str) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 16);
    super::super::runtime_class_messages::emit_runtime_class_name_to_string_result(
        ctx,
        result_reg,
        "object",
    );
    super::super::runtime_class_messages::emit_concat_static_prefix(
        ctx,
        &format!("Cannot access {visibility} property "),
    );
    super::super::runtime_class_messages::emit_concat_static_suffix(ctx, "::$");
    let (right_ptr, right_len) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x3", "x4"),
        Arch::X86_64 => ("rdi", "rsi"),
    };
    abi::emit_load_temporary_stack_slot(ctx.emitter, right_ptr, 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, right_len, 8);
    abi::emit_call_label(ctx.emitter, "__rt_concat");
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    super::super::exceptions::emit_error_from_string_result(ctx);
}
