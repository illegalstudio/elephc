//! Purpose:
//! Keeps native binding metadata attached to the scope owner selected by codegen.
//!
//! Called from:
//! - Validation of scope producer, copy and retirement instructions.
//!
//! Key details:
//! - Main owns its scope through EvalGlobalScope; function activations use EvalScope.

use super::*;

/// Requires the same owning hidden slot that backend scope access will consume.
pub(super) fn has_scope_owner(function: &Function) -> bool {
    let kind = if function.flags.is_main { LocalKind::EvalGlobalScope } else { LocalKind::EvalScope };
    function.locals.iter().any(|local| local.kind == kind)
}

/// Selects the semantic owner of checked binding effects without coupling source evidence to Locals.
pub(super) fn required_binding_feature(id: InstId, inst: &Instruction,
    profile: elephc_semantics::SemanticProfile,
) -> Result<u64, ValidationError> {
    use elephc_semantics::StrictFeature;
    if let Some(Immediate::EvalScopeStore(metadata)) = &inst.immediate {
        if metadata.role != crate::ir::EvalBindingRole::Contract {
            if inst.op != Op::EvalScopeBindLocalType || metadata.contract.as_ref().is_none_or(|ty|
                crate::types::native_type_spec::logical_type(ty, false).is_none()) {
                return Err(ValidationError::UnexpectedImmediate(id));
            }
            return Ok(if profile.contains(StrictFeature::Mixed) { StrictFeature::Mixed.bit() }
                else { StrictFeature::ArrayTypes.bit() });
        }
    }
    Ok(if matches!(inst.op, Op::BindLocalRefContract | Op::EvalScopeBindingType
        | Op::EvalScopeBindLocalType | Op::EvalScopeStartBinding | Op::EvalScopeEndBinding
        | Op::EvalScopeCopyBindingType) || matches!(&inst.immediate,
            Some(Immediate::EvalScopeStore(metadata)) if metadata.contract.is_some()) {
        StrictFeature::Locals.bit()
    } else { 0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The alternate hidden scope cannot substitute for this activation's actual owner.
    #[test]
    fn binding_metadata_requires_the_selected_scope_owner() {
        for main in [false, true] {
            for owner in [LocalKind::EvalScope, LocalKind::EvalGlobalScope] {
                let mut function = Function::new("scope_owner".into(), IrType::Void, PhpType::Void);
                function.flags.is_main = main;
                function.add_local(Some("scope".into()), IrType::I64, PhpType::Int, owner);
                {
                    let mut builder = crate::ir::Builder::new(&mut function);
                    let entry = builder.create_named_block("entry", vec![]);
                    builder.position_at_end(entry);
                    builder.emit(Op::EvalScopeStartBinding, vec![],
                        Some(Immediate::EvalScopeStore(Box::new(crate::ir::EvalScopeStoreImmediate {
                            name: crate::ir::DataId::from_raw(0), contract: None, role: crate::ir::EvalBindingRole::Contract,
                        }))), IrType::Void, PhpType::Void, Ownership::NonHeap);
                    builder.terminate(Terminator::Return { value: None });
                }
                let result = validate_function(&function);
                let expected = owner == if main { LocalKind::EvalGlobalScope } else { LocalKind::EvalScope };
                assert_eq!(result.is_ok(), expected, "main={main}, owner={owner:?}: {result:?}");
                if !expected { assert!(matches!(result, Err(ValidationError::InvalidLocalReferenceContract(_)))); }
            }
        }
    }
}
