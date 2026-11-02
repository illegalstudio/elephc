//! Purpose:
//! Shared pieces of the AOT backtrace frame reader that do not depend on the function being
//! read: appending the surplus-argument tail of a frame and trimming its argument list to the
//! caller-supplied count.
//!
//! Called from:
//! - `crate::codegen::lower_inst::core_builtins::backtrace` for typed collectors, inline.
//! - The runtime object, as `__rt_backtrace_append_tail` and `__rt_backtrace_trim_args`, for
//!   every `mixed` collector -- the case every function with a hidden argument collector has.
//!
//! Key details:
//! - Every function that can appear in a trace used to carry its own copy of these loops, about
//!   150 instructions each, on the Symfony `--web` app's 5 700 frames. The sequences are MOVED
//!   into the two runtime routines below, not rewritten: they are the same emitter code, run
//!   against the stack layout the inline fragment used.
//! - `labels` supplies unique label names: the frame reader's context numbers them per body, the
//!   runtime routines use fixed names.

use crate::codegen_support::abi;
use crate::codegen_support::arrays::emit_array_value_type_stamp;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::value_boxing::{
    emit_box_current_owned_value_as_mixed, emit_box_current_value_as_mixed,
};
use crate::types::PhpType;

/// Branches to `hash_source` when the argument array saved at `[sp+32]` is hash storage.
///
/// The surplus collector a spread call fills (`$listener(...$args)`, Symfony's lazy event
/// listeners) arrives as a hash with integer keys; `func_get_args()` reads it through the ordinary
/// array operations, which dispatch on the heap kind, but the packed walk below read the hash
/// header as slots and listed `0, 12` as the frame's arguments.
fn emit_branch_if_hash_argument_source(emitter: &mut Emitter, labels: &mut dyn FnMut(&str) -> String, hash_source: &str) {
    let packed = labels("debug_backtrace_arg_tail_packed");
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x0", 32);
            emitter.instruction(&format!("cbz x0, {packed}"));              // a cleared collector takes the empty packed walk
            abi::emit_call_label(emitter, "__rt_heap_kind");
            emitter.instruction("cmp x0, #3");                              // is the collector hash storage?
            emitter.instruction(&format!("b.eq {hash_source}"));            // walk a hash by insertion order
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "rdi", 32);
            emitter.instruction("test rdi, rdi");                           // inspect whether the collector slot was cleared
            emitter.instruction(&format!("jz {packed}"));                   // a cleared collector takes the empty packed walk
            abi::emit_load_temporary_stack_slot(emitter, "rax", 32);
            abi::emit_call_label(emitter, "__rt_heap_kind");
            emitter.instruction("cmp rax, 3");                              // is the collector hash storage?
            emitter.instruction(&format!("je {hash_source}"));              // walk a hash by insertion order
        }
    }
    emitter.label(&packed);
}

