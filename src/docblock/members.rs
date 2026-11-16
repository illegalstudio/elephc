//! Purpose:
//! Applies generic PHPDoc to properties and methods, including method-local templates.
//! Keeps class scope separate from templates bound by an individual method call.
//!
//! Called from:
//! - `crate::docblock::apply_to_stmt()` for class, interface, trait and enum members.
//!
//! Key details:
//! - Native method templates take precedence over PHPDoc.
//! - Ordinary member annotations only apply when they mention a class type parameter.
//! - A method template honours annotations naming its own or the class's type parameters.
//! - A bodiless (interface/abstract) method keeps its native signature: its PHPDoc types are
//!   claims about dynamic dispatch that php-src never enforces.
//! - Promoted properties retain the annotated constructor parameter's storage type.

use std::collections::HashMap;

use crate::parser::ast::{ClassMethod, ClassProperty, NativeHint, TypeExpr, TypeParam};

use super::{DocBlock, NameScope};

/// Applies class-scoped member annotations and adopts each method's own template declaration.
pub(super) fn apply(
    class_templates: &mut [TypeParam],
    properties: &mut [ClassProperty],
    methods: &mut [ClassMethod],
    blocks: &HashMap<(u32, u32), DocBlock>,
    scope: &NameScope,
) {
    anchor_class_hints(class_templates, properties, methods, blocks, scope);
    let class_templates: &[TypeParam] = class_templates;
    let class_names: Vec<String> =
        class_templates.iter().map(|param| param.name.clone()).collect();
    let class_params = class_names.as_slice();
    for property in properties.iter_mut() {
        if let Some(annotated) = blocks
            .get(&(property.span.line, property.span.col))
            .and_then(|block| block.var_type.as_ref())
        {
            apply_signature_type(
                &mut property.type_expr,
                annotated,
                class_params,
                class_templates,
                scope,
            );
        }
    }
    let mut promoted = Vec::new();
    for method in methods.iter_mut() {
        let Some(block) = blocks.get(&(method.span.line, method.span.col)) else {
            continue;
        };
        if !method.type_params.is_empty() {
            continue;
        }
        // A method with no body (an interface or abstract method) is only ever reached through
        // dynamic dispatch, and an instantiated generic method never takes a vtable slot. Its
        // PHPDoc is therefore not adopted at all: the method keeps its native signature, so an
        // implementation that omits a type PHPDoc only claimed is still compatible, exactly as
        // php-src sees it.
        if !method.has_body {
            continue;
        }
        // The names an annotation must mention to be honoured: the class's parameters, plus the
        // method's own when it is a template. `@param string $key` beside a `@template` says
        // nothing generic, and adopting it would narrow a parameter the native signature (and
        // every interface the method implements) leaves untyped.
        let mut in_scope: Vec<String> = class_params.to_vec();
        let own: Vec<TypeParam> = if block.declares_generics() {
            in_scope.extend(block.type_params.iter().map(|param| param.name.clone()));
            let anchored = anchor_native_hints(
                &block.type_params,
                &annotated_positions(
                    &method.params,
                    method.variadic.as_deref(),
                    method.variadic_type.as_ref(),
                    method.return_type.as_ref(),
                    block,
                ),
                scope,
            );
            method.type_params = anchored.clone();
            anchored
        } else {
            Vec::new()
        };
        let own = own.as_slice();
        // The method's own templates shadow a class template of the same name.
        let templates: Vec<TypeParam> = own
            .iter()
            .chain(
                class_templates
                    .iter()
                    .filter(|param| !own.iter().any(|shadow| shadow.name == param.name)),
            )
            .cloned()
            .collect();
        let templates = templates.as_slice();
        let in_scope = in_scope.as_slice();
        let is_constructor = method.name.eq_ignore_ascii_case("__construct");
        for (name, declared, _, _) in method.params.iter_mut() {
            let Some(annotated) = block.params.get(name) else {
                continue;
            };
            if apply_signature_type(declared, annotated, in_scope, templates, scope)
                && is_constructor
            {
                promoted.extend(declared.clone().map(|adopted| (name.clone(), adopted)));
            }
        }
        if let Some(annotated) = method
            .variadic
            .as_ref()
            .and_then(|name| block.params.get(name))
        {
            apply_signature_type(&mut method.variadic_type, annotated, in_scope, templates, scope);
        }
        if let Some(annotated) = &block.return_type {
            apply_signature_type(&mut method.return_type, annotated, in_scope, templates, scope);
        }
        // A template whose `@param` lost to a native hint (`@param T $v` on `mixed $v`) but
        // whose `@return T` was adopted is kept: no argument binds it any more, and a call that
        // leaves a PHPDoc parameter undetermined reads it as `mixed`
        // (`crate::generics::infer_bindings_with_args`), which is the native signature php-src
        // runs. Keeping it is what lets a written `$repo->id<int>(7)` still select an
        // instantiation.
        //
        // A template no position adopted (every annotation lost to a native hint) changes no
        // declared type, so the method is not a template at all: it keeps its name and its
        // vtable slot, exactly as php-src declares it.
        if !own.is_empty() {
            let names: Vec<String> = own.iter().map(|param| param.name.clone()).collect();
            let adopted = method
                .params
                .iter()
                .map(|(_, declared, _, _)| declared)
                .chain([&method.variadic_type, &method.return_type])
                .flatten()
                .any(|declared| declared.mentions_type_param(&names));
            if !adopted {
                method.type_params.clear();
            }
        }
    }
    // The parser creates a promoted parameter and its property separately. Both must agree.
    for property in properties.iter_mut().filter(|property| property.is_promoted) {
        if let Some((_, annotated)) = promoted.iter().find(|(name, _)| *name == property.name) {
            property.type_expr = Some(annotated.clone());
        }
    }
}

