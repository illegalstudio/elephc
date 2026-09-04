//! Purpose:
//! Emits C-ABI wrappers used by the optional `elephc-magician` bridge crate.
//! Adapts Rust staticlib calls to elephc's internal runtime value helper ABI.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` when `RuntimeFeatures.eval` is enabled.
//!
//! Key details:
//! - Exported wrapper labels use platform C-symbol mangling because they are
//!   referenced from Rust object files, while internal `__rt_*` calls keep the
//!   existing assembly ABI.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::sentinels::emit_branch_if_null_container;

const EVAL_RUNTIME_TAG_MIXED: i64 = 7;
const INVOKER_ARG_REF_CELL_TAG: i64 = 11;

/// Builds the x86_64 instruction that installs the Mixed heap-kind marker.
fn x86_64_mixed_heap_kind_instruction() -> String {
    format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(5))
}

/// Emits every eval value wrapper required by `libelephc-magician`.
pub(crate) fn emit_eval_bridge_runtime(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: eval bridge value wrappers ---");
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64_wrappers(emitter),
        Arch::X86_64 => emit_x86_64_wrappers(emitter),
    }
    emit_gc_lifecycle_wrappers(emitter);
}


mod aarch64_values_classes;
mod aarch64_arrays;
mod aarch64_casts;
mod aarch64_numeric;
mod aarch64_compare;
mod aarch64_output;
mod x86_64_values_classes;
mod x86_64_arrays;
mod x86_64_casts;
mod x86_64_numeric;
mod x86_64_compare;
mod x86_64_output;
mod raw_object_helpers;
mod aarch64_reflection_names;
mod aarch64_reflection_members;
mod x86_64_reflection_names;
mod x86_64_reflection_members;
mod reflection_name_tables;
mod reflection_class_flags;
mod aarch64_clone;
mod x86_64_clone;
mod clone_rejections;
mod runtime_builtin_dispatch;

#[allow(unused_imports)]
use aarch64_values_classes::*;
#[allow(unused_imports)]
use aarch64_arrays::*;
#[allow(unused_imports)]
use aarch64_casts::*;
#[allow(unused_imports)]
use aarch64_numeric::*;
#[allow(unused_imports)]
use aarch64_compare::*;
#[allow(unused_imports)]
use aarch64_output::*;
#[allow(unused_imports)]
use x86_64_values_classes::*;
#[allow(unused_imports)]
use x86_64_arrays::*;
#[allow(unused_imports)]
use x86_64_casts::*;
#[allow(unused_imports)]
use x86_64_numeric::*;
#[allow(unused_imports)]
use x86_64_compare::*;
#[allow(unused_imports)]
use x86_64_output::*;
#[allow(unused_imports)]
use raw_object_helpers::*;
#[allow(unused_imports)]
use aarch64_reflection_names::*;
#[allow(unused_imports)]
use aarch64_reflection_members::*;
#[allow(unused_imports)]
use x86_64_reflection_names::*;
#[allow(unused_imports)]
use x86_64_reflection_members::*;
#[allow(unused_imports)]
use reflection_name_tables::*;
#[allow(unused_imports)]
use reflection_class_flags::*;
#[allow(unused_imports)]
use aarch64_clone::*;
#[allow(unused_imports)]
use x86_64_clone::*;
#[allow(unused_imports)]
use clone_rejections::*;
#[allow(unused_imports)]
use runtime_builtin_dispatch::*;

/// Emits ARM64 C-ABI wrappers around the internal mixed value helpers.
fn emit_aarch64_wrappers(emitter: &mut Emitter) {
    emit_aarch64_values_classes(emitter);
    emit_aarch64_arrays(emitter);
    emit_aarch64_casts(emitter);
    emit_aarch64_numeric(emitter);
    emit_aarch64_compare(emitter);
    emit_aarch64_output(emitter);
    emit_aarch64_runtime_builtin_dispatch(emitter);
}

/// Emits Linux x86_64 C-ABI wrappers around the internal mixed value helpers.
fn emit_x86_64_wrappers(emitter: &mut Emitter) {
    emit_x86_64_values_classes(emitter);
    emit_x86_64_arrays(emitter);
    emit_x86_64_casts(emitter);
    emit_x86_64_numeric(emitter);
    emit_x86_64_compare(emitter);
    emit_x86_64_output(emitter);
    emit_x86_64_runtime_builtin_dispatch(emitter);
}

