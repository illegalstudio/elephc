//! Purpose:
//! Computes fixed-point callable effects and closed-world class dispatch metadata.
//!
//! Called from:
//! - Optimizer propagation, control-flow normalization, pruning, and dead-code elimination entry points.
//!
//! Key details:
//! - Summaries remain conservative around dynamic eval, trait dispatch, visibility, and never-returning bodies.

use super::*;

/// Couples immutable summaries with their case-folded declaration index for scoped lookups.
#[derive(Clone)]
pub(super) struct FunctionEffectLookup {
    effects: Rc<HashMap<String, Effect>>,
    names: Rc<HashMap<String, String>>,
    results: Rc<HashMap<String, crate::types::PhpType>>,
}

impl FunctionEffectLookup {
    /// Builds one name index for an immutable callable-summary map.
    pub(super) fn new(effects: Rc<HashMap<String, Effect>>, results: Rc<HashMap<String, crate::types::PhpType>>) -> Self {
        let names = Rc::new(effects.keys().map(|name|
            (php_symbol_key(name), name.clone())).collect());
        Self { effects, names, results }
    }

    /// Keeps user return contracts ahead of builtin fallback, including unknown and never results.
    pub(super) fn result_type(&self, name: &str) -> Option<crate::types::PhpType> {
        let canonical = if self.effects.contains_key(name) { name }
            else { self.names.get(&php_symbol_key(name))?.as_str() };
        Some(self.results.get(canonical).cloned().unwrap_or(crate::types::PhpType::Mixed))
    }

    /// Resolves exact and case-insensitive names without scanning unrelated declarations.
    pub(super) fn get(&self, name: &str) -> Option<&Effect> {
        self.effects.get(name).or_else(|| self.names.get(&php_symbol_key(name))
            .and_then(|canonical| self.effects.get(canonical)))
    }
}

