//! Purpose:
//! Rejects abstract instance calls before their arguments are evaluated.
//!
//! Called from:
//! - EIR MethodLookup emitted for native SimpleXML factory receivers.
//!
//! Key details:
//! - Concrete method declarations cannot become abstract in a subclass.
//! - Eval object identity takes precedence over its native backing class.

use super::*;
use crate::parser::ast::Visibility;

/// Checks the actual receiver while retaining the caller's existing argument and owner scopes.
pub(super) fn lower_method_lookup(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let method = method_name_data(ctx, inst)?.to_string();
    let key = php_symbol_key(&method);
    if let PhpType::Object(class) = ctx.value_php_type(object)?.codegen_repr() {
        if ctx.module.class_infos.get(class.trim_start_matches('\\'))
            .is_some_and(|info| info.method_impl_classes.contains_key(&key)) {
            return Ok(());
        }
    }
    let mut candidates = ctx.module.class_infos.iter()
        .filter(|(_, info)| info.is_abstract && info.abstract_methods.contains(&key))
        .map(|(class, info)| {
            let declaring = info.method_declaring_classes.get(&key).unwrap_or(class);
            let scope = ctx.defined_class_scope();
            let visibility = info.method_visibilities.get(&key).cloned().unwrap_or(Visibility::Public);
            let accessible = match visibility {
                Visibility::Public => true,
                Visibility::Private => scope == Some(declaring.as_str()),
                Visibility::Protected => scope.is_some_and(|scope|
                    class_is_same_or_subclass(ctx, scope, declaring)
                        || class_is_same_or_subclass(ctx, declaring, scope)),
            };
            let message = if accessible {
                format!("Cannot call abstract method {declaring}::{method}()")
            } else {
                let access = if matches!(visibility, Visibility::Private) { "private" } else { "protected" };
                let caller = scope.map(|scope| format!("scope {scope}")).unwrap_or_else(|| "global scope".to_string());
                format!("Call to {access} method {declaring}::{method}() from {caller}")
            };
            (info.class_id, message)
        })
        .collect::<Vec<_>>();
    candidates.sort();
    if ctx.module.required_runtime_features.eval_bridge {
        builtins::lower_eval_abstract_method_guard(ctx, inst, object, &method)?;
    }
    if candidates.is_empty() { return Ok(()); }
    let done = ctx.next_label("abstract_method_lookup_done");
    let receiver = abi::tertiary_scratch_reg(ctx.emitter);
    let ty = ctx.load_value_to_result(object)?.codegen_repr();
    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        emit_mixed_method_object_payload_or_fatal(ctx, receiver, &done);
    } else if matches!(ty, PhpType::Object(_)) {
        ctx.load_value_to_reg(object, receiver)?;
    } else { return Ok(()); }
    let scratch = abi::secondary_scratch_reg(ctx.emitter);
    crate::codegen::sentinels::emit_branch_if_null_container(ctx.emitter, receiver, scratch, &done);
    let class_id = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_load_from_address(ctx.emitter, class_id, receiver, 0);
    let labels = candidates.iter().map(|_| ctx.next_label("abstract_method_lookup_error")).collect::<Vec<_>>();
    for ((id, _), label) in candidates.iter().zip(&labels) {
        abi::emit_load_int_immediate(ctx.emitter, scratch, *id as i64);
        // -- select the abstract receiver before any argument expression --
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("cmp {class_id}, {scratch}")); // identify the actual abstract receiver
                ctx.emitter.instruction(&format!("b.eq {label}"));              // reject its missing method before arguments
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("cmp {class_id}, {scratch}")); // identify the actual abstract receiver
                ctx.emitter.instruction(&format!("je {label}"));                // reject its missing method before arguments
            }
        }
    }
    abi::emit_jump(ctx.emitter, &done);
    for ((_, message), label) in candidates.iter().zip(&labels) {
        ctx.emitter.label(label);
        exceptions::emit_error(ctx, message);
    }
    ctx.emitter.label(&done);
    Ok(())
}
