//! Purpose:
//! Shares equivalent native return cleanup without duplicating the entire frame at every return.
//!
//! Called from:
//! - Function return terminator lowering through the frame emitter.
//!
//! Key details:
//! - Multiple return sites select their transferred local through one fixed frame word.
//! - Selection keeps the live slot intact while sibling destructors can observe the frame.
//! - Dynamic markers use an aligned temporary slot before their shared cleanup branch.
//! - Function-wide local representation and cleanup metadata are immutable during emission.
//! - The first site emits the original sequence; later equivalent sites jump to that sequence.

use super::*;
use crate::codegen::return_ownership::ReturnOwnershipStatus;

/// Emits or reuses a complete epilogue while preserving this path's precise ownership marker.
pub(super) fn emit(
    ctx: &mut FunctionContext<'_>, skip_return_slot: Option<LocalSlotId>,
    return_ownership: ReturnOwnershipStatus,
) {
    let status = match return_ownership {
        ReturnOwnershipStatus::Static(owned) => Some(owned),
        ReturnOwnershipStatus::Dynamic(offset) => {
            crate::codegen::return_ownership::emit_load_status(ctx.emitter, offset);
            abi::emit_push_php_return_status(ctx.emitter);
            None
        }
    };
    let runtime_return_slot = ctx.return_cleanup_skip_offset;
    let skip_return_slot = if let Some(offset) = runtime_return_slot {
        let scratch = abi::secondary_scratch_reg(ctx.emitter);
        let selected = skip_return_slot.map_or(0, |slot| u64::from(slot.as_raw()) + 1);
        abi::emit_load_int_immediate(ctx.emitter, scratch, selected as i64);
        abi::store_at_offset(ctx.emitter, scratch, offset);
        None
    } else { skip_return_slot };
    let key = (skip_return_slot, status);
    if let Some(label) = ctx.return_epilogues.get(&key).cloned() {
        abi::emit_jump(ctx.emitter, &label);
        return;
    }
    let label = ctx.next_label("shared_return_cleanup");
    ctx.return_epilogues.insert(key, label.clone());
    ctx.emitter.label(&label);
    emit_function_local_epilogue_cleanup(ctx, skip_return_slot, runtime_return_slot);
    for local in &ctx.function.locals {
        if local.kind == LocalKind::ReturnRefCell {
            let offset = ctx.local_offset(local.id).expect("return cell owner has frame storage");
            abi::emit_store_zero_to_local_slot(ctx.emitter, offset);
        }
    }
    emit_exception_activation_pop(ctx);
    emit_callee_saved_restores(ctx);
    match return_ownership {
        ReturnOwnershipStatus::Static(owned) => {
            crate::codegen::return_ownership::emit_status(ctx.emitter, owned);
        }
        ReturnOwnershipStatus::Dynamic(_) => {
            abi::emit_pop_php_return_status(ctx.emitter);
        }
    }
    abi::emit_frame_restore(ctx.emitter, ctx.frame_size);
    abi::emit_return(ctx.emitter);
}

/// Skips exactly the selected transfer without clearing storage observable by cleanup callbacks.
pub(super) fn emit_transfer_guard(
    ctx: &mut FunctionContext<'_>, slot: LocalSlotId, selection: Option<usize>,
) -> Option<String> {
    let offset = selection?;
    let done = ctx.next_label("return_transfer_cleanup_done");
    let result = abi::int_result_reg(ctx.emitter);
    let scratch = abi::secondary_scratch_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, result, offset);
    abi::emit_load_int_immediate(ctx.emitter, scratch, i64::from(slot.as_raw()) + 1);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("cmp {result}, {scratch}")), // compare this local with the path's transferred owner
        Arch::X86_64 => ctx.emitter.instruction(&format!("cmp {result}, {scratch}")),  // compare this local with the path's transferred owner
    }
    abi::emit_branch_if_equal_wide(ctx.emitter, &done);
    Some(done)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Distinct nested-call marker spills share one cleanup while preserving every marker source.
    #[test]
    fn dynamic_return_markers_share_cleanup_on_every_supported_target() {
        let branches = (0..24).map(|index| format!(
            "if ($choice === {index}) {{ return sharedMarkerSource($owned, $seed); }}\n",
        )).collect::<String>();
        let source = format!("<?php \
            function sharedMarkerSource(bool $owned, string $seed): string {{ \
                if ($owned) {{ return str_repeat($seed, 2); }} return $seed; }} \
            function sharedDynamicCleanup(int $choice, bool $owned, string $seed): string {{ \
                $guard = [$seed]; $guard[] = $choice; echo count($guard); \
                {branches} return sharedMarkerSource($owned, $seed); }} \
            echo sharedDynamicCleanup($argc, $argc > 1, 'seed');");
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let module = crate::ir_lower::tests::lower_source_at_for_target(
                &source, std::path::Path::new("main.php"), std::path::Path::new("."),
                Target::parse(name).unwrap(),
            );
            let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            let body = assembly.split_once("@fn name=sharedDynamicCleanup symbol=").unwrap().1
                .split_once("@endfn name=sharedDynamicCleanup").unwrap().0;
            let shared = body.lines().filter(|line|
                line.ends_with(':') && line.contains("shared_return_cleanup")).count();
            assert_eq!(shared, 1, "{name}: marker spill offsets must not duplicate cleanup");
            let push = if name == "linux-x86_64" { "mov QWORD PTR [rsp], r11" }
                else { "str x15, [sp, #-16]!" };
            let pop = if name == "linux-x86_64" { "mov r11, QWORD PTR [rsp]" }
                else { "ldr x15, [sp], #16" };
            assert_eq!(body.matches(push).count(), 24,
                "{name}: snapshot each distinct marker after equivalent return paths are merged");
            assert_eq!(body.matches(pop).count(), 1, "{name}: restore the shared marker after cleanup");
        }
    }

    /// Fresh container returns and a transferred local share cleanup with a runtime selector.
    #[test]
    fn equivalent_return_cleanup_is_bounded_on_every_supported_target() {
        let branches = (0..24).map(|index| format!(
            "if ($choice === {index}) {{ return [$seed]; }}\n",
        )).collect::<String>();
        let source = format!("<?php function sharedNativeCleanup(int $choice, mixed $seed): array {{ \
            $owned = [$seed]; $owned[] = $choice; {branches} return $owned; }} \
            echo count(sharedNativeCleanup($argc, 'seed'));");
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let module = crate::ir_lower::tests::lower_source_at_for_target(
                &source, std::path::Path::new("main.php"), std::path::Path::new("."),
                Target::parse(name).unwrap(),
            );
            let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            let body = assembly.split_once("@fn name=sharedNativeCleanup symbol=").unwrap().1
                .split_once("@endfn name=sharedNativeCleanup").unwrap().0;
            let shared = body.lines().filter(|line|
                line.ends_with(':') && line.contains("shared_return_cleanup")).count();
            assert_eq!(shared, 1, "{name}: the runtime selector preserves the transferred local in shared cleanup");
            let cleanup = body.matches("epilogue cleanup $owned").count();
            assert_eq!(cleanup, 2,
                "{name}: retain one normal and one exceptional cleanup independently of the 24 return sites: {cleanup}");
        }
    }
}
