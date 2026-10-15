//! Purpose:
//! Lowers class-relation introspection builtins for the EIR backend.
//! Materializes `class_implements()`, `class_parents()`, and `class_uses()`
//! from compile-time class/interface/trait metadata.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::lower_language_construct_call()`.
//!
//! Key details:
//! - Results are boxed `Mixed` because PHP returns `array<string,string>|false`.
//! - Associative array results use the shared hash runtime and preserve
//!   `name => name` insertion order.

use crate::codegen::platform::Arch;
use crate::codegen::{abi, emit_box_current_value_as_mixed};
use crate::codegen::{CodegenIrError, Result};
use crate::ir::class_relations::{self, ClassLikeTarget};
use crate::ir::{Immediate, Instruction, Op, ValueDef, ValueId};
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::{expect_operand, has_eval_context, lower_eval_class_relation, store_if_result};

/// Lowers `class_implements()`, `class_parents()`, and `class_uses()` from static metadata.
pub(crate) fn lower_class_relation(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    super::ensure_arg_count_between(inst, name, 1, 2)?;
    let target_value = expect_operand(inst, 0)?;
    if has_eval_context(ctx) {
        return lower_eval_class_relation(ctx, inst, target_value, name);
    }

    let target = resolve_relation_target(ctx, target_value)?;
    if matches!(target, ClassLikeTarget::Unknown) {
        emit_boxed_bool(ctx, false);
        return store_if_result(ctx, inst);
    }

    let names = relation_names(ctx, name, &target)?;
    emit_string_hash(ctx, &names);
    emit_box_current_value_as_mixed(ctx.emitter, &class_relation_array_type());
    store_if_result(ctx, inst)
}

/// Returns the associative string-set type used by class-relation builtins.
fn class_relation_array_type() -> PhpType {
    PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Str),
    }
}

/// Emits a boxed boolean result for union-typed class relation fallbacks.
fn emit_boxed_bool(ctx: &mut FunctionContext<'_>, value: bool) {
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        i64::from(value),
    );
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
}

/// Resolves a class-relation target from a literal class-like name or static object type.
///
/// The resolution itself is shared with the WASM backend (`ir::class_relations`): it reads only
/// the module's declarations, so it is a property of the program rather than of a backend.
fn resolve_relation_target(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<ClassLikeTarget> {
    match ctx.value_php_type(value)? {
        PhpType::Object(class_name) => {
            Ok(class_relations::resolve_object_target(ctx.module, &class_name))
        }
        PhpType::Str => {
            let Some(raw) = optional_const_string_operand(ctx, value)? else {
                return Err(CodegenIrError::unsupported(
                    "class-relation builtin with non-literal class name",
                ));
            };
            Ok(class_relations::resolve_named_target(ctx.module, &raw))
        }
        other => Err(CodegenIrError::unsupported(format!(
            "class-relation target PHP type {:?}",
            other
        ))),
    }
}

/// Returns the relation names for a known class-like target.
fn relation_names(
    ctx: &FunctionContext<'_>,
    name: &str,
    target: &ClassLikeTarget,
) -> Result<Vec<String>> {
    let relation = class_relations::ClassRelation::from_builtin_name(name).ok_or_else(|| {
        CodegenIrError::unsupported(format!("class-relation builtin {}", name))
    })?;
    Ok(class_relations::relation_names(ctx.module, relation, target))
}

/// Looks up an interface by PHP-style case-insensitive name.
pub(super) fn lookup_interface<'a>(
    ctx: &'a FunctionContext<'_>,
    name: &str,
) -> Option<&'a crate::types::InterfaceInfo> {
    class_relations::lookup_interface(ctx.module, name)
}

/// Collects parent interfaces without duplicates.
pub(super) fn collect_interface_parents(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
    names: &mut Vec<String>,
) {
    class_relations::collect_interface_parents(ctx.module, interface_name, names);
}

