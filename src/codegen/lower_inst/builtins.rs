//! Purpose:
//! Lowers the first scalar PHP builtin calls emitted as EIR `BuiltinCall` instructions.
//! Covers concrete scalar casts, type predicates, selected Mixed tag predicates, and string length.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - Runtime conversions reuse existing target-aware helpers instead of duplicating parsing logic.
//! - Selected Mixed predicates inspect the boxed runtime tag through shared predicate lowering.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::ir::{Immediate, Instruction, Op, ValueDef, ValueId};
use crate::names::{define_seen_symbol, ir_global_symbol, php_symbol_key};
use crate::parser::ast::Visibility;
use crate::types::checker::builtins::is_php_visible_builtin_function;
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{expect_data, expect_operand, load_value_to_first_int_arg, predicates, store_if_result};
use crate::codegen::{CodegenIrError, Result};

pub(crate) mod attributes;
pub(crate) mod arrays;
mod buffers;
pub(crate) mod class_relations;
pub(crate) mod ctype;
pub(crate) mod debug;
pub(crate) mod io;
mod isset;
pub(crate) mod is_numeric;
pub(crate) mod json;
pub(crate) mod math;
pub(crate) mod pointers;
pub(crate) mod regex;
pub(crate) mod serialize;
pub(crate) mod spl;
pub(crate) mod system;
pub(crate) mod strings;
pub(crate) mod types;

const DEFINE_ALREADY_DEFINED_WARNING: &str =
    "Warning: define(): Constant already defined\n";

/// Lowers a scalar builtin call by matching the canonical PHP function name.
///
/// Consults the builtin registry first using the canonical key; falls back to the
/// legacy match table when the name is not registered. This makes the registry the
/// authoritative dispatch path while keeping the legacy emitters as a fallback.
pub(super) fn lower_builtin_call(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let name = ctx.function_name_data(expect_data(inst)?)?;
    let key = php_symbol_key(name.trim_start_matches('\\'));
    // Registry-first: if the builtin is registered, invoke its lowering hook.
    // Falls through to the legacy match when the name is not registered.
    if let Some(def) = crate::builtins::registry::lookup(key.as_str()) {
        return (def.spec.lower)(ctx, inst);
    }
    match key.as_str() {
        "closure_bind" => lower_closure_bind(ctx, inst),
        "buffer_len" => buffers::lower_buffer_len(ctx, inst),
        "buffer_free" => buffers::lower_buffer_free(ctx, inst),

        "empty" => lower_empty(ctx, inst),
        "unset" => types::lower_unset_builtin(ctx, inst),
        "isset" => isset::lower_isset(ctx, inst),
        "exit" | "die" => system::lower_exit(ctx, inst),
        _ => Err(CodegenIrError::unsupported(format!("builtin call {}", name))),
    }
}

/// Lowers an EIR native indexed-array `isset($array[$offset])` probe.
pub(super) fn lower_array_isset(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    isset::lower_array_isset(ctx, inst)
}

/// Lowers an EIR native associative-array `isset($hash[$key])` probe.
pub(super) fn lower_hash_isset(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    isset::lower_hash_isset(ctx, inst)
}

/// Lowers `define("NAME", value)` with the legacy duplicate-name runtime guard.
pub(crate) fn lower_define(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "define", 2)?;
    let name_value = expect_operand(inst, 0)?;
    let value = expect_operand(inst, 1)?;
    let constant_name = const_string_operand(ctx, name_value)?;
    let flag_symbol = ctx.data.add_comm(define_seen_symbol(&constant_name), 8);
    let global_symbol = ir_global_symbol(&constant_name);
    let value_ty = ctx.value_php_type(value)?;
    ctx.data
        .add_comm(global_symbol.clone(), value_ty.codegen_repr().stack_size().max(8));

    let first_label = ctx.next_label("define_first");
    let done_label = ctx.next_label("define_done");
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_symbol_to_reg(ctx.emitter, result_reg, &flag_symbol, 0);
    abi::emit_branch_if_int_result_zero(ctx.emitter, &first_label);
    emit_duplicate_define_warning(ctx);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&first_label);
    ctx.load_value_to_result(value)?;
    abi::emit_store_result_to_symbol(ctx.emitter, &global_symbol, &value_ty, false);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 1);
    abi::emit_store_reg_to_symbol(ctx.emitter, result_reg, &flag_symbol, 0);

    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Emits the PHP warning for a repeated `define()` call.
fn emit_duplicate_define_warning(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.adrp("x1", "_diag_define_already_defined_msg");
            ctx.emitter.add_lo12("x1", "x1", "_diag_define_already_defined_msg");
            ctx.emitter.instruction(&format!("mov x2, #{}", DEFINE_ALREADY_DEFINED_WARNING.len())); // pass the duplicate-define warning byte length
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("lea rdi, [rip + _diag_define_already_defined_msg]"); // pass the duplicate-define warning pointer
            ctx.emitter.instruction(&format!("mov esi, {}", DEFINE_ALREADY_DEFINED_WARNING.len())); // pass the duplicate-define warning byte length
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
}

