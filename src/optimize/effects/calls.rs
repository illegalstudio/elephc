//! Purpose:
//! Models optimizer side effects for calls behavior.
//! Feeds purity, callable alias, builtin, and call-effect decisions into pruning and dead-code elimination.
//!
//! Called from:
//! - `crate::optimize::effects`
//!
//! Key details:
//! - Effect summaries must account for globals, heap/runtime state, output, throws, and by-reference mutation.

use super::*;
use crate::types::PhpType;

/// Looks up the effect for a named function call.
///
/// Uses thread-local `ACTIVE_FUNCTION_EFFECTS` for user-defined functions. Falls back to
/// Registry builtins consume their shared descriptor; unknown calls remain conservative.
pub(in crate::optimize) fn function_call_effect(name: &str, args: &[Expr]) -> Effect {
    let function_key = crate::names::php_symbol_key(name.trim_start_matches('\\'));
    let mut pcntl_process_write = false;
    let effect = with_active_function_effects(|effects| {
        effects.and_then(|effects| {
            effects.get(name).copied().or_else(|| {
                effects.iter().find_map(|(candidate, effect)| {
                    (crate::names::php_symbol_key(candidate.trim_start_matches('\\'))
                        == function_key)
                    .then_some(*effect)
                })
            })
        })
    })
    .unwrap_or_else(|| {
        if let Some(def) = crate::builtins::registry::lookup(name) {
            let arg_types = semantic_optimizer_arg_types(def, args);
            let input = crate::builtins::semantics::BuiltinSemanticInput {
                name: def.name,
                args,
                arg_types: &arg_types,
                span: crate::span::Span::dummy(),
            };
            let effects = crate::builtins::semantics::resolve_builtin_effects(def, &input);
            pcntl_process_write = function_key.starts_with("pcntl_")
                && effects.contains(crate::ir::Effects::WRITES_PROCESS);
            if let Some((callback_index, intrinsic_effects)) = builtin_callback_effects(def) {
                let callback_effect = args
                    .get(callback_index)
                    .map(expr_call_effect)
                    .unwrap_or_else(conservative_call_effect);
                return Effect::from_eir(intrinsic_effects).combine(callback_effect);
            }
            Effect::from_eir(effects)
        } else {
            let effect = conservative_call_effect();
            if crate::optimize::parallel_extern_is_worker_safe(&function_key)
                || matches!(
                    function_key.as_str(),
                    "die" | "empty" | "exit" | "isset" | "unset"
                )
            {
                effect
            } else {
                // A statically named call absent from both the user-function summaries and
                // builtin registry is an external/unknown boundary. Parallel cannot assume
                // that arbitrary native code is worker-safe.
                effect.with_process_global_runtime_state()
            }
        }
    });
    let effect = if args.iter().any(|arg| matches!(arg.kind, ExprKind::Spread(_))) {
        effect.with_may_throw()
    } else { effect };
    let key = crate::names::php_symbol_key(name.trim_start_matches('\\'));
    let effect = match key.as_str() {
        "elephc\\parallel\\run" => effect.with_parallel_scope(),
        "elephc\\async\\run" => effect.with_async_scope(),
        _ => effect,
    };
    let effect = if uses_process_global_runtime_state(&key) {
        effect.with_process_global_runtime_state()
    } else {
        effect
    };
    if pcntl_process_write
        || call_may_dispatch_user_wrapper(&key, args)
        || matches!(
            key.as_str(),
            "stream_wrapper_register"
                | "stream_wrapper_unregister"
                | "stream_wrapper_restore"
                | "stream_filter_register"
                | "stream_filter_append"
                | "stream_filter_prepend"
                | "stream_get_wrappers"
                | "stream_get_filters"
                | "bcscale"
                | "chdir"
                | "iconv_set_encoding"
                | "umask"
        )
    {
        effect.with_process_global_storage()
    } else {
        effect
    }
}

