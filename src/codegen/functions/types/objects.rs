//! Purpose:
//! Computes class, interface, enum, and object relationships needed by code generation.
//! Keeps emission-time type decisions separate from instruction lowering.
//!
//! Called from:
//! - `crate::codegen::functions::types`
//!
//! Key details:
//! - Results must agree with `crate::types` so local slots and runtime value shapes are selected correctly.

use crate::codegen::context::Context;
use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, StaticReceiver};
use crate::types::{FunctionSig, PhpType};

use super::infer_local_type;
use super::union::merge_union_members;

/// Extract the canonical object class name from a type that statically
/// resolves to an object — either directly (`Object("Foo")`) or as the
/// single object member of a nullable / object-only union
/// (`Union([Object("Foo"), Void])`). Returns `None` for `Mixed` and any
/// union that mixes multiple classes or non-object members.
pub(crate) fn singular_object_class(ty: &PhpType) -> Option<&str> {
    match ty {
        PhpType::Object(name) => Some(name.as_str()),
        PhpType::Union(members) => {
            let mut found: Option<&str> = None;
            for member in members {
                match member {
                    PhpType::Void => continue,
                    PhpType::Object(name) => {
                        if found.is_some_and(|existing| existing != name.as_str()) {
                            return None;
                        }
                        found = Some(name.as_str());
                    }
                    _ => return None,
                }
            }
            found
        }
        _ => None,
    }
}

pub(super) fn infer_property_access_type(
    object: &Expr,
    property: &str,
    sig: &FunctionSig,
    ctx: Option<&Context>,
) -> PhpType {
    let _ = property;
    if let Some(c) = ctx {
        if let Some((cn, nullable)) = nullsafe_context_class(object, sig, c) {
            // stdClass property access is dynamic; surface Mixed up-front so
            // chained accesses (`$obj->nested->x`) flow through the dynamic
            // dispatch instead of falling back to the integer default.
            if crate::types::checker::builtin_stdclass::is_stdclass(&cn) {
                return if nullable {
                    merge_union_members(vec![PhpType::Mixed, PhpType::Void])
                } else {
                    PhpType::Mixed
                };
            }
            if let Some(ci) = c.classes.get(&cn) {
                if let Some((_, ty)) = ci.properties.iter().find(|(n, _)| n == property) {
                    return if nullable {
                        merge_union_members(vec![ty.clone(), PhpType::Void])
                    } else {
                        ty.clone()
                    };
                }
                if let Some(sig) = ci.methods.get("__get") {
                    return if nullable {
                        merge_union_members(vec![sig.return_type.clone(), PhpType::Void])
                    } else {
                        sig.return_type.clone()
                    };
                }
            }
        }
        let obj_ty = infer_local_type(object, sig, Some(c));
        if let PhpType::Pointer(Some(cn)) = &obj_ty {
            if let Some(ci) = c.extern_classes.get(cn) {
                if let Some(field) = ci.fields.iter().find(|field| field.name == *property) {
                    return field.php_type.clone();
                }
            }
            if let Some(ci) = c.packed_classes.get(cn) {
                if let Some(field) = ci.fields.iter().find(|field| field.name == *property) {
                    return field.php_type.clone();
                }
            }
        }
        // Property access on a Mixed receiver evaluates to Mixed at runtime
        // (the helper unboxes and dispatches through the stdClass hash). Match
        // that here so chained property accesses keep flowing through the
        // dynamic dispatch path instead of degrading to PhpType::Int.
        if matches!(obj_ty, PhpType::Mixed) {
            return PhpType::Mixed;
        }
    }
    PhpType::Int
}

pub(super) fn infer_nullsafe_property_access_type(
    object: &Expr,
    property: &str,
    sig: &FunctionSig,
    ctx: Option<&Context>,
) -> PhpType {
    if let Some(c) = ctx {
        if let Some((cn, nullable)) = nullsafe_context_class(object, sig, c) {
            if let Some(ci) = c.classes.get(&cn) {
                if let Some((_, ty)) = ci.properties.iter().find(|(n, _)| n == property) {
                    return if nullable {
                        merge_union_members(vec![ty.clone(), PhpType::Void])
                    } else {
                        ty.clone()
                    };
                }
                if let Some(sig) = ci.methods.get("__get") {
                    return if nullable {
                        merge_union_members(vec![sig.return_type.clone(), PhpType::Void])
                    } else {
                        sig.return_type.clone()
                    };
                }
            }
        }
    }
    PhpType::Void
}

