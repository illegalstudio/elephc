//! Purpose:
//! Shares compile-time reflection metadata helpers with the EIR pipeline.
//!
//! Called from:
//! - `crate::ir_lower::program`
//! - EIR reflection and attribute builtin lowerers.
//!
//! Key details:
//! - Attribute factory ids are deterministic over the full class metadata
//!   table so `ReflectionAttribute::newInstance()` and metadata materializers
//!   agree without runtime registration state.

use std::collections::{BTreeMap, HashMap};

use crate::names::{php_symbol_key, Name};
use crate::parser::ast::{BinOp, Expr, ExprKind, StaticReceiver, Stmt, StmtKind};
use crate::types::{AttrArgEntry, AttrArgValue, AttrKey, ClassInfo};

/// Borrowed attribute-name/argument metadata from a reflection-visible source.
pub(crate) type AttributeMetadataSource<'a> =
    (&'a [String], &'a [Option<Vec<AttrArgEntry>>]);

#[derive(Clone)]
/// Factory record for compile-time reflection attribute metadata.
/// `id` is assigned sequentially and must match across all compilation units
/// so `ReflectionAttribute::newInstance()` and codegen agree on the factory index.
pub(crate) struct ReflectionAttributeFactory {
    pub(crate) id: i64,
    pub(crate) class_name: String,
    pub(crate) args: Vec<AttrArgEntry>,
    /// True when `class_name` resolves to a real class. `newInstance()` only
    /// emits a construction branch for resolvable factories; `getArguments()`
    /// uses every factory (including non-class attributes) to return arguments.
    pub(crate) resolvable: bool,
}

/// Looks up `class_name` in `classes` using PHPsymbol-key normalization
/// (leading-backslash stripping and case-insensitive comparison).
/// Returns the canonical class name string from the HashMap key, or `None`
/// if the class is not registered.
pub(crate) fn resolve_class_name<'a>(
    classes: &'a HashMap<String, ClassInfo>,
    class_name: &str,
) -> Option<&'a str> {
    let class_key = php_symbol_key(class_name.trim_start_matches('\\'));
    classes
        .keys()
        .find(|existing| php_symbol_key(existing) == class_key)
        .map(String::as_str)
}

/// Scans every class in `classes` and collects all distinct class-level,
/// method-level, property-level, and constant-level attribute name/argument
/// pairs into a sorted vector of `ReflectionAttributeFactory` records with
/// sequential ids.
pub(crate) fn collect_attribute_factories(
    classes: &HashMap<String, ClassInfo>,
) -> Vec<ReflectionAttributeFactory> {
    collect_attribute_factories_with_extra(classes, &[])
}

