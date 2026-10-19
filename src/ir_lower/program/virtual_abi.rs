//! Purpose:
//! Gives every override family of a dispatched method one shared EIR call ABI.
//!
//! Called from:
//! - `crate::ir_lower::program::metadata::populate_metadata()`, after per-class normalization.
//!
//! Key details:
//! - A vtable call is compiled against the RECEIVER's declared class signature, but it jumps to
//!   whichever override the object carries. When an override passes or returns a value in another
//!   representation (an untyped `$v` boxed as Mixed against an `int $v` slot, a body returning a
//!   string where the abstract placeholder said int), the caller read a pointer as an integer.
//! - Families come from `crate::types::override_families`, the grouping the checker also uses.
//!   A parameter or return whose representation differs inside a family becomes Mixed in every
//!   member; an abstract placeholder takes the representation all implementations agree on.
//! - A body whose DECLARED scalar parameter or return was boxed this way loses the coercion its
//!   call boundary used to apply (`f(string $v)` receiving `12345`). The boxed declarations are
//!   returned as `FamilyBodyContract`s, and the body re-applies PHP's coercion on entry and
//!   before boxing its result (`crate::ir_lower::function::lower_class_method`).
//! - A family whose members disagree in a shape no boxing can bridge (arity, by-reference or
//!   variadic positions, a by-reference or generator return) is refused with a diagnostic, and so
//!   is a body whose declared union parameter would need PHP's union coercion of a boxed argument.
//! - A boxed return keeps each member's own precise type (`static` stays the member's class), and
//!   a member that implements an interface declaring the method with a raw return is refused: an
//!   interface call reads the result with the interface's layout and nothing unboxes it.

use std::collections::HashMap;

use crate::span::Span;
use crate::types::override_families::{self, FamilyMember};
use crate::types::{ClassInfo, FunctionSig, InterfaceInfo, PhpType};

pub(crate) use crate::ir::FamilyBodyContract;

/// Key of a method body in `FamilyBodyContracts`: `(class, method key, is_static)`.
pub(crate) type FamilyBodyKey = (String, String, bool);

/// Every boxed method body's declared contract.
pub(crate) type FamilyBodyContracts = HashMap<FamilyBodyKey, FamilyBodyContract>;

/// Unifies parameter and return ABIs across every instance and static override family.
pub(super) fn unify_override_family_abis(
    classes: &mut HashMap<String, ClassInfo>,
    interfaces: &HashMap<String, InterfaceInfo>,
) -> FamilyBodyContracts {
    let mut contracts = FamilyBodyContracts::new();
    for group in override_families::groups(classes) {
        let members = group.members(classes);
        if unify_family(classes, &group.method_key, &members, group.is_static, &mut contracts) {
            refuse_raw_interface_returns(classes, interfaces, &group.method_key, &members, group.is_static);
        }
    }
    contracts
}

/// Refuses a member whose return the family boxed while an interface it implements declares the
/// method with a raw (unboxed) return: a call through that interface reads the result with the
/// interface's layout, and the interface wrappers only box, never unbox.
fn refuse_raw_interface_returns(
    classes: &HashMap<String, ClassInfo>,
    interfaces: &HashMap<String, InterfaceInfo>,
    method_key: &str,
    members: &[FamilyMember],
    is_static: bool,
) {
    for member in members {
        if member.sig.return_type.codegen_repr() == PhpType::Mixed {
            continue;
        }
        let Some(info) = classes.get(&member.class_name) else {
            continue;
        };
        let Some(boxed) = override_families::methods(info, is_static).get(method_key) else {
            continue;
        };
        if boxed.return_type.codegen_repr() != PhpType::Mixed {
            continue;
        }
        for interface_name in &info.interfaces {
            let Some(interface_sig) = interfaces.get(interface_name).and_then(|interface| {
                if is_static {
                    interface.static_methods.get(method_key)
                } else {
                    interface.methods.get(method_key)
                }
            }) else {
                continue;
            };
            let interface_repr = interface_sig.return_type.codegen_repr();
            if interface_repr == PhpType::Mixed || interface_repr == PhpType::Void {
                continue;
            }
            refuse(
                std::slice::from_ref(member),
                method_key,
                &format!(
                    "class {} implements interface {} whose {}() returns {}, but its override family returns the boxed {} (not yet supported)",
                    member.class_name,
                    interface_name,
                    method_key,
                    interface_sig.return_type,
                    boxed.return_type
                ),
            );
            return;
        }
    }
}

