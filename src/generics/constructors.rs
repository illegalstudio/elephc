//! Purpose:
//! Specializes constructor-local templates through concrete class instantiations.
//! Keeps promoted property storage consistent with each construction's parameter types.
//!
//! Called from:
//! - `crate::generics::monomorphize()` before checking newly instantiated classes.
//!
//! Key details:
//! - A constructor must retain its reserved name because `new` calls `__construct`.
//! - Class template arguments bind first; constructor arguments specialize the resulting class.

use crate::errors::CompileError;
use crate::parser::ast::{ClassMethod, GenericDecl, Program, Stmt, StmtKind, TypeExpr};

use super::classes::{self, InferredConstructions, Instantiated, Templates};

/// Gives ordinary classes constructor parameters as their specialization parameters.
pub(super) fn lift(mut program: Program) -> Program {
    lift_in(&mut program);
    program
}

/// Lifts constructors only after enclosing class parameters have become concrete.
fn lift_in(program: &mut [Stmt]) -> bool {
    let mut lifted = false;
    for stmt in program {
        match &mut stmt.kind {
            StmtKind::ClassDecl { generics, properties, methods, .. } => {
                let Some(constructor) = methods.iter_mut().find(|method| {
                    method.name.eq_ignore_ascii_case("__construct") && !method.type_params.is_empty()
                }) else {
                    continue;
                };
                preserve_constructor_scope(constructor);
                // Promotion creates a separate property AST node outside the method's scope.
                for property in properties.iter_mut().filter(|property| property.is_promoted) {
                    if let Some((_, ty, _, _)) = constructor.params.iter().find(|param| param.0 == property.name) {
                        property.type_expr = ty.clone();
                    }
                }
                if generics.as_ref().is_some_and(|decl| !decl.type_params.is_empty()) {
                    continue;
                }
                let (extends_args, interface_args) = generics.take()
                    .map(|decl| (decl.extends_args, decl.interface_args))
                    .unwrap_or_default();
                *generics = GenericDecl::new(
                    std::mem::take(&mut constructor.type_params), extends_args, interface_args,
                );
                lifted = true;
            }
            StmtKind::NamespaceBlock { body, .. } | StmtKind::Synthetic(body) => {
                lifted |= lift_in(body);
            }
            _ => {}
        }
    }
    lifted
}

/// Keeps constructor parameters separate from class names and every sibling member's types.
fn preserve_constructor_scope(constructor: &mut ClassMethod) {
    let mut bindings = Vec::new();
    for parameter in &constructor.type_params {
        if !parameter.name.starts_with("@constructor:") {
            // This spelling cannot occur in a source type name or template declaration.
            let fresh = format!("@constructor:{}", parameter.name);
            bindings.push((parameter.name.clone(), TypeExpr::Named(crate::names::Name::unqualified(&fresh))));
        }
    }
    if bindings.is_empty() {
        return;
    }
    for parameter in &mut constructor.type_params {
        if let Some((_, TypeExpr::Named(fresh))) = bindings.iter().find(|(name, _)| name == &parameter.name) {
            parameter.name = fresh.as_str().to_string();
        }
        parameter.variance = crate::parser::ast::Variance::Invariant;
        for ty in [&mut parameter.bound, &mut parameter.default].into_iter().flatten() {
            *ty = ty.substitute_type_params(&bindings);
        }
    }
    for ty in constructor.params.iter_mut().filter_map(|param| param.1.as_mut())
        .chain(constructor.variadic_type.as_mut()).chain(constructor.return_type.as_mut())
    {
        *ty = ty.substitute_type_params(&bindings);
    }
    constructor.body = super::substitute_in_body(std::mem::take(&mut constructor.body), &bindings);
}

/// Exposes the source spelling of an internally scoped constructor parameter in diagnostics.
pub(crate) fn parameter_name(name: &str) -> &str {
    name.strip_prefix("@constructor:").unwrap_or(name)
}

/// Retains constructor templates exposed by class instantiation before metadata checking.
pub(super) fn instantiate(
    program: Program,
    templates: &mut Templates,
    inferred: &InferredConstructions,
) -> Result<Instantiated, CompileError> {
    let mut result = classes::instantiate(program, templates, inferred)?;
    for _ in 0..super::MAX_INSTANTIATION_ROUNDS {
        if !lift_in(&mut result.program) {
            return Ok(result);
        }
        templates.extend(classes::collect(&result.program));
        let next = classes::instantiate(result.program, templates, &Default::default())?;
        result.program = next.program;
        result.obligations.extend(next.obligations);
        result.warnings.extend(next.warnings);
    }
    Err(CompileError::new(
        crate::span::Span::dummy(), "Generic constructor instantiation did not settle",
    ))
}

#[cfg(test)]
mod tests {
    /// A constructor parameter never replaces a same-named class in a sibling signature.
    #[test]
    fn constructor_parameters_do_not_capture_sibling_class_names() {
        let source = "<?php class T { public int $value = 8; } class Box { public function __construct<T>(T $value) {} public function read(T $value): int { return $value->value; } } $box = new Box(7); echo $box->read(new T());";
        let tokens = crate::lexer::tokenize(source).expect("tokens");
        let program = crate::parser::parse(&tokens).expect("program");
        let program = crate::name_resolver::resolve(program).expect("resolved names");
        let result = crate::generics::monomorphize(program, |program, context| {
            crate::types::check_with_options_and_bounds(program, Default::default(), context)
        });
        assert!(result.is_ok(), "{}", result.unwrap_err().message);
    }

    /// Constructor templates must be instantiated before their declarations are stripped.
    #[test]
    fn constructor_templates_are_not_erased() {
        let source = "<?php class Box { public function __construct<T>(T $value) {} } new Box(7);";
        let tokens = crate::lexer::tokenize(source).expect("tokens");
        let program = crate::parser::parse(&tokens).expect("program");
        let program = crate::name_resolver::resolve(program).expect("resolved names");
        let result = crate::generics::monomorphize(program, |program, context| {
            crate::types::check_with_options_and_bounds(program, Default::default(), context)
        });
        assert!(result.is_ok(), "{}", result.unwrap_err().message);
    }
}
