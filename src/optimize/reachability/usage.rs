//! Purpose:
//! Scans PHP AST subtrees for declaration references and dynamic lookup hazards.
//! Supplies executable-root and per-declaration dependency and hazard summaries.
//!
//! Called from:
//! - `crate::optimize::reachability::graph` and the web prelude injector.
//!
//! Key details:
//! - Names use PHP case-insensitive keys after namespace resolution.
//! - Unknown runtime lookup widens hazards; it never guesses a declaration is unreachable.
//! - Local receiver facts union known classes and remain poisoned after any opaque write.
//! - Reachable global aliases and by-reference call arguments invalidate cross-scope facts.

use std::collections::{HashMap, HashSet};

use crate::names::{php_symbol_key, property_hook_get_method, property_hook_set_method};
use crate::parser::ast::{
    AttributeGroup, BinOp, ClassConst, ClassMethod, ClassProperty, Expr, ExprKind, Stmt, StmtKind,
    TraitUse, TypeExpr,
};
use crate::types::{CheckResult, FunctionSig, PhpType};

mod expressions;

/// References and hazards found in one AST subtree.
#[derive(Clone, Debug, Default)]
pub struct Usage {
    pub functions: HashSet<String>,
    pub classes: HashSet<String>,
    pub methods: HashSet<(String, String, bool)>,
    pub externs: HashSet<String>,
    pub hazards: Hazards,
    pub(crate) scoped_methods: HashSet<(String, String, bool)>,
    pub(crate) wildcard_methods: HashSet<(String, bool)>,
    pub(crate) instantiated_classes: HashSet<String>,
    pub(crate) instantiated_subclass_roots: HashSet<String>,
    pub(crate) required_libraries: HashSet<String>,
    pub(crate) global_aliases: HashSet<String>,
    pub(crate) dynamic_global_alias: bool,
    pub(crate) variable_methods: HashMap<String, HashSet<(String, bool)>>,
}

/// Runtime lookup shapes that require conservative keep-set widening.
#[derive(Clone, Copy, Debug, Default)]
pub struct Hazards {
    pub dynamic_function: bool,
    pub dynamic_method: bool,
    pub dynamic_class: bool,
}

impl Usage {
    /// Unions another subtree summary into this one.
    pub(crate) fn merge(&mut self, other: Self) {
        self.functions.extend(other.functions);
        self.classes.extend(other.classes);
        self.methods.extend(other.methods);
        self.externs.extend(other.externs);
        self.scoped_methods.extend(other.scoped_methods);
        self.wildcard_methods.extend(other.wildcard_methods);
        self.instantiated_classes.extend(other.instantiated_classes);
        self.instantiated_subclass_roots
            .extend(other.instantiated_subclass_roots);
        self.required_libraries.extend(other.required_libraries);
        self.global_aliases.extend(other.global_aliases);
        self.dynamic_global_alias |= other.dynamic_global_alias;
        for (variable, methods) in other.variable_methods {
            self.variable_methods
                .entry(variable)
                .or_default()
                .extend(methods);
        }
        self.hazards.dynamic_function |= other.hazards.dynamic_function;
        self.hazards.dynamic_method |= other.hazards.dynamic_method;
        self.hazards.dynamic_class |= other.hazards.dynamic_class;
    }

    /// Returns whether a free function is referenced using PHP lookup semantics.
    #[allow(dead_code)]
    pub fn references_function(&self, name: &str) -> bool {
        self.functions.contains(&php_symbol_key(name))
    }
}

/// Scans every statement and declaration body in a program.
pub fn scan_program(program: &[Stmt]) -> Usage {
    Scanner::default().scan_program(program, true)
}

/// Scans one statement and every nested declaration body.
#[allow(dead_code)]
pub fn scan_stmt(stmt: &Stmt) -> Usage {
    Scanner::default().scan_program(std::slice::from_ref(stmt), true)
}