/// Appends the values of a hash argument array in insertion order, skipping the first `[sp]`.
///
/// `[sp+16]` holds the iteration cursor and `[sp]` the number of leading entries still to skip
/// (the optional-argument count a collector may start with). Boxed values are appended as
/// borrowed cells; any other value is boxed by its entry tag first.
fn emit_append_hash_argument_tail(emitter: &mut Emitter, labels: &mut dyn FnMut(&str) -> String, done: &str) {
    let next = labels("debug_backtrace_arg_tail_hash_next");
    let take = labels("debug_backtrace_arg_tail_hash_take");
    let native = labels("debug_backtrace_arg_tail_hash_native");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("str xzr, [sp, #16]");                      // start a fresh insertion-order walk
            emitter.label(&next);
            abi::emit_load_temporary_stack_slot(emitter, "x0", 32);
            abi::emit_load_temporary_stack_slot(emitter, "x1", 16);
            abi::emit_call_label(emitter, "__rt_hash_iter_next");
            emitter.instruction("cmn x0, #1");                              // has the walk finished?
            emitter.instruction(&format!("b.eq {done}"));                   // every entry was visited
            emitter.instruction("str x0, [sp, #16]");                       // keep the cursor for the next entry
            emitter.instruction("ldr x9, [sp]");                            // load the number of entries still to skip
            emitter.instruction(&format!("cbz x9, {take}"));                // nothing left to skip: append this entry
            emitter.instruction("sub x9, x9, #1");                          // skip one leading entry
            emitter.instruction("str x9, [sp]");                            // remember how many remain to skip
            emitter.instruction(&format!("b {next}"));                      // continue with the next entry
            emitter.label(&take);
            emitter.instruction("cmp x5, #7");                              // is the entry already a boxed Mixed cell?
            emitter.instruction(&format!("b.ne {native}"));                 // native values are boxed first
            emitter.instruction("mov x1, x3");                              // the borrowed box is the entry's low word
            abi::emit_load_temporary_stack_slot(emitter, "x0", 48);
            abi::emit_call_label(emitter, "__rt_array_push_refcounted");
            abi::emit_store_to_sp(emitter, "x0", 48);
            emitter.instruction(&format!("b {next}"));                      // continue with the next entry
            emitter.label(&native);
            emitter.instruction("mov x0, x5");                              // box by the entry's own value tag
            emitter.instruction("mov x1, x3");                              // pass the entry's low payload word
            emitter.instruction("mov x2, x4");                              // pass the entry's high payload word
        }
        Arch::X86_64 => {
            emitter.instruction("mov QWORD PTR [rsp + 16], 0");             // start a fresh insertion-order walk
            emitter.label(&next);
            abi::emit_load_temporary_stack_slot(emitter, "rdi", 32);
            abi::emit_load_temporary_stack_slot(emitter, "rsi", 16);
            abi::emit_call_label(emitter, "__rt_hash_iter_next");
            emitter.instruction("cmp rax, -1");                             // has the walk finished?
            emitter.instruction(&format!("je {done}"));                     // every entry was visited
            emitter.instruction("mov QWORD PTR [rsp + 16], rax");           // keep the cursor for the next entry
            emitter.instruction("mov r10, QWORD PTR [rsp]");                // load the number of entries still to skip
            emitter.instruction("test r10, r10");                           // anything left to skip?
            emitter.instruction(&format!("jz {take}"));                     // nothing left to skip: append this entry
            emitter.instruction("dec r10");                                 // skip one leading entry
            emitter.instruction("mov QWORD PTR [rsp], r10");                // remember how many remain to skip
            emitter.instruction(&format!("jmp {next}"));                    // continue with the next entry
            emitter.label(&take);
            emitter.instruction("cmp r9, 7");                               // is the entry already a boxed Mixed cell?
            emitter.instruction(&format!("jne {native}"));                  // native values are boxed first
            emitter.instruction("mov rsi, rcx");                            // the borrowed box is the entry's low word
            abi::emit_load_temporary_stack_slot(emitter, "rdi", 48);
            abi::emit_call_label(emitter, "__rt_array_push_refcounted");
            abi::emit_store_to_sp(emitter, "rax", 48);
            emitter.instruction(&format!("jmp {next}"));                    // continue with the next entry
            emitter.label(&native);
            emitter.instruction("mov rax, r9");                             // box by the entry's own value tag
            emitter.instruction("mov rdi, rcx");                            // pass the entry's low payload word
            emitter.instruction("mov rsi, r8");                             // pass the entry's high payload word
        }
    }
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    append_argument_tail_box(emitter, true);
    abi::emit_jump(emitter, &next);
}

/// Branches after every requested hidden argument has been appended.
fn emit_argument_tail_done_guard(emitter: &mut Emitter, done: &str) {
    let index_reg = abi::int_result_reg(emitter);
    let length_reg = abi::secondary_scratch_reg(emitter);
    abi::emit_load_temporary_stack_slot(emitter, index_reg, 0);
    abi::emit_load_temporary_stack_slot(emitter, length_reg, 8);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter
                .instruction(&format!("cmp {index_reg}, {length_reg}"));
            emitter.instruction(&format!("b.ge {done}"));                   // stop after every hidden argument is appended
        }
        Arch::X86_64 => {
            emitter
                .instruction(&format!("cmp {index_reg}, {length_reg}"));
            emitter.instruction(&format!("jge {done}"));                    // stop after every hidden argument is appended
        }
    }
}

