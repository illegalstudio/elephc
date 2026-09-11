//! Purpose:
//! Emits validation and collection helpers for dynamic PHP call-unpack sources.
//! Keeps ordered key checks and owned Mixed value copies shared across supported targets.
//!
//! Called from:
//! - EIR `call_argument.validate_unpack`, `collect_positionals`, and `collect_named` lowering.
//!
//! Key details:
//! - Validation completes before either destination is mutated.
//! - Numeric entries are reindexed into the positional accumulator while string keys are copied
//!   into the named accumulator, preserving source insertion order within each group.
//! - Every destination value is a fresh owned Mixed cell; source containers remain borrowed.
//! - Descriptor ref-cell marker cells remain markers when cloned, so later invocation can still
//!   publish native by-reference updates instead of dereferencing them as ordinary PHP reads.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the three call-unpack runtime helpers for the active target.
pub fn emit_call_argument_unpack(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the AArch64 validation and disjoint collection loops.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: call_argument_validate_unpack ---");
    emitter.label_global("__rt_call_argument_validate_unpack");

    // Frame slots: named hash 0, source payload 8, cursor 16, saw-named 24, key 32/40,
    // saved frame pointer and return address 48/56.
    emitter.instruction("sub sp, sp, #64");                                     // reserve validation walk state and linkage
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #48");                                    // establish a stable validation frame
    emitter.instruction("str x0, [sp, #0]");                                    // save the borrowed named accumulator
    emitter.instruction("mov x0, x1");                                          // move the boxed source into mixed_deref's input
    emitter.instruction("bl __rt_mixed_deref");                                 // follow nested Mixed and reference wrappers
    emitter.instruction("cbz x0, __rt_call_argument_validate_null");            // an absent cell is the canonical null rejection
    emitter.instruction("ldr x1, [x0]");                                        // load the concrete runtime source tag for validation
    emitter.instruction("ldr x2, [x0, #8]");                                    // load the low payload for bool wording or array traversal
    emitter.instruction("cmp x1, #4");                                          // packed arrays contain only positional keys
    emitter.instruction("b.eq __rt_call_argument_validate_packed");             // reject them only after an earlier source contributed names
    emitter.instruction("cmp x1, #5");                                          // hashes may mix positional and named keys
    emitter.instruction("b.ne __rt_call_argument_validate_non_array");          // reject every other concrete Mixed tag
    emitter.instruction("str x2, [sp, #8]");                                    // save the borrowed source hash payload
    emitter.instruction("str xzr, [sp, #16]");                                  // initialize its insertion-order cursor
    emitter.instruction("str xzr, [sp, #24]");                                  // no named key has appeared in this source yet

    emitter.label("__rt_call_argument_validate_loop");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the borrowed source hash
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload the insertion-order cursor
    emitter.instruction("bl __rt_hash_iter_next");                              // fetch the next key and borrowed value tuple
    emitter.instruction("cmn x0, #1");                                          // did the ordered iterator report exhaustion?
    emitter.instruction("b.eq __rt_call_argument_validate_ok");                 // every key in this source passed validation
    emitter.instruction("str x0, [sp, #16]");                                   // save the next insertion-order cursor
    emitter.instruction("cmn x2, #1");                                          // is this an integer key?
    emitter.instruction("b.ne __rt_call_argument_validate_named");              // string keys participate in duplicate-name checks
    emitter.instruction("ldr x9, [sp, #24]");                                   // load whether this source already yielded a named key
    emitter.instruction("tbnz x9, #0, __rt_call_argument_validate_positional_after_named"); // reject positional entries after a named entry in one source
    emitter.instruction("ldr x9, [sp, #0]");                                    // inspect names accumulated by earlier unpack sources
    emitter.instruction("ldr x9, [x9]");                                        // load the number of earlier named arguments
    emitter.instruction("cbnz x9, __rt_call_argument_validate_cross_source_order"); // reject this first offending positional key immediately
    emitter.instruction("b __rt_call_argument_validate_loop");                  // accept this positional entry and continue

    emitter.label("__rt_call_argument_validate_named");
    emitter.instruction("mov x9, #1");                                          // remember that this source has entered its named-key suffix
    emitter.instruction("str x9, [sp, #24]");                                   // persist the per-source named-key state
    emitter.instruction("stp x1, x2, [sp, #32]");                               // preserve the borrowed key across the destination lookup
    emitter.instruction("ldr x0, [sp, #0]");                                    // load the named accumulator for duplicate detection
    emitter.instruction("bl __rt_hash_get");                                    // probe for a name inserted by an earlier source
    emitter.instruction("cbz x0, __rt_call_argument_validate_loop");            // a new name is valid and will be collected later
    emitter.instruction("ldp x1, x2, [sp, #32]");                               // return the duplicate key pointer and byte length
    emitter.instruction("mov x0, #2");                                          // status two denotes a duplicate named argument
    emitter.instruction("b __rt_call_argument_validate_done");                  // return without mutating either accumulator

    emitter.label("__rt_call_argument_validate_null");
    emitter.instruction("mov x1, #8");                                          // runtime tag eight names canonical PHP null
    emitter.instruction("mov x2, #0");                                          // canonical null has no low payload
    emitter.label("__rt_call_argument_validate_non_array");
    emitter.instruction("mov x0, #1");                                          // status one carries the rejected runtime tag and payload
    emitter.instruction("b __rt_call_argument_validate_done");                  // return the non-array status to exception lowering

    emitter.label("__rt_call_argument_validate_positional_after_named");
    emitter.instruction("mov x0, #3");                                          // status three identifies invalid within-source key order
    emitter.instruction("b __rt_call_argument_validate_done");                  // return before either collector can mutate state

    emitter.label("__rt_call_argument_validate_packed");
    emitter.instruction("ldr x9, [x2]");                                        // load the packed source length before considering cross-source ordering
    emitter.instruction("cbz x9, __rt_call_argument_validate_ok");              // an empty source contributes no positional argument after earlier names
    emitter.instruction("ldr x9, [sp, #0]");                                    // inspect names accumulated by earlier unpack sources
    emitter.instruction("ldr x9, [x9]");                                        // load the number of earlier named arguments
    emitter.instruction("cbz x9, __rt_call_argument_validate_ok");              // a positional-first dynamic ordering remains representable
    emitter.instruction("b __rt_call_argument_validate_cross_source_order");    // reject positional arguments after earlier named sources

    emitter.label("__rt_call_argument_validate_cross_source_order");
    emitter.instruction("mov x0, #4");                                          // status four reports the unsupported cross-source ordering
    emitter.instruction("b __rt_call_argument_validate_done");                  // return before either collector can mutate state

    emitter.label("__rt_call_argument_validate_ok");
    emitter.instruction("mov x0, #0");                                          // status zero means this source is safe to collect
    emitter.label("__rt_call_argument_validate_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #64");                                     // release validation spill state
    emitter.instruction("ret");                                                 // return status and optional detail words

    emitter.blank();
    emitter.comment("--- runtime: call_argument_collect ---");
    emitter.label_global("__rt_call_argument_collect_positionals");
    emitter.instruction("mov x2, #0");                                          // select integer-key collection with sequential reindexing
    emitter.instruction("b __rt_call_argument_collect_entry");                  // share the ordered value-copy loop
    emitter.label_global("__rt_call_argument_collect_named");
    emitter.instruction("mov x2, #1");                                          // select string-key collection with key preservation
    emitter.label_shared("__rt_call_argument_collect_entry");

    // Frame slots: destination 0, source payload 8, cursor 16, mode 24, key 32/40,
    // value tag/low/high 48/56/64, saved frame pointer and return address 80/88.
    emitter.instruction("sub sp, sp, #96");                                     // reserve disjoint collection state and linkage
    emitter.instruction("stp x29, x30, [sp, #80]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #80");                                    // establish a stable collector frame
    emitter.instruction("str x0, [sp, #0]");                                    // save the private destination accumulator
    emitter.instruction("str x2, [sp, #24]");                                   // save positional or named collection mode
    emitter.instruction("mov x0, x1");                                          // move the boxed source into mixed_deref's input
    emitter.instruction("bl __rt_mixed_deref");                                 // follow nested Mixed and reference wrappers
    emitter.instruction("ldr x9, [x0, #8]");                                    // load the validated packed or hash payload pointer
    emitter.instruction("str x9, [sp, #8]");                                    // preserve the borrowed source payload
    emitter.instruction("str xzr, [sp, #16]");                                  // initialize the uniform insertion-order cursor

    emitter.label("__rt_call_argument_collect_loop");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the borrowed source array or hash
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload the uniform iterator cursor
    emitter.instruction("bl __rt_array_iter_next");                             // fetch the next key and borrowed runtime value tuple
    emitter.instruction("cmn x0, #1");                                          // did the uniform iterator report exhaustion?
    emitter.instruction("b.eq __rt_call_argument_collect_done");                // return the current destination after the full scan
    emitter.instruction("str x0, [sp, #16]");                                   // save the next insertion-order cursor
    emitter.instruction("stp x1, x2, [sp, #32]");                               // preserve the borrowed key across value cloning
    emitter.instruction("stp x3, x4, [sp, #48]");                               // save value tag and low payload word
    emitter.instruction("str x5, [sp, #64]");                                   // save the borrowed value high payload word
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload positional or named collection mode
    emitter.instruction("cmn x2, #1");                                          // classify the source key as integer or string
    emitter.instruction("b.eq __rt_call_argument_collect_integer");             // integer keys belong only in the positional accumulator
    emitter.instruction("cbz x9, __rt_call_argument_collect_loop");             // positional collection skips string-keyed entries
    emitter.instruction("b __rt_call_argument_collect_clone");                  // named collection preserves this string key

    emitter.label("__rt_call_argument_collect_integer");
    emitter.instruction("cbnz x9, __rt_call_argument_collect_loop");            // named collection skips integer-keyed entries

    emitter.label("__rt_call_argument_collect_clone");
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the borrowed source value tag
    emitter.instruction("cmp x9, #7");                                          // is the source entry already a boxed Mixed cell?
    emitter.instruction("b.ne __rt_call_argument_collect_box");                 // concrete values need retaining box construction
    emitter.instruction("ldr x0, [sp, #56]");                                   // pass the borrowed Mixed cell to the authoritative clone helper
    emitter.instruction("bl __rt_mixed_clone");                                 // detach references and return one owned Mixed cell
    emitter.instruction("b __rt_call_argument_collect_insert");                 // insert the cloned cell into the private destination

    emitter.label("__rt_call_argument_collect_box");
    emitter.instruction("ldr x0, [sp, #48]");                                   // pass the concrete runtime tag to mixed_from_value
    emitter.instruction("ldr x1, [sp, #56]");                                   // pass the borrowed low payload word
    emitter.instruction("ldr x2, [sp, #64]");                                   // pass the borrowed high payload word
    emitter.instruction("bl __rt_mixed_from_value");                            // retain or persist the payload in one owned Mixed cell

    emitter.label("__rt_call_argument_collect_insert");
    emitter.instruction("mov x3, x0");                                          // transfer the owned Mixed cell as the hash value low word
    emitter.instruction("mov x4, #0");                                          // boxed Mixed hash values have no high payload word
    emitter.instruction("mov x5, #7");                                          // destination hashes store pointers to boxed Mixed cells
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the private destination accumulator
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload positional or named collection mode
    emitter.instruction("cbnz x9, __rt_call_argument_collect_named_key");       // named collection preserves the source string key
    emitter.instruction("ldr x1, [x0]");                                        // next positional key equals the current accumulator count
    emitter.instruction("mov x2, #-1");                                         // key high sentinel marks the generated integer key
    emitter.instruction("b __rt_call_argument_collect_set");                    // insert the next positional argument
    emitter.label("__rt_call_argument_collect_named_key");
    emitter.instruction("ldp x1, x2, [sp, #32]");                               // restore the borrowed source string key
    emitter.label("__rt_call_argument_collect_set");
    emitter.instruction("bl __rt_hash_set");                                    // transfer the owned Mixed cell into the private hash
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the current destination pointer after growth
    emitter.instruction("b __rt_call_argument_collect_loop");                   // continue scanning the validated source

    emitter.label("__rt_call_argument_collect_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // return the current destination pointer for SSA writeback
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #96");                                     // release collector spill state
    emitter.instruction("ret");                                                 // return to generated call-argument lowering
}

/// Emits the x86_64 System V validation and disjoint collection loops.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: call_argument_validate_unpack ---");
    emitter.label_global("__rt_call_argument_validate_unpack");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable validation frame
    emitter.instruction("sub rsp, 48");                                         // reserve named/source/cursor/key spill slots with call alignment
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the borrowed named accumulator
    emitter.instruction("mov rax, rsi");                                        // move the boxed source into mixed_deref's input
    emitter.instruction("call __rt_mixed_deref");                               // follow nested Mixed and reference wrappers
    emitter.instruction("test rax, rax");                                       // did dereference produce an absent cell?
    emitter.instruction("jz __rt_call_argument_validate_null");                 // an absent cell is the canonical null rejection
    emitter.instruction("mov rdi, QWORD PTR [rax]");                            // load the concrete runtime source tag
    emitter.instruction("mov rsi, QWORD PTR [rax + 8]");                        // load the low payload for bool wording or traversal
    emitter.instruction("cmp rdi, 4");                                          // packed arrays contain only positional keys
    emitter.instruction("je __rt_call_argument_validate_packed");               // reject them only after an earlier source contributed names
    emitter.instruction("cmp rdi, 5");                                          // hashes may mix positional and named keys
    emitter.instruction("jne __rt_call_argument_validate_non_array");           // reject every other concrete Mixed tag
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the borrowed source hash payload
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // initialize its insertion-order cursor
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // no named key has appeared in this source yet

    emitter.label("__rt_call_argument_validate_loop");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the borrowed source hash
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // reload the insertion-order cursor
    emitter.instruction("call __rt_hash_iter_next");                            // fetch the next key and borrowed value tuple
    emitter.instruction("cmp rax, -1");                                         // did the ordered iterator report exhaustion?
    emitter.instruction("je __rt_call_argument_validate_ok");                   // every key in this source passed validation
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the next insertion-order cursor
    emitter.instruction("cmp rdx, -1");                                         // is this an integer key?
    emitter.instruction("jne __rt_call_argument_validate_named");               // string keys participate in duplicate-name checks
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // load whether this source already yielded a named key
    emitter.instruction("test r11, 1");                                         // has this source already yielded a named key?
    emitter.instruction("jnz __rt_call_argument_validate_positional_after_named"); // reject positional entries after a named entry in one source
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // inspect names accumulated by earlier unpack sources
    emitter.instruction("cmp QWORD PTR [r11], 0");                              // does the named accumulator already contain an entry?
    emitter.instruction("jne __rt_call_argument_validate_cross_source_order");  // reject this first offending positional key immediately
    emitter.instruction("jmp __rt_call_argument_validate_loop");                // accept this positional entry and continue

    emitter.label("__rt_call_argument_validate_named");
    emitter.instruction("mov QWORD PTR [rbp - 32], 1");                         // record the per-source named-key suffix
    emitter.instruction("mov QWORD PTR [rbp - 40], rdi");                       // preserve the borrowed key pointer across lookup
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // preserve the borrowed key length across lookup
    emitter.instruction("mov rsi, rdi");                                        // move the key pointer into hash_get's second argument
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // load the named accumulator for duplicate detection
    emitter.instruction("call __rt_hash_get");                                  // probe for a name inserted by an earlier source
    emitter.instruction("test rax, rax");                                       // did the named accumulator contain this key?
    emitter.instruction("jz __rt_call_argument_validate_loop");                 // a new name is valid and will be collected later
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // return the duplicate key pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // return the duplicate key byte length
    emitter.instruction("mov rax, 2");                                          // status two denotes a duplicate named argument
    emitter.instruction("jmp __rt_call_argument_validate_done");                // return without mutating either accumulator

    emitter.label("__rt_call_argument_validate_null");
    emitter.instruction("mov rdi, 8");                                          // runtime tag eight names canonical PHP null
    emitter.instruction("xor esi, esi");                                        // canonical null has no low payload
    emitter.label("__rt_call_argument_validate_non_array");
    emitter.instruction("mov rax, 1");                                          // status one carries the rejected runtime tag and payload
    emitter.instruction("jmp __rt_call_argument_validate_done");                // return the non-array status to exception lowering

    emitter.label("__rt_call_argument_validate_positional_after_named");
    emitter.instruction("mov rax, 3");                                          // status three identifies invalid within-source key order
    emitter.instruction("jmp __rt_call_argument_validate_done");                // return before either collector can mutate state

    emitter.label("__rt_call_argument_validate_packed");
    emitter.instruction("cmp QWORD PTR [rsi], 0");                              // inspect the packed source length before cross-source ordering
    emitter.instruction("je __rt_call_argument_validate_ok");                   // an empty source contributes no positional argument after earlier names
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // inspect names accumulated by earlier unpack sources
    emitter.instruction("cmp QWORD PTR [r11], 0");                              // does the named accumulator already contain an entry?
    emitter.instruction("je __rt_call_argument_validate_ok");                   // a positional-first dynamic ordering remains representable
    emitter.instruction("jmp __rt_call_argument_validate_cross_source_order");  // reject positional arguments after earlier named sources

    emitter.label("__rt_call_argument_validate_cross_source_order");
    emitter.instruction("mov rax, 4");                                          // status four reports the unsupported cross-source ordering
    emitter.instruction("jmp __rt_call_argument_validate_done");                // return before either collector can mutate state

    emitter.label("__rt_call_argument_validate_ok");
    emitter.instruction("xor eax, eax");                                        // status zero means this source is safe to collect
    emitter.label("__rt_call_argument_validate_done");
    emitter.instruction("mov rsp, rbp");                                        // release validation spill state
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return status and optional detail words

    emitter.blank();
    emitter.comment("--- runtime: call_argument_collect ---");
    emitter.label_global("__rt_call_argument_collect_positionals");
    emitter.instruction("xor edx, edx");                                        // select integer-key collection with sequential reindexing
    emitter.instruction("jmp __rt_call_argument_collect_entry");                // share the ordered value-copy loop
    emitter.label_global("__rt_call_argument_collect_named");
    emitter.instruction("mov edx, 1");                                          // select string-key collection with key preservation
    emitter.label_shared("__rt_call_argument_collect_entry");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable collector frame
    emitter.instruction("sub rsp, 80");                                         // reserve collection state with System V call alignment
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the private destination accumulator
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // save positional or named collection mode
    emitter.instruction("mov rax, rsi");                                        // move the boxed source into mixed_deref's input
    emitter.instruction("call __rt_mixed_deref");                               // follow nested Mixed and reference wrappers
    emitter.instruction("mov r11, QWORD PTR [rax + 8]");                        // load the validated packed or hash payload pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // preserve the borrowed source payload
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // initialize the uniform insertion-order cursor

    emitter.label("__rt_call_argument_collect_loop");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the borrowed source array or hash
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // reload the uniform iterator cursor
    emitter.instruction("call __rt_array_iter_next");                           // fetch the next key and borrowed runtime value tuple
    emitter.instruction("cmp rax, -1");                                         // did the uniform iterator report exhaustion?
    emitter.instruction("je __rt_call_argument_collect_done");                  // return the current destination after the full scan
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the next insertion-order cursor
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // preserve the borrowed source key pointer or integer
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // preserve the source key length or integer sentinel
    emitter.instruction("mov QWORD PTR [rbp - 56], r8");                        // save the borrowed source value tag
    emitter.instruction("mov QWORD PTR [rbp - 64], r9");                        // save the borrowed source value low word
    emitter.instruction("mov QWORD PTR [rbp - 72], r10");                       // save the borrowed source value high word
    emitter.instruction("cmp rdx, -1");                                         // classify the source key as integer or string
    emitter.instruction("je __rt_call_argument_collect_integer");               // integer keys belong only in the positional accumulator
    emitter.instruction("cmp QWORD PTR [rbp - 32], 0");                         // is this the positional-only collection pass?
    emitter.instruction("je __rt_call_argument_collect_loop");                  // positional collection skips string-keyed entries
    emitter.instruction("jmp __rt_call_argument_collect_clone");                // named collection preserves this string key

    emitter.label("__rt_call_argument_collect_integer");
    emitter.instruction("cmp QWORD PTR [rbp - 32], 0");                         // is this the named-only collection pass?
    emitter.instruction("jne __rt_call_argument_collect_loop");                 // named collection skips integer-keyed entries

    emitter.label("__rt_call_argument_collect_clone");
    emitter.instruction("cmp QWORD PTR [rbp - 56], 7");                         // is the source entry already a boxed Mixed cell?
    emitter.instruction("jne __rt_call_argument_collect_box");                  // concrete values need retaining box construction
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // pass the borrowed Mixed cell to the authoritative clone helper
    emitter.instruction("call __rt_mixed_clone");                               // detach references and return one owned Mixed cell
    emitter.instruction("jmp __rt_call_argument_collect_insert");               // insert the cloned cell into the private destination

    emitter.label("__rt_call_argument_collect_box");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // pass the concrete runtime tag to mixed_from_value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 64]");                       // pass the borrowed low payload word
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // pass the borrowed high payload word
    emitter.instruction("call __rt_mixed_from_value");                          // retain or persist the payload in one owned Mixed cell

    emitter.label("__rt_call_argument_collect_insert");
    emitter.instruction("mov rcx, rax");                                        // transfer the owned Mixed cell as the hash value low word
    emitter.instruction("xor r8d, r8d");                                        // boxed Mixed hash values have no high payload word
    emitter.instruction("mov r9d, 7");                                          // destination hashes store pointers to boxed Mixed cells
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the private destination accumulator
    emitter.instruction("cmp QWORD PTR [rbp - 32], 0");                         // choose generated positional or preserved named key
    emitter.instruction("jne __rt_call_argument_collect_named_key");            // named collection preserves the source string key
    emitter.instruction("mov rsi, QWORD PTR [rdi]");                            // next positional key equals the current accumulator count
    emitter.instruction("mov rdx, -1");                                         // key high sentinel marks the generated integer key
    emitter.instruction("jmp __rt_call_argument_collect_set");                  // insert the next positional argument
    emitter.label("__rt_call_argument_collect_named_key");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // restore the borrowed source string key pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // restore the borrowed source string key length
    emitter.label("__rt_call_argument_collect_set");
    emitter.instruction("call __rt_hash_set");                                  // transfer the owned Mixed cell into the private hash
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the current destination pointer after growth
    emitter.instruction("jmp __rt_call_argument_collect_loop");                 // continue scanning the validated source

    emitter.label("__rt_call_argument_collect_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the current destination pointer for SSA writeback
    emitter.instruction("mov rsp, rbp");                                        // release collector spill state
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to generated call-argument lowering
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Arch, Platform, Target};
    use std::process::Command;

    /// Cross-assembles the complete validator and collectors after PIC label localization.
    #[test]
    #[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
    fn call_argument_unpack_helpers_assemble_on_all_supported_targets() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "call-argument-unpack-targets-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&directory).unwrap();
        for (name, triple) in [
            ("linux-x86_64", "x86_64-linux-gnu"),
            ("linux-aarch64", "aarch64-linux-gnu"),
            ("macos-aarch64", "arm64-apple-macos11"),
            ("ios-arm64", "arm64-apple-ios13"),
            ("ios-sim-arm64", "arm64-apple-ios13-simulator"),
        ] {
            let target = Target::parse(name).unwrap();
            let localize = target.platform == Platform::MacOS;
            let mut emitter = Emitter::new_pic(target);
            emitter.dead_strip = localize;
            if target.arch == Arch::X86_64 {
                emitter.raw(".intel_syntax noprefix");
            }
            emitter.raw(".text");
            emit_call_argument_unpack(&mut emitter);
            if localize {
                emitter.raw(".subsections_via_symbols");
            } else {
                emitter.raw(".section .note.GNU-stack,\"\",@progbits");
            }
            let internal_labels = emitter.take_internal_labels();
            let assembly = emitter.output();
            let assembly = if localize {
                crate::codegen_support::emit::localize_internal_labels(
                    &assembly,
                    &internal_labels,
                )
            } else {
                assembly
            };
            let source = directory.join(format!("{name}.s"));
            let object = directory.join(format!("{name}.o"));
            std::fs::write(&source, assembly).unwrap();
            let built = Command::new("clang")
                .args(["-target", triple, "-c"])
                .arg(&source)
                .arg("-o")
                .arg(object)
                .output()
                .unwrap();
            assert!(
                built.status.success(),
                "{name}: {}",
                String::from_utf8_lossy(&built.stderr)
            );
        }
        std::fs::remove_dir_all(directory).unwrap();
    }
}
