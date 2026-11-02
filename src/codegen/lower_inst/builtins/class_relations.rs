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
use crate::ir::{Immediate, Instruction, Op, ValueDef, ValueId};
use crate::names::php_symbol_key;
use crate::types::{ClassInfo, InterfaceInfo, PhpType};

use super::super::super::context::FunctionContext;
use super::{
    expect_operand, has_eval_context,
    lower_eval_class_relation, store_if_result,
};

enum ClassLikeTarget {
    Class(String),
    Interface(String),
    Trait(String),
    Unknown,
}

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
    let target_ty = ctx.value_php_type(target_value)?;
    let runtime_target = match target_ty.codegen_repr() {
        PhpType::Str => optional_const_string_operand(ctx, target_value)?.is_none(),
        PhpType::Mixed => true,
        PhpType::Object(class_name) => lookup_class_name(ctx, &class_name).is_none(),
        _ => false,
    };
    if runtime_target {
        lower_runtime_class_relation(ctx, target_value, name)?;
        return store_if_result(ctx, inst);
    }

    let target = resolve_relation_target(ctx, target_value)?;
    if matches!(target, ClassLikeTarget::Unknown) {
        emit_unknown_relation_answer(ctx, target_value, name)?;
        return store_if_result(ctx, inst);
    }

    let names = relation_names(ctx, name, &target)?;
    emit_string_hash(ctx, &names);
    emit_box_current_value_as_mixed(ctx.emitter, &class_relation_array_type());
    store_if_result(ctx, inst)
}

/// Resolves a runtime object or class-like name through the compact relation registry.
fn lower_runtime_class_relation(
    ctx: &mut FunctionContext<'_>,
    target: ValueId,
    name: &str,
) -> Result<()> {
    let payload_offset = match name {
        "class_implements" => 16,
        "class_parents" => 32,
        "class_uses" => 48,
        _ => {
            return Err(CodegenIrError::unsupported(format!(
                "class-relation builtin {}",
                name
            )));
        }
    };
    materialize_runtime_relation_target_name(ctx, target, name)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // class-like name pointer becomes lookup argument zero
            ctx.emitter.instruction("mov x1, x2");                              // class-like name length becomes lookup argument one
            abi::emit_load_int_immediate(ctx.emitter, "x2", payload_offset);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // class-like name pointer becomes lookup argument zero
            ctx.emitter.instruction("mov rsi, rdx");                            // class-like name length becomes lookup argument one
            abi::emit_load_int_immediate(ctx.emitter, "rdx", payload_offset);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_class_relation_lookup");

    let found_label = ctx.next_label("class_relation_dynamic_found");
    let done_label = ctx.next_label("class_relation_dynamic_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz x0, {}", found_label));      // materialize the relation hash only for a known target
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // distinguish an unknown name from an empty relation list
            ctx.emitter.instruction(&format!("jnz {}", found_label));           // materialize the relation hash only for a known target
        }
    }
    emit_unknown_relation_answer(ctx, target, name)?;
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&found_label);
    if ctx.emitter.target.arch == Arch::AArch64 {
        ctx.emitter.instruction("mov x0, x1");                                  // relation list pointer becomes hash-builder argument zero
        ctx.emitter.instruction("mov x1, x2");                                  // relation list count becomes hash-builder argument one
    }
    abi::emit_call_label(ctx.emitter, "__rt_hash_from_name_list");
    emit_box_current_value_as_mixed(ctx.emitter, &class_relation_array_type());
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Answers a relation probe for a name the compile-time registry does not know.
///
/// With the eval bridge linked the name may belong to a class an eval declared, so the
/// process-global eval metadata is asked first; `false` is PHP's answer for a genuinely unknown
/// name. The boxed answer is left in the result register.
fn emit_unknown_relation_answer(
    ctx: &mut FunctionContext<'_>,
    target: ValueId,
    name: &str,
) -> Result<()> {
    if ctx.module.required_runtime_features.eval_bridge {
        return super::lower_eval_global_class_relation_to_result(ctx, target, name);
    }
    emit_boxed_bool(ctx, false);
    Ok(())
}