/// Lowers `gettype(value)` for statically concrete PHP types.
pub(crate) fn lower_gettype(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "gettype", 1)?;
    let value = expect_operand(inst, 0)?;
    let ty = ctx.raw_value_php_type(value)?;
    if matches!(ty, PhpType::TaggedScalar) {
        emit_tagged_scalar_gettype(ctx, value)?;
        return store_if_result(ctx, inst);
    }
    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        emit_mixed_gettype(ctx, value)?;
        return store_if_result(ctx, inst);
    }
    let Some(type_name) = static_gettype_name(&ty) else {
        return Err(CodegenIrError::unsupported(format!(
            "gettype for PHP type {:?}",
            ty
        )));
    };
    emit_type_name_result(ctx, type_name);
    store_if_result(ctx, inst)
}

/// Emits `gettype()` for an inline tagged scalar by dispatching on its tag word.
fn emit_tagged_scalar_gettype(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let null_case = ctx.next_label("gettype_tagged_null");
    let done = ctx.next_label("gettype_tagged_done");
    ctx.load_value_to_result(value)?;
    crate::codegen::sentinels::emit_branch_if_tagged_scalar_null(ctx.emitter, &null_case);
    emit_type_name_result(ctx, b"integer");
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&null_case);
    emit_type_name_result(ctx, b"NULL");
    ctx.emitter.label(&done);
    Ok(())
}

/// Emits `gettype()` for a boxed Mixed or Union payload by dispatching on runtime tags.
fn emit_mixed_gettype(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let integer_case = ctx.next_label("gettype_mixed_integer");
    let double_case = ctx.next_label("gettype_mixed_double");
    let string_case = ctx.next_label("gettype_mixed_string");
    let boolean_case = ctx.next_label("gettype_mixed_boolean");
    let null_case = ctx.next_label("gettype_mixed_null");
    let array_case = ctx.next_label("gettype_mixed_array");
    let object_case = ctx.next_label("gettype_mixed_object");
    let resource_case = ctx.next_label("gettype_mixed_resource");
    let done = ctx.next_label("gettype_mixed_done");
    ctx.load_value_to_result(value)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_branch_on_gettype_mixed_tag(ctx, 0, &integer_case);
    emit_branch_on_gettype_mixed_tag(ctx, 1, &string_case);
    emit_branch_on_gettype_mixed_tag(ctx, 2, &double_case);
    emit_branch_on_gettype_mixed_tag(ctx, 3, &boolean_case);
    emit_branch_on_gettype_mixed_tag(ctx, 4, &array_case);
    emit_branch_on_gettype_mixed_tag(ctx, 5, &array_case);
    emit_branch_on_gettype_mixed_tag(ctx, 6, &object_case);
    emit_branch_on_gettype_mixed_tag(ctx, 9, &resource_case);
    abi::emit_jump(ctx.emitter, &null_case);

    emit_mixed_gettype_case(ctx, &integer_case, b"integer", &done);
    emit_mixed_gettype_case(ctx, &double_case, b"double", &done);
    emit_mixed_gettype_case(ctx, &string_case, b"string", &done);
    emit_mixed_gettype_case(ctx, &boolean_case, b"boolean", &done);
    emit_mixed_gettype_case(ctx, &null_case, b"NULL", &done);
    emit_mixed_gettype_case(ctx, &array_case, b"array", &done);
    emit_mixed_gettype_case(ctx, &object_case, b"object", &done);
    emit_mixed_gettype_case(ctx, &resource_case, b"resource", &done);
    ctx.emitter.label(&done);
    Ok(())
}

/// Branches to a `gettype()` case when the unboxed Mixed runtime tag matches.
fn emit_branch_on_gettype_mixed_tag(ctx: &mut FunctionContext<'_>, tag: u8, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp x0, #{}", tag));              // compare the unboxed Mixed tag against this gettype() case
            ctx.emitter.instruction(&format!("b.eq {}", label));                // branch to the matching gettype() type-name case
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp rax, {}", tag));              // compare the unboxed Mixed tag against this gettype() case
            ctx.emitter.instruction(&format!("je {}", label));                  // branch to the matching gettype() type-name case
        }
    }
}

/// Selects one static PHP type-name string and rejoins the `gettype()` dispatch.
fn emit_mixed_gettype_case(ctx: &mut FunctionContext<'_>, label: &str, type_name: &[u8], done: &str) {
    ctx.emitter.label(label);
    emit_type_name_result(ctx, type_name);
    abi::emit_jump(ctx.emitter, done);
}

/// Returns PHP's `gettype()` spelling for concrete statically known types.
fn static_gettype_name(ty: &PhpType) -> Option<&'static [u8]> {
    match ty {
        PhpType::Int => Some(b"integer".as_slice()),
        PhpType::Float => Some(b"double".as_slice()),
        PhpType::Str => Some(b"string".as_slice()),
        PhpType::Bool => Some(b"boolean".as_slice()),
        PhpType::Void | PhpType::Never => Some(b"NULL".as_slice()),
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable => {
            Some(b"array".as_slice())
        }
        PhpType::Callable => Some(b"callable".as_slice()),
        PhpType::Object(_) => Some(b"object".as_slice()),
        PhpType::Pointer(_) => Some(b"pointer".as_slice()),
        PhpType::Buffer(_) => Some(b"buffer".as_slice()),
        PhpType::Packed(_) => Some(b"packed".as_slice()),
        PhpType::Resource(_) => Some(b"resource".as_slice()),
        PhpType::Mixed | PhpType::Union(_) | PhpType::TaggedScalar => None,
    }
}