/// Scans only executable roots, omitting declaration bodies until their nodes become live.
pub(super) fn scan_executable_program(
    program: &[Stmt],
    call_signatures: &CallSignatureIndex,
) -> Usage {
    Scanner {
        call_signatures: Some(call_signatures),
        ..Scanner::default()
    }
    .scan_program(program, false)
}

/// Scans one function declaration's contract and body without treating the declaration as a root.
pub(super) fn scan_function(stmt: &Stmt, call_signatures: &CallSignatureIndex) -> Usage {
    let mut scanner = Scanner {
        call_signatures: Some(call_signatures),
        ..Scanner::default()
    };
    scanner.scan_attributes(&stmt.attributes);
    if let StmtKind::FunctionDecl {
        params,
        param_attributes,
        variadic_type,
        return_type,
        body,
        ..
    } = &stmt.kind
    {
        scanner.scan_parameter_attributes(param_attributes);
        scanner.scan_params(params, variadic_type.as_ref(), return_type.as_ref());
        scanner.scan_nested(body, false);
    }
    scanner.usage
}

/// Scans one method's annotations, defaults, attributes, and body in its declaring class.
pub(super) fn scan_method(
    method: &ClassMethod,
    class_name: &str,
    parent_class: Option<&str>,
    call_signatures: &CallSignatureIndex,
) -> Usage {
    let mut scanner = Scanner {
        current_class: Some(class_name.to_string()),
        parent_class: parent_class.map(str::to_string),
        call_signatures: Some(call_signatures),
        ..Scanner::default()
    };
    scanner.scan_method(method);
    scanner.usage
}

/// Scans declaration-level class dependencies without traversing method bodies.
pub(super) fn scan_class_shell(stmt: &Stmt) -> Usage {
    let mut scanner = Scanner::default();
    scanner.scan_attributes(&stmt.attributes);
    match &stmt.kind {
        StmtKind::ClassDecl {
            name,
            extends,
            implements,
            trait_uses,
            properties,
            constants,
            ..
        } => {
            scanner.current_class = Some(name.clone());
            scanner.parent_class = extends.as_ref().map(|name| name.as_str().to_string());
            if let Some(parent) = extends {
                scanner.record_class(parent.as_str());
            }
            for interface in implements {
                scanner.record_class(interface.as_str());
            }
            scanner.scan_trait_uses(trait_uses);
            scanner.scan_properties(properties);
            scanner.scan_constants(constants);
        }
        StmtKind::EnumDecl {
            name,
            backing_type,
            cases,
            implements,
            trait_uses,
            constants,
            ..
        } => {
            scanner.current_class = Some(name.clone());
            if let Some(ty) = backing_type {
                scanner.scan_type(ty);
            }
            for interface in implements {
                scanner.record_class(interface.as_str());
            }
            scanner.scan_trait_uses(trait_uses);
            for case in cases {
                scanner.scan_attributes(&case.attributes);
                if let Some(value) = &case.value {
                    scanner.scan_expr(value);
                }
            }
            scanner.scan_constants(constants);
        }
        StmtKind::InterfaceDecl {
            name,
            extends,
            properties,
            constants,
            ..
        } => {
            scanner.current_class = Some(name.clone());
            for parent in extends {
                scanner.record_class(parent.as_str());
            }
            scanner.scan_properties(properties);
            scanner.scan_constants(constants);
        }
        StmtKind::TraitDecl {
            name,
            trait_uses,
            properties,
            constants,
            ..
        } => {
            scanner.current_class = Some(name.clone());
            scanner.scan_trait_uses(trait_uses);
            scanner.scan_properties(properties);
            scanner.scan_constants(constants);
        }
        _ => {}
    }
    scanner.usage
}

#[derive(Default)]
struct Scanner<'a> {
    usage: Usage,
    variable_classes: HashMap<String, HashSet<String>>,
    definitely_non_object_variables: HashSet<String>,
    invalidated_variables: HashSet<String>,
    current_class: Option<String>,
    current_method: Option<String>,
    parent_class: Option<String>,
    call_signatures: Option<&'a CallSignatureIndex>,
}

