//! Purpose:
//! Emits the `__rt_mixed_from_value`, `__rt_mixed_from_value_string` runtime helper assembly for mixed from value.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Mixed helpers use boxed tag/payload cells; tag constants and ownership rules are shared with type checking and codegen.
//! - Null or sentinel payloads presented with a container-shaped tag (4–6) are
//!   normalized to the canonical Mixed null tag before ownership or allocation.
//! - Tag 9 (resource) takes no ownership, but it is this function that guarantees
//!   every boxed resource carries a PHP resource id: the tag-9 arm calls
//!   `__rt_resource_id_of`, which binds the next id when the native payload has
//!   none yet and leaves an existing binding alone. Boxing is the one point every
//!   resource passes through — `fopen`, `opendir`, `popen`, the stream filters, the
//!   eval bridge and `zval_unpack` alike — so ids follow creation order without each
//!   of those sites having to opt in, while `$b = $a` re-boxing the same payload keeps
//!   the id it already had. Creation sites that can hand back a RECYCLED native payload
//!   (a descriptor number the kernel reissued after `fclose`) call
//!   `__rt_resource_id_mint` first, which overwrites instead of preserving; see
//!   `runtime::resource_ids`.
//! - FIVE RESOURCE KINDS ARE EXCLUDED. Two are incremental-hash contexts; the other three
//!   are libcurl handles (easy, multi, share). All five are OBJECT-backed in PHP 8, so none
//!   of them may consume anything from the resource counter. PHP 8
//!   makes `hash_init()` return a `HashContext` OBJECT (`crate::hash_prelude`), so the
//!   context belongs to the OBJECT handle space and must consume nothing from the
//!   resource counter — otherwise every `fopen()` in a hashing program would report an
//!   id one higher than PHP's. The cell is still tag 9 because that is the shape the
//!   hash helpers read back; only the id binding is skipped. It is safe to skip because
//!   these cells live in an object property or an eval-local scope and never reach a
//!   display path: should one ever be reached anyway, `__rt_resource_id_of` still mints
//!   lazily, so no path can print a raw address.
//!   - KIND 2 is the HOST context. Its low payload word IS the malloc'd
//!     `elephc_crypto` handle, and the tag-9 cell OWNS it: `__rt_mixed_free_deep`
//!     routes kind 2 to `__rt_hash_ctx_free`.
//!   - KIND 5 is the EVAL context, and it is INERT — id-less AND destructor-less.
//!     `eval()`'s hash builtins keep the real `elephc_crypto` handle inside
//!     `elephc_magician::stream_resources::EvalHashContext`, which frees it in its own
//!     `Drop`; the boxed low word is only a table KEY carrying
//!     `EVAL_RESOURCE_PAYLOAD_BASE` (`1 << 62`). That is exactly why kind 5 exists
//!     instead of reusing kind 2: handing `0x4000000000000000 + n` to
//!     `elephc_crypto_free` would be a wild free. Kind 5 has no arm in
//!     `__rt_mixed_free_deep`'s kind ladder and falls through to the plain box free.
//!   - KIND 6 is a `CurlHandle`. Its low payload word is the `elephc_curl` bridge's own
//!     `i64` handle id, and the tag-9 cell OWNS it: `__rt_mixed_free_deep` routes kind 6
//!     to `__rt_curl_easy_free`. PHP 8 migrated curl sessions from resources to the
//!     `CurlHandle` OBJECT exactly as it did `HashContext`, so it is excluded here for
//!     exactly the reason kind 2 is — otherwise every `fopen()` in a curl program would
//!     report an id one higher than PHP's.
//!   - KIND 7 is a `CurlMultiHandle`, kind 8 a `CurlShareHandle`/`CurlSharePersistentHandle`
//!     — the identical OBJECT-not-resource reasoning kind 6 documents, for the identical
//!     `fopen()`-id-shift reason.
//! - SINCE THE REGISTRY LANDED: for registry-owned stream kinds 1, 3, 4 and 9 the tag-9
//!   arm also retains the opaque handle through `__rt_resource_retain`, making boxing the
//!   ownership boundary shared by `fopen`, `opendir`, `popen`, stream contexts and every
//!   later re-boxing. Legacy kind-0 resources keep only the display-id bind.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::sentinels::emit_branch_if_null_container;

