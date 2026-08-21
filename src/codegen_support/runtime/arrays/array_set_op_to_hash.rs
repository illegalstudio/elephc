//! Purpose:
//! Emits `__rt_array_diff_to_hash`, `__rt_array_intersect_to_hash` and `__rt_array_unique_to_hash`:
//! the value set operations over an indexed array, returning an owned INT-KEYED HASH that keeps
//! each surviving element's ORIGINAL index instead of renumbering the result from zero.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - php preserves the first operand's keys in all three operations. `array_diff` re-adds each
//!   survivor with `zend_hash_index_add_new(…, idx, …)` (ext/standard/array.c), `array_intersect`
//!   copies the first array and `zend_hash_index_del()`s the rest, and `array_unique`'s default
//!   `SORT_STRING` path re-adds survivors the same way — none of them ever calls
//!   `zend_hash_next_index_insert`. A dense indexed array cannot hold `{0:"a", 2:"c"}`, so the
//!   result has to be a hash, exactly as `array_slice($a,$o,$l,true)` already does.
//! - ONE emitter serves all three operations because the loop is the same: walk the source, decide
//!   each element against a comparison window, and insert the survivors at their source index. The
//!   operations differ only in WHICH window (`$b` for diff/intersect, the source's own `0..i`
//!   prefix for unique) and in the keep-on-match sense (intersect keeps a match, the other two
//!   drop it). Comparing unique's candidate against the source PREFIX rather than against the
//!   survivors kept so far is equivalent — a dropped element always equals an earlier kept one —
//!   and it keeps php's FIRST occurrence, which php's sort path enforces explicitly.
//! - Element extraction mirrors `__rt_array_to_hash` slot for slot and is driven by the value_type
//!   read from the source header at RUNTIME, so one helper covers the scalar, string and
//!   heap-backed element layouts the three-way `scalar`/`refcounted`/`str` helper split used to
//!   need: string elements (16-byte slots) are persisted into independent heap copies, heap-backed
//!   elements are retained, and scalar elements are copied by value, so the result owns its
//!   payloads.
//! - Element EQUALITY is unchanged from the dense helpers it replaces: strings compare through
//!   `__rt_str_eq`, and every other layout compares its low word, which is pointer identity for
//!   heap-backed elements. php compares string casts instead; that divergence is pre-existing and
//!   deliberately not widened here.
//! - Every live value is reloaded from the frame around each call. `__rt_str_eq`, `__rt_incref`,
//!   `__rt_str_persist` and `__rt_hash_set` are all free to clobber caller-saved registers, and an
//!   index or a candidate kept in one across a call is the exact x86-only defect this repo has
//!   already shipped twice.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Which value set operation a `__rt_array_*_to_hash` helper performs.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SetOpToHash {
    /// `array_diff($a, $b)`: keep the elements of `$a` that `$b` does not contain.
    Diff,
    /// `array_intersect($a, $b)`: keep the elements of `$a` that `$b` also contains.
    Intersect,
    /// `array_unique($a)`: keep the first occurrence of each value.
    Unique,
}

impl SetOpToHash {
    /// Reports whether a match keeps the candidate (intersect) or drops it (diff, unique).
    fn keep_on_match(self) -> bool {
        self == SetOpToHash::Intersect
    }

    /// Reports whether the comparison window is the source's own `0..i` prefix.
    fn compares_against_prefix(self) -> bool {
        self == SetOpToHash::Unique
    }
}

