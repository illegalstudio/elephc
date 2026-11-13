//! Purpose:
//! Rejects provable checked integer overflow before source pruning erases the operation.
//!
//! Called from:
//! - Source semantic validation, including recursively parsed literal eval fragments.
//!
//! Key details:
//! - Memoized evidence preserves exact integers instead of evaluating through f64.
//! - Uncertain bindings and cyclic constants remain unknown rather than becoming guessed values.

use std::collections::{HashMap, HashSet};
use crate::ast_visit::{walk_program, AstVisitor};
use crate::errors::CompileError;
use crate::parser::ast::{BinOp, Expr, ExprKind, Program, StaticReceiver, Stmt, StmtKind};
use crate::source::SourceMode;
use elephc_semantics::{SemanticProfile, StrictFeature};
use elephc_semantics::numeric::{checked_integer, IntegerOp, NumericFailure, INTEGER_OVERFLOW_MESSAGE};

/// Numeric evidence retains the PHP value kind of every proven intermediate.
#[derive(Clone, Copy)]
enum Number {
    Int(i64), Float(f64), Bool(bool), Null,
    StringInt { value: i64, fully_numeric: bool },
    StringFloat { value: f64, fully_numeric: bool },
    NonNumericString,
    /// A known array kind whose payload is outside scalar numeric evidence.
    Array,
}

impl Number {
    /// Retains source PHP kinds before arithmetic normalization can erase them.
    fn value_kind(self) -> elephc_semantics::conversions::ValueKind {
        use elephc_semantics::conversions::ValueKind;
        match self {
            Self::Int(_) => ValueKind::Integer, Self::Float(_) => ValueKind::Float,
            Self::Bool(_) => ValueKind::Boolean, Self::Null => ValueKind::Null,
            Self::StringInt { .. } | Self::StringFloat { .. } | Self::NonNumericString => ValueKind::String,
            Self::Array => ValueKind::NonScalar,
        }
    }

    /// Adapts an operand only after its operator has selected a floating result.
    fn as_float(self) -> f64 {
        match self.numeric() { Self::Int(value) => value as f64, Self::Float(value) => value, _ => unreachable!() }
    }

    /// Applies arithmetic string conversion only after preserving parameter-domain evidence.
    fn numeric(self) -> Self {
        match self { Self::StringInt { value, .. } => Self::Int(value), Self::StringFloat { value, .. } => Self::Float(value),
            Self::Bool(value) => Self::Int(i64::from(value)), Self::Null => Self::Int(0), value => value }
    }

    /// Builtin numeric parameters accept full numeric strings rather than arithmetic prefixes.
    fn valid_numeric_parameter(self, strict_types: bool) -> bool {
        match self { Self::NonNumericString | Self::Array => false,
            Self::StringInt { fully_numeric, .. } | Self::StringFloat { fully_numeric, .. } => fully_numeric && !strict_types,
            Self::Bool(_) | Self::Null => !strict_types, _ => true }
    }

    /// Integer parameters reject float arguments in strict PHP and out-of-range weak conversions.
    fn integer_parameter(self, strict_types: bool) -> Option<i64> {
        if strict_types { return if let Self::Int(value) = self { Some(value) } else { None }; }
        if !self.valid_numeric_parameter(false) { return None; }
        match self.numeric() {
            Self::Int(value) => Some(value),
            Self::Float(value) => elephc_semantics::numeric::checked_float_to_int(value).ok(),
            _ => unreachable!("numeric normalization removes source-kind evidence"),
        }
    }
}

/// Distinguishes case-sensitive global/constant names from case-insensitive class names.
#[derive(Clone, PartialEq, Eq, Hash)]
enum ConstantKey { Global(String), Class(String, String) }

/// An unconditional declaration's expression and its lexical implementation policy.
struct Definition { value: Expr, mode: SourceMode, strict_types: bool }