/// Appends one argument of a collector statically typed `array<mixed>`, by its RUNTIME layout.
///
/// The static type promises boxed cells, but the array a caller hands over need not have them:
/// a spread call (`function (...$args) use ($listener) { return $listener(...$args); }`, how
/// Symfony's event dispatcher reaches a lazy listener) passes the surplus arguments in a packed
/// array of native values -- a string in a two-word slot. Reading those slots as box pointers
/// pushed `0xc` as a cell, and the trace of the 404 a route listener throws killed the worker.
/// The value type stamped in the array header decides: boxed cells are appended as before, any
/// other layout is read at its real width and boxed.
fn emit_append_runtime_typed_argument_tail(emitter: &mut Emitter, labels: &mut dyn FnMut(&str) -> String) {
    let boxed = labels("debug_backtrace_arg_tail_boxed");
    let one_word = labels("debug_backtrace_arg_tail_one_word");
    let loaded = labels("debug_backtrace_arg_tail_loaded");
    let done = labels("debug_backtrace_arg_tail_value_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x10", 32);
            emitter.instruction("ldr x11, [x10, #-8]");                     // load the packed array header
            emitter.instruction("lsr x11, x11, #8");                        // move the runtime value_type byte into the low bits
            emitter.instruction("and x11, x11, #0x7f");                     // isolate the element value_type
            emitter.instruction("cmp x11, #7");                             // are the elements already boxed Mixed cells?
            emitter.instruction(&format!("b.eq {boxed}"));                  // boxed cells are appended as borrowed boxes
            abi::emit_load_temporary_stack_slot(emitter, "x9", 0);
            emitter.instruction("cmp x11, #1");                             // strings occupy two-word slots
            emitter.instruction(&format!("b.ne {one_word}"));               // every other native value is one word
            emitter.instruction("add x12, x10, x9, lsl #4");                // select the indexed two-word slot
            emitter.instruction("ldp x1, x2, [x12, #24]");                  // load the string pointer and length past the metadata
            emitter.instruction(&format!("b {loaded}"));                    // box the loaded string
            emitter.label(&one_word);
            emitter.instruction("add x12, x10, #24");                       // advance past array metadata
            emitter.instruction("ldr x1, [x12, x9, lsl #3]");               // load the one-word native value
            emitter.instruction("mov x2, xzr");                             // one-word values carry no second word
            emitter.label(&loaded);
            emitter.instruction("mov x0, x11");                             // box by the array's own value_type
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "r10", 32);
            emitter.instruction("mov r11, QWORD PTR [r10 - 8]");            // load the packed array header
            emitter.instruction("shr r11, 8");                              // move the runtime value_type byte into the low bits
            emitter.instruction("and r11, 0x7f");                           // isolate the element value_type
            emitter.instruction("cmp r11, 7");                              // are the elements already boxed Mixed cells?
            emitter.instruction(&format!("je {boxed}"));                    // boxed cells are appended as borrowed boxes
            abi::emit_load_temporary_stack_slot(emitter, "r9", 0);
            emitter.instruction("cmp r11, 1");                              // strings occupy two-word slots
            emitter.instruction(&format!("jne {one_word}"));                // every other native value is one word
            emitter.instruction("shl r9, 4");                               // scale the index for a two-word slot
            emitter.instruction("mov rdi, QWORD PTR [r10 + r9 + 24]");      // load the string pointer past the metadata
            emitter.instruction("mov rsi, QWORD PTR [r10 + r9 + 32]");      // load the string length
            emitter.instruction(&format!("jmp {loaded}"));                  // box the loaded string
            emitter.label(&one_word);
            emitter.instruction("mov rdi, QWORD PTR [r10 + r9 * 8 + 24]");  // load the one-word native value
            emitter.instruction("xor esi, esi");                            // one-word values carry no second word
            emitter.label(&loaded);
            emitter.instruction("mov rax, r11");                            // box by the array's own value_type
        }
    }
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    append_argument_tail_box(emitter, true);
    abi::emit_jump(emitter, &done);
    emitter.label(&boxed);
    emit_append_borrowed_argument_tail_box(emitter);
    emitter.label(&done);
}

/// Appends one borrowed boxed-Mixed hidden argument without rebuilding its cell.
fn emit_append_borrowed_argument_tail_box(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x9", 0);
            abi::emit_load_temporary_stack_slot(emitter, "x10", 32);
            emitter.instruction("add x10, x10, #24");                       // advance from array metadata to Mixed slots
            emitter.instruction("ldr x1, [x10, x9, lsl #3]");               // load the borrowed boxed argument cell
            abi::emit_load_temporary_stack_slot(emitter, "x0", 48);
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "r10", 0);
            abi::emit_load_temporary_stack_slot(emitter, "r11", 32);
            emitter.instruction("mov rsi, QWORD PTR [r11 + r10 * 8 + 24]"); // load the borrowed boxed argument cell
            abi::emit_load_temporary_stack_slot(emitter, "rdi", 48);
        }
    }
    abi::emit_call_label(emitter, "__rt_array_push_refcounted");
    abi::emit_store_to_sp(emitter, abi::int_result_reg(emitter), 48);
}

