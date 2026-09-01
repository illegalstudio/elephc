//! Purpose:
//! Builds the declaration index consumed by whole-program reachability.
//! Keeps declaration discovery separate from fixed-point graph traversal.
//!
//! Called from:
//! - `crate::optimize::reachability::graph::compute()` and reconciliation setup.
//!
//! Key details:
//! - Declarations inside grouping statements are indexed, while callable bodies remain deferred.
//! - Class shells and checker-flattened method bodies receive independent usage summaries.

use crate::names::php_symbol_key;
use crate::parser::ast::{ClassMethod, Stmt, StmtKind, TraitUse};
use crate::types::CheckResult;

use super::{ClassKind, ClassNode, DeclarationIndex};
use crate::optimize::reachability::usage::{self, CallSignatureIndex};

impl DeclarationIndex {
    /// Builds a declaration index matching the recursive statement shapes lowered by EIR.
    pub(crate) fn build(program: &[Stmt], check_result: &CheckResult) -> Self {
        let call_signatures = CallSignatureIndex::from_check_result(check_result);
        Self::build_with_signatures(program, check_result, &call_signatures)
    }

    /// Builds an index while reusing the call signatures shared with executable-root scanning.
    pub(super) fn build_with_signatures(
        program: &[Stmt],
        check_result: &CheckResult,
        call_signatures: &CallSignatureIndex,
    ) -> Self {
        let mut index = Self::default();
        index.index_statements(program, call_signatures);
        index.index_flattened_methods(check_result, call_signatures);
        index
    }

    /// Replaces source-class method indexes with the flattened bodies consumed by EIR lowering.
    fn index_flattened_methods(
        &mut self,
        check_result: &CheckResult,
        call_signatures: &CallSignatureIndex,
    ) {
        for (class_name, info) in &check_result.classes {
            let class_key = php_symbol_key(class_name);
            let parent = info.parent.as_deref().map(php_symbol_key);
            let mut methods: std::collections::HashMap<_, _> = info
                .method_decls
                .iter()
                .map(|method| {
                    (
                        (php_symbol_key(&method.name), method.is_static),
                        usage::scan_method(
                            method,
                            class_name,
                            parent.as_deref(),
                            call_signatures,
                        ),
                    )
                })
                .collect();
            let has_runtime_owned_parent = info
                .parent
                .as_deref()
                .map(php_symbol_key)
                .is_some_and(|parent| !self.classes.contains_key(&parent));
            for method in info.methods.keys() {
                let implemented_here = info
                    .method_impl_classes
                    .get(method)
                    .is_some_and(|owner| php_symbol_key(owner) == class_key);
                if has_runtime_owned_parent || implemented_here {
                    methods
                        .entry((php_symbol_key(method), false))
                        .or_default();
                }
            }
            for method in info.static_methods.keys() {
                let implemented_here = info
                    .static_method_impl_classes
                    .get(method)
                    .is_some_and(|owner| php_symbol_key(owner) == class_key);
                if has_runtime_owned_parent || implemented_here {
                    methods
                        .entry((php_symbol_key(method), true))
                        .or_default();
                }
            }
            if let Some(node) = self.classes.get_mut(&class_key) {
                let attribute_classes: Vec<_> = info
                    .attribute_names
                    .iter()
                    .chain(info.method_attribute_names.values().flatten())
                    .chain(info.property_attribute_names.values().flatten())
                    .chain(info.constant_attribute_names.values().flatten())
                    .map(|name| php_symbol_key(name))
                    .collect();
                node.usage
                    .classes
                    .extend(attribute_classes.iter().cloned());
                node.usage
                    .instantiated_classes
                    .extend(attribute_classes);
                node.interfaces
                    .extend(info.interfaces.iter().map(|name| php_symbol_key(name)));
                node.interfaces.sort_unstable();
                node.interfaces.dedup();
                node.traits
                    .extend(info.used_traits.iter().map(|name| php_symbol_key(name)));
                node.traits.sort_unstable();
                node.traits.dedup();
                node.methods = methods;
            } else {
                self.checker_methods.extend(methods.into_iter().map(
                    |((method, is_static), usage)| {
                        ((class_key.clone(), method, is_static), usage)
                    },
                ));
            }
        }
    }

