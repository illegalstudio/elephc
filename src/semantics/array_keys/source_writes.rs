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
use elephc_semantics::{array_keys::StringKey, SemanticProfile};

/// One exact literal key retained by a source array shape.
#[derive(Clone, Debug, PartialEq, Eq)]
enum ArrayKeyFact {
    Integer(i64),
    String(String),
}

/// Ordered literal slots whose absence is known while the shape remains complete.
#[derive(Clone, Debug, PartialEq)]
struct ArrayFact {
    entries: Vec<(ArrayKeyFact, ValueFact)>,
    maximum_integer: Option<i64>,
}

impl ArrayFact {
    /// Creates a complete empty literal shape.
    fn empty() -> Self {
        Self {
            entries: Vec::new(),
            maximum_integer: None,
        }
    }

    /// Stores one explicit key, preserving its insertion position across overwrites.
    fn insert(&mut self, key: ArrayKeyFact, value: ValueFact) {
        if let Some((_, stored)) = self.entries.iter_mut().find(|(stored, _)| *stored == key) {
            *stored = value;
            return;
        }
        match key {
            ArrayKeyFact::Integer(integer) => {
                self.maximum_integer = Some(
                    self.maximum_integer
                        .map_or(integer, |maximum| maximum.max(integer)),
                );
                self.entries.push((ArrayKeyFact::Integer(integer), value));
            }
            ArrayKeyFact::String(key) => {
                self.entries.push((ArrayKeyFact::String(key), value));
            }
        }
    }

    /// Appends one implicit value when version-independent integer-key history is known.
    fn append(&mut self, value: ValueFact) -> Option<()> {
        let key = match self.maximum_integer {
            None => 0,
            Some(maximum) if maximum >= 0 => maximum.checked_add(1)?,
            Some(_) => return None,
        };
        self.insert(ArrayKeyFact::Integer(key), value);
        Some(())
    }

    /// Appends a static spread, retaining string keys and renumbering integer keys.
    fn spread(&mut self, source: &Self) -> Option<()> {
        for (key, value) in &source.entries {
            match key {
                ArrayKeyFact::Integer(_) => self.append(value.clone())?,
                ArrayKeyFact::String(key) => {
                    self.insert(ArrayKeyFact::String(key.clone()), value.clone());
                }
            }
        }
        Some(())
    }
}

/// One local receiver fact retained by the syntax-preserving write audit.
#[derive(Clone, Debug, PartialEq)]
enum ValueFact {
    Known(PhpType),
    Integer(i64),
    String(String),
    Array(ArrayFact),
    Unknown,
}

impl ValueFact {
    /// Returns the PHP category represented by this fact without discarding literal identity.
    fn value_type(&self) -> Option<PhpType> {
        match self {
            Self::Known(ty) => Some(ty.clone()),
            Self::Integer(_) => Some(PhpType::Int),
            Self::String(_) => Some(PhpType::Str),
            Self::Array(_) => Some(PhpType::php_array()),
            Self::Unknown => None,
        }
    }

    /// Returns an exact scalar key retained by a literal or assigned constant.
    fn array_key(&self) -> Option<ArrayKeyFact> {
        match self {
            Self::Integer(value) => Some(ArrayKeyFact::Integer(*value)),
            Self::String(value) => Some(ArrayKeyFact::String(value.clone())),
            _ => None,
        }
    }

    /// Reports whether every represented value becomes an array parent for a nested write.
    fn is_write_array_parent(&self) -> bool {
        match self {
            Self::Array(_) | Self::Known(PhpType::Void | PhpType::False) => true,
            Self::Known(ty) => definite_array_type(ty),
            _ => false,
        }
    }
}

