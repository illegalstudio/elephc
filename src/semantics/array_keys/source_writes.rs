//! Purpose:
//! Preserves definite array receiver facts while auditing writes before target pruning.
//!
//! Called from:
//! - `crate::semantics::validation::validate_source` under `--strict-array-keys`.
//!
//! Key details:
//! - Flow facts are deliberately small and conservative: unknown receivers remain checker work.
//! - Branches merge only conservative common facts, including autovivifying write-parent shapes.
//! - Declared array properties are checked without treating strings or ArrayAccess objects as arrays.

use std::cell::Cell;
use std::collections::{BTreeSet, HashMap};

use crate::ast_visit::{self, AstVisitor, SourcePolicy};
use crate::errors::CompileError;
use crate::parser::ast::{
    ArrayEntry, CastType, CatchClause, ClassMethod, Expr, ExprKind, Program, StaticReceiver, Stmt,
    StmtKind, TypeExpr,
};
use crate::types::PhpType;
use elephc_semantics::{array_keys::StringKey, SemanticProfile};

thread_local! {
    /// Prevents repeated loop-entry refinement from multiplying across nested loop bodies.
    static LOOP_REFINEMENT_DEPTH: Cell<u8> = const { Cell::new(0) };
}

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

    /// Appends one implicit value while preserving PHP 8.5 next-key history.
    fn append(&mut self, value: ValueFact) -> Option<()> {
        let key = match self.maximum_integer {
            None => 0,
            Some(i64::MAX) => return Some(()),
            Some(maximum) => maximum + 1,
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

    /// Replaces every retained element with one value written through a foreach reference.
    fn fill_values(&mut self, value: &ValueFact) {
        for (_, stored) in &mut self.entries {
            *stored = value.clone();
        }
    }

    /// Replaces the last insertion-order element retained by a completed reference foreach.
    fn replace_last(&mut self, value: ValueFact) {
        if let Some((_, stored)) = self.entries.last_mut() {
            *stored = value;
        }
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
            return candidates
                .into_iter()
                .cloned()
                .reduce(merge_write_parent_facts)
                .unwrap_or(ValueFact::Unknown);
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
#[derive(Clone, Debug, Default, PartialEq)]
struct LocalFacts {
    values: HashMap<String, ValueFact>,
    foreach_references: HashMap<String, ForeachReference>,
    ordinary_references: BTreeSet<String>,
}

/// One lingering reference left by a by-reference foreach value variable.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ForeachReference {
    source: String,
    exact_last: bool,
    definite: bool,
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
        let fact = if self.ordinary_references.contains(name) {
            ValueFact::Unknown
        } else {
            fact
        };
        self.values.insert(name.to_owned(), fact);
    }

    /// Records a source assignment, including writes through a lingering foreach reference.
    fn assign(&mut self, name: &str, fact: ValueFact) {
        if self.ordinary_references.contains(name) {
            self.foreach_references.remove(name);
            self.foreach_references
                .retain(|_, reference| reference.source != name);
            self.set(name, ValueFact::Unknown);
            return;
        }
        if let Some(reference) = self.foreach_references.get(name).cloned() {
            let original = self.get(&reference.source);
            let updated = match original.clone() {
                ValueFact::Array(mut array) if reference.exact_last => {
                    array.replace_last(fact.clone());
                    ValueFact::Array(array)
                }
                ValueFact::Array(_) => ValueFact::Known(PhpType::php_array()),
                source => source,
            };
            let source_fact = if reference.definite {
                updated
            } else {
                merge_value_facts(original, updated)
            };
            self.set(&reference.source, source_fact);
        } else {
            self.foreach_references
                .retain(|_, reference| reference.source != name);
        }
        self.set(name, fact);
    }

    /// Rebinds a local name without storing through the reference it previously held.
    fn rebind(&mut self, name: &str, fact: ValueFact) {
        self.foreach_references.remove(name);
        self.ordinary_references.remove(name);
        self.set(name, fact);
    }

    /// Marks an ordinary PHP reference pair as unknown until either name is rebound.
    fn bind_ordinary_reference(&mut self, target: &str, source: &str) {
        self.foreach_references.remove(target);
        if let Some(reference_source) = self
            .foreach_references
            .get(source)
            .map(|reference| reference.source.clone())
        {
            self.set(&reference_source, ValueFact::Known(PhpType::php_array()));
        }
        self.ordinary_references.insert(target.to_owned());
        self.ordinary_references.insert(source.to_owned());
        self.set(target, ValueFact::Unknown);
        self.set(source, ValueFact::Unknown);
    }

    /// Remembers that PHP leaves the foreach value variable bound to its last element.
    fn bind_foreach_reference(&mut self, alias: &str, source: &str, exact_last: bool) {
        self.foreach_references.insert(
            alias.to_owned(),
            ForeachReference {
                source: source.to_owned(),
                exact_last,
                definite: true,
            },
        );
    }
}

/// Declared property types keyed by their canonical source class and property names.
#[derive(Default)]
struct PropertyFacts {
    values: HashMap<(String, String), PhpType>,
    parents: HashMap<String, String>,
}

/// Describes where the initial scope obtains its process argument bindings.
pub(crate) enum SourceWriteScope {
    Program,
    LiteralEval,
}

/// Audits definite array writes before target-dependent folding can discard their branches.
pub(crate) fn validate_source_writes(
    program: &Program,
    profile: SemanticProfile,
    scope: SourceWriteScope,
) -> Result<(), CompileError> {
    crate::optimize::with_source_fold_user_functions(program, || {
        let mut properties = PropertyFacts::default();
        collect_properties(program, &mut properties);
        validate_nested_closures(program, &properties, profile)?;
        let mut locals = LocalFacts::default();
        for (name, ty) in crate::superglobals::process_argument_types() {
            // Eval inherits the caller's bindings, which may shadow either name.
            let fact = match scope {
                SourceWriteScope::Program => ValueFact::Known(ty),
                SourceWriteScope::LiteralEval => ValueFact::Unknown,
            };
            locals.set(name, fact);
        }
        validate_block(program, &mut locals, &properties, profile, None)
    })
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
            None,
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
        locals: locals.clone(),
        properties,
        profile,
        current_class,
        source: SourcePolicy::default(),
        statement_depth: 0,
        error: None,
        assignment_values: HashMap::new(),
    };
    ast_visit::walk_statements(std::slice::from_ref(stmt), &mut visitor);
    visitor.error.map_or(Ok(()), Err)
}

/// Audits one expression and returns facts after its source-order evaluation.
fn validate_expression_flow(
    expr: &Expr,
    locals: &LocalFacts,
    properties: &PropertyFacts,
    profile: SemanticProfile,
    current_class: Option<&str>,
) -> Result<LocalFacts, CompileError> {
    let mut visitor = ExpressionWriteValidator {
        locals: locals.clone(),
        properties,
        profile,
        current_class,
        source: SourcePolicy::default(),
        statement_depth: 1,
        error: None,
        assignment_values: HashMap::new(),
    };
    ast_visit::walk_expression(expr, &mut visitor);
    match visitor.error {
        Some(error) => Err(error),
        None => Ok(visitor.locals),
    }
}

/// Resolves literal truthiness after source-order effects without pruning evaluated entries.
fn source_literal_truthiness(expr: &Expr) -> Option<bool> {
    match &expr.kind {
        ExprKind::ArrayLiteral(values) => source_indexed_array_nonempty(values),
        ExprKind::ArrayLiteralAssoc(entries) => Some(!entries.is_empty()),
        ExprKind::ArrayLiteralMixed(entries) => source_mixed_array_nonempty(entries),
        ExprKind::Assignment { value, .. } => source_literal_truthiness(value),
        ExprKind::BinaryOp { left, op, right }
            if matches!(op, crate::parser::ast::BinOp::And) =>
        {
            match source_literal_truthiness(left) {
                Some(false) => Some(false),
                Some(true) => source_literal_truthiness(right),
                None if source_literal_truthiness(right) == Some(false) => Some(false),
                None => None,
            }
        }
        ExprKind::BinaryOp { left, op, right }
            if matches!(op, crate::parser::ast::BinOp::Or) =>
        {
            match source_literal_truthiness(left) {
                Some(true) => Some(true),
                Some(false) => source_literal_truthiness(right),
                None if source_literal_truthiness(right) == Some(true) => Some(true),
                None => None,
            }
        }
        ExprKind::Not(inner) => source_literal_truthiness(inner).map(|truthy| !truthy),
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => match source_literal_truthiness(condition)? {
            true => source_literal_truthiness(then_expr),
            false => source_literal_truthiness(else_expr),
        },
        ExprKind::ShortTernary { value, default } => {
            match source_literal_truthiness(value)? {
                true => Some(true),
                false => source_literal_truthiness(default),
            }
        }
        ExprKind::NullCoalesce { value, default } => match source_literal_nullness(value)? {
            true => source_literal_truthiness(default),
            false => source_literal_truthiness(value),
        },
        _ => crate::types::deferred_defaults::literal_truthiness(expr),
    }
}

/// Resolves whether a source literal or assignment result is definitely null.
fn source_literal_nullness(expr: &Expr) -> Option<bool> {
    match &expr.kind {
        ExprKind::Null => Some(true),
        ExprKind::BoolLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::ArrayLiteral(_)
        | ExprKind::ArrayLiteralAssoc(_)
        | ExprKind::ArrayLiteralMixed(_) => Some(false),
        ExprKind::Assignment { value, .. } => source_literal_nullness(value),
        ExprKind::BinaryOp { op, .. }
            if matches!(
                op,
                crate::parser::ast::BinOp::And | crate::parser::ast::BinOp::Or
            ) =>
        {
            Some(false)
        }
        ExprKind::Not(_) => Some(false),
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => match source_literal_truthiness(condition)? {
            true => source_literal_nullness(then_expr),
            false => source_literal_nullness(else_expr),
        },
        ExprKind::ShortTernary { value, default } => {
            match source_literal_truthiness(value)? {
                true => Some(false),
                false => source_literal_nullness(default),
            }
        }
        ExprKind::NullCoalesce { value, default } => match source_literal_nullness(value)? {
            true => source_literal_nullness(default),
            false => Some(false),
        },
        _ => None,
    }
}

/// Selects the expression PHP returns from a statically decided lazy value.
fn statically_selected_source_expr(expr: &Expr) -> Option<&Expr> {
    match &expr.kind {
        ExprKind::Assignment { value, .. } => statically_selected_source_expr(value),
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => match source_literal_truthiness(condition)? {
            true => statically_selected_source_expr(then_expr),
            false => statically_selected_source_expr(else_expr),
        },
        ExprKind::ShortTernary { value, default } => match source_literal_truthiness(value)? {
            true => statically_selected_source_expr(value),
            false => statically_selected_source_expr(default),
        },
        ExprKind::NullCoalesce { value, default } => match source_literal_nullness(value)? {
            true => statically_selected_source_expr(default),
            false => statically_selected_source_expr(value),
        },
        _ => Some(expr),
    }
}

