//! Purpose:
//! Provides the optimizer entry points used by the compile pipeline.
//! Coordinates constant folding, propagation, control normalization, pruning, effect modeling, and DCE.
//!
//! Called from:
//! - `crate::pipeline::compile()`
//!
//! Key details:
//! - Passes must preserve PHP-visible side effects and run after magic constants and type checking have produced canonical AST metadata.

use crate::names::{php_symbol_key, Name};
use crate::parser::ast::{
    BinOp, CallableTarget, CastType, ClassMethod, ClassProperty, EnumCaseDecl, Expr, ExprKind,
    InstanceOfTarget, Program, Stmt, StmtKind, TypeExpr,
};
use crate::termination::{block_terminal_effect, stmt_terminal_effect, TerminalEffect};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

mod control;
mod effect_analysis;
mod effects;
mod exception_flow;
mod fold;
mod propagate;
pub mod reachability;

use control::*;
use effect_analysis::{
    collect_instance_dispatch_metadata, compute_program_callable_effects, method_effect_key,
};
use effects::*;
use exception_flow::{with_exception_flow_analysis, ExceptionFlowAnalysis};
use fold::*;
use propagate::*;

pub use reachability::prune_unreachable_declarations;

#[cfg(test)]
mod tests;

thread_local! {
    static ACTIVE_FUNCTION_EFFECTS: RefCell<Option<Rc<HashMap<String, Effect>>>> = const { RefCell::new(None) };
    static ACTIVE_STATIC_METHOD_EFFECTS: RefCell<Option<Rc<HashMap<String, Effect>>>> = const { RefCell::new(None) };
    static ACTIVE_INSTANCE_METHOD_EFFECTS: RefCell<Option<Rc<HashMap<String, Effect>>>> = const { RefCell::new(None) };
    static ACTIVE_INSTANCE_DISPATCH_METADATA: RefCell<Option<Rc<InstanceDispatchMetadata>>> = const { RefCell::new(None) };
    static ACTIVE_CLASS_EFFECT_CONTEXT: RefCell<Option<ClassEffectContext>> = const { RefCell::new(None) };
    static ACTIVE_CALLABLE_ALIAS_EFFECTS: RefCell<Option<HashMap<String, Effect>>> = const { RefCell::new(None) };
}

/// Borrows the active function-effect summary without cloning its whole map.
pub(in crate::optimize) fn with_active_function_effects<R>(
    f: impl FnOnce(Option<&HashMap<String, Effect>>) -> R,
) -> R {
    ACTIVE_FUNCTION_EFFECTS.with(|slot| f(slot.borrow().as_deref()))
}

/// Borrows the active static-method summary without cloning its whole map.
pub(in crate::optimize) fn with_active_static_method_effects<R>(
    f: impl FnOnce(Option<&HashMap<String, Effect>>) -> R,
) -> R {
    ACTIVE_STATIC_METHOD_EFFECTS.with(|slot| f(slot.borrow().as_deref()))
}

/// Borrows the active instance-method summary without cloning its whole map.
pub(in crate::optimize) fn with_active_instance_method_effects<R>(
    f: impl FnOnce(Option<&HashMap<String, Effect>>) -> R,
) -> R {
    ACTIVE_INSTANCE_METHOD_EFFECTS.with(|slot| f(slot.borrow().as_deref()))
}

/// Borrows the active instance-dispatch metadata without cloning it.
pub(in crate::optimize) fn with_active_instance_dispatch_metadata<R>(
    f: impl FnOnce(Option<&InstanceDispatchMetadata>) -> R,
) -> R {
    ACTIVE_INSTANCE_DISPATCH_METADATA.with(|slot| f(slot.borrow().as_deref()))
}

/// Folds constant expressions to their compile-time values.
///
/// Also gives a CLI program the superglobals PHP's CLI SAPI would already have
/// created. That is not folding, and it rides here for a reason spelled out in
/// `superglobals::COMPILING_FOR_WEB`: this function is the last phase every one of
/// the thirteen hand-rolled pipelines calls before the checker, so a semantic pass
/// placed here cannot be forgotten by one of them. It runs FIRST so the literal it
/// prepends folds like any other.
pub fn fold_constants(program: Program) -> Program {
    let program = crate::superglobals::seed_cli_populated_superglobals(program);
    program.into_iter().map(fold_stmt).collect()
}

