//! Purpose:
//! Emits `__rt_mixed_array_loose_eq`, the runtime implementation of PHP's `==`
//! between two arrays: equal element counts, and for every key of the left array a
//! matching key in the right array whose value is loosely equal.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::compare`.
//! - `__rt_mixed_loose_eq` once both operands unbox to an array-like tag.
//!
//! Key details:
//! - PHP's `==` on arrays is ORDER-INDEPENDENT (unlike `===`): the walk enumerates
//!   the left array's keys and looks each one up in the right array, so
//!   `["a"=>1,"b"=>2] == ["b"=>2,"a"=>1]` is true while `[1,2] == [2=>1,3=>2]` is
//!   false.
//! - Key presence is checked BEFORE the value is read. `__rt_mixed_array_get`
//!   answers `null` both for "absent" and for "present but null", so a missing key
//!   would otherwise compare equal to a stored `null`.
//! - elephc has two array representations. Tag 4 (indexed) is always the list
//!   `0..count-1`, so its keys are enumerated by counting; tag 5 (hash) is walked
//!   with the shared `__rt_hash_iter_next_value` cursor protocol. Both feed the same
//!   per-entry comparison block.
//! - Every value read through `__rt_mixed_array_get` is owned by this helper and is
//!   released before the next entry.
//! - Operand cells that wrap another Mixed cell (tag 7) are peeled on entry, because the
//!   count and read helpers only recognize a container tag on the cell they are handed.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_mixed_array_loose_eq` for the active target.
///
/// Input: AArch64 `x0`/`x1` = the two boxed array cells, `x2` = recursion depth;
/// x86_64 `rdi`/`rsi`/`rdx`. Output: `x0` / `rax` = 1 when loosely equal. Both
/// operands stay borrowed.
pub fn emit_mixed_array_loose_eq(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_array_loose_eq_x86_64(emitter);
        return;
    }
    emit_mixed_array_loose_eq_aarch64(emitter);
}

