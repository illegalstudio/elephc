//! Purpose:
//! Defines backend-neutral semantic contracts shared by builtin checking, EIR
//! lowering, optimizer effects, ownership, callable dispatch, and requirements.
//!
//! Called from:
//! - `crate::builtins::spec::BuiltinSpec` for the single-source descriptor.
//! - `crate::types::checker::builtins` and `crate::ir_lower::expr` while consuming it.
//!
//! Key details:
//! - Semantic hooks only see PHP types, AST arguments, EIR values/opcodes, and source spans.
//! - Assembly contexts, physical registers, frame layout, ABI details, and raw symbols are absent.

use std::fmt;

use crate::errors::CompileError;
use crate::ir::{
    Effects, Immediate, Op, PhpTypePredicate, RuntimeCallTarget, RuntimeFnId, ValueId,
};
use crate::parser::ast::Expr;
use crate::span::Span;
use crate::types::PhpType;

pub use crate::builtins::requirements::{
    file_get_contents_requirements, file_put_contents_requirements, fopen_requirements,
    stream_filter_requirements, unlink_requirements, BuiltinRequirement,
    BuiltinRequirementInput, BuiltinRequirements, RequirementsFn,
};

/// Inputs shared by backend-neutral validation, result-type, and effect resolvers.
#[allow(dead_code)]
pub struct BuiltinSemanticInput<'a> {
    /// Canonical lower-case PHP builtin name.
    pub name: &'a str,
    /// Source-order argument expressions after common call normalization.
    pub args: &'a [Expr],
    /// Inferred PHP types in normalized source order.
    pub arg_types: &'a [PhpType],
    /// Source span of the complete call expression.
    pub span: Span,
}

/// Backend-neutral validator for argument semantics beyond signature and arity checks.
pub type ValidateFn = for<'a> fn(&BuiltinSemanticInput<'a>) -> Result<(), CompileError>;

/// Backend-neutral resolver for argument- or value-dependent return types.
pub type ResultTypeFn = for<'a> fn(&BuiltinSemanticInput<'a>) -> PhpType;

/// Backend-neutral resolver for argument- or value-dependent effect summaries.
pub type EffectsFn = for<'a> fn(&BuiltinSemanticInput<'a>) -> Effects;

/// Describes how checker validation is provided for a builtin.
#[derive(Clone, Copy)]
pub enum BuiltinValidation {
    /// Run the registry-owned checker hook for validation and result inference.
    CheckerHook {
        /// Checker hook embedded in the shared semantic descriptor.
        check: crate::builtins::spec::CheckFn,
        /// Whether the hook controls argument inference order itself.
        lazy: bool,
    },
    /// Signature/arity validation is sufficient.
    SignatureOnly,
    /// Run one backend-neutral semantic validator after inferring arguments once.
    Shared(ValidateFn),
}

/// Describes the single authoritative return-type resolver for a builtin.
#[allow(dead_code)]
#[derive(Clone, Copy)]
pub enum BuiltinResultType {
    /// Use the result recorded by the checker for this exact call site.
    Checked,
    /// Use the registry's declared `returns` type in checker and EIR.
    Declared,
    /// Resolve from normalized argument types and source constants in both consumers.
    Shared(ResultTypeFn),
}

/// Describes the single authoritative effect resolver for a builtin.
#[derive(Clone, Copy)]
pub enum BuiltinEffects {
    /// The builtin always has this precise conservative effect summary.
    Static(Effects),
    /// Resolve effects from normalized argument types and source constants.
    Shared(EffectsFn),
}

/// Describes ownership and argument-aliasing of the builtin result.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltinResultOwnership {
    /// Scalar or otherwise non-refcounted result.
    NonHeap,
    /// Fresh caller-owned result that aliases no argument.
    Fresh,
    /// Borrowed result owned by runtime or persistent storage.
    Borrowed,
    /// Non-fresh result guaranteed not to alias an argument.
    Independent,
    /// Result may alias the listed zero-based argument positions.
    Aliases(&'static [usize]),
    /// Result may conservatively alias any argument storage.
    MayAliasArguments,
}

/// Explains whether and how a builtin participates in dynamic callable dispatch.
pub type CallableSourceFn = for<'a> fn(Option<&'a PhpType>) -> bool;