/// Emits a static PHP type-name string into the target string result registers.
fn emit_type_name_result(ctx: &mut FunctionContext<'_>, type_name: &[u8]) {
    let (label, len) = ctx.data.add_string(type_name);
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
}

/// Lowers `phpversion()` as the compiler package version string.
pub(crate) fn lower_phpversion(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "phpversion", 0)?;
    let (label, len) = ctx.data.add_string(env!("CARGO_PKG_VERSION").as_bytes());
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
    store_if_result(ctx, inst)
}

/// Lowers `defined("NAME")` for compile-time string constant names.
pub(crate) fn lower_defined(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "defined", 1)?;
    let value = expect_operand(inst, 0)?;
    let constant_name = const_string_operand(ctx, value)?;
    emit_static_bool(ctx, ctx.has_global_name(&constant_name));
    store_if_result(ctx, inst)
}

/// Lowers `function_exists("name")` for compile-time string names.
///
/// Recognizes user functions, externs, catalog builtins, and the date/time procedural aliases
/// that `name_resolver` desugars (including the injected timezone-introspection prelude
/// functions). The aliases are matched through `is_date_procedural_alias` rather than the catalog
/// because their call sites are rewritten before codegen, so they never reach the builtin catalog
/// yet must still report as existing to match PHP.
pub(crate) fn lower_function_exists(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "function_exists", 1)?;
    let value = expect_operand(inst, 0)?;
    let function_name = const_string_operand(ctx, value)?;
    if let Some(group_name) = ctx.function_variant_group_name(&function_name) {
        emit_variant_function_exists(ctx, &group_name);
    } else {
        let exists = ctx.function_by_name(&function_name).is_some()
            || ctx.has_extern_function(&function_name)
            || is_php_visible_builtin_function(function_name.trim_start_matches('\\'))
            || crate::name_resolver::is_date_procedural_alias(&function_name);
        emit_static_bool(ctx, exists);
    }
    store_if_result(ctx, inst)
}

/// Lowers AOT class/interface/enum existence checks for literal names.
pub(crate) fn lower_class_like_exists(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    ensure_arg_count_between(inst, name, 1, 2)?;
    let value = expect_operand(inst, 0)?;
    let symbol_name = const_string_operand(ctx, value)?;
    let exists = match name {
        "class_exists" => contains_folded(
            ctx.module
                .class_infos
                .keys()
                .filter(|class_name| !is_internal_synthetic_class_name(class_name)),
            &symbol_name,
        ),
        "interface_exists" => contains_folded(ctx.module.interface_infos.keys(), &symbol_name),
        "trait_exists" => contains_folded(ctx.module.trait_table.names.iter(), &symbol_name),
        "enum_exists" => contains_folded(ctx.module.enum_infos.keys(), &symbol_name),
        _ => false,
    };
    emit_static_bool(ctx, exists);
    store_if_result(ctx, inst)
}

/// Lowers `is_callable(value)` through static lookup or runtime callable-shape helpers.
pub(crate) fn lower_is_callable(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "is_callable", 1)?;
    let value = expect_operand(inst, 0)?;
    match ctx.value_php_type(value)?.codegen_repr() {
        PhpType::Callable => emit_static_bool(ctx, true),
        PhpType::Str => {
            if let Ok(function_name) = const_string_operand(ctx, value) {
                if let Some((class_name, method_name)) = function_name.rsplit_once("::") {
                    emit_static_bool(ctx, static_method_string_is_callable(ctx, class_name, method_name));
                } else {
                    emit_static_bool(ctx, callable_name_exists(ctx, &function_name));
                }
            } else {
                ctx.load_value_to_result(value)?;
                emit_is_callable_dynamic_string_lookup(ctx);
            }
        }
        PhpType::Array(_) => {
            ctx.load_value_to_result(value)?;
            emit_is_callable_pointer_lookup(ctx, "__rt_is_callable_array");
        }
        PhpType::AssocArray { .. } => {
            ctx.load_value_to_result(value)?;
            emit_is_callable_pointer_lookup(ctx, "__rt_is_callable_assoc");
        }
        PhpType::Object(_) => {
            ctx.load_value_to_result(value)?;
            emit_is_callable_pointer_lookup(ctx, "__rt_is_callable_object");
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_result(value)?;
            emit_is_callable_pointer_lookup(ctx, "__rt_is_callable_mixed");
        }
        PhpType::Iterable => {
            ctx.load_value_to_result(value)?;
            emit_is_callable_pointer_lookup(ctx, "__rt_is_callable_heap");
        }
        PhpType::Int
        | PhpType::Bool
        | PhpType::Float
        | PhpType::Void
        | PhpType::Never
        | PhpType::Pointer(_)
        | PhpType::Buffer(_)
        | PhpType::Packed(_)
        | PhpType::Resource(_)
        | PhpType::TaggedScalar => {
            emit_static_bool(ctx, false);
        }
    }
    store_if_result(ctx, inst)
}

/// Calls the runtime `is_callable` helper for pointer-shaped values already in result regs.
fn emit_is_callable_pointer_lookup(ctx: &mut FunctionContext<'_>, label: &str) {
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // move pointer-shaped value into helper argument 0
    }
    abi::emit_call_label(ctx.emitter, label);
}

