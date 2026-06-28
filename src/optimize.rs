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

mod control;
mod effects;
mod fold;
mod propagate;

use control::*;
use effects::*;
use fold::*;
use propagate::*;

#[cfg(test)]
mod tests;

thread_local! {
    static ACTIVE_FUNCTION_EFFECTS: RefCell<Option<HashMap<String, Effect>>> = const { RefCell::new(None) };
    static ACTIVE_STATIC_METHOD_EFFECTS: RefCell<Option<HashMap<String, Effect>>> = const { RefCell::new(None) };
    static ACTIVE_PRIVATE_INSTANCE_METHOD_EFFECTS: RefCell<Option<HashMap<String, Effect>>> = const { RefCell::new(None) };
    static ACTIVE_CLASS_EFFECT_CONTEXT: RefCell<Option<ClassEffectContext>> = const { RefCell::new(None) };
    static ACTIVE_CALLABLE_ALIAS_EFFECTS: RefCell<Option<HashMap<String, Effect>>> = const { RefCell::new(None) };
}

/// Folds constant expressions to their compile-time values.
pub fn fold_constants(program: Program) -> Program {
    program.into_iter().map(fold_stmt).collect()
}

/// Propagates scalar constants across statements and control flow.
pub fn propagate_constants(program: Program) -> Program {
    reset_reference_volatile();
    propagate_block(program, HashMap::new()).0
}

/// Normalizes control flow structures (ifs, switches, try/catch) for easier optimization.
pub fn normalize_control_flow(program: Program) -> Program {
    let (function_effects, static_method_effects, private_instance_method_effects) =
        compute_program_callable_effects(&program);
    with_callable_effects(
        function_effects,
        static_method_effects,
        private_instance_method_effects,
        || prune_block(program),
    )
}

/// Prunes branches with constant conditions that cannot be reached.
pub fn prune_constant_control_flow(program: Program) -> Program {
    let (function_effects, static_method_effects, private_instance_method_effects) =
        compute_program_callable_effects(&program);
    with_callable_effects(
        function_effects,
        static_method_effects,
        private_instance_method_effects,
        || prune_block(program),
    )
}

/// Eliminates code with no observable side effects.
type ConstantEnv = HashMap<String, ScalarValue>;
/// Eliminates dead code for this module.
pub fn eliminate_dead_code(program: Program) -> Program {
    let (function_effects, static_method_effects, private_instance_method_effects) =
        compute_program_callable_effects(&program);
    with_callable_effects(
        function_effects,
        static_method_effects,
        private_instance_method_effects,
        || dce_block(program),
    )
}

/// Effect describes whether a callable or expression has observable runtime behavior.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Effect {
    has_side_effects: bool,
    may_throw: bool,
}

impl Effect {
    /// Pure effect: no side effects and cannot throw.
    const PURE: Self = Self {
        has_side_effects: false,
        may_throw: false,
    };

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

    /// Combines two effects. The result is observable if either operand is observable.
    fn combine(self, other: Self) -> Self {
        Self {
            has_side_effects: self.has_side_effects || other.has_side_effects,
            may_throw: self.may_throw || other.may_throw,
        }
    }

    /// Returns true if this effect has side effects or may throw.
    fn is_observable(self) -> bool {
        self.has_side_effects || self.may_throw
    }
}

/// Carries class resolution context for private instance method effect analysis.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ClassEffectContext {
    class_name: String,
    parent_name: Option<String>,
}

/// Holds the body and never-return metadata for a function during effect analysis.
#[derive(Clone, Debug)]
struct FunctionEffectBody {
    body: Vec<Stmt>,
    declared_never: bool,
}

/// Holds the body, class context, and never-return metadata for a static method during effect analysis.
#[derive(Clone, Debug)]
struct StaticMethodBody {
    context: ClassEffectContext,
    body: Vec<Stmt>,
    declared_never: bool,
}

/// Maps names to scalar constants during constant propagation.