/// Explains whether and how a builtin participates in dynamic callable dispatch.
#[derive(Clone, Copy, Debug)]
pub enum BuiltinCallablePolicy {
    /// Direct, first-class, and runtime-known dynamic callable paths are supported.
    Dynamic(CallableSourceFn),
    /// Dynamic dispatch eligibility is defined by the typed builtin target descriptor.
    DynamicRuntime(RuntimeFnId),
    /// Direct and first-class calls work, but runtime-selected names are unsupported.
    StaticOnly(&'static str),
}

/// Declares the backend-neutral implementation shape selected for a builtin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltinTargetStrategy {
    /// Lower directly to one existing general-purpose EIR operation.
    EirPrimitive,
    /// Lower to a graph of reusable EIR operations and control flow.
    EirGraph,
    /// Lower through a typed runtime call whose ABI is resolved by the backend.
    RuntimeCall,
}

/// Declares which supported compiler targets implement the builtin semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltinTargetSupport {
    /// The semantic strategy is valid on macOS AArch64, Linux AArch64, and Linux x86_64.
    All,
}

/// Runtime functions that a backend-neutral lowering may emit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltinRuntimeFunctions {
    /// The lowering emits no `RuntimeFnId` operation.
    None,
    /// The lowering may emit one runtime function.
    One(RuntimeFnId),
}

/// Selects source-order argument preparation needed before semantic EIR lowering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltinArgumentLowering {
    /// Use shared signature planning and ordinary source-order evaluation.
    Standard,
    /// Drop the unsupported statically-default count mode after shared planning.
    Count,
    /// Preserve date's literal-format specialization inputs.
    Date,
    /// Preserve JSON decode's source-sensitive option handling.
    JsonDecode,
    /// Lower a statically known callback descriptor before its subject.
    PregReplaceCallback,
    /// Keep positional regex operands in raw source order.
    PositionalRegex,
    /// Preserve by-reference array storage while lowering user-comparator sorts.
    UserValueSort,
}

impl BuiltinRuntimeFunctions {
    /// Returns whether this semantic descriptor may emit the requested runtime function.
    pub fn contains(self, runtime_fn: RuntimeFnId) -> bool {
        match self {
            BuiltinRuntimeFunctions::None => false,
            BuiltinRuntimeFunctions::One(candidate) => candidate == runtime_fn,
        }
    }
}

/// One value produced by backend-neutral builtin EIR lowering.
#[derive(Debug, Clone, Copy)]
pub struct LoweredBuiltinValue {
    /// SSA value produced by the lowering.
    pub value: ValueId,
}

/// Error returned when checked builtin semantics cannot be represented in EIR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltinLoweringError {
    message: String,
}

impl BuiltinLoweringError {
    /// Creates an explicit semantic-lowering error with user-facing context.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for BuiltinLoweringError {
    /// Formats the backend-neutral lowering diagnostic.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

/// Minimal EIR construction surface exposed to builtin semantic lowering.
pub trait BuiltinLoweringContext {
    /// Returns the PHP type metadata attached to an existing SSA operand.
    fn value_php_type(&self, value: ValueId) -> PhpType;

    /// Emits one typed value-producing EIR operation with explicit effects and span.
    fn emit_value(
        &mut self,
        op: Op,
        operands: Vec<ValueId>,
        immediate: Option<Immediate>,
        php_type: PhpType,
        effects: Effects,
        span: Option<Span>,
    ) -> LoweredBuiltinValue;

    /// Emits one typed runtime operation whose symbol and ABI are backend-owned.
    fn emit_runtime_call(
        &mut self,
        target: RuntimeCallTarget,
        operands: Vec<ValueId>,
        php_type: PhpType,
        effects: Effects,
        span: Option<Span>,
    ) -> LoweredBuiltinValue;
}

/// Normalized builtin call consumed by backend-neutral EIR lowering.
pub struct NormalizedBuiltinCall<'a> {
    /// Canonical lower-case PHP builtin name.
    pub name: &'a str,
    /// EIR operands after shared named/positional/spread planning.
    pub operands: &'a [ValueId],
    /// Authoritative PHP result type resolved from the registry.
    pub result_type: &'a PhpType,
    /// Source span of the complete call expression.
    pub span: Span,
}

impl NormalizedBuiltinCall<'_> {
    /// Returns one required operand or a structured lowering error.
    pub fn operand(&self, index: usize) -> Result<ValueId, BuiltinLoweringError> {
        self.operands.get(index).copied().ok_or_else(|| {
            BuiltinLoweringError::new(format!(
                "{} lowering expected operand {} but received {} operands",
                self.name,
                index,
                self.operands.len(),
            ))
        })
    }
}

/// Backend-neutral EIR lowering hook for one normalized builtin call.
pub type BuiltinLowerFn = for<'a> fn(
    &mut dyn BuiltinLoweringContext,
    &NormalizedBuiltinCall<'a>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError>;

