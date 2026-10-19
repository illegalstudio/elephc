//! Purpose:
//! Groups dispatched methods into override families and answers which members share one ABI.
//!
//! Called from:
//! - `Checker::unify_override_family_returns` in `crate::types::checker::override_returns`,
//!   so a caller's inferred type agrees with whatever override the object carries.
//! - `crate::ir_lower::program::virtual_abi`, which gives each family one EIR call ABI.
//!
//! Key details:
//! - A vtable call is typed and compiled against the RECEIVER's declared class, but it runs the
//!   override the object carries. Both halves must reach the same family answer, so the grouping
//!   lives here rather than in either caller.
//! - A family is every class that declares or inherits a non-private, slot-dispatched method from
//!   the same topmost ancestor. Constructors, magic hooks and instantiated generic methods keep
//!   their own contracts and are never grouped.
//! - An abstract declaration without a declared return is a PLACEHOLDER: its recorded return type
//!   is not a contract, and it takes whatever the implementations agree on.

use std::collections::{BTreeMap, HashMap};

use crate::names::php_symbol_key;
use crate::parser::ast::Visibility;
use crate::span::Span;
use crate::types::{ClassInfo, FunctionSig, PhpType};

/// One member of an override family, as seen through its own class.
pub(crate) struct FamilyMember {
    pub class_name: String,
    pub sig: FunctionSig,
    /// Abstract here, with no declared return to stand for.
    pub placeholder: bool,
    /// The class whose body this member dispatches to; `None` for an abstract member.
    pub impl_class: Option<String>,
    /// Declaration span of that body; `None` for an abstract member.
    pub body_span: Option<Span>,
}

/// The shape of one override family: which classes share the dispatched method.
///
/// It depends only on the class hierarchy, never on signatures, so the checker computes it once
/// and re-reads the signatures on every method pass.
#[derive(Clone)]
pub(crate) struct FamilyGroup {
    pub method_key: String,
    pub is_static: bool,
    /// `(class, implementing class, body span)` per member, sorted by class name.
    members: Vec<(String, Option<String>, Option<Span>)>,
}

impl FamilyGroup {
    /// Returns whether every member already returns in one representation.
    pub(crate) fn returns_agree(&self, classes: &HashMap<String, ClassInfo>) -> bool {
        let mut returns = self.members.iter().filter_map(|(class_name, impl_class, _)| {
            let sig = classes.get(class_name)?.lookup(self.is_static, &self.method_key)?;
            (impl_class.is_some() || sig.declared_return).then_some(&sig.return_type)
        });
        let Some(first) = returns.next() else {
            return true;
        };
        returns.all(|ty| same_abi(ty, first))
    }

    /// Returns the member class names.
    pub(crate) fn class_names(&self) -> impl Iterator<Item = &String> {
        self.members.iter().map(|(class_name, _, _)| class_name)
    }

    /// Reads every member's current signature.
    pub(crate) fn members(&self, classes: &HashMap<String, ClassInfo>) -> Vec<FamilyMember> {
        self.members
            .iter()
            .filter_map(|(class_name, impl_class, body_span)| {
                let sig = classes.get(class_name)?.lookup(self.is_static, &self.method_key)?.clone();
                Some(FamilyMember {
                    class_name: class_name.clone(),
                    placeholder: impl_class.is_none() && !sig.declared_return,
                    impl_class: impl_class.clone(),
                    body_span: *body_span,
                    sig,
                })
            })
            .collect()
    }
}

/// Signature lookup on the instance or static table.
trait MethodTable {
    fn lookup(&self, is_static: bool, method_key: &str) -> Option<&FunctionSig>;
}

impl MethodTable for ClassInfo {
    fn lookup(&self, is_static: bool, method_key: &str) -> Option<&FunctionSig> {
        methods(self, is_static).get(method_key)
    }
}

/// Returns every instance and static override family with more than one member, in stable order.
///
/// A family reaching a builtin or synthetic body is left out: that body keeps its fixed contract.
pub(crate) fn groups(classes: &HashMap<String, ClassInfo>) -> Vec<FamilyGroup> {
    let mut result = Vec::new();
    for is_static in [false, true] {
        let mut families: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
        for (class_name, info) in classes {
            for method_key in methods(info, is_static).keys() {
                if !is_dispatched(info, method_key, is_static) {
                    continue;
                }
                let mut root = class_name.clone();
                while let Some(parent) = classes.get(&root).and_then(|info| info.parent.clone()) {
                    match classes.get(&parent) {
                        Some(parent_info)
                            if methods(parent_info, is_static).contains_key(method_key)
                                && is_dispatched(parent_info, method_key, is_static) =>
                        {
                            root = parent;
                        }
                        _ => break,
                    }
                }
                families
                    .entry((root, method_key.clone()))
                    .or_default()
                    .push(class_name.clone());
            }
        }
        'family: for ((_, method_key), mut names) in families {
            if names.len() < 2 {
                continue;
            }
            names.sort();
            let mut members = Vec::with_capacity(names.len());
            for class_name in names {
                let info = &classes[&class_name];
                let impl_class = if is_static {
                    info.static_method_impl_classes.get(&method_key)
                } else {
                    info.method_impl_classes.get(&method_key)
                };
                let body_span = match impl_class {
                    Some(owner) => match source_decl_span(classes, owner, &method_key, is_static) {
                        Some(span) => Some(span),
                        None => continue 'family,
                    },
                    None => None,
                };
                members.push((class_name, impl_class.cloned(), body_span));
            }
            result.push(FamilyGroup { method_key, is_static, members });
        }
    }
    result
}