/// Applies a property, parameter or return annotation, only when it mentions a type parameter
/// in scope (class or method) and refines, rather than contradicts, the native hint.
///
/// PHPDoc may sharpen a hint php-src treats loosely (`array`, `iterable`, `callable`, `object`,
/// none at all, or the bare name of the class it parameterizes). A native `mixed` is kept. A
/// native class or scalar hint is a contract php-src enforces and every override is checked
/// against, so it wins: `getIterator(): Traversable` documented
/// `@return ArrayIterator<TKey, TValue>` stays `Traversable`.
///
/// A position annotated directly with a template (`@param T $m`, `@return T`, `@var T`) hands
/// its native hint over to the type argument, so it does only where the template recorded that
/// hint (`anchor_native_hints`) and the checker keeps enforcing it. And whether the position
/// accepts `null` is the native hint's to say, never the annotation's.
fn apply_signature_type(
    declared: &mut Option<TypeExpr>,
    annotated: &TypeExpr,
    in_scope: &[String],
    own: &[TypeParam],
    scope: &NameScope,
) -> bool {
    if !annotated.mentions_type_param(in_scope) {
        return false;
    }
    let Some(adopted) = adopted_signature_type(declared.as_ref(), annotated, own, scope) else {
        return false;
    };
    *declared = Some(adopted);
    true
}

