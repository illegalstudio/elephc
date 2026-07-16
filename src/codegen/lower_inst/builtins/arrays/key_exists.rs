//! Purpose:
//! Lowers PHP `array_key_exists()` calls for indexed arrays and associative hashes
//! in the Phase 04 EIR backend.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays::lower_array_key_exists()`.
//!
//! Key details:
//! - Indexed arrays use `__rt_array_key_exists` with integer-like keys, and
//!   `__rt_array_key_exists_mixed_key` (the storage-kind-dispatching presence
//!   probe, mirroring `__rt_array_get_mixed_key`'s packed/hash dispatch) for a
//!   Str/Mixed/Union/null key — an `Array(_)`-typed local can still be
//!   runtime-backed by promoted hash storage even though the checker only
//!   promotes the *static* type to `AssocArray` at a provably string-keyed write.
//! - Associative arrays probe `__rt_hash_get`; its found flag is already a PHP bool result.
//! - Boxed Mixed/Union arrays unbox at runtime and dispatch tags 4/5 to the same packed/hash
//!   probes, which preserves key presence after flow checks such as `is_array()`.
//! - `array_key_exists()` must answer `true` for a key present with a `null`
//!   value (unlike `isset()`, which answers `false`), so the mixed-key path
//!   cannot reuse `__rt_array_get_mixed_key` plus an is-null check — it needs
//!   its own presence-only helper.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::context::FunctionContext;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use super::super::super::{expect_operand, store_if_result};

/// Lowers `array_key_exists()` for indexed arrays and associative arrays.
pub(super) fn lower_array_key_exists(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "array_key_exists", 2)?;
    let key = expect_operand(inst, 0)?;
    let array = expect_operand(inst, 1)?;
    match ctx.value_php_type(array)?.codegen_repr() {
        PhpType::Array(_) => lower_indexed_array_key_exists(ctx, inst, key, array),
        PhpType::AssocArray { .. } => lower_assoc_array_key_exists(ctx, inst, key, array),
        PhpType::Mixed | PhpType::Union(_) => {
            lower_mixed_array_key_exists(ctx, inst, key, array)
        }
        other => Err(CodegenIrError::unsupported(format!(
            "array_key_exists for PHP array type {:?}",
            other
        ))),
    }
}

