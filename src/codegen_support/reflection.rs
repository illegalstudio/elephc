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
    /// Declaration site `(source id, line)` of a factory whose arguments read a constant
    /// (see [`attr_arg_deprecation_site`]).
    pub(crate) deprecation_site: Option<(u32, u32)>,
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
    let mut unique: BTreeMap<AttributeFactoryKey, bool> = BTreeMap::new();
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
            |(idx, ((class_name, args, deprecation_site), resolvable))| ReflectionAttributeFactory {
                id: (idx as i64) + 1,
                class_name,
                args,
                resolvable,
                deprecation_site,
            },
        )
        .collect()
}

/// Identity of one reflection attribute factory.
///
/// Two occurrences of the same attribute share a factory unless an argument reads a
/// constant: a deprecated constant reports that read at its declaration site, so those
/// occurrences must stay distinct.
type AttributeFactoryKey = (String, Vec<AttrArgEntry>, Option<(u32, u32)>);

/// Returns the declaration site of an argument that reads a constant, if the arguments contain
/// one. Any constant read may be deprecated, whichever the active profile.
fn attr_arg_deprecation_site(entries: &[AttrArgEntry]) -> Option<(u32, u32)> {
    /// Reports whether one captured value evaluates a constant read.
    fn reads_deprecated_constant(value: &AttrArgValue) -> bool {
        match value {
            AttrArgValue::ConstRef(..) | AttrArgValue::ScopedConst(..) => true,
            AttrArgValue::Array(entries) => entries
                .iter()
                .any(|entry| reads_deprecated_constant(&entry.value)),
            _ => false,
        }
    }
    entries
        .iter()
        .find(|entry| reads_deprecated_constant(&entry.value))
        .map(|entry| (entry.span.source_id(), entry.span.line))
}

/// Builds the deduplication key of one attribute occurrence.
fn attribute_factory_key(name: String, args: &[AttrArgEntry]) -> AttributeFactoryKey {
    (name, args.to_vec(), attr_arg_deprecation_site(args))
}

/// The attribute factory ids of one program, keyed by resolved attribute name and captured
/// arguments.
///
/// Numbering the factories walks every class and every lowered function, so a caller that
/// materializes several attributes builds this table once and looks each attribute up in it,
/// instead of renumbering the whole program per attribute.
pub(crate) struct AttributeFactoryIds<'a> {
    classes: &'a HashMap<String, ClassInfo>,
    ids: HashMap<AttributeFactoryKey, i64>,
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
            .map(|factory| {
                (
                    attribute_factory_key(factory.class_name, &factory.args),
                    factory.id,
                )
            })
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
            .get(&attribute_factory_key(lookup_name, attr_args))
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
    let function_sites = function_attribute_sites(extra_attrs);
    let mut body = Vec::new();
    for factory in factories {
        let reads = AttrReads::new(AttrReadSite::Attribute, &factory, &function_sites);
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
            for (target, _) in elephc_builtin_contract::CORE_ATTRIBUTE_TARGET_NAMES.iter().filter(|(target, _)| flags & target == 0) {
                let message = elephc_builtin_contract::core_attribute_target_error(&factory.class_name, *target, flags);
                then_body.push(s_if(e_binop(e_this_prop("__target"), BinOp::StrictEq, e_int(*target)),
                    vec![s_throw(e_new_fq("Error", vec![e_str(&message)]))], vec![], None));
            }
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
                                    value: Box::new(attr_arg_expr(entry, reads, true)),
                                },
                                span,
                            ),
                            _ => attr_arg_expr(entry, reads, true),
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

/// The site whose file and line an attribute argument's constant read reports.
#[derive(Clone, Copy, PartialEq, Eq)]
enum AttrReadSite {
    /// `getArguments()`: the caller of the reflection method.
    Caller,
    /// `newInstance()`: the attribute's declaration (php-src's `EG(lineno_override)`).
    Attribute,
}

/// How one factory's constant reads report, per php-src: a global constant read in a
/// function's (or a function parameter's) attribute always reports the [`AttrReadSite`]; in a
/// class or class member's attribute, a top-level global constant reports the site while a
/// nested one reports its own line. A class constant read reports its own line (php-src
/// reports the site for one in a function's attribute).
#[derive(Clone, Copy)]
struct AttrReads {
    site: AttrReadSite,
    function_scope: bool,
}

impl AttrReads {
    /// Reads of `factory`, which belongs to a function when its declaration site is one of
    /// `function_sites`.
    fn new(site: AttrReadSite, factory: &ReflectionAttributeFactory, function_sites: &std::collections::HashSet<(u32, u32)>) -> Self {
        let function_scope = factory.deprecation_site.is_some_and(|declared| function_sites.contains(&declared));
        Self { site, function_scope }
    }

    /// Span of a constant read written at `line` in `entry`, `top` when it is a top-level
    /// argument and `global` for a global constant. A dummy span asks for the caller's file and
    /// line.
    fn read_span(self, entry: &AttrArgEntry, line: u32, top: bool, global: bool) -> crate::span::Span {
        // A class constant read keeps its own line even in a function's attribute, where php-src
        // reports the override: lowering resolves a class constant only from a source span.
        let overridden = global && (self.function_scope || top);
        if overridden || line == 0 {
            return match self.site {
                AttrReadSite::Attribute if entry.span.line != 0 => entry.span,
                _ => crate::span::Span::dummy(),
            };
        }
        crate::span::Span::new_in_source(line, 1, entry.span.source_id())
    }
}

