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
//!   elephc_curl bridge), kind 7 = CurlMultiHandle (__rt_curl_multi_free), kind 8 =
//!   CurlShareHandle/CurlSharePersistentHandle (__rt_curl_share_free — a documented no-op
//!   for the persistent case, see `crates/elephc-curl/src/share.rs`). Kind 6 is what makes
//!   a `CurlHandle` OBJECT's ordinary teardown close its transfer: the object holds the
//!   cell in a property, property release reaches here, and this ladder is the only free
//!   path for the native handle.
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
//! - SINCE THE REGISTRY LANDED: tag 9 releases registry-owned kinds 1, 3, 4 and 9
//!   through `__rt_resource_release`, which owns the backend-specific destructor and
//!   rejects stale opaque handles. Kind 2 remains the legacy raw HashContext and still
//!   releases directly through `__rt_hash_ctx_free`. The kind-5 reservation below still
//!   holds: it must never gain an arm here.

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

    emitter.instruction("cmp x9, #1");                                          // is this a registry-owned native stream handle?

    emitter.instruction("b.eq __rt_mixed_free_deep_resource_registry");         // release native streams through the authoritative registry

    emitter.instruction("cmp x9, #2");                                          // is the resource a HashContext handle?

    emitter.instruction("b.eq __rt_mixed_free_deep_resource_hash");             // HashContext needs crypto_free

    emitter.instruction("cmp x9, #3");                                          // is this a registry-owned popen stream handle?

    emitter.instruction("b.eq __rt_mixed_free_deep_resource_registry");         // release popen streams through the authoritative registry

    emitter.instruction("cmp x9, #4");                                          // is this a registry-owned directory stream handle?

    emitter.instruction("b.eq __rt_mixed_free_deep_resource_registry");         // release directory streams through the authoritative registry

    emitter.instruction("cmp x9, #9");                                          // is this a registry-owned stream-context handle?

    emitter.instruction("b.eq __rt_mixed_free_deep_resource_registry");         // release contexts through the authoritative registry

    emitter.instruction("cmp x9, #6");                                          // is the resource a libcurl easy handle?

    emitter.instruction("b.eq __rt_mixed_free_deep_resource_curl");             // CurlHandle needs curl_easy_cleanup via the elephc_curl bridge

    emitter.instruction("cmp x9, #7");                                          // is the resource a libcurl multi handle?

    emitter.instruction("b.eq __rt_mixed_free_deep_resource_curl_multi");       // CurlMultiHandle needs curl_multi_cleanup via the elephc_curl bridge

    emitter.instruction("cmp x9, #8");                                          // is the resource a libcurl share handle?

    emitter.instruction("b.eq __rt_mixed_free_deep_resource_curl_share");       // CurlShareHandle needs curl_share_cleanup via the elephc_curl bridge (a no-op if persistent)

    emitter.instruction("b __rt_mixed_free_deep_box");                          // unknown resource kind, free the box without destructor


    emitter.label("__rt_mixed_free_deep_resource_registry");
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the opaque resource handle from the low payload word

    emitter.instruction("bl __rt_resource_release");                            // release the Mixed owner's registry reference and close at refcount zero

    emitter.instruction("b __rt_mixed_free_deep_box");                          // free the mixed box after releasing its registry ownership

    emitter.label("__rt_mixed_free_deep_resource_hash");
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the HashContext handle from the low payload word

    emitter.instruction("bl __rt_hash_ctx_free");                               // free a HashContext through the indirect crypto slot

    emitter.instruction("b __rt_mixed_free_deep_box");                          // free the mixed box after releasing the context


    emitter.label("__rt_mixed_free_deep_resource_curl");
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the libcurl handle id from the low payload word

    emitter.instruction("bl __rt_curl_easy_free");                              // release the easy handle through the indirect curl slot

    emitter.instruction("b __rt_mixed_free_deep_box");                          // free the mixed box after releasing the handle


    emitter.label("__rt_mixed_free_deep_resource_curl_multi");
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the libcurl multi handle id from the low payload word

    emitter.instruction("bl __rt_curl_multi_free");                             // release the multi handle through the indirect curl slot

    emitter.instruction("b __rt_mixed_free_deep_box");                          // free the mixed box after releasing the handle


    emitter.label("__rt_mixed_free_deep_resource_curl_share");
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the libcurl share handle id from the low payload word

    emitter.instruction("bl __rt_curl_share_free");                             // release the share handle through the indirect curl slot (no-op if persistent)

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
// `features` gated the per-kind destructor arms upstream. It is inert since the resource
// REGISTRY landed: kinds 1, 3, 4 and 9 all release through `__rt_resource_release`, so there is
// no per-kind arm left to leave out. Kept in the signature because the dispatcher and the tests
// pass it, and because the two implementations are still to be reconciled.
fn emit_mixed_free_deep_linux_x86_64(emitter: &mut Emitter, _features: RuntimeFeatures) {
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

    emitter.instruction("cmp r9, 1");                                           // is this a registry-owned native stream handle?

    emitter.instruction("je __rt_mixed_free_deep_resource_registry");           // release native streams through the authoritative registry

    emitter.instruction("cmp r9, 2");                                           // is the resource a HashContext handle?

    emitter.instruction("je __rt_mixed_free_deep_resource_hash");               // HashContext needs crypto_free

    emitter.instruction("cmp r9, 3");                                           // is this a registry-owned popen stream handle?

    emitter.instruction("je __rt_mixed_free_deep_resource_registry");           // release popen streams through the authoritative registry

    emitter.instruction("cmp r9, 4");                                           // is this a registry-owned directory stream handle?

    emitter.instruction("je __rt_mixed_free_deep_resource_registry");           // release directory streams through the authoritative registry

    emitter.instruction("cmp r9, 9");                                           // is this a registry-owned stream-context handle?

    emitter.instruction("je __rt_mixed_free_deep_resource_registry");           // release contexts through the authoritative registry

    emitter.instruction("cmp r9, 6");                                           // is the resource a libcurl easy handle?

    emitter.instruction("je __rt_mixed_free_deep_resource_curl");               // CurlHandle needs curl_easy_cleanup via the elephc_curl bridge

    emitter.instruction("cmp r9, 7");                                           // is the resource a libcurl multi handle?

    emitter.instruction("je __rt_mixed_free_deep_resource_curl_multi");         // CurlMultiHandle needs curl_multi_cleanup via the elephc_curl bridge

    emitter.instruction("cmp r9, 8");                                           // is the resource a libcurl share handle?

    emitter.instruction("je __rt_mixed_free_deep_resource_curl_share");         // CurlShareHandle needs curl_share_cleanup via the elephc_curl bridge (a no-op if persistent)

    emitter.instruction("jmp __rt_mixed_free_deep_box");                        // unknown resource kind, free the box without destructor


    emitter.label("__rt_mixed_free_deep_resource_registry");
    emitter.instruction("mov rdi, QWORD PTR [rax + 8]");                        // load the opaque resource handle from the low payload word

    emitter.instruction("call __rt_resource_release");                          // release the Mixed owner's registry reference and close at refcount zero

    emitter.instruction("jmp __rt_mixed_free_deep_box");                        // free the mixed box after releasing its registry ownership

    emitter.label("__rt_mixed_free_deep_resource_hash");
    emitter.instruction("mov rdi, QWORD PTR [rax + 8]");                        // load the HashContext handle from the low payload word

    emitter.instruction("call __rt_hash_ctx_free");                             // free a HashContext through the indirect crypto slot

    emitter.instruction("jmp __rt_mixed_free_deep_box");                        // free the mixed box after releasing the context


    emitter.label("__rt_mixed_free_deep_resource_curl");
    emitter.instruction("mov rdi, QWORD PTR [rax + 8]");                        // load the libcurl handle id from the low payload word

    emitter.instruction("call __rt_curl_easy_free");                            // release the easy handle through the indirect curl slot

    emitter.instruction("jmp __rt_mixed_free_deep_box");                        // free the mixed box after releasing the handle


    emitter.label("__rt_mixed_free_deep_resource_curl_multi");
    emitter.instruction("mov rdi, QWORD PTR [rax + 8]");                        // load the libcurl multi handle id from the low payload word

    emitter.instruction("call __rt_curl_multi_free");                           // release the multi handle through the indirect curl slot

    emitter.instruction("jmp __rt_mixed_free_deep_box");                        // free the mixed box after releasing the handle


    emitter.label("__rt_mixed_free_deep_resource_curl_share");
    emitter.instruction("mov rdi, QWORD PTR [rax + 8]");                        // load the libcurl share handle id from the low payload word

    emitter.instruction("call __rt_curl_share_free");                           // release the share handle through the indirect curl slot (no-op if persistent)

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

    /// One target's ladder shapes: how it spells a comparison and the branch each arm takes.
    struct LadderShapes {
        platform: Platform,
        arch: Arch,
        compare: &'static str,
        registry_branch: &'static str,
        hash_branch: &'static str,
    }

    const LADDERS: &[LadderShapes] = &[
        LadderShapes {
            platform: Platform::MacOS,
            arch: Arch::AArch64,
            compare: "cmp x9, #",
            registry_branch: "b.eq __rt_mixed_free_deep_resource_registry\n",
            hash_branch: "b.eq __rt_mixed_free_deep_resource_hash\n",
        },
        LadderShapes {
            platform: Platform::Linux,
            arch: Arch::X86_64,
            compare: "cmp r9, ",
            registry_branch: "je __rt_mixed_free_deep_resource_registry\n",
            hash_branch: "je __rt_mixed_free_deep_resource_hash\n",
        },
    ];

    /// Emits the helper for one target and feature set.
    fn emit_for(shapes: &LadderShapes, features: RuntimeFeatures) -> String {
        let mut emitter = Emitter::new(Target::new(shapes.platform, shapes.arch));
        emit_mixed_free_deep(&mut emitter, features);
        emitter.output()
    }

    /// Every registry-owned resource kind releases through the REGISTRY, on both targets.
    ///
    /// A close has to be refcounted, exact-once and lifecycle-published, and
    /// `__rt_resource_release` is the one helper that does all three. A per-kind arm calling
    /// `__rt_pclose` or `__rt_closedir` straight from this ladder bypasses the registry's
    /// bookkeeping, which is what makes the destination — not merely "some destructor" — the
    /// property worth pinning.
    ///
    /// The HashContext kind keeps its own arm because it is not a registry resource: it is
    /// stamped by the runtime helper `__rt_hash_init` rather than by any lowering, and closes
    /// through `crypto_free`.
    #[test]
    fn every_registry_owned_kind_releases_through_the_registry() {
        for shapes in LADDERS {
            let arch = shapes.arch;
            let asm = emit_for(shapes, RuntimeFeatures::all());
            for kind in [1, 3, 4, 9] {
                let dispatch = format!("{}{}\n    {}", shapes.compare, kind, shapes.registry_branch);
                assert!(
                    asm.contains(&dispatch),
                    "{arch:?}: resource kind {kind} must release through the registry:\n{asm}"
                );
            }
            let hash = format!("{}2\n    {}", shapes.compare, shapes.hash_branch);
            assert!(
                asm.contains(&hash),
                "{arch:?}: the HashContext kind keeps its own arm, it is not a registry resource:\n{asm}"
            );
            assert!(
                asm.contains("__rt_resource_release"),
                "{arch:?}: the registry arm must call the registry:\n{asm}"
            );
            assert!(
                asm.contains("__rt_mixed_free_deep_box:\n"),
                "{arch:?}: the generic box-free path must exist for kind 0:\n{asm}"
            );
        }
    }

    /// The ladder tests the SAME number the lowering stamps.
    ///
    /// `RuntimeFnId::resource_cleanup_kind` is the one authority: the lowering stamps `stamp()`
    /// into the Mixed high payload word, and this ladder compares against it. Nothing else ties
    /// this emitter's literals to it, so renumbering a kind on one side only is caught here
    /// rather than by a resource that quietly stops being released.
    #[test]
    fn each_arm_matches_the_kind_its_producer_stamps() {
        for shapes in LADDERS {
            let arch = shapes.arch;
            let asm = emit_for(shapes, RuntimeFeatures::all());
            for kind in [
                ResourceCleanupKind::StreamFd,
                ResourceCleanupKind::PopenPipe,
                ResourceCleanupKind::Directory,
            ] {
                let dispatch = format!(
                    "{}{}\n    {}",
                    shapes.compare,
                    kind.stamp(),
                    shapes.registry_branch
                );
                assert!(
                    asm.contains(&dispatch),
                    "{arch:?}: {kind:?} must dispatch on the kind its producer stamps ({}), \
                     not on some other number:\n{asm}",
                    kind.stamp()
                );
            }
        }
    }

    /// The ladder does not depend on the feature set, and says so rather than pretending to.
    ///
    /// Upstream gated a per-kind destructor arm on `popen_resource` / `directory_resource`. The
    /// registry replaced those arms, so there is nothing left in this emitter for a bit to
    /// select and the two feature sets must produce the SAME assembly. Asserting it keeps the
    /// inert parameter honest: a future gate added here without a test would otherwise pass
    /// unnoticed, and a gate on this ladder is exactly what must not come back.
    ///
    /// The pay-for-use property still holds one level up, MEASURED on this tree: a program of
    /// arrays and scalars references neither this helper nor `__rt_resource_release`, and a
    /// program that opens a stream reaches `__rt_pclose` through `__rt_stream_close_backend` —
    /// the path `fclose()` takes anyway, which no gate here could have avoided.
    #[test]
    fn the_ladder_is_the_same_whatever_the_feature_set() {
        for shapes in LADDERS {
            let arch = shapes.arch;
            assert_eq!(
                emit_for(shapes, RuntimeFeatures::none()),
                emit_for(shapes, RuntimeFeatures::all()),
                "{arch:?}: the resource ladder must not depend on a feature bit"
            );
        }
    }
}
