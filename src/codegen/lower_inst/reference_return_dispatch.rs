//! Purpose:
//! Adapts reference-returning overrides selected by a by-value class call.
//!
//! Called from:
//! - Instance and late-static vtable dispatch, including callable descriptor entries.
//!
//! Key details:
//! - Only affected inheritance families emit entry-address branches.
//! - Branching before the call preserves overflow arguments and the PHP return marker.
//! - Reference payloads become owned snapshots before their transferred lease is retired.

use super::*;
use crate::codegen_support::source_method_adapters::{self, MethodKind};

/// One physical entry that needs reference-to-value return adaptation.
pub(super) struct ReferenceReturnEntry {
    symbol: String,
    return_ty: PhpType,
}

/// Finds reference-returning implementations in one by-value caller's inheritance family.
pub(super) fn entries(
    ctx: &FunctionContext<'_>, class: &str, method: &str, caller_returns_reference: bool,
    kind: MethodKind, source_abi: bool,
) -> Result<Vec<ReferenceReturnEntry>> {
    if caller_returns_reference { return Ok(Vec::new()); }
    let mut implementations = std::collections::BTreeMap::new();
    for (name, info) in &ctx.module.class_infos {
        if !class_is_same_or_subclass(ctx, name, class) { continue; }
        let owner = match kind {
            MethodKind::Instance => info.method_impl_classes.get(method),
            MethodKind::Static => info.static_method_impl_classes.get(method),
        };
        let Some(owner) = owner else { continue; };
        let Some(physical) = ctx.module.class_infos.get(owner).and_then(|owner| match kind {
            MethodKind::Instance => owner.methods.get(method),
            MethodKind::Static => owner.static_methods.get(method),
        }) else { continue; };
        if !physical.by_ref_return { continue; }
        let symbol = if source_abi {
            source_method_adapters::source_vtable_entry_symbol(owner, method, physical, kind)
                .map_err(CodegenIrError::unsupported)?
        } else {
            match kind {
                MethodKind::Instance => method_symbol(owner, method),
                MethodKind::Static => static_method_symbol(owner, method),
            }
        };
        implementations.insert(symbol, physical.return_type.clone());
    }
    Ok(implementations.into_iter().map(|(symbol, return_ty)|
        ReferenceReturnEntry { symbol, return_ty }).collect())
}

/// Calls a loaded entry, converting only implementations whose physical ABI returns a cell.
pub(super) fn emit_call(
    ctx: &mut FunctionContext<'_>, entry_reg: &str,
    entries: &[ReferenceReturnEntry], caller_return_ty: &PhpType,
) -> Result<()> {
    if entries.is_empty() {
        abi::emit_call_reg(ctx.emitter, entry_reg);
        return Ok(());
    }
    let done = ctx.next_label("reference_override_done");
    let labels = entries.iter().map(|_| ctx.next_label("reference_override_value"))
        .collect::<Vec<_>>();
    let compare = abi::secondary_scratch_reg(ctx.emitter);
    for (entry, label) in entries.iter().zip(&labels) {
        abi::emit_symbol_address(ctx.emitter, compare, &entry.symbol);
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("cmp {entry_reg}, {compare}"));// identify the physical reference-returning implementation
                ctx.emitter.instruction(&format!("b.eq {label}"));              // select reference conversion before staging any extra stack words
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("cmp {entry_reg}, {compare}"));// identify the physical reference-returning implementation
                ctx.emitter.instruction(&format!("je {label}"));                // select reference conversion before staging any extra stack words
            }
        }
    }
    abi::emit_call_reg(ctx.emitter, entry_reg);
    abi::emit_jump(ctx.emitter, &done);
    for (entry, label) in entries.iter().zip(&labels) {
        ctx.emitter.label(label);
        abi::emit_call_reg(ctx.emitter, entry_reg);
        emit_value_snapshot(ctx, &entry.return_ty, caller_return_ty)?;
        abi::emit_jump(ctx.emitter, &done);
    }
    ctx.emitter.label(&done);
    Ok(())
}

/// Converts a returned reference cell into the owned representation promised to its caller.
pub(super) fn emit_value_snapshot(
    ctx: &mut FunctionContext<'_>, physical_ty: &PhpType, caller_ty: &PhpType,
) -> Result<()> {
    crate::codegen::runtime_callable_invoker::reference_return::emit_boxed_reference_return(
        ctx.emitter, physical_ty,
    );
    super::callables::results::emit_unbox_owned_descriptor_result(
        ctx.emitter, &caller_ty.codegen_repr(), Ownership::Owned,
    )?;
    crate::codegen_support::abi::emit_php_return_status(ctx.emitter, true);
    Ok(())
}