/// Emits the AArch64 array comparison walker.
///
/// Frame (128 bytes): `[sp,#0]` left cell, `[sp,#8]` right cell, `[sp,#16]` depth,
/// `[sp,#24]` shared element count, `[sp,#32]`/`[sp,#40]` right tag and payload,
/// `[sp,#48]` index-or-cursor, `[sp,#56]`/`[sp,#64]` left tag and payload,
/// `[sp,#72]`/`[sp,#80]` the current key pair, `[sp,#88]`/`[sp,#96]` the two owned
/// value cells, `[sp,#104]` the comparison result, `[sp,#112]` saved `x29`/`x30`.
fn emit_mixed_array_loose_eq_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_array_loose_eq ---");
    emitter.label_global("__rt_mixed_array_loose_eq");
    emit_peel_nested_mixed_operands(emitter);

    emitter.instruction("sub sp, sp, #128");                                    // allocate the array comparison frame
    emitter.instruction("stp x29, x30, [sp, #112]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #112");                                   // establish the array comparison frame pointer
    emitter.instruction("stp x0, x1, [sp, #0]");                                // save both boxed array operands
    emitter.instruction("str x2, [sp, #16]");                                   // save the current recursion depth
    emitter.instruction("bl __rt_mixed_count");                                 // count the left array's elements
    emitter.instruction("str x0, [sp, #24]");                                   // save the left element count
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the right boxed array operand
    emitter.instruction("bl __rt_mixed_count");                                 // count the right array's elements
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the left element count
    emitter.instruction("cmp x9, x0");                                          // PHP requires both arrays to hold the same number of entries
    emitter.instruction("b.ne __rt_male_false");                                // different sizes are never loosely equal
    emitter.instruction("cbz x9, __rt_male_true");                              // two empty arrays are loosely equal

    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the left boxed array operand
    emitter.instruction("bl __rt_mixed_unbox");                                 // left cell -> x0=tag, x1=payload pointer
    emitter.instruction("str x0, [sp, #56]");                                   // save the left container tag
    emitter.instruction("str x1, [sp, #64]");                                   // save the left container payload pointer
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the right boxed array operand
    emitter.instruction("bl __rt_mixed_unbox");                                 // right cell -> x0=tag, x1=payload pointer
    emitter.instruction("str x0, [sp, #32]");                                   // save the right container tag
    emitter.instruction("str x1, [sp, #40]");                                   // save the right container payload pointer
    emitter.instruction("mov x11, #0");                                         // the index/cursor starts at the first entry
    emitter.instruction("str x11, [sp, #48]");                                  // save the initial index/cursor
    emitter.instruction("ldr x9, [sp, #56]");                                   // reload the left container tag
    emitter.instruction("cmp x9, #4");                                          // is the left container an indexed array?
    emitter.instruction("b.ne __rt_male_hash_loop");                            // hashes enumerate through the iterator protocol

    // -- indexed arrays are the list 0..count-1, so keys come from the index --
    emitter.label("__rt_male_indexed_loop");
    emitter.instruction("ldr x11, [sp, #48]");                                  // reload the current element index
    emitter.instruction("ldr x12, [sp, #24]");                                  // reload the shared element count
    emitter.instruction("cmp x11, x12");                                        // has every element been compared?
    emitter.instruction("b.ge __rt_male_true");                                 // every key matched loosely
    emitter.instruction("str x11, [sp, #72]");                                  // the element index is the PHP key
    emitter.instruction("mov x13, #-1");                                        // key_hi = -1 marks an integer key
    emitter.instruction("str x13, [sp, #80]");                                  // save the integer-key marker
    emitter.instruction("b __rt_male_entry");                                   // compare this key against the right array

    emitter.label("__rt_male_indexed_next");
    emitter.instruction("ldr x11, [sp, #48]");                                  // reload the current element index
    emitter.instruction("add x11, x11, #1");                                    // advance to the next list slot
    emitter.instruction("str x11, [sp, #48]");                                  // save the advanced element index
    emitter.instruction("b __rt_male_indexed_loop");                            // keep walking the list

    // -- hashes enumerate in insertion order through the shared cursor protocol --
    emitter.label("__rt_male_hash_loop");
    emitter.instruction("ldr x0, [sp, #64]");                                   // reload the left hash payload pointer
    emitter.instruction("ldr x1, [sp, #48]");                                   // reload the iteration cursor
    emitter.instruction("bl __rt_hash_iter_next_value");                        // x0=next cursor, x1=key pointer, x2=key length
    emitter.instruction("cmp x0, #-1");                                         // has the walk consumed every entry?
    emitter.instruction("b.eq __rt_male_true");                                 // every key matched loosely
    emitter.instruction("str x0, [sp, #48]");                                   // save the next iteration cursor
    emitter.instruction("str x1, [sp, #72]");                                   // save the current key low word
    emitter.instruction("str x2, [sp, #80]");                                   // save the current key high word
    emitter.instruction("b __rt_male_entry");                                   // compare this key against the right array

    emitter.label("__rt_male_hash_next");
    emitter.instruction("b __rt_male_hash_loop");                               // keep walking the hash in insertion order

    // -- one key: it must exist on the right and hold a loosely equal value --
    emitter.label("__rt_male_entry");
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the right container tag
    emitter.instruction("cmp x9, #4");                                          // is the right container an indexed array?
    emitter.instruction("b.ne __rt_male_entry_hash_lookup");                    // hashes answer key presence themselves
    emitter.instruction("ldr x13, [sp, #80]");                                  // reload the current key high word
    emitter.instruction("cmp x13, #-1");                                        // is the key an integer key?
    emitter.instruction("b.ne __rt_male_false");                                // a string key cannot exist in a list
    emitter.instruction("ldr x13, [sp, #72]");                                  // reload the current key low word
    emitter.instruction("cmp x13, #0");                                         // is the integer key non-negative?
    emitter.instruction("b.lt __rt_male_false");                                // a negative key is outside every list
    emitter.instruction("ldr x12, [sp, #24]");                                  // reload the shared element count
    emitter.instruction("cmp x13, x12");                                        // is the integer key inside the list bounds?
    emitter.instruction("b.ge __rt_male_false");                                // an out-of-range key is absent
    emitter.instruction("b __rt_male_entry_compare");                           // the key exists, compare the values
    emitter.label("__rt_male_entry_hash_lookup");
    emitter.instruction("ldr x0, [sp, #40]");                                   // reload the right hash payload pointer
    emitter.instruction("ldr x1, [sp, #72]");                                   // reload the current key low word
    emitter.instruction("ldr x2, [sp, #80]");                                   // reload the current key high word
    emitter.instruction("bl __rt_hash_get");                                    // probe the right hash for this key
    emitter.instruction("cbz x0, __rt_male_false");                             // a key missing on the right ends the comparison

    emitter.label("__rt_male_entry_compare");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the left boxed array operand
    emitter.instruction("ldr x1, [sp, #72]");                                   // reload the current key low word
    emitter.instruction("ldr x2, [sp, #80]");                                   // reload the current key high word
    emitter.instruction("mov x3, #0");                                          // read quietly: a comparison must not warn
    emitter.instruction("bl __rt_mixed_array_get");                             // read the left value as an owned boxed cell
    emitter.instruction("str x0, [sp, #88]");                                   // save the owned left value cell
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the right boxed array operand
    emitter.instruction("ldr x1, [sp, #72]");                                   // reload the current key low word
    emitter.instruction("ldr x2, [sp, #80]");                                   // reload the current key high word
    emitter.instruction("mov x3, #0");                                          // read quietly: a comparison must not warn
    emitter.instruction("bl __rt_mixed_array_get");                             // read the right value as an owned boxed cell
    emitter.instruction("str x0, [sp, #96]");                                   // save the owned right value cell
    emitter.instruction("ldr x0, [sp, #88]");                                   // reload the left value cell
    emitter.instruction("ldr x1, [sp, #96]");                                   // reload the right value cell
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the current recursion depth
    emitter.instruction("bl __rt_mixed_loose_eq_d");                            // compare the two element values loosely
    emitter.instruction("str x0, [sp, #104]");                                  // save the element comparison result
    emitter.instruction("ldr x0, [sp, #88]");                                   // reload the owned left value cell
    emitter.instruction("bl __rt_decref_mixed");                                // release the left element copy
    emitter.instruction("ldr x0, [sp, #96]");                                   // reload the owned right value cell
    emitter.instruction("bl __rt_decref_mixed");                                // release the right element copy
    emitter.instruction("ldr x0, [sp, #104]");                                  // reload the element comparison result
    emitter.instruction("cbz x0, __rt_male_false");                             // one differing element ends the comparison
    emitter.instruction("ldr x9, [sp, #56]");                                   // reload the left container tag
    emitter.instruction("cmp x9, #4");                                          // did this entry come from the list walk?
    emitter.instruction("b.eq __rt_male_indexed_next");                         // resume the list walk
    emitter.instruction("b __rt_male_hash_next");                               // resume the hash walk

    emitter.label("__rt_male_true");
    emitter.instruction("mov x0, #1");                                          // report that the two arrays are loosely equal
    emitter.instruction("b __rt_male_done");                                    // return the true result

    emitter.label("__rt_male_false");
    emitter.instruction("mov x0, #0");                                          // report that the two arrays differ

    emitter.label("__rt_male_done");
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // release the array comparison frame
    emitter.instruction("ret");                                                 // return the array loose-equality boolean
}

