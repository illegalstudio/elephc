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

    /// Looks up a method contract using the canonical dispatch key from the effect analysis.
    pub(super) fn method_result_type(&self, key: &str) -> Option<crate::types::PhpType> {
        self.results.get(key).cloned()
    }

    /// Resolves exact and case-insensitive names without scanning unrelated declarations.
    pub(super) fn get(&self, name: &str) -> Option<&Effect> {
        self.effects.get(name).or_else(|| self.names.get(&php_symbol_key(name))
            .and_then(|canonical| self.effects.get(canonical)))
    }

    /// Replaces one declaration summary, keeping the case-folded index in step.
    pub(super) fn insert(&mut self, name: String, effect: Effect) {
        Rc::make_mut(&mut self.names).insert(php_symbol_key(&name), name.clone());
        Rc::make_mut(&mut self.effects).insert(name, effect);
    }
}

/// String binding can invoke user code; by-reference storage is never coerced here.
pub(super) fn binds_string_parameter(params: &[(String, Option<TypeExpr>, Option<Expr>, bool)]) -> bool {
    params.iter().any(|(_, ty, _, by_ref)| !by_ref && ty.as_ref().is_some_and(admits_string))
}

/// Whether a declared type permits the Stringable binding route.
fn admits_string(ty: &TypeExpr) -> bool {
        match ty {
            TypeExpr::Str => true,
            TypeExpr::Nullable(inner) => admits_string(inner),
            TypeExpr::Union(types) => types.iter().any(admits_string),
            _ => false,
        }
}

/// Includes the element binding performed for a by-value variadic capture.
pub(super) fn callable_binds_string(
    params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
    variadic_type: Option<&TypeExpr>,
    variadic_by_ref: bool,
) -> bool {
    binds_string_parameter(params)
        || (!variadic_by_ref && variadic_type.is_some_and(admits_string))
}

/// Shared implicit binding barrier for named functions, methods and closures.
pub(super) fn string_binding_effect() -> Effect {
    let effect = with_active_instance_method_effects(|effects| {
        effects.map(|effects| effects.iter()
            .filter(|(name, _)| name.ends_with("::__tostring"))
            .fold(Effect::PURE.with_may_throw(), |effect, (_, stringifier)| {
                effect.combine(*stringifier)
            }))
    }).unwrap_or(Effect::PURE.with_may_throw());
    if with_active_instance_dispatch_metadata(|metadata| {
        metadata.is_none_or(|metadata| metadata.has_dynamic_class_barrier)
    }) {
        effect.with_side_effects().with_writes_globals()
            .with_process_global_storage().with_process_global_runtime_state()
    } else { effect }
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
    compute_program_callable_effects_with_initial_function_effects(program, HashMap::new())
}

