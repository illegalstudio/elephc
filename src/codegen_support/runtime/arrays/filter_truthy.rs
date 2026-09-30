//! Purpose:
//! Emits the implicit predicates `array_filter($array)` uses when its `$callback` is omitted
//! or null: `__rt_filter_truthy_int`, `__rt_filter_truthy_float`, `__rt_filter_truthy_str`
//! and `__rt_filter_truthy_mixed`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - php's `array_filter(array $array, ?callable $callback = null, int $mode = 0)` keeps the
//!   elements that are truthy when no callback is given. These predicates carry the SAME ABI
//!   as a user callback wrapper, so the existing `__rt_array_filter` /
//!   `__rt_array_filter_refcounted` loops drive them unchanged — no second filter loop exists
//!   to drift from the first.
//! - Callback ABI: ARM64 `x0` = the element (string elements arrive as `x0` = pointer,
//!   `x1` = length), truthy answer in `x0`. x86_64 `rdi` (and `rsi` for a string's length),
//!   answer in `rax`.
//! - php's own truthiness rules, measured: `0` and `0.0` and `-0.0` are falsy but `NAN` is
//!   truthy (so the float test drops the sign bit and compares the remaining bits, rather
//!   than doing a floating-point compare, which NaN would fail); `""` and `"0"` are the only
//!   falsy strings, so `"00"` and `"0.0"` are truthy.
//! - The Mixed predicate tail-calls `__rt_mixed_cast_bool`, which already implements the
//!   per-tag rule. Its x86_64 input register is `rax`, NOT the SysV first argument register,
//!   so the shim moves `rdi` into `rax` first — this runtime does not share one x86
//!   convention across helpers.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits every implicit `array_filter()` truthiness predicate for the active target.
pub fn emit_filter_truthy_predicates(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_filter_truthy_predicates_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: filter_truthy_int ---");
    emitter.label_global("__rt_filter_truthy_int");
    emitter.instruction("cmp x0, #0");                                          // php: 0 is the only falsy int, and false is stored as 0
    emitter.instruction("cset x0, ne");
    emitter.instruction("ret");

    emitter.blank();
    emitter.comment("--- runtime: filter_truthy_float ---");
    emitter.label_global("__rt_filter_truthy_float");
    emitter.instruction("lsl x0, x0, #1");                                      // drop the sign bit so -0.0 tests equal to 0.0
    emitter.instruction("cmp x0, #0");                                          // every other bit pattern is truthy, NAN included
    emitter.instruction("cset x0, ne");
    emitter.instruction("ret");

    emitter.blank();
    emitter.comment("--- runtime: filter_truthy_str ---");
    emitter.label_global("__rt_filter_truthy_str");
    emitter.instruction("cbz x1, __rt_filter_truthy_str_false");                // "" is falsy
    emitter.instruction("cmp x1, #1");                                          // only a one-byte string can be the falsy "0"
    emitter.instruction("b.ne __rt_filter_truthy_str_true");                    // "00" and "0.0" are truthy in php
    emitter.instruction("ldrb w9, [x0]");
    emitter.instruction("cmp w9, #48");                                         // '0'
    emitter.instruction("b.eq __rt_filter_truthy_str_false");
    emitter.label("__rt_filter_truthy_str_true");
    emitter.instruction("mov x0, #1");
    emitter.instruction("ret");
    emitter.label("__rt_filter_truthy_str_false");
    emitter.instruction("mov x0, #0");
    emitter.instruction("ret");

    emitter.blank();
    emitter.comment("--- runtime: filter_truthy_mixed ---");
    emitter.label_global("__rt_filter_truthy_mixed");
    emitter.instruction("b __rt_mixed_cast_bool");                              // the boxed cell already arrives in x0
}

/// Emits the x86_64 form of [`emit_filter_truthy_predicates`].
fn emit_filter_truthy_predicates_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: filter_truthy_int ---");
    emitter.label_global("__rt_filter_truthy_int");
    emitter.instruction("xor eax, eax");
    emitter.instruction("test rdi, rdi");                                       // php: 0 is the only falsy int, and false is stored as 0
    emitter.instruction("setne al");
    emitter.instruction("ret");

    emitter.blank();
    emitter.comment("--- runtime: filter_truthy_float ---");
    emitter.label_global("__rt_filter_truthy_float");
    emitter.instruction("mov r10, rdi");
    emitter.instruction("add r10, r10");                                        // drop the sign bit so -0.0 tests equal to 0.0
    emitter.instruction("xor eax, eax");
    emitter.instruction("test r10, r10");                                       // every other bit pattern is truthy, NAN included
    emitter.instruction("setne al");
    emitter.instruction("ret");

    emitter.blank();
    emitter.comment("--- runtime: filter_truthy_str ---");
    emitter.label_global("__rt_filter_truthy_str");
    emitter.instruction("test rsi, rsi");                                       // "" is falsy
    emitter.instruction("jz __rt_filter_truthy_str_false_x");
    emitter.instruction("cmp rsi, 1");                                          // only a one-byte string can be the falsy "0"
    emitter.instruction("jne __rt_filter_truthy_str_true_x");                   // "00" and "0.0" are truthy in php
    emitter.instruction("movzx r10d, BYTE PTR [rdi]");
    emitter.instruction("cmp r10d, 48");                                        // '0'
    emitter.instruction("je __rt_filter_truthy_str_false_x");
    emitter.label("__rt_filter_truthy_str_true_x");
    emitter.instruction("mov rax, 1");
    emitter.instruction("ret");
    emitter.label("__rt_filter_truthy_str_false_x");
    emitter.instruction("xor eax, eax");
    emitter.instruction("ret");

    emitter.blank();
    emitter.comment("--- runtime: filter_truthy_mixed ---");
    emitter.label_global("__rt_filter_truthy_mixed");
    emitter.instruction("mov rax, rdi");                                        // __rt_mixed_cast_bool reads its cell from rax, not rdi
    emitter.instruction("jmp __rt_mixed_cast_bool");
}