/// Checker-validated signatures used to recognize callable and by-reference arguments.
#[derive(Debug, Default)]
pub(super) struct CallSignatureIndex {
    functions: HashMap<String, FunctionSig>,
    methods: HashMap<(String, String, bool), FunctionSig>,
    property_classes: HashMap<(String, String), HashSet<String>>,
    class_parents: HashMap<String, Option<String>>,
}

impl CallSignatureIndex {
    /// Builds case-insensitive free-function and class-method signature indexes.
    pub(super) fn from_check_result(check_result: &CheckResult) -> Self {
        let mut functions: HashMap<_, _> = check_result
            .functions
            .iter()
            .map(|(name, signature)| (php_symbol_key(name), signature.clone()))
            .collect();
        for (name, signature) in &check_result.extern_functions {
            functions.entry(php_symbol_key(name)).or_insert_with(|| FunctionSig {
                params: signature.params.clone(),
                param_type_exprs: vec![None; signature.params.len()],
                param_attributes: vec![Vec::new(); signature.params.len()],
                defaults: vec![None; signature.params.len()],
                return_type: signature.return_type.clone(),
                by_ref_return: false,
                ref_params: vec![false; signature.params.len()],
                declared_params: vec![true; signature.params.len()],
                variadic: None,
                deprecation: None,
                declared_return: true,
            });
        }
        let methods = check_result
            .classes
            .iter()
            .flat_map(|(class, info)| {
                let class = php_symbol_key(class);
                info.methods
                    .iter()
                    .map({
                        let class = class.clone();
                        move |(method, signature)| {
                            (
                                (class.clone(), php_symbol_key(method), false),
                                signature.clone(),
                            )
                        }
                    })
                    .chain(info.static_methods.iter().map({
                        let class = class.clone();
                        move |(method, signature)| {
                            (
                                (class.clone(), php_symbol_key(method), true),
                                signature.clone(),
                            )
                        }
                    }))
            })
            .collect();
        let property_classes = check_result
            .classes
            .iter()
            .flat_map(|(class, info)| {
                let class = php_symbol_key(class);
                info.properties.iter().filter_map(move |(property, property_type)| {
                    let classes = php_type_classes(property_type);
                    (!classes.is_empty()).then(|| ((class.clone(), property.clone()), classes))
                })
            })
            .collect();
        let class_parents = check_result
            .classes
            .iter()
            .map(|(class, info)| {
                (
                    php_symbol_key(class),
                    info.parent.as_deref().map(php_symbol_key),
                )
            })
            .collect();
        Self {
            functions,
            methods,
            property_classes,
            class_parents,
        }
    }

    /// Returns concrete object classes stored in a declared property on any receiver candidate.
    fn property(&self, owners: &HashSet<String>, property: &str) -> HashSet<String> {
        owners
            .iter()
            .filter_map(|owner| {
                self.property_classes
                    .get(&(owner.clone(), property.to_string()))
            })
            .flat_map(|classes| classes.iter().cloned())
            .collect()
    }

    /// Returns the signature of one direct free-function call.
    fn function(&self, name: &str) -> Option<FunctionSig> {
        self.functions
            .get(name)
            .cloned()
            .or_else(|| crate::builtins::registry::function_sig(name))
    }

    /// Returns every possible signature for a method call over the known receiver set.
    fn method(&self, classes: &HashSet<String>, method: &str, is_static: bool) -> Vec<FunctionSig> {
        if classes.is_empty() {
            self.methods
                .iter()
                .filter_map(|((_, candidate, candidate_static), signature)| {
                    (candidate == method && *candidate_static == is_static)
                        .then(|| signature.clone())
                })
                .collect()
        } else {
            classes
                .iter()
                .filter_map(|class| {
                    self.methods
                        .get(&(class.clone(), method.to_string(), is_static))
                        .cloned()
                })
            .collect()
        }
    }

