//! Purpose:
//! Emits the AArch64 runtime loop for `array_filter()` over a boxed gradual array.
//! Preserves PHP keys, insertion order, and retained child ownership in a unified result hash.
//!
//! Called from:
//! - `super::emit_array_filter_mixed()` for the AArch64 targets.
//!
//! Key details:
//! - Callback-facing values and keys are temporary owned `Mixed` cells released after each call.
//! - Kept hash entries copy the original payload so PHP reference cells remain shared.

use crate::codegen_support::emit::Emitter;

use super::ARRAY_FILTER_MODE_MSG_LEN;
use super::super::value_error;

/// Emits the AArch64 `__rt_array_filter_mixed` helper.
pub(super) fn emit(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_filter_mixed ---");
    emitter.label_global("__rt_array_filter_mixed");

    // Frame slots: callback=0, source box=8, env=16, mode=24, source tag=32,
    // source ptr=40, result ptr=48, cursor=56, key=64/72, value=80/88/96,
    // callback value=104, callback key=112, keep=120, length=128, width=136,
    // indexed value tag=144, saved fp/lr=176/184.
    emitter.instruction("sub sp, sp, #192");                                   // reserve filter state and an aligned nested-call frame
    emitter.instruction("stp x29, x30, [sp, #176]");                          // preserve frame pointer and return address
    emitter.instruction("add x29, sp, #176");                                 // establish the runtime helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                  // save callback address
    emitter.instruction("str x1, [sp, #8]");                                  // save boxed source array
    emitter.instruction("str x2, [sp, #16]");                                 // save optional callback environment
    emitter.instruction("str x3, [sp, #24]");                                 // save callback mode

    emitter.instruction("cmp x3, #0");                                        // accept value-only mode
    emitter.instruction("b.eq __rt_array_filter_mixed_mode_valid");
    emitter.instruction("cmp x3, #1");                                        // accept value-and-key mode
    emitter.instruction("b.eq __rt_array_filter_mixed_mode_valid");
    emitter.instruction("cmp x3, #2");                                        // accept key-only mode
    emitter.instruction("b.eq __rt_array_filter_mixed_mode_valid");
    emitter.instruction("b __rt_array_filter_mixed_invalid_mode");            // reject every other mode with ValueError
    emitter.label("__rt_array_filter_mixed_mode_valid");

    emitter.instruction("ldr x0, [sp, #8]");                                  // reload boxed source for runtime shape dispatch
    emitter.instruction("bl __rt_mixed_unbox");                               // x0=tag, x1=payload pointer
    emitter.instruction("str x0, [sp, #32]");                                 // preserve source runtime tag
    emitter.instruction("str x1, [sp, #40]");                                 // preserve borrowed source payload pointer
    emitter.instruction("cmp x0, #4");                                        // tag 4 = indexed array
    emitter.instruction("b.eq __rt_array_filter_mixed_indexed_setup");
    emitter.instruction("b __rt_array_filter_mixed_hash_setup");              // the codegen guard permits only tag 5 otherwise

    emitter.label("__rt_array_filter_mixed_indexed_setup");
    emitter.instruction("ldr x9, [x1]");                                      // snapshot indexed source length
    emitter.instruction("str x9, [sp, #128]");                                // preserve source length across callbacks
    emitter.instruction("ldr x10, [x1, #16]");                               // load indexed element width
    emitter.instruction("str x10, [sp, #136]");                               // preserve element width for reads and appends
    emitter.instruction("ldr x11, [x1, #-8]");                               // load packed indexed-array metadata
    emitter.instruction("ubfx x11, x11, #8, #7");                            // extract runtime element value tag
    emitter.instruction("str x11, [sp, #144]");                               // preserve source value tag
    emitter.instruction("cmp x9, #4");                                        // hash insertion needs a non-zero minimum capacity
    emitter.instruction("b.ge __rt_array_filter_mixed_indexed_capacity_ready");
    emitter.instruction("mov x9, #4");
    emitter.label("__rt_array_filter_mixed_indexed_capacity_ready");
    emitter.instruction("mov x0, x9");                                        // destination capacity follows source length
    emitter.instruction("mov x1, #7");                                        // filtered entries may be heterogeneous
    emitter.instruction("bl __rt_hash_new");                                  // hash storage preserves original numeric keys
    emitter.instruction("str x0, [sp, #48]");                                 // save destination hash pointer
    emitter.instruction("str xzr, [sp, #56]");                                // source index starts at zero

    emitter.label("__rt_array_filter_mixed_indexed_loop");
    emitter.instruction("ldr x9, [sp, #56]");                                 // reload source index
    emitter.instruction("ldr x10, [sp, #128]");                               // reload source length
    emitter.instruction("cmp x9, x10");
    emitter.instruction("b.ge __rt_array_filter_mixed_indexed_done");          // stop after the source snapshot is consumed
    emitter.instruction("ldr x0, [sp, #40]");                                 // raw indexed source pointer
    emitter.instruction("mov x1, x9");                                        // integer key low word
    emitter.instruction("mov x2, #-1");                                       // integer-key sentinel
    emitter.instruction("mov x3, #0");                                        // filtering reads never warn for known-present keys
    emitter.instruction("bl __rt_array_get_mixed_key");                       // return an owned callback-facing Mixed value
    emitter.instruction("str x0, [sp, #104]");                                // preserve callback value across key boxing
    emitter.instruction("str xzr, [sp, #112]");                               // default to no key box for value-only mode
    emitter.instruction("ldr x9, [sp, #24]");                                 // reload callback mode
    emitter.instruction("cbz x9, __rt_array_filter_mixed_indexed_call");       // value-only mode needs no key box
    emitter.instruction("mov x0, #0");                                        // runtime tag 0 = integer key
    emitter.instruction("ldr x1, [sp, #56]");                                 // key payload is the source index
    emitter.instruction("mov x2, #0");                                        // integer keys have no high payload word
    emitter.instruction("bl __rt_mixed_from_value");                          // allocate callback-facing Mixed key
    emitter.instruction("str x0, [sp, #112]");                                // preserve owned key cell for callback and release

    emitter.label("__rt_array_filter_mixed_indexed_call");
    emit_callback_call(
        emitter,
        "__rt_array_filter_mixed_indexed_callback",
        "__rt_array_filter_mixed_indexed_after_call",
    );
    emitter.label("__rt_array_filter_mixed_indexed_after_call");
    emit_release_callback_values(emitter);
    emitter.instruction("ldr x9, [sp, #120]");                                // reload callback truthiness result
    emitter.instruction("cbz x9, __rt_array_filter_mixed_indexed_next");       // discard rejected elements
    emitter.instruction("ldr x10, [sp, #136]");                               // reload source element width
    emitter.instruction("cmp x10, #16");                                      // string slots carry a second payload word
    emitter.instruction("b.eq __rt_array_filter_mixed_indexed_value_string");
    emitter.instruction("ldr x11, [sp, #40]");                                // reload raw source array
    emitter.instruction("add x11, x11, #24");                                 // advance to source payload base
    emitter.instruction("ldr x12, [sp, #56]");                                // reload source index
    emitter.instruction("ldr x3, [x11, x12, lsl #3]");                        // load original scalar/refcounted payload
    emitter.instruction("mov x4, #0");                                        // non-string payloads have no high word
    emitter.instruction("ldr x5, [sp, #144]");                                // preserve original source value tag
    emitter.instruction("b __rt_array_filter_mixed_indexed_value_ready");
    emitter.label("__rt_array_filter_mixed_indexed_value_string");
    emitter.instruction("ldr x11, [sp, #40]");                                // reload raw source array
    emitter.instruction("add x11, x11, #24");                                 // advance to source payload base
    emitter.instruction("ldr x12, [sp, #56]");                                // reload source index
    emitter.instruction("lsl x12, x12, #4");                                  // string slots are 16 bytes wide
    emitter.instruction("add x11, x11, x12");
    emitter.instruction("ldr x3, [x11]");                                     // source string pointer
    emitter.instruction("ldr x4, [x11, #8]");                                 // source string length
    emitter.instruction("mov x5, #1");                                        // runtime tag 1 = string
    emitter.label("__rt_array_filter_mixed_indexed_value_ready");
    emitter.instruction("str x3, [sp, #80]");                                 // preserve original value low word
    emitter.instruction("str x4, [sp, #88]");                                 // preserve original value high word
    emitter.instruction("str x5, [sp, #96]");                                 // preserve original value tag
    emitter.instruction("cmp x5, #1");                                        // string payload requires a retained owner
    emitter.instruction("b.eq __rt_array_filter_mixed_indexed_retain");
    emitter.instruction("cmp x5, #4");                                        // indexed child pointer?
    emitter.instruction("b.lt __rt_array_filter_mixed_indexed_insert");
    emitter.instruction("cmp x5, #7");                                        // array/hash/object/Mixed pointer range?
    emitter.instruction("b.le __rt_array_filter_mixed_indexed_retain");
    emitter.instruction("cmp x5, #10");                                       // callable descriptor pointer?
    emitter.instruction("b.eq __rt_array_filter_mixed_indexed_retain");
    emitter.instruction("cmp x5, #11");                                       // PHP reference cell pointer?
    emitter.instruction("b.ne __rt_array_filter_mixed_indexed_insert");
    emitter.label("__rt_array_filter_mixed_indexed_retain");
    emitter.instruction("mov x0, x3");                                        // borrowed source child pointer
    emitter.instruction("bl __rt_incref");                                    // result hash takes an owning share
    emitter.label("__rt_array_filter_mixed_indexed_insert");
    emitter.instruction("ldr x0, [sp, #48]");                                 // destination hash pointer
    emitter.instruction("ldr x1, [sp, #56]");                                 // preserve original integer key
    emitter.instruction("mov x2, #-1");                                       // integer-key sentinel
    emitter.instruction("ldr x3, [sp, #80]");                                 // original value low word
    emitter.instruction("ldr x4, [sp, #88]");                                 // original value high word
    emitter.instruction("ldr x5, [sp, #96]");                                 // original value tag
    emitter.instruction("bl __rt_hash_set");                                  // transfer retained payload into filtered hash
    emitter.instruction("str x0, [sp, #48]");                                 // save possibly grown destination pointer
    emitter.label("__rt_array_filter_mixed_indexed_next");
    emitter.instruction("ldr x9, [sp, #56]");                                 // reload source index after nested calls
    emitter.instruction("add x9, x9, #1");
    emitter.instruction("str x9, [sp, #56]");                                 // advance to next indexed element
    emitter.instruction("b __rt_array_filter_mixed_indexed_loop");

    emitter.label("__rt_array_filter_mixed_indexed_done");
    emit_box_result_hash(emitter);
    emitter.instruction("b __rt_array_filter_mixed_return");

    emitter.label("__rt_array_filter_mixed_hash_setup");
    emitter.instruction("ldr x9, [x1, #8]");                                  // preserve source hash capacity
    emitter.instruction("mov x0, x9");
    emitter.instruction("mov x1, #7");                                        // filtered hash may contain heterogeneous values
    emitter.instruction("bl __rt_hash_new");                                  // allocate destination hash
    emitter.instruction("str x0, [sp, #48]");                                 // save destination hash pointer
    emitter.instruction("str xzr, [sp, #56]");                                // insertion-order cursor starts at zero

    emitter.label("__rt_array_filter_mixed_hash_loop");
    emitter.instruction("ldr x0, [sp, #40]");                                 // source hash pointer
    emitter.instruction("ldr x1, [sp, #56]");                                 // current insertion-order cursor
    emitter.instruction("bl __rt_hash_iter_next");                            // fetch key and original payload tuple
    emitter.instruction("cmn x0, #1");
    emitter.instruction("b.eq __rt_array_filter_mixed_hash_done");             // finish after the insertion-order tail
    emitter.instruction("str x0, [sp, #56]");                                 // save next cursor
    emitter.instruction("str x1, [sp, #64]");                                 // save key low word
    emitter.instruction("str x2, [sp, #72]");                                 // save key high word/sentinel
    emitter.instruction("str x3, [sp, #80]");                                 // save original value low word
    emitter.instruction("str x4, [sp, #88]");                                 // save original value high word
    emitter.instruction("str x5, [sp, #96]");                                 // save original value tag
    emitter.instruction("ldr x0, [sp, #40]");                                 // raw source hash for generic read boxing
    emitter.instruction("ldr x1, [sp, #64]");                                 // current key low word
    emitter.instruction("ldr x2, [sp, #72]");                                 // current key high word
    emitter.instruction("mov x3, #0");                                        // current iterator keys are known present
    emitter.instruction("bl __rt_array_get_mixed_key");                       // return owned, reference-dereferenced callback value
    emitter.instruction("str x0, [sp, #104]");                                // preserve callback value
    emitter.instruction("str xzr, [sp, #112]");                               // default to no callback key
    emitter.instruction("ldr x9, [sp, #24]");
    emitter.instruction("cbz x9, __rt_array_filter_mixed_hash_call");          // value-only callback omits the key
    emitter.instruction("ldr x1, [sp, #64]");                                 // key payload low word
    emitter.instruction("ldr x2, [sp, #72]");                                 // key length or integer sentinel
    emitter.instruction("cmn x2, #1");                                        // classify integer versus string key
    emitter.instruction("b.ne __rt_array_filter_mixed_hash_key_string");
    emitter.instruction("mov x0, #0");                                        // runtime tag 0 = integer
    emitter.instruction("mov x2, #0");                                        // integer keys have no high word
    emitter.instruction("b __rt_array_filter_mixed_hash_key_box");
    emitter.label("__rt_array_filter_mixed_hash_key_string");
    emitter.instruction("mov x0, #1");                                        // runtime tag 1 = string
    emitter.label("__rt_array_filter_mixed_hash_key_box");
    emitter.instruction("bl __rt_mixed_from_value");                          // allocate callback-facing key cell
    emitter.instruction("str x0, [sp, #112]");                                // preserve owned key cell

    emitter.label("__rt_array_filter_mixed_hash_call");
    emit_callback_call(
        emitter,
        "__rt_array_filter_mixed_hash_callback",
        "__rt_array_filter_mixed_hash_after_call",
    );
    emitter.label("__rt_array_filter_mixed_hash_after_call");
    emit_release_callback_values(emitter);
    emitter.instruction("ldr x9, [sp, #120]");                                // reload callback result
    emitter.instruction("cbz x9, __rt_array_filter_mixed_hash_loop");          // rejected entry is omitted
    emitter.instruction("ldr x9, [sp, #96]");                                 // original payload runtime tag
    emitter.instruction("cmp x9, #1");                                        // string payload requires a retained owner
    emitter.instruction("b.eq __rt_array_filter_mixed_hash_retain");
    emitter.instruction("cmp x9, #4");                                        // indexed child pointer?
    emitter.instruction("b.lt __rt_array_filter_mixed_hash_insert");
    emitter.instruction("cmp x9, #7");                                        // array/hash/object/Mixed pointer range?
    emitter.instruction("b.le __rt_array_filter_mixed_hash_retain");
    emitter.instruction("cmp x9, #10");                                       // callable descriptor pointer?
    emitter.instruction("b.eq __rt_array_filter_mixed_hash_retain");
    emitter.instruction("cmp x9, #11");                                       // shared PHP reference cell?
    emitter.instruction("b.ne __rt_array_filter_mixed_hash_insert");
    emitter.label("__rt_array_filter_mixed_hash_retain");
    emitter.instruction("ldr x0, [sp, #80]");                                 // borrowed source child pointer
    emitter.instruction("bl __rt_incref");                                    // result hash takes an owning share
    emitter.label("__rt_array_filter_mixed_hash_insert");
    emitter.instruction("ldr x0, [sp, #48]");                                 // destination hash pointer
    emitter.instruction("ldr x1, [sp, #64]");                                 // original key low word
    emitter.instruction("ldr x2, [sp, #72]");                                 // original key high word/sentinel
    emitter.instruction("ldr x3, [sp, #80]");                                 // original value low word
    emitter.instruction("ldr x4, [sp, #88]");                                 // original value high word
    emitter.instruction("ldr x5, [sp, #96]");                                 // original value tag, including tag-11 references
    emitter.instruction("bl __rt_hash_set");                                  // persist key and transfer retained value ownership
    emitter.instruction("str x0, [sp, #48]");                                 // save possibly grown destination hash
    emitter.instruction("b __rt_array_filter_mixed_hash_loop");

    emitter.label("__rt_array_filter_mixed_hash_done");
    emit_box_result_hash(emitter);

    emitter.label("__rt_array_filter_mixed_return");
    emitter.instruction("ldp x29, x30, [sp, #176]");                          // restore caller frame state
    emitter.instruction("add sp, sp, #192");                                  // release filter frame
    emitter.instruction("ret");                                               // return boxed filtered array

    emitter.label("__rt_array_filter_mixed_invalid_mode");
    value_error::emit_throw_value_error_aarch64(
        emitter,
        "_array_filter_mode_msg",
        ARRAY_FILTER_MODE_MSG_LEN,
    );

    emitter.blank();
    emitter.comment("--- runtime: array_filter_mixed_raw ---");
    emitter.label_global("__rt_array_filter_mixed_raw");
    emitter.instruction("sub sp, sp, #64");                                    // reserve arguments, temporary box, result, and saved fp/lr
    emitter.instruction("stp x29, x30, [sp, #48]");                           // preserve caller frame state
    emitter.instruction("add x29, sp, #48");                                  // establish wrapper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                  // callback address
    emitter.instruction("str x1, [sp, #8]");                                  // borrowed raw array/hash pointer
    emitter.instruction("str x2, [sp, #16]");                                 // optional callback environment
    emitter.instruction("str x3, [sp, #24]");                                 // callback mode
    emitter.instruction("mov x0, x1");                                        // pass raw container to kind-aware boxer
    emitter.instruction("bl __rt_mixed_from_array_kind");                     // create an owned temporary source box
    emitter.instruction("str x0, [sp, #32]");                                 // preserve temporary source box
    emitter.instruction("mov x1, x0");                                        // boxed source is the main helper's second argument
    emitter.instruction("ldr x0, [sp, #0]");                                  // reload callback address
    emitter.instruction("ldr x2, [sp, #16]");                                 // reload optional environment
    emitter.instruction("ldr x3, [sp, #24]");                                 // reload callback mode
    emitter.instruction("bl __rt_array_filter_mixed");                        // filter through the boxed dynamic path
    emitter.instruction("str x0, [sp, #40]");                                 // preserve boxed filtered result
    emitter.instruction("ldr x0, [sp, #32]");                                 // temporary source box
    emitter.instruction("bl __rt_decref_mixed");                              // release temporary source box and its retained child
    emitter.instruction("ldr x0, [sp, #40]");                                 // restore boxed filtered result
    emitter.instruction("ldp x29, x30, [sp, #48]");                           // restore caller frame state
    emitter.instruction("add sp, sp, #64");                                   // release wrapper frame
    emitter.instruction("ret");                                               // return boxed filtered result
}

