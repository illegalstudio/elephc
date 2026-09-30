//! Purpose:
//! Emits the `__rt_get_usec_context_option` runtime helper — the duration-typed
//! sibling of `__rt_get_string_context_option` / `__rt_get_int_context_option`.
//! Looks up `_stream_context_options[wrapper][option]` and resolves it to a
//! whole number of MICROSECONDS, whatever PHP shape the value was written in.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::io`.
//! - `__rt_http_build_request`, for `_stream_context_options["http"]["timeout"]`.
//!
//! Key details:
//! - php-src reads `http.timeout` with `php_stream_context_get_option` plus a
//!   double cast, so `2`, `2.5`, `"2.5"` and `true` are all legal spellings of
//!   the same duration. Reading it only as a string (the pre-existing elephc
//!   behavior) missed every int and float spelling, and the base-10 parser
//!   truncated `"0.5"` to `0`.
//! - int and bool payloads scale by 1_000_000; float payloads (tag 2, f64 bits)
//!   are multiplied then truncated toward zero, exactly like php-src's
//!   `tv_sec = (long) d; tv_usec = (d - tv_sec) * 1000000`; string payloads
//!   (tag 1) go through the leading-numeric-prefix parser below, mirroring
//!   PHP's string-to-double cast (a non-numeric string resolves to 0).
//! - Boxed Mixed cells (tag 7) are unboxed once and then re-dispatched. The two
//!   sources disagree on where the payload high word lands on x86_64
//!   (`__rt_hash_get` returns it in `rsi`, `__rt_mixed_unbox` in `rdx`), so the
//!   x86_64 path normalizes both into `r11` before parsing.
//! - A negative result is preserved and left for the caller to interpret; php
//!   treats a negative timeout as "wait forever".
//! - On hit, writes `out_usec_addr` and returns 1. On miss the output is left
//!   untouched and 0 is returned, so callers can pre-load a default.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// `__rt_get_usec_context_option`:
/// Input:  AArch64 x0 = wrapper_ptr, x1 = wrapper_len,
///                 x2 = opt_ptr, x3 = opt_len,
///                 x4 = out_usec_addr.
///         x86_64  rdi = wrapper_ptr, rsi = wrapper_len,
///                 rdx = opt_ptr,     rcx = opt_len,
///                 r8 = out_usec_addr.
/// Output: x0/rax = 1 on hit (out_usec written), 0 on miss.
pub fn emit_get_usec_context_option(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_get_usec_context_option_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: get_usec_context_option ---");
    emitter.label_global("__rt_get_usec_context_option");

    // Frame (64 bytes):
    //   [sp,  0] wrapper_ptr
    //   [sp,  8] wrapper_len
    //   [sp, 16] opt_ptr
    //   [sp, 24] opt_len
    //   [sp, 32] out_usec_addr
    //   [sp, 40] padding
    //   [sp, 48] saved x29
    //   [sp, 56] saved x30
    emitter.instruction("sub sp, sp, #64");                                     // allocate the helper frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save wrapper_ptr
    emitter.instruction("str x1, [sp, #8]");                                    // save wrapper_len
    emitter.instruction("str x2, [sp, #16]");                                   // save opt_ptr
    emitter.instruction("str x3, [sp, #24]");                                   // save opt_len
    emitter.instruction("str x4, [sp, #32]");                                   // save out_usec_addr

    // -- resolve the options hash the same way the int/string readers do:
    //    an explicit scope wins, an explicit empty scope masks the default,
    //    and only a total absence of scope falls back to the request default. --
    abi::emit_symbol_address(emitter, "x9", "_stream_context_options");
    emitter.instruction("ldr x0, [x9]");                                        // top hash pointer (may be null)
    emitter.instruction("cbnz x0, __rt_guco_have_options");                     // an explicit scope wins
    abi::emit_symbol_address(emitter, "x9", "_stream_current_context_handle");
    emitter.instruction("ldr x9, [x9]");                                        // handle of the active context scope
    emitter.instruction("cbnz x9, __rt_guco_have_options");                     // a scope is active: its emptiness is meaningful
    abi::emit_symbol_address(emitter, "x9", "_stream_default_context_handle");
    emitter.instruction("ldr x0, [x9]");                                        // request-default context handle
    emitter.instruction("cbz x0, __rt_guco_have_options");                      // no default context exists
    emitter.instruction("bl __rt_context_state");                               // resolve its ContextState
    emitter.instruction("cbz x0, __rt_guco_have_options");                      // a closed default context has no options
    emitter.instruction("ldr x0, [x0, #0]");                                    // CONTEXT_OPTIONS_OFFSET
    emitter.label("__rt_guco_have_options");
    emitter.instruction("cbz x0, __rt_guco_miss");                              // no options table at all

    // -- hash_get(top, wrapper) → x1 = sub-hash on hit --
    emitter.instruction("ldr x1, [sp, #0]");                                    // wrapper_ptr
    emitter.instruction("ldr x2, [sp, #8]");                                    // wrapper_len
    emitter.instruction("bl __rt_hash_get");                                    // x0=found, x1=lo, x2=hi, x3=tag
    emitter.instruction("cbz x0, __rt_guco_miss");                              // the wrapper section is absent

    // -- hash_get(sub, option) → x1 = value_lo, x2 = value_hi, x3 = tag --
    emitter.instruction("mov x0, x1");                                          // sub-hash → first arg
    emitter.instruction("ldr x1, [sp, #16]");                                   // opt_ptr
    emitter.instruction("ldr x2, [sp, #24]");                                   // opt_len
    emitter.instruction("bl __rt_hash_get");                                    // x0=found, x1=lo, x2=hi, x3=tag
    emitter.instruction("cbz x0, __rt_guco_miss");                              // the option is absent

    // -- a boxed Mixed cell is unwrapped once, then dispatched like a direct value --
    emitter.instruction("cmp x3, #7");                                          // is the option stored as a boxed Mixed cell?
    emitter.instruction("b.ne __rt_guco_dispatch");                             // direct values dispatch as-is
    emitter.instruction("mov x0, x1");                                          // pass the boxed option cell to Mixed unboxing
    emitter.instruction("bl __rt_mixed_unbox");                                 // x0=tag, x1=lo, x2=hi
    emitter.instruction("mov x3, x0");                                          // the inner tag drives the dispatch below
    emitter.label("__rt_guco_dispatch");

    emitter.instruction("cmp x3, #0");                                          // tag 0 = int
    emitter.instruction("b.eq __rt_guco_from_int");                             // whole seconds
    emitter.instruction("cmp x3, #3");                                          // tag 3 = bool
    emitter.instruction("b.eq __rt_guco_from_int");                             // true is one second, like PHP's cast
    emitter.instruction("cmp x3, #2");                                          // tag 2 = float (f64 bits in the payload)
    emitter.instruction("b.eq __rt_guco_from_float");                           // fractional seconds
    emitter.instruction("cmp x3, #1");                                          // tag 1 = string (ptr in lo, len in hi)
    emitter.instruction("b.eq __rt_guco_from_string");                          // decimal text
    emitter.instruction("b __rt_guco_miss");                                    // arrays and objects are not durations

    // -- int/bool: microseconds = value * 1_000_000 --
    emitter.label("__rt_guco_from_int");
    emitter.instruction("mov x9, #1000");                                       // build 1e6 without a wide immediate
    emitter.instruction("mul x9, x9, x9");                                      // x9 = 1_000_000
    emitter.instruction("mul x1, x1, x9");                                      // seconds → microseconds
    emitter.instruction("b __rt_guco_write");                                   // publish the resolved duration

    // -- float: microseconds = trunc(value * 1_000_000) --
    emitter.label("__rt_guco_from_float");
    emitter.instruction("fmov d0, x1");                                         // reinterpret the payload bits as a double
    emitter.instruction("mov x9, #1000");                                       // build 1e6 without a wide immediate
    emitter.instruction("mul x9, x9, x9");                                      // x9 = 1_000_000
    emitter.instruction("scvtf d1, x9");                                        // 1e6 as a double
    emitter.instruction("fmul d0, d0, d1");                                     // seconds → microseconds
    emitter.instruction("fcvtzs x1, d0");                                       // truncate toward zero, like php-src's cast
    emitter.instruction("b __rt_guco_write");                                   // publish the resolved duration

    // -- string: parse the leading `[+-]?digits[.digits]` prefix --
    emitter.label("__rt_guco_from_string");
    emitter.instruction("mov x10, x1");                                         // x10 = byte pointer
    emitter.instruction("mov x11, x2");                                         // x11 = byte length
    emitter.instruction("mov x12, #0");                                         // x12 = scan index
    emitter.instruction("mov x13, #0");                                         // x13 = whole-seconds accumulator
    emitter.instruction("mov x14, #0");                                         // x14 = negative flag
    emitter.instruction("mov x17, #0");                                         // x17 = fractional digits accumulator
    emitter.instruction("mov x9, #0");                                          // x9 = fractional digit count
    emitter.instruction("cbz x11, __rt_guco_str_scale");                        // an empty string resolves to zero
    emitter.instruction("ldrb w15, [x10]");                                     // first byte
    emitter.instruction("cmp w15, #45");                                        // '-'
    emitter.instruction("b.ne __rt_guco_str_plus");                             // not a minus sign
    emitter.instruction("mov x14, #1");                                         // remember the sign
    emitter.instruction("add x12, x12, #1");                                    // consume the sign byte
    emitter.instruction("b __rt_guco_str_int_loop");                            // start on the digits
    emitter.label("__rt_guco_str_plus");
    emitter.instruction("cmp w15, #43");                                        // '+'
    emitter.instruction("b.ne __rt_guco_str_int_loop");                         // no explicit sign
    emitter.instruction("add x12, x12, #1");                                    // consume the sign byte
    emitter.label("__rt_guco_str_int_loop");
    emitter.instruction("cmp x12, x11");                                        // ran off the end?
    emitter.instruction("b.ge __rt_guco_str_scale");                            // no fractional part follows
    emitter.instruction("ldrb w15, [x10, x12]");                                // current byte
    emitter.instruction("sub w15, w15, #48");                                   // '0'..'9' → 0..9
    emitter.instruction("cmp w15, #9");                                         // still a digit?
    emitter.instruction("b.hi __rt_guco_str_int_end");                          // the integer part stops here
    emitter.instruction("mov x16, #10");                                        // decimal base
    emitter.instruction("mul x13, x13, x16");                                   // shift the accumulator one place
    emitter.instruction("add x13, x13, x15");                                   // the w-form sub already zeroed the high half
    emitter.instruction("add x12, x12, #1");                                    // advance the scan index
    emitter.instruction("b __rt_guco_str_int_loop");                            // keep consuming digits
    emitter.label("__rt_guco_str_int_end");
    emitter.instruction("ldrb w15, [x10, x12]");                                // reload the terminator byte
    emitter.instruction("cmp w15, #46");                                        // '.' starts a fractional part
    emitter.instruction("b.ne __rt_guco_str_scale");                            // no fraction: the integer part is final
    emitter.instruction("add x12, x12, #1");                                    // consume the decimal point
    emitter.label("__rt_guco_str_frac_loop");
    emitter.instruction("cmp x9, #6");                                          // microsecond resolution is six digits
    emitter.instruction("b.ge __rt_guco_str_scale");                            // extra digits are below the resolution
    emitter.instruction("cmp x12, x11");                                        // ran off the end?
    emitter.instruction("b.ge __rt_guco_str_scale");                            // the fraction stops here
    emitter.instruction("ldrb w15, [x10, x12]");                                // current byte
    emitter.instruction("sub w15, w15, #48");                                   // '0'..'9' → 0..9
    emitter.instruction("cmp w15, #9");                                         // still a digit?
    emitter.instruction("b.hi __rt_guco_str_scale");                            // trailing junk ends the number
    emitter.instruction("mov x16, #10");                                        // decimal base
    emitter.instruction("mul x17, x17, x16");                                   // shift the fraction one place
    emitter.instruction("add x17, x17, x15");                                   // accumulate the fractional digit
    emitter.instruction("add x12, x12, #1");                                    // advance the scan index
    emitter.instruction("add x9, x9, #1");                                      // count the consumed fractional digit
    emitter.instruction("b __rt_guco_str_frac_loop");                           // keep consuming digits
    emitter.label("__rt_guco_str_scale");
    // -- pad the fraction out to six digits so "0.5" means 500000 microseconds --
    emitter.instruction("cmp x9, #6");                                          // already at microsecond resolution?
    emitter.instruction("b.ge __rt_guco_str_combine");                          // nothing left to scale
    emitter.instruction("mov x16, #10");                                        // decimal base
    emitter.instruction("mul x17, x17, x16");                                   // shift the fraction one place left
    emitter.instruction("add x9, x9, #1");                                      // one more digit of padding applied
    emitter.instruction("b __rt_guco_str_scale");                               // keep padding to six digits
    emitter.label("__rt_guco_str_combine");
    emitter.instruction("mov x16, #1000");                                      // build 1e6 without a wide immediate
    emitter.instruction("mul x16, x16, x16");                                   // x16 = 1_000_000
    emitter.instruction("madd x1, x13, x16, x17");                              // usec = whole * 1e6 + fraction
    emitter.instruction("cbz x14, __rt_guco_write");                            // a positive duration is already final
    emitter.instruction("neg x1, x1");                                          // apply the leading minus sign

    emitter.label("__rt_guco_write");
    emitter.instruction("ldr x9, [sp, #32]");                                   // out_usec_addr
    emitter.instruction("str x1, [x9]");                                        // *out_usec = resolved microseconds
    emitter.instruction("mov x0, #1");                                          // report a hit
    emitter.instruction("b __rt_guco_done");                                    // return to the caller

    emitter.label("__rt_guco_miss");
    emitter.instruction("mov x0, #0");                                          // report a miss
    emitter.label("__rt_guco_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 variant of `__rt_get_usec_context_option`.
fn emit_get_usec_context_option_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: get_usec_context_option ---");
    emitter.label_global("__rt_get_usec_context_option");

    // rbp-relative frame:
    //   [rbp -  8] wrapper_ptr
    //   [rbp - 16] wrapper_len
    //   [rbp - 24] opt_ptr
    //   [rbp - 32] opt_len
    //   [rbp - 40] out_usec_addr
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish runtime frame pointer
    emitter.instruction("sub rsp, 48");                                         // allocate runtime stack frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save wrapper_ptr
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save wrapper_len
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save opt_ptr
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save opt_len
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // save out_usec_addr

    // -- resolve the options hash exactly like the int/string readers --
    abi::emit_load_symbol_to_reg(emitter, "rdi", "_stream_context_options", 0); // top hash pointer (may be null)
    emitter.instruction("test rdi, rdi");                                       // is an explicit scope published?
    emitter.instruction("jnz __rt_guco_have_options_x86");                      // an explicit scope wins
    abi::emit_load_symbol_to_reg(emitter, "r10", "_stream_current_context_handle", 0);
    emitter.instruction("test r10, r10");                                       // is a context scope active?
    emitter.instruction("jnz __rt_guco_have_options_x86");                      // a scope is active: its emptiness is meaningful
    abi::emit_load_symbol_to_reg(emitter, "rdi", "_stream_default_context_handle", 0);
    emitter.instruction("test rdi, rdi");                                       // was a request default context ever created?
    emitter.instruction("jz __rt_guco_have_options_x86");                       // no default context exists
    emitter.instruction("call __rt_context_state");                             // resolve its ContextState
    emitter.instruction("test rax, rax");                                       // did the default context resolve?
    emitter.instruction("jz __rt_guco_have_options_x86");                       // a closed default context has no options
    emitter.instruction("mov rdi, QWORD PTR [rax]");                            // CONTEXT_OPTIONS_OFFSET
    emitter.label("__rt_guco_have_options_x86");
    emitter.instruction("test rdi, rdi");                                       // no options table at all?
    emitter.instruction("jz __rt_guco_miss_x86");                               // report a miss

    // -- hash_get(top, wrapper) --
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // wrapper_ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // wrapper_len
    emitter.instruction("call __rt_hash_get");                                  // rax=found, rdi=lo, rsi=hi, rcx=tag
    emitter.instruction("test rax, rax");                                       // the wrapper section is absent?
    emitter.instruction("jz __rt_guco_miss_x86");                               // report a miss

    // -- hash_get(sub, option) — the sub-hash is already in rdi --
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // opt_ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // opt_len
    emitter.instruction("call __rt_hash_get");                                  // rax=found, rdi=lo, rsi=hi, rcx=tag
    emitter.instruction("test rax, rax");                                       // the option is absent?
    emitter.instruction("jz __rt_guco_miss_x86");                               // report a miss

    // -- normalize the payload high word: hash_get answers in rsi, mixed_unbox in rdx --
    emitter.instruction("mov r11, rsi");                                        // r11 = payload high word (string length)
    emitter.instruction("cmp rcx, 7");                                          // is the option stored as a boxed Mixed cell?
    emitter.instruction("jne __rt_guco_dispatch_x86");                          // direct values dispatch as-is
    emitter.instruction("mov rax, rdi");                                        // this adapter takes its boxed cell in rax
    emitter.instruction("call __rt_mixed_unbox");                               // rax=tag, rdi=lo, rdx=hi
    emitter.instruction("mov rcx, rax");                                        // the inner tag drives the dispatch below
    emitter.instruction("mov r11, rdx");                                        // re-normalize the payload high word
    emitter.label("__rt_guco_dispatch_x86");

    emitter.instruction("cmp rcx, 0");                                          // tag 0 = int
    emitter.instruction("je __rt_guco_from_int_x86");                           // whole seconds
    emitter.instruction("cmp rcx, 3");                                          // tag 3 = bool
    emitter.instruction("je __rt_guco_from_int_x86");                           // true is one second, like PHP's cast
    emitter.instruction("cmp rcx, 2");                                          // tag 2 = float (f64 bits in the payload)
    emitter.instruction("je __rt_guco_from_float_x86");                         // fractional seconds
    emitter.instruction("cmp rcx, 1");                                          // tag 1 = string (ptr in lo, len in high word)
    emitter.instruction("je __rt_guco_from_string_x86");                        // decimal text
    emitter.instruction("jmp __rt_guco_miss_x86");                              // arrays and objects are not durations

    // -- int/bool: microseconds = value * 1_000_000 --
    emitter.label("__rt_guco_from_int_x86");
    emitter.instruction("imul rdi, rdi, 1000000");                              // seconds → microseconds
    emitter.instruction("jmp __rt_guco_write_x86");                             // publish the resolved duration

    // -- float: microseconds = trunc(value * 1_000_000) --
    emitter.label("__rt_guco_from_float_x86");
    emitter.instruction("movq xmm0, rdi");                                      // reinterpret the payload bits as a double
    emitter.instruction("mov rax, 1000000");                                    // the microsecond scale
    emitter.instruction("cvtsi2sd xmm1, rax");                                  // 1e6 as a double
    emitter.instruction("mulsd xmm0, xmm1");                                    // seconds → microseconds
    emitter.instruction("cvttsd2si rdi, xmm0");                                 // truncate toward zero, like php-src's cast
    emitter.instruction("jmp __rt_guco_write_x86");                             // publish the resolved duration

    // -- string: parse the leading `[+-]?digits[.digits]` prefix --
    emitter.label("__rt_guco_from_string_x86");
    emitter.instruction("mov r10, rdi");                                        // r10 = byte pointer
    emitter.instruction("xor r8, r8");                                          // r8 = scan index
    emitter.instruction("xor r9, r9");                                          // r9 = whole-seconds accumulator
    emitter.instruction("xor rdx, rdx");                                        // rdx = negative flag
    emitter.instruction("xor rsi, rsi");                                        // rsi = fractional digits accumulator
    emitter.instruction("xor rcx, rcx");                                        // rcx = fractional digit count
    emitter.instruction("test r11, r11");                                       // is the string empty?
    emitter.instruction("jz __rt_guco_str_scale_x86");                          // an empty string resolves to zero
    emitter.instruction("movzx eax, BYTE PTR [r10]");                           // first byte
    emitter.instruction("cmp al, 45");                                          // '-'
    emitter.instruction("jne __rt_guco_str_plus_x86");                          // not a minus sign
    emitter.instruction("mov rdx, 1");                                          // remember the sign
    emitter.instruction("inc r8");                                              // consume the sign byte
    emitter.instruction("jmp __rt_guco_str_int_loop_x86");                      // start on the digits
    emitter.label("__rt_guco_str_plus_x86");
    emitter.instruction("cmp al, 43");                                          // '+'
    emitter.instruction("jne __rt_guco_str_int_loop_x86");                      // no explicit sign
    emitter.instruction("inc r8");                                              // consume the sign byte
    emitter.label("__rt_guco_str_int_loop_x86");
    emitter.instruction("cmp r8, r11");                                         // ran off the end?
    emitter.instruction("jge __rt_guco_str_scale_x86");                         // no fractional part follows
    emitter.instruction("movzx eax, BYTE PTR [r10 + r8]");                      // current byte
    emitter.instruction("sub al, 48");                                          // '0'..'9' → 0..9
    emitter.instruction("cmp al, 9");                                           // still a digit?
    emitter.instruction("ja __rt_guco_str_int_end_x86");                        // the integer part stops here
    emitter.instruction("imul r9, r9, 10");                                     // shift the accumulator one place
    emitter.instruction("movzx eax, al");                                       // zero-extend the digit
    emitter.instruction("add r9, rax");                                         // accumulate the digit
    emitter.instruction("inc r8");                                              // advance the scan index
    emitter.instruction("jmp __rt_guco_str_int_loop_x86");                      // keep consuming digits
    emitter.label("__rt_guco_str_int_end_x86");
    emitter.instruction("movzx eax, BYTE PTR [r10 + r8]");                      // reload the terminator byte
    emitter.instruction("cmp al, 46");                                          // '.' starts a fractional part
    emitter.instruction("jne __rt_guco_str_scale_x86");                         // no fraction: the integer part is final
    emitter.instruction("inc r8");                                              // consume the decimal point
    emitter.label("__rt_guco_str_frac_loop_x86");
    emitter.instruction("cmp rcx, 6");                                          // microsecond resolution is six digits
    emitter.instruction("jge __rt_guco_str_scale_x86");                         // extra digits are below the resolution
    emitter.instruction("cmp r8, r11");                                         // ran off the end?
    emitter.instruction("jge __rt_guco_str_scale_x86");                         // the fraction stops here
    emitter.instruction("movzx eax, BYTE PTR [r10 + r8]");                      // current byte
    emitter.instruction("sub al, 48");                                          // '0'..'9' → 0..9
    emitter.instruction("cmp al, 9");                                           // still a digit?
    emitter.instruction("ja __rt_guco_str_scale_x86");                          // trailing junk ends the number
    emitter.instruction("imul rsi, rsi, 10");                                   // shift the fraction one place
    emitter.instruction("movzx eax, al");                                       // zero-extend the digit
    emitter.instruction("add rsi, rax");                                        // accumulate the fractional digit
    emitter.instruction("inc r8");                                              // advance the scan index
    emitter.instruction("inc rcx");                                             // count the consumed fractional digit
    emitter.instruction("jmp __rt_guco_str_frac_loop_x86");                     // keep consuming digits
    emitter.label("__rt_guco_str_scale_x86");
    // -- pad the fraction out to six digits so "0.5" means 500000 microseconds --
    emitter.instruction("cmp rcx, 6");                                          // already at microsecond resolution?
    emitter.instruction("jge __rt_guco_str_combine_x86");                       // nothing left to scale
    emitter.instruction("imul rsi, rsi, 10");                                   // shift the fraction one place left
    emitter.instruction("inc rcx");                                             // one more digit of padding applied
    emitter.instruction("jmp __rt_guco_str_scale_x86");                         // keep padding to six digits
    emitter.label("__rt_guco_str_combine_x86");
    emitter.instruction("imul r9, r9, 1000000");                                // whole seconds → microseconds
    emitter.instruction("add r9, rsi");                                         // add the fractional microseconds
    emitter.instruction("mov rdi, r9");                                         // the resolved duration
    emitter.instruction("test rdx, rdx");                                       // was a leading minus seen?
    emitter.instruction("jz __rt_guco_write_x86");                              // a positive duration is already final
    emitter.instruction("neg rdi");                                             // apply the leading minus sign

    emitter.label("__rt_guco_write_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // out_usec_addr
    emitter.instruction("mov QWORD PTR [r10], rdi");                            // *out_usec = resolved microseconds
    emitter.instruction("mov eax, 1");                                          // report a hit
    emitter.instruction("jmp __rt_guco_done_x86");                              // return to the caller

    emitter.label("__rt_guco_miss_x86");
    emitter.instruction("xor eax, eax");                                        // report a miss
    emitter.label("__rt_guco_done_x86");
    emitter.instruction("add rsp, 48");                                         // release runtime stack frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}