    /// Returns every checker-known class equal to or descending from one late-static root.
    fn subclasses_including(&self, root: &str) -> HashSet<String> {
        let root = php_symbol_key(root);
        self.class_parents
            .keys()
            .filter(|class| {
                let mut current = Some((*class).clone());
                let mut seen = HashSet::new();
                while let Some(candidate) = current {
                    if !seen.insert(candidate.clone()) {
                        return false;
                    }
                    if candidate == root {
                        return true;
                    }
                    current = self.class_parents.get(&candidate).cloned().flatten();
                }
                false
            })
            .cloned()
            .collect()
    }
}

/// Extracts concrete object members from one checker-resolved PHP type.
fn php_type_classes(php_type: &PhpType) -> HashSet<String> {
    match php_type.codegen_repr() {
        PhpType::Object(class) if !class.is_empty() => {
            [php_symbol_key(&class)].into_iter().collect()
        }
        PhpType::Union(members) => members.iter().flat_map(php_type_classes).collect(),
        _ => HashSet::new(),
    }
}

impl Scanner<'_> {
    /// Scans a statement list, optionally descending into declaration bodies.
    fn scan_program(mut self, program: &[Stmt], declarations: bool) -> Usage {
        for stmt in program {
            self.scan_statement(stmt, declarations);
        }
        self.usage
    }

    /// Scans every expression-bearing position of one statement.
    fn scan_statement(&mut self, stmt: &Stmt, declarations: bool) {
        if declarations {
            self.scan_attributes(&stmt.attributes);
        }
        match &stmt.kind {
            StmtKind::Echo(e) | StmtKind::Throw(e) | StmtKind::ExprStmt(e)
            | StmtKind::ConstDecl { value: e, .. } | StmtKind::Return(Some(e))
            | StmtKind::Include { path: e, .. }
            | StmtKind::StaticPropertyArrayPush { value: e, .. } => self.scan_expr(e),
            StmtKind::Assign { name, value } => {
                self.scan_expr(value);
                self.remember_assignment(name, value);
            }
            StmtKind::TypedAssign {
                type_expr,
                name,
                value,
            } => {
                self.scan_type(type_expr);
                self.scan_expr(value);
                self.remember_typed_assignment(name, type_expr, value);
            }
            StmtKind::RefAssign { target, source } => {
                self.scan_expr(source);
                self.forget_variable(target);
                if let ExprKind::Variable(source) = &source.kind {
                    self.forget_variable(source);
                }
            }
            StmtKind::ListUnpack { vars, value } => {
                self.scan_expr(value);
                for variable in vars {
                    self.forget_variable(variable);
                }
            }
            StmtKind::StaticVar { name, init } => {
                self.scan_expr(init);
                self.forget_variable(name);
            }
            StmtKind::ArrayAssign { array, index, value } => {
                if array == "GLOBALS" {
                    self.record_globals_index(index);
                } else {
                    self.record_variable_protocol_methods(array, &["offsetSet"]);
                }
                self.scan_expr(index);
                self.scan_expr(value);
            }
            StmtKind::ArrayPush { array, value } => {
                if array == "GLOBALS" {
                    self.usage.dynamic_global_alias = true;
                } else {
                    self.record_variable_protocol_methods(array, &["offsetSet"]);
                }
                self.scan_expr(value);
            }
            StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
                self.scan_expr(index);
                self.scan_expr(value);
            }
            StmtKind::NestedArrayAssign { target, value } => {
                self.scan_assignment_target(target);
                self.scan_expr(target);
                self.scan_expr(value);
            }
            StmtKind::PropertyAssign {
                object,
                property,
                value,
            } => {
                self.record_instance_method(object, &property_hook_set_method(property));
                self.scan_expr(object);
                self.scan_expr(value);
            }
            StmtKind::PropertyArrayPush {
                object,
                property,
                value,
            } => {
                self.record_instance_method(object, &property_hook_get_method(property));
                self.record_instance_method(object, &property_hook_set_method(property));
                self.scan_expr(object);
                self.scan_expr(value);
            }
            StmtKind::DynamicPropertyArrayPush {
                object,
                property,
                value,
            } => {
                self.usage.hazards.dynamic_method = true;
                self.scan_expr(object);
                self.scan_expr(property);
                self.scan_expr(value);
            }
            StmtKind::PropertyArrayAssign {
                object,
                property,
                index,
                value,
            } => {
                self.record_instance_method(object, &property_hook_get_method(property));
                self.record_instance_method(object, &property_hook_set_method(property));
                self.scan_expr(object);
                self.scan_expr(index);
                self.scan_expr(value);
            }
            StmtKind::StaticPropertyAssign { receiver, value, .. } => {
                self.scan_receiver(receiver);
                self.scan_expr(value);
            }
            StmtKind::If { condition, then_body, elseif_clauses, else_body } => {
                self.scan_expr(condition);
                self.scan_guarded_non_object_body(condition, then_body, declarations);
                for (condition, body) in elseif_clauses {
                    self.scan_expr(condition);
                    self.scan_guarded_non_object_body(condition, body, declarations);
                }
                if let Some(body) = else_body { self.scan_nested(body, declarations); }
            }
            StmtKind::IfDef { then_body, else_body, .. } => {
                self.scan_nested(then_body, declarations);
                if let Some(body) = else_body { self.scan_nested(body, declarations); }
            }
            StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
                self.scan_expr(condition);
                self.scan_nested(body, declarations);
            }
            StmtKind::For { init, condition, update, body } => {
                if let Some(init) = init { self.scan_statement(init, declarations); }
                if let Some(condition) = condition { self.scan_expr(condition); }
                if let Some(update) = update { self.scan_statement(update, declarations); }
                self.scan_nested(body, declarations);
            }
            StmtKind::Foreach {
                array,
                key_var,
                value_var,
                body,
                ..
            } => {
                self.record_protocol_methods(
                    array,
                    &["getIterator", "rewind", "valid", "current", "key", "next"],
                );
                self.scan_expr(array);
                if let Some(key_var) = key_var {
                    self.forget_variable(key_var);
                }
                self.forget_variable(value_var);
                self.scan_nested(body, declarations);
            }
            StmtKind::Switch { subject, cases, default } => {
                self.scan_expr(subject);
                for (patterns, body) in cases {
                    for pattern in patterns { self.scan_expr(pattern); }
                    self.scan_nested(body, declarations);
                }
                if let Some(body) = default { self.scan_nested(body, declarations); }
            }
            StmtKind::Synthetic(body) | StmtKind::NamespaceBlock { body, .. }
            | StmtKind::IncludeOnceGuard { body, .. } => self.scan_nested(body, declarations),
            StmtKind::Try { try_body, catches, finally_body } => {
                self.scan_nested(try_body, declarations);
                for catch in catches {
                    for ty in &catch.exception_types { self.record_class(ty.as_str()); }
                    if let Some(variable) = &catch.variable {
                        self.forget_variable(variable);
                    }
                    self.scan_nested(&catch.body, declarations);
                }
                if let Some(body) = finally_body { self.scan_nested(body, declarations); }
            }
            StmtKind::FunctionDecl { params, variadic_type, return_type, body, .. } if declarations => {
                self.scan_params(params, variadic_type.as_ref(), return_type.as_ref());
                self.scan_nested(body, true);
            }
            StmtKind::ClassDecl { name, methods, .. }
            | StmtKind::EnumDecl { name, methods, .. }
            | StmtKind::InterfaceDecl { name, methods, .. }
            | StmtKind::TraitDecl { name, methods, .. } if declarations => {
                let prior = self.current_class.replace(name.clone());
                self.usage.merge(scan_class_shell(stmt));
                for method in methods { self.scan_method(method); }
                self.current_class = prior;
            }
            StmtKind::PackedClassDecl { name, .. } if declarations => {
                self.record_class(name);
            }
            StmtKind::ExternClassDecl { name, .. } if declarations => {
                self.record_class(name);
            }
            StmtKind::ExternFunctionDecl { name, .. } if declarations => {
                self.record_callable(name);
            }
            StmtKind::Return(None) | StmtKind::Break(_) | StmtKind::Continue(_)
            | StmtKind::NamespaceDecl { .. } | StmtKind::UseDecl { .. }
            | StmtKind::FunctionVariantGroup { .. } | StmtKind::FunctionVariantMark { .. }
            | StmtKind::IncludeOnceMark { .. }
            | StmtKind::ExternGlobalDecl { .. } | StmtKind::FunctionDecl { .. }
            | StmtKind::ClassDecl { .. } | StmtKind::EnumDecl { .. }
            | StmtKind::InterfaceDecl { .. } | StmtKind::TraitDecl { .. }
            | StmtKind::PackedClassDecl { .. } | StmtKind::ExternClassDecl { .. }
            | StmtKind::ExternFunctionDecl { .. } => {}
            StmtKind::Global { vars } => {
                for variable in vars {
                    self.usage.global_aliases.insert(variable.clone());
                    self.forget_variable(variable);
                }
            }
        }
    }

    /// Scans a nested list while preserving the surrounding variable/class facts.
    fn scan_nested(&mut self, body: &[Stmt], declarations: bool) {
        for stmt in body { self.scan_statement(stmt, declarations); }
    }

    /// Scans a branch with positive `is_array()` facts that exclude protocol dispatch.
    fn scan_guarded_non_object_body(
        &mut self,
        condition: &Expr,
        body: &[Stmt],
        declarations: bool,
    ) {
        let mut guarded = HashSet::new();
        collect_is_array_guards(condition, &mut guarded);
        let added: Vec<_> = guarded
            .into_iter()
            .filter(|name| self.definitely_non_object_variables.insert(name.clone()))
            .collect();
        self.scan_nested(body, declarations);
        for name in added {
            self.definitely_non_object_variables.remove(&name);
        }
    }

    /// Scans one class method contract and body.
    fn scan_method(&mut self, method: &ClassMethod) {
        let prior_method = self.current_method.replace(method.name.clone());
        self.scan_attributes(&method.attributes);
        self.scan_parameter_attributes(&method.param_attributes);
        self.scan_params(&method.params, method.variadic_type.as_ref(), method.return_type.as_ref());
        self.scan_nested(&method.body, true);
        self.current_method = prior_method;
    }

    /// Scans attribute classes and arguments attached to fixed or variadic parameters.
    fn scan_parameter_attributes(&mut self, parameter_attributes: &[Vec<AttributeGroup>]) {
        for groups in parameter_attributes {
            self.scan_attributes(groups);
        }
    }

    /// Scans parameter defaults and records declared receiver domains for the body scan.
    fn scan_params(&mut self, params: &[(String, Option<TypeExpr>, Option<Expr>, bool)], variadic: Option<&TypeExpr>, ret: Option<&TypeExpr>) {
        for (name, ty, default, _) in params {
            if let Some(ty) = ty {
                self.scan_type(ty);
                let classes = self.type_classes(ty);
                if classes.is_empty() {
                    if type_is_definitely_non_object(ty) {
                        self.definitely_non_object_variables.insert(name.clone());
                    }
                } else {
                    self.variable_classes
                        .entry(name.clone())
                        .or_default()
                        .extend(classes);
                }
            }
            if let Some(default) = default { self.scan_expr(default); }
        }
        if let Some(ty) = variadic { self.scan_type(ty); }
        if let Some(ty) = ret { self.scan_type(ty); }
    }

    /// Scans class property types, defaults, and attributes.
    fn scan_properties(&mut self, properties: &[ClassProperty]) {
        for property in properties {
            self.scan_attributes(&property.attributes);
            if let Some(ty) = &property.type_expr { self.scan_type(ty); }
            if let Some(default) = &property.default { self.scan_expr(default); }
        }
    }

    /// Scans class constant types, values, and attributes.
    fn scan_constants(&mut self, constants: &[ClassConst]) {
        for constant in constants {
            self.scan_attributes(&constant.attributes);
            if let Some(ty) = &constant.type_expr { self.scan_type(ty); }
            self.scan_expr(&constant.value);
        }
    }

    /// Scans trait relations and adaptation type names.
    fn scan_trait_uses(&mut self, uses: &[TraitUse]) {
        for trait_use in uses {
            for name in &trait_use.trait_names { self.record_class(name.as_str()); }
        }
    }

    /// Scans attribute class names and argument expressions.
    fn scan_attributes(&mut self, groups: &[AttributeGroup]) {
        for group in groups {
            for attribute in &group.attributes {
                let class = self.record_class(attribute.name.as_str());
                self.usage.instantiated_classes.insert(class);
                for argument in &attribute.args { self.scan_expr(argument); }
            }
        }
    }

    /// Records a `$GLOBALS[...]` alias, preserving literal variable names when possible.
    fn record_globals_index(&mut self, index: &Expr) {
        if let ExprKind::StringLiteral(variable) = &index.kind {
            self.usage.global_aliases.insert(variable.clone());
        } else {
            self.usage.dynamic_global_alias = true;
        }
    }

    /// Scans named types nested in nullable, union, intersection, and container forms.
    fn scan_type(&mut self, ty: &TypeExpr) {
        match ty {
            TypeExpr::Named(name) => {
                self.record_class(name.as_str());
            }
            TypeExpr::Ptr(Some(name)) => {
                self.record_class(name.as_str());
            }
            TypeExpr::Array(inner) | TypeExpr::Buffer(inner) | TypeExpr::Nullable(inner) => self.scan_type(inner),
            TypeExpr::Union(types) | TypeExpr::Intersection(types) => {
                for ty in types { self.scan_type(ty); }
            }
            TypeExpr::Int | TypeExpr::Float | TypeExpr::Bool | TypeExpr::False | TypeExpr::Str
            | TypeExpr::Void | TypeExpr::Never | TypeExpr::Iterable | TypeExpr::Ptr(None) => {}
        }
    }

}

