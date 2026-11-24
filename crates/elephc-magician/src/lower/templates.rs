//! Purpose:
//! Retains generic declaration syntax alongside the lowered EvalIR prototype.
//!
//! Called from:
//! - Shared lowering for declarations with type parameters.
//!
//! Key details:
//! - Chopper identifies exact type-reference tokens, including lexically shadowed names.
//! - Magician retains the binding inputs it needs to specialize the prototype later.

use std::cell::Cell;

use elephc_chopper::syntax::{
    names::{Name, NameKind},
    parser::{
        ast::{Stmt, StmtKind, TypeExpr, TypeParam, Variance},
        RuntimeTemplateSyntax,
    },
    span::Span,
};
use elephc_generics::{Parameter, Type};

use crate::{
    eval_ir::{generic_type, EvalGenericTemplate, EvalStmt},
    lexer::{Token, TokenKind},
};

use super::{ordinary_class_name, signatures::{RelativeTypes, lower_type_in_scope}, Scope};

/// Keeps the source tokens and Chopper's type-position metadata aligned.
pub(super) struct TemplateSource<'a> {
    pub tokens: &'a [TokenKind],
    pub lines: &'a [i64],
    pub phpdoc: &'a [bool],
    pub syntax: &'a RuntimeTemplateSyntax,
    pub uses_native_generics: Cell<bool>,
    pub invalid_jump: Cell<bool>,
}

/// Lowers a class-like prototype in its declared type scope and retains its header.
pub(super) fn lower_declaration(scope: &Scope<'_>, mut stmt: Stmt) -> Option<EvalStmt> {
    let span = stmt.span;
    let (name, generics) = match &mut stmt.kind {
        StmtKind::ClassDecl { name, generics, .. }
        | StmtKind::InterfaceDecl { name, generics, .. }
        | StmtKind::TraitDecl { name, generics, .. }
        | StmtKind::EnumDecl { name, generics, .. } => (scope.qualify(name), generics.take()),
        _ => return None,
    };
    let (parameters, parent_args, interface_args) = match generics {
        Some(generics) => (generics.type_params, generics.extends_args, generics.interface_args),
        None => (Vec::new(), Vec::new(), Vec::new()),
    };
    let type_scope = scope.with_type_parameters(&parameters);
    match &mut stmt.kind {
        StmtKind::ClassDecl { extends, implements, .. } => {
            if let Some(parent) = extends.take() {
                let parent = lower_class_arguments(&type_scope, parent, parent_args)?;
                *extends = Some(absolute_name(parent));
            }
            apply_interface_arguments(&type_scope, implements, interface_args)?;
        }
        StmtKind::InterfaceDecl { extends, .. } => apply_interface_arguments(&type_scope, extends, interface_args)?,
        StmtKind::EnumDecl { implements, .. } => apply_interface_arguments(&type_scope, implements, interface_args)?,
        _ => {}
    }
    let kind = if matches!(stmt.kind, StmtKind::ClassDecl { .. }) {
        EvalStmt::ClassDecl(super::classes::lower_class(&type_scope, stmt)?)
    } else {
        super::class_like::lower_class_like(&type_scope, stmt)?
    };
    if parameters.is_empty() && constructor_template(&kind).is_none() {
        Some(kind)
    } else {
        Some(EvalStmt::GenericDecl(Box::new(retain_template(
            &type_scope, parameters, name, span, kind,
        )?)))
    }
}

/// Resolves argument-bearing relations before the ordinary class-like lowering.
fn apply_interface_arguments(
    scope: &Scope<'_>,
    names: &mut Vec<Name>,
    arguments: Vec<Vec<TypeExpr>>,
) -> Option<()> {
    if !arguments.is_empty() {
        *names = lower_class_arguments_list(scope, std::mem::take(names), arguments)?
            .into_iter().map(absolute_name).collect();
    }
    Some(())
}

/// Prevents later class resolution from qualifying an already resolved type name again.
fn absolute_name(name: String) -> Name {
    Name::from_parts(NameKind::FullyQualified, vec![name])
}

/// Resolves the classes selected by one written inheritance or catch clause.
pub(super) fn lower_class_arguments_list(
    scope: &Scope<'_>,
    names: Vec<Name>,
    arguments: Vec<Vec<TypeExpr>>,
) -> Option<Vec<String>> {
    names.into_iter().zip(arguments.into_iter().chain(std::iter::repeat_with(Vec::new)))
        .map(|(name, args)| lower_class_arguments(scope, name, args)).collect()
}

/// Renders a resolved class name with its optional structural type arguments.
fn lower_class_arguments(scope: &Scope<'_>, name: Name, args: Vec<TypeExpr>) -> Option<String> {
    let name = ordinary_class_name(scope, name)?;
    if args.is_empty() {
        Some(name)
    } else {
        Some(Type::Apply(name, lower_types(scope, args)?).to_string())
    }
}

