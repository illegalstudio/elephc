//! Purpose:
//! Preserves definite array receiver facts while auditing writes before target pruning.
//!
//! Called from:
//! - `crate::semantics::validation::validate_source` under `--strict-array-keys`.
//!
//! Key details:
//! - Flow facts are deliberately small and conservative: unknown receivers remain checker work.
//! - Branches receive independent facts and only identical facts survive their merge.
//! - Declared array properties are checked without treating strings or ArrayAccess objects as arrays.

use std::collections::{BTreeSet, HashMap};

use crate::ast_visit::{self, AstVisitor, SourcePolicy};
use crate::errors::CompileError;
use crate::parser::ast::{
    ArrayEntry, CastType, ClassMethod, Expr, ExprKind, Program, StaticReceiver, Stmt, StmtKind,
    TypeExpr,
};
use crate::types::PhpType;
use elephc_semantics::SemanticProfile;

/// One local receiver fact retained by the syntax-preserving write audit.
#[derive(Clone, Debug, PartialEq)]
enum ValueFact {
    Known(PhpType),
    Unknown,
}

/// Flow facts for one PHP activation.
#[derive(Clone, Debug, Default)]
struct LocalFacts {
    values: HashMap<String, ValueFact>,
}

impl LocalFacts {
    /// Returns an established fact; a never-mentioned local is PHP null before an array write.
    fn get(&self, name: &str) -> ValueFact {
        self.values
            .get(name)
            .cloned()
            .unwrap_or_else(|| {
                if crate::names::is_generated_local_name(name) {
                    ValueFact::Unknown
                } else {
                    ValueFact::Known(PhpType::Void)
                }
            })
    }

    /// Replaces one local fact after a source-order assignment.
    fn set(&mut self, name: &str, fact: ValueFact) {
        self.values.insert(name.to_owned(), fact);
    }
}

/// Declared property types keyed by their canonical source class and property names.
#[derive(Default)]
struct PropertyFacts {
    values: HashMap<(String, String), PhpType>,
}

/// Audits definite array writes before target-dependent folding can discard their branches.
pub(crate) fn validate_source_writes(
    program: &Program,
    profile: SemanticProfile,
) -> Result<(), CompileError> {
    let mut properties = PropertyFacts::default();
    collect_properties(program, &mut properties);
    validate_nested_closures(program, &properties, profile)?;
    let mut locals = LocalFacts::default();
    validate_block(program, &mut locals, &properties, profile, None)
}

/// Audits closures in every expression position before the statement flow walk begins.
fn validate_nested_closures(
    program: &Program,
    properties: &PropertyFacts,
    profile: SemanticProfile,
) -> Result<(), CompileError> {
    let mut visitor = ClosureWriteValidator {
        properties,
        profile,
        source: SourcePolicy::default(),
        classes: Vec::new(),
        error: None,
    };
    ast_visit::walk_program(program, &mut visitor);
    visitor.error.map_or(Ok(()), Err)
}

/// Retains source policy and lexical class context while visiting closure expressions.
struct ClosureWriteValidator<'a> {
    properties: &'a PropertyFacts,
    profile: SemanticProfile,
    source: SourcePolicy,
    classes: Vec<String>,
    error: Option<CompileError>,
}