/// Calls the runtime `is_callable` string-name helper for a loaded dynamic string value.
fn emit_is_callable_dynamic_string_lookup(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // move string pointer into helper argument 0
            ctx.emitter.instruction("mov x1, x2");                              // move string length into helper argument 1
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // move string pointer into helper argument 0
            ctx.emitter.instruction("mov rsi, rdx");                            // move string length into helper argument 1
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_is_callable_string");
}

/// Returns true when a static `Class::method` string names a public static method.
fn static_method_string_is_callable(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    method_name: &str,
) -> bool {
    let class_key = php_symbol_key(class_name.trim_start_matches('\\'));
    let Some((_, class_info)) = ctx.module.class_infos.iter().find(|(candidate, _)| {
        php_symbol_key(candidate.trim_start_matches('\\')) == class_key
    }) else {
        return false;
    };
    let method_key = php_symbol_key(method_name);
    if !class_info.static_methods.contains_key(&method_key) {
        return false;
    }
    class_info.static_method_visibilities.get(&method_key) == Some(&Visibility::Public)
}

/// Emits a runtime check for whether an include-loaded function variant is active.
fn emit_variant_function_exists(ctx: &mut FunctionContext<'_>, function_name: &str) {
    let active_symbol = crate::names::function_variant_active_symbol(function_name);
    ctx.data.add_comm(active_symbol.clone(), 8);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_symbol_to_reg(ctx.emitter, result_reg, &active_symbol, 0);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, #0", result_reg));        // test whether an include has activated this function variant
            ctx.emitter.instruction(&format!("cset {}, ne", result_reg));       // return true only when a function variant is active
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {}, {}", result_reg, result_reg)); // test whether an include has activated this function variant
            ctx.emitter.instruction("setne al");                                // return true only when a function variant is active
            ctx.emitter.instruction("movzx rax, al");                           // widen the boolean byte into the integer result register
        }
    }
}

/// Lowers `count(array)` for concrete array values by reading the runtime length header.
///
/// Called from `crate::builtins::array::count` (the registry home) via a thin wrapper.
/// Handles Array/AssocArray (reads length directly from the runtime header), Mixed/Union
/// (delegates to `__rt_mixed_count`), and Countable Object (calls the object's `count`
/// method via intrinsic or dynamic dispatch).
pub(crate) fn lower_count(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "count", 1)?;
    let value = expect_operand(inst, 0)?;
    let ty = ctx.value_php_type(value)?.codegen_repr();
    match ty {
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            ctx.load_value_to_result(value)?;
            let result_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_load_from_address(ctx.emitter, result_reg, result_reg, 0);
            store_if_result(ctx, inst)
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_count");
            store_if_result(ctx, inst)
        }
        PhpType::Object(class_name)
            if super::class_implements_interface(ctx, &class_name, "Countable") =>
        {
            if let Some(intrinsic) = super::runtime_backed_instance_intrinsic(&class_name, "count") {
                super::lower_instance_runtime_intrinsic(ctx, inst, &class_name, "count", intrinsic)
            } else {
                super::lower_runtime_object_method_call(ctx, inst, &class_name, "count")
            }
        }
        other => Err(CodegenIrError::unsupported(format!(
            "count for PHP type {:?}",
            other
        ))),
    }
}

/// Lowers the synthetic `closure_bind` call: rebinds a closure's captured
/// `$this` to a new receiver via `__rt_closure_bind(descriptor, new_this)`,
/// returning the rebound closure descriptor.
fn lower_closure_bind(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "closure_bind", 2)?;
    let descriptor = expect_operand(inst, 0)?;
    let new_this = expect_operand(inst, 1)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(descriptor, "x0")?;
            ctx.load_value_to_reg(new_this, "x1")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(descriptor, "rdi")?;
            ctx.load_value_to_reg(new_this, "rsi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_closure_bind");
    store_if_result(ctx, inst)
}