/// Owns stable expression addresses and marks duplicate declarations as ambiguous.
#[derive(Default)]
struct Definitions {
    values: HashMap<ConstantKey, Option<Definition>>,
    unresolved_namespaces: bool,
    functions: HashSet<String>,
}

impl Definitions {
    /// Collects only unconditional declarations; name resolution supplies canonical keys later.
    fn collect(&mut self, program: &[Stmt]) {
        for stmt in program {
            match &stmt.kind {
                StmtKind::Synthetic(body) => self.collect(body),
                StmtKind::NamespaceBlock { .. } | StmtKind::NamespaceDecl { .. } => self.unresolved_namespaces = true,
                StmtKind::ConstDecl { name, value } => self.insert(ConstantKey::Global(name.trim_start_matches('\\').to_string()), value, stmt.source_mode, stmt.strict_types),
                StmtKind::ClassDecl { name, constants, .. } | StmtKind::InterfaceDecl { name, constants, .. }
                | StmtKind::EnumDecl { name, constants, .. } => {
                    for constant in constants {
                        self.insert(ConstantKey::Class(crate::names::php_symbol_key(name.trim_start_matches('\\')), constant.name.clone()),
                            &constant.value, stmt.source_mode, stmt.strict_types);
                    }
                }
                _ => {}
            }
        }
    }

    /// Refuses to infer a unique value when more than one declaration owns the same key.
    fn insert(&mut self, key: ConstantKey, value: &Expr, mode: SourceMode, strict_types: bool) {
        use std::collections::hash_map::Entry;
        match self.values.entry(key) {
            Entry::Vacant(entry) => { entry.insert(Some(Definition { value: value.clone(), mode, strict_types })); }
            Entry::Occupied(mut entry) => { entry.insert(None); }
        }
    }
}

/// Prevents a PHP builtin proof from overriding any source or extern declaration's binding.
struct FunctionDeclarations<'a>(&'a mut HashSet<String>);

impl AstVisitor for FunctionDeclarations<'_> {
    /// Collects forward and conditional declarations before any call is interpreted as a builtin.
    fn statement(&mut self, stmt: &Stmt) {
        if let StmtKind::FunctionDecl { name, .. } | StmtKind::ExternFunctionDecl { name, .. } = &stmt.kind {
            self.0.insert(crate::names::php_symbol_key(name.trim_start_matches('\\')));
        }
    }
}

/// Retains evidence only for the lifetime of one immutable source tree and its owned definitions.
#[derive(Default)]
struct Evidence {
    profile: SemanticProfile,
    values: HashMap<(usize, bool, bool), Option<Number>>,
    active: HashSet<ConstantKey>,
    strict_types: bool,
}

/// Checks all syntactic branches without sharing address-based memo entries between source trees.
pub(super) fn validate(program: &Program, profile: SemanticProfile) -> Result<(), CompileError> {
    if ![StrictFeature::Overflow, StrictFeature::Arithmetic, StrictFeature::Coercions].into_iter()
        .any(|feature| profile.contains(feature)) { return Ok(()); }
    let mut definitions = Definitions::default();
    definitions.collect(program);
    walk_program(program, &mut FunctionDeclarations(&mut definitions.functions));
    let mut visitor = NumericValidator { definitions, evidence: Evidence { profile, ..Default::default() }, scopes: Vec::new(), error: None };
    walk_program(program, &mut visitor);
    visitor.error.map_or(Ok(()), Err)
}

/// Applies each operation's physical source mode and lexical class context.
struct NumericValidator {
    definitions: Definitions,
    evidence: Evidence,
    scopes: Vec<(SourceMode, Option<String>, bool)>,
    error: Option<CompileError>,
}

