//! Purpose:
//! Validates schema validation declarations for the checker.
//! Turns parsed declarations into canonical metadata and rejects invalid contracts before code generation.
//!
//! Called from:
//! - `crate::types::checker::schema`
//!
//! Key details:
//! - Declaration metadata must align with name resolution, inheritance flattening, and runtime/codegen expectations.

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{
    Attribute, ClassMethod, Expr, ExprKind, StaticReceiver, StmtKind, TypeExpr, Visibility,
};
use crate::types::{FunctionSig, PhpType};

use super::super::Checker;

/// Resolves `self::class` (and, when the parent is known, `parent::class`) inside a parameter
/// default to the declaring class name.
///
/// Omitted arguments are materialized at the CALL site, which lowers the default expression in
/// the caller's scope: there `self` names the caller's class, or nothing at all, and the
/// default came back as the literal string `"self"`. php-src evaluates the default in the
/// declaring class, so the name is fixed here, where the declaring class is known. Array
/// defaults (`[self::class]`) are resolved element by element.
fn resolve_self_class_default(
    default: Expr,
    declaring_type: &str,
    parent_type: Option<&str>,
) -> Expr {
    use crate::parser::ast::{ExprKind, StaticReceiver};
    let span = default.span;
    let kind = match default.kind {
        ExprKind::ClassConstant {
            receiver: StaticReceiver::Self_,
        } => ExprKind::StringLiteral(declaring_type.trim_start_matches('\\').to_string()),
        ExprKind::ClassConstant {
            receiver: StaticReceiver::Parent,
        } if parent_type.is_some() => ExprKind::StringLiteral(
            parent_type.unwrap_or_default().trim_start_matches('\\').to_string(),
        ),
        ExprKind::ArrayLiteral(items) => ExprKind::ArrayLiteral(
            items
                .into_iter()
                .map(|item| resolve_self_class_default(item, declaring_type, parent_type))
                .collect(),
        ),
        ExprKind::ArrayLiteralAssoc(items) => ExprKind::ArrayLiteralAssoc(
            items
                .into_iter()
                .map(|(key, value)| {
                    (
                        resolve_self_class_default(key, declaring_type, parent_type),
                        resolve_self_class_default(value, declaring_type, parent_type),
                    )
                })
                .collect(),
        ),
        other => other,
    };
    Expr { kind, span }
}

/// Builds a `FunctionSig` from a parsed class method, resolving parameter and return type
/// annotations through the checker. Untyped by-reference parameters use canonical `Mixed`
/// storage; other parameters without hints use a declared default's syntactic type when present,
/// matching free-function signature construction, and otherwise default to `PhpType::Int`.
/// Validates that each declared parameter's default value is compatible with its resolved type.
/// Infers return type from method body when no return annotation is present.
pub(crate) fn build_method_sig(
    checker: &Checker,
    method: &ClassMethod,
    declaring_type: &str,
) -> Result<FunctionSig, CompileError> {
    build_method_sig_in_class(checker, method, declaring_type, None)
}

/// [`build_method_sig`] for a method of a class whose parent is known, so a `parent::class`
/// parameter default resolves to it.
pub(crate) fn build_method_sig_in_class(
    checker: &Checker,
    method: &ClassMethod,
    declaring_type: &str,
    parent_type: Option<&str>,
) -> Result<FunctionSig, CompileError> {
    let method_key = php_symbol_key(&method.name);
    let params: Vec<(String, PhpType)> = method
        .params
        .iter()
        .enumerate()
        .map(|(i, (n, type_ann, default, is_ref))| {
            // User hydration hooks receive a PHP array with arbitrary integer/string
            // keys. Use its boxed declaration contract, including for untyped hooks.
            // Synthetic SPL hooks retain their explicit raw Array(Mixed) ABI.
            if method_key == "__unserialize" && i == 0 && method.span.line != 0 {
                return Ok((n.clone(), PhpType::php_array()));
            }
            let ty = match type_ann {
                Some(type_ann) => checker.resolve_declared_param_type_hint(
                    type_ann,
                    method.span,
                    &format!("Method parameter ${}", n),
                )?,
                None if *is_ref => PhpType::Mixed,
                None => default
                    .as_ref()
                    .map(super::super::infer_expr_type_syntactic)
                    .unwrap_or(PhpType::Int),
            };
            Ok((n.clone(), ty))
        })
        .collect::<Result<Vec<_>, CompileError>>()?;
    let defaults: Vec<Option<Expr>> = method
        .params
        .iter()
        .map(|(_, _, d, _)| {
            d.clone()
                .map(|default| resolve_self_class_default(default, declaring_type, parent_type))
        })
        .collect();
    let mut ref_params: Vec<bool> = method.params.iter().map(|(_, _, _, r)| *r).collect();
    for ((param_name, type_ann, default, _), (_, resolved_ty)) in
        method.params.iter().zip(params.iter())
    {
        if type_ann.is_some() {
            checker.validate_schema_parameter_default_type(
                resolved_ty,
                default.as_ref(),
                method.span,
                &format!("Method parameter ${}", param_name),
            )?;
        }
    }
    let return_type = match method.return_type.as_ref() {
        Some(type_ann) => checker.resolve_method_return_type_hint(
            type_ann,
            declaring_type,
            method.span,
            &format!("Method '{}'", method.name),
        )?,
        // The syntactic pass types `return $this;` and `return new static;` as an object of no
        // class: it sees the body, not its owner. That provisional return is what every call
        // site checked before the method pass reads, so a fluent chain `$m->a()->b($v)` found no
        // `b` on a nameless class, typed the call as the `Int` placeholder and taught `b`'s
        // untyped parameters nothing. The owner is known here.
        None => owner_relative_object_return(
            super::super::infer_return_type_syntactic(&method.body),
            declaring_type,
        ),
    };
    if method.variadic.is_some() {
        ref_params.push(method.variadic_by_ref);
    }
    let mut sig = Checker::callable_wrapper_sig(&FunctionSig {
        params,
        param_type_exprs: method
            .params
            .iter()
            .map(|(_, type_ann, _, _)| type_ann.clone())
            .chain(method.variadic.iter().map(|_| method.variadic_type.clone()))
            .collect(),
        param_attributes: method.param_attributes.clone(),
        defaults,
        return_type,
        declared_return: method.return_type.is_some(),
        by_ref_return: method.by_ref_return,
        ref_params,
        declared_params: method
            .params
            .iter()
            .map(|(_, type_ann, _, _)| type_ann.is_some())
            .chain(
                method
                    .variadic
                    .iter()
                    .map(|_| method.variadic_type.is_some()),
            )
            .collect(),
        variadic: method.variadic.clone(),
        deprecation: extract_deprecation(&method.attributes),
        // Generator-ness is syntactic and must survive DCE removing an unreachable yield.
        is_generator: crate::types::checker::yield_validation::body_contains_yield(&method.body),
    });
    // A declared element type on the variadic (`int ...$xs`) constrains every collected argument.
    // `callable_wrapper_sig` defaults the variadic container to `array<mixed>`; refine it to the
    // declared element type so call validation enforces it.
    if !method.variadic_by_ref {
        if let Some(variadic_type) = &method.variadic_type {
            let elem_ty = checker.resolve_declared_param_type_hint(
                variadic_type,
                method.span,
                &format!(
                    "Method variadic parameter ${}",
                    method.variadic.as_deref().unwrap_or_default()
                ),
            )?;
            if let Some((_, ty)) = sig.params.last_mut() {
                *ty = PhpType::Array(Box::new(elem_ty));
            }
        }
    }
    Ok(sig)
}