/// Lowers `strlen()` by coercing string-like values and returning the byte length.
pub(crate) fn lower_strlen(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "strlen", 1)?;
    let value = expect_operand(inst, 0)?;
    let ty = ctx.load_value_to_result(value)?;
    match ty.codegen_repr() {
        PhpType::Str => {}
        PhpType::Mixed | PhpType::Union(_) => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "strlen for PHP type {:?}",
                other
            )));
        }
    }
    let result_reg = abi::int_result_reg(ctx.emitter);
    let len_reg = abi::string_result_regs(ctx.emitter).1;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov {}, {}", result_reg, len_reg)); // return the byte length of the loaded PHP string
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov {}, {}", result_reg, len_reg)); // return the byte length of the loaded PHP string
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `intval()` for concrete scalar operands.
pub(crate) fn lower_intval(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "intval", 1)?;
    let value = expect_operand(inst, 0)?;
    match ctx.value_php_type(value)? {
        PhpType::Int | PhpType::Bool => {
            ctx.load_value_to_result(value)?;
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
            abi::emit_float_result_to_int_result(ctx.emitter);
        }
        PhpType::Str => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_str_to_int");
        }
        PhpType::Mixed | PhpType::Union(_) => {
            load_value_to_first_int_arg(ctx, value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "intval for PHP type {:?}",
                other
            )))
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `floatval()` for concrete scalar operands.
pub(crate) fn lower_floatval(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "floatval", 1)?;
    let value = expect_operand(inst, 0)?;
    match ctx.value_php_type(value)? {
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
        }
        PhpType::Int | PhpType::Bool => {
            ctx.load_value_to_result(value)?;
            abi::emit_int_result_to_float_result(ctx.emitter);
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            abi::emit_int_result_to_float_result(ctx.emitter);
        }
        PhpType::Str => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_str_to_number");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "floatval for PHP type {:?}",
                other
            )))
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `boolval()` using the same concrete scalar PHP truthiness rules as `IsTruthy`.
pub(crate) fn lower_boolval(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "boolval", 1)?;
    let value = expect_operand(inst, 0)?;
    match ctx.value_php_type(value)? {
        PhpType::Bool | PhpType::Int => {
            ctx.load_value_to_result(value)?;
            predicates::emit_int_result_nonzero_bool(ctx);
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
            predicates::emit_float_result_nonzero_bool(ctx);
        }
        PhpType::Str => {
            predicates::emit_string_truthiness(ctx, value)?;
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable => {
            predicates::emit_array_truthiness(ctx, value)?;
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "boolval for PHP type {:?}",
                other
            )))
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `empty()` for concrete scalar and array-like operands.
fn lower_empty(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "empty", 1)?;
    let value = expect_operand(inst, 0)?;
    match ctx.raw_value_php_type(value)? {
        PhpType::Int | PhpType::Bool | PhpType::Pointer(_) => {
            ctx.load_value_to_result(value)?;
            emit_int_result_zero_bool(ctx);
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
        }
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
            emit_float_result_zero_bool(ctx);
        }
        PhpType::Str => {
            ctx.load_value_to_result(value)?;
            emit_string_length_zero_bool(ctx);
        }
        PhpType::TaggedScalar => {
            emit_tagged_scalar_empty_bool(ctx, value)?;
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable => {
            predicates::emit_array_truthiness(ctx, value)?;
            invert_bool_result(ctx);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_is_empty");
        }
        PhpType::Callable | PhpType::Object(_) | PhpType::Resource(_) => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "empty for PHP type {:?}",
                other
            )))
        }
    }
    store_if_result(ctx, inst)
}

/// Emits true for a tagged scalar that is null or an integer zero.
fn emit_tagged_scalar_empty_bool(ctx: &mut FunctionContext<'_>, value: crate::ir::ValueId) -> Result<()> {
    let empty_label = ctx.next_label("empty_tagged_true");
    let done_label = ctx.next_label("empty_tagged_done");
    ctx.load_value_to_result(value)?;
    crate::codegen::sentinels::emit_branch_if_tagged_scalar_null(ctx.emitter, &empty_label);
    emit_int_result_zero_bool(ctx);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&empty_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits true when the canonical integer result register is zero.
fn emit_int_result_zero_bool(ctx: &mut FunctionContext<'_>) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, #0", result_reg));        // compare the empty() integer operand against zero
            ctx.emitter.instruction(&format!("cset {}, eq", result_reg));       // return true when the integer operand is zero
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, 0", result_reg));         // compare the empty() integer operand against zero
            ctx.emitter.instruction("sete al");                                 // materialize true when the integer operand is zero
            ctx.emitter.instruction("movzx rax, al");                           // widen the boolean byte into the integer result register
        }
    }
}

/// Emits true when the canonical float result register is zero.
fn emit_float_result_zero_bool(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fcmp d0, #0.0");                           // compare the empty() float operand against zero
            ctx.emitter.instruction("cset x0, eq");                             // return true when the float operand is zero
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("xorpd xmm1, xmm1");                        // materialize a zero float register for empty() comparison
            ctx.emitter.instruction("ucomisd xmm0, xmm1");                      // compare the empty() float operand against zero
            ctx.emitter.instruction("sete al");                                 // materialize true when the float operand is zero
            ctx.emitter.instruction("movzx rax, al");                           // widen the boolean byte into the integer result register
        }
    }
}

/// Emits true when the loaded string length register is zero.
fn emit_string_length_zero_bool(ctx: &mut FunctionContext<'_>) {
    let len_reg = abi::string_result_regs(ctx.emitter).1;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, #0", len_reg));           // compare the empty() string length against zero
            ctx.emitter.instruction("cset x0, eq");                             // return true when the string length is zero
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, 0", len_reg));            // compare the empty() string length against zero
            ctx.emitter.instruction("sete al");                                 // materialize true when the string length is zero
            ctx.emitter.instruction("movzx rax, al");                           // widen the boolean byte into the integer result register
        }
    }
}

/// Inverts a canonical 0/1 boolean result in the integer result register.
fn invert_bool_result(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("eor x0, x0, #1");                          // invert the canonical boolean result for empty()
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("xor rax, 1");                              // invert the canonical boolean result for empty()
        }
    }
}

