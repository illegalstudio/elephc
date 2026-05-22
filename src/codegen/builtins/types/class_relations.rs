//! Purpose:
//! Emits AOT metadata arrays for `class_implements`, `class_parents`, and `class_uses`.
//! Resolves class-like names from static type information and declaration tables.
//!
//! Called from:
//! - `crate::codegen::builtins::types::emit()`
//!
//! Key details:
//! - Arguments are evaluated for side effects before the folded metadata array is materialized.
//! - Results use PHP's associative shape: each key is the same string as its value.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::arrays::{
    emit_assoc_array_literal, emit_empty_assoc_array_literal,
};
use crate::codegen::expr::emit_expr;
use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;
use crate::types::{ClassInfo, InterfaceInfo, PhpType};

enum ClassLikeTarget {
    Class(String),
    Interface(String),
    Trait(String),
    Unknown,
}

pub fn emit(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment(&format!("{}() — AOT class metadata snapshot", name));

    let first_ty = args.first().map(|arg| emit_expr(arg, emitter, ctx, data));
    for arg in args.iter().skip(1) {
        emit_expr(arg, emitter, ctx, data);
    }

    let target = resolve_target(args.first(), first_ty.as_ref(), ctx);
    let names = relation_names(name, &target, ctx)?;
    emit_assoc_string_set(&names, args.first().map(|arg| arg.span), emitter, ctx, data);
    Some(class_relation_array_type())
}

fn class_relation_array_type() -> PhpType {
    PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Str),
    }
}

fn resolve_target(arg: Option<&Expr>, arg_ty: Option<&PhpType>, ctx: &Context) -> ClassLikeTarget {
    if let Some(Expr {
        kind: ExprKind::StringLiteral(raw),
        ..
    }) = arg
    {
        if let Some(name) = lookup_class_name(ctx, raw) {
            return ClassLikeTarget::Class(name);
        }
        if let Some(name) = lookup_interface_name(ctx, raw) {
            return ClassLikeTarget::Interface(name);
        }
        if let Some(name) = lookup_trait_name(ctx, raw) {
            return ClassLikeTarget::Trait(name);
        }
        return ClassLikeTarget::Unknown;
    }

    if let Some(PhpType::Object(class_name)) = arg_ty {
        if let Some(name) = lookup_class_name(ctx, class_name) {
            return ClassLikeTarget::Class(name);
        }
    }

    ClassLikeTarget::Unknown
}

fn relation_names(name: &str, target: &ClassLikeTarget, ctx: &Context) -> Option<Vec<String>> {
    match name {
        "class_implements" => Some(class_implements(target, ctx)),
        "class_parents" => Some(class_parents(target, ctx)),
        "class_uses" => Some(class_uses(target, ctx)),
        _ => None,
    }
}

fn class_implements(target: &ClassLikeTarget, ctx: &Context) -> Vec<String> {
    match target {
        ClassLikeTarget::Class(class_name) => lookup_class(ctx, class_name)
            .map(|info| info.interfaces.clone())
            .unwrap_or_default(),
        ClassLikeTarget::Interface(interface_name) => {
            let mut names = Vec::new();
            collect_interface_parents(ctx, interface_name, &mut names);
            names
        }
        ClassLikeTarget::Trait(_) | ClassLikeTarget::Unknown => Vec::new(),
    }
}

fn class_parents(target: &ClassLikeTarget, ctx: &Context) -> Vec<String> {
    let ClassLikeTarget::Class(class_name) = target else {
        return Vec::new();
    };

    let mut names = Vec::new();
    let mut current = class_name.clone();
    while let Some(info) = lookup_class(ctx, &current) {
        let Some(parent) = &info.parent else {
            break;
        };
        let parent_name = lookup_class_name(ctx, parent).unwrap_or_else(|| parent.clone());
        names.push(parent_name.clone());
        current = parent_name;
    }
    names
}

fn class_uses(target: &ClassLikeTarget, ctx: &Context) -> Vec<String> {
    match target {
        ClassLikeTarget::Class(class_name) => lookup_class(ctx, class_name)
            .map(|info| info.used_traits.clone())
            .unwrap_or_default(),
        ClassLikeTarget::Trait(trait_name) => crate::codegen::declared_trait_uses(trait_name),
        ClassLikeTarget::Interface(_) | ClassLikeTarget::Unknown => Vec::new(),
    }
}

fn collect_interface_parents(ctx: &Context, interface_name: &str, names: &mut Vec<String>) {
    let Some(interface) = lookup_interface(ctx, interface_name) else {
        return;
    };
    for parent in &interface.parents {
        let parent_name = lookup_interface_name(ctx, parent).unwrap_or_else(|| parent.clone());
        if !names
            .iter()
            .any(|name| php_symbol_key(name) == php_symbol_key(&parent_name))
        {
            names.push(parent_name.clone());
            collect_interface_parents(ctx, &parent_name, names);
        }
    }
}

fn emit_assoc_string_set(
    names: &[String],
    span: Option<Span>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    if names.is_empty() {
        emit_empty_assoc_array_literal(PhpType::Str, PhpType::Str, emitter);
        return;
    }

    let span = span.unwrap_or_else(Span::dummy);
    let pairs: Vec<(Expr, Expr)> = names
        .iter()
        .map(|name| {
            let key = Expr::new(ExprKind::StringLiteral(name.clone()), span);
            let value = Expr::new(ExprKind::StringLiteral(name.clone()), span);
            (key, value)
        })
        .collect();
    emit_assoc_array_literal(&pairs, emitter, ctx, data);
}

fn lookup_class_name(ctx: &Context, raw: &str) -> Option<String> {
    lookup_folded(ctx.classes.keys(), raw)
}

fn lookup_interface_name(ctx: &Context, raw: &str) -> Option<String> {
    lookup_folded(ctx.interfaces.keys(), raw)
}

fn lookup_trait_name(ctx: &Context, raw: &str) -> Option<String> {
    lookup_folded(ctx.traits.iter(), raw)
}

fn lookup_folded<'a>(names: impl Iterator<Item = &'a String>, raw: &str) -> Option<String> {
    let clean = raw.trim_start_matches('\\');
    let key = php_symbol_key(clean);
    names
        .into_iter()
        .find(|name| php_symbol_key(name.trim_start_matches('\\')) == key)
        .cloned()
}

fn lookup_class<'a>(ctx: &'a Context, raw: &str) -> Option<&'a ClassInfo> {
    let name = lookup_class_name(ctx, raw)?;
    ctx.classes.get(&name)
}

fn lookup_interface<'a>(ctx: &'a Context, raw: &str) -> Option<&'a InterfaceInfo> {
    let name = lookup_interface_name(ctx, raw)?;
    ctx.interfaces.get(&name)
}