/// Selects nested lazy and match keys without inspecting an unselected match result.
fn statically_selected_array_key_expr(
    expr: &Expr,
    profile: SemanticProfile,
) -> Option<Expr> {
    match &expr.kind {
        ExprKind::Assignment { value, .. } => {
            statically_selected_array_key_expr(value, profile)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => match source_literal_truthiness(condition)? {
            true => statically_selected_array_key_expr(then_expr, profile),
            false => statically_selected_array_key_expr(else_expr, profile),
        },
        ExprKind::ShortTernary { value, default } => match source_literal_truthiness(value)? {
            true => statically_selected_array_key_expr(value, profile),
            false => statically_selected_array_key_expr(default, profile),
        },
        ExprKind::NullCoalesce { value, default } => match source_literal_nullness(value)? {
            true => statically_selected_array_key_expr(default, profile),
            false => statically_selected_array_key_expr(value, profile),
        },
        ExprKind::Match { .. } => {
            let selected = crate::optimize::select_match_array_key_for_source(expr, profile)?;
            statically_selected_array_key_expr(&selected, profile)
        }
        _ => Some(expr.clone()),
    }
}

/// Proves whether an indexed literal contributes an element after expanding its spreads.
fn source_indexed_array_nonempty(values: &[Expr]) -> Option<bool> {
    let mut unknown_spread = false;
    for value in values {
        let ExprKind::Spread(source) = &value.kind else { return Some(true); };
        match source_array_literal_nonempty(source) {
            Some(true) => return Some(true),
            Some(false) => {}
            None => unknown_spread = true,
        }
    }
    (!unknown_spread).then_some(false)
}

/// Proves whether an ordered mixed literal contributes an element after expanding its spreads.
fn source_mixed_array_nonempty(entries: &[ArrayEntry]) -> Option<bool> {
    let mut unknown_spread = false;
    for entry in entries {
        let ArrayEntry::Spread(source) = entry else { return Some(true); };
        match source_array_literal_nonempty(source) {
            Some(true) => return Some(true),
            Some(false) => {}
            None => unknown_spread = true,
        }
    }
    (!unknown_spread).then_some(false)
}

/// Restricts spread cardinality proofs to literal arrays.
fn source_array_literal_nonempty(expr: &Expr) -> Option<bool> {
    match &expr.kind {
        ExprKind::ArrayLiteral(values) => source_indexed_array_nonempty(values),
        ExprKind::ArrayLiteralAssoc(entries) => Some(!entries.is_empty()),
        ExprKind::ArrayLiteralMixed(entries) => source_mixed_array_nonempty(entries),
        _ => None,
    }
}

/// Resolves foreach cardinality from either literal syntax or an exact local array fact.
fn source_array_nonempty(
    expr: &Expr,
    locals: &LocalFacts,
    properties: &PropertyFacts,
    profile: SemanticProfile,
    current_class: Option<&str>,
) -> Option<bool> {
    source_array_literal_nonempty(expr).or_else(|| {
        let ValueFact::Array(array) =
            infer_expr(expr, locals, properties, profile, current_class)
        else {
            return None;
        };
        Some(!array.entries.is_empty())
    })
}

/// Returns the shared element fact for one exact foreach source shape.
fn source_array_iteration_value(
    expr: &Expr,
    locals: &LocalFacts,
    properties: &PropertyFacts,
    profile: SemanticProfile,
    current_class: Option<&str>,
) -> ValueFact {
    let ValueFact::Array(array) = infer_expr(expr, locals, properties, profile, current_class)
    else {
        return ValueFact::Unknown;
    };
    array
        .entries
        .into_iter()
        .map(|(_, value)| value)
        .reduce(merge_value_facts)
        .unwrap_or(ValueFact::Unknown)
}

/// Facts produced by the two possible exits from one condition evaluation.
struct ConditionFlow {
    when_true: LocalFacts,
    when_false: LocalFacts,
    true_reachable: bool,
    false_reachable: bool,
}

/// Audits one condition once and returns both of its possible source-order exits.
fn validate_condition_flow(
    expr: &Expr,
    locals: &LocalFacts,
    properties: &PropertyFacts,
    profile: SemanticProfile,
    current_class: Option<&str>,
) -> Result<ConditionFlow, CompileError> {
    match &expr.kind {
        ExprKind::BinaryOp { left, op, right }
            if matches!(op, crate::parser::ast::BinOp::And) =>
        {
            let left_flow = validate_condition_flow(
                left,
                locals,
                properties,
                profile,
                current_class,
            )?;
            let right_flow = validate_condition_flow(
                right,
                &left_flow.when_true,
                properties,
                profile,
                current_class,
            )?;
            let true_reachable = left_flow.true_reachable && right_flow.true_reachable;
            let false_from_left = left_flow.false_reachable;
            let false_from_right = left_flow.true_reachable && right_flow.false_reachable;
            let when_true = if true_reachable {
                right_flow.when_true
            } else {
                left_flow.when_true.clone()
            };
            let when_false = merge_reachable_condition_exits(
                &[
                    (false_from_left, &left_flow.when_false),
                    (false_from_right, &right_flow.when_false),
                ],
                &left_flow.when_false,
            );
            Ok(ConditionFlow {
                when_true,
                when_false,
                true_reachable,
                false_reachable: false_from_left || false_from_right,
            })
        }
        ExprKind::BinaryOp { left, op, right }
            if matches!(op, crate::parser::ast::BinOp::Or) =>
        {
            let left_flow = validate_condition_flow(
                left,
                locals,
                properties,
                profile,
                current_class,
            )?;
            let right_flow = validate_condition_flow(
                right,
                &left_flow.when_false,
                properties,
                profile,
                current_class,
            )?;
            let true_from_left = left_flow.true_reachable;
            let true_from_right = left_flow.false_reachable && right_flow.true_reachable;
            let when_true = merge_reachable_condition_exits(
                &[
                    (true_from_left, &left_flow.when_true),
                    (true_from_right, &right_flow.when_true),
                ],
                &left_flow.when_true,
            );
            let false_reachable = left_flow.false_reachable && right_flow.false_reachable;
            let when_false = if false_reachable {
                right_flow.when_false
            } else {
                left_flow.when_false.clone()
            };
            Ok(ConditionFlow {
                when_true,
                when_false,
                true_reachable: true_from_left || true_from_right,
                false_reachable,
            })
        }
        ExprKind::Not(inner) => {
            let inner = validate_condition_flow(
                inner,
                locals,
                properties,
                profile,
                current_class,
            )?;
            Ok(ConditionFlow {
                when_true: inner.when_false,
                when_false: inner.when_true,
                true_reachable: inner.false_reachable,
                false_reachable: inner.true_reachable,
            })
        }
        _ => {
            let exit = validate_expression_flow(
                expr,
                locals,
                properties,
                profile,
                current_class,
            )?;
            let truthiness = source_literal_truthiness(expr);
            Ok(ConditionFlow {
                when_true: exit.clone(),
                when_false: exit,
                true_reachable: truthiness != Some(false),
                false_reachable: truthiness != Some(true),
            })
        }
    }
}

/// Merges only condition exits PHP can reach while retaining a snapshot for source auditing.
fn merge_reachable_condition_exits(
    exits: &[(bool, &LocalFacts)],
    fallback: &LocalFacts,
) -> LocalFacts {
    let reachable: Vec<_> = exits
        .iter()
        .filter(|(reachable, _)| *reachable)
        .map(|(_, facts)| (*facts).clone())
        .collect();
    if reachable.is_empty() {
        fallback.clone()
    } else {
        merge_exits(&reachable)
    }
}

/// Keeps nested statement bodies on their own flow-sensitive validation pass.
struct ExpressionWriteValidator<'a> {
    locals: LocalFacts,
    properties: &'a PropertyFacts,
    profile: SemanticProfile,
    current_class: Option<&'a str>,
    source: SourcePolicy,
    statement_depth: usize,
    error: Option<CompileError>,
    assignment_values: HashMap<usize, ValueFact>,
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
        if is_lazy_flow_expression(expr) {
            match validate_lazy_expression_writes(
                expr,
                &self.locals,
                self.properties,
                self.profile,
                self.current_class,
            ) {
                Ok(locals) => self.locals = locals,
                Err(error) => self.error = Some(error),
            }
            return;
        }
        if matches!(&expr.kind, ExprKind::Closure { .. }) {
            self.error = validate_closure_value(
                expr,
                Some(&self.locals),
                self.properties,
                self.profile,
                self.current_class,
            )
            .err();
            return;
        }
        let ExprKind::Assignment {
            target,
            value,
            prelude,
            ..
        } = &expr.kind
        else {
            return;
        };
        let mut locals = self.locals.clone();
        apply_expression_facts(
            target,
            &mut locals,
            self.properties,
            self.profile,
            self.current_class,
        );
        let value = apply_expression_facts(
            value,
            &mut locals,
            self.properties,
            self.profile,
            self.current_class,
        );
        apply_prelude_facts(
            prelude,
            &mut locals,
            self.properties,
            self.profile,
            self.current_class,
        );
        self.error = validate_array_access_chain(
            self.profile,
            target,
            &locals,
            self.properties,
            self.current_class,
        )
        .err();
        self.assignment_values.insert(expression_id(expr), value);
    }

    /// Lets lazy nodes use their branch-aware validation instead of the generic source walk.
    fn descend_expression(&mut self, expr: &Expr) -> bool {
        self.error.is_none() && !is_lazy_flow_expression(expr)
    }

    /// Publishes assignment facts for later operands, then restores source policy.
    fn expression_end(&mut self, expr: &Expr) {
        if self.error.is_none() && self.statement_depth == 1 {
            if let ExprKind::Assignment {
                target,
                value,
                prelude,
                ..
            } = &expr.kind
            {
                apply_prelude_facts(
                    prelude,
                    &mut self.locals,
                    self.properties,
                    self.profile,
                    self.current_class,
                );
                let value = self
                    .assignment_values
                    .remove(&expression_id(expr))
                    .unwrap_or_else(|| {
                        infer_expr(
                            value,
                            &self.locals,
                            self.properties,
                            self.profile,
                            self.current_class,
                        )
                    });
                record_assignment_fact(
                    target,
                    value,
                    &mut self.locals,
                    self.properties,
                    self.profile,
                    self.current_class,
                );
            }
        }
        self.source.leave_expression();
    }
}

/// Audits every lazy branch while merging only effects that can reach the following operand.
fn validate_lazy_expression_writes(
    expr: &Expr,
    locals: &LocalFacts,
    properties: &PropertyFacts,
    profile: SemanticProfile,
    current_class: Option<&str>,
) -> Result<LocalFacts, CompileError> {
    match &expr.kind {
        ExprKind::BinaryOp { left, op, right }
            if matches!(
                op,
                crate::parser::ast::BinOp::And | crate::parser::ast::BinOp::Or
            ) =>
        {
            let left_exit =
                validate_expression_flow(left, locals, properties, profile, current_class)?;
            let right_exit =
                validate_expression_flow(right, &left_exit, properties, profile, current_class)?;
            let truthiness = source_literal_truthiness(left);
            let right_runs = match op {
                crate::parser::ast::BinOp::And => truthiness,
                crate::parser::ast::BinOp::Or => truthiness.map(|value| !value),
                _ => unreachable!("lazy binary operator was matched above"),
            };
            Ok(match right_runs {
                Some(true) => right_exit,
                Some(false) => left_exit,
                None => merge_exits(&[left_exit, right_exit]),
            })
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            let condition_exit =
                validate_expression_flow(condition, locals, properties, profile, current_class)?;
            let then_exit = validate_expression_flow(
                then_expr,
                &condition_exit,
                properties,
                profile,
                current_class,
            )?;
            let else_exit = validate_expression_flow(
                else_expr,
                &condition_exit,
                properties,
                profile,
                current_class,
            )?;
            Ok(
                match source_literal_truthiness(condition) {
                    Some(true) => then_exit,
                    Some(false) => else_exit,
                    None => merge_exits(&[then_exit, else_exit]),
                },
            )
        }
        ExprKind::ShortTernary { value, default } => {
            let value_exit =
                validate_expression_flow(value, locals, properties, profile, current_class)?;
            let default_exit = validate_expression_flow(
                default,
                &value_exit,
                properties,
                profile,
                current_class,
            )?;
            Ok(
                match source_literal_truthiness(value) {
                    Some(true) => value_exit,
                    Some(false) => default_exit,
                    None => merge_exits(&[value_exit, default_exit]),
                },
            )
        }
        ExprKind::NullCoalesce { value, default } => {
            let value_exit =
                validate_expression_flow(value, locals, properties, profile, current_class)?;
            let default_exit = validate_expression_flow(
                default,
                &value_exit,
                properties,
                profile,
                current_class,
            )?;
            Ok(match source_literal_nullness(value) {
                Some(true) => default_exit,
                Some(false) => value_exit,
                None => merge_exits(&[value_exit, default_exit]),
            })
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            let subject_exit =
                validate_expression_flow(subject, locals, properties, profile, current_class)?;
            let selected = statically_selected_match_value(
                subject,
                arms,
                default.as_deref(),
                profile,
            );
            let mut fallthrough = subject_exit.clone();
            let mut exits = Vec::new();
            for (conditions, value) in arms {
                for condition in conditions {
                    fallthrough = validate_expression_flow(
                        condition,
                        &fallthrough,
                        properties,
                        profile,
                        current_class,
                    )?;
                    exits.push(validate_expression_flow(
                        value,
                        &fallthrough,
                        properties,
                        profile,
                        current_class,
                    )?);
                }
            }
            if let Some(default) = default {
                exits.push(validate_expression_flow(
                    default,
                    &fallthrough,
                    properties,
                    profile,
                    current_class,
                )?);
            }
            if let Some(selected) = selected {
                return selected.map_or(Ok(subject_exit.clone()), |value| {
                    validate_expression_flow(
                        value,
                        &subject_exit,
                        properties,
                        profile,
                        current_class,
                    )
                });
            }
            Ok(if exits.is_empty() {
                fallthrough
            } else {
                merge_exits(&exits)
            })
        }
        _ => unreachable!("only lazy expressions reach branch-aware write validation"),
    }
}

/// Applies assignment side effects from one evaluated expression to a scratch flow state.
fn apply_expression_facts(
    expr: &Expr,
    locals: &mut LocalFacts,
    properties: &PropertyFacts,
    profile: SemanticProfile,
    current_class: Option<&str>,
) -> ValueFact {
    let mut visitor = ExpressionFactUpdater {
        locals,
        properties,
        profile,
        current_class,
        expression_values: HashMap::new(),
    };
    ast_visit::walk_expression(expr, &mut visitor);
    visitor
        .expression_values
        .remove(&expression_id(expr))
        .unwrap_or(ValueFact::Unknown)
}

/// Records nested assignment results after their operands have produced their side effects.
struct ExpressionFactUpdater<'a> {
    locals: &'a mut LocalFacts,
    properties: &'a PropertyFacts,
    profile: SemanticProfile,
    current_class: Option<&'a str>,
    expression_values: HashMap<usize, ValueFact>,
}

impl AstVisitor for ExpressionFactUpdater<'_> {
    /// Applies lazy expression effects through independent branch states.
    fn expression(&mut self, expr: &Expr) {
        if is_lazy_flow_expression(expr) {
            let value = apply_lazy_expression_facts(
                expr,
                self.locals,
                self.properties,
                self.profile,
                self.current_class,
            );
            self.expression_values.insert(expression_id(expr), value);
        }
    }

    /// Prevents the generic source walker from replaying conditionally evaluated operands.
    fn descend_expression(&mut self, expr: &Expr) -> bool {
        !is_lazy_flow_expression(expr)
    }

    /// Closure and declaration bodies execute later and cannot mutate the current activation now.
    fn descend_statement(&mut self, _stmt: &Stmt) -> bool {
        false
    }

    /// Commits one assignment result after nested RHS assignments have updated the flow state.
    fn expression_end(&mut self, expr: &Expr) {
        if self.expression_values.contains_key(&expression_id(expr)) {
            return;
        }
        forget_call_mutated_facts(expr, self.locals);
        let value = match &expr.kind {
            ExprKind::Assignment {
                target,
                value,
                prelude,
                ..
            } => {
                apply_prelude_facts(
                    prelude,
                    self.locals,
                    self.properties,
                    self.profile,
                    self.current_class,
                );
                let value = self.expression_value(value);
                record_assignment_fact(
                    target,
                    value.clone(),
                    self.locals,
                    self.properties,
                    self.profile,
                    self.current_class,
                );
                value
            }
            ExprKind::ArrayLiteral(values) => self.evaluated_indexed_array_fact(values),
            ExprKind::ArrayLiteralAssoc(entries) => self.evaluated_assoc_array_fact(entries),
            ExprKind::ArrayLiteralMixed(entries) => self.evaluated_mixed_array_fact(entries),
            ExprKind::Spread(inner) => self.expression_value(inner),
            _ => infer_expr(
                expr,
                self.locals,
                self.properties,
                self.profile,
                self.current_class,
            ),
        };
        self.expression_values.insert(expression_id(expr), value);
    }
}