impl AstVisitor for NumericValidator {
    /// Tracks nested implementation statements without exempting their user children.
    fn statement(&mut self, stmt: &Stmt) {
        let class = match &stmt.kind {
            StmtKind::ClassDecl { name, .. } | StmtKind::InterfaceDecl { name, .. } | StmtKind::EnumDecl { name, .. } =>
                Some(crate::names::php_symbol_key(name.trim_start_matches('\\'))),
            _ => self.scopes.last().and_then(|(_, class, _)| class.clone()),
        };
        self.scopes.push((stmt.source_mode, class, stmt.strict_types));
    }

    /// Restores the lexical class after a declaration or nested statement finishes.
    fn statement_end(&mut self, _stmt: &Stmt) { self.scopes.pop(); }

    /// Evaluates only proven scalar evidence, leaving unknown operands to runtime guards.
    fn expression(&mut self, expr: &Expr) {
        if self.error.is_some() { return; }
        let (mode, class, strict_types) = self.scopes.last().expect("expressions have an enclosing statement");
        if *mode == SourceMode::Internal { return; }
        self.evidence.strict_types = *strict_types;
        if let Err(error) = evaluate(expr, true, class.as_deref(), &self.definitions, &mut self.evidence) {
            self.error = Some(error);
        }
    }
}

/// Resolves each immutable expression once under its operation policy.
fn evaluate(expr: &Expr, checked: bool, class: Option<&str>, definitions: &Definitions, evidence: &mut Evidence) -> Result<Option<Number>, CompileError> {
    let key = (std::ptr::from_ref(expr) as usize, checked, evidence.strict_types);
    if let Some(value) = evidence.values.get(&key) { return Ok(*value); }
    let value = match &expr.kind {
        ExprKind::IntLiteral(value) => Some(Number::Int(*value)),
        ExprKind::FloatLiteral(value) => Some(Number::Float(*value)),
        ExprKind::BoolLiteral(value) => Some(Number::Bool(*value)),
        ExprKind::Null => Some(Number::Null),
        ExprKind::StringLiteral(value) => string_number(value),
        ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) | ExprKind::ArrayLiteralMixed(_) => Some(Number::Array),
        ExprKind::ConstRef(name) if !definitions.unresolved_namespaces => {
            let name = name.as_str().trim_start_matches('\\');
            let key = ConstantKey::Global(name.to_string());
            if definitions.values.contains_key(&key) { constant(&key, definitions, evidence)? }
            else { match name { "PHP_INT_MAX" => Some(Number::Int(i64::MAX)), "PHP_INT_MIN" => Some(Number::Int(i64::MIN)), _ => None } }
        }
        ExprKind::ScopedConstantAccess { receiver, name } if !definitions.unresolved_namespaces => {
            let class = match receiver {
                StaticReceiver::Named(name) => Some(crate::names::php_symbol_key(name.as_str().trim_start_matches('\\'))),
                StaticReceiver::Self_ => class.map(str::to_string),
                _ => None,
            };
            match class { Some(class) => constant(&ConstantKey::Class(class, name.clone()), definitions, evidence)?, None => None }
        }
        ExprKind::UnaryPlus(inner) => match unary_operand(expr, inner, checked, class, definitions, evidence,
            elephc_semantics::numeric_domains::NumericOperator::Unary)?.map(Number::numeric) {
            value @ Some(Number::Int(_) | Number::Float(_)) => value,
            _ => None,
        },
        ExprKind::Negate(inner) => match unary_operand(expr, inner, checked, class, definitions, evidence,
            elephc_semantics::numeric_domains::NumericOperator::Unary)?.map(Number::numeric) {
            Some(Number::Int(value)) => match value.checked_neg() {
                Some(value) => Some(Number::Int(value)),
                None if checked && evidence.profile.contains(StrictFeature::Overflow) => return Err(CompileError::new(expr.span, INTEGER_OVERFLOW_MESSAGE)),
                None => Some(Number::Float(-(value as f64))),
            },
            Some(Number::Float(value)) => Some(Number::Float(-value)),
            _ => None,
        },
        ExprKind::BitNot(inner) => match unary_operand(expr, inner, checked, class, definitions, evidence,
            elephc_semantics::numeric_domains::NumericOperator::BitNot)? {
            Some(Number::Int(value)) => Some(Number::Int(!value)), _ => None,
        },
        ExprKind::FunctionCall { name, args } if !definitions.unresolved_namespaces
            && !definitions.functions.contains(&crate::names::php_symbol_key(name.as_str().trim_start_matches('\\'))) =>
            builtin_number(expr, name.as_str(), args, checked, class, definitions, evidence)?,
        ExprKind::BinaryOp { left, op, right } if super::numeric::operator(op).is_some() => {
            let left = evaluate(left, checked, class, definitions, evidence)?;
            let right = evaluate(right, checked, class, definitions, evidence)?;
            match (left, right) { (Some(left), Some(right)) => binary(expr, op, left, right, checked, evidence.profile, true)?, _ => None }
        }
        ExprKind::ErrorSuppress(inner) => evaluate(inner, checked, class, definitions, evidence)?,
        _ => None,
    };
    evidence.values.insert(key, value);
    Ok(value)
}

