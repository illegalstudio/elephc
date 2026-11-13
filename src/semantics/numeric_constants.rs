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
}

impl Number {
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
        match self { Self::StringInt { fully_numeric, .. } | Self::StringFloat { fully_numeric, .. } => fully_numeric && !strict_types,
            Self::Bool(_) | Self::Null => !strict_types, _ => true }
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
    values: HashMap<(usize, bool, bool), Option<Number>>,
    active: HashSet<ConstantKey>,
    strict_types: bool,
}

/// Checks all syntactic branches without sharing address-based memo entries between source trees.
pub(super) fn validate(program: &Program, profile: SemanticProfile) -> Result<(), CompileError> {
    if !profile.contains(StrictFeature::Overflow) { return Ok(()); }
    let mut definitions = Definitions::default();
    definitions.collect(program);
    walk_program(program, &mut FunctionDeclarations(&mut definitions.functions));
    let mut visitor = NumericValidator { definitions, evidence: Evidence::default(), scopes: Vec::new(), error: None };
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
        ExprKind::Negate(inner) => match evaluate(inner, checked, class, definitions, evidence)?.map(Number::numeric) {
            Some(Number::Int(value)) => match value.checked_neg() {
                Some(value) => Some(Number::Int(value)),
                None if checked => return Err(CompileError::new(expr.span, INTEGER_OVERFLOW_MESSAGE)),
                None => Some(Number::Float(-(value as f64))),
            },
            Some(Number::Float(value)) => Some(Number::Float(-value)),
            None => None,
            _ => unreachable!("numeric normalization removes string evidence"),
        },
        ExprKind::FunctionCall { name, args } if !definitions.unresolved_namespaces
            && !definitions.functions.contains(&crate::names::php_symbol_key(name.as_str().trim_start_matches('\\'))) =>
            builtin_number(expr, name.as_str(), args, checked, class, definitions, evidence)?,
        ExprKind::BinaryOp { left, op, right } if matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod | BinOp::Pow) => {
            let left = evaluate(left, checked, class, definitions, evidence)?;
            let right = evaluate(right, checked, class, definitions, evidence)?;
            match (left, right) { (Some(left), Some(right)) => binary(expr, op, left, right, checked)?, _ => None }
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

/// Classifies PHP numeric strings without changing full-width integer values through float rounding.
fn string_number(value: &str) -> Option<Number> {
    let scan = crate::numeric_string::scan_numeric_prefix(value)?;
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
    if !matches!(operation, crate::ir::RuntimeFnId::Abs | crate::ir::RuntimeFnId::Pow) { return Ok(None); }
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
    let mut call_evidence = Evidence { active: evidence.active.clone(), strict_types: evidence.strict_types, ..Default::default() };
    if operation == crate::ir::RuntimeFnId::Pow {
        let [left, right] = normalized.as_slice() else { return Ok(None); };
        let Some(left) = evaluate(left, checked, class, definitions, &mut call_evidence)? else { return Ok(None); };
        let Some(right) = evaluate(right, checked, class, definitions, &mut call_evidence)? else { return Ok(None); };
        // Mixed pow parameters follow operator conversion even with PHP strict_types enabled.
        // The existing permissive/Internal builtin implementation always returns a float.
        return if checked { binary(expr, &BinOp::Pow, left, right, checked) }
            else { Ok(Some(Number::Float(left.as_float().powf(right.as_float())))) };
    }
    let [argument] = normalized.as_slice() else { return Ok(None); };
    let Some(value) = evaluate(argument, checked, class, definitions, &mut call_evidence)? else { return Ok(None); };
    if !value.valid_numeric_parameter(evidence.strict_types) { return Ok(None); }
    Ok(match value.numeric() {
        Number::Int(value) => match value.checked_abs() {
            Some(value) => Some(Number::Int(value)),
            None if checked => return Err(CompileError::new(expr.span, INTEGER_OVERFLOW_MESSAGE)),
            None => Some(Number::Float((value as f64).abs())),
        },
        Number::Float(value) => Some(Number::Float(value.abs())),
        _ => unreachable!("numeric normalization removes string evidence"),
    })
}

/// Applies checked integer kernels only when PHP would actually choose integer arithmetic.
fn binary(expr: &Expr, op: &BinOp, left: Number, right: Number, checked: bool) -> Result<Option<Number>, CompileError> {
    let (left, right) = (left.numeric(), right.numeric());
    if let (Number::Int(left), Number::Int(right)) = (left, right) {
        let operation = match op { BinOp::Add => Some(IntegerOp::Add), BinOp::Sub => Some(IntegerOp::Sub),
            BinOp::Mul => Some(IntegerOp::Mul), BinOp::Mod => Some(IntegerOp::Mod), BinOp::Pow if right >= 0 => Some(IntegerOp::Pow), _ => None };
        if let Some(operation) = operation {
            match checked_integer(operation, left, right) {
                Ok(value) => return Ok(Some(Number::Int(value))),
                Err(NumericFailure::Overflow) if checked => return Err(CompileError::new(expr.span, INTEGER_OVERFLOW_MESSAGE)),
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
mod tests {
    use super::*;

    /// Parses checked integer spellings without exposing the unfinished CLI feature.
    fn parse(source: &str) -> Program {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true);
        crate::parser::parse(&crate::lexer::tokenize_with_semantics(source, SourceMode::Php, 0, profile).unwrap()).unwrap()
    }

    /// Power evidence uses Mixed operand rules, planned arguments and lexical source contracts.
    #[test]
    fn strict_pow_source_constants_share_operator_evidence_and_argument_planning() {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true);
        for source in [
            "<?php if (false) { pow(2, 63); }",
            "<?php POW(exponent: 2, num: PHP_INT_MAX);",
            "<?php pow(...[PHP_INT_MAX, 2]);",
            "<?php declare(strict_types=1); pow('9223372036854775807', 2);",
            "<?php const BASE = '9223372036854775807'; pow(BASE, 2);",
            "<?php pow('9223372036854775807x', true + 1);",
            "<?php eval('pow(2, 63);');",
        ] {
            let error = super::super::validate_source(&parse(source), profile).expect_err(source);
            assert!(error.message.contains("strict-overflow"), "{source}: {error:?}");
        }
        for source in [
            "<?php pow(-2, 63); pow(2, -1); pow('9223372036854775808', 2);",
            "<?php const LOOP = pow(LOOP, 2); pow(LOOP, 2);",
            "<?php function pow($n, $e) { return 0; } pow(PHP_INT_MAX, 2);",
            "<?php pow(num: 0, 2);",
        ] { assert!(super::super::validate_source(&parse(source), profile).is_ok(), "{source}"); }
        let mut program = parse("<?php const BASE = pow(PHP_INT_MAX, 2); echo BASE + 1;");
        program[0].source_mode = SourceMode::Internal;
        assert!(validate(&program, profile).is_ok());
    }

    /// Provable overflow is rejected in dead syntax, exact-width literals and scalar coercions.
    #[test]
    fn strict_overflow_constants_validate_unpruned_numeric_trees() {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true);
        for expression in ["9223372036854775807 + 1", "-9223372036854775808 - 1", "9223372036854775807 * 2",
            "2 ** 63", "-(-9223372036854775808)", "(9223372036854775806 + 1) + 1",
            "'9223372036854775807' + true", "('9223372036854775807' + null) + 1"] {
            let program = parse(&format!("<?php if (false) {{ echo {expression}; }}"));
            let error = validate(&program, profile).unwrap_err();
            assert!(error.message.contains("strict-overflow"), "{expression}: {error:?}");
            assert!(validate(&program, SemanticProfile::PERMISSIVE).is_ok());
        }
        for expression in ["9223372036854775807 + 0", "-9223372036854775808 - 0", "(-2) ** 63", "2 ** -1",
            "1.0e308 * 1.0e308", "'9223372036854775808' + 1", "-9223372036854775808 % -1", "1 / 0"] {
            assert!(validate(&parse(&format!("<?php echo {expression};")), profile).is_ok(), "{expression}");
        }
    }

    /// Forward aliases and lexical class constants share exact evidence and never recurse through cycles.
    #[test]
    fn strict_overflow_constants_resolve_aliases_without_guessing_cycles() {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true);
        for source in [
            "<?php const LAST = LIMIT; const LIMIT = PHP_INT_MAX; if (false) { echo LAST + 1; }",
            "<?php class Limits { const LAST = self::LIMIT; const LIMIT = PHP_INT_MAX; } echo lImItS::LAST + 1;",
            "<?php class Limits { const LAST = PHP_INT_MAX; function check() { echo self::LAST + 1; } }",
        ] { assert!(validate(&parse(source), profile).is_err(), "{source}"); }
        for source in ["<?php const A = B; const B = A; echo A + 1;",
            "<?php if ($argc) { const A = PHP_INT_MAX; } echo A + 1;",
            "<?php const A = PHP_INT_MAX; const A = 0; echo A + 1;",
            "<?php namespace One { const LIMIT = PHP_INT_MAX; } namespace Two { const LIMIT = 0; echo LIMIT + 1; }",
        ] { assert!(validate(&parse(source), profile).is_ok(), "{source}"); }
        for (source, invalid) in [
            ("<?php namespace One { const LIMIT = PHP_INT_MAX; } namespace Two { const LIMIT = 0; echo LIMIT + 1; }", false),
            ("<?php namespace One { const LIMIT = PHP_INT_MAX; } namespace Two { use const One\\LIMIT as MAX; echo MAX + 1; }", true),
        ] {
            let resolved = crate::name_resolver::resolve(parse(source)).unwrap();
            assert_eq!(validate(&resolved, profile).is_err(), invalid, "{source}");
        }
    }

    /// Trusted implementation constants keep their PHP kind at a checked user consumption site.
    #[test]
    fn strict_overflow_constants_keep_internal_constant_result_kinds() {
        let mut program = parse("<?php const LIMIT = PHP_INT_MAX + 1; echo -LIMIT;");
        program[0].source_mode = SourceMode::Internal;
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true);
        assert!(validate(&program, profile).is_ok());
        program[0].source_mode = SourceMode::Php;
        assert!(validate(&program, profile).is_err());
    }

    /// The shared source boundary checks literal eval and all declaration expression positions.
    #[test]
    fn strict_overflow_constants_validate_literal_eval_and_declaration_defaults() {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true);
        for source in [
            "<?php if (false) { eval('echo PHP_INT_MAX + 1;'); }",
            "<?php eval('echo 1;'); eval('echo 2 ** 63;');",
            "<?php function example(int $value = PHP_INT_MAX + 1) {}",
            "<?php class Example { public int $value = PHP_INT_MAX + 1; }",
            "<?php #[Example(PHP_INT_MAX + 1)] class Annotated {}",
            "<?php $closure = fn() => PHP_INT_MAX + 1;",
        ] {
            let error = super::super::validate_source(&parse(source), profile).unwrap_err();
            assert!(error.message.contains("strict-overflow"), "{source}: {error:?}");
        }
        let program = parse("<?php const OUTER_MAX = PHP_INT_MAX; eval('echo OUTER_MAX + 1;'); eval('echo 1 + 2;');");
        assert!(super::super::validate_source(&program, profile).is_ok());
    }

    /// Abs uses full numeric-string parameters, shared call planning and declaration-safe proofs.
    #[test]
    fn strict_abs_source_constants_validate_known_magnitudes_without_guessing_names() {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true);
        for source in [
            "<?php abs(PHP_INT_MIN);",
            "<?php if (false) { ABS(num: PHP_INT_MIN); }",
            "<?php abs(...[PHP_INT_MIN]);",
            "<?php const MINIMUM = PHP_INT_MIN; abs(MINIMUM);",
            "<?php const MINIMUM = '-9223372036854775808'; abs(MINIMUM);",
            "<?php class Limits { const MINIMUM = PHP_INT_MIN; } abs(Limits::MINIMUM);",
            "<?php eval('abs(PHP_INT_MIN);');",
        ] {
            let error = super::super::validate_source(&parse(source), profile)
                .expect_err(source);
            assert!(error.message.contains("strict-overflow"), "{source}: {error:?}");
        }
        for source in [
            "<?php abs(PHP_INT_MAX); abs(-1.25); abs('-9223372036854775809');",
            "<?php abs('-9223372036854775808x');",
            "<?php declare(strict_types=1); abs('-9223372036854775808');",
            "<?php declare(strict_types=1); echo PHP_INT_MAX + abs(true);",
            "<?php const PREFIX = '-9223372036854775808x'; abs(PREFIX);",
            "<?php const LOOP = abs(LOOP); abs(LOOP);",
            "<?php namespace Local; function abs($n) { return 0; } abs(PHP_INT_MIN);",
            "<?php function abs($n) { return 0; } abs(PHP_INT_MIN);",
            "<?php extern function abs(int $n): int; abs(PHP_INT_MIN);",
        ] {
            assert!(super::super::validate_source(&parse(source), profile).is_ok(), "{source}");
        }
        let mut internal = parse("<?php abs(PHP_INT_MIN);");
        internal[0].source_mode = SourceMode::Internal;
        assert!(validate(&internal, profile).is_ok());
    }

}