/// Lowers a static `is_*` predicate for concrete non-Mixed values.
pub(crate) fn lower_static_type_predicate(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    expected: PhpType,
) -> Result<()> {
    ensure_arg_count(inst, name, 1)?;
    let value = expect_operand(inst, 0)?;
    let ty = ctx.value_php_type(value)?;
    if ty == PhpType::TaggedScalar {
        if expected == PhpType::Int {
            emit_tagged_scalar_int_predicate(ctx, value)?;
        } else {
            emit_static_bool(ctx, false);
        }
        return store_if_result(ctx, inst);
    }
    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        if let Some(tag) = mixed_type_predicate_tag(&expected) {
            predicates::emit_mixed_tag_eq(ctx, value, tag)?;
        } else {
            emit_static_bool(ctx, false);
        }
        return store_if_result(ctx, inst);
    }
    emit_static_bool(ctx, ty == expected);
    store_if_result(ctx, inst)
}

/// Emits `is_int()` for a tagged scalar by checking that its tag is not null.
fn emit_tagged_scalar_int_predicate(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
) -> Result<()> {
    ctx.load_value_to_result(value)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            let cmp_inst = format!(
                "cmp x1, #{}",
                crate::codegen::sentinels::TAGGED_SCALAR_TAG_NULL
            );
            ctx.emitter.instruction(&cmp_inst);                                 // does the tagged scalar carry the runtime null tag?
            ctx.emitter.instruction("cset x0, ne");                             // materialize true when the tagged scalar holds an integer
        }
        Arch::X86_64 => {
            let cmp_inst = format!(
                "cmp rdx, {}",
                crate::codegen::sentinels::TAGGED_SCALAR_TAG_NULL
            );
            ctx.emitter.instruction(&cmp_inst);                                 // does the tagged scalar carry the runtime null tag?
            ctx.emitter.instruction("setne al");                                // materialize true when the tagged scalar holds an integer
            ctx.emitter.instruction("movzx rax, al");                           // widen the boolean byte into the integer result register
        }
    }
    Ok(())
}

/// Lowers `is_iterable()` for concrete values and boxed Mixed payloads.
pub(crate) fn lower_is_iterable(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "is_iterable", 1)?;
    let value = expect_operand(inst, 0)?;
    let ty = ctx.value_php_type(value)?;
    let result = match ty {
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable => true,
        PhpType::Object(name) => object_type_implements_iterable(ctx, &name),
        PhpType::Int
        | PhpType::Float
        | PhpType::Str
        | PhpType::Bool
        | PhpType::Void
        | PhpType::Never
        | PhpType::Callable
        | PhpType::Pointer(_)
        | PhpType::Buffer(_)
        | PhpType::Packed(_)
        | PhpType::Resource(_)
        | PhpType::TaggedScalar => false,
        PhpType::Mixed | PhpType::Union(_) => {
            emit_mixed_is_iterable(ctx, value)?;
            return store_if_result(ctx, inst);
        }
    };
    emit_static_bool(ctx, result);
    store_if_result(ctx, inst)
}

/// Emits runtime `is_iterable()` checks for a boxed Mixed or Union value.
fn emit_mixed_is_iterable(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let true_case = ctx.next_label("is_iterable_mixed_true");
    let object_case = ctx.next_label("is_iterable_mixed_object");
    let done = ctx.next_label("is_iterable_mixed_done");
    let ty = ctx.load_value_to_result(value)?;
    if !matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        return Err(CodegenIrError::unsupported(format!(
            "is_iterable Mixed check for PHP type {:?}",
            ty
        )));
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #4");                              // check for a boxed indexed-array payload
            ctx.emitter.instruction(&format!("b.eq {}", true_case));            // indexed arrays satisfy is_iterable
            ctx.emitter.instruction("cmp x0, #5");                              // check for a boxed associative-array payload
            ctx.emitter.instruction(&format!("b.eq {}", true_case));            // associative arrays satisfy is_iterable
            ctx.emitter.instruction("cmp x0, #6");                              // check for a boxed object payload
            ctx.emitter.instruction(&format!("b.eq {}", object_case));          // objects need a Traversable interface check
            ctx.emitter.instruction("mov x0, #0");                              // all other Mixed payloads are not iterable
            ctx.emitter.instruction(&format!("b {}", done));                    // skip the truthy result path
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 4");                              // check for a boxed indexed-array payload
            ctx.emitter.instruction(&format!("je {}", true_case));              // indexed arrays satisfy is_iterable
            ctx.emitter.instruction("cmp rax, 5");                              // check for a boxed associative-array payload
            ctx.emitter.instruction(&format!("je {}", true_case));              // associative arrays satisfy is_iterable
            ctx.emitter.instruction("cmp rax, 6");                              // check for a boxed object payload
            ctx.emitter.instruction(&format!("je {}", object_case));            // objects need a Traversable interface check
            ctx.emitter.instruction("mov rax, 0");                              // all other Mixed payloads are not iterable
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip the truthy result path
        }
    }
    ctx.emitter.label(&object_case);
    emit_runtime_object_iterable_check(ctx, &true_case, &done);
    ctx.emitter.label(&true_case);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
    ctx.emitter.label(&done);
    Ok(())
}