/// Forgets tracked local shapes that one completed call may replace through an argument.
fn forget_call_mutated_facts(expr: &Expr, locals: &mut LocalFacts) {
    if !matches!(
        &expr.kind,
        ExprKind::FunctionCall { .. }
            | ExprKind::ClosureCall { .. }
            | ExprKind::ExprCall { .. }
            | ExprKind::StaticMethodCall { .. }
            | ExprKind::MethodCall { .. }
            | ExprKind::NullsafeMethodCall { .. }
            | ExprKind::NullsafeDynamicMethodCall { .. }
            | ExprKind::NewObject { .. }
            | ExprKind::NewGeneric { .. }
            | ExprKind::NewScopedObject { .. }
            | ExprKind::NewDynamic { .. }
            | ExprKind::NewDynamicObject { .. }
    ) {
        return;
    }
    for name in crate::ir_lower::call_mutated_local_arguments(expr) {
        locals.assign(&name, ValueFact::Unknown);
    }
}

impl ExpressionFactUpdater<'_> {
    /// Returns the value captured when one child finished evaluating.
    fn expression_value(&self, expr: &Expr) -> ValueFact {
        self.expression_values
            .get(&expression_id(expr))
            .cloned()
            .unwrap_or_else(|| {
                infer_expr(
                    expr,
                    self.locals,
                    self.properties,
                    self.profile,
                    self.current_class,
                )
            })
    }

    /// Builds an indexed literal from the value captured for each ordered element.
    fn evaluated_indexed_array_fact(&self, values: &[Expr]) -> ValueFact {
        let mut array = ArrayFact::empty();
        for expr in values {
            let value = self.expression_value(expr);
            let success = if matches!(&expr.kind, ExprKind::Spread(_)) {
                let ValueFact::Array(source) = value else {
                    return ValueFact::Known(PhpType::php_array());
                };
                array.spread(&source)
            } else {
                array.append(value)
            };
            if success.is_none() {
                return ValueFact::Known(PhpType::php_array());
            }
        }
        ValueFact::Array(array)
    }

    /// Builds an associative literal from values captured before later entries execute.
    fn evaluated_assoc_array_fact(&self, entries: &[(Expr, Expr)]) -> ValueFact {
        let mut array = ArrayFact::empty();
        for (key, value) in entries {
            let Some(key) = source_array_key(key, self.profile) else {
                return ValueFact::Known(PhpType::php_array());
            };
            array.insert(key, self.expression_value(value));
        }
        ValueFact::Array(array)
    }

    /// Builds a mixed literal from ordered snapshots of keyed, implicit, and spread entries.
    fn evaluated_mixed_array_fact(&self, entries: &[ArrayEntry]) -> ValueFact {
        if let Some(array) = planned_mixed_array_fact(entries, self.profile, |value| {
            self.expression_value(value)
        }) {
            return ValueFact::Array(array);
        }
        let mut array = ArrayFact::empty();
        for entry in entries {
            let success = match entry {
                ArrayEntry::Spread(source) => {
                    let ValueFact::Array(source) = self.expression_value(source) else {
                        return ValueFact::Known(PhpType::php_array());
                    };
                    array.spread(&source)
                }
                ArrayEntry::Value(value) | ArrayEntry::Reference(value) => {
                    array.append(self.expression_value(value))
                }
                ArrayEntry::Keyed(key, value) | ArrayEntry::KeyReference(key, value) => {
                    let Some(key) = source_array_key(key, self.profile) else {
                        return ValueFact::Known(PhpType::php_array());
                    };
                    array.insert(key, self.expression_value(value));
                    Some(())
                }
            };
            if success.is_none() {
                return ValueFact::Known(PhpType::php_array());
            }
        }
        ValueFact::Array(array)
    }
}

/// Identifies one borrowed expression while its source tree remains alive.
fn expression_id(expr: &Expr) -> usize {
    std::ptr::from_ref(expr).addr()
}

/// Reports whether an expression contains operands that PHP may skip at runtime.
fn is_lazy_flow_expression(expr: &Expr) -> bool {
    matches!(
        &expr.kind,
        ExprKind::BinaryOp {
            op: crate::parser::ast::BinOp::And | crate::parser::ast::BinOp::Or,
            ..
        } | ExprKind::Ternary { .. }
            | ExprKind::ShortTernary { .. }
            | ExprKind::NullCoalesce { .. }
            | ExprKind::Match { .. }
    )
}

/// Applies side effects from one lazy expression without making a skipped branch definite.
fn apply_lazy_expression_facts(
    expr: &Expr,
    locals: &mut LocalFacts,
    properties: &PropertyFacts,
    profile: SemanticProfile,
    current_class: Option<&str>,
) -> ValueFact {
    match &expr.kind {
        ExprKind::BinaryOp { left, op, right }
            if matches!(
                op,
                crate::parser::ast::BinOp::And | crate::parser::ast::BinOp::Or
            ) =>
        {
            apply_expression_facts(left, locals, properties, profile, current_class);
            let truthiness = source_literal_truthiness(left);
            let right_runs = match op {
                crate::parser::ast::BinOp::And => truthiness,
                crate::parser::ast::BinOp::Or => truthiness.map(|value| !value),
                _ => unreachable!("lazy binary operator was matched above"),
            };
            apply_optional_expression_facts(
                right,
                right_runs,
                locals,
                properties,
                profile,
                current_class,
            );
            ValueFact::Known(PhpType::Bool)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            apply_expression_facts(condition, locals, properties, profile, current_class);
            match source_literal_truthiness(condition) {
                Some(true) => apply_expression_facts(
                    then_expr,
                    locals,
                    properties,
                    profile,
                    current_class,
                ),
                Some(false) => apply_expression_facts(
                    else_expr,
                    locals,
                    properties,
                    profile,
                    current_class,
                ),
                None => {
                    let entry = locals.clone();
                    let mut then_exit = entry.clone();
                    let mut else_exit = entry;
                    let then_value = apply_expression_facts(
                        then_expr,
                        &mut then_exit,
                        properties,
                        profile,
                        current_class,
                    );
                    let else_value = apply_expression_facts(
                        else_expr,
                        &mut else_exit,
                        properties,
                        profile,
                        current_class,
                    );
                    *locals = merge_exits(&[then_exit, else_exit]);
                    merge_value_facts(then_value, else_value)
                }
            }
        }
        ExprKind::ShortTernary { value, default } => {
            let value_fact =
                apply_expression_facts(value, locals, properties, profile, current_class);
            let default_runs = source_literal_truthiness(value)
                .map(|truthy| !truthy);
            match apply_optional_expression_facts(
                default,
                default_runs,
                locals,
                properties,
                profile,
                current_class,
            ) {
                Some(default_fact) if default_runs == Some(true) => default_fact,
                Some(default_fact) => merge_value_facts(value_fact, default_fact),
                None => value_fact,
            }
        }
        ExprKind::NullCoalesce { value, default } => {
            let value_fact =
                apply_expression_facts(value, locals, properties, profile, current_class);
            let default_runs = source_literal_nullness(value);
            match apply_optional_expression_facts(
                default,
                default_runs,
                locals,
                properties,
                profile,
                current_class,
            ) {
                Some(default_fact) if default_runs == Some(true) => default_fact,
                Some(default_fact) => merge_value_facts(value_fact, default_fact),
                None => value_fact,
            }
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            apply_expression_facts(subject, locals, properties, profile, current_class);
            if let Some(selected) = statically_selected_match_value(
                subject,
                arms,
                default.as_deref(),
                profile,
            ) {
                return selected.map_or(ValueFact::Unknown, |value| {
                    apply_expression_facts(value, locals, properties, profile, current_class)
                });
            }
            let mut fallthrough = locals.clone();
            let mut exits = Vec::new();
            let mut values = Vec::new();
            for (conditions, value) in arms {
                for condition in conditions {
                    apply_expression_facts(
                        condition,
                        &mut fallthrough,
                        properties,
                        profile,
                        current_class,
                    );
                    let mut selected = fallthrough.clone();
                    values.push(apply_expression_facts(
                        value,
                        &mut selected,
                        properties,
                        profile,
                        current_class,
                    ));
                    exits.push(selected);
                }
            }
            if let Some(default) = default {
                values.push(apply_expression_facts(
                    default,
                    &mut fallthrough,
                    properties,
                    profile,
                    current_class,
                ));
                exits.push(fallthrough);
            }
            if !exits.is_empty() {
                *locals = merge_exits(&exits);
            }
            values
                .into_iter()
                .reduce(merge_value_facts)
                .unwrap_or(ValueFact::Unknown)
        }
        _ => unreachable!("only lazy expressions reach the lazy fact updater"),
    }
}

/// Selects a match result when every compared operand has exact scalar identity.
fn statically_selected_match_value<'a>(
    subject: &Expr,
    arms: &'a [(Vec<Expr>, Expr)],
    default: Option<&'a Expr>,
    profile: SemanticProfile,
) -> Option<Option<&'a Expr>> {
    let subject = literal_match_fact(subject, profile)?;
    for (conditions, value) in arms {
        for condition in conditions {
            if literal_match_fact(condition, profile)? == subject {
                return Some(Some(value));
            }
        }
    }
    Some(default)
}

/// Retains only literal identities for which PHP match equality is unambiguous.
fn literal_match_fact(expr: &Expr, profile: SemanticProfile) -> Option<ValueFact> {
    if !matches!(
        &expr.kind,
        ExprKind::IntLiteral(_)
            | ExprKind::StringLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::Null
    ) {
        return None;
    }
    source_expr_fact(expr, profile)
}

/// Applies an operand that runs always, never, or along one possible control-flow edge.
fn apply_optional_expression_facts(
    expr: &Expr,
    runs: Option<bool>,
    locals: &mut LocalFacts,
    properties: &PropertyFacts,
    profile: SemanticProfile,
    current_class: Option<&str>,
) -> Option<ValueFact> {
    match runs {
        Some(true) => Some(apply_expression_facts(
            expr,
            locals,
            properties,
            profile,
            current_class,
        )),
        Some(false) => None,
        None => {
            let skipped = locals.clone();
            let mut evaluated = skipped.clone();
            let value = apply_expression_facts(
                expr,
                &mut evaluated,
                properties,
                profile,
                current_class,
            );
            *locals = merge_exits(&[skipped, evaluated]);
            Some(value)
        }
    }
}