/// array_set_op_to_hash: a value set operation returning an owned hash with the SOURCE keys.
/// Input:  x0 = source indexed array `$a`, x1 = comparison indexed array `$b` (unused by unique)
/// Output: x0 = new owned hash whose integer keys are the surviving elements' source indices
///
/// Backs `array_diff`, `array_intersect` and `array_unique`, which all preserve the first
/// operand's keys in php.
pub fn emit_array_set_op_to_hash(emitter: &mut Emitter, label: &str, op: SetOpToHash) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_set_op_to_hash_linux_x86_64(emitter, label, op);
        return;
    }

    let cap_ok = format!("{}_cap_ok", label);
    let outer = format!("{}_outer", label);
    let cand_nostr = format!("{}_cand_nostr", label);
    let cand_done = format!("{}_cand_done", label);
    let inner = format!("{}_inner", label);
    let inner_str = format!("{}_inner_str", label);
    let inner_next = format!("{}_inner_next", label);
    let matched = format!("{}_matched", label);
    let unmatched = format!("{}_unmatched", label);
    let keep = format!("{}_keep", label);
    let keep_str = format!("{}_keep_str", label);
    let keep_set = format!("{}_keep_set", label);
    let next = format!("{}_next", label);
    let done = format!("{}_done", label);

    emitter.blank();
    emitter.comment(&format!("--- runtime: {} ---", label));
    emitter.label_global(label);

    // Frame: [0]=a [8]=hash [16]=i [24]=value_type [32]=stride_a [40]=cand_lo [48]=cand_hi
    //        [56]=b [64]=j [72]=bound [80]=stride_b, linkage at [96].
    emitter.instruction("sub sp, sp, #112");                                    // allocate the set-operation stack frame
    emitter.instruction("stp x29, x30, [sp, #96]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #96");                                    // set up the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the source array pointer
    if op.compares_against_prefix() {
        emitter.instruction("str x0, [sp, #56]");                               // unique compares the source against itself
    } else {
        emitter.instruction("str x1, [sp, #56]");                               // save the comparison array pointer
    }
    emitter.instruction("ldr x10, [x0, #-8]");                                  // load the uniform heap-kind header word
    emitter.instruction("lsr x10, x10, #8");                                    // shift the packed value_type into the low bits
    emitter.instruction("and x10, x10, #0x7f");                                 // isolate the indexed-array value_type (also the Mixed tag)
    emitter.instruction("str x10, [sp, #24]");                                  // save the value_type / runtime tag
    emitter.instruction("ldr x11, [x0, #16]");                                  // load the source element size (stride) from the header
    emitter.instruction("str x11, [sp, #32]");                                  // save the source element stride
    emitter.instruction("ldr x12, [sp, #56]");                                  // reload the comparison array pointer
    emitter.instruction("ldr x11, [x12, #16]");                                 // load the comparison element size (stride) from its own header
    emitter.instruction("str x11, [sp, #80]");                                  // save the comparison element stride
    emitter.instruction("ldr x9, [x0]");                                        // load the source length as the capacity hint
    emitter.instruction("cmp x9, #8");                                          // is the source below the minimum hash capacity?
    emitter.instruction(&format!("b.ge {}", cap_ok));                           // use the source length as the capacity hint
    emitter.instruction("mov x9, #8");                                          // clamp the capacity hint to a small minimum
    emitter.label(&cap_ok);
    emitter.instruction("mov x0, x9");                                          // capacity hint for the new hash
    emitter.instruction("ldr x1, [sp, #24]");                                   // value_type for the new hash header
    emitter.instruction("bl __rt_hash_new");                                    // allocate the result hash, x0 = result
    emitter.instruction("str x0, [sp, #8]");                                    // save the result hash pointer
    emitter.instruction("str xzr, [sp, #16]");                                  // i = 0

    emitter.label(&outer);
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the source cursor i
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the source array pointer
    emitter.instruction("ldr x11, [x10]");                                      // load the source length
    emitter.instruction("cmp x9, x11");                                         // has every source element been decided?
    emitter.instruction(&format!("b.ge {}", done));                             // the whole source has been walked
    emitter.instruction("add x10, x10, #24");                                   // skip the 24-byte indexed-array header
    emitter.instruction("ldr x12, [sp, #32]");                                  // reload the source element stride
    emitter.instruction("mul x13, x9, x12");                                    // byte offset of element[i]
    emitter.instruction("add x10, x10, x13");                                   // x10 = address of element[i]
    emitter.instruction("ldr x14, [x10]");                                      // load the candidate low word
    emitter.instruction("str x14, [sp, #40]");                                  // save the candidate low word
    emitter.instruction("ldr x15, [sp, #24]");                                  // reload the value_type
    emitter.instruction("cmp x15, #1");                                         // is the candidate a string?
    emitter.instruction(&format!("b.ne {}", cand_nostr));                       // only strings carry a high word
    emitter.instruction("ldr x14, [x10, #8]");                                  // load the candidate length from the 16-byte slot
    emitter.instruction("str x14, [sp, #48]");                                  // save the candidate length
    emitter.instruction(&format!("b {}", cand_done));                           // the candidate is fully loaded
    emitter.label(&cand_nostr);
    emitter.instruction("str xzr, [sp, #48]");                                  // non-string candidates have no high word
    emitter.label(&cand_done);
    if op.compares_against_prefix() {
        emitter.instruction("ldr x9, [sp, #16]");                               // reload the source cursor i
        emitter.instruction("str x9, [sp, #72]");                               // unique scans the source prefix 0..i
    } else {
        emitter.instruction("ldr x10, [sp, #56]");                              // reload the comparison array pointer
        emitter.instruction("ldr x11, [x10]");                                  // load the comparison length
        emitter.instruction("str x11, [sp, #72]");                              // the whole comparison array is scanned
    }
    emitter.instruction("str xzr, [sp, #64]");                                  // j = 0

    emitter.label(&inner);
    emitter.instruction("ldr x9, [sp, #64]");                                   // reload the comparison cursor j
    emitter.instruction("ldr x11, [sp, #72]");                                  // reload the comparison bound
    emitter.instruction("cmp x9, x11");                                         // has the comparison window been exhausted?
    emitter.instruction(&format!("b.ge {}", unmatched));                        // nothing in the window matched the candidate
    emitter.instruction("ldr x10, [sp, #56]");                                  // reload the comparison array pointer
    emitter.instruction("add x10, x10, #24");                                   // skip the 24-byte indexed-array header
    emitter.instruction("ldr x12, [sp, #80]");                                  // reload the comparison element stride
    emitter.instruction("mul x13, x9, x12");                                    // byte offset of comparison element[j]
    emitter.instruction("add x10, x10, x13");                                   // x10 = address of comparison element[j]
    emitter.instruction("ldr x15, [sp, #24]");                                  // reload the value_type
    emitter.instruction("cmp x15, #1");                                         // is the element a string?
    emitter.instruction(&format!("b.eq {}", inner_str));                        // strings compare by bytes
    emitter.instruction("ldr x14, [x10]");                                      // load the comparison low word
    emitter.instruction("ldr x13, [sp, #40]");                                  // reload the candidate low word
    emitter.instruction("cmp x13, x14");                                        // scalar and heap elements compare by their low word
    emitter.instruction(&format!("b.eq {}", matched));                          // the candidate is present in the window
    emitter.instruction(&format!("b {}", inner_next));                          // keep scanning the window
    emitter.label(&inner_str);
    emitter.instruction("ldr x1, [sp, #40]");                                   // the candidate string pointer
    emitter.instruction("ldr x2, [sp, #48]");                                   // the candidate string length
    emitter.instruction("ldr x3, [x10]");                                       // the comparison string pointer
    emitter.instruction("ldr x4, [x10, #8]");                                   // the comparison string length
    emitter.instruction("bl __rt_str_eq");                                      // x0 = 1 when the bytes agree
    emitter.instruction(&format!("cbnz x0, {}", matched));                      // the candidate is present in the window
    emitter.label(&inner_next);
    emitter.instruction("ldr x9, [sp, #64]");                                   // reload the comparison cursor
    emitter.instruction("add x9, x9, #1");                                      // j += 1
    emitter.instruction("str x9, [sp, #64]");                                   // save the advanced comparison cursor
    emitter.instruction(&format!("b {}", inner));                               // continue scanning the window

    emitter.label(&matched);
    if op.keep_on_match() {
        emitter.instruction(&format!("b {}", keep));                            // intersect keeps a matched element
    } else {
        emitter.instruction(&format!("b {}", next));                            // diff and unique drop a matched element
    }
    emitter.label(&unmatched);
    if op.keep_on_match() {
        emitter.instruction(&format!("b {}", next));                            // intersect drops an unmatched element
    } else {
        emitter.instruction(&format!("b {}", keep));                            // diff and unique keep an unmatched element
    }

    emitter.label(&keep);
    emitter.instruction("ldr x15, [sp, #24]");                                  // reload the value_type
    emitter.instruction("cmp x15, #1");                                         // is the survivor a string?
    emitter.instruction(&format!("b.eq {}", keep_str));                         // strings need persistence
    emitter.instruction("cmp x15, #4");                                         // is the survivor below the heap-backed tag range?
    emitter.instruction(&format!("b.lt {}", keep_set));                         // scalar elements need no retain
    emitter.instruction("cmp x15, #7");                                         // is the survivor above the heap-backed tag range?
    emitter.instruction(&format!("b.gt {}", keep_set));                         // non-heap tags need no retain
    emitter.instruction("ldr x0, [sp, #40]");                                   // load the heap-backed survivor pointer
    emitter.instruction("bl __rt_incref");                                      // retain the heap-backed survivor for the result hash
    emitter.instruction(&format!("b {}", keep_set));                            // continue to insertion
    emitter.label(&keep_str);
    emitter.instruction("ldr x1, [sp, #40]");                                   // the survivor string pointer
    emitter.instruction("ldr x2, [sp, #48]");                                   // the survivor string length
    emitter.instruction("bl __rt_str_persist");                                 // copy the string into an independent heap block, x1 = new pointer
    emitter.instruction("str x1, [sp, #40]");                                   // save the persisted string pointer
    emitter.instruction("str x2, [sp, #48]");                                   // save the string length
    emitter.label(&keep_set);
    emitter.instruction("ldr x0, [sp, #8]");                                    // result hash pointer
    emitter.instruction("ldr x1, [sp, #16]");                                   // integer key = the preserved source index i
    emitter.instruction("mov x2, #-1");                                         // key_hi = -1 marks an integer key
    emitter.instruction("ldr x3, [sp, #40]");                                   // value low word
    emitter.instruction("ldr x4, [sp, #48]");                                   // value high word
    emitter.instruction("ldr x5, [sp, #24]");                                   // value runtime tag (= value_type)
    emitter.instruction("bl __rt_hash_set");                                    // insert the survivor at its preserved integer key
    emitter.instruction("str x0, [sp, #8]");                                    // update the result pointer after possible reallocation

    emitter.label(&next);
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the source cursor
    emitter.instruction("add x9, x9, #1");                                      // i += 1
    emitter.instruction("str x9, [sp, #16]");                                   // save the advanced source cursor
    emitter.instruction(&format!("b {}", outer));                               // continue walking the source

    emitter.label(&done);
    emitter.instruction("ldr x0, [sp, #8]");                                    // x0 = result hash pointer
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // deallocate the stack frame
    emitter.instruction("ret");                                                 // return the result hash in x0
}

