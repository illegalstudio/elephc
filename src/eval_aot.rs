//! Purpose:
//! Shared compile-time analysis helpers for literal `eval` AOT eligibility.
//! Keeps parser/classifier decisions out of target assembly lowering where possible.
//!
//! Called from:
//! - `crate::ir_lower::program` while deriving runtime feature requirements.
//! - `crate::codegen::lower_inst::builtins::eval` while lowering AOT fragments.
//!
//! Key details:
//! - Only exposes semantics that are fully target-independent.
//! - Plans keep fallback and scope metadata alongside any fully static lowering.

use std::collections::BTreeSet;
use std::path::Path;

use crate::names::{php_symbol_key, Name};
use crate::parser::ast::{
    BinOp, CallableTarget, CastType, Expr, ExprKind, Program, StaticReceiver, Stmt, StmtKind,
};
use crate::span::Span;
use crate::types::call_args::{has_named_args, plan_call_args};
use crate::types::{builtin_call_sig, is_php_integer_array_key, FunctionSig, PhpType};

const EIR_AOT_FUNCTION_PREFIX: &str = "__eir@evalaot";
const MAX_STATIC_STRING_FOLD_BYTES: usize = 1_048_576;

/// Static call support available while classifying eval fragments for EIR AOT.
trait EirStaticCallSupport {
    /// Returns true when a function call can be lowered inside an EIR AOT fragment.
    fn function_supported(&self, name: &str, args: &[Expr]) -> bool;

    /// Returns true when a static method call can be lowered inside an EIR AOT fragment.
    fn static_method_supported(
        &self,
        receiver: &StaticReceiver,
        method: &str,
        args: &[Expr],
    ) -> bool;
}

/// Pair of caller-provided support predicates for eval EIR AOT static calls.
struct EirStaticCallPredicates<'a, F, M> {
    function: &'a F,
    static_method: &'a M,
}

impl<F, M> EirStaticCallSupport for EirStaticCallPredicates<'_, F, M>
where
    F: Fn(&str, &[Expr]) -> bool,
    M: Fn(&StaticReceiver, &str, &[Expr]) -> bool,
{
    /// Delegates function-call eligibility to the caller-provided predicate.
    fn function_supported(&self, name: &str, args: &[Expr]) -> bool {
        (self.function)(name, args)
    }

    /// Delegates static-method eligibility to the caller-provided predicate.
    fn static_method_supported(
        &self,
        receiver: &StaticReceiver,
        method: &str,
        args: &[Expr],
    ) -> bool {
        (self.static_method)(receiver, method, args)
    }
}

/// Compile-time plan for one literal eval fragment.
pub(crate) struct EvalAotPlan {
    function_name: Option<String>,
    eir_program: Option<Program>,
    scope_function_name: Option<String>,
    scope_eir_program: Option<Program>,
    reads: BTreeSet<String>,
    array_read_constraints: BTreeSet<String>,
    assoc_array_read_constraints: BTreeSet<String>,
    float_predicate_read_constraints: BTreeSet<String>,
    writes: BTreeSet<String>,
    scope_direct_writes: BTreeSet<String>,
    scope_flush_writes: BTreeSet<String>,
    creates_unknown_vars: bool,
    needs_eval_context: bool,
    needs_global_scope: bool,
    fallback_reason: Option<EvalAotFallbackReason>,
}

impl EvalAotPlan {
    /// Returns true when this fragment is fully native and cannot call the eval bridge.
    pub(crate) fn is_fully_static_no_bridge(&self) -> bool {
        !self.needs_eval_context
            && self.fallback_reason.is_none()
            && self.scope_eir_program.is_none()
            && self.eir_program.is_some()
    }

    /// Returns true when the scope-read EIR body can receive reads as direct parameters.
    pub(crate) fn uses_scope_read_params(&self) -> bool {
        self.scope_eir_program.is_some()
            && !self.reads.is_empty()
            && self.writes.is_empty()
            && self.scope_direct_writes.is_empty()
            && self.scope_flush_writes.is_empty()
    }

    /// Returns true when the fragment still requires the magician eval bridge.
    pub(crate) fn requires_runtime_eval_bridge(&self) -> bool {
        if self.is_fully_static_no_bridge() {
            return false;
        }
        if self.scope_eir_program.is_some() {
            return false;
        }
        self.needs_eval_context
            || self.needs_global_scope
            || self.creates_unknown_vars
            || !self.reads.is_empty()
            || !self.writes.is_empty()
            || self.fallback_reason.is_some()
    }

    /// Returns true when the fragment needs only core eval-scope runtime state.
    pub(crate) fn requires_runtime_eval_scope(&self) -> bool {
        self.scope_eir_program.is_some() && !self.uses_scope_read_params()
    }

    /// Takes the deterministic internal EIR function name, when one exists.
    pub(crate) fn take_function_name(&mut self) -> Option<String> {
        self.function_name.take()
    }

    /// Takes the parsed and folded EIR AOT body, when one exists.
    pub(crate) fn take_eir_program(&mut self) -> Option<Program> {
        self.eir_program.take()
    }

    /// Takes the deterministic EIR function name for a scope-aware AOT body.
    pub(crate) fn take_scope_function_name(&mut self) -> Option<String> {
        self.scope_function_name.take()
    }

    /// Takes the parsed and folded body for a scope-aware EIR AOT function.
    pub(crate) fn take_scope_eir_program(&mut self) -> Option<Program> {
        self.scope_eir_program.take()
    }

    /// Returns the statically known eval-scope reads for this fragment.
    pub(crate) fn reads(&self) -> &BTreeSet<String> {
        &self.reads
    }

    /// Returns scope reads that must be caller-side arrays for direct-param AOT.
    pub(crate) fn array_read_constraints(&self) -> &BTreeSet<String> {
        &self.array_read_constraints
    }

    /// Returns scope reads that must be caller-side associative arrays.
    pub(crate) fn assoc_array_read_constraints(&self) -> &BTreeSet<String> {
        &self.assoc_array_read_constraints
    }

    /// Returns scope reads that must be caller-side int/float values.
    pub(crate) fn float_predicate_read_constraints(&self) -> &BTreeSet<String> {
        &self.float_predicate_read_constraints
    }

    /// Returns the statically known eval-scope writes for this fragment.
    pub(crate) fn writes(&self) -> &BTreeSet<String> {
        &self.writes
    }

    /// Returns eval-scope writes that are stored immediately during EIR lowering.
    pub(crate) fn direct_writes(&self) -> &BTreeSet<String> {
        &self.scope_direct_writes
    }

    /// Returns local writes that are flushed to eval scope by the EIR finalizer.
    pub(crate) fn flush_writes(&self) -> &BTreeSet<String> {
        &self.scope_flush_writes
    }

    /// Returns the conservative bridge fallback reason, when this plan has one.
    pub(crate) fn fallback_reason(&self) -> Option<EvalAotFallbackReason> {
        self.fallback_reason
    }
}

/// Conservative reason a literal eval fragment cannot be fully static today.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EvalAotFallbackReason {
    ParseError,
    IncludeOrRequire,
    Declaration,
    GlobalOrStatic,
    ReferenceOrByRef,
    DynamicCall,
    DynamicClassOrMember,
    ObjectOrMemberAccess,
    ArrayOrIterable,
    TryOrThrow,
    UnsupportedControlFlow,
    UnsupportedScope,
    UnsupportedStaticCall,
    UnsupportedConstruct,
}

impl EvalAotFallbackReason {
    /// Returns a stable assembly-marker description for this fallback reason.
    pub(crate) fn description(self) -> &'static str {
        match self {
            EvalAotFallbackReason::ParseError => "parse error",
            EvalAotFallbackReason::IncludeOrRequire => "include/require needs bridge semantics",
            EvalAotFallbackReason::Declaration => "runtime declarations need bridge semantics",
            EvalAotFallbackReason::GlobalOrStatic => "global/static scope needs bridge semantics",
            EvalAotFallbackReason::ReferenceOrByRef => "references/by-ref need bridge semantics",
            EvalAotFallbackReason::DynamicCall => "dynamic call needs bridge semantics",
            EvalAotFallbackReason::DynamicClassOrMember => {
                "dynamic class/member access needs bridge semantics"
            }
            EvalAotFallbackReason::ObjectOrMemberAccess => {
                "object/member access needs bridge semantics"
            }
            EvalAotFallbackReason::ArrayOrIterable => {
                "array/iterable semantics need bridge fallback"
            }
            EvalAotFallbackReason::TryOrThrow => "try/throw needs bridge semantics",
            EvalAotFallbackReason::UnsupportedControlFlow => "unsupported control flow",
            EvalAotFallbackReason::UnsupportedScope => "unsupported scope synchronization",
            EvalAotFallbackReason::UnsupportedStaticCall => "unsupported static call",
            EvalAotFallbackReason::UnsupportedConstruct => "unsupported construct",
        }
    }
}

/// Parses a literal eval fragment as a PHP statement fragment.
pub(crate) fn parse_literal_fragment(fragment: &str) -> Option<Program> {
    let source = format!("<?php {}", fragment);
    let tokens = crate::lexer::tokenize(&source).ok()?;
    crate::parser::parse(&tokens).ok()
}

/// Parses a literal eval fragment and applies call-site magic-constant metadata when available.
pub(crate) fn parse_literal_fragment_with_source_path(
    fragment: &str,
    source_path: Option<&str>,
) -> Option<Program> {
    let program = parse_literal_fragment(fragment)?;
    Some(match source_path {
        Some(source_path) => crate::magic_constants::substitute_file_and_scope_constants(
            program,
            Path::new(source_path),
        ),
        None => program,
    })
}
/// Returns a deterministic internal function name for a literal eval fragment.
pub(crate) fn eir_function_name(fragment: &str) -> String {
    format!(
        "{}_{:016x}",
        EIR_AOT_FUNCTION_PREFIX,
        stable_fragment_hash(fragment)
    )
}

/// Returns a deterministic internal function name for a scope-aware eval fragment.
pub(crate) fn eir_scope_function_name(fragment: &str) -> String {
    format!(
        "{}_scope_{:016x}",
        EIR_AOT_FUNCTION_PREFIX,
        stable_fragment_hash(fragment)
    )
}

/// Builds the shared literal eval AOT plan for scan, lowering, and codegen decisions.
pub(crate) fn plan_literal_fragment_with_static_calls<F>(
    fragment: &str,
    static_call_supported: F,
) -> EvalAotPlan
where
    F: Fn(&str, &[Expr]) -> bool,
{
    plan_literal_fragment_with_static_and_method_calls(
        fragment,
        static_call_supported,
        |_, _, _| false,
    )
}

/// Builds the shared literal eval AOT plan with function and static-method support.
pub(crate) fn plan_literal_fragment_with_static_and_method_calls<F, M>(
    fragment: &str,
    static_call_supported: F,
    static_method_supported: M,
) -> EvalAotPlan
where
    F: Fn(&str, &[Expr]) -> bool,
    M: Fn(&StaticReceiver, &str, &[Expr]) -> bool,
{
    let Some(program) = parse_literal_fragment(fragment) else {
        return parse_error_plan();
    };
    plan_parsed_literal_fragment_with_static_and_method_calls(
        fragment,
        program,
        static_call_supported,
        static_method_supported,
    )
}

/// Builds the literal eval AOT plan with call-site source and static-method metadata.
pub(crate) fn plan_literal_fragment_with_source_path_and_static_and_method_calls<F, M>(
    fragment: &str,
    source_path: Option<&str>,
    static_call_supported: F,
    static_method_supported: M,
) -> EvalAotPlan
where
    F: Fn(&str, &[Expr]) -> bool,
    M: Fn(&StaticReceiver, &str, &[Expr]) -> bool,
{
    let Some(program) = parse_literal_fragment_with_source_path(fragment, source_path) else {
        return parse_error_plan();
    };
    plan_parsed_literal_fragment_with_static_and_method_calls(
        fragment,
        program,
        static_call_supported,
        static_method_supported,
    )
}

/// Returns a conservative plan for fragments that cannot be parsed statically.
fn parse_error_plan() -> EvalAotPlan {
    EvalAotPlan {
        function_name: None,
        eir_program: None,
        scope_function_name: None,
        scope_eir_program: None,
        reads: BTreeSet::new(),
        array_read_constraints: BTreeSet::new(),
        assoc_array_read_constraints: BTreeSet::new(),
        float_predicate_read_constraints: BTreeSet::new(),
        writes: BTreeSet::new(),
        scope_direct_writes: BTreeSet::new(),
        scope_flush_writes: BTreeSet::new(),
        creates_unknown_vars: true,
        needs_eval_context: true,
        needs_global_scope: true,
        fallback_reason: Some(EvalAotFallbackReason::ParseError),
    }
}

/// Builds the shared literal eval AOT plan from an already parsed fragment program.
fn plan_parsed_literal_fragment_with_static_and_method_calls<F, M>(
    fragment: &str,
    program: Program,
    static_call_supported: F,
    static_method_supported: M,
) -> EvalAotPlan
where
    F: Fn(&str, &[Expr]) -> bool,
    M: Fn(&StaticReceiver, &str, &[Expr]) -> bool,
{
    let mut scope_access = collect_scope_accesses(&program);
    scope_access.reads = collect_scope_reads_before_writes(&program);
    let folded_program = fold_static_builtin_calls_in_program(program.clone());
    let support = EirStaticCallPredicates {
        function: &static_call_supported,
        static_method: &static_method_supported,
    };
    let eir_program =
        program_is_eir_function_safe(&folded_program, &support).then_some(folded_program.clone());
    let scope_names = scope_access
        .reads
        .union(&scope_access.writes)
        .cloned()
        .collect::<BTreeSet<_>>();
    let scope_eir_safe = eir_program.is_none()
        && !scope_names.is_empty()
        && !scope_access.creates_unknown_vars
        && program_is_eir_scope_function_safe(&folded_program, &support, &scope_names);
    let scope_flush_local =
        scope_eir_safe && scope_access.reads.is_empty() && !scope_access.writes.is_empty();
    let scope_direct = scope_eir_safe && !scope_access.reads.is_empty();
    let scope_direct_writes = if scope_direct {
        scope_access.writes.clone()
    } else {
        BTreeSet::new()
    };
    let scope_flush_writes = if scope_flush_local {
        scope_access.writes.clone()
    } else {
        BTreeSet::new()
    };
    let array_read_constraint_sets =
        collect_array_scope_read_constraint_sets(&folded_program, &scope_access.reads);
    let array_read_constraints = array_read_constraint_sets.array_like;
    let assoc_array_read_constraints = array_read_constraint_sets.assoc;
    let float_predicate_read_constraints =
        collect_float_predicate_scope_read_constraints(&folded_program, &scope_access.reads);
    let scope_eir_program = (scope_direct || scope_flush_local).then_some(folded_program);
    let is_fully_static_no_bridge = eir_program.is_some();
    let has_scope_eir = scope_eir_program.is_some();
    let needs_global_scope =
        !is_fully_static_no_bridge && !has_scope_eir && scope_access.has_scope_access();
    EvalAotPlan {
        function_name: eir_program.as_ref().map(|_| eir_function_name(fragment)),
        eir_program,
        scope_function_name: scope_eir_program
            .as_ref()
            .map(|_| eir_scope_function_name(fragment)),
        scope_eir_program,
        reads: scope_access.reads,
        array_read_constraints,
        assoc_array_read_constraints,
        float_predicate_read_constraints,
        writes: scope_access.writes,
        scope_direct_writes,
        scope_flush_writes,
        creates_unknown_vars: scope_access.creates_unknown_vars,
        needs_eval_context: !is_fully_static_no_bridge && !has_scope_eir,
        needs_global_scope,
        fallback_reason: (!is_fully_static_no_bridge && !has_scope_eir)
            .then(|| classify_fallback_reason(&program)),
    }
}

/// Classifies the first visible reason this fragment cannot avoid the bridge.
fn classify_fallback_reason(program: &[Stmt]) -> EvalAotFallbackReason {
    program
        .iter()
        .find_map(stmt_fallback_reason)
        .unwrap_or(EvalAotFallbackReason::UnsupportedScope)
}

