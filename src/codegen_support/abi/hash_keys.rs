//! Purpose:
//! Shares boxed key materialization between EIR codegen and runtime receiver dispatch.
//!
//! Called from:
//! - Hash key lowering and the generic boxed offset reader.
//!
//! Key details:
//! - Conversion preserves existing diagnostics; exact-key guards inspect original kinds first.
//! - AArch64 uses x0 input and x1/x2 output; x86_64 uses rax input and rsi/rdx output.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Internal generic-reader marker carrying a boxed key before receiver dispatch.
pub(crate) const BOXED_ARRAY_KEY: i64 = -2;

/// Whether a hash key goes through PHP's numeric-string normalization.
///
/// `Php` is every ordinary array and hash write. `RawString` preserves descriptor
/// parameter names and original string offsets until receiver dispatch.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum HashKeyNormalization {
    /// `__rt_hash_normalize_key` decides: a numeric string becomes an integer key.
    Php,
    /// Normalize again without repeating a float diagnostic already emitted by this access.
    PhpAlreadyDiagnosed,
    /// A string key stays a string key, byte for byte.
    RawString,
}

/// Emits one boxed-key conversion using unique labels and a persistent empty-string address.
pub(crate) fn emit_boxed_hash_key(emitter: &mut Emitter, normalization: HashKeyNormalization,
    stem: &str, empty_string: &str) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter, normalization, stem, empty_string),
        Arch::X86_64 => emit_x86_64(emitter, normalization, stem, empty_string),
    }
}

/// Emits the existing aarch64 boxed-key conversion in the caller's frame.
fn emit_aarch64(emitter: &mut Emitter, normalization: HashKeyNormalization, stem: &str, empty_string: &str) {
    let string_key = format!("{stem}_string");
    let null_key = format!("{stem}_null");
    let float_key = format!("{stem}_float");
    let scalar_key = format!("{stem}_scalar");
    let done = format!("{stem}_done");
    super::emit_call_label(emitter, "__rt_mixed_unbox");
    emitter.instruction("cmp x0, #1");                                          // string mixed keys need PHP numeric-string normalization
    emitter.instruction(&format!("b.eq {}", string_key));                       // route string keys through the normal hash-key helper
    emitter.instruction("cmp x0, #8");                                          // null mixed keys normalize to the empty string like PHP
    emitter.instruction(&format!("b.eq {}", null_key));                         // route null keys to the empty-string key path
    emitter.instruction("cmp x0, #0");                                          // integer mixed keys are already scalar hash keys
    emitter.instruction(&format!("b.eq {}", scalar_key));                       // keep integer keys as integer hash keys
    emitter.instruction("cmp x0, #3");                                          // boolean mixed keys normalize like integer keys
    emitter.instruction(&format!("b.eq {}", scalar_key));                       // keep boolean keys as integer hash keys
    emitter.instruction("cmp x0, #2");                                          // float mixed keys truncate toward zero, exactly like PHP
    emitter.instruction(&format!("b.eq {}", float_key));                        // route float keys through the truncating conversion
    emitter.instruction("mov x1, #0");                                          // unsupported mixed key tags fall back to integer key zero
    emitter.instruction(&format!("b {}", scalar_key));                          // the float arm sits between here and the scalar path
    emitter.label(&float_key);
    emitter.instruction("fmov d0, x1");                                         // move the raw IEEE-754 payload bits into an FP register
    if normalization == HashKeyNormalization::PhpAlreadyDiagnosed {
        super::emit_call_label(emitter, "__rt_php_float_to_int");
        emitter.instruction("mov x0, x9");                                      // recover the normalized PHP integer key without another warning
    } else {
        super::emit_call_label(emitter, "__rt_float_key_to_int");
    }
    emitter.instruction("mov x1, x0");                                          // use the diagnosed PHP integer key
    emitter.label(&scalar_key);
    emitter.instruction("mov x2, #-1");                                         // key_hi sentinel marks scalar mixed keys as integers
    emitter.instruction(&format!("b {}", done));                                // skip string-key normalization after scalar selection
    emitter.label(&null_key);
    super::emit_symbol_address(emitter, "x1", empty_string);
    super::emit_load_int_immediate(emitter, "x2", 0);                           // an empty string keeps its string-key high word
    emitter.instruction(&format!("b {}", done));                                // skip the string-key normalization path
    emitter.label(&string_key);
    if normalization != HashKeyNormalization::RawString {
        super::emit_call_label(emitter, "__rt_hash_normalize_key");
    }
    emitter.label(&done);
}

