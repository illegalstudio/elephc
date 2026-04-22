use crate::names::Name;
use crate::parser::ast::{
    BinOp, CallableTarget, CastType, ClassMethod, ClassProperty, EnumCaseDecl, Expr, ExprKind,
    Program, Stmt, StmtKind,
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

pub fn fold_constants(program: Program) -> Program {
    program.into_iter().map(fold_stmt).collect()
}

pub fn propagate_constants(program: Program) -> Program {
    propagate_block(program, HashMap::new()).0
}

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

type ConstantEnv = HashMap<String, ScalarValue>;

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Effect {
    has_side_effects: bool,
    may_throw: bool,
}

impl Effect {
    const PURE: Self = Self {
        has_side_effects: false,
        may_throw: false,
    };

    fn with_side_effects(mut self) -> Self {
        self.has_side_effects = true;
        self
    }

    fn with_may_throw(mut self) -> Self {
        self.may_throw = true;
        self
    }

    fn combine(self, other: Self) -> Self {
        Self {
            has_side_effects: self.has_side_effects || other.has_side_effects,
            may_throw: self.may_throw || other.may_throw,
        }
    }

    fn is_observable(self) -> bool {
        self.has_side_effects || self.may_throw
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ClassEffectContext {
    class_name: String,
    parent_name: Option<String>,
}

#[derive(Clone, Debug)]
struct StaticMethodBody {
    context: ClassEffectContext,
    body: Vec<Stmt>,
}

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

fn with_class_effect_context<R>(context: Option<ClassEffectContext>, f: impl FnOnce() -> R) -> R {
    ACTIVE_CLASS_EFFECT_CONTEXT.with(|slot| {
        let previous = slot.replace(context);
        let result = f();
        slot.replace(previous);
        result
    })
}

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

fn current_callable_alias_effects() -> HashMap<String, Effect> {
    ACTIVE_CALLABLE_ALIAS_EFFECTS.with(|slot| slot.borrow().clone().unwrap_or_default())
}

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

                    for (name, body) in &function_bodies {
                        let effect = block_effect(body);
                        if function_effects.get(name).copied() != Some(effect) {
                            function_effects.insert(name.clone(), effect);
                            changed = true;
                        }
                    }

                    for (name, method) in &static_method_bodies {
                        let effect = with_class_effect_context(Some(method.context.clone()), || {
                            block_effect(&method.body)
                        });
                        if static_method_effects.get(name).copied() != Some(effect) {
                            static_method_effects.insert(name.clone(), effect);
                            changed = true;
                        }
                    }

                    for (name, method) in &private_instance_method_bodies {
                        let effect = with_class_effect_context(Some(method.context.clone()), || {
                            block_effect(&method.body)
                        });
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

fn collect_program_function_bodies(stmts: &[Stmt], out: &mut HashMap<String, Vec<Stmt>>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::FunctionDecl { name, body, .. } => {
                out.insert(name.clone(), body.clone());
            }
            StmtKind::NamespaceBlock { body, .. } => collect_program_function_bodies(body, out),
            _ => {}
        }
    }
}

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

fn method_effect_key(class_name: &str, method_name: &str) -> String {
    format!("{class_name}::{method_name}")
}
