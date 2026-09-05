//! Purpose:
//! Materializes AOT Core function and include inventories from compiler metadata.
//!
//! Called from:
//! - `super::lower_core_builtin()` for declaration and extension introspection.
//!
//! Key details:
//! - Internal and user function lists are derived independently and de-duplicated.
//! - Included-file order comes from the compile-time resolver and autoload manifest.

use std::collections::HashSet;

use elephc_builtin_contract::{aot_support, contracts, BackendSupport, BuiltinKind};

use crate::codegen::platform::Arch;
use crate::codegen::{abi, emit_box_current_owned_value_as_mixed, emit_box_current_value_as_mixed};
use crate::ir::Instruction;
use crate::names::php_symbol_key;
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use crate::codegen::{CodegenIrError, Result};

/// Returns the canonical functions exposed by the Core extension or PHP false.
pub(super) fn lower_get_extension_funcs(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let extension = super::expect_operand(inst, 0)?;
    if let Some(name) = crate::codegen::lower_inst::builtins::maybe_const_string_operand(ctx, extension)? {
        if name.eq_ignore_ascii_case("core") {
            emit_boxed_static_string_array(ctx, elephc_builtin_contract::CORE_FUNCTION_NAMES)?;
        } else {
            emit_boxed_false(ctx);
        }
        return Ok(());
    }

    if ctx.value_php_type(extension)?.codegen_repr() != PhpType::Str {
        return Err(CodegenIrError::unsupported(
            "get_extension_funcs with non-string dynamic extension name",
        ));
    }
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(extension, ptr_reg, len_reg)?;
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);
    let matched = ctx.next_label("get_extension_funcs_core");
    let done = ctx.next_label("get_extension_funcs_done");
    crate::codegen::lower_inst::builtins::emit_branch_if_saved_string_matches_ci(
        ctx,
        b"Core",
        &matched,
    );
    emit_boxed_false(ctx);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&matched);
    emit_boxed_static_string_array(ctx, elephc_builtin_contract::CORE_FUNCTION_NAMES)?;
    ctx.emitter.label(&done);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    Ok(())
}

/// Returns every canonical PHP file compiled into this AOT module.
pub(super) fn lower_get_included_files(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let mut names = ctx.module.included_files.clone();
    if names.is_empty() {
        if let Some(source_path) = &ctx.module.source_path {
            names.push(source_path.clone());
        }
    }
    crate::codegen::lower_inst::builtins::types::emit_string_array(ctx, &names)
}

/// Builds PHP's `internal` and `user` inventories after validating the accepted flag operand.
pub(super) fn lower_get_defined_functions(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let _exclude_disabled = super::expect_operand(inst, 0)?;
    let mut internal = internal_function_names();
    let internal_keys = internal
        .iter()
        .map(|name| php_symbol_key(name.trim_start_matches('\\')))
        .collect::<HashSet<_>>();
    let groups = crate::ir::function_variants::collect_dispatch_groups(ctx.module);
    let variant_keys = groups
        .iter()
        .flat_map(|group| group.variants.iter())
        .map(|name| php_symbol_key(name.trim_start_matches('\\')))
        .collect::<HashSet<_>>();
    let mut user = ctx
        .module
        .functions
        .iter()
        .filter(|function| !function.flags.is_synthetic)
        .filter(|function| {
            let key = php_symbol_key(function.name.trim_start_matches('\\'));
            !internal_keys.contains(&key)
                && !variant_keys.contains(&key)
                && !key.starts_with("__elephc")
        })
        .map(|function| php_symbol_key(function.name.trim_start_matches('\\')))
        .collect::<Vec<_>>();
    user.extend(
        groups
            .into_iter()
            .map(|group| php_symbol_key(group.name.trim_start_matches('\\'))),
    );
    sort_php_function_names(&mut internal);
    sort_php_function_names(&mut user);
    emit_defined_functions_hash(ctx, &internal, &user)
}

