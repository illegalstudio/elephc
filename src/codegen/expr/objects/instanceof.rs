//! Purpose:
//! Lowers instanceof checks against class, interface, and dynamic targets.
//! Produces object-related expression results while respecting runtime metadata and ownership rules.
//!
//! Called from:
//! - `crate::codegen::expr::objects`
//!
//! Key details:
//! - Object handles, property storage, and class ids must stay consistent with emitted class tables.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::names::Name;
use crate::parser::ast::{Expr, InstanceOfTarget};
use crate::types::PhpType;

use super::super::emit_expr;
use super::dispatch;

pub(super) fn emit_instanceof(
    value: &Expr,
    target: &InstanceOfTarget,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    match target {
        InstanceOfTarget::Name(name) => emit_named_instanceof(value, name, emitter, ctx, data),
        InstanceOfTarget::Expr(target) => emit_dynamic_instanceof(value, target, emitter, ctx, data),
    }
}

fn emit_named_instanceof(
    value: &Expr,
    target: &Name,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment(&format!("instanceof {}", target.as_str()));
    let value_ty = emit_expr(value, emitter, ctx, data);
    let value_repr = value_ty.codegen_repr();

    let target_kind = match classify_named_target(target, ctx) {
        Some(kind) => kind,
        None => {
            emit_false(emitter);
            return PhpType::Bool;
        }
    };

    if !can_hold_object_or_boxed_value(&value_repr) {
        emit_false(emitter);
        return PhpType::Bool;
    }

    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the tested value while materializing the target type id
    let target_kind_id = match target_kind {
        ResolvedInstanceOfTarget::Class(class_id) => {
            abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), class_id as i64);
            0
        }
        ResolvedInstanceOfTarget::Interface(interface_id) => {
            abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), interface_id as i64);
            1
        }
        ResolvedInstanceOfTarget::LateStaticClass => {
            if !dispatch::emit_forwarded_called_class_id(emitter, ctx) {
                abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));       // discard the preserved tested value before returning false
                emit_false(emitter);
                return PhpType::Bool;
            }
            0
        }
    };
    let matcher = if matches!(value_repr, PhpType::Mixed | PhpType::Union(_)) {
        "__rt_mixed_instanceof"
    } else {
        "__rt_exception_matches"
    };
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the target id while loading runtime matcher arguments
    abi::emit_pop_reg(emitter, abi::int_arg_reg_name(emitter.target, 1));       // pass target class/interface id as matcher argument 2
    abi::emit_pop_reg(emitter, abi::int_arg_reg_name(emitter.target, 0));       // pass the tested object pointer as matcher argument 1
    abi::emit_load_int_immediate(
        emitter,
        abi::int_arg_reg_name(emitter.target, 2),
        target_kind_id,
    );
    abi::emit_call_label(emitter, matcher);                                     // run the object/class/interface matcher for plain or boxed values
    PhpType::Bool
}

fn emit_dynamic_instanceof(
    value: &Expr,
    target: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment("dynamic instanceof");
    let value_ty = emit_expr(value, emitter, ctx, data);
    let value_repr = value_ty.codegen_repr();

    let target_false = ctx.next_label("instanceof_dynamic_target_false");
    let done = ctx.next_label("instanceof_dynamic_done");

    emit_normalized_object_value(&value_repr, emitter, ctx);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the tested object-or-null pointer while validating the dynamic target
    emit_dynamic_target(target, &target_false, emitter, ctx, data);
    emit_dynamic_match_call(emitter);
    abi::emit_jump(emitter, &done);                                             // skip the false paths after the runtime matcher returns a boolean

    emitter.label(&target_false);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // discard the preserved tested value for an unknown string target
    emit_false(emitter);
    abi::emit_jump(emitter, &done);                                             // converge on the common dynamic instanceof result

    emitter.label(&done);
    PhpType::Bool
}

enum ResolvedInstanceOfTarget {
    Class(u64),
    Interface(u64),
    LateStaticClass,
}

