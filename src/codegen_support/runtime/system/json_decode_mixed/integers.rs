//! Purpose:
//! Emits a bounded, allocation-free signed-integer token probe for JSON array prefixes.
//!
//! Called from:
//! - `super::emit_json_decode_mixed()` and `super::integer_prefix`.
//!
//! Key details:
//! - Success requires strict JSON integer grammar, signed-i64 range and an unconsumed comma or bracket.
//! - Failure publishes no parser state, preserving generic float, bigint, flags and error handling.
//! - Only caller-saved registers are used; every source load is bounded by the remaining slice.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits the target-specific leaf probe without heap, diagnostics or depth dependencies.
pub(super) fn emit(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_integer_token ---");
    emitter.label_global("__rt_json_integer_token");
    if emitter.target.arch == Arch::X86_64 { emit_x86_64(emitter); } else { emit_aarch64(emitter); }
}

/// Probes x1/x2 and returns success in x0, raw integer in x1 and consumed bytes in x2.
fn emit_aarch64(emitter: &mut Emitter) {
    // -- reject nonnumeric tokens without publishing parser state --
    emitter.instruction("mov x9, #0");                                          // start at the first token byte
    emitter.instruction("mov x10, #0");                                         // initialize the unsigned decimal magnitude
    emitter.instruction("mov x11, #0");                                         // remember a positive sign initially
    emitter.instruction("cbz x2, __rt_json_integer_token_fail");                // an empty remaining slice has no integer
    emitter.instruction("ldrb w12, [x1]");                                      // inspect the first byte inside the slice
    emitter.instruction("cmp w12, #45");                                        // check for a leading minus
    emitter.instruction("b.ne __rt_json_integer_token_first");                  // positive tokens start with a digit
    emitter.instruction("mov x11, #1");                                         // permit one additional magnitude unit for INT64_MIN
    emitter.instruction("add x9, x9, #1");                                      // consume the leading minus
    emitter.instruction("cmp x9, x2");                                          // require at least one byte after the sign
    emitter.instruction("b.hs __rt_json_integer_token_fail");                   // a bare minus uses the generic error path
    emitter.instruction("ldrb w12, [x1, x9]");                                  // inspect the first signed digit
    emitter.label("__rt_json_integer_token_first");
    emitter.instruction("sub w12, w12, #48");                                   // convert the first ASCII byte to an unsigned digit
    emitter.instruction("cmp w12, #9");                                         // accept decimal digits only
    emitter.instruction("b.hi __rt_json_integer_token_fail");                   // other JSON values use recursive decoding
    emitter.instruction("cbz w12, __rt_json_integer_token_zero");               // zero cannot be followed by another digit
    emitter.instruction("movz x14, #0xcccc");                                   // materialize INT64_MAX divided by ten
    emitter.instruction("movk x14, #0xcccc, lsl #16");                          // append the next threshold word
    emitter.instruction("movk x14, #0xcccc, lsl #32");                          // append the third threshold word
    emitter.instruction("movk x14, #0x0ccc, lsl #48");                          // complete the unsigned overflow threshold
    // -- accumulate a signed i64 without wrapping an oversized magnitude --
    emitter.label("__rt_json_integer_token_digit");
    emitter.instruction("cmp x10, x14");                                        // check the magnitude before multiplication
    emitter.instruction("b.hi __rt_json_integer_token_fail");                   // oversized integers preserve generic float or bigint handling
    emitter.instruction("b.ne __rt_json_integer_token_accumulate");             // smaller magnitudes accept every decimal digit
    emitter.instruction("mov x13, #7");                                         // the final positive digit cannot exceed seven
    emitter.instruction("add x13, x13, x11");                                   // a negative magnitude may end in eight
    emitter.instruction("cmp x12, x13");                                        // compare the boundary digit without signed overflow
    emitter.instruction("b.hi __rt_json_integer_token_fail");                   // leave oversized tokens to the generic decoder
    emitter.label("__rt_json_integer_token_accumulate");
    emitter.instruction("add x13, x10, x10, lsl #2");                           // compute five times the prior magnitude
    emitter.instruction("add x10, x12, x13, lsl #1");                           // append the digit as ten times magnitude plus digit
    emitter.instruction("add x9, x9, #1");                                      // advance past the accepted digit
    emitter.instruction("cmp x9, x2");                                          // bound the next byte load by the remaining slice
    emitter.instruction("b.hs __rt_json_integer_token_fail");                   // a token needs an array separator
    emitter.instruction("ldrb w12, [x1, x9]");                                  // inspect the next token or separator byte
    emitter.instruction("sub w13, w12, #48");                                   // test another digit without losing the original byte
    emitter.instruction("cmp w13, #9");                                         // decimal digits continue accumulation
    emitter.instruction("b.hi __rt_json_integer_token_suffix");                 // nondigits must be whitespace or a separator
    emitter.instruction("mov w12, w13");                                        // pass the decoded digit to the accumulation step
    emitter.instruction("b __rt_json_integer_token_digit");                     // continue the checked decimal scan
    emitter.label("__rt_json_integer_token_zero");
    emitter.instruction("add x9, x9, #1");                                      // consume the only permitted zero digit
    emitter.instruction("cmp x9, x2");                                          // require a suffix byte before loading it
    emitter.instruction("b.hs __rt_json_integer_token_fail");                   // preserve generic handling for a truncated array
    emitter.instruction("ldrb w12, [x1, x9]");                                  // inspect the byte after zero
    // -- accept only JSON whitespace followed by an array separator --
    emitter.label("__rt_json_integer_token_suffix");
    emitter.instruction("cmp w12, #44");                                        // a comma terminates this array element
    emitter.instruction("b.eq __rt_json_integer_token_success");                // return with the comma unconsumed
    emitter.instruction("cmp w12, #93");                                        // a closing bracket also terminates an element
    emitter.instruction("b.eq __rt_json_integer_token_success");                // leave closing and trailing-comma checks to the caller
    emitter.instruction("cmp w12, #32");                                        // accept an ASCII space after the integer
    emitter.instruction("b.eq __rt_json_integer_token_space");                  // advance only over JSON whitespace
    emitter.instruction("cmp w12, #9");                                         // accept an ASCII tab
    emitter.instruction("b.eq __rt_json_integer_token_space");                  // continue bounded suffix scanning
    emitter.instruction("cmp w12, #10");                                        // accept an ASCII line feed
    emitter.instruction("b.eq __rt_json_integer_token_space");                  // continue bounded suffix scanning
    emitter.instruction("cmp w12, #13");                                        // accept an ASCII carriage return
    emitter.instruction("b.ne __rt_json_integer_token_fail");                   // decimals, exponents and invalid suffixes use the generic path
    emitter.label("__rt_json_integer_token_space");
    emitter.instruction("add x9, x9, #1");                                      // consume one permitted whitespace byte
    emitter.instruction("cmp x9, x2");                                          // bound every suffix load
    emitter.instruction("b.hs __rt_json_integer_token_fail");                   // no separator means the token cannot use this path
    emitter.instruction("ldrb w12, [x1, x9]");                                  // load the next suffix byte
    emitter.instruction("b __rt_json_integer_token_suffix");                    // continue until a separator or unsupported byte
    emitter.label("__rt_json_integer_token_success");
    emitter.instruction("cmp x11, #0");                                         // apply the saved sign after range validation
    emitter.instruction("cneg x1, x10, ne");                                    // produce the raw two's-complement integer including INT64_MIN
    emitter.instruction("mov x2, x9");                                          // return the byte count before the unconsumed separator
    emitter.instruction("mov x0, #1");                                          // report a proven complete integer token
    emitter.instruction("ret");                                                 // return without allocating or publishing diagnostics
    emitter.label("__rt_json_integer_token_fail");
    emitter.instruction("mov x0, #0");                                          // request the ordinary JSON decoder without changing runtime state
    emitter.instruction("ret");                                                 // leave flags, depth, diagnostics and ownership untouched
}