/// Updates a plain local or a statically projected local array assignment target.
fn record_assignment_fact(
    target: &Expr,
    value: ValueFact,
    locals: &mut LocalFacts,
    properties: &PropertyFacts,
    profile: SemanticProfile,
    current_class: Option<&str>,
) {
    if let ExprKind::Variable(name) = &target.kind {
        locals.assign(name, value);
        return;
    }
    let mut indices = Vec::new();
    let Some(array) = local_array_target(target, &mut indices) else {
        return;
    };
    let indices = indices
        .into_iter()
        .map(|index| infer_expr(index, locals, properties, profile, current_class))
        .collect::<Vec<_>>();
    locals.assign(
        array,
        nested_array_write_fact(locals.get(array), &indices, value),
    );
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
                locals.assign(name, fact);
            }
            StmtKind::TypedAssign {
                type_expr, name, ..
            } => locals.assign(
                name,
                ValueFact::Known(crate::types::constant_binding::declared_type(type_expr)),
            ),
            StmtKind::RefAssign { target, source } => {
                if let ExprKind::Variable(source) = &source.kind {
                    locals.bind_ordinary_reference(target, source);
                } else {
                    locals.rebind(target, ValueFact::Unknown);
                }
            }
            StmtKind::StaticVar { name, init } => {
                let fact = infer_expr(init, locals, properties, profile, current_class);
                locals.assign(name, fact);
            }
            StmtKind::ListUnpack { vars, .. } => {
                for name in vars {
                    locals.assign(name, ValueFact::Unknown);
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
        if let StmtKind::ClassDecl {
            name,
            extends: Some(parent),
            ..
        } = &stmt.kind
        {
            facts.parents.insert(
                normalized_class_name(name),
                normalized_class_name(parent.as_str()),
            );
        }
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
    if let Some(flow) = validate_control_flow(
        stmt,
        locals,
        properties,
        profile,
        current_class,
    )? {
        *locals = flow.fallthrough.unwrap_or(flow.audit_exit);
        return Ok(());
    }
    if !matches!(&stmt.kind, StmtKind::DoWhile { .. }) {
        validate_expression_writes(stmt, locals, properties, profile, current_class)?;
    }
    match &stmt.kind {
        StmtKind::Echo(expr) | StmtKind::ExprStmt(expr) | StmtKind::Throw(expr) => {
            apply_expression_facts(expr, locals, properties, profile, current_class);
        }
        StmtKind::Return(Some(expr)) => {
            apply_expression_facts(expr, locals, properties, profile, current_class);
        }
        StmtKind::Assign { name, value } => {
            let value =
                apply_expression_facts(value, locals, properties, profile, current_class);
            locals.assign(name, value);
        }
        StmtKind::TypedAssign {
            type_expr,
            name,
            value: _,
        } => {
            locals.assign(
                name,
                ValueFact::Known(crate::types::constant_binding::declared_type(type_expr)),
            );
        }
        StmtKind::RefAssign { target, source } => {
            if let ExprKind::Variable(source) = &source.kind {
                locals.bind_ordinary_reference(target, source);
            } else {
                locals.rebind(target, ValueFact::Unknown);
            }
        }
        StmtKind::Global { vars } => {
            for name in vars {
                locals.rebind(name, ValueFact::Unknown);
            }
        }
        StmtKind::StaticVar { name, init } => {
            locals.assign(
                name,
                infer_expr(init, locals, properties, profile, current_class),
            );
        }
        StmtKind::ListUnpack { vars, .. } => {
            for name in vars {
                locals.assign(name, ValueFact::Unknown);
            }
        }
        StmtKind::ArrayAssign {
            array,
            index,
            value,
        } => {
            apply_expression_facts(index, locals, properties, profile, current_class);
            let value =
                apply_expression_facts(value, locals, properties, profile, current_class);
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
            locals.assign(array, array_write_fact(receiver, index, value));
        }
        StmtKind::ArrayPush { array, value } => {
            let value =
                apply_expression_facts(value, locals, properties, profile, current_class);
            let receiver = locals.get(array);
            locals.assign(array, array_push_fact(receiver, value));
        }
        StmtKind::NestedArrayAssign { target, value } => {
            apply_expression_facts(target, locals, properties, profile, current_class);
            let value =
                apply_expression_facts(value, locals, properties, profile, current_class);
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
                locals.assign(
                    array,
                    nested_array_write_fact(locals.get(array), &indices, value),
                );
            }
        }
        StmtKind::PropertyArrayAssign {
            object,
            property,
            index,
            value,
        } => {
            apply_expression_facts(object, locals, properties, profile, current_class);
            apply_expression_facts(index, locals, properties, profile, current_class);
            apply_expression_facts(value, locals, properties, profile, current_class);
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
            value,
        } => {
            apply_expression_facts(index, locals, properties, profile, current_class);
            apply_expression_facts(value, locals, properties, profile, current_class);
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
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            let condition_flow = validate_condition_flow(
                condition,
                locals,
                properties,
                profile,
                current_class,
            )?;
            let then_entry = condition_flow.when_true;
            let mut fallthrough = condition_flow.when_false;
            let mut fallthrough_reachable = condition_flow.false_reachable;
            let mut exits = Vec::with_capacity(elseif_clauses.len() + 2);
            let then_exit = validate_branch(
                then_body,
                &then_entry,
                properties,
                profile,
                current_class,
            )?;
            if condition_flow.true_reachable && branch_falls_through(then_body) {
                exits.push(then_exit);
            }
            for (condition, body) in elseif_clauses {
                let condition_flow = validate_condition_flow(
                    condition,
                    &fallthrough,
                    properties,
                    profile,
                    current_class,
                )?;
                let body_entry = condition_flow.when_true;
                let body_exit = validate_branch(
                    body,
                    &body_entry,
                    properties,
                    profile,
                    current_class,
                )?;
                if fallthrough_reachable
                    && condition_flow.true_reachable
                    && branch_falls_through(body)
                {
                    exits.push(body_exit);
                }
                fallthrough = condition_flow.when_false;
                fallthrough_reachable &= condition_flow.false_reachable;
            }
            if let Some(body) = else_body {
                let else_exit = validate_branch(
                    body,
                    &fallthrough,
                    properties,
                    profile,
                    current_class,
                )?;
                if fallthrough_reachable && branch_falls_through(body) {
                    exits.push(else_exit);
                }
            } else if fallthrough_reachable {
                exits.push(fallthrough);
            }
            *locals = exits
                .is_empty()
                .then_some(locals.clone())
                .unwrap_or_else(|| merge_exits(&exits));
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            let flow = collect_try_flow(
                try_body,
                catches,
                finally_body.as_deref(),
                locals,
                properties,
                profile,
                current_class,
            )?;
            *locals = flow.fallthrough.unwrap_or(flow.audit_exit);
        }
        StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. } => {
            validate_block(body, locals, properties, profile, current_class)?;
        }
        StmtKind::FunctionDecl {
            params,
            variadic,
            body,
            ..
        } => {
            let mut function = parameter_facts(params, variadic.as_deref());
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

/// Reports whether at least one path through a validated branch reaches its continuation.
fn branch_falls_through(body: &[Stmt]) -> bool {
    matches!(
        crate::termination::block_terminal_effect(body),
        crate::termination::TerminalEffect::FallsThrough
    )
}

/// Produces the case-insensitive class key used by source-only catch matching.
fn normalized_class_name(name: &str) -> String {
    crate::names::php_symbol_key(name.trim_start_matches('\\'))
}

/// Retains an exact thrown class when the source expression proves one.
fn source_throw_class(
    expr: &Expr,
    locals: &LocalFacts,
    properties: &PropertyFacts,
    profile: SemanticProfile,
    current_class: Option<&str>,
) -> Option<String> {
    let Some(PhpType::Object(class)) =
        infer_expr(expr, locals, properties, profile, current_class).value_type()
    else {
        return None;
    };
    (!class.is_empty()).then(|| normalized_class_name(&class))
}

/// Returns the next declared or builtin exception parent for source-only catch routing.
fn source_exception_parent(class: &str, properties: &PropertyFacts) -> Option<String> {
    if let Some(parent) = properties.parents.get(class) {
        return Some(parent.clone());
    }
    if let Some((_, parent)) = crate::types::checker::SPL_EXCEPTION_HIERARCHY
        .iter()
        .find(|(candidate, _)| normalized_class_name(candidate) == class)
    {
        return Some(normalized_class_name(parent));
    }
    match class {
        "exception" | "error" => Some("throwable".to_owned()),
        _ => None,
    }
}

/// Checks one exact thrown class against a catch type through the known parent chain.
fn source_class_is_a(class: &str, target: &str, properties: &PropertyFacts) -> bool {
    let target = normalized_class_name(target);
    let mut current = normalized_class_name(class);
    let mut seen = BTreeSet::new();
    while seen.insert(current.clone()) {
        if current == target {
            return true;
        }
        let Some(parent) = source_exception_parent(&current, properties) else {
            return false;
        };
        current = parent;
    }
    false
}

/// Reports whether one catch clause consumes a source throw.
fn terminal_matches_catch(
    terminal: &TerminalFlow,
    catch: &CatchClause,
    properties: &PropertyFacts,
) -> bool {
    let TerminalKind::Throw(class) = &terminal.kind else {
        return false;
    };
    catch.exception_types.iter().any(|handler| {
        let handler = handler.as_str();
        class.as_deref().map_or_else(
            || normalized_class_name(handler) == "throwable",
            |class| source_class_is_a(class, handler, properties),
        )
    })
}

/// Reports whether one catch clause consumes every PHP throwable.
fn catch_handles_every_throw(catch: &CatchClause) -> bool {
    catch
        .exception_types
        .iter()
        .any(|handler| normalized_class_name(handler.as_str()) == "throwable")
}

/// Reports whether earlier clauses consume every unknown throwable one target clause accepts.
fn catches_cover_target(
    catches: &[CatchClause],
    target: &CatchClause,
    properties: &PropertyFacts,
) -> bool {
    target.exception_types.iter().all(|target_type| {
        catches.iter().flat_map(|catch| &catch.exception_types).any(|handled_type| {
            source_class_is_a(target_type.as_str(), handled_type.as_str(), properties)
        })
    })
}

/// Reports whether an unknown implicit throw can reach one catch after earlier clauses.
fn untracked_throw_may_reach_catch(
    catches: &[CatchClause],
    catch_index: usize,
    properties: &PropertyFacts,
) -> bool {
    !catches_cover_target(&catches[..catch_index], &catches[catch_index], properties)
}

/// Starts a catch with unknown facts for locals the try body may have changed before throwing.
fn catch_entry_facts(
    entry: &LocalFacts,
    try_body: &[Stmt],
    properties: &PropertyFacts,
    profile: SemanticProfile,
    current_class: Option<&str>,
) -> LocalFacts {
    let reachable_body = reachable_statement_prefix(try_body);
    let mut catch_entry = entry.clone();
    let mut candidates: BTreeSet<_> = catch_entry.values.keys().cloned().collect();
    candidates.extend(crate::types::checker::loop_assigned_local_names(
        None,
        reachable_body,
        None,
    ));
    let changed: Vec<_> = candidates
        .into_iter()
        .filter(|name| {
            crate::ir_lower::body_writes_local(reachable_body, name)
                && !body_writes_only_static_fact(
                    reachable_body,
                    name,
                    &entry.get(name),
                    properties,
                    profile,
                    current_class,
                )
        })
        .collect();
    for name in changed {
        catch_entry.set(&name, ValueFact::Unknown);
    }
    catch_entry
}

/// Reports whether an explicit throw operand can fail before publishing its snapshot.
fn explicit_throw_operand_may_throw(
    expr: &Expr,
    properties: &PropertyFacts,
    profile: SemanticProfile,
) -> bool {
    if let ExprKind::NewObject { class_name, args } = &expr.kind {
        let class = normalized_class_name(class_name.as_str());
        let builtin_throwable = !properties.parents.contains_key(&class)
            && source_class_is_a(&class, "throwable", properties);
        if builtin_throwable {
            return args
                .iter()
                .any(|arg| crate::optimize::source_expression_may_throw(arg, profile));
        }
    }
    crate::optimize::source_expression_may_throw(expr, profile)
}

/// Finds a reachable throw edge for which the source walk has no exact fact snapshot.
fn block_has_untracked_throw(
    body: &[Stmt],
    properties: &PropertyFacts,
    profile: SemanticProfile,
    target: Option<&CatchClause>,
) -> bool {
    reachable_statement_prefix(body)
        .iter()
        .any(|stmt| stmt_has_untracked_throw(stmt, properties, profile, target))
}

/// Separates explicit throw terminals from implicit expression and statement failures.
fn stmt_has_untracked_throw(
    stmt: &Stmt,
    properties: &PropertyFacts,
    profile: SemanticProfile,
    target: Option<&CatchClause>,
) -> bool {
    match &stmt.kind {
        StmtKind::Throw(expr) => explicit_throw_operand_may_throw(expr, properties, profile),
        StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. } => {
            block_has_untracked_throw(body, properties, profile, target)
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            crate::optimize::source_expression_may_throw(condition, profile)
                || block_has_untracked_throw(then_body, properties, profile, target)
                || elseif_clauses.iter().any(|(condition, body)| {
                    crate::optimize::source_expression_may_throw(condition, profile)
                        || block_has_untracked_throw(body, properties, profile, target)
                })
                || else_body.as_ref().is_some_and(|body| {
                    block_has_untracked_throw(body, properties, profile, target)
                })
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            block_has_untracked_throw(then_body, properties, profile, target)
                || else_body.as_ref().is_some_and(|body| {
                    block_has_untracked_throw(body, properties, profile, target)
                })
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            crate::optimize::source_expression_may_throw(condition, profile)
                || block_has_untracked_throw(body, properties, profile, target)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_deref()
                .is_some_and(|stmt| stmt_has_untracked_throw(stmt, properties, profile, target))
                || condition.as_ref().is_some_and(|condition| {
                    crate::optimize::source_expression_may_throw(condition, profile)
                })
                || update
                    .as_deref()
                    .is_some_and(|stmt| stmt_has_untracked_throw(stmt, properties, profile, target))
                || block_has_untracked_throw(body, properties, profile, target)
        }
        StmtKind::Foreach { array, body, .. } => {
            crate::optimize::source_expression_may_throw(array, profile)
                || block_has_untracked_throw(body, properties, profile, target)
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            crate::optimize::source_expression_may_throw(subject, profile)
                || cases.iter().any(|(labels, body)| {
                    labels.iter().any(|label| {
                        crate::optimize::source_expression_may_throw(label, profile)
                    }) || block_has_untracked_throw(body, properties, profile, target)
                })
                || default.as_ref().is_some_and(|body| {
                    block_has_untracked_throw(body, properties, profile, target)
                })
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            (block_has_untracked_throw(try_body, properties, profile, target)
                && target.map_or_else(
                    || !catches.iter().any(catch_handles_every_throw),
                    |target| !catches_cover_target(catches, target, properties),
                ))
                || catches
                    .iter()
                    .any(|catch| block_has_untracked_throw(&catch.body, properties, profile, target))
                || finally_body.as_ref().is_some_and(|body| {
                    block_has_untracked_throw(body, properties, profile, target)
                })
        }
        _ => crate::optimize::source_statement_may_throw(stmt, profile),
    }
}

/// Keeps literal-only switch comparisons out of the generic throwing statement fallback.
fn source_switch_comparison_may_throw(
    subject: &Expr,
    label: &Expr,
    profile: SemanticProfile,
) -> bool {
    let _ = profile;
    !crate::optimize::source_literal_scalar_operand(subject)
        || !crate::optimize::source_literal_scalar_operand(label)
}

/// Excludes statements after the first unconditional control transfer from reachable facts.
fn reachable_statement_prefix(body: &[Stmt]) -> &[Stmt] {
    let length = body
        .iter()
        .position(|stmt| {
            !matches!(
                crate::termination::stmt_terminal_effect(stmt),
                crate::termination::TerminalEffect::FallsThrough
            )
        })
        .map_or(body.len(), |index| index + 1);
    &body[..length]
}

/// Updates an exact source array when every reachable iteration writes one static ref value.
#[allow(clippy::too_many_arguments)]
fn apply_foreach_reference_fact(
    source: &Expr,
    value_var: &str,
    body: &[Stmt],
    entry: &LocalFacts,
    flow: &LoopBreakFlow,
    exit: &mut LocalFacts,
    properties: &PropertyFacts,
    profile: SemanticProfile,
    current_class: Option<&str>,
) {
    let ExprKind::Variable(source_name) = &source.kind else {
        return;
    };
    if crate::ir_lower::body_writes_local(body, source_name) {
        return;
    }
    let Some(iteration) = flow.back_edge() else {
        return;
    };
    let replacement = iteration.get(value_var);
    if !body_writes_only_static_fact(
        body,
        value_var,
        &replacement,
        properties,
        profile,
        current_class,
    ) {
        return;
    }
    let ValueFact::Array(mut source) = entry.get(source_name) else {
        return;
    };
    if source.entries.is_empty() {
        return;
    }
    let exact_last = flow.exits.is_empty() && flow.terminals.is_empty() && flow.complete;
    exit.bind_foreach_reference(value_var, source_name, exact_last);
    if matches!(replacement, ValueFact::Unknown) || !exact_last {
        return;
    }
    source.fill_values(&replacement);
    exit.set(source_name, ValueFact::Array(source));
}

/// Reachable exits, terminals, and multi-level jumps produced by one switch statement.
struct SwitchFlow {
    exit: LocalFacts,
    outer_jumps: Vec<OuterLoopJump>,
    terminals: Vec<TerminalFlow>,
    can_fallthrough: bool,
}

/// The PHP control transfer whose source facts must resume after an enclosing finally.
#[derive(Clone, Debug, PartialEq, Eq)]
enum TerminalKind {
    Return,
    Throw(Option<String>),
}

/// Facts captured where one return or throw leaves the current statement list.
#[derive(Clone)]
struct TerminalFlow {
    kind: TerminalKind,
    facts: LocalFacts,
}

/// One statically proven switch entry, an exact miss, or an unresolved loose comparison.
#[derive(Clone, Copy)]
enum SwitchSelection {
    Body(usize),
    Miss,
    Unknown,
}

/// Selects an integer-literal switch arm without approximating PHP loose string comparisons.
fn source_switch_selection(
    subject: &ValueFact,
    cases: &[(Vec<Expr>, Vec<Stmt>)],
) -> SwitchSelection {
    let ValueFact::Integer(subject) = subject else {
        return SwitchSelection::Unknown;
    };
    for (case_index, (labels, _)) in cases.iter().enumerate() {
        for label in labels {
            let Some(ValueFact::Integer(label)) =
                source_expr_fact(label, SemanticProfile::PERMISSIVE)
            else {
                return SwitchSelection::Unknown;
            };
            if label == *subject {
                return SwitchSelection::Body(case_index);
            }
        }
    }
    SwitchSelection::Miss
}

/// Validates switch direct entries and fallthrough paths without merging them before each body.
fn validate_switch_flow(
    subject_expr: &Expr,
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: Option<&[Stmt]>,
    subject: &ValueFact,
    entry: &LocalFacts,
    properties: &PropertyFacts,
    profile: SemanticProfile,
    current_class: Option<&str>,
) -> Result<SwitchFlow, CompileError> {
    let selection = source_switch_selection(subject, cases);
    let mut direct_inputs = vec![Vec::new(); cases.len()];
    let mut no_match = None;
    let mut comparison_terminals = Vec::new();
    match selection {
        SwitchSelection::Body(case) => direct_inputs[case].push(entry.clone()),
        SwitchSelection::Miss => no_match = Some(entry.clone()),
        SwitchSelection::Unknown => {
            let mut label_entry = entry.clone();
            for (case_index, (labels, _)) in cases.iter().enumerate() {
                for label in labels {
                    apply_expression_facts(
                        label,
                        &mut label_entry,
                        properties,
                        profile,
                        current_class,
                    );
                    if source_switch_comparison_may_throw(subject_expr, label, profile) {
                        comparison_terminals.push(TerminalFlow {
                            kind: TerminalKind::Throw(None),
                            facts: label_entry.clone(),
                        });
                    }
                    if !direct_inputs[case_index].contains(&label_entry) {
                        direct_inputs[case_index].push(label_entry.clone());
                    }
                }
            }
            no_match = Some(label_entry);
        }
    }

    let mut bodies: Vec<&[Stmt]> = cases.iter().map(|(_, body)| body.as_slice()).collect();
    let default_position =
        default.map(|default| crate::termination::switch_default_position(cases, default));
    if let Some(default) = default {
        let position = default_position.expect("default position");
        bodies.insert(position, default);
        direct_inputs.insert(position, no_match.take().into_iter().collect());
    }

    let mut switch_exits = Vec::new();
    let mut outer_jumps = Vec::new();
    let mut terminals = comparison_terminals;
    let mut prior_fallthrough = None;
    for (body_index, body) in bodies.into_iter().enumerate() {
        let mut inputs = std::mem::take(&mut direct_inputs[body_index]);
        if let Some(fallthrough) = prior_fallthrough.take() {
            if !inputs.contains(&fallthrough) {
                inputs.push(fallthrough);
            }
        }
        if inputs.is_empty() {
            validate_branch(body, entry, properties, profile, current_class)?;
        }
        let mut fallthroughs = Vec::new();
        for input in inputs {
            let flow = collect_loop_break_flow(
                body,
                &input,
                properties,
                profile,
                current_class,
            )?;
            switch_exits.extend(flow.exits);
            switch_exits.extend(flow.continues);
            outer_jumps.extend(flow.outer_jumps);
            terminals.extend(flow.terminals);
            fallthroughs.push(flow.fallthrough);
        }
        prior_fallthrough = merge_optional_exits(fallthroughs);
    }
    if let Some(fallthrough) = prior_fallthrough {
        switch_exits.push(fallthrough);
    }
    if let Some(no_match) = no_match {
        switch_exits.push(no_match);
    }
    let can_fallthrough = !switch_exits.is_empty();
    let exit = if switch_exits.is_empty() {
        entry.clone()
    } else {
        merge_exits(&switch_exits)
    };
    Ok(SwitchFlow {
        exit,
        outer_jumps,
        terminals: coalesce_terminal_flows(terminals),
        can_fallthrough,
    })
}

/// Facts from breaks that directly leave the loop whose body is being analyzed.
#[derive(Clone)]
struct LoopBreakFlow {
    fallthrough: Option<LocalFacts>,
    exits: Vec<LocalFacts>,
    continues: Vec<LocalFacts>,
    outer_jumps: Vec<OuterLoopJump>,
    terminals: Vec<TerminalFlow>,
    complete: bool,
    audit_exit: LocalFacts,
}

impl LoopBreakFlow {
    /// Merges paths that reach this loop's next condition or update.
    fn back_edge(&self) -> Option<LocalFacts> {
        let mut exits = self.continues.clone();
        exits.extend(self.fallthrough.iter().cloned());
        (!exits.is_empty()).then(|| merge_exits(&exits))
    }
}

/// One multi-level loop action after the innermost construct consumed its level.
#[derive(Clone, Copy)]
enum OuterLoopJumpKind {
    Break,
    Continue,
}

/// Facts captured where a multi-level break or continue executes.
#[derive(Clone)]
struct OuterLoopJump {
    kind: OuterLoopJumpKind,
    levels: usize,
    facts: LocalFacts,
}

/// Restores the previous nested-loop refinement depth when one bounded replay finishes.
struct LoopRefinementGuard<'a> {
    depth: &'a Cell<u8>,
    previous: u8,
}

