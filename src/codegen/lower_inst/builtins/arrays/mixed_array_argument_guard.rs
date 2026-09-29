//! Purpose:
//! Emits PHP's `<function>(): Argument #1 ($array) must be of type array, <type> given` TypeError
//! for a boxed `mixed` argument that holds no array, before an array builtin reads it.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays::misc_dispatch` (`array_first`, `array_last`,
//!   `array_key_first`, `array_key_last`).
//!
//! Key details:
//! - The checker admits a `mixed` argument to an `array` parameter, so the check is PHP's
//!   run-time one. Without it the edge helpers answered `null` for a string or an int.
//! - The type name follows php-src: `int`, `string`, `float`, `null`, `resource`, `Closure` for a
//!   callable, and the literal `true` / `false` for a bool. An object is named `object`, where
//!   php-src names its class: the class name is not available at this lowering site (the same
//!   divergence `array_keys()` has).
//! - On the array path the guard leaves the UNBOXED container in `holder`, peeled through every
//!   nested Mixed box by `__rt_mixed_unbox`, so the edge helpers never see a box wrapping another
//!   box that their own normalizer would not unwrap. The caller's box keeps owning it.

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::lower_inst::exceptions::emit_type_error;
use crate::codegen::platform::Arch;
use crate::codegen::Result;

/// Runtime Mixed tags that are not array storage, with the type name php-src reports for them.
const NON_ARRAY_TAG_TYPE_NAMES: [(u64, &str); 6] = [
    (0, "int"),
    (1, "string"),
    (2, "float"),
    (8, "null"),
    (9, "resource"),
    (10, "Closure"),
];

/// Checks the boxed `mixed` value in `holder` and raises PHP's TypeError when it holds no array.
/// On the array path `holder` then holds the unboxed container (borrowed from the box); every
/// other caller-saved register may have been clobbered by the unbox call.
pub(super) fn emit_mixed_array_argument_guard(
    ctx: &mut FunctionContext<'_>,
    function: &str,
    holder: &str,
) -> Result<()> {
    let ok_label = ctx.next_label("mixed_array_arg_ok");
    let bool_label = ctx.next_label("mixed_array_arg_bool");
    let true_label = ctx.next_label("mixed_array_arg_true");
    let false_label = ctx.next_label("mixed_array_arg_false");
    let object_label = ctx.next_label("mixed_array_arg_object");
    let error_labels: Vec<(u64, &'static str, String)> = NON_ARRAY_TAG_TYPE_NAMES
        .iter()
        .map(|(tag, name)| (*tag, *name, ctx.next_label("mixed_array_arg_error")))
        .collect();
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, holder);
    if holder != result {
        ctx.emitter.instruction(&format!("mov {result}, {holder}"));            // __rt_mixed_unbox reads the boxed cell from the result register
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x9, x0");                              // keep the runtime tag past the pointer restore
            ctx.emitter.instruction("mov x10, x1");                             // keep the low payload word for the bool check
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r10, rax");                            // keep the runtime tag past the pointer restore
            ctx.emitter.instruction("mov r11, rdi");                            // keep the low payload word for the bool check
        }
    }
    abi::emit_pop_reg(ctx.emitter, holder);
    let (tag, payload, cmp, beq, jmp) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x9", "x10", "cmp", "b.eq", "b"),
        Arch::X86_64 => ("r10", "r11", "cmp", "je", "jmp"),
    };
    let arm = ctx.emitter.target.arch == Arch::AArch64;
    let imm = move |value: u64| if arm { format!("#{value}") } else { format!("{value}") };
    ctx.emitter.instruction(&format!("{cmp} {tag}, {}", imm(4)));               // runtime tag 4 = indexed-array payload
    ctx.emitter.instruction(&format!("{beq} {ok_label}"));                      // an indexed array is accepted
    ctx.emitter.instruction(&format!("{cmp} {tag}, {}", imm(5)));               // runtime tag 5 = associative-hash payload
    ctx.emitter.instruction(&format!("{beq} {ok_label}"));                      // a hash is accepted
    ctx.emitter.instruction(&format!("{cmp} {tag}, {}", imm(3)));               // runtime tag 3 = bool payload
    ctx.emitter.instruction(&format!("{beq} {bool_label}"));                    // php-src names the literal true/false, not bool
    for (value, _, label) in &error_labels {
        ctx.emitter.instruction(&format!("{cmp} {tag}, {}", imm(*value)));      // identify the payload kind for PHP's TypeError wording
        ctx.emitter.instruction(&format!("{beq} {label}"));                     // raise the TypeError naming this payload kind
    }
    ctx.emitter.instruction(&format!("{jmp} {object_label}"));                  // any remaining tag is an object or other non-array
    ctx.emitter.label(&bool_label);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz {payload}, {false_label}"));  // a zero bool payload is php-src's false
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {payload}, {payload}"));     // inspect the unboxed bool payload
            ctx.emitter.instruction(&format!("jz {false_label}"));              // a zero bool payload is php-src's false
        }
    }
    ctx.emitter.instruction(&format!("{jmp} {true_label}"));                    // every other bool payload is php-src's true
    let message = |type_name: &str| {
        format!("{function}(): Argument #1 ($array) must be of type array, {type_name} given")
    };
    ctx.emitter.label(&false_label);
    emit_type_error(ctx, &message("false"));
    ctx.emitter.label(&true_label);
    emit_type_error(ctx, &message("true"));
    for (_, type_name, label) in &error_labels {
        ctx.emitter.label(label);
        emit_type_error(ctx, &message(type_name));
    }
    ctx.emitter.label(&object_label);
    emit_type_error(ctx, &message("object"));
    ctx.emitter.label(&ok_label);
    ctx.emitter.instruction(&format!("mov {holder}, {payload}"));               // hand the helper the unboxed container, not the box
    Ok(())
}
