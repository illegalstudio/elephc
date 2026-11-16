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

use super::DocBlock;

/// Applies class-scoped member annotations and adopts each method's own template declaration.
pub(super) fn apply(
    class_params: &[String],
    properties: &mut [ClassProperty],
    methods: &mut [ClassMethod],
    blocks: &HashMap<(u32, u32), DocBlock>,
) {
    for property in properties.iter_mut() {
        if let Some(annotated) = blocks
            .get(&(property.span.line, property.span.col))
            .and_then(|block| block.var_type.as_ref())
        {
            apply_type(&mut property.type_expr, annotated, class_params, &[]);
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
            );
            method.type_params = anchored.clone();
            anchored
        } else {
            Vec::new()
        };
        let own = own.as_slice();
        let in_scope = in_scope.as_slice();
        let is_constructor = method.name.eq_ignore_ascii_case("__construct");
        for (name, declared, _, _) in method.params.iter_mut() {
            let Some(annotated) = block.params.get(name) else {
                continue;
            };
            if apply_signature_type(declared, annotated, in_scope, own) && is_constructor {
                promoted.extend(declared.clone().map(|adopted| (name.clone(), adopted)));
            }
        }
        if let Some(annotated) = method
            .variadic
            .as_ref()
            .and_then(|name| block.params.get(name))
        {
            apply_signature_type(&mut method.variadic_type, annotated, in_scope, own);
        }
        if let Some(annotated) = &block.return_type {
            apply_signature_type(&mut method.return_type, annotated, in_scope, own);
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

/// Applies an annotation only when it mentions a type parameter in scope (class or method) and
/// refines, rather than contradicts, the native hint.
///
/// PHPDoc may sharpen a hint php-src treats loosely (`array`, `iterable`, `callable`, `object`,
/// none at all, or the bare name of the class it parameterizes, or the bound of a method
/// template). A native `mixed` is kept. A native class or scalar hint is a contract php-src
/// enforces and every override is checked against, so it wins:
/// `getIterator(): Traversable` documented `@return ArrayIterator<TKey, TValue>` stays
/// `Traversable`.
fn apply_type(
    declared: &mut Option<TypeExpr>,
    annotated: &TypeExpr,
    in_scope: &[String],
    method_params: &[TypeParam],
) -> bool {
    if !annotated.mentions_type_param(in_scope)
        || !refines_native(declared.as_ref(), annotated, method_params)
    {
        return false;
    }
    *declared = Some(annotated.clone());
    true
}

/// Applies a parameter or return annotation: `apply_type`'s rule, plus the two a signature needs.
///
/// A position annotated directly with one of the declaration's own templates (`@param T $m`,
/// `@return T`) hands its native hint over to the type argument, so it does only where the
/// template recorded that hint (`anchor_native_hints`) and the checker keeps enforcing it. And
/// whether the position accepts `null` is the native hint's to say, never the annotation's.
fn apply_signature_type(
    declared: &mut Option<TypeExpr>,
    annotated: &TypeExpr,
    in_scope: &[String],
    own: &[TypeParam],
) -> bool {
    if !annotated.mentions_type_param(in_scope) {
        return false;
    }
    let Some(adopted) = adopted_signature_type(declared.as_ref(), annotated, own) else {
        return false;
    };
    *declared = Some(adopted);
    true
}

/// Returns the type a parameter or return position takes from its annotation, or `None` when
/// its native hint wins.
///
/// `own` are the declaration's templates after `anchor_native_hints`. An untyped position takes
/// the annotation as written. A hinted one annotated directly with a template is adopted when
/// the hint is the one the template recorded, and any other annotation follows
/// `refines_native`. Either way the adopted type keeps the native hint's nullability:
/// `?Model $m` documented `@param T $m` becomes `?T`, and `Model $m` documented `@param ?T $m`
/// becomes `T`, because php-src decides from the hint alone whether `null` is accepted.
pub(super) fn adopted_signature_type(
    native: Option<&TypeExpr>,
    annotated: &TypeExpr,
    own: &[TypeParam],
) -> Option<TypeExpr> {
    let Some(native) = native else {
        return Some(annotated.clone());
    };
    match direct_template(annotated, own) {
        Some(param) => {
            let hint = param.native_hint.as_ref()?;
            if !hint_covers(&hint.ty, strip_nullable(native)) {
                return None;
            }
        }
        None if !refines_native(Some(native), annotated, own) => return None,
        None => {}
    }
    Some(with_nullability_of(native, annotated))
}

/// Returns `annotated` accepting `null` exactly when the native hint `native` does.
fn with_nullability_of(native: &TypeExpr, annotated: &TypeExpr) -> TypeExpr {
    let bare = match annotated {
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
    };
    match (native, bare) {
        (TypeExpr::Nullable(_), TypeExpr::Union(mut members)) => {
            members.push(TypeExpr::Named(crate::names::Name::unqualified("null")));
            TypeExpr::Union(members)
        }
        (TypeExpr::Nullable(_), bare) => TypeExpr::Nullable(Box::new(bare)),
        (_, bare) => bare,
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
/// nothing and keep their hints. An untyped position constrains nothing and simply takes the
/// template.
pub(super) fn anchor_native_hints(declared: &[TypeParam], positions: &[Position]) -> Vec<TypeParam> {
    declared
        .iter()
        .map(|param| {
            let hinted: Vec<(Option<&str>, &TypeExpr)> = positions
                .iter()
                .filter(|(_, _, annotated)| {
                    direct_template(annotated, std::slice::from_ref(param)).is_some()
                })
                .filter_map(|(name, native, _)| native.map(|native| (*name, native)))
                .collect();
            let candidate = hinted
                .iter()
                .map(|(_, native)| strip_nullable(native))
                .find(|native| !is_object_hint(native))
                .or_else(|| hinted.first().map(|(_, native)| strip_nullable(native)));
            let native_hint = candidate
                .filter(|candidate| {
                    replaceable_by(candidate, param)
                        && hinted
                            .iter()
                            .all(|(_, native)| hint_covers(candidate, strip_nullable(native)))
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
                });
            TypeParam { native_hint, ..param.clone() }
        })
        .collect()
}

/// Returns whether the recorded hint `hint` speaks for a position natively hinted `native`:
/// the same type, or `object` under a class hint.
fn hint_covers(hint: &TypeExpr, native: &TypeExpr) -> bool {
    same_hint(hint, native)
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

/// Returns whether two native hints are the same type, comparing names as php-src does.
fn same_hint(a: &TypeExpr, b: &TypeExpr) -> bool {
    match (a, b) {
        (TypeExpr::Named(a), TypeExpr::Named(b)) => a
            .as_str()
            .trim_start_matches('\\')
            .eq_ignore_ascii_case(b.as_str().trim_start_matches('\\')),
        _ => a == b,
    }
}

/// Returns whether a position annotated directly with `param` may give up the hint `native`:
/// a hint php-src treats loosely, or the class that bounds the template (`T of Model` on
/// `Model`), the two cases `refines_native` admits.
fn replaceable_by(native: &TypeExpr, param: &TypeParam) -> bool {
    refines_native(
        Some(native),
        &TypeExpr::Named(crate::names::Name::unqualified(&param.name)),
        std::slice::from_ref(param),
    )
}

/// Returns whether `annotated` may replace the native hint `native`.
///
/// `method_params` are the method's own PHPDoc templates: `T` with `@template T of Model`
/// refines a native `Model`, because the bound says every `T` is one. That is the test
/// `anchor_native_hints` applies before recording the hint; an annotation that names a template
/// directly is then adopted through `adopted_signature_type`, which keeps the hint binding.
fn refines_native(
    native: Option<&TypeExpr>,
    annotated: &TypeExpr,
    method_params: &[TypeParam],
) -> bool {
    let Some(native) = native else {
        return true;
    };
    let same_class = |a: &crate::names::Name, b: &crate::names::Name| {
        a.last_segment()
            .zip(b.last_segment())
            .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))
    };
    match native {
        TypeExpr::Array(_) | TypeExpr::AssocArray { .. } | TypeExpr::Iterable => true,
        TypeExpr::Nullable(inner) => {
            let annotated = match annotated {
                TypeExpr::Nullable(inner) => inner.as_ref(),
                other => other,
            };
            refines_native(Some(inner), annotated, method_params)
        }
        TypeExpr::Named(name) => {
            let weak = is_loose_keyword(name.as_str());
            weak || match annotated {
                TypeExpr::GenericClass { name: generic, .. } => same_class(generic, name),
                // A method template bounded by the native class (`T of Model` on `Model`).
                TypeExpr::Named(param) => method_params.iter().any(|tp| {
                    tp.name == param.as_str()
                        && matches!(&tp.bound, Some(TypeExpr::Named(bound)) if same_class(bound, name))
                }),
                _ => false,
            }
        }
        _ => false,
    }
}