fn classify_named_target(target: &Name, ctx: &Context) -> Option<ResolvedInstanceOfTarget> {
    let target_name = match target.as_str() {
        "self" => ctx.current_class.as_deref()?,
        "parent" => ctx
            .current_class
            .as_ref()
            .and_then(|class_name| ctx.classes.get(class_name))
            .and_then(|class_info| class_info.parent.as_deref())?,
        "static" => return Some(ResolvedInstanceOfTarget::LateStaticClass),
        other => other,
    };
    if let Some(class_info) = ctx.classes.get(target_name) {
        Some(ResolvedInstanceOfTarget::Class(class_info.class_id))
    } else {
        ctx.interfaces
            .get(target_name)
            .map(|interface_info| ResolvedInstanceOfTarget::Interface(interface_info.interface_id))
    }
}

fn emit_normalized_object_value(value_repr: &PhpType, emitter: &mut Emitter, ctx: &mut Context) {
    match value_repr {
        PhpType::Object(_) => {}
        PhpType::Mixed | PhpType::Union(_) => {
            let object_label = ctx.next_label("instanceof_dynamic_value_object");
            let done = ctx.next_label("instanceof_dynamic_value_done");

            abi::emit_call_label(emitter, "__rt_mixed_unbox");                  // inspect boxed values before validating the dynamic target
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("cmp x0, #6");                          // runtime tag 6 means the tested mixed payload is an object
                    emitter.instruction(&format!("b.eq {}", object_label));     // object payloads can be tested after target validation
                    emitter.instruction("mov x0, #0");                          // non-object payloads become null so the matcher returns false
                    emitter.instruction(&format!("b {}", done));                // skip object-payload promotion for scalar, array, and null payloads

                    emitter.label(&object_label);
                    emitter.instruction("mov x0, x1");                          // promote the unboxed object pointer into the normal result register
                }
                Arch::X86_64 => {
                    emitter.instruction("cmp rax, 6");                          // runtime tag 6 means the tested mixed payload is an object
                    emitter.instruction(&format!("je {}", object_label));       // object payloads can be tested after target validation
                    emitter.instruction("xor eax, eax");                        // non-object payloads become null so the matcher returns false
                    emitter.instruction(&format!("jmp {}", done));              // skip object-payload promotion for scalar, array, and null payloads

                    emitter.label(&object_label);
                    emitter.instruction("mov rax, rdi");                        // promote the unboxed object pointer into the normal result register
                }
            }
            emitter.label(&done);
        }
        _ => {
            abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
        }
    }
}

fn emit_dynamic_target(
    target: &Expr,
    false_label: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let target_ty = emit_expr(target, emitter, ctx, data).codegen_repr();
    match target_ty {
        PhpType::Str => emit_lookup_string_target(false_label, emitter),
        PhpType::Object(_) => emit_object_target_metadata(emitter, ctx),
        PhpType::Mixed | PhpType::Union(_) => emit_mixed_target_metadata(false_label, emitter, ctx),
        _ => emit_invalid_target_fatal(emitter),
    }
}

fn emit_lookup_string_target(false_label: &str, emitter: &mut Emitter) {
    abi::emit_call_label(emitter, "__rt_instanceof_lookup");                    // resolve a dynamic class-string target to matcher metadata
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #0");                                  // did the dynamic string resolve to a known class/interface?
            emitter.instruction(&format!("b.eq {}", false_label));              // unknown class-string targets make instanceof false
            emitter.instruction("mov x0, x1");                                  // move the resolved target id into the target-id result register
            emitter.instruction("mov x1, x2");                                  // move the resolved target kind into the target-kind result register
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // did the dynamic string resolve to a known class/interface?
            emitter.instruction(&format!("je {}", false_label));                // unknown class-string targets make instanceof false
            emitter.instruction("mov rax, rdi");                                // move the resolved target id into the target-id result register
        }
    }
}

