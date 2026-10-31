//! Purpose:
//! Emits allocation-free ASCII screens and ASCII-only case converters for explicit-UTF-8
//! `mb_strtoupper()`, `mb_strtolower()`, `mb_convert_case()`, `mb_ucfirst()`, and
//! `mb_lcfirst()` calls.
//!
//! Called from:
//! - `crate::codegen_support::runtime::strings::mbstring::emit_mbstring()`.
//!
//! Key details:
//! - Every wrapper returns a null result pointer when the subject contains a byte outside the
//!   ASCII range, so the caller can run the full Unicode bridge.
//! - ASCII subjects convert identically under UTF-8, so one owned allocation and a bytewise
//!   transform produce the same PHP string without the origin-tracking path used by
//!   `strtoupper()`.
//! - Title case follows PHP's cased/case-ignorable rules: only `'`, `.`, `:`, `^` and `` ` ``
//!   keep a word open; every other non-letter starts a new word.
//! - AArch64 input is `x1`/`x2` = pointer/length, output is the same pair; x86_64 input is
//!   `rax`/`rdx` = pointer/length, output is the same pair. A zero pointer means fallback.
//! - `__rt_mb_is_ascii` reads that same input pair and reports the predicate through `x0`
//!   (AArch64) or `rcx` (x86_64), leaving the pointer and length intact for the caller.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits the ASCII case helpers for the active target.
pub(super) fn emit(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
    } else {
        emit_aarch64(emitter);
    }
}

/// Emits the AArch64 ASCII screens and converters.
fn emit_aarch64(emitter: &mut Emitter) {
    for (name, uppercase) in [
        ("__rt_mb_strtoupper_utf8", true),
        ("__rt_mb_strtolower_utf8", false),
    ] {
        emit_case_aarch64(emitter, name, uppercase);
        fallback_aarch64(emitter, name);
    }
    for (name, uppercase) in [
        ("__rt_mb_ucfirst_utf8", true),
        ("__rt_mb_lcfirst_utf8", false),
    ] {
        emit_scan_aarch64(emitter, name, &format!("{name}_ascii"));
        emit_first_aarch64(emitter, name, uppercase);
        fallback_aarch64(emitter, name);
    }
    emit_is_ascii_aarch64(emitter);
    emit_title_aarch64(emitter);
}

/// Emits the AArch64 ASCII predicate used by width and codepoint fast paths.
fn emit_is_ascii_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: __rt_mb_is_ascii ---");
    emitter.label_global("__rt_mb_is_ascii");
    emitter.instruction("mov x9, x1");                                          // scan through a cursor without changing the input pair
    emitter.instruction("mov x10, x2");                                        // bound every access by the PHP byte length
    emitter.label("__rt_mb_is_ascii_scan");
    emitter.instruction("cmp x10, #16");                                       // is one full sixteen-byte block available?
    emitter.instruction("b.lo __rt_mb_is_ascii_tail");                         // the short tail is checked byte by byte
    emitter.instruction("ldr q0, [x9]");                                        // load sixteen subject bytes
    emitter.instruction("cmlt v1.16b, v0.16b, #0");                             // mark bytes whose high bit is set
    emitter.instruction("umaxv b1, v1.16b");                                   // did any byte leave the ASCII range?
    emitter.instruction("umov w11, v1.b[0]");                                  // move the block verdict into an integer register
    emitter.instruction("cbnz w11, __rt_mb_is_ascii_no");                      // a non-ASCII byte rejects the predicate
    emitter.instruction("add x9, x9, #16");                                    // consume sixteen ASCII bytes
    emitter.instruction("sub x10, x10, #16");                                  // account for the consumed block
    emitter.instruction("b __rt_mb_is_ascii_scan");                            // inspect the next block
    emitter.label("__rt_mb_is_ascii_tail");
    emitter.instruction("cbz x10, __rt_mb_is_ascii_yes");                      // an exhausted subject is entirely ASCII
    emitter.instruction("ldrb w11, [x9]");                                     // load the next subject byte
    emitter.instruction("cmp w11, #0x80");                                     // is this byte outside ASCII?
    emitter.instruction("b.hs __rt_mb_is_ascii_no");                           // reject the predicate
    emitter.instruction("add x9, x9, #1");                                     // consume one ASCII byte
    emitter.instruction("sub x10, x10, #1");                                   // account for the consumed byte
    emitter.instruction("b __rt_mb_is_ascii_tail");                            // inspect the remaining tail
    emitter.label("__rt_mb_is_ascii_yes");
    emitter.instruction("mov x0, #1");                                         // report an entirely ASCII subject
    emitter.instruction("ret");                                               // return the predicate
    emitter.label("__rt_mb_is_ascii_no");
    emitter.instruction("mov x0, xzr");                                        // report a non-ASCII byte
    emitter.instruction("ret");                                               // return the predicate
}

