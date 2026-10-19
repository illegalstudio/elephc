//! Purpose:
//! Injects the elephc-PHP helper that verifies a value against a DECLARED scalar return type
//! (`bool`, `int`, `float`, `string`, their nullable forms and scalar unions such as
//! `int|string` or `string|false`) with PHP's coercive and `strict_types` rules, and owns the
//! type-mask encoding shared by `ir_lower`, reachability and the exception-flow analysis.
//!
//! Called from:
//! - `crate::pipeline::compile()` and the test harnesses via `inject`, next to the object-cast
//!   prelude (after `autoload::run` and name resolution).
//! - `crate::ir_lower::stmt::return_coercions`, which lowers a `return` whose static type does
//!   not prove the declared contract to a call of [`HELPER`].
//! - `crate::optimize::reachability::usage`, which keeps the helper reachable from every
//!   declaration whose return annotation is scalar.
//! - `crate::optimize::exception_flow`, which records that a call to such a declaration can
//!   throw `TypeError`, so dead-code elimination keeps an unused call and its `catch`.
//!
//! Key details:
//! - The checker still rejects, by design, a return whose static type is another scalar
//!   (`: string` returning an int literal, `: int` from `int|false`); the helper covers what it
//!   admits but cannot prove: `mixed`, heterogeneous ternary/match results, scalar unions.
//! - A prelude rather than a runtime helper: the rules (exact member, int-to-float widening,
//!   int > float > string > bool preference, numeric strings, `TypeError` naming the runtime
//!   value) are ordinary PHP over `is_*` checks and casts, so every supported target gets them
//!   with no per-target assembly — the same reason `object_cast_prelude` is a prelude.
//! - Always injected (three small declarations); declaration reachability prunes it from any
//!   program that has no reachable scalar return annotation, so unrelated binaries carry
//!   nothing.
//! - The `TypeError` message is php-src's: `f(): Return value must be of type T, V returned`,
//!   where `T` is the declaration in php-src's canonical order (`string|int`, `?int`) and `V`
//!   names the value (`true`/`false` for booleans, the class for objects).

use crate::errors::CompileError;
use crate::opcache_prelude::detect;
use crate::parser::ast::{Program, TypeExpr};
use crate::types::PhpType;

/// The helper `ir_lower` calls for a return value that needs a runtime verification.
pub(crate) const HELPER: &str = "__elephc_return_coerce";

/// Every declaration the prelude adds, for the reserved-name check.
const DECLARED: [&str; 3] = [
    HELPER,
    "__elephc_return_float_fits_int",
    "__elephc_return_value_name",
];

/// Mask bit for `int`.
pub(crate) const MASK_INT: u32 = 1;
/// Mask bit for `float`.
pub(crate) const MASK_FLOAT: u32 = 2;
/// Mask bit for `string`.
pub(crate) const MASK_STRING: u32 = 4;
/// Mask bit for `bool` (both values).
pub(crate) const MASK_BOOL: u32 = 8;
/// Mask bit for the literal `false` type.
pub(crate) const MASK_FALSE: u32 = 16;
/// Mask bit for the literal `true` type.
pub(crate) const MASK_TRUE: u32 = 32;
/// Mask bit for `null`.
pub(crate) const MASK_NULL: u32 = 64;

