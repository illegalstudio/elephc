//! Purpose:
//! Resolves names in the Chopper syntax AST using eval's lexical namespace and imports.
//!
//! Called from:
//! - Magician's shared AST-to-EvalIR lowering.
//!
//! Key details:
//! - Function and constant aliases are separate from class aliases.
//! - Only unqualified functions and constants can fall back to the global namespace.

use std::collections::HashMap;

use elephc_chopper::syntax::{
    names::{Name, NameKind},
    parser::ast::{Stmt, StmtKind, UseItem, UseKind},
};

use crate::{eval_ir::EvalStmt, parser::state::NamespaceImports};

/// Bindings active in one file or namespace region.
#[derive(Clone)]
pub(super) struct Scope<'ast> {
    pub namespace: String,
    pub imports: NamespaceImports,
    pub anonymous_classes: &'ast HashMap<String, Stmt>,
    pub strict_types: bool,
    pub source: &'ast super::templates::TemplateSource<'ast>,
    type_parameters: Vec<String>,
}

impl<'ast> Scope<'ast> {
    /// Starts a namespace region while retaining its fragment source and metadata.
    fn in_namespace(&self, name: Option<Name>) -> Self {
        Self {
            namespace: name.map(|name| name.as_str().to_owned()).unwrap_or_default(),
            imports: NamespaceImports::default(),
            ..self.clone()
        }
    }

    /// Adds declaration-local type parameters to the lexical scope.
    pub fn with_type_parameters(&self, params: &[elephc_chopper::syntax::parser::ast::TypeParam]) -> Self {
        let mut scope = self.clone();
        scope.type_parameters.extend(params.iter().map(|param| param.name.clone()));
        scope
    }

    /// Recognizes a type parameter whose spelling resolves in the current declaration.
    pub fn has_type_parameter(&self, name: &str) -> bool {
        self.type_parameters.iter().any(|param| param == name)
    }

    /// Qualifies one declaration or relative spelling in the current namespace.
    pub fn qualify(&self, name: &str) -> String {
        if self.namespace.is_empty() {
            name.to_owned()
        } else {
            format!("{}\\{name}", self.namespace)
        }
    }

    /// Resolves a class reference, including an alias used as its prefix.
    pub fn class_name(&self, name: &Name) -> String {
        if name.is_fully_qualified()
            || name.is_unqualified() && self.has_type_parameter(name.as_str())
        {
            return name.as_str().to_owned();
        }
        self.imports
            .resolve_class(name.as_str())
            .unwrap_or_else(|| self.qualify(name.as_str()))
    }

    /// Returns the name and optional global fallback for a function or callable.
    pub fn function_name(&self, name: &Name) -> (String, Option<String>) {
        self.resolve(name, |spelling| self.imports.resolve_function(spelling))
    }

    /// Returns the name and optional global fallback for a constant fetch.
    pub fn constant_name(&self, name: &Name) -> (String, Option<String>) {
        self.resolve(name, |spelling| self.imports.resolve_constant(spelling))
    }

    /// Resolves one written name using the caller's alias table for unqualified names.
    fn resolve<'a>(
        &self,
        name: &Name,
        import: impl FnOnce(&str) -> Option<&'a str>,
    ) -> (String, Option<String>) {
        let spelling = name.as_str();
        match name.kind {
            NameKind::FullyQualified => (spelling.to_owned(), None),
            NameKind::Qualified => (self.qualify(spelling), None),
            NameKind::Unqualified => {
                if let Some(imported) = import(spelling) {
                    (imported.to_owned(), None)
                } else {
                    let fallback = (!self.namespace.is_empty()).then(|| spelling.to_owned());
                    (self.qualify(spelling), fallback)
                }
            }
        }
    }

    /// Adds an alias to the declaration region.
    fn register(&mut self, item: UseItem) {
        let name = item.name.as_str().to_owned();
        match item.kind {
            UseKind::Class => self.imports.insert_class(item.alias, name),
            UseKind::Function => self.imports.insert_function(item.alias, name),
            UseKind::Const => self.imports.insert_constant(item.alias, name),
        }
    }
}

/// Separates Chopper's hoisted anonymous classes so lowering can place them at construction sites.
pub(super) fn lower_program(
    ast: Vec<Stmt>,
    strict_types: bool,
    namespace: &str,
    imports: &NamespaceImports,
    source: &super::templates::TemplateSource<'_>,
) -> Option<Vec<EvalStmt>> {
    let mut anonymous_classes = HashMap::new();
    let mut statements = Vec::new();
    for stmt in ast {
        if let StmtKind::ClassDecl { name, .. } = &stmt.kind {
            if elephc_chopper::syntax::parser::is_anonymous_class_name(name) {
                anonymous_classes.insert(name.clone(), stmt);
                continue;
            }
        }
        statements.push(stmt);
    }
    let scope = Scope {
        namespace: namespace.to_owned(),
        imports: imports.clone(),
        anonymous_classes: &anonymous_classes,
        strict_types,
        source,
        type_parameters: Vec::new(),
    };
    lower_namespace_items(statements, scope)
}

/// Resolves a standalone runtime type in the global namespace.
pub(super) fn lower_type(ty: elephc_chopper::syntax::parser::ast::TypeExpr) -> Option<elephc_generics::Type> {
    let source = super::templates::TemplateSource {
        tokens: &[], lines: &[], phpdoc: &[], syntax: &Default::default(), uses_native_generics: false.into(),
        invalid_jump: false.into(),
    };
    let scope = Scope {
        namespace: String::new(),
        imports: NamespaceImports::default(),
        anonymous_classes: &HashMap::new(),
        strict_types: false,
        source: &source,
        type_parameters: Vec::new(),
    };
    super::templates::lower_structural_type(&scope, ty)
}

/// Lowers a file or namespace region, applying imports in written order.
fn lower_namespace_items(ast: Vec<Stmt>, mut scope: Scope<'_>) -> Option<Vec<EvalStmt>> {
    let mut statements = Vec::new();
    for stmt in ast {
        match stmt.kind {
            StmtKind::NamespaceDecl { name } => scope = scope.in_namespace(name),
            StmtKind::NamespaceBlock { name, body } => {
                statements.extend(lower_namespace_items(body, scope.in_namespace(name))?);
            }
            StmtKind::UseDecl { imports } => {
                for item in imports {
                    scope.register(item);
                }
            }
            _ => super::lower_statements(&scope, std::iter::once(stmt), &mut statements)?,
        }
    }
    Some(statements)
}