impl Drop for LoopRefinementGuard<'_> {
    /// Restores the thread-local depth on success, error, or unwinding.
    fn drop(&mut self) {
        self.depth.set(self.previous);
    }
}

/// Replays one changed loop entry while bounding recursive replays in nested loops.
fn collect_repeated_loop_flow(
    body: &[Stmt],
    first_entry: &LocalFacts,
    repeated_entry: &LocalFacts,
    first_flow: &LoopBreakFlow,
    properties: &PropertyFacts,
    profile: SemanticProfile,
    current_class: Option<&str>,
) -> Result<LoopBreakFlow, CompileError> {
    if repeated_entry == first_entry {
        return Ok(first_flow.clone());
    }
    LOOP_REFINEMENT_DEPTH.with(|depth| {
        let previous = depth.get();
        if previous >= 2 {
            let mut conservative = first_flow.clone();
            conservative.complete = false;
            return Ok(conservative);
        }
        depth.set(previous + 1);
        let _guard = LoopRefinementGuard { depth, previous };
        collect_loop_break_flow(
            body,
            repeated_entry,
            properties,
            profile,
            current_class,
        )
    })
}

/// Flow produced when a nested loop or switch consumes one control level.
struct NestedControlFlow {
    fallthrough: Option<LocalFacts>,
    outer_jumps: Vec<OuterLoopJump>,
    terminals: Vec<TerminalFlow>,
    audit_exit: LocalFacts,
}

/// Validates a nested breakable statement and preserves outbound jumps and terminals.
#[allow(clippy::too_many_arguments)]
fn validate_control_flow(
    stmt: &Stmt,
    entry: &LocalFacts,
    properties: &PropertyFacts,
    profile: SemanticProfile,
    current_class: Option<&str>,
) -> Result<Option<NestedControlFlow>, CompileError> {
    if stmt.source_mode == crate::source::SourceMode::Internal {
        return Ok(None);
    }
    if !matches!(
        &stmt.kind,
        StmtKind::While { .. }
            | StmtKind::Foreach { .. }
            | StmtKind::DoWhile { .. }
            | StmtKind::For { .. }
            | StmtKind::Switch { .. }
    ) {
        return Ok(None);
    }

    if !matches!(&stmt.kind, StmtKind::DoWhile { .. }) {
        validate_expression_writes(stmt, entry, properties, profile, current_class)?;
    }

    let (outer_jumps, terminals, can_fallthrough, audit_exit) = match &stmt.kind {
        StmtKind::While { condition, body } => {
            let first_condition = validate_condition_flow(
                condition,
                entry,
                properties,
                profile,
                current_class,
            )?;
            let first_body_entry = first_condition.when_true;
            let first_flow = collect_loop_break_flow(
                body,
                &first_body_entry,
                properties,
                profile,
                current_class,
            )?;
            let first_back_edge = first_flow.back_edge();
            let first_reaches_condition = first_back_edge.is_some();
            let repeated_condition = validate_condition_flow(
                condition,
                &first_back_edge.unwrap_or_else(|| first_body_entry.clone()),
                properties,
                profile,
                current_class,
            )?;
            let repeated_body_reachable = first_condition.true_reachable
                && first_reaches_condition
                && repeated_condition.true_reachable;
            let repeated_exit_reachable = first_condition.true_reachable
                && first_reaches_condition
                && repeated_condition.false_reachable;
            let repeated_flow = collect_repeated_loop_flow(
                body,
                &first_body_entry,
                &repeated_condition.when_true,
                &first_flow,
                properties,
                profile,
                current_class,
            )?;
            let mut outer_jumps = Vec::new();
            if first_condition.true_reachable {
                outer_jumps.extend(first_flow.outer_jumps.clone());
            }
            if repeated_body_reachable {
                outer_jumps.extend(repeated_flow.outer_jumps.clone());
            }
            let mut terminals = Vec::new();
            if first_condition.true_reachable {
                terminals.extend(first_flow.terminals.clone());
            }
            if repeated_body_reachable {
                terminals.extend(repeated_flow.terminals.clone());
            }
            let terminals = coalesce_terminal_flows(terminals);
            let mut normal_exits = Vec::new();
            if first_condition.false_reachable {
                normal_exits.push(first_condition.when_false);
            }
            if repeated_exit_reachable {
                normal_exits.push(repeated_condition.when_false);
            }
            let mut break_flows = Vec::new();
            if first_condition.true_reachable {
                break_flows.push(&first_flow);
            }
            if repeated_body_reachable {
                break_flows.push(&repeated_flow);
            }
            let can_fallthrough = !normal_exits.is_empty()
                || break_flows.iter().any(|flow| !flow.exits.is_empty());
            let audit_exit = merge_loop_exits(
                body,
                Some(condition),
                None,
                &normal_exits,
                &break_flows,
                entry,
                properties,
                profile,
                current_class,
            );
            (outer_jumps, terminals, can_fallthrough, audit_exit)
        }
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            value_by_ref,
            body,
            ..
        } => {
            let mut loop_entry = entry.clone();
            apply_expression_facts(
                array,
                &mut loop_entry,
                properties,
                profile,
                current_class,
            );
            let source_nonempty = source_array_nonempty(
                array,
                &loop_entry,
                properties,
                profile,
                current_class,
            );
            let body_reachable = source_nonempty != Some(false);
            let iteration_value = source_array_iteration_value(
                array,
                &loop_entry,
                properties,
                profile,
                current_class,
            );
            let mut iteration = loop_entry.clone();
            if let Some(key) = key_var {
                iteration.assign(key, ValueFact::Unknown);
            }
            if *value_by_ref {
                iteration.rebind(value_var, iteration_value);
                if let ExprKind::Variable(source_name) = &array.kind {
                    let exact_current = matches!(
                        iteration.get(source_name),
                        ValueFact::Array(ref source) if source.entries.len() == 1
                    );
                    iteration.bind_foreach_reference(value_var, source_name, exact_current);
                }
            } else {
                iteration.assign(value_var, iteration_value);
            }
            let flow = collect_loop_break_flow(
                body,
                &iteration,
                properties,
                profile,
                current_class,
            )?;
            let mut normal_exits = Vec::new();
            if source_nonempty != Some(true) {
                normal_exits.push(loop_entry.clone());
            }
            if body_reachable {
                if let Some(iteration) = flow.back_edge() {
                    normal_exits.push(iteration);
                }
            }
            let break_flows: Vec<_> = body_reachable.then_some(&flow).into_iter().collect();
            let can_fallthrough = !normal_exits.is_empty() || !flow.exits.is_empty();
            let mut audit_exit = merge_loop_exits(
                body,
                None,
                None,
                &normal_exits,
                &break_flows,
                &loop_entry,
                properties,
                profile,
                current_class,
            );
            if *value_by_ref && body_reachable {
                apply_foreach_reference_fact(
                    array,
                    value_var,
                    body,
                    &loop_entry,
                    &flow,
                    &mut audit_exit,
                    properties,
                    profile,
                    current_class,
                );
            }
            let outer_jumps = body_reachable
                .then(|| flow.outer_jumps.clone())
                .unwrap_or_default();
            let terminals = body_reachable
                .then(|| flow.terminals.clone())
                .unwrap_or_default();
            (outer_jumps, terminals, can_fallthrough, audit_exit)
        }
        StmtKind::DoWhile { body, condition } => {
            let first_flow = collect_loop_break_flow(
                body,
                entry,
                properties,
                profile,
                current_class,
            )?;
            let first_back_edge = first_flow.back_edge();
            let first_reaches_condition = first_back_edge.is_some();
            let first_condition = validate_condition_flow(
                condition,
                &first_back_edge.unwrap_or_else(|| entry.clone()),
                properties,
                profile,
                current_class,
            )?;
            let repeated_body_reachable =
                first_reaches_condition && first_condition.true_reachable;
            let repeated_flow = collect_repeated_loop_flow(
                body,
                entry,
                &first_condition.when_true,
                &first_flow,
                properties,
                profile,
                current_class,
            )?;
            let mut outer_jumps = first_flow.outer_jumps.clone();
            if repeated_body_reachable {
                outer_jumps.extend(repeated_flow.outer_jumps.clone());
            }
            let mut terminals = first_flow.terminals.clone();
            if repeated_body_reachable {
                terminals.extend(repeated_flow.terminals.clone());
            }
            let terminals = coalesce_terminal_flows(terminals);
            let repeated_back_edge = repeated_flow.back_edge();
            let repeated_reaches_condition = repeated_back_edge.is_some();
            let repeated_condition = validate_condition_flow(
                condition,
                &repeated_back_edge
                    .unwrap_or_else(|| first_condition.when_true.clone()),
                properties,
                profile,
                current_class,
            )?;
            let mut normal_exits = Vec::new();
            if first_reaches_condition && first_condition.false_reachable {
                normal_exits.push(first_condition.when_false);
            }
            if repeated_body_reachable
                && repeated_reaches_condition
                && repeated_condition.false_reachable
            {
                normal_exits.push(repeated_condition.when_false);
            }
            let mut break_flows = vec![&first_flow];
            if repeated_body_reachable {
                break_flows.push(&repeated_flow);
            }
            let can_fallthrough = !normal_exits.is_empty()
                || break_flows.iter().any(|flow| !flow.exits.is_empty());
            let audit_exit = merge_loop_exits(
                body,
                Some(condition),
                None,
                &normal_exits,
                &break_flows,
                entry,
                properties,
                profile,
                current_class,
            );
            (outer_jumps, terminals, can_fallthrough, audit_exit)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            let mut initialized = entry.clone();
            if let Some(init) = init {
                validate_stmt(
                    init,
                    &mut initialized,
                    properties,
                    profile,
                    current_class,
                )?;
            }
            let first_condition = match condition {
                Some(condition) => validate_condition_flow(
                    condition,
                    &initialized,
                    properties,
                    profile,
                    current_class,
                )?,
                None => ConditionFlow {
                    when_true: initialized.clone(),
                    when_false: initialized.clone(),
                    true_reachable: true,
                    false_reachable: false,
                },
            };
            let first_body_entry = first_condition.when_true;
            let first_flow = collect_loop_break_flow(
                body,
                &first_body_entry,
                properties,
                profile,
                current_class,
            )?;
            let first_back_edge = first_flow.back_edge();
            let first_reaches_update = first_back_edge.is_some();
            let mut repeated_entry =
                first_back_edge.unwrap_or_else(|| first_body_entry.clone());
            if let Some(update) = update {
                validate_stmt(
                    update,
                    &mut repeated_entry,
                    properties,
                    profile,
                    current_class,
                )?;
            }
            let repeated_condition = match condition {
                Some(condition) => validate_condition_flow(
                    condition,
                    &repeated_entry,
                    properties,
                    profile,
                    current_class,
                )?,
                None => ConditionFlow {
                    when_true: repeated_entry.clone(),
                    when_false: repeated_entry,
                    true_reachable: true,
                    false_reachable: false,
                },
            };
            let repeated_body_reachable = first_condition.true_reachable
                && first_reaches_update
                && repeated_condition.true_reachable;
            let repeated_exit_reachable = first_condition.true_reachable
                && first_reaches_update
                && repeated_condition.false_reachable;
            let repeated_flow = collect_repeated_loop_flow(
                body,
                &first_body_entry,
                &repeated_condition.when_true,
                &first_flow,
                properties,
                profile,
                current_class,
            )?;
            let mut outer_jumps = Vec::new();
            if first_condition.true_reachable {
                outer_jumps.extend(first_flow.outer_jumps.clone());
            }
            if repeated_body_reachable {
                outer_jumps.extend(repeated_flow.outer_jumps.clone());
            }
            let mut terminals = Vec::new();
            if first_condition.true_reachable {
                terminals.extend(first_flow.terminals.clone());
            }
            if repeated_body_reachable {
                terminals.extend(repeated_flow.terminals.clone());
            }
            let terminals = coalesce_terminal_flows(terminals);
            let mut normal_exits = Vec::new();
            if first_condition.false_reachable {
                normal_exits.push(first_condition.when_false);
            }
            if repeated_exit_reachable {
                normal_exits.push(repeated_condition.when_false);
            }
            let mut break_flows = Vec::new();
            if first_condition.true_reachable {
                break_flows.push(&first_flow);
            }
            if repeated_body_reachable {
                break_flows.push(&repeated_flow);
            }
            let can_fallthrough = !normal_exits.is_empty()
                || break_flows.iter().any(|flow| !flow.exits.is_empty());
            let audit_exit = merge_loop_exits(
                body,
                condition.as_ref(),
                update.as_deref(),
                &normal_exits,
                &break_flows,
                &initialized,
                properties,
                profile,
                current_class,
            );
            (outer_jumps, terminals, can_fallthrough, audit_exit)
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            let mut switch_entry = entry.clone();
            apply_expression_facts(
                subject,
                &mut switch_entry,
                properties,
                profile,
                current_class,
            );
            let subject_fact =
                infer_expr(subject, &switch_entry, properties, profile, current_class);
            let flow = validate_switch_flow(
                subject,
                cases,
                default.as_deref(),
                &subject_fact,
                &switch_entry,
                properties,
                profile,
                current_class,
            )?;
            (
                flow.outer_jumps,
                flow.terminals,
                flow.can_fallthrough,
                flow.exit,
            )
        }
        _ => unreachable!("nested control flow is filtered above"),
    };

    Ok(Some(NestedControlFlow {
        fallthrough: can_fallthrough.then_some(audit_exit.clone()),
        outer_jumps,
        terminals,
        audit_exit,
    }))
}