/// Mutable runtime state that is process-wide in the current Parallel implementation.
///
/// These builtins are rejected from worker call graphs until their backing state is moved into
/// `_rt_ctx` or made explicitly thread-safe. This list is deliberately separate from PHP global
/// storage and generic EIR `READS_GLOBAL`/`WRITES_GLOBAL` effects.
fn uses_process_global_runtime_state(name: &str) -> bool {
    matches!(
        name,
        "date"
            | "date_default_timezone_get"
            | "date_default_timezone_set"
            | "getdate"
            | "getenv"
            | "gmdate"
            | "localtime"
            | "mktime"
            | "putenv"
            | "setenv"
            | "strtotime"
            | "unsetenv"
    ) || name.starts_with("json_")
        || name.starts_with("stream_context_")
}

/// User stream-wrapper schemes live in the shared PHP registry, unlike ordinary file paths.
/// Unknown paths may contain a scheme at runtime, so Parallel v1 rejects wrapper-aware paths
/// conservatively even when their values are not statically known.
fn path_may_dispatch_user_wrapper(path: Option<&Expr>) -> bool {
    match path.map(|path| &path.kind) {
        Some(ExprKind::StringLiteral(path)) => path.contains("://"),
        _ => true,
    }
}

/// Checks each builtin's path positions for possible dispatch through the shared wrapper registry.
fn call_may_dispatch_user_wrapper(name: &str, args: &[Expr]) -> bool {
    let first_path = || path_may_dispatch_user_wrapper(args.first());
    match name {
        "copy" | "rename" => first_path() || path_may_dispatch_user_wrapper(args.get(1)),
        "hash_file" => path_may_dispatch_user_wrapper(args.get(1)),
        "fopen"
        | "readfile"
        | "opendir"
        | "scandir"
        | "file"
        | "file_get_contents"
        | "file_put_contents"
        | "file_exists"
        | "getimagesize"
        | "imagecreatefrombmp"
        | "imagecreatefromgif"
        | "imagecreatefromjpeg"
        | "imagecreatefrompng"
        | "imagecreatefromtga"
        | "imagecreatefromwebp"
        | "stat"
        | "lstat"
        | "fileatime"
        | "filectime"
        | "filegroup"
        | "fileinode"
        | "fileowner"
        | "fileperms"
        | "filetype"
        | "filesize"
        | "filemtime"
        | "is_file"
        | "is_dir"
        | "is_link"
        | "is_readable"
        | "is_writable"
        | "is_writeable"
        | "is_executable"
        | "unlink"
        | "mkdir"
        | "rmdir"
        | "chmod"
        | "chown"
        | "chgrp"
        | "lchown"
        | "lchgrp"
        | "touch" => first_path(),
        _ => false,
    }
}

/// Returns the callback operand and callback-free intrinsic effects for a typed runtime builtin.
fn builtin_callback_effects(
    def: &crate::builtins::registry::BuiltinDef,
) -> Option<(usize, crate::ir::Effects)> {
    let crate::builtins::semantics::BuiltinLowering::Runtime(
        crate::ir::RuntimeCallTarget::Function(target),
    ) = def.spec.semantics.lowering
    else {
        return None;
    };
    target
        .string_callback_operand_index()
        .map(|index| (index, target.intrinsic_effects()))
}

/// Returns the full conservative barrier used for an unresolved callable target.
fn conservative_call_effect() -> Effect {
    Effect::PURE
        .with_side_effects()
        .with_may_throw()
        .with_writes_globals()
}