/// Emits the AArch64 ASCII acceptance scan shared by every wrapper.
fn emit_scan_aarch64(emitter: &mut Emitter, name: &str, ascii: &str) {
    let scan = format!("{name}_scan");
    let tail = format!("{name}_tail");
    let fallback = format!("{name}_fallback");
    emitter.blank();
    emitter.comment(&format!("--- runtime: {name} ---"));
    emitter.label_global(name);
    emitter.instruction("mov x9, x1");                                          // scan through a cursor while preserving the result pair
    emitter.instruction("mov x10, x2");                                        // bound every access by the PHP byte length
    emitter.label(&scan);
    emitter.instruction("cmp x10, #16");                                       // is one full sixteen-byte block available?
    emitter.instruction(&format!("b.lo {tail}"));                              // the short tail is checked byte by byte
    emitter.instruction("ldr q0, [x9]");                                        // load sixteen subject bytes
    emitter.instruction("cmlt v1.16b, v0.16b, #0");                             // mark bytes whose high bit is set
    emitter.instruction("umaxv b1, v1.16b");                                   // did any byte leave the ASCII range?
    emitter.instruction("umov w11, v1.b[0]");                                  // move the block verdict into an integer register
    emitter.instruction(&format!("cbnz w11, {fallback}"));                     // non-ASCII input needs the Unicode bridge
    emitter.instruction("add x9, x9, #16");                                    // consume sixteen ASCII bytes
    emitter.instruction("sub x10, x10, #16");                                  // account for the consumed block
    emitter.instruction(&format!("b {scan}"));                                // inspect the next block
    emitter.label(&tail);
    emitter.instruction(&format!("cbz x10, {ascii}"));                        // an exhausted subject is entirely ASCII
    emitter.instruction("ldrb w11, [x9]");                                     // load the next subject byte
    emitter.instruction("cmp w11, #0x80");                                     // is this byte outside ASCII?
    emitter.instruction(&format!("b.hs {fallback}"));                         // fall back to the Unicode converter
    emitter.instruction("add x9, x9, #1");                                     // consume one ASCII byte
    emitter.instruction("sub x10, x10, #1");                                   // account for the consumed byte
    emitter.instruction(&format!("b {tail}"));                                // inspect the remaining tail
}

/// Emits the AArch64 ASCII upper/lower allocation and conversion body.
fn emit_case_aarch64(emitter: &mut Emitter, name: &str, uppercase: bool) {
    let loop_label = format!("{name}_loop");
    let tail = format!("{name}_convert_tail");
    let store = format!("{name}_store");
    let discard = format!("{name}_discard");
    let done = format!("{name}_done");
    let (low, operation) = if uppercase { (b'a', "sub") } else { (b'A', "add") };
    emitter.blank();
    emitter.comment(&format!("--- runtime: {name} ---"));
    emitter.label_global(name);
    emitter.instruction("sub sp, sp, #48");                                    // reserve the source pair, destination, and linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                            // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #32");                                   // establish a stable frame base
    emitter.instruction("stp x1, x2, [sp, #0]");                               // save the source pointer and logical length
    emitter.instruction("mov x0, x2");                                         // the result has exactly the source length
    emitter.instruction("bl __rt_str_new_owned");                             // allocate the exact owned heap result up front
    emitter.instruction("str x0, [sp, #16]");                                  // remember the result start for the epilogue
    emitter.instruction("ldr x1, [sp, #0]");                                   // reload the source pointer
    emitter.instruction("ldr x2, [sp, #8]");                                   // reload the source length
    emitter.instruction("mov x9, x0");                                         // destination cursor
    emitter.instruction("mov x10, x2");                                        // remaining source byte count
    emitter.instruction("movi v2.16b, #0x20");                                  // ASCII case offset
    emitter.instruction(&format!("movi v3.16b, #0x{:02x}", low));               // first byte of the conversion range
    emitter.instruction(&format!("movi v4.16b, #0x{:02x}", low + 26));          // one past the final byte of the range
    emitter.instruction("cmp x10, #16");                                       // can the conversion move a full vector?
    emitter.instruction(&format!("b.lt {tail}"));                             // short strings use the byte tail
    emitter.label(&loop_label);
    emitter.instruction("ldr q0, [x1], #16");                                   // load the next sixteen source bytes
    emitter.instruction("cmlt v1.16b, v0.16b, #0");                             // mark bytes whose high bit is set
    emitter.instruction("umaxv b1, v1.16b");                                   // did any byte leave the ASCII range?
    emitter.instruction("umov w11, v1.b[0]");                                  // move the block verdict into an integer register
    emitter.instruction(&format!("cbnz w11, {discard}"));                      // abandon the fast path for non-ASCII input
    emitter.instruction(&format!("{operation} v5.16b, v0.16b, v2.16b"));        // compute the case-shifted candidate bytes
    emitter.instruction("cmhs v6.16b, v0.16b, v3.16b");                         // byte >= first letter in the range
    emitter.instruction("cmhi v7.16b, v4.16b, v0.16b");                         // byte < one past the final letter
    emitter.instruction("and v6.16b, v6.16b, v7.16b");                          // keep only bytes inside the ASCII range
    emitter.instruction("bit v0.16b, v5.16b, v6.16b");                          // select the shifted byte where the range matched
    emitter.instruction("str q0, [x9], #16");                                   // store the converted vector
    emitter.instruction("sub x10, x10, #16");                                  // account for the converted bytes
    emitter.instruction("cmp x10, #16");                                       // is another full vector available?
    emitter.instruction(&format!("b.ge {loop_label}"));                       // continue converting full vectors
    emitter.label(&tail);
    emitter.instruction(&format!("cbz x10, {done}"));                         // stop when every source byte is converted
    emitter.instruction("ldrb w11, [x1], #1");                                  // load one trailing source byte
    emitter.instruction("cmp w11, #0x80");                                     // is this byte outside ASCII?
    emitter.instruction(&format!("b.hs {discard}"));                           // abandon the fast path for non-ASCII input
    emitter.instruction(&format!("cmp w11, #{low}"));                          // is the byte below the conversion range?
    emitter.instruction(&format!("b.lo {store}"));                            // preserve bytes below the range
    emitter.instruction(&format!("cmp w11, #{}", low + 25));                   // is the byte above the conversion range?
    emitter.instruction(&format!("b.hi {store}"));                            // preserve bytes above the range
    emitter.instruction(&format!("{operation} w11, w11, #32"));                // apply the ASCII case offset
    emitter.label(&store);
    emitter.instruction("strb w11, [x9], #1");                                  // store the converted or preserved byte
    emitter.instruction("sub x10, x10, #1");                                    // account for the converted byte
    emitter.instruction(&format!("b {tail}"));                                // continue converting trailing bytes
    emitter.label(&done);
    emitter.instruction("ldr x1, [sp, #16]");                                   // return the owned result pointer without a second copy
    emitter.instruction("ldr x2, [sp, #8]");                                    // return the unchanged logical length
    emitter.instruction("ldp x29, x30, [sp, #32]");                            // restore the caller frame and return address
    emitter.instruction("add sp, sp, #48");                                    // release the conversion frame
    emitter.instruction("ret");                                               // return the directly written owned string
    emitter.label(&discard);
    emitter.instruction("ldr x0, [sp, #16]");                                  // reload the partially written result before discarding it
    emitter.instruction("bl __rt_heap_free_safe");                            // release the abandoned fast-path allocation
    emitter.instruction("ldp x29, x30, [sp, #32]");                            // restore the caller frame and return address
    emitter.instruction("add sp, sp, #48");                                    // release the conversion frame
    emitter.instruction(&format!("b {name}_fallback"));                       // let the caller run the Unicode bridge
}