/// Classifies one statement for a human-readable eval AOT fallback marker.
fn stmt_fallback_reason(stmt: &Stmt) -> Option<EvalAotFallbackReason> {
    match &stmt.kind {
        StmtKind::Include { .. } | StmtKind::IncludeOnceMark { .. } => {
            Some(EvalAotFallbackReason::IncludeOrRequire)
        }
        StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. } => body.iter().find_map(stmt_fallback_reason),
        StmtKind::FunctionDecl { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. }
        | StmtKind::ConstDecl { .. }
        | StmtKind::ClassDecl { .. }
        | StmtKind::EnumDecl { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::InterfaceDecl { .. }
        | StmtKind::TraitDecl { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => Some(EvalAotFallbackReason::Declaration),
        StmtKind::Global { .. } | StmtKind::StaticVar { .. } => {
            Some(EvalAotFallbackReason::GlobalOrStatic)
        }
        StmtKind::RefAssign { .. } => Some(EvalAotFallbackReason::ReferenceOrByRef),
        StmtKind::Foreach {
            array,
            value_by_ref,
            body,
            ..
        } => {
            if *value_by_ref {
                return Some(EvalAotFallbackReason::ReferenceOrByRef);
            }
            expr_fallback_reason(array)
                .or_else(|| body.iter().find_map(stmt_fallback_reason))
                .or(Some(EvalAotFallbackReason::ArrayOrIterable))
        }
        StmtKind::Try { .. } | StmtKind::Throw(_) => Some(EvalAotFallbackReason::TryOrThrow),
        StmtKind::ArrayAssign { .. }
        | StmtKind::NestedArrayAssign { .. }
        | StmtKind::ArrayPush { .. }
        | StmtKind::ListUnpack { .. } => Some(EvalAotFallbackReason::ArrayOrIterable),
        StmtKind::PropertyAssign { .. }
        | StmtKind::StaticPropertyAssign { .. }
        | StmtKind::StaticPropertyArrayPush { .. }
        | StmtKind::StaticPropertyArrayAssign { .. }
        | StmtKind::PropertyArrayPush { .. }
        | StmtKind::PropertyArrayAssign { .. } => Some(EvalAotFallbackReason::ObjectOrMemberAccess),
        StmtKind::Echo(expr) | StmtKind::ExprStmt(expr) | StmtKind::Return(Some(expr)) => {
            expr_fallback_reason(expr)
        }
        StmtKind::Assign { value, .. } => expr_fallback_reason(value),
        // Typed local declarations are an elephc extension: under `--strict-php`
        // the fragment must reach the runtime bridge, whose parser rejects the
        // syntax like the PHP interpreter would (runtime parse error), instead
        // of being AOT-compiled and silently executing non-PHP code.
        StmtKind::TypedAssign { value, .. } => {
            if crate::strict_php::is_enabled() {
                return Some(EvalAotFallbackReason::UnsupportedConstruct);
            }
            expr_fallback_reason(value)
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => expr_fallback_reason(condition)
            .or_else(|| then_body.iter().find_map(stmt_fallback_reason))
            .or_else(|| {
                elseif_clauses.iter().find_map(|(condition, body)| {
                    expr_fallback_reason(condition)
                        .or_else(|| body.iter().find_map(stmt_fallback_reason))
                })
            })
            .or_else(|| {
                else_body
                    .as_deref()
                    .and_then(|body| body.iter().find_map(stmt_fallback_reason))
            }),
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            expr_fallback_reason(condition).or_else(|| body.iter().find_map(stmt_fallback_reason))
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => init
            .as_deref()
            .and_then(stmt_fallback_reason)
            .or_else(|| condition.as_ref().and_then(expr_fallback_reason))
            .or_else(|| update.as_deref().and_then(stmt_fallback_reason))
            .or_else(|| body.iter().find_map(stmt_fallback_reason)),
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => expr_fallback_reason(subject)
            .or_else(|| {
                cases.iter().find_map(|(conditions, body)| {
                    conditions
                        .iter()
                        .find_map(expr_fallback_reason)
                        .or_else(|| body.iter().find_map(stmt_fallback_reason))
                })
            })
            .or_else(|| {
                default
                    .as_deref()
                    .and_then(|body| body.iter().find_map(stmt_fallback_reason))
            })
            .or(Some(EvalAotFallbackReason::UnsupportedControlFlow)),
        StmtKind::Break(_) | StmtKind::Continue(_) => {
            Some(EvalAotFallbackReason::UnsupportedControlFlow)
        }
        StmtKind::Return(None) | StmtKind::NamespaceDecl { .. } | StmtKind::UseDecl { .. } => None,
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            // `ifdef` is an elephc extension: under `--strict-php` the fragment
            // must reach the runtime bridge, whose parser rejects the syntax
            // like the PHP interpreter would, instead of being AOT-compiled.
            if crate::strict_php::is_enabled() {
                return Some(EvalAotFallbackReason::UnsupportedConstruct);
            }
            then_body.iter().find_map(stmt_fallback_reason).or_else(|| {
                else_body
                    .as_deref()
                    .and_then(|body| body.iter().find_map(stmt_fallback_reason))
            })
        }
    }
}

/// Classifies one expression for a human-readable eval AOT fallback marker.
fn expr_fallback_reason(expr: &Expr) -> Option<EvalAotFallbackReason> {
    match &expr.kind {
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::Variable(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null => None,
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Clone(inner)
        | ExprKind::YieldFrom(inner) => expr_fallback_reason(inner),
        ExprKind::Throw(_) => Some(EvalAotFallbackReason::TryOrThrow),
        ExprKind::BinaryOp { left, right, .. }
        | ExprKind::NullCoalesce {
            value: left,
            default: right,
        }
        | ExprKind::ShortTernary {
            value: left,
            default: right,
        }
        | ExprKind::ArrayAccess {
            array: left,
            index: right,
        } => expr_fallback_reason(left)
            .or_else(|| expr_fallback_reason(right))
            .or(Some(EvalAotFallbackReason::ArrayOrIterable)),
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => expr_fallback_reason(condition)
            .or_else(|| expr_fallback_reason(then_expr))
            .or_else(|| expr_fallback_reason(else_expr)),
        ExprKind::Cast { target, expr } => {
            if matches!(target, CastType::Array) {
                return Some(EvalAotFallbackReason::ArrayOrIterable);
            }
            expr_fallback_reason(expr)
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => expr_fallback_reason(subject)
            .or_else(|| {
                arms.iter().find_map(|(conditions, result)| {
                    conditions
                        .iter()
                        .find_map(expr_fallback_reason)
                        .or_else(|| expr_fallback_reason(result))
                })
            })
            .or_else(|| default.as_deref().and_then(expr_fallback_reason)),
        ExprKind::FunctionCall { args, .. } => args
            .iter()
            .find_map(expr_fallback_reason)
            .or(Some(EvalAotFallbackReason::UnsupportedStaticCall)),
        ExprKind::ClosureCall { .. } | ExprKind::ExprCall { .. } => {
            Some(EvalAotFallbackReason::DynamicCall)
        }
        ExprKind::Pipe { value, callable } => expr_fallback_reason(value)
            .or_else(|| expr_fallback_reason(callable))
            .or(Some(EvalAotFallbackReason::DynamicCall)),
        ExprKind::NewDynamic { .. } | ExprKind::NewDynamicObject { .. } => {
            Some(EvalAotFallbackReason::DynamicClassOrMember)
        }
        ExprKind::DynamicPropertyAccess { .. }
        | ExprKind::NullsafeDynamicPropertyAccess { .. }
        | ExprKind::NullsafeDynamicMethodCall { .. } => {
            Some(EvalAotFallbackReason::DynamicClassOrMember)
        }
        ExprKind::NewObject { .. }
        | ExprKind::NewScopedObject { .. }
        | ExprKind::PropertyAccess { .. }
        | ExprKind::NullsafePropertyAccess { .. }
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::MethodCall { .. }
        | ExprKind::NullsafeMethodCall { .. }
        | ExprKind::StaticMethodCall { .. }
        | ExprKind::ClassConstant { .. }
        | ExprKind::ObjectClassName { .. }
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::This => Some(EvalAotFallbackReason::ObjectOrMemberAccess),
        ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) | ExprKind::Spread(_) => {
            Some(EvalAotFallbackReason::ArrayOrIterable)
        }
        ExprKind::Assignment { .. }
        | ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_)
        | ExprKind::NamedArg { .. } => Some(EvalAotFallbackReason::UnsupportedScope),
        ExprKind::Closure { .. } => Some(EvalAotFallbackReason::Declaration),
        ExprKind::IncludeValue { .. } => Some(EvalAotFallbackReason::IncludeOrRequire),
        ExprKind::InstanceOf { value, target } => expr_fallback_reason(value)
            .or_else(|| match target {
                crate::parser::ast::InstanceOfTarget::Name(_) => None,
                crate::parser::ast::InstanceOfTarget::Expr(expr) => expr_fallback_reason(expr),
            })
            .or(Some(EvalAotFallbackReason::ObjectOrMemberAccess)),
        ExprKind::FirstClassCallable(target) => callable_target_fallback_reason(target),
        ExprKind::ConstRef(_) | ExprKind::MagicConstant(_) => {
            Some(EvalAotFallbackReason::UnsupportedConstruct)
        }
        ExprKind::PtrCast { expr, .. } => {
            expr_fallback_reason(expr).or(Some(EvalAotFallbackReason::UnsupportedConstruct))
        }
        ExprKind::BufferNew { len, .. } => {
            expr_fallback_reason(len).or(Some(EvalAotFallbackReason::UnsupportedConstruct))
        }
        ExprKind::Yield { .. } => Some(EvalAotFallbackReason::UnsupportedControlFlow),
    }
}

/// Classifies first-class callable expressions for fallback markers.
fn callable_target_fallback_reason(target: &CallableTarget) -> Option<EvalAotFallbackReason> {
    match target {
        CallableTarget::Function(_) => Some(EvalAotFallbackReason::DynamicCall),
        CallableTarget::StaticMethod { .. } => Some(EvalAotFallbackReason::ObjectOrMemberAccess),
        CallableTarget::Method { object, .. } => {
            expr_fallback_reason(object).or(Some(EvalAotFallbackReason::ObjectOrMemberAccess))
        }
    }
}


/// Variable read/write metadata collected from a parsed eval fragment.
struct EvalScopeAccess {
    reads: BTreeSet<String>,
    writes: BTreeSet<String>,
    creates_unknown_vars: bool,
}

impl EvalScopeAccess {
    /// Creates an empty eval scope access accumulator.
    fn new() -> Self {
        Self {
            reads: BTreeSet::new(),
            writes: BTreeSet::new(),
            creates_unknown_vars: false,
        }
    }

    /// Returns true when the fragment touches any eval-visible variable storage.
    fn has_scope_access(&self) -> bool {
        !self.reads.is_empty() || !self.writes.is_empty() || self.creates_unknown_vars
    }

    /// Records a variable read.
    fn read(&mut self, name: &str) {
        self.reads.insert(name.to_string());
    }

    /// Records a variable write.
    fn write(&mut self, name: &str) {
        self.writes.insert(name.to_string());
    }

    /// Marks an access shape that cannot be mapped to a static variable name.
    fn unknown_write(&mut self) {
        self.creates_unknown_vars = true;
    }
}

/// Collects conservative eval-scope reads and writes from a parsed fragment.
fn collect_scope_accesses(program: &[Stmt]) -> EvalScopeAccess {
    let mut access = EvalScopeAccess::new();
    for stmt in program {
        collect_stmt_scope_access(stmt, &mut access);
    }
    access
}

/// Collects variable reads that must come from the caller before local writes exist.
fn collect_scope_reads_before_writes(program: &[Stmt]) -> BTreeSet<String> {
    let mut reads = BTreeSet::new();
    let mut assigned = BTreeSet::new();
    collect_block_scope_reads_before_writes(program, &mut assigned, &mut reads);
    reads
}

/// Collects caller reads across a statement block and tracks definite local writes.
fn collect_block_scope_reads_before_writes(
    body: &[Stmt],
    assigned: &mut BTreeSet<String>,
    reads: &mut BTreeSet<String>,
) {
    for stmt in body {
        collect_stmt_scope_reads_before_writes(stmt, assigned, reads);
    }
}

/// Collects caller reads for one statement before updating local assignment facts.
fn collect_stmt_scope_reads_before_writes(
    stmt: &Stmt,
    assigned: &mut BTreeSet<String>,
    reads: &mut BTreeSet<String>,
) {
    match &stmt.kind {
        StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
            collect_expr_scope_reads_before_writes(value, assigned, reads);
            assigned.insert(name.clone());
        }
        StmtKind::Echo(expr)
        | StmtKind::Throw(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::Return(Some(expr)) => {
            collect_expr_scope_reads_before_writes(expr, assigned, reads);
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            collect_expr_scope_reads_before_writes(condition, assigned, reads);
            let before = assigned.clone();
            let mut branch_outputs = Vec::new();
            let mut then_assigned = before.clone();
            collect_block_scope_reads_before_writes(then_body, &mut then_assigned, reads);
            branch_outputs.push(then_assigned);
            for (condition, body) in elseif_clauses {
                collect_expr_scope_reads_before_writes(condition, &before, reads);
                let mut branch_assigned = before.clone();
                collect_block_scope_reads_before_writes(body, &mut branch_assigned, reads);
                branch_outputs.push(branch_assigned);
            }
            if let Some(else_body) = else_body {
                let mut else_assigned = before.clone();
                collect_block_scope_reads_before_writes(else_body, &mut else_assigned, reads);
                branch_outputs.push(else_assigned);
                retain_definitely_assigned_after_branches(assigned, before, &branch_outputs);
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            collect_expr_scope_reads_before_writes(condition, assigned, reads);
            let mut body_assigned = assigned.clone();
            collect_block_scope_reads_before_writes(body, &mut body_assigned, reads);
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_stmt_scope_reads_before_writes(init, assigned, reads);
            }
            if let Some(condition) = condition {
                collect_expr_scope_reads_before_writes(condition, assigned, reads);
            }
            let mut body_assigned = assigned.clone();
            collect_block_scope_reads_before_writes(body, &mut body_assigned, reads);
            if let Some(update) = update {
                collect_stmt_scope_reads_before_writes(update, &mut body_assigned, reads);
            }
        }
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            body,
            ..
        } => {
            collect_expr_scope_reads_before_writes(array, assigned, reads);
            if expr_is_static_empty_array_literal_source(array) {
                return;
            }
            let mut body_assigned = assigned.clone();
            body_assigned.insert(value_var.clone());
            if let Some(key_var) = key_var {
                body_assigned.insert(key_var.clone());
            }
            collect_block_scope_reads_before_writes(body, &mut body_assigned, reads);
            if expr_is_non_empty_static_array_literal_source(array) {
                assigned.insert(value_var.clone());
                if let Some(key_var) = key_var {
                    assigned.insert(key_var.clone());
                }
            }
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            collect_expr_scope_reads_before_writes(subject, assigned, reads);
            for (conditions, body) in cases {
                for condition in conditions {
                    collect_expr_scope_reads_before_writes(condition, assigned, reads);
                }
                let mut case_assigned = assigned.clone();
                collect_block_scope_reads_before_writes(body, &mut case_assigned, reads);
            }
            if let Some(default) = default {
                let mut default_assigned = assigned.clone();
                collect_block_scope_reads_before_writes(default, &mut default_assigned, reads);
            }
        }
        StmtKind::Synthetic(body) | StmtKind::NamespaceBlock { body, .. } => {
            collect_block_scope_reads_before_writes(body, assigned, reads);
        }
        _ => {
            let mut access = EvalScopeAccess::new();
            collect_stmt_scope_access(stmt, &mut access);
            extend_reads_not_assigned(reads, assigned, access.reads);
            assigned.extend(access.writes);
        }
    }
}

/// Keeps only names assigned on every branch after an if/elseif/else chain.
fn retain_definitely_assigned_after_branches(
    assigned: &mut BTreeSet<String>,
    before: BTreeSet<String>,
    branch_outputs: &[BTreeSet<String>],
) {
    let mut definitely = before;
    for name in branch_outputs
        .first()
        .into_iter()
        .flat_map(|branch| branch.iter())
    {
        if branch_outputs.iter().all(|branch| branch.contains(name)) {
            definitely.insert(name.clone());
        }
    }
    *assigned = definitely;
}

/// Collects caller reads from one expression using current assignment facts.
fn collect_expr_scope_reads_before_writes(
    expr: &Expr,
    assigned: &BTreeSet<String>,
    reads: &mut BTreeSet<String>,
) {
    match &expr.kind {
        ExprKind::Variable(name) => {
            if !assigned.contains(name) {
                reads.insert(name.clone());
            }
        }
        ExprKind::Assignment {
            prelude,
            target,
            value,
            result_target,
            ..
        } => {
            let mut expr_assigned = assigned.clone();
            for stmt in prelude {
                collect_stmt_scope_reads_before_writes(stmt, &mut expr_assigned, reads);
            }
            collect_expr_scope_reads_before_writes(value, &expr_assigned, reads);
            match &target.kind {
                ExprKind::Variable(name) => {
                    expr_assigned.insert(name.clone());
                }
                _ => collect_expr_scope_reads_before_writes(target, &expr_assigned, reads),
            }
            if let Some(result_target) = result_target {
                collect_expr_scope_reads_before_writes(result_target, &expr_assigned, reads);
            }
        }
        _ => {
            let mut access = EvalScopeAccess::new();
            collect_expr_scope_access(expr, &mut access);
            extend_reads_not_assigned(reads, assigned, access.reads);
        }
    }
}