/// Appends a boxed hidden argument and optionally releases the temporary cell afterward.
fn append_argument_tail_box(emitter: &mut Emitter, release_temporary: bool) {
    let box_reg = abi::int_result_reg(emitter).to_string();
    abi::emit_push_reg(emitter, &box_reg);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x0", 64);
            abi::emit_load_temporary_stack_slot(emitter, "x1", 0);
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "rdi", 64);
            abi::emit_load_temporary_stack_slot(emitter, "rsi", 0);
        }
    }
    abi::emit_call_label(emitter, "__rt_array_push_refcounted");
    abi::emit_store_to_sp(emitter, &box_reg, 64);
    if release_temporary {
        abi::emit_load_temporary_stack_slot(emitter, &box_reg, 0);
        abi::emit_call_label(emitter, "__rt_decref_mixed");
    }
    abi::emit_release_temporary_stack(emitter, 16);
}

/// Advances the hidden-argument array index after one retained append.
fn increment_argument_tail_index(emitter: &mut Emitter) {
    let index_reg = abi::int_result_reg(emitter);
    abi::emit_load_temporary_stack_slot(emitter, index_reg, 0);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("add x0, x0, #1"),             // advance the hidden-argument index
        Arch::X86_64 => emitter.instruction("add rax, 1"),                  // advance the hidden-argument index
    }
    abi::emit_store_to_sp(emitter, index_reg, 0);
}

/// Where an appended range starts.
pub(crate) enum TailStart<'a> {
    /// A compile-time index.
    Immediate(usize),
    /// A register that holds the index and survives one push.
    Register(&'a str),
}

/// Appends an indexed-array range into the boxed-Mixed destination saved on the stack.
pub(crate) fn append_argument_array_range(
    emitter: &mut Emitter, labels: &mut dyn FnMut(&str) -> String,
    start: TailStart<'_>,
    element_ty: &PhpType,
) {
    let result_reg = abi::int_result_reg(emitter).to_string();
    abi::emit_push_reg(emitter, &result_reg);
    abi::emit_reserve_temporary_stack(emitter, 32);
    match start {
        TailStart::Immediate(start) => {
            abi::emit_load_int_immediate(emitter, &result_reg, start as i64);
            abi::emit_store_to_sp(emitter, &result_reg, 0);
        }
        TailStart::Register(register) => abi::emit_store_to_sp(emitter, register, 0),
    }
    let source_reg = abi::secondary_scratch_reg(emitter);
    abi::emit_load_temporary_stack_slot(emitter, source_reg, 32);
    // A frame read while its own cleanup runs (a destructor creating an exception during the
    // epilogue) can find the collector slot already zeroed: that is an empty tail, not a
    // pointer to dereference.
    let null_source = labels("debug_backtrace_arg_tail_null_source");
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("cbz {source_reg}, {null_source}")), // an already-cleared collector has no arguments
        Arch::X86_64 => {
            emitter.instruction(&format!("test {source_reg}, {source_reg}")); // inspect whether the collector slot was cleared
            emitter.instruction(&format!("jz {null_source}"));              // an already-cleared collector has no arguments
        }
    }
    abi::emit_load_from_address(emitter, source_reg, source_reg, 0);
    emitter.label(&null_source);
    abi::emit_store_to_sp(emitter, source_reg, 8);

    let loop_label = labels("debug_backtrace_arg_tail");
    let done = labels("debug_backtrace_arg_tail_done");
    let hash_source = labels("debug_backtrace_arg_tail_hash");
    emit_branch_if_hash_argument_source(emitter, labels, &hash_source);
    emitter.label(&loop_label);
    emit_argument_tail_done_guard(emitter, &done);
    emit_append_argument_tail_value(emitter, labels, element_ty);
    increment_argument_tail_index(emitter);
    abi::emit_jump(emitter, &loop_label);
    emitter.label(&hash_source);
    emit_append_hash_argument_tail(emitter, labels, &done);
    emitter.label(&done);
    abi::emit_release_temporary_stack(emitter, 32);
    let discard = abi::secondary_scratch_reg(emitter);
    abi::emit_pop_reg(emitter, discard);
}

/// Boxes and appends the current indexed hidden argument according to its slot type.
fn emit_append_argument_tail_value(emitter: &mut Emitter, labels: &mut dyn FnMut(&str) -> String, element_ty: &PhpType) {
    let repr = element_ty.codegen_repr();
    if matches!(repr, PhpType::Mixed | PhpType::Union(_)) {
        emit_append_runtime_typed_argument_tail(emitter, labels);
        return;
    }

    emit_load_argument_tail_value(emitter, &repr);
    emit_box_current_value_as_mixed(emitter, &repr);
    append_argument_tail_box(emitter, true);
}