/// Computes callable summaries with fixed external-call effects seeded into the call graph.
///
/// Extern functions accepting PHP callables may re-enter arbitrary user code, so Parallel's
/// isolation analysis supplies a process-global effect for those symbols before this fixed point.
pub(super) fn compute_program_callable_effects_with_initial_function_effects(
    program: &[Stmt],
    mut initial_function_effects: HashMap<String, Effect>,
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

    for name in function_bodies.keys() {
        initial_function_effects
            .entry(name.clone())
            .or_insert(Effect::PURE);
    }
    let mut function_effects = Rc::new(initial_function_effects);
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
        (name.clone(), body.return_boundary.declared_result()))
        .chain(static_method_bodies.iter().map(|(name, body)|
            (name.clone(), body.return_boundary.declared_result())))
        .chain(instance_method_bodies.iter().map(|(name, body)|
            (name.clone(), body.return_boundary.declared_result()))).collect());
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

                            // Parameter binding happens even when a pure body's result is
                            // discarded. A boxed string argument may dispatch __toString
                            // or reject its payload before the body starts. Include the
                            // closed-world stringifier effects in the same fixed point.
                            let string_binding_effect = string_binding_effect();

                            for (name, function) in &function_bodies {
                                let effect = never_declared_effect(
                                    function.declared_never,
                                    parameter_guard_effect(function.parameter_guard_may_throw,
                                        with_reference_parameter_scope(false, function.reference_parameters.clone(), ||
                                            with_return_boundary_facts(Some(&function.return_boundary), || block_effect(function.body))))
                                        .combine(return_binding_effects::effect(function.params, function.return_type, function.body))
                                        .combine(if function.binds_string {
                                            string_binding_effect
                                        } else { Effect::PURE }),
                                );
                                if function_effects.get(name).copied() != Some(effect) {
                                    function_updates.push((name.clone(), effect));
                                }
                            }

                            for (name, method) in &static_method_bodies {
                                let effect = with_class_effect_context(
                                    Some(method.context.clone()),
                                    || with_reference_parameter_scope(false, method.reference_parameters.clone(), ||
                                        with_return_boundary_facts(Some(&method.return_boundary), || block_effect(method.body)))
                                        .combine(return_binding_effects::effect(method.params, method.return_type, method.body))
                                        .combine(if method.binds_string {
                                            string_binding_effect
                                        } else { Effect::PURE }),
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
                                    || with_reference_parameter_scope(false, method.reference_parameters.clone(), ||
                                        with_return_boundary_facts(Some(&method.return_boundary), || block_effect(method.body)))
                                        .combine(return_binding_effects::effect(method.params, method.return_type, method.body))
                                        .combine(if method.binds_string {
                                            string_binding_effect
                                        } else { Effect::PURE }),
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
                variadic,
                variadic_by_ref,
                return_type,
                variadic_type,
                ..
            } => {
                out.insert(
                    name.clone(),
                    FunctionEffectBody {
                        params, return_type,
                        body,
                        declared_never: is_never_return_type(return_type),
                        parameter_guard_may_throw: params.iter().any(|(_, ty, _, _)| type_guard_may_throw(ty.as_ref())),
                        reference_parameters: reference_parameter_names(params, variadic.as_deref(), *variadic_by_ref),
                        return_boundary: ReturnBoundaryFacts::new(return_type.as_ref(),
                            params.iter().map(|(name, hint, ..)| (name.as_str(), hint.as_ref())), body),
                        binds_string: callable_binds_string(params, variadic_type.as_ref(), *variadic_by_ref),
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
                                params: &method.params, return_type: &method.return_type,
                                context: context.clone(),
                                body: &method.body,
                                declared_never: is_never_return_type(&method.return_type),
                                parameter_guard_may_throw: method.params.iter().any(|(_, ty, _, _)| type_guard_may_throw(ty.as_ref())),
                                reference_parameters: reference_parameter_names(&method.params,
                                    method.variadic.as_deref(), method.variadic_by_ref),
                                return_boundary: ReturnBoundaryFacts::new(method.return_type.as_ref(),
                                    method.params.iter().map(|(name, hint, ..)| (name.as_str(), hint.as_ref())), &method.body),
                                binds_string: callable_binds_string(&method.params, method.variadic_type.as_ref(), method.variadic_by_ref),
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
                                params: &method.params, return_type: &method.return_type,
                                context: context.clone(),
                                body: &method.body,
                                declared_never: is_never_return_type(&method.return_type),
                                parameter_guard_may_throw: method.params.iter().any(|(_, ty, _, _)| type_guard_may_throw(ty.as_ref())),
                                reference_parameters: reference_parameter_names(&method.params,
                                    method.variadic.as_deref(), method.variadic_by_ref),
                                return_boundary: ReturnBoundaryFacts::new(method.return_type.as_ref(),
                                    method.params.iter().map(|(name, hint, ..)| (name.as_str(), hint.as_ref())), &method.body),
                                binds_string: callable_binds_string(&method.params, method.variadic_type.as_ref(), method.variadic_by_ref),
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

/// Returns whether binding this declared parameter can raise before the body runs.
///
/// A literal `false` type and a class type both run a call-boundary guard: `false` checks the
/// literal, and a class checks that a `Class|false` / `Class|null` union argument carries the
/// object alternative. Keep the call observable even when the body itself is pure.
fn type_guard_may_throw(ty: Option<&TypeExpr>) -> bool {
    let Some(ty) = ty else { return false };
    match ty {
        TypeExpr::False | TypeExpr::Named(_) if !type_expr_is_builtin(ty) => true,
        TypeExpr::GenericClass { .. } | TypeExpr::Intersection(_) => true,
        TypeExpr::Nullable(inner) => type_guard_may_throw(Some(inner)),
        TypeExpr::Union(members) => members.iter().any(|member| type_guard_may_throw(Some(member))),
        _ => false,
    }
}

/// Returns whether a named type expression is a PHP builtin rather than a class.
fn type_expr_is_builtin(ty: &TypeExpr) -> bool {
    match ty {
        TypeExpr::Named(name) => matches!(
            name.as_str().trim_start_matches('\\').to_ascii_lowercase().as_str(),
            "mixed" | "array" | "object" | "callable" | "iterable" | "string" | "int"
                | "float" | "bool" | "null" | "false" | "true" | "static" | "self"
                | "parent" | "void" | "never"
        ),
        _ => false,
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