/// Adds collected reads that are not already definitely local to this fragment.
fn extend_reads_not_assigned(
    reads: &mut BTreeSet<String>,
    assigned: &BTreeSet<String>,
    names: BTreeSet<String>,
) {
    reads.extend(names.into_iter().filter(|name| !assigned.contains(name)));
}

/// Caller-scope variables that need array-specific call-site proof.
#[derive(Default)]
struct ArrayScopeReadConstraintSets {
    array_like: BTreeSet<String>,
    assoc: BTreeSet<String>,
}

/// Collects caller-scope reads that must be array-like for accepted AOT calls.
fn collect_array_scope_read_constraint_sets(
    program: &[Stmt],
    scope_reads: &BTreeSet<String>,
) -> ArrayScopeReadConstraintSets {
    let mut constraints = ArrayScopeReadConstraintSets::default();
    for stmt in program {
        collect_stmt_array_scope_read_constraints(stmt, scope_reads, &mut constraints);
    }
    constraints
}

/// Collects array constraints from one statement in the EIR AOT subset.
fn collect_stmt_array_scope_read_constraints(
    stmt: &Stmt,
    scope_reads: &BTreeSet<String>,
    constraints: &mut ArrayScopeReadConstraintSets,
) {
    match &stmt.kind {
        StmtKind::Synthetic(body) => {
            for stmt in body {
                collect_stmt_array_scope_read_constraints(stmt, scope_reads, constraints);
            }
        }
        StmtKind::Echo(expr) | StmtKind::ExprStmt(expr) | StmtKind::Return(Some(expr)) => {
            collect_expr_array_scope_read_constraints(expr, scope_reads, constraints);
        }
        StmtKind::Assign { value, .. } => {
            collect_expr_array_scope_read_constraints(value, scope_reads, constraints);
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            collect_expr_array_scope_read_constraints(condition, scope_reads, constraints);
            for stmt in then_body {
                collect_stmt_array_scope_read_constraints(stmt, scope_reads, constraints);
            }
            for (condition, body) in elseif_clauses {
                collect_expr_array_scope_read_constraints(condition, scope_reads, constraints);
                for stmt in body {
                    collect_stmt_array_scope_read_constraints(stmt, scope_reads, constraints);
                }
            }
            if let Some(else_body) = else_body {
                for stmt in else_body {
                    collect_stmt_array_scope_read_constraints(stmt, scope_reads, constraints);
                }
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            collect_expr_array_scope_read_constraints(condition, scope_reads, constraints);
            for stmt in body {
                collect_stmt_array_scope_read_constraints(stmt, scope_reads, constraints);
            }
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_stmt_array_scope_read_constraints(init, scope_reads, constraints);
            }
            if let Some(condition) = condition {
                collect_expr_array_scope_read_constraints(condition, scope_reads, constraints);
            }
            if let Some(update) = update {
                collect_stmt_array_scope_read_constraints(update, scope_reads, constraints);
            }
            for stmt in body {
                collect_stmt_array_scope_read_constraints(stmt, scope_reads, constraints);
            }
        }
        StmtKind::Foreach { array, body, .. } => {
            if let ExprKind::Variable(variable) = &array.kind {
                if scope_reads.contains(variable) {
                    constraints.array_like.insert(variable.clone());
                }
            }
            collect_expr_array_scope_read_constraints(array, scope_reads, constraints);
            if expr_is_static_empty_array_literal_source(array) {
                return;
            }
            for stmt in body {
                collect_stmt_array_scope_read_constraints(stmt, scope_reads, constraints);
            }
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            collect_expr_array_scope_read_constraints(subject, scope_reads, constraints);
            for (conditions, body) in cases {
                for condition in conditions {
                    collect_expr_array_scope_read_constraints(condition, scope_reads, constraints);
                }
                for stmt in body {
                    collect_stmt_array_scope_read_constraints(stmt, scope_reads, constraints);
                }
            }
            if let Some(default) = default {
                for stmt in default {
                    collect_stmt_array_scope_read_constraints(stmt, scope_reads, constraints);
                }
            }
        }
        _ => {}
    }
}

/// Collects array constraints from one expression in the EIR AOT subset.
fn collect_expr_array_scope_read_constraints(
    expr: &Expr,
    scope_reads: &BTreeSet<String>,
    constraints: &mut ArrayScopeReadConstraintSets,
) {
    match &expr.kind {
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::NamedArg { value: inner, .. } => {
            collect_expr_array_scope_read_constraints(inner, scope_reads, constraints);
        }
        ExprKind::BinaryOp { left, right, .. } => {
            collect_expr_array_scope_read_constraints(left, scope_reads, constraints);
            collect_expr_array_scope_read_constraints(right, scope_reads, constraints);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_expr_array_scope_read_constraints(condition, scope_reads, constraints);
            collect_expr_array_scope_read_constraints(then_expr, scope_reads, constraints);
            collect_expr_array_scope_read_constraints(else_expr, scope_reads, constraints);
        }
        ExprKind::ShortTernary { value, default } | ExprKind::NullCoalesce { value, default } => {
            collect_expr_array_scope_read_constraints(value, scope_reads, constraints);
            collect_expr_array_scope_read_constraints(default, scope_reads, constraints);
        }
        ExprKind::Cast { expr, .. } => {
            collect_expr_array_scope_read_constraints(expr, scope_reads, constraints);
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            collect_expr_array_scope_read_constraints(subject, scope_reads, constraints);
            for (conditions, result) in arms {
                for condition in conditions {
                    collect_expr_array_scope_read_constraints(condition, scope_reads, constraints);
                }
                collect_expr_array_scope_read_constraints(result, scope_reads, constraints);
            }
            if let Some(default) = default {
                collect_expr_array_scope_read_constraints(default, scope_reads, constraints);
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            collect_expr_array_scope_read_constraints(array, scope_reads, constraints);
            collect_expr_array_scope_read_constraints(index, scope_reads, constraints);
        }
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                collect_expr_array_scope_read_constraints(item, scope_reads, constraints);
            }
        }
        ExprKind::ArrayLiteralAssoc(pairs) => {
            for (key, value) in pairs {
                collect_expr_array_scope_read_constraints(key, scope_reads, constraints);
                collect_expr_array_scope_read_constraints(value, scope_reads, constraints);
            }
        }
        ExprKind::FunctionCall { name, args } => {
            collect_builtin_array_scope_read_constraints(
                name.as_str(),
                args,
                scope_reads,
                constraints,
            );
            for arg in args {
                collect_expr_array_scope_read_constraints(arg, scope_reads, constraints);
            }
        }
        _ => {}
    }
}

/// Collects array caller-type constraints from supported runtime builtin calls.
fn collect_builtin_array_scope_read_constraints(
    name: &str,
    args: &[Expr],
    scope_reads: &BTreeSet<String>,
    constraints: &mut ArrayScopeReadConstraintSets,
) {
    let short_name = php_symbol_key(name.trim_start_matches('\\'));
    let Some(args) = normalize_eir_runtime_builtin_args(&short_name, args) else {
        return;
    };
    match short_name.as_str() {
        "count" if (1..=2).contains(&args.len()) && eir_count_mode_is_default_zero(args.get(1)) => {
            collect_scope_array_like_constraint(&args[0], scope_reads, constraints);
        }
        "array_key_exists"
            if args.len() == 2 && eir_array_key_exists_static_key_is_safe(&args[0]) =>
        {
            collect_scope_array_like_constraint(&args[1], scope_reads, constraints);
            if eir_array_key_exists_static_key_needs_assoc_array(&args[0]) {
                collect_scope_assoc_array_constraint(&args[1], scope_reads, constraints);
            }
        }
        _ => {}
    }
}

/// Records that one expression must be a caller-side array when it reads scope.
fn collect_scope_array_like_constraint(
    expr: &Expr,
    scope_reads: &BTreeSet<String>,
    constraints: &mut ArrayScopeReadConstraintSets,
) {
    if let ExprKind::Variable(variable) = &expr.kind {
        if scope_reads.contains(variable) {
            constraints.array_like.insert(variable.clone());
        }
    }
}

/// Records that one expression must be a caller-side associative array.
fn collect_scope_assoc_array_constraint(
    expr: &Expr,
    scope_reads: &BTreeSet<String>,
    constraints: &mut ArrayScopeReadConstraintSets,
) {
    if let ExprKind::Variable(variable) = &expr.kind {
        if scope_reads.contains(variable) {
            constraints.assoc.insert(variable.clone());
        }
    }
}

/// Collects caller-scope reads that must be int/float for IEEE float predicates.
fn collect_float_predicate_scope_read_constraints(
    program: &[Stmt],
    scope_reads: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut constraints = BTreeSet::new();
    for stmt in program {
        collect_stmt_float_predicate_scope_read_constraints(stmt, scope_reads, &mut constraints);
    }
    constraints
}

/// Collects float-predicate constraints from one statement in the EIR AOT subset.
fn collect_stmt_float_predicate_scope_read_constraints(
    stmt: &Stmt,
    scope_reads: &BTreeSet<String>,
    constraints: &mut BTreeSet<String>,
) {
    match &stmt.kind {
        StmtKind::Synthetic(body) => {
            for stmt in body {
                collect_stmt_float_predicate_scope_read_constraints(stmt, scope_reads, constraints);
            }
        }
        StmtKind::Echo(expr) | StmtKind::ExprStmt(expr) | StmtKind::Return(Some(expr)) => {
            collect_expr_float_predicate_scope_read_constraints(expr, scope_reads, constraints);
        }
        StmtKind::Assign { value, .. } => {
            collect_expr_float_predicate_scope_read_constraints(value, scope_reads, constraints);
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            collect_expr_float_predicate_scope_read_constraints(
                condition,
                scope_reads,
                constraints,
            );
            for stmt in then_body {
                collect_stmt_float_predicate_scope_read_constraints(stmt, scope_reads, constraints);
            }
            for (condition, body) in elseif_clauses {
                collect_expr_float_predicate_scope_read_constraints(
                    condition,
                    scope_reads,
                    constraints,
                );
                for stmt in body {
                    collect_stmt_float_predicate_scope_read_constraints(
                        stmt,
                        scope_reads,
                        constraints,
                    );
                }
            }
            if let Some(else_body) = else_body {
                for stmt in else_body {
                    collect_stmt_float_predicate_scope_read_constraints(
                        stmt,
                        scope_reads,
                        constraints,
                    );
                }
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            collect_expr_float_predicate_scope_read_constraints(
                condition,
                scope_reads,
                constraints,
            );
            for stmt in body {
                collect_stmt_float_predicate_scope_read_constraints(stmt, scope_reads, constraints);
            }
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_stmt_float_predicate_scope_read_constraints(init, scope_reads, constraints);
            }
            if let Some(condition) = condition {
                collect_expr_float_predicate_scope_read_constraints(
                    condition,
                    scope_reads,
                    constraints,
                );
            }
            if let Some(update) = update {
                collect_stmt_float_predicate_scope_read_constraints(
                    update,
                    scope_reads,
                    constraints,
                );
            }
            for stmt in body {
                collect_stmt_float_predicate_scope_read_constraints(stmt, scope_reads, constraints);
            }
        }
        StmtKind::Foreach { array, body, .. } => {
            collect_expr_float_predicate_scope_read_constraints(array, scope_reads, constraints);
            if expr_is_static_empty_array_literal_source(array) {
                return;
            }
            for stmt in body {
                collect_stmt_float_predicate_scope_read_constraints(stmt, scope_reads, constraints);
            }
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            collect_expr_float_predicate_scope_read_constraints(subject, scope_reads, constraints);
            for (conditions, body) in cases {
                for condition in conditions {
                    collect_expr_float_predicate_scope_read_constraints(
                        condition,
                        scope_reads,
                        constraints,
                    );
                }
                for stmt in body {
                    collect_stmt_float_predicate_scope_read_constraints(
                        stmt,
                        scope_reads,
                        constraints,
                    );
                }
            }
            if let Some(default) = default {
                for stmt in default {
                    collect_stmt_float_predicate_scope_read_constraints(
                        stmt,
                        scope_reads,
                        constraints,
                    );
                }
            }
        }
        _ => {}
    }
}

/// Collects float-predicate constraints from one expression in the EIR AOT subset.
fn collect_expr_float_predicate_scope_read_constraints(
    expr: &Expr,
    scope_reads: &BTreeSet<String>,
    constraints: &mut BTreeSet<String>,
) {
    match &expr.kind {
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::NamedArg { value: inner, .. } => {
            collect_expr_float_predicate_scope_read_constraints(inner, scope_reads, constraints);
        }
        ExprKind::BinaryOp { left, right, .. } => {
            collect_expr_float_predicate_scope_read_constraints(left, scope_reads, constraints);
            collect_expr_float_predicate_scope_read_constraints(right, scope_reads, constraints);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_expr_float_predicate_scope_read_constraints(
                condition,
                scope_reads,
                constraints,
            );
            collect_expr_float_predicate_scope_read_constraints(
                then_expr,
                scope_reads,
                constraints,
            );
            collect_expr_float_predicate_scope_read_constraints(
                else_expr,
                scope_reads,
                constraints,
            );
        }
        ExprKind::ShortTernary { value, default } | ExprKind::NullCoalesce { value, default } => {
            collect_expr_float_predicate_scope_read_constraints(value, scope_reads, constraints);
            collect_expr_float_predicate_scope_read_constraints(default, scope_reads, constraints);
        }
        ExprKind::Cast { expr, .. } => {
            collect_expr_float_predicate_scope_read_constraints(expr, scope_reads, constraints);
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            collect_expr_float_predicate_scope_read_constraints(subject, scope_reads, constraints);
            for (conditions, result) in arms {
                for condition in conditions {
                    collect_expr_float_predicate_scope_read_constraints(
                        condition,
                        scope_reads,
                        constraints,
                    );
                }
                collect_expr_float_predicate_scope_read_constraints(
                    result,
                    scope_reads,
                    constraints,
                );
            }
            if let Some(default) = default {
                collect_expr_float_predicate_scope_read_constraints(
                    default,
                    scope_reads,
                    constraints,
                );
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            collect_expr_float_predicate_scope_read_constraints(array, scope_reads, constraints);
            collect_expr_float_predicate_scope_read_constraints(index, scope_reads, constraints);
        }
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                collect_expr_float_predicate_scope_read_constraints(item, scope_reads, constraints);
            }
        }
        ExprKind::ArrayLiteralAssoc(pairs) => {
            for (key, value) in pairs {
                collect_expr_float_predicate_scope_read_constraints(key, scope_reads, constraints);
                collect_expr_float_predicate_scope_read_constraints(
                    value,
                    scope_reads,
                    constraints,
                );
            }
        }
        ExprKind::FunctionCall { name, args } => {
            let name = php_symbol_key(name.as_str().trim_start_matches('\\'));
            if matches!(name.as_str(), "is_finite" | "is_infinite" | "is_nan") && args.len() == 1 {
                collect_scope_read_variables_in_expr(&args[0], scope_reads, constraints);
            }
            for arg in args {
                collect_expr_float_predicate_scope_read_constraints(arg, scope_reads, constraints);
            }
        }
        _ => {}
    }
}

/// Collects scope-read variable names that occur anywhere inside an expression.
fn collect_scope_read_variables_in_expr(
    expr: &Expr,
    scope_reads: &BTreeSet<String>,
    variables: &mut BTreeSet<String>,
) {
    match &expr.kind {
        ExprKind::Variable(name) => {
            if scope_reads.contains(name) {
                variables.insert(name.clone());
            }
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::NamedArg { value: inner, .. }
        | ExprKind::Cast { expr: inner, .. } => {
            collect_scope_read_variables_in_expr(inner, scope_reads, variables);
        }
        ExprKind::BinaryOp { left, right, .. } => {
            collect_scope_read_variables_in_expr(left, scope_reads, variables);
            collect_scope_read_variables_in_expr(right, scope_reads, variables);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_scope_read_variables_in_expr(condition, scope_reads, variables);
            collect_scope_read_variables_in_expr(then_expr, scope_reads, variables);
            collect_scope_read_variables_in_expr(else_expr, scope_reads, variables);
        }
        ExprKind::ShortTernary { value, default } | ExprKind::NullCoalesce { value, default } => {
            collect_scope_read_variables_in_expr(value, scope_reads, variables);
            collect_scope_read_variables_in_expr(default, scope_reads, variables);
        }
        ExprKind::ArrayAccess { array, index } => {
            collect_scope_read_variables_in_expr(array, scope_reads, variables);
            collect_scope_read_variables_in_expr(index, scope_reads, variables);
        }
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                collect_scope_read_variables_in_expr(item, scope_reads, variables);
            }
        }
        ExprKind::ArrayLiteralAssoc(pairs) => {
            for (key, value) in pairs {
                collect_scope_read_variables_in_expr(key, scope_reads, variables);
                collect_scope_read_variables_in_expr(value, scope_reads, variables);
            }
        }
        ExprKind::FunctionCall { args, .. } => {
            for arg in args {
                collect_scope_read_variables_in_expr(arg, scope_reads, variables);
            }
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            collect_scope_read_variables_in_expr(subject, scope_reads, variables);
            for (conditions, result) in arms {
                for condition in conditions {
                    collect_scope_read_variables_in_expr(condition, scope_reads, variables);
                }
                collect_scope_read_variables_in_expr(result, scope_reads, variables);
            }
            if let Some(default) = default {
                collect_scope_read_variables_in_expr(default, scope_reads, variables);
            }
        }
        _ => {}
    }
}