/// Allocates and fills an associative string hash in the target result register.
fn emit_string_hash(ctx: &mut FunctionContext<'_>, names: &[String]) {
    let capacity = (names.len() * 2).max(16);
    let value_tag = runtime_str_tag();
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "x1", value_tag);
            abi::emit_call_label(ctx.emitter, "__rt_hash_new");
            emit_string_hash_entries_aarch64(ctx, names);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", value_tag);
            abi::emit_call_label(ctx.emitter, "__rt_hash_new");
            emit_string_hash_entries_x86_64(ctx, names);
        }
    }
}

/// Appends string-set hash entries on AArch64.
fn emit_string_hash_entries_aarch64(ctx: &mut FunctionContext<'_>, names: &[String]) {
    if names.is_empty() {
        return;
    }
    ctx.emitter.instruction("str x0, [sp, #-16]!");                             // park the class-relation hash while inserting metadata entries
    for name in names {
        let (label, len) = ctx.data.add_string(name.as_bytes());
        abi::emit_symbol_address(ctx.emitter, "x1", &label);
        abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
        abi::emit_call_label(ctx.emitter, "__rt_hash_normalize_key");
        abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
        abi::emit_symbol_address(ctx.emitter, "x1", &label);
        abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
        abi::emit_call_label(ctx.emitter, "__rt_str_persist");
        ctx.emitter.instruction("mov x3, x1");                                  // pass the owned relation name as the hash value pointer
        ctx.emitter.instruction("mov x4, x2");                                  // pass the relation name length as the hash value high word
        abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
        ctx.emitter.instruction("ldr x0, [sp]");                                // reload the current class-relation hash pointer
        abi::emit_load_int_immediate(ctx.emitter, "x5", runtime_str_tag());
        abi::emit_call_label(ctx.emitter, "__rt_hash_set");
        ctx.emitter.instruction("str x0, [sp]");                                // preserve the possibly-grown class-relation hash
    }
    ctx.emitter.instruction("ldr x0, [sp], #16");                               // restore the final class-relation hash as the result
}

/// Appends string-set hash entries on x86_64.
fn emit_string_hash_entries_x86_64(ctx: &mut FunctionContext<'_>, names: &[String]) {
    if names.is_empty() {
        return;
    }
    ctx.emitter.instruction("push rax");                                        // park the class-relation hash while inserting metadata entries
    ctx.emitter.instruction("sub rsp, 8");                                      // keep stack alignment stable across hash helper calls
    for name in names {
        let (label, len) = ctx.data.add_string(name.as_bytes());
        abi::emit_symbol_address(ctx.emitter, "rax", &label);
        abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
        abi::emit_call_label(ctx.emitter, "__rt_hash_normalize_key");
        abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
        abi::emit_symbol_address(ctx.emitter, "rax", &label);
        abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
        abi::emit_call_label(ctx.emitter, "__rt_str_persist");
        ctx.emitter.instruction("mov rcx, rax");                                // pass the owned relation name as the hash value pointer
        ctx.emitter.instruction("mov r8, rdx");                                 // pass the relation name length as the hash value high word
        abi::emit_pop_reg_pair(ctx.emitter, "rsi", "rdx");
        ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 8]");                // reload the current class-relation hash pointer
        abi::emit_load_int_immediate(ctx.emitter, "r9", runtime_str_tag());
        abi::emit_call_label(ctx.emitter, "__rt_hash_set");
        ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");                // preserve the possibly-grown class-relation hash
    }
    ctx.emitter.instruction("add rsp, 8");                                      // drop the temporary alignment slot
    ctx.emitter.instruction("pop rax");                                         // restore the final class-relation hash as the result
}

/// Returns the runtime tag for string hash values.
fn runtime_str_tag() -> i64 {
    crate::codegen::runtime_value_tag(&PhpType::Str) as i64
}

/// Returns a `ConstStr` operand value, or `None` when the operand is not a literal string.
fn optional_const_string_operand(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<Option<String>> {
    let value_ref = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(None);
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    if inst_ref.op != Op::ConstStr {
        return Ok(None);
    }
    let Some(Immediate::Data(data)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(
            "string literal operand has no data id",
        ));
    };
    Ok(Some(ctx
        .module
        .data
        .strings
        .get(data.as_raw() as usize)
        .cloned()
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))?))
}