/// Returns the type a property, parameter or return position takes from its annotation, or
/// `None` when its native hint wins.
///
/// `own` are the templates in scope after `anchor_native_hints`: the declaration's own, then
/// the enclosing class's. An untyped position takes the annotation as written. A hinted one
/// annotated directly with a PHPDoc template is adopted when the hint is the one the template
/// recorded. Any other annotation must refine the hint (`refines_native`), and a union only
/// does when every member refines it and none is a template on its own: `T|false` on
/// `: object` would let `false` through, and so would `T|Model` with `T` bound to `int`.
/// Either way the adopted type keeps the native hint's nullability: `?Model $m` documented
/// `@param T $m` becomes `?T`, and `Model $m` documented `@param ?T $m` becomes `T`, because
/// php-src decides from the hint alone whether `null` is accepted.
pub(super) fn adopted_signature_type(
    native: Option<&TypeExpr>,
    annotated: &TypeExpr,
    own: &[TypeParam],
    scope: &NameScope,
) -> Option<TypeExpr> {
    let Some(native) = native else {
        return Some(annotated.clone());
    };
    let hinted = strip_nullable(native);
    let bare = without_null(annotated);
    match direct_template(&bare, own) {
        Some(param) if param.from_docblock => {
            let hint = param.native_hint.as_ref()?;
            if !hint_covers(&hint.ty, hinted, scope) {
                return None;
            }
        }
        // A native type parameter (`class Box<T>`) documented on a member: as before, it
        // replaces a loose hint and nothing else.
        Some(_) => {
            if !is_loose_hint(hinted) {
                return None;
            }
        }
        None => {
            let members = match &bare {
                TypeExpr::Union(members) => members.as_slice(),
                single => std::slice::from_ref(single),
            };
            let refines = members.iter().all(|member| {
                (members.len() == 1 || direct_template(member, own).is_none())
                    && refines_native(hinted, member, scope)
            });
            if !refines {
                return None;
            }
        }
    }
    Some(match (native, bare) {
        (TypeExpr::Nullable(_), TypeExpr::Union(mut members)) => {
            members.push(TypeExpr::Named(crate::names::Name::unqualified("null")));
            TypeExpr::Union(members)
        }
        (TypeExpr::Nullable(_), bare) => TypeExpr::Nullable(Box::new(bare)),
        (_, bare) => bare,
    })
}

/// Returns `annotated` without its `null`: `?T` and `T|null` are `T`.
fn without_null(annotated: &TypeExpr) -> TypeExpr {
    match annotated {
        TypeExpr::Nullable(inner) => (**inner).clone(),
        TypeExpr::Union(members) if members.iter().any(is_null) => {
            let mut rest: Vec<TypeExpr> =
                members.iter().filter(|member| !is_null(member)).cloned().collect();
            if rest.len() == 1 {
                rest.remove(0)
            } else {
                TypeExpr::Union(rest)
            }
        }
        other => other.clone(),
    }
}

/// One annotated position of a signature: its name (`None` for the return type), its native
/// hint and its annotation.
pub(super) type Position<'a> = (Option<&'a str>, Option<&'a TypeExpr>, &'a TypeExpr);

/// Lists every annotated parameter (the variadic included) and the annotated return type.
pub(super) fn annotated_positions<'a>(
    params: &'a [(String, Option<TypeExpr>, Option<crate::parser::ast::Expr>, bool)],
    variadic: Option<&'a str>,
    variadic_type: Option<&'a TypeExpr>,
    return_type: Option<&'a TypeExpr>,
    block: &'a DocBlock,
) -> Vec<Position<'a>> {
    let mut positions: Vec<Position<'a>> = params
        .iter()
        .filter_map(|(name, declared, _, _)| {
            block
                .params
                .get(name)
                .map(|annotated| (Some(name.as_str()), declared.as_ref(), annotated))
        })
        .collect();
    if let Some((name, annotated)) =
        variadic.and_then(|name| block.params.get(name).map(|annotated| (name, annotated)))
    {
        positions.push((Some(name), variadic_type, annotated));
    }
    if let Some(annotated) = &block.return_type {
        positions.push((None, return_type, annotated));
    }
    positions
}

