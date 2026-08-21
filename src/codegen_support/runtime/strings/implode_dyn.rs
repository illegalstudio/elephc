//! Purpose:
//! Emits the `__rt_implode_dyn` runtime helper: the element-layout dispatcher used when
//! `implode()`/`join()` receives an array operand whose STATIC type is `Mixed`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//! - Selected by `implode_runtime_label` in `crate::codegen::lower_inst::builtins::strings::split`.
//!
//! Key details:
//! - The four element renderers (`__rt_implode`, `__rt_implode_int`, `__rt_implode_bool`,
//!   `__rt_implode_float`) share ONE ABI — AArch64 `x1`/`x2` = glue ptr/len, `x3` = array ptr →
//!   `x1`/`x2` = result ptr/len; x86_64 `rdi`/`rsi` = glue ptr/len, `rdx` = array ptr →
//!   `rax`/`rdx` = result ptr/len — so this dispatcher is a pure TAIL BRANCH: it never builds a
//!   frame and never touches the operands.
//! - A `Mixed` operand carries no compile-time element type, but the indexed-array heap header
//!   does: the packed kind word at `[array - 8]` holds the runtime `value_type` tag in bits 8..15.
//!   Tag 0 is int (the unstamped default, which an empty array also carries), 1 is string, 2 is
//!   float, 3 is bool, 7 is boxed Mixed. `__rt_implode` reads 16-byte string pointer/length slots
//!   for every tag that is not 7, so sending it an INT or FLOAT array made it dereference the
//!   payload as a string pointer — a SIGSEGV — and sending it a BOOL array made it read an 8-byte
//!   payload as a 16-byte pair, rendering `implode(",", [true,false])` as `","` instead of PHP's
//!   `"1,"`.
//! - Only the whole-operand-`Mixed` case routes here. A statically typed operand still selects its
//!   renderer directly, so no existing call site changes shape.
//! - The heap kind byte is checked BEFORE the element tag: hash storage (kind 3) stamps a
//!   value_type too, so classifying it as an indexed array turned `implode(",", $hashInMixed)`
//!   from a segfault into SILENT garbage (`"10,13"` for `["k"=>1,"j"=>2]`). Non-indexed storage is
//!   therefore routed straight to `__rt_implode`, exactly where it went before this dispatcher.
//!   That fallback is now unreachable from `implode()` itself: the call site flattens hash storage
//!   into a temporary indexed array of boxed Mixed cells before branching here
//!   (`emit_dynamic_implode_hash_values_*`), because the values walk needs an emitted loop that no
//!   frameless tail-branch dispatcher can host. It is kept as a defensive arm for any other heap
//!   kind a `Mixed` operand can hold.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_implode_dyn` runtime element-layout dispatcher for `implode()`.
///
/// ABI (AArch64): x1/x2 = glue_ptr/glue_len, x3 = array_ptr → x1 = result_ptr, x2 = result_len.
/// ABI (x86_64): rdi/rsi = glue_ptr/glue_len, rdx = array_ptr → rax = result_ptr, rdx = result_len.
/// The helper only reads the array heap header and tail-branches; it allocates nothing and
/// leaves `_concat_off` entirely to the renderer it selects.
pub fn emit_implode_dyn(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_implode_dyn_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: implode_dyn (Mixed operand element-layout dispatch) ---");
    emitter.label_global("__rt_implode_dyn");

    emitter.instruction("ldr x12, [x3, #-8]");                                  // load the packed heap kind word holding the storage kind and the runtime element layout
    emitter.instruction("and x13, x12, #0xff");                                 // isolate the heap kind byte, which the persistent copy-on-write flag never occupies
    emitter.instruction("cmp x13, #2");                                         // is the operand indexed-array storage with element slots this dispatcher can classify?
    emitter.instruction("b.eq __rt_implode_dyn_indexed");                       // only indexed arrays carry an element value_type worth dispatching on
    emitter.instruction("b __rt_implode");                                      // hash storage (kind 3) and every other heap kind keep the pre-existing renderer

    emitter.label("__rt_implode_dyn_indexed");
    emitter.instruction("lsr x12, x12, #8");                                    // move the runtime value_type tag into the low bits
    emitter.instruction("and x12, x12, #0x7f");                                 // isolate the value_type tag, dropping the persistent copy-on-write flag
    emitter.instruction("cmp x12, #3");                                         // are the elements bool payloads?
    emitter.instruction("b.ne __rt_implode_dyn_not_bool");                      // fall through to the remaining layouts when the elements are not bools
    emitter.instruction("b __rt_implode_bool");                                 // PHP renders true as "1" and false as the empty string, which only this renderer does

    emitter.label("__rt_implode_dyn_not_bool");
    emitter.instruction("cmp x12, #2");                                         // are the elements raw 8-byte IEEE-754 doubles?
    emitter.instruction("b.ne __rt_implode_dyn_not_float");                     // fall through to the remaining layouts when the elements are not floats
    emitter.instruction("b __rt_implode_float");                                // doubles need PHP's precision=14 __rt_ftoa spelling, not a string pair read

    emitter.label("__rt_implode_dyn_not_float");
    emitter.instruction("cbnz x12, __rt_implode_dyn_not_int");                  // tag 0 is the unstamped int layout that an empty array also carries
    emitter.instruction("b __rt_implode_int");                                  // 8-byte int payloads must be rendered through __rt_itoa, never as string pairs

    emitter.label("__rt_implode_dyn_not_int");
    emitter.instruction("b __rt_implode");                                      // string slots (tag 1) and boxed Mixed cells (tag 7) keep the generic renderer
}