fn emit_object_target_metadata(emitter: &mut Emitter, ctx: &mut Context) {
    let ok_label = ctx.next_label("instanceof_target_object_ok");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbnz x0, {}", ok_label));             // non-null object targets can provide runtime class metadata
            emit_invalid_target_fatal(emitter);
            emitter.label(&ok_label);
            emitter.instruction("ldr x0, [x0]");                                // load the runtime class id from the target object header
            emitter.instruction("mov x1, #0");                                  // dynamic object targets are always class targets
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // null dynamic targets are not valid class-string/object targets
            emitter.instruction(&format!("jne {}", ok_label));                  // non-null object targets can provide runtime class metadata
            emit_invalid_target_fatal(emitter);
            emitter.label(&ok_label);
            emitter.instruction("mov rax, QWORD PTR [rax]");                    // load the runtime class id from the target object header
            emitter.instruction("xor edx, edx");                                // dynamic object targets are always class targets
        }
    }
}

fn emit_mixed_target_metadata(false_label: &str, emitter: &mut Emitter, ctx: &mut Context) {
    let string_label = ctx.next_label("instanceof_target_string");
    let object_label = ctx.next_label("instanceof_target_object");
    let done = ctx.next_label("instanceof_target_done");

    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // inspect a boxed dynamic target before resolving matcher metadata
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #1");                                  // runtime tag 1 means the dynamic target is a string
            emitter.instruction(&format!("b.eq {}", string_label));             // resolve boxed string targets through class-string lookup
            emitter.instruction("cmp x0, #6");                                  // runtime tag 6 means the dynamic target is an object
            emitter.instruction(&format!("b.eq {}", object_label));             // use the target object's runtime class id
            emit_invalid_target_fatal(emitter);

            emitter.label(&string_label);
            emit_lookup_string_target(false_label, emitter);
            abi::emit_jump(emitter, &done);                                     // keep resolved class-string metadata as the target result

            emitter.label(&object_label);
            emitter.instruction("mov x0, x1");                                  // move the unboxed target object pointer into the normal result register
            emit_object_target_metadata(emitter, ctx);
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 1");                                  // runtime tag 1 means the dynamic target is a string
            emitter.instruction(&format!("je {}", string_label));               // resolve boxed string targets through class-string lookup
            emitter.instruction("cmp rax, 6");                                  // runtime tag 6 means the dynamic target is an object
            emitter.instruction(&format!("je {}", object_label));               // use the target object's runtime class id
            emit_invalid_target_fatal(emitter);

            emitter.label(&string_label);
            emitter.instruction("mov rax, rdi");                                // move the unboxed string pointer into the lookup input register
            emit_lookup_string_target(false_label, emitter);
            abi::emit_jump(emitter, &done);                                     // keep resolved class-string metadata as the target result

            emitter.label(&object_label);
            emitter.instruction("mov rax, rdi");                                // move the unboxed target object pointer into the normal result register
            emit_object_target_metadata(emitter, ctx);
        }
    }
    emitter.label(&done);
}

fn emit_dynamic_match_call(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(emitter, "x0");                                  // preserve the resolved dynamic target id
            abi::emit_push_reg(emitter, "x1");                                  // preserve the resolved dynamic target kind
        }
        Arch::X86_64 => {
            abi::emit_push_reg(emitter, "rax");                                 // preserve the resolved dynamic target id
            abi::emit_push_reg(emitter, "rdx");                                 // preserve the resolved dynamic target kind
        }
    }
    abi::emit_pop_reg(emitter, abi::int_arg_reg_name(emitter.target, 2));       // pass target kind as matcher argument 3
    abi::emit_pop_reg(emitter, abi::int_arg_reg_name(emitter.target, 1));       // pass target class/interface id as matcher argument 2
    abi::emit_pop_reg(emitter, abi::int_arg_reg_name(emitter.target, 0));       // pass the tested object pointer as matcher argument 1
    abi::emit_call_label(emitter, "__rt_exception_matches");                    // run the object/class/interface matcher for the dynamic target
}

fn emit_invalid_target_fatal(emitter: &mut Emitter) {
    abi::emit_call_label(emitter, "__rt_instanceof_invalid_target");            // abort when a dynamic target is neither string nor object
}

fn can_hold_object_or_boxed_value(ty: &PhpType) -> bool {
    match ty {
        PhpType::Object(_) | PhpType::Mixed | PhpType::Union(_) => true,
        _ => false,
    }
}

fn emit_false(emitter: &mut Emitter) {
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
}