/// Resolves a statically known function-name string used as an indirect callable.
///
/// Dynamic scheduler/Fiber entrypoints intentionally retain only a conservative call barrier
/// here: Parallel's runtime guard owns those indirect-call diagnostics, and static propagation
/// must not turn a guarded `call_user_func("Fiber::suspend", ...)` into a compile-time rejection.
pub(super) fn callable_string_effect(name: &str) -> Option<Effect> {
    let key = crate::names::php_symbol_key(name.trim_start_matches('\\'));
    if matches!(
        key.as_str(),
        "elephc\\parallel\\run" | "elephc\\async\\run" | "fiber::suspend"
    ) {
        return Some(conservative_call_effect());
    }

    let known_user_function = with_active_function_effects(|effects| {
        effects.is_some_and(|effects| {
            effects
                .keys()
                .any(|candidate| crate::names::php_symbol_key(candidate.trim_start_matches('\\')) == key)
        })
    });
    if known_user_function || crate::builtins::registry::lookup(name).is_some() {
        Some(function_call_effect(name, &[]))
    } else {
        Some(conservative_call_effect().with_process_global_storage())
    }
}

/// Derives safe semantic argument types from literals or the checked registry signature.
fn semantic_optimizer_arg_types(
    def: &crate::builtins::registry::BuiltinDef,
    args: &[Expr],
) -> Vec<PhpType> {
    if args.is_empty() {
        return def.params.iter().map(|(_, ty)| ty.clone()).collect();
    }
    args.iter()
        .enumerate()
        .map(|(index, arg)| match &arg.kind {
            ExprKind::StringLiteral(_) => PhpType::Str,
            ExprKind::IntLiteral(_) => PhpType::Int,
            ExprKind::FloatLiteral(_) => PhpType::Float,
            ExprKind::BoolLiteral(_) => PhpType::Bool,
            ExprKind::Null => PhpType::Void,
            ExprKind::ArrayLiteral(_) => PhpType::Array(Box::new(PhpType::Mixed)),
            ExprKind::ArrayLiteralAssoc(_) => PhpType::AssocArray {
                key: Box::new(PhpType::Mixed),
                value: Box::new(PhpType::Mixed),
            },
            ExprKind::NewObject { class_name, .. } => {
                PhpType::Object(class_name.as_str().to_string())
            }
            _ => def
                .params
                .get(index)
                .map(|(_, ty)| ty.clone())
                .or_else(|| {
                    def.variadic
                        .as_ref()
                        .and_then(|_| def.params.last().map(|(_, ty)| ty.clone()))
                })
                .unwrap_or(PhpType::Mixed),
        })
        .collect()
}

/// Computes the effect for a closure body by delegating to `block_effect`.
pub(super) fn closure_body_call_effect(body: &[Stmt]) -> Effect {
    block_effect(body)
}

/// Closure invocation includes declared element binding, even for unused results.
fn closure_invocation_effect(expr: &Expr) -> Effect {
    let ExprKind::Closure { body, params, variadic_type, variadic_by_ref, return_type, .. } = &expr.kind else {
        return conservative_call_effect();
    };
    let effect = closure_body_call_effect(body).combine(super::super::return_binding_effects::effect(params, return_type, body));
    if super::super::effect_analysis::callable_binds_string(params, variadic_type.as_ref(), *variadic_by_ref) {
        effect.combine(super::super::effect_analysis::string_binding_effect())
    } else { effect }
}

/// Computes the effect for an expression that may be called at runtime.
///
/// Dispatches based on expression variant:
/// - `FirstClassCallable` → delegates to `callable_target_call_effect`
/// - `Closure` → delegates to `closure_body_call_effect`
/// - All other expressions → conservatively returns an observable, throwing call barrier
pub(in crate::optimize) fn expr_call_effect(callee: &Expr) -> Effect {
    match &callee.kind {
        ExprKind::FirstClassCallable(target) => callable_target_call_effect(target),
        ExprKind::Closure { .. } => closure_invocation_effect(callee),
        ExprKind::Variable(name) => callable_alias_effect(name),
        _ => conservative_call_effect(),
    }
}