/// Names the owner in a syntactic return type that holds an object of no class.
fn owner_relative_object_return(return_type: PhpType, owner: &str) -> PhpType {
    match return_type {
        PhpType::Object(name) if name.is_empty() && !owner.is_empty() => {
            PhpType::Object(owner.to_string())
        }
        PhpType::Union(members) => PhpType::Union(
            members
                .into_iter()
                .map(|member| owner_relative_object_return(member, owner))
                .collect(),
        ),
        other => other,
    }
}

/// Returns `Some(reason)` when the attribute list contains a `#[\Deprecated]`
/// marker, with `reason` set to the attribute's first string argument (or an
/// empty string if absent). Match is case-insensitive on the last segment of
/// the attribute name.
pub(crate) fn extract_deprecation(groups: &[crate::parser::ast::AttributeGroup]) -> Option<String> {
    for group in groups {
        for attr in &group.attributes {
            if !matches_global_builtin_attribute(attr, "Deprecated") {
                continue;
            }
            let reason = attr.args.iter().find_map(|expr| match &expr.kind {
                ExprKind::StringLiteral(s) => Some(s.clone()),
                _ => None,
            });
            return Some(reason.unwrap_or_default());
        }
    }
    None
}

/// Gives an untyped `__toString()` PHP 8's implicit `: string` where a contract requires it.
///
/// PHP adds that declaration itself, so an untyped `__toString()` satisfies `Stringable` and a
/// parent's `__toString(): string`. elephc applies it only to classes whose ancestry implements
/// `Stringable` or inherits a declared `__toString()` return: those were rejected before
/// ("without declaring a compatible return type"), while an unconstrained untyped `__toString()`
/// keeps its inferred return and so cannot start failing body checks it passed before.
/// Rewrites both the schema map and the flattened declarations, so signatures, body checking and
/// lowering all see the same declared return.
pub(crate) fn apply_implicit_tostring_return_types(
    class_map: &mut std::collections::HashMap<String, crate::types::traits::FlattenedClass>,
    flattened: &mut [crate::types::traits::FlattenedClass],
    interface_map: &std::collections::HashMap<
        String,
        crate::types::checker::builtin_types::InterfaceDeclInfo,
    >,
) {
    fn untyped_tostring(method: &ClassMethod) -> bool {
        method.return_type.is_none()
            && !method.by_ref_return
            && !method.is_static
            && php_symbol_key(&method.name) == "__tostring"
    }
    fn reaches_stringable(
        interface: &str,
        interface_map: &std::collections::HashMap<
            String,
            crate::types::checker::builtin_types::InterfaceDeclInfo,
        >,
        seen: &mut std::collections::HashSet<String>,
    ) -> bool {
        let key = php_symbol_key(interface.trim_start_matches('\\'));
        if key == "stringable" {
            return true;
        }
        if !seen.insert(key) {
            return false;
        }
        interface_map
            .iter()
            .find(|(name, _)| php_symbol_key(name) == php_symbol_key(interface))
            .is_some_and(|(_, info)| {
                info.extends
                    .iter()
                    .any(|parent| reaches_stringable(parent, interface_map, seen))
            })
    }
    // `(string)`, `echo` and interpolation call `__toString()` statically on the declared class
    // (no virtual dispatch yet). A class whose `__toString()` a
    // subclass overrides would convert with the wrong body, so it keeps its untyped declaration
    // and the contract error it had before instead of compiling to the wrong output.
    let overridden_by_subclass = |class_name: &str| {
        class_map.values().any(|other| {
            if other.name == class_name
                || !other
                    .methods
                    .iter()
                    .any(|method| php_symbol_key(&method.name) == "__tostring")
            {
                return false;
            }
            let mut seen = std::collections::HashSet::new();
            let mut parent = other.extends.as_deref();
            while let Some(name) = parent {
                if name == class_name {
                    return true;
                }
                if !seen.insert(name.to_string()) {
                    break;
                }
                parent = class_map.get(name).and_then(|class| class.extends.as_deref());
            }
            false
        })
    };
    let needs_implicit: Vec<String> = class_map
        .values()
        .filter(|class| class.methods.iter().any(untyped_tostring))
        .filter(|class| !overridden_by_subclass(&class.name))
        .filter(|class| {
            let mut seen_classes = std::collections::HashSet::new();
            let mut seen_interfaces = std::collections::HashSet::new();
            let mut current = Some(*class);
            while let Some(ancestor) = current {
                if !seen_classes.insert(ancestor.name.clone()) {
                    break;
                }
                if ancestor
                    .implements
                    .iter()
                    .any(|iface| reaches_stringable(iface, interface_map, &mut seen_interfaces))
                {
                    return true;
                }
                if ancestor.name != class.name
                    && ancestor.methods.iter().any(|method| {
                        php_symbol_key(&method.name) == "__tostring"
                            && method.return_type.is_some()
                    })
                {
                    return true;
                }
                current = ancestor
                    .extends
                    .as_deref()
                    .and_then(|parent| class_map.get(parent));
            }
            false
        })
        .map(|class| class.name.clone())
        .collect();
    let declare = |methods: &mut [ClassMethod]| {
        for method in methods.iter_mut().filter(|method| untyped_tostring(method)) {
            method.return_type = Some(TypeExpr::Str);
        }
    };
    for name in &needs_implicit {
        if let Some(class) = class_map.get_mut(name) {
            declare(&mut class.methods);
        }
        if let Some(class) = flattened.iter_mut().find(|class| &class.name == name) {
            declare(&mut class.methods);
        }
    }
}

/// Returns whether a method carries PHP's `#[\ReturnTypeWillChange]` marker.
pub(crate) fn has_return_type_will_change(groups: &[crate::parser::ast::AttributeGroup]) -> bool {
    groups.iter().any(|group| {
        group
            .attributes
            .iter()
            .any(|attr| matches_global_builtin_attribute(attr, "ReturnTypeWillChange"))
    })
}

/// Returns whether collected attribute names (see `collect_attribute_names`) carry
/// `#[\ReturnTypeWillChange]`; used where only the class state's method metadata survives.
pub(crate) fn attribute_names_include_return_type_will_change(names: Option<&Vec<String>>) -> bool {
    names.is_some_and(|names| {
        names.iter().any(|name| {
            name.trim_start_matches('\\')
                .eq_ignore_ascii_case("ReturnTypeWillChange")
        })
    })
}