/// Returns whether a declared local type excludes every object representation.
fn type_is_definitely_non_object(ty: &TypeExpr) -> bool {
    match ty {
        TypeExpr::Int
        | TypeExpr::Float
        | TypeExpr::Bool
        | TypeExpr::False
        | TypeExpr::Str
        | TypeExpr::Void
        | TypeExpr::Never
        | TypeExpr::Array(_)
        | TypeExpr::Ptr(_)
        | TypeExpr::Buffer(_) => true,
        TypeExpr::Named(name) => matches!(
            name.as_str().to_ascii_lowercase().as_str(),
            "array" | "bool" | "false" | "float" | "int" | "never" | "null" | "string" | "void"
        ),
        TypeExpr::Nullable(inner) => type_is_definitely_non_object(inner),
        TypeExpr::Union(types) => types.iter().all(type_is_definitely_non_object),
        TypeExpr::Iterable | TypeExpr::Intersection(_) => false,
    }
}

/// Collects variables proven arrays by positive conjuncts of one branch condition.
fn collect_is_array_guards(condition: &Expr, variables: &mut HashSet<String>) {
    match &condition.kind {
        ExprKind::FunctionCall { name, args }
            if php_symbol_key(name.as_str().trim_start_matches('\\')) == "is_array" =>
        {
            if let Some(argument) = args.first() {
                let argument = if let ExprKind::NamedArg { value, .. } = &argument.kind {
                    value.as_ref()
                } else {
                    argument
                };
                if let ExprKind::Variable(name) = &argument.kind {
                    variables.insert(name.clone());
                }
            }
        }
        ExprKind::BinaryOp {
            left,
            op: BinOp::And,
            right,
        } => {
            collect_is_array_guards(left, variables);
            collect_is_array_guards(right, variables);
        }
        _ => {}
    }
}