/// Looks up the effect for a callable alias (e.g. `$f = foo;`).
///
/// Uses thread-local `ACTIVE_CALLABLE_ALIAS_EFFECTS`. Unknown aliases default to `Effect::PURE`
/// with side effects and may-throw.
pub(in crate::optimize) fn callable_alias_effect(name: &str) -> Effect {
    ACTIVE_CALLABLE_ALIAS_EFFECTS.with(|slot| {
        slot.borrow()
            .as_ref()
            .and_then(|effects| effects.get(name).copied())
    })
    .unwrap_or_else(|| conservative_call_effect().with_process_global_storage())
}

/// Computes the effect for a callable target resolved at compile time.
///
/// Variant-specific dispatch:
/// - `Function(name)` → delegates to `function_call_effect`
/// - `StaticMethod { receiver, method }` → delegates to `static_method_call_effect`
/// - `Method { object, method }` → combines `expr_effect(object)` with `instance_method_call_effect`
pub(super) fn callable_target_call_effect(target: &CallableTarget) -> Effect {
    match target {
        CallableTarget::Function(name) => function_call_effect(name.as_str(), &[]),
        CallableTarget::StaticMethod { receiver, method } => static_method_call_effect(receiver, method),
        CallableTarget::Method { object, method } => {
            expr_effect(object).combine(instance_method_call_effect(object, method))
        }
    }
}

/// Returns the effect for a closure alias expression, if the expression is a closure.
///
/// For `ExprKind::Closure`, returns `Some(closure_body_call_effect(body))`. For all other
/// variants, returns `None`.
pub(super) fn closure_alias_effect(expr: &Expr) -> Option<Effect> {
    match &expr.kind {
        ExprKind::Closure { .. } => Some(closure_invocation_effect(expr)),
        _ => None,
    }
}

/// Merges a collection of optional call effects into a single optional effect.
///
/// Combines all possible callable effects into one conservative summary.
///
/// Returns `None` when a branch cannot be resolved or when the iterator is empty; differing
/// effects are combined because a runtime-selected callable may exhibit any branch's behavior.
pub(super) fn merge_callable_value_effects(
    effects: impl IntoIterator<Item = Option<Effect>>,
) -> Option<Effect> {
    effects
        .into_iter()
        .try_fold(None, |combined, effect| {
            let effect = effect?;
            Some(Some(combined.map_or(effect, |combined: Effect| combined.combine(effect))))
        })?
}

/// Looks up the effect for a static method call.
///
/// Resolves the receiver to a class name via `resolve_static_receiver_class`. If resolution
/// fails, returns `Effect::PURE` with side effects and may-throw. Otherwise looks up the effect
/// from thread-local `ACTIVE_STATIC_METHOD_EFFECTS`, falling back to the same conservative default.
pub(in crate::optimize) fn static_method_call_effect(
    receiver: &crate::parser::ast::StaticReceiver,
    method_name: &str,
) -> Effect {
    let Some(class_name) = resolve_static_receiver_class(receiver) else {
        return Effect::PURE
            .with_side_effects()
            .with_may_throw()
            .with_writes_globals();
    };

    let effect = with_active_static_method_effects(|effects| {
        effects.and_then(|effects| effects.get(&method_effect_key(&class_name, method_name)).copied())
    })
    .unwrap_or_else(|| {
        Effect::PURE
            .with_side_effects()
            .with_may_throw()
            .with_writes_globals()
    });
    if crate::optimize::parallel_enum_class_access_is_process_global(&class_name) {
        effect.with_process_global_runtime_state()
    } else {
        effect
    }
}

/// Resolves the effect of an instance call when the receiver's runtime class set is closed.
///
/// `$this` includes every concrete declared subclass unless the class/method is final or private.
/// `new Class`, `new self`, and `new parent` have an exact runtime class; `new static` includes
/// subclasses. Unknown receiver expressions remain conservative.
pub(in crate::optimize) fn instance_method_call_effect(
    object: &Expr,
    method_name: &str,
) -> Effect {
    let Some(receiver) = resolve_instance_receiver(object, method_name) else {
        return conservative_call_effect();
    };
    let Some(targets) = instance_dispatch_targets(&receiver, method_name) else {
        return conservative_call_effect();
    };

    with_active_instance_method_effects(|effects| {
        let Some(effects) = effects else {
            return conservative_call_effect();
        };
        targets
            .iter()
            .try_fold(Effect::PURE, |combined, target| {
                effects.get(target).copied().map(|effect| combined.combine(effect))
            })
            .unwrap_or_else(conservative_call_effect)
    })
}