/// Loads one typed hidden-argument array slot into the canonical result registers.
fn emit_load_argument_tail_value(emitter: &mut Emitter, element_ty: &PhpType) {
    let stride = element_ty.stack_size().max(8);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x9", 0);
            abi::emit_load_temporary_stack_slot(emitter, "x10", 32);
            if stride == 16 {
                emitter.instruction("lsl x9, x9, #4");                      // scale the index for a two-word typed slot
                emitter.instruction("add x10, x10, x9");                    // select the indexed typed slot
                emitter.instruction("add x10, x10, #24");                   // advance past array metadata
            } else {
                emitter.instruction("add x10, x10, #24");                   // advance past array metadata
                emitter.instruction("add x10, x10, x9, lsl #3");            // select the indexed one-word slot
            }
            match element_ty {
                PhpType::Float => emitter.instruction("ldr d0, [x10]"),     // load a typed floating-point argument
                PhpType::Str => emitter.instruction("ldp x1, x2, [x10]"),   // load a typed string pointer and length
                PhpType::TaggedScalar => emitter.instruction("ldp x0, x1, [x10]"), // load a typed tagged-scalar pair
                _ => emitter.instruction("ldr x0, [x10]"),                  // load a one-word typed argument
            }
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "r10", 0);
            abi::emit_load_temporary_stack_slot(emitter, "r11", 32);
            if stride == 16 {
                emitter.instruction("shl r10, 4");                          // scale the index for a two-word typed slot
                emitter.instruction("lea r11, [r11 + r10 + 24]");           // select the indexed typed slot after metadata
            } else {
                emitter.instruction("lea r11, [r11 + r10 * 8 + 24]");       // select the indexed one-word slot after metadata
            }
            match element_ty {
                PhpType::Float => emitter.instruction("movsd xmm0, QWORD PTR [r11]"), // load a typed floating-point argument
                PhpType::Str => {
                    emitter.instruction("mov rax, QWORD PTR [r11]");        // load the typed string pointer
                    emitter.instruction("mov rdx, QWORD PTR [r11 + 8]");    // load the typed string length
                }
                PhpType::TaggedScalar => {
                    emitter.instruction("mov rax, QWORD PTR [r11]");        // load the tagged-scalar payload
                    emitter.instruction("mov rdx, QWORD PTR [r11 + 8]");    // load the tagged-scalar tag
                }
                _ => emitter.instruction("mov rax, QWORD PTR [r11]"),       // load a one-word typed argument
            }
        }
    }
}

/// Drops defaulted parameters beyond the actual caller-supplied argument count.
///
/// The array is in the result register and the caller-supplied count (negative when unknown)
/// in the secondary scratch register.
pub(crate) fn trim_argument_array_to_actual_count(emitter: &mut Emitter, labels: &mut dyn FnMut(&str) -> String) {
    let no_trim = labels("debug_backtrace_args_not_trimmed");
    // The trim only ever SHORTENS the list. A caller count above what the frame collected (a
    // spread or invoker path that did not hand every argument over) used to slice PAST the end:
    // the result claimed three arguments, held two, and `getTraceAsString()` dereferenced the
    // unfilled third slot -- Symfony's 404 page died rendering the exception's trace.
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x10, #0");                             // test whether caller-count metadata is available
            emitter.instruction(&format!("b.lt {no_trim}"));                // keep every argument when the count is unknown
            emitter.instruction("ldr x9, [x0]");                            // load how many arguments the frame collected
            emitter.instruction("cmp x10, x9");                             // would the caller count lengthen the list?
            emitter.instruction(&format!("b.ge {no_trim}"));                // never extend past the collected arguments
            emitter.instruction("mov x2, x10");                             // pass the caller-visible slice length
            emitter.instruction("mov x1, #0");                              // slice from the first argument
            emitter.instruction("mov x3, #1");                              // retain refcounted Mixed elements in the slice
        }
        Arch::X86_64 => {
            emitter.instruction("cmp r10, 0");                              // test whether caller-count metadata is available
            emitter.instruction(&format!("jl {no_trim}"));                  // keep every argument when the count is unknown
            emitter.instruction("cmp r10, QWORD PTR [rax]");                // would the caller count lengthen the list?
            emitter.instruction(&format!("jge {no_trim}"));                 // never extend past the collected arguments
            emitter.instruction("mov rdx, r10");                            // pass the caller-visible slice length
            emitter.instruction("mov rdi, rax");                            // pass the original argument array
            emitter.instruction("xor esi, esi");                            // slice from the first argument
            emitter.instruction("mov ecx, 1");                              // retain refcounted Mixed elements in the slice
        }
    }
    let original = abi::int_result_reg(emitter).to_string();
    abi::emit_push_reg(emitter, &original);
    if emitter.target.arch == Arch::AArch64 {
        abi::emit_load_temporary_stack_slot(emitter, "x0", 0);
    }
    abi::emit_call_label(emitter, "__rt_array_slice_refcounted");
    let sliced = abi::int_result_reg(emitter).to_string();
    emit_array_value_type_stamp(emitter, &sliced, &PhpType::Mixed);
    abi::emit_push_reg(emitter, &sliced);
    abi::emit_load_temporary_stack_slot(emitter, &original, 16);
    abi::emit_call_label(emitter, "__rt_decref_array");
    abi::emit_pop_reg(emitter, &sliced);
    abi::emit_release_temporary_stack(emitter, 16);
    emitter.label(&no_trim);
}


