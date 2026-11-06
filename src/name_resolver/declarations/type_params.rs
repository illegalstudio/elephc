//! Purpose:
//! Restores generic parameter names after ordinary namespace and import resolution.
//! Distinguishes declaration scope from method-local template scope.
//!
//! Called from:
//! - `crate::name_resolver::declarations::resolve_decl_stmt()` for generic declarations.
//!
//! Key details:
//! - Bounds and defaults retain resolved class names; parameter references become bare names.
//! - Method-local parameters apply only while walking that method's signature and body.
//! - The shared AST walker reaches every type position, including nested closures.

use crate::generics::Bindings;
use crate::magic_constants::walker::{walk_program, Pass};
use crate::names::Name;
use crate::parser::ast::{ExprKind, MagicConstant, Stmt, StmtKind, TypeExpr, TypeParam};
use crate::span::Span;

use super::{resolve_type_expr, Imports, Symbols};

/// Restores declaration and method parameter references without qualifying them as classes.
pub(super) fn restore(
    stmt: Stmt,
    type_params: &[TypeParam],
    namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> Stmt {
    let declaration = bindings(type_params, namespace, imports, symbols);
    let has_method_templates = match &stmt.kind {
        StmtKind::ClassDecl { methods, .. }
        | StmtKind::InterfaceDecl { methods, .. }
        | StmtKind::TraitDecl { methods, .. }
        | StmtKind::EnumDecl { methods, .. } => {
            methods.iter().any(|method| !method.type_params.is_empty())
        }
        _ => false,
    };
    if declaration.is_empty() && !has_method_templates {
        return stmt;
    }
    let mut pass = Restore {
        declaration,
        methods: Vec::new(),
        namespace,
        imports,
        symbols,
    };
    walk_program(vec![stmt], &mut pass)
        .pop()
        .expect("one declaration")
}

/// Builds the reverse mapping from resolved class spelling to a declared type parameter name.
fn bindings(
    type_params: &[TypeParam],
    namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> Bindings {
    type_params
        .iter()
        .filter_map(|param| {
            let bare = TypeExpr::Named(Name::unqualified(&param.name));
            let resolved = resolve_type_expr(&bare, namespace, imports, symbols);
            let TypeExpr::Named(resolved) = resolved else {
                return None;
            };
            (resolved.as_str() != param.name).then(|| (resolved.as_str().to_string(), bare))
        })
        .collect()
}

/// Tracks the generic name mappings currently visible to the shared type walker.
struct Restore<'a> {
    declaration: Bindings,
    methods: Vec<Bindings>,
    namespace: Option<&'a str>,
    imports: &'a Imports,
    symbols: &'a Symbols,
}

impl Pass for Restore<'_> {
    /// Leaves magic constants alone during the type-name restoration pass.
    fn transform_magic(&self, _span: Span, mc: MagicConstant) -> ExprKind {
        ExprKind::MagicConstant(mc)
    }

    /// Restores names from the enclosing declaration and current method template.
    fn transform_type(&self, ty: TypeExpr, _span: Span) -> TypeExpr {
        let mut ty = ty.substitute_type_params(&self.declaration);
        for method in &self.methods {
            ty = ty.substitute_type_params(method);
        }
        ty
    }

    /// Opens a method-local mapping without exposing its parameters to sibling members.
    fn enter_method(&mut self, _name: &str, type_params: &[TypeParam]) {
        self.methods.push(bindings(
            type_params, self.namespace, self.imports, self.symbols,
        ));
    }

    /// Restores the enclosing declaration's scope after completing a method.
    fn leave_method(&mut self) {
        self.methods.pop();
    }
}