/// Propagates scalar constants across statements and control flow.
#[allow(dead_code)] // public test/support API; the compiler binary uses PostTypecheckOptimizer directly.
pub fn propagate_constants(program: Program) -> Program {
    PostTypecheckOptimizer::new(&program).propagate(program)
}

/// Normalizes control flow structures (ifs, switches, try/catch) for easier optimization.
#[allow(dead_code)] // public test/support API; the compiler binary uses PostTypecheckOptimizer directly.
pub fn normalize_control_flow(program: Program) -> Program {
    PostTypecheckOptimizer::new(&program).normalize(program)
}

/// Prunes branches with constant conditions that cannot be reached.
#[allow(dead_code)] // public test/support API; the compiler binary uses PostTypecheckOptimizer directly.
pub fn prune_constant_control_flow(program: Program) -> Program {
    PostTypecheckOptimizer::new(&program).prune(program)
}

/// A fact the propagation environment records for a local variable.
#[derive(Debug, Clone, PartialEq)]
enum PropagatedValue {
    /// An immutable scalar constant, substitutable at variable reads.
    Scalar(ScalarValue),
    /// A qualifying array literal (all keys and values scalar literals, size
    /// capped). Never substituted at plain variable reads — only consumed by
    /// array-access folding. Sound to copy on `$b = $a` because PHP array
    /// assignment has value semantics (COW).
    ArrayLit(Expr),
}

impl PropagatedValue {
    /// Returns the scalar payload, if this fact is a scalar constant.
    fn as_scalar(&self) -> Option<&ScalarValue> {
        match self {
            PropagatedValue::Scalar(value) => Some(value),
            PropagatedValue::ArrayLit(_) => None,
        }
    }

    /// Returns whether two facts denote the *same constant*, i.e. whether merging control-flow
    /// paths that carry them can substitute either one without changing program output.
    ///
    /// Stricter than `PartialEq` for floats: `0.0` and `-0.0` compare equal under IEEE but
    /// `echo` prints `0` and `-0`, so a merge that unified them would change the program.
    fn same_constant(&self, other: &Self) -> bool {
        match (self, other) {
            (PropagatedValue::Scalar(left), PropagatedValue::Scalar(right)) => {
                left.same_constant(right)
            }
            (PropagatedValue::ArrayLit(left), PropagatedValue::ArrayLit(right)) => {
                same_array_literal_fact(left, right)
            }
            _ => false,
        }
    }
}

/// Returns whether two array-literal facts hold identical constants.
///
/// `assigned_array_fact` only produces literals whose keys and values are scalar literals, so
/// the comparison walks them through `ScalarValue::same_constant` and keeps signed zeros apart.
/// Anything that is not one of those two literal shapes falls back to structural equality.
fn same_array_literal_fact(left: &Expr, right: &Expr) -> bool {
    /// Compares two scalar-literal expressions by constant identity.
    fn same_scalar(left: &Expr, right: &Expr) -> bool {
        match (scalar_value(left), scalar_value(right)) {
            (Some(left), Some(right)) => left.same_constant(&right),
            _ => left == right,
        }
    }

    match (&left.kind, &right.kind) {
        (ExprKind::ArrayLiteral(left), ExprKind::ArrayLiteral(right)) => {
            left.len() == right.len()
                && left.iter().zip(right).all(|(left, right)| same_scalar(left, right))
        }
        (ExprKind::ArrayLiteralAssoc(left), ExprKind::ArrayLiteralAssoc(right)) => {
            left.len() == right.len()
                && left.iter().zip(right).all(|((left_key, left_value), (right_key, right_value))| {
                    same_scalar(left_key, right_key) && same_scalar(left_value, right_value)
                })
        }
        _ => left == right,
    }
}

/// Maps local names to propagated facts during constant propagation.
type ConstantEnv = HashMap<String, PropagatedValue>;