/// Scans class metadata plus additional attribute metadata sources and collects
/// all distinct attribute name/argument pairs into deterministic factory records.
pub(crate) fn collect_attribute_factories_with_extra(
    classes: &HashMap<String, ClassInfo>,
    extra_attrs: &[AttributeMetadataSource<'_>],
) -> Vec<ReflectionAttributeFactory> {
    let mut unique = BTreeMap::new();
    for class_info in classes.values() {
        collect_from_attribute_lists(
            classes,
            &class_info.attribute_names,
            &class_info.attribute_args,
            &mut unique,
        );
        for (member, names) in &class_info.method_attribute_names {
            if let Some(args) = class_info.method_attribute_args.get(member) {
                collect_from_attribute_lists(classes, names, args, &mut unique);
            }
        }
        for (member, names) in &class_info.property_attribute_names {
            if let Some(args) = class_info.property_attribute_args.get(member) {
                collect_from_attribute_lists(classes, names, args, &mut unique);
            }
        }
        for (member, names) in &class_info.constant_attribute_names {
            if let Some(args) = class_info.constant_attribute_args.get(member) {
                collect_from_attribute_lists(classes, names, args, &mut unique);
            }
        }
    }
    for (names, args) in extra_attrs {
        collect_from_attribute_lists(classes, names, args, &mut unique);
    }

    unique
        .into_iter()
        .enumerate()
        .map(
            |(idx, ((class_name, args), resolvable))| ReflectionAttributeFactory {
                id: (idx as i64) + 1,
                class_name,
                args,
                resolvable,
            },
        )
        .collect()
}

/// Returns the factory id for the given attribute `attr_name` with
/// `attr_args`. Returns 0 if the class cannot be resolved or no matching
/// factory exists.
pub(crate) fn attribute_factory_id(
    classes: &HashMap<String, ClassInfo>,
    attr_name: &str,
    attr_args: &[AttrArgEntry],
) -> i64 {
    attribute_factory_id_with_extra(classes, &[], attr_name, attr_args)
}

/// Returns the factory id for an attribute, considering classes plus extra
/// metadata sources such as top-level function attributes retained by EIR.
pub(crate) fn attribute_factory_id_with_extra(
    classes: &HashMap<String, ClassInfo>,
    extra_attrs: &[AttributeMetadataSource<'_>],
    attr_name: &str,
    attr_args: &[AttrArgEntry],
) -> i64 {
    // Non-class attributes are registered under their raw name (see
    // `collect_from_attribute_lists`), so fall back to it when the name does
    // not resolve to a real class.
    let lookup_name = resolve_class_name(classes, attr_name)
        .map(|resolved| resolved.to_string())
        .unwrap_or_else(|| attr_name.to_string());
    collect_attribute_factories_with_extra(classes, extra_attrs)
        .into_iter()
        .find(|factory| factory.class_name == lookup_name && factory.args == attr_args)
        .map(|factory| factory.id)
        .unwrap_or(0)
}

/// Builds the synthetic dispatch body for `ReflectionAttribute::newInstance()`.
pub(crate) fn build_attribute_new_instance_body(classes: &HashMap<String, ClassInfo>) -> Vec<Stmt> {
    build_attribute_new_instance_body_with_extra(classes, &[])
}

/// Builds the synthetic `ReflectionAttribute::newInstance()` body using class
/// metadata plus additional attribute metadata sources.
pub(crate) fn build_attribute_new_instance_body_with_extra(
    classes: &HashMap<String, ClassInfo>,
    extra_attrs: &[AttributeMetadataSource<'_>],
) -> Vec<Stmt> {
    let span = crate::span::Span::dummy();
    let factories = collect_attribute_factories_with_extra(classes, extra_attrs);
    let mut body = Vec::new();
    for factory in factories {
        // Only resolvable attribute classes can be instantiated. Non-class
        // attributes are registered (so `getArguments()` can find them) but
        // have no construction branch here; they fall through to `return null`.
        if !factory.resolvable {
            continue;
        }
        let condition = factory_condition(factory.id);
        let then_body = vec![Stmt::new(
            StmtKind::Return(Some(Expr::new(
                ExprKind::NewObject {
                    class_name: name_from_canonical(&factory.class_name),
                    args: factory
                        .args
                        .iter()
                        .map(|entry| attr_arg_expr(&entry.value))
                        .collect(),
                },
                span,
            ))),
            span,
        )];
        body.push(Stmt::new(
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses: Vec::new(),
                else_body: None,
            },
            span,
        ));
    }
    body.push(Stmt::new(
        StmtKind::Return(Some(Expr::new(ExprKind::Null, span))),
        span,
    ));
    body
}

/// Creates `this->__factory === factory_id` for `newInstance()` dispatch routing.
fn factory_condition(factory_id: i64) -> Expr {
    let span = crate::span::Span::dummy();
    Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(Expr::new(
                ExprKind::PropertyAccess {
                    object: Box::new(Expr::new(ExprKind::This, span)),
                    property: "__factory".to_string(),
                },
                span,
            )),
            op: BinOp::StrictEq,
            right: Box::new(Expr::new(ExprKind::IntLiteral(factory_id), span)),
        },
        span,
    )
}

/// Converts one captured attribute argument into a synthetic AST expression.
/// Nested arrays (positional or associative) become the corresponding
/// array-literal expression, and symbolic references (global constant, class
/// constant, enum case) become the corresponding reference expression — all so
/// they lower through the normal, architecture-independent paths. Reference
/// names were canonicalised by name resolution at capture time, so the
/// re-emitted nodes resolve directly during lowering (the global/class constant
/// folds to its value; an enum case materializes the case object, matching
/// PHP's `ReflectionAttribute::getArguments()`).
fn attr_arg_expr(arg: &AttrArgValue) -> Expr {
    let span = crate::span::Span::dummy();
    match arg {
        AttrArgValue::Null => Expr::new(ExprKind::Null, span),
        AttrArgValue::Int(value) => Expr::new(ExprKind::IntLiteral(*value), span),
        AttrArgValue::Float(bits) => Expr::new(ExprKind::FloatLiteral(f64::from_bits(*bits)), span),
        AttrArgValue::Bool(value) => Expr::new(ExprKind::BoolLiteral(*value), span),
        AttrArgValue::Str(value) => Expr::new(ExprKind::StringLiteral(value.clone()), span),
        AttrArgValue::Array(entries) => entries_to_array_expr(entries, false),
        AttrArgValue::ConstRef(name) => {
            Expr::new(ExprKind::ConstRef(name_from_canonical(name)), span)
        }
        AttrArgValue::ScopedConst(type_name, member) => Expr::new(
            ExprKind::ScopedConstantAccess {
                receiver: StaticReceiver::Named(name_from_canonical(type_name)),
                name: member.clone(),
            },
            span,
        ),
    }
}

