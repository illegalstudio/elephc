//! Purpose:
//! Shared lowering helpers for SPL iterator helper builtins.
//! Builds temporary result containers and bridges Iterator method results to array/hash storage.
//!
//! Called from:
//! - `crate::codegen::builtins::spl::iterator_count`
//! - `crate::codegen::builtins::spl::iterator_to_array`
//!
//! Key details:
//! - Iterator loop state keeps the receiver at the top of the temporary stack.
//! - Extra builtin state is stored underneath that receiver so foreach-style dispatch can reuse it.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::stmt::{reload_iterator_receiver, IteratorDispatchTarget};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// Provides the Iterator object name helper used by the iterator common module.
pub(super) fn iterator_object_name(ty: &PhpType) -> Option<&str> {
    match ty {
        PhpType::Object(class_name) => Some(class_name.as_str()),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PreserveKeysArg {
    Static(bool),
    Dynamic,
}

/// Builds the argument metadata for preserve keys.
pub(super) fn preserve_keys_arg(args: &[Expr]) -> PreserveKeysArg {
    match args.get(1).map(|arg| &arg.kind) {
        Some(kind) => static_truthiness(kind)
            .map(PreserveKeysArg::Static)
            .unwrap_or(PreserveKeysArg::Dynamic),
        None => PreserveKeysArg::Static(true),
    }
}

/// Provides the Static truthiness helper used by the iterator common module.
fn static_truthiness(kind: &ExprKind) -> Option<bool> {
    match kind {
        ExprKind::BoolLiteral(value) => Some(*value),
        ExprKind::IntLiteral(value) => Some(*value != 0),
        ExprKind::FloatLiteral(value) => Some(*value != 0.0),
        ExprKind::StringLiteral(value) => Some(!value.is_empty() && value != "0"),
        ExprKind::Null => Some(false),
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(value) => Some(*value != 0),
            ExprKind::FloatLiteral(value) => Some(*value != 0.0),
            _ => None,
        },
        _ => None,
    }
}

/// Emits assembly for count loaded array.
pub(super) fn emit_count_loaded_array(source_ty: &PhpType, emitter: &mut Emitter) -> bool {
    match source_ty.codegen_repr() {
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            abi::emit_load_from_address(
                emitter,
                abi::int_result_reg(emitter),
                abi::int_result_reg(emitter),
                0,
            );
            true
        }
        _ => false,
    }
}

/// Emits assembly for clone loaded array.
pub(super) fn emit_clone_loaded_array(source_ty: &PhpType, emitter: &mut Emitter) -> Option<PhpType> {
    match source_ty.codegen_repr() {
        PhpType::Array(elem_ty) => {
            if emitter.target.arch == Arch::X86_64 {
                emitter.instruction("mov rdi, rax");                            // pass the loaded indexed array to the shallow-clone helper
            }
            abi::emit_call_label(emitter, "__rt_array_clone_shallow");
            Some(PhpType::Array(elem_ty))
        }
        PhpType::AssocArray { key, value } => {
            if emitter.target.arch == Arch::X86_64 {
                emitter.instruction("mov rdi, rax");                            // pass the loaded hash to the shallow-clone helper
            }
            abi::emit_call_label(emitter, "__rt_hash_clone_shallow");
            Some(PhpType::AssocArray { key, value })
        }
        _ => None,
    }
}

/// Emits assembly for clone loaded runtime indexed array as mixed.
pub(super) fn emit_clone_loaded_runtime_indexed_array_as_mixed(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // pass the runtime indexed array to the shallow-clone helper
    }
    abi::emit_call_label(emitter, "__rt_array_clone_shallow");
    emit_loaded_runtime_indexed_array_as_mixed(emitter);
}