/// Retains or persists a runtime value and boxes it into a Mixed cell.
///
/// Container-shaped tags with a zero or null-sentinel payload are normalized to
/// canonical PHP null before retain/allocation, preventing malformed boxes from
/// escaping into downstream consumers.
/// Input:  x0=value_tag, x1=value_lo, x2=value_hi
/// Output: x0=boxed mixed pointer
pub fn emit_mixed_from_value(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_from_value_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: mixed_from_value ---");
    emitter.label_global("__rt_mixed_from_value");

    emitter.instruction("sub sp, sp, #48");                                     // allocate stack frame for the incoming payload and boxed result
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the runtime value tag across helper calls
    emitter.instruction("stp x1, x2, [sp, #8]");                                // save the incoming payload words across helper calls

    emitter.instruction("cmp x0, #4");                                          // do only container-shaped tags need null-payload normalization?
    emitter.instruction("b.lt __rt_mixed_from_value_dispatch");                 // scalar tags preserve every payload bit pattern
    emitter.instruction("cmp x0, #6");                                          // indexed arrays, hashes, and objects are the container-shaped tags
    emitter.instruction("b.gt __rt_mixed_from_value_dispatch");                 // nested Mixed/callable tags use ordinary pointer ownership
    emit_branch_if_null_container(
        emitter,
        "x1",
        "x9",
        "__rt_mixed_from_value_null_container",
    );
    emitter.label("__rt_mixed_from_value_dispatch");
    emitter.instruction("cmp x0, #1");                                          // does this mixed payload hold a string?
    emitter.instruction("b.eq __rt_mixed_from_value_string");                   // strings must be persisted for the boxed owner
    emitter.instruction("cmp x0, #4");                                          // does this mixed payload hold an indexed array?
    emitter.instruction("b.eq __rt_mixed_from_value_retain");                   // refcounted child pointers must be retained for the boxed owner
    emitter.instruction("cmp x0, #5");                                          // does this mixed payload hold an associative array?
    emitter.instruction("b.eq __rt_mixed_from_value_retain");                   // refcounted child pointers must be retained for the boxed owner
    emitter.instruction("cmp x0, #6");                                          // does this mixed payload hold an object?
    emitter.instruction("b.eq __rt_mixed_from_value_retain");                   // refcounted child pointers must be retained for the boxed owner
    emitter.instruction("cmp x0, #7");                                          // does this mixed payload hold another mixed cell?
    emitter.instruction("b.eq __rt_mixed_from_value_retain");                   // nested mixed cells must also be retained
    emitter.instruction("cmp x0, #10");                                         // does this mixed payload hold a callable descriptor?
    emitter.instruction("b.eq __rt_mixed_from_value_retain");                   // callable descriptors are retained for the boxed owner
    emitter.instruction("cmp x0, #9");                                          // does this mixed payload hold a PHP resource?
    emitter.instruction("b.eq __rt_mixed_from_value_resource");                 // resources need a display id bound before they can be shown
    emitter.instruction("b __rt_mixed_from_value_alloc");                       // scalars can be boxed without additional retention

    emitter.label("__rt_mixed_from_value_resource");
    emitter.instruction("cmp x2, #2");                                          // resource kind 2 = the raw incremental-hash context
    emitter.instruction("b.eq __rt_mixed_from_value_alloc");                    // a HashContext is an OBJECT in PHP and must consume no resource id
    emitter.instruction("cmp x2, #5");                                          // resource kind 5 = eval's inert handle on the same HashContext
    emitter.instruction("b.eq __rt_mixed_from_value_alloc");                    // eval hashing must not shift the ids the host program's fopen() reports
    emitter.instruction("cmp x2, #6");                                          // resource kind 6 = a libcurl easy handle
    emitter.instruction("b.eq __rt_mixed_from_value_alloc");                    // a CurlHandle is an OBJECT in PHP and must consume no resource id
    emitter.instruction("cmp x2, #7");                                          // resource kind 7 = a libcurl multi handle
    emitter.instruction("b.eq __rt_mixed_from_value_alloc");                    // a CurlMultiHandle is an OBJECT in PHP and must consume no resource id
    emitter.instruction("cmp x2, #8");                                          // resource kind 8 = a libcurl share handle
    emitter.instruction("b.eq __rt_mixed_from_value_alloc");                    // a CurlShareHandle/CurlSharePersistentHandle is an OBJECT in PHP and must consume no resource id
    emitter.instruction("mov x0, x1");                                          // move the native resource payload into the registry argument
    emitter.instruction("bl __rt_resource_id_of");                              // bind a display id if this payload does not already have one
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the resource kind after display-id binding
    emitter.instruction("cmp x9, #1");                                          // kind 1 = registry-owned native stream
    emitter.instruction("b.eq __rt_mixed_from_value_resource_retain");          // the Mixed box becomes another owner of the stream handle
    emitter.instruction("cmp x9, #3");                                          // kind 3 = registry-owned process pipe
    emitter.instruction("b.eq __rt_mixed_from_value_resource_retain");          // retain the popen stream handle for the Mixed owner
    emitter.instruction("cmp x9, #4");                                          // kind 4 = registry-owned directory stream
    emitter.instruction("b.eq __rt_mixed_from_value_resource_retain");          // retain the directory stream handle for the Mixed owner
    emitter.instruction("cmp x9, #9");                                          // kind 9 = registry-owned stream context
    emitter.instruction("b.eq __rt_mixed_from_value_resource_retain");          // retain the context handle for the Mixed owner
    emitter.instruction("b __rt_mixed_from_value_alloc");                       // legacy resource kinds remain outside the stream registry for now

    emitter.label("__rt_mixed_from_value_resource_retain");
    emitter.instruction("ldr x0, [sp, #8]");                                    // pass the opaque resource handle saved before display-id binding
    emitter.instruction("bl __rt_resource_retain");                             // retain the registry entry for ownership by the new Mixed cell
    emitter.instruction("b __rt_mixed_from_value_alloc");                       // allocate the box after the registry retain succeeds

    emitter.label("__rt_mixed_from_value_null_container");
    emitter.instruction("mov x9, #8");                                          // runtime tag 8 is the canonical boxed PHP null
    emitter.instruction("str x9, [sp, #0]");                                    // replace the container-shaped tag before allocation
    emitter.instruction("stp xzr, xzr, [sp, #8]");                              // canonical null has no payload words or ownership
    emitter.instruction("b __rt_mixed_from_value_alloc");                       // allocate the normalized Mixed null without retaining the sentinel

    emitter.label("__rt_mixed_from_value_string");
    emitter.instruction("bl __rt_str_persist");                                 // duplicate the string payload for the boxed owner
    emitter.instruction("stp x1, x2, [sp, #8]");                                // replace the saved payload with the owned string pointer and length
    emitter.instruction("b __rt_mixed_from_value_alloc");                       // continue with allocation after persisting the string

    emitter.label("__rt_mixed_from_value_retain");
    emitter.instruction("mov x0, x1");                                          // move the child heap pointer into the incref argument register
    emitter.instruction("bl __rt_incref");                                      // retain the shared child pointer for the boxed owner

    emitter.label("__rt_mixed_from_value_alloc");
    emitter.instruction("mov x0, #24");                                         // mixed cells store tag plus two payload words
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the mixed cell storage
    emitter.instruction("mov x9, #5");                                          // low byte 5 = mixed cell heap kind
    emitter.instruction("str x9, [x0, #-8]");                                   // install the mixed-cell heap kind in the uniform header
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the saved runtime value tag
    emitter.instruction("str x10, [x0]");                                       // store the runtime value tag at mixed[0]
    emitter.instruction("ldp x11, x12, [sp, #8]");                              // reload the normalized payload words
    emitter.instruction("stp x11, x12, [x0, #8]");                              // store the payload words at mixed[8] and mixed[16]
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate the stack frame
    emitter.instruction("ret");                                                 // return the boxed mixed pointer in x0
}

