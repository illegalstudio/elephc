//! Purpose:
//! Computes fixed-point callable effects and closed-world class dispatch metadata.
//!
//! Called from:
//! - Optimizer propagation, control-flow normalization, pruning, and dead-code elimination entry points.
//!
//! Key details:
//! - Summaries remain conservative around dynamic eval, trait dispatch, visibility, and never-returning bodies.

use super::*;

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

    loop {
        let (function_updates, static_method_updates, instance_method_updates) =
            ACTIVE_FUNCTION_EFFECTS.with(|function_slot| {
                ACTIVE_STATIC_METHOD_EFFECTS.with(|static_slot| {
                    ACTIVE_INSTANCE_METHOD_EFFECTS.with(|instance_slot| {
                        ACTIVE_INSTANCE_DISPATCH_METADATA.with(|metadata_slot| {
                            let previous_functions =
                                function_slot.replace(Some(Rc::clone(&function_effects)));
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
                                let effect = declared_return_effect(
                                    function.declared_return_may_throw,
                                    never_declared_effect(
                                    function.declared_never,
                                    block_effect(&function.body),
                                    ),
                                );
                                if function_effects.get(name).copied() != Some(effect) {
                                    function_updates.push((name.clone(), effect));
                                }
                            }

                            for (name, method) in &static_method_bodies {
                                let effect = with_class_effect_context(
                                    Some(method.context.clone()),
                                    || block_effect(&method.body),
                                );
                                let effect = declared_return_effect(
                                    method.declared_return_may_throw,
                                    never_declared_effect(method.declared_never, effect),
                                );
                                if static_method_effects.get(name).copied() != Some(effect) {
                                    static_method_updates.push((name.clone(), effect));
                                }
                            }

                            for (name, method) in &instance_method_bodies {
                                let effect = with_class_effect_context(
                                    Some(method.context.clone()),
                                    || block_effect(&method.body),
                                );
                                let effect = declared_return_effect(
                                    method.declared_return_may_throw,
                                    never_declared_effect(method.declared_never, effect),
                                );
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
                return_type,
                ..
            } => {
                out.insert(
                    name.clone(),
                    FunctionEffectBody {
                        body,
                        declared_never: is_never_return_type(return_type),
                        declared_return_may_throw: declared_return_may_throw(return_type),
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
                                declared_return_may_throw: declared_return_may_throw(
                                    &method.return_type,
                                ),
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
                                declared_return_may_throw: declared_return_may_throw(
                                    &method.return_type,
                                ),
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

/// Returns whether PHP can raise `TypeError` while enforcing this declared return contract.
fn declared_return_may_throw(return_type: &Option<TypeExpr>) -> bool {
    match return_type {
        None | Some(TypeExpr::Void | TypeExpr::Never) => false,
        Some(TypeExpr::Named(name))
            if name
                .as_str()
                .trim_start_matches('\\')
                .eq_ignore_ascii_case("mixed") =>
        {
            false
        }
        _ => true,
    }
}

/// Adds the catchable runtime return-boundary effect for an ordinary declared return type.
fn declared_return_effect(may_throw: bool, effect: Effect) -> Effect {
    if may_throw {
        effect.with_may_throw()
    } else {
        effect
    }
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