/// Resolves a named instance-property read for syntactically bounded receiver classes.
///
/// Untyped declared slots are non-throwing reads. Typed slots may raise the catchable
/// uninitialized-property `Error`; property hooks and `__get` inherit their method summaries.
/// Unknown layouts remain conservative; a proven missing slot keeps only its warning effect.
pub(in crate::optimize) fn instance_property_read_effect(
    object: &Expr,
    property_name: &str,
) -> Effect {
    let Some(receiver) = resolve_property_receiver(object) else {
        return conservative_call_effect();
    };
    let Some(runtime_classes) = property_receiver_runtime_classes(&receiver) else {
        return conservative_call_effect();
    };

    let property_key = property_name.to_string();
    let mut effect = Effect::PURE;
    let mut hooked = false;
    let mut needs_magic_get = false;
    let mut warns_missing = false;
    let resolved = with_active_instance_dispatch_metadata(|metadata| {
        let metadata = metadata?;
        for runtime_class in &runtime_classes {
            if !class_hierarchy_is_complete(metadata, runtime_class) {
                return None;
            }
            match resolve_instance_property(metadata, runtime_class, &property_key) {
                Some(property) => {
                    if !member_visibility_is_accessible(
                        metadata,
                        &property.declaring_class,
                        &property.visibility,
                    ) {
                        return None;
                    }
                    hooked |= property.hooked;
                    if property.typed && !property.hooked {
                        effect = effect.with_may_throw();
                    }
                }
                None if class_has_magic_get(metadata, runtime_class) => {
                    needs_magic_get = true;
                }
                None => warns_missing = true,
            }
        }
        Some(())
    });
    if resolved.is_none() {
        return conservative_call_effect();
    }
    if hooked {
        effect = effect.combine(instance_method_call_effect(
            object,
            &format!("__propget_{property_name}"),
        ));
    }
    if needs_magic_get {
        effect = effect.combine(instance_method_call_effect(object, "__get"));
    }
    if warns_missing {
        effect = effect.with_side_effects().with_may_throw().with_writes_globals();
    }
    effect
}

/// One statically-bounded receiver class set for instance effect dispatch.
struct InstanceEffectReceiver {
    class_name: String,
    exact: bool,
}

/// Resolves receiver forms for property lookup without applying method-finality rules.
fn resolve_property_receiver(object: &Expr) -> Option<InstanceEffectReceiver> {
    match &object.kind {
        ExprKind::This => {
            let class_name = ACTIVE_CLASS_EFFECT_CONTEXT
                .with(|slot| slot.borrow().as_ref().map(|context| context.class_name.clone()))?;
            let exact = with_active_instance_dispatch_metadata(|metadata| {
                metadata
                    .and_then(|metadata| metadata.classes.get(&class_name))
                    .is_some_and(|class| class.is_final)
            });
            Some(InstanceEffectReceiver { class_name, exact })
        }
        ExprKind::NewObject { class_name, .. } => Some(InstanceEffectReceiver {
            class_name: class_name.as_str().to_string(),
            exact: true,
        }),
        ExprKind::NewScopedObject { receiver, .. } => {
            let context = ACTIVE_CLASS_EFFECT_CONTEXT.with(|slot| slot.borrow().clone())?;
            // A receiver written `Box<int>::of()` names `Box` until instantiation renames
            // it, and this pass can run on a generic function's template body — which is
            // walked and then stripped, never instantiated.
            let receiver = &receiver.written_class_receiver();
            match receiver {
                // A generic receiver is instantiated into an ordinary named one before type checking;
                // a template has no class to reach through.
                crate::parser::ast::StaticReceiver::Generic(_) => unreachable!(
                    "StaticReceiver::Generic must be instantiated by generics::classes"
                ),
                crate::parser::ast::StaticReceiver::Self_ => Some(InstanceEffectReceiver {
                    class_name: context.class_name,
                    exact: true,
                }),
                crate::parser::ast::StaticReceiver::Parent => Some(InstanceEffectReceiver {
                    class_name: context.parent_name?,
                    exact: true,
                }),
                crate::parser::ast::StaticReceiver::Static => Some(InstanceEffectReceiver {
                    class_name: context.class_name,
                    exact: false,
                }),
                crate::parser::ast::StaticReceiver::Named(name) => Some(InstanceEffectReceiver {
                    class_name: name.as_str().to_string(),
                    exact: true,
                }),
            }
        }
        _ => None,
    }
}