/// Resolves forward aliases with an explicit cycle guard and the declaration's own source mode.
fn constant(key: &ConstantKey, definitions: &Definitions, evidence: &mut Evidence) -> Result<Option<Number>, CompileError> {
    let Some(Some(definition)) = definitions.values.get(key) else { return Ok(None); };
    if !evidence.active.insert(key.clone()) { return Ok(None); }
    let class = match key { ConstantKey::Class(class, _) => Some(class.as_str()), _ => None };
    let previous_strict_types = evidence.strict_types;
    evidence.strict_types = definition.strict_types;
    let result = evaluate(&definition.value, definition.mode != SourceMode::Internal, class, definitions, evidence);
    evidence.strict_types = previous_strict_types;
    evidence.active.remove(key);
    result
}

/// Audits a proven unary operand before conversion while preserving declaration-cycle evidence.
fn unary_operand(expr: &Expr, inner: &Expr, checked: bool, class: Option<&str>, definitions: &Definitions,
    evidence: &mut Evidence, operator: elephc_semantics::numeric_domains::NumericOperator) -> Result<Option<Number>, CompileError> {
    let value = evaluate(inner, checked, class, definitions, evidence)?;
    if checked {
        if let (Some(value), Some(rule)) = (value, super::numeric::operator_rule(evidence.profile, SourceMode::Php, operator)) {
            if !rule.domain.allows(value.value_kind()) { return Err(CompileError::new(expr.span, &rule.message())); }
        }
    }
    Ok(value)
}

/// Classifies PHP numeric strings without changing full-width integer values through float rounding.
fn string_number(value: &str) -> Option<Number> {
    let Some(scan) = crate::numeric_string::scan_numeric_prefix(value) else { return Some(Number::NonNumericString); };
    if !scan.is_float {
        if let Ok(value) = scan.text.parse::<i64>() { return Some(Number::StringInt { value, fully_numeric: scan.is_fully_numeric() }); }
    }
    scan.text.parse().ok().map(|value| Number::StringFloat { value, fully_numeric: scan.is_fully_numeric() })
}

