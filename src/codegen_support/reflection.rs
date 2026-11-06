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

use crate::ir::Function;
use crate::names::{php_symbol_key, Name};
use crate::parser::ast::{BinOp, Expr, ExprKind, StaticReceiver, Stmt, StmtKind};
use crate::types::{
    collect_attribute_args, collect_attribute_names, AttrArgEntry, AttrArgValue, AttrKey,
    ClassInfo, FunctionSig,
};

/// Borrowed attribute-name/argument metadata from a reflection-visible source.
pub(crate) type AttributeMetadataSource<'a> =
    (&'a [String], &'a [Option<Vec<AttrArgEntry>>]);

/// Owned attribute-name/argument metadata, for a source only reachable through AST groups.
pub(crate) type OwnedAttributeMetadata = (Vec<String>, Vec<Option<Vec<AttrArgEntry>>>);

/// Returns the attribute lists declared on the parameters of `signature`, one per parameter that
/// carries any. `ReflectionParameter::getAttributes()` reads the same groups, so a factory built
/// from these is the one its `newInstance()` dispatches to.
pub(crate) fn parameter_attribute_metadata(signature: &FunctionSig) -> Vec<OwnedAttributeMetadata> {
    signature
        .param_attributes
        .iter()
        .filter(|groups| !groups.is_empty())
        .map(|groups| (collect_attribute_names(groups), collect_attribute_args(groups)))
        .collect()
}

/// Returns the reflection-visible attribute metadata of the lowered top-level functions: each
/// function's own attributes and its parameters'. Both factory consumers (the synthetic
/// `ReflectionAttribute` bodies and the attribute-array emitter) build from this one list, so
/// they number the factories identically.
pub(crate) fn function_attribute_metadata(functions: &[Function]) -> Vec<OwnedAttributeMetadata> {
    let mut sources = Vec::new();
    for function in functions {
        if !function.attribute_names.is_empty() {
            sources.push((function.attribute_names.clone(), function.attribute_args.clone()));
        }
        if let Some(signature) = &function.signature {
            sources.extend(parameter_attribute_metadata(signature));
        }
    }
    sources
}

/// Borrows owned attribute metadata in the shape the factory collectors take.
pub(crate) fn borrow_attribute_metadata(
    owned: &[OwnedAttributeMetadata],
) -> Vec<AttributeMetadataSource<'_>> {
    owned
        .iter()
        .map(|(names, args)| (names.as_slice(), args.as_slice()))
        .collect()
}

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
/// method-level, method-parameter-level, property-level, and constant-level
/// attribute name/argument pairs into a sorted vector of
/// `ReflectionAttributeFactory` records with sequential ids.
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
        // Method parameters carry attributes too; without a factory their `newInstance()`
        // falls through to `null`.
        for signature in class_info
            .methods
            .values()
            .chain(class_info.static_methods.values())
        {
            for (names, args) in parameter_attribute_metadata(signature) {
                collect_from_attribute_lists(classes, &names, &args, &mut unique);
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

/// The attribute factory ids of one program, keyed by resolved attribute name and captured
/// arguments.
///
/// Numbering the factories walks every class and every lowered function, so a caller that
/// materializes several attributes builds this table once and looks each attribute up in it,
/// instead of renumbering the whole program per attribute.
pub(crate) struct AttributeFactoryIds<'a> {
    classes: &'a HashMap<String, ClassInfo>,
    ids: HashMap<(String, Vec<AttrArgEntry>), i64>,
}

impl<'a> AttributeFactoryIds<'a> {
    /// Numbers the factories of `classes` plus `extra_attrs` exactly as the synthetic
    /// `ReflectionAttribute` bodies do, so the ids agree with `newInstance()`'s dispatch.
    pub(crate) fn new(
        classes: &'a HashMap<String, ClassInfo>,
        extra_attrs: &[AttributeMetadataSource<'_>],
    ) -> Self {
        let ids = collect_attribute_factories_with_extra(classes, extra_attrs)
            .into_iter()
            .map(|factory| ((factory.class_name, factory.args), factory.id))
            .collect();
        Self { classes, ids }
    }

    /// Returns the factory id of one attribute, or `0` when no factory matches it.
    pub(crate) fn id(&self, attr_name: &str, attr_args: &[AttrArgEntry]) -> i64 {
        // Non-class attributes are registered under their raw name (see
        // `collect_from_attribute_lists`), so fall back to it when the name does
        // not resolve to a real class.
        let lookup_name = resolve_class_name(self.classes, attr_name)
            .map(|resolved| resolved.to_string())
            .unwrap_or_else(|| attr_name.to_string());
        self.ids
            .get(&(lookup_name, attr_args.to_vec()))
            .copied()
            .unwrap_or(0)
    }
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
        let mut then_body = Vec::new();
        if let Some(flags) = crate::types::checker::core_attributes::flags(&factory.class_name) {
            use crate::synthetic_class::{e_binop, e_int, e_new_fq, e_str, e_this_prop, s_if, s_throw};
            then_body.push(s_if(e_binop(e_binop(e_this_prop("__target"), BinOp::BitAnd, e_int(flags)), BinOp::StrictEq, e_int(0)),
                vec![s_throw(e_new_fq("Error", vec![e_str(&format!("Attribute \"{}\" cannot target this declaration", factory.class_name))]))], vec![], None));
            if flags & elephc_builtin_contract::CORE_ATTRIBUTE_IS_REPEATABLE == 0 {
                then_body.push(s_if(e_this_prop("__is_repeated"),
                    vec![s_throw(e_new_fq("Error", vec![e_str(&format!("Attribute \"{}\" must not be repeated", factory.class_name))]))], vec![], None));
            }
        }
        then_body.push(Stmt::new(
            StmtKind::Return(Some(Expr::new(
                ExprKind::NewObject {
                    class_name: name_from_canonical(&factory.class_name),
                    args: factory
                        .args
                        .iter()
                        .map(|entry| match &entry.key {
                            // Named attribute arguments must construct with
                            // named-argument semantics, not positionally.
                            Some(crate::types::AttrKey::Str(name)) => Expr::new(
                                ExprKind::NamedArg {
                                    name: name.clone(),
                                    value: Box::new(attr_arg_expr(&entry.value)),
                                },
                                span,
                            ),
                            _ => attr_arg_expr(&entry.value),
                        })
                        .collect(),
                },
                span,
            ))),
            span,
        ));
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
    // Runtime eval materializes `ReflectionAttribute` objects with factory id
    // zero and stores their retained arguments in `__args`, so the fallback
    // must preserve that path instead of returning an empty factory miss.
    body.push(Stmt::new(
        StmtKind::Return(Some(Expr::new(
            ExprKind::PropertyAccess {
                object: Box::new(Expr::new(ExprKind::This, span)),
                property: "__args".to_string(),
            },
            span,
        ))),
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
