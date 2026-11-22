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

    /// Appends one implicit value while preserving PHP 8.5 next-key history.
    fn append(&mut self, value: ValueFact) -> Option<()> {
        let key = match self.maximum_integer {
            None | Some(..0) => 0,
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
        _ => crate::types::deferred_defaults::literal_truthiness(expr),
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
            let when_true = right_flow.when_true;
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
            let when_false = right_flow.when_false;
            Ok(ConditionFlow {
                when_true,
                when_false,
                true_reachable: true_from_left || true_from_right,
                false_reachable: left_flow.false_reachable && right_flow.false_reachable,
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
            Ok(match &value.kind {
                ExprKind::Null => default_exit,
                ExprKind::BoolLiteral(_)
                | ExprKind::IntLiteral(_)
                | ExprKind::FloatLiteral(_)
                | ExprKind::StringLiteral(_)
                | ExprKind::ArrayLiteral(_)
                | ExprKind::ArrayLiteralAssoc(_)
                | ExprKind::ArrayLiteralMixed(_) => value_exit,
                _ => merge_exits(&[value_exit, default_exit]),
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
            let default_runs = match &value.kind {
                ExprKind::Null => Some(true),
                ExprKind::BoolLiteral(_)
                | ExprKind::IntLiteral(_)
                | ExprKind::FloatLiteral(_)
                | ExprKind::StringLiteral(_)
                | ExprKind::ArrayLiteral(_)
                | ExprKind::ArrayLiteralAssoc(_)
                | ExprKind::ArrayLiteralMixed(_) => Some(false),
                _ => None,
            };
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
        locals.set(name, value);
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
    locals.set(
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
    if !matches!(&stmt.kind, StmtKind::DoWhile { .. }) {
        validate_expression_writes(stmt, locals, properties, profile, current_class)?;
    }
    match &stmt.kind {
        StmtKind::Assign { name, value } => {
            let value =
                apply_expression_facts(value, locals, properties, profile, current_class);
            locals.set(name, value);
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
            locals.set(array, array_write_fact(receiver, index, value));
        }
        StmtKind::ArrayPush { array, value } => {
            let value =
                apply_expression_facts(value, locals, properties, profile, current_class);
            let receiver = locals.get(array);
            locals.set(array, array_push_fact(receiver, value));
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
            if condition_flow.true_reachable {
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
                if fallthrough_reachable && condition_flow.true_reachable {
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
                if fallthrough_reachable {
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
        StmtKind::While { condition, body } => {
            let first_flow = validate_condition_flow(
                condition,
                locals,
                properties,
                profile,
                current_class,
            )?;
            let first_body_entry = first_flow.when_true;
            let first_exit = first_flow.when_false;
            let first_body =
                validate_branch(body, &first_body_entry, properties, profile, current_class)?;
            let repeated_flow = validate_condition_flow(
                condition,
                &first_body,
                properties,
                profile,
                current_class,
            )?;
            let repeated_body_entry = repeated_flow.when_true;
            let repeated_exit = repeated_flow.when_false;
            validate_branch(
                body,
                &repeated_body_entry,
                properties,
                profile,
                current_class,
            )?;
            *locals = merge_loop_exits(
                body,
                Some(condition),
                None,
                &[first_exit, repeated_exit],
            );
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
            *locals = merge_loop_exits(body, None, None, &[entry, iteration]);
        }
        StmtKind::DoWhile { body, condition } => {
            validate_block(body, locals, properties, profile, current_class)?;
            let first_flow = validate_condition_flow(
                condition,
                locals,
                properties,
                profile,
                current_class,
            )?;
            let first_exit = first_flow.when_false;
            let mut repeated_body = first_flow.when_true;
            validate_block(
                body,
                &mut repeated_body,
                properties,
                profile,
                current_class,
            )?;
            let repeated_exit = validate_condition_flow(
                condition,
                &repeated_body,
                properties,
                profile,
                current_class,
            )?
            .when_false;
            *locals = merge_loop_exits(
                body,
                Some(condition),
                None,
                &[first_exit, repeated_exit],
            );
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                validate_stmt(init, locals, properties, profile, current_class)?;
            }
            let first_flow = match condition {
                Some(condition) => validate_condition_flow(
                    condition,
                    locals,
                    properties,
                    profile,
                    current_class,
                )?,
                None => ConditionFlow {
                    when_true: locals.clone(),
                    when_false: locals.clone(),
                    true_reachable: true,
                    false_reachable: false,
                },
            };
            let first_body_entry = first_flow.when_true;
            let first_exit = first_flow.when_false;
            let mut first_body =
                validate_branch(body, &first_body_entry, properties, profile, current_class)?;
            if let Some(update) = update {
                validate_stmt(
                    update,
                    &mut first_body,
                    properties,
                    profile,
                    current_class,
                )?;
            }
            let repeated_flow = match condition {
                Some(condition) => validate_condition_flow(
                    condition,
                    &first_body,
                    properties,
                    profile,
                    current_class,
                )?,
                None => ConditionFlow {
                    when_true: first_body.clone(),
                    when_false: first_body,
                    true_reachable: true,
                    false_reachable: false,
                },
            };
            let repeated_body_entry = repeated_flow.when_true;
            let repeated_exit = repeated_flow.when_false;
            validate_branch(
                body,
                &repeated_body_entry,
                properties,
                profile,
                current_class,
            )?;
            *locals = merge_loop_exits(
                body,
                condition.as_ref(),
                update.as_deref(),
                &[first_exit, repeated_exit],
            );
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

/// Merges loop fallthroughs unless a break can bypass the modeled backedge state.
fn merge_loop_exits(
    body: &[Stmt],
    condition: Option<&Expr>,
    update: Option<&Stmt>,
    exits: &[LocalFacts],
) -> LocalFacts {
    if crate::termination::block_may_break_current_loop(body) {
        let mut merged = merge_exits(exits);
        for (name, fact) in &mut merged.values {
            let bypassed_write = crate::ir_lower::body_writes_local(body, name)
                || condition
                    .is_some_and(|condition| crate::ir_lower::expr_writes_local(condition, name))
                || update.is_some_and(|update| crate::ir_lower::stmt_writes_local(update, name));
            if bypassed_write {
                *fact = ValueFact::Unknown;
            }
        }
        return merged;
    }
    merge_exits(exits)
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
    capture_facts: Option<&LocalFacts>,
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
            condition,
            then_expr,
            else_expr,
        } => match source_literal_truthiness(condition) {
            Some(true) => infer_expr(then_expr, locals, properties, profile, current_class),
            Some(false) => infer_expr(else_expr, locals, properties, profile, current_class),
            None => merge_value_facts(
                infer_expr(then_expr, locals, properties, profile, current_class),
                infer_expr(else_expr, locals, properties, profile, current_class),
            ),
        },
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
    } else if let (ValueFact::Array(left), ValueFact::Array(right)) = (&left, &right) {
        merge_array_facts(left, right)
            .map(ValueFact::Array)
            .unwrap_or_else(|| ValueFact::Known(PhpType::php_array()))
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
    merge_array_facts_with(left, right, merge_write_parent_facts)
        .map(ValueFact::Array)
        .unwrap_or_else(|| ValueFact::Known(PhpType::php_array()))
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