impl AstVisitor for ClosureWriteValidator<'_> {
    /// Enters the physical statement scope and records lexical class declarations.
    fn statement(&mut self, stmt: &Stmt) {
        self.source.enter_statement(stmt);
        match &stmt.kind {
            StmtKind::ClassDecl { name, .. }
            | StmtKind::InterfaceDecl { name, .. }
            | StmtKind::TraitDecl { name, .. }
            | StmtKind::EnumDecl { name, .. } => self.classes.push(name.clone()),
            _ => {}
        }
    }

    /// Restores lexical class and physical source state after one statement subtree.
    fn statement_end(&mut self, stmt: &Stmt) {
        if matches!(&stmt.kind, StmtKind::ClassDecl { .. }
            | StmtKind::InterfaceDecl { .. }
            | StmtKind::TraitDecl { .. }
            | StmtKind::EnumDecl { .. }) {
            self.classes.pop().expect("balanced class source traversal");
        }
        self.source.leave_statement();
    }

    /// Validates every user-authored closure body with its own activation facts.
    fn expression(&mut self, expr: &Expr) {
        let source = self.source.enter_expression(expr);
        if self.error.is_some() || source.mode == crate::source::SourceMode::Internal {
            return;
        }
        self.error = validate_closure_value(
            expr,
            self.properties,
            self.profile,
            self.classes.last().map(String::as_str),
        )
        .err();
    }

    /// Restores the enclosing expression producer policy.
    fn expression_end(&mut self, _expr: &Expr) {
        self.source.leave_expression();
    }
}

/// Audits assignment expressions belonging directly to one statement's expression tree.
fn validate_expression_writes(
    stmt: &Stmt,
    locals: &LocalFacts,
    properties: &PropertyFacts,
    profile: SemanticProfile,
    current_class: Option<&str>,
) -> Result<(), CompileError> {
    let mut visitor = ExpressionWriteValidator {
        locals,
        properties,
        profile,
        current_class,
        source: SourcePolicy::default(),
        statement_depth: 0,
        error: None,
    };
    ast_visit::walk_statements(std::slice::from_ref(stmt), &mut visitor);
    visitor.error.map_or(Ok(()), Err)
}

/// Keeps nested statement bodies on their own flow-sensitive validation pass.
struct ExpressionWriteValidator<'a> {
    locals: &'a LocalFacts,
    properties: &'a PropertyFacts,
    profile: SemanticProfile,
    current_class: Option<&'a str>,
    source: SourcePolicy,
    statement_depth: usize,
    error: Option<CompileError>,
}

impl AstVisitor for ExpressionWriteValidator<'_> {
    /// Tracks the root statement separately from nested control-flow and closure bodies.
    fn statement(&mut self, stmt: &Stmt) {
        self.source.enter_statement(stmt);
        self.statement_depth += 1;
    }

    /// Leaves nested statement bodies to their own flow-sensitive validation call.
    fn descend_statement(&mut self, _stmt: &Stmt) -> bool {
        self.statement_depth == 1
    }

    /// Restores the enclosing statement depth and source policy.
    fn statement_end(&mut self, _stmt: &Stmt) {
        self.statement_depth -= 1;
        self.source.leave_statement();
    }

    /// Validates array targets kept in expression form before target pruning can erase them.
    fn expression(&mut self, expr: &Expr) {
        let source = self.source.enter_expression(expr);
        if self.error.is_some()
            || self.statement_depth != 1
            || source.mode == crate::source::SourceMode::Internal
        {
            return;
        }
        let ExprKind::Assignment {
            target, prelude, ..
        } = &expr.kind
        else {
            return;
        };
        let overlay = if prelude.is_empty() {
            None
        } else {
            Some(locals_with_prelude(
                self.locals,
                prelude,
                self.properties,
                self.current_class,
            ))
        };
        let locals = overlay.as_ref().unwrap_or(self.locals);
        self.error = validate_array_access_chain(
            self.profile,
            target,
            locals,
            self.properties,
            self.current_class,
        )
        .err();
    }

    /// Restores the enclosing expression producer policy.
    fn expression_end(&mut self, _expr: &Expr) {
        self.source.leave_expression();
    }
}

/// Applies parser-generated target stabilization facts in source evaluation order.
fn locals_with_prelude(
    entry: &LocalFacts,
    statements: &[Stmt],
    properties: &PropertyFacts,
    current_class: Option<&str>,
) -> LocalFacts {
    let mut locals = entry.clone();
    apply_prelude_facts(statements, &mut locals, properties, current_class);
    locals
}