/// Owns one conservative whole-program callable-effect analysis for all
/// post-typecheck AST passes. Reusing the summary is sound because these
/// passes only remove or simplify behavior, so the pre-pass effect summary is
/// an over-approximation for every later pass.
pub struct PostTypecheckOptimizer {
    callable_effects: CallableEffectAnalysis,
    by_ref_signatures: ByRefSignatures,
    exception_flow: Rc<ExceptionFlowAnalysis>,
}

impl PostTypecheckOptimizer {
    /// Builds the reusable analysis from the program entering post-typecheck optimization.
    pub fn new(program: &Program) -> Self {
        Self::new_with_optional_type_metadata(program, None)
    }

    /// Builds reusable analyses with the checker's canonical class and interface hierarchy.
    pub fn new_with_type_metadata(
        program: &Program,
        classes: &HashMap<String, crate::types::ClassInfo>,
        interfaces: &HashMap<String, crate::types::InterfaceInfo>,
    ) -> Self {
        Self::new_with_optional_type_metadata(program, Some((classes, interfaces)))
    }

    /// Builds callable and exception analyses, optionally using authoritative checker metadata.
    fn new_with_optional_type_metadata(
        program: &Program,
        type_metadata: Option<(
            &HashMap<String, crate::types::ClassInfo>,
            &HashMap<String, crate::types::InterfaceInfo>,
        )>,
    ) -> Self {
        let callable_effects = CallableEffectAnalysis::from_program(program);
        let exception_flow = with_callable_effect_analysis(&callable_effects, || {
            Rc::new(ExceptionFlowAnalysis::from_program(program, type_metadata))
        });
        Self {
            callable_effects,
            by_ref_signatures: collect_by_ref_signatures(program),
            exception_flow,
        }
    }

    /// Propagates constants while using the shared callable-effect summary.
    pub fn propagate(&self, program: Program) -> Program {
        reset_reference_volatile();
        // Request superglobals are writable from any scope under `--web`, so they
        // can never carry propagated facts.
        for name in crate::superglobals::SUPERGLOBALS {
            mark_reference_volatile(name);
        }
        // Install the callable effect summaries and by-ref signatures so calls to
        // known-pure user callables stop clearing the environment. Substitution
        // into by-ref argument positions is masked by `propagate_args`, which
        // keeps those arguments lvalues.
        with_callable_effect_analysis(&self.callable_effects, || {
            with_by_ref_signatures(self.by_ref_signatures.clone(), || {
                propagate_block(program, HashMap::new()).0
            })
        })
    }

    /// Normalizes control flow using the shared callable-effect summary.
    pub fn normalize(&self, program: Program) -> Program {
        with_callable_effect_analysis(&self.callable_effects, || prune_block(program))
    }

    /// Prunes constant control-flow branches using the shared callable-effect summary.
    pub fn prune(&self, program: Program) -> Program {
        with_callable_effect_analysis(&self.callable_effects, || prune_block(program))
    }

    /// Eliminates dead code using the shared callable-effect summary.
    pub fn eliminate_dead_code(&self, program: Program) -> Program {
        with_callable_effect_analysis(&self.callable_effects, || {
            with_exception_flow_analysis(&self.exception_flow, || {
                with_by_ref_signatures(self.by_ref_signatures.clone(), || dce_block(program))
            })
        })
    }
}

/// Eliminates dead code for this module.
#[allow(dead_code)] // public test/support API; the compiler binary uses PostTypecheckOptimizer directly.
pub fn eliminate_dead_code(program: Program) -> Program {
    PostTypecheckOptimizer::new(&program).eliminate_dead_code(program)
}

/// Returns true when the named builtin can invoke user code through a callback
/// argument. Such builtins inherit the callback's powers: they can write globals
/// and mutate any argument reachable through the callback's by-ref parameters.
fn builtin_invokes_callbacks(name: &str) -> bool {
    crate::builtins::registry::lookup(name).is_some_and(|definition| {
        !crate::builtins::registry::callback_parameter_indices(definition).is_empty()
    })
}