/// Replaces each operand cell that wraps another Mixed cell (tag 7) with the cell it wraps.
///
/// `__rt_mixed_count` and `__rt_mixed_array_get` read the tag word of the cell they are given
/// and treat tag 7 as "not a container", so a wrapper operand counted 0 and read every entry as
/// null. `__rt_in_array_boxed` hands over exactly such cells — a Mixed needle, and each element
/// of a Mixed-slot haystack, is described by a borrowed tag-7 stack cell — which made
/// `in_array([1, 2], $mixed)` miss a real match and match an unrelated array of the same size.
/// The walker is only entered once `__rt_mixed_loose_eq` unboxed both operands to array tags, so
/// every chain ends at a concrete array cell and no wrapper holds a null pointer. Only the
/// operand registers are rewritten; the cells stay borrowed.
fn emit_peel_nested_mixed_operands(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.label("__rt_male_peel_left");
            emitter.instruction("ldr x9, [x0]");                                // inspect the left operand cell tag
            emitter.instruction("cmp x9, #7");                                  // does the left cell wrap another Mixed cell?
            emitter.instruction("b.ne __rt_male_peel_right");                   // a concrete left cell is ready for the walk
            emitter.instruction("ldr x0, [x0, #8]");                            // follow the wrapped left cell pointer
            emitter.instruction("b __rt_male_peel_left");                       // keep peeling nested left wrappers
            emitter.label("__rt_male_peel_right");
            emitter.instruction("ldr x9, [x1]");                                // inspect the right operand cell tag
            emitter.instruction("cmp x9, #7");                                  // does the right cell wrap another Mixed cell?
            emitter.instruction("b.ne __rt_male_peeled");                       // a concrete right cell is ready for the walk
            emitter.instruction("ldr x1, [x1, #8]");                            // follow the wrapped right cell pointer
            emitter.instruction("b __rt_male_peel_right");                      // keep peeling nested right wrappers
            emitter.label("__rt_male_peeled");
        }
        Arch::X86_64 => {
            emitter.label("__rt_male_peel_left");
            emitter.instruction("cmp QWORD PTR [rdi], 7");                      // does the left cell wrap another Mixed cell?
            emitter.instruction("jne __rt_male_peel_right");                    // a concrete left cell is ready for the walk
            emitter.instruction("mov rdi, QWORD PTR [rdi + 8]");                // follow the wrapped left cell pointer
            emitter.instruction("jmp __rt_male_peel_left");                     // keep peeling nested left wrappers
            emitter.label("__rt_male_peel_right");
            emitter.instruction("cmp QWORD PTR [rsi], 7");                      // does the right cell wrap another Mixed cell?
            emitter.instruction("jne __rt_male_peeled");                        // a concrete right cell is ready for the walk
            emitter.instruction("mov rsi, QWORD PTR [rsi + 8]");                // follow the wrapped right cell pointer
            emitter.instruction("jmp __rt_male_peel_right");                    // keep peeling nested right wrappers
            emitter.label("__rt_male_peeled");
        }
    }
}