/// Returns `true` if `attr` is a global builtin attribute matching `builtin` by name.
/// Fully-qualified names must match exactly (case-insensitive); unqualified names
/// match the last segment case-insensitively. Used to detect `#[\Deprecated]` and similar.
pub(crate) fn matches_global_builtin_attribute(attr: &Attribute, builtin: &str) -> bool {
    let name = attr.name.as_canonical();
    if attr.name.is_fully_qualified() {
        return name.eq_ignore_ascii_case(builtin);
    }
    attr.name.is_unqualified() && name.eq_ignore_ascii_case(builtin)
}

/// Builds a mapping from constructor parameter index to property name for each parameter.
/// For each parameter, searches constructor body for `PropertyAssign` statements where
/// the right-hand side is a Variable with the same name as the parameter; if found,
/// returns `Some(property_name)`, otherwise `None`. Returns empty vec if no constructor.
pub(crate) fn build_constructor_param_map(methods: &[ClassMethod]) -> Vec<Option<String>> {
    let mut param_to_prop = Vec::new();
    if let Some(constructor) = methods
        .iter()
        .find(|m| php_symbol_key(&m.name) == "__construct")
    {
        param_to_prop = constructor
            .params
            .iter()
            .map(|(pname, _, _, _)| {
                for stmt in &constructor.body {
                    if let StmtKind::PropertyAssign {
                        property, value, ..
                    } = &stmt.kind
                    {
                        if let ExprKind::Variable(vn) = &value.kind {
                            if vn == pname {
                                return Some(property.clone());
                            }
                        }
                    }
                }
                None
            })
            .collect();
    }
    param_to_prop
}

/// Returns a numeric rank for visibility levels: `private=0`, `protected=1`, `public=2`.
/// Used to enforce that overriding methods are not less visible than the parent method.
pub(crate) fn visibility_rank(visibility: &Visibility) -> u8 {
    match visibility {
        Visibility::Private => 0,
        Visibility::Protected => 1,
        Visibility::Public => 2,
    }
}

/// The parameter shape a signature's SOURCE declared, with generated slots removed.
///
/// `crate::func_args` appends hidden slots (the surplus-argument collector, and the actual-count
/// parameter that accompanies a source variadic) to every frame it captures, and it captures all
/// of them as soon as the program contains an `eval()` or a backtrace call. Those slots are not
/// declared parameters, so comparing them against a compiler-injected parent signature, which
/// can never carry one, would report a difference the source never wrote.
struct SourceVisibleShape {
    param_count: usize,
    param_names: Vec<String>,
    param_types: Vec<PhpType>,
    declared_params: Vec<bool>,
    ref_params: Vec<bool>,
    has_defaults: Vec<bool>,
    defaults: Vec<Option<Expr>>,
    variadic: Option<String>,
}

impl SourceVisibleShape {
    /// Projects `sig` onto the parameters its source declared.
    fn of(sig: &FunctionSig) -> Self {
        let generated: Vec<bool> = sig
            .params
            .iter()
            .map(|(name, _)| {
                name == crate::func_args::HIDDEN_ARGS_PARAM
                    || name == crate::func_args::HIDDEN_ARGC_PARAM
            })
            .collect();
        let keep = |index: usize| !generated.get(index).copied().unwrap_or(false);
        Self {
            param_count: generated.iter().filter(|hidden| !**hidden).count(),
            param_names: sig
                .params
                .iter()
                .enumerate()
                .filter(|(index, _)| keep(*index))
                .map(|(_, (name, _))| name.clone())
                .collect(),
            param_types: sig
                .params
                .iter()
                .enumerate()
                .filter(|(index, _)| keep(*index))
                .map(|(_, (_, ty))| ty.clone())
                .collect(),
            declared_params: sig
                .declared_params
                .iter()
                .enumerate()
                .filter(|(index, _)| keep(*index))
                .map(|(_, declared)| *declared)
                .collect(),
            ref_params: sig
                .ref_params
                .iter()
                .enumerate()
                .filter(|(index, _)| keep(*index))
                .map(|(_, by_ref)| *by_ref)
                .collect(),
            has_defaults: sig
                .defaults
                .iter()
                .enumerate()
                .filter(|(index, _)| keep(*index))
                .map(|(_, default)| default.is_some())
                .collect(),
            defaults: sig
                .defaults
                .iter()
                .enumerate()
                .filter(|(index, _)| keep(*index))
                .map(|(_, default)| default.clone())
                .collect(),
            variadic: sig
                .variadic
                .clone()
                .filter(|variadic| variadic != crate::func_args::HIDDEN_ARGS_PARAM),
        }
    }

    /// Counts the declared parameters a caller must supply.
    ///
    /// A variadic parameter is never required, even without a default, which is why the shape
    /// keeps the variadic name rather than folding it into the default flags.
    fn required_param_count(&self) -> usize {
        self.has_defaults
            .iter()
            .enumerate()
            .filter(|(index, has_default)| {
                if self.variadic.is_some() && *index + 1 == self.has_defaults.len() {
                    return false;
                }
                !**has_default
            })
            .count()
    }
}

/// Validates that `child_sig` is compatible with `parent_sig` for override purposes.
/// Checks parameter shape and rejects a declared type where the source parent accepts any value.
/// Reports errors with `context` and `kind` (e.g., "overriding method") in the message.
///
/// `compare_generated_abi` is true only when both signatures originate in PHP source. Compiler-
/// injected contracts are synthesized after the `func_args` pass and can never carry its hidden
/// collector or actual-count parameter, so comparing those slots against such a contract would
/// report an ABI difference the source never declared.
#[allow(clippy::too_many_arguments)]
pub(crate) fn validate_signature_compatibility(
    span: crate::span::Span,
    owner_name: &str,
    method_name: &str,
    child_sig: &FunctionSig,
    parent_sig: &FunctionSig,
    kind: &str,
    context: &str,
    compare_generated_abi: bool,
) -> Result<(), CompileError> {
    validate_signature_compatibility_with_defaults(
        span,
        owner_name,
        method_name,
        child_sig,
        parent_sig,
        kind,
        context,
        compare_generated_abi,
        &|_, _| false,
    )
}