/// Effect describes whether a callable or expression has observable runtime behavior.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Effect {
    has_side_effects: bool,
    may_throw: bool,
    /// True when the callable/expression can write PHP global storage
    /// (`global`-bound variables). Consumed by constant propagation to decide
    /// whether a call can rewrite top-level locals; deliberately excluded from
    /// `is_observable` so DCE and pruning decisions are unchanged.
    writes_globals: bool,
}

impl Effect {
    /// Pure effect: no side effects and cannot throw.
    const PURE: Self = Self {
        has_side_effects: false,
        may_throw: false,
        writes_globals: false,
    };

    /// Converts shared EIR effect metadata into the AST optimizer's coarse safety model.
    fn from_eir(effects: crate::ir::Effects) -> Self {
        let mut effect = Self {
            has_side_effects: effects.intersects(
                crate::ir::Effects::WRITES_LOCAL
                    | crate::ir::Effects::WRITES_HEAP
                    | crate::ir::Effects::WRITES_GLOBAL
                    | crate::ir::Effects::WRITES_FS
                    | crate::ir::Effects::WRITES_PROCESS
                    | crate::ir::Effects::OUTPUT
                    | crate::ir::Effects::REFCOUNT_OP,
            ),
            may_throw: effects.contains(crate::ir::Effects::MAY_THROW),
            writes_globals: effects.contains(crate::ir::Effects::WRITES_GLOBAL),
        };
        if effects.intersects(
            crate::ir::Effects::READS_FS
                | crate::ir::Effects::READS_PROCESS
                | crate::ir::Effects::MAY_FATAL
                | crate::ir::Effects::MAY_WARN
                | crate::ir::Effects::MAY_DEOPT,
        ) {
            effect.has_side_effects = true;
        }
        effect
    }

    /// Marks this effect as having side effects.
    fn with_side_effects(mut self) -> Self {
        self.has_side_effects = true;
        self
    }

    /// Marks this effect as possibly throwing.
    fn with_may_throw(mut self) -> Self {
        self.may_throw = true;
        self
    }

    /// Marks this effect as possibly writing PHP global storage.
    fn with_writes_globals(mut self) -> Self {
        self.writes_globals = true;
        self
    }

    /// Combines two effects. The result is observable if either operand is observable.
    fn combine(self, other: Self) -> Self {
        Self {
            has_side_effects: self.has_side_effects || other.has_side_effects,
            may_throw: self.may_throw || other.may_throw,
            writes_globals: self.writes_globals || other.writes_globals,
        }
    }

    /// Returns true if this effect has side effects or may throw.
    fn is_observable(self) -> bool {
        self.has_side_effects || self.may_throw
    }
}

/// Carries lexical class resolution context for method and property effect analysis.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ClassEffectContext {
    class_name: String,
    parent_name: Option<String>,
}

/// Class-level facts needed to resolve closed-world instance dispatch and property reads.
#[derive(Clone, Debug, Default)]
struct InstanceClassMetadata {
    parent_name: Option<String>,
    is_abstract: bool,
    is_final: bool,
    has_trait_uses: bool,
    private_methods: HashSet<String>,
    final_methods: HashSet<String>,
    method_visibilities: HashMap<String, crate::parser::ast::Visibility>,
    properties: HashMap<String, PropertyReadMetadata>,
    has_magic_get: bool,
}

/// Declared property facts that determine whether a direct read can throw or invoke user code.
#[derive(Clone, Debug)]
struct PropertyReadMetadata {
    declaring_class: String,
    visibility: crate::parser::ast::Visibility,
    typed: bool,
    hooked: bool,
}

/// Closed-world class hierarchy and member facts installed during effect analysis.
#[derive(Clone, Debug, Default)]
struct InstanceDispatchMetadata {
    classes: HashMap<String, InstanceClassMetadata>,
    /// True when AST-level `eval()` can add subclasses beyond the declared hierarchy.
    has_dynamic_class_barrier: bool,
}

/// Holds the body and never-return metadata for a function during effect analysis.
#[derive(Clone, Debug)]
struct FunctionEffectBody<'a> {
    body: &'a [Stmt],
    declared_never: bool,
}

/// Holds the body, class context, and never-return metadata for a static method during effect analysis.
#[derive(Clone, Debug)]
struct StaticMethodBody<'a> {
    context: ClassEffectContext,
    body: &'a [Stmt],
    declared_never: bool,
}

