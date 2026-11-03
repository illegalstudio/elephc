//! Purpose:
//! Shares equivalent native return cleanup without duplicating the entire frame at every return.
//!
//! Called from:
//! - Function return terminator lowering through the frame emitter.
//!
//! Key details:
//! - Cleanup depends on the transferred local slot and the precise return ownership status.
//! - Function-wide local representation and cleanup metadata are immutable during emission.
//! - The first site emits the original sequence; later equivalent sites jump to that sequence.

use super::*;
use crate::codegen::return_ownership::ReturnOwnershipStatus;

/// Emits or reuses a complete epilogue for exactly this ownership transfer and status source.
pub(super) fn emit(
    ctx: &mut FunctionContext<'_>, skip_return_slot: Option<LocalSlotId>,
    return_ownership: ReturnOwnershipStatus,
) {
    let key = (skip_return_slot, return_ownership);
    if let Some(label) = ctx.return_epilogues.get(&key).cloned() {
        abi::emit_jump(ctx.emitter, &label);
        return;
    }
    let label = ctx.next_label("shared_return_cleanup");
    ctx.return_epilogues.insert(key, label.clone());
    ctx.emitter.label(&label);
    emit_function_local_epilogue_cleanup(ctx, skip_return_slot);
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
        ReturnOwnershipStatus::Dynamic(offset) => {
            crate::codegen::return_ownership::emit_load_status(ctx.emitter, offset);
        }
    }
    abi::emit_frame_restore(ctx.emitter, ctx.frame_size);
    abi::emit_return(ctx.emitter);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fresh container returns reuse cleanup while a returned local keeps its distinct transfer.
    #[test]
    fn equivalent_return_cleanup_is_bounded_on_every_supported_target() {
        let branches = (0..24).map(|index| format!(
            "if ($choice === {index}) {{ return [$seed]; }}\n",
        )).collect::<String>();
        let source = format!("<?php function sharedNativeCleanup(int $choice, mixed $seed): array {{ \
            $owned = [$seed]; {branches} return $owned; }} \
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
            assert_eq!(shared, 2, "{name}: ordinary values and a transferred local need distinct plans");
            let cleanup = body.matches("epilogue cleanup $owned").count();
            assert!(cleanup > 0 && cleanup <= 2,
                "{name}: cleanup copies must be bounded independently of the 24 source return sites: {cleanup}");
        }
    }
}