/// Returns all concrete runtime classes represented by one property receiver.
fn property_receiver_runtime_classes(
    receiver: &InstanceEffectReceiver,
) -> Option<Vec<String>> {
    with_active_instance_dispatch_metadata(|metadata| {
        let metadata = metadata?;
        if receiver.exact {
            return metadata
                .classes
                .contains_key(&receiver.class_name)
                .then(|| vec![receiver.class_name.clone()]);
        }
        if metadata.has_dynamic_class_barrier {
            return None;
        }
        let classes = metadata
            .classes
            .iter()
            .filter(|(_, class)| !class.is_abstract)
            .filter_map(|(candidate, _)| {
                class_is_same_or_subclass(metadata, candidate, &receiver.class_name)
                    .then_some(candidate.clone())
            })
            .collect::<Vec<_>>();
        (!classes.is_empty()).then_some(classes)
    })
}

/// Resolves the visible property declaration through one runtime class's parent chain.
fn resolve_instance_property(
    metadata: &InstanceDispatchMetadata,
    runtime_class: &str,
    property_name: &str,
) -> Option<PropertyReadMetadata> {
    let mut current = Some(runtime_class);
    while let Some(class_name) = current {
        let class = metadata.classes.get(class_name)?;
        if let Some(property) = class.properties.get(property_name) {
            return Some(property.clone());
        }
        current = class.parent_name.as_deref();
    }
    None
}

/// Returns whether a runtime class inherits a concrete `__get` implementation.
fn class_has_magic_get(metadata: &InstanceDispatchMetadata, runtime_class: &str) -> bool {
    let mut current = Some(runtime_class);
    while let Some(class_name) = current {
        let Some(class) = metadata.classes.get(class_name) else {
            return false;
        };
        if class.has_magic_get {
            return true;
        }
        current = class.parent_name.as_deref();
    }
    false
}

/// Returns whether every parent/member source is locally known, rejecting unresolved trait use.
fn class_hierarchy_is_complete(
    metadata: &InstanceDispatchMetadata,
    runtime_class: &str,
) -> bool {
    let mut current = Some(runtime_class);
    while let Some(class_name) = current {
        let Some(class) = metadata.classes.get(class_name) else {
            return false;
        };
        if class.has_trait_uses {
            return false;
        }
        current = class.parent_name.as_deref();
    }
    true
}