/// Emits `__rt_backtrace_append_tail` and `__rt_backtrace_trim_args` for the active target.
///
/// * `__rt_backtrace_append_tail(dest, collector, start)` appends the values of a hidden argument
///   collector (packed or hash, `mixed` elements read by their runtime layout) from index `start`
///   to the boxed-Mixed array `dest` and returns the possibly reallocated array.
/// * `__rt_backtrace_trim_args(array, count)` shortens `array` to the first `count` entries when
///   `count` is known (non-negative) and smaller than the array, and returns the result.
pub(crate) fn emit_runtime(emitter: &mut Emitter) {
    let x86 = emitter.target.arch == Arch::X86_64;
    let mut counter = 0usize;
    let mut labels = move |prefix: &str| {
        counter += 1;
        format!("__rt_{prefix}_{counter}")
    };

    emitter.blank();
    emitter.comment("--- runtime: backtrace_append_tail ---");
    emitter.label_global("__rt_backtrace_append_tail");
    if x86 {
        emitter.instruction("push rbp");                                        // preserve the caller frame pointer
        emitter.instruction("mov rbp, rsp");                                    // establish a stable helper frame
        // dest (rdi) is pushed like the inline fragment's caller did; the collector goes in rax,
        // and the start index stays in rdx across the push.
        abi::emit_push_reg(emitter, "rdi");
        emitter.instruction("mov rax, rsi");                                    // the collector rides in the result register
        append_argument_array_range(emitter, &mut labels, TailStart::Register("rdx"), &PhpType::Mixed);
        abi::emit_pop_reg(emitter, "rax");                                      // the grown destination is the result
        emitter.instruction("pop rbp");                                         // restore the caller frame pointer
        emitter.instruction("ret");                                             // return the destination array
    } else {
        emitter.instruction("stp x29, x30, [sp, #-16]!");                       // preserve frame linkage and the return address
        abi::emit_push_reg(emitter, "x0");
        emitter.instruction("mov x0, x1");                                      // the collector rides in the result register
        append_argument_array_range(emitter, &mut labels, TailStart::Register("x2"), &PhpType::Mixed);
        abi::emit_pop_reg(emitter, "x0");                                       // the grown destination is the result
        emitter.instruction("ldp x29, x30, [sp], #16");                         // restore frame linkage and the return address
        emitter.instruction("ret");                                             // return the destination array
    }

    emitter.blank();
    emitter.comment("--- runtime: backtrace_trim_args ---");
    emitter.label_global("__rt_backtrace_trim_args");
    if x86 {
        emitter.instruction("push rbp");                                        // preserve the caller frame pointer
        emitter.instruction("mov rbp, rsp");                                    // establish a stable helper frame
        emitter.instruction("mov rax, rdi");                                    // the array rides in the result register
        emitter.instruction("mov r10, rsi");                                    // the caller count rides in the secondary scratch register
        trim_argument_array_to_actual_count(emitter, &mut labels);
        emitter.instruction("pop rbp");                                         // restore the caller frame pointer
        emitter.instruction("ret");                                             // return the trimmed array
    } else {
        emitter.instruction("stp x29, x30, [sp, #-16]!");                       // preserve frame linkage and the return address
        emitter.instruction("mov x10, x1");                                     // the caller count rides in the secondary scratch register
        trim_argument_array_to_actual_count(emitter, &mut labels);
        emitter.instruction("ldp x29, x30, [sp], #16");                         // restore frame linkage and the return address
        emitter.instruction("ret");                                             // return the trimmed array
    }
    emit_frame_hash(emitter);
}