/// Emits assembly for loaded runtime indexed array as mixed.
pub(super) fn emit_loaded_runtime_indexed_array_as_mixed(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x1, [x0, #-8]");                           // load packed indexed-array metadata before widening to Mixed slots
            emitter.instruction("lsr x1, x1, #8");                              // move the runtime value_type tag into the low bits
            emitter.instruction("and x1, x1, #0x7f");                           // isolate the indexed-array value_type tag for conversion
            abi::emit_call_label(emitter, "__rt_array_to_mixed");              // convert cloned indexed-array slots to boxed Mixed cells
        }
        Arch::X86_64 => {
            emitter.instruction("mov rsi, QWORD PTR [rax - 8]");                // load packed indexed-array metadata before widening to Mixed slots
            emitter.instruction("shr rsi, 8");                                  // move the runtime value_type tag into the low bits
            emitter.instruction("and rsi, 0x7f");                               // isolate the indexed-array value_type tag for conversion
            emitter.instruction("mov rdi, rax");                                // pass the cloned indexed array to the Mixed conversion helper
            abi::emit_call_label(emitter, "__rt_array_to_mixed");              // convert cloned indexed-array slots to boxed Mixed cells
        }
    }
}

/// Emits assembly for clone loaded runtime hash as mixed.
pub(super) fn emit_clone_loaded_runtime_hash_as_mixed(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // pass the runtime hash to the shallow-clone helper
    }
    abi::emit_call_label(emitter, "__rt_hash_clone_shallow");
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_call_label(emitter, "__rt_hash_to_mixed");               // convert cloned hash entries to boxed Mixed cells
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, rax");                                // pass the cloned hash to the Mixed conversion helper
            abi::emit_call_label(emitter, "__rt_hash_to_mixed");               // convert cloned hash entries to boxed Mixed cells
        }
    }
}

/// Emits assembly for new mixed indexed array.
pub(super) fn emit_new_mixed_indexed_array(emitter: &mut Emitter) {
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 0), 16);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 1), 8);
    abi::emit_call_label(emitter, "__rt_array_new");
    crate::codegen::expr::arrays::emit_array_value_type_stamp(
        emitter,
        abi::int_result_reg(emitter),
        &PhpType::Mixed,
    );
}

/// Emits assembly for new mixed hash.
pub(super) fn emit_new_mixed_hash(emitter: &mut Emitter) {
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 0), 16);
    abi::emit_load_int_immediate(
        emitter,
        abi::int_arg_reg_name(emitter.target, 1),
        crate::codegen::runtime_value_tag(&PhpType::Mixed) as i64,
    );
    abi::emit_call_label(emitter, "__rt_hash_new");
}

/// Emits assembly for save result under receiver.
pub(super) fn emit_save_result_under_receiver(emitter: &mut Emitter) {
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
}

/// Emits assembly for restore receiver from preserved reg.
pub(super) fn emit_restore_receiver_from_preserved_reg(emitter: &mut Emitter, receiver_reg: &str) {
    emitter.instruction(&format!(
        "mov {}, {}",
        abi::int_result_reg(emitter),
        receiver_reg
    )); // restore the iterator receiver as the next loop-driver input
}

/// Emits assembly for increment saved count.
pub(super) fn emit_increment_saved_count(emitter: &mut Emitter) {
    emit_increment_saved_count_at_offset(16, emitter);
}

/// Emits assembly for increment saved count at offset.
pub(super) fn emit_increment_saved_count_at_offset(offset: usize, emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x9, [sp, #{}]", offset));         // load the saved iterator helper counter beneath the receiver slot
            emitter.instruction("add x9, x9, #1");                              // count this valid iterator position
            emitter.instruction(&format!("str x9, [sp, #{}]", offset));         // persist the updated iterator helper counter
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("add QWORD PTR [rsp + {}], 1", offset)); // count this valid iterator position beneath the receiver slot
        }
    }
}