/// Emits a global label with platform C-symbol mangling.
fn label_c_global(emitter: &mut Emitter, name: &str) {
    let symbol = emitter.target.extern_symbol(name);
    emitter.label_global(&symbol);
}

/// Exposes GC controls to Rust through platform-mangled C ABI tail wrappers.
fn emit_gc_lifecycle_wrappers(emitter: &mut Emitter) {
    let branch = match emitter.target.arch {
        Arch::AArch64 => "b",
        Arch::X86_64 => "jmp",
    };
    for (wrapper, target) in [
        ("__elephc_eval_gc_collect_cycles", "__rt_gc_collect_cycles_explicit"),
        ("__elephc_eval_gc_disable", "__rt_gc_disable"),
        ("__elephc_eval_gc_enable", "__rt_gc_enable"),
        ("__elephc_eval_gc_enabled", "__rt_gc_enabled"),
        ("__elephc_eval_gc_mem_caches", "__rt_gc_mem_caches"),
        ("__elephc_eval_gc_status_metric", "__rt_gc_status_metric"),
    ] {
        label_c_global(emitter, wrapper);
        emitter.instruction(&format!("{branch} {target}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Emits the whole eval bridge for one target and returns the assembly text.
    fn emit_for(target: Target) -> String {
        let mut emitter = Emitter::new(target);
        emit_eval_bridge_runtime(&mut emitter);
        emitter.output()
    }

    /// Verifies GC bridge exports use the platform C symbol while targeting internal helpers.
    #[test]
    fn gc_lifecycle_wrappers_apply_platform_c_symbol_mangling() {
        for (target, c_prefix, branch) in [
            (
                Target::new(Platform::MacOS, Arch::AArch64),
                "_",
                "b",
            ),
            (
                Target::new(Platform::Linux, Arch::X86_64),
                "",
                "jmp",
            ),
        ] {
            let asm = emit_for(target);
            for (wrapper, internal) in [
                ("__elephc_eval_gc_collect_cycles", "__rt_gc_collect_cycles_explicit"),
                ("__elephc_eval_gc_disable", "__rt_gc_disable"),
                ("__elephc_eval_gc_enable", "__rt_gc_enable"),
                ("__elephc_eval_gc_enabled", "__rt_gc_enabled"),
                ("__elephc_eval_gc_mem_caches", "__rt_gc_mem_caches"),
                ("__elephc_eval_gc_status_metric", "__rt_gc_status_metric"),
            ] {
                let export = format!("{c_prefix}{wrapper}:");
                let tail = format!("{branch} {internal}");
                assert!(asm.contains(&export), "missing {export} on {target:?}:\n{asm}");
                assert!(asm.contains(&tail), "missing {tail} on {target:?}:\n{asm}");
            }
        }
    }

    /// Pins the AArch64 tag-9 arm of `__elephc_eval_value_cast_string`.
    ///
    /// The eval bridge re-implements its own tag dispatch rather than calling
    /// `__rt_mixed_cast_string`, so fixing the boxed-Mixed cast alone left
    /// `strval($r)` inside a runtime-interpreted `eval()` returning the EMPTY string
    /// while PHP 8.5.6 returned `Resource id #5`. Both dispatches must carry the arm.
    #[test]
    fn aarch64_eval_string_cast_renders_resources() {
        let asm = emit_for(Target::new(Platform::MacOS, Arch::AArch64));
        assert!(
            asm.contains("    cmp x0, #9\n    b.eq __elephc_eval_value_cast_string_resource\n"),
            "{asm}"
        );
        assert!(asm.contains("__elephc_eval_value_cast_string_resource:\n"), "{asm}");
        assert!(asm.contains("bl __rt_resource_to_string"), "{asm}");
    }

    /// Pins the same arm on x86_64, so the eval bridge cannot render resources on one
    /// target and empty strings on the other.
    #[test]
    fn x86_64_eval_string_cast_renders_resources() {
        let asm = emit_for(Target::new(Platform::Linux, Arch::X86_64));
        assert!(
            asm.contains("    cmp rax, 9\n    je __elephc_eval_value_cast_string_resource_x86\n"),
            "{asm}"
        );
        assert!(asm.contains("__elephc_eval_value_cast_string_resource_x86:\n"), "{asm}");
        assert!(asm.contains("call __rt_resource_to_string"), "{asm}");
    }

    /// The eval arm hands the borrowed `_concat_buf` scratch straight to
    /// `__rt_mixed_from_value` with tag 1, which PERSISTS the bytes into a fresh boxed
    /// string. That copy is what makes returning scratch safe here: the Rust side owns a
    /// real boxed value and nothing ever frees the concat buffer.
    #[test]
    fn the_eval_resource_arm_boxes_a_persisted_string_on_both_targets() {
        for (target, label, next) in [
            (
                Target::new(Platform::MacOS, Arch::AArch64),
                "__elephc_eval_value_cast_string_resource:\n",
                "__elephc_eval_value_cast_string_float:\n",
            ),
            (
                Target::new(Platform::Linux, Arch::X86_64),
                "__elephc_eval_value_cast_string_resource_x86:\n",
                "__elephc_eval_value_cast_string_float_x86:\n",
            ),
        ] {
            let asm = emit_for(target);
            let arm = asm
                .split(label)
                .nth(1)
                .unwrap_or_else(|| panic!("missing eval resource arm for {target:?}:\n{asm}"));
            let arm = arm.split(next).next().expect("resource arm precedes the float arm");
            assert!(
                arm.contains("__rt_mixed_from_value"),
                "the eval resource arm must box the formatted string ({target:?}):\n{arm}"
            );
            assert!(
                !arm.contains("__rt_heap_free"),
                "the eval resource arm must not free borrowed scratch ({target:?}):\n{arm}"
            );
        }
    }

    /// Pins the whole body of `__elephc_eval_value_hash_context` on AArch64.
    ///
    /// The symbol exists to stamp resource KIND 5 — eval-owned inert handle — into the
    /// high payload word, so `__rt_mixed_from_value` skips `__rt_resource_id_of` and
    /// `__rt_mixed_free_deep` runs no destructor. PHP 8's `hash_init()` returns a
    /// `HashContext` OBJECT and consumes nothing from the resource counter; routing eval's
    /// context through `__elephc_eval_value_resource` (which writes `xzr`, i.e. kind 0)
    /// burned an id and shifted every later `fopen()` in the request.
    ///
    /// The full four-line body is pinned rather than `contains("mov x2, #5")`, which
    /// `mov x2, #50` also satisfies. `mov x2, #5` next to a `b` (never a `bl`) is also the
    /// tail-branch shape this helper needs: it takes no frame and saves no link register.
    #[test]
    fn aarch64_boxes_eval_hash_contexts_as_inert_kind_five() {
        let asm = emit_for(Target::new(Platform::MacOS, Arch::AArch64));
        assert!(
            asm.contains(
                "__elephc_eval_value_hash_context:\n\
                 \x20   mov x1, x0\n\
                 \x20   mov x0, #9\n\
                 \x20   mov x2, #5\n\
                 \x20   b __rt_mixed_from_value\n"
            ),
            "{asm}"
        );
    }

    /// Pins the same body on x86_64, where the key already sits in `rdi` per the SysV ABI
    /// and the internal `__rt_mixed_from_value` contract is tag=rax, lo=rdi, hi=rsi.
    ///
    /// Without this the x86 half could be omitted or left writing `xor esi, esi` and every
    /// aarch64 pin above would still pass — the single-arch blind spot that has already
    /// let a runtime fix be deleted from one target in this tree.
    #[test]
    fn x86_64_boxes_eval_hash_contexts_as_inert_kind_five() {
        let asm = emit_for(Target::new(Platform::Linux, Arch::X86_64));
        assert!(
            asm.contains(
                "__elephc_eval_value_hash_context:\n\
                 \x20   mov eax, 9\n\
                 \x20   mov esi, 5\n\
                 \x20   jmp __rt_mixed_from_value\n"
            ),
            "{asm}"
        );
    }

    /// Pins that the hash-context wrapper is DISTINCT from the plain resource wrapper.
    ///
    /// The likeliest silent regression is someone "simplifying" the new symbol into an
    /// alias of `__elephc_eval_value_resource`: the magician side would still compile and
    /// link, every hash digest would still be correct, and the id leak would come back.
    /// The plain wrapper must keep zeroing the kind word (genuine eval resources — fopen,
    /// opendir, popen, sockets — MUST consume ids), and the hash wrapper must not.
    #[test]
    fn the_resource_and_hash_context_wrappers_stamp_different_kinds() {
        for (target, resource_zero, hash_kind) in [
            (Target::new(Platform::MacOS, Arch::AArch64), "mov x2, xzr", "mov x2, #5"),
            (Target::new(Platform::Linux, Arch::X86_64), "xor esi, esi", "mov esi, 5"),
        ] {
            let asm = emit_for(target);
            let resource_body = body_of(&asm, "__elephc_eval_value_resource");
            let hash_body = body_of(&asm, "__elephc_eval_value_hash_context");
            assert!(
                resource_body.contains(resource_zero),
                "genuine eval resources must stay kind 0 and keep consuming ids ({target:?}):\n{resource_body}"
            );
            assert!(
                hash_body.contains(hash_kind),
                "eval hash contexts must be stamped kind 5 ({target:?}):\n{hash_body}"
            );
            assert!(
                !hash_body.contains(resource_zero),
                "the hash-context wrapper must not zero the kind word ({target:?}):\n{hash_body}"
            );
        }
    }

    /// Pins the AArch64 eval key-sort bridge to the shared AOT regular comparator.
    #[test]
    fn aarch64_eval_key_sort_uses_native_regular_comparator() {
        let asm = emit_for(Target::new(Platform::MacOS, Arch::AArch64));
        let body = body_of(&asm, "__elephc_eval_value_regular_key_compare");
        assert!(body.contains("bl __rt_mixed_unbox"), "{body}");
        assert!(body.contains("bl __rt_key_compare_regular"), "{body}");
    }

    /// Pins the x86_64 eval key-sort bridge to the shared AOT regular comparator.
    #[test]
    fn x86_64_eval_key_sort_uses_native_regular_comparator() {
        let asm = emit_for(Target::new(Platform::Linux, Arch::X86_64));
        let body = body_of(&asm, "__elephc_eval_value_regular_key_compare");
        assert!(body.contains("call __rt_mixed_unbox"), "{body}");
        assert!(body.contains("call __rt_key_compare_regular"), "{body}");
    }

    /// Verifies relational and spaceship eval wrappers share PHP's tag-aware ordering ABI.
    #[test]
    fn eval_ordering_wrappers_use_php_compare_on_both_targets() {
        for (target, call, cast) in [
            (
                Target::new(Platform::MacOS, Arch::AArch64),
                "bl __rt_php_compare",
                "bl __rt_mixed_cast_float",
            ),
            (
                Target::new(Platform::Linux, Arch::X86_64),
                "call __rt_php_compare",
                "call __rt_mixed_cast_float",
            ),
        ] {
            let asm = emit_for(target);
            for label in ["__elephc_eval_value_compare", "__elephc_eval_value_spaceship"] {
                let body = body_of(&asm, label);
                let ordering_body = body
                    .split("__elephc_eval_value_compare_eq:")
                    .next()
                    .expect("split yields the ordering wrapper prefix");
                assert!(body.contains(call), "{label} must use PHP ordering on {target:?}:\n{body}");
                assert!(
                    !ordering_body.contains(cast),
                    "{label} must not erase runtime tags through float casting on {target:?}:\n{body}"
                );
            }
        }

        let x86_64 = emit_for(Target::new(Platform::Linux, Arch::X86_64));
        let compare = body_of(&x86_64, "__elephc_eval_value_compare");
        let compare_prefix = compare
            .split("__elephc_eval_value_compare_eq:")
            .next()
            .expect("split yields the x86_64 ordering wrapper prefix");
        assert!(compare.contains("mov r10, rax"), "{compare}");
        assert!(compare.contains("mov rcx, QWORD PTR [rbp - 24]"), "{compare}");
        assert!(
            !compare_prefix.contains("mov r10, QWORD PTR [rbp - 24]"),
            "the x86_64 opcode dispatch must preserve the ordering result:\n{compare}"
        );
    }

    /// Returns the instruction lines following `label` up to the next exported helper.
    ///
    /// `label_c_global` emits `.globl <sym>` immediately before each wrapper's label, so
    /// the next `.globl` is where this wrapper's body ends. Splitting on a BLANK line
    /// would not work — the bridge emits none between wrappers — and would silently hand
    /// back the whole remainder of the file, making every negative assertion below
    /// vacuous: `mov x2, xzr` from the very next wrapper would satisfy it.
    fn body_of<'a>(asm: &'a str, label: &str) -> &'a str {
        let marker = format!("{label}:\n");
        let body = asm
            .split(&marker)
            .nth(1)
            .unwrap_or_else(|| panic!("missing {label} in emitted assembly:\n{asm}"));
        let body = body.split("\n.globl ").next().expect("split yields a first segment");
        assert!(
            !body.is_empty(),
            "isolated an empty body for {label}; the emitter's helper separator changed"
        );
        body
    }
}