/// Emits the AArch64 ASCII first-character allocation and copy body.
fn emit_first_aarch64(emitter: &mut Emitter, name: &str, uppercase: bool) {
    let ascii = format!("{name}_ascii");
    let copy = format!("{name}_copy");
    let copy_tail = format!("{name}_copy_tail");
    let adjust = format!("{name}_adjust");
    let done = format!("{name}_done");
    let (low, high, operation) = if uppercase { (b'a', b'z', "sub") } else { (b'A', b'Z', "add") };
    emitter.label(&ascii);
    emitter.instruction("sub sp, sp, #48");                                    // reserve the source pair, destination, and linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                            // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #32");                                   // establish a stable frame base
    emitter.instruction("stp x1, x2, [sp, #0]");                               // save the source pointer and logical length
    emitter.instruction("mov x0, x2");                                         // the result has exactly the source length
    emitter.instruction("bl __rt_str_new_owned");                             // allocate the exact owned heap result up front
    emitter.instruction("str x0, [sp, #16]");                                  // remember the result start for the epilogue
    emitter.instruction("ldr x1, [sp, #0]");                                   // reload the source pointer
    emitter.instruction("ldr x2, [sp, #8]");                                   // reload the source length
    emitter.instruction("mov x9, x0");                                         // destination cursor
    emitter.instruction("mov x10, x2");                                        // remaining source byte count
    emitter.instruction(&format!("cbz x10, {done}"));                         // an empty source needs no first-byte adjustment
    emitter.instruction("ldrb w11, [x1], #1");                                  // load and consume the first source byte
    emitter.instruction(&format!("cmp w11, #{low}"));                          // is the first byte below the case range?
    emitter.instruction(&format!("b.lo {adjust}"));                           // leave bytes below the range unchanged
    emitter.instruction(&format!("cmp w11, #{high}"));                         // is the first byte above the case range?
    emitter.instruction(&format!("b.hi {adjust}"));                           // leave bytes above the range unchanged
    emitter.instruction(&format!("{operation} w11, w11, #32"));                // apply the ASCII case offset
    emitter.label(&adjust);
    emitter.instruction("strb w11, [x9], #1");                                  // store the adjusted first byte
    emitter.instruction("sub x10, x10, #1");                                    // account for the consumed first byte
    emitter.label(&copy);
    emitter.instruction("cmp x10, #16");                                       // can the copy move a full vector?
    emitter.instruction(&format!("b.lt {copy_tail}"));                        // short remainders use the byte loop
    emitter.instruction("ldr q0, [x1], #16");                                   // load sixteen trailing source bytes
    emitter.instruction("str q0, [x9], #16");                                   // copy them unchanged
    emitter.instruction("sub x10, x10, #16");                                  // account for the copied block
    emitter.instruction(&format!("b {copy}"));                                // continue copying full vectors
    emitter.label(&copy_tail);
    emitter.instruction(&format!("cbz x10, {done}"));                         // stop when every source byte is copied
    emitter.instruction("ldrb w11, [x1], #1");                                  // load one trailing source byte
    emitter.instruction("strb w11, [x9], #1");                                  // copy it unchanged
    emitter.instruction("sub x10, x10, #1");                                    // account for the copied byte
    emitter.instruction(&format!("b {copy_tail}"));                           // continue with the next trailing byte
    emitter.label(&done);
    emitter.instruction("ldr x1, [sp, #16]");                                   // return the owned result pointer without a second copy
    emitter.instruction("ldr x2, [sp, #8]");                                    // return the unchanged logical length
    emitter.instruction("ldp x29, x30, [sp, #32]");                            // restore the caller frame and return address
    emitter.instruction("add sp, sp, #48");                                    // release the copy frame
    emitter.instruction("ret");                                               // return the owned copied string
}