/// Holds callable summaries and dispatch metadata shared by optimizer passes.
#[derive(Clone)]
struct CallableEffectAnalysis {
    function_effects: Rc<HashMap<String, Effect>>,
    static_method_effects: Rc<HashMap<String, Effect>>,
    instance_method_effects: Rc<HashMap<String, Effect>>,
    instance_dispatch_metadata: Rc<InstanceDispatchMetadata>,
}

impl CallableEffectAnalysis {
    /// Computes the conservative callable summaries once for a post-typecheck program.
    fn from_program(program: &[Stmt]) -> Self {
        let (function_effects, static_method_effects, instance_method_effects) =
            compute_program_callable_effects(program);
        Self {
            function_effects,
            static_method_effects,
            instance_method_effects,
            instance_dispatch_metadata: Rc::new(collect_instance_dispatch_metadata(program)),
        }
    }
}

/// Installs function, static method, instance method, and class-dispatch summaries for the
/// closure's duration, then restores the previous state.
fn with_callable_effect_analysis<R>(
    analysis: &CallableEffectAnalysis,
    f: impl FnOnce() -> R,
) -> R {
    ACTIVE_FUNCTION_EFFECTS.with(|function_slot| {
        ACTIVE_STATIC_METHOD_EFFECTS.with(|static_slot| {
            ACTIVE_INSTANCE_METHOD_EFFECTS.with(|instance_slot| {
                ACTIVE_INSTANCE_DISPATCH_METADATA.with(|metadata_slot| {
                    let previous_functions =
                        function_slot.replace(Some(Rc::clone(&analysis.function_effects)));
                    let previous_static_methods =
                        static_slot.replace(Some(Rc::clone(&analysis.static_method_effects)));
                    let previous_instance_methods =
                        instance_slot.replace(Some(Rc::clone(&analysis.instance_method_effects)));
                    let previous_metadata = metadata_slot
                        .replace(Some(Rc::clone(&analysis.instance_dispatch_metadata)));
                    let result = f();
                    metadata_slot.replace(previous_metadata);
                    instance_slot.replace(previous_instance_methods);
                    static_slot.replace(previous_static_methods);
                    function_slot.replace(previous_functions);
                    result
                })
            })
        })
    })
}

/// Installs one-off callable summaries for focused optimizer tests and helpers.
#[cfg(test)]
fn with_callable_effects<R>(
    function_effects: HashMap<String, Effect>,
    static_method_effects: HashMap<String, Effect>,
    instance_method_effects: HashMap<String, Effect>,
    instance_dispatch_metadata: InstanceDispatchMetadata,
    f: impl FnOnce() -> R,
) -> R {
    let analysis = CallableEffectAnalysis {
        function_effects: Rc::new(function_effects),
        static_method_effects: Rc::new(static_method_effects),
        instance_method_effects: Rc::new(instance_method_effects),
        instance_dispatch_metadata: Rc::new(instance_dispatch_metadata),
    };
    with_callable_effect_analysis(&analysis, f)
}

/// Installs a class effect context for instance dispatch analysis, then restores it.
fn with_class_effect_context<R>(context: Option<ClassEffectContext>, f: impl FnOnce() -> R) -> R {
    ACTIVE_CLASS_EFFECT_CONTEXT.with(|slot| {
        let previous = slot.replace(context);
        let result = f();
        slot.replace(previous);
        result
    })
}

/// Installs callable alias effects for the closure's duration, then restores the previous map.
fn with_callable_alias_effects<R>(
    alias_effects: HashMap<String, Effect>,
    f: impl FnOnce() -> R,
) -> R {
    ACTIVE_CALLABLE_ALIAS_EFFECTS.with(|slot| {
        let previous = slot.replace(Some(alias_effects));
        let result = f();
        slot.replace(previous);
        result
    })
}

/// Returns the currently active callable alias effect map, or an empty map if none is set.
fn current_callable_alias_effects() -> HashMap<String, Effect> {
    ACTIVE_CALLABLE_ALIAS_EFFECTS.with(|slot| slot.borrow().clone().unwrap_or_default())
}