/// Lowers key existence for a boxed runtime array by dispatching Mixed tags 4/5.
fn lower_mixed_array_key_exists(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    key: ValueId,
    array: ValueId,
) -> Result<()> {
    let indexed = ctx.next_label("array_key_exists_mixed_indexed");
    let assoc = ctx.next_label("array_key_exists_mixed_assoc");
    let done = ctx.next_label("array_key_exists_mixed_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            super::super::super::hashes::materialize_hash_key_aarch64(ctx, key)?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            ctx.load_value_to_reg(array, "x0")?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            ctx.emitter.instruction("mov x9, x1");                              // preserve the unboxed array/hash payload pointer
            ctx.emitter.instruction("cmp x0, #4");                             // runtime tag 4 selects indexed-array storage
            ctx.emitter.instruction(&format!("b.eq {}", indexed));             // dispatch indexed arrays to their presence helper
            ctx.emitter.instruction("cmp x0, #5");                             // runtime tag 5 selects associative-array storage
            ctx.emitter.instruction(&format!("b.eq {}", assoc));               // dispatch associative arrays to hash lookup
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            abi::emit_load_int_immediate(ctx.emitter, "x0", 0);
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&indexed);
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
            ctx.emitter.instruction("mov x0, x9");                              // pass the indexed-array payload to the presence helper
            abi::emit_call_label(ctx.emitter, "__rt_array_key_exists_mixed_key");
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&assoc);
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
            ctx.emitter.instruction("mov x0, x9");                              // pass the associative-array payload to hash lookup
            abi::emit_call_label(ctx.emitter, "__rt_hash_get");
        }
        Arch::X86_64 => {
            super::super::super::hashes::materialize_hash_key_x86_64(ctx, key)?;
            abi::emit_push_reg_pair(ctx.emitter, "rsi", "rdx");
            ctx.load_value_to_reg(array, "rax")?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            ctx.emitter.instruction("mov r9, rdi");                             // preserve the unboxed array/hash payload pointer
            ctx.emitter.instruction("cmp rax, 4");                             // runtime tag 4 selects indexed-array storage
            ctx.emitter.instruction(&format!("je {}", indexed));               // dispatch indexed arrays to their presence helper
            ctx.emitter.instruction("cmp rax, 5");                             // runtime tag 5 selects associative-array storage
            ctx.emitter.instruction(&format!("je {}", assoc));                 // dispatch associative arrays to hash lookup
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            abi::emit_load_int_immediate(ctx.emitter, "rax", 0);
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&indexed);
            abi::emit_pop_reg_pair(ctx.emitter, "rsi", "rdx");
            ctx.emitter.instruction("mov rdi, r9");                            // pass the indexed-array payload to the presence helper
            abi::emit_call_label(ctx.emitter, "__rt_array_key_exists_mixed_key");
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&assoc);
            abi::emit_pop_reg_pair(ctx.emitter, "rsi", "rdx");
            ctx.emitter.instruction("mov rdi, r9");                            // pass the associative-array payload to hash lookup
            abi::emit_call_label(ctx.emitter, "__rt_hash_get");
        }
    }
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Lowers indexed-array key existence, dispatching on the key's PHP type: an
/// Int/Bool key uses the bounds-check-only fast path, while a Str/Mixed/Union/
/// null key routes through the storage-kind-dispatching mixed-key helper (the
/// key's runtime tag, and possibly the array's runtime storage kind, are only
/// known at runtime for these).
fn lower_indexed_array_key_exists(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    key: ValueId,
    array: ValueId,
) -> Result<()> {
    match ctx.value_php_type(key)?.codegen_repr() {
        PhpType::Int | PhpType::Bool => lower_indexed_array_key_exists_int(ctx, inst, key, array),
        PhpType::Str | PhpType::Mixed | PhpType::Union(_) | PhpType::Void | PhpType::Never => {
            lower_indexed_array_key_exists_mixed_key(ctx, inst, key, array)
        }
        other => Err(CodegenIrError::unsupported(format!(
            "array_key_exists key PHP type {:?}",
            other
        ))),
    }
}

/// Lowers indexed-array key existence for an Int/Bool key through the
/// bounds-check runtime helper.
fn lower_indexed_array_key_exists_int(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    key: ValueId,
    array: ValueId,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
            ctx.load_value_to_reg(key, "x1")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
            ctx.load_value_to_reg(key, "rsi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_key_exists");
    store_if_result(ctx, inst)
}

/// Lowers indexed-array key existence for a Str/Mixed/Union/null key through
/// `__rt_array_key_exists_mixed_key`, which dispatches on the array's runtime
/// storage kind (packed vs. promoted-to-hash) exactly like
/// `__rt_array_get_mixed_key`'s read path, but only reports presence.
fn lower_indexed_array_key_exists_mixed_key(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    key: ValueId,
    array: ValueId,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            super::super::super::hashes::materialize_hash_key_aarch64(ctx, key)?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            ctx.load_value_to_reg(array, "x0")?;
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
        }
        Arch::X86_64 => {
            super::super::super::hashes::materialize_hash_key_x86_64(ctx, key)?;
            abi::emit_push_reg_pair(ctx.emitter, "rsi", "rdx");
            ctx.load_value_to_reg(array, "rdi")?;
            abi::emit_pop_reg_pair(ctx.emitter, "rsi", "rdx");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_key_exists_mixed_key");
    store_if_result(ctx, inst)
}

/// Lowers associative-array key existence by probing the hash table.
fn lower_assoc_array_key_exists(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    key: ValueId,
    array: ValueId,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            materialize_hash_key_aarch64(ctx, key)?;
            ctx.load_value_to_reg(array, "x0")?;
            abi::emit_call_label(ctx.emitter, "__rt_hash_get");
        }
        Arch::X86_64 => {
            materialize_hash_key_x86_64(ctx, key)?;
            ctx.load_value_to_reg(array, "rdi")?;
            abi::emit_call_label(ctx.emitter, "__rt_hash_get");
        }
    }
    store_if_result(ctx, inst)
}