/// Emits the AArch64 fallback sentinel for one wrapper.
fn fallback_aarch64(emitter: &mut Emitter, name: &str) {
    emitter.label(&format!("{name}_fallback"));
    emitter.instruction("mov x1, xzr");                                        // signal that the caller must use the bridge
    emitter.instruction("ret");                                               // return the fallback sentinel
}

/// Emits the AArch64 ASCII title converter.
fn emit_title_aarch64(emitter: &mut Emitter) {
    let name = "__rt_mb_convert_case_title_utf8";
    let ascii = format!("{name}_ascii");
    emit_scan_aarch64(emitter, name, &ascii);
    emitter.label(&ascii);
    emitter.instruction("sub sp, sp, #32");                                    // preserve the source pair across allocation
    emitter.instruction("stp x29, x30, [sp, #16]");                            // retain the caller frame and return address
    emitter.instruction("add x29, sp, #16");                                   // establish a stable frame base
    emitter.instruction("stp x1, x2, [sp, #0]");                               // save the source pointer and logical length
    emitter.instruction("mov x0, x2");                                         // the title result has exactly the source length
    emitter.instruction("bl __rt_str_new_owned");                             // allocate one owned result up front
    emitter.instruction("ldr x1, [sp, #0]");                                   // reload the source cursor
    emitter.instruction("ldr x2, [sp, #8]");                                   // reload the source length
    emitter.instruction("mov x9, x0");                                         // destination cursor
    emitter.instruction("mov x10, x2");                                        // remaining source bytes
    emitter.instruction("mov w12, #0");                                        // begin a new word: the next cased byte is uppercased
    emitter.label(&format!("{name}_loop"));
    emitter.instruction(&format!("cbz x10, {name}_done"));                    // stop after the final source byte
    emitter.instruction("ldrb w11, [x1], #1");                                 // load and consume one source byte
    emitter.instruction(&format!("cmp w11, #0x41"));                           // is this below uppercase A?
    emitter.instruction(&format!("b.lo {name}_not_upper"));                   // check lowercase and separators
    emitter.instruction(&format!("cmp w11, #0x5A"));                           // is this above uppercase Z?
    emitter.instruction(&format!("b.hi {name}_not_upper"));                   // check lowercase and separators
    emitter.instruction(&format!("cbnz w12, {name}_lower"));                  // an uppercase letter inside a word is lowercased
    emitter.instruction("mov w12, #1");                                        // the following letters belong to this word
    emitter.instruction(&format!("b {name}_store"));                          // store the unchanged word-start uppercase
    emitter.label(&format!("{name}_not_upper"));
    emitter.instruction(&format!("cmp w11, #0x61"));                           // is this below lowercase a?
    emitter.instruction(&format!("b.lo {name}_not_lower"));                   // check the case-ignorable separators
    emitter.instruction(&format!("cmp w11, #0x7A"));                           // is this above lowercase z?
    emitter.instruction(&format!("b.hi {name}_not_lower"));                   // check the case-ignorable separators
    emitter.instruction(&format!("cbnz w12, {name}_store"));                  // a lowercase letter inside a word stays lowercase
    emitter.instruction("sub w11, w11, #32");                                  // uppercase the first letter of the word
    emitter.instruction("mov w12, #1");                                        // the following letters belong to this word
    emitter.instruction(&format!("b {name}_store"));                          // publish the converted byte
    emitter.label(&format!("{name}_lower"));
    emitter.instruction("add w11, w11, #32");                                  // lowercase an uppercase letter inside a word
    emitter.instruction(&format!("b {name}_store"));                          // publish the converted byte
    emitter.label(&format!("{name}_not_lower"));
    for (byte, meaning) in [
        (0x27, "apostrophe"),
        (0x2E, "full stop"),
        (0x3A, "colon"),
        (0x5E, "caret"),
        (0x60, "backtick"),
    ] {
        emitter.instruction(&format!("cmp w11, #{byte:#04x}"));                // does this separator keep the current word open?
        emitter.instruction(&format!("b.eq {name}_store"));                    // preserve the byte without changing the word state
        let _ = meaning;
    }
    emitter.instruction("mov w12, #0");                                        // every other non-letter starts a new word
    emitter.label(&format!("{name}_store"));
    emitter.instruction("strb w11, [x9], #1");                                 // store the converted or preserved byte
    emitter.instruction("sub x10, x10, #1");                                   // account for the consumed byte
    emitter.instruction(&format!("b {name}_loop"));                           // continue with the next source byte
    emitter.label(&format!("{name}_done"));
    emitter.instruction("sub x1, x9, x2");                                     // recover the result start from the destination cursor
    emitter.instruction("ldr x2, [sp, #8]");                                   // return the unchanged logical length
    emitter.instruction("ldp x29, x30, [sp, #16]");                            // restore the caller frame and return address
    emitter.instruction("add sp, sp, #32");                                   // release the allocation frame
    emitter.instruction("ret");                                              // return the owned title result
    fallback_aarch64(emitter, name);
}