/// Prepends the return-coercion helper declarations when some function, method or closure has a
/// scalar return annotation; otherwise returns the program unchanged.
///
/// The declarations are hoisted functions only, so prepending cannot change top-level execution
/// order. A program that declares one of the reserved names is rejected rather than having its
/// own definition silently become the return-coercion semantics.
pub fn inject(
    program: Program,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Result<Program, CompileError> {
    crate::compiler_stack::with_compiler_stack(|| inject_on_compiler_stack(program, inventory))
}

/// The body of [`inject`], run on the stack `with_compiler_stack` sized.
fn inject_on_compiler_stack(
    program: Program,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Result<Program, CompileError> {
    let generic = program_declares_generic_template(&program);
    let (program, scalar_return) = program_has_scalar_return_annotation(program);
    if !scalar_return && !generic {
        return Ok(program);
    }
    for name in DECLARED {
        if let Some(span) = detect::first_declaration(&program, name) {
            return Err(CompileError::new(
                span,
                &format!(
                    "Cannot declare {}(): the name is reserved for the compiler's return-type \
                     coercion helper. Rename the function.",
                    name
                ),
            ));
        }
    }
    let tokens = crate::lexer::tokenize(include_str!("return_coerce_prelude/return_coerce.php"))
        .unwrap_or_else(|e| panic!("return-coercion prelude must tokenize: {e:?}"));
    let declarations = crate::parser::parse_internal(&tokens)
        .unwrap_or_else(|e| panic!("return-coercion prelude must parse: {e:?}"));
    let mut combined = crate::name_resolver::resolve(declarations)?;
    inventory.record_program("return_coerce", &combined);
    combined.extend(program);
    Ok(combined)
}

/// Finds a scalar return annotation anywhere in the program, closures included.
///
/// Runs the exhaustive magic-constant walker with a pass that only looks at return annotations
/// (`Pass::transform_return_type`), so no declaration position can be missed; the walk rebuilds
/// the program unchanged.
fn program_has_scalar_return_annotation(program: Program) -> (Program, bool) {
    struct Detector {
        found: std::cell::Cell<bool>,
    }
    impl crate::magic_constants::walker::Pass for Detector {
        fn transform_magic(
            &self,
            _span: crate::span::Span,
            mc: crate::parser::ast::MagicConstant,
        ) -> crate::parser::ast::ExprKind {
            crate::parser::ast::ExprKind::MagicConstant(mc)
        }

        fn transform_return_type(&self, ty: TypeExpr, _span: crate::span::Span) -> TypeExpr {
            if annotation_may_need_coercion(&ty) {
                self.found.set(true);
            }
            ty
        }
    }
    let mut detector = Detector {
        found: std::cell::Cell::new(false),
    };
    let program = crate::magic_constants::walker::walk_program(program, &mut detector);
    (program, detector.found.get())
}

/// Returns whether the program declares a generic template (`function f<T>`, `class Box<T>`, a
/// generic method, or their `@template` docblock forms).
///
/// A template's `: T` return annotation names no scalar, but monomorphization turns it into
/// `: int` or `: string` after this prelude has run, so any template keeps the helper in.
fn program_declares_generic_template(stmts: &[crate::parser::ast::Stmt]) -> bool {
    use crate::parser::ast::StmtKind;
    let generic_methods = |methods: &[crate::parser::ast::ClassMethod]| {
        methods.iter().any(|method| !method.type_params.is_empty())
    };
    let generic_decl = |generics: &Option<Box<crate::parser::ast::GenericDecl>>| {
        generics.as_ref().is_some_and(|decl| !decl.type_params.is_empty())
    };
    stmts.iter().any(|stmt| match &stmt.kind {
        StmtKind::FunctionDecl { type_params, .. } => !type_params.is_empty(),
        StmtKind::ClassDecl { generics, methods, .. }
        | StmtKind::InterfaceDecl { generics, methods, .. }
        | StmtKind::TraitDecl { generics, methods, .. } => {
            generic_decl(generics) || generic_methods(methods)
        }
        StmtKind::NamespaceBlock { body, .. }
        | StmtKind::Synthetic(body)
        | StmtKind::IncludeOnceGuard { body, .. } => program_declares_generic_template(body),
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            program_declares_generic_template(then_body)
                || elseif_clauses
                    .iter()
                    .any(|(_, body)| program_declares_generic_template(body))
                || else_body
                    .as_deref()
                    .is_some_and(program_declares_generic_template)
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            program_declares_generic_template(then_body)
                || else_body
                    .as_deref()
                    .is_some_and(program_declares_generic_template)
        }
        _ => false,
    })
}

/// Returns whether a written return annotation names a scalar the helper may have to verify.
///
/// Used by declaration reachability, which runs on the AST: a declaration whose annotation
/// passes keeps the helper alive. Over-approximates freely (`int|array` passes), since a kept
/// helper only costs its own code.
pub(crate) fn annotation_may_need_coercion(ty: &TypeExpr) -> bool {
    match ty {
        TypeExpr::Int
        | TypeExpr::Float
        | TypeExpr::Bool
        | TypeExpr::False
        | TypeExpr::True
        | TypeExpr::Str => {
            true
        }
        TypeExpr::Nullable(inner) => annotation_may_need_coercion(inner),
        TypeExpr::Union(members) => members.iter().any(annotation_may_need_coercion),
        _ => false,
    }
}

/// Returns whether a return annotation names the literal `true` type without a full `bool`.
pub(crate) fn annotation_is_literal_true(ty: Option<&TypeExpr>) -> bool {
    fn names(ty: &TypeExpr, wanted: &dyn Fn(&TypeExpr) -> bool) -> bool {
        match ty {
            TypeExpr::Nullable(inner) => names(inner, wanted),
            TypeExpr::Union(members) => members.iter().any(|member| names(member, wanted)),
            other => wanted(other),
        }
    }
    ty.is_some_and(|ty| {
        names(ty, &|t| matches!(t, TypeExpr::True))
            && !names(ty, &|t| matches!(t, TypeExpr::Bool | TypeExpr::False))
    })
}

/// Replaces the `bool` bit of a mask by the literal `true` bit, for a `: true` declaration.
pub(crate) fn literal_true_mask(mask: u32) -> u32 {
    if mask & MASK_BOOL != 0 {
        (mask & !MASK_BOOL) | MASK_TRUE
    } else {
        mask
    }
}

/// Encodes a declared return type as the helper's type mask, or `None` when the declaration is
/// not purely scalar (`mixed`, arrays, objects, `void`, ...), which this helper never verifies.
pub(crate) fn scalar_mask(declared: &PhpType) -> Option<u32> {
    match declared {
        PhpType::Int => Some(MASK_INT),
        PhpType::Float => Some(MASK_FLOAT),
        PhpType::Str => Some(MASK_STRING),
        PhpType::Bool => Some(MASK_BOOL),
        PhpType::False => Some(MASK_FALSE),
        PhpType::Union(members) => {
            let mut mask = 0;
            for member in members {
                mask |= match member {
                    PhpType::Void => MASK_NULL,
                    other => scalar_mask(other)?,
                };
            }
            (mask & !MASK_NULL != 0).then_some(mask)
        }
        _ => None,
    }
}

/// Returns whether a value of static type `actual` always satisfies `mask` EXACTLY, so the
/// return needs no verification at all (only, possibly, a change of storage).
pub(crate) fn statically_satisfies(actual: &PhpType, mask: u32) -> bool {
    match actual {
        PhpType::Int => mask & MASK_INT != 0,
        PhpType::Float => mask & MASK_FLOAT != 0,
        PhpType::Str => mask & MASK_STRING != 0,
        PhpType::Bool => mask & MASK_BOOL != 0,
        PhpType::False => mask & (MASK_BOOL | MASK_FALSE) != 0,
        // A `bool` value proves neither literal member: `: true` must check the value.
        PhpType::Void => mask & MASK_NULL != 0,
        PhpType::Never => true,
        PhpType::TaggedScalar => mask & (MASK_INT | MASK_NULL) == MASK_INT | MASK_NULL,
        PhpType::Union(members) => members.iter().all(|member| statically_satisfies(member, mask)),
        _ => false,
    }
}

/// Renders a scalar mask the way php-src's `zend_type_to_string()` spells the declaration:
/// `string`, `int`, `float`, then `bool`/`false`, with `null` as a `?` prefix on a single type
/// and a trailing `|null` otherwise.
pub(crate) fn declared_type_name(mask: u32) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if mask & MASK_STRING != 0 {
        parts.push("string");
    }
    if mask & MASK_INT != 0 {
        parts.push("int");
    }
    if mask & MASK_FLOAT != 0 {
        parts.push("float");
    }
    if mask & MASK_BOOL != 0 {
        parts.push("bool");
    } else if mask & MASK_FALSE != 0 {
        parts.push("false");
    } else if mask & MASK_TRUE != 0 {
        parts.push("true");
    }
    let joined = parts.join("|");
    if mask & MASK_NULL == 0 {
        joined
    } else if parts.len() == 1 {
        format!("?{}", joined)
    } else {
        format!("{}|null", joined)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses a fixture the way the pipeline does.
    fn parsed(source: &str) -> Program {
        let tokens = crate::lexer::tokenize(source).expect("test source must tokenize");
        crate::parser::parse(&tokens).expect("test source must parse")
    }

    /// The embedded source tokenizes, parses and declares every reserved name once a scalar
    /// return annotation asks for it, including one on a closure.
    #[test]
    fn the_prelude_declares_its_helpers() {
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let program = inject(parsed("<?php $f = fn($v): ?int => $v;"), &mut inventory)
            .expect("prelude must inject");
        for name in DECLARED {
            assert!(detect::first_declaration(&program, name).is_some(), "{} missing", name);
        }
    }

    /// Programs without a scalar return annotation (parameter hints do not count) get nothing.
    #[test]
    fn programs_without_scalar_returns_carry_nothing() {
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let program = inject(
            parsed("<?php function f(int $v): void {} function g(): array { return []; }"),
            &mut inventory,
        )
        .expect("prelude must inject");
        assert!(detect::first_declaration(&program, HELPER).is_none());
    }

    /// Declared types render in php-src's canonical order.
    #[test]
    fn declared_type_names_match_php() {
        let union = |members: Vec<PhpType>| PhpType::Union(members);
        let name = |ty: PhpType| declared_type_name(scalar_mask(&ty).unwrap());
        assert_eq!(name(PhpType::Bool), "bool");
        assert_eq!(name(union(vec![PhpType::Int, PhpType::Str])), "string|int");
        assert_eq!(name(union(vec![PhpType::Int, PhpType::Void])), "?int");
        assert_eq!(name(union(vec![PhpType::Int, PhpType::Str, PhpType::Void])), "string|int|null");
        assert_eq!(name(union(vec![PhpType::Str, PhpType::False])), "string|false");
        assert_eq!(name(union(vec![PhpType::Float, PhpType::Int])), "int|float");
    }

    /// Non-scalar declarations are never verified by the helper.
    #[test]
    fn non_scalar_declarations_have_no_mask() {
        assert_eq!(scalar_mask(&PhpType::Mixed), None);
        assert_eq!(scalar_mask(&PhpType::Object("C".to_string())), None);
        assert_eq!(
            scalar_mask(&PhpType::Union(vec![PhpType::Int, PhpType::Object("C".to_string())])),
            None
        );
        assert_eq!(scalar_mask(&PhpType::Union(vec![PhpType::Void])), None);
    }
}
