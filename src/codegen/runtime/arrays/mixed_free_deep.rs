//! Purpose:
//! Emits the `__rt_mixed_free_deep`, `__rt_mixed_free_deep_done` runtime helper assembly for mixed free deep.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Deep free helpers recursively release owned child storage and must match the heap kind/tag layout exactly.
//! - Tag 9 (resource) dispatches to a kind-specific destructor stored in the high payload word:
//!   kind 0 = generic/unknown (no destructor), kind 1 = native stream fd (close),
//!   kind 2 = HashContext (elephc_crypto_free), kind 3 = popen pipe (__rt_pclose,
//!   closes the FILE* and reaps the child), kind 4 = opendir stream (__rt_closedir).
//! - Each fd-backed kind skips handles >= 0x40000000: synthetic wrapper handles and
//!   the -1 sentinel written into the low payload word by an explicit close (see #4)
//!   so an already-released descriptor is never closed twice.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// mixed_free_deep: free a mixed cell and release its owned child payload.
/// Input: x0 = mixed cell pointer
/// Output: none
pub fn emit_mixed_free_deep(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_free_deep_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: mixed_free_deep ---");
    emitter.label_global("__rt_mixed_free_deep");

    emitter.instruction("cbz x0, __rt_mixed_free_deep_done");                   // skip null mixed cells immediately

    emitter.instruction("sub sp, sp, #32");                                     // allocate a small frame to preserve the mixed pointer

    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address

    emitter.instruction("add x29, sp, #16");                                    // set up the new frame pointer

    emitter.instruction("str x0, [sp, #0]");                                    // save the mixed pointer across child release

    emitter.instruction("ldr x9, [x0]");                                        // load the boxed runtime value_tag

    emitter.instruction("cmp x9, #1");                                          // is the boxed payload a string?

    emitter.instruction("b.eq __rt_mixed_free_deep_string");                    // strings release through heap_free_safe

    emitter.instruction("cmp x9, #4");                                          // does the boxed payload hold a heap-backed child?

    emitter.instruction("b.lo __rt_mixed_free_deep_box");                       // scalars/bools/floats/null need no nested release

    emitter.instruction("cmp x9, #7");                                          // do boxed heap-backed tags stay within the supported range?

    emitter.instruction("b.eq __rt_mixed_free_deep_value_any");                 // boxed mixed cells release through the uniform dispatcher

    emitter.instruction("cmp x9, #10");                                         // does the boxed payload hold a callable descriptor?

    emitter.instruction("b.eq __rt_mixed_free_deep_callable");                  // callable descriptors release through the descriptor helper

    emitter.instruction("cmp x9, #9");                                          // does the boxed payload hold a resource handle?

    emitter.instruction("b.eq __rt_mixed_free_deep_resource");                  // resources release through their kind-specific destructor

    emitter.instruction("cmp x9, #7");                                          // restore the heap-backed upper-bound comparison for array/hash/object tags

    emitter.instruction("b.hi __rt_mixed_free_deep_box");                       // unknown tags are ignored by mixed deep-free

    emitter.label("__rt_mixed_free_deep_value_any");
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the boxed heap child pointer

    emitter.instruction("bl __rt_decref_any");                                  // release the boxed child through the uniform dispatcher

    emitter.instruction("b __rt_mixed_free_deep_box");                          // free the mixed cell storage after releasing the child


    emitter.label("__rt_mixed_free_deep_callable");
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the boxed callable descriptor pointer

    emitter.instruction("bl __rt_callable_descriptor_release");                 // release the callable descriptor owned by the mixed cell

    emitter.instruction("b __rt_mixed_free_deep_box");                          // free the mixed cell storage after releasing the descriptor


    emitter.label("__rt_mixed_free_deep_resource");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the original Mixed cell pointer from the saved slot

    emitter.instruction("ldr x9, [x0, #16]");                                   // load the resource kind from the high payload word

    emitter.instruction("cbz x9, __rt_mixed_free_deep_box");                    // kind 0 = generic/unknown resource, no destructor

    emitter.instruction("cmp x9, #1");                                          // is the resource a native stream fd?

    emitter.instruction("b.eq __rt_mixed_free_deep_resource_stream");           // native streams need a close() syscall

    emitter.instruction("cmp x9, #2");                                          // is the resource a HashContext handle?

    emitter.instruction("b.eq __rt_mixed_free_deep_resource_hash");             // HashContext needs crypto_free

    emitter.instruction("cmp x9, #3");                                          // is the resource a popen pipe?

    emitter.instruction("b.eq __rt_mixed_free_deep_resource_popen");            // popen pipes close + reap the child via __rt_pclose

    emitter.instruction("cmp x9, #4");                                          // is the resource an opendir directory stream?

    emitter.instruction("b.eq __rt_mixed_free_deep_resource_dir");              // directory streams release their DIR* via __rt_closedir

    emitter.instruction("b __rt_mixed_free_deep_box");                          // unknown resource kind, free the box without destructor


    emitter.label("__rt_mixed_free_deep_resource_stream");
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the native fd from the low payload word

    emitter.instruction("mov x9, #0x40000000");                                 // load the synthetic/sentinel handle threshold into a scratch register

    emitter.instruction("cmp x0, x9");                                          // skip synthetic handles and the -1 sentinel left by an explicit close

    emitter.instruction("b.hs __rt_mixed_free_deep_box");                       // skip close for synthetic/already-closed handles

    emitter.syscall(6);                                                         // close(fd) — AArch64 macOS x16=6/svc #0x80, Linux remapped to x8=57/svc #0
    emitter.instruction("b __rt_mixed_free_deep_box");                          // free the mixed box after closing the native fd


    emitter.label("__rt_mixed_free_deep_resource_hash");
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the HashContext handle from the low payload word

    emitter.instruction("bl __rt_hash_ctx_free");                               // free a HashContext through the indirect crypto slot

    emitter.instruction("b __rt_mixed_free_deep_box");                          // free the mixed box after releasing the context


    emitter.label("__rt_mixed_free_deep_resource_popen");
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the pipe fd from the low payload word

    emitter.instruction("mov x9, #0x40000000");                                 // load the synthetic/sentinel handle threshold into a scratch register

    emitter.instruction("cmp x0, x9");                                          // skip the -1 sentinel left by an explicit pclose

    emitter.instruction("b.hs __rt_mixed_free_deep_box");                       // skip release for already-closed pipe handles

    emitter.instruction("bl __rt_pclose");                                      // pclose the pipe FILE* and reap the child process

    emitter.instruction("b __rt_mixed_free_deep_box");                          // free the mixed box after releasing the pipe


    emitter.label("__rt_mixed_free_deep_resource_dir");
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the directory fd from the low payload word

    emitter.instruction("mov x9, #0x40000000");                                 // load the synthetic/sentinel handle threshold into a scratch register

    emitter.instruction("cmp x0, x9");                                          // skip synthetic and the -1 sentinel left by an explicit closedir

    emitter.instruction("b.hs __rt_mixed_free_deep_box");                       // skip release for synthetic/already-closed directory handles

    emitter.instruction("bl __rt_closedir");                                    // closedir the DIR* recorded for this directory descriptor

    emitter.instruction("b __rt_mixed_free_deep_box");                          // free the mixed box after releasing the directory


    emitter.label("__rt_mixed_free_deep_string");
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the boxed string pointer

    emitter.instruction("bl __rt_heap_free_safe");                              // release the boxed string payload


    emitter.label("__rt_mixed_free_deep_box");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the mixed pointer after child release

    emitter.instruction("bl __rt_heap_free");                                   // free the mixed cell storage itself

    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address

    emitter.instruction("add sp, sp, #32");                                     // deallocate the mixed-free frame


    emitter.label("__rt_mixed_free_deep_done");
    emitter.instruction("ret");                                                 // return to caller

}