/// [`validate_signature_compatibility`] with a resolver that decides whether a parent and child
/// default spelled with `self::`/`parent::` name the same constant value in their own scopes.
/// Without a declaring-class context (interface-to-interface checks) such defaults count as
/// changed.
#[allow(clippy::too_many_arguments)]
pub(crate) fn validate_signature_compatibility_with_defaults(
    span: crate::span::Span,
    owner_name: &str,
    method_name: &str,
    child_sig: &FunctionSig,
    parent_sig: &FunctionSig,
    kind: &str,
    context: &str,
    compare_generated_abi: bool,
    same_relative_default: &dyn Fn(&Expr, &Expr) -> bool,
) -> Result<(), CompileError> {
    // The hidden variadic that collects surplus positional arguments for
    // `func_num_args()`/`func_get_args()`/`func_get_arg()` is a real ABI parameter, so an
    // inherited signature that does not carry it cannot dispatch to a body that does.
    // Report that directly instead of the generic parameter-count mismatch, which names a
    // parameter the source never wrote.
    if compare_generated_abi
        && (crate::func_args::sig_collects_surplus_args(child_sig)
            != crate::func_args::sig_collects_surplus_args(parent_sig)
            || crate::func_args::sig_has_hidden_argc_param(child_sig)
                != crate::func_args::sig_has_hidden_argc_param(parent_sig))
    {
        return Err(CompileError::new(
            span,
            &format!(
                "func_num_args()/func_get_args()/func_get_arg() are not supported in {}::{} when {} {}: the inherited signature cannot be widened to collect surplus arguments",
                owner_name, method_name, context, kind
            ),
        ));
    }

    let child = SourceVisibleShape::of(child_sig);
    let parent = SourceVisibleShape::of(parent_sig);

    // PHP accepts trailing optional parameters added by an override, but elephc's method
    // dispatch is fixed-arity: a call through the parent's type passes the parent's parameter
    // count into the child's vtable slot, so nothing would supply the added defaults. Refuse it
    // with its own diagnostic rather than reading an argument that was never passed.
    if child.param_count > parent.param_count
        && child.variadic.is_none()
        && parent.variadic.is_none()
        && child.has_defaults[parent.param_count..]
            .iter()
            .all(|has_default| *has_default)
    {
        return Err(CompileError::new(
            span,
            &format!(
                "Cannot add optional parameters when {} {}: {}::{} (not yet supported: elephc dispatches overrides with the parent's parameter count and cannot supply the added defaults)",
                context, kind, owner_name, method_name
            ),
        ));
    }

    if child.param_count != parent.param_count {
        return Err(CompileError::new(
            span,
            &format!(
                "Cannot change parameter count when {} {}: {}::{}",
                context, kind, owner_name, method_name
            ),
        ));
    }

    if child.ref_params != parent.ref_params {
        return Err(CompileError::new(
            span,
            &format!(
                "Cannot change pass-by-reference parameters when {} {}: {}::{}",
                context, kind, owner_name, method_name
            ),
        ));
    }

    if compare_generated_abi {
        for (index, (&parent_declared, &child_declared)) in parent
            .declared_params
            .iter()
            .zip(&child.declared_params)
            .enumerate()
        {
            if !parent_declared && child_declared && child.param_types[index] != PhpType::Mixed {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "Cannot narrow untyped parameter ${} when {} {}: {}::{}",
                        child.param_names[index], context, kind, owner_name, method_name
                    ),
                ));
            }
        }
    }

    // PHP lets an override make a required parameter optional, but elephc cannot dispatch it yet:
    // a boxed `mixed` receiver that omits the argument only finds candidates whose
    // whole family agrees on the default, so the call resolves to no method and raises a
    // catchable `Error` instead of running the child. Refuse it rather than compile that.
    if let Some(index) = parent
        .has_defaults
        .iter()
        .zip(&child.has_defaults)
        .position(|(&parent_optional, &child_optional)| !parent_optional && child_optional)
    {
        return Err(CompileError::new(
            span,
            &format!(
                "Cannot make parameter ${} optional when {} {}: {}::{} (not yet supported: dynamic dispatch cannot supply the override's default)",
                child.param_names[index], context, kind, owner_name, method_name
            ),
        ));
    }
    if parent
        .has_defaults
        .iter()
        .zip(&child.has_defaults)
        .any(|(&parent_optional, &child_optional)| parent_optional && !child_optional)
    {
        return Err(CompileError::new(
            span,
            &format!(
                "Cannot change optional parameter layout when {} {}: {}::{}",
                context, kind, owner_name, method_name
            ),
        ));
    }

    // A default is filled at the call site from the caller's static signature, so a call through
    // the parent's (or interface's) type passes the parent's default even when the runtime class
    // declares another one. Until defaults are supplied by the callee, an override
    // must keep the inherited default value wherever the parent has one.
    if let Some(index) = parent
        .defaults
        .iter()
        .zip(&child.defaults)
        .position(|pair| matches!(pair, (Some(parent_default), Some(child_default)) if !inherited_default_unchanged(parent_default, child_default, same_relative_default)))
    {
        // `self::class` is resolved to the declaring class (see `resolve_self_class_default`),
        // so the same spelling on both sides is two different values; say so.
        let both_class_names = matches!(
            &child.defaults[index],
            Some(Expr { kind: crate::parser::ast::ExprKind::StringLiteral(name), .. })
                if name.eq_ignore_ascii_case(owner_name.trim_start_matches('\\'))
        );
        let note = if both_class_names {
            " A `self::class` default names a different class in each declaration."
        } else {
            ""
        };
        return Err(CompileError::new(
            span,
            &format!(
                "Cannot change the default value of parameter ${} when {} {}: {}::{} (not yet supported: calls through the parent's type would pass the parent's default).{}",
                child.param_names[index], context, kind, owner_name, method_name, note
            ),
        ));
    }

    // Only the presence of a variadic is part of the contract. Its name is not: PHP accepts
    // `...$arrays` overridden as `...$lists`, and positional arguments collect identically.
    if child.variadic.is_some() != parent.variadic.is_some() {
        return Err(CompileError::new(
            span,
            &format!(
                "Cannot change variadic parameter shape when {} {}: {}::{}",
                context, kind, owner_name, method_name
            ),
        ));
    }

    if child.required_param_count() > parent.required_param_count() {
        return Err(CompileError::new(
            span,
            &format!(
                "Cannot change required parameter count when {} {}: {}::{}",
                context, kind, owner_name, method_name
            ),
        ));
    }

    Ok(())
}

/// Returns `true` if `actual` is a compatible declared return type for `expected`.
/// Allows `PhpType::Never` (unreachable) as a wildcard match. Otherwise delegates to
/// `checker.type_accepts(expected, actual)` for standard subtype checking.
pub(crate) fn declared_return_type_compatible(
    checker: &Checker,
    expected: &PhpType,
    actual: &PhpType,
) -> bool {
    matches!(actual, PhpType::Never) || checker.type_accepts(expected, actual)
}

/// Returns true for PDO's internal SQLSTATE-aware widening of `Exception::getCode()`.
pub(crate) fn is_pdo_exception_get_code_contract(
    class_name: &str,
    method_name: &str,
    return_type: &PhpType,
) -> bool {
    let PhpType::Union(types) = return_type else {
        return false;
    };
    class_name.trim_start_matches('\\') == "PDOException"
        && php_symbol_key(method_name) == "getcode"
        && types.len() == 2
        && types.contains(&PhpType::Str)
        && types.contains(&PhpType::Int)
}