/// Materializes an object|string gradual target as a runtime class-like name.
fn materialize_runtime_relation_target_name(
    ctx: &mut FunctionContext<'_>,
    target: ValueId,
    name: &str,
) -> Result<()> {
    match ctx.value_php_type(target)?.codegen_repr() {
        PhpType::Object(_) => {
            ctx.load_value_to_result(target)?;
            super::types::emit_dynamic_object_class_name(ctx, name);
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            ctx.load_string_value_to_regs(target, ptr_reg, len_reg)?;
        }
        PhpType::Mixed => materialize_gradual_relation_target_name(ctx, target, name)?,
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "{} target PHP type {:?}",
                name, other
            )));
        }
    }
    Ok(())
}

/// Unboxes a gradual class-relation target and accepts only runtime object or string payloads.
fn materialize_gradual_relation_target_name(
    ctx: &mut FunctionContext<'_>,
    target: ValueId,
    name: &str,
) -> Result<()> {
    let object_label = ctx.next_label("class_relation_dynamic_object");
    let string_label = ctx.next_label("class_relation_dynamic_string");
    let ready_label = ctx.next_label("class_relation_dynamic_target_ready");
    ctx.load_value_to_result(target)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #6");                              // boxed object targets resolve through their runtime class id
            ctx.emitter.instruction(&format!("b.eq {}", object_label));
            ctx.emitter.instruction("cmp x0, #1");                              // boxed string targets already carry a class-like name
            ctx.emitter.instruction(&format!("b.eq {}", string_label));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 6");                              // boxed object targets resolve through their runtime class id
            ctx.emitter.instruction(&format!("je {}", object_label));
            ctx.emitter.instruction("cmp rax, 1");                              // boxed string targets already carry a class-like name
            ctx.emitter.instruction(&format!("je {}", string_label));
        }
    }
    super::super::exceptions::emit_type_error(
        ctx,
        &format!(
            "{}(): Argument #1 ($object_or_class) must be of type object|string, mixed given",
            name
        ),
    );

    ctx.emitter.label(&object_label);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("mov x0, x1"),                 // object payload becomes get_class input
        Arch::X86_64 => ctx.emitter.instruction("mov rax, rdi"),                // object payload becomes get_class input
    }
    super::types::emit_dynamic_object_class_name(ctx, name);
    abi::emit_jump(ctx.emitter, &ready_label);

    ctx.emitter.label(&string_label);
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rax, rdi");                                // move the unboxed string pointer into result convention
        ctx.emitter.instruction("mov rdx, rsi");                                // move the unboxed string length into result convention
    }
    ctx.emitter.label(&ready_label);
    Ok(())
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
fn resolve_relation_target(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<ClassLikeTarget> {
    match ctx.value_php_type(value)? {
        PhpType::Object(class_name) => Ok(lookup_class_name(ctx, &class_name)
            .map(ClassLikeTarget::Class)
            .unwrap_or(ClassLikeTarget::Unknown)),
        PhpType::Str => {
            let Some(raw) = optional_const_string_operand(ctx, value)? else {
                return Err(CodegenIrError::unsupported(
                    "class-relation builtin with non-literal class name",
                ));
            };
            if let Some(name) = lookup_class_name(ctx, &raw) {
                return Ok(ClassLikeTarget::Class(name));
            }
            if let Some(name) = lookup_interface_name(ctx, &raw) {
                return Ok(ClassLikeTarget::Interface(name));
            }
            if let Some(name) = lookup_trait_name(ctx, &raw) {
                return Ok(ClassLikeTarget::Trait(name));
            }
            Ok(ClassLikeTarget::Unknown)
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
    match name {
        "class_implements" => Ok(class_implements(ctx, target)),
        "class_parents" => Ok(class_parents(ctx, target)),
        "class_uses" => Ok(class_uses(ctx, target)),
        _ => Err(CodegenIrError::unsupported(format!(
            "class-relation builtin {}",
            name
        ))),
    }
}

/// Computes implemented interface names for a class or parent interfaces for an interface.
fn class_implements(ctx: &FunctionContext<'_>, target: &ClassLikeTarget) -> Vec<String> {
    match target {
        ClassLikeTarget::Class(class_name) => lookup_class(ctx, class_name)
            .map(|info| info.interfaces.clone())
            .unwrap_or_default(),
        ClassLikeTarget::Interface(interface_name) => {
            let mut names = Vec::new();
            collect_interface_parents(ctx, interface_name, &mut names);
            names
        }
        ClassLikeTarget::Trait(_) | ClassLikeTarget::Unknown => Vec::new(),
    }
}

/// Computes parent class names from the immediate parent through ancestors.
fn class_parents(ctx: &FunctionContext<'_>, target: &ClassLikeTarget) -> Vec<String> {
    let ClassLikeTarget::Class(class_name) = target else {
        return Vec::new();
    };

    let mut names = Vec::new();
    let mut current = class_name.clone();
    while let Some(info) = lookup_class(ctx, &current) {
        let Some(parent) = &info.parent else {
            break;
        };
        let parent_name = lookup_class_name(ctx, parent).unwrap_or_else(|| parent.clone());
        names.push(parent_name.clone());
        current = parent_name;
    }
    names
}

/// Computes direct trait uses for classes or trait declarations.
fn class_uses(ctx: &FunctionContext<'_>, target: &ClassLikeTarget) -> Vec<String> {
    match target {
        ClassLikeTarget::Class(class_name) => lookup_class(ctx, class_name)
            .map(|info| info.used_traits.clone())
            .unwrap_or_default(),
        ClassLikeTarget::Trait(trait_name) => ctx
            .module
            .declared_trait_uses
            .get(trait_name)
            .cloned()
            .unwrap_or_default(),
        ClassLikeTarget::Interface(_) | ClassLikeTarget::Unknown => Vec::new(),
    }
}

/// Collects parent interfaces without duplicates.
pub(super) fn collect_interface_parents(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
    names: &mut Vec<String>,
) {
    let Some(interface) = lookup_interface(ctx, interface_name) else {
        return;
    };
    for parent in &interface.parents {
        let parent_name = lookup_interface_name(ctx, parent).unwrap_or_else(|| parent.clone());
        if !names
            .iter()
            .any(|name| php_symbol_key(name) == php_symbol_key(&parent_name))
        {
            names.push(parent_name.clone());
            collect_interface_parents(ctx, &parent_name, names);
        }
    }
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

/// Looks up a class by PHP-style case-insensitive name.
fn lookup_class<'a>(ctx: &'a FunctionContext<'_>, name: &str) -> Option<&'a ClassInfo> {
    let name = lookup_class_name(ctx, name)?;
    ctx.module.class_infos.get(&name)
}

/// Looks up an interface by PHP-style case-insensitive name.
pub(super) fn lookup_interface<'a>(
    ctx: &'a FunctionContext<'_>,
    name: &str,
) -> Option<&'a InterfaceInfo> {
    let name = lookup_interface_name(ctx, name)?;
    ctx.module.interface_infos.get(&name)
}

/// Looks up a class name by PHP-style case-insensitive name.
fn lookup_class_name(ctx: &FunctionContext<'_>, raw: &str) -> Option<String> {
    lookup_folded(ctx.module.class_infos.keys(), raw)
}

/// Looks up an interface name by PHP-style case-insensitive name.
fn lookup_interface_name(ctx: &FunctionContext<'_>, raw: &str) -> Option<String> {
    lookup_folded(ctx.module.interface_infos.keys(), raw)
}

/// Looks up a trait name by PHP-style case-insensitive name.
fn lookup_trait_name(ctx: &FunctionContext<'_>, raw: &str) -> Option<String> {
    lookup_folded(ctx.module.trait_table.names.iter(), raw)
}

/// Returns a matching symbol name using PHP case-insensitive comparison.
fn lookup_folded<'a>(names: impl Iterator<Item = &'a String>, raw: &str) -> Option<String> {
    let clean = raw.trim_start_matches('\\');
    let key = php_symbol_key(clean);
    names
        .into_iter()
        .find(|name| php_symbol_key(name.trim_start_matches('\\')) == key)
        .cloned()
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
        .map(|value| crate::names::literal_symbol_name(&value).unwrap_or(value))
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))?))
}