/// Emits assembly for append current to saved array.
pub(super) fn emit_append_current_to_saved_array(
    dispatch_target: &IteratorDispatchTarget,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    reload_iterator_receiver(emitter);
    let current_ty = dispatch_target.dispatch("current", emitter, ctx);
    crate::codegen::emit_box_current_value_as_mixed(emitter, &current_ty.codegen_repr());

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("str x0, [sp, #-16]!");                         // preserve the boxed current() value while loading the result array
            emitter.instruction("ldr x0, [sp, #32]");                           // load iterator_to_array()'s indexed result array beneath receiver and value
            emitter.instruction("ldr x1, [sp], #16");                           // restore boxed current() as the appended mixed payload
            emitter.instruction("bl __rt_array_push_int");                      // append the owned mixed value to the indexed result array
            emitter.instruction("str x0, [sp, #16]");                           // save the possibly-grown result array beneath the receiver slot
        }
        Arch::X86_64 => {
            emitter.instruction("sub rsp, 16");                                 // reserve a temporary slot for the boxed current() value
            emitter.instruction("mov QWORD PTR [rsp], rax");                    // preserve the boxed current() value while loading the result array
            emitter.instruction("mov rdi, QWORD PTR [rsp + 32]");               // load iterator_to_array()'s indexed result array beneath receiver and value
            emitter.instruction("mov rsi, QWORD PTR [rsp]");                    // pass boxed current() as the appended mixed payload
            emitter.instruction("add rsp, 16");                                 // restore the stack so the receiver is again the top temporary slot
            emitter.instruction("call __rt_array_push_int");                    // append the owned mixed value to the indexed result array
            emitter.instruction("mov QWORD PTR [rsp + 16], rax");               // save the possibly-grown result array beneath the receiver slot
        }
    }
}

/// Emits assembly for insert current with iterator key.
pub(super) fn emit_insert_current_with_iterator_key(
    dispatch_target: &IteratorDispatchTarget,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    reload_iterator_receiver(emitter);
    let key_ty = dispatch_target.dispatch("key", emitter, ctx);
    emit_normalized_key_from_result(&key_ty.codegen_repr(), emitter, ctx, data);
    reload_iterator_receiver(emitter);
    let (key_lo_reg, key_hi_reg) = normalized_key_regs(emitter);
    abi::emit_push_reg_pair(emitter, key_lo_reg, key_hi_reg);                   // preserve the normalized iterator key while current() is dispatched

    let current_ty = dispatch_target.dispatch("current", emitter, ctx);
    crate::codegen::emit_box_current_value_as_mixed(emitter, &current_ty.codegen_repr());

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x3, x0");                                  // pass the boxed current() value as hash value_lo
            emitter.instruction("mov x4, xzr");                                 // boxed mixed hash values do not use value_hi
            emitter.instruction("mov x5, #7");                                  // value tag 7 tells the hash it owns a boxed mixed cell
            abi::emit_pop_reg_pair(emitter, "x1", "x2");                       // restore the normalized iterator key into hash-set argument registers
            emitter.instruction("ldr x0, [sp, #16]");                           // load iterator_to_array()'s associative result hash beneath the receiver slot
            emitter.instruction("bl __rt_hash_set");                            // insert or update the preserved key with the owned mixed current() value
            emitter.instruction("str x0, [sp, #16]");                           // save the possibly-grown result hash beneath the receiver slot
        }
        Arch::X86_64 => {
            emitter.instruction("mov rcx, rax");                                // pass the boxed current() value as hash value_lo
            emitter.instruction("xor r8, r8");                                  // boxed mixed hash values do not use value_hi
            emitter.instruction("mov r9, 7");                                   // value tag 7 tells the hash it owns a boxed mixed cell
            abi::emit_pop_reg_pair(emitter, "rsi", "rdx");                     // restore the normalized iterator key into hash-set argument registers
            emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");               // load iterator_to_array()'s associative result hash beneath the receiver slot
            emitter.instruction("call __rt_hash_set");                          // insert or update the preserved key with the owned mixed current() value
            emitter.instruction("mov QWORD PTR [rsp + 16], rax");               // save the possibly-grown result hash beneath the receiver slot
        }
    }
}