/// Consumes jumps that target the current loop and propagates the remaining levels.
fn route_nested_outer_jumps(
    jumps: Vec<OuterLoopJump>,
    exits: &mut Vec<LocalFacts>,
    continues: &mut Vec<LocalFacts>,
    outer_jumps: &mut Vec<OuterLoopJump>,
) {
    for mut jump in jumps {
        if jump.levels == 1 {
            match jump.kind {
                OuterLoopJumpKind::Break => exits.push(jump.facts),
                OuterLoopJumpKind::Continue => continues.push(jump.facts),
            }
        } else {
            jump.levels -= 1;
            outer_jumps.push(jump);
        }
    }
}

/// Replays reachable body paths to snapshot facts exactly where `break 1` executes.
fn collect_loop_break_flow(
    body: &[Stmt],
    entry: &LocalFacts,
    properties: &PropertyFacts,
    profile: SemanticProfile,
    current_class: Option<&str>,
) -> Result<LoopBreakFlow, CompileError> {
    let mut fallthrough = Some(entry.clone());
    let mut exits = Vec::new();
    let mut continues = Vec::new();
    let mut outer_jumps = Vec::new();
    let mut terminals = Vec::new();
    let mut complete = true;
    let mut audit_exit = entry.clone();
    for stmt in body {
        let Some(mut locals) = fallthrough.take() else {
            validate_stmt(
                stmt,
                &mut audit_exit,
                properties,
                profile,
                current_class,
            )?;
            continue;
        };
        match &stmt.kind {
            StmtKind::Break(level) => {
                if *level == 1 {
                    exits.push(locals.clone());
                } else {
                    outer_jumps.push(OuterLoopJump {
                        kind: OuterLoopJumpKind::Break,
                        levels: level - 1,
                        facts: locals.clone(),
                    });
                }
            }
            StmtKind::Continue(level) => {
                if *level == 1 {
                    continues.push(locals.clone());
                } else {
                    outer_jumps.push(OuterLoopJump {
                        kind: OuterLoopJumpKind::Continue,
                        levels: level - 1,
                        facts: locals.clone(),
                    });
                }
            }
            StmtKind::Return(_) => {
                validate_stmt(stmt, &mut locals, properties, profile, current_class)?;
                terminals.push(TerminalFlow {
                    kind: TerminalKind::Return,
                    facts: locals.clone(),
                });
            }
            StmtKind::Throw(expr) => {
                let thrown_class = source_throw_class(
                    expr,
                    &locals,
                    properties,
                    profile,
                    current_class,
                );
                validate_stmt(stmt, &mut locals, properties, profile, current_class)?;
                terminals.push(TerminalFlow {
                    kind: TerminalKind::Throw(thrown_class),
                    facts: locals.clone(),
                });
            }
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses,
                else_body,
            } => {
                let branch = collect_loop_break_if(
                    condition,
                    then_body,
                    elseif_clauses,
                    else_body.as_deref(),
                    &locals,
                    properties,
                    profile,
                    current_class,
                )?;
                audit_exit = branch.audit_exit;
                exits.extend(branch.exits);
                continues.extend(branch.continues);
                outer_jumps.extend(branch.outer_jumps);
                terminals.extend(branch.terminals);
                complete &= branch.complete;
                fallthrough = branch.fallthrough;
                continue;
            }
            StmtKind::Synthetic(nested) | StmtKind::NamespaceBlock { body: nested, .. } => {
                let nested = collect_loop_break_flow(
                    nested,
                    &locals,
                    properties,
                    profile,
                    current_class,
                )?;
                audit_exit = nested.audit_exit;
                exits.extend(nested.exits);
                continues.extend(nested.continues);
                outer_jumps.extend(nested.outer_jumps);
                terminals.extend(nested.terminals);
                complete &= nested.complete;
                fallthrough = nested.fallthrough;
                continue;
            }
            StmtKind::IncludeOnceGuard { body: nested, .. } => {
                let nested = collect_loop_break_flow(
                    nested,
                    &locals,
                    properties,
                    profile,
                    current_class,
                )?;
                audit_exit = nested.audit_exit;
                exits.extend(nested.exits);
                continues.extend(nested.continues);
                outer_jumps.extend(nested.outer_jumps);
                terminals.extend(nested.terminals);
                complete &= nested.complete;
                let mut paths = vec![locals];
                if let Some(exit) = nested.fallthrough {
                    paths.push(exit);
                }
                fallthrough = Some(merge_exits(&paths));
                continue;
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                let flow = collect_try_flow(
                    try_body,
                    catches,
                    finally_body.as_deref(),
                    &locals,
                    properties,
                    profile,
                    current_class,
                )?;
                audit_exit = flow.audit_exit;
                exits.extend(flow.exits);
                continues.extend(flow.continues);
                outer_jumps.extend(flow.outer_jumps);
                terminals.extend(flow.terminals);
                complete &= flow.complete;
                fallthrough = flow.fallthrough;
                continue;
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                let then_flow = collect_loop_break_flow(
                    then_body,
                    &locals,
                    properties,
                    profile,
                    current_class,
                )?;
                let else_flow = if let Some(else_body) = else_body {
                    collect_loop_break_flow(
                        else_body,
                        &locals,
                        properties,
                        profile,
                        current_class,
                    )?
                } else {
                    LoopBreakFlow {
                        fallthrough: Some(locals.clone()),
                        exits: Vec::new(),
                        continues: Vec::new(),
                        outer_jumps: Vec::new(),
                        terminals: Vec::new(),
                        complete: true,
                        audit_exit: locals,
                    }
                };
                audit_exit = merge_exits(&[
                    then_flow.audit_exit.clone(),
                    else_flow.audit_exit.clone(),
                ]);
                exits.extend(then_flow.exits);
                exits.extend(else_flow.exits);
                continues.extend(then_flow.continues);
                continues.extend(else_flow.continues);
                outer_jumps.extend(then_flow.outer_jumps);
                outer_jumps.extend(else_flow.outer_jumps);
                terminals.extend(then_flow.terminals);
                terminals.extend(else_flow.terminals);
                complete &= then_flow.complete && else_flow.complete;
                fallthrough = merge_optional_exits([
                    then_flow.fallthrough,
                    else_flow.fallthrough,
                ]);
                continue;
            }
            _ => {
                if let Some(nested) = validate_control_flow(
                    stmt,
                    &locals,
                    properties,
                    profile,
                    current_class,
                )? {
                    audit_exit = nested.audit_exit;
                    terminals.extend(nested.terminals);
                    route_nested_outer_jumps(
                        nested.outer_jumps,
                        &mut exits,
                        &mut continues,
                        &mut outer_jumps,
                    );
                    fallthrough = nested.fallthrough;
                    continue;
                }
                validate_stmt(stmt, &mut locals, properties, profile, current_class)?;
                audit_exit = locals.clone();
                fallthrough = Some(locals);
                continue;
            }
        }
        audit_exit = locals;
        fallthrough = None;
    }
    Ok(LoopBreakFlow {
        fallthrough,
        exits,
        continues,
        outer_jumps,
        terminals: coalesce_terminal_flows(terminals),
        complete,
        audit_exit,
    })
}

/// Combines mutually exclusive try and catch paths before an optional finally block.
fn merge_parallel_loop_flows(flows: Vec<LoopBreakFlow>) -> LoopBreakFlow {
    let audit_exit = merge_exits(
        &flows
            .iter()
            .map(|flow| flow.audit_exit.clone())
            .collect::<Vec<_>>(),
    );
    let fallthrough = merge_optional_exits(
        flows
            .iter()
            .map(|flow| flow.fallthrough.clone())
            .collect::<Vec<_>>(),
    );
    let complete = flows.iter().all(|flow| flow.complete);
    let mut exits = Vec::new();
    let mut continues = Vec::new();
    let mut outer_jumps = Vec::new();
    let mut terminals = Vec::new();
    for flow in flows {
        exits.extend(flow.exits);
        continues.extend(flow.continues);
        outer_jumps.extend(flow.outer_jumps);
        terminals.extend(flow.terminals);
    }
    LoopBreakFlow {
        fallthrough,
        exits,
        continues,
        outer_jumps,
        terminals: coalesce_terminal_flows(terminals),
        complete,
        audit_exit,
    }
}