/// Checks a preserved late-static parent/interface return against a child declaration.
///
/// A concrete class name cannot replace `static`: that would become unsound for further
/// subclasses. `never` remains a valid covariant narrowing, while compound returns are
/// compared after binding only their `static` members to the current receiver.
pub(crate) fn late_static_return_compatible(
    checker: &Checker,
    expected: Option<&TypeExpr>,
    actual: Option<&TypeExpr>,
    actual_resolved: &PhpType,
    receiver_type: &str,
    span: crate::span::Span,
) -> Result<Option<bool>, CompileError> {
    let Some(expected) = expected else {
        return Ok(None);
    };
    if matches!(actual_resolved, PhpType::Never) {
        return Ok(Some(true));
    }
    let Some(actual) = actual.filter(|return_type| return_type.contains_late_static()) else {
        return Ok(Some(false));
    };
    let expected =
        checker.resolve_late_static_return_type_hint(expected, receiver_type, span)?;
    let actual = checker.resolve_late_static_return_type_hint(actual, receiver_type, span)?;
    Ok(Some(declared_return_type_compatible(
        checker, &expected, &actual,
    )))
}

/// Returns whether a class-like symbol was declared in PHP source.
///
/// Unknown owners stay strict. Only a symbol proven compiler-injected may suppress comparison of
/// the generated `func_args` ABI slots.
pub(super) fn declaration_is_source(checker: &Checker, owner: &str) -> bool {
    checker
        .classes
        .get(owner)
        .map(|info| info.declaration_span != crate::span::Span::dummy())
        .or_else(|| {
            checker
                .interfaces
                .get(owner)
                .map(|info| info.declaration_span != crate::span::Span::dummy())
        })
        .unwrap_or(true)
}

/// Validates that `method` can override `parent_sig` in class `class_name`.
/// Builds the child signature via `build_method_sig`, skips validation for `__construct`,
/// checks signature compatibility, and ensures the child does not remove a declared
/// return type when the parent has one or make it incompatible.
pub(crate) fn validate_override_signature(
    checker: &Checker,
    class: &crate::types::traits::FlattenedClass,
    method: &ClassMethod,
    parent_sig: &FunctionSig,
    parent_late_static_return: Option<&TypeExpr>,
    is_static: bool,
    parent_declaration_is_source: bool,
    parent_owner: Option<&str>,
) -> Result<(), CompileError> {
    let kind = if is_static { "static method" } else { "method" };
    let class_name = class.name.as_str();
    let child_sig =
        build_method_sig_in_class(checker, method, class_name, class.extends.as_deref())?;
    if php_symbol_key(&method.name) == "__construct" {
        return Ok(());
    }
    let same_relative_default = |parent_default: &Expr, child_default: &Expr| {
        parent_owner.is_some_and(|owner| {
            relative_defaults_name_same_value(
                checker,
                parent_default,
                &DefaultScope::Registered(owner),
                child_default,
                &DefaultScope::Flattened(class),
            )
        })
    };
    validate_signature_compatibility_with_defaults(
        method.span,
        class_name,
        &method.name,
        &child_sig,
        parent_sig,
        kind,
        "overriding",
        parent_declaration_is_source && method.span.line != 0,
        &same_relative_default,
    )?;
    // The override family ABI (`crate::types::override_families`, applied by
    // `ir_lower::program::virtual_abi`) boxes by-value positions and returns whose representation
    // differs inside a source family and re-applies PHP's coercion in each body, so those
    // differences need no refusal here; everything it does not unify still does.
    let family_abi = method.span.line != 0
        && override_family_unifies(checker, class, &method.name, &method.visibility, is_static);
    // Compiler-injected overrides are written against their builtin parents' real ABI.
    if method.span.line != 0 {
        validate_override_parameter_abi(
            checker,
            class_name,
            &method.name,
            kind,
            &child_sig,
            parent_sig,
            method.span,
            family_abi,
        )?;
    }
    // `#[\ReturnTypeWillChange]` only relaxes PHP's tentative return types, which exist on
    // internal (compiler-injected) parents. A source parent stays strict, as in PHP.
    let return_type_will_change =
        !parent_declaration_is_source && has_return_type_will_change(&method.attributes);
    if parent_sig.declared_return && !child_sig.declared_return && !return_type_will_change {
        return Err(CompileError::new(
            method.span,
            &format!(
                "Cannot override {} {}::{} without declaring a compatible return type (parent returns {})",
                kind, class_name, method.name, parent_sig.return_type
            ),
        ));
    }
    let late_static_compatible = late_static_return_compatible(
        checker,
        parent_late_static_return,
        method.return_type.as_ref(),
        &child_sig.return_type,
        class_name,
        method.span,
    )?;
    let return_compatible = is_pdo_exception_get_code_contract(
        class_name,
        &method.name,
        &child_sig.return_type,
    ) || late_static_compatible.unwrap_or_else(|| {
        declared_return_type_compatible(
            checker,
            &parent_sig.return_type,
            &child_sig.return_type,
        ) || covariant_self_return_compatible(
            checker,
            class_name,
            class.extends.as_deref(),
            &class.implements,
            parent_sig,
            &child_sig,
        ) || union_self_return_compatible(
            checker,
            class_name,
            class.extends.as_deref(),
            &class.implements,
            parent_sig,
            &child_sig,
        )
    });
    if parent_sig.declared_return && return_type_will_change {
        // A class override shares the parent's vtable slot and its caller-visible return ABI,
        // and no adapter re-boxes the result, so a relaxed return must keep the parent's
        // physical representation, even when the types are otherwise compatible (a `mixed`
        // parent read through a concrete child word crashes). An untyped child is refused too:
        // body inference may still change its return representation after this check.
        if method.return_type.is_some()
            && child_sig
                .return_type
                .reference_payload_compatible(&parent_sig.return_type)
        {
            return Ok(());
        }
        let detail = if method.return_type.is_none() {
            format!(
                "an untyped #[\\ReturnTypeWillChange] override cannot be dispatched through the parent's native return type {}; declare a return type with that representation",
                parent_sig.return_type
            )
        } else {
            format!(
                "#[\\ReturnTypeWillChange] return type {} cannot be dispatched through the parent's native return type {}",
                child_sig.return_type, parent_sig.return_type
            )
        };
        return Err(CompileError::new(
            method.span,
            &format!(
                "Cannot override {} {}::{}: {} (not yet supported)",
                kind, class_name, method.name, detail
            ),
        ));
    }
    // A union return narrowed by the override is only sound while both sides share one physical
    // return representation, unless the override family ABI boxes it (`family_abi`): the
    // parent's caller reads the result with the parent's layout. `int|null` (tagged scalar) or `false`
    // under a boxed `Node|int|null`/`Node|false` would be read as the wrong word.
    if parent_sig.declared_return
        && return_compatible
        && !family_abi
        && (matches!(parent_sig.return_type, PhpType::Union(_))
            || matches!(child_sig.return_type, PhpType::Union(_)))
        && !child_sig
            .return_type
            .reference_payload_compatible(&parent_sig.return_type)
    {
        return Err(CompileError::new(
            method.span,
            &format!(
                "Cannot override {} {}::{} with union return type {}: it changes the physical return representation of the parent's {} (not yet supported)",
                kind, class_name, method.name, child_sig.return_type, parent_sig.return_type
            ),
        ));
    }
    if parent_sig.declared_return && !return_compatible {
        return Err(CompileError::new(
            method.span,
            &format!(
                "Cannot override {} {}::{} with incompatible return type {} (parent returns {})",
                kind, class_name, method.name, child_sig.return_type, parent_sig.return_type
            ),
        ));
    }
    Ok(())
}