/// Provides the Normalized key regs helper used by the iterator common module.
fn normalized_key_regs(emitter: &Emitter) -> (&'static str, &'static str) {
    match emitter.target.arch {
        Arch::AArch64 => ("x1", "x2"),
        Arch::X86_64 => ("rax", "rdx"),
    }
}

/// Emits assembly for normalized key from result.
fn emit_normalized_key_from_result(
    key_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    match key_ty {
        PhpType::Int | PhpType::Bool => emit_integer_key_from_result(emitter),
        PhpType::Float => emit_float_key_from_result(emitter),
        PhpType::Str => {
            abi::emit_call_label(emitter, "__rt_hash_normalize_key");
        }
        PhpType::Mixed | PhpType::Union(_) => emit_mixed_key_from_result(emitter, ctx, data),
        _ => emit_integer_key_from_result(emitter),
    }
}

/// Emits assembly for integer key from result.
fn emit_integer_key_from_result(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x1, x0");                                  // use the scalar key payload as normalized key_lo
            emitter.instruction("mov x2, #-1");                                 // key_hi sentinel marks the iterator key as integer
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdx, -1");                                 // key_hi sentinel marks the iterator key as integer while rax stays key_lo
        }
    }
}

/// Emits assembly for float key from result.
fn emit_float_key_from_result(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("fcvtzs x1, d0");                               // PHP casts float iterator keys to integer array keys
            emitter.instruction("mov x2, #-1");                                 // key_hi sentinel marks the iterator key as integer
        }
        Arch::X86_64 => {
            emitter.instruction("cvttsd2si rax, xmm0");                         // PHP casts float iterator keys to integer array keys
            emitter.instruction("mov rdx, -1");                                 // key_hi sentinel marks the iterator key as integer
        }
    }
}