/// Records on each template the native hint the positions annotated directly with it replace.
///
/// `@param T $m` on a native `Model $m` (or `@return T` on `: Model`) trades a hint php-src
/// enforces for a type argument, so the hint is kept on the template
/// (`TypeParam::native_hint`) and stays binding. That is sound only when the template speaks
/// for ONE hint: every natively hinted position annotated directly with it must carry the same
/// replaceable hint, or an `object` beside the class the others name (the class is the
/// narrower of the two, so it speaks for both). Positions whose hints differ otherwise record
/// nothing and keep their hints. A hint the template cannot replace at all (`int`, `mixed`,
/// an unrelated class) is not counted: that position keeps it whatever the others do. An
/// untyped position constrains nothing and simply takes the template.
pub(super) fn anchor_native_hints(
    declared: &[TypeParam],
    positions: &[Position],
    scope: &NameScope,
) -> Vec<TypeParam> {
    declared
        .iter()
        .map(|param| {
            let hinted: Vec<(Option<&str>, &TypeExpr)> = positions
                .iter()
                .filter(|(_, _, annotated)| {
                    direct_template(annotated, std::slice::from_ref(param)).is_some()
                })
                .filter_map(|(name, native, _)| native.map(|native| (*name, native)))
                .filter(|(_, native)| replaceable_by(strip_nullable(native), param, scope))
                .collect();
            let candidate = hinted
                .iter()
                .map(|(_, native)| strip_nullable(native))
                .find(|native| !is_object_hint(native))
                .or_else(|| hinted.first().map(|(_, native)| strip_nullable(native)));
            let native_hint = candidate
                .filter(|candidate| {
                    hinted
                        .iter()
                        .all(|(_, native)| hint_covers(candidate, strip_nullable(native), scope))
                })
                .map(|candidate| NativeHint {
                    ty: candidate.clone(),
                    params: hinted
                        .iter()
                        .filter_map(|(name, native)| {
                            name.map(|name| (name.to_string(), (*native).clone()))
                        })
                        .collect(),
                    return_type: hinted
                        .iter()
                        .find(|(name, _)| name.is_none())
                        .map(|(_, native)| (*native).clone()),
                    member_returns: Vec::new(),
                    member_params: Vec::new(),
                });
            TypeParam { native_hint, ..param.clone() }
        })
        .collect()
}

/// Records on each PHPDoc class template the native hint its members let it replace.
///
/// The rule is `anchor_native_hints`, applied to every position of the class at once: the
/// `@var` of each property and the `@param`/`@return` of each method that adopts annotations.
/// A class template has one type argument per instantiation, so one hint must speak for all of
/// them. Only a loosely enforced hint qualifies: a class hint (`Model $m`) is never handed to a
/// class template, bounded or not. The method positions it took over are recorded per method
/// (`NativeHint::member_returns`, `NativeHint::member_params`), which is what lets a dispatched
/// method erased for the class template keep the signature php-src enforces (see
/// `crate::generics::erasure`).
fn anchor_class_hints(
    class_templates: &mut [TypeParam],
    properties: &[ClassProperty],
    methods: &[ClassMethod],
    blocks: &HashMap<(u32, u32), DocBlock>,
    scope: &NameScope,
) {
    if !class_templates.iter().any(|param| param.from_docblock) {
        return;
    }
    // The methods that adopt annotations, each with its positions that name no template of the
    // method's own: such a template shadows a class template of the same name.
    let adopting: Vec<(&ClassMethod, Vec<Position>)> = methods
        .iter()
        .filter(|method| method.type_params.is_empty() && method.has_body)
        .filter_map(|method| {
            let block = blocks.get(&(method.span.line, method.span.col))?;
            let own: &[TypeParam] =
                if block.declares_generics() { &block.type_params } else { &[] };
            let positions = annotated_positions(
                &method.params,
                method.variadic.as_deref(),
                method.variadic_type.as_ref(),
                method.return_type.as_ref(),
                block,
            )
            .into_iter()
            .filter(|(_, _, annotated)| direct_template(annotated, own).is_none())
            .collect();
            Some((method, positions))
        })
        .collect();
    let mut positions: Vec<Position> = Vec::new();
    for property in properties {
        if let Some(annotated) = blocks
            .get(&(property.span.line, property.span.col))
            .and_then(|block| block.var_type.as_ref())
        {
            positions.push((None, property.type_expr.as_ref(), annotated));
        }
    }
    positions.extend(adopting.iter().flat_map(|(_, positions)| positions.iter().copied()));
    // Anchored without its bound, so `T of Model` does not claim a native `Model` hint.
    let unbounded: Vec<TypeParam> = class_templates
        .iter()
        .map(|param| TypeParam { bound: None, ..param.clone() })
        .collect();
    let anchored = anchor_native_hints(&unbounded, &positions, scope);
    for (param, anchored) in class_templates.iter_mut().zip(anchored) {
        if !param.from_docblock {
            continue;
        }
        param.native_hint = anchored.native_hint.map(|hint| {
            let mut member_returns = Vec::new();
            let mut member_params = Vec::new();
            for (method, positions) in &adopting {
                let taken = positions.iter().filter_map(|(name, native, annotated)| {
                    let native = (*native)?;
                    (direct_template(annotated, std::slice::from_ref(param)).is_some()
                        && hint_covers(&hint.ty, strip_nullable(native), scope))
                    .then_some((*name, native))
                });
                for (name, native) in taken {
                    let method_key = method.name.to_ascii_lowercase();
                    match name {
                        Some(name) => {
                            member_params.push((method_key, name.to_string(), native.clone()))
                        }
                        None => member_returns.push((method_key, native.clone())),
                    }
                }
            }
            NativeHint {
                ty: hint.ty,
                params: Vec::new(),
                return_type: None,
                member_returns,
                member_params,
            }
        });
    }
}