/// Emits the x86_64 Linux variant of `__rt_mixed_free_deep`.
/// Input: rax = mixed cell pointer
/// Output: none
/// ABI: preserves rbp, uses rax for input/output, calls `__rt_decref_any` and `__rt_heap_free` as needed.
fn emit_mixed_free_deep_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_free_deep ---");
    emitter.label_global("__rt_mixed_free_deep");

    emitter.instruction("test rax, rax");                                       // skip null mixed cells immediately because they do not own heap storage

    emitter.instruction("jz __rt_mixed_free_deep_done");                        // null mixed values need no release work

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before spilling the mixed pointer

    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved mixed pointer

    emitter.instruction("sub rsp, 16");                                         // reserve local storage for the mixed pointer across nested helper calls

    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the mixed pointer across any nested child release helper call

    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load the boxed runtime value tag to decide whether the child owns heap storage

    emitter.instruction("cmp r10, 1");                                          // detect string payloads that need their owned string storage released explicitly

    emitter.instruction("je __rt_mixed_free_deep_string");                      // string payloads release through heap_free_safe before the mixed box storage itself is freed

    emitter.instruction("cmp r10, 4");                                          // does the mixed cell point at a heap-backed child such as array/hash/object/mixed?

    emitter.instruction("jl __rt_mixed_free_deep_box");                         // scalar, bool, float, and null payloads can skip directly to freeing the mixed box storage itself

    emitter.instruction("cmp r10, 7");                                          // do the heap-backed child tags stay within the supported runtime range?

    emitter.instruction("je __rt_mixed_free_deep_value_any");                   // boxed mixed cells release through the uniform dispatcher

    emitter.instruction("cmp r10, 10");                                         // does the boxed payload hold a callable descriptor?

    emitter.instruction("je __rt_mixed_free_deep_callable");                    // callable descriptors release through the descriptor helper

    emitter.instruction("cmp r10, 9");                                          // does the boxed payload hold a resource handle?

    emitter.instruction("je __rt_mixed_free_deep_resource");                    // resources release through their kind-specific destructor

    emitter.instruction("cmp r10, 7");                                          // restore the heap-backed upper-bound comparison for array/hash/object tags

    emitter.instruction("jg __rt_mixed_free_deep_box");                         // unknown tags are ignored by the current x86_64 mixed deep-free helper

    emitter.label("__rt_mixed_free_deep_value_any");
    emitter.instruction("mov rax, QWORD PTR [rax + 8]");                        // load the boxed string pointer from the mixed payload before releasing it

    emitter.instruction("call __rt_decref_any");                                // release the boxed heap-backed child through the uniform x86_64 dispatcher before freeing the mixed box

    emitter.instruction("jmp __rt_mixed_free_deep_box");                        // free the mixed box storage itself after the boxed heap-backed child has been released


    emitter.label("__rt_mixed_free_deep_callable");
    emitter.instruction("mov rax, QWORD PTR [rax + 8]");                        // load the boxed callable descriptor pointer from the mixed payload

    emitter.instruction("call __rt_callable_descriptor_release");               // release the callable descriptor owned by the mixed cell

    emitter.instruction("jmp __rt_mixed_free_deep_box");                        // free the mixed box storage itself after the descriptor has been released


    emitter.label("__rt_mixed_free_deep_resource");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the original Mixed cell pointer from the saved slot

    emitter.instruction("mov r9, QWORD PTR [rax + 16]");                        // load the resource kind from the high payload word

    emitter.instruction("test r9, r9");                                         // kind 0 = generic/unknown resource?

    emitter.instruction("jz __rt_mixed_free_deep_box");                         // no destructor for generic resources

    emitter.instruction("cmp r9, 1");                                           // is the resource a native stream fd?

    emitter.instruction("je __rt_mixed_free_deep_resource_stream");             // native streams need close()

    emitter.instruction("cmp r9, 2");                                           // is the resource a HashContext handle?

    emitter.instruction("je __rt_mixed_free_deep_resource_hash");               // HashContext needs crypto_free

    emitter.instruction("cmp r9, 3");                                           // is the resource a popen pipe?

    emitter.instruction("je __rt_mixed_free_deep_resource_popen");              // popen pipes close + reap the child via __rt_pclose

    emitter.instruction("cmp r9, 4");                                           // is the resource an opendir directory stream?

    emitter.instruction("je __rt_mixed_free_deep_resource_dir");                // directory streams release their DIR* via __rt_closedir

    emitter.instruction("jmp __rt_mixed_free_deep_box");                        // unknown resource kind, free the box without destructor


    emitter.label("__rt_mixed_free_deep_resource_stream");
    emitter.instruction("mov rdi, QWORD PTR [rax + 8]");                        // load the native fd from the low payload word into the close argument

    emitter.instruction("cmp rdi, 0x40000000");                                 // synthetic/sentinel handle threshold (-1 marks an explicit close)

    emitter.instruction("jae __rt_mixed_free_deep_box");                        // skip synthetic/already-closed handles

    emitter.instruction("call close");                                          // close(fd) via the C library on x86_64 Linux

    emitter.instruction("jmp __rt_mixed_free_deep_box");                        // free the mixed box after closing the native fd


    emitter.label("__rt_mixed_free_deep_resource_hash");
    emitter.instruction("mov rdi, QWORD PTR [rax + 8]");                        // load the HashContext handle from the low payload word

    emitter.instruction("call __rt_hash_ctx_free");                             // free a HashContext through the indirect crypto slot

    emitter.instruction("jmp __rt_mixed_free_deep_box");                        // free the mixed box after releasing the context


    emitter.label("__rt_mixed_free_deep_resource_popen");
    emitter.instruction("mov rdi, QWORD PTR [rax + 8]");                        // load the pipe fd from the low payload word

    emitter.instruction("cmp rdi, 0x40000000");                                 // sentinel(-1)/synthetic handle threshold

    emitter.instruction("jae __rt_mixed_free_deep_box");                        // skip release for already-closed pipe handles

    emitter.instruction("call __rt_pclose");                                    // pclose the pipe FILE* and reap the child process

    emitter.instruction("jmp __rt_mixed_free_deep_box");                        // free the mixed box after releasing the pipe


    emitter.label("__rt_mixed_free_deep_resource_dir");
    emitter.instruction("mov rdi, QWORD PTR [rax + 8]");                        // load the directory fd from the low payload word

    emitter.instruction("cmp rdi, 0x40000000");                                 // sentinel(-1)/synthetic handle threshold

    emitter.instruction("jae __rt_mixed_free_deep_box");                        // skip release for synthetic/already-closed directory handles

    emitter.instruction("call __rt_closedir");                                  // closedir the DIR* recorded for this directory descriptor

    emitter.instruction("jmp __rt_mixed_free_deep_box");                        // free the mixed box after releasing the directory


    emitter.label("__rt_mixed_free_deep_string");
    emitter.instruction("mov rax, QWORD PTR [rax + 8]");                        // load the boxed string pointer from the mixed payload before releasing it

    emitter.instruction("call __rt_heap_free_safe");                            // release the boxed string payload when the mixed cell owns a persisted string


    emitter.label("__rt_mixed_free_deep_box");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the mixed pointer after the optional child release helper call

    emitter.instruction("call __rt_heap_free");                                 // release the mixed box storage itself through the shared x86_64 heap wrapper

    emitter.instruction("add rsp, 16");                                         // release the spill slot reserved for the mixed pointer

    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning

    emitter.label("__rt_mixed_free_deep_done");
    emitter.instruction("ret");                                                 // return to the caller after releasing the mixed box and its optional string child

}
