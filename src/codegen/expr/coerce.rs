//! Purpose:
//! Performs expression-result coercions between PHP scalar, string, object, array, nullable, and Mixed shapes.
//! Used when assignment, calls, or operators need a value in a declared target type.
//!
//! Called from:
//! - `crate::codegen::expr` and statement assignment emitters
//!
//! Key details:
//! - Coercions may allocate, retain, or box values, so ownership state must be updated with the result.

use crate::codegen::context::Context;
use crate::codegen::NULL_SENTINEL;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::{abi, platform::Arch};
use crate::types::PhpType;

/// Coerce a value to a PHP string for concatenation (`x1`/`x2` on ARM64, `rsi`/`rdx` on x86_64).
///
/// Dispatches to runtime helpers based on type:
/// - `Int` → `__rt_itoa` (integer-to-ASCII)
/// - `Float` → `__rt_ftoa` (float-to-ASCII)
/// - `Bool` → true `"1"` / false `""` (zero-length)
/// - `Void`/`Never` → `""` (zero-length)
/// - `Resource` → `__rt_resource_to_string`
/// - `Mixed`/`Union` → `__rt_mixed_cast_string` (runtime dispatch on boxed payload)
/// - `Iterable` → literal `"Array"` string
/// - `Object` → invokes `__toString()` if present, otherwise emits a fatal error and terminates
///
/// ABI: places string pointer in first string-result register and length in second.
/// Ownership: callers must treat the returned string as owned (runtime may allocate).
pub fn coerce_to_string(
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    ty: &PhpType,
) {
    coerce_to_string_inner(emitter, ctx, data, ty, false);
}

/// Like [`coerce_to_string`], but when `release_owned_object` is set and `ty` is an object
/// stringified via `__toString`, the owned object temporary is released after conversion.
///
/// Callers pass `true` only when the source expression produced an owned object temporary
/// (e.g. `new C()` or a call result); a borrowed object (a variable or property) must pass
/// `false` so its owner — not this coercion — releases it, avoiding a double free.
pub fn coerce_to_string_releasing_owned(
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    ty: &PhpType,
    release_owned_object: bool,
) {
    coerce_to_string_inner(emitter, ctx, data, ty, release_owned_object);
}