/// Adds one statement's eval-scope reads and writes to the accumulator.
fn collect_stmt_scope_access(stmt: &Stmt, access: &mut EvalScopeAccess) {
    match &stmt.kind {
        StmtKind::Echo(expr)
        | StmtKind::Throw(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::Return(Some(expr)) => collect_expr_scope_access(expr, access),
        StmtKind::Return(None)
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::IncludeOnceMark { .. }
        | StmtKind::FunctionVariantMark { .. } => {}
        StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
            collect_expr_scope_access(value, access);
            access.write(name);
        }
        StmtKind::RefAssign { target, source } => {
            access.write(target);
            collect_expr_scope_access(source, access);
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            collect_expr_scope_access(condition, access);
            collect_block_scope_access(then_body, access);
            for (condition, body) in elseif_clauses {
                collect_expr_scope_access(condition, access);
                collect_block_scope_access(body, access);
            }
            if let Some(else_body) = else_body {
                collect_block_scope_access(else_body, access);
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            collect_block_scope_access(then_body, access);
            if let Some(else_body) = else_body {
                collect_block_scope_access(else_body, access);
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            collect_expr_scope_access(condition, access);
            collect_block_scope_access(body, access);
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_stmt_scope_access(init, access);
            }
            if let Some(condition) = condition {
                collect_expr_scope_access(condition, access);
            }
            if let Some(update) = update {
                collect_stmt_scope_access(update, access);
            }
            collect_block_scope_access(body, access);
        }
        StmtKind::ArrayAssign {
            array,
            index,
            value,
        } => {
            access.read(array);
            access.write(array);
            collect_expr_scope_access(index, access);
            collect_expr_scope_access(value, access);
        }
        StmtKind::NestedArrayAssign { target, value } => {
            collect_assignment_target_scope_access(target, access);
            collect_expr_scope_access(value, access);
        }
        StmtKind::ArrayPush { array, value } => {
            access.read(array);
            access.write(array);
            collect_expr_scope_access(value, access);
        }
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            body,
            ..
        } => {
            collect_expr_scope_access(array, access);
            if expr_is_static_empty_array_literal_source(array) {
                return;
            }
            if let Some(key_var) = key_var {
                access.write(key_var);
            }
            access.write(value_var);
            collect_block_scope_access(body, access);
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            collect_expr_scope_access(subject, access);
            for (conditions, body) in cases {
                for condition in conditions {
                    collect_expr_scope_access(condition, access);
                }
                collect_block_scope_access(body, access);
            }
            if let Some(default) = default {
                collect_block_scope_access(default, access);
            }
        }
        StmtKind::IncludeOnceGuard { body, .. } | StmtKind::Synthetic(body) => {
            collect_block_scope_access(body, access);
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            collect_block_scope_access(try_body, access);
            for catch in catches {
                if let Some(variable) = &catch.variable {
                    access.write(variable);
                }
                collect_block_scope_access(&catch.body, access);
            }
            if let Some(finally_body) = finally_body {
                collect_block_scope_access(finally_body, access);
            }
        }
        StmtKind::NamespaceBlock { body, .. } => collect_block_scope_access(body, access),
        StmtKind::FunctionDecl { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::ConstDecl { .. }
        | StmtKind::ClassDecl { .. }
        | StmtKind::EnumDecl { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::InterfaceDecl { .. }
        | StmtKind::TraitDecl { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => {}
        StmtKind::ListUnpack { vars, value } => {
            collect_expr_scope_access(value, access);
            for var in vars {
                access.write(var);
            }
        }
        StmtKind::Global { vars } => {
            for var in vars {
                access.write(var);
            }
            access.creates_unknown_vars = true;
        }
        StmtKind::StaticVar { name, init } => {
            collect_expr_scope_access(init, access);
            access.write(name);
            access.creates_unknown_vars = true;
        }
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            collect_expr_scope_access(object, access);
            collect_expr_scope_access(value, access);
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => {
            collect_expr_scope_access(object, access);
            collect_expr_scope_access(index, access);
            collect_expr_scope_access(value, access);
        }
        StmtKind::StaticPropertyAssign { value, .. }
        | StmtKind::StaticPropertyArrayPush { value, .. } => {
            collect_expr_scope_access(value, access);
        }
        StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            collect_expr_scope_access(index, access);
            collect_expr_scope_access(value, access);
        }
        StmtKind::Include { path, .. } => collect_expr_scope_access(path, access),
    }
}

/// Adds every statement in a block to the scope access accumulator.
fn collect_block_scope_access(body: &[Stmt], access: &mut EvalScopeAccess) {
    for stmt in body {
        collect_stmt_scope_access(stmt, access);
    }
}

/// Adds one expression's eval-scope reads and writes to the accumulator.
fn collect_expr_scope_access(expr: &Expr, access: &mut EvalScopeAccess) {
    match &expr.kind {
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::ConstRef(_)
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::MagicConstant(_) => {}
        ExprKind::ObjectClassName { object } => collect_expr_scope_access(object, access),
        ExprKind::Variable(name)
        | ExprKind::PreIncrement(name)
        | ExprKind::PostIncrement(name)
        | ExprKind::PreDecrement(name)
        | ExprKind::PostDecrement(name) => {
            access.read(name);
            if matches!(
                &expr.kind,
                ExprKind::PreIncrement(_)
                    | ExprKind::PostIncrement(_)
                    | ExprKind::PreDecrement(_)
                    | ExprKind::PostDecrement(_)
            ) {
                access.write(name);
            }
        }
        ExprKind::BinaryOp { left, right, .. } => {
            collect_expr_scope_access(left, access);
            collect_expr_scope_access(right, access);
        }
        ExprKind::InstanceOf { value, target } => {
            collect_expr_scope_access(value, access);
            if let crate::parser::ast::InstanceOfTarget::Expr(target) = target {
                collect_expr_scope_access(target, access);
            }
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::Clone(inner)
        | ExprKind::YieldFrom(inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::PtrCast { expr: inner, .. } => collect_expr_scope_access(inner, access),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default }
        | ExprKind::Pipe {
            value,
            callable: default,
        } => {
            collect_expr_scope_access(value, access);
            collect_expr_scope_access(default, access);
        }
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            for stmt in prelude {
                collect_stmt_scope_access(stmt, access);
            }
            collect_assignment_target_scope_access(target, access);
            collect_expr_scope_access(value, access);
            if let Some(result_target) = result_target {
                collect_assignment_target_scope_access(result_target, access);
            }
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::ExprCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::NewScopedObject { args, .. } => {
            if let ExprKind::ExprCall { callee, .. } = &expr.kind {
                collect_expr_scope_access(callee, access);
            }
            for arg in args {
                collect_expr_scope_access(arg, access);
            }
        }
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                collect_expr_scope_access(item, access);
            }
        }
        ExprKind::ArrayLiteralAssoc(pairs) => {
            for (key, value) in pairs {
                collect_expr_scope_access(key, access);
                collect_expr_scope_access(value, access);
            }
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            collect_expr_scope_access(subject, access);
            for (conditions, value) in arms {
                for condition in conditions {
                    collect_expr_scope_access(condition, access);
                }
                collect_expr_scope_access(value, access);
            }
            if let Some(default) = default {
                collect_expr_scope_access(default, access);
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            collect_expr_scope_access(array, access);
            collect_expr_scope_access(index, access);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_expr_scope_access(condition, access);
            collect_expr_scope_access(then_expr, access);
            collect_expr_scope_access(else_expr, access);
        }
        ExprKind::Closure {
            params,
            body,
            captures,
            capture_refs,
            ..
        } => {
            for (_, _, default, _) in params {
                if let Some(default) = default {
                    collect_expr_scope_access(default, access);
                }
            }
            for capture in captures.iter().chain(capture_refs.iter()) {
                access.read(capture);
            }
            collect_block_scope_access(body, access);
        }
        ExprKind::NamedArg { value, .. } => collect_expr_scope_access(value, access),
        ExprKind::IncludeValue { path, .. } => collect_expr_scope_access(path, access),
        ExprKind::NewDynamic { name_expr, args } => {
            collect_expr_scope_access(name_expr, access);
            for arg in args {
                collect_expr_scope_access(arg, access);
            }
        }
        ExprKind::NewDynamicObject {
            class_name, args, ..
        } => {
            collect_expr_scope_access(class_name, access);
            for arg in args {
                collect_expr_scope_access(arg, access);
            }
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => {
            collect_expr_scope_access(object, access);
        }
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            collect_expr_scope_access(object, access);
            collect_expr_scope_access(property, access);
        }
        ExprKind::NullsafeMethodCall { object, args, .. }
        | ExprKind::MethodCall { object, args, .. } => {
            collect_expr_scope_access(object, access);
            for arg in args {
                collect_expr_scope_access(arg, access);
            }
        }
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => {
            collect_expr_scope_access(object, access);
            collect_expr_scope_access(method, access);
            for arg in args {
                collect_expr_scope_access(arg, access);
            }
        }
        ExprKind::StaticPropertyAccess { .. } | ExprKind::This => {}
        ExprKind::BufferNew { len, .. } => collect_expr_scope_access(len, access),
        ExprKind::FirstClassCallable(target) => {
            collect_callable_target_scope_access(target, access)
        }
        ExprKind::Yield { key, value } => {
            if let Some(key) = key {
                collect_expr_scope_access(key, access);
            }
            if let Some(value) = value {
                collect_expr_scope_access(value, access);
            }
        }
    }
}

/// Records the variable effects of an assignment target expression.
fn collect_assignment_target_scope_access(expr: &Expr, access: &mut EvalScopeAccess) {
    match &expr.kind {
        ExprKind::Variable(name) => access.write(name),
        ExprKind::ArrayAccess { array, index } => {
            collect_expr_scope_access(array, access);
            collect_expr_scope_access(index, access);
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => {
            collect_expr_scope_access(object, access);
        }
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            collect_expr_scope_access(object, access);
            collect_expr_scope_access(property, access);
        }
        _ => {
            collect_expr_scope_access(expr, access);
            access.unknown_write();
        }
    }
}

/// Adds variable reads from a first-class callable target.
fn collect_callable_target_scope_access(
    target: &crate::parser::ast::CallableTarget,
    access: &mut EvalScopeAccess,
) {
    match target {
        crate::parser::ast::CallableTarget::Function(_) => {}
        crate::parser::ast::CallableTarget::StaticMethod { .. } => {}
        crate::parser::ast::CallableTarget::Method { object, .. } => {
            collect_expr_scope_access(object, access);
        }
    }
}

/// Hashes a fragment with a stable FNV-1a variant for deterministic symbol names.
fn stable_fragment_hash(fragment: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in fragment.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Definite local facts tracked while classifying an eval fragment for EIR AOT.
#[derive(Clone, Default)]
struct EirLocalFacts {
    assigned: BTreeSet<String>,
    int_locals: BTreeSet<String>,
    float_locals: BTreeSet<String>,
    array_locals: BTreeSet<String>,
}

impl EirLocalFacts {
    /// Creates empty local facts for a fresh eval fragment block.
    fn new() -> Self {
        Self::default()
    }

    /// Returns true when a variable is definitely assigned in this control-flow path.
    fn is_assigned(&self, name: &str) -> bool {
        self.assigned.contains(name)
    }

    /// Returns true when a variable is definitely assigned as an integer value.
    fn is_int_local(&self, name: &str) -> bool {
        self.int_locals.contains(name)
    }

    /// Returns true when a variable is definitely assigned as a floating value.
    fn is_float_local(&self, name: &str) -> bool {
        self.float_locals.contains(name)
    }

    /// Returns true when a variable is definitely assigned from a static array literal.
    fn is_array_local(&self, name: &str) -> bool {
        self.array_locals.contains(name)
    }

    /// Records an assignment and updates scalar/array local facts for that variable.
    fn assign<S>(
        &mut self,
        name: &str,
        value: &Expr,
        support: &S,
        scope_reads: Option<&BTreeSet<String>>,
    ) where
        S: EirStaticCallSupport,
    {
        self.assigned.insert(name.to_string());
        if expr_is_eir_int_value_safe(value, support, self, scope_reads) {
            self.int_locals.insert(name.to_string());
        } else {
            self.int_locals.remove(name);
        }
        if expr_is_eir_float_value_safe(value, support, self, scope_reads) {
            self.float_locals.insert(name.to_string());
        } else {
            self.float_locals.remove(name);
        }
        if expr_is_eir_static_array_literal_source_safe(value, support, self, scope_reads) {
            self.array_locals.insert(name.to_string());
        } else {
            self.array_locals.remove(name);
        }
    }

    /// Records that a variable is definitely assigned, but with no narrower local fact.
    fn assign_unknown(&mut self, name: &str) {
        self.assigned.insert(name.to_string());
        self.int_locals.remove(name);
        self.array_locals.remove(name);
    }
}

/// Returns true when the fragment can be lowered as a no-scope EIR function today.
fn program_is_eir_function_safe<S>(program: &[Stmt], support: &S) -> bool
where
    S: EirStaticCallSupport,
{
    let mut facts = EirLocalFacts::new();
    block_is_eir_function_safe(program, support, &mut facts, None, 0).is_some()
}

/// Returns true when a fragment can be lowered as a scope-aware EIR function.
fn program_is_eir_scope_function_safe<S>(
    program: &[Stmt],
    support: &S,
    scope_names: &BTreeSet<String>,
) -> bool
where
    S: EirStaticCallSupport,
{
    let mut facts = EirLocalFacts::new();
    block_is_eir_function_safe(program, support, &mut facts, Some(scope_names), 0).is_some()
}

/// Checks a statement block for the no-scope EIR-function eval subset.
fn block_is_eir_function_safe<S>(
    body: &[Stmt],
    support: &S,
    facts: &mut EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
    loop_depth: usize,
) -> Option<bool>
where
    S: EirStaticCallSupport,
{
    let mut terminated = false;
    for stmt in body {
        if terminated {
            return None;
        }
        let done = stmt_is_eir_function_safe(stmt, support, facts, scope_reads, loop_depth)?;
        terminated = done;
    }
    Some(terminated)
}

/// Checks one statement for the initial no-scope EIR-function eval subset.
fn stmt_is_eir_function_safe<S>(
    stmt: &Stmt,
    support: &S,
    facts: &mut EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
    loop_depth: usize,
) -> Option<bool>
where
    S: EirStaticCallSupport,
{
    match &stmt.kind {
        StmtKind::Synthetic(body) => {
            block_is_eir_function_safe(body, support, facts, scope_reads, loop_depth)
        }
        StmtKind::Echo(expr) => {
            expr_is_eir_function_safe(expr, support, facts, scope_reads).then_some(false)
        }
        StmtKind::Assign { name, value }
            if scope_reads.is_some_and(|names| names.contains(name)) =>
        {
            expr_is_eir_function_safe(value, support, facts, scope_reads).then_some(())?;
            facts.assign(name, value, support, scope_reads);
            Some(false)
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            expr_is_eir_function_safe(condition, support, facts, scope_reads).then_some(())?;
            if_stmt_is_eir_function_safe(
                then_body,
                elseif_clauses,
                else_body.as_deref(),
                support,
                facts,
                scope_reads,
                loop_depth,
            )
            .then_some(false)
        }
        StmtKind::While { condition, body } => {
            expr_is_eir_function_safe(condition, support, facts, scope_reads).then_some(())?;
            let mut body_facts = facts.clone();
            block_is_eir_function_safe(body, support, &mut body_facts, scope_reads, loop_depth + 1)
                .map(|_| false)
        }
        StmtKind::DoWhile { condition, body } => {
            let mut body_facts = facts.clone();
            block_is_eir_function_safe(
                body,
                support,
                &mut body_facts,
                scope_reads,
                loop_depth + 1,
            )?;
            expr_is_eir_function_safe(condition, support, &body_facts, scope_reads).then_some(false)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                if stmt_is_eir_function_safe(init, support, facts, scope_reads, loop_depth)? {
                    return None;
                }
            }
            if let Some(condition) = condition {
                expr_is_eir_function_safe(condition, support, facts, scope_reads).then_some(())?;
            }
            let mut body_facts = facts.clone();
            block_is_eir_function_safe(
                body,
                support,
                &mut body_facts,
                scope_reads,
                loop_depth + 1,
            )?;
            if let Some(update) = update {
                if stmt_is_eir_function_safe(
                    update,
                    support,
                    &mut body_facts,
                    scope_reads,
                    loop_depth + 1,
                )? {
                    return None;
                }
            }
            Some(false)
        }
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            value_by_ref,
            body,
        } => {
            let static_empty = expr_is_static_empty_array_literal_source(array);
            if (scope_reads.is_none() && !static_empty)
                || *value_by_ref
                || !expr_is_eir_foreach_source_safe(array, scope_reads)
            {
                return None;
            }
            expr_is_eir_foreach_source_lowerable(array, support, facts, scope_reads)
                .then_some(())?;
            let mut body_facts = facts.clone();
            body_facts.assign_unknown(value_var);
            if let Some(key_var) = key_var {
                body_facts.assign_unknown(key_var);
            }
            block_is_eir_function_safe(
                body,
                support,
                &mut body_facts,
                scope_reads,
                loop_depth + 1,
            )?;
            if expr_is_non_empty_static_array_literal_source(array) {
                facts.assign_unknown(value_var);
                if let Some(key_var) = key_var {
                    facts.assign_unknown(key_var);
                }
            }
            Some(false)
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => switch_stmt_is_eir_function_safe(
            subject,
            cases,
            default.as_deref(),
            support,
            facts,
            scope_reads,
            loop_depth,
        )
        .then_some(false),
        StmtKind::Break(level) => (*level > 0 && *level <= loop_depth).then_some(true),
        StmtKind::Continue(level) => (*level > 0 && *level <= loop_depth).then_some(true),
        StmtKind::Return(Some(expr)) => {
            expr_is_eir_function_safe(expr, support, facts, scope_reads).then_some(true)
        }
        StmtKind::Return(None) => Some(true),
        StmtKind::ExprStmt(expr) => match &expr.kind {
            ExprKind::Print(inner) => {
                expr_is_eir_function_safe(inner, support, facts, scope_reads).then_some(false)
            }
            _ => expr_is_eir_function_safe(expr, support, facts, scope_reads).then_some(false),
        },
        _ => None,
    }
}

/// Returns true when a foreach source has EIR-safe eval AOT semantics.
fn expr_is_eir_foreach_source_safe(expr: &Expr, scope_reads: Option<&BTreeSet<String>>) -> bool {
    if expr_is_static_array_literal_source(expr) {
        return true;
    }
    matches!(
        &expr.kind,
        ExprKind::Variable(name) if scope_reads.is_some_and(|reads| reads.contains(name))
    )
}

/// Returns true when a foreach source can be lowered by the EIR backend.
fn expr_is_eir_foreach_source_lowerable<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    match &expr.kind {
        ExprKind::Variable(name) if scope_reads.is_some_and(|reads| reads.contains(name)) => true,
        _ => expr_is_eir_static_array_source_safe(expr, support, facts, scope_reads),
    }
}

/// Returns true when a static array source is a literal expression.
fn expr_is_static_array_literal_source(expr: &Expr) -> bool {
    matches!(
        &expr.kind,
        ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_)
    )
}