impl ArrayFact {
    /// Projects one slot, distinguishing a plain read from a nested write parent.
    fn element(&self, index: &ValueFact, write_parent: bool) -> ValueFact {
        if let Some(key) = index.array_key() {
            return self
                .entries
                .iter()
                .find(|(stored, _)| *stored == key)
                .map(|(_, value)| write_parent_fact(value.clone(), write_parent))
                .unwrap_or_else(|| missing_element(write_parent));
        }
        let Some(index_type) = index.value_type() else {
            return ValueFact::Unknown;
        };
        if !matches!(index_type, PhpType::Int | PhpType::Str) {
            return ValueFact::Unknown;
        }
        let candidates: Vec<_> = self
            .entries
            .iter()
            .filter(|(key, _)| {
                matches!(
                    (key, &index_type),
                    (ArrayKeyFact::Integer(_), PhpType::Int)
                        | (ArrayKeyFact::String(_), PhpType::Str)
                )
            })
            .map(|(_, value)| value)
            .collect();
        if candidates.is_empty() {
            return missing_element(write_parent);
        }
        if write_parent && candidates.iter().all(|value| value.is_write_array_parent()) {
            return ValueFact::Known(PhpType::php_array());
        }
        ValueFact::Unknown
    }
}

/// Returns the missing-slot fact appropriate to a read or an autovivifying nested write.
fn missing_element(write_parent: bool) -> ValueFact {
    if write_parent {
        ValueFact::Array(ArrayFact::empty())
    } else {
        ValueFact::Known(PhpType::Void)
    }
}

/// Promotes null and false only when PHP uses the value as a nested write parent.
fn write_parent_fact(fact: ValueFact, write_parent: bool) -> ValueFact {
    if write_parent && matches!(&fact, ValueFact::Known(PhpType::Void | PhpType::False)) {
        ValueFact::Array(ArrayFact::empty())
    } else {
        fact
    }
}

/// Recognizes an array-only type without treating `Mixed` or object dimensions as arrays.
fn definite_array_type(ty: &PhpType) -> bool {
    match ty {
        PhpType::Array(_) | PhpType::AssocArray { .. } => true,
        PhpType::Union(members) => {
            !members.is_empty() && members.iter().all(definite_array_type)
        }
        _ => false,
    }
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
                self.profile,
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
    profile: SemanticProfile,
    current_class: Option<&str>,
) -> LocalFacts {
    let mut locals = entry.clone();
    apply_prelude_facts(statements, &mut locals, properties, profile, current_class);
    locals
}