/// Returns whether the recorded hint `hint` speaks for a position natively hinted `native`:
/// the same type, or `object` under a class hint.
fn hint_covers(hint: &TypeExpr, native: &TypeExpr, scope: &NameScope) -> bool {
    same_hint(hint, native, scope)
        || (is_object_hint(native)
            && matches!(hint, TypeExpr::Named(name) if !is_loose_keyword(name.as_str())))
}

/// Returns whether `ty` is the native `object` hint.
fn is_object_hint(ty: &TypeExpr) -> bool {
    matches!(ty, TypeExpr::Named(name)
        if name.as_str().trim_start_matches('\\').eq_ignore_ascii_case("object"))
}

/// Returns whether `written` is a hint php-src enforces loosely enough for an annotation to
/// refine. `mixed` is not one: it is already the widest type, so the PHPDoc cannot sharpen it
/// without a later erasure (a dispatched method) having to guess whether to give the `mixed`
/// back or leave the position untyped, and only `mixed` keeps an implementation of
/// `JsonSerializable::jsonSerialize(): mixed` compatible.
fn is_loose_keyword(written: &str) -> bool {
    let written = written.trim_start_matches('\\');
    ["callable", "object", "closure", "array", "iterable"]
        .iter()
        .any(|loose| written.eq_ignore_ascii_case(loose))
}

/// Returns the template an annotation names directly: `T`, `?T` or `T|null`.
fn direct_template<'a>(annotated: &TypeExpr, own: &'a [TypeParam]) -> Option<&'a TypeParam> {
    match annotated {
        TypeExpr::Named(name) => own.iter().find(|param| param.name == name.as_str()),
        TypeExpr::Nullable(inner) => direct_template(inner, own),
        TypeExpr::Union(members) => {
            let mut rest = members.iter().filter(|member| !is_null(member));
            let template = direct_template(rest.next()?, own)?;
            rest.next().is_none().then_some(template)
        }
        _ => None,
    }
}

/// Returns whether `ty` is the `null` member of a union.
fn is_null(ty: &TypeExpr) -> bool {
    matches!(ty, TypeExpr::Void)
        || matches!(ty, TypeExpr::Named(name) if name.as_str().eq_ignore_ascii_case("null"))
}

/// Returns the type under a leading `?`.
fn strip_nullable(ty: &TypeExpr) -> &TypeExpr {
    match ty {
        TypeExpr::Nullable(inner) => inner,
        other => other,
    }
}

