//! Purpose:
//! Emits the `__rt_hash_map` runtime helper assembly: `array_map()` over an ASSOCIATIVE
//! source (a hash), producing a hash that carries the SOURCE keys and the callback results.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - The indexed-array counterparts live in `array_map.rs` / `array_map_str.rs` /
//!   `array_map_mixed.rs`. Those build a LIST; php-src's single-array `array_map()` preserves
//!   string keys, so an associative source needs a hash destination instead — that is this
//!   module.
//! - One helper covers every callback result shape. The caller passes a RESULT KIND selector
//!   (`HashMapResultKind`) that says where the callback left its result and who owns it, plus
//!   the destination `value_type` tag. That keeps the four indexed helpers' worth of variation
//!   in one place and out of the lowering.
//! - The callback ARGUMENT ABI is chosen from the per-entry RUNTIME value tag the iterator
//!   returns (tag 1 = string ⇒ ptr/len pair, everything else ⇒ one scalar register), exactly
//!   the way the indexed helpers branch on `elem_size`.
//! - OWNERSHIP: the source hash is only ever READ; this helper never retains or releases a
//!   source key or value. The destination owns everything it stores — `__rt_hash_set` persists
//!   the inserted string KEY itself, `Persist` results are copied through `__rt_str_persist`
//!   before insertion, and `Owned`/`Scalar` results are values the callback wrapper already
//!   transferred. No refcount traffic means no double-free surface.
//! - The destination is allocated with `__rt_hash_new`, so it can never alias the source. That
//!   is what makes `RuntimeFnId::ArrayMap`'s `Fresh` result ownership correct for this path too.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Where `__rt_hash_map` finds the callback result, and who owns it.
///
/// The discriminants are the ABI: the lowering passes them to `__rt_hash_map` as an integer
/// argument and the helper branches on them after every callback call.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HashMapResultKind {
    /// One pointer-sized result in the integer return register.
    ///
    /// Covers `int`/`bool` callback results and the boxed-Mixed pointer a descriptor callback
    /// wrapper returns. A boxed Mixed cell is already OWNED by the wrapper, so storing the
    /// pointer transfers ownership to the destination slot; a scalar owns nothing.
    Scalar = 0,
    /// A borrowed string pair the helper must copy before storing.
    ///
    /// This is the `__rt_array_map_str` contract: the callback returns a pointer into storage
    /// it does not hand over, so the destination gets its own copy via `__rt_str_persist`.
    Persist = 1,
    /// An already-owned string pair the helper stores verbatim.
    ///
    /// This is the `__rt_array_map_str_owned` contract: a descriptor callback wrapper detached
    /// the string from its boxed Mixed owner, so persisting again would leak the detached copy.
    Owned = 2,
}