/// Emits the x86_64 ASCII screens and converters.
fn emit_x86_64(emitter: &mut Emitter) {
    for (name, uppercase) in [
        ("__rt_mb_strtoupper_utf8", true),
        ("__rt_mb_strtolower_utf8", false),
    ] {
        emit_case_x86_64(emitter, name, uppercase);
        fallback_x86_64(emitter, name);
    }
    for (name, uppercase) in [
        ("__rt_mb_ucfirst_utf8", true),
        ("__rt_mb_lcfirst_utf8", false),
    ] {
        emit_scan_x86_64(emitter, name, &format!("{name}_ascii"));
        emit_first_x86_64(emitter, name, uppercase);
        fallback_x86_64(emitter, name);
    }
    emit_is_ascii_x86_64(emitter);
    emit_title_x86_64(emitter);
}

/// Emits the x86_64 ASCII predicate used by width and codepoint fast paths.
fn emit_is_ascii_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: __rt_mb_is_ascii ---");
    emitter.label_global("__rt_mb_is_ascii");
    emitter.instruction("mov r8, rax");                                         // scan through a cursor without changing the input pair
    emitter.instruction("mov r9, rdx");                                        // bound every access by the PHP byte length
    emitter.label("__rt_mb_is_ascii_scan_x86");
    emitter.instruction("cmp r9, 16");                                         // is one full sixteen-byte block available?
    emitter.instruction("jb __rt_mb_is_ascii_tail_x86");                       // the short tail is checked byte by byte
    emitter.instruction("movdqu xmm0, XMMWORD PTR [r8]");                      // load sixteen subject bytes
    emitter.instruction("pmovmskb ecx, xmm0");                                 // collect the high bit of every byte
    emitter.instruction("test ecx, ecx");                                      // did any byte leave the ASCII range?
    emitter.instruction("jnz __rt_mb_is_ascii_no_x86");                        // a non-ASCII byte rejects the predicate
    emitter.instruction("add r8, 16");                                         // consume sixteen ASCII bytes
    emitter.instruction("sub r9, 16");                                         // account for the consumed block
    emitter.instruction("jmp __rt_mb_is_ascii_scan_x86");                      // inspect the next block
    emitter.label("__rt_mb_is_ascii_tail_x86");
    emitter.instruction("test r9, r9");                                        // is the subject exhausted?
    emitter.instruction("jz __rt_mb_is_ascii_yes_x86");                        // an exhausted subject is entirely ASCII
    emitter.instruction("movzx ecx, BYTE PTR [r8]");                           // load the next subject byte
    emitter.instruction("cmp ecx, 0x80");                                      // is this byte outside ASCII?
    emitter.instruction("jae __rt_mb_is_ascii_no_x86");                        // reject the predicate
    emitter.instruction("inc r8");                                             // consume one ASCII byte
    emitter.instruction("dec r9");                                             // account for the consumed byte
    emitter.instruction("jmp __rt_mb_is_ascii_tail_x86");                      // inspect the remaining tail
    emitter.label("__rt_mb_is_ascii_yes_x86");
    emitter.instruction("mov ecx, 1");                                         // report an entirely ASCII subject
    emitter.instruction("ret");                                               // return the predicate
    emitter.label("__rt_mb_is_ascii_no_x86");
    emitter.instruction("xor ecx, ecx");                                       // report a non-ASCII byte
    emitter.instruction("ret");                                               // return the predicate
}

/// Emits the x86_64 ASCII acceptance scan shared by every wrapper.
fn emit_scan_x86_64(emitter: &mut Emitter, name: &str, ascii: &str) {
    let scan = format!("{name}_scan_x86");
    let tail = format!("{name}_tail_x86");
    let fallback = format!("{name}_fallback_x86");
    emitter.blank();
    emitter.comment(&format!("--- runtime: {name} ---"));
    emitter.label_global(name);
    emitter.instruction("mov r8, rax");                                         // scan through a cursor while preserving the result pair
    emitter.instruction("mov r9, rdx");                                       // bound every access by the PHP byte length
    emitter.label(&scan);
    emitter.instruction("cmp r9, 16");                                        // is one full sixteen-byte block available?
    emitter.instruction(&format!("jb {tail}"));                               // the short tail is checked byte by byte
    emitter.instruction("movdqu xmm0, XMMWORD PTR [r8]");                     // load sixteen subject bytes
    emitter.instruction("pmovmskb ecx, xmm0");                                // collect the high bit of every byte
    emitter.instruction("test ecx, ecx");                                     // did any byte leave the ASCII range?
    emitter.instruction(&format!("jnz {fallback}"));                          // non-ASCII input needs the Unicode bridge
    emitter.instruction("add r8, 16");                                        // consume sixteen ASCII bytes
    emitter.instruction("sub r9, 16");                                        // account for the consumed block
    emitter.instruction(&format!("jmp {scan}"));                              // inspect the next block
    emitter.label(&tail);
    emitter.instruction("test r9, r9");                                       // is the subject exhausted?
    emitter.instruction(&format!("jz {ascii}"));                              // an exhausted subject is entirely ASCII
    emitter.instruction("movzx ecx, BYTE PTR [r8]");                          // load the next subject byte
    emitter.instruction("cmp ecx, 0x80");                                     // is this byte outside ASCII?
    emitter.instruction(&format!("jae {fallback}"));                          // fall back to the Unicode converter
    emitter.instruction("inc r8");                                            // consume one ASCII byte
    emitter.instruction("dec r9");                                            // account for the consumed byte
    emitter.instruction(&format!("jmp {tail}"));                              // inspect the remaining tail
}