/// Materializes an EIR value as a normalized AArch64 associative-array key.
fn materialize_hash_key_aarch64(ctx: &mut FunctionContext<'_>, key: ValueId) -> Result<()> {
    match ctx.value_php_type(key)?.codegen_repr() {
        PhpType::Str => {
            ctx.load_string_value_to_regs(key, "x1", "x2")?;
            abi::emit_call_label(ctx.emitter, "__rt_hash_normalize_key");
            Ok(())
        }
        PhpType::Int | PhpType::Bool | PhpType::Callable => {
            ctx.load_value_to_reg(key, "x1")?;
            abi::emit_load_int_immediate(ctx.emitter, "x2", -1);
            Ok(())
        }
        // PHP null normalizes to the empty string "" as an array key.
        PhpType::Void | PhpType::Never => {
            let (label, len) = ctx.data.add_string(b"");
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
            Ok(())
        }
        PhpType::Mixed | PhpType::Union(_) => materialize_mixed_hash_key_aarch64(ctx, key),
        other => Err(CodegenIrError::unsupported(format!(
            "array_key_exists key PHP type {:?}",
            other
        ))),
    }
}

/// Materializes an EIR value as a normalized x86_64 associative-array key.
fn materialize_hash_key_x86_64(ctx: &mut FunctionContext<'_>, key: ValueId) -> Result<()> {
    match ctx.value_php_type(key)?.codegen_repr() {
        PhpType::Str => {
            ctx.load_string_value_to_regs(key, "rax", "rdx")?;
            abi::emit_call_label(ctx.emitter, "__rt_hash_normalize_key");
            ctx.emitter.instruction("mov rsi, rax");                            // move the normalized string-or-integer key low word into the hash lookup ABI register
            Ok(())
        }
        PhpType::Int | PhpType::Bool | PhpType::Callable => {
            ctx.load_value_to_reg(key, "rsi")?;
            abi::emit_load_int_immediate(ctx.emitter, "rdx", -1);
            Ok(())
        }
        // PHP null normalizes to the empty string "" as an array key.
        PhpType::Void | PhpType::Never => {
            let (label, len) = ctx.data.add_string(b"");
            abi::emit_symbol_address(ctx.emitter, "rax", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
            ctx.emitter.instruction("mov rsi, rax");                            // move the empty-string pointer into the hash ABI key low word
            Ok(())
        }
        PhpType::Mixed | PhpType::Union(_) => materialize_mixed_hash_key_x86_64(ctx, key),
        other => Err(CodegenIrError::unsupported(format!(
            "array_key_exists key PHP type {:?}",
            other
        ))),
    }
}

/// Materializes a boxed Mixed key as the AArch64 hash-lookup key pair.
fn materialize_mixed_hash_key_aarch64(
    ctx: &mut FunctionContext<'_>,
    key: ValueId,
) -> Result<()> {
    let string_key = ctx.next_label("mixed_hash_key_string");
    let null_key = ctx.next_label("mixed_hash_key_null");
    let scalar_key = ctx.next_label("mixed_hash_key_scalar");
    let done = ctx.next_label("mixed_hash_key_done");
    ctx.load_value_to_reg(key, "x0")?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    ctx.emitter.instruction("cmp x0, #1");                                      // string mixed keys need PHP numeric-string normalization
    ctx.emitter.instruction(&format!("b.eq {}", string_key));                   // route string keys through the normal hash-key helper
    ctx.emitter.instruction("cmp x0, #8");                                      // null mixed keys normalize to the empty string like PHP
    ctx.emitter.instruction(&format!("b.eq {}", null_key));                    // route null keys to the empty-string key path
    ctx.emitter.instruction("cmp x0, #0");                                      // integer mixed keys are already scalar hash keys
    ctx.emitter.instruction(&format!("b.eq {}", scalar_key));                  // keep integer keys as integer hash keys
    ctx.emitter.instruction("cmp x0, #3");                                      // boolean mixed keys normalize like integer keys
    ctx.emitter.instruction(&format!("b.eq {}", scalar_key));                   // keep boolean keys as integer keys
    ctx.emitter.instruction("mov x1, #0");                                      // unsupported mixed key tags fall back to integer key zero
    ctx.emitter.label(&scalar_key);
    ctx.emitter.instruction("mov x2, #-1");                                     // key_hi sentinel marks scalar mixed keys as integers
    ctx.emitter.instruction(&format!("b {}", done));                            // skip string-key normalization after scalar selection
    ctx.emitter.label(&null_key);
    let (empty_label, empty_len) = ctx.data.add_string(b"");
    abi::emit_symbol_address(ctx.emitter, "x1", &empty_label);                   // null normalizes to the empty string "" hash key pointer
    abi::emit_load_int_immediate(ctx.emitter, "x2", empty_len as i64);          // the empty-string key has zero length (string-key marker)
    ctx.emitter.instruction(&format!("b {}", done));                            // skip the string-key normalization path
    ctx.emitter.label(&string_key);
    abi::emit_call_label(ctx.emitter, "__rt_hash_normalize_key");
    ctx.emitter.label(&done);
    Ok(())
}

/// Materializes a boxed Mixed key as the x86_64 hash-lookup key pair.
fn materialize_mixed_hash_key_x86_64(
    ctx: &mut FunctionContext<'_>,
    key: ValueId,
) -> Result<()> {
    let string_key = ctx.next_label("mixed_hash_key_string");
    let null_key = ctx.next_label("mixed_hash_key_null");
    let scalar_key = ctx.next_label("mixed_hash_key_scalar");
    let done = ctx.next_label("mixed_hash_key_done");
    ctx.load_value_to_reg(key, "rax")?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    ctx.emitter.instruction("cmp rax, 1");                                      // string mixed keys need PHP numeric-string normalization
    ctx.emitter.instruction(&format!("je {}", string_key));                     // route string keys through the normal hash-key helper
    ctx.emitter.instruction("cmp rax, 8");                                      // null mixed keys normalize to the empty string like PHP
    ctx.emitter.instruction(&format!("je {}", null_key));                      // route null keys to the empty-string key path
    ctx.emitter.instruction("cmp rax, 0");                                      // integer mixed keys are already scalar hash keys
    ctx.emitter.instruction(&format!("je {}", scalar_key));                     // keep integer keys as integer hash keys
    ctx.emitter.instruction("cmp rax, 3");                                      // boolean mixed keys normalize like integer keys
    ctx.emitter.instruction(&format!("je {}", scalar_key));                    // keep boolean keys as integer hash keys
    ctx.emitter.instruction("xor esi, esi");                                    // unsupported mixed key tags fall back to integer key zero
    ctx.emitter.instruction("mov rdx, -1");                                     // key_hi sentinel marks fallback mixed keys as integers
    ctx.emitter.instruction(&format!("jmp {}", done));                          // skip string-key normalization after fallback selection
    ctx.emitter.label(&null_key);
    let (empty_label, empty_len) = ctx.data.add_string(b"");
    abi::emit_symbol_address(ctx.emitter, "rax", &empty_label);                 // null normalizes to the empty string "" hash key pointer
    abi::emit_load_int_immediate(ctx.emitter, "rdx", empty_len as i64);          // the empty-string key has zero length (string-key marker)
    ctx.emitter.instruction("mov rsi, rax");                                    // move the empty-string pointer into the hash ABI key low word
    ctx.emitter.instruction(&format!("jmp {}", done));                         // skip the string-key normalization path
    ctx.emitter.label(&scalar_key);
    ctx.emitter.instruction("mov rsi, rdi");                                    // publish the unboxed scalar payload as key_lo
    ctx.emitter.instruction("mov rdx, -1");                                     // key_hi sentinel marks scalar mixed keys as integers
    ctx.emitter.instruction(&format!("jmp {}", done));                          // skip string-key normalization after scalar selection
    ctx.emitter.label(&string_key);
    ctx.emitter.instruction("mov rax, rdi");                                    // move the unboxed string pointer into the hash normalizer input
    abi::emit_call_label(ctx.emitter, "__rt_hash_normalize_key");
    ctx.emitter.instruction("mov rsi, rax");                                    // move normalized key_lo into the hash lookup ABI register
    ctx.emitter.label(&done);
    Ok(())
}