/// Selects the active lowering path for a registry-backed builtin.
#[derive(Clone, Copy)]
pub enum BuiltinLowering {
    /// Emit backend-neutral EIR through the registered semantic hook.
    Eir(BuiltinLowerFn),
    /// Emit the reusable PHP runtime-type predicate EIR primitive.
    TypePredicate(PhpTypePredicate),
    /// Emit one typed runtime operation using the call's normalized operands.
    Runtime(RuntimeCallTarget),
}

/// Complete shared semantic descriptor referenced by `BuiltinSpec`.
#[derive(Clone, Copy)]
pub struct BuiltinSemantics {
    /// Argument validation contract.
    pub validation: BuiltinValidation,
    /// Authoritative result-type contract.
    pub result_type: BuiltinResultType,
    /// Precise conservative effect contract.
    pub effects: BuiltinEffects,
    /// Result ownership and aliasing contract.
    pub result_ownership: BuiltinResultOwnership,
    /// Runtime/link requirements visible without inspecting a PHP function name.
    pub requirements: BuiltinRequirements,
    /// Backend-neutral implementation shape used after call normalization.
    pub target_strategy: BuiltinTargetStrategy,
    /// Explicit supported-target coverage contract.
    pub target_support: BuiltinTargetSupport,
    /// Runtime functions the backend-neutral lowering may emit.
    pub runtime_functions: BuiltinRuntimeFunctions,
    /// Source-order argument preparation used before normalized semantic lowering.
    pub argument_lowering: BuiltinArgumentLowering,
    /// Callable availability contract.
    pub callable: BuiltinCallablePolicy,
    /// Backend-neutral lowering strategy.
    pub lowering: BuiltinLowering,
}

/// Embeds the registry checker contract into the shared semantic descriptor.
pub const fn with_registry_checker_contract(
    mut semantics: BuiltinSemantics,
    check: Option<crate::builtins::spec::CheckFn>,
    lazy: bool,
) -> BuiltinSemantics {
    if let Some(check) = check {
        semantics.validation = BuiltinValidation::CheckerHook { check, lazy };
        if matches!(semantics.result_type, BuiltinResultType::Declared) {
            semantics.result_type = BuiltinResultType::Checked;
        }
    }
    semantics
}

/// Overrides a target's fixed requirements with a source-dependent resolver.
pub const fn with_registry_requirement_resolver(
    mut semantics: BuiltinSemantics,
    resolver: Option<RequirementsFn>,
) -> BuiltinSemantics {
    if let Some(resolver) = resolver {
        semantics.requirements = BuiltinRequirements::Shared(resolver);
    }
    semantics
}

/// Overrides ordinary source-order argument lowering with a registry-owned strategy.
pub const fn with_argument_lowering(
    mut semantics: BuiltinSemantics,
    argument_lowering: BuiltinArgumentLowering,
) -> BuiltinSemantics {
    semantics.argument_lowering = argument_lowering;
    semantics
}

/// Builds the complete semantic descriptor for a fresh `Str -> Str` runtime transform.
pub const fn unary_string_runtime(
    target: RuntimeCallTarget,
    effects: Effects,
) -> BuiltinSemantics {
    BuiltinSemantics {
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Declared,
        effects: BuiltinEffects::Static(effects),
        result_ownership: BuiltinResultOwnership::Fresh,
        requirements: BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::RuntimeCall,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::None,
        argument_lowering: BuiltinArgumentLowering::Standard,
        callable: BuiltinCallablePolicy::Dynamic(callable_accepts_string_source),
        lowering: BuiltinLowering::Runtime(target),
    }
}

/// Builds the complete shared descriptor for one typed builtin backend target.
pub const fn runtime_fn_semantics(target: RuntimeFnId) -> BuiltinSemantics {
    BuiltinSemantics {
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Declared,
        effects: BuiltinEffects::Static(target.effects()),
        result_ownership: target.result_ownership(),
        requirements: BuiltinRequirements::Static(target.requirements()),
        target_strategy: BuiltinTargetStrategy::RuntimeCall,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::One(target),
        argument_lowering: BuiltinArgumentLowering::Standard,
        callable: if target.runtime_callable_supported() {
            BuiltinCallablePolicy::DynamicRuntime(target)
        } else {
            BuiltinCallablePolicy::StaticOnly(
                "typed backend operation has no runtime-selected wrapper contract",
            )
        },
        lowering: BuiltinLowering::Runtime(RuntimeCallTarget::Function(target)),
    }
}