/// Declaration sites of the function attributes in `extra_attrs` that read a constant.
fn function_attribute_sites(extra_attrs: &[AttributeMetadataSource<'_>]) -> std::collections::HashSet<(u32, u32)> {
    extra_attrs
        .iter()
        .flat_map(|(_, args)| args.iter().flatten())
        .filter_map(|args| attr_arg_deprecation_site(args))
        .collect()
}

/// Converts one captured attribute argument into a synthetic AST expression.
/// Nested arrays (positional or associative) become the corresponding
/// array-literal expression, and symbolic references (global constant, class
/// constant, enum case) become the corresponding reference expression, all so
/// they lower through the normal, architecture-independent paths. Reference
/// names were canonicalised by name resolution at capture time, so the
/// re-emitted nodes resolve directly during lowering (the global/class constant
/// folds to its value; an enum case materializes the case object, matching
/// PHP's `ReflectionAttribute::getArguments()`). A constant read carries the span
/// it reports at (see [`AttrReads`]); `top` marks a top-level argument.
fn attr_arg_expr(entry: &AttrArgEntry, reads: AttrReads, top: bool) -> Expr {
    let span = crate::span::Span::dummy();
    match &entry.value {
        AttrArgValue::Null => Expr::new(ExprKind::Null, span),
        AttrArgValue::Int(value) => Expr::new(ExprKind::IntLiteral(*value), span),
        AttrArgValue::Float(bits) => Expr::new(ExprKind::FloatLiteral(f64::from_bits(*bits)), span),
        AttrArgValue::Bool(value) => Expr::new(ExprKind::BoolLiteral(*value), span),
        AttrArgValue::Str(value) => Expr::new(ExprKind::StringLiteral(value.clone()), span),
        AttrArgValue::Array(entries) => entries_to_array_expr(entries, false, reads, false),
        AttrArgValue::ConstRef(name, line) => Expr::new(
            ExprKind::ConstRef(name_from_canonical(name)),
            reads.read_span(entry, *line, top, true),
        ),
        AttrArgValue::ScopedConst(type_name, member, line) => Expr::new(
            ExprKind::ScopedConstantAccess {
                receiver: StaticReceiver::Named(name_from_canonical(type_name)),
                name: member.clone(),
            },
            reads.read_span(entry, *line, top, false),
        ),
    }
}

/// Builds an array-literal AST expression from captured attribute-arg entries.
/// When `force_assoc` is set (or any entry carries a key, a named argument or
/// explicit array key) it produces an associative `ArrayLiteralAssoc` with
/// positional entries taking their sequential integer key, matching PHP's
/// `getArguments()` ordering; otherwise it produces a positional `ArrayLiteral`.
/// `force_assoc` keeps the top-level `getArguments()` result a single array kind
/// (a hash) so its declared associative type matches the runtime value. `top` marks the
/// top-level argument list.
fn entries_to_array_expr(entries: &[AttrArgEntry], force_assoc: bool, reads: AttrReads, top: bool) -> Expr {
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
                (key, attr_arg_expr(entry, reads, top))
            })
            .collect();
        Expr::new(ExprKind::ArrayLiteralAssoc(pairs), span)
    } else {
        Expr::new(
            ExprKind::ArrayLiteral(entries.iter().map(|entry| attr_arg_expr(entry, reads, top)).collect()),
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
    let function_sites = function_attribute_sites(extra_attrs);
    let mut body = Vec::new();
    for factory in factories {
        let condition = factory_condition(factory.id);
        let reads = AttrReads::new(AttrReadSite::Caller, &factory, &function_sites);
        let then_body = vec![Stmt::new(
            StmtKind::Return(Some(entries_to_array_expr(&factory.args, true, reads, true))),
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
    unique: &mut BTreeMap<AttributeFactoryKey, bool>,
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
            .entry(attribute_factory_key(name, attr_args))
            .or_insert(resolvable);
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for reflection attribute factory identity.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Identical attributes must share a factory so `newInstance()` dispatch does not grow
    //!   per occurrence; a deprecated constant read needs one factory per declaration site.

    use super::*;
    use crate::span::Span;

    /// Builds one captured attribute argument at `line`.
    fn entry(value: AttrArgValue, line: u32) -> AttrArgEntry {
        AttrArgEntry {
            key: None,
            value,
            span: Span::new_in_source(line, 1, 0),
        }
    }

    /// Returns the deduplication key of one occurrence.
    fn key(name: &str, args: &[AttrArgEntry]) -> AttributeFactoryKey {
        attribute_factory_key(name.to_string(), args)
    }

    /// Identical non-deprecated attributes at different lines share one factory.
    #[test]
    fn attribute_span_is_not_part_of_factory_identity() {
        let first = entry(AttrArgValue::Str("x".to_string()), 3);
        let second = entry(AttrArgValue::Str("x".to_string()), 9);
        assert_eq!(first, second);
        assert_eq!(first.cmp(&second), std::cmp::Ordering::Equal);
        assert_eq!(key("A", &[first]), key("A", &[second]));
    }

    /// A deprecated constant read splits factories per declaration site.
    #[test]
    fn deprecated_attribute_reads_keep_distinct_factories() {
        let first = entry(AttrArgValue::ConstRef("MT_RAND_PHP".to_string(), 1), 3);
        let second = entry(AttrArgValue::ConstRef("MT_RAND_PHP".to_string(), 1), 9);
        assert_eq!(first, second);
        assert_ne!(key("A", &[first]), key("A", &[second]));
    }

    /// A nested array argument still identifies its deprecated constant read.
    #[test]
    fn nested_deprecated_attribute_read_is_detected() {
        let nested = entry(
            AttrArgValue::Array(vec![entry(
                AttrArgValue::ConstRef("MT_RAND_PHP".to_string(), 4),
                4,
            )]),
            4,
        );
        assert_eq!(
            attr_arg_deprecation_site(&[nested]),
            Some((0, 4)),
            "the nested MT_RAND_PHP read must split the factory"
        );
    }
}