/// Rewrites one family so every member shares each parameter and the return representation.
///
/// Returns whether the family was unified (it was not refused).
fn unify_family(
    classes: &mut HashMap<String, ClassInfo>,
    method_key: &str,
    members: &[FamilyMember],
    is_static: bool,
    contracts: &mut FamilyBodyContracts,
) -> bool {
    let Some(first) = members.first().map(|member| &member.sig) else {
        return false;
    };
    let return_type = override_families::agreed_return(members);
    // A member body boxes its own result only when the shared return became Mixed under it;
    // a placeholder has no body and just takes the agreed type.
    let return_widened = return_type
        .as_ref()
        .is_some_and(|ty| ty.codegen_repr() == PhpType::Mixed)
        && members.iter().any(|member| {
            !member.placeholder && member.sig.return_type.codegen_repr() != PhpType::Mixed
        });
    let shape_differs = members.iter().any(|member| !same_shape(&member.sig, first));
    let differing: Vec<usize> = (0..first.params.len())
        .filter(|&index| {
            members.iter().any(|member| {
                member.sig.params.get(index).is_none_or(|(_, ty)| {
                    !override_families::same_abi(ty, &first.params[index].1)
                })
            })
        })
        .collect();
    if shape_differs {
        // Nothing to bridge when every shared position already travels the same way.
        if differing.is_empty() && !return_widened {
            return false;
        }
        refuse(members, method_key, "different parameter shapes (arity, by-reference or variadic)");
        return false;
    }
    let fixed = fixed_positions(first);
    if differing.iter().any(|index| !fixed.contains(index)) {
        refuse(members, method_key, "a by-reference or variadic parameter in different representations");
        return false;
    }
    if return_widened
        && members.iter().any(|member| {
            !member.placeholder
                && member.sig.return_type.codegen_repr() != PhpType::Mixed
                && (member.sig.by_ref_return || member.sig.is_generator)
        })
    {
        refuse(members, method_key, "a by-reference or generator return in a different representation");
        return false;
    }
    if let Some(member) = members.iter().find(|member| {
        member.body_span.is_some()
            && differing.iter().any(|&index| {
                member.sig.declared_params.get(index).copied().unwrap_or(false)
                    && member.sig.params[index].1.codegen_repr() == PhpType::TaggedScalar
            })
    }) {
        refuse(
            std::slice::from_ref(member),
            method_key,
            "a declared nullable int parameter that would need boxing",
        );
        return false;
    }
    if let Some(member) = members.iter().find(|member| {
        member.body_span.is_some()
            && differing.iter().any(|&index| {
                member.sig.declared_params.get(index).copied().unwrap_or(false)
                    && override_families::union_needs_scalar_coercion(&member.sig.params[index].1)
            })
    }) {
        refuse(
            std::slice::from_ref(member),
            method_key,
            "a declared union parameter with scalar members that would need PHP's union coercion of a boxed argument",
        );
        return false;
    }
    for member in members {
        if member.impl_class.as_deref() == Some(member.class_name.as_str()) {
            let contract = body_contract(&member.sig, &differing, return_widened);
            if !contract.params.is_empty() || contract.declared_return.is_some() {
                contracts.insert(
                    (member.class_name.clone(), method_key.to_string(), is_static),
                    contract,
                );
            }
        }
        let Some(sig) = classes
            .get_mut(&member.class_name)
            .and_then(|info| override_families::methods_mut(info, is_static).get_mut(method_key))
        else {
            continue;
        };
        for &index in &differing {
            sig.params[index].1 = PhpType::Mixed;
        }
        // A placeholder takes the agreed type; a body keeps its own unless the shared return
        // became Mixed under it (objects of different classes already share one pointer ABI).
        if let Some(return_type) = &return_type {
            if member.placeholder {
                sig.return_type = return_type.clone();
            } else if return_widened {
                sig.return_type = override_families::boxed_member_return(return_type, &member.sig.return_type);
            }
        }
    }
    true
}

/// Returns the declared scalar parameters and return a boxed body must coerce itself.
fn body_contract(sig: &FunctionSig, widened: &[usize], return_widened: bool) -> FamilyBodyContract {
    let params = widened
        .iter()
        .filter(|&&index| sig.declared_params.get(index).copied().unwrap_or(false))
        .filter_map(|&index| {
            let ty = &sig.params[index].1;
            is_coerced_scalar(ty).then(|| (index, ty.clone()))
        })
        .collect();
    let declared_return = (return_widened
        && sig.declared_return
        && is_coerced_scalar(&sig.return_type))
    .then(|| sig.return_type.clone());
    FamilyBodyContract { params, declared_return }
}

