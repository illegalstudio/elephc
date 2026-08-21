//! Purpose:
//! Lowers the compiler-internal `expect_array_arg` runtime call: unbox an `array|false`
//! builtin argument, or throw php's TypeError for the `false`.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions` dispatch, for calls the ARGUMENT lowering
//!   inserts when an `array|false` union (scandir, glob, file…) flows into an array-taking
//!   builtin.
//!
//! Key details:
//! - The consumer's own lowering never sees the box: it receives a raw array pointer typed with
//!   the union's array member, so the ~forty array-taking lowerings stay untouched.
//! - The message is the second OPERAND — an ordinary compile-time string constant composed by
//!   the argument lowering (`{fn}(): Argument #{n} (${param}) must be of type array, false
//!   given`) — so no per-builtin symbol registration exists anywhere.

use super::*;

/// Unboxes a boxed `array|false` value or throws the TypeError the message operand spells.
pub(crate) fn lower_expect_array_arg(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "expect_array_arg", 2)?;
    let boxed = expect_operand(inst, 0)?;
    let message = expect_operand(inst, 1)?;
    ctx.load_value_to_result(boxed)?;
    let unboxed = ctx.next_label("eaa_array");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x0]");                            // the boxed payload tag
            ctx.emitter.instruction("cmp x9, #4");                              // an indexed array?
            ctx.emitter.instruction(&format!("b.eq {}", unboxed));
            ctx.emitter.instruction("cmp x9, #5");                              // or an assoc hash?
            ctx.emitter.instruction(&format!("b.eq {}", unboxed));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9, QWORD PTR [rax]");                 // the boxed payload tag
            ctx.emitter.instruction("cmp r9, 4");                               // an indexed array?
            ctx.emitter.instruction(&format!("je {}", unboxed));
            ctx.emitter.instruction("cmp r9, 5");                               // or an assoc hash?
            ctx.emitter.instruction(&format!("je {}", unboxed));
        }
    }
    // By typing the only other inhabitant is `false`, and the message operand already spells
    // it; the throw never returns.
    io::load_string_to_result(ctx, message, "expect_array_arg message")?;
    super::exceptions::emit_type_error_from_string_result(ctx);
    ctx.emitter.label(&unboxed);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x0, [x0, #8]");                        // the raw array pointer the consumer expects
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, QWORD PTR [rax + 8]");            // the raw array pointer the consumer expects
        }
    }
    store_if_result(ctx, inst)
}