pub(super) fn infer_static_property_access_type(
    receiver: &StaticReceiver,
    property: &str,
    ctx: Option<&Context>,
) -> PhpType {
    if let Some(c) = ctx {
        let Some(class_name) = class_name_from_static_receiver(receiver, c) else {
            return PhpType::Int;
        };
        if let Some(ci) = c.classes.get(&class_name) {
            if let Some((_, ty)) = ci
                .static_properties
                .iter()
                .find(|(name, _)| name == property)
            {
                return ty.clone();
            }
        }
    }
    PhpType::Int
}

pub(super) fn infer_method_call_type(
    object: &Expr,
    method: &str,
    sig: &FunctionSig,
    ctx: Option<&Context>,
) -> PhpType {
    if let Some(c) = ctx {
        let obj_ty = infer_local_type(object, sig, Some(c));
        if let Some(cn) = singular_object_class(&obj_ty) {
            if let Some(ci) = c.classes.get(cn) {
                let method_key = php_symbol_key(method);
                if let Some(msig) = ci.methods.get(&method_key) {
                    return msig.return_type.clone();
                }
                if let Some(msig) = ci.methods.get("__call") {
                    return msig.return_type.clone();
                }
            }
        }
    }
    PhpType::Int
}

pub(super) fn infer_nullsafe_method_call_type(
    object: &Expr,
    method: &str,
    sig: &FunctionSig,
    ctx: Option<&Context>,
) -> PhpType {
    if let Some(c) = ctx {
        if let Some((cn, nullable)) = nullsafe_context_class(object, sig, c) {
            if let Some(ci) = c.classes.get(&cn) {
                let method_key = php_symbol_key(method);
                if let Some(msig) = ci.methods.get(&method_key) {
                    return if nullable {
                        merge_union_members(vec![msig.return_type.clone(), PhpType::Void])
                    } else {
                        msig.return_type.clone()
                    };
                }
                if let Some(msig) = ci.methods.get("__call") {
                    return if nullable {
                        merge_union_members(vec![msig.return_type.clone(), PhpType::Void])
                    } else {
                        msig.return_type.clone()
                    };
                }
            }
        }
    }
    PhpType::Void
}

pub(super) fn infer_static_method_call_type(
    receiver: &StaticReceiver,
    method: &str,
    ctx: Option<&Context>,
) -> PhpType {
    if let Some(c) = ctx {
        let Some(class_name) = class_name_from_static_receiver(receiver, c) else {
            return PhpType::Int;
        };
        if let Some(ci) = c.classes.get(&class_name) {
            if let Some(msig) = ci.static_methods.get(method) {
                return msig.return_type.clone();
            }
        }
    }
    PhpType::Int
}

pub(super) fn infer_this_type(ctx: Option<&Context>) -> PhpType {
    if let Some(c) = ctx {
        if let Some(cn) = &c.current_class {
            return PhpType::Object(cn.clone());
        }
    }
    PhpType::Object(String::new())
}

fn nullsafe_context_class(
    object: &Expr,
    sig: &FunctionSig,
    ctx: &Context,
) -> Option<(String, bool)> {
    match infer_local_type(object, sig, Some(ctx)) {
        PhpType::Object(class_name) => Some((class_name, false)),
        PhpType::Void => None,
        PhpType::Union(members) => {
            let mut class_name = None;
            let mut nullable = false;
            for member in members {
                match member {
                    PhpType::Void => nullable = true,
                    PhpType::Object(candidate) => class_name = Some(candidate),
                    _ => return None,
                }
            }
            class_name.map(|name| (name, nullable))
        }
        _ => None,
    }
}

fn class_name_from_static_receiver(receiver: &StaticReceiver, ctx: &Context) -> Option<String> {
    match receiver {
        StaticReceiver::Named(class_name) => Some(class_name.as_str().to_string()),
        StaticReceiver::Self_ | StaticReceiver::Static => ctx.current_class.clone(),
        StaticReceiver::Parent => ctx.current_class.as_ref().and_then(|current_class| {
            ctx.classes
                .get(current_class)
                .and_then(|ci| ci.parent.as_ref())
                .cloned()
        }),
    }
}