/// Emits the mode-sensitive callback argument setup and indirect call.
fn emit_callback_call(emitter: &mut Emitter, prefix: &str, return_label: &str) {
    let key_label = format!("{}_key", prefix);
    let both_label = format!("{}_both", prefix);
    let ready_label = format!("{}_ready", prefix);
    emitter.instruction("ldr x9, [sp, #24]");                                 // reload filter mode
    emitter.instruction("cmp x9, #2");                                        // key-only callback?
    emitter.instruction(&format!("b.eq {}", key_label));
    emitter.instruction("cmp x9, #1");                                        // value-and-key callback?
    emitter.instruction(&format!("b.eq {}", both_label));
    emitter.instruction("ldr x0, [sp, #104]");                                // value-only callback first argument
    emitter.instruction("ldr x9, [sp, #16]");                                 // optional environment
    emitter.instruction(&format!("cbz x9, {}", ready_label));
    emitter.instruction("mov x1, x9");                                        // environment follows value
    emitter.instruction(&format!("b {}", ready_label));
    emitter.label(&both_label);
    emitter.instruction("ldr x0, [sp, #104]");                                // value argument
    emitter.instruction("ldr x1, [sp, #112]");                                // key argument
    emitter.instruction("ldr x9, [sp, #16]");                                 // optional environment
    emitter.instruction(&format!("cbz x9, {}", ready_label));
    emitter.instruction("mov x2, x9");                                        // environment follows value and key
    emitter.instruction(&format!("b {}", ready_label));
    emitter.label(&key_label);
    emitter.instruction("ldr x0, [sp, #112]");                                // key-only callback first argument
    emitter.instruction("ldr x9, [sp, #16]");                                 // optional environment
    emitter.instruction(&format!("cbz x9, {}", ready_label));
    emitter.instruction("mov x1, x9");                                        // environment follows key
    emitter.label(&ready_label);
    emitter.instruction("ldr x9, [sp, #0]");                                  // reload callback address
    emitter.instruction("blr x9");                                            // invoke predicate; truthiness returns in x0
    emitter.instruction("str x0, [sp, #120]");                                // preserve callback result across temporary releases
    emitter.instruction(&format!("b {}", return_label));                      // return to the caller-specific continuation
}

/// Releases the owned callback-facing value and optional key cells.
fn emit_release_callback_values(emitter: &mut Emitter) {
    emitter.instruction("ldr x0, [sp, #104]");                                // owned callback value cell
    emitter.instruction("bl __rt_decref_mixed");                              // release value after predicate returns
    emitter.instruction("ldr x0, [sp, #112]");                                // optional owned callback key cell
    emitter.instruction("bl __rt_decref_mixed");                              // null-safe release of the key cell
}

/// Boxes the owned result hash as `Mixed` and transfers its raw ownership into the box.
fn emit_box_result_hash(emitter: &mut Emitter) {
    emitter.instruction("mov x0, #5");                                        // runtime tag 5 = associative array
    emitter.instruction("ldr x1, [sp, #48]");                                 // raw filtered hash payload
    emitter.instruction("mov x2, #0");
    emitter.instruction("bl __rt_mixed_from_value");                          // result box retains the raw hash
    emitter.instruction("str x0, [sp, #104]");                                // preserve result box across ownership transfer
    emitter.instruction("ldr x0, [sp, #48]");
    emitter.instruction("bl __rt_decref_any");                                // release the helper's original raw-hash owner
    emitter.instruction("ldr x0, [sp, #104]");                                // restore boxed result
}