/// Shared body of the string coercion. `release_owned_object` controls whether an owned
/// object temporary that is stringified via `__toString` is released after conversion.
fn coerce_to_string_inner(
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    ty: &PhpType,
    release_owned_object: bool,
) {
    match ty {
        PhpType::Int => {
            // -- convert integer in x0 to string in x1/x2 --
            abi::emit_call_label(emitter, "__rt_itoa");                         // runtime: integer-to-ASCII string conversion
        }
        PhpType::Resource(_) => {
            // -- convert resource in x0/rax to PHP's display string --
            abi::emit_call_label(emitter, "__rt_resource_to_string");           // runtime: resource-to-display-string conversion
        }
        PhpType::Float => {
            // -- convert float in d0 to string in x1/x2 --
            abi::emit_call_label(emitter, "__rt_ftoa");                         // runtime: float-to-ASCII string conversion
        }
        PhpType::Bool => {
            // true -> "1" (via itoa), false -> "" (len=0)
            // -- convert bool to string: true="1", false="" --
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("cbz x0, 1f");                          // if false (zero), skip to empty string path
                    abi::emit_call_label(emitter, "__rt_itoa");                 // convert true (1) to string "1"
                    emitter.instruction("b 2f");                                // skip over the empty-string fallback
                    emitter.raw("1:");
                    emitter.instruction("mov x2, #0");                          // false produces empty string (length = 0)
                    emitter.raw("2:");
                }
                Arch::X86_64 => {
                    let false_label = ctx.next_label("bool_to_str_false");
                    let done_label = ctx.next_label("bool_to_str_done");
                    emitter.instruction("test rax, rax");                       // test whether the boolean payload is false
                    emitter.instruction(&format!("je {}", false_label));        // skip to the empty-string path when the boolean is false
                    abi::emit_call_label(emitter, "__rt_itoa");                 // convert true (1) to string "1"
                    emitter.instruction(&format!("jmp {}", done_label));        // skip over the empty-string fallback after conversion
                    emitter.label(&false_label);
                    emitter.instruction("mov rdx, 0");                          // false produces empty string (length = 0)
                    emitter.label(&done_label);
                }
            }
        }
        PhpType::Void | PhpType::Never => {
            // -- null coerces to empty string in PHP --
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("mov x2, #0");                          // null produces empty string (length = 0)
                }
                Arch::X86_64 => {
                    emitter.instruction("mov rdx, 0");                          // null produces empty string (length = 0)
                }
            }
        }
        PhpType::TaggedScalar => {
            // -- tagged scalar: null -> empty string, int payload -> decimal text --
            let null_label = ctx.next_label("tagged_to_str_null");
            let done_label = ctx.next_label("tagged_to_str_done");
            crate::codegen::sentinels::emit_branch_if_tagged_scalar_null(emitter, &null_label);
            abi::emit_call_label(emitter, "__rt_itoa");                         // convert the non-null tagged scalar payload to decimal text
            abi::emit_jump(emitter, &done_label);                               // skip the empty-string fallback after conversion
            emitter.label(&null_label);
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("mov x2, #0");                          // null produces empty string (length = 0)
                }
                Arch::X86_64 => {
                    emitter.instruction("mov rdx, 0");                          // null produces empty string (length = 0)
                }
            }
            emitter.label(&done_label);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            // -- mixed strings dispatch on the boxed payload at runtime --
            abi::emit_call_label(emitter, "__rt_mixed_cast_string");            // cast the boxed mixed payload to string in the ABI string result registers
        }
        PhpType::Iterable | PhpType::Array(_) | PhpType::AssocArray { .. } => {
            // -- iterable and array values stringify to the literal "Array", matching PHP --
            let (label, len) = data.add_string(b"Array");
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_symbol_address(emitter, ptr_reg, &label);                 // materialize the literal "Array" address in the active string-pointer result register
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction(&format!("mov {}, #{}", len_reg, len)); // load the literal "Array" byte length into the active AArch64 string-length result register
                }
                Arch::X86_64 => {
                    emitter.instruction(&format!("mov {}, {}", len_reg, len));  // load the literal "Array" byte length into the active x86_64 string-length result register
                }
            }
        }
        PhpType::Object(class_name) => {
            if ctx
                .classes
                .get(class_name)
                .is_some_and(|class_info| class_info.methods.contains_key("__tostring"))
            {
                if release_owned_object {
                    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));  // save the owned object temp below $this so it can be released after __toString borrows it
                }
                abi::emit_push_reg(emitter, abi::int_result_reg(emitter));      // push $this pointer for __toString dispatch using the active target ABI
                super::objects::emit_method_call_with_pushed_args(
                    class_name,
                    "__tostring",
                    &[],
                    emitter,
                    ctx,
                );
                if release_owned_object {
                    emit_release_saved_object_temp(emitter, ty);
                }
            } else {
                emit_missing_tostring_fatal(emitter, data, class_name);
            }
        }
        PhpType::Str
        | PhpType::Callable
        | PhpType::Buffer(_)
        | PhpType::Packed(_)
        | PhpType::Pointer(_) => {}
    }
}

/// Releases an owned object temporary that was just stringified via `__toString`.
///
/// On entry the produced string is in the string result registers, and the object pointer
/// was saved on the temporary stack below the (already-popped) `$this` slot. The string
/// result is preserved across the object decref, then restored, and the saved object slot
/// is discarded — leaving the string result in place and the object temporary freed.
fn emit_release_saved_object_temp(emitter: &mut Emitter, ty: &PhpType) {
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);                         // preserve the __toString result string across the object decref call
    abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 16); // reload the saved owned object pointer from below the 16-byte string slot
    abi::emit_decref_if_refcounted(emitter, ty);                                // release the owned object temporary now that __toString produced its string
    abi::emit_pop_reg_pair(emitter, ptr_reg, len_reg);                          // restore the preserved __toString result string
    abi::emit_release_temporary_stack(emitter, 16);                             // discard the saved owned object slot
}