/// Resolves syntactic receiver forms whose runtime class set is known without checker state.
fn resolve_instance_receiver(
    object: &Expr,
    method_name: &str,
) -> Option<InstanceEffectReceiver> {
    match &object.kind {
        ExprKind::This => {
            let class_name = ACTIVE_CLASS_EFFECT_CONTEXT
                .with(|slot| slot.borrow().as_ref().map(|context| context.class_name.clone()))?;
            let exact = instance_member_is_statically_bound(&class_name, method_name);
            Some(InstanceEffectReceiver { class_name, exact })
        }
        ExprKind::NewObject { class_name, .. } => Some(InstanceEffectReceiver {
            class_name: class_name.as_str().to_string(),
            exact: true,
        }),
        ExprKind::NewScopedObject { receiver, .. } => {
            let context = ACTIVE_CLASS_EFFECT_CONTEXT.with(|slot| slot.borrow().clone())?;
            // A receiver written `Box<int>::of()` names `Box` until instantiation renames
            // it, and this pass can run on a generic function's template body — which is
            // walked and then stripped, never instantiated.
            let receiver = &receiver.written_class_receiver();
            match receiver {
                // A generic receiver is instantiated into an ordinary named one before type checking;
                // a template has no class to reach through.
                crate::parser::ast::StaticReceiver::Generic(_) => unreachable!(
                    "StaticReceiver::Generic must be instantiated by generics::classes"
                ),
                crate::parser::ast::StaticReceiver::Self_ => Some(InstanceEffectReceiver {
                    class_name: context.class_name,
                    exact: true,
                }),
                crate::parser::ast::StaticReceiver::Parent => Some(InstanceEffectReceiver {
                    class_name: context.parent_name?,
                    exact: true,
                }),
                crate::parser::ast::StaticReceiver::Static => Some(InstanceEffectReceiver {
                    exact: instance_member_is_statically_bound(&context.class_name, method_name),
                    class_name: context.class_name,
                }),
                crate::parser::ast::StaticReceiver::Named(name) => Some(InstanceEffectReceiver {
                    class_name: name.as_str().to_string(),
                    exact: true,
                }),
            }
        }
        _ => None,
    }
}

/// Returns whether class/method modifiers make `$this` or `new static` dispatch exact.
fn instance_member_is_statically_bound(class_name: &str, method_name: &str) -> bool {
    let method_key = php_symbol_key(method_name);
    with_active_instance_dispatch_metadata(|metadata| {
        let Some(metadata) = metadata else {
            return false;
        };
        let Some(class) = metadata.classes.get(class_name) else {
            return false;
        };
        class.is_final
            || class.private_methods.contains(&method_key)
            || class.final_methods.contains(&method_key)
    })
}

/// Expands one receiver class to the concrete method implementations reachable at runtime.
fn instance_dispatch_targets(
    receiver: &InstanceEffectReceiver,
    method_name: &str,
) -> Option<Vec<String>> {
    let method_key = php_symbol_key(method_name);
    with_active_instance_dispatch_metadata(|metadata| {
        let metadata = metadata?;
        if !receiver.exact && metadata.has_dynamic_class_barrier {
            return None;
        }
        let runtime_classes = if receiver.exact {
            vec![receiver.class_name.clone()]
        } else {
            metadata
                .classes
                .iter()
                .filter(|(_, class)| !class.is_abstract)
                .filter_map(|(candidate, _)| {
                    class_is_same_or_subclass(metadata, candidate, &receiver.class_name)
                        .then_some(candidate.clone())
                })
                .collect()
        };
        if runtime_classes.is_empty() {
            return None;
        }

        let mut targets = Vec::new();
        for runtime_class in runtime_classes {
            if !class_hierarchy_is_complete(metadata, &runtime_class) {
                return None;
            }
            let implementation =
                resolve_instance_method_implementation(metadata, &runtime_class, &method_key)?;
            if !instance_method_target_is_accessible(metadata, &implementation) {
                return None;
            }
            if !targets.contains(&implementation) {
                targets.push(implementation);
            }
        }
        Some(targets)
    })
}

/// Checks PHP instance-method visibility from the active lexical class scope.
fn instance_method_target_is_accessible(
    metadata: &InstanceDispatchMetadata,
    target: &str,
) -> bool {
    let Some((declaring_class, method_key)) = target.rsplit_once("::") else {
        return false;
    };
    let Some(visibility) = metadata
        .classes
        .get(declaring_class)
        .and_then(|class| class.method_visibilities.get(method_key))
    else {
        return false;
    };
    member_visibility_is_accessible(metadata, declaring_class, visibility)
}