/// Emits the `__rt_hash_map` runtime helper for associative (hash) sources.
///
/// Walks the source hash in insertion order, invokes the callback once per entry with that
/// entry's VALUE, and inserts `source key => callback result` into a freshly allocated
/// destination hash. Keys are copied across untouched, which is what makes the single-array
/// form of php-src `array_map()` key-preserving.
///
/// # ABI
/// - Input: `x0` / `rdi` = callback function pointer, `x1` / `rsi` = source hash pointer,
///   `x2` / `rdx` = callback environment pointer (`0` when the callback captures nothing),
///   `x3` / `rcx` = `HashMapResultKind` discriminant, `x4` / `r8` = destination `value_type`
///   tag.
/// - Output: `x0` / `rax` = destination hash pointer.
///
/// Dispatches to the target-specific implementation; x86_64 uses the System V register
/// convention, every other target uses the AArch64 path.
pub fn emit_hash_map(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_map_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_map ---");
    emitter.label_global("__rt_hash_map");

    // -- set up the frame and preserve the callee-saved source/destination/callback registers --
    // Stack layout:
    //   [sp, #0]  = insertion-order iterator cursor
    //   [sp, #8]  = source key pointer (or inline integer key payload)
    //   [sp, #16] = source key length (-1 marks an inline integer key)
    //   [sp, #24] = HashMapResultKind selector
    //   [sp, #32] = destination value_type tag
    //   [sp, #48] = saved x21/x22
    //   [sp, #64] = saved x19/x20
    //   [sp, #80] = saved x29/x30
    emitter.instruction("sub sp, sp, #96");                                     // allocate the hash-map frame
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // set up the hash-map frame pointer
    emitter.instruction("stp x19, x20, [sp, #64]");                             // save callee-saved x19/x20 for the source and destination tables
    emitter.instruction("stp x21, x22, [sp, #48]");                             // save callee-saved x21/x22 for the callback and its environment
    emitter.instruction("mov x21, x0");                                         // x21 = callback address, live across every loop iteration
    emitter.instruction("mov x19, x1");                                         // x19 = source hash pointer, live across every helper call
    emitter.instruction("mov x22, x2");                                         // x22 = callback environment pointer (0 when unused)
    emitter.instruction("str x3, [sp, #24]");                                   // save the result-kind selector for the post-callback dispatch
    emitter.instruction("str x4, [sp, #32]");                                   // save the destination value_type tag for every insertion

    // -- allocate the destination table with headroom for every mapped entry --
    emitter.instruction("ldr x0, [x19]");                                       // x0 = source entry count
    emitter.instruction("lsl x0, x0, #1");                                      // double the entry count to give the destination insertion headroom
    emitter.instruction("mov x9, #16");                                         // x9 = minimum destination bucket count
    emitter.instruction("cmp x0, x9");                                          // compare the derived capacity against the runtime minimum
    emitter.instruction("csel x0, x9, x0, lt");                                 // clamp very small sources up to the minimum bucket count
    emitter.instruction("ldr x1, [sp, #32]");                                   // pass the destination value_type tag to the table allocator
    emitter.instruction("bl __rt_hash_new");                                    // allocate the destination hash
    emitter.instruction("mov x20, x0");                                         // x20 = destination hash pointer, updated after every insertion
    emitter.instruction("str xzr, [sp, #0]");                                   // iterator cursor = 0 (start from header.head)

    // -- walk the source hash in insertion order --
    emitter.label("__rt_hash_map_loop");
    emitter.instruction("mov x0, x19");                                         // x0 = source hash pointer
    emitter.instruction("ldr x1, [sp, #0]");                                    // x1 = current insertion-order cursor
    emitter.instruction("bl __rt_hash_iter_next_value");                        // fetch the next source entry
    emitter.instruction("cmn x0, #1");                                          // did the iterator signal end-of-walk?
    emitter.instruction("b.eq __rt_hash_map_done");                             // yes - the destination hash is complete
    emitter.instruction("str x0, [sp, #0]");                                    // save the next insertion-order cursor
    emitter.instruction("str x1, [sp, #8]");                                    // save the source key pointer before the callback call
    emitter.instruction("str x2, [sp, #16]");                                   // save the source key length before the callback call

    // -- php passes the VALUE only; the argument ABI follows the entry's runtime value tag --
    emitter.instruction("cmp x5, #1");                                          // runtime tag 1 = string, which uses the two-register string ABI
    emitter.instruction("b.eq __rt_hash_map_call_str");                         // string values are passed as a pointer/length pair
    emitter.instruction("mov x0, x3");                                          // x0 = scalar source value (int, bool, or boxed Mixed pointer)
    emitter.instruction("mov x1, x22");                                         // pass the capture environment after the scalar argument
    emitter.instruction("b __rt_hash_map_call");                                // invoke the callback through the shared call site

    emitter.label("__rt_hash_map_call_str");
    emitter.instruction("mov x0, x3");                                          // x0 = source string pointer
    emitter.instruction("mov x1, x4");                                          // x1 = source string length
    emitter.instruction("mov x2, x22");                                         // pass the capture environment after the string pointer/length pair

    emitter.label("__rt_hash_map_call");
    emitter.instruction("blr x21");                                             // invoke the user callback on this entry's value

    // -- read the callback result from wherever this result kind leaves it --
    emitter.instruction("ldr x9, [sp, #24]");                                   // x9 = HashMapResultKind selector
    emitter.instruction("cmp x9, #1");                                          // is the result a borrowed string pair needing a copy?
    emitter.instruction("b.eq __rt_hash_map_result_persist");                   // yes - persist it before the destination takes ownership
    emitter.instruction("cmp x9, #2");                                          // is the result an already-owned string pair?
    emitter.instruction("b.eq __rt_hash_map_result_owned");                     // yes - store the pair verbatim
    emitter.instruction("mov x3, x0");                                          // scalar and boxed-Mixed results arrive in the integer return register
    emitter.instruction("mov x4, xzr");                                         // pointer-sized results carry no high word
    emitter.instruction("b __rt_hash_map_insert");                              // insert the scalar-shaped mapped entry

    emitter.label("__rt_hash_map_result_persist");
    emitter.instruction("bl __rt_str_persist");                                 // copy the borrowed callback string so the destination owns its own bytes
    emitter.instruction("mov x3, x1");                                          // x3 = owned mapped value pointer
    emitter.instruction("mov x4, x2");                                          // x4 = owned mapped value length
    emitter.instruction("b __rt_hash_map_insert");                              // insert the owned string-valued entry

    emitter.label("__rt_hash_map_result_owned");
    emitter.instruction("mov x3, x1");                                          // x3 = already-owned mapped value pointer
    emitter.instruction("mov x4, x2");                                          // x4 = already-owned mapped value length

    // -- the destination keeps the SOURCE key; hash_set persists string keys itself --
    emitter.label("__rt_hash_map_insert");
    emitter.instruction("ldr x5, [sp, #32]");                                   // x5 = destination value_type tag
    emitter.instruction("mov x0, x20");                                         // x0 = destination hash pointer
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = source key_lo
    emitter.instruction("ldr x2, [sp, #16]");                                   // x2 = source key_hi (-1 marks an integer key)
    emitter.instruction("bl __rt_hash_set");                                    // insert the mapped pair under the preserved source key
    emitter.instruction("mov x20, x0");                                         // keep the destination pointer current after possible growth
    emitter.instruction("b __rt_hash_map_loop");                                // continue with the next source entry

    emitter.label("__rt_hash_map_done");
    emitter.instruction("mov x0, x20");                                         // return the destination hash pointer
    emitter.instruction("ldp x21, x22, [sp, #48]");                             // restore callee-saved x21/x22
    emitter.instruction("ldp x19, x20, [sp, #64]");                             // restore callee-saved x19/x20
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // deallocate the hash-map frame
    emitter.instruction("ret");                                                 // return with x0 = destination hash pointer
}