/// Emits the object half of runtime `is_iterable()` by checking Traversable interfaces.
fn emit_runtime_object_iterable_check(
    ctx: &mut FunctionContext<'_>,
    true_case: &str,
    done: &str,
) {
    let object_true = ctx.next_label("is_iterable_object_true");
    let interface_ids = traversable_interface_ids(ctx);
    if interface_ids.is_empty() {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        abi::emit_jump(ctx.emitter, done);
        return;
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str x1, [sp, #-16]!");                     // preserve the unboxed object pointer across Traversable checks
            for interface_id in interface_ids {
                emit_saved_object_interface_check(ctx, interface_id, &object_true);
            }
            ctx.emitter.instruction("add sp, sp, #16");                         // discard the saved object pointer after failed checks
            ctx.emitter.instruction("mov x0, #0");                              // non-Traversable objects are not iterable
            ctx.emitter.instruction(&format!("b {}", done));                    // skip the truthy result path
            ctx.emitter.label(&object_true);
            ctx.emitter.instruction("add sp, sp, #16");                         // discard the saved object pointer before returning true
            ctx.emitter.instruction(&format!("b {}", true_case));               // continue through the shared truthy result path
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rdi");
            for interface_id in interface_ids {
                emit_saved_object_interface_check(ctx, interface_id, &object_true);
            }
            abi::emit_pop_reg(ctx.emitter, "r10");
            ctx.emitter.instruction("xor eax, eax");                            // non-Traversable objects are not iterable
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip the truthy result path
            ctx.emitter.label(&object_true);
            abi::emit_pop_reg(ctx.emitter, "r10");
            ctx.emitter.instruction(&format!("jmp {}", true_case));             // continue through the shared truthy result path
        }
    }
}

/// Emits one interface matcher call for a saved object pointer.
fn emit_saved_object_interface_check(
    ctx: &mut FunctionContext<'_>,
    interface_id: u64,
    true_case: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x0, [sp]");                            // reload the object pointer as matcher argument 1
            abi::emit_load_int_immediate(ctx.emitter, "x1", interface_id as i64);
            abi::emit_load_int_immediate(ctx.emitter, "x2", 1);
            abi::emit_call_label(ctx.emitter, "__rt_exception_matches");        // check whether the object implements the Traversable interface
            ctx.emitter.instruction("cmp x0, #0");                              // test whether the runtime matcher succeeded
            ctx.emitter.instruction(&format!("b.ne {}", true_case));            // a matching interface makes the object iterable
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp]");                // reload the object pointer as matcher argument 1
            abi::emit_load_int_immediate(ctx.emitter, "rsi", interface_id as i64);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", 1);
            abi::emit_call_label(ctx.emitter, "__rt_exception_matches");        // check whether the object implements the Traversable interface
            ctx.emitter.instruction("test rax, rax");                           // test whether the runtime matcher succeeded
            ctx.emitter.instruction(&format!("jne {}", true_case));             // a matching interface makes the object iterable
        }
    }
}

/// Returns runtime interface IDs for the interfaces that make an object iterable.
fn traversable_interface_ids(ctx: &FunctionContext<'_>) -> Vec<u64> {
    ["Iterator", "IteratorAggregate"]
        .into_iter()
        .filter_map(|name| {
            ctx.module
                .interface_infos
                .get(name)
                .map(|info| info.interface_id)
        })
        .collect()
}

/// Returns whether a statically known class or interface satisfies `is_iterable()`.
fn object_type_implements_iterable(ctx: &FunctionContext<'_>, type_name: &str) -> bool {
    let normalized = normalized_type_name(type_name);
    if let Some(class_info) = ctx.module.class_infos.get(normalized) {
        return class_info.interfaces.iter().any(|interface_name| {
            is_traversable_interface_name(interface_name)
                || interface_extends_traversable(ctx, interface_name)
        });
    }
    if ctx.module.interface_infos.contains_key(normalized) {
        return is_traversable_interface_name(normalized)
            || interface_extends_traversable(ctx, normalized);
    }
    false
}

/// Returns whether an interface name is one of PHP's Traversable contracts.
fn is_traversable_interface_name(interface_name: &str) -> bool {
    let key = php_symbol_key(normalized_type_name(interface_name));
    key == php_symbol_key("Iterator") || key == php_symbol_key("IteratorAggregate")
}

/// Returns whether an interface extends Iterator or IteratorAggregate.
fn interface_extends_traversable(ctx: &FunctionContext<'_>, interface_name: &str) -> bool {
    let mut stack = vec![normalized_type_name(interface_name).to_string()];
    while let Some(current) = stack.pop() {
        if is_traversable_interface_name(&current) {
            return true;
        }
        if let Some(interface_info) = ctx.module.interface_infos.get(&current) {
            stack.extend(
                interface_info
                    .parents
                    .iter()
                    .map(|parent| normalized_type_name(parent).to_string()),
            );
        }
    }
    false
}

/// Normalizes a PHP class or interface name for metadata lookups.
fn normalized_type_name(type_name: &str) -> &str {
    type_name.trim_start_matches('\\')
}

/// Lowers `is_null()` for concrete scalar values and boxed Mixed payloads.
pub(crate) fn lower_is_null_builtin(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "is_null", 1)?;
    let value = expect_operand(inst, 0)?;
    predicates::emit_is_null_result(ctx, value)?;
    store_if_result(ctx, inst)
}

/// Lowers `is_array()`: true for statically-known arrays/hashes, or a boxed Mixed/Union value
/// whose runtime tag is an indexed (4) or associative (5) array. An `iterable`-typed value is
/// not treated as a definite array here (it may hold a Traversable); use `is_iterable` for that.
pub(crate) fn lower_is_array(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "is_array", 1)?;
    let value = expect_operand(inst, 0)?;
    match ctx.value_php_type(value)? {
        PhpType::Array(_) | PhpType::AssocArray { .. } => emit_static_bool(ctx, true),
        PhpType::Mixed | PhpType::Union(_) => {
            predicates::emit_mixed_tag_membership(ctx, value, &[4, 5])?;
        }
        _ => emit_static_bool(ctx, false),
    }
    store_if_result(ctx, inst)
}