/// Returns the return type every caller can share, or `None` when no member has a body yet.
///
/// Implementations that agree on one representation keep their type. Otherwise the family
/// travels boxed: when one member's own return is already a boxed type naming every other
/// member's value (`?N` over an override returning `N`, `static|false` over `static`), that type
/// is shared, so callers keep its precise PHP type (property reads, `=== null` narrowing) instead
/// of an untyped `mixed`; anything else is `Mixed`.
pub(crate) fn agreed_return(members: &[FamilyMember]) -> Option<PhpType> {
    let concrete: Vec<&PhpType> = members
        .iter()
        .filter(|member| !member.placeholder)
        .map(|member| &member.sig.return_type)
        .collect();
    let first = *concrete.first()?;
    if concrete.iter().all(|ty| same_abi(ty, first)) {
        return Some(first.clone());
    }
    let boxed_cover = concrete.iter().find(|candidate| {
        matches!(candidate, PhpType::Union(_))
            && candidate.codegen_repr() == PhpType::Mixed
            && concrete.iter().all(|ty| union_names(candidate, ty))
    });
    Some(boxed_cover.map_or(PhpType::Mixed, |ty| (*ty).clone()))
}

/// Returns whether every value of `ty` is one of the union `cover`'s members in the same
/// representation (objects of any class share one pointer representation).
fn union_names(cover: &PhpType, ty: &PhpType) -> bool {
    let PhpType::Union(members) = cover else {
        return false;
    };
    let member_of = |ty: &PhpType| members.iter().any(|member| same_abi(member, ty));
    match ty {
        PhpType::Union(tys) => tys.iter().all(member_of),
        other => member_of(other),
    }
}

/// Returns the boxed return a member shares with its family, as precise as its own return.
///
/// An already boxed return (`?static` resolved to the member's class, `int|string`) travels as it
/// is. A raw one takes the family's boxed union with its own value in place of the members of the
/// same representation: `static` under `static|false` stays the member's class (`C|false`), so a
/// call through the member reads the properties only it declares. A plain `Mixed` agreement stays
/// `Mixed`.
pub(crate) fn boxed_member_return(agreed: &PhpType, own: &PhpType) -> PhpType {
    if own.codegen_repr() == PhpType::Mixed {
        return own.clone();
    }
    let PhpType::Union(cover) = agreed else {
        return agreed.clone();
    };
    let mut refined: Vec<PhpType> = Vec::with_capacity(cover.len());
    let mut placed = false;
    for member in cover {
        if same_abi(member, own) {
            if !placed {
                refined.push(own.clone());
                placed = true;
            }
        } else {
            refined.push(member.clone());
        }
    }
    let refined = PhpType::Union(refined);
    if placed && refined.codegen_repr() == PhpType::Mixed {
        refined
    } else {
        agreed.clone()
    }
}

/// Returns whether a declared parameter type is a union PHP would coerce a scalar argument into
/// (`?string`, `string|array`, `int|float`): it names a scalar member, so a value outside the
/// union is converted rather than passed as is.
pub(crate) fn union_needs_scalar_coercion(ty: &PhpType) -> bool {
    let PhpType::Union(members) = ty else {
        return false;
    };
    ty.codegen_repr() == PhpType::Mixed
        && members.iter().any(|member| {
            matches!(member, PhpType::Int | PhpType::Float | PhpType::Str | PhpType::Bool | PhpType::False)
        })
}

/// Returns whether two types travel in the same physical representation.
pub(crate) fn same_abi(left: &PhpType, right: &PhpType) -> bool {
    left.reference_payload_compatible(right) && right.reference_payload_compatible(left)
}

/// Returns whether calls to `method_key` on `info` dispatch through a vtable slot.
fn is_dispatched(info: &ClassInfo, method_key: &str, is_static: bool) -> bool {
    let (visibility, slots) = if is_static {
        (&info.static_method_visibilities, &info.static_vtable_slots)
    } else {
        (&info.method_visibilities, &info.vtable_slots)
    };
    !method_key.starts_with("__")
        && !method_key.contains('<')
        && visibility.get(method_key) != Some(&Visibility::Private)
        && slots.contains_key(method_key)
}

/// Returns the declaration span of a user-source method body, or `None` for synthetic bodies.
fn source_decl_span(
    classes: &HashMap<String, ClassInfo>,
    owner: &str,
    method_key: &str,
    is_static: bool,
) -> Option<Span> {
    classes
        .get(owner)?
        .method_decls
        .iter()
        .find(|method| method.is_static == is_static && php_symbol_key(&method.name) == method_key)
        .filter(|method| method.span.line != 0)
        .map(|method| method.span)
}

/// Selects the instance or static method signature table.
pub(crate) fn methods(info: &ClassInfo, is_static: bool) -> &HashMap<String, FunctionSig> {
    if is_static {
        &info.static_methods
    } else {
        &info.methods
    }
}

/// Mutable counterpart of `methods`.
pub(crate) fn methods_mut(info: &mut ClassInfo, is_static: bool) -> &mut HashMap<String, FunctionSig> {
    if is_static {
        &mut info.static_methods
    } else {
        &mut info.methods
    }
}