/// Proves typed numeric builtin operations through the shared argument planner and runtime inventory.
fn builtin_number(
    expr: &Expr, name: &str, args: &[Expr], checked: bool, class: Option<&str>,
    definitions: &Definitions, evidence: &mut Evidence,
) -> Result<Option<Number>, CompileError> {
    use crate::builtins::semantics::BuiltinRuntimeFunctions;
    let Some(def) = crate::builtins::registry::lookup(name.trim_start_matches('\\')) else { return Ok(None); };
    let BuiltinRuntimeFunctions::One(operation) = def.spec.semantics.runtime_functions else { return Ok(None); };
    use crate::ir::RuntimeFnId;
    if !matches!(operation, RuntimeFnId::Abs | RuntimeFnId::Pow | RuntimeFnId::Intdiv
        | RuntimeFnId::ArraySum | RuntimeFnId::ArrayProduct) { return Ok(None); }
    // Only the profile-independent parameter shape is consumed here, never its result type.
    let Some(signature) = crate::builtins::registry::function_sig(def.name) else { return Ok(None); };
    let Ok(plan) = crate::types::call_args::plan_call_args(&signature, args, expr.span, true, false) else { return Ok(None); };
    // Validate unpack ordering before flattening a positional literal spread for evidence.
    let expanded = crate::types::call_args::expand_planned_positional_spreads(args);
    let normalized = if let Some(expanded) = expanded {
        let Ok(expanded_plan) = crate::types::call_args::plan_call_args(&signature, &expanded, expr.span, true, false)
            else { return Ok(None); };
        expanded_plan.normalized_args()
    } else { plan.normalized_args() };
    // The planner owns cloned AST nodes. Do not retain their addresses in the source-tree memo.
    // Carry the active declaration guard so a constant cycle through a builtin stays unknown.
    let mut call_evidence = Evidence { profile: evidence.profile, active: evidence.active.clone(), strict_types: evidence.strict_types, ..Default::default() };
    if matches!(operation, RuntimeFnId::ArraySum | RuntimeFnId::ArrayProduct) {
        let [argument] = normalized.as_slice() else { return Ok(None); };
        let rule = if checked { super::numeric::operator_rule(evidence.profile, SourceMode::Php,
            elephc_semantics::numeric_domains::NumericOperator::Arithmetic) } else { None };
        let Some(values) = array_numbers(argument, checked, class, definitions, &mut call_evidence, rule)? else { return Ok(None); };
        let product = operation == RuntimeFnId::ArrayProduct;
        let mut carry = Number::Int(i64::from(product));
        for value in values {
            let Some(next) = binary(expr, if product { &BinOp::Mul } else { &BinOp::Add }, carry, value, checked, evidence.profile, false)?
                else { return Ok(None); };
            carry = next;
        }
        return Ok(Some(carry));
    }
    if operation == RuntimeFnId::Intdiv {
        let [left, right] = normalized.as_slice() else { return Ok(None); };
        let left = evaluate(left, checked, class, definitions, &mut call_evidence)?;
        let right = evaluate(right, checked, class, definitions, &mut call_evidence)?;
        for value in [left, right].into_iter().flatten() {
            validate_numeric_parameter(expr, value, elephc_semantics::numeric_domains::NumericOperator::Modulo, checked, evidence.profile)?;
        }
        let (Some(left), Some(right)) = (left, right) else { return Ok(None); };
        let (Some(left), Some(right)) = (left.integer_parameter(evidence.strict_types),
            right.integer_parameter(evidence.strict_types)) else { return Ok(None); };
        return match checked_integer(IntegerOp::Div, left, right) {
            Ok(value) => Ok(Some(Number::Int(value))),
            Err(NumericFailure::Overflow) if checked && evidence.profile.contains(StrictFeature::Overflow) => Err(CompileError::new(expr.span, INTEGER_OVERFLOW_MESSAGE)),
            _ => Ok(None),
        };
    }
    if operation == crate::ir::RuntimeFnId::Pow {
        let [left, right] = normalized.as_slice() else { return Ok(None); };
        let Some(left) = evaluate(left, checked, class, definitions, &mut call_evidence)? else { return Ok(None); };
        let Some(right) = evaluate(right, checked, class, definitions, &mut call_evidence)? else { return Ok(None); };
        // Mixed pow parameters follow operator conversion even with PHP strict_types enabled.
        // Public numeric profiles share source kinds and integer-power domains with the operator.
        if checked { return binary(expr, &BinOp::Pow, left, right, checked, evidence.profile, true); }
        if matches!(left, Number::NonNumericString | Number::Array) || matches!(right, Number::NonNumericString | Number::Array) { return Ok(None); }
        return Ok(Some(Number::Float(left.as_float().powf(right.as_float()))));
    }
    let [argument] = normalized.as_slice() else { return Ok(None); };
    let Some(value) = evaluate(argument, checked, class, definitions, &mut call_evidence)? else { return Ok(None); };
    validate_numeric_parameter(expr, value, elephc_semantics::numeric_domains::NumericOperator::Unary, checked, evidence.profile)?;
    if !value.valid_numeric_parameter(evidence.strict_types) { return Ok(None); }
    Ok(match value.numeric() {
        Number::Int(value) => match value.checked_abs() {
            Some(value) => Some(Number::Int(value)),
            None if checked && evidence.profile.contains(StrictFeature::Overflow) => return Err(CompileError::new(expr.span, INTEGER_OVERFLOW_MESSAGE)),
            None => Some(Number::Float((value as f64).abs())),
        },
        Number::Float(value) => Some(Number::Float(value.abs())),
        _ => unreachable!("numeric normalization removes string evidence"),
    })
}