/// Emits `__rt_backtrace_frame_hash` for the active target.
///
/// `__rt_backtrace_frame_hash(file record, line, info record, this, args)` builds the
/// `debug_backtrace()` entry of one frame: a string-keyed hash holding `file`, `line`,
/// `function`, then `class` and `type` for a method, `object` when `this` is a box, and `args`
/// when the argument array is present (the hash takes ownership of both). The file record is a
/// `{ptr, len}` pair; the info record is `{function ptr, len, class ptr, len, type ptr, len}`
/// with a zero class pointer for a plain function. It returns the entry as an owned Mixed cell.
///
/// Every frame reader used to insert these keys itself, about 100 instructions per function.
fn emit_frame_hash(emitter: &mut Emitter) {
    let x86 = emitter.target.arch == Arch::X86_64;
    emitter.blank();
    emitter.comment("--- runtime: backtrace_frame_hash ---");
    emitter.label_global("__rt_backtrace_frame_hash");
    // State words: hash, file record, line, info record, this, args.
    let slot = |n: usize| -> String {
        if x86 {
            format!("QWORD PTR [rbp - {}]", (n + 1) * 8)
        } else {
            format!("[sp, #{}]", n * 8)
        }
    };
    if x86 {
        emitter.instruction("push rbp");                                        // preserve the caller frame pointer
        emitter.instruction("mov rbp, rsp");                                    // establish a stable helper frame
        emitter.instruction("sub rsp, 48");                                     // reserve the state words, keeping the stack 16-byte aligned
        emitter.instruction(&format!("mov {}, rdi", slot(1)));                  // save the file record
        emitter.instruction(&format!("mov {}, rsi", slot(2)));                  // save the line
        emitter.instruction(&format!("mov {}, rdx", slot(3)));                  // save the info record
        emitter.instruction(&format!("mov {}, rcx", slot(4)));                  // save the receiver box
        emitter.instruction(&format!("mov {}, r8", slot(5)));                   // save the argument array
        emitter.instruction("mov rdi, 8");                                      // initial hash capacity
        emitter.instruction("mov rsi, 7");                                      // the hash holds boxed Mixed values
        emitter.instruction("call __rt_hash_new");                              // allocate the entry
        emitter.instruction(&format!("mov {}, rax", slot(0)));                  // keep the entry
    } else {
        emitter.instruction("stp x29, x30, [sp, #-16]!");                       // preserve frame linkage and the return address
        emitter.instruction("sub sp, sp, #48");                                 // reserve the state words
        emitter.instruction("stp x0, x1, [sp, #8]");                            // save the file record and the line
        emitter.instruction("stp x2, x3, [sp, #24]");                           // save the info record and the receiver box
        emitter.instruction("str x4, [sp, #40]");                               // save the argument array
        emitter.instruction("mov x0, #8");                                      // initial hash capacity
        emitter.instruction("mov x1, #7");                                      // the hash holds boxed Mixed values
        emitter.instruction("bl __rt_hash_new");                                // allocate the entry
        emitter.instruction("str x0, [sp]");                                    // keep the entry
    }
    let insert = |emitter: &mut Emitter, key: &str, len: usize| {
        if x86 {
            emitter.instruction("mov rcx, rax");                                // pass the boxed value
            emitter.instruction("xor r8d, r8d");                                // string keys have no high payload word
            emitter.instruction(&format!("mov rdi, {}", slot(0)));              // pass the entry
            abi::emit_symbol_address(emitter, "rsi", key);
            emitter.instruction(&format!("mov rdx, {len}"));                    // pass the key length
            emitter.instruction("mov r9d, 7");                                  // the value is a boxed Mixed cell
            emitter.instruction("call __rt_hash_set");                          // store the value under the key
            emitter.instruction(&format!("mov {}, rax", slot(0)));              // the entry may have been reallocated
        } else {
            emitter.instruction("mov x3, x0");                                  // pass the boxed value
            emitter.instruction("mov x4, xzr");                                 // string keys have no high payload word
            emitter.instruction("ldr x0, [sp]");                                // pass the entry
            abi::emit_symbol_address(emitter, "x1", key);
            emitter.instruction(&format!("mov x2, #{len}"));                    // pass the key length
            emitter.instruction("mov x5, #7");                                  // the value is a boxed Mixed cell
            emitter.instruction("bl __rt_hash_set");                            // store the value under the key
            emitter.instruction("str x0, [sp]");                                // the entry may have been reallocated
        }
    };
    let load_record_string = |emitter: &mut Emitter, record_word: usize, offset: usize| {
        // Loads the {ptr, len} pair at `offset` of the record held in state word `record_word`
        // into the string result registers.
        if x86 {
            emitter.instruction(&format!("mov r10, {}", slot(record_word)));   // load the record
            emitter.instruction(&format!("mov rax, QWORD PTR [r10 + {offset}]")); // load the string pointer
            emitter.instruction(&format!("mov rdx, QWORD PTR [r10 + {}]", offset + 8)); // load the string length
        } else {
            emitter.instruction(&format!("ldr x9, {}", slot(record_word)));    // load the record
            emitter.instruction(&format!("ldp x1, x2, [x9, #{offset}]"));       // load the string pointer and length
        }
    };
    // file
    load_record_string(emitter, 1, 0);
    emit_box_current_value_as_mixed(emitter, &PhpType::Str);
    insert(emitter, "_bt_key_file", 4);
    // line
    if x86 {
        emitter.instruction(&format!("mov rax, {}", slot(2)));                  // load the line
    } else {
        emitter.instruction(&format!("ldr x0, {}", slot(2)));                   // load the line
    }
    emit_box_current_value_as_mixed(emitter, &PhpType::Int);
    insert(emitter, "_bt_key_line", 4);
    // function
    load_record_string(emitter, 3, 0);
    emit_box_current_value_as_mixed(emitter, &PhpType::Str);
    insert(emitter, "_bt_key_function", 8);
    // class and type, for a method only
    let no_class = "__rt_bt_frame_hash_no_class";
    if x86 {
        emitter.instruction(&format!("mov r10, {}", slot(3)));                  // load the info record
        emitter.instruction("cmp QWORD PTR [r10 + 16], 0");                     // is this a method frame?
        emitter.instruction(&format!("je {no_class}"));                         // a plain function has no class
    } else {
        emitter.instruction(&format!("ldr x9, {}", slot(3)));                   // load the info record
        emitter.instruction("ldr x10, [x9, #16]");                              // load the class pointer
        emitter.instruction(&format!("cbz x10, {no_class}"));                   // a plain function has no class
    }
    load_record_string(emitter, 3, 16);
    emit_box_current_value_as_mixed(emitter, &PhpType::Str);
    insert(emitter, "_bt_key_class", 5);
    // object (php lists it between class and type)
    let no_object = "__rt_bt_frame_hash_no_object";
    if x86 {
        emitter.instruction(&format!("mov rax, {}", slot(4)));                  // load the receiver box
        emitter.instruction("test rax, rax");                                   // was the receiver requested?
        emitter.instruction(&format!("jz {no_object}"));                        // no receiver entry
    } else {
        emitter.instruction(&format!("ldr x0, {}", slot(4)));                   // load the receiver box
        emitter.instruction(&format!("cbz x0, {no_object}"));                   // no receiver entry
    }
    insert(emitter, "_bt_key_object", 6);
    emitter.label(no_object);
    load_record_string(emitter, 3, 32);
    emit_box_current_value_as_mixed(emitter, &PhpType::Str);
    insert(emitter, "_bt_key_type", 4);
    emitter.label(no_class);
    // args
    let no_args = "__rt_bt_frame_hash_no_args";
    if x86 {
        emitter.instruction(&format!("mov rax, {}", slot(5)));                  // load the argument array
        emitter.instruction("test rax, rax");                                   // were arguments suppressed?
        emitter.instruction(&format!("jz {no_args}"));                          // no args entry
    } else {
        emitter.instruction(&format!("ldr x0, {}", slot(5)));                   // load the argument array
        emitter.instruction(&format!("cbz x0, {no_args}"));                     // no args entry
    }
    emit_box_current_owned_value_as_mixed(emitter, &PhpType::Array(Box::new(PhpType::Mixed)));
    insert(emitter, "_bt_key_args", 4);
    emitter.label(no_args);
    // The entry, boxed as an associative array.
    if x86 {
        emitter.instruction(&format!("mov rax, {}", slot(0)));                  // load the entry
    } else {
        emitter.instruction("ldr x0, [sp]");                                    // load the entry
    }
    emit_box_current_owned_value_as_mixed(
        emitter,
        &PhpType::AssocArray { key: Box::new(PhpType::Str), value: Box::new(PhpType::Mixed) },
    );
    if x86 {
        emitter.instruction("leave");                                           // release the state words and restore the caller frame pointer
        emitter.instruction("ret");                                             // return the entry
    } else {
        emitter.instruction("add sp, sp, #48");                                 // release the state words
        emitter.instruction("ldp x29, x30, [sp], #16");                         // restore frame linkage and the return address
        emitter.instruction("ret");                                             // return the entry
    }
}
