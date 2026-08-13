//! Purpose:
//! Emits the `__rt_mixed_free_deep`, `__rt_mixed_free_deep_done` runtime helper assembly for mixed free deep.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Deep free helpers recursively release owned child storage and must match the heap kind/tag layout exactly.
//! - Tag 9 (resource) dispatches to a kind-specific destructor stored in the high payload word:
//!   kind 0 = generic/unknown (no destructor), kind 1 = native stream fd (close),
//!   kind 2 = HashContext (elephc_crypto_free), kind 3 = popen pipe (__rt_pclose,
//!   closes the FILE* and reaps the child), kind 4 = opendir stream (__rt_closedir),
//!   kind 6 = CurlHandle (__rt_curl_easy_free, which runs curl_easy_cleanup through the
//!   elephc_curl bridge). Kind 6 is what makes a `CurlHandle` OBJECT's ordinary teardown
//!   close its transfer: the object holds the cell in a property, property release
//!   reaches here, and this ladder is the only free path for the native handle.
//! - KIND 5 IS RESERVED AND MUST NEVER GAIN AN ARM HERE. It is the eval-owned inert
//!   hash-context handle boxed by `__elephc_eval_value_hash_context`, and its low
//!   payload word is NOT a pointer: it is a key into
//!   `elephc_magician::stream_resources::EvalStreamResources` offset by
//!   `EVAL_RESOURCE_PAYLOAD_BASE` (`1 << 62`). The real `elephc_crypto` handle behind
//!   it is owned by `EvalHashContext` and released by its `Drop`, so freeing anything
//!   from here would be a double free of the context and a wild free of the key. Kind 5
//!   deliberately falls off the end of the ladder into `__rt_mixed_free_deep_box`.
//!   A future resource kind must therefore take 7 or higher (6 is CurlHandle).
//! - Each fd-backed kind skips handles >= 0x40000000: synthetic wrapper handles and
//!   the -1 sentinel written into the low payload word by an explicit close (see #4)
//!   so an already-released descriptor is never closed twice.
//! - The kind 3 and kind 4 arms are PAY-FOR-USE. Each is emitted only when the lowered
//!   program calls the one builtin that can produce that kind, which
//!   `RuntimeFnId::resource_cleanup_kind` names. They were the sole reference to
//!   `__rt_pclose` and `__rt_closedir`, so every binary imported `pclose`, `closedir`,
//!   `globfree` and `close` to release handles it had no way to open. Kinds 1 and 2 are
//!   NOT gated: kind 1 closes with a raw syscall on AArch64 and imports nothing, and
//!   kind 2 is stamped by the runtime helper `__rt_hash_init` rather than by a lowering,
//!   so no EIR call names it.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::RuntimeFeatures;