/// Emit a fatal error and terminate when an object without `__toString()` is coerced to string.
///
/// Writes the error message to stderr using platform syscalls, then exits with code 1.
/// This function does not return.
fn emit_missing_tostring_fatal(emitter: &mut Emitter, data: &mut DataSection, class_name: &str) {
    let message = format!(
        "Fatal error: Object of class {} could not be converted to string\n",
        class_name
    );
    let (label, len) = data.add_string(message.as_bytes());
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // fd = stderr for fatal conversion diagnostics
            abi::emit_symbol_address(emitter, "x1", &label);                    // load the page that contains the fatal conversion message
            emitter.instruction(&format!("mov x2, #{}", len));                  // pass the fatal conversion message length to write()
            emitter.syscall(4);
            emitter.instruction("mov x0, #1");                                  // exit status 1 indicates abnormal termination
            emitter.syscall(1);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "rsi", &label);                   // point the Linux write() buffer register at the fatal conversion message
            emitter.instruction(&format!("mov edx, {}", len));                  // pass the fatal conversion message length to write()
            emitter.instruction("mov edi, 2");                                  // fd = stderr for fatal conversion diagnostics
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall 1 = write
            emitter.instruction("syscall");                                     // emit the fatal conversion message before terminating
            emitter.instruction("mov edi, 1");                                  // exit status 1 indicates abnormal termination
            emitter.instruction("mov eax, 60");                                 // Linux x86_64 syscall 60 = exit
            emitter.instruction("syscall");                                     // terminate the process after reporting the failed string conversion
        }
    }
}

/// Replace null sentinel with 0 in the integer result register after `coerce_null_to_zero`.
///
/// Two cases are handled:
/// - **Compile-time null** (`Void`/`Never` type): directly emit `mov x0/rax, #0/0`.
/// - **Runtime null** (Int payload that may hold the sentinel `0x7FFFFFFFFFFFFFFF_FFFE`): compare
///   against the sentinel and select zero when equal; ordinary integers are unchanged.
///
/// `Bool` and `Float` are no-ops because their representations already match integer zero
/// (bool is 0/1 in x0, float is in d0).
pub fn coerce_null_to_zero(emitter: &mut Emitter, ty: &PhpType) {
    if *ty == PhpType::Void {
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x0, #0");                              // null is zero in arithmetic/comparison context
            }
            Arch::X86_64 => {
                emitter.instruction("mov rax, 0");                              // null is zero in arithmetic/comparison context
            }
        }
    } else if *ty == PhpType::Bool {
        // Bool is already 0/1 in x0, compatible with Int arithmetic
    } else if *ty == PhpType::Float {
        // Float is already in d0, no null sentinel to check
    } else if *ty == PhpType::TaggedScalar {
        crate::codegen::sentinels::emit_tagged_scalar_to_int_null_as_zero(emitter);
    } else if *ty == PhpType::Int && crate::codegen::sentinels::null_repr_is_tagged() {
        // Under the tagged representation a plain Int can never hold the null sentinel
    } else if *ty == PhpType::Int {
        match emitter.target.arch {
            Arch::AArch64 => {
                let sentinel = NULL_SENTINEL as u64;
                emitter.instruction(&format!("movz x9, #0x{:X}", sentinel & 0xFFFF)); // build null sentinel in x9: bits 0-15
                emitter.instruction(&format!("movk x9, #0x{:X}, lsl #16", (sentinel >> 16) & 0xFFFF)); // null sentinel bits 16-31
                emitter.instruction(&format!("movk x9, #0x{:X}, lsl #32", (sentinel >> 32) & 0xFFFF)); // null sentinel bits 32-47
                emitter.instruction(&format!("movk x9, #0x{:X}, lsl #48", (sentinel >> 48) & 0xFFFF)); // null sentinel bits 48-63, completing value
                emitter.instruction("cmp x0, x9");                              // compare value against null sentinel
                emitter.instruction("csel x0, xzr, x0, eq");                    // if x0 == sentinel, replace with zero
            }
            Arch::X86_64 => {
                let sentinel_reg = abi::temp_int_reg(emitter.target);
                let zero_reg = abi::symbol_scratch_reg(emitter);
                emitter.instruction(&format!("mov {}, {}", sentinel_reg, NULL_SENTINEL)); // materialize the runtime null sentinel in a scratch register
                emitter.instruction(&format!("xor {}, {}", zero_reg, zero_reg)); // materialize an integer zero in a second scratch register
                emitter.instruction(&format!("cmp {}, {}", abi::int_result_reg(emitter), sentinel_reg)); // compare the current integer result against the runtime null sentinel
                emitter.instruction(&format!("cmove {}, {}", abi::int_result_reg(emitter), zero_reg)); // replace the sentinel with zero while leaving ordinary integers unchanged
            }
        }
    }
}