/// Wraps a lowered prototype with its own generic header and substitutions.
pub(super) fn retain_template(
    scope: &Scope<'_>,
    parameter_list: Vec<TypeParam>,
    name: String,
    span: Span,
    prototype: EvalStmt,
) -> Option<EvalGenericTemplate> {
    if parameter_list.iter().any(|parameter| crate::parser::is_reserved_word(&parameter.name)) {
        return None;
    }
    let source = scope.source;
    let start = token_index(span)?;
    let end = usize::try_from(span.end_column()).ok()?;
    let header = if parameter_list.is_empty() {
        None
    } else {
        Some(source.syntax.headers.iter().find(|header| {
            let Some(index) = token_index(header.name) else { return false };
            (start + 1 == index || start + 2 == index)
                && header.parameters.len() == parameter_list.len()
        })?)
    };
    let (name_index, header_start, header_end) = match header {
        Some(header) => (token_index(header.name)?, token_index(header.open)?, token_index(header.after)?),
        None => (start + 1, start + 2, start + 2),
    };
    let constructor = constructor_template(&prototype);
    let native_syntax = header.is_some() && !source.phpdoc.get(header_start).copied().unwrap_or(false)
        || constructor.is_some_and(|template| template.native_syntax);
    if native_syntax {
        source.uses_native_generics.set(true);
    }
    let parameters = parameter_list.into_iter().map(|param| {
        Some(Parameter {
            name: param.name,
            bound: match param.bound {
                Some(ty) => Some(lower_structural_type(scope, ty)?),
                None => None,
            },
            default: match param.default {
                Some(ty) => Some(lower_structural_type(scope, ty)?),
                None => None,
            },
            variance: match param.variance {
                Variance::Invariant => 0,
                Variance::Covariant => 1,
                Variance::Contravariant => -1,
            },
        })
    }).collect::<Option<Vec<_>>>()?;
    let mut origins = header.into_iter().flat_map(|header| &header.parameters).zip(&parameters)
        .map(|(span, param)| (span.col, param.name.clone()))
        .collect::<std::collections::HashMap<_, _>>();
    let mut constructor_parameters = Vec::new();
    let mut constructor_header = None;
    if let Some(constructor) = constructor {
        let (open, after) = constructor.header_range;
        let header = source.syntax.headers.iter().find(|header| token_index(header.open) == Some(open))?;
        constructor_parameters = aliased_constructor_parameters(&constructor.parameters);
        origins.extend(header.parameters.iter().zip(&constructor_parameters)
            .map(|(span, param)| (span.col, param.name.clone())));
        constructor_header = Some(open..after);
    }
    let mut uses = source.syntax.references.iter().filter_map(|usage| {
        let index = token_index(usage.usage)?;
        let name = origins.get(&usage.parameter.col)?;
        Some((index, name.as_str()))
    }).collect::<std::collections::HashMap<_, _>>();
    let mut substitutions = Vec::new();
    let mut tokens = Vec::new();
    for index in (start..end).filter(|index| {
        !(header_start..header_end).contains(index)
            && !constructor_header.as_ref().is_some_and(|header| header.contains(index))
    }) {
        if let Some(parameter) = uses.remove(&index) {
            substitutions.push((tokens.len(), parameter.to_owned()));
        }
        tokens.push(Token::new(source.tokens.get(index)?.clone(), *source.lines.get(index)?)
            .with_phpdoc(source.phpdoc.get(index).copied().unwrap_or(false)));
    }
    Some(EvalGenericTemplate {
        name,
        parameters,
        prototype: Box::new(prototype),
        tokens,
        substitutions,
        header_range: (header_start, header_end),
        constructor_parameters,
        namespace: scope.namespace.clone(),
        imports: scope.imports.clone(),
        name_index: name_index.checked_sub(start)?,
        native_syntax,
        strict_types: scope.strict_types,
    })
}

/// Returns the constructor's generic template when specializing a class declaration.
fn constructor_template(declaration: &EvalStmt) -> Option<&EvalGenericTemplate> {
    let EvalStmt::ClassDecl(class) = declaration else { return None };
    class.method("__construct")?.generic_template()
}

/// Keeps constructor slots distinct from slots declared on the enclosing class.
pub(crate) fn aliased_constructor_parameters(parameters: &[Parameter]) -> Vec<Parameter> {
    let aliases = parameters.iter()
        .map(|param| (param.name.clone(), Type::atom(format!("@ctor_{}", param.name))))
        .collect();
    parameters.iter().map(|param| Parameter {
        name: format!("@ctor_{}", param.name),
        bound: param.bound.as_ref().map(|ty| ty.substitute(&aliases)),
        default: param.default.as_ref().map(|ty| ty.substitute(&aliases)),
        variance: param.variance,
    }).collect()
}

/// Converts type declaration syntax into the type algebra used by specialization.
pub(super) fn lower_structural_type(scope: &Scope<'_>, ty: TypeExpr) -> Option<Type> {
    Some(match ty {
        TypeExpr::Void => Type::atom("void"),
        TypeExpr::Never => Type::atom("never"),
        TypeExpr::Array(value) => Type::Apply("array".to_owned(), vec![lower_structural_type(scope, *value)?]),
        TypeExpr::AssocArray { key, value } => {
            let key = lower_structural_type(scope, *key)?;
            if !matches!(&key, Type::Atom(name) if ["int", "string", "mixed"].contains(&name.as_str())
                || scope.has_type_parameter(name))
            {
                return None;
            }
            Type::Apply("array".to_owned(), vec![key, lower_structural_type(scope, *value)?])
        }
        TypeExpr::GenericClass { name, args } => Type::Apply(scope.class_name(&name), lower_types(scope, args)?),
        TypeExpr::CallableSig { params, ret } => Type::Callable(lower_types(scope, params)?, Box::new(lower_structural_type(scope, *ret)?)),
        ty => {
            let ty = lower_type_in_scope(scope, Some(ty), RelativeTypes::LateStatic)??;
            if ty.variants().is_empty() && ty.allows_null() {
                Type::atom("null")
            } else {
                generic_type(&ty)
            }
        }
    })
}

/// Converts a complete list of generic arguments or compound type members.
pub(super) fn lower_types(scope: &Scope<'_>, types: Vec<TypeExpr>) -> Option<Vec<Type>> {
    types.into_iter().map(|ty| lower_structural_type(scope, ty)).collect()
}

/// Decodes the runtime adapter's one-based token ordinal.
fn token_index(span: Span) -> Option<usize> {
    usize::try_from(span.col).ok()?.checked_sub(1)
}