/// Collects every reachable try and catch path before replaying an optional finally body.
fn collect_try_flow(
    try_body: &[Stmt],
    catches: &[CatchClause],
    finally_body: Option<&[Stmt]>,
    entry: &LocalFacts,
    properties: &PropertyFacts,
    profile: SemanticProfile,
    current_class: Option<&str>,
) -> Result<LoopBreakFlow, CompileError> {
    let mut try_flow = collect_loop_break_flow(
        try_body,
        entry,
        properties,
        profile,
        current_class,
    )?;
    let mut caught_entries = vec![Vec::new(); catches.len()];
    let mut escaping = Vec::new();
    for terminal in std::mem::take(&mut try_flow.terminals) {
        if let Some(index) = catches
            .iter()
            .position(|catch| terminal_matches_catch(&terminal, catch, properties))
        {
            caught_entries[index].push(terminal.facts);
        } else {
            escaping.push(terminal);
        }
    }
    try_flow.terminals = escaping;
    let mut flows = vec![try_flow];
    let mut isolated_audit_flows = Vec::new();
    for (catch_index, (catch, exact_entries)) in
        catches.iter().zip(caught_entries).enumerate()
    {
        let has_untracked_throw =
            block_has_untracked_throw(try_body, properties, profile, Some(catch));
        let mut catch_entry =
            catch_entry_facts(entry, try_body, properties, profile, current_class);
        if let Some(variable) = &catch.variable {
            catch_entry.set(variable, ValueFact::Unknown);
        }
        let mut distinct_entries = Vec::new();
        for mut exact_entry in exact_entries {
            if let Some(variable) = &catch.variable {
                exact_entry.set(variable, ValueFact::Unknown);
            }
            if !distinct_entries.contains(&exact_entry) {
                distinct_entries.push(exact_entry);
            }
        }
        if has_untracked_throw
            && untracked_throw_may_reach_catch(catches, catch_index, properties)
            && !distinct_entries.contains(&catch_entry)
        {
            distinct_entries.push(catch_entry.clone());
        }
        let Some(merged_entry) = (!distinct_entries.is_empty())
            .then(|| merge_exits(&distinct_entries))
        else {
            isolated_audit_flows.push(collect_loop_break_flow(
                &catch.body,
                &catch_entry,
                properties,
                profile,
                current_class,
            )?);
            continue;
        };
        flows.push(collect_loop_break_flow(
            &catch.body,
            &merged_entry,
            properties,
            profile,
            current_class,
        )?);
    }
    let flow = merge_parallel_loop_flows(flows);
    if let Some(finally_body) = finally_body {
        for audit_flow in isolated_audit_flows {
            apply_loop_finally(
                audit_flow,
                finally_body,
                properties,
                profile,
                current_class,
            )?;
        }
        apply_loop_finally(flow, finally_body, properties, profile, current_class)
    } else {
        Ok(flow)
    }
}

/// Control action resumed when a finally body falls through normally.
#[derive(Clone)]
enum LoopResume {
    Fallthrough,
    Break,
    Continue,
    Outer(OuterLoopJumpKind, usize),
    Terminal(TerminalKind),
}

/// Groups control actions that can share one deterministic finally traversal.
fn group_loop_resume_paths(
    paths: Vec<(LoopResume, LocalFacts)>,
) -> Vec<(LocalFacts, Vec<LoopResume>)> {
    let mut groups: Vec<(LocalFacts, Vec<LoopResume>)> = Vec::new();
    for (resume, entry) in paths {
        if let Some((_, resumes)) = groups
            .iter_mut()
            .find(|(candidate, _)| candidate == &entry)
        {
            resumes.push(resume);
        } else {
            groups.push((entry, vec![resume]));
        }
    }
    groups
}

/// Applies one finally body to every distinct incoming loop-control category.
fn apply_loop_finally(
    flow: LoopBreakFlow,
    finally_body: &[Stmt],
    properties: &PropertyFacts,
    profile: SemanticProfile,
    current_class: Option<&str>,
) -> Result<LoopBreakFlow, CompileError> {
    let fallback_audit_exit = flow.audit_exit.clone();
    let mut paths = Vec::with_capacity(4);
    if let Some(fallthrough) = flow.fallthrough {
        paths.push((LoopResume::Fallthrough, fallthrough));
    }
    if !flow.exits.is_empty() {
        paths.push((LoopResume::Break, merge_exits(&flow.exits)));
    }
    if !flow.continues.is_empty() {
        paths.push((LoopResume::Continue, merge_exits(&flow.continues)));
    }
    for jump in flow.outer_jumps {
        paths.push((LoopResume::Outer(jump.kind, jump.levels), jump.facts));
    }
    for terminal in coalesce_terminal_flows(flow.terminals) {
        paths.push((LoopResume::Terminal(terminal.kind), terminal.facts));
    }

    let mut fallthroughs = Vec::new();
    let mut exits = Vec::new();
    let mut continues = Vec::new();
    let mut outer_jumps = Vec::new();
    let mut terminals = Vec::new();
    let mut audit_exits = Vec::new();
    let mut complete = flow.complete;
    for (entry, resumes) in group_loop_resume_paths(paths) {
        let finally_flow = collect_loop_break_flow(
            finally_body,
            &entry,
            properties,
            profile,
            current_class,
        )?;
        audit_exits.push(finally_flow.audit_exit.clone());
        exits.extend(finally_flow.exits);
        continues.extend(finally_flow.continues);
        outer_jumps.extend(finally_flow.outer_jumps);
        terminals.extend(finally_flow.terminals);
        complete &= finally_flow.complete;
        let Some(fallthrough) = finally_flow.fallthrough else {
            continue;
        };
        for resume in resumes {
            match resume {
                LoopResume::Fallthrough => fallthroughs.push(Some(fallthrough.clone())),
                LoopResume::Break => exits.push(fallthrough.clone()),
                LoopResume::Continue => continues.push(fallthrough.clone()),
                LoopResume::Outer(kind, levels) => outer_jumps.push(OuterLoopJump {
                    kind,
                    levels,
                    facts: fallthrough.clone(),
                }),
                LoopResume::Terminal(kind) => terminals.push(TerminalFlow {
                    kind,
                    facts: fallthrough.clone(),
                }),
            }
        }
    }
    let audit_exit = if audit_exits.is_empty() {
        validate_branch(
            finally_body,
            &fallback_audit_exit,
            properties,
            profile,
            current_class,
        )?
    } else {
        merge_exits(&audit_exits)
    };
    Ok(LoopBreakFlow {
        fallthrough: merge_optional_exits(fallthroughs),
        exits,
        continues,
        outer_jumps,
        terminals: coalesce_terminal_flows(terminals),
        complete,
        audit_exit,
    })
}

/// Audits an `if` chain while collecting only its reachable loop-control paths.
fn collect_loop_break_if(
    condition: &Expr,
    then_body: &[Stmt],
    elseif_clauses: &[(Expr, Vec<Stmt>)],
    else_body: Option<&[Stmt]>,
    entry: &LocalFacts,
    properties: &PropertyFacts,
    profile: SemanticProfile,
    current_class: Option<&str>,
) -> Result<LoopBreakFlow, CompileError> {
    let condition_flow = validate_condition_flow(
        condition,
        entry,
        properties,
        profile,
        current_class,
    )?;
    let mut exits = Vec::new();
    let mut continues = Vec::new();
    let mut outer_jumps = Vec::new();
    let mut terminals = Vec::new();
    let mut fallthroughs = Vec::new();
    let mut audit_exits = Vec::new();
    let mut complete = true;
    let then_flow = collect_loop_break_flow(
        then_body,
        &condition_flow.when_true,
        properties,
        profile,
        current_class,
    )?;
    if condition_flow.true_reachable {
        audit_exits.push(then_flow.audit_exit.clone());
        exits.extend(then_flow.exits);
        continues.extend(then_flow.continues);
        outer_jumps.extend(then_flow.outer_jumps);
        terminals.extend(then_flow.terminals);
        complete &= then_flow.complete;
        fallthroughs.push(then_flow.fallthrough);
    }
    let mut alternative = condition_flow.when_false;
    let mut alternative_reachable = condition_flow.false_reachable;
    for (condition, body) in elseif_clauses {
        let condition_flow = validate_condition_flow(
            condition,
            &alternative,
            properties,
            profile,
            current_class,
        )?;
        let body_flow = collect_loop_break_flow(
            body,
            &condition_flow.when_true,
            properties,
            profile,
            current_class,
        )?;
        if alternative_reachable && condition_flow.true_reachable {
            audit_exits.push(body_flow.audit_exit.clone());
            exits.extend(body_flow.exits);
            continues.extend(body_flow.continues);
            outer_jumps.extend(body_flow.outer_jumps);
            terminals.extend(body_flow.terminals);
            complete &= body_flow.complete;
            fallthroughs.push(body_flow.fallthrough);
        }
        alternative = condition_flow.when_false;
        alternative_reachable &= condition_flow.false_reachable;
    }
    if let Some(else_body) = else_body {
        let else_flow = collect_loop_break_flow(
            else_body,
            &alternative,
            properties,
            profile,
            current_class,
        )?;
        if alternative_reachable {
            audit_exits.push(else_flow.audit_exit.clone());
            exits.extend(else_flow.exits);
            continues.extend(else_flow.continues);
            outer_jumps.extend(else_flow.outer_jumps);
            terminals.extend(else_flow.terminals);
            complete &= else_flow.complete;
            fallthroughs.push(else_flow.fallthrough);
        }
    } else if alternative_reachable {
        audit_exits.push(alternative.clone());
        fallthroughs.push(Some(alternative));
    }
    let audit_exit = if audit_exits.is_empty() {
        entry.clone()
    } else {
        merge_exits(&audit_exits)
    };
    Ok(LoopBreakFlow {
        fallthrough: merge_optional_exits(fallthroughs),
        exits,
        continues,
        outer_jumps,
        terminals: coalesce_terminal_flows(terminals),
        complete,
        audit_exit,
    })
}

/// Merges the fallthrough states that remain reachable after a control-flow construct.
fn merge_optional_exits(
    exits: impl IntoIterator<Item = Option<LocalFacts>>,
) -> Option<LocalFacts> {
    let exits: Vec<_> = exits.into_iter().flatten().collect();
    (!exits.is_empty()).then(|| merge_exits(&exits))
}

/// Collapses facts for each terminal category before an enclosing loop can clone them.
fn coalesce_terminal_flows(flows: Vec<TerminalFlow>) -> Vec<TerminalFlow> {
    let mut coalesced: Vec<TerminalFlow> = Vec::new();
    for flow in flows {
        if let Some(existing) = coalesced
            .iter_mut()
            .find(|existing| existing.kind == flow.kind)
        {
            existing.facts = merge_exits(&[existing.facts.clone(), flow.facts]);
        } else {
            coalesced.push(flow);
        }
    }
    coalesced
}

/// Merges normal loop exits with exact break snapshots and degrades only unmodeled writes.
fn merge_loop_exits(
    body: &[Stmt],
    condition: Option<&Expr>,
    update: Option<&Stmt>,
    normal_exits: &[LocalFacts],
    break_flows: &[&LoopBreakFlow],
    fallback: &LocalFacts,
    properties: &PropertyFacts,
    profile: SemanticProfile,
    current_class: Option<&str>,
) -> LocalFacts {
    let mut exits = normal_exits.to_vec();
    for flow in break_flows {
        exits.extend(flow.exits.iter().cloned());
    }
    if exits.is_empty() {
        let back_edges: Vec<_> = break_flows
            .iter()
            .filter_map(|flow| flow.back_edge())
            .collect();
        return if back_edges.is_empty() {
            fallback.clone()
        } else {
            merge_exits(&back_edges)
        };
    }
    let mut merged = merge_exits(&exits);
    if break_flows.iter().any(|flow| !flow.complete) {
        for (name, fact) in &mut merged.values {
            let body_writes = crate::ir_lower::body_writes_local(body, name);
            let condition_writes = condition
                .is_some_and(|condition| crate::ir_lower::expr_writes_local(condition, name));
            let update_writes =
                update.is_some_and(|update| crate::ir_lower::stmt_writes_local(update, name));
            if body_writes
                && !condition_writes
                && !update_writes
                && body_writes_only_static_fact(
                    body,
                    name,
                    &fallback.get(name),
                    properties,
                    profile,
                    current_class,
                )
            {
                *fact = fallback.get(name);
            } else if body_writes || condition_writes || update_writes {
                *fact = ValueFact::Unknown;
            }
        }
    }
    merged
}

/// Proves that every lexical write to one local stores the same static fact.
///
/// This is deliberately narrower than general flow inference. It preserves facts
/// when exact path snapshots are unavailable and proves stable reference-foreach
/// stores. It accepts constant assignments plus control-flow wrappers around them;
/// any dynamic expression, aliasing write, call, or container mutation fails closed.
fn body_writes_only_static_fact(
    body: &[Stmt],
    name: &str,
    expected: &ValueFact,
    properties: &PropertyFacts,
    profile: SemanticProfile,
    current_class: Option<&str>,
) -> bool {
    body.iter().all(|stmt| {
        if !crate::ir_lower::stmt_writes_local(stmt, name) {
            return true;
        }
        match &stmt.kind {
            StmtKind::Assign { name: target, value }
            | StmtKind::TypedAssign {
                name: target,
                value,
                ..
            } if target == name => {
                !crate::ir_lower::expr_writes_local(value, name)
                    && infer_expr(
                        value,
                        &LocalFacts::default(),
                        properties,
                        profile,
                        current_class,
                    ) == *expected
            }
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses,
                else_body,
            } => {
                !crate::ir_lower::expr_writes_local(condition, name)
                    && body_writes_only_static_fact(
                        then_body,
                        name,
                        expected,
                        properties,
                        profile,
                        current_class,
                    )
                    && elseif_clauses.iter().all(|(condition, body)| {
                        !crate::ir_lower::expr_writes_local(condition, name)
                            && body_writes_only_static_fact(
                                body,
                                name,
                                expected,
                                properties,
                                profile,
                                current_class,
                            )
                    })
                    && else_body.as_ref().is_none_or(|body| {
                        body_writes_only_static_fact(
                            body,
                            name,
                            expected,
                            properties,
                            profile,
                            current_class,
                        )
                    })
            }
            StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
                !crate::ir_lower::expr_writes_local(condition, name)
                    && body_writes_only_static_fact(
                        body,
                        name,
                        expected,
                        properties,
                        profile,
                        current_class,
                    )
            }
            StmtKind::For {
                init,
                condition,
                update,
                body,
            } => {
                init.as_deref()
                    .is_none_or(|stmt| !crate::ir_lower::stmt_writes_local(stmt, name))
                    && condition.as_ref().is_none_or(|condition| {
                        !crate::ir_lower::expr_writes_local(condition, name)
                    })
                    && update.as_deref().is_none_or(|stmt| {
                        !crate::ir_lower::stmt_writes_local(stmt, name)
                    })
                    && body_writes_only_static_fact(
                        body,
                        name,
                        expected,
                        properties,
                        profile,
                        current_class,
                    )
            }
            StmtKind::Foreach {
                array,
                key_var,
                value_var,
                body,
                ..
            } => {
                value_var != name
                    && key_var.as_deref() != Some(name)
                    && !crate::ir_lower::expr_writes_local(array, name)
                    && body_writes_only_static_fact(
                        body,
                        name,
                        expected,
                        properties,
                        profile,
                        current_class,
                    )
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                body_writes_only_static_fact(
                    then_body,
                    name,
                    expected,
                    properties,
                    profile,
                    current_class,
                ) && else_body.as_ref().is_none_or(|body| {
                    body_writes_only_static_fact(
                        body,
                        name,
                        expected,
                        properties,
                        profile,
                        current_class,
                    )
                })
            }
            StmtKind::Synthetic(body)
            | StmtKind::NamespaceBlock { body, .. }
            | StmtKind::IncludeOnceGuard { body, .. } => body_writes_only_static_fact(
                body,
                name,
                expected,
                properties,
                profile,
                current_class,
            ),
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                body_writes_only_static_fact(
                    try_body,
                    name,
                    expected,
                    properties,
                    profile,
                    current_class,
                ) && catches.iter().all(|catch| {
                    body_writes_only_static_fact(
                        &catch.body,
                        name,
                        expected,
                        properties,
                        profile,
                        current_class,
                    )
                }) && finally_body.as_ref().is_none_or(|body| {
                    body_writes_only_static_fact(
                        body,
                        name,
                        expected,
                        properties,
                        profile,
                        current_class,
                    )
                })
            }
            _ => false,
        }
    })
}