/// Emits the x86_64 ASCII upper/lower allocation and conversion body.
fn emit_case_x86_64(emitter: &mut Emitter, name: &str, uppercase: bool) {
    let loop_label = format!("{name}_loop_x86");
    let tail = format!("{name}_convert_tail_x86");
    let store = format!("{name}_store_x86");
    let discard = format!("{name}_discard_x86");
    let done = format!("{name}_done_x86");
    let (low, operation, scalar_operation) = if uppercase {
        (b'a', "psubb", "sub")
    } else {
        (b'A', "paddb", "add")
    };
    emitter.blank();
    emitter.comment(&format!("--- runtime: {name} ---"));
    emitter.label_global(name);
    emitter.instruction("push rbp");                                          // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                      // establish a stable frame base
    emitter.instruction("sub rsp, 48");                                       // reserve the source pair, destination, and call padding
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                      // save the source pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                     // save the source length
    emitter.instruction("mov rax, rdx");                                      // the result has exactly the source length
    emitter.instruction("call __rt_str_new_owned");                           // allocate the exact owned heap result up front
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                     // remember the result start for the epilogue
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                       // source cursor
    emitter.instruction("mov r9, rax");                                       // destination cursor
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                     // remaining source byte count
    for (register, value) in [("xmm2", low - 1), ("xmm3", low + 26), ("xmm6", 32)] {
        emitter.instruction(&format!("mov eax, {value}"));                    // materialize one broadcast constant
        emitter.instruction(&format!("movd {register}, eax"));                // seed the constant's vector lane
        emitter.instruction(&format!("punpcklbw {register}, {register}"));    // duplicate the byte through its word
        emitter.instruction(&format!("punpcklwd {register}, {register}"));    // duplicate the word through its dword
        emitter.instruction(&format!("pshufd {register}, {register}, 0"));    // broadcast the dword through the vector
    }
    emitter.instruction("cmp r10, 16");                                       // can the conversion move a full vector?
    emitter.instruction(&format!("jl {tail}"));                               // short strings use the byte tail
    emitter.label(&loop_label);
    emitter.instruction("movdqu xmm0, XMMWORD PTR [r8]");                     // load the next sixteen source bytes
    emitter.instruction("pmovmskb ecx, xmm0");                                // collect the high bit of every byte
    emitter.instruction("test ecx, ecx");                                     // did any byte leave the ASCII range?
    emitter.instruction(&format!("jnz {discard}"));                           // abandon the fast path for non-ASCII input
    emitter.instruction("movdqa xmm5, xmm0");                                 // copy the source vector for the case-shifted candidate
    emitter.instruction(&format!("{operation} xmm5, xmm6"));                  // compute the case-shifted candidate bytes
    emitter.instruction("movdqa xmm4, xmm0");                                 // copy the source vector for the range mask
    emitter.instruction("pcmpgtb xmm4, xmm2");                                // byte > first range byte - 1
    emitter.instruction("movdqa xmm7, xmm3");                                 // copy the one-past-final range bound
    emitter.instruction("pcmpgtb xmm7, xmm0");                                // one-past-final > byte
    emitter.instruction("pand xmm4, xmm7");                                   // keep only bytes inside the ASCII range
    emitter.instruction("pand xmm5, xmm4");                                   // keep the shifted bytes inside the range
    emitter.instruction("pandn xmm4, xmm0");                                  // keep the original bytes outside the range
    emitter.instruction("por xmm5, xmm4");                                    // merge preserved and converted bytes
    emitter.instruction("movdqu XMMWORD PTR [r9], xmm5");                     // store the converted vector
    emitter.instruction("add r8, 16");                                        // advance the source cursor
    emitter.instruction("add r9, 16");                                        // advance the destination cursor
    emitter.instruction("sub r10, 16");                                       // account for the converted bytes
    emitter.instruction("cmp r10, 16");                                       // is another full vector available?
    emitter.instruction(&format!("jge {loop_label}"));                       // continue converting full vectors
    emitter.label(&tail);
    emitter.instruction("test r10, r10");                                     // stop when every source byte is converted
    emitter.instruction(&format!("jz {done}"));                              // publish the completed result
    emitter.instruction("movzx eax, BYTE PTR [r8]");                          // load one trailing source byte
    emitter.instruction("cmp eax, 0x80");                                     // is this byte outside ASCII?
    emitter.instruction(&format!("jae {discard}"));                           // abandon the fast path for non-ASCII input
    emitter.instruction(&format!("cmp eax, {low}"));                          // is the byte below the conversion range?
    emitter.instruction(&format!("jb {store}"));                             // preserve bytes below the range
    emitter.instruction(&format!("cmp eax, {}", low + 25));                   // is the byte above the conversion range?
    emitter.instruction(&format!("ja {store}"));                             // preserve bytes above the range
    emitter.instruction(&format!("{scalar_operation} eax, 32"));              // apply the ASCII case offset
    emitter.label(&store);
    emitter.instruction("mov BYTE PTR [r9], al");                             // store the converted or preserved byte
    emitter.instruction("inc r8");                                            // advance the source cursor
    emitter.instruction("inc r9");                                            // advance the destination cursor
    emitter.instruction("dec r10");                                           // account for the converted byte
    emitter.instruction(&format!("jmp {tail}"));                             // continue converting trailing bytes
    emitter.label(&done);
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                     // return the owned result pointer without a second copy
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                     // return the unchanged logical length
    emitter.instruction("leave");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                              // return the directly written owned string
    emitter.label(&discard);
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                     // reload the partially written result before discarding it
    emitter.instruction("call __rt_heap_free_safe");                         // release the abandoned fast-path allocation
    emitter.instruction("leave");                                             // restore the caller frame pointer
    emitter.instruction(&format!("jmp {name}_fallback_x86"));                // let the caller run the Unicode bridge
}