/// Emits the x86_64 System V variant of `__rt_hash_map`.
///
/// Mirrors the AArch64 logic exactly; only the register convention differs.
///
/// # ABI notes
/// - `__rt_hash_iter_next_value` returns the entry key pointer in `rdi`, which doubles as argument
///   zero, so every returned field is consumed or spilled before the callback call.
/// - The callback follows the same convention the indexed `__rt_array_map*` helpers use on
///   x86_64: a string result comes back in `rax`/`rdx`, which is exactly the pair
///   `__rt_str_persist` reads and returns (its own docblock still claims `rdi`/`rdx`, but its
///   body reads `rax` — `__rt_hash_set` calls it the same way).
/// - The frame is `push rbp` + `sub rsp, 96`: entry leaves `rsp ≡ 8 (mod 16)`, the push makes
///   it `≡ 0`, and 96 is a multiple of 16, so every nested `call` stays System V aligned.
fn emit_hash_map_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_map ---");
    emitter.label_global("__rt_hash_map");

    // Frame layout:
    //   [rbp - 8]  = source hash pointer
    //   [rbp - 16] = destination hash pointer
    //   [rbp - 24] = insertion-order iterator cursor
    //   [rbp - 32] = source key pointer (or inline integer key payload)
    //   [rbp - 40] = source key length (-1 marks an inline integer key)
    //   [rbp - 48] = callback function pointer
    //   [rbp - 56] = callback environment pointer
    //   [rbp - 64] = HashMapResultKind selector
    //   [rbp - 72] = destination value_type tag
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving hash-map spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the mapping bookkeeping
    emitter.instruction("sub rsp, 96");                                         // reserve aligned spill slots so nested helper calls stay 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // preserve the source hash pointer across every helper call
    emitter.instruction("mov QWORD PTR [rbp - 48], rdi");                       // preserve the callback address across every helper call
    emitter.instruction("mov QWORD PTR [rbp - 56], rdx");                       // preserve the callback environment pointer across every helper call
    emitter.instruction("mov QWORD PTR [rbp - 64], rcx");                       // preserve the result-kind selector for the post-callback dispatch
    emitter.instruction("mov QWORD PTR [rbp - 72], r8");                        // preserve the destination value_type tag for every insertion

    // -- allocate the destination table with headroom for every mapped entry --
    emitter.instruction("mov rax, QWORD PTR [rsi]");                            // rax = source entry count
    emitter.instruction("shl rax, 1");                                          // double the entry count to give the destination insertion headroom
    emitter.instruction("cmp rax, 16");                                         // compare the derived capacity against the runtime minimum
    emitter.instruction("jge __rt_hash_map_capacity_x86");                      // keep the doubled count when it already meets the minimum
    emitter.instruction("mov rax, 16");                                         // clamp very small sources up to the minimum bucket count
    emitter.label("__rt_hash_map_capacity_x86");
    emitter.instruction("mov rdi, rax");                                        // rdi = destination bucket count
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // rsi = destination value_type tag
    emitter.instruction("call __rt_hash_new");                                  // allocate the destination hash
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the destination hash pointer across insertions
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // iterator cursor = 0 (start from header.head)

    // -- walk the source hash in insertion order --
    emitter.label("__rt_hash_map_loop_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // rdi = source hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // rsi = current insertion-order cursor
    emitter.instruction("call __rt_hash_iter_next_value");                      // rax=cursor, rdi=key_ptr, rdx=key_len, rcx=lo, r8=hi, r9=tag
    emitter.instruction("cmp rax, -1");                                         // did the iterator signal end-of-walk?
    emitter.instruction("je __rt_hash_map_done_x86");                           // yes - the destination hash is complete
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the next insertion-order cursor
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // spill the key pointer before rdi is reused as argument zero
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // spill the source key length before the callback call

    // -- php passes the VALUE only; the argument ABI follows the entry's runtime value tag --
    emitter.instruction("cmp r9, 1");                                           // runtime tag 1 = string, which uses the two-register string ABI
    emitter.instruction("je __rt_hash_map_call_str_x86");                       // string values are passed as a pointer/length pair
    emitter.instruction("mov rdi, rcx");                                        // rdi = scalar source value (int, bool, or boxed Mixed pointer)
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // pass the capture environment after the scalar argument
    emitter.instruction("jmp __rt_hash_map_call_x86");                          // invoke the callback through the shared call site

    emitter.label("__rt_hash_map_call_str_x86");
    emitter.instruction("mov rdi, rcx");                                        // rdi = source string pointer
    emitter.instruction("mov rsi, r8");                                         // rsi = source string length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // pass the capture environment after the string pointer/length pair

    emitter.label("__rt_hash_map_call_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // load the callback address into a caller-saved scratch register
    emitter.instruction("call r10");                                            // invoke the user callback on this entry's value

    // -- read the callback result from wherever this result kind leaves it --
    emitter.instruction("mov r10, QWORD PTR [rbp - 64]");                       // r10 = HashMapResultKind selector
    emitter.instruction("cmp r10, 1");                                          // is the result a borrowed string pair needing a copy?
    emitter.instruction("je __rt_hash_map_result_persist_x86");                 // yes - persist it before the destination takes ownership
    emitter.instruction("cmp r10, 2");                                          // is the result an already-owned string pair?
    emitter.instruction("je __rt_hash_map_result_owned_x86");                   // yes - store the pair verbatim
    emitter.instruction("mov rcx, rax");                                        // scalar and boxed-Mixed results arrive in the integer return register
    emitter.instruction("xor r8d, r8d");                                        // pointer-sized results carry no high word
    emitter.instruction("jmp __rt_hash_map_insert_x86");                        // insert the scalar-shaped mapped entry

    emitter.label("__rt_hash_map_result_persist_x86");
    emitter.instruction("call __rt_str_persist");                               // copy the borrowed callback string so the destination owns its own bytes
    emitter.instruction("mov rcx, rax");                                        // rcx = owned mapped value pointer
    emitter.instruction("mov r8, rdx");                                         // r8 = owned mapped value length
    emitter.instruction("jmp __rt_hash_map_insert_x86");                        // insert the owned string-valued entry

    emitter.label("__rt_hash_map_result_owned_x86");
    emitter.instruction("mov rcx, rax");                                        // rcx = already-owned mapped value pointer
    emitter.instruction("mov r8, rdx");                                         // r8 = already-owned mapped value length

    // -- the destination keeps the SOURCE key; hash_set persists string keys itself --
    emitter.label("__rt_hash_map_insert_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 72]");                        // r9 = destination value_type tag
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // rdi = destination hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // rsi = source key_lo
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // rdx = source key_hi (-1 marks an integer key)
    emitter.instruction("call __rt_hash_set");                                  // insert the mapped pair under the preserved source key
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // keep the destination pointer current after possible growth
    emitter.instruction("jmp __rt_hash_map_loop_x86");                          // continue with the next source entry

    emitter.label("__rt_hash_map_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the destination hash pointer in rax
    emitter.instruction("add rsp, 96");                                         // release the hash-map spill slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return with rax = destination hash pointer
}