/// Computes the effect for every function, static method, and concrete instance method in the
/// program. Uses a fixed-point iteration: effects start as PURE and are refined by examining
/// bodies, accounting for nested calls and closed-world override sets.
pub(super) fn compute_program_callable_effects(
    program: &[Stmt],
) -> (
    Rc<HashMap<String, Effect>>,
    Rc<HashMap<String, Effect>>,
    Rc<HashMap<String, Effect>>,
) {
    let mut function_bodies = HashMap::new();
    collect_program_function_bodies(program, &mut function_bodies);
    let mut static_method_bodies = HashMap::new();
    collect_program_static_method_bodies(program, &mut static_method_bodies);
    let mut instance_method_bodies = HashMap::new();
    collect_program_instance_method_bodies(program, &mut instance_method_bodies);
    let instance_dispatch_metadata = Rc::new(collect_instance_dispatch_metadata(program));

    let mut function_effects = Rc::new(
        function_bodies
            .keys()
            .cloned()
            .map(|name| (name, Effect::PURE))
            .collect(),
    );
    let mut static_method_effects = Rc::new(
        static_method_bodies
            .keys()
            .cloned()
            .map(|name| (name, Effect::PURE))
            .collect(),
    );
    let mut instance_method_effects = Rc::new(
        instance_method_bodies
            .keys()
            .cloned()
            .map(|name| (name, Effect::PURE))
            .collect(),
    );

    // Declaration keys stay fixed while effects converge, so every round shares this index.
    let function_names = Rc::new(function_bodies.keys().map(|name|
        (php_symbol_key(name), name.clone())).collect());
    let function_results = Rc::new(function_bodies.iter().map(|(name, body)|
        (name.clone(), body.return_boundary.declared_result())).collect());
    loop {
        let (function_updates, static_method_updates, instance_method_updates) =
            ACTIVE_FUNCTION_EFFECTS.with(|function_slot| {
                ACTIVE_STATIC_METHOD_EFFECTS.with(|static_slot| {
                    ACTIVE_INSTANCE_METHOD_EFFECTS.with(|instance_slot| {
                        ACTIVE_INSTANCE_DISPATCH_METADATA.with(|metadata_slot| {
                            let previous_functions =
                                function_slot.replace(Some(FunctionEffectLookup {
                                    effects: Rc::clone(&function_effects),
                                    names: Rc::clone(&function_names),
                                    results: Rc::clone(&function_results),
                                }));
                            let previous_static_methods =
                                static_slot.replace(Some(Rc::clone(&static_method_effects)));
                            let previous_instance_methods =
                                instance_slot.replace(Some(Rc::clone(&instance_method_effects)));
                            let previous_metadata = metadata_slot
                                .replace(Some(Rc::clone(&instance_dispatch_metadata)));

                            let mut function_updates = Vec::new();
                            let mut static_method_updates = Vec::new();
                            let mut instance_method_updates = Vec::new();

                            for (name, function) in &function_bodies {
                                let effect = never_declared_effect(
                                    function.declared_never,
                                    parameter_guard_effect(function.parameter_guard_may_throw,
                                        with_return_boundary_facts(Some(&function.return_boundary), || block_effect(function.body))),
                                );
                                if function_effects.get(name).copied() != Some(effect) {
                                    function_updates.push((name.clone(), effect));
                                }
                            }

                            for (name, method) in &static_method_bodies {
                                let effect = with_class_effect_context(
                                    Some(method.context.clone()),
                                    || with_return_boundary_facts(Some(&method.return_boundary), || block_effect(method.body)),
                                );
                                let effect = never_declared_effect(method.declared_never,
                                    parameter_guard_effect(method.parameter_guard_may_throw, effect));
                                if static_method_effects.get(name).copied() != Some(effect) {
                                    static_method_updates.push((name.clone(), effect));
                                }
                            }

                            for (name, method) in &instance_method_bodies {
                                let effect = with_class_effect_context(
                                    Some(method.context.clone()),
                                    || with_return_boundary_facts(Some(&method.return_boundary), || block_effect(method.body)),
                                );
                                let effect = never_declared_effect(method.declared_never,
                                    parameter_guard_effect(method.parameter_guard_may_throw, effect));
                                if instance_method_effects.get(name).copied() != Some(effect) {
                                    instance_method_updates.push((name.clone(), effect));
                                }
                            }

                            metadata_slot.replace(previous_metadata);
                            instance_slot.replace(previous_instance_methods);
                            static_slot.replace(previous_static_methods);
                            function_slot.replace(previous_functions);
                            (
                                function_updates,
                                static_method_updates,
                                instance_method_updates,
                            )
                        })
                    })
                })
            });

        if function_updates.is_empty()
            && static_method_updates.is_empty()
            && instance_method_updates.is_empty()
        {
            return (
                function_effects,
                static_method_effects,
                instance_method_effects,
            );
        }

        // The TLS owners are restored before these staged updates, so make_mut
        // applies only the changed summaries without cloning every map per body.
        let functions = Rc::make_mut(&mut function_effects);
        for (name, effect) in function_updates {
            functions.insert(name, effect);
        }
        let static_methods = Rc::make_mut(&mut static_method_effects);
        for (name, effect) in static_method_updates {
            static_methods.insert(name, effect);
        }
        let instance_methods = Rc::make_mut(&mut instance_method_effects);
        for (name, effect) in instance_method_updates {
            instance_methods.insert(name, effect);
        }
    }
}

/// Collects all top-level and namespace-scoped function bodies into `out` for effect analysis.
fn collect_program_function_bodies<'a>(
    stmts: &'a [Stmt],
    out: &mut HashMap<String, FunctionEffectBody<'a>>,
) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::FunctionDecl {
                name,
                body,
                params,
                return_type,
                ..
            } => {
                out.insert(
                    name.clone(),
                    FunctionEffectBody {
                        body,
                        declared_never: is_never_return_type(return_type),
                        parameter_guard_may_throw: params.iter().any(|(_, ty, _, _)| matches!(ty, Some(TypeExpr::False))),
                        return_boundary: ReturnBoundaryFacts::new(return_type.as_ref(),
                            params.iter().map(|(name, hint, ..)| (name.as_str(), hint.as_ref())), body),
                    },
                );
            }
            StmtKind::NamespaceBlock { body, .. } => collect_program_function_bodies(body, out),
            _ => {}
        }
    }
}

/// Collects all static method bodies in classes into `out` for effect analysis.
fn collect_program_static_method_bodies<'a>(
    stmts: &'a [Stmt],
    out: &mut HashMap<String, StaticMethodBody<'a>>,
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
                                body: &method.body,
                                declared_never: is_never_return_type(&method.return_type),
                                parameter_guard_may_throw: method.params.iter().any(|(_, ty, _, _)| matches!(ty, Some(TypeExpr::False))),
                                return_boundary: ReturnBoundaryFacts::new(method.return_type.as_ref(),
                                    method.params.iter().map(|(name, hint, ..)| (name.as_str(), hint.as_ref())), &method.body),
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