/// Returns true when a child method may return the child class itself against a wider parent return.
///
/// Covers PHP covariant returns (`parent::w(): Base` overridden as `w(): static` / `w(): Child`)
/// while the child is mid-construction — `type_accepts` cannot see the subclass edge yet because
/// `checker.classes` lacks the child, but the parent class is already registered.
fn covariant_self_return_compatible(
    checker: &Checker,
    class_name: &str,
    extends: Option<&str>,
    implements: &[String],
    parent_sig: &FunctionSig,
    child_sig: &FunctionSig,
) -> bool {
    match (&parent_sig.return_type, &child_sig.return_type) {
        (PhpType::Object(expected_name), PhpType::Object(actual_name))
            if actual_name == class_name =>
        {
            if expected_name == class_name {
                return true;
            }
            if let Some(parent) = extends {
                if parent == expected_name
                    || checker.is_subclass_of(parent, expected_name)
                    || checker.class_implements_interface(parent, expected_name)
                {
                    return true;
                }
            }
            implements.iter().any(|iface| {
                iface == expected_name
                    || checker.interface_extends_interface(iface, expected_name)
            })
        }
        _ => false,
    }
}

/// Returns true when a union child return is covariant member by member, letting members that
/// name the class under construction match through its declared parent and interfaces.
///
/// Covers PHP returns like `Node|bool` overridden as `Leaf|false`: `type_accepts` cannot see
/// `Leaf` yet because the child is mid-construction. A class override shares the parent's vtable
/// slot and return ABI, so the relaxed acceptance also requires an unchanged physical return
/// representation (a scalar narrowed out of a boxed union would be read as the wrong word).
fn union_self_return_compatible(
    checker: &Checker,
    class_name: &str,
    extends: Option<&str>,
    implements: &[String],
    parent_sig: &FunctionSig,
    child_sig: &FunctionSig,
) -> bool {
    let child_members: Vec<&PhpType> = match &child_sig.return_type {
        PhpType::Union(members) => members.iter().collect(),
        other => vec![other],
    };
    let parent_members: Vec<&PhpType> = match &parent_sig.return_type {
        PhpType::Union(members) => members.iter().collect(),
        other => vec![other],
    };
    if child_members.len() < 2 && parent_members.len() < 2 {
        return false;
    }
    if !child_sig
        .return_type
        .reference_payload_compatible(&parent_sig.return_type)
    {
        return false;
    }
    child_members.into_iter().all(|member| {
        declared_return_type_compatible(checker, &parent_sig.return_type, member)
            || (matches!(member, PhpType::Object(name) if name == class_name)
                && parent_members.iter().any(|expected| {
                    let PhpType::Object(expected_name) = expected else {
                        return false;
                    };
                    expected_name == class_name
                        || extends.is_some_and(|parent| {
                            parent == expected_name
                                || checker.is_subclass_of(parent, expected_name)
                                || checker.class_implements_interface(parent, expected_name)
                        })
                        || implements.iter().any(|iface| {
                            iface == expected_name
                                || checker.interface_extends_interface(iface, expected_name)
                        })
                }))
    })
}

/// Rejects class overrides whose parameters the parent's callers would pass in another physical
/// representation.
///
/// A call through the parent's static type materializes every argument with the parent's
/// signature and jumps to the child's vtable entry, and no adapter converts the words yet.
/// So every declared parameter, by-reference ones included (the reference cell's payload
/// is read with the child's layout), must keep the parent's representation, an untyped child
/// parameter cannot replace a declared one (its type is only inferred later from call sites),
/// and a renamed variadic must still accept the parent's element type.
fn validate_override_parameter_abi(
    checker: &Checker,
    class_name: &str,
    method_name: &str,
    kind: &str,
    child_sig: &FunctionSig,
    parent_sig: &FunctionSig,
    span: crate::span::Span,
    family_abi: bool,
) -> Result<(), CompileError> {
    let child = SourceVisibleShape::of(child_sig);
    let parent = SourceVisibleShape::of(parent_sig);
    let variadic_index = child.variadic.as_ref().map(|_| child.param_count.saturating_sub(1));
    let widen_error = |index: usize, detail: String| {
        CompileError::new(
            span,
            &format!(
                "Cannot widen parameter representation when overriding {}: {}::{} parameter ${} ({}; not yet supported)",
                kind, class_name, method_name, child.param_names[index], detail
            ),
        )
    };
    for index in 0..child.param_count.min(parent.param_count) {
        let by_ref = child.ref_params.get(index).copied().unwrap_or(false);
        let parent_declared = parent.declared_params.get(index).copied().unwrap_or(false);
        let child_declared = child.declared_params.get(index).copied().unwrap_or(false);
        // A by-value, non-variadic position is boxed by the family ABI when it differs. PHP's
        // own rule still applies: a declared override parameter must accept what the parent's
        // declared type accepts (the representation rule below used to catch `int` -> `string`).
        if family_abi && !by_ref && Some(index) != variadic_index {
            if parent_declared
                && child_declared
                && !checker.type_accepts(&child.param_types[index], &parent.param_types[index])
            {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "Cannot narrow parameter ${} when overriding {}: {}::{} (expects {}, parent accepts {})",
                        child.param_names[index],
                        kind,
                        class_name,
                        method_name,
                        child.param_types[index],
                        parent.param_types[index]
                    ),
                ));
            }
            // The family body re-applies PHP's coercion only for a plain scalar
            // (`virtual_abi::body_contract`). A declared union with scalar members would receive
            // the boxed argument uncoerced (`12345` stays an int under `string|array`), so a
            // union override is only sound while the parent already passes that representation.
            let child_ty = &child.param_types[index];
            if child_declared
                && crate::types::override_families::union_needs_scalar_coercion(child_ty)
                && (!parent_declared
                    || !crate::types::override_families::same_abi(child_ty, &parent.param_types[index]))
            {
                let parent_passes = if parent_declared {
                    parent.param_types[index].to_string()
                } else {
                    "an untyped value".to_string()
                };
                return Err(widen_error(
                    index,
                    format!(
                        "the parent passes {}, the override expects the union {} and elephc cannot apply PHP's union coercion to the boxed argument",
                        parent_passes, child_ty
                    ),
                ));
            }
            continue;
        }
        let parent_ty = &parent.param_types[index];
        let child_ty = &child.param_types[index];
        if parent_declared && !child_declared {
            return Err(widen_error(
                index,
                format!("the parent passes {}, the override drops the declared type", parent_ty),
            ));
        }
        if parent_declared && child_declared && !child_ty.reference_payload_compatible(parent_ty) {
            return Err(widen_error(
                index,
                format!("the parent passes {}, the override expects {}", parent_ty, child_ty),
            ));
        }
    }
    if child.variadic.is_some() && parent.variadic.is_some() {
        if let (Some(child_ty), Some(parent_ty)) =
            (child.param_types.last(), parent.param_types.last())
        {
            let index = child.param_count - 1;
            if !checker.type_accepts(child_ty, parent_ty) {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "Cannot narrow variadic parameter ${} when overriding {}: {}::{} (expects {}, parent accepts {})",
                        child.param_names[index], kind, class_name, method_name, child_ty, parent_ty
                    ),
                ));
            }
            if !child_ty.reference_payload_compatible(parent_ty) {
                return Err(widen_error(
                    index,
                    format!("the parent passes {}, the override expects {}", parent_ty, child_ty),
                ));
            }
        }
    }
    Ok(())
}