/// Probes rax/rdx and returns success in rax, raw integer in rdi and consumed bytes in rdx.
fn emit_x86_64(emitter: &mut Emitter) {
    // -- preserve the borrowed slice in caller-saved registers --
    emitter.instruction("mov r8, rax");                                         // keep the source pointer while rax becomes scratch
    emitter.instruction("mov r9, rdx");                                         // keep the remaining slice length
    emitter.instruction("xor ecx, ecx");                                        // start at the first token byte
    emitter.instruction("xor r10d, r10d");                                      // initialize the unsigned decimal magnitude
    emitter.instruction("xor r11d, r11d");                                      // remember a positive sign initially
    emitter.instruction("test r9, r9");                                         // reject an empty remaining slice
    emitter.instruction("jz __rt_json_integer_token_fail_x");                   // avoid loading outside the slice
    emitter.instruction("movzx edi, BYTE PTR [r8]");                            // inspect the first byte inside the slice
    emitter.instruction("cmp edi, 45");                                         // check for a leading minus
    emitter.instruction("jne __rt_json_integer_token_first_x");                 // positive tokens start with a digit
    emitter.instruction("mov r11d, 1");                                         // permit one additional magnitude unit for INT64_MIN
    emitter.instruction("inc rcx");                                             // consume the minus
    emitter.instruction("cmp rcx, r9");                                         // require a byte after the sign
    emitter.instruction("jae __rt_json_integer_token_fail_x");                  // a bare minus uses the generic error path
    emitter.instruction("movzx edi, BYTE PTR [r8 + rcx]");                      // inspect the first signed digit
    emitter.label("__rt_json_integer_token_first_x");
    emitter.instruction("sub edi, 48");                                         // convert the first ASCII byte to an unsigned digit
    emitter.instruction("cmp edi, 9");                                          // accept decimal digits only
    emitter.instruction("ja __rt_json_integer_token_fail_x");                   // other JSON values use recursive decoding
    emitter.instruction("test edi, edi");                                       // zero cannot be followed by another digit
    emitter.instruction("jz __rt_json_integer_token_zero_x");                   // inspect its suffix without accepting leading zeroes
    emitter.instruction("mov rsi, 922337203685477580");                         // materialize INT64_MAX divided by ten
    // -- accumulate a signed i64 without wrapping an oversized magnitude --
    emitter.label("__rt_json_integer_token_digit_x");
    emitter.instruction("cmp r10, rsi");                                        // check the magnitude before multiplication
    emitter.instruction("ja __rt_json_integer_token_fail_x");                   // oversized integers preserve generic float or bigint handling
    emitter.instruction("jne __rt_json_integer_token_accumulate_x");            // smaller magnitudes accept every decimal digit
    emitter.instruction("lea rax, [r11 + 7]");                                  // permit a final eight only for a negative magnitude
    emitter.instruction("cmp rdi, rax");                                        // compare the boundary digit without signed overflow
    emitter.instruction("ja __rt_json_integer_token_fail_x");                   // leave oversized tokens to the generic decoder
    emitter.label("__rt_json_integer_token_accumulate_x");
    emitter.instruction("imul r10, r10, 10");                                   // multiply only after proving the resulting magnitude fits
    emitter.instruction("add r10, rdi");                                        // append the accepted decimal digit
    emitter.instruction("inc rcx");                                             // advance past the digit
    emitter.instruction("cmp rcx, r9");                                         // bound the next byte load
    emitter.instruction("jae __rt_json_integer_token_fail_x");                  // a token needs an array separator
    emitter.instruction("movzx edi, BYTE PTR [r8 + rcx]");                      // inspect the next token or separator byte
    emitter.instruction("lea eax, [rdi - 48]");                                 // test another digit without losing the original byte
    emitter.instruction("cmp eax, 9");                                          // decimal digits continue accumulation
    emitter.instruction("ja __rt_json_integer_token_suffix_x");                 // nondigits must be whitespace or a separator
    emitter.instruction("mov edi, eax");                                        // pass the decoded digit to the accumulation step
    emitter.instruction("jmp __rt_json_integer_token_digit_x");                 // continue the checked decimal scan
    emitter.label("__rt_json_integer_token_zero_x");
    emitter.instruction("inc rcx");                                             // consume the only permitted zero digit
    emitter.instruction("cmp rcx, r9");                                         // require a suffix byte before loading it
    emitter.instruction("jae __rt_json_integer_token_fail_x");                  // preserve generic handling for a truncated array
    emitter.instruction("movzx edi, BYTE PTR [r8 + rcx]");                      // inspect the byte after zero
    // -- accept only JSON whitespace followed by an array separator --
    emitter.label("__rt_json_integer_token_suffix_x");
    emitter.instruction("cmp edi, 44");                                         // a comma terminates this array element
    emitter.instruction("je __rt_json_integer_token_success_x");                // return with the comma unconsumed
    emitter.instruction("cmp edi, 93");                                         // a closing bracket also terminates an element
    emitter.instruction("je __rt_json_integer_token_success_x");                // leave closing and trailing-comma checks to the caller
    emitter.instruction("cmp edi, 32");                                         // accept an ASCII space after the integer
    emitter.instruction("je __rt_json_integer_token_space_x");                  // advance only over JSON whitespace
    emitter.instruction("cmp edi, 9");                                          // accept an ASCII tab
    emitter.instruction("je __rt_json_integer_token_space_x");                  // continue bounded suffix scanning
    emitter.instruction("cmp edi, 10");                                         // accept an ASCII line feed
    emitter.instruction("je __rt_json_integer_token_space_x");                  // continue bounded suffix scanning
    emitter.instruction("cmp edi, 13");                                         // accept an ASCII carriage return
    emitter.instruction("jne __rt_json_integer_token_fail_x");                  // decimals, exponents and invalid suffixes use the generic path
    emitter.label("__rt_json_integer_token_space_x");
    emitter.instruction("inc rcx");                                             // consume one permitted whitespace byte
    emitter.instruction("cmp rcx, r9");                                         // bound every suffix load
    emitter.instruction("jae __rt_json_integer_token_fail_x");                  // no separator means the token cannot use this path
    emitter.instruction("movzx edi, BYTE PTR [r8 + rcx]");                      // load the next suffix byte
    emitter.instruction("jmp __rt_json_integer_token_suffix_x");                // continue until a separator or unsupported byte
    emitter.label("__rt_json_integer_token_success_x");
    emitter.instruction("test r11, r11");                                       // inspect the saved sign
    emitter.instruction("jz __rt_json_integer_token_positive_x");               // positive magnitudes need no adjustment
    emitter.instruction("neg r10");                                             // produce the raw negative integer including INT64_MIN
    emitter.label("__rt_json_integer_token_positive_x");
    emitter.instruction("mov rdi, r10");                                        // return the raw integer payload
    emitter.instruction("mov rdx, rcx");                                        // return the byte count before the unconsumed separator
    emitter.instruction("mov eax, 1");                                          // report a proven complete integer token
    emitter.instruction("ret");                                                 // return without allocating or publishing diagnostics
    emitter.label("__rt_json_integer_token_fail_x");
    emitter.instruction("xor eax, eax");                                        // request the ordinary JSON decoder without changing runtime state
    emitter.instruction("ret");                                                 // leave flags, depth, diagnostics and ownership untouched
}