/// Emits assembly for mixed key from result.
fn emit_mixed_key_from_result(emitter: &mut Emitter, ctx: &mut Context, data: &mut DataSection) {
    let string_label = ctx.next_label("iterator_key_string");
    let int_label = ctx.next_label("iterator_key_int");
    let bool_label = ctx.next_label("iterator_key_bool");
    let float_label = ctx.next_label("iterator_key_float");
    let null_label = ctx.next_label("iterator_key_null");
    let done_label = ctx.next_label("iterator_key_done");
    let (empty_label, _) = data.add_string(b"");

    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #1");                                  // is the mixed iterator key a string?
            emitter.instruction(&format!("b.eq {}", string_label));             // normalize string iterator keys through the hash key helper
            emitter.instruction("cmp x0, #0");                                  // is the mixed iterator key an integer?
            emitter.instruction(&format!("b.eq {}", int_label));                // use integer payloads directly as array keys
            emitter.instruction("cmp x0, #3");                                  // is the mixed iterator key a boolean?
            emitter.instruction(&format!("b.eq {}", bool_label));               // use boolean payloads as integer array keys
            emitter.instruction("cmp x0, #2");                                  // is the mixed iterator key a float?
            emitter.instruction(&format!("b.eq {}", float_label));              // cast float iterator keys to integer array keys
            emitter.instruction("cmp x0, #8");                                  // is the mixed iterator key null?
            emitter.instruction(&format!("b.eq {}", null_label));               // PHP treats null array keys as the empty string
            emitter.instruction(&format!("b {}", int_label));                   // unsupported key payloads fall back to their low word

            emitter.label(&string_label);
            emitter.instruction("bl __rt_hash_normalize_key");                  // normalize numeric-string iterator keys before insertion
            emitter.instruction(&format!("b {}", done_label));                  // skip scalar-key normalization after string handling

            emitter.label(&int_label);
            emitter.instruction("mov x2, #-1");                                 // mark the unboxed integer low word as an integer key
            emitter.instruction(&format!("b {}", done_label));                  // finish normalized mixed-key handling

            emitter.label(&bool_label);
            emitter.instruction("mov x2, #-1");                                 // mark the unboxed boolean low word as an integer key
            emitter.instruction(&format!("b {}", done_label));                  // finish normalized mixed-key handling

            emitter.label(&float_label);
            emitter.instruction("fmov d0, x1");                                 // reinterpret the unboxed float payload bits for integer-key casting
            emitter.instruction("fcvtzs x1, d0");                               // PHP casts float array keys to integer keys
            emitter.instruction("mov x2, #-1");                                 // mark the converted float payload as an integer key
            emitter.instruction(&format!("b {}", done_label));                  // finish normalized mixed-key handling

            emitter.label(&null_label);
            abi::emit_symbol_address(emitter, "x1", &empty_label);
            emitter.instruction("mov x2, #0");                                  // null iterator keys become the empty-string key
            emitter.instruction("bl __rt_hash_normalize_key");                  // preserve empty-string key semantics for hash insertion
            emitter.label(&done_label);
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 1");                                  // is the mixed iterator key a string?
            emitter.instruction(&format!("je {}", string_label));               // normalize string iterator keys through the hash key helper
            emitter.instruction("cmp rax, 0");                                  // is the mixed iterator key an integer?
            emitter.instruction(&format!("je {}", int_label));                  // use integer payloads directly as array keys
            emitter.instruction("cmp rax, 3");                                  // is the mixed iterator key a boolean?
            emitter.instruction(&format!("je {}", bool_label));                 // use boolean payloads as integer array keys
            emitter.instruction("cmp rax, 2");                                  // is the mixed iterator key a float?
            emitter.instruction(&format!("je {}", float_label));                // cast float iterator keys to integer array keys
            emitter.instruction("cmp rax, 8");                                  // is the mixed iterator key null?
            emitter.instruction(&format!("je {}", null_label));                 // PHP treats null array keys as the empty string
            emitter.instruction(&format!("jmp {}", int_label));                 // unsupported key payloads fall back to their low word

            emitter.label(&string_label);
            emitter.instruction("mov rax, rdi");                                // move the unboxed string pointer into hash-normalize key_lo
            emitter.instruction("call __rt_hash_normalize_key");                // normalize numeric-string iterator keys before insertion
            emitter.instruction(&format!("jmp {}", done_label));                // skip scalar-key normalization after string handling

            emitter.label(&int_label);
            emitter.instruction("mov rax, rdi");                                // move the unboxed integer low word into normalized key_lo
            emitter.instruction("mov rdx, -1");                                 // mark the key as integer
            emitter.instruction(&format!("jmp {}", done_label));                // finish normalized mixed-key handling

            emitter.label(&bool_label);
            emitter.instruction("mov rax, rdi");                                // move the unboxed boolean low word into normalized key_lo
            emitter.instruction("mov rdx, -1");                                 // mark the key as integer
            emitter.instruction(&format!("jmp {}", done_label));                // finish normalized mixed-key handling

            emitter.label(&float_label);
            emitter.instruction("movq xmm0, rdi");                              // reinterpret the unboxed float payload bits for integer-key casting
            emitter.instruction("cvttsd2si rax, xmm0");                         // PHP casts float array keys to integer keys
            emitter.instruction("mov rdx, -1");                                 // mark the converted float payload as an integer key
            emitter.instruction(&format!("jmp {}", done_label));                // finish normalized mixed-key handling

            emitter.label(&null_label);
            abi::emit_symbol_address(emitter, "rax", &empty_label);
            emitter.instruction("xor rdx, rdx");                                // null iterator keys become the empty-string key
            emitter.instruction("call __rt_hash_normalize_key");                // preserve empty-string key semantics for hash insertion
            emitter.label(&done_label);
        }
    }
}