/// Emits the x86_64 ASCII first-character allocation and copy body.
fn emit_first_x86_64(emitter: &mut Emitter, name: &str, uppercase: bool) {
    let ascii = format!("{name}_ascii");
    let copy = format!("{name}_copy_x86");
    let copy_tail = format!("{name}_copy_tail_x86");
    let adjust = format!("{name}_adjust_x86");
    let done = format!("{name}_done_x86");
    let (low, high, operation) = if uppercase { (b'a', b'z', "sub") } else { (b'A', b'Z', "add") };
    emitter.label(&ascii);
    emitter.instruction("push rbp");                                          // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                      // establish a stable frame base
    emitter.instruction("sub rsp, 48");                                       // reserve the source pair, destination, and call padding
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                      // save the source pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                     // save the source length
    emitter.instruction("mov rax, rdx");                                      // the result has exactly the source length
    emitter.instruction("call __rt_str_new_owned");                           // allocate the exact owned heap result up front
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                     // remember the result start for the epilogue
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                       // source cursor
    emitter.instruction("mov r9, rax");                                       // destination cursor
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                     // remaining source byte count
    emitter.instruction(&format!("test r10, r10"));                          // an empty source needs no first-byte adjustment
    emitter.instruction(&format!("jz {done}"));                             // finish without reading past the source
    emitter.instruction("movzx eax, BYTE PTR [r8]");                          // load the first source byte
    emitter.instruction("inc r8");                                            // consume the first source byte
    emitter.instruction(&format!("cmp eax, {low}"));                          // is the first byte below the case range?
    emitter.instruction(&format!("jb {adjust}"));                           // leave bytes below the range unchanged
    emitter.instruction(&format!("cmp eax, {high}"));                         // is the first byte above the case range?
    emitter.instruction(&format!("ja {adjust}"));                           // leave bytes above the range unchanged
    emitter.instruction(&format!("{operation} eax, 32"));                     // apply the ASCII case offset
    emitter.label(&adjust);
    emitter.instruction("mov BYTE PTR [r9], al");                             // store the adjusted first byte
    emitter.instruction("inc r9");                                            // advance the destination cursor
    emitter.instruction("dec r10");                                           // account for the consumed first byte
    emitter.label(&copy);
    emitter.instruction("cmp r10, 16");                                       // can the copy move a full vector?
    emitter.instruction(&format!("jl {copy_tail}"));                         // short remainders use the byte loop
    emitter.instruction("movdqu xmm0, XMMWORD PTR [r8]");                     // load sixteen trailing source bytes
    emitter.instruction("movdqu XMMWORD PTR [r9], xmm0");                     // copy them unchanged
    emitter.instruction("add r8, 16");                                        // advance the source cursor
    emitter.instruction("add r9, 16");                                        // advance the destination cursor
    emitter.instruction("sub r10, 16");                                       // account for the copied block
    emitter.instruction(&format!("jmp {copy}"));                             // continue copying full vectors
    emitter.label(&copy_tail);
    emitter.instruction(&format!("test r10, r10"));                         // stop when every source byte is copied
    emitter.instruction(&format!("jz {done}"));                             // publish the completed result
    emitter.instruction("movzx eax, BYTE PTR [r8]");                          // load one trailing source byte
    emitter.instruction("mov BYTE PTR [r9], al");                            // copy it unchanged
    emitter.instruction("inc r8");                                            // advance the source cursor
    emitter.instruction("inc r9");                                            // advance the destination cursor
    emitter.instruction("dec r10");                                           // account for the copied byte
    emitter.instruction(&format!("jmp {copy_tail}"));                        // continue with the next trailing byte
    emitter.label(&done);
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                     // return the owned result pointer without a second copy
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                     // return the unchanged logical length
    emitter.instruction("leave");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                              // return the owned copied string
}