/// Returns true when a static array source is a literal known to skip its body.
fn expr_is_static_empty_array_literal_source(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::ArrayLiteral(items) => items.is_empty(),
        ExprKind::ArrayLiteralAssoc(pairs) => pairs.is_empty(),
        _ => false,
    }
}

/// Returns true when a static array source is a literal known to iterate at least once.
fn expr_is_non_empty_static_array_literal_source(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::ArrayLiteral(items) => !items.is_empty(),
        ExprKind::ArrayLiteralAssoc(pairs) => !pairs.is_empty(),
        _ => false,
    }
}

/// Checks a switch statement while preserving conservative assignment facts.
fn switch_stmt_is_eir_function_safe<S>(
    subject: &Expr,
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: Option<&[Stmt]>,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
    loop_depth: usize,
) -> bool
where
    S: EirStaticCallSupport,
{
    if !expr_is_eir_function_safe(subject, support, facts, scope_reads) {
        return false;
    }
    if !switch_default_position_is_eir_safe(cases, default) {
        return false;
    }
    for (conditions, body) in cases {
        for condition in conditions {
            if !expr_is_eir_function_safe(condition, support, facts, scope_reads) {
                return false;
            }
        }
        let mut case_facts = facts.clone();
        if block_is_eir_function_safe(body, support, &mut case_facts, scope_reads, loop_depth + 1)
            .is_none()
        {
            return false;
        }
    }
    if let Some(default) = default {
        let mut default_facts = facts.clone();
        if block_is_eir_function_safe(
            default,
            support,
            &mut default_facts,
            scope_reads,
            loop_depth + 1,
        )
        .is_none()
        {
            return false;
        }
    }
    true
}

/// Returns true when EIR switch lowering can reconstruct the default source position.
fn switch_default_position_is_eir_safe(
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: Option<&[Stmt]>,
) -> bool {
    let Some(default) = default else {
        return true;
    };
    if cases.is_empty() {
        return true;
    }
    let Some(default_start) = default.first().map(|stmt| stmt.span) else {
        return false;
    };
    if default_start == crate::span::Span::dummy() {
        return false;
    }
    cases.iter().all(|(conditions, _)| {
        conditions
            .first()
            .is_some_and(|condition| condition.span != crate::span::Span::dummy())
    })
}

/// Checks an if/elseif/else chain and propagates only definitely assigned locals.
fn if_stmt_is_eir_function_safe<S>(
    then_body: &[Stmt],
    elseif_clauses: &[(Expr, Vec<Stmt>)],
    else_body: Option<&[Stmt]>,
    support: &S,
    facts: &mut EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
    loop_depth: usize,
) -> bool
where
    S: EirStaticCallSupport,
{
    let before = facts.clone();
    let mut branch_outputs = Vec::new();
    let mut then_facts = before.clone();
    if block_is_eir_function_safe(then_body, support, &mut then_facts, scope_reads, loop_depth)
        .is_none()
    {
        return false;
    }
    branch_outputs.push(then_facts);

    for (condition, body) in elseif_clauses {
        if !expr_is_eir_function_safe(condition, support, &before, scope_reads) {
            return false;
        }
        let mut branch_facts = before.clone();
        if block_is_eir_function_safe(body, support, &mut branch_facts, scope_reads, loop_depth)
            .is_none()
        {
            return false;
        }
        branch_outputs.push(branch_facts);
    }

    let Some(else_body) = else_body else {
        return true;
    };
    let mut else_facts = before.clone();
    if block_is_eir_function_safe(else_body, support, &mut else_facts, scope_reads, loop_depth)
        .is_none()
    {
        return false;
    }
    branch_outputs.push(else_facts);

    *facts = definitely_assigned_after_eir_branches(before, &branch_outputs);
    true
}

/// Keeps only local facts that are true after every branch in an if/elseif/else chain.
fn definitely_assigned_after_eir_branches(
    before: EirLocalFacts,
    branch_outputs: &[EirLocalFacts],
) -> EirLocalFacts {
    let mut definitely = before;
    for name in branch_outputs
        .first()
        .into_iter()
        .flat_map(|branch| branch.assigned.iter())
    {
        if branch_outputs
            .iter()
            .all(|branch| branch.assigned.contains(name))
        {
            definitely.assigned.insert(name.clone());
        }
    }
    for name in branch_outputs
        .first()
        .into_iter()
        .flat_map(|branch| branch.int_locals.iter())
    {
        if branch_outputs
            .iter()
            .all(|branch| branch.int_locals.contains(name))
        {
            definitely.int_locals.insert(name.clone());
        }
    }
    for name in branch_outputs
        .first()
        .into_iter()
        .flat_map(|branch| branch.array_locals.iter())
    {
        if branch_outputs
            .iter()
            .all(|branch| branch.array_locals.contains(name))
        {
            definitely.array_locals.insert(name.clone());
        }
    }
    definitely
}

/// Checks one expression for the initial no-scope EIR-function eval subset.
fn expr_is_eir_function_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    match &expr.kind {
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null => true,
        ExprKind::Variable(name) => {
            facts.is_assigned(name) || scope_reads.is_some_and(|reads| reads.contains(name))
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner) => expr_is_eir_function_safe(inner, support, facts, scope_reads),
        ExprKind::PreIncrement(name)
        | ExprKind::PostIncrement(name)
        | ExprKind::PreDecrement(name)
        | ExprKind::PostDecrement(name) => facts.is_int_local(name),
        ExprKind::BinaryOp { left, op, right } => {
            matches!(
                op,
                BinOp::Add
                    | BinOp::Sub
                    | BinOp::Mul
                    | BinOp::Div
                    | BinOp::Mod
                    | BinOp::Pow
                    | BinOp::Lt
                    | BinOp::Gt
                    | BinOp::LtEq
                    | BinOp::GtEq
                    | BinOp::Eq
                    | BinOp::NotEq
                    | BinOp::StrictEq
                    | BinOp::StrictNotEq
                    | BinOp::And
                    | BinOp::Or
                    | BinOp::Xor
                    | BinOp::BitAnd
                    | BinOp::BitOr
                    | BinOp::BitXor
                    | BinOp::ShiftLeft
                    | BinOp::ShiftRight
                    | BinOp::Spaceship
                    | BinOp::Concat
            ) && expr_is_eir_function_safe(left, support, facts, scope_reads)
                && expr_is_eir_function_safe(right, support, facts, scope_reads)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_is_eir_function_safe(condition, support, facts, scope_reads)
                && expr_is_eir_function_safe(then_expr, support, facts, scope_reads)
                && expr_is_eir_function_safe(else_expr, support, facts, scope_reads)
        }
        ExprKind::ShortTernary { value, default } => {
            expr_is_eir_function_safe(value, support, facts, scope_reads)
                && expr_is_eir_function_safe(default, support, facts, scope_reads)
        }
        ExprKind::NullCoalesce { value, default } => {
            expr_is_eir_function_safe(value, support, facts, scope_reads)
                && expr_is_eir_function_safe(default, support, facts, scope_reads)
        }
        ExprKind::Cast { target, expr } => {
            matches!(
                target,
                CastType::Int | CastType::Float | CastType::String | CastType::Bool
            ) && expr_is_eir_function_safe(expr, support, facts, scope_reads)
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_is_eir_function_safe(subject, support, facts, scope_reads)
                && default.as_ref().is_some_and(|default| {
                    expr_is_eir_function_safe(default, support, facts, scope_reads)
                })
                && arms.iter().all(|(conditions, result)| {
                    conditions.iter().all(|condition| {
                        expr_is_eir_function_safe(condition, support, facts, scope_reads)
                    }) && expr_is_eir_function_safe(result, support, facts, scope_reads)
                })
        }
        ExprKind::ArrayAccess { array, index } => {
            expr_is_eir_static_array_source_safe(array, support, facts, scope_reads)
                && expr_is_eir_function_safe(index, support, facts, scope_reads)
        }
        ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) => {
            expr_is_eir_static_array_source_safe(expr, support, facts, scope_reads)
        }
        ExprKind::FunctionCall { name, args } => {
            eir_call_user_func_call_is_safe(name.as_str(), args, support, facts, scope_reads)
                || eir_construct_call_is_safe(name.as_str(), args, support, facts, scope_reads)
                || eir_runtime_builtin_call_is_safe(
                    name.as_str(),
                    args,
                    support,
                    facts,
                    scope_reads,
                )
                || fold_static_builtin_int_call(name.as_str().trim_start_matches('\\'), args)
                    .is_some()
                || support.function_supported(name.as_str(), args)
        }
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => support.static_method_supported(receiver, method, args),
        _ => false,
    }
}

/// Returns true when a static `call_user_func*()` callback maps to an AOT-safe call.
fn eir_call_user_func_call_is_safe<S>(
    name: &str,
    args: &[Expr],
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    match php_symbol_key(name.trim_start_matches('\\')).as_str() {
        "call_user_func" => {
            let Some((callback, callback_args)) = args.split_first() else {
                return false;
            };
            static_callback_call_is_eir_safe(callback, callback_args, support, facts, scope_reads)
        }
        "call_user_func_array" => {
            let [callback, arg_array] = args else {
                return false;
            };
            let Some(callback_args) = static_call_user_func_array_args(arg_array) else {
                return false;
            };
            static_callback_call_is_eir_safe(callback, &callback_args, support, facts, scope_reads)
        }
        _ => false,
    }
}

/// Returns true when a compile-time callback names a safe function or static method.
fn static_callback_call_is_eir_safe<S>(
    callback: &Expr,
    callback_args: &[Expr],
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    if let Some((receiver, method)) = static_callback_static_method_parts(callback) {
        return support.static_method_supported(&receiver, method.as_str(), callback_args);
    }
    static_callback_function_name(callback).is_some_and(|callback_name| {
        let short_callback = callback_name.trim_start_matches('\\');
        eir_runtime_builtin_call_is_safe(short_callback, callback_args, support, facts, scope_reads)
            || fold_static_builtin_call(short_callback, callback_args).is_some()
            || support.function_supported(short_callback, callback_args)
    })
}

/// Returns the function name from a compile-time callback expression.
fn static_callback_function_name(callback: &Expr) -> Option<&str> {
    match &callback.kind {
        ExprKind::StringLiteral(name) if !name.contains("::") => Some(name.as_str()),
        ExprKind::FirstClassCallable(CallableTarget::Function(name)) => Some(name.as_str()),
        _ => None,
    }
}

/// Returns the named receiver and method from a compile-time static-method callback.
fn static_callback_static_method_parts(callback: &Expr) -> Option<(StaticReceiver, String)> {
    match &callback.kind {
        ExprKind::StringLiteral(name) => static_callback_static_method_string_parts(name),
        ExprKind::FirstClassCallable(CallableTarget::StaticMethod { receiver, method }) => {
            Some((receiver.clone(), method.clone()))
        }
        ExprKind::ArrayLiteral(items) => static_callback_static_method_array_parts(items),
        _ => None,
    }
}

/// Splits a literal `Class::method` callback into its receiver and method.
fn static_callback_static_method_string_parts(name: &str) -> Option<(StaticReceiver, String)> {
    let (class_name, method) = name.trim_start_matches('\\').rsplit_once("::")?;
    let receiver = static_callback_static_method_named_receiver(class_name)?;
    if method.is_empty() {
        return None;
    }
    Some((receiver, method.to_string()))
}

/// Extracts a literal `["Class", "method"]` callback target.
fn static_callback_static_method_array_parts(items: &[Expr]) -> Option<(StaticReceiver, String)> {
    let [class_expr, method_expr] = items else {
        return None;
    };
    let receiver = static_callback_static_method_array_receiver(class_expr)?;
    let ExprKind::StringLiteral(method) = &method_expr.kind else {
        return None;
    };
    if method.is_empty() {
        return None;
    }
    Some((receiver, method.clone()))
}

/// Returns the static receiver from the class part of a callable array.
fn static_callback_static_method_array_receiver(class_expr: &Expr) -> Option<StaticReceiver> {
    match &class_expr.kind {
        ExprKind::StringLiteral(class_name) => {
            static_callback_static_method_named_receiver(class_name)
        }
        ExprKind::ClassConstant {
            receiver: StaticReceiver::Named(name),
        } => Some(StaticReceiver::Named(name.clone())),
        _ => None,
    }
}

/// Returns a named static receiver from a literal class name.
fn static_callback_static_method_named_receiver(class_name: &str) -> Option<StaticReceiver> {
    let class_name = class_name.trim_start_matches('\\');
    if class_name.is_empty() {
        return None;
    }
    Some(StaticReceiver::Named(Name::from(class_name.to_string())))
}

/// Converts a static `call_user_func_array()` argument array into callback args.
fn static_call_user_func_array_args(arg_array: &Expr) -> Option<Vec<Expr>> {
    match &arg_array.kind {
        ExprKind::ArrayLiteral(items) => Some(items.clone()),
        ExprKind::ArrayLiteralAssoc(pairs) => {
            static_call_user_func_array_assoc_args(pairs.as_slice())
        }
        _ => None,
    }
}

/// Converts literal associative callback arrays into positional or named callback args.
fn static_call_user_func_array_assoc_args(pairs: &[(Expr, Expr)]) -> Option<Vec<Expr>> {
    let mut args = Vec::with_capacity(pairs.len());
    for (key, value) in pairs {
        match &key.kind {
            ExprKind::StringLiteral(name) => {
                args.push(Expr::new(
                    ExprKind::NamedArg {
                        name: name.clone(),
                        value: Box::new(value.clone()),
                    },
                    value.span,
                ));
            }
            ExprKind::IntLiteral(_) => args.push(value.clone()),
            _ => return None,
        }
    }
    Some(args)
}