/// Returns every PHP-visible function-like contract available to AOT callers.
fn internal_function_names() -> Vec<String> {
    contracts()
        .iter()
        .filter(|contract| !contract.internal && !contract.extension)
        .filter(|contract| {
            matches!(contract.kind, BuiltinKind::Function | BuiltinKind::PreludeProvided)
                || matches!(contract.name, "die" | "exit")
        })
        .filter(|contract| matches!(aot_support(contract), BackendSupport::Implemented(_)))
        .map(|contract| contract.name.to_string())
        .collect()
}

/// Sorts names by PHP's case-insensitive symbol key and removes duplicate aliases.
fn sort_php_function_names(names: &mut Vec<String>) {
    names.sort_by_key(|name| php_symbol_key(name.trim_start_matches('\\')));
    names.dedup_by(|left, right| {
        php_symbol_key(left.trim_start_matches('\\'))
            == php_symbol_key(right.trim_start_matches('\\'))
    });
}

/// Allocates and boxes an indexed array containing owned copies of static strings.
fn emit_boxed_static_string_array(
    ctx: &mut FunctionContext<'_>,
    names: &[&str],
) -> Result<()> {
    let owned = names.iter().map(|name| (*name).to_string()).collect::<Vec<_>>();
    emit_boxed_string_array(ctx, &owned)
}

/// Allocates and boxes an indexed string array.
fn emit_boxed_string_array(ctx: &mut FunctionContext<'_>, names: &[String]) -> Result<()> {
    crate::codegen::lower_inst::builtins::types::emit_string_array(ctx, names)?;
    emit_box_current_owned_value_as_mixed(
        ctx.emitter,
        &PhpType::Array(Box::new(PhpType::Str)),
    );
    Ok(())
}

/// Boxes PHP false as a Mixed result.
fn emit_boxed_false(ctx: &mut FunctionContext<'_>) {
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
}

/// Builds the two-key associative array returned by `get_defined_functions()`.
fn emit_defined_functions_hash(
    ctx: &mut FunctionContext<'_>,
    internal: &[String],
    user: &[String],
) -> Result<()> {
    allocate_string_array_hash(ctx, 2);
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    crate::codegen::lower_inst::builtins::types::emit_string_array(ctx, internal)?;
    insert_string_array_hash_value(ctx, "internal");
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    crate::codegen::lower_inst::builtins::types::emit_string_array(ctx, user)?;
    insert_string_array_hash_value(ctx, "user");
    Ok(())
}

/// Allocates a string-keyed hash whose values are indexed string arrays.
fn allocate_string_array_hash(ctx: &mut FunctionContext<'_>, capacity: usize) {
    let value_tag = crate::codegen::runtime_value_tag(&PhpType::Array(Box::new(PhpType::Str)));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", capacity.max(1) as i64);
            abi::emit_load_int_immediate(ctx.emitter, "x1", value_tag as i64);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", capacity.max(1) as i64);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", value_tag as i64);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_hash_new");
}

/// Inserts the current owned string array into the hash pointer saved on the stack.
fn insert_string_array_hash_value(ctx: &mut FunctionContext<'_>, key: &str) {
    let (key_label, key_len) = ctx.data.add_string(key.as_bytes());
    let value_tag = crate::codegen::runtime_value_tag(&PhpType::Array(Box::new(PhpType::Str)));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x3, x0");                              // pass the inventory array as the hash value
            ctx.emitter.instruction("mov x4, xzr");                             // indexed arrays do not use a high payload word
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_symbol_address(ctx.emitter, "x1", &key_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", key_len as i64);
            abi::emit_load_int_immediate(ctx.emitter, "x5", value_tag as i64);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rcx, rax");                            // pass the inventory array as the hash value
            ctx.emitter.instruction("xor r8, r8");                              // indexed arrays do not use a high payload word
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_symbol_address(ctx.emitter, "rsi", &key_label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", key_len as i64);
            abi::emit_load_int_immediate(ctx.emitter, "r9", value_tag as i64);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_hash_set");
}