/// Emits the x86_64 fallback sentinel for one wrapper.
fn fallback_x86_64(emitter: &mut Emitter, name: &str) {
    emitter.label(&format!("{name}_fallback_x86"));
    emitter.instruction("xor eax, eax");                                      // signal that the caller must use the bridge
    emitter.instruction("ret");                                              // return the fallback sentinel
}

/// Emits the x86_64 ASCII title converter.
fn emit_title_x86_64(emitter: &mut Emitter) {
    let name = "__rt_mb_convert_case_title_utf8";
    let ascii = format!("{name}_ascii_x86");
    emit_scan_x86_64(emitter, name, &ascii);
    emitter.label(&ascii);
    emitter.instruction("push rbp");                                          // preserve the caller frame across allocation
    emitter.instruction("mov rbp, rsp");                                      // establish a stable frame base
    emitter.instruction("sub rsp, 32");                                       // reserve the source pair and alignment
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                      // save the source pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                     // save the source length
    emitter.instruction("mov rax, rdx");                                      // the title result has exactly the source length
    emitter.instruction("call __rt_str_new_owned");                           // allocate one owned result up front
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                       // source cursor
    emitter.instruction("mov r9, rax");                                       // destination cursor
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                     // remaining source bytes
    emitter.instruction("xor r11d, r11d");                                     // begin a new word: the next cased byte is uppercased
    emitter.label(&format!("{name}_loop_x86"));
    emitter.instruction("test r10, r10");                                     // stop after the final source byte
    emitter.instruction(&format!("jz {name}_done_x86"));                      // publish the owned result
    emitter.instruction("movzx ecx, BYTE PTR [r8]");                          // load one source byte
    emitter.instruction("inc r8");                                            // consume the source byte
    emitter.instruction("cmp ecx, 0x41");                                      // is this below uppercase A?
    emitter.instruction(&format!("jb {name}_not_upper_x86"));                // check lowercase and separators
    emitter.instruction("cmp ecx, 0x5A");                                      // is this above uppercase Z?
    emitter.instruction(&format!("ja {name}_not_upper_x86"));                // check lowercase and separators
    emitter.instruction("test r11d, r11d");                                    // is this letter inside an open word?
    emitter.instruction(&format!("jnz {name}_lower_x86"));                   // an uppercase letter inside a word is lowercased
    emitter.instruction("mov r11d, 1");                                        // the following letters belong to this word
    emitter.instruction(&format!("jmp {name}_store_x86"));                   // store the unchanged word-start uppercase
    emitter.label(&format!("{name}_not_upper_x86"));
    emitter.instruction("cmp ecx, 0x61");                                      // is this below lowercase a?
    emitter.instruction(&format!("jb {name}_not_lower_x86"));                // check the case-ignorable separators
    emitter.instruction("cmp ecx, 0x7A");                                      // is this above lowercase z?
    emitter.instruction(&format!("ja {name}_not_lower_x86"));                // check the case-ignorable separators
    emitter.instruction("test r11d, r11d");                                    // is this letter inside an open word?
    emitter.instruction(&format!("jnz {name}_store_x86"));                   // a lowercase letter inside a word stays lowercase
    emitter.instruction("sub ecx, 32");                                       // uppercase the first letter of the word
    emitter.instruction("mov r11d, 1");                                        // the following letters belong to this word
    emitter.instruction(&format!("jmp {name}_store_x86"));                   // publish the converted byte
    emitter.label(&format!("{name}_lower_x86"));
    emitter.instruction("add ecx, 32");                                        // lowercase an uppercase letter inside a word
    emitter.instruction(&format!("jmp {name}_store_x86"));                   // publish the converted byte
    emitter.label(&format!("{name}_not_lower_x86"));
    for byte in [0x27, 0x2E, 0x3A, 0x5E, 0x60] {
        emitter.instruction(&format!("cmp ecx, {byte:#04x}"));               // does this separator keep the current word open?
        emitter.instruction(&format!("je {name}_store_x86"));                // preserve the byte without changing the word state
    }
    emitter.instruction("xor r11d, r11d");                                     // every other non-letter starts a new word
    emitter.label(&format!("{name}_store_x86"));
    emitter.instruction("mov BYTE PTR [r9], cl");                             // store the converted or preserved byte
    emitter.instruction("inc r9");                                            // advance the destination cursor
    emitter.instruction("dec r10");                                           // account for the consumed byte
    emitter.instruction(&format!("jmp {name}_loop_x86"));                    // continue with the next source byte
    emitter.label(&format!("{name}_done_x86"));
    emitter.instruction("mov rax, r9");                                        // result pointer is the destination cursor minus its length
    emitter.instruction("sub rax, QWORD PTR [rbp - 16]");                      // recover the allocation start
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                      // return the unchanged logical length
    emitter.instruction("leave");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                              // return the owned title result
    fallback_x86_64(emitter, name);
}