/// Returns true when an array source can be materialized inside eval EIR AOT.
fn expr_is_eir_static_array_source_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    match &expr.kind {
        ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) => {
            expr_is_eir_static_array_literal_source_safe(expr, support, facts, scope_reads)
        }
        ExprKind::Variable(name) => facts.is_array_local(name),
        _ => false,
    }
}

/// Returns true when a literal array expression can be materialized inside eval EIR AOT.
fn expr_is_eir_static_array_literal_source_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    match &expr.kind {
        ExprKind::ArrayLiteral(items) => items
            .iter()
            .all(|item| expr_is_eir_static_array_value_safe(item, support, facts, scope_reads)),
        ExprKind::ArrayLiteralAssoc(pairs) => {
            expr_is_eir_static_assoc_array_source_safe(pairs, support, facts, scope_reads)
        }
        _ => false,
    }
}

/// Returns true when an associative array source has statically reconstructable key semantics.
fn expr_is_eir_static_assoc_array_source_safe<S>(
    pairs: &[(Expr, Expr)],
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    let mut next_auto_key = 0i64;
    let mut auto_key_initialized = false;
    for (key, value) in pairs {
        if static_assoc_key_is_parser_generated(key, value) {
            let ExprKind::IntLiteral(generated) = &key.kind else {
                return false;
            };
            if *generated != next_auto_key {
                return false;
            }
            if !expr_is_eir_static_array_value_safe(value, support, facts, scope_reads) {
                return false;
            }
            advance_static_array_auto_key(&mut next_auto_key, &mut auto_key_initialized);
            continue;
        }
        if !expr_is_eir_static_array_key_safe(key, support, facts, scope_reads)
            || !expr_is_eir_static_array_value_safe(value, support, facts, scope_reads)
        {
            return false;
        }
        update_static_array_auto_key_from_explicit_key(
            key,
            &mut next_auto_key,
            &mut auto_key_initialized,
        );
    }
    true
}

/// Returns true when a static array key can be lowered without eval bridge state.
fn expr_is_eir_static_array_key_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    match &expr.kind {
        ExprKind::IntLiteral(_) | ExprKind::BoolLiteral(_) | ExprKind::Null => {
            expr_is_eir_function_safe(expr, support, facts, scope_reads)
        }
        ExprKind::FloatLiteral(_) if static_integral_float_array_key_value(expr).is_some() => {
            expr_is_eir_function_safe(expr, support, facts, scope_reads)
        }
        ExprKind::Negate(inner)
            if matches!(inner.kind, ExprKind::IntLiteral(_))
                || static_integral_float_array_key_value(expr).is_some() =>
        {
            expr_is_eir_function_safe(expr, support, facts, scope_reads)
        }
        ExprKind::StringLiteral(_) => expr_is_eir_function_safe(expr, support, facts, scope_reads),
        _ => false,
    }
}

/// Returns true for parser-synthesized integer keys from unkeyed assoc entries.
fn static_assoc_key_is_parser_generated(key: &Expr, value: &Expr) -> bool {
    matches!(key.kind, ExprKind::IntLiteral(_)) && key.span == value.span
}

/// Advances the static array auto-key cursor after an implicit generated key.
fn advance_static_array_auto_key(next_auto_key: &mut i64, auto_key_initialized: &mut bool) {
    *next_auto_key = next_auto_key.saturating_add(1);
    *auto_key_initialized = true;
}

/// Updates the static array auto-key cursor from an explicit integer-like key.
fn update_static_array_auto_key_from_explicit_key(
    key: &Expr,
    next_auto_key: &mut i64,
    auto_key_initialized: &mut bool,
) {
    if let Some(value) = static_integer_array_key_value(key) {
        let candidate = value.saturating_add(1);
        if !*auto_key_initialized || candidate > *next_auto_key {
            *next_auto_key = candidate;
        }
        *auto_key_initialized = true;
    }
}

/// Returns the integer value for static keys that affect PHP's next auto key.
fn static_integer_array_key_value(key: &Expr) -> Option<i64> {
    match &key.kind {
        ExprKind::IntLiteral(value) => Some(*value),
        ExprKind::BoolLiteral(value) => Some(i64::from(*value)),
        ExprKind::FloatLiteral(_) => static_integral_float_array_key_value(key),
        ExprKind::StringLiteral(value) if is_php_integer_array_key(value) => value.parse().ok(),
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(value) => value.checked_neg(),
            ExprKind::FloatLiteral(_) => static_integral_float_array_key_value(key),
            _ => None,
        },
        _ => None,
    }
}

/// Returns the integer key for a float literal that PHP casts without a precision warning.
fn static_integral_float_array_key_value(key: &Expr) -> Option<i64> {
    let value = match &key.kind {
        ExprKind::FloatLiteral(value) => *value,
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::FloatLiteral(value) => -*value,
            _ => return None,
        },
        _ => return None,
    };
    if !value.is_finite() || value.fract() != 0.0 {
        return None;
    }
    if value < i64::MIN as f64 || value >= i64::MAX as f64 {
        return None;
    }
    Some(value as i64)
}

/// Returns true when a static array value can be lowered without eval bridge state.
fn expr_is_eir_static_array_value_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    match &expr.kind {
        ExprKind::Spread(_) => false,
        ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) => {
            expr_is_eir_static_array_source_safe(expr, support, facts, scope_reads)
        }
        _ => expr_is_eir_function_safe(expr, support, facts, scope_reads),
    }
}

/// Returns true for language-construct calls that can safely lower through EIR AOT.
fn eir_construct_call_is_safe<S>(
    name: &str,
    args: &[Expr],
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    if has_named_args(args)
        || args
            .iter()
            .any(|arg| matches!(arg.kind, ExprKind::Spread(_)))
    {
        return false;
    }
    match php_symbol_key(name.trim_start_matches('\\')).as_str() {
        "isset" => {
            !args.is_empty()
                && args
                    .iter()
                    .all(|arg| eir_isset_probe_is_safe(arg, support, facts, scope_reads))
        }
        "empty" if args.len() == 1 => match &args[0].kind {
            ExprKind::Variable(name) => eir_variable_probe_is_safe(name, facts, scope_reads),
            _ => expr_is_eir_function_safe(&args[0], support, facts, scope_reads),
        },
        _ => false,
    }
}

/// Returns true when an `isset()` operand can lower without evaluating dynamic scope state.
fn eir_isset_probe_is_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    match &expr.kind {
        ExprKind::Variable(name) => eir_variable_probe_is_safe(name, facts, scope_reads),
        ExprKind::ArrayAccess { .. } => {
            expr_is_eir_function_safe(expr, support, facts, scope_reads)
        }
        _ => false,
    }
}

/// Returns true when a variable probe can use local facts or direct eval read params.
fn eir_variable_probe_is_safe(
    name: &str,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool {
    facts.is_assigned(name) || scope_reads.is_some_and(|reads| reads.contains(name))
}

/// Returns true for builtin calls that the normal EIR backend can lower at runtime.
fn eir_runtime_builtin_call_is_safe<S>(
    name: &str,
    args: &[Expr],
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    let short_name = php_symbol_key(name.trim_start_matches('\\'));
    let Some(args) = normalize_eir_runtime_builtin_args(&short_name, args) else {
        return false;
    };
    match short_name.as_str() {
        "boolval" if args.len() == 1 => {
            eir_boolval_arg_is_safe(&args[0], support, facts, scope_reads)
        }
        "array_key_exists" if args.len() == 2 => {
            eir_array_key_exists_args_are_safe(&args[0], &args[1], support, facts, scope_reads)
        }
        "count" if (1..=2).contains(&args.len()) => {
            eir_count_mode_is_default_zero(args.get(1))
                && eir_count_arg_is_safe(&args[0], support, facts, scope_reads)
        }
        "floatval" if args.len() == 1 => {
            eir_floatval_arg_is_safe(&args[0], support, facts, scope_reads)
        }
        "gettype" if args.len() == 1 => {
            eir_type_probe_arg_is_safe(&args[0], support, facts, scope_reads)
        }
        "intval" if args.len() == 1 => {
            eir_intval_arg_is_safe(&args[0], support, facts, scope_reads)
        }
        "is_array" if args.len() == 1 => {
            eir_array_type_probe_arg_is_safe(&args[0], support, facts, scope_reads)
        }
        "is_iterable" if args.len() == 1 => {
            eir_array_like_type_probe_arg_is_safe(&args[0], support, facts, scope_reads)
        }
        "is_object" if args.len() == 1 => {
            eir_object_type_probe_arg_is_safe(&args[0], support, facts, scope_reads)
        }
        "is_numeric" | "is_resource" if args.len() == 1 => {
            eir_scalar_cast_arg_is_safe(&args[0], support, facts, scope_reads)
        }
        "is_finite" | "is_infinite" | "is_nan" if args.len() == 1 => {
            eir_float_predicate_arg_is_safe(&args[0], support, facts, scope_reads)
        }
        "is_bool" | "is_double" | "is_float" | "is_int" | "is_integer" | "is_long" | "is_null"
        | "is_real" | "is_scalar" | "is_string"
            if args.len() == 1 =>
        {
            eir_type_probe_arg_is_safe(&args[0], support, facts, scope_reads)
        }
        "strval" if args.len() == 1 => {
            eir_strval_arg_is_safe(&args[0], support, facts, scope_reads)
        }
        "strlen" if args.len() == 1 => {
            eir_strlen_arg_is_safe(&args[0], support, facts, scope_reads)
        }
        _ => false,
    }
}

/// Normalizes EIR-safe builtin call arguments for eval AOT gating.
///
/// Static spread arrays are expanded through the shared call planner; dynamic
/// spreads that remain after planning stay on the eval bridge fallback.
fn normalize_eir_runtime_builtin_args(short_name: &str, args: &[Expr]) -> Option<Vec<Expr>> {
    let has_spread = args
        .iter()
        .any(|arg| matches!(arg.kind, ExprKind::Spread(_)));
    if !has_named_args(args) && !has_spread {
        return Some(args.to_vec());
    }
    let sig = builtin_call_sig(short_name)?;
    let call_span = args.first().map(|arg| arg.span).unwrap_or_else(Span::dummy);
    let plan = plan_call_args(&sig, args, call_span, false, false).ok()?;
    if plan.has_spread_args() {
        return None;
    }
    Some(plan.normalized_args())
}

/// Returns true when `array_key_exists()` can lower through EIR without eval bridge state.
fn eir_array_key_exists_args_are_safe<S>(
    key: &Expr,
    array: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    if !eir_array_key_exists_static_key_is_safe(key) {
        return false;
    }
    match &array.kind {
        ExprKind::Variable(name) if scope_reads.is_some_and(|reads| reads.contains(name)) => true,
        ExprKind::ArrayLiteralAssoc(_) => {
            expr_is_eir_static_array_source_safe(array, support, facts, scope_reads)
        }
        ExprKind::ArrayLiteral(_) => {
            !eir_array_key_exists_static_key_needs_assoc_array(key)
                && expr_is_eir_static_array_source_safe(array, support, facts, scope_reads)
        }
        ExprKind::Variable(name) => {
            !eir_array_key_exists_static_key_needs_assoc_array(key) && facts.is_array_local(name)
        }
        _ => false,
    }
}

/// Returns true when the key type has target-aware lowering for mixed array probes.
fn eir_array_key_exists_static_key_is_safe(key: &Expr) -> bool {
    match &key.kind {
        ExprKind::IntLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::Null => true,
        ExprKind::FloatLiteral(_) => static_integral_float_array_key_value(key).is_some(),
        ExprKind::Negate(inner) => {
            matches!(inner.kind, ExprKind::IntLiteral(_))
                || static_integral_float_array_key_value(key).is_some()
        }
        _ => false,
    }
}

/// Returns true when the static key only has safe mixed-array semantics for hashes.
///
/// String keys can now probe indexed arrays too: numeric strings normalize to an
/// integer bounds check and non-integer strings return false on indexed arrays.
fn eir_array_key_exists_static_key_needs_assoc_array(key: &Expr) -> bool {
    matches!(key.kind, ExprKind::Null)
}

/// Returns true when `count()` uses PHP's default non-recursive mode.
fn eir_count_mode_is_default_zero(mode: Option<&Expr>) -> bool {
    match mode {
        None => true,
        Some(expr) => matches!(expr.kind, ExprKind::IntLiteral(0)),
    }
}

/// Returns true when a value can reach `count()` as a concrete EIR array.
fn eir_count_arg_is_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    match &expr.kind {
        ExprKind::Variable(name) if scope_reads.is_some_and(|reads| reads.contains(name)) => true,
        _ => expr_is_eir_static_array_source_safe(expr, support, facts, scope_reads),
    }
}

/// Returns true when a value can reach `boolval()` through an EIR-supported scalar path.
fn eir_boolval_arg_is_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    eir_scalar_cast_arg_is_safe(expr, support, facts, scope_reads)
}

/// Returns true when a value can reach `floatval()` through an EIR-supported scalar path.
fn eir_floatval_arg_is_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    eir_scalar_cast_arg_is_safe(expr, support, facts, scope_reads)
}

/// Returns true when a value can reach `intval()` through an EIR-supported scalar path.
fn eir_intval_arg_is_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    eir_scalar_cast_arg_is_safe(expr, support, facts, scope_reads)
}

/// Returns true when a value can reach `gettype()`/`is_*()` through EIR-safe probes.
fn eir_type_probe_arg_is_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    eir_scalar_cast_arg_is_safe(expr, support, facts, scope_reads)
}

/// Returns true when a value can reach array-like type probes through safe EIR paths.
fn eir_array_type_probe_arg_is_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    expr_is_eir_static_array_source_safe(expr, support, facts, scope_reads)
        || eir_scalar_cast_arg_is_safe(expr, support, facts, scope_reads)
}

/// Returns true when a value can reach `is_iterable()` through currently safe EIR paths.
fn eir_array_like_type_probe_arg_is_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    expr_is_eir_static_array_source_safe(expr, support, facts, scope_reads)
        || eir_scalar_cast_arg_is_safe(expr, support, facts, scope_reads)
}

/// Returns true when a value can reach `is_object()` through currently safe EIR paths.
fn eir_object_type_probe_arg_is_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    eir_scalar_cast_arg_is_safe(expr, support, facts, scope_reads)
        || expr_is_eir_static_array_source_safe(expr, support, facts, scope_reads)
}

/// Returns true when a value can reach IEEE float predicates without PHP coercion surprises.
fn eir_float_predicate_arg_is_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    match &expr.kind {
        ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_) | ExprKind::BoolLiteral(_) => true,
        ExprKind::Variable(name) => {
            scope_reads.is_some_and(|reads| reads.contains(name))
                || facts.is_int_local(name)
                || facts.is_float_local(name)
        }
        ExprKind::Negate(inner) | ExprKind::ErrorSuppress(inner) => {
            eir_float_predicate_arg_is_safe(inner, support, facts, scope_reads)
        }
        ExprKind::Cast { target, expr }
            if matches!(target, CastType::Int | CastType::Float | CastType::Bool) =>
        {
            expr_is_eir_function_safe(expr, support, facts, scope_reads)
        }
        _ => false,
    }
}

/// Returns true when a value can reach `strval()` through an EIR-supported scalar path.
fn eir_strval_arg_is_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    eir_scalar_cast_arg_is_safe(expr, support, facts, scope_reads)
}

/// Returns true when an expression is scalar-like enough for EIR cast builtins.
fn eir_scalar_cast_arg_is_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    match &expr.kind {
        ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::Null => true,
        ExprKind::Variable(name) => {
            scope_reads.is_some_and(|reads| reads.contains(name))
                || (facts.is_assigned(name) && !facts.is_array_local(name))
        }
        ExprKind::Negate(inner) | ExprKind::ErrorSuppress(inner) => {
            eir_scalar_cast_arg_is_safe(inner, support, facts, scope_reads)
        }
        ExprKind::Cast { target, expr }
            if matches!(
                target,
                CastType::Int | CastType::Float | CastType::String | CastType::Bool
            ) =>
        {
            expr_is_eir_function_safe(expr, support, facts, scope_reads)
        }
        ExprKind::ArrayAccess { .. } => {
            expr_is_eir_function_safe(expr, support, facts, scope_reads)
        }
        ExprKind::FunctionCall { name, args } => {
            eir_call_user_func_call_is_safe(name.as_str(), args, support, facts, scope_reads)
                || eir_runtime_builtin_call_is_safe(
                    name.as_str(),
                    args,
                    support,
                    facts,
                    scope_reads,
                )
                || fold_static_builtin_int_call(name.as_str().trim_start_matches('\\'), args)
                    .is_some()
                || support.function_supported(name.as_str(), args)
        }
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => support.static_method_supported(receiver, method, args),
        _ => false,
    }
}