/// Lowers `is_object()`: true for statically-known objects, or a boxed Mixed/Union value whose
/// runtime tag is an object (6).
pub(crate) fn lower_is_object(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "is_object", 1)?;
    let value = expect_operand(inst, 0)?;
    match ctx.value_php_type(value)? {
        PhpType::Object(_) => emit_static_bool(ctx, true),
        PhpType::Mixed | PhpType::Union(_) => {
            predicates::emit_mixed_tag_membership(ctx, value, &[6])?;
        }
        _ => emit_static_bool(ctx, false),
    }
    store_if_result(ctx, inst)
}

/// Lowers `is_scalar()`: true for int/float/string/bool, a non-null tagged scalar, or a boxed
/// Mixed/Union value whose runtime tag is int (0), string (1), float (2), or bool (3). Null,
/// arrays, objects, and resources are not scalars, matching PHP.
pub(crate) fn lower_is_scalar(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "is_scalar", 1)?;
    let value = expect_operand(inst, 0)?;
    match ctx.value_php_type(value)? {
        PhpType::Int | PhpType::Float | PhpType::Str | PhpType::Bool => {
            emit_static_bool(ctx, true)
        }
        PhpType::TaggedScalar => emit_tagged_scalar_int_predicate(ctx, value)?,
        PhpType::Mixed | PhpType::Union(_) => {
            predicates::emit_mixed_tag_membership(ctx, value, &[0, 1, 2, 3])?;
        }
        _ => emit_static_bool(ctx, false),
    }
    store_if_result(ctx, inst)
}

/// Returns the runtime Mixed tag used by a supported type predicate.
fn mixed_type_predicate_tag(expected: &PhpType) -> Option<u8> {
    match expected {
        PhpType::Int => Some(0),
        PhpType::Str => Some(1),
        PhpType::Float => Some(2),
        PhpType::Bool => Some(3),
        _ => None,
    }
}

/// Emits a boolean immediate into the integer result register.
fn emit_static_bool(ctx: &mut FunctionContext<'_>, value: bool) {
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        i64::from(value),
    );
}

/// Returns true when a static callable name resolves to any known callable function.
fn callable_name_exists(ctx: &FunctionContext<'_>, name: &str) -> bool {
    ctx.function_variant_group_name(name).is_some()
        || ctx.function_by_name(name).is_some()
        || ctx.has_extern_function(name)
        || is_php_visible_builtin_function(name.trim_start_matches('\\'))
}

/// Checks whether a PHP symbol is present in an iterator of known names.
fn contains_folded<'a>(
    mut names: impl Iterator<Item = &'a String>,
    needle: &str,
) -> bool {
    let needle_key = php_symbol_key(needle.trim_start_matches('\\'));
    names.any(|name| php_symbol_key(name.trim_start_matches('\\')) == needle_key)
}

/// Returns true for internal helper classes that should not be visible to PHP class_exists().
fn is_internal_synthetic_class_name(name: &str) -> bool {
    php_symbol_key(name).starts_with("__elephc")
}

/// Returns a string literal value defined by a `ConstStr` instruction.
fn const_string_operand(ctx: &FunctionContext<'_>, value: ValueId) -> Result<String> {
    let value_ref = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Err(CodegenIrError::unsupported(
            "function_exists with non-literal function name",
        ));
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    if inst_ref.op != Op::ConstStr {
        return Err(CodegenIrError::unsupported(
            "function_exists with non-literal function name",
        ));
    }
    let Some(Immediate::Data(data)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(
            "function_exists string literal has no data id",
        ));
    };
    ctx.module
        .data
        .strings
        .get(data.as_raw() as usize)
        .cloned()
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))
}

/// Verifies that the builtin call has the expected number of lowered operands.
fn ensure_arg_count(inst: &Instruction, name: &str, expected: usize) -> Result<()> {
    if inst.operands.len() == expected {
        return Ok(());
    }
    Err(CodegenIrError::invalid_module(format!(
        "{} expected {} args, got {}",
        name,
        expected,
        inst.operands.len()
    )))
}

/// Verifies that the builtin call has at least the expected number of lowered operands.
fn ensure_min_arg_count(inst: &Instruction, name: &str, expected: usize) -> Result<()> {
    if inst.operands.len() >= expected {
        return Ok(());
    }
    Err(CodegenIrError::invalid_module(format!(
        "{} expected at least {} args, got {}",
        name,
        expected,
        inst.operands.len()
    )))
}

/// Verifies that the builtin call has between the expected lowered operand counts.
fn ensure_arg_count_between(
    inst: &Instruction,
    name: &str,
    min: usize,
    max: usize,
) -> Result<()> {
    if (min..=max).contains(&inst.operands.len()) {
        return Ok(());
    }
    Err(CodegenIrError::invalid_module(format!(
        "{} expected {} to {} args, got {}",
        name,
        min,
        max,
        inst.operands.len()
    )))
}