/// Records only local facts needed to type stabilized assignment targets.
fn apply_prelude_facts(
    statements: &[Stmt],
    locals: &mut LocalFacts,
    properties: &PropertyFacts,
    current_class: Option<&str>,
) {
    for stmt in statements {
        match &stmt.kind {
            StmtKind::Assign { name, value } => {
                let fact = infer_expr(value, locals, properties, current_class);
                locals.set(name, fact);
            }
            StmtKind::TypedAssign {
                type_expr, name, ..
            } => locals.set(
                name,
                ValueFact::Known(crate::types::constant_binding::declared_type(type_expr)),
            ),
            StmtKind::RefAssign { target, source } => {
                locals.set(target, ValueFact::Unknown);
                if let ExprKind::Variable(source) = &source.kind {
                    locals.set(source, ValueFact::Unknown);
                }
            }
            StmtKind::StaticVar { name, init } => {
                let fact = infer_expr(init, locals, properties, current_class);
                locals.set(name, fact);
            }
            StmtKind::ListUnpack { vars, .. } => {
                for name in vars {
                    locals.set(name, ValueFact::Unknown);
                }
            }
            StmtKind::Synthetic(body) => {
                apply_prelude_facts(body, locals, properties, current_class);
            }
            _ => {}
        }
    }
}

/// Collects declared property contracts independently of executable source order.
fn collect_properties(program: &[Stmt], facts: &mut PropertyFacts) {
    for stmt in program {
        match &stmt.kind {
            StmtKind::ClassDecl {
                name,
                properties,
                methods: _,
                ..
            }
            | StmtKind::InterfaceDecl {
                name,
                properties,
                methods: _,
                ..
            }
            | StmtKind::TraitDecl {
                name,
                properties,
                methods: _,
                ..
            } => {
                for property in properties {
                    if let Some(ty) = property.type_expr.as_ref() {
                        facts.values.insert(
                            (name.clone(), property.name.clone()),
                            crate::types::constant_binding::declared_type(ty),
                        );
                    }
                }
            }
            StmtKind::Synthetic(body)
            | StmtKind::NamespaceBlock { body, .. }
            | StmtKind::IncludeOnceGuard { body, .. } => collect_properties(body, facts),
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                collect_properties(then_body, facts);
                for (_, body) in elseif_clauses {
                    collect_properties(body, facts);
                }
                if let Some(body) = else_body {
                    collect_properties(body, facts);
                }
            }
            _ => {}
        }
    }
}

/// Validates one statement list in source order and returns its surviving local facts.
fn validate_block(
    statements: &[Stmt],
    locals: &mut LocalFacts,
    properties: &PropertyFacts,
    profile: SemanticProfile,
    current_class: Option<&str>,
) -> Result<(), CompileError> {
    for stmt in statements {
        validate_stmt(stmt, locals, properties, profile, current_class)?;
    }
    Ok(())
}