/// Rejects a proven original parameter kind before PHP numeric normalization can erase it.
fn validate_numeric_parameter(expr: &Expr, value: Number,
    operator: elephc_semantics::numeric_domains::NumericOperator, checked: bool, profile: SemanticProfile)
    -> Result<(), CompileError> {
    if checked {
        if let Some(rule) = super::numeric::operator_rule(profile, SourceMode::Php, operator) {
            if !rule.domain.allows(value.value_kind()) { return Err(CompileError::new(expr.span, &rule.message())); }
        }
    }
    Ok(())
}

/// Retains source evaluation, first-insertion order and last-write values through shared key planning.
fn array_numbers(
    expr: &Expr, checked: bool, class: Option<&str>, definitions: &Definitions, evidence: &mut Evidence,
    rule: Option<elephc_semantics::numeric_domains::NumericDomainRule>,
) -> Result<Option<Vec<Number>>, CompileError> {
    let profile = if checked { evidence.profile } else { SemanticProfile::PERMISSIVE };
    if let Some(entries) = crate::types::plan_literal_array_entries(&expr.kind,
        crate::codegen_support::compile_php_version(), profile) {
        let mut result: Vec<Option<Number>> = Vec::with_capacity(entries.len());
        let mut positions = HashMap::new();
        for entry in entries {
            let value = evaluate(entry.value, checked, class, definitions, evidence)?;
            if let Some(position) = positions.get(&entry.key).copied() { result[position] = value; }
            else {
                positions.insert(entry.key, result.len());
                result.push(value);
            }
        }
        if let Some(rule) = rule {
            for value in result.iter().flatten() {
                if !rule.domain.allows(value.value_kind()) { return Err(CompileError::new(expr.span, &rule.message())); }
            }
        }
        return Ok(result.into_iter().collect());
    }
    if definitions.unresolved_namespaces { return Ok(None); }
    let key = match &expr.kind {
        ExprKind::ConstRef(name) => ConstantKey::Global(name.as_str().trim_start_matches('\\').to_string()),
        ExprKind::ScopedConstantAccess { receiver, name } => {
            let class = match receiver {
                StaticReceiver::Named(name) => crate::names::php_symbol_key(name.as_str().trim_start_matches('\\')),
                StaticReceiver::Self_ => match class { Some(class) => class.to_string(), None => return Ok(None) },
                _ => return Ok(None),
            };
            ConstantKey::Class(class, name.clone())
        }
        _ => return Ok(None),
    };
    let Some(Some(definition)) = definitions.values.get(&key) else { return Ok(None); };
    if !evidence.active.insert(key.clone()) { return Ok(None); }
    let class = match &key { ConstantKey::Class(class, _) => Some(class.as_str()), _ => None };
    let previous = evidence.strict_types;
    evidence.strict_types = definition.strict_types;
    let result = array_numbers(&definition.value, definition.mode != SourceMode::Internal, class, definitions, evidence, rule);
    evidence.strict_types = previous;
    evidence.active.remove(&key);
    result
}