/// Builds an array-literal AST expression from captured attribute-arg entries.
/// When `force_assoc` is set (or any entry carries a key — a named argument or
/// explicit array key) it produces an associative `ArrayLiteralAssoc` with
/// positional entries taking their sequential integer key, matching PHP's
/// `getArguments()` ordering; otherwise it produces a positional `ArrayLiteral`.
/// `force_assoc` keeps the top-level `getArguments()` result a single array kind
/// (a hash) so its declared associative type matches the runtime value.
fn entries_to_array_expr(entries: &[AttrArgEntry], force_assoc: bool) -> Expr {
    let span = crate::span::Span::dummy();
    if force_assoc || entries.iter().any(|entry| entry.key.is_some()) {
        let mut next_index = 0i64;
        let pairs = entries
            .iter()
            .map(|entry| {
                let key = match &entry.key {
                    Some(key) => attr_key_expr(key),
                    None => {
                        let index = next_index;
                        next_index += 1;
                        Expr::new(ExprKind::IntLiteral(index), span)
                    }
                };
                (key, attr_arg_expr(&entry.value))
            })
            .collect();
        Expr::new(ExprKind::ArrayLiteralAssoc(pairs), span)
    } else {
        Expr::new(
            ExprKind::ArrayLiteral(
                entries
                    .iter()
                    .map(|entry| attr_arg_expr(&entry.value))
                    .collect(),
            ),
            span,
        )
    }
}

/// Converts a captured attribute array/named key into a synthetic AST key
/// expression.
fn attr_key_expr(key: &AttrKey) -> Expr {
    let span = crate::span::Span::dummy();
    let kind = match key {
        AttrKey::Int(value) => ExprKind::IntLiteral(*value),
        AttrKey::Str(value) => ExprKind::StringLiteral(value.clone()),
    };
    Expr::new(kind, span)
}

/// Builds the synthetic body for `ReflectionAttribute::getArguments()`. For
/// each attribute whose class resolves, it dispatches on the factory id and
/// returns the captured arguments as a lowered array literal — so named
/// arguments and associative arrays are materialized through the normal array
/// path. Attributes without a resolvable class fall back to the `$__args`
/// property populated at construction.
pub(crate) fn build_attribute_get_arguments_body(
    classes: &HashMap<String, ClassInfo>,
) -> Vec<Stmt> {
    build_attribute_get_arguments_body_with_extra(classes, &[])
}

/// Builds the synthetic `ReflectionAttribute::getArguments()` body using class
/// metadata plus additional attribute metadata sources.
pub(crate) fn build_attribute_get_arguments_body_with_extra(
    classes: &HashMap<String, ClassInfo>,
    extra_attrs: &[AttributeMetadataSource<'_>],
) -> Vec<Stmt> {
    let span = crate::span::Span::dummy();
    let factories = collect_attribute_factories_with_extra(classes, extra_attrs);
    let mut body = Vec::new();
    for factory in factories {
        let condition = factory_condition(factory.id);
        let then_body = vec![Stmt::new(
            StmtKind::Return(Some(entries_to_array_expr(&factory.args, true))),
            span,
        )];
        body.push(Stmt::new(
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses: Vec::new(),
                else_body: None,
            },
            span,
        ));
    }
    // Every attribute with supported arguments is registered as a factory
    // above (class or not), so this is only a defensive default; return an
    // empty associative array to match the declared return type.
    body.push(Stmt::new(
        StmtKind::Return(Some(entries_to_array_expr(&[], true))),
        span,
    ));
    body
}

/// Converts a canonical class string into the `Name` shape expected by `NewObject`.
fn name_from_canonical(class_name: &str) -> Name {
    Name::qualified(class_name.split('\\').map(str::to_string).collect())
}

/// Iterates over parallel `names` and `args` slices and inserts each
/// resolved (class-name, args) pair into `unique`. Skips entries where
/// args is `None` or the class name cannot be resolved.
fn collect_from_attribute_lists(
    classes: &HashMap<String, ClassInfo>,
    names: &[String],
    args: &[Option<Vec<AttrArgEntry>>],
    unique: &mut BTreeMap<(String, Vec<AttrArgEntry>), bool>,
) {
    if names.len() != args.len() {
        return;
    }
    for (idx, attr_name) in names.iter().enumerate() {
        let Some(Some(attr_args)) = args.get(idx) else {
            continue;
        };
        // Non-class attributes (`#[Foo(1)]` with no `Foo` class) still expose
        // their arguments through reflection, so they are registered under
        // their raw name with `resolvable = false`. The map value records
        // resolvability so `newInstance()` can skip them.
        let (name, resolvable) = match resolve_class_name(classes, attr_name) {
            Some(resolved) => (resolved.to_string(), true),
            None => (attr_name.clone(), false),
        };
        unique
            .entry((name, attr_args.clone()))
            .or_insert(resolvable);
    }
}