/// Returns whether an override's default denotes the inherited default's value.
///
/// Plain defaults compare structurally. A default spelled with `self::`/`parent::` on either side
/// is compared after resolving those constants in each side's own scope, so `self::X` declared
/// only on the parent (or inherited from an interface) matches, a redefined `X` does not, and
/// `static::` never does.
fn inherited_default_unchanged(
    parent_default: &Expr,
    child_default: &Expr,
    same_relative_default: &dyn Fn(&Expr, &Expr) -> bool,
) -> bool {
    if default_fetches_class_constant(parent_default) || default_fetches_class_constant(child_default) {
        same_relative_default(parent_default, child_default)
    } else {
        parent_default == child_default
    }
}

/// The class-like scope a `self::`/`parent::` default is resolved in.
pub(crate) enum DefaultScope<'a> {
    /// The class being built (not registered in the checker yet).
    Flattened(&'a crate::types::traits::FlattenedClass),
    /// An already registered class or interface, by name.
    Registered(&'a str),
}

/// Returns whether two defaults that use `self::`/`parent::` evaluate to the same constant values
/// once each is resolved in its own declaring scope (`self::X` declared only on the parent, or
/// inherited from an interface, names the same constant from both sides). `static::` is late
/// bound and never compares equal; anything the resolver does not understand counts as changed.
pub(crate) fn relative_defaults_name_same_value(
    checker: &Checker,
    parent_default: &Expr,
    parent_scope: &DefaultScope<'_>,
    child_default: &Expr,
    child_scope: &DefaultScope<'_>,
) -> bool {
    match (
        canonical_default(checker, parent_default, parent_scope, 0),
        canonical_default(checker, child_default, child_scope, 0),
    ) {
        (Some(parent), Some(child)) => parent == child,
        _ => false,
    }
}

/// Replaces each class constant fetch in a default (`self::X`, `parent::X`, `Other::X`) by the
/// constant's value, resolved in the scope that declares it, and folds simple integer/float
/// arithmetic and string concatenation over literals. Returns `None` for anything it cannot
/// resolve with certainty (`static::`, unknown constants, unsupported expression kinds that
/// still mention a class-relative receiver), which the caller treats as a changed default.
fn canonical_default(
    checker: &Checker,
    expr: &Expr,
    scope: &DefaultScope<'_>,
    depth: usize,
) -> Option<Expr> {
    if depth > 16 {
        return None;
    }
    let kind = match &expr.kind {
        ExprKind::ScopedConstantAccess { receiver, name } => {
            let lookup_scope = match receiver {
                StaticReceiver::Self_ => scope_clone(scope),
                StaticReceiver::Parent => DefaultScope::Registered(scope_parent(checker, scope)?),
                StaticReceiver::Named(class) => {
                    let class = class.as_str().trim_start_matches('\\');
                    match scope {
                        DefaultScope::Flattened(flat) if flat.name == class => scope_clone(scope),
                        _ if checker.classes.contains_key(class)
                            || checker.interfaces.contains_key(class) =>
                        {
                            DefaultScope::Registered(registered_name(checker, class)?)
                        }
                        // Not a class or interface constant the checker knows (an enum case, a
                        // builtin): keep it as written; it compares structurally.
                        _ => return Some(expr.clone()),
                    }
                }
                _ => return None,
            };
            let (value, declaring) = scoped_constant_value(checker, &lookup_scope, name, 0)?;
            return canonical_default(checker, &value, &declaring, depth + 1);
        }
        ExprKind::BinaryOp { left, op, right } => {
            let left = canonical_default(checker, left, scope, depth)?;
            let right = canonical_default(checker, right, scope, depth)?;
            if let Some(folded) = fold_literal_binary(&left.kind, op, &right.kind) {
                folded
            } else {
                ExprKind::BinaryOp {
                    left: Box::new(left),
                    op: op.clone(),
                    right: Box::new(right),
                }
            }
        }
        ExprKind::Negate(inner) => match canonical_default(checker, inner, scope, depth)?.kind {
            ExprKind::IntLiteral(value) => ExprKind::IntLiteral(value.checked_neg()?),
            ExprKind::FloatLiteral(value) => ExprKind::FloatLiteral(-value),
            other => ExprKind::Negate(Box::new(Expr::new(other, inner.span))),
        },
        ExprKind::Not(inner) => {
            ExprKind::Not(Box::new(canonical_default(checker, inner, scope, depth)?))
        }
        ExprKind::ArrayLiteral(items) => ExprKind::ArrayLiteral(
            items
                .iter()
                .map(|item| canonical_default(checker, item, scope, depth))
                .collect::<Option<Vec<_>>>()?,
        ),
        _ if default_is_class_relative(expr) || default_fetches_class_constant(expr) => {
            return None
        }
        other => other.clone(),
    };
    Some(Expr::new(kind, expr.span))
}

/// Folds `int|float (+ - *) int|float` and `string . string` over literals, as PHP evaluates
/// them in a constant expression. Anything else (division, overflow, mixed kinds) stays unfolded.
fn fold_literal_binary(
    left: &ExprKind,
    op: &crate::parser::ast::BinOp,
    right: &ExprKind,
) -> Option<ExprKind> {
    use crate::parser::ast::BinOp;
    match (left, op, right) {
        (ExprKind::IntLiteral(a), BinOp::Add, ExprKind::IntLiteral(b)) => {
            a.checked_add(*b).map(ExprKind::IntLiteral)
        }
        (ExprKind::IntLiteral(a), BinOp::Sub, ExprKind::IntLiteral(b)) => {
            a.checked_sub(*b).map(ExprKind::IntLiteral)
        }
        (ExprKind::IntLiteral(a), BinOp::Mul, ExprKind::IntLiteral(b)) => {
            a.checked_mul(*b).map(ExprKind::IntLiteral)
        }
        (ExprKind::StringLiteral(a), BinOp::Concat, ExprKind::StringLiteral(b)) => {
            Some(ExprKind::StringLiteral(format!("{}{}", a, b)))
        }
        _ => None,
    }
}

fn scope_clone<'a>(scope: &DefaultScope<'a>) -> DefaultScope<'a> {
    match scope {
        DefaultScope::Flattened(class) => DefaultScope::Flattened(class),
        DefaultScope::Registered(owner) => DefaultScope::Registered(owner),
    }
}