/// Validates writes in one statement and updates conservative local receiver facts.
fn validate_stmt(
    stmt: &Stmt,
    locals: &mut LocalFacts,
    properties: &PropertyFacts,
    profile: SemanticProfile,
    current_class: Option<&str>,
) -> Result<(), CompileError> {
    // Compiler-owned preludes keep their established PHP-compatible implementation details.
    // This pass exists only to preserve user source that target pruning could otherwise erase;
    // applying it to an internal declaration would make a strict caller revalidate injected
    // helper bodies under a policy those bodies do not opt into.
    if stmt.source_mode == crate::source::SourceMode::Internal {
        return Ok(());
    }
    validate_expression_writes(stmt, locals, properties, profile, current_class)?;
    match &stmt.kind {
        StmtKind::Assign { name, value } => {
            locals.set(name, infer_expr(value, locals, properties, current_class));
        }
        StmtKind::TypedAssign {
            type_expr,
            name,
            value: _,
        } => {
            locals.set(
                name,
                ValueFact::Known(crate::types::constant_binding::declared_type(type_expr)),
            );
        }
        StmtKind::RefAssign { target, source } => {
            locals.set(target, ValueFact::Unknown);
            if let ExprKind::Variable(source) = &source.kind {
                locals.set(source, ValueFact::Unknown);
            }
        }
        StmtKind::Global { vars } => {
            for name in vars {
                locals.set(name, ValueFact::Unknown);
            }
        }
        StmtKind::StaticVar { name, init } => {
            locals.set(name, infer_expr(init, locals, properties, current_class));
        }
        StmtKind::ListUnpack { vars, .. } => {
            for name in vars {
                locals.set(name, ValueFact::Unknown);
            }
        }
        StmtKind::ArrayAssign { array, index, .. } => {
            let receiver = locals.get(array);
            validate_index(
                profile,
                &receiver,
                index,
                locals,
                properties,
                current_class,
            )?;
            if matches!(receiver, ValueFact::Known(PhpType::Void | PhpType::False)) {
                locals.set(array, ValueFact::Known(PhpType::php_array()));
            }
        }
        StmtKind::ArrayPush { array, .. } => {
            if matches!(locals.get(array), ValueFact::Known(PhpType::Void | PhpType::False)) {
                locals.set(array, ValueFact::Known(PhpType::php_array()));
            }
        }
        StmtKind::NestedArrayAssign { target, .. } => {
            validate_array_access_chain(
                profile,
                target,
                locals,
                properties,
                current_class,
            )?;
        }
        StmtKind::PropertyArrayAssign {
            object,
            property,
            index,
            ..
        } => {
            let receiver = property_fact(
                infer_expr(object, locals, properties, current_class),
                property,
                properties,
            );
            validate_index(
                profile,
                &receiver,
                index,
                locals,
                properties,
                current_class,
            )?;
        }
        StmtKind::StaticPropertyArrayAssign {
            receiver,
            property,
            index,
            ..
        } => {
            let receiver = static_property_fact(receiver, property, properties, current_class);
            validate_index(
                profile,
                &receiver,
                index,
                locals,
                properties,
                current_class,
            )?;
        }
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            let entry = locals.clone();
            let mut exits = Vec::with_capacity(elseif_clauses.len() + 2);
            exits.push(validate_branch(
                then_body,
                &entry,
                properties,
                profile,
                current_class,
            )?);
            for (_, body) in elseif_clauses {
                exits.push(validate_branch(
                    body,
                    &entry,
                    properties,
                    profile,
                    current_class,
                )?);
            }
            if let Some(body) = else_body {
                exits.push(validate_branch(
                    body,
                    &entry,
                    properties,
                    profile,
                    current_class,
                )?);
            } else {
                exits.push(entry);
            }
            *locals = merge_exits(&exits);
        }
        StmtKind::While { body, .. } | StmtKind::Foreach { body, .. } => {
            let entry = locals.clone();
            let exit = validate_branch(body, &entry, properties, profile, current_class)?;
            *locals = merge_exits(&[entry, exit]);
        }
        StmtKind::DoWhile { body, .. } => {
            validate_block(body, locals, properties, profile, current_class)?;
        }
        StmtKind::For {
            init,
            update,
            body,
            ..
        } => {
            if let Some(init) = init {
                validate_stmt(init, locals, properties, profile, current_class)?;
            }
            let entry = locals.clone();
            let mut exit = validate_branch(body, &entry, properties, profile, current_class)?;
            if let Some(update) = update {
                validate_stmt(update, &mut exit, properties, profile, current_class)?;
            }
            *locals = merge_exits(&[entry, exit]);
        }
        StmtKind::Switch { cases, default, .. } => {
            let entry = locals.clone();
            let mut exits = Vec::with_capacity(cases.len() + 1);
            for (_, body) in cases {
                exits.push(validate_branch(
                    body,
                    &entry,
                    properties,
                    profile,
                    current_class,
                )?);
            }
            if let Some(body) = default {
                exits.push(validate_branch(
                    body,
                    &entry,
                    properties,
                    profile,
                    current_class,
                )?);
            } else {
                exits.push(entry);
            }
            *locals = merge_exits(&exits);
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            let entry = locals.clone();
            let mut exits = vec![validate_branch(
                try_body,
                &entry,
                properties,
                profile,
                current_class,
            )?];
            for catch in catches {
                let mut catch_facts = entry.clone();
                if let Some(variable) = &catch.variable {
                    catch_facts.set(variable, ValueFact::Unknown);
                }
                validate_block(
                    &catch.body,
                    &mut catch_facts,
                    properties,
                    profile,
                    current_class,
                )?;
                exits.push(catch_facts);
            }
            *locals = merge_exits(&exits);
            if let Some(body) = finally_body {
                validate_block(body, locals, properties, profile, current_class)?;
            }
        }
        StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. } => {
            validate_block(body, locals, properties, profile, current_class)?;
        }
        StmtKind::FunctionDecl { params, body, .. } => {
            let mut function = parameter_facts(params);
            validate_block(body, &mut function, properties, profile, None)?;
        }
        StmtKind::ClassDecl { name, methods, .. }
        | StmtKind::InterfaceDecl { name, methods, .. }
        | StmtKind::TraitDecl { name, methods, .. } => {
            validate_methods(name, methods, properties, profile)?;
        }
        StmtKind::EnumDecl { name, methods, .. } => {
            validate_methods(name, methods, properties, profile)?;
        }
        _ => {}
    }
    Ok(())
}