/// x86_64 Linux implementation of [`emit_array_set_op_to_hash`].
/// Input:  rdi = source indexed array `$a`, rsi = comparison indexed array `$b` (unused by unique)
/// Output: rax = new owned hash whose integer keys are the surviving elements' source indices
///
/// The runtime helpers do NOT share one x86 convention: `__rt_str_persist` reads its string in
/// `rax`/`rdx` and answers there, `__rt_incref` reads its pointer in `rax` (NOT `rdi` — its own
/// body opens with `test rax, rax`), and only `__rt_str_eq`, `__rt_hash_new` and `__rt_hash_set`
/// take the SysV integer argument registers. Each call site below follows the convention of the
/// helper it calls.
fn emit_array_set_op_to_hash_linux_x86_64(emitter: &mut Emitter, label: &str, op: SetOpToHash) {
    let cap_ok = format!("{}_cap_ok", label);
    let outer = format!("{}_outer", label);
    let cand_nostr = format!("{}_cand_nostr", label);
    let cand_done = format!("{}_cand_done", label);
    let inner = format!("{}_inner", label);
    let inner_str = format!("{}_inner_str", label);
    let inner_next = format!("{}_inner_next", label);
    let matched = format!("{}_matched", label);
    let unmatched = format!("{}_unmatched", label);
    let keep = format!("{}_keep", label);
    let keep_str = format!("{}_keep_str", label);
    let keep_set = format!("{}_keep_set", label);
    let next = format!("{}_next", label);
    let done = format!("{}_done", label);

    emitter.blank();
    emitter.comment(&format!("--- runtime: {} ---", label));
    emitter.label_global(label);

    // Frame: [rbp-8]=a [rbp-16]=hash [rbp-24]=i [rbp-32]=value_type [rbp-40]=stride_a
    //        [rbp-48]=cand_lo [rbp-56]=cand_hi [rbp-64]=b [rbp-72]=j [rbp-80]=bound
    //        [rbp-88]=stride_b.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 96");                                         // reserve local slots for the set-operation loop state
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the source array pointer
    if op.compares_against_prefix() {
        emitter.instruction("mov QWORD PTR [rbp - 64], rdi");                   // unique compares the source against itself
    } else {
        emitter.instruction("mov QWORD PTR [rbp - 64], rsi");                   // save the comparison array pointer
    }
    emitter.instruction("mov r10, QWORD PTR [rdi - 8]");                        // load the uniform heap-kind header word
    emitter.instruction("shr r10, 8");                                          // shift the packed value_type into the low bits
    emitter.instruction("and r10, 127");                                        // isolate the indexed-array value_type (also the Mixed tag)
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save the value_type / runtime tag
    emitter.instruction("mov r11, QWORD PTR [rdi + 16]");                       // load the source element size (stride) from the header
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // save the source element stride
    emitter.instruction("mov r11, QWORD PTR [rbp - 64]");                       // reload the comparison array pointer
    emitter.instruction("mov r11, QWORD PTR [r11 + 16]");                       // load the comparison element size (stride) from its own header
    emitter.instruction("mov QWORD PTR [rbp - 88], r11");                       // save the comparison element stride
    emitter.instruction("mov rdi, QWORD PTR [rdi]");                            // load the source length as the capacity hint
    emitter.instruction("cmp rdi, 8");                                          // is the source below the minimum hash capacity?
    emitter.instruction(&format!("jge {}", cap_ok));                            // use the source length as the capacity hint
    emitter.instruction("mov rdi, 8");                                          // clamp the capacity hint to a small minimum
    emitter.label(&cap_ok);
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // value_type for the new hash header
    emitter.instruction("call __rt_hash_new");                                  // allocate the result hash, rax = result
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the result hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // i = 0

    emitter.label(&outer);
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the source cursor i
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source array pointer
    emitter.instruction("cmp r9, QWORD PTR [r10]");                             // has every source element been decided?
    emitter.instruction(&format!("jge {}", done));                              // the whole source has been walked
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the source element stride
    emitter.instruction("imul r11, r9");                                        // byte offset of element[i]
    emitter.instruction("lea r10, [r10 + r11 + 24]");                           // r10 = address of element[i], past the 24-byte header
    emitter.instruction("mov rcx, QWORD PTR [r10]");                            // load the candidate low word
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // save the candidate low word
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the value_type
    emitter.instruction("cmp r8, 1");                                           // is the candidate a string?
    emitter.instruction(&format!("jne {}", cand_nostr));                        // only strings carry a high word
    emitter.instruction("mov rcx, QWORD PTR [r10 + 8]");                        // load the candidate length from the 16-byte slot
    emitter.instruction("mov QWORD PTR [rbp - 56], rcx");                       // save the candidate length
    emitter.instruction(&format!("jmp {}", cand_done));                         // the candidate is fully loaded
    emitter.label(&cand_nostr);
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // non-string candidates have no high word
    emitter.label(&cand_done);
    if op.compares_against_prefix() {
        emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                    // reload the source cursor i
        emitter.instruction("mov QWORD PTR [rbp - 80], r9");                    // unique scans the source prefix 0..i
    } else {
        emitter.instruction("mov r10, QWORD PTR [rbp - 64]");                   // reload the comparison array pointer
        emitter.instruction("mov r10, QWORD PTR [r10]");                        // load the comparison length
        emitter.instruction("mov QWORD PTR [rbp - 80], r10");                   // the whole comparison array is scanned
    }
    emitter.instruction("mov QWORD PTR [rbp - 72], 0");                         // j = 0

    emitter.label(&inner);
    emitter.instruction("mov r9, QWORD PTR [rbp - 72]");                        // reload the comparison cursor j
    emitter.instruction("cmp r9, QWORD PTR [rbp - 80]");                        // has the comparison window been exhausted?
    emitter.instruction(&format!("jge {}", unmatched));                         // nothing in the window matched the candidate
    emitter.instruction("mov r10, QWORD PTR [rbp - 64]");                       // reload the comparison array pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 88]");                       // reload the comparison element stride
    emitter.instruction("imul r11, r9");                                        // byte offset of comparison element[j]
    emitter.instruction("lea r10, [r10 + r11 + 24]");                           // r10 = address of comparison element[j]
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the value_type
    emitter.instruction("cmp r8, 1");                                           // is the element a string?
    emitter.instruction(&format!("je {}", inner_str));                          // strings compare by bytes
    emitter.instruction("mov rcx, QWORD PTR [r10]");                            // load the comparison low word
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 48]");                       // scalar and heap elements compare by their low word
    emitter.instruction(&format!("je {}", matched));                            // the candidate is present in the window
    emitter.instruction(&format!("jmp {}", inner_next));                        // keep scanning the window
    emitter.label(&inner_str);
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // the candidate string pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // the candidate string length
    emitter.instruction("mov rdx, QWORD PTR [r10]");                            // the comparison string pointer
    emitter.instruction("mov rcx, QWORD PTR [r10 + 8]");                        // the comparison string length
    emitter.instruction("call __rt_str_eq");                                    // rax = 1 when the bytes agree
    emitter.instruction("test rax, rax");                                       // did the bytes agree?
    emitter.instruction(&format!("jnz {}", matched));                           // the candidate is present in the window
    emitter.label(&inner_next);
    emitter.instruction("mov r9, QWORD PTR [rbp - 72]");                        // reload the comparison cursor
    emitter.instruction("add r9, 1");                                           // j += 1
    emitter.instruction("mov QWORD PTR [rbp - 72], r9");                        // save the advanced comparison cursor
    emitter.instruction(&format!("jmp {}", inner));                             // continue scanning the window

    emitter.label(&matched);
    if op.keep_on_match() {
        emitter.instruction(&format!("jmp {}", keep));                          // intersect keeps a matched element
    } else {
        emitter.instruction(&format!("jmp {}", next));                          // diff and unique drop a matched element
    }
    emitter.label(&unmatched);
    if op.keep_on_match() {
        emitter.instruction(&format!("jmp {}", next));                          // intersect drops an unmatched element
    } else {
        emitter.instruction(&format!("jmp {}", keep));                          // diff and unique keep an unmatched element
    }

    emitter.label(&keep);
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the value_type
    emitter.instruction("cmp r8, 1");                                           // is the survivor a string?
    emitter.instruction(&format!("je {}", keep_str));                           // strings need persistence
    emitter.instruction("cmp r8, 4");                                           // is the survivor below the heap-backed tag range?
    emitter.instruction(&format!("jl {}", keep_set));                           // scalar elements need no retain
    emitter.instruction("cmp r8, 7");                                           // is the survivor above the heap-backed tag range?
    emitter.instruction(&format!("jg {}", keep_set));                           // non-heap tags need no retain
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // load the heap-backed survivor pointer where __rt_incref reads it on x86_64
    emitter.instruction("call __rt_incref");                                    // retain the heap-backed survivor for the result hash
    emitter.instruction(&format!("jmp {}", keep_set));                          // continue to insertion
    emitter.label(&keep_str);
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // the survivor string pointer, where __rt_str_persist reads it
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // the survivor string length, where __rt_str_persist reads it
    emitter.instruction("call __rt_str_persist");                               // copy the string into an independent heap block, rax = new pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the persisted string pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], rdx");                       // save the string length
    emitter.label(&keep_set);
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // result hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // integer key = the preserved source index i
    emitter.instruction("mov rdx, -1");                                         // key_hi = -1 marks an integer key
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // value low word
    emitter.instruction("mov r8, QWORD PTR [rbp - 56]");                        // value high word
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // value runtime tag (= value_type)
    emitter.instruction("call __rt_hash_set");                                  // insert the survivor at its preserved integer key
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // update the result pointer after possible reallocation

    emitter.label(&next);
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the source cursor
    emitter.instruction("add r9, 1");                                           // i += 1
    emitter.instruction("mov QWORD PTR [rbp - 24], r9");                        // save the advanced source cursor
    emitter.instruction(&format!("jmp {}", outer));                             // continue walking the source

    emitter.label(&done);
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // rax = result hash pointer
    emitter.instruction("add rsp, 96");                                         // release the local slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the result hash in rax
}