/// Records only local facts needed to type stabilized assignment targets.
fn apply_prelude_facts(
    statements: &[Stmt],
    locals: &mut LocalFacts,
    properties: &PropertyFacts,
    profile: SemanticProfile,
    current_class: Option<&str>,
) {
    for stmt in statements {
        match &stmt.kind {
            StmtKind::Assign { name, value } => {
                let fact = infer_expr(value, locals, properties, profile, current_class);
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
                let fact = infer_expr(init, locals, properties, profile, current_class);
                locals.set(name, fact);
            }
            StmtKind::ListUnpack { vars, .. } => {
                for name in vars {
                    locals.set(name, ValueFact::Unknown);
                }
            }
            StmtKind::Synthetic(body) => {
                apply_prelude_facts(body, locals, properties, profile, current_class);
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
            locals.set(
                name,
                infer_expr(value, locals, properties, profile, current_class),
            );
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
            locals.set(
                name,
                infer_expr(init, locals, properties, profile, current_class),
            );
        }
        StmtKind::ListUnpack { vars, .. } => {
            for name in vars {
                locals.set(name, ValueFact::Unknown);
            }
        }
        StmtKind::ArrayAssign {
            array,
            index,
            value,
        } => {
            let receiver = locals.get(array);
            validate_index(
                profile,
                &receiver,
                index,
                locals,
                properties,
                current_class,
            )?;
            let index = infer_expr(index, locals, properties, profile, current_class);
            let value = infer_expr(value, locals, properties, profile, current_class);
            locals.set(array, array_write_fact(receiver, index, value));
        }
        StmtKind::ArrayPush { array, value } => {
            let receiver = locals.get(array);
            let value = infer_expr(value, locals, properties, profile, current_class);
            locals.set(array, array_push_fact(receiver, value));
        }
        StmtKind::NestedArrayAssign { target, value } => {
            validate_array_access_chain(
                profile,
                target,
                locals,
                properties,
                current_class,
            )?;
            let mut indices = Vec::new();
            if let Some(array) = local_array_target(target, &mut indices) {
                let indices: Vec<_> = indices
                    .into_iter()
                    .map(|index| infer_expr(index, locals, properties, profile, current_class))
                    .collect();
                let value = infer_expr(value, locals, properties, profile, current_class);
                locals.set(
                    array,
                    nested_array_write_fact(locals.get(array), &indices, value),
                );
            }
        }
        StmtKind::PropertyArrayAssign {
            object,
            property,
            index,
            ..
        } => {
            let receiver = property_fact(
                infer_expr(object, locals, properties, profile, current_class),
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
        StmtKind::While { body, .. } => {
            let entry = locals.clone();
            let exit = validate_branch(body, &entry, properties, profile, current_class)?;
            *locals = merge_exits(&[entry, exit]);
        }
        StmtKind::Foreach {
            key_var,
            value_var,
            body,
            ..
        } => {
            let entry = locals.clone();
            let mut iteration = entry.clone();
            if let Some(key) = key_var {
                iteration.set(key, ValueFact::Unknown);
            }
            iteration.set(value_var, ValueFact::Unknown);
            validate_block(body, &mut iteration, properties, profile, current_class)?;
            *locals = merge_exits(&[entry, iteration]);
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
    let Some(receiver) = receiver.value_type() else {
        return Ok(());
    };
    let Some(key) = infer_expr(index, locals, properties, profile, current_class).value_type() else {
        return Ok(());
    };
    super::validate_write_receiver(profile, &receiver, &key, index.span)
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
        let receiver = infer_write_receiver(array, locals, properties, profile, current_class);
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
    profile: SemanticProfile,
    current_class: Option<&str>,
) -> ValueFact {
    if let Some(fact) = source_expr_fact(expr, profile) {
        return fact;
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
        ExprKind::Clone(inner) => infer_expr(inner, locals, properties, profile, current_class),
        ExprKind::PropertyAccess { object, property } => property_fact(
            infer_expr(object, locals, properties, profile, current_class),
            property,
            properties,
        ),
        ExprKind::ArrayAccess { array, index } => array_element_fact(
            infer_expr(array, locals, properties, profile, current_class),
            infer_expr(index, locals, properties, profile, current_class),
            false,
        ),
        ExprKind::StaticPropertyAccess { receiver, property } => {
            static_property_fact(receiver, property, properties, current_class)
        }
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => merge_value_facts(
            infer_expr(then_expr, locals, properties, profile, current_class),
            infer_expr(else_expr, locals, properties, profile, current_class),
        ),
        ExprKind::NullCoalesce { value, default } => merge_value_facts(
            infer_expr(value, locals, properties, profile, current_class),
            infer_expr(default, locals, properties, profile, current_class),
        ),
        _ => ValueFact::Unknown,
    }
}

/// Infers a receiver that is itself an intermediate parent of a nested write.
fn infer_write_receiver(
    expr: &Expr,
    locals: &LocalFacts,
    properties: &PropertyFacts,
    profile: SemanticProfile,
    current_class: Option<&str>,
) -> ValueFact {
    let ExprKind::ArrayAccess { array, index } = &expr.kind else {
        return infer_expr(expr, locals, properties, profile, current_class);
    };
    array_element_fact(
        infer_write_receiver(array, locals, properties, profile, current_class),
        infer_expr(index, locals, properties, profile, current_class),
        true,
    )
}

/// Projects an element category while distinguishing reads from autovivifying write parents.
fn array_element_fact(receiver: ValueFact, index: ValueFact, write_parent: bool) -> ValueFact {
    match (receiver, index) {
        (ValueFact::Array(array), index) => array.element(&index, write_parent),
        (ValueFact::Known(PhpType::Void | PhpType::False), _) if write_parent => {
            ValueFact::Array(ArrayFact::empty())
        }
        (ValueFact::Known(PhpType::Array(element)), index)
            if index.value_type() == Some(PhpType::Int) =>
        {
            if *element == PhpType::Never {
                missing_element(write_parent)
            } else {
                write_parent_fact(ValueFact::Known(*element), write_parent)
            }
        }
        (
            ValueFact::Known(PhpType::AssocArray { key, value }),
            index,
        ) if index.value_type().is_some_and(|actual| *key == actual) => {
            if *value == PhpType::Never {
                missing_element(write_parent)
            } else {
                write_parent_fact(ValueFact::Known(*value), write_parent)
            }
        }
        _ => ValueFact::Unknown,
    }
}

/// Updates a local literal shape after a direct keyed write without guessing a dynamic slot.
fn array_write_fact(receiver: ValueFact, index: ValueFact, value: ValueFact) -> ValueFact {
    let mut array = match receiver {
        ValueFact::Array(array) => array,
        ValueFact::Known(PhpType::Void | PhpType::False) => ArrayFact::empty(),
        other => return other,
    };
    let Some(key) = index.array_key() else {
        return ValueFact::Known(PhpType::php_array());
    };
    array.insert(key, value);
    ValueFact::Array(array)
}

/// Updates a local literal shape after an append while retaining a static next key.
fn array_push_fact(receiver: ValueFact, value: ValueFact) -> ValueFact {
    let mut array = match receiver {
        ValueFact::Array(array) => array,
        ValueFact::Known(PhpType::Void | PhpType::False) => ArrayFact::empty(),
        other => return other,
    };
    if array.append(value).is_some() {
        ValueFact::Array(array)
    } else {
        ValueFact::Known(PhpType::php_array())
    }
}

/// Updates a local shape through an exact nested path, degrading dynamic paths conservatively.
fn nested_array_write_fact(
    receiver: ValueFact,
    indices: &[ValueFact],
    value: ValueFact,
) -> ValueFact {
    let Some((index, rest)) = indices.split_first() else {
        return value;
    };
    if rest.is_empty() {
        return array_write_fact(receiver, index.clone(), value);
    }
    let child = array_element_fact(receiver.clone(), index.clone(), true);
    let child = nested_array_write_fact(child, rest, value);
    array_write_fact(receiver, index.clone(), child)
}

/// Returns a plain local root and its array-access indices in source order.
fn local_array_target<'a>(target: &'a Expr, indices: &mut Vec<&'a Expr>) -> Option<&'a str> {
    match &target.kind {
        ExprKind::Variable(name) => Some(name),
        ExprKind::ArrayAccess { array, index } => {
            let root = local_array_target(array, indices)?;
            indices.push(index);
            Some(root)
        }
        _ => None,
    }
}

/// Returns a property contract only when its object class is statically established.
fn property_fact(
    object: ValueFact,
    property: &str,
    properties: &PropertyFacts,
) -> ValueFact {
    let Some(PhpType::Object(class)) = object.value_type() else {
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
    } else if let (Some(left), Some(right)) = (left.value_type(), right.value_type()) {
        if left == right {
            ValueFact::Known(left)
        } else {
            ValueFact::Unknown
        }
    } else {
        ValueFact::Unknown
    }
}

/// Projects literal identity and explicit-cast categories used by source-only key validation.
fn source_expr_fact(expr: &Expr, profile: SemanticProfile) -> Option<ValueFact> {
    Some(match &expr.kind {
        ExprKind::IntLiteral(value) => ValueFact::Integer(*value),
        ExprKind::FloatLiteral(_) => ValueFact::Known(PhpType::Float),
        ExprKind::BoolLiteral(false) => ValueFact::Known(PhpType::False),
        ExprKind::BoolLiteral(true) | ExprKind::Not(_) => ValueFact::Known(PhpType::Bool),
        ExprKind::StringLiteral(value) => ValueFact::String(value.clone()),
        ExprKind::Null => ValueFact::Known(PhpType::Void),
        ExprKind::ArrayLiteral(values) => source_indexed_array_fact(values, profile),
        ExprKind::ArrayLiteralAssoc(entries) => source_assoc_array_fact(entries, profile),
        ExprKind::ArrayLiteralMixed(entries) => source_mixed_array_fact(entries, profile),
        ExprKind::Spread(inner) => source_expr_fact(inner, profile)?,
        ExprKind::UnaryPlus(inner) => match source_expr_fact(inner, profile)? {
            value @ (ValueFact::Integer(_) | ValueFact::Known(PhpType::Float)) => value,
            _ => return None,
        },
        ExprKind::Negate(inner) => match source_expr_fact(inner, profile)? {
            ValueFact::Integer(value) => ValueFact::Integer(value.checked_neg()?),
            ValueFact::Known(PhpType::Float) => ValueFact::Known(PhpType::Float),
            _ => return None,
        },
        ExprKind::Cast { target, .. } => ValueFact::Known(match target {
            CastType::Int => PhpType::Int,
            CastType::Float => PhpType::Float,
            CastType::Bool => PhpType::Bool,
            CastType::String | CastType::StringFormat => PhpType::Str,
            CastType::Array => PhpType::php_array(),
            CastType::Object => PhpType::Object(String::new()),
        }),
        _ => return None,
    })
}

/// Builds a complete indexed literal shape, expanding only literal array spreads.
fn source_indexed_array_fact(values: &[Expr], profile: SemanticProfile) -> ValueFact {
    let mut array = ArrayFact::empty();
    for value in values {
        if let ExprKind::Spread(source) = &value.kind {
            let Some(ValueFact::Array(source)) = source_expr_fact(source, profile) else {
                return ValueFact::Known(PhpType::php_array());
            };
            if array.spread(&source).is_none() {
                return ValueFact::Known(PhpType::php_array());
            }
        } else if array
            .append(source_expr_fact(value, profile).unwrap_or(ValueFact::Unknown))
            .is_none()
        {
            return ValueFact::Known(PhpType::php_array());
        }
    }
    ValueFact::Array(array)
}

/// Builds a complete explicitly keyed literal shape under the selected exact-key policy.
fn source_assoc_array_fact(entries: &[(Expr, Expr)], profile: SemanticProfile) -> ValueFact {
    let mut array = ArrayFact::empty();
    for (key, value) in entries {
        let Some(key) = source_array_key(key, profile) else {
            return ValueFact::Known(PhpType::php_array());
        };
        array.insert(
            key,
            source_expr_fact(value, profile).unwrap_or(ValueFact::Unknown),
        );
    }
    ValueFact::Array(array)
}

/// Builds an ordered mixed literal shape when every spread and key is static.
fn source_mixed_array_fact(entries: &[ArrayEntry], profile: SemanticProfile) -> ValueFact {
    let mut array = ArrayFact::empty();
    for entry in entries {
        let success = match entry {
            ArrayEntry::Spread(source) => {
                let source = match &source.kind {
                    ExprKind::Spread(source) => source.as_ref(),
                    _ => source,
                };
                let Some(ValueFact::Array(source)) = source_expr_fact(source, profile) else {
                    return ValueFact::Known(PhpType::php_array());
                };
                array.spread(&source)
            }
            ArrayEntry::Value(value) | ArrayEntry::Reference(value) => {
                array.append(source_expr_fact(value, profile).unwrap_or(ValueFact::Unknown))
            }
            ArrayEntry::Keyed(key, value) | ArrayEntry::KeyReference(key, value) => {
                let Some(key) = source_array_key(key, profile) else {
                    return ValueFact::Known(PhpType::php_array());
                };
                array.insert(
                    key,
                    source_expr_fact(value, profile).unwrap_or(ValueFact::Unknown),
                );
                Some(())
            }
        };
        if success.is_none() {
            return ValueFact::Known(PhpType::php_array());
        }
    }
    ValueFact::Array(array)
}

/// Converts one static key through the shared literal-array key classifier.
/// Under exact keys, string `"0"` deliberately remains distinct from integer `0`; an integer
/// lookup is then a known missing slot whose nested write still receives array-key validation.
fn source_array_key(expr: &Expr, profile: SemanticProfile) -> Option<ArrayKeyFact> {
    match crate::types::literal_array_key(expr, profile)? {
        StringKey::Integer(value) => Some(ArrayKeyFact::Integer(value)),
        StringKey::String(value) => Some(ArrayKeyFact::String(
            String::from_utf8_lossy(value).into_owned(),
        )),
    }
}