/// Installs function, static method, and private instance method effect maps for the closure's
/// duration, then restores the previous maps. Effect analysis uses thread-local state so
/// `block_effect` and `stmt_effect` can recursively query effects of nested callables.
fn with_callable_effects<R>(
    function_effects: HashMap<String, Effect>,
    static_method_effects: HashMap<String, Effect>,
    private_instance_method_effects: HashMap<String, Effect>,
    f: impl FnOnce() -> R,
) -> R {
    ACTIVE_FUNCTION_EFFECTS.with(|function_slot| {
        ACTIVE_STATIC_METHOD_EFFECTS.with(|static_slot| {
            ACTIVE_PRIVATE_INSTANCE_METHOD_EFFECTS.with(|instance_slot| {
                let previous_functions = function_slot.replace(Some(function_effects));
                let previous_static_methods = static_slot.replace(Some(static_method_effects));
                let previous_instance_methods =
                    instance_slot.replace(Some(private_instance_method_effects));
                let result = f();
                instance_slot.replace(previous_instance_methods);
                static_slot.replace(previous_static_methods);
                function_slot.replace(previous_functions);
                result
            })
        })
    })
}

/// Installs a class effect context for private instance method effect analysis, then restores
/// the previous context.
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

/// Computes the effect for every function, static method, and private instance method in the
/// program. Uses a fixed-point iteration: effects start as PURE and are refined by examining
/// bodies, accounting for nested calls.
fn compute_program_callable_effects(
    program: &[Stmt],
) -> (
    HashMap<String, Effect>,
    HashMap<String, Effect>,
    HashMap<String, Effect>,
) {
    let mut function_bodies = HashMap::new();
    collect_program_function_bodies(program, &mut function_bodies);
    let mut static_method_bodies = HashMap::new();
    collect_program_static_method_bodies(program, &mut static_method_bodies);
    let mut private_instance_method_bodies = HashMap::new();
    collect_program_private_instance_method_bodies(program, &mut private_instance_method_bodies);

    let mut function_effects: HashMap<String, Effect> = function_bodies
        .keys()
        .cloned()
        .map(|name| (name, Effect::PURE))
        .collect();
    let mut static_method_effects: HashMap<String, Effect> = static_method_bodies
        .keys()
        .cloned()
        .map(|name| (name, Effect::PURE))
        .collect();
    let mut private_instance_method_effects: HashMap<String, Effect> = private_instance_method_bodies
        .keys()
        .cloned()
        .map(|name| (name, Effect::PURE))
        .collect();

    loop {
        let function_snapshot = function_effects.clone();
        let static_method_snapshot = static_method_effects.clone();
        let private_instance_method_snapshot = private_instance_method_effects.clone();
        let mut changed = false;

        ACTIVE_FUNCTION_EFFECTS.with(|function_slot| {
            ACTIVE_STATIC_METHOD_EFFECTS.with(|static_slot| {
                ACTIVE_PRIVATE_INSTANCE_METHOD_EFFECTS.with(|instance_slot| {
                    let previous_functions = function_slot.replace(Some(function_snapshot));
                    let previous_static_methods = static_slot.replace(Some(static_method_snapshot));
                    let previous_instance_methods =
                        instance_slot.replace(Some(private_instance_method_snapshot));

                    for (name, function) in &function_bodies {
                        let effect = never_declared_effect(function.declared_never, block_effect(&function.body));
                        if function_effects.get(name).copied() != Some(effect) {
                            function_effects.insert(name.clone(), effect);
                            changed = true;
                        }
                    }

                    for (name, method) in &static_method_bodies {
                        let effect = with_class_effect_context(Some(method.context.clone()), || {
                            block_effect(&method.body)
                        });
                        let effect = never_declared_effect(method.declared_never, effect);
                        if static_method_effects.get(name).copied() != Some(effect) {
                            static_method_effects.insert(name.clone(), effect);
                            changed = true;
                        }
                    }

                    for (name, method) in &private_instance_method_bodies {
                        let effect = with_class_effect_context(Some(method.context.clone()), || {
                            block_effect(&method.body)
                        });
                        let effect = never_declared_effect(method.declared_never, effect);
                        if private_instance_method_effects.get(name).copied() != Some(effect) {
                            private_instance_method_effects.insert(name.clone(), effect);
                            changed = true;
                        }
                    }

                    instance_slot.replace(previous_instance_methods);
                    static_slot.replace(previous_static_methods);
                    function_slot.replace(previous_functions);
                });
            });
        });

        if !changed {
            return (
                function_effects,
                static_method_effects,
                private_instance_method_effects,
            );
        }
    }
}