/// Builds shared semantics for a PHP runtime-type predicate.
pub const fn type_predicate_semantics(predicate: PhpTypePredicate) -> BuiltinSemantics {
    BuiltinSemantics {
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Declared,
        effects: BuiltinEffects::Shared(type_predicate_effects),
        result_ownership: BuiltinResultOwnership::NonHeap,
        requirements: BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::EirPrimitive,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::None,
        argument_lowering: BuiltinArgumentLowering::Standard,
        callable: BuiltinCallablePolicy::Dynamic(callable_accepts_any_source),
        lowering: BuiltinLowering::TypePredicate(predicate),
    }
}

/// Returns the conservative effect contract of a runtime type predicate.
fn type_predicate_effects(_input: &BuiltinSemanticInput<'_>) -> Effects {
    Op::TypePredicate.default_effects()
}

/// Accepts every source representation for a fully generic callable wrapper.
fn callable_accepts_any_source(_source: Option<&PhpType>) -> bool {
    true
}

/// Accepts runtime wrapper sources that already use concrete string storage.
pub fn callable_accepts_string_source(source: Option<&PhpType>) -> bool {
    source.is_none_or(|source| source.codegen_repr() == PhpType::Str)
}

/// Accepts the dynamic string-like sources supported by shared `strlen` validation.
pub fn callable_accepts_strlen_source(source: Option<&PhpType>) -> bool {
    source.is_none_or(|source| {
        matches!(
            source.codegen_repr(),
            PhpType::Mixed | PhpType::Str | PhpType::Union(_)
        )
    })
}

/// Lowers one registry builtin through its complete backend-neutral semantic descriptor.
pub fn lower_registry_call(
    ctx: &mut dyn BuiltinLoweringContext,
    def: &crate::builtins::registry::BuiltinDef,
    operands: &[ValueId],
    result_type: &PhpType,
    span: Span,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    let arg_types = operands
        .iter()
        .map(|operand| ctx.value_php_type(*operand))
        .collect::<Vec<_>>();
    let semantic_input = BuiltinSemanticInput {
        name: def.name,
        args: &[],
        arg_types: &arg_types,
        span,
    };
    let effects = match def.spec.semantics.effects {
        BuiltinEffects::Static(effects) => effects,
        BuiltinEffects::Shared(resolve) => resolve(&semantic_input),
    };
    let resolved_result_type = match def.spec.semantics.result_type {
        BuiltinResultType::Checked => result_type.clone(),
        BuiltinResultType::Declared => def.return_type.clone(),
        BuiltinResultType::Shared(resolve) => resolve(&semantic_input),
    };
    let normalized = NormalizedBuiltinCall {
        name: def.name,
        operands,
        result_type: &resolved_result_type,
        span,
    };
    match def.spec.semantics.lowering {
        BuiltinLowering::Eir(lower) => lower(ctx, &normalized),
        BuiltinLowering::TypePredicate(predicate) => Ok(ctx.emit_value(
            Op::TypePredicate,
            vec![normalized.operand(0)?],
            Some(Immediate::TypePredicate(predicate)),
            resolved_result_type.clone(),
            Op::TypePredicate.default_effects(),
            Some(span),
        )),
        BuiltinLowering::Runtime(target) => Ok(ctx.emit_runtime_call(
            target,
            operands.to_vec(),
            resolved_result_type,
            effects,
            Some(span),
        )),
    }
}

/// Returns neutral semantics for registry plumbing probes compiled only in tests.
#[cfg(test)]
pub(crate) const fn test_probe_semantics() -> BuiltinSemantics {
    BuiltinSemantics {
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Declared,
        effects: BuiltinEffects::Static(Effects::empty()),
        result_ownership: BuiltinResultOwnership::MayAliasArguments,
        requirements: BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::EirGraph,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::None,
        argument_lowering: BuiltinArgumentLowering::Standard,
        callable: BuiltinCallablePolicy::StaticOnly("test-only registry probe"),
        lowering: BuiltinLowering::Eir(lower_test_probe),
    }
}

/// Returns test-only registry probe semantics with the requested ownership contract.
#[cfg(test)]
pub(crate) const fn test_probe_semantics_with_ownership(
    result_ownership: BuiltinResultOwnership,
) -> BuiltinSemantics {
    let mut semantics = test_probe_semantics();
    semantics.result_ownership = result_ownership;
    semantics
}

/// Returns test-only registry semantics that conservatively model by-reference mutation.
#[cfg(test)]
pub(crate) const fn test_probe_by_ref_semantics() -> BuiltinSemantics {
    let mut semantics = test_probe_semantics();
    semantics.effects = BuiltinEffects::Static(Effects::WRITES_HEAP);
    semantics
}

/// Rejects accidental lowering of a registry plumbing probe.
#[cfg(test)]
fn lower_test_probe(
    _ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    Err(BuiltinLoweringError::new(format!(
        "test-only builtin {} cannot be lowered",
        call.name,
    )))
}