/// Validates one control-flow branch from an immutable entry snapshot.
fn validate_branch(
    body: &[Stmt],
    entry: &LocalFacts,
    properties: &PropertyFacts,
    profile: SemanticProfile,
    current_class: Option<&str>,
) -> Result<LocalFacts, CompileError> {
    let mut exit = entry.clone();
    validate_block(body, &mut exit, properties, profile, current_class)?;
    Ok(exit)
}

/// Keeps only facts that agree on every possible control-flow exit.
fn merge_exits(exits: &[LocalFacts]) -> LocalFacts {
    let mut names = BTreeSet::new();
    for exit in exits {
        names.extend(exit.values.keys().cloned());
    }
    let mut merged = LocalFacts::default();
    for name in names {
        let first = exits[0].get(&name);
        let fact = if exits.iter().skip(1).all(|exit| exit.get(&name) == first) {
            first
        } else {
            ValueFact::Unknown
        };
        merged.set(&name, fact);
    }
    merged
}

/// Seeds function or method locals from their written parameter contracts.
fn parameter_facts(params: &[(String, Option<TypeExpr>, Option<Expr>, bool)]) -> LocalFacts {
    let mut facts = LocalFacts::default();
    for (name, hint, _, _) in params {
        let fact = hint.as_ref().map_or(ValueFact::Unknown, |hint| {
            ValueFact::Known(crate::types::constant_binding::declared_type(hint))
        });
        facts.set(name, fact);
    }
    facts
}

/// Audits every method activation with `$this` and parameter receiver facts.
fn validate_methods(
    class: &str,
    methods: &[ClassMethod],
    properties: &PropertyFacts,
    profile: SemanticProfile,
) -> Result<(), CompileError> {
    for method in methods {
        let mut locals = parameter_facts(&method.params);
        if !method.is_static {
            locals.set("this", ValueFact::Known(PhpType::Object(class.to_owned())));
        }
        validate_block(
            &method.body,
            &mut locals,
            properties,
            profile,
            Some(class),
        )?;
    }
    Ok(())
}

/// Audits a directly assigned closure body without inheriting unproven capture types.
fn validate_closure_value(
    value: &Expr,
    properties: &PropertyFacts,
    profile: SemanticProfile,
    current_class: Option<&str>,
) -> Result<(), CompileError> {
    let ExprKind::Closure {
        params,
        captures,
        capture_refs,
        body,
        ..
    } = &value.kind
    else {
        return Ok(());
    };
    let mut locals = parameter_facts(params);
    for name in captures.iter().chain(capture_refs) {
        locals.set(name, ValueFact::Unknown);
    }
    validate_block(body, &mut locals, properties, profile, current_class)
}