/// Returns whether a declared type is a scalar PHP coerces at a call or return boundary.
fn is_coerced_scalar(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Int | PhpType::Float | PhpType::Str | PhpType::Bool
    )
}

/// Returns whether two signatures pass arguments in the same positions and modes.
fn same_shape(left: &FunctionSig, right: &FunctionSig) -> bool {
    left.params.len() == right.params.len()
        && left.ref_params == right.ref_params
        && left.variadic.is_some() == right.variadic.is_some()
        && crate::func_args::sig_collects_surplus_args(left)
            == crate::func_args::sig_collects_surplus_args(right)
        && crate::func_args::sig_has_hidden_argc_param(left)
            == crate::func_args::sig_has_hidden_argc_param(right)
}

/// Returns the by-value positions boxing may widen: not by-reference, not the variadic or
/// generated `func_args` slots.
fn fixed_positions(sig: &FunctionSig) -> Vec<usize> {
    let generated = usize::from(sig.variadic.is_some())
        + usize::from(crate::func_args::sig_collects_surplus_args(sig))
        + usize::from(crate::func_args::sig_has_hidden_argc_param(sig));
    let regular = sig.params.len().saturating_sub(generated);
    (0..regular)
        .filter(|&index| !sig.ref_params.get(index).copied().unwrap_or(false))
        .collect()
}

/// Records the refusal at the first member body (or a dummy span when the family has none).
fn refuse(members: &[FamilyMember], method_key: &str, reason: &str) {
    let span = members
        .iter()
        .find_map(|member| member.body_span)
        .unwrap_or_else(Span::dummy);
    crate::ir_lower::diagnostics::refuse(
        span,
        &format!(
            "Overrides of method '{}' cannot share one virtual call ABI: {}",
            method_key, reason
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds an untyped by-value instance signature with `params` positions.
    fn sig(params: &[(&str, PhpType)], ref_params: &[bool], return_type: PhpType) -> FunctionSig {
        FunctionSig {
            params: params.iter().map(|(name, ty)| (name.to_string(), ty.clone())).collect(),
            param_type_exprs: vec![None; params.len()],
            param_attributes: vec![Vec::new(); params.len()],
            defaults: vec![None; params.len()],
            return_type,
            declared_return: false,
            by_ref_return: false,
            ref_params: ref_params.to_vec(),
            declared_params: vec![false; params.len()],
            variadic: None,
            deprecation: None,
            is_generator: false,
        }
    }

    /// Wraps a signature as a family member with a source body.
    fn member(class_name: &str, sig: FunctionSig) -> FamilyMember {
        FamilyMember {
            class_name: class_name.to_string(),
            sig,
            placeholder: false,
            impl_class: Some(class_name.to_string()),
            body_span: Some(Span::new(1, 1)),
        }
    }

    /// A family whose members differ in by-reference positions AND representations is refused
    /// instead of being skipped with its call ABI still divergent.
    #[test]
    fn differing_shapes_with_divergent_abis_are_refused() {
        crate::ir_lower::diagnostics::begin_collection();
        let members = [
            member("A", sig(&[("x", PhpType::Int)], &[true], PhpType::Int)),
            member("B", sig(&[("x", PhpType::Mixed)], &[false], PhpType::Str)),
        ];
        let mut classes = HashMap::new();
        let mut contracts = FamilyBodyContracts::new();
        unify_family(&mut classes, "f", &members, false, &mut contracts);
        let error = crate::ir_lower::diagnostics::take_first().expect("family must be refused");
        assert!(error.message.contains("cannot share one virtual call ABI"), "{}", error.message);
    }

    /// A family whose shapes differ but whose shared positions already agree needs no bridge.
    #[test]
    fn differing_shapes_with_agreeing_abis_are_accepted() {
        crate::ir_lower::diagnostics::begin_collection();
        let members = [
            member("A", sig(&[("x", PhpType::Int)], &[true], PhpType::Int)),
            member("B", sig(&[("x", PhpType::Int)], &[false], PhpType::Int)),
        ];
        let mut classes = HashMap::new();
        let mut contracts = FamilyBodyContracts::new();
        unify_family(&mut classes, "f", &members, false, &mut contracts);
        assert!(crate::ir_lower::diagnostics::take_first().is_none());
    }
}
