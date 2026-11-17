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
    if inst.immediate.is_none() {
        return lower_dynamic_method_lookup(ctx, inst, object, expect_operand(inst, 1)?);
    }
    let method = method_name_data(ctx, inst)?.to_string();
    if method_is_definitely_concrete(ctx, object, &method)? { return Ok(()); }
    if ctx.module.required_runtime_features.eval_bridge {
        builtins::lower_eval_abstract_method_guard(ctx, inst, object, Some(&method))?;
    }
    lower_native_method_lookup(ctx, object, &method)
}

/// Rejects native abstract targets through the existing class and lexical visibility metadata.
fn lower_native_method_lookup(ctx: &mut FunctionContext<'_>, object: ValueId, method: &str) -> Result<()> {
    let key = php_symbol_key(method);
    if method_is_definitely_concrete(ctx, object, method)? { return Ok(()); }
    let mut candidates = ctx.module.class_infos.iter()
        .filter(|(_, info)| info.is_abstract && info.abstract_methods.contains(&key))
        .filter_map(|(class, info)| {
            let declaring = info.method_declaring_classes.get(&key).unwrap_or(class);
            let declarations = ctx.module.class_infos.get(declaring).map(|info| &info.method_decls)
                .or_else(|| ctx.module.interface_infos.get(declaring).map(|info| &info.method_decls));
            let method = declarations.and_then(|methods| methods.iter().find(|decl| php_symbol_key(&decl.name) == key))
                .map_or(method, |decl| decl.name.as_str());
            let scope = ctx.defined_class_scope();
            let visibility = info.method_visibilities.get(&key).cloned().unwrap_or(Visibility::Public);
            let accessible = match visibility {
                Visibility::Public => true,
                Visibility::Private => scope == Some(declaring.as_str()),
                Visibility::Protected => scope.is_some_and(|scope|
                    class_is_same_or_subclass(ctx, scope, declaring)
                        || class_is_same_or_subclass(ctx, declaring, scope)),
            };
            if !accessible && info.method_impl_classes.contains_key("__call") { return None; }
            let message = if accessible {
                format!("Cannot call abstract method {declaring}::{method}()")
            } else {
                let access = if matches!(visibility, Visibility::Private) { "private" } else { "protected" };
                let caller = scope.map(|scope| format!("scope {scope}")).unwrap_or_else(|| "global scope".to_string());
                format!("Call to {access} method {declaring}::{method}() from {caller}")
            };
            Some((info.class_id, message))
        })
        .collect::<Vec<_>>();
    candidates.sort();
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

/// A concrete declaration cannot acquire an abstract override in a legal subclass.
fn method_is_definitely_concrete(ctx: &FunctionContext<'_>, object: ValueId, method: &str) -> Result<bool> {
    Ok(match ctx.value_php_type(object)?.codegen_repr() {
        PhpType::Object(class) => ctx.module.class_infos.get(class.trim_start_matches('\\'))
            .is_some_and(|info| info.method_impl_classes.contains_key(&php_symbol_key(method))),
        _ => false,
    })
}

/// Matches runtime names before arguments, reusing the literal guard on a native match.
fn lower_dynamic_method_lookup(
    ctx: &mut FunctionContext<'_>, inst: &Instruction, object: ValueId, method: ValueId,
) -> Result<()> {
    let names = ctx.module.class_infos.iter()
        .filter(|(class, info)| info.is_abstract && class_is_same_or_subclass(ctx, class, "SimpleXMLElement"))
        .flat_map(|(_, info)| info.abstract_methods.iter().cloned())
        .collect::<std::collections::BTreeSet<_>>();
    if names.is_empty() && !ctx.module.required_runtime_features.eval_bridge { return Ok(()); }
    let done = ctx.next_label("dynamic_abstract_method_lookup_done");
    if !load_dynamic_method_name(ctx, method, &done)? { return Ok(()); }
    let (ptr, len) = abi::string_result_regs(ctx.emitter);
    abi::emit_push_reg_pair(ctx.emitter, ptr, len);
    if ctx.module.required_runtime_features.eval_bridge {
        builtins::lower_eval_abstract_method_guard(ctx, inst, object, None)?;
    }
    let matches = names.into_iter().map(|name| (name, ctx.next_label("dynamic_abstract_method_lookup_match")))
        .collect::<Vec<_>>();
    for (name, label) in &matches {
        builtins::emit_branch_if_saved_string_matches_ci(ctx, name.as_bytes(), label);
    }
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    abi::emit_jump(ctx.emitter, &done);
    for (name, label) in matches {
        ctx.emitter.label(&label);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        lower_native_method_lookup(ctx, object, &name)?;
        abi::emit_jump(ctx.emitter, &done);
    }
    ctx.emitter.label(&done);
    Ok(())
}

/// Borrows only string names, leaving invalid-name diagnostics to ordinary method dispatch.
fn load_dynamic_method_name(ctx: &mut FunctionContext<'_>, method: ValueId, done: &str) -> Result<bool> {
    match ctx.value_php_type(method)?.codegen_repr() {
        PhpType::Str => { ctx.load_value_to_result(method)?; }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_result(method)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            let string = ctx.next_label("dynamic_abstract_method_lookup_string");
            enums::emit_mixed_tag_branch(ctx, abi::int_result_reg(ctx.emitter), 1, &string);
            abi::emit_jump(ctx.emitter, done);
            ctx.emitter.label(&string);
            if ctx.emitter.target.arch == Arch::X86_64 {
                enums::emit_move_reg(ctx, "rax", "rdi");
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}