/// Checks PHP member visibility from the active lexical class scope.
fn member_visibility_is_accessible(
    metadata: &InstanceDispatchMetadata,
    declaring_class: &str,
    visibility: &crate::parser::ast::Visibility,
) -> bool {
    match visibility {
        crate::parser::ast::Visibility::Public => true,
        crate::parser::ast::Visibility::Private => ACTIVE_CLASS_EFFECT_CONTEXT.with(|slot| {
            slot.borrow()
                .as_ref()
                .is_some_and(|context| context.class_name == declaring_class)
        }),
        crate::parser::ast::Visibility::Protected => ACTIVE_CLASS_EFFECT_CONTEXT.with(|slot| {
            slot.borrow().as_ref().is_some_and(|context| {
                class_is_same_or_subclass(metadata, &context.class_name, declaring_class)
                    || class_is_same_or_subclass(metadata, declaring_class, &context.class_name)
            })
        }),
    }
}

/// Returns true when `candidate` is `base` or inherits from it.
fn class_is_same_or_subclass(
    metadata: &InstanceDispatchMetadata,
    candidate: &str,
    base: &str,
) -> bool {
    let mut current = Some(candidate);
    while let Some(class_name) = current {
        if class_name == base {
            return true;
        }
        current = metadata
            .classes
            .get(class_name)
            .and_then(|class| class.parent_name.as_deref());
    }
    false
}

/// Resolves the concrete class body implementing one inherited instance method.
fn resolve_instance_method_implementation(
    metadata: &InstanceDispatchMetadata,
    runtime_class: &str,
    method_key: &str,
) -> Option<String> {
    let mut current = Some(runtime_class);
    while let Some(class_name) = current {
        let key = method_effect_key(class_name, method_key);
        let has_body = with_active_instance_method_effects(|effects| {
            effects.is_some_and(|effects| effects.contains_key(&key))
        });
        if has_body {
            return Some(key);
        }
        current = metadata
            .classes
            .get(class_name)
            .and_then(|class| class.parent_name.as_deref());
    }
    None
}

/// Resolves a static receiver to a class name string.
///
/// - `StaticReceiver::Named` → returns the class name directly
/// - `StaticReceiver::Self_` → looks up the current class name from `ACTIVE_CLASS_EFFECT_CONTEXT`
/// - `StaticReceiver::Parent` → looks up the parent class name from `ACTIVE_CLASS_EFFECT_CONTEXT`
/// - `StaticReceiver::Static` → returns `None` (cannot resolve without more context)
pub(super) fn resolve_static_receiver_class(receiver: &crate::parser::ast::StaticReceiver) -> Option<String> {
    // A receiver written `Box<int>::of()` names `Box` until instantiation renames
    // it, and this pass can run on a generic function's template body — which is
    // walked and then stripped, never instantiated.
    let receiver = &receiver.written_class_receiver();
    match receiver {
        // A generic receiver is instantiated into an ordinary named one before type checking;
        // a template has no class to reach through.
        crate::parser::ast::StaticReceiver::Generic(_) => unreachable!(
            "StaticReceiver::Generic must be instantiated by generics::classes"
        ),
        crate::parser::ast::StaticReceiver::Named(class_name) => Some(class_name.as_str().to_string()),
        crate::parser::ast::StaticReceiver::Self_ => ACTIVE_CLASS_EFFECT_CONTEXT
            .with(|slot| slot.borrow().as_ref().map(|context| context.class_name.clone())),
        crate::parser::ast::StaticReceiver::Parent => ACTIVE_CLASS_EFFECT_CONTEXT.with(|slot| {
            slot.borrow()
                .as_ref()
                .and_then(|context| context.parent_name.clone())
        }),
        crate::parser::ast::StaticReceiver::Static => None,
    }
}
