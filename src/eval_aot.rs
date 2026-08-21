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


mod planning;
mod fallbacks;
mod scope_reads;
mod array_constraints;
mod float_constraints;
mod scope_access;
mod local_facts;
mod statement_safety;
mod expression_safety;
mod array_safety;
mod builtin_safety;
mod value_safety;
mod fold_rewrite;
mod builtin_folds;
mod ascii_folds;
mod public_utilities;

#[allow(unused_imports)]
use planning::*;
#[allow(unused_imports)]
use fallbacks::*;
#[allow(unused_imports)]
use scope_reads::*;
#[allow(unused_imports)]
use array_constraints::*;
#[allow(unused_imports)]
use float_constraints::*;
#[allow(unused_imports)]
use scope_access::*;
#[allow(unused_imports)]
use local_facts::*;
#[allow(unused_imports)]
use statement_safety::*;
#[allow(unused_imports)]
use expression_safety::*;
#[allow(unused_imports)]
use array_safety::*;
#[allow(unused_imports)]
use builtin_safety::*;
#[allow(unused_imports)]
use value_safety::*;
#[allow(unused_imports)]
use fold_rewrite::*;
#[allow(unused_imports)]
use builtin_folds::*;
#[allow(unused_imports)]
use ascii_folds::*;
#[allow(unused_imports)]
use public_utilities::*;

#[allow(unused_imports)]
pub(crate) use planning::{
    eir_function_name, eir_scope_function_name, literal_fragment_parse_error_line,
    parse_literal_fragment,
    parse_literal_fragment_with_source_path, plan_literal_fragment_with_source_path_and_static_and_method_calls,
    plan_literal_fragment_with_static_and_method_calls, plan_literal_fragment_with_static_calls,
};
pub(crate) use builtin_folds::fold_static_builtin_int_call;
pub(crate) use public_utilities::{
    const_int_expr, static_function_signature_supported,
};