/// Emits the Linux x86_64 implementation of `__rt_implode_dyn`.
///
/// Uses `r10` as the only scratch register so the SysV argument registers carrying the glue
/// string (`rdi`/`rsi`) and the array pointer (`rdx`) reach the selected renderer untouched.
fn emit_implode_dyn_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: implode_dyn (Mixed operand element-layout dispatch) ---");
    emitter.label_global("__rt_implode_dyn");

    emitter.instruction("mov r10, QWORD PTR [rdx - 8]");                        // load the packed heap kind word holding the storage kind and the runtime element layout
    emitter.instruction("mov r11, r10");                                        // keep a copy of the kind word while the heap kind byte is isolated
    emitter.instruction("and r11, 0xff");                                       // isolate the heap kind byte, which the persistent copy-on-write flag never occupies
    emitter.instruction("cmp r11, 2");                                          // is the operand indexed-array storage with element slots this dispatcher can classify?
    emitter.instruction("je __rt_implode_dyn_indexed");                         // only indexed arrays carry an element value_type worth dispatching on
    emitter.instruction("jmp __rt_implode");                                    // hash storage (kind 3) and every other heap kind keep the pre-existing renderer

    emitter.label("__rt_implode_dyn_indexed");
    emitter.instruction("shr r10, 8");                                          // move the runtime value_type tag into the low bits
    emitter.instruction("and r10, 127");                                        // isolate the value_type tag, dropping the persistent copy-on-write flag
    emitter.instruction("cmp r10, 3");                                          // are the elements bool payloads?
    emitter.instruction("jne __rt_implode_dyn_not_bool");                       // fall through to the remaining layouts when the elements are not bools
    emitter.instruction("jmp __rt_implode_bool");                               // PHP renders true as "1" and false as the empty string, which only this renderer does

    emitter.label("__rt_implode_dyn_not_bool");
    emitter.instruction("cmp r10, 2");                                          // are the elements raw 8-byte IEEE-754 doubles?
    emitter.instruction("jne __rt_implode_dyn_not_float");                      // fall through to the remaining layouts when the elements are not floats
    emitter.instruction("jmp __rt_implode_float");                              // doubles need PHP's precision=14 __rt_ftoa spelling, not a string pair read

    emitter.label("__rt_implode_dyn_not_float");
    emitter.instruction("test r10, r10");                                       // tag 0 is the unstamped int layout that an empty array also carries
    emitter.instruction("jnz __rt_implode_dyn_not_int");                        // string and boxed Mixed layouts skip the integer renderer
    emitter.instruction("jmp __rt_implode_int");                                // 8-byte int payloads must be rendered through __rt_itoa, never as string pairs

    emitter.label("__rt_implode_dyn_not_int");
    emitter.instruction("jmp __rt_implode");                                    // string slots (tag 1) and boxed Mixed cells (tag 7) keep the generic renderer
}