/// Coerce a typed expression result to a raw PHP integer in the integer result register.
///
/// First normalizes null via [`coerce_null_to_zero`], then for `Mixed`/`Union` values calls
/// `__rt_mixed_cast_int` to unbox the boxed payload (int|bool|string|float) into a plain `i64`.
/// `Int`/`Bool`/`Float` already occupy the right register and are left unchanged.
///
/// This is the shared coercion used by arithmetic, bitwise, comparison, and integer-argument
/// builtins (e.g. `intdiv`), so a boxed Mixed operand is never consumed as a raw integer.
pub fn coerce_to_int(emitter: &mut Emitter, ty: &PhpType) {
    coerce_null_to_zero(emitter, ty);
    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_call_label(emitter, "__rt_mixed_cast_int");                   // normalize boxed int|bool|string values into a raw integer
    } else if *ty == PhpType::Str {
        // A string operand lives in the string-result registers; on x86_64 the pointer also
        // occupies the integer result register, so an unconverted string would be compared as a
        // raw pointer. Parse it through PHP string-to-int rules into the integer register.
        abi::emit_call_label(emitter, "__rt_str_to_int");                       // numeric value of the string operand in the int register
    }
}

/// Coerce any type to a PHP truthiness value in the integer result register.
///
/// Handles all PHP truthiness rules:
/// - Calls `coerce_null_to_zero` first to normalize null for all types.
/// - `Str`: `""` and `"0"` are falsy; all other strings (including `"1"`, `"-1"`) are truthy.
/// - `Float`: 0.0 is falsy; non-zero (including negative zero) is truthy.
/// - `Int`/`Bool`/`Void`/`Callable`/`Object`/`Buffer`/`Packed`/`Pointer`: non-zero is truthy.
/// - `Resource`: always truthy (regardless of native handle value).
/// - `Array`/`AssocArray`/`Iterable`: non-empty (runtime length > 0) is truthy.
/// - `Mixed`/`Union`: delegates to `__rt_mixed_cast_bool` for runtime dispatch.
///
/// Result is placed in the canonical integer result register (`x0`/`rax`).
pub fn coerce_to_truthiness(emitter: &mut Emitter, ctx: &mut Context, ty: &PhpType) {
    coerce_null_to_zero(emitter, ty);
    if *ty == PhpType::Str {
        // -- PHP string truthiness: "" and "0" are falsy, everything else truthy --
        let falsy_label = ctx.next_label("str_falsy");
        let truthy_label = ctx.next_label("str_truthy");
        let done_label = ctx.next_label("str_truth_done");
        let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("cbz {}, {falsy_label}", len_reg)); // empty string is falsy
                emitter.instruction(&format!("cmp {}, #1", len_reg));           // check if length is 1
                emitter.instruction(&format!("b.ne {truthy_label}"));           // length != 1 means truthy
                emitter.instruction(&format!("ldrb w9, [{}]", ptr_reg));        // load first byte of string
                emitter.instruction("cmp w9, #48");                             // compare with ASCII '0'
                emitter.instruction(&format!("b.eq {falsy_label}"));            // string "0" is falsy
                emitter.label(&truthy_label);
                emitter.instruction(&format!("mov {}, #1", abi::int_result_reg(emitter))); // truthy: set result = 1
                emitter.instruction(&format!("b {done_label}"));                // skip falsy path
                emitter.label(&falsy_label);
                emitter.instruction(&format!("mov {}, #0", abi::int_result_reg(emitter))); // falsy: set result = 0
                emitter.label(&done_label);
            }
            Arch::X86_64 => {
                let scratch = abi::temp_int_reg(emitter.target);
                emitter.instruction(&format!("test {}, {}", len_reg, len_reg)); // empty string is falsy
                emitter.instruction(&format!("je {}", falsy_label));            // branch to falsy path when the string length is zero
                emitter.instruction(&format!("cmp {}, 1", len_reg));            // check if length is 1
                emitter.instruction(&format!("jne {}", truthy_label));          // any other non-empty length is truthy
                emitter.instruction(&format!("movzx {}d, BYTE PTR [{}]", scratch, ptr_reg)); // load the first byte of the one-character string
                emitter.instruction(&format!("cmp {}d, 48", scratch));          // compare against ASCII '0'
                emitter.instruction(&format!("je {}", falsy_label));            // the string \"0\" is falsy in PHP
                emitter.label(&truthy_label);
                emitter.instruction(&format!("mov {}, 1", abi::int_result_reg(emitter))); // truthy: set result = 1
                emitter.instruction(&format!("jmp {}", done_label));            // skip the falsy path once the result is known
                emitter.label(&falsy_label);
                emitter.instruction(&format!("mov {}, 0", abi::int_result_reg(emitter))); // falsy: set result = 0
                emitter.label(&done_label);
            }
        }
    } else if *ty == PhpType::Float {
        // -- float truthiness: 0.0 is falsy --
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("fcmp d0, #0.0");                           // compare float against zero
                emitter.instruction("cset x0, ne");                             // x0=1 if nonzero (truthy), 0 if zero
            }
            Arch::X86_64 => {
                let bits_reg = abi::temp_int_reg(emitter.target);
                emitter.instruction(&format!("movq {}, xmm0", bits_reg));       // move the current float bits into a scratch integer register
                emitter.instruction(&format!("shl {}, 1", bits_reg));           // discard the sign bit so +0.0 and -0.0 both normalize to zero
                emitter.instruction(&format!("cmp {}, 0", bits_reg));           // compare the signless float bits against zero
                emitter.instruction("setne al");                                // set al when the float payload is non-zero
                emitter.instruction("movzx rax, al");                           // widen the boolean byte into the full integer result register
            }
        }
    } else if matches!(
        ty,
        PhpType::Int
            | PhpType::Bool
            | PhpType::Void
            | PhpType::Callable
            | PhpType::Object(_)
            | PhpType::Buffer(_)
            | PhpType::Packed(_)
            | PhpType::Pointer(_)
    ) {
        // -- scalars and pointer-like values are truthy when non-zero --
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("cmp x0, #0");                              // compare the normalized scalar/pointer value against zero
                emitter.instruction("cset x0, ne");                             // produce 1 when the scalar/pointer value is non-zero, else 0
            }
            Arch::X86_64 => {
                let result_reg = abi::int_result_reg(emitter);
                emitter.instruction(&format!("test {}, {}", result_reg, result_reg)); // compare the normalized scalar/pointer value against zero
                emitter.instruction("setne al");                                // produce a boolean byte when the scalar/pointer value is non-zero
                emitter.instruction("movzx rax, al");                           // widen the boolean byte into the canonical integer result register
            }
        }
    } else if matches!(ty, PhpType::Resource(_)) {
        // -- PHP resources are truthy regardless of their underlying native handle value --
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x0, #1");                              // resources always coerce to true
            }
            Arch::X86_64 => {
                emitter.instruction("mov rax, 1");                              // resources always coerce to true
            }
        }
    } else if matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable) {
        // -- arrays and iterable hash payloads are truthy when their runtime length is non-zero --
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("ldr x0, [x0]");                            // load the runtime array length from the header
                emitter.instruction("cmp x0, #0");                              // compare the array length against zero
                emitter.instruction("cset x0, ne");                             // produce 1 for non-empty arrays, else 0
            }
            Arch::X86_64 => {
                emitter.instruction("mov rax, QWORD PTR [rax]");                // load the runtime array length from the header
                emitter.instruction("test rax, rax");                           // compare the array length against zero
                emitter.instruction("setne al");                                // produce a boolean byte when the array is non-empty
                emitter.instruction("movzx rax, al");                           // widen the boolean byte into the canonical integer result register
            }
        }
    } else if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        // -- mixed/union truthiness dispatches on the boxed payload at runtime --
        abi::emit_call_label(emitter, "__rt_mixed_cast_bool");                  // normalize the boxed mixed payload to PHP truthiness
    }
}