/// Returns true when a value can reach `strlen()` as `Str` or boxed `Mixed`.
fn eir_strlen_arg_is_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    match &expr.kind {
        ExprKind::StringLiteral(_) => true,
        ExprKind::Variable(name) => {
            facts.is_assigned(name) || scope_reads.is_some_and(|reads| reads.contains(name))
        }
        ExprKind::Cast { target, expr } if matches!(target, CastType::String) => {
            expr_is_eir_function_safe(expr, support, facts, scope_reads)
        }
        _ => false,
    }
}

/// Returns true when an expression is known to produce an integer in the EIR AOT subset.
fn expr_is_eir_int_value_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    match &expr.kind {
        ExprKind::IntLiteral(_) => true,
        ExprKind::Variable(name) => facts.is_int_local(name),
        ExprKind::Negate(inner) | ExprKind::BitNot(inner) | ExprKind::ErrorSuppress(inner) => {
            expr_is_eir_int_value_safe(inner, support, facts, scope_reads)
        }
        ExprKind::Print(inner) => expr_is_eir_function_safe(inner, support, facts, scope_reads),
        ExprKind::PreIncrement(name)
        | ExprKind::PostIncrement(name)
        | ExprKind::PreDecrement(name)
        | ExprKind::PostDecrement(name) => facts.is_int_local(name),
        ExprKind::Cast { target, expr } if matches!(target, CastType::Int) => {
            expr_is_eir_function_safe(expr, support, facts, scope_reads)
        }
        ExprKind::BinaryOp { left, op, right } => {
            let int_operands = expr_is_eir_int_value_safe(left, support, facts, scope_reads)
                && expr_is_eir_int_value_safe(right, support, facts, scope_reads);
            match op {
                BinOp::Add
                | BinOp::Sub
                | BinOp::Mul
                | BinOp::Mod
                | BinOp::BitAnd
                | BinOp::BitOr
                | BinOp::BitXor
                | BinOp::ShiftLeft
                | BinOp::ShiftRight => int_operands,
                BinOp::Spaceship => {
                    expr_is_eir_function_safe(left, support, facts, scope_reads)
                        && expr_is_eir_function_safe(right, support, facts, scope_reads)
                }
                _ => false,
            }
        }
        ExprKind::FunctionCall { name, args } => {
            fold_static_builtin_int_call(name.as_str().trim_start_matches('\\'), args).is_some()
        }
        _ => false,
    }
}

/// Returns true when an expression is known to produce a float in the EIR AOT subset.
fn expr_is_eir_float_value_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    match &expr.kind {
        ExprKind::FloatLiteral(_) => true,
        ExprKind::Variable(name) => facts.is_float_local(name),
        ExprKind::Negate(inner) | ExprKind::ErrorSuppress(inner) => {
            expr_is_eir_float_value_safe(inner, support, facts, scope_reads)
        }
        ExprKind::Cast { target, expr } if matches!(target, CastType::Float) => {
            expr_is_eir_function_safe(expr, support, facts, scope_reads)
        }
        _ => false,
    }
}

/// Rewrites foldable static builtin calls in a program to integer literals.
fn fold_static_builtin_calls_in_program(program: Program) -> Program {
    program
        .into_iter()
        .map(fold_static_builtin_calls_in_stmt)
        .collect()
}

/// Rewrites foldable static builtin calls inside one statement.
fn fold_static_builtin_calls_in_stmt(stmt: Stmt) -> Stmt {
    let kind = match stmt.kind {
        StmtKind::Echo(expr) => StmtKind::Echo(fold_static_builtin_calls_in_expr(expr)),
        StmtKind::Assign { name, value } => StmtKind::Assign {
            name,
            value: fold_static_builtin_calls_in_expr(value),
        },
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => StmtKind::If {
            condition: fold_static_builtin_calls_in_expr(condition),
            then_body: fold_static_builtin_calls_in_program(then_body),
            elseif_clauses: elseif_clauses
                .into_iter()
                .map(|(condition, body)| {
                    (
                        fold_static_builtin_calls_in_expr(condition),
                        fold_static_builtin_calls_in_program(body),
                    )
                })
                .collect(),
            else_body: else_body.map(fold_static_builtin_calls_in_program),
        },
        StmtKind::While { condition, body } => StmtKind::While {
            condition: fold_static_builtin_calls_in_expr(condition),
            body: fold_static_builtin_calls_in_program(body),
        },
        StmtKind::DoWhile { condition, body } => StmtKind::DoWhile {
            condition: fold_static_builtin_calls_in_expr(condition),
            body: fold_static_builtin_calls_in_program(body),
        },
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => StmtKind::For {
            init: init.map(|stmt| Box::new(fold_static_builtin_calls_in_stmt(*stmt))),
            condition: condition.map(fold_static_builtin_calls_in_expr),
            update: update.map(|stmt| Box::new(fold_static_builtin_calls_in_stmt(*stmt))),
            body: fold_static_builtin_calls_in_program(body),
        },
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => StmtKind::Switch {
            subject: fold_static_builtin_calls_in_expr(subject),
            cases: cases
                .into_iter()
                .map(|(conditions, body)| {
                    (
                        conditions
                            .into_iter()
                            .map(fold_static_builtin_calls_in_expr)
                            .collect(),
                        fold_static_builtin_calls_in_program(body),
                    )
                })
                .collect(),
            default: default.map(fold_static_builtin_calls_in_program),
        },
        StmtKind::Return(Some(expr)) => {
            StmtKind::Return(Some(fold_static_builtin_calls_in_expr(expr)))
        }
        StmtKind::ExprStmt(expr) => StmtKind::ExprStmt(fold_static_builtin_calls_in_expr(expr)),
        other => other,
    };
    Stmt {
        kind,
        span: stmt.span,
        attributes: stmt.attributes,
    }
}

/// Rewrites foldable static builtin calls inside one expression.
fn fold_static_builtin_calls_in_expr(expr: Expr) -> Expr {
    let span = expr.span;
    let kind = match expr.kind {
        ExprKind::Negate(inner) => {
            ExprKind::Negate(Box::new(fold_static_builtin_calls_in_expr(*inner)))
        }
        ExprKind::Not(inner) => ExprKind::Not(Box::new(fold_static_builtin_calls_in_expr(*inner))),
        ExprKind::BitNot(inner) => {
            ExprKind::BitNot(Box::new(fold_static_builtin_calls_in_expr(*inner)))
        }
        ExprKind::Print(inner) => {
            ExprKind::Print(Box::new(fold_static_builtin_calls_in_expr(*inner)))
        }
        ExprKind::BinaryOp { left, op, right } => ExprKind::BinaryOp {
            left: Box::new(fold_static_builtin_calls_in_expr(*left)),
            op,
            right: Box::new(fold_static_builtin_calls_in_expr(*right)),
        },
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => ExprKind::Ternary {
            condition: Box::new(fold_static_builtin_calls_in_expr(*condition)),
            then_expr: Box::new(fold_static_builtin_calls_in_expr(*then_expr)),
            else_expr: Box::new(fold_static_builtin_calls_in_expr(*else_expr)),
        },
        ExprKind::ShortTernary { value, default } => ExprKind::ShortTernary {
            value: Box::new(fold_static_builtin_calls_in_expr(*value)),
            default: Box::new(fold_static_builtin_calls_in_expr(*default)),
        },
        ExprKind::NullCoalesce { value, default } => ExprKind::NullCoalesce {
            value: Box::new(fold_static_builtin_calls_in_expr(*value)),
            default: Box::new(fold_static_builtin_calls_in_expr(*default)),
        },
        ExprKind::Cast { target, expr } => ExprKind::Cast {
            target,
            expr: Box::new(fold_static_builtin_calls_in_expr(*expr)),
        },
        ExprKind::Match {
            subject,
            arms,
            default,
        } => ExprKind::Match {
            subject: Box::new(fold_static_builtin_calls_in_expr(*subject)),
            arms: arms
                .into_iter()
                .map(|(conditions, result)| {
                    (
                        conditions
                            .into_iter()
                            .map(fold_static_builtin_calls_in_expr)
                            .collect(),
                        fold_static_builtin_calls_in_expr(result),
                    )
                })
                .collect(),
            default: default.map(|expr| Box::new(fold_static_builtin_calls_in_expr(*expr))),
        },
        ExprKind::ArrayLiteral(items) => ExprKind::ArrayLiteral(
            items
                .into_iter()
                .map(fold_static_builtin_calls_in_expr)
                .collect(),
        ),
        ExprKind::ArrayLiteralAssoc(pairs) => ExprKind::ArrayLiteralAssoc(
            pairs
                .into_iter()
                .map(|(key, value)| {
                    (
                        fold_static_builtin_calls_in_expr(key),
                        fold_static_builtin_calls_in_expr(value),
                    )
                })
                .collect(),
        ),
        ExprKind::FunctionCall { name, args } => {
            let folded_args = args
                .into_iter()
                .map(fold_static_builtin_calls_in_expr)
                .collect::<Vec<_>>();
            if let Some(kind) = fold_static_call_user_func_call(
                name.as_str().trim_start_matches('\\'),
                &folded_args,
            ) {
                kind
            } else if let Some(kind) =
                fold_static_builtin_call(name.as_str().trim_start_matches('\\'), &folded_args)
            {
                kind
            } else {
                ExprKind::FunctionCall {
                    name,
                    args: folded_args,
                }
            }
        }
        other => other,
    };
    Expr { kind, span }
}

/// Folds `call_user_func*()` when the callback is a pure foldable builtin.
fn fold_static_call_user_func_call(short_name: &str, args: &[Expr]) -> Option<ExprKind> {
    match php_symbol_key(short_name).as_str() {
        "call_user_func" => {
            let (callback, callback_args) = args.split_first()?;
            fold_static_callback_call(callback, callback_args)
        }
        "call_user_func_array" => {
            let [callback, arg_array] = args else {
                return None;
            };
            let callback_args = static_call_user_func_array_args(arg_array)?;
            fold_static_callback_call(callback, &callback_args)
        }
        _ => None,
    }
}

/// Folds one static string callback when it names a pure foldable builtin.
fn fold_static_callback_call(callback: &Expr, callback_args: &[Expr]) -> Option<ExprKind> {
    let ExprKind::StringLiteral(callback_name) = &callback.kind else {
        return None;
    };
    if callback_name.contains("::") {
        return None;
    }
    fold_static_builtin_call(callback_name.trim_start_matches('\\'), callback_args)
}


/// Folds pure static builtin calls whose integer result is fully known at compile time.
pub(crate) fn fold_static_builtin_int_call(short_name: &str, args: &[Expr]) -> Option<i64> {
    let ExprKind::IntLiteral(value) = fold_static_builtin_call(short_name, args)? else {
        return None;
    };
    Some(value)
}

/// Folds pure static builtin calls whose scalar result is fully known at compile time.
fn fold_static_builtin_call(short_name: &str, args: &[Expr]) -> Option<ExprKind> {
    let name = php_symbol_key(short_name);
    let normalized_args = normalize_static_builtin_args(&name, args)?;
    let args = normalized_args.as_slice();
    match name.as_str() {
        name if name == "strlen" => fold_strlen(args).map(ExprKind::IntLiteral),
        name if name == "intval" => fold_intval(args).map(ExprKind::IntLiteral),
        name if name == "floatval" => fold_floatval(args).map(ExprKind::FloatLiteral),
        name if name == "strval" => fold_strval(args).map(ExprKind::StringLiteral),
        name if name == "boolval" => fold_boolval(args).map(ExprKind::BoolLiteral),
        name if name == "is_int" || name == "is_integer" || name == "is_long" => {
            fold_type_probe(args, LiteralTypeProbe::Int).map(ExprKind::BoolLiteral)
        }
        name if name == "is_string" => {
            fold_type_probe(args, LiteralTypeProbe::String).map(ExprKind::BoolLiteral)
        }
        name if name == "is_bool" => {
            fold_type_probe(args, LiteralTypeProbe::Bool).map(ExprKind::BoolLiteral)
        }
        name if name == "is_float" || name == "is_double" || name == "is_real" => {
            fold_type_probe(args, LiteralTypeProbe::Float).map(ExprKind::BoolLiteral)
        }
        name if name == "is_null" => {
            fold_type_probe(args, LiteralTypeProbe::Null).map(ExprKind::BoolLiteral)
        }
        name if name == "is_scalar" => fold_is_scalar(args).map(ExprKind::BoolLiteral),
        name if name == "gettype" => fold_gettype(args).map(ExprKind::StringLiteral),
        name if name == "abs" => fold_abs(args).map(ExprKind::IntLiteral),
        name if name == "count" => fold_count(args).map(ExprKind::IntLiteral),
        name if name == "array_key_exists" => {
            fold_array_key_exists(args).map(ExprKind::BoolLiteral)
        }
        name if name == "floor" => fold_floor(args).map(ExprKind::FloatLiteral),
        name if name == "ceil" => fold_ceil(args).map(ExprKind::FloatLiteral),
        name if name == "sqrt" => fold_sqrt(args).map(ExprKind::FloatLiteral),
        name if name == "round" => fold_round(args).map(ExprKind::FloatLiteral),
        name if name == "ord" => fold_ord(args).map(ExprKind::IntLiteral),
        name if name == "chr" => fold_chr(args).map(ExprKind::StringLiteral),
        name if name == "min" => fold_min(args).map(ExprKind::IntLiteral),
        name if name == "max" => fold_max(args).map(ExprKind::IntLiteral),
        name if name == "strtolower" => {
            fold_ascii_case(args, AsciiCaseFold::Lower).map(ExprKind::StringLiteral)
        }
        name if name == "strtoupper" => {
            fold_ascii_case(args, AsciiCaseFold::Upper).map(ExprKind::StringLiteral)
        }
        name if name == "ucfirst" => {
            fold_ascii_first_char_case(args, FirstCharCaseFold::Upper).map(ExprKind::StringLiteral)
        }
        name if name == "lcfirst" => {
            fold_ascii_first_char_case(args, FirstCharCaseFold::Lower).map(ExprKind::StringLiteral)
        }
        name if name == "strrev" => fold_ascii_strrev(args).map(ExprKind::StringLiteral),
        name if name == "substr" => fold_ascii_substr(args).map(ExprKind::StringLiteral),
        name if name == "str_repeat" => fold_ascii_str_repeat(args).map(ExprKind::StringLiteral),
        name if name == "trim" => {
            fold_ascii_default_trim(args, TrimSide::Both).map(ExprKind::StringLiteral)
        }
        name if name == "ltrim" => {
            fold_ascii_default_trim(args, TrimSide::Left).map(ExprKind::StringLiteral)
        }
        name if name == "rtrim" || name == "chop" => {
            fold_ascii_default_trim(args, TrimSide::Right).map(ExprKind::StringLiteral)
        }
        name if name == "str_contains" => {
            fold_ascii_string_predicate(args, StringPredicate::Contains).map(ExprKind::BoolLiteral)
        }
        name if name == "str_starts_with" => {
            fold_ascii_string_predicate(args, StringPredicate::StartsWith)
                .map(ExprKind::BoolLiteral)
        }
        name if name == "str_ends_with" => {
            fold_ascii_string_predicate(args, StringPredicate::EndsWith).map(ExprKind::BoolLiteral)
        }
        _ => None,
    }
}

/// Normalizes named/static-spread builtin arguments before attempting a static fold.
fn normalize_static_builtin_args(short_name: &str, args: &[Expr]) -> Option<Vec<Expr>> {
    let sig = builtin_call_sig(short_name)?;
    let call_span = args.first().map(|arg| arg.span).unwrap_or_else(Span::dummy);
    let plan = plan_call_args(&sig, args, call_span, true, false).ok()?;
    Some(plan.normalized_args())
}

/// Folds `strlen("literal")` to an integer result.
fn fold_strlen(args: &[Expr]) -> Option<i64> {
    if args.len() != 1 {
        return None;
    }
    let ExprKind::StringLiteral(value) = &args[0].kind else {
        return None;
    };
    i64::try_from(value.len()).ok()
}

/// Folds `intval()` for literal scalar inputs whose PHP result is unambiguous here.
fn fold_intval(args: &[Expr]) -> Option<i64> {
    if args.len() != 1 {
        return None;
    }
    match &args[0].kind {
        ExprKind::IntLiteral(value) => Some(*value),
        ExprKind::BoolLiteral(value) => Some(i64::from(*value)),
        ExprKind::StringLiteral(value) => value.trim().parse::<i64>().ok(),
        _ => None,
    }
}

/// Folds `floatval()` for literal scalar inputs whose PHP result is unambiguous here.
fn fold_floatval(args: &[Expr]) -> Option<f64> {
    if args.len() != 1 {
        return None;
    }
    let value = match &args[0].kind {
        ExprKind::IntLiteral(value) => *value as f64,
        ExprKind::FloatLiteral(value) => *value,
        ExprKind::BoolLiteral(value) => f64::from(u8::from(*value)),
        ExprKind::StringLiteral(value) => value.trim().parse::<f64>().ok()?,
        ExprKind::Null => 0.0,
        _ => return None,
    };
    value.is_finite().then_some(value)
}

