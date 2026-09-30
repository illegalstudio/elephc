//! Purpose:
//! Emits the opaque, dynamically growing PHP resource registry and initial stream-state helpers.
//! This is the Gate 1 runtime boundary shared by every supported target.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()`.
//!
//! Key details:
//! - Registry slots contain no raw PHP-visible descriptor identity.
//! - Target-specific leaf emitters share the layouts declared in `layout`.

mod context;
mod filter;
pub(crate) mod layout;
mod registry;
mod stream;

use crate::codegen_support::emit::Emitter;

/// Emits the resource-registry lifecycle and stream-state helper entry points.
pub(crate) fn emit_resource_runtime(emitter: &mut Emitter) {
    registry::emit_resource_registry(emitter);
    stream::emit_stream_resources(emitter);
    context::emit_context_resources(emitter);
    filter::emit_filter_resources(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Arch, Platform, Target};

    /// Verifies every supported architecture exposes the complete Gate 1 helper ABI.
    #[test]
    fn emits_gate_one_resource_entry_points() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_resource_runtime(&mut emitter);
            let asm = emitter.output();
            for label in [
                "__rt_resource_registry_init:",
                "__rt_resource_alloc:",
                "__rt_resource_lookup_any:",
                "__rt_resource_retain:",
                "__rt_resource_release:",
                "__rt_resource_registry_request_reset:",
                "__rt_resource_mark_closing:",
                "__rt_resource_mark_closed:",
                "__rt_resource_id_of_registry:",
                "__rt_resource_is_open:",
                "__rt_resource_kind_if_open:",
                "__rt_stream_adopt_fd:",
                "__rt_stream_state:",
                "__rt_stream_fd:",
                "__rt_stream_chunk_size:",
                "__rt_stream_set_chunk_size:",
                "__rt_stream_close_backend:",
                "__rt_context_state:",
                "__rt_context_destroy_state:",
                "__rt_stream_default_context_ensure:",
            ] {
                assert!(asm.contains(label), "{target:?} omitted {label}");
            }
        }
    }

    /// Verifies process-exit teardown hands the slot array to `__rt_heap_free` in the
    /// register that helper actually reads.
    ///
    /// `__rt_heap_free` takes its operand in the integer RESULT register — `x0` on
    /// AArch64, `rax` on x86_64 (`heap_free.rs` opens with `test rax, rax`) — NOT in the
    /// SysV first-argument register. The x86_64 teardown loaded the pointer into `rdi`,
    /// so the free path walked the heap list off whatever `rax` happened to hold and
    /// **every CLI program on linux-x86_64 segfaulted at exit**, after printing correct
    /// output. AArch64 could not show it: there the operand register and the first
    /// argument register are both `x0`.
    ///
    /// Pinned per target for that exact reason — a single-target assertion is what let
    /// this ship.
    #[test]
    fn registry_teardown_passes_the_slot_array_in_the_heap_free_operand_register() {
        for (target, expected) in [
            (
                Target::new(Platform::MacOS, Arch::AArch64),
                "ldr x0, [x9]",
            ),
            (
                Target::new(Platform::Linux, Arch::X86_64),
                "mov rax, QWORD PTR [r9]",
            ),
        ] {
            let mut emitter = Emitter::new(target);
            emit_resource_runtime(&mut emitter);
            let asm = emitter.output();
            let body = &asm[asm
                .find("__rt_resource_registry_teardown:")
                .expect("teardown label")..];
            let body = &body[..body.find("ret").map(|i| i + 3).unwrap_or(body.len())];
            assert!(
                body.contains(expected),
                "{target:?} must load the slot array with `{expected}` before heap_free:\n{body}"
            );
            assert!(
                !body.contains("mov rdi, QWORD PTR [r9]"),
                "{target:?} passed the slot array in the SysV argument register, which \
                 `__rt_heap_free` never reads:\n{body}"
            );
        }
    }

    /// Verifies the request-default context is created ONCE and owns its state.
    ///
    /// PHP mints resource id 4 for the request's default stream context at the first
    /// stream open and retains it, so a second creation would consume a second id and
    /// shift every later resource. The emitted body must therefore return the stored
    /// handle when one exists (the early-out), register the state as an OWNED Context
    /// (kind 2, request-owned 1) so request teardown frees it, and publish the handle
    /// back into `_stream_default_context_handle`.
    ///
    /// Asserted per target rather than once: this helper exists because a target that
    /// skipped it reported every eval resource id one lower than PHP's, and that is
    /// precisely the kind of divergence a single-target assertion hides.
    #[test]
    fn the_request_default_context_is_created_once_and_owns_its_state() {
        for (target, reuse, kind, owned) in [
            (
                Target::new(Platform::MacOS, Arch::AArch64),
                "cbnz x0, __rt_stream_default_context_ensure_done",
                "mov x0, #2",
                "mov x2, #1",
            ),
            (
                Target::new(Platform::Linux, Arch::X86_64),
                "jnz __rt_stream_default_context_ensure_done_x86",
                "mov edi, 2",
                "mov edx, 1",
            ),
        ] {
            let mut emitter = Emitter::new(target);
            emit_resource_runtime(&mut emitter);
            let asm = emitter.output();
            for needle in [reuse, kind, owned] {
                assert!(
                    asm.contains(needle),
                    "{target:?} lost `{needle}` from the default-context helper:\n{asm}"
                );
            }
        }
    }

    /// Verifies the public layouts remain compatible with handle and stream ABI contracts.
    #[test]
    fn layouts_match_gate_one_contract() {
        assert_eq!(layout::RESOURCE_SLOT_SIZE, 64);
        assert_eq!(layout::STREAM_STATE_SIZE, 320);
        assert_eq!(layout::STREAM_FD_OFFSET, 16);
        assert_eq!(layout::STREAM_CONTEXT_HANDLE_OFFSET, 80);
        assert_eq!(layout::CONTEXT_STATE_SIZE, 32);
        assert_eq!(layout::CONTEXT_OPTIONS_OFFSET, 0);
        assert_eq!(layout::CONTEXT_PARAMS_OFFSET, 8);
        assert_eq!(layout::CONTEXT_NOTIFIER_OFFSET, 16);
        assert_eq!(layout::CONTEXT_FLAGS_OFFSET, 24);
        assert_eq!(layout::STANDARD_STREAM_COUNT, 3);
    }

    /// Verifies ContextState teardown releases both retained children before parent storage.
    #[test]
    fn context_destructor_releases_children_before_state() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_resource_runtime(&mut emitter);
            let asm = emitter.output();
            let destructor = &asm[asm
                .find("__rt_context_destroy_state:")
                .expect("context destructor label")..];
            let options = destructor
                .find("__rt_decref_any")
                .expect("context options release");
            let notifier = destructor
                .find("__rt_callable_descriptor_release")
                .expect("context notifier release");
            let state = destructor
                .find("__rt_heap_free")
                .expect("context state release");
            assert!(
                options < notifier && notifier < state,
                "{target:?} emitted ContextState teardown out of order"
            );
        }
    }

    /// Verifies request reset clears borrowed bridges, destroys streams first, then drops the default context owner.
    #[test]
    fn request_reset_preserves_contexts_until_stream_teardown_finishes() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_resource_runtime(&mut emitter);
            let asm = emitter.output();
            let reset = &asm[asm
                .find("__rt_resource_registry_request_reset:")
                .expect("request reset label")..asm
                .find("__rt_resource_mark_closing:")
                .expect("next resource helper label")];
            let options = reset
                .find("_stream_context_options")
                .expect("borrowed options bridge clear");
            let notifier = reset
                .find("_stream_notification_callback")
                .expect("borrowed notifier bridge clear");
            let current = reset
                .find("_stream_current_context_handle")
                .expect("borrowed wrapper context clear");
            let stream_phase = reset
                .find("__rt_resource_registry_request_reset_kind_ready")
                .expect("stream-kind phase guard");
            let default_context = reset
                .find("_stream_default_context_handle")
                .expect("default context owner detach");
            let epoch = reset
                .find("_resource_registry_epoch")
                .expect("request epoch advance");
            assert!(
                options < notifier
                    && notifier < current
                    && current < stream_phase
                    && stream_phase < default_context
                    && default_context < epoch,
                "{target:?} emitted request reset phases out of order"
            );
        }
    }

    /// Verifies typed state destructors re-resolve their slot after callbacks may grow the registry.
    #[test]
    fn resource_release_refreshes_slot_after_reentrant_state_teardown() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_resource_runtime(&mut emitter);
            let asm = emitter.output();
            let release = &asm[asm
                .find("__rt_resource_release:")
                .expect("resource release label")..asm
                .find("__rt_resource_registry_request_reset:")
                .expect("request reset label")];
            let refresh = release
                .find("__rt_resource_release_after_state_destroy:")
                .expect("post-destructor slot refresh label");
            let recycle = release
                .find("__rt_resource_release_recycle:")
                .expect("resource recycle label");
            assert!(refresh < recycle, "{target:?} refreshes after recycling");
            let refresh_body = &release[refresh..recycle];
            assert!(
                refresh_body.contains("__rt_resource_lookup_any"),
                "{target:?} does not re-resolve the slot after typed teardown"
            );
            assert!(
                release
                    .matches("__rt_resource_release_after_state_destroy")
                    .count()
                    >= 3,
                "{target:?} does not route both typed destructors through slot refresh"
            );
        }
    }

    /// Verifies x86_64 calls the custom heap allocator with its size in `rax`.
    ///
    /// The registry's own 512-byte request is gone on purpose: the initial slot array is
    /// now the static `_resource_registry_static_slots` reservation, so a program needs no
    /// runtime heap before its first statement. What remains heap-allocated here is the
    /// 320-byte StreamState and the grown slot array, and both must pass their size in the
    /// operand register `__rt_heap_alloc` reads — passing it in `rdi` is precisely the slip
    /// that made every linux-x86_64 program segfault at exit.
    #[test]
    fn x86_64_resource_allocations_use_heap_runtime_abi() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_resource_runtime(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("mov eax, 320"), "StreamState size must reach rax");
        assert!(
            asm.contains("shl rax, 6"),
            "the grown slot array must size itself in rax"
        );
        assert!(!asm.contains("mov edi, 512"));
        assert!(!asm.contains("mov edi, 320"));
        assert!(
            !asm.contains("mov eax, 512"),
            "the registry must no longer request its initial slot array from the heap"
        );
    }

    /// Verifies the initial slot array is static and never handed to `__rt_heap_free`.
    ///
    /// Allocating it in every program's prologue meant a program compiled with a small
    /// `--heap-size` died at startup with `Fatal error: heap memory exhausted` — the two
    /// 256-byte `runtime_gc::heap` harnesses could not run at all — and it was the block
    /// reported as leaked in every stream-free program. Growth still moves to the heap, so
    /// both the growth and teardown paths must recognize the static base and skip the free.
    #[test]
    fn the_initial_slot_array_is_static_and_never_freed() {
        for (target, cmp) in [
            (Target::new(Platform::MacOS, Arch::AArch64), "cmp x0, x9"),
            (Target::new(Platform::Linux, Arch::X86_64), "cmp rax, r10"),
        ] {
            let mut emitter = Emitter::new(target);
            emit_resource_runtime(&mut emitter);
            let asm = emitter.output();
            assert!(
                asm.contains("_resource_registry_static_slots"),
                "{target:?} must seed the registry from the static reservation:\n{asm}"
            );
            let teardown = &asm[asm
                .find("__rt_resource_registry_teardown:")
                .expect("teardown label")..];
            let teardown = &teardown[..teardown.find("ret").map(|i| i + 3).unwrap_or(teardown.len())];
            assert!(
                teardown.contains("_resource_registry_static_slots") && teardown.contains(cmp),
                "{target:?} teardown must compare against the static base before freeing:\n{teardown}"
            );
        }
    }
}