/// Emits the existing x86_64 boxed-key conversion in the caller's frame.
fn emit_x86_64(emitter: &mut Emitter, normalization: HashKeyNormalization, stem: &str, empty_string: &str) {
    let string_key = format!("{stem}_string");
    let null_key = format!("{stem}_null");
    let float_key = format!("{stem}_float");
    let scalar_key = format!("{stem}_scalar");
    let done = format!("{stem}_done");
    super::emit_call_label(emitter, "__rt_mixed_unbox");
    emitter.instruction("cmp rax, 1");                                          // string mixed keys need PHP numeric-string normalization
    emitter.instruction(&format!("je {}", string_key));                         // route string keys through the normal hash-key helper
    emitter.instruction("cmp rax, 8");                                          // null mixed keys normalize to the empty string like PHP
    emitter.instruction(&format!("je {}", null_key));                           // route null keys to the empty-string key path
    emitter.instruction("cmp rax, 0");                                          // integer mixed keys are already scalar hash keys
    emitter.instruction(&format!("je {}", scalar_key));                         // keep integer keys as integer hash keys
    emitter.instruction("cmp rax, 3");                                          // boolean mixed keys normalize like integer keys
    emitter.instruction(&format!("je {}", scalar_key));                         // keep boolean keys as integer hash keys
    emitter.instruction("cmp rax, 2");                                          // float mixed keys truncate toward zero, exactly like PHP
    emitter.instruction(&format!("je {}", float_key));                          // route float keys through the truncating conversion
    emitter.instruction("xor esi, esi");                                        // unsupported mixed key tags fall back to integer key zero
    emitter.instruction("mov rdx, -1");                                         // key_hi sentinel marks fallback mixed keys as integers
    emitter.instruction(&format!("jmp {}", done));                              // skip string-key normalization after fallback selection
    emitter.label(&float_key);
    emitter.instruction("movq xmm0, rdi");                                      // move the raw IEEE-754 payload bits into an FP register
    if normalization == HashKeyNormalization::PhpAlreadyDiagnosed {
        super::emit_call_label(emitter, "__rt_php_float_to_int");
        emitter.instruction("mov rax, r11");                                    // recover the normalized PHP integer key without another warning
    } else {
        super::emit_call_label(emitter, "__rt_float_key_to_int");
    }
    emitter.instruction("mov rsi, rax");                                        // use the diagnosed PHP integer key
    emitter.instruction("mov rdx, -1");                                         // key_hi sentinel marks the truncated float as an integer key
    emitter.instruction(&format!("jmp {}", done));                              // skip string-key normalization after the float conversion
    emitter.label(&null_key);
    super::emit_symbol_address(emitter, "rax", empty_string);
    super::emit_load_int_immediate(emitter, "rdx", 0);                          // an empty string keeps its string-key high word
    emitter.instruction("mov rsi, rax");                                        // publish the empty string pointer as the low word
    emitter.instruction(&format!("jmp {}", done));                              // skip the string-key normalization path
    emitter.label(&scalar_key);
    emitter.instruction("mov rsi, rdi");                                        // publish the unboxed scalar payload as key_lo
    emitter.instruction("mov rdx, -1");                                         // key_hi sentinel marks scalar mixed keys as integers
    emitter.instruction(&format!("jmp {}", done));                              // skip string-key normalization after scalar selection
    emitter.label(&string_key);
    emitter.instruction("mov rax, rdi");                                        // move the unboxed string pointer into the hash key low word
    if normalization != HashKeyNormalization::RawString {
        super::emit_call_label(emitter, "__rt_hash_normalize_key");
    }
    emitter.instruction("mov rsi, rax");                                        // move key_lo into the hash-set ABI register
    emitter.label(&done);
}