/// Keeps the strongest shared fact across every possible control-flow exit.
fn merge_exits(exits: &[LocalFacts]) -> LocalFacts {
    let mut names = BTreeSet::new();
    for exit in exits {
        names.extend(exit.values.keys().cloned());
    }
    let mut merged = LocalFacts::default();
    merged.ordinary_references.extend(
        exits
            .iter()
            .flat_map(|exit| exit.ordinary_references.iter().cloned()),
    );
    for name in names {
        let fact = exits
            .iter()
            .map(|exit| exit.get(&name))
            .reduce(merge_value_facts)
            .unwrap_or(ValueFact::Unknown);
        merged.set(&name, fact);
    }
    let aliases: BTreeSet<_> = exits
        .iter()
        .flat_map(|exit| exit.foreach_references.keys().cloned())
        .collect();
    for alias in aliases {
        let references: Vec<_> = exits
            .iter()
            .filter_map(|exit| exit.foreach_references.get(&alias))
            .collect();
        let sources: BTreeSet<_> = references
            .iter()
            .map(|reference| reference.source.clone())
            .collect();
        if sources.len() == 1 {
            merged.foreach_references.insert(
                alias,
                ForeachReference {
                    source: sources.into_iter().next().expect("one reference source"),
                    exact_last: references.iter().all(|reference| reference.exact_last),
                    definite: references.len() == exits.len()
                        && references.iter().all(|reference| reference.definite),
                },
            );
        } else {
            for source in sources {
                if matches!(merged.get(&source), ValueFact::Array(_)) {
                    merged.set(&source, ValueFact::Known(PhpType::php_array()));
                }
            }
        }
    }
    merged
}

/// Seeds callable locals from their written parameter contracts.
fn parameter_facts(
    params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
    variadic: Option<&str>,
) -> LocalFacts {
    let mut facts = LocalFacts::default();
    for (name, hint, _, _) in params {
        let fact = hint.as_ref().map_or(ValueFact::Unknown, |hint| {
            ValueFact::Known(crate::types::constant_binding::declared_type(hint))
        });
        facts.set(name, fact);
    }
    if let Some(name) = variadic {
        // The tail is always an array. Its integer and named slots depend on the call.
        facts.set(name, ValueFact::Known(PhpType::php_array()));
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
        let mut locals = parameter_facts(&method.params, method.variadic.as_deref());
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
    capture_facts: Option<&LocalFacts>,
    properties: &PropertyFacts,
    profile: SemanticProfile,
    current_class: Option<&str>,
) -> Result<(), CompileError> {
    let ExprKind::Closure {
        params,
        variadic,
        captures,
        capture_refs,
        body,
        ..
    } = &value.kind
    else {
        return Ok(());
    };
    let mut locals = parameter_facts(params, variadic.as_deref());
    for name in captures {
        locals.set(
            name,
            capture_facts.map_or(ValueFact::Unknown, |facts| facts.get(name)),
        );
    }
    for name in capture_refs {
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
    match &expr.kind {
        ExprKind::ArrayLiteral(values) => {
            return inferred_indexed_array_fact(
                values,
                locals,
                properties,
                profile,
                current_class,
            );
        }
        ExprKind::ArrayLiteralAssoc(entries) => {
            return inferred_assoc_array_fact(
                entries,
                locals,
                properties,
                profile,
                current_class,
            );
        }
        ExprKind::ArrayLiteralMixed(entries) => {
            return inferred_mixed_array_fact(
                entries,
                locals,
                properties,
                profile,
                current_class,
            );
        }
        ExprKind::Spread(inner) => {
            return infer_expr(inner, locals, properties, profile, current_class);
        }
        _ => {}
    }
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
        ExprKind::Assignment { value, .. } => {
            infer_expr(value, locals, properties, profile, current_class)
        }
        ExprKind::StaticPropertyAccess { receiver, property } => {
            static_property_fact(receiver, property, properties, current_class)
        }
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => statically_selected_source_expr(expr).map_or_else(
            || {
                merge_value_facts(
                    infer_expr(then_expr, locals, properties, profile, current_class),
                    infer_expr(else_expr, locals, properties, profile, current_class),
                )
            },
            |selected| infer_expr(selected, locals, properties, profile, current_class),
        ),
        ExprKind::ShortTernary { value, default }
        | ExprKind::NullCoalesce { value, default } => statically_selected_source_expr(expr)
            .map_or_else(
                || {
                    merge_value_facts(
                        infer_expr(value, locals, properties, profile, current_class),
                        infer_expr(default, locals, properties, profile, current_class),
                    )
                },
                |selected| infer_expr(selected, locals, properties, profile, current_class),
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
///
/// An exact non-array alternative cannot participate in array-key validation, so an exact array
/// shape remains authoritative for that audit. An unknown alternative still widens the result.
fn merge_value_facts(left: ValueFact, right: ValueFact) -> ValueFact {
    if left == right {
        left
    } else if let (ValueFact::Array(left), ValueFact::Array(right)) = (&left, &right) {
        merge_array_facts(left, right)
            .map(ValueFact::Array)
            .unwrap_or_else(|| ValueFact::Known(PhpType::php_array()))
    } else if left.is_write_array_parent() && right.is_write_array_parent() {
        merge_write_parent_facts(left, right)
    } else if matches!(&left, ValueFact::Array(_))
        && right.value_type().is_some_and(|ty| !ty.is_php_array())
    {
        left
    } else if matches!(&right, ValueFact::Array(_))
        && left.value_type().is_some_and(|ty| !ty.is_php_array())
    {
        right
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

/// Merges equal-key shapes recursively while retaining their common projection depth.
fn merge_array_facts(left: &ArrayFact, right: &ArrayFact) -> Option<ArrayFact> {
    merge_array_facts_with(left, right, merge_value_facts)
}

/// Merges facts that every candidate can autovivify into a nested array parent.
fn merge_write_parent_facts(left: ValueFact, right: ValueFact) -> ValueFact {
    if !left.is_write_array_parent() || !right.is_write_array_parent() {
        return merge_value_facts(left, right);
    }
    let left = write_parent_fact(left, true);
    let right = write_parent_fact(right, true);
    let (ValueFact::Array(left), ValueFact::Array(right)) = (&left, &right) else {
        return ValueFact::Known(PhpType::php_array());
    };
    ValueFact::Array(merge_write_parent_arrays(left, right))
}

/// Unifies write-parent slots, treating an absent branch slot as autovivifiable storage.
fn merge_write_parent_arrays(left: &ArrayFact, right: &ArrayFact) -> ArrayFact {
    let mut keys: Vec<_> = left.entries.iter().map(|(key, _)| key.clone()).collect();
    for (key, _) in &right.entries {
        if !keys.contains(key) {
            keys.push(key.clone());
        }
    }
    let entries = keys
        .into_iter()
        .map(|key| {
            let left = left
                .entries
                .iter()
                .find(|(candidate, _)| candidate == &key)
                .map(|(_, value)| value.clone())
                .unwrap_or_else(|| missing_element(true));
            let right = right
                .entries
                .iter()
                .find(|(candidate, _)| candidate == &key)
                .map(|(_, value)| value.clone())
                .unwrap_or_else(|| missing_element(true));
            (key, merge_write_parent_facts(left, right))
        })
        .collect();
    ArrayFact {
        entries,
        maximum_integer: match (left.maximum_integer, right.maximum_integer) {
            (Some(left), Some(right)) => Some(left.max(right)),
            (left, right) => left.or(right),
        },
    }
}

/// Merges equal-key shapes with the value policy required by their projection context.
fn merge_array_facts_with(
    left: &ArrayFact,
    right: &ArrayFact,
    merge: fn(ValueFact, ValueFact) -> ValueFact,
) -> Option<ArrayFact> {
    if left.maximum_integer != right.maximum_integer || left.entries.len() != right.entries.len() {
        return None;
    }
    let mut entries = Vec::with_capacity(left.entries.len());
    for ((left_key, left_value), (right_key, right_value)) in
        left.entries.iter().zip(&right.entries)
    {
        if left_key != right_key {
            return None;
        }
        entries.push((
            left_key.clone(),
            merge(left_value.clone(), right_value.clone()),
        ));
    }
    Some(ArrayFact {
        entries,
        maximum_integer: left.maximum_integer,
    })
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

/// Builds a complete indexed literal shape from values inferred in the current flow state.
fn inferred_indexed_array_fact(
    values: &[Expr],
    locals: &LocalFacts,
    properties: &PropertyFacts,
    profile: SemanticProfile,
    current_class: Option<&str>,
) -> ValueFact {
    let mut array = ArrayFact::empty();
    for value in values {
        if let ExprKind::Spread(source) = &value.kind {
            let ValueFact::Array(source) =
                infer_expr(source, locals, properties, profile, current_class)
            else {
                return ValueFact::Known(PhpType::php_array());
            };
            if array.spread(&source).is_none() {
                return ValueFact::Known(PhpType::php_array());
            }
        } else if array
            .append(infer_expr(
                value,
                locals,
                properties,
                profile,
                current_class,
            ))
            .is_none()
        {
            return ValueFact::Known(PhpType::php_array());
        }
    }
    ValueFact::Array(array)
}

/// Builds an explicitly keyed literal shape from values inferred in the current flow state.
fn inferred_assoc_array_fact(
    entries: &[(Expr, Expr)],
    locals: &LocalFacts,
    properties: &PropertyFacts,
    profile: SemanticProfile,
    current_class: Option<&str>,
) -> ValueFact {
    let mut array = ArrayFact::empty();
    for (key, value) in entries {
        let Some(key) = source_array_key(key, profile) else {
            return ValueFact::Known(PhpType::php_array());
        };
        array.insert(
            key,
            infer_expr(value, locals, properties, profile, current_class),
        );
    }
    ValueFact::Array(array)
}

/// Builds an ordered mixed literal shape from values inferred in the current flow state.
fn inferred_mixed_array_fact(
    entries: &[ArrayEntry],
    locals: &LocalFacts,
    properties: &PropertyFacts,
    profile: SemanticProfile,
    current_class: Option<&str>,
) -> ValueFact {
    if let Some(array) = planned_mixed_array_fact(entries, profile, |value| {
        infer_expr(value, locals, properties, profile, current_class)
    }) {
        return ValueFact::Array(array);
    }
    let mut array = ArrayFact::empty();
    for entry in entries {
        let success = match entry {
            ArrayEntry::Spread(source) => {
                let source = match &source.kind {
                    ExprKind::Spread(source) => source.as_ref(),
                    _ => source,
                };
                let ValueFact::Array(source) =
                    infer_expr(source, locals, properties, profile, current_class)
                else {
                    return ValueFact::Known(PhpType::php_array());
                };
                array.spread(&source)
            }
            ArrayEntry::Value(value) | ArrayEntry::Reference(value) => {
                array.append(infer_expr(
                    value,
                    locals,
                    properties,
                    profile,
                    current_class,
                ))
            }
            ArrayEntry::Keyed(key, value) | ArrayEntry::KeyReference(key, value) => {
                let Some(key) = source_array_key(key, profile) else {
                    return ValueFact::Known(PhpType::php_array());
                };
                array.insert(
                    key,
                    infer_expr(value, locals, properties, profile, current_class),
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

/// Builds a mixed literal fact from the same versioned keys that constant folding will emit.
fn planned_mixed_array_fact(
    entries: &[ArrayEntry], profile: SemanticProfile, mut value_fact: impl FnMut(&Expr) -> ValueFact,
) -> Option<ArrayFact> {
    let php_version = crate::codegen_support::compile_php_version();
    let mut array = ArrayFact::empty();
    for entry in entries {
        let (key, value) = match entry {
            ArrayEntry::Value(value) => (
                ArrayKeyFact::Integer(crate::types::next_literal_array_index(
                    array.maximum_integer,
                    php_version,
                )?),
                value,
            ),
            ArrayEntry::Keyed(key, value) => {
                let folded = crate::optimize::fold_array_key_for_source(key, profile)?;
                let key = literal_array_key_fact(&folded, profile)?;
                (key, value)
            }
            ArrayEntry::Spread(_)
            | ArrayEntry::Reference(_)
            | ArrayEntry::KeyReference(_, _) => return None,
        };
        array.insert(key, value_fact(value));
    }
    Some(array)
}

/// Converts one static key through the shared literal-array key classifier.
/// Under exact keys, string `"0"` deliberately remains distinct from integer `0`; an integer
/// lookup is then a known missing slot whose nested write still receives array-key validation.
fn source_array_key(expr: &Expr, profile: SemanticProfile) -> Option<ArrayKeyFact> {
    let selected = statically_selected_array_key_expr(expr, profile)?;
    literal_array_key_fact(&selected, profile)
}

/// Converts one literal key into the exact source fact shared by both planning paths.
fn literal_array_key_fact(expr: &Expr, profile: SemanticProfile) -> Option<ArrayKeyFact> {
    match crate::types::literal_array_key(expr, profile)? {
        StringKey::Integer(value) => Some(ArrayKeyFact::Integer(value)),
        StringKey::String(value) => Some(ArrayKeyFact::String(
            String::from_utf8_lossy(value).into_owned(),
        )),
    }
}