/// x86_64 Linux implementation of `__rt_mixed_from_value`.
/// Detects the runtime value tag in `rax`, applies ownership normalization (string persistence or refcount retention),
/// allocates a 24-byte mixed cell on the heap, and writes the tagged payload at offsets 0, 8, and 16.
/// Input:  rax=value_tag, rdi=value_lo, rsi=value_hi
/// Output: rax=boxed mixed pointer
/// Clobbers: r10 as temporary scratch during header stamping and payload installation.
fn emit_mixed_from_value_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_from_value ---");
    emitter.label_global("__rt_mixed_from_value");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before boxing the mixed payload
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the temporary payload spill
    emitter.instruction("sub rsp, 32");                                         // reserve local slots for tag, value_lo, value_hi, and future helper-preserved scratch state
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the runtime value tag across helper-driven ownership normalization
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // save the low payload word across helper calls and the final heap allocation
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // save the high payload word across helper calls and the final heap allocation
    emitter.instruction("cmp rax, 4");                                          // do only container-shaped tags need null-payload normalization?
    emitter.instruction("jl __rt_mixed_from_value_dispatch");                   // scalar tags preserve every payload bit pattern
    emitter.instruction("cmp rax, 6");                                          // indexed arrays, hashes, and objects are the container-shaped tags
    emitter.instruction("jg __rt_mixed_from_value_dispatch");                   // nested Mixed/callable tags use ordinary pointer ownership
    emit_branch_if_null_container(
        emitter,
        "rdi",
        "r10",
        "__rt_mixed_from_value_null_container",
    );
    emitter.label("__rt_mixed_from_value_dispatch");
    emitter.instruction("cmp rax, 1");                                          // detect string payloads that need their own owned copy inside the mixed box
    emitter.instruction("je __rt_mixed_from_value_string");                     // strings must be persisted so the mixed cell owns a stable payload
    emitter.instruction("cmp rax, 4");                                          // detect indexed arrays that participate in refcounted ownership
    emitter.instruction("je __rt_mixed_from_value_retain");                     // retain indexed arrays before storing them inside the mixed cell
    emitter.instruction("cmp rax, 5");                                          // detect associative arrays that participate in refcounted ownership
    emitter.instruction("je __rt_mixed_from_value_retain");                     // retain associative arrays before storing them inside the mixed cell
    emitter.instruction("cmp rax, 6");                                          // detect objects that participate in refcounted ownership
    emitter.instruction("je __rt_mixed_from_value_retain");                     // retain objects before storing them inside the mixed cell
    emitter.instruction("cmp rax, 7");                                          // detect nested mixed cells that participate in refcounted ownership
    emitter.instruction("je __rt_mixed_from_value_retain");                     // retain nested mixed cells before storing them inside the parent mixed cell
    emitter.instruction("cmp rax, 10");                                         // detect callable descriptors that participate in callable ownership
    emitter.instruction("je __rt_mixed_from_value_retain");                     // retain callable descriptors before storing them inside the mixed cell
    emitter.instruction("cmp rax, 9");                                          // detect PHP resources, which carry registry or legacy native handles
    emitter.instruction("je __rt_mixed_from_value_resource");                   // resources bind display identity and registry ownership as applicable
    emitter.instruction("jmp __rt_mixed_from_value_alloc");                     // scalars can be boxed directly without additional ownership work

    emitter.label("__rt_mixed_from_value_resource");
    emitter.instruction("cmp QWORD PTR [rbp - 24], 2");                         // resource kind 2 = the raw incremental-hash context
    emitter.instruction("je __rt_mixed_from_value_alloc");                      // a HashContext is an OBJECT in PHP and must consume no resource id
    emitter.instruction("cmp QWORD PTR [rbp - 24], 5");                         // resource kind 5 = eval's inert handle on the same HashContext
    emitter.instruction("je __rt_mixed_from_value_alloc");                      // eval hashing must not shift the ids the host program's fopen() reports
    emitter.instruction("cmp QWORD PTR [rbp - 24], 6");                         // resource kind 6 = a libcurl easy handle
    emitter.instruction("je __rt_mixed_from_value_alloc");                      // a CurlHandle is an OBJECT in PHP and must consume no resource id
    emitter.instruction("cmp QWORD PTR [rbp - 24], 7");                         // resource kind 7 = a libcurl multi handle
    emitter.instruction("je __rt_mixed_from_value_alloc");                      // a CurlMultiHandle is an OBJECT in PHP and must consume no resource id
    emitter.instruction("cmp QWORD PTR [rbp - 24], 8");                         // resource kind 8 = a libcurl share handle
    emitter.instruction("je __rt_mixed_from_value_alloc");                      // a CurlShareHandle/CurlSharePersistentHandle is an OBJECT in PHP and must consume no resource id
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // move the native resource payload into the registry argument
    emitter.instruction("call __rt_resource_id_of");                            // bind a display id if this payload does not already have one
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the resource kind after display-id binding
    emitter.instruction("cmp r10, 1");                                          // kind 1 = registry-owned native stream
    emitter.instruction("je __rt_mixed_from_value_resource_retain_x86");        // the Mixed box becomes another owner of the stream handle
    emitter.instruction("cmp r10, 3");                                          // kind 3 = registry-owned process pipe
    emitter.instruction("je __rt_mixed_from_value_resource_retain_x86");        // retain the popen stream handle for the Mixed owner
    emitter.instruction("cmp r10, 4");                                          // kind 4 = registry-owned directory stream
    emitter.instruction("je __rt_mixed_from_value_resource_retain_x86");        // retain the directory stream handle for the Mixed owner
    emitter.instruction("cmp r10, 9");                                          // kind 9 = registry-owned stream context
    emitter.instruction("je __rt_mixed_from_value_resource_retain_x86");        // retain the context handle for the Mixed owner
    emitter.instruction("jmp __rt_mixed_from_value_alloc");                     // legacy resource kinds remain outside the stream registry for now

    emitter.label("__rt_mixed_from_value_resource_retain_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // pass the opaque resource handle saved before display-id binding
    emitter.instruction("call __rt_resource_retain");                           // retain the registry entry for ownership by the new Mixed cell
    emitter.instruction("jmp __rt_mixed_from_value_alloc");                     // allocate the box after the registry retain succeeds

    emitter.label("__rt_mixed_from_value_null_container");
    emitter.instruction("mov QWORD PTR [rbp - 8], 8");                          // replace the container-shaped tag with canonical Mixed null
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // canonical null has no low payload or ownership
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // canonical null has no high payload
    emitter.instruction("jmp __rt_mixed_from_value_alloc");                     // allocate the normalized Mixed null without retaining the sentinel

    emitter.label("__rt_mixed_from_value_string");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // move the source string pointer into the x86_64 string helper input register
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // move the source string length into the paired x86_64 string helper register
    emitter.instruction("call __rt_str_persist");                               // duplicate the string payload so the new mixed owner receives heap-backed storage
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // replace the saved low payload word with the owned string pointer returned by str_persist
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // replace the saved high payload word with the owned string length returned by str_persist
    emitter.instruction("jmp __rt_mixed_from_value_alloc");                     // continue boxing once the string payload is safely owned

    emitter.label("__rt_mixed_from_value_retain");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // move the shared heap child into the x86_64 refcount helper input register
    emitter.instruction("call __rt_incref");                                    // retain the shared heap child for the new mixed owner

    emitter.label("__rt_mixed_from_value_alloc");
    emitter.instruction("mov rax, 24");                                         // mixed cells store tag plus two payload words in the owned heap allocation
    emitter.instruction("call __rt_heap_alloc");                                // allocate the mixed cell storage through the x86_64 heap wrapper
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(5))); // materialize the mixed-cell heap kind word with the x86_64 heap marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the allocated payload as a mixed cell in the uniform heap header
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the saved runtime value tag after helper-driven ownership normalization
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store the runtime value tag at mixed[0]
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the normalized low payload word after ownership helpers completed
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // store the low payload word at mixed[8]
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the normalized high payload word after ownership helpers completed
    emitter.instruction("mov QWORD PTR [rax + 16], r10");                       // store the high payload word at mixed[16]
    emitter.instruction("add rsp, 32");                                         // release the temporary payload spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the boxed mixed pointer in rax
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Emits `__rt_mixed_from_value` for one target and returns the assembly text.
    fn emit_for(target: Target) -> String {
        let mut emitter = Emitter::new(target);
        emit_mixed_from_value(&mut emitter);
        emitter.output()
    }

    /// Pins ALL FIVE object-backed exclusions in the tag-9 arm, as exact multi-line
    /// sequences: the two hash contexts and the three libcurl handles (easy, multi,
    /// share).
    ///
    /// WHY THE WHOLE SEQUENCE AND NOT A SUBSTRING. `contains("cmp x2, #5")` is satisfied
    /// by `cmp x2, #50`, and `contains("cmp x2, #2")` by the `#2` inside `#24` — a pin
    /// that loose passes while the arm it claims to protect is gone. Pinning the compare
    /// TOGETHER WITH the branch that must follow it is what makes the test fail when
    /// either half is deleted, reordered, or pointed at a different label.
    ///
    /// Nothing pinned kind 2 on either target before this module existed, which is how
    /// the exclusion could have been dropped unnoticed; kinds 5, 6, 7 and 8 were added
    /// beside it and all five are asserted here so none can regress alone.
    #[test]
    fn aarch64_excludes_object_backed_kinds_from_resource_id_binding() {
        let asm = emit_for(Target::new(Platform::MacOS, Arch::AArch64));
        assert!(asm.contains("__rt_mixed_from_value_resource:\n"), "{asm}");
        assert!(
            asm.contains(
                "    cmp x2, #2\n\
                 \x20   b.eq __rt_mixed_from_value_alloc\n\
                 \x20   cmp x2, #5\n\
                 \x20   b.eq __rt_mixed_from_value_alloc\n\
                 \x20   cmp x2, #6\n\
                 \x20   b.eq __rt_mixed_from_value_alloc\n\
                 \x20   cmp x2, #7\n\
                 \x20   b.eq __rt_mixed_from_value_alloc\n\
                 \x20   cmp x2, #8\n\
                 \x20   b.eq __rt_mixed_from_value_alloc\n\
                 \x20   mov x0, x1\n\
                 \x20   bl __rt_resource_id_of\n"
            ),
            "{asm}"
        );
    }

    /// Pins the same five exclusions on x86_64.
    ///
    /// Separate emitters, separate arms: forgetting one target is the default way this
    /// change goes wrong, and an aarch64-only pin would not notice.
    #[test]
    fn x86_64_excludes_object_backed_kinds_from_resource_id_binding() {
        let asm = emit_for(Target::new(Platform::Linux, Arch::X86_64));
        assert!(asm.contains("__rt_mixed_from_value_resource:\n"), "{asm}");
        assert!(
            asm.contains(
                "    cmp QWORD PTR [rbp - 24], 2\n\
                 \x20   je __rt_mixed_from_value_alloc\n\
                 \x20   cmp QWORD PTR [rbp - 24], 5\n\
                 \x20   je __rt_mixed_from_value_alloc\n\
                 \x20   cmp QWORD PTR [rbp - 24], 6\n\
                 \x20   je __rt_mixed_from_value_alloc\n\
                 \x20   cmp QWORD PTR [rbp - 24], 7\n\
                 \x20   je __rt_mixed_from_value_alloc\n\
                 \x20   cmp QWORD PTR [rbp - 24], 8\n\
                 \x20   je __rt_mixed_from_value_alloc\n\
                 \x20   mov rax, QWORD PTR [rbp - 16]\n\
                 \x20   call __rt_resource_id_of\n"
            ),
            "{asm}"
        );
    }

    /// Pins that the exclusions read the resource KIND word, not the payload word.
    ///
    /// The one substitution that would keep both compares present and still be wrong:
    /// testing the LOW word (`x1` / `[rbp - 16]`) would compare an eval table key —
    /// `EVAL_RESOURCE_PAYLOAD_BASE + n`, never 2 or 5 — and silently bind ids again,
    /// while every sequence pin above still matched on the other target.
    #[test]
    fn the_exclusions_test_the_kind_word_on_both_targets() {
        for (target, kind_operand, payload_operand) in [
            (Target::new(Platform::MacOS, Arch::AArch64), "x2", "x1"),
            (
                Target::new(Platform::Linux, Arch::X86_64),
                "QWORD PTR [rbp - 24]",
                "QWORD PTR [rbp - 16]",
            ),
        ] {
            let asm = emit_for(target);
            for kind in [2, 5, 6, 7, 8] {
                assert!(
                    asm.contains(&format!("    cmp {kind_operand}, {}{kind}\n", sigil(target))),
                    "kind {kind} exclusion must compare the resource kind word\n{asm}"
                );
                assert!(
                    !asm.contains(&format!("    cmp {payload_operand}, {}{kind}\n", sigil(target))),
                    "kind {kind} exclusion must not compare the native payload word\n{asm}"
                );
            }
        }
    }

    /// Returns the immediate-operand sigil the target's assembly syntax uses.
    fn sigil(target: Target) -> &'static str {
        if target.arch == Arch::X86_64 {
            ""
        } else {
            "#"
        }
    }
}