/// Emits the x86_64 array comparison walker.
///
/// Frame (112 bytes below `rbp`): `[rbp-8]` left cell, `[rbp-16]` right cell,
/// `[rbp-24]` depth, `[rbp-32]` shared element count, `[rbp-40]`/`[rbp-48]` right
/// tag and payload, `[rbp-56]` index-or-cursor, `[rbp-64]`/`[rbp-72]` left tag and
/// payload, `[rbp-80]`/`[rbp-88]` the current key pair, `[rbp-96]`/`[rbp-104]` the
/// two owned value cells, `[rbp-112]` the comparison result.
fn emit_mixed_array_loose_eq_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_array_loose_eq ---");
    emitter.label_global("__rt_mixed_array_loose_eq");
    emit_peel_nested_mixed_operands(emitter);

    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the array comparison frame pointer
    emitter.instruction("sub rsp, 112");                                        // allocate the aligned array comparison frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the left boxed array operand
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the right boxed array operand
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the current recursion depth
    emitter.instruction("mov rax, rdi");                                        // move the left cell into the count input register
    abi::emit_call_label(emitter, "__rt_mixed_count"); // count the left array's elements
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the left element count
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the right cell for counting
    abi::emit_call_label(emitter, "__rt_mixed_count"); // count the right array's elements
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the left element count
    emitter.instruction("cmp r10, rax");                                        // PHP requires both arrays to hold the same number of entries
    emitter.instruction("jne __rt_male_false");                                 // different sizes are never loosely equal
    emitter.instruction("test r10, r10");                                       // are both arrays empty?
    emitter.instruction("jz __rt_male_true");                                   // two empty arrays are loosely equal

    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the left cell for unboxing
    abi::emit_call_label(emitter, "__rt_mixed_unbox"); // left cell -> rax=tag, rdi=payload pointer
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // save the left container tag
    emitter.instruction("mov QWORD PTR [rbp - 72], rdi");                       // save the left container payload pointer
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the right cell for unboxing
    abi::emit_call_label(emitter, "__rt_mixed_unbox"); // right cell -> rax=tag, rdi=payload pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the right container tag
    emitter.instruction("mov QWORD PTR [rbp - 48], rdi");                       // save the right container payload pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // the index/cursor starts at the first entry
    emitter.instruction("cmp QWORD PTR [rbp - 64], 4");                         // is the left container an indexed array?
    emitter.instruction("jne __rt_male_hash_loop");                             // hashes enumerate through the iterator protocol

    // -- indexed arrays are the list 0..count-1, so keys come from the index --
    emitter.label("__rt_male_indexed_loop");
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // reload the current element index
    emitter.instruction("cmp r10, QWORD PTR [rbp - 32]");                       // has every element been compared?
    emitter.instruction("jge __rt_male_true");                                  // every key matched loosely
    emitter.instruction("mov QWORD PTR [rbp - 80], r10");                       // the element index is the PHP key
    emitter.instruction("mov QWORD PTR [rbp - 88], -1");                        // key_hi = -1 marks an integer key
    emitter.instruction("jmp __rt_male_entry");                                 // compare this key against the right array

    emitter.label("__rt_male_indexed_next");
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // reload the current element index
    emitter.instruction("add r10, 1");                                          // advance to the next list slot
    emitter.instruction("mov QWORD PTR [rbp - 56], r10");                       // save the advanced element index
    emitter.instruction("jmp __rt_male_indexed_loop");                          // keep walking the list

    // -- hashes enumerate in insertion order through the shared cursor protocol --
    emitter.label("__rt_male_hash_loop");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 72]");                       // reload the left hash payload pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // reload the iteration cursor
    abi::emit_call_label(emitter, "__rt_hash_iter_next_value"); // rax=next cursor, rdi=key pointer, rdx=key length
    emitter.instruction("cmp rax, -1");                                         // has the walk consumed every entry?
    emitter.instruction("je __rt_male_true");                                   // every key matched loosely
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save the next iteration cursor
    emitter.instruction("mov QWORD PTR [rbp - 80], rdi");                       // save the current key low word
    emitter.instruction("mov QWORD PTR [rbp - 88], rdx");                       // save the current key high word
    emitter.instruction("jmp __rt_male_entry");                                 // compare this key against the right array

    emitter.label("__rt_male_hash_next");
    emitter.instruction("jmp __rt_male_hash_loop");                             // keep walking the hash in insertion order

    // -- one key: it must exist on the right and hold a loosely equal value --
    emitter.label("__rt_male_entry");
    emitter.instruction("cmp QWORD PTR [rbp - 40], 4");                         // is the right container an indexed array?
    emitter.instruction("jne __rt_male_entry_hash_lookup");                     // hashes answer key presence themselves
    emitter.instruction("cmp QWORD PTR [rbp - 88], -1");                        // is the key an integer key?
    emitter.instruction("jne __rt_male_false");                                 // a string key cannot exist in a list
    emitter.instruction("mov r10, QWORD PTR [rbp - 80]");                       // reload the current key low word
    emitter.instruction("cmp r10, 0");                                          // is the integer key non-negative?
    emitter.instruction("jl __rt_male_false");                                  // a negative key is outside every list
    emitter.instruction("cmp r10, QWORD PTR [rbp - 32]");                       // is the integer key inside the list bounds?
    emitter.instruction("jge __rt_male_false");                                 // an out-of-range key is absent
    emitter.instruction("jmp __rt_male_entry_compare");                         // the key exists, compare the values
    emitter.label("__rt_male_entry_hash_lookup");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // reload the right hash payload pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 80]");                       // reload the current key low word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 88]");                       // reload the current key high word
    abi::emit_call_label(emitter, "__rt_hash_get"); // probe the right hash for this key
    emitter.instruction("test rax, rax");                                       // did the right hash contain this key?
    emitter.instruction("jz __rt_male_false");                                  // a key missing on the right ends the comparison

    emitter.label("__rt_male_entry_compare");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the left boxed array operand
    emitter.instruction("mov rsi, QWORD PTR [rbp - 80]");                       // reload the current key low word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 88]");                       // reload the current key high word
    emitter.instruction("xor ecx, ecx");                                        // read quietly: a comparison must not warn
    abi::emit_call_label(emitter, "__rt_mixed_array_get"); // read the left value as an owned boxed cell
    emitter.instruction("mov QWORD PTR [rbp - 96], rax");                       // save the owned left value cell
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the right boxed array operand
    emitter.instruction("mov rsi, QWORD PTR [rbp - 80]");                       // reload the current key low word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 88]");                       // reload the current key high word
    emitter.instruction("xor ecx, ecx");                                        // read quietly: a comparison must not warn
    abi::emit_call_label(emitter, "__rt_mixed_array_get"); // read the right value as an owned boxed cell
    emitter.instruction("mov QWORD PTR [rbp - 104], rax");                      // save the owned right value cell
    emitter.instruction("mov rdi, QWORD PTR [rbp - 96]");                       // reload the left value cell
    emitter.instruction("mov rsi, QWORD PTR [rbp - 104]");                      // reload the right value cell
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload the current recursion depth
    abi::emit_call_label(emitter, "__rt_mixed_loose_eq_d"); // compare the two element values loosely
    emitter.instruction("mov QWORD PTR [rbp - 112], rax");                      // save the element comparison result
    emitter.instruction("mov rax, QWORD PTR [rbp - 96]");                       // reload the owned left value cell
    abi::emit_call_label(emitter, "__rt_decref_mixed"); // release the left element copy
    emitter.instruction("mov rax, QWORD PTR [rbp - 104]");                      // reload the owned right value cell
    abi::emit_call_label(emitter, "__rt_decref_mixed"); // release the right element copy
    emitter.instruction("cmp QWORD PTR [rbp - 112], 0");                        // did the two element values compare equal?
    emitter.instruction("je __rt_male_false");                                  // one differing element ends the comparison
    emitter.instruction("cmp QWORD PTR [rbp - 64], 4");                         // did this entry come from the list walk?
    emitter.instruction("je __rt_male_indexed_next");                           // resume the list walk
    emitter.instruction("jmp __rt_male_hash_next");                             // resume the hash walk

    emitter.label("__rt_male_true");
    emitter.instruction("mov rax, 1");                                          // report that the two arrays are loosely equal
    emitter.instruction("jmp __rt_male_done");                                  // return the true result

    emitter.label("__rt_male_false");
    emitter.instruction("xor rax, rax");                                        // report that the two arrays differ

    emitter.label("__rt_male_done");
    emitter.instruction("add rsp, 112");                                        // release the array comparison frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the array loose-equality boolean
}