/// Collects all concrete instance method bodies in classes into `out` for effect analysis.
fn collect_program_instance_method_bodies<'a>(
    stmts: &'a [Stmt],
    out: &mut HashMap<String, StaticMethodBody<'a>>,
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
                    if !method.is_static && method.has_body {
                        out.insert(
                            method_effect_key(name, &method.name),
                            StaticMethodBody {
                                context: context.clone(),
                                body: &method.body,
                                declared_never: is_never_return_type(&method.return_type),
                                parameter_guard_may_throw: method.params.iter().any(|(_, ty, _, _)| matches!(ty, Some(TypeExpr::False))),
                                return_boundary: ReturnBoundaryFacts::new(method.return_type.as_ref(),
                                    method.params.iter().map(|(name, hint, ..)| (name.as_str(), hint.as_ref())), &method.body),
                            },
                        );
                    }
                }
            }
            StmtKind::NamespaceBlock { body, .. } => {
                collect_program_instance_method_bodies(body, out)
            }
            _ => {}
        }
    }
}

/// Collects class hierarchy, method modifiers, and direct property facts for effect resolution.
pub(super) fn collect_instance_dispatch_metadata(stmts: &[Stmt]) -> InstanceDispatchMetadata {
    let mut metadata = InstanceDispatchMetadata {
        has_dynamic_class_barrier: crate::ir_lower::body_contains_eval_call(stmts),
        ..InstanceDispatchMetadata::default()
    };
    collect_instance_dispatch_metadata_into(stmts, &mut metadata);
    metadata
}

/// Recursively adds class declarations from one statement block to dispatch metadata.
fn collect_instance_dispatch_metadata_into(
    stmts: &[Stmt],
    metadata: &mut InstanceDispatchMetadata,
) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::ClassDecl {
                name,
                extends,
                is_abstract,
                is_final,
                trait_uses,
                properties,
                methods,
                ..
            } => {
                metadata.has_internal_object_operators |= elephc_builtin_contract::lookup_class(name)
                    .is_some_and(|class| class.operators.is_some());
                let private_methods = methods
                    .iter()
                    .filter(|method| {
                        !method.is_static
                            && matches!(
                                method.visibility,
                                crate::parser::ast::Visibility::Private
                            )
                    })
                    .map(|method| php_symbol_key(&method.name))
                    .collect();
                let final_methods = methods
                    .iter()
                    .filter(|method| !method.is_static && method.is_final)
                    .map(|method| php_symbol_key(&method.name))
                    .collect();
                let method_visibilities = methods
                    .iter()
                    .filter(|method| !method.is_static)
                    .map(|method| (php_symbol_key(&method.name), method.visibility.clone()))
                    .collect();
                let direct_properties = properties
                    .iter()
                    .filter(|property| !property.is_static)
                    .map(|property| {
                        (
                            property.name.clone(),
                            PropertyReadMetadata {
                                declaring_class: name.clone(),
                                visibility: property.visibility.clone(),
                                typed: property.type_expr.is_some(),
                                hooked: property.hooks.requires_get(),
                            },
                        )
                    })
                    .collect();
                let has_magic_get = methods.iter().any(|method| {
                    !method.is_static && php_symbol_key(&method.name) == "__get"
                });
                metadata.classes.insert(
                    name.clone(),
                    InstanceClassMetadata {
                        parent_name: extends.as_ref().map(|parent| parent.as_str().to_string()),
                        is_abstract: *is_abstract,
                        is_final: *is_final,
                        has_trait_uses: !trait_uses.is_empty(),
                        private_methods,
                        final_methods,
                        method_visibilities,
                        properties: direct_properties,
                        has_magic_get,
                    },
                );
            }
            StmtKind::NamespaceBlock { body, .. } => {
                collect_instance_dispatch_metadata_into(body, metadata);
            }
            _ => {}
        }
    }
}

/// Builds the map key for a method effect entry, using PHP symbol keying for the method name.
pub(super) fn method_effect_key(class_name: &str, method_name: &str) -> String {
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

/// Keeps call-boundary literal-type guards observable even when the callable body is pure.
fn parameter_guard_effect(may_throw: bool, effect: Effect) -> Effect {
    if may_throw {
        effect.with_may_throw()
    } else {
        effect
    }
}