/// Validates a proven write receiver while leaving dynamic and object dimensions to the checker.
fn validate_index(
    profile: SemanticProfile,
    receiver: &ValueFact,
    index: &Expr,
    locals: &LocalFacts,
    properties: &PropertyFacts,
    current_class: Option<&str>,
) -> Result<(), CompileError> {
    let ValueFact::Known(receiver) = receiver else {
        return Ok(());
    };
    let ValueFact::Known(key) = infer_expr(index, locals, properties, current_class) else {
        return Ok(());
    };
    super::validate_write_receiver(profile, receiver, &key, index.span)
}

/// Validates every key whose immediate receiver can be proven while walking to the root.
fn validate_array_access_chain(
    profile: SemanticProfile,
    target: &Expr,
    locals: &LocalFacts,
    properties: &PropertyFacts,
    current_class: Option<&str>,
) -> Result<(), CompileError> {
    let mut current = target;
    while let ExprKind::ArrayAccess { array, index } = &current.kind {
        let receiver = infer_expr(array, locals, properties, current_class);
        validate_index(
            profile,
            &receiver,
            index,
            locals,
            properties,
            current_class,
        )?;
        current = array;
    }
    Ok(())
}

/// Infers only receiver categories that are stable without full name or call resolution.
fn infer_expr(
    expr: &Expr,
    locals: &LocalFacts,
    properties: &PropertyFacts,
    current_class: Option<&str>,
) -> ValueFact {
    if let Some(ty) = source_expr_type(expr) {
        return ValueFact::Known(ty);
    }
    match &expr.kind {
        ExprKind::Variable(name) => locals.get(name),
        ExprKind::NewObject { class_name, .. } => {
            ValueFact::Known(PhpType::Object(class_name.as_str().to_owned()))
        }
        ExprKind::NewGeneric { class_type, .. } => ValueFact::Known(
            crate::types::constant_binding::declared_type(class_type),
        ),
        ExprKind::NewScopedObject { receiver, .. } => static_receiver_name(receiver, current_class)
            .map(|name| ValueFact::Known(PhpType::Object(name)))
            .unwrap_or(ValueFact::Unknown),
        ExprKind::Clone(inner) => infer_expr(inner, locals, properties, current_class),
        ExprKind::PropertyAccess { object, property } => property_fact(
            infer_expr(object, locals, properties, current_class),
            property,
            properties,
        ),
        ExprKind::ArrayAccess { array, index } => array_element_fact(
            infer_expr(array, locals, properties, current_class),
            infer_expr(index, locals, properties, current_class),
        ),
        ExprKind::StaticPropertyAccess { receiver, property } => {
            static_property_fact(receiver, property, properties, current_class)
        }
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => merge_value_facts(
            infer_expr(then_expr, locals, properties, current_class),
            infer_expr(else_expr, locals, properties, current_class),
        ),
        ExprKind::NullCoalesce { value, default } => merge_value_facts(
            infer_expr(value, locals, properties, current_class),
            infer_expr(default, locals, properties, current_class),
        ),
        _ => ValueFact::Unknown,
    }
}

/// Projects an element category only when both the array representation and key kind are known.
fn array_element_fact(receiver: ValueFact, index: ValueFact) -> ValueFact {
    match (receiver, index) {
        (ValueFact::Known(PhpType::Array(element)), ValueFact::Known(PhpType::Int)) => {
            ValueFact::Known(*element)
        }
        (
            ValueFact::Known(PhpType::AssocArray { key, value }),
            ValueFact::Known(actual_key),
        ) if *key == actual_key => ValueFact::Known(*value),
        _ => ValueFact::Unknown,
    }
}

