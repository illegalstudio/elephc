use crate::codegen::context::Context;
use crate::codegen::emit::Emitter;
use crate::types::PhpType;

/// Coerce a value to string (x1=ptr, x2=len) for concatenation.
/// PHP behavior: false -> "", true -> "1", null -> "", int -> itoa
pub(super) fn coerce_to_string(emitter: &mut Emitter, ty: &PhpType) {
    match ty {
        PhpType::Int => {
            // -- convert integer in x0 to string in x1/x2 --
            emitter.instruction("bl __rt_itoa");                                    // runtime: integer-to-ASCII string conversion
        }
        PhpType::Float => {
            // -- convert float in d0 to string in x1/x2 --
            emitter.instruction("bl __rt_ftoa");                                    // runtime: float-to-ASCII string conversion
        }
        PhpType::Bool => {
            // true -> "1" (via itoa), false -> "" (len=0)
            // -- convert bool to string: true="1", false="" --
            emitter.instruction("cbz x0, 1f");                                      // if false (zero), skip to empty string path
            emitter.instruction("bl __rt_itoa");                                    // convert true (1) to string "1"
            emitter.instruction("b 2f");                                            // skip over the empty-string fallback
            emitter.raw("1:");
            emitter.instruction("mov x2, #0");                                      // false produces empty string (length = 0)
            emitter.raw("2:");
        }
        PhpType::Void => {
            // -- null coerces to empty string in PHP --
            emitter.instruction("mov x2, #0");                                      // null produces empty string (length = 0)
        }
        PhpType::Str
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Callable
        | PhpType::Object(_)
        | PhpType::Pointer(_) => {}
    }
}

/// Replace null sentinel with 0 in x0 (for arithmetic/comparison with null).
/// Handles both compile-time null (Void type) and runtime null (variable
/// that was assigned null - sentinel value in x0).
pub(super) fn coerce_null_to_zero(emitter: &mut Emitter, ty: &PhpType) {
    if *ty == PhpType::Void {
        // -- compile-time null: just load zero --
        emitter.instruction("mov x0, #0");                                          // null is zero in arithmetic/comparison context
    } else if *ty == PhpType::Bool {
        // Bool is already 0/1 in x0, compatible with Int arithmetic
    } else if *ty == PhpType::Float {
        // Float is already in d0, no null sentinel to check
    } else if *ty == PhpType::Int {
        // -- runtime null check: compare x0 against sentinel value --
        emitter.instruction("movz x9, #0xFFFE");                                    // build null sentinel in x9: bits 0-15
        emitter.instruction("movk x9, #0xFFFF, lsl #16");                           // null sentinel bits 16-31
        emitter.instruction("movk x9, #0xFFFF, lsl #32");                           // null sentinel bits 32-47
        emitter.instruction("movk x9, #0x7FFF, lsl #48");                           // null sentinel bits 48-63, completing value
        emitter.instruction("cmp x0, x9");                                          // compare value against null sentinel
        emitter.instruction("csel x0, xzr, x0, eq");                                // if x0 == sentinel, replace with zero
    }
}

/// Coerce any type to a truthiness value in x0 for use in conditions
/// (if, while, for, ternary, &&, ||). For strings, PHP treats both ""
/// and "0" as falsy. For other types, x0 already holds the truthiness.
pub(super) fn coerce_to_truthiness(emitter: &mut Emitter, ctx: &mut Context, ty: &PhpType) {
    coerce_null_to_zero(emitter, ty);
    if *ty == PhpType::Str {
        // -- PHP string truthiness: "" and "0" are falsy, everything else truthy --
        let falsy_label = ctx.next_label("str_falsy");
        let truthy_label = ctx.next_label("str_truthy");
        let done_label = ctx.next_label("str_truth_done");
        emitter.instruction(&format!("cbz x2, {falsy_label}"));                     // empty string is falsy
        emitter.instruction("cmp x2, #1");                                          // check if length is 1
        emitter.instruction(&format!("b.ne {truthy_label}"));                       // length != 1 means truthy
        emitter.instruction("ldrb w9, [x1]");                                       // load first byte of string
        emitter.instruction("cmp w9, #48");                                         // compare with ASCII '0'
        emitter.instruction(&format!("b.eq {falsy_label}"));                        // string "0" is falsy
        emitter.label(&truthy_label);
        emitter.instruction("mov x0, #1");                                          // truthy: set x0 = 1
        emitter.instruction(&format!("b {done_label}"));                            // skip falsy path
        emitter.label(&falsy_label);
        emitter.instruction("mov x0, #0");                                          // falsy: set x0 = 0
        emitter.label(&done_label);
    } else if *ty == PhpType::Float {
        // -- float truthiness: 0.0 is falsy --
        emitter.instruction("fcmp d0, #0.0");                                       // compare float against zero
        emitter.instruction("cset x0, ne");                                         // x0=1 if nonzero (truthy), 0 if zero
    }
}