/// Applies checked integer kernels only when PHP would actually choose integer arithmetic.
fn binary(expr: &Expr, op: &BinOp, left: Number, right: Number, checked: bool, profile: SemanticProfile, source_operator: bool) -> Result<Option<Number>, CompileError> {
    // A literal-array union is an explicit nonnumeric overload, before scalar-domain checks.
    if source_operator && *op == BinOp::Add && matches!((left, right), (Number::Array, Number::Array)) {
        return Ok(Some(Number::Array));
    }
    if checked && source_operator {
        if let Some(rule) = super::numeric::operand_rule(profile, SourceMode::Php, op) {
            if !rule.domain.allows(left.value_kind()) || !rule.domain.allows(right.value_kind()) {
                return Err(CompileError::new(expr.span, &rule.message()));
            }
        }
        if matches!(op, BinOp::Pow) {
            if let (Number::Int(_), Number::Int(exponent)) = (left, right) {
                if elephc_semantics::numeric_domains::rejects_negative_integer_power(profile,
                    elephc_semantics::conversions::ValueKind::Integer, elephc_semantics::conversions::ValueKind::Integer, exponent) {
                    return Err(CompileError::new(expr.span, elephc_semantics::numeric_domains::NEGATIVE_INTEGER_POWER_MESSAGE));
                }
            }
        }
    }
    if let Some(operation) = super::numeric::integer_bit_op(op) {
        return Ok(match (left, right) {
            (Number::Int(left), Number::Int(right)) => elephc_semantics::numeric_domains::integer_bitwise(operation, left, right)
                .ok().map(Number::Int),
            _ => None,
        });
    }
    if matches!(left, Number::NonNumericString | Number::Array) || matches!(right, Number::NonNumericString | Number::Array) { return Ok(None); }
    let (left, right) = (left.numeric(), right.numeric());
    if checked && profile.contains(StrictFeature::Arithmetic) && matches!(op, BinOp::Div) {
        let (left, right) = (left.as_float(), right.as_float());
        return Ok((right != 0.0).then(|| Number::Float(left / right)));
    }
    if let (Number::Int(left), Number::Int(right)) = (left, right) {
        let operation = match op { BinOp::Add => Some(IntegerOp::Add), BinOp::Sub => Some(IntegerOp::Sub),
            BinOp::Mul => Some(IntegerOp::Mul), BinOp::Mod => Some(IntegerOp::Mod), BinOp::Pow if right >= 0 => Some(IntegerOp::Pow), _ => None };
        if let Some(operation) = operation {
            match checked_integer(operation, left, right) {
                Ok(value) => return Ok(Some(Number::Int(value))),
                Err(NumericFailure::Overflow) if checked && profile.contains(StrictFeature::Overflow) => return Err(CompileError::new(expr.span, INTEGER_OVERFLOW_MESSAGE)),
                Err(NumericFailure::DivisionByZero) => return Ok(None),
                // Internal power promotion has an exact PHP continuation, so do not approximate it.
                Err(_) if matches!(op, BinOp::Pow) => return Ok(None),
                Err(_) => {}
            }
        }
        if matches!(op, BinOp::Div) {
            if right == 0 { return Ok(None); }
            if left.checked_rem(right) == Some(0) {
                if let Some(value) = left.checked_div(right) { return Ok(Some(Number::Int(value))); }
            }
        }
    }
    let (left, right) = (left.as_float(), right.as_float());
    if matches!(op, BinOp::Div | BinOp::Mod) && right == 0.0 { return Ok(None); }
    let value = match op { BinOp::Add => left + right, BinOp::Sub => left - right, BinOp::Mul => left * right,
        BinOp::Div => left / right, BinOp::Pow => left.powf(right), _ => return Ok(None) };
    Ok(Some(Number::Float(value)))
}

#[cfg(test)]
#[path = "numeric_constants/tests.rs"]
mod tests;