/// Returns the checker's own spelling of a registered class or interface name.
fn registered_name<'c>(checker: &'c Checker, name: &str) -> Option<&'c str> {
    checker
        .classes
        .get_key_value(name)
        .map(|(key, _)| key.as_str())
        .or_else(|| checker.interfaces.get_key_value(name).map(|(key, _)| key.as_str()))
}

/// The registered parent class of `scope`, for `parent::`.
fn scope_parent<'c>(checker: &'c Checker, scope: &DefaultScope<'_>) -> Option<&'c str> {
    let parent = match scope {
        DefaultScope::Flattened(class) => class.extends.as_deref()?,
        DefaultScope::Registered(owner) => checker.classes.get(*owner)?.parent.as_deref()?,
    };
    registered_name(checker, parent)
}

/// Looks up constant `name` as `self::name` sees it from `scope`, returning its value together
/// with the scope that declares it (a constant's value is written in its declaring class).
/// Searches the class's own constants, then its parent chain, then its interfaces.
fn scoped_constant_value<'a>(
    checker: &'a Checker,
    scope: &DefaultScope<'a>,
    name: &str,
    depth: usize,
) -> Option<(Expr, DefaultScope<'a>)> {
    if depth > 32 {
        return None;
    }
    match scope {
        DefaultScope::Flattened(class) => {
            if let Some(constant) = class.constants.iter().find(|constant| constant.name == name) {
                return Some((constant.value.clone(), scope_clone(scope)));
            }
            class
                .extends
                .as_deref()
                .and_then(|parent| registered_name(checker, parent))
                .and_then(|parent| {
                    scoped_constant_value(checker, &DefaultScope::Registered(parent), name, depth + 1)
                })
                .or_else(|| {
                    class.implements.iter().find_map(|iface| {
                        let iface = registered_name(checker, iface)?;
                        scoped_constant_value(checker, &DefaultScope::Registered(iface), name, depth + 1)
                    })
                })
        }
        DefaultScope::Registered(owner) => {
            if let Some(info) = checker.classes.get(*owner) {
                if let Some(value) = info.constants.get(name) {
                    return Some((value.clone(), DefaultScope::Registered(owner)));
                }
                return info
                    .parent
                    .as_deref()
                    .and_then(|parent| registered_name(checker, parent))
                    .and_then(|parent| {
                        scoped_constant_value(checker, &DefaultScope::Registered(parent), name, depth + 1)
                    })
                    .or_else(|| {
                        info.interfaces.iter().find_map(|iface| {
                            let iface = registered_name(checker, iface)?;
                            scoped_constant_value(checker, &DefaultScope::Registered(iface), name, depth + 1)
                        })
                    });
            }
            // Interface metadata already merges inherited constants; the declaring interface is
            // recorded for each, which is the scope its value is written in.
            let info = checker.interfaces.get(*owner)?;
            let value = info.constants.get(name)?.clone();
            let declaring = info
                .constant_declaring_interfaces
                .get(name)
                .and_then(|declaring| registered_name(checker, declaring))
                .unwrap_or(owner);
            Some((value, DefaultScope::Registered(declaring)))
        }
    }
}

/// Returns whether a default reads any class constant (`X::NAME`, including relative ones).
fn default_fetches_class_constant(default: &Expr) -> bool {
    format!("{:?}", default.kind).contains("ScopedConstantAccess")
}

/// Returns whether a default names its declaring class relatively (`self::X`, `static::X`,
/// `parent::X`). Two such defaults can be spelled identically in parent and override yet resolve
/// against different classes, so they never count as the same inherited default.
fn default_is_class_relative(default: &Expr) -> bool {
    let debug = format!("{:?}", default.kind);
    ["receiver: Self_", "receiver: Static ", "receiver: Static,", "receiver: Parent"]
        .iter()
        .any(|marker| debug.contains(marker))
}

/// Returns whether an override belongs to a source override family that
/// `ir_lower::program::virtual_abi` unifies, mirroring `crate::types::override_families`:
/// a slot-dispatched (non-private, non-magic, non-generic) method whose every declaring ancestor
/// is a user-source class. A family that reaches a builtin or compiler-injected body keeps that
/// body's fixed contract and is not unified, so its overrides stay under the strict rules.
fn override_family_unifies(
    checker: &Checker,
    class: &crate::types::traits::FlattenedClass,
    method_name: &str,
    visibility: &Visibility,
    is_static: bool,
) -> bool {
    let method_key = php_symbol_key(method_name);
    if method_key.starts_with("__") || method_key.contains('<') || *visibility == Visibility::Private {
        return false;
    }
    let mut seen = std::collections::HashSet::new();
    let mut cursor = class.extends.clone();
    let mut reached_declaration = false;
    while let Some(ancestor) = cursor {
        if !seen.insert(ancestor.clone()) {
            return false;
        }
        let Some(info) = checker.classes.get(&ancestor) else {
            return false;
        };
        let (table, impls, visibilities) = if is_static {
            (&info.static_methods, &info.static_method_impl_classes, &info.static_method_visibilities)
        } else {
            (&info.methods, &info.method_impl_classes, &info.method_visibilities)
        };
        if !table.contains_key(&method_key) {
            break;
        }
        if visibilities.get(&method_key) == Some(&Visibility::Private) {
            // A private ancestor is not dispatched: `override_families::groups` roots the family
            // below it, so the walk stops here as well.
            break;
        }
        reached_declaration = true;
        if let Some(owner) = impls.get(&method_key) {
            let source_body = checker.classes.get(owner).is_some_and(|owner_info| {
                !owner_info.is_internal
                    && owner_info.method_decls.iter().any(|decl| {
                        decl.is_static == is_static
                            && php_symbol_key(&decl.name) == method_key
                            && decl.span.line != 0
                    })
            });
            if !source_body {
                return false;
            }
        } else if info.is_internal || info.declaration_span == crate::span::Span::dummy() {
            // An abstract declaration on a builtin.
            return false;
        }
        cursor = info.parent.clone();
    }
    reached_declaration
}