/// Returns a property contract only when its object class is statically established.
fn property_fact(
    object: ValueFact,
    property: &str,
    properties: &PropertyFacts,
) -> ValueFact {
    let ValueFact::Known(PhpType::Object(class)) = object else {
        return ValueFact::Unknown;
    };
    properties
        .values
        .get(&(class, property.to_owned()))
        .cloned()
        .map(ValueFact::Known)
        .unwrap_or(ValueFact::Unknown)
}

/// Returns a declared static property contract for a resolved source receiver.
fn static_property_fact(
    receiver: &StaticReceiver,
    property: &str,
    properties: &PropertyFacts,
    current_class: Option<&str>,
) -> ValueFact {
    let Some(class) = static_receiver_name(receiver, current_class) else {
        return ValueFact::Unknown;
    };
    properties
        .values
        .get(&(class, property.to_owned()))
        .cloned()
        .map(ValueFact::Known)
        .unwrap_or(ValueFact::Unknown)
}

/// Resolves only named, self and static receivers available before type checking.
fn static_receiver_name(
    receiver: &StaticReceiver,
    current_class: Option<&str>,
) -> Option<String> {
    match receiver.written_class_receiver() {
        StaticReceiver::Named(name) => Some(name.as_str().to_owned()),
        StaticReceiver::Self_ | StaticReceiver::Static => current_class.map(str::to_owned),
        StaticReceiver::Parent | StaticReceiver::Generic(_) => None,
    }
}

/// Merges two expression facts without inventing a union receiver proof.
fn merge_value_facts(left: ValueFact, right: ValueFact) -> ValueFact {
    if left == right {
        left
    } else {
        ValueFact::Unknown
    }
}

/// Projects literal and explicit-cast categories used by source-only key validation.
fn source_expr_type(expr: &Expr) -> Option<PhpType> {
    Some(match &expr.kind {
        ExprKind::IntLiteral(_) => PhpType::Int,
        ExprKind::FloatLiteral(_) => PhpType::Float,
        ExprKind::BoolLiteral(_) | ExprKind::Not(_) => PhpType::Bool,
        ExprKind::StringLiteral(_) => PhpType::Str,
        ExprKind::Null => PhpType::Void,
        ExprKind::ArrayLiteral(values) => {
            PhpType::Array(Box::new(source_array_value_type(values.iter())))
        }
        ExprKind::ArrayLiteralAssoc(entries) => PhpType::AssocArray {
            key: Box::new(source_array_value_type(entries.iter().map(|(key, _)| key))),
            value: Box::new(source_array_value_type(entries.iter().map(|(_, value)| value))),
        },
        ExprKind::ArrayLiteralMixed(entries) => PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(source_array_value_type(entries.iter().map(|entry| match entry {
                ArrayEntry::Spread(source)
                | ArrayEntry::Value(source)
                | ArrayEntry::Reference(source) => source,
                ArrayEntry::Keyed(_, value) | ArrayEntry::KeyReference(_, value) => value,
            }))),
        },
        ExprKind::UnaryPlus(inner) | ExprKind::Negate(inner) => match source_expr_type(inner)? {
            value @ (PhpType::Int | PhpType::Float) => value,
            _ => return None,
        },
        ExprKind::Cast { target, .. } => match target {
            CastType::Int => PhpType::Int,
            CastType::Float => PhpType::Float,
            CastType::Bool => PhpType::Bool,
            CastType::String | CastType::StringFormat => PhpType::Str,
            CastType::Array => PhpType::php_array(),
            CastType::Object => PhpType::Object(String::new()),
        },
        _ => return None,
    })
}

/// Merges literal element categories without inventing facts for unknown or heterogeneous values.
fn source_array_value_type<'a>(values: impl Iterator<Item = &'a Expr>) -> PhpType {
    values.fold(PhpType::Never, |current, value| {
        let next = source_expr_type(value).unwrap_or(PhpType::Mixed);
        match (current, next) {
            (PhpType::Never, next) => next,
            (current, PhpType::Never) => current,
            (current, next) if current == next => current,
            _ => PhpType::Mixed,
        }
    })
}