/// mixed_free_deep: free a mixed cell and release its owned child payload.
/// Input: x0 = mixed cell pointer
/// Output: none
pub fn emit_mixed_free_deep(emitter: &mut Emitter, features: RuntimeFeatures) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_free_deep_linux_x86_64(emitter, features);
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

    if features.popen_resource {
        emitter.instruction("cmp x9, #3");                                      // is the resource a popen pipe?

        emitter.instruction("b.eq __rt_mixed_free_deep_resource_popen");        // popen pipes close + reap the child via __rt_pclose
    }

    if features.directory_resource {
        emitter.instruction("cmp x9, #4");                                      // is the resource an opendir directory stream?

        emitter.instruction("b.eq __rt_mixed_free_deep_resource_dir");          // directory streams release their DIR* via __rt_closedir
    }

    emitter.instruction("cmp x9, #6");                                          // is the resource a libcurl easy handle?

    emitter.instruction("b.eq __rt_mixed_free_deep_resource_curl");             // CurlHandle needs curl_easy_cleanup via the elephc_curl bridge

    emitter.instruction("cmp x9, #7");                                          // is the resource a libcurl multi handle?

    emitter.instruction("b.eq __rt_mixed_free_deep_resource_curl_multi");       // CurlMultiHandle needs curl_multi_cleanup via the elephc_curl bridge

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


    if features.popen_resource {
        emitter.label("__rt_mixed_free_deep_resource_popen");
        emitter.instruction("ldr x0, [x0, #8]");                                // load the pipe fd from the low payload word

        emitter.instruction("mov x9, #0x40000000");                             // load the synthetic/sentinel handle threshold into a scratch register

        emitter.instruction("cmp x0, x9");                                      // skip the -1 sentinel left by an explicit pclose

        emitter.instruction("b.hs __rt_mixed_free_deep_box");                   // skip release for already-closed pipe handles

        emitter.instruction("bl __rt_pclose");                                  // pclose the pipe FILE* and reap the child process

        emitter.instruction("b __rt_mixed_free_deep_box");                      // free the mixed box after releasing the pipe
    }


    if features.directory_resource {
        emitter.label("__rt_mixed_free_deep_resource_dir");
        emitter.instruction("ldr x0, [x0, #8]");                                // load the directory fd from the low payload word

        emitter.instruction("mov x9, #0x40000000");                             // load the synthetic/sentinel handle threshold into a scratch register

        emitter.instruction("cmp x0, x9");                                      // skip synthetic and the -1 sentinel left by an explicit closedir

        emitter.instruction("b.hs __rt_mixed_free_deep_box");                   // skip release for synthetic/already-closed directory handles

        emitter.instruction("bl __rt_closedir");                                // closedir the DIR* recorded for this directory descriptor

        emitter.instruction("b __rt_mixed_free_deep_box");                      // free the mixed box after releasing the directory
    }


    emitter.label("__rt_mixed_free_deep_resource_curl");
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the libcurl handle id from the low payload word

    emitter.instruction("bl __rt_curl_easy_free");                              // release the easy handle through the indirect curl slot

    emitter.instruction("b __rt_mixed_free_deep_box");                          // free the mixed box after releasing the handle


    emitter.label("__rt_mixed_free_deep_resource_curl_multi");
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the libcurl multi handle id from the low payload word

    emitter.instruction("bl __rt_curl_multi_free");                             // release the multi handle through the indirect curl slot

    emitter.instruction("b __rt_mixed_free_deep_box");                          // free the mixed box after releasing the handle


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
fn emit_mixed_free_deep_linux_x86_64(emitter: &mut Emitter, features: RuntimeFeatures) {
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

    if features.popen_resource {
        emitter.instruction("cmp r9, 3");                                       // is the resource a popen pipe?

        emitter.instruction("je __rt_mixed_free_deep_resource_popen");          // popen pipes close + reap the child via __rt_pclose
    }

    if features.directory_resource {
        emitter.instruction("cmp r9, 4");                                       // is the resource an opendir directory stream?

        emitter.instruction("je __rt_mixed_free_deep_resource_dir");            // directory streams release their DIR* via __rt_closedir
    }

    emitter.instruction("cmp r9, 6");                                           // is the resource a libcurl easy handle?

    emitter.instruction("je __rt_mixed_free_deep_resource_curl");               // CurlHandle needs curl_easy_cleanup via the elephc_curl bridge

    emitter.instruction("cmp r9, 7");                                           // is the resource a libcurl multi handle?

    emitter.instruction("je __rt_mixed_free_deep_resource_curl_multi");         // CurlMultiHandle needs curl_multi_cleanup via the elephc_curl bridge

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


    if features.popen_resource {
        emitter.label("__rt_mixed_free_deep_resource_popen");
        emitter.instruction("mov rdi, QWORD PTR [rax + 8]");                    // load the pipe fd from the low payload word

        emitter.instruction("cmp rdi, 0x40000000");                             // sentinel(-1)/synthetic handle threshold

        emitter.instruction("jae __rt_mixed_free_deep_box");                    // skip release for already-closed pipe handles

        emitter.instruction("call __rt_pclose");                                // pclose the pipe FILE* and reap the child process

        emitter.instruction("jmp __rt_mixed_free_deep_box");                    // free the mixed box after releasing the pipe
    }


    if features.directory_resource {
        emitter.label("__rt_mixed_free_deep_resource_dir");
        emitter.instruction("mov rdi, QWORD PTR [rax + 8]");                    // load the directory fd from the low payload word

        emitter.instruction("cmp rdi, 0x40000000");                             // sentinel(-1)/synthetic handle threshold

        emitter.instruction("jae __rt_mixed_free_deep_box");                    // skip release for synthetic/already-closed directory handles

        emitter.instruction("call __rt_closedir");                              // closedir the DIR* recorded for this directory descriptor

        emitter.instruction("jmp __rt_mixed_free_deep_box");                    // free the mixed box after releasing the directory
    }


    emitter.label("__rt_mixed_free_deep_resource_curl");
    emitter.instruction("mov rdi, QWORD PTR [rax + 8]");                        // load the libcurl handle id from the low payload word

    emitter.instruction("call __rt_curl_easy_free");                            // release the easy handle through the indirect curl slot

    emitter.instruction("jmp __rt_mixed_free_deep_box");                        // free the mixed box after releasing the handle


    emitter.label("__rt_mixed_free_deep_resource_curl_multi");
    emitter.instruction("mov rdi, QWORD PTR [rax + 8]");                        // load the libcurl multi handle id from the low payload word

    emitter.instruction("call __rt_curl_multi_free");                           // release the multi handle through the indirect curl slot

    emitter.instruction("jmp __rt_mixed_free_deep_box");                        // free the mixed box after releasing the handle


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

#[cfg(test)]
mod tests {
    use crate::codegen_support::emit::Emitter;
    use crate::codegen_support::platform::{Arch, Platform, Target};
    use crate::ir::ResourceCleanupKind;

    use super::*;

    /// One target's ladder shapes: the dispatch test and the branch taken by each gated arm.
    struct LadderShapes {
        platform: Platform,
        arch: Arch,
        compare: &'static str,
        popen_branch: &'static str,
        dir_branch: &'static str,
    }

    const LADDERS: &[LadderShapes] = &[
        LadderShapes {
            platform: Platform::MacOS,
            arch: Arch::AArch64,
            compare: "cmp x9, #",
            popen_branch: "b.eq __rt_mixed_free_deep_resource_popen\n",
            dir_branch: "b.eq __rt_mixed_free_deep_resource_dir\n",
        },
        LadderShapes {
            platform: Platform::Linux,
            arch: Arch::X86_64,
            compare: "cmp r9, ",
            popen_branch: "je __rt_mixed_free_deep_resource_popen\n",
            dir_branch: "je __rt_mixed_free_deep_resource_dir\n",
        },
    ];

    /// Emits the helper for one target and feature set.
    fn emit_for(shapes: &LadderShapes, features: RuntimeFeatures) -> String {
        let mut emitter = Emitter::new(Target::new(shapes.platform, shapes.arch));
        emit_mixed_free_deep(&mut emitter, features);
        emitter.output()
    }

    /// Returns a feature set with only the named resource kinds enabled.
    fn only(popen: bool, directory: bool) -> RuntimeFeatures {
        RuntimeFeatures {
            popen_resource: popen,
            directory_resource: directory,
            ..RuntimeFeatures::none()
        }
    }

    /// The two kind-specific destructor arms appear only for a program whose EIR can produce
    /// that kind, and each one follows its OWN bit.
    ///
    /// Both directions are checked, because an "is it absent" assertion alone passes just as
    /// well against an emitter that has stopped emitting anything at all. The two bits are then
    /// checked independently: they are separate producers, and a gate written as one shared
    /// condition would still satisfy a both-on/both-off test.
    #[test]
    fn the_kind_specific_destructor_arms_follow_their_producers() {
        for shapes in LADDERS {
            let arch = shapes.arch;

            let wide = emit_for(shapes, RuntimeFeatures::all());
            assert!(wide.contains(shapes.popen_branch), "{arch:?}: popen arm must be emitted");
            assert!(wide.contains(shapes.dir_branch), "{arch:?}: directory arm must be emitted");
            assert!(
                wide.contains("__rt_pclose"),
                "{arch:?}: the popen arm is the only runtime reference to the pclose helper"
            );
            assert!(
                wide.contains("__rt_closedir"),
                "{arch:?}: the directory arm is the only runtime reference to the closedir helper"
            );

            let narrow = emit_for(shapes, RuntimeFeatures::none());
            assert!(!narrow.contains(shapes.popen_branch), "{arch:?}: popen arm must be gone");
            assert!(!narrow.contains(shapes.dir_branch), "{arch:?}: directory arm must be gone");
            assert!(
                !narrow.contains("__rt_pclose"),
                "{arch:?}: nothing may still reach the pclose helper, or pclose stays imported"
            );
            assert!(
                !narrow.contains("__rt_closedir"),
                "{arch:?}: nothing may still reach the closedir helper, or closedir, globfree \
                 and close stay imported"
            );

            let popen_only = emit_for(shapes, only(true, false));
            assert!(
                popen_only.contains(shapes.popen_branch) && !popen_only.contains(shapes.dir_branch),
                "{arch:?}: the popen bit must select the popen arm alone"
            );

            let dir_only = emit_for(shapes, only(false, true));
            assert!(
                dir_only.contains(shapes.dir_branch) && !dir_only.contains(shapes.popen_branch),
                "{arch:?}: the directory bit must select the directory arm alone"
            );
        }
    }

    /// The kinds with no producing builtin are NOT gated and must survive the narrowest build.
    ///
    /// Kind 1 closes with a raw syscall on AArch64 and imports nothing, and kind 2 is stamped by
    /// the runtime helper `__rt_hash_init` rather than by a lowering, so no EIR call names it and
    /// no feature bit could honestly carry it. Gating either by mistake would leak a descriptor or
    /// a hash context in exactly the programs that never mention a directory or a pipe.
    #[test]
    fn the_ungated_kinds_survive_the_narrowest_build() {
        for shapes in LADDERS {
            let arch = shapes.arch;
            let narrow = emit_for(shapes, RuntimeFeatures::none());
            assert!(
                narrow.contains("__rt_mixed_free_deep_resource_stream:\n"),
                "{arch:?}: the kind 1 stream arm is not gated"
            );
            assert!(
                narrow.contains("__rt_hash_ctx_free"),
                "{arch:?}: the kind 2 hash-context arm is not gated"
            );
            assert!(
                narrow.contains("__rt_mixed_free_deep_box:\n"),
                "{arch:?}: the generic box-free path must survive the gating"
            );
        }
    }

    /// The ladder tests the SAME number the lowering stamps.
    ///
    /// `RuntimeFnId::resource_cleanup_kind` is the one authority: the lowering stamps `stamp()`
    /// into the Mixed high payload word, and `lowered_runtime_features` turns that same answer
    /// into the bit gating the arm here. Nothing else ties this emitter's literal to it, so
    /// renumbering a kind on one side only is caught here rather than by a resource that quietly
    /// stops being released.
    #[test]
    fn each_arm_matches_the_kind_its_producer_stamps() {
        for shapes in LADDERS {
            let arch = shapes.arch;
            let wide = emit_for(shapes, RuntimeFeatures::all());
            for (kind, branch) in [
                (ResourceCleanupKind::PopenPipe, shapes.popen_branch),
                (ResourceCleanupKind::Directory, shapes.dir_branch),
            ] {
                let dispatch = format!("{}{}\n    {}", shapes.compare, kind.stamp(), branch);
                assert!(
                    wide.contains(&dispatch),
                    "{arch:?}: {kind:?} must dispatch on the kind its producer stamps ({}), \
                     not on some other number:\n{wide}",
                    kind.stamp()
                );
            }
        }
    }
}