/// Folds `strval()` for literal scalar inputs with stable PHP string results.
fn fold_strval(args: &[Expr]) -> Option<String> {
    if args.len() != 1 {
        return None;
    }
    match &args[0].kind {
        ExprKind::IntLiteral(value) => Some(value.to_string()),
        ExprKind::BoolLiteral(true) => Some("1".to_string()),
        ExprKind::BoolLiteral(false) | ExprKind::Null => Some(String::new()),
        ExprKind::StringLiteral(value) => Some(value.clone()),
        _ => None,
    }
}

/// Folds `boolval()` for literal scalar inputs whose PHP truthiness is clear.
fn fold_boolval(args: &[Expr]) -> Option<bool> {
    if args.len() != 1 {
        return None;
    }
    match &args[0].kind {
        ExprKind::IntLiteral(value) => Some(*value != 0),
        ExprKind::BoolLiteral(value) => Some(*value),
        ExprKind::StringLiteral(value) => Some(!(value.is_empty() || value == "0")),
        ExprKind::Null => Some(false),
        _ => None,
    }
}

/// Literal scalar type checked by pure `is_*` builtin folds.
enum LiteralTypeProbe {
    Int,
    String,
    Bool,
    Float,
    Null,
}

/// Folds pure `is_*` type probes for literal scalar inputs.
fn fold_type_probe(args: &[Expr], probe: LiteralTypeProbe) -> Option<bool> {
    if args.len() != 1 {
        return None;
    }
    match &args[0].kind {
        ExprKind::IntLiteral(_) => Some(matches!(probe, LiteralTypeProbe::Int)),
        ExprKind::StringLiteral(_) => Some(matches!(probe, LiteralTypeProbe::String)),
        ExprKind::BoolLiteral(_) => Some(matches!(probe, LiteralTypeProbe::Bool)),
        ExprKind::FloatLiteral(_) => Some(matches!(probe, LiteralTypeProbe::Float)),
        ExprKind::Null => Some(matches!(probe, LiteralTypeProbe::Null)),
        _ => None,
    }
}

/// Folds `is_scalar()` for literal scalar and null inputs.
fn fold_is_scalar(args: &[Expr]) -> Option<bool> {
    if args.len() != 1 {
        return None;
    }
    match &args[0].kind {
        ExprKind::IntLiteral(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::FloatLiteral(_) => Some(true),
        ExprKind::Null => Some(false),
        _ => None,
    }
}

/// Folds `gettype()` for literal scalar and null inputs with stable PHP spellings.
fn fold_gettype(args: &[Expr]) -> Option<String> {
    if args.len() != 1 {
        return None;
    }
    let ty = match &args[0].kind {
        ExprKind::IntLiteral(_) => "integer",
        ExprKind::FloatLiteral(_) => "double",
        ExprKind::StringLiteral(_) => "string",
        ExprKind::BoolLiteral(_) => "boolean",
        ExprKind::Null => "NULL",
        _ => return None,
    };
    Some(ty.to_string())
}

/// Folds `abs()` for integer literals that stay representable as `int`.
fn fold_abs(args: &[Expr]) -> Option<i64> {
    if args.len() != 1 {
        return None;
    }
    const_int_expr(&args[0])?.checked_abs()
}

/// Folds `count()` for static array literals whose element expressions have no side effects.
fn fold_count(args: &[Expr]) -> Option<i64> {
    if args.len() != 1 {
        return None;
    }
    i64::try_from(static_array_key_ids(&args[0])?.len()).ok()
}

/// Folds `array_key_exists()` for static array literals and static scalar keys.
fn fold_array_key_exists(args: &[Expr]) -> Option<bool> {
    if args.len() != 2 {
        return None;
    }
    let key = static_array_key_fold_id(&args[0])?;
    Some(static_array_key_ids(&args[1])?.contains(&key))
}

/// Returns normalized key identifiers for a static array literal.
fn static_array_key_ids(expr: &Expr) -> Option<BTreeSet<String>> {
    match &expr.kind {
        ExprKind::ArrayLiteral(items) => {
            if !items.iter().all(static_array_value_is_fold_safe) {
                return None;
            }
            (0..items.len())
                .map(|index| Some(format!("i:{index}")))
                .collect()
        }
        ExprKind::ArrayLiteralAssoc(pairs) => {
            let mut keys = BTreeSet::new();
            for (key, value) in pairs {
                if !static_array_value_is_fold_safe(value) {
                    return None;
                }
                keys.insert(static_array_key_fold_id(key)?);
            }
            Some(keys)
        }
        _ => None,
    }
}

/// Returns true when evaluating this expression while building a static array has no side effects.
fn static_array_value_is_fold_safe(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null => true,
        ExprKind::Negate(inner) | ExprKind::Not(inner) | ExprKind::BitNot(inner) => {
            static_array_value_is_fold_safe(inner)
        }
        ExprKind::ArrayLiteral(items) => items.iter().all(static_array_value_is_fold_safe),
        ExprKind::ArrayLiteralAssoc(pairs) => pairs.iter().all(|(key, value)| {
            static_array_key_fold_id(key).is_some() && static_array_value_is_fold_safe(value)
        }),
        _ => false,
    }
}

/// Returns a normalized key identifier for static array-literal count folding.
fn static_array_key_fold_id(expr: &Expr) -> Option<String> {
    if let Some(value) = static_integer_array_key_value(expr) {
        return Some(format!("i:{value}"));
    }
    match &expr.kind {
        ExprKind::Null => Some("s:".to_string()),
        ExprKind::StringLiteral(value) => Some(format!("s:{value}")),
        _ => None,
    }
}

/// Folds `floor()` for finite numeric literals.
fn fold_floor(args: &[Expr]) -> Option<f64> {
    fold_finite_numeric_unary(args, f64::floor)
}

/// Folds `ceil()` for finite numeric literals.
fn fold_ceil(args: &[Expr]) -> Option<f64> {
    fold_finite_numeric_unary(args, f64::ceil)
}

/// Folds `sqrt()` for non-negative finite numeric literals.
fn fold_sqrt(args: &[Expr]) -> Option<f64> {
    if args.len() != 1 {
        return None;
    }
    let value = const_finite_numeric_expr(&args[0])?;
    (value >= 0.0)
        .then(|| value.sqrt())
        .filter(|sqrt| sqrt.is_finite())
}

/// Folds one-argument `round()` for finite numeric literals.
fn fold_round(args: &[Expr]) -> Option<f64> {
    fold_finite_numeric_unary(args, f64::round)
}

/// Applies a finite `f64` builtin fold to one numeric literal argument.
fn fold_finite_numeric_unary(args: &[Expr], fold: fn(f64) -> f64) -> Option<f64> {
    if args.len() != 1 {
        return None;
    }
    let value = fold(const_finite_numeric_expr(&args[0])?);
    value.is_finite().then_some(value)
}

/// Folds `ord()` for literal strings by returning the first byte value.
fn fold_ord(args: &[Expr]) -> Option<i64> {
    if args.len() != 1 {
        return None;
    }
    let ExprKind::StringLiteral(value) = &args[0].kind else {
        return None;
    };
    value.as_bytes().first().map(|byte| i64::from(*byte))
}

/// Folds `chr()` for ASCII byte values representable by the AST string type.
fn fold_chr(args: &[Expr]) -> Option<String> {
    if args.len() != 1 {
        return None;
    }
    let value = const_int_expr(&args[0])?;
    let byte = u8::try_from(value).ok()?;
    if !byte.is_ascii() {
        return None;
    }
    Some(char::from(byte).to_string())
}

/// Folds `min()` over integer literal arguments.
fn fold_min(args: &[Expr]) -> Option<i64> {
    fold_int_values(args)?.into_iter().min()
}

/// Folds `max()` over integer literal arguments.
fn fold_max(args: &[Expr]) -> Option<i64> {
    fold_int_values(args)?.into_iter().max()
}

/// Collects integer literal arguments for variadic pure numeric folds.
fn fold_int_values(args: &[Expr]) -> Option<Vec<i64>> {
    if args.is_empty() {
        return None;
    }
    args.iter().map(const_int_expr).collect()
}

/// ASCII-only case conversion supported by literal eval builtin folding.
enum AsciiCaseFold {
    Lower,
    Upper,
}

/// First-byte ASCII case conversion supported by literal eval builtin folding.
enum FirstCharCaseFold {
    Lower,
    Upper,
}

/// Side selected by default-mask ASCII trim folding.
enum TrimSide {
    Left,
    Right,
    Both,
}

/// Two-string ASCII predicates supported by literal eval builtin folding.
enum StringPredicate {
    Contains,
    StartsWith,
    EndsWith,
}

/// Folds ASCII-only `strtolower()` and `strtoupper()` literal calls.
fn fold_ascii_case(args: &[Expr], mode: AsciiCaseFold) -> Option<String> {
    if args.len() != 1 {
        return None;
    }
    let ExprKind::StringLiteral(value) = &args[0].kind else {
        return None;
    };
    if !value.is_ascii() {
        return None;
    }
    let folded = match mode {
        AsciiCaseFold::Lower => value.to_ascii_lowercase(),
        AsciiCaseFold::Upper => value.to_ascii_uppercase(),
    };
    Some(folded)
}

/// Folds ASCII-only `ucfirst()` and `lcfirst()` literal calls.
fn fold_ascii_first_char_case(args: &[Expr], mode: FirstCharCaseFold) -> Option<String> {
    if args.len() != 1 {
        return None;
    }
    let ExprKind::StringLiteral(value) = &args[0].kind else {
        return None;
    };
    if !value.is_ascii() {
        return None;
    }
    let mut bytes = value.as_bytes().to_vec();
    if let Some(first) = bytes.first_mut() {
        match mode {
            FirstCharCaseFold::Lower => first.make_ascii_lowercase(),
            FirstCharCaseFold::Upper => first.make_ascii_uppercase(),
        }
    }
    String::from_utf8(bytes).ok()
}

/// Folds ASCII-only `strrev()` literal calls with PHP byte-order behavior.
fn fold_ascii_strrev(args: &[Expr]) -> Option<String> {
    if args.len() != 1 {
        return None;
    }
    let ExprKind::StringLiteral(value) = &args[0].kind else {
        return None;
    };
    if !value.is_ascii() {
        return None;
    }
    Some(value.bytes().rev().map(char::from).collect())
}

/// Folds ASCII-only `substr()` literal calls with non-negative offset and length.
fn fold_ascii_substr(args: &[Expr]) -> Option<String> {
    if !(2..=3).contains(&args.len()) {
        return None;
    }
    let ExprKind::StringLiteral(value) = &args[0].kind else {
        return None;
    };
    if !value.is_ascii() {
        return None;
    }
    let offset = usize::try_from(const_int_expr(&args[1])?).ok()?;
    let start = offset.min(value.len());
    let end = if let Some(length_arg) = args.get(2) {
        let length = usize::try_from(const_int_expr(length_arg)?).ok()?;
        start.saturating_add(length).min(value.len())
    } else {
        value.len()
    };
    Some(value[start..end].to_string())
}

/// Folds ASCII-only `str_repeat()` literal calls with a bounded static result.
fn fold_ascii_str_repeat(args: &[Expr]) -> Option<String> {
    if args.len() != 2 {
        return None;
    }
    let ExprKind::StringLiteral(value) = &args[0].kind else {
        return None;
    };
    if !value.is_ascii() {
        return None;
    }
    let times = usize::try_from(const_int_expr(&args[1])?).ok()?;
    let bytes = value.len().checked_mul(times)?;
    if bytes > MAX_STATIC_STRING_FOLD_BYTES {
        return None;
    }
    Some(value.repeat(times))
}

/// Folds one-argument ASCII `trim()`/`ltrim()`/`rtrim()` calls using PHP's default mask.
fn fold_ascii_default_trim(args: &[Expr], side: TrimSide) -> Option<String> {
    if args.len() != 1 {
        return None;
    }
    let ExprKind::StringLiteral(value) = &args[0].kind else {
        return None;
    };
    if !value.is_ascii() {
        return None;
    }
    let trimmed = match side {
        TrimSide::Left => value.trim_start_matches(is_php_default_trim_char),
        TrimSide::Right => value.trim_end_matches(is_php_default_trim_char),
        TrimSide::Both => value.trim_matches(is_php_default_trim_char),
    };
    Some(trimmed.to_string())
}

/// Returns true for characters removed by PHP's default trim character mask.
fn is_php_default_trim_char(ch: char) -> bool {
    matches!(ch, '\0' | '\t' | '\n' | '\r' | '\x0b' | ' ')
}

/// Folds ASCII-only two-string predicate calls to their boolean result.
fn fold_ascii_string_predicate(args: &[Expr], predicate: StringPredicate) -> Option<bool> {
    if args.len() != 2 {
        return None;
    }
    let (ExprKind::StringLiteral(haystack), ExprKind::StringLiteral(needle)) =
        (&args[0].kind, &args[1].kind)
    else {
        return None;
    };
    if !haystack.is_ascii() || !needle.is_ascii() {
        return None;
    }
    Some(match predicate {
        StringPredicate::Contains => haystack.contains(needle),
        StringPredicate::StartsWith => haystack.starts_with(needle),
        StringPredicate::EndsWith => haystack.ends_with(needle),
    })
}

/// Evaluates integer-only literal expressions recognized by eval AOT analysis.
pub(crate) fn const_int_expr(expr: &Expr) -> Option<i64> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(*value),
        ExprKind::Negate(inner) => const_int_expr(inner)?.checked_neg(),
        _ => None,
    }
}

/// Evaluates finite numeric literal expressions recognized by eval AOT analysis.
fn const_finite_numeric_expr(expr: &Expr) -> Option<f64> {
    const MAX_EXACT_F64_INT: i64 = 9_007_199_254_740_992;
    let value = match &expr.kind {
        ExprKind::IntLiteral(value) if (-MAX_EXACT_F64_INT..=MAX_EXACT_F64_INT).contains(value) => {
            *value as f64
        }
        ExprKind::FloatLiteral(value) => *value,
        ExprKind::Negate(inner) => -const_finite_numeric_expr(inner)?,
        _ => return None,
    };
    value.is_finite().then_some(value)
}

/// Checks a user-function signature against the native-only eval call subset.
pub(crate) fn static_function_signature_supported(signature: &FunctionSig, args: &[Expr]) -> bool {
    if !signature.declared_return
        || signature.declared_params.iter().any(|declared| !declared)
        || signature.ref_params.len() != signature.params.len()
        || signature.variadic.is_some()
        || !static_function_return_type_supported(&signature.return_type)
    {
        return false;
    }
    let Some(args) = normalize_static_function_args(signature, args) else {
        return false;
    };
    signature.params.len() == args.len()
        && signature
            .params
            .iter()
            .zip(signature.ref_params.iter().copied())
            .zip(args.iter())
            .all(|((param, by_ref), arg)| !by_ref && static_function_arg_supported(&param.1, arg))
}

/// Normalizes user-function arguments for eval AOT eligibility checks.
///
/// Static spread arrays are expanded through the shared call planner; dynamic
/// spreads that remain after planning stay on the eval bridge fallback.
fn normalize_static_function_args(signature: &FunctionSig, args: &[Expr]) -> Option<Vec<Expr>> {
    if !crate::types::call_args::has_named_args(args)
        && !args
            .iter()
            .any(|arg| matches!(arg.kind, ExprKind::Spread(_)))
    {
        return normalize_positional_static_function_args(signature, args);
    }
    let call_span = args.first().map(|arg| arg.span).unwrap_or_else(Span::dummy);
    let plan = plan_call_args(signature, args, call_span, false, false).ok()?;
    if plan.has_spread_args() {
        return None;
    }
    Some(plan.normalized_args())
}

/// Appends scalar default values for positional static user-function calls.
fn normalize_positional_static_function_args(
    signature: &FunctionSig,
    args: &[Expr],
) -> Option<Vec<Expr>> {
    if args.len() > signature.params.len() {
        return None;
    }
    let mut normalized = args.to_vec();
    for idx in args.len()..signature.params.len() {
        let default = signature.defaults.get(idx)?.clone()?;
        normalized.push(default);
    }
    Some(normalized)
}

/// Returns true when a user function return can be boxed by eval EIR AOT.
fn static_function_return_type_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Int | PhpType::Bool | PhpType::Float | PhpType::Str
    )
}

/// Returns true when a literal argument matches the supported scalar parameter type.
fn static_function_arg_supported(param_ty: &PhpType, arg: &Expr) -> bool {
    matches!(
        (param_ty.codegen_repr(), &arg.kind),
        (PhpType::Int, ExprKind::IntLiteral(_))
            | (PhpType::Bool, ExprKind::BoolLiteral(_))
            | (PhpType::Float, ExprKind::FloatLiteral(_))
            | (PhpType::Str, ExprKind::StringLiteral(_))
    )
}