    /// Indexes declarations nested in grouping/control-flow statements but not callable bodies.
    fn index_statements(
        &mut self,
        statements: &[Stmt],
        call_signatures: &CallSignatureIndex,
    ) {
        for statement in statements {
            match &statement.kind {
                StmtKind::FunctionDecl { name, .. } => {
                    self.functions
                        .insert(
                            php_symbol_key(name),
                            usage::scan_function(statement, call_signatures),
                        );
                }
                StmtKind::FunctionVariantGroup { name, variants } => {
                    self.function_variants.insert(
                        php_symbol_key(name),
                        variants.iter().map(|name| php_symbol_key(name)).collect(),
                    );
                }
                StmtKind::ClassDecl {
                    name,
                    extends,
                    implements,
                    trait_uses,
                    methods,
                    ..
                } => self.index_class(
                    statement,
                    name,
                    ClassKind::Class,
                    extends.as_ref().map(|name| php_symbol_key(name.as_str())),
                    implements
                        .iter()
                        .map(|name| php_symbol_key(name.as_str()))
                        .collect(),
                    trait_names(trait_uses),
                    methods,
                    call_signatures,
                ),
                StmtKind::EnumDecl {
                    name,
                    implements,
                    trait_uses,
                    methods,
                    ..
                } => self.index_class(
                    statement,
                    name,
                    ClassKind::Enum,
                    None,
                    implements
                        .iter()
                        .map(|name| php_symbol_key(name.as_str()))
                        .collect(),
                    trait_names(trait_uses),
                    methods,
                    call_signatures,
                ),
                StmtKind::InterfaceDecl {
                    name,
                    extends,
                    methods,
                    ..
                } => self.index_class(
                    statement,
                    name,
                    ClassKind::Interface,
                    None,
                    extends
                        .iter()
                        .map(|name| php_symbol_key(name.as_str()))
                        .collect(),
                    Vec::new(),
                    methods,
                    call_signatures,
                ),
                StmtKind::TraitDecl {
                    name,
                    trait_uses,
                    methods,
                    ..
                } => self.index_class(
                    statement,
                    name,
                    ClassKind::Trait,
                    None,
                    Vec::new(),
                    trait_names(trait_uses),
                    methods,
                    call_signatures,
                ),
                StmtKind::PackedClassDecl { name, .. } => {
                    self.packed_classes.insert(php_symbol_key(name));
                }
                StmtKind::ExternClassDecl { name, .. } => {
                    self.extern_classes.insert(php_symbol_key(name));
                }
                StmtKind::ExternFunctionDecl { name, .. } => {
                    self.externs.insert(php_symbol_key(name));
                }
                StmtKind::NamespaceBlock { body, .. }
                | StmtKind::Synthetic(body)
                | StmtKind::IncludeOnceGuard { body, .. } => {
                    self.index_statements(body, call_signatures)
                }
                StmtKind::If {
                    then_body,
                    elseif_clauses,
                    else_body,
                    ..
                } => {
                    self.index_statements(then_body, call_signatures);
                    for (_, body) in elseif_clauses {
                        self.index_statements(body, call_signatures);
                    }
                    if let Some(body) = else_body {
                        self.index_statements(body, call_signatures);
                    }
                }
                StmtKind::IfDef {
                    then_body,
                    else_body,
                    ..
                } => {
                    self.index_statements(then_body, call_signatures);
                    if let Some(body) = else_body {
                        self.index_statements(body, call_signatures);
                    }
                }
                StmtKind::While { body, .. }
                | StmtKind::DoWhile { body, .. }
                | StmtKind::For { body, .. }
                | StmtKind::Foreach { body, .. } => {
                    self.index_statements(body, call_signatures)
                }
                StmtKind::Switch { cases, default, .. } => {
                    for (_, body) in cases {
                        self.index_statements(body, call_signatures);
                    }
                    if let Some(body) = default {
                        self.index_statements(body, call_signatures);
                    }
                }
                StmtKind::Try {
                    try_body,
                    catches,
                    finally_body,
                } => {
                    self.index_statements(try_body, call_signatures);
                    for catch in catches {
                        self.index_statements(&catch.body, call_signatures);
                    }
                    if let Some(body) = finally_body {
                        self.index_statements(body, call_signatures);
                    }
                }
                StmtKind::Echo(_)
                | StmtKind::Assign { .. }
                | StmtKind::RefAssign { .. }
                | StmtKind::ArrayAssign { .. }
                | StmtKind::NestedArrayAssign { .. }
                | StmtKind::ArrayPush { .. }
                | StmtKind::TypedAssign { .. }
                | StmtKind::Include { .. }
                | StmtKind::IncludeOnceMark { .. }
                | StmtKind::Throw(_)
                | StmtKind::Break(_)
                | StmtKind::Continue(_)
                | StmtKind::ExprStmt(_)
                | StmtKind::NamespaceDecl { .. }
                | StmtKind::UseDecl { .. }
                | StmtKind::FunctionVariantMark { .. }
                | StmtKind::Return(_)
                | StmtKind::ConstDecl { .. }
                | StmtKind::ListUnpack { .. }
                | StmtKind::Global { .. }
                | StmtKind::StaticVar { .. }
                | StmtKind::PropertyAssign { .. }
                | StmtKind::DynamicPropertyArrayPush { .. }
                | StmtKind::StaticPropertyAssign { .. }
                | StmtKind::StaticPropertyArrayPush { .. }
                | StmtKind::StaticPropertyArrayAssign { .. }
                | StmtKind::PropertyArrayPush { .. }
                | StmtKind::PropertyArrayAssign { .. }
                | StmtKind::ExternGlobalDecl { .. } => {}
            }
        }
    }

    /// Indexes one class-like declaration and its methods independently.
    fn index_class(
        &mut self,
        statement: &Stmt,
        name: &str,
        kind: ClassKind,
        parent: Option<String>,
        interfaces: Vec<String>,
        traits: Vec<String>,
        methods: &[ClassMethod],
        call_signatures: &CallSignatureIndex,
    ) {
        let class_key = php_symbol_key(name);
        let methods = methods
            .iter()
            .map(|method| {
                (
                    (php_symbol_key(&method.name), method.is_static),
                    usage::scan_method(method, name, parent.as_deref(), call_signatures),
                )
            })
            .collect();
        self.classes.insert(
            class_key,
            ClassNode {
                kind,
                usage: usage::scan_class_shell(statement),
                methods,
                parent,
                interfaces,
                traits,
            },
        );
    }
}

/// Extracts canonical trait names from one class-like trait-use list.
fn trait_names(uses: &[TraitUse]) -> Vec<String> {
    uses.iter()
        .flat_map(|trait_use| trait_use.trait_names.iter())
        .map(|name| php_symbol_key(name.as_str()))
        .collect()
}