/// Returns whether two native hints are the same type: a keyword compared as php-src compares
/// it, a class by the name it resolves to in `scope`.
fn same_hint(a: &TypeExpr, b: &TypeExpr, scope: &NameScope) -> bool {
    match (a, b) {
        (TypeExpr::Named(a), TypeExpr::Named(b)) => {
            is_loose_keyword(a.as_str()) == is_loose_keyword(b.as_str())
                && (same_class(a, b, scope)
                    || (is_loose_keyword(a.as_str())
                        && a.as_str()
                            .trim_start_matches('\\')
                            .eq_ignore_ascii_case(b.as_str().trim_start_matches('\\'))))
        }
        _ => a == b,
    }
}

/// Returns whether a position annotated directly with `param` may give up the hint `native`:
/// a hint php-src treats loosely, or the class that bounds the template (`T of Model` on
/// `Model`, where the bound says every `T` is one).
fn replaceable_by(native: &TypeExpr, param: &TypeParam, scope: &NameScope) -> bool {
    is_loose_hint(native)
        || matches!(
            (native, &param.bound),
            (TypeExpr::Named(name), Some(TypeExpr::Named(bound))) if same_class(bound, name, scope)
        )
}

/// Returns whether `native` is a hint php-src enforces loosely enough for an annotation to
/// refine: `array` (in any spelling), `iterable`, `callable`, `object` or `Closure`.
fn is_loose_hint(native: &TypeExpr) -> bool {
    match native {
        TypeExpr::Array(_) | TypeExpr::AssocArray { .. } | TypeExpr::Iterable => true,
        TypeExpr::Named(name) => is_loose_keyword(name.as_str()),
        _ => false,
    }
}

/// Returns whether two class names, both written in `scope`, name the same class.
///
/// Compared after resolving each against the namespace and the `use` imports in effect, never
/// by basename: `Crate $b` under `use Lib\Box as Crate` is `\Lib\Box<T>`, and `App\Box` is not.
fn same_class(a: &crate::names::Name, b: &crate::names::Name, scope: &NameScope) -> bool {
    scope.class_key(a) == scope.class_key(b)
}

/// Returns whether `annotated` (not a union, not nullable) may replace the native hint `native`
/// (not nullable).
///
/// A loose hint gives way to an annotation of its own shape, so that every value the annotation
/// admits is one php-src admits too: `array` to `array<T>`, `iterable` to an array or a class,
/// `object` to a class, `callable` and `Closure` to a callable signature or a class. A class
/// hint gives way to its own generic form (`Box $b` documented `@param Box<T> $b`). Anything
/// else is a contract php-src enforces, and wins.
fn refines_native(native: &TypeExpr, annotated: &TypeExpr, scope: &NameScope) -> bool {
    let array_shaped = matches!(annotated, TypeExpr::Array(_) | TypeExpr::AssocArray { .. });
    let class_shaped = match annotated {
        TypeExpr::GenericClass { .. } => true,
        TypeExpr::Named(name) => {
            let written = name.as_str().trim_start_matches('\\');
            !is_loose_keyword(written)
                && !["mixed", "null"].iter().any(|keyword| written.eq_ignore_ascii_case(keyword))
        }
        _ => false,
    };
    let iterable_shaped = array_shaped || class_shaped || matches!(annotated, TypeExpr::Iterable);
    match native {
        TypeExpr::Array(_) | TypeExpr::AssocArray { .. } => array_shaped,
        TypeExpr::Iterable => iterable_shaped,
        TypeExpr::Named(name) => {
            let written = name.as_str().trim_start_matches('\\').to_ascii_lowercase();
            match written.as_str() {
                "array" => array_shaped,
                "iterable" => iterable_shaped,
                "object" => class_shaped,
                "callable" | "closure" => {
                    class_shaped || matches!(annotated, TypeExpr::CallableSig { .. })
                }
                _ => matches!(
                    annotated,
                    TypeExpr::GenericClass { name: generic, .. } if same_class(generic, name, scope)
                ),
            }
        }
        _ => false,
    }
}