/// Collects all top-level and namespace-scoped function bodies into `out` for effect analysis.
fn collect_program_function_bodies(stmts: &[Stmt], out: &mut HashMap<String, FunctionEffectBody>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::FunctionDecl {
                name,
                body,
                return_type,
                ..
            } => {
                out.insert(
                    name.clone(),
                    FunctionEffectBody {
                        body: body.clone(),
                        declared_never: is_never_return_type(return_type),
                    },
                );
            }
            StmtKind::NamespaceBlock { body, .. } => collect_program_function_bodies(body, out),
            _ => {}
        }
    }
}

/// Collects all static method bodies in classes into `out` for effect analysis.
fn collect_program_static_method_bodies(
    stmts: &[Stmt],
    out: &mut HashMap<String, StaticMethodBody>,
) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::ClassDecl {
                name,
                extends,
                methods,
                ..
            } => {
                let context = ClassEffectContext {
                    class_name: name.clone(),
                    parent_name: extends.as_ref().map(|parent| parent.as_str().to_string()),
                };
                for method in methods {
                    if method.is_static && method.has_body {
                        out.insert(
                            method_effect_key(name, &method.name),
                            StaticMethodBody {
                                context: context.clone(),
                                body: method.body.clone(),
                                declared_never: is_never_return_type(&method.return_type),
                            },
                        );
                    }
                }
            }
            StmtKind::NamespaceBlock { body, .. } => collect_program_static_method_bodies(body, out),
            _ => {}
        }
    }
}

/// Collects all private instance method bodies in classes into `out` for effect analysis.
fn collect_program_private_instance_method_bodies(
    stmts: &[Stmt],
    out: &mut HashMap<String, StaticMethodBody>,
) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::ClassDecl {
                name,
                extends,
                methods,
                ..
            } => {
                let context = ClassEffectContext {
                    class_name: name.clone(),
                    parent_name: extends.as_ref().map(|parent| parent.as_str().to_string()),
                };
                for method in methods {
                    if !method.is_static
                        && method.has_body
                        && matches!(method.visibility, crate::parser::ast::Visibility::Private)
                    {
                        out.insert(
                            method_effect_key(name, &method.name),
                            StaticMethodBody {
                                context: context.clone(),
                                body: method.body.clone(),
                                declared_never: is_never_return_type(&method.return_type),
                            },
                        );
                    }
                }
            }
            StmtKind::NamespaceBlock { body, .. } => {
                collect_program_private_instance_method_bodies(body, out)
            }
            _ => {}
        }
    }
}

/// Builds the map key for a method effect entry, using PHP symbol keying for the method name.
fn method_effect_key(class_name: &str, method_name: &str) -> String {
    format!("{class_name}::{}", php_symbol_key(method_name))
}

/// Returns true if the type expression is `Never`.
fn is_never_return_type(return_type: &Option<TypeExpr>) -> bool {
    matches!(return_type, Some(TypeExpr::Never))
}

/// Adjusts an effect when the callable has a `never` return type. A `never` function is
/// considered to have side effects because it exits abruptly (e.g., via exit/die or an
/// infinite loop) and the PHP-visible control flow never continues past it.
fn never_declared_effect(declared_never: bool, effect: Effect) -> Effect {
    if declared_never {
        effect.with_side_effects()
    } else {
        effect
    }
}
