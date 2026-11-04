//! Purpose:
//! Infers object methods expression types.
//! Validates class, method, constructor, property, and magic-access contracts against schema metadata.
//!
//! Called from:
//! - `crate::types::checker::inference::objects`
//!
//! Key details:
//! - Object inference depends on flattened class metadata, visibility, inheritance, and declared property types.

use std::collections::{HashMap, HashSet};

use crate::errors::{CompileError, RuntimeFault};
use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, ExprKind, StaticReceiver, TypeExpr};
use crate::types::{FunctionSig, PhpType, TypeEnv};

use super::super::super::{Checker, MethodOwnerIndex};
use super::super::syntactic::wider_type_syntactic;

impl Checker {
    /// Infers the type of a method call expression (`$obj->method(...)`).
    ///
    /// Dispatches to `infer_method_call_on_class_type` for `Object` types,
    /// `infer_method_call_on_interface_type` for interface types, and
    /// handles nullable union receivers. A `Mixed` receiver dispatches at
    /// runtime over the classes that declare the method, so its result is the
    /// union of those candidates' return types (see
    /// `mixed_receiver_method_return_type`). Other unhandled receiver types fall
    /// back to `PhpType::Int`.
    pub(crate) fn infer_method_call_type(
        &mut self,
        object: &Expr,
        method: &str,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let obj_ty = self.infer_type(object, env)?;
        self.record_member_receiver_type(expr.span, &obj_ty);
        if let PhpType::Object(class_name) = &obj_ty {
            if class_name.is_empty() {
                for arg in args {
                    self.infer_type(arg, env)?;
                }
                return Ok(self
                    .mixed_receiver_method_return_type(method, args.len())
                    .unwrap_or(PhpType::Mixed));
            }
            if self.interfaces.contains_key(class_name) {
                return self
                    .infer_method_call_on_interface_type(class_name, method, args, expr, env);
            }
            let return_ty = self.infer_method_call_on_class_type(class_name, method, args, expr, env)?;
            return Ok(self
                .tracked_reflection_class_method_return_type(object, method)
                .unwrap_or(return_ty));
        }
        // Method calls on a union object type are allowed when the union has a
        // single object class. `?Foo` / `Foo|null` faults on a null receiver as in
        // PHP; `Foo|false` (and other object-plus-scalar unions) dispatch on the
        // runtime class id and fault when the value is not an object. Either way
        // the checker surfaces the method's return type so callers can chain.
        if let PhpType::Union(_) = &obj_ty {
            let class_name = self.union_single_object_class(&obj_ty).or_else(|| {
                self.nullsafe_object_receiver(&obj_ty, expr, "method call")
                    .ok()
                    .flatten()
                    .map(|(name, _nullable)| name)
            });
            if let Some(class_name) = class_name {
                if self.interfaces.contains_key(&class_name) {
                    return self.infer_method_call_on_interface_type(
                        &class_name,
                        method,
                        args,
                        expr,
                        env,
                    );
                }
                let return_ty =
                    self.infer_method_call_on_class_type(&class_name, method, args, expr, env)?;
                return Ok(self
                    .tracked_reflection_class_method_return_type(object, method)
                    .unwrap_or(return_ty));
            }
            // Union of two or more distinct object classes (`A|B`, `A|B|false`):
            // codegen dispatches on the runtime class id. Validate every member
            // that can receive the call and merge their return types; a runtime
            // member without that method faults like PHP instead of making the
            // entire union a compile-time error.
            let object_classes = self.union_object_classes(&obj_ty);
            if object_classes.len() >= 2 {
                let mut return_types = Vec::with_capacity(object_classes.len());
                for class_name in &object_classes {
                    let supports_method = self
                        .classes
                        .get(class_name)
                        .is_some_and(|info| {
                            info.methods.contains_key(&php_symbol_key(method))
                                || info.methods.contains_key("__call")
                        })
                        || self
                            .interfaces
                            .get(class_name)
                            .is_some_and(|info| {
                                info.methods.contains_key(&php_symbol_key(method))
                            });
                    if !supports_method {
                        continue;
                    }
                    let return_ty = if self.interfaces.contains_key(class_name) {
                        self.infer_method_call_on_interface_type(class_name, method, args, expr, env)?
                    } else {
                        self.infer_method_call_on_class_type(class_name, method, args, expr, env)?
                    };
                    return_types.push(return_ty);
                }
                if !return_types.is_empty() {
                    return Ok(self.normalize_union_type(return_types));
                }
                // No union MEMBER declares the method, but a concrete class implementing them may:
                // Symfony's `Router` declares
                // `protected UrlMatcherInterface|RequestMatcherInterface $matcher;` and calls
                // `addExpressionLanguageProvider()` on it behind a `method_exists()` guard —
                // declared on neither interface, and PHP resolves it on the runtime object.
                // Codegen dispatches on the runtime class id exactly as it does for the members,
                // so the implementors that DO declare it decide the return type, and one that does
                // not faults at run time the way PHP does.
                let implementor_returns =
                    self.union_implementor_method_return_types(&object_classes, method, args, expr, env)?;
                if !implementor_returns.is_empty() {
                    return Ok(self.normalize_union_type(implementor_returns));
                }
                // No member and no implementor declares it: PHP raises "Call to undefined
                // method" when the call RUNS, and the call is often guarded by a check this
                // checker does not track. Symfony's `ContextListener` calls
                // `$refreshedUser->getSalt()` on `App\Entity\User|InMemoryUser` behind an
                // `instanceof LegacyPasswordAuthenticatedUserInterface` test on ANOTHER variable;
                // refusing it left the firewall's context listener to the interpreter. The boxed
                // dispatch has no candidate and faults at run time, as PHP does.
                self.record_unresolved_callee_argument_aliases(args);
                for arg in args {
                    self.infer_type(arg, env)?;
                }
                return Ok(PhpType::Mixed);
            }
            // The closed world only gets to refuse a call when it actually knows the receiver.
            // A union naming a class this build never compiled says nothing about what the
            // object has: Symfony's `RedisTrait` declares
            // `\Redis|Relay\Relay|Relay\Cluster|\RedisArray|\RedisCluster|\Predis\ClientInterface`
            // and none of those extensions or packages is installed here, so every member is
            // absent and no implementor can be found. PHP dispatches on the runtime class, and
            // so does codegen — the union is boxed, so this lowers through the same Mixed
            // receiver path, over exactly the classes that declare the method.
            let names_a_class_outside_the_build = object_classes.iter().any(|class_name| {
                !self.classes.contains_key(class_name) && !self.interfaces.contains_key(class_name)
            });
            if names_a_class_outside_the_build {
                self.record_unresolved_callee_argument_aliases(args);
                for arg in args {
                    self.infer_type(arg, env)?;
                }
                return Ok(self
                    .mixed_receiver_method_return_type(method, args.len())
                    .unwrap_or(PhpType::Mixed));
            }
            // No object class at all (`null|int`, inferred for Laravel's untyped
            // `Worker::$currentJob` from its call sites): the inference is a hint, not the
            // run-time value, and php resolves the call on whatever arrives, raising "Call to a
            // member function m() on int" when it is not an object. The boxed union dispatches
            // through the Mixed receiver path, which raises exactly that.
            if object_classes.is_empty() {
                self.record_unresolved_callee_argument_aliases(args);
                for arg in args {
                    self.infer_type(arg, env)?;
                }
                return Ok(self
                    .mixed_receiver_method_return_type(method, args.len())
                    .unwrap_or(PhpType::Mixed));
            }
            self.nullsafe_object_receiver(&obj_ty, expr, "method call")?;
        }
        // Closure rebinding methods on a callable receiver. `bindTo` rebinds
        // `$this` and returns a new closure; `call` binds `$this` and invokes the
        // closure in one step, returning its result. `$scope` is accepted and
        // ignored (visibility is resolved at compile time).
        if matches!(obj_ty, PhpType::Callable) {
            match php_symbol_key(method).as_str() {
                "bindto" => {
                    for arg in args {
                        self.infer_type(arg, env)?;
                    }
                    return Ok(PhpType::Callable);
                }
                // `$f->__invoke(...)` is the closure call written the long way. Its result is
                // whatever the closure returns, which is not resolvable from the receiver's
                // `callable` type, and its arguments go to a callee this does not know — so they
                // are conservatively reference-aliased, exactly as `call` does below.
                "__invoke" => {
                    self.record_unresolved_callee_argument_aliases(args);
                    for arg in args {
                        self.infer_type(arg, env)?;
                    }
                    return Ok(PhpType::Mixed);
                }
                "call" => {
                    // `call` INVOKES the receiver closure with `$args[1..]`, and the receiver's
                    // signature is not resolvable here — the arguments are conservatively
                    // reference-aliased. (`bindTo` above only rebinds `$this`; it invokes
                    // nothing, so its arguments are ordinary by-value reads.)
                    self.record_unresolved_callee_argument_aliases(args);
                    for arg in args {
                        self.infer_type(arg, env)?;
                    }
                    return Ok(PhpType::Mixed);
                }
                _ => {}
            }
        }
        // A method call on a `mixed` receiver dispatches on the runtime class id
        // over exactly the classes that declare the method (see
        // `mixed_method_candidates` / `lower_mixed_method_call` and the
        // Mixed-receiver method emission in `ir_lower::program`). The static
        // result is therefore the union of those candidates' return types. When
        // they all agree on one type, codegen stores the call result raw (no
        // boxing), so the precise type is correct; when they differ it is a
        // union, which codegen boxes like the two-class union dispatch. Returning
        // the historical `Int` fallback here instead made an *inferred* function
        // return type silently coerce a boxed result: an un-annotated
        // `function f($x) { return $x->name(); }` rendered the returned string as
        // `0`. With no declaring class the runtime would fatal, so `mixed` is the
        // safe static result.
        if matches!(obj_ty, PhpType::Mixed) {
            // A Reflection owner is one of those candidates, so the `getAttributes()` flag guard
            // applies here as well.
            self.reject_unsupported_reflection_attribute_filter_flags(
                None,
                &php_symbol_key(method),
                args,
                expr.span,
            )?;
            // The callee is picked from the runtime class id over every candidate that declares
            // the method, so no single parameter list governs the arguments: any candidate's
            // by-reference parameter would alias the local behind them.
            self.record_unresolved_callee_argument_aliases(args);
            return Ok(self
                .mixed_receiver_method_return_type(method, args.len())
                .unwrap_or(PhpType::Mixed));
        }
        self.record_unresolved_callee_argument_aliases(args);
        // A receiver that is none of the shapes above — a scalar, an array, a `void` — cannot be
        // dispatched on, and PHP fatals when it is reached. `Int` was the historical answer and it
        // is a fabrication for exactly the reason the `Mixed` arm above spells out: it is
        // indistinguishable from a real `int`, and a chain built on it hands the backend a raw
        // integer where a receiver belongs. Symfony's `PdoAdapter::doFetch` is the case — `PDO` is
        // absent from the build, `$stmt->execute()` answered `Int`, and the next link,
        // `$result->iterateNumeric()`, reached codegen as `method call receiver for PHP type Int`.
        Ok(PhpType::Mixed)
    }

    /// Computes the static return type of a method call on a `mixed` receiver as
    /// the normalized union of the declared return types of every class that
    /// declares `method` with a matching arity. This mirrors the runtime
    /// candidate set used by `mixed_method_candidates` in codegen, so the
    /// inferred type stays consistent with how each candidate branch stores its
    /// result. Nominal object results degrade to `mixed`, because a runtime-only
    /// class may implement the same method with an unrelated object result.
    /// Falls back to the name-only candidate set when arity filtering finds
    /// nothing (e.g. methods with default parameters), and returns `None` when no
    /// class declares the method at all.
    fn mixed_receiver_method_return_type(&self, method: &str, arg_count: usize) -> Option<PhpType> {
        let method_key = php_symbol_key(method);
        let owners = self.method_owners_of(&method_key)?;
        let mut arity_matched: Vec<PhpType> = Vec::new();
        for class_name in owners.iter() {
            let Some(sig) = self
                .classes
                .get(class_name)
                .and_then(|class_info| class_info.methods.get(&method_key))
            else {
                continue;
            };
            if !Self::signature_accepts_argument_count(sig, arg_count) {
                continue;
            }
            let ty = sig.return_type.clone();
            if !arity_matched.contains(&ty) {
                arity_matched.push(ty);
            }
        }
        // A class that lacks the method but declares `__call` answers it too, with whatever its
        // `__call` returns. Carbon's `nthOfQuarter()` chains `->day(1)->month(...)` through
        // `__call`, and the one declared `month()` in the Laravel world was Faker's `: string`,
        // so `$date->month` was refused as a property read on a string and Carbon went to the
        // interpreter.
        if !arity_matched.is_empty()
            && self.method_owners_of("__call").is_some_and(|magic| {
                magic.iter().any(|class_name| !owners.contains(class_name))
            })
        {
            return Some(PhpType::Mixed);
        }
        self.mixed_receiver_candidates_type(arity_matched)
    }

    /// The classes declaring instance method `method_key`, in `classes` iteration order.
    ///
    /// The scans that ask this used to visit every class of the program per call, and the method
    /// pass repeats those calls on every round: on the Symfony build the mixed-receiver scan alone
    /// was 1.2 million calls and 23% of type checking. Which classes DECLARE a method is fixed
    /// once the class table is built (the pass changes signatures, which callers read fresh), so
    /// the owners are indexed per table, in the table's own order: the order the scans visited.
    pub(crate) fn method_owners_of(&self, method_key: &str) -> Option<std::sync::Arc<[String]>> {
        let mut index = self.method_owners.borrow_mut();
        if index.as_ref().is_none_or(|index| index.class_count != self.classes.len()) {
            let mut owners: crate::fast_hash::FastMap<String, Vec<String>> =
                crate::fast_hash::FastMap::default();
            for (class_name, class_info) in &self.classes {
                for key in class_info.methods.keys() {
                    owners.entry(key.clone()).or_default().push(class_name.clone());
                }
            }
            *index = Some(MethodOwnerIndex {
                class_count: self.classes.len(),
                owners: owners
                    .into_iter()
                    .map(|(key, classes)| (key, std::sync::Arc::from(classes)))
                    .collect(),
            });
        }
        index.as_ref().and_then(|index| index.owners.get(method_key)).cloned()
    }

    /// Turns the arity-matched return types of same-named methods into the call's type.
    fn mixed_receiver_candidates_type(&self, arity_matched: Vec<PhpType>) -> Option<PhpType> {
        // A candidate that cannot be CALLED with this many arguments is not a candidate. The
        // fallback to `any_matched` existed because the arity test used to compare `params.len()`
        // exactly and was therefore empty far too often; now that it accepts the callable range,
        // falling back means answering with a signature the call could never reach. Symfony's
        // `PdoAdapter::doFetch` is the case: `$stmt->execute()` on a gradual receiver matched
        // `Console\Command\Command::execute($input, $output): int` — two required parameters, a
        // class with nothing to do with a statement — so `$result` became `int` and the next link,
        // `$result->iterateNumeric()`, reached the backend as `method call receiver for PHP type
        // Int`.
        let candidates = arity_matched;
        if candidates.is_empty() {
            None
        } else {
            let normalized = self.normalize_union_type(candidates);
            match &normalized {
                // One candidate's class is not the receiver's, so a precise object type would name
                // the wrong one.
                PhpType::Object(_) => Some(PhpType::Mixed),
                // Candidates that DISAGREE are not a fact about this call — they are methods that
                // share a name. Symfony's `--web` build unioned every `write()` in the program into
                // `null|bool` for a `Twig\Compiler` receiver whose class is absent from the closed
                // world (its own `write()` returns `$this` and was never a candidate), and the next
                // link in the chain was refused against that. Codegen boxes a union exactly as it
                // boxes a Mixed, so only the static reading changes.
                PhpType::Union(_) => Some(PhpType::Mixed),
                // `void` is no value at all, so it cannot be what a gradual receiver's call
                // evaluates to: the class that answers may be one whose same-named method returns
                // something. Twig's `CoreExtension::include()` returns `$loaded->render(...)`; with
                // `$loaded` gradual, every `render()` the world declared with one parameter was a
                // `: void` one (the console's `Table::render()`), and the method was refused for
                // returning "void" from `: string`, which sent all of `CoreExtension` to the
                // interpreter.
                PhpType::Void => Some(PhpType::Mixed),
                _ => Some(normalized),
            }
        }
    }

    /// Returns whether a signature can be CALLED with `arg_count` arguments.
    ///
    /// The arity filter above used to test `params.len() == arg_count`, which is the total
    /// parameter count, not the callable range. A method with an optional parameter was therefore
    /// excluded from its own call: `$pool->clear()` on a gradual receiver skipped every
    /// `clear(string $prefix = '')` in the program and matched only the unrelated zero-parameter
    /// `clear(): void` methods that happen to share the name. Symfony's `ProxyAdapter::clear()`
    /// reads `$this->pool` — declared `private object $pool` in `ProxyTrait`, so fully gradual —
    /// and the call came back `void` against its own declared `: bool`.
    ///
    /// A variadic tail takes any number beyond the fixed parameters, and `defaults` gives the
    /// first optional position, so the accepted range is `[required, params.len()]` — exactly
    /// what PHP checks at the call.
    fn signature_accepts_argument_count(sig: &crate::types::FunctionSig, arg_count: usize) -> bool {
        // The variadic tail is never required. Counting it made every method that carries the
        // hidden `func_get_args()` collector demand one argument more than it declares, so in a
        // world where all of them carry it, `$this->extensionSet->getExtension($class)` on Twig's
        // untyped property kept only the translation dumpers' `getExtension(): string` -- they
        // have no declared parameter for the collector to follow -- and `Environment` was refused
        // for returning a string. Same rule as lowering's
        // `ir_lower::expr::method_metadata::signature_accepts_argument_count`.
        let fixed = if sig.variadic.is_some() {
            sig.params.len().saturating_sub(1)
        } else {
            sig.params.len()
        };
        let required = sig
            .defaults
            .iter()
            .take(fixed)
            .position(|default| default.is_some())
            .unwrap_or(fixed);
        if arg_count < required {
            return false;
        }
        sig.variadic.is_some() || arg_count <= sig.params.len()
    }

    /// Returns a concrete reflected object type for tracked `ReflectionClass` construction helpers.
    fn tracked_reflection_class_method_return_type(
        &self,
        object: &Expr,
        method: &str,
    ) -> Option<PhpType> {
        let ExprKind::Variable(name) = &object.kind else {
            return None;
        };
        let reflected_class = self.reflection_class_targets.get(name)?;
        match php_symbol_key(method).as_str() {
            "newinstance" | "newinstanceargs" | "newinstancewithoutconstructor" => {
                Some(PhpType::Object(reflected_class.clone()))
            }
            _ => None,
        }
    }

    /// Infers a method return through the interface or concrete-class checker for a resolved
    /// receiver name.
    fn infer_method_return_on_class_or_interface(
        &mut self,
        class_name: &str,
        method: &str,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        if self.interfaces.contains_key(class_name) {
            self.infer_method_call_on_interface_type(class_name, method, args, expr, env)
        } else {
            self.infer_method_call_on_class_type(class_name, method, args, expr, env)
        }
    }

    /// Infers the type of a nullsafe method call expression (`$obj?->method(...)`).
    ///
    /// A union with one object class keeps that class available for method validation even when
    /// gradual scalar members are also present. A nullable receiver adds `Void` to the result;
    /// non-object runtime members retain PHP's runtime failure behavior.
    pub(crate) fn infer_nullsafe_method_call_type(
        &mut self,
        object: &Expr,
        method: &str,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let obj_ty = self.infer_type(object, env)?;
        self.record_member_receiver_type(expr.span, &obj_ty);
        if matches!(obj_ty, PhpType::Mixed)
            || matches!(&obj_ty, PhpType::Union(members) if members.iter().any(|member| *member == PhpType::Mixed))
        {
            return Ok(PhpType::Mixed);
        }
        // Several classes or null (`\Dom\NamedNodeMap|\DOMNamedNodeMap|null`, Symfony's
        // `DomVisitor`): the union is a boxed value, so the call takes the `mixed` receiver's
        // run-time dispatch, null short-circuiting as php does.
        if let PhpType::Union(members) = &obj_ty {
            let mut classes = members.iter().filter_map(|member| match member {
                PhpType::Object(class) => Some(php_symbol_key(class.trim_start_matches('\\'))),
                _ => None,
            });
            let first = classes.next();
            let several = first.is_some_and(|first| classes.any(|other| other != first));
            if several
                && members
                    .iter()
                    .all(|member| matches!(member, PhpType::Object(_) | PhpType::Void))
            {
                self.record_unresolved_callee_argument_aliases(args);
                for arg in args {
                    self.infer_type(arg, env)?;
                }
                return Ok(PhpType::Mixed);
            }
        }
        // A receiver that cannot be an object: php raises "Call to a member function m() on
        // string" when the line runs, exactly as for `->` (see `infer_method_call_type`). The type
        // is often a guess from an incomplete world -- `$param->getDescription()?->render()` in
        // Symfony's `PhpDocExtractor` reads `getDescription()` from the program's only declaring
        // class because phpdocumentor is not installed -- and refusing it sent the file to the
        // interpreter for a line that never runs.
        if matches!(
            obj_ty,
            PhpType::Str
                | PhpType::Int
                | PhpType::Float
                | PhpType::Bool
                | PhpType::False
                | PhpType::Array(_)
                | PhpType::AssocArray { .. }
        ) {
            self.record_unresolved_callee_argument_aliases(args);
            for arg in args {
                self.infer_type(arg, env)?;
            }
            return Ok(PhpType::Mixed);
        }
        if matches!(&obj_ty, PhpType::Union(_)) {
            if let Some(class_name) = self.union_single_object_class(&obj_ty) {
                let return_ty = self.infer_method_return_on_class_or_interface(
                    &class_name,
                    method,
                    args,
                    expr,
                    env,
                )?;
                let return_ty = self
                    .tracked_reflection_attribute_new_instance_return_type(object, method)
                    .unwrap_or(return_ty);
                let nullable = matches!(&obj_ty, PhpType::Union(members)
                    if members.iter().any(|member| *member == PhpType::Void));
                return if nullable {
                    Ok(self.normalize_union_type(vec![return_ty, PhpType::Void]))
                } else {
                    Ok(return_ty)
                };
            }
        }
        // `$f?->__invoke(...)` over a `?\Closure` property: a callable is not an object class, so
        // the object-receiver rule below cannot describe it, and PHP runs this every day —
        // Symfony's `TraceableAdapter` guards each traced operation with
        // `$this->disabled?->__invoke()`. It is the closure call with a null guard in front, so
        // the result is the closure's, plus null from the guarded arm.
        if php_symbol_key(method) == "__invoke" && Self::is_nullable_callable_type(&obj_ty) {
            self.record_unresolved_callee_argument_aliases(args);
            for arg in args {
                self.infer_type(arg, env)?;
            }
            return Ok(PhpType::Mixed);
        }
        let Some((class_name, nullable)) =
            self.nullsafe_object_receiver(&obj_ty, expr, "method call")?
        else {
            return Ok(PhpType::Void);
        };
        let return_ty = self.infer_method_return_on_class_or_interface(
            &class_name,
            method,
            args,
            expr,
            env,
        )?;
        let return_ty = self
            .tracked_reflection_attribute_new_instance_return_type(object, method)
            .unwrap_or(return_ty);
        if nullable {
            Ok(self.normalize_union_type(vec![return_ty, PhpType::Void]))
        } else {
            Ok(return_ty)
        }
    }

    /// Returns whether a type is a callable that may also be null (`?\Closure`, `callable|null`).
    fn is_nullable_callable_type(php_type: &PhpType) -> bool {
        let PhpType::Union(members) = php_type else {
            return false;
        };
        let mut callable = false;
        let mut nullable = false;
        for member in members {
            match member {
                PhpType::Callable => callable = true,
                PhpType::Void => nullable = true,
                _ => return false,
            }
        }
        callable && nullable
    }

    /// Recovers the concrete result of `ReflectionAttribute::newInstance()` from a filtered
    /// `getAttributes(ClassName::class)` receiver chain.
    ///
    /// The synthetic reflection signature is necessarily `mixed`, but a selected attribute whose
    /// filter name is statically known can only instantiate that class. This covers both direct
    /// array access and the common `($r->getAttributes(Foo::class)[0] ?? null)?->newInstance()`
    /// shape while leaving unfiltered or dynamic reflection attributes conservative.
    fn tracked_reflection_attribute_new_instance_return_type(
        &self,
        object: &Expr,
        method: &str,
    ) -> Option<PhpType> {
        if php_symbol_key(method) != "newinstance" {
            return None;
        }
        let selected = match &object.kind {
            ExprKind::NullCoalesce { value, .. } | ExprKind::ShortTernary { value, .. } => {
                value.as_ref()
            }
            _ => object,
        };
        let ExprKind::ArrayAccess { array, .. } = &selected.kind else {
            return None;
        };
        let ExprKind::MethodCall {
            method: owner_method,
            args,
            ..
        } = &array.kind
        else {
            return None;
        };
        if php_symbol_key(owner_method) != "getattributes" {
            return None;
        }
        let filter = args.first()?;
        let filter = match &filter.kind {
            ExprKind::NamedArg { value, .. } => value.as_ref(),
            _ => filter,
        };
        let class_name = match &filter.kind {
            ExprKind::StringLiteral(name) => name.trim_start_matches('\\').to_string(),
            ExprKind::ClassConstant { receiver } => match receiver {
                // A generic receiver is instantiated into an ordinary named one before type checking;
                // a template has no class to reach through.
                StaticReceiver::Generic(_) => unreachable!(
                    "StaticReceiver::Generic must be instantiated by generics::classes"
                ),
                StaticReceiver::Named(name) => name.as_canonical(),
                StaticReceiver::Self_ | StaticReceiver::Static => self.current_class.clone()?,
                StaticReceiver::Parent => {
                    let current = self.current_class.as_ref()?;
                    self.classes.get(current)?.parent.clone()?
                }
            },
            _ => return None,
        };
        Some(PhpType::Object(class_name))
    }

    /// Infers `$obj?->$method(...)` when the method name is known only at runtime.
    ///
    /// The receiver must still be an object-or-null like other nullsafe member
    /// accesses. The dynamic target prevents method signature validation, so the
    /// return type is `Mixed`; method-name and argument expressions are inferred
    /// only when the receiver is not statically null.
    pub(crate) fn infer_nullsafe_dynamic_method_call_type(
        &mut self,
        object: &Expr,
        method: &Expr,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let obj_ty = self.infer_type(object, env)?;
        let Some((_class_name, _nullable)) =
            self.nullsafe_object_receiver(&obj_ty, expr, "method call")?
        else {
            return Ok(PhpType::Void);
        };
        // A runtime method name resolves no signature, so the arguments are conservatively
        // reference-aliased — see `Checker::record_unresolved_callee_argument_aliases`.
        self.record_unresolved_callee_argument_aliases(args);
        self.infer_type(method, env)?;
        for arg in args {
            self.infer_type(arg, env)?;
        }
        Ok(PhpType::Mixed)
    }

    /// Infers the type of a method call on an interface type.
    ///
    /// Looks up the method in the interface schema, validates arguments via
    /// `normalize_named_call_args` and `check_known_callable_call`, and
    /// returns the declared return type.
    pub(crate) fn infer_method_call_on_interface_type(
        &mut self,
        interface_name: &str,
        method: &str,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let method_key = php_symbol_key(method);
        let resolved_method = self
            .interfaces
            .get(interface_name)
            .and_then(|interface_info| {
                interface_info
                    .methods
                    .get(&method_key)
                    .map(|sig| (sig, false))
                    .or_else(|| {
                        interface_info
                            .static_methods
                            .get(&method_key)
                            .map(|sig| (sig, true))
                    })
            })
            .map(|(sig, is_static)| (sig.clone(), is_static));
        let Some((sig, is_static)) = resolved_method else {
            return self.infer_lenient_subtype_method_call(
                interface_name,
                method,
                &method_key,
                args,
                expr,
                env,
                true,
            );
        };
        // The interface governs shared calls; enum-specific extensions are allowed when
        // any concrete signature accepts them. The selected runtime branch owns binding.
        let implementations: std::sync::Arc<[String]> = if is_static {
            std::sync::Arc::from(Vec::new())
        } else {
            self.direct_interface_implementers(interface_name)
        };
        let all_enums = !implementations.is_empty()
            && implementations.iter().all(|name| self.enums.contains_key(name));
        // Signatures only for the all-enum branch that reads them, taken before the spread
        // promotion below as before: cloning every implementation's signature on every interface
        // call was a top checker frame on Symfony.
        let implementation_sigs: Vec<(String, Option<FunctionSig>)> = if all_enums {
            implementations.iter()
                .map(|name| {
                    #[cfg(test)]
                    INTERFACE_IMPLEMENTATION_SIG_COPIES.with(|count| count.set(count.get() + 1));
                    (name.clone(), self.classes.get(name).and_then(|info| info.methods.get(&method_key)).cloned())
                })
                .collect()
        } else {
            Vec::new()
        };
        if args.iter().any(|arg| matches!(arg.kind, ExprKind::Spread(_)))
            && crate::types::call_args::expand_planned_positional_spreads(args).is_none()
        {
            // Runtime enum unpacks can retain named tail keys, even when the
            // interface also has ordinary class implementations. Recheck enum
            // bodies against boxed collectors without changing source element hints.
            for name in implementations.iter() {
                if self.enums.contains_key(name) {
                    self.promote_descriptor_variadic_container_for_method(name, &method_key, false);
                }
            }
        }
        if all_enums {
            let shared = self.check_user_declared_call(
                &sig, args, expr.span, env,
                &format!("Method {interface_name}::{method}"), interface_name,
            );
            if let Err(shared_error) = shared {
                let mut accepted = false;
                let mut missing = false;
                let mut runtime_shortage = false;
                let mut satisfiable_error = None;
                for (name, implementation) in implementation_sigs {
                    let Some(implementation) = implementation else {
                        // Schema validation already recorded the missing contract. Continue
                        // collecting diagnostics without turning that error into a panic.
                        missing = true;
                        continue;
                    };
                    // Final enum dispatch diagnoses omissions in the selected branch,
                    // including named slots and defaults before a later required parameter.
                    let binding = crate::types::call_args::required_argument_binding_signature(&implementation);
                    let (shortage, satisfiable) = match crate::types::call_args::plan_call_args(
                        &binding, args, expr.span, false, true,
                    ) {
                        Ok(plan) => {
                            let shortage = plan.statically_missing_positionals(&implementation).is_some();
                            (shortage, !shortage)
                        }
                        Err(crate::types::call_args::CallArgPlanError::MissingRequired { .. }) => (true, false),
                        Err(_) => (false, false),
                    };
                    runtime_shortage |= shortage;
                    match self.check_user_declared_call(
                        &implementation, args, expr.span, env,
                        &format!("Method {name}::{method}"), &name,
                    ) {
                        Ok(_) => { accepted = true; break; }
                        Err(error) if satisfiable && satisfiable_error.is_none() => {
                            satisfiable_error = Some(error);
                        }
                        Err(_) => {}
                    }
                }
                if !accepted && !missing {
                    if let Some(error) = satisfiable_error { return Err(error); }
                    if !runtime_shortage { return Err(shared_error); }
                    // Deferring binding must not skip validation of the caller's source
                    // expressions when a missing slot made every signature check stop early.
                    for arg in args { self.infer_type(arg, env)?; }
                }
            }
        // An implementation may declare more parameters than the interface -- php dispatches on
        // the runtime class and binds what it declares -- so an over-supplied call is judged
        // against the widest implementation, as it already is for a class receiver.
        // `PasswordHasherInterface::hash($plain)` is called with `$salt` by every Symfony
        // password-hashing listener, and the legacy hashers declare it.
        } else if !is_static
            && self.subtype_dispatch_max_param_count(interface_name, &method_key) > sig.params.len()
        {
            self.check_user_declared_call_with_advisory_arity(
                &sig,
                args,
                expr.span,
                env,
                &format!("Method {}::{}", interface_name, method),
                interface_name,
            )?;
        } else {
            self.check_user_declared_call(
                &sig,
                args,
                expr.span,
                env,
                &format!("Method {}::{}", interface_name, method),
                interface_name,
            )?;
        }
        let late_static_return = if is_static {
            self.static_method_late_static_return(interface_name, &method_key)
        } else {
            self.instance_method_late_static_return(interface_name, &method_key)
        };
        match late_static_return {
            Some(return_type) => self.resolve_late_static_return_type_hint(
                &return_type,
                interface_name,
                expr.span,
            ),
            None => Ok(sig.return_type),
        }
    }

    /// Infers the type of a method call on a class type.
    ///
    /// Looks up the method in the class schema, checks deprecation warnings,
    /// validates visibility, normalizes named arguments, validates the
    /// callable signature, and updates the method's parameter types from
    /// argument types (for local type inference). Handles `__call` magic
    /// methods and falls back to `PhpType::Int`.
    pub(crate) fn infer_method_call_on_class_type(
        &mut self,
        class_name: &str,
        method: &str,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        // `$stmt->bind_result(...$refs)` (doctrine/dbal `Mysqli\Result`): php binds each
        // by-reference parameter to its unpacked element. Lowering routes such a call through
        // the descriptor invoker (`lower_runtime_spread_method_call`), which binds reference
        // markers per parameter, so it is checked under the descriptor contract.
        let allow_by_ref_spread = args.iter().any(|arg| matches!(arg.kind, ExprKind::Spread(_)))
            && self.classes.get(class_name)
                .and_then(|class_info| class_info.methods.get(&php_symbol_key(method)))
                .is_some_and(|sig| crate::types::call_args::spread_reaches_by_ref_suffix(sig, args));
        self.infer_method_call_on_class_type_with_options(
            class_name, method, args, expr, env, allow_by_ref_spread,
        )
    }

    /// Returns the receiver expression of a method-call expression, in either call form.
    fn method_call_receiver_expr(expr: &Expr) -> Option<&Expr> {
        match &expr.kind {
            ExprKind::MethodCall { object, .. } | ExprKind::NullsafeMethodCall { object, .. } => {
                Some(object)
            }
            _ => None,
        }
    }

    /// Returns php's `TypeError` message when a forwarded constructor argument cannot bind.
    ///
    /// php checks argument types when the call RUNS, and it exempts constructors from inheritance
    /// signature rules — an overriding `__construct` may declare a different parameter list
    /// entirely and still call `parent::__construct` positionally, which lands arguments in
    /// differently typed slots. That is legal, loadable php that throws only if the path executes.
    /// `twig/twig`'s `ExtensionSet::convertInfixExpressionParser` is the shape: its anonymous
    /// subclass declares `array $aliases` in the slot the parent declares `?string $description`,
    /// and forwards it. Refusing the build there refused every program that merely CONTAINS Twig.
    ///
    /// Only a provable mismatch is reported, using the same acceptance the strict check uses, so a
    /// call this returns `None` for still goes through `check_user_declared_call` unchanged.
    fn deferred_constructor_argument_type_error(
        &mut self,
        sig: &FunctionSig,
        args: &[Expr],
        env: &TypeEnv,
        class_name: &str,
    ) -> Result<Option<String>, CompileError> {
        for (index, arg) in args.iter().enumerate() {
            let Some((param_name, param_ty)) = sig.params.get(index) else {
                break;
            };
            // A by-reference parameter binds storage rather than a value, and a spread has no
            // single argument type to compare; both stay with the strict path.
            if sig.ref_params.get(index).copied().unwrap_or(false)
                || matches!(arg.kind, ExprKind::Spread(_))
            {
                continue;
            }
            let actual = self.infer_type(arg, env)?;
            if Self::types_compatible(param_ty, &actual) || self.type_accepts(param_ty, &actual) {
                continue;
            }
            // A value narrowed to an interface can still be the declared class: only its runtime
            // class decides, as `DefinedTest` handing `TestExpression` a `Node` it narrowed to
            // `SupportDefinedTestInterface` shows. Nothing is proved, so the call runs its guard.
            if matches!((param_ty, &actual), (PhpType::Object(_), PhpType::Object(_)))
                && self.definite_param_mismatch_given(param_ty, &actual, arg).is_none()
            {
                continue;
            }
            return Ok(Some(format!(
                "{}::__construct(): Argument #{} (${}) must be of type {}, {} given",
                class_name,
                index + 1,
                param_name,
                php_declared_type_spelling(param_ty),
                php_value_type_spelling(&actual),
            )));
        }
        Ok(None)
    }

    /// Accepts a method absent from a nominal receiver when a compatible concrete subtype owns it.
    ///
    /// PHP dispatches on the runtime class. The closed-world backend mirrors that behavior with a
    /// class-id switch, so the checker derives the call result from the same compatible candidates.
    fn infer_lenient_subtype_method_call(
        &mut self,
        receiver_type: &str,
        method: &str,
        method_key: &str,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
        receiver_is_interface: bool,
    ) -> Result<PhpType, CompileError> {
        let return_types = self.subtype_dispatch_return_types(receiver_type, method_key);
        if return_types.is_empty() {
            // Nothing in the closed world declares it — a typo, unless the body itself asked
            // whether the method is there. Twig's `getOperatorTokensFor()` is the second case: it
            // calls `$parser->getOperatorTokens()` behind `method_exists()` so a 4.0 interface
            // method can be adopted early, and no bundled parser implements it yet. The guarded
            // call keeps PHP's outcome if it is reached — with no candidate,
            // `lower_narrowed_interface_method_call` fatals with `Call to undefined method`.
            let guarded = Self::method_call_receiver_expr(expr)
                .is_some_and(|receiver| self.method_exists_capability_guarded(receiver, method_key));
            // An INTERFACE receiver defers unguarded too, but ONLY in a program that can gain
            // classes at run time -- the same condition `allows_absent_runtime_class` already
            // uses, and for the same reason. There, the runtime class behind an interface is not
            // something a closed world can enumerate: `Foo::class` is a string, so a class named
            // only that way is never walked in, and a runtime include or a registered autoloader
            // can declare one the compiler never saw. `twig/twig`'s CoreExtension is exactly that
            // -- `$env->getExtension(SandboxExtension::class)` is declared `: ExtensionInterface`
            // and the next line calls `$sandbox->isSandboxed()`, a method only the concrete
            // extension has -- and refusing it stopped a build of code php runs.
            //
            // In a program with no such route the world really is closed, and a method that
            // nothing anywhere declares is a typo: `$parser->getNmae()` keeps its diagnostic.
            // Deferring is not accepting either way; with no candidate,
            // `lower_narrowed_interface_method_call` fatals with php's `Call to undefined method`.
            let defers_to_runtime = receiver_is_interface && self.program_defers_unknown_classes;
            // `$this` inside a closure is the receiver it is BOUND to, which `Closure::bind()` or
            // `bindTo()` can make any class: the enclosing class only supplies the initial one,
            // as `infer_property_access` already accepts for properties. Symfony's debug commands
            // run `$this->buildContainer()` through a closure bound to the kernel, and refusing it
            // kept `debug:container`, `lint:container` and `debug:router` out of the build.
            let rebindable_this = self.closure_depth > 0
                && Self::method_call_receiver_expr(expr)
                    .is_some_and(|receiver| matches!(receiver.kind, ExprKind::This));
            if !guarded && !defers_to_runtime && !rebindable_this {
                // php binds the method when the call RUNS, against the run-time class; see
                // `RuntimeFault::UndefinedMethod` for who may compile this as that fault.
                return Err(CompileError::new(
                    expr.span,
                    &format!("Undefined method: {}::{}", receiver_type, method),
                )
                .with_runtime_fault(RuntimeFault::UndefinedMethod {
                    class: receiver_type.to_string(),
                    method: method.to_string(),
                }));
            }
            for arg in args {
                self.infer_type(arg, env)?;
            }
            return Ok(PhpType::Mixed);
        }
        for arg in args {
            self.infer_type(arg, env)?;
        }
        Ok(match return_types.as_slice() {
            [only] => only.clone(),
            _ => PhpType::Mixed,
        })
    }

    /// Collects distinct return types from compatible concrete classes declaring a method.
    /// Returns the largest declared parameter count for `method_key` across the receiver's family.
    ///
    /// PHP binds arguments against the RUNTIME class's signature, so a call that is over-supplied
    /// for the type in hand can be exactly right for an override. Only classes the closed world
    /// actually has are counted.
    pub(crate) fn subtype_dispatch_max_param_count(
        &self,
        receiver_type: &str,
        method_key: &str,
    ) -> usize {
        let mut widest = 0;
        for class_name in self.subtype_dispatch_owners(receiver_type, method_key).iter() {
            if let Some(sig) = self
                .classes
                .get(class_name)
                .and_then(|class_info| class_info.methods.get(method_key))
            {
                widest = widest.max(sig.params.len());
            }
        }
        widest
    }

    fn subtype_dispatch_return_types(&self, receiver_type: &str, method_key: &str) -> Vec<PhpType> {
        let mut return_types = Vec::new();
        for class_name in self.subtype_dispatch_owners(receiver_type, method_key).iter() {
            if let Some(sig) = self
                .classes
                .get(class_name)
                .and_then(|class_info| class_info.methods.get(method_key))
            {
                if !return_types.contains(&sig.return_type) {
                    return_types.push(sig.return_type.clone());
                }
            }
        }
        return_types
    }

    /// The classes declaring `method_key` that are the receiver type itself or extend/implement
    /// it, in `method_owners_of` order. Hierarchy is fixed once the class table is built (the
    /// method pass rewrites signatures, which callers read fresh), so the answer is memoized per
    /// table size: every call site used to re-walk each owner's parent chain.
    /// The classes naming `interface_name` in their own `implements` list, sorted. Memoized per
    /// table size: every interface-typed call collected them from the whole class table.
    fn direct_interface_implementers(&self, interface_name: &str) -> std::sync::Arc<[String]> {
        {
            let mut cache = self.interface_implementers_cache.borrow_mut();
            if cache.0 != self.classes.len() {
                cache.0 = self.classes.len();
                cache.1.clear();
            }
            if let Some(names) = cache.1.get(interface_name) {
                return names.clone();
            }
        }
        #[cfg(test)]
        crate::types::checker::INTERFACE_IMPLEMENTER_SCANS.with(|count| count.set(count.get() + 1));
        let mut names: Vec<String> = self
            .classes
            .iter()
            .filter(|(_, info)| info.interfaces.iter().any(|name| name == interface_name))
            .map(|(name, _)| name.clone())
            .collect();
        names.sort();
        let names: std::sync::Arc<[String]> = names.into();
        self.interface_implementers_cache
            .borrow_mut()
            .1
            .insert(interface_name.to_string(), names.clone());
        names
    }

    fn subtype_dispatch_owners(&self, receiver_type: &str, method_key: &str) -> std::sync::Arc<[String]> {
        {
            let mut cache = self.subtype_dispatch_cache.borrow_mut();
            if cache.0 != self.classes.len() {
                cache.0 = self.classes.len();
                cache.1.clear();
            }
            if let Some(owners) = cache.1.get(&(receiver_type.to_string(), method_key.to_string())) {
                return owners.clone();
            }
        }
        let owners = self.method_owners_of(method_key).unwrap_or_default();
        let compatible: std::sync::Arc<[String]> = owners
            .iter()
            .filter(|class_name| {
                #[cfg(test)]
                crate::types::checker::SUBTYPE_DISPATCH_OWNERS_EXAMINED.with(|count| count.set(count.get() + 1));
                self.classes
                    .get(class_name.as_str())
                    .is_some_and(|class_info| class_info.methods.contains_key(method_key))
                    && (class_name.as_str() == receiver_type
                        || self.is_subclass_of(class_name, receiver_type)
                        || self.class_implements_interface(class_name, receiver_type))
            })
            .cloned()
            .collect();
        self.subtype_dispatch_cache
            .borrow_mut()
            .1
            .insert((receiver_type.to_string(), method_key.to_string()), compatible.clone());
        compatible
    }

    /// Infers a class method call for descriptor-backed callback paths that can
    /// preserve by-reference spread arguments through runtime invoker metadata.
    pub(crate) fn infer_method_call_on_class_type_allowing_by_ref_spread(
        &mut self,
        class_name: &str,
        method: &str,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        self.infer_method_call_on_class_type_with_options(class_name, method, args, expr, env, true)
    }

    /// Normalizes method arguments under the lowering contract selected by the call surface.
    ///
    /// Descriptor invocations keep dynamic spread sources for their runtime key walk. Ordinary
    /// method calls retain the array-only planner, so accepting a Traversable here cannot widen
    /// direct `$object->method(...$source)` semantics.
    fn normalize_method_call_args_with_options(
        &self,
        sig: &FunctionSig,
        args: &[Expr],
        span: crate::span::Span,
        callee_desc: &str,
        env: &TypeEnv,
        descriptor_invocation: bool,
    ) -> Result<Vec<Expr>, CompileError> {
        if descriptor_invocation {
            Ok(self
                .plan_descriptor_call_args(sig, args, span, callee_desc)?
                .normalized_args())
        } else {
            self.normalize_named_call_args(sig, args, span, callee_desc, env)
        }
    }

    /// Shared implementation for class method call inference.
    fn infer_method_call_on_class_type_with_options(
        &mut self,
        class_name: &str,
        method: &str,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
        allow_by_ref_spread: bool,
    ) -> Result<PhpType, CompileError> {
        let result = self.infer_method_call_on_class_type_inner(
            class_name,
            method,
            args,
            expr,
            env,
            allow_by_ref_spread,
        )?;
        Ok(self.self_recursive_call_type(class_name, method, false, result))
    }

    /// Types a call a method makes to ITSELF as `mixed` once its body was refused on the seed.
    ///
    /// See `Checker::recursive_mixed_methods`: the callee's return type is the `int` seed until
    /// its body checks, and a body that feeds the recursive result to `array_merge()` never does.
    fn self_recursive_call_type(
        &self,
        class_name: &str,
        method: &str,
        is_static: bool,
        result: PhpType,
    ) -> PhpType {
        // Only while the callee still has the `int` seed: once a pass has inferred its real
        // return type, the call is typed by it and the lowering reads the same storage.
        if result != PhpType::Int {
            return result;
        }
        let (Some(current_class), Some(current_method)) =
            (self.current_class.as_deref(), self.current_method.as_deref())
        else {
            return result;
        };
        if self.current_method_is_static != is_static
            || !current_class.eq_ignore_ascii_case(class_name.trim_start_matches('\\'))
            || current_method != php_symbol_key(method)
            || !self.recursive_mixed_methods.contains(&(
                current_class.to_string(),
                current_method.to_string(),
                is_static,
            ))
        {
            return result;
        }
        PhpType::Mixed
    }

    fn infer_method_call_on_class_type_inner(
        &mut self,
        class_name: &str,
        method: &str,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
        allow_by_ref_spread: bool,
    ) -> Result<PhpType, CompileError> {
        let method_key = php_symbol_key(method);
        // A private method belongs to the scope that declared it, not to the receiver's class.
        let lexical_scope = self.lexical_private_method_scope(class_name, &method_key);
        let class_name = lexical_scope.as_deref().unwrap_or(class_name);
        if self.enums.contains_key(class_name)
            && args.iter().any(|arg| matches!(arg.kind, ExprKind::Spread(_)))
            && crate::types::call_args::expand_planned_positional_spreads(args).is_none()
        {
            // Concrete and nullsafe enum calls share the selected runtime binder.
            // Its boxed collector must also be the layout the method body reads.
            self.promote_descriptor_variadic_container_for_method(class_name, &method_key, false);
        }
        self.reject_unsupported_reflection_attribute_filter_flags(
            Some(class_name),
            &method_key,
            args,
            expr.span,
        )?;
        let late_static_return_type = self
            .instance_method_late_static_return(class_name, &method_key)
            .map(|return_type| {
                self.resolve_late_static_return_type_hint(&return_type, class_name, expr.span)
            })
            .transpose()?;
        let mut normalized_args = args.to_vec();
        let mut magic_return_ty = None;
        let mut magic_original_args = None;
        if let Some(class_info) = self.classes.get(class_name) {
            if let Some(sig) = class_info.methods.get(&method_key) {
                if let Some(reason) = sig.deprecation.clone() {
                    let message = if reason.is_empty() {
                        format!("Call to deprecated method: {}::{}()", class_name, method)
                    } else {
                        format!(
                            "Call to deprecated method: {}::{}() — {}",
                            class_name, method, reason
                        )
                    };
                    self.warnings
                        .push(crate::errors::CompileWarning::new(expr.span, &message));
                }
                if let Some(visibility) = class_info.method_visibilities.get(&method_key) {
                    let declaring_class = class_info
                        .method_declaring_classes
                        .get(&method_key)
                        .map(String::as_str)
                        .unwrap_or(class_name);
                    if !self.can_access_member(declaring_class, visibility)
                        && !self.can_access_pdo_prelude_internal_method(class_name, &method_key)
                        && !self.can_access_mysqli_prelude_internal_method(class_name, &method_key)
                    {
                        // PHP raises this as a catchable `Error` at runtime instead of a
                        // compile-time rejection. Record the throw site so EIR lowering
                        // emits the throw sequence, and continue with the declared return
                        // type so later passes stay type-consistent.
                        crate::types::checker::record_throw_access_site(
                            &mut self.throw_access_sites,
                            self.current_loop_storage_scope.clone(),
                            expr.span,
                            crate::types::ThrowAccessInfo {
                                span: expr.span,
                                kind: crate::types::ThrowAccessKind::PrivateMethod {
                                    visibility: match visibility {
                                        crate::parser::ast::Visibility::Private => {
                                            "private".to_string()
                                        }
                                        crate::parser::ast::Visibility::Protected => {
                                            "protected".to_string()
                                        }
                                        _ => "private".to_string(),
                                    },
                                    class_name: class_name.to_string(),
                                    method: method.to_string(),
                                },
                            },
                        );
                        return Ok(Self::late_static_or_storage_return(
                            late_static_return_type.as_ref(),
                            &sig.return_type,
                        ));
                    }
                }
                let declared_flags =
                    Self::declared_method_param_flags(class_info, &method_key, false);
                let mut effective_sig =
                    Self::callable_sig_for_declared_params(sig, &declared_flags);
                if method_key == "__call" {
                    Self::relax_magic_call_validation_sig(&mut effective_sig);
                }
                normalized_args = self.normalize_method_call_args_with_options(
                    &effective_sig,
                    args,
                    expr.span,
                    &format!("Method {}::{}", class_name, method),
                    env,
                    allow_by_ref_spread,
                )?;
                if allow_by_ref_spread {
                    self.check_user_declared_call_allowing_by_ref_spread(
                        &effective_sig,
                        args,
                        expr.span,
                        env,
                        &format!("Method {}::{}", class_name, method),
                        class_name,
                    )
                    .map_err(|error| Self::tag_too_few_arguments(error, class_name, method))?;
                } else if self.subtype_dispatch_max_param_count(class_name, &method_key)
                    > effective_sig.params.len()
                {
                    self.check_user_declared_call_with_advisory_arity(
                        &effective_sig,
                        &normalized_args,
                        expr.span,
                        env,
                        &format!("Method {}::{}", class_name, method),
                        class_name,
                    )
                    .map_err(|error| Self::tag_too_few_arguments(error, class_name, method))?;
                } else {
                    self.check_user_declared_call(
                        &effective_sig,
                        args,
                        expr.span,
                        env,
                        &format!("Method {}::{}", class_name, method),
                        class_name,
                    )
                    .map_err(|error| Self::tag_too_few_arguments(error, class_name, method))?;
                }
            } else if class_info.static_methods.contains_key(&method_key) {
                let receiver = StaticReceiver::Named(crate::names::Name::from(
                    class_name.to_string(),
                ));
                return self.infer_static_method_call_type_with_options(
                    &receiver,
                    method,
                    args,
                    expr,
                    env,
                    allow_by_ref_spread,
                );
            } else if let Some(sig) = class_info.methods.get("__call") {
                let magic_args = Self::magic_call_args(method, args, expr.span);
                let declared_flags = Self::declared_method_param_flags(class_info, "__call", false);
                let mut effective_sig =
                    Self::callable_sig_for_declared_params(sig, &declared_flags);
                Self::relax_magic_call_validation_sig(&mut effective_sig);
                normalized_args = self.normalize_method_call_args_with_options(
                    &effective_sig,
                    &magic_args,
                    expr.span,
                    &format!("Method {}::__call", class_name),
                    env,
                    allow_by_ref_spread,
                )?;
                if allow_by_ref_spread {
                    self.check_user_declared_call_allowing_by_ref_spread(
                        &effective_sig,
                        &magic_args,
                        expr.span,
                        env,
                        &format!("Method {}::__call", class_name),
                        class_name,
                    )?;
                } else {
                    self.check_user_declared_call(
                        &effective_sig,
                        &normalized_args,
                        expr.span,
                        env,
                        &format!("Method {}::__call", class_name),
                        class_name,
                    )?;
                }
                magic_return_ty = Some(effective_sig.return_type.clone());
                magic_original_args = Some(args.to_vec());
            } else if let Some(return_ty) =
                self.infer_generic_method_call(class_name, method, args, expr, env)?
            {
                // A generic method has no signature, so every lookup above has already missed.
                // It is resolved here, one round before its instantiation exists.
                return Ok(return_ty);
            } else {
                return self.infer_lenient_subtype_method_call(
                    class_name,
                    method,
                    &method_key,
                    args,
                    expr,
                    env,
                    false,
                );
            }
        }
        if let Some(return_ty) = magic_return_ty {
            if let Some(args) = magic_original_args {
                self.specialize_magic_call_signature(
                    class_name,
                    &args,
                    env,
                    allow_by_ref_spread,
                )?;
            }
            return Ok(return_ty);
        }
        let mut arg_types = Vec::new();
        for arg in &normalized_args {
            arg_types.push(if allow_by_ref_spread {
                self.infer_descriptor_call_arg_type(arg, env)?
            } else {
                self.infer_type(arg, env)?
            });
        }
        let tail_spread_keeps_keys = self.variadic_tail_spread_keeps_keys(&normalized_args, 0, env);

        let impl_class_name = self
            .classes
            .get(class_name)
            .and_then(|class_info| class_info.method_impl_classes.get(&method_key))
            .cloned()
            .unwrap_or_else(|| class_name.to_string());
        let declared_flags = self
            .classes
            .get(&impl_class_name)
            .map(|class_info| Self::declared_method_param_flags(class_info, &method_key, false))
            .unwrap_or_default();
        let tested_flags: Vec<bool> = (0..arg_types.len())
            .map(|i| self.instanceof_tested_param(&impl_class_name, &method_key, i))
            .collect();
        if let Some(class_info) = self.classes.get_mut(&impl_class_name) {
            if let Some(sig) = class_info.methods.get_mut(&method_key) {
                let regular_param_count = if sig.variadic.is_some() {
                    sig.params.len().saturating_sub(1)
                } else {
                    sig.params.len()
                };
                // A callable argument widens like any other, as it does for functions: excluding it
                // left a parameter one site called with an int and another with a closure at
                // `int`, and `$callable(...)` in the body was refused (phpoption's `Some::filter`).
                for (i, arg_ty) in arg_types.iter().enumerate() {
                    if i < regular_param_count
                        && !declared_flags.get(i).copied().unwrap_or(false)
                        && !self.interface_method_boxed_params.contains(&(
                            format!("{}::{}", impl_class_name, method_key), i,
                        ))
                        && !tested_flags.get(i).copied().unwrap_or(false)
                        && !matches!(*arg_ty, PhpType::Void | PhpType::Never)
                    {
                        let key = (format!("{}::{}", impl_class_name, method_key), i);
                        let seen = self.param_specialization_seen.contains(&key);
                        if sig.params[i].1 == PhpType::Int && !seen {
                            self.param_specialization_seen.insert(key);
                            sig.params[i].1 = arg_ty.clone();
                        } else {
                            sig.params[i].1 = Self::union_param_type(&sig.params[i].1, arg_ty);
                        }
                    }
                }
                let variadic_is_declared = declared_flags
                    .get(regular_param_count)
                    .copied()
                    .unwrap_or(false);
                if !variadic_is_declared
                    && method_variadic_tail_needs_iterable(
                        &normalized_args,
                        sig,
                        regular_param_count,
                        env,
                        tail_spread_keeps_keys,
                    )
                    && !method_variadic_param_is_by_ref(sig)
                {
                    if let Some((_, variadic_ty)) = sig.params.last_mut() {
                        *variadic_ty = PhpType::Iterable;
                    }
                } else if !variadic_is_declared
                    && sig.variadic.is_some()
                    && arg_types.len() > regular_param_count
                    && !method_variadic_param_is_by_ref(sig)
                    // A declared element type on the variadic (`mixed ...$xs`, `int ...$xs`) is
                    // the contract, exactly like a declared regular parameter above: call-site
                    // arguments are validated against it and must never narrow it. Without this
                    // guard `mixed ...$xs` was rewritten to the widened argument type, and a
                    // later checker pass then rejected the very call that produced it.
                    && !declared_flags.get(regular_param_count).copied().unwrap_or(false)
                {
                    let mut elem_ty = arg_types[regular_param_count].clone();
                    for arg_ty in arg_types.iter().skip(regular_param_count + 1) {
                        elem_ty = wider_type_syntactic(&elem_ty, arg_ty);
                    }
                    if let Some((_, PhpType::Array(existing_elem_ty))) = sig.params.last_mut() {
                        **existing_elem_ty =
                            widen_method_variadic_element(existing_elem_ty.as_ref(), &elem_ty);
                    }
                }
                return Ok(Self::late_static_or_storage_return(
                    late_static_return_type.as_ref(),
                    &sig.return_type,
                ));
            }
        }
        // PHP permits unresolved nominal types in declarations. When such a receiver reaches a
        // method call, no declaration is available to determine its return representation, so a
        // boxed gradual result is the only sound fallback. The historical integer fallback could
        // corrupt a nullable chain into `int|null` and reject the following member access.
        Ok(PhpType::Mixed)
    }

    /// Picks the type a call site reads from a method whose return mentions `static`.
    ///
    /// The late-static resolution narrows the result to the receiver class, but the callee's body
    /// returns in the storage its signature records. An override that narrows an ancestor's boxed
    /// return (`static|false` over to `static`) adopts the ancestor's boxed storage, so reading the
    /// result as a raw object pointer treated a box as an object and crashed. Whenever the two
    /// disagree on representation, the signature's own type wins.
    fn late_static_or_storage_return(late_static: Option<&PhpType>, sig_return: &PhpType) -> PhpType {
        match late_static {
            Some(late)
                if late.codegen_repr() == sig_return.codegen_repr()
                    || !matches!(sig_return.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) =>
            {
                late.clone()
            }
            _ => sig_return.clone(),
        }
    }

    /// Returns `ancestor` (when it is a registered class) and every registered class below it.
    ///
    /// The same set the parent-chain walk `class is ancestor or one of its subclasses` selects
    /// over the whole table, found from the children index instead: a cycle in the parent
    /// links is cut by the visited set, exactly where the walk's hop limit cut it.
    pub(crate) fn class_and_descendants(&mut self, ancestor: &str) -> Vec<String> {
        let class_count = self.classes.len();
        if self
            .class_children_index
            .as_ref()
            .is_none_or(|(built_for, _)| *built_for != class_count)
        {
            let mut children: HashMap<String, Vec<String>> = HashMap::new();
            for (name, info) in &self.classes {
                if let Some(parent) = &info.parent {
                    children.entry(parent.clone()).or_default().push(name.clone());
                }
            }
            self.class_children_index = Some((class_count, children));
        }
        let children = &self
            .class_children_index
            .as_ref()
            .expect("children index built above")
            .1;
        let mut found = Vec::new();
        let mut visited: HashSet<&str> = HashSet::new();
        let mut stack: Vec<&str> = vec![ancestor];
        while let Some(name) = stack.pop() {
            if !visited.insert(name) {
                continue;
            }
            if self.classes.contains_key(name) {
                found.push(name.to_string());
            }
            if let Some(direct) = children.get(name) {
                stack.extend(direct.iter().map(String::as_str));
            }
        }
        found
    }

    /// Returns preserved late-static return syntax for an instance method.
    fn instance_method_late_static_return(
        &self,
        receiver_type: &str,
        method_key: &str,
    ) -> Option<TypeExpr> {
        if let Some(class_info) = self.classes.get(receiver_type) {
            if let Some(return_type) = class_info.late_static_method_returns.get(method_key) {
                return Some(return_type.clone());
            }
        }
        self.interfaces
            .get(receiver_type)
            .and_then(|interface_info| interface_info.late_static_method_returns.get(method_key))
            .cloned()
    }

    /// Builds synthetic `__call` arguments: `[method_name, [args...]]`.
    ///
    /// Constructs a `StringLiteral` for the method name and an `ArrayLiteral`
    /// of the original arguments, used when forwarding to `__call`.
    fn magic_call_args(method: &str, args: &[Expr], span: crate::span::Span) -> Vec<Expr> {
        vec![
            Expr::new(ExprKind::StringLiteral(method.to_string()), span),
            Expr::new(ExprKind::ArrayLiteral(args.to_vec()), span),
        ]
    }

    /// Tags a `Method C::m expects … arguments, got N` refusal that passes TOO FEW arguments
    /// as php's run-time `ArgumentCountError`, so `autoload::runtime_faults` can compile the call
    /// as that throw instead of deferring the file. php raises it only when the call runs:
    /// Laravel's `HandleExceptions` calls `$instance->enable()` without the argument the
    /// installed PHPUnit requires, on the branch `version_compare()` rules out. Too MANY
    /// arguments is not a fault in php (the extra ones are ignored) and is left as it is.
    fn tag_too_few_arguments(error: CompileError, class: &str, method: &str) -> CompileError {
        let prefix = format!("Method {}::{} expects ", class, method);
        let Some(rest) = error.message.strip_prefix(prefix.as_str()) else {
            return error;
        };
        let Some((arity, passed)) = rest.split_once(" arguments, got ") else {
            return error;
        };
        let Ok(passed) = passed.trim().parse::<usize>() else {
            return error;
        };
        let (required, exactly) = if let Some(minimum) = arity.strip_prefix("at least ") {
            (minimum.parse::<usize>().ok(), false)
        } else if let Some((minimum, _)) = arity.split_once(" to ") {
            (minimum.parse::<usize>().ok(), false)
        } else {
            (arity.parse::<usize>().ok(), true)
        };
        match required {
            Some(required) if passed < required => {
                error.with_runtime_fault(crate::errors::RuntimeFault::TooFewArguments {
                    class: class.to_string(),
                    method: method.to_string(),
                    passed,
                    required,
                    exactly,
                })
            }
            _ => error,
        }
    }

    /// Specializes `__call`'s second parameter (the args array) type based on
    /// the actual call arguments' inferred types.
    ///
    /// Merges all argument types into an element type, then updates the
    /// `__call` signature's params[1] (the array parameter) accordingly,
    /// respecting `declared_flags` and avoiding widening to `Mixed` when
    /// the declared type is already `Mixed`.
    fn specialize_magic_call_signature(
        &mut self,
        class_name: &str,
        args: &[Expr],
        env: &TypeEnv,
        descriptor_invocation: bool,
    ) -> Result<(), CompileError> {
        self.specialize_magic_dispatch_signature(
            class_name,
            "__call",
            false,
            args,
            env,
            descriptor_invocation,
        )
    }

    /// Refines a `__callStatic($name, $args)` signature's array parameter from
    /// the actual static-call arguments, the static counterpart of
    /// `specialize_magic_call_signature`.
    fn specialize_magic_static_call_signature(
        &mut self,
        class_name: &str,
        args: &[Expr],
        env: &TypeEnv,
        descriptor_invocation: bool,
    ) -> Result<(), CompileError> {
        self.specialize_magic_dispatch_signature(
            class_name,
            "__callstatic",
            true,
            args,
            env,
            descriptor_invocation,
        )
    }

    /// Shared body for `__call`/`__callStatic` argument-array specialization.
    ///
    /// Merges all argument types into an element type, then updates the magic
    /// method signature's params[1] (the array parameter) on its implementing
    /// class, selecting the instance or static method tables via `is_static`. Descriptor calls
    /// infer spread elements under their Traversable-aware runtime-walk contract; ordinary magic
    /// dispatch remains array-only.
    fn specialize_magic_dispatch_signature(
        &mut self,
        class_name: &str,
        method_key: &str,
        is_static: bool,
        args: &[Expr],
        env: &TypeEnv,
        descriptor_invocation: bool,
    ) -> Result<(), CompileError> {
        let mut elem_ty = PhpType::Never;
        for arg in args {
            let arg_ty = if descriptor_invocation {
                self.infer_descriptor_call_arg_type(arg, env)?
            } else {
                self.infer_type(arg, env)?
            };
            elem_ty = Self::merge_magic_call_arg_type(elem_ty, arg_ty);
        }
        // Interpreted code reaches the same magic method with whatever arguments it has, packed
        // as boxed values: the compiled call sites alone do not describe `$args`. Typed from them,
        // Laravel's `Facade::__callStatic($method, $args)` read an interpreted
        // `Log::debug("b")`'s argument array as raw strings and crashed in `implode()`.
        if self.program_has_interpreted_callers {
            elem_ty = PhpType::Mixed;
        }
        let args_array_ty = PhpType::Array(Box::new(elem_ty.clone()));
        let impl_class_name = self
            .classes
            .get(class_name)
            .and_then(|class_info| {
                if is_static {
                    class_info.static_method_impl_classes.get(method_key)
                } else {
                    class_info.method_impl_classes.get(method_key)
                }
            })
            .cloned()
            .unwrap_or_else(|| class_name.to_string());
        let declared_flags = self
            .classes
            .get(&impl_class_name)
            .map(|class_info| Self::declared_method_param_flags(class_info, method_key, is_static))
            .unwrap_or_default();
        let sig_slot = self.classes.get_mut(&impl_class_name).and_then(|class_info| {
            if is_static {
                class_info.static_methods.get_mut(method_key)
            } else {
                class_info.methods.get_mut(method_key)
            }
        });
        if let Some(sig) = sig_slot {
            if !sig.params.is_empty() {
                sig.params[0].1 = PhpType::Str;
            }
            if sig.params.len() > 1 {
                let declared_array_param = declared_flags.get(1).copied().unwrap_or(false);
                sig.params[1].1 = match &sig.params[1].1 {
                    PhpType::Array(existing)
                        if declared_array_param
                            && matches!(existing.as_ref(), PhpType::Mixed)
                            && !matches!(elem_ty, PhpType::Mixed) =>
                    {
                        args_array_ty
                    }
                    PhpType::Array(existing) => PhpType::Array(Box::new(
                        Self::merge_magic_call_arg_type(*existing.clone(), elem_ty.clone()),
                    )),
                    PhpType::Int => args_array_ty,
                    _ => sig.params[1].1.clone(),
                };
            }
        }
        // A class that inherits the magic method carries its own copy of the signature, and the
        // generated dispatch for `Sub::missing()` binds its arguments with THAT copy: a stale
        // `$args` element type there handed the implementation an array in the wrong shape.
        let implemented = self.classes.get(&impl_class_name).and_then(|class_info| {
            if is_static {
                class_info.static_methods.get(method_key).cloned()
            } else {
                class_info.methods.get(method_key).cloned()
            }
        });
        if let Some(implemented) = implemented {
            for (inheriting_name, class_info) in self.classes.iter_mut() {
                if inheriting_name == &impl_class_name {
                    continue;
                }
                let inherits = if is_static {
                    class_info.static_method_impl_classes.get(method_key)
                } else {
                    class_info.method_impl_classes.get(method_key)
                }
                .is_some_and(|owner| owner == &impl_class_name);
                if !inherits {
                    continue;
                }
                let copy = if is_static {
                    class_info.static_methods.get_mut(method_key)
                } else {
                    class_info.methods.get_mut(method_key)
                };
                if let Some(copy) = copy {
                    copy.params = implemented.params.clone();
                }
            }
        }
        Ok(())
    }

    /// Merges two types for `__call` argument type inference.
    ///
    /// Returns `right` when `left` is `Never`, `left` when `right` is `Never`,
    /// `left` when equal, and `PhpType::Mixed` otherwise. Used to compute the
    /// element type of the synthetic args array.
    fn merge_magic_call_arg_type(left: PhpType, right: PhpType) -> PhpType {
        if left == right {
            return left;
        }
        if matches!(left, PhpType::Never) {
            return right;
        }
        if matches!(right, PhpType::Never) {
            return left;
        }
        PhpType::Mixed
    }

    /// Relaxes a `__call` signature for validation-only use.
    ///
    /// Sets the first parameter to `PhpType::Str` and the second to
    /// `PhpType::Array(PhpType::Mixed)`, bypassing strict type checking so
    /// arbitrary arguments can be forwarded without false validation errors.
    fn relax_magic_call_validation_sig(sig: &mut crate::types::FunctionSig) {
        if let Some(param) = sig.params.get_mut(0) {
            param.1 = PhpType::Str;
        }
        if let Some(param) = sig.params.get_mut(1) {
            param.1 = PhpType::Array(Box::new(PhpType::Mixed));
        }
    }

    /// Infers the type of a static method call expression (`Foo::method()`, `self::`, `parent::`, `static::`).
    ///
    /// Resolves the receiver to a class name, checks deprecation and visibility,
    /// validates arguments via `normalize_named_call_args` and `check_known_callable_call`,
    /// and updates parameter types from argument types for local type inference.
    /// Handles enum static calls, `parent::`/`self::` forwarding to instance methods,
    /// and falls back to `PhpType::Int`.
    pub(crate) fn infer_static_method_call_type(
        &mut self,
        receiver: &StaticReceiver,
        method: &str,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        // Like the instance form: an unpack into a by-reference parameter is checked under the
        // descriptor contract its lowering uses.
        let class_name = match receiver {
            StaticReceiver::Named(name) => Some(name.as_str().to_string()),
            StaticReceiver::Self_ | StaticReceiver::Static => self.current_class.clone(),
            StaticReceiver::Parent => self.current_class.as_ref()
                .and_then(|class| self.classes.get(class))
                .and_then(|class_info| class_info.parent.clone()),
            _ => None,
        };
        let method_key = php_symbol_key(method);
        let allow_by_ref_spread = args.iter().any(|arg| matches!(arg.kind, ExprKind::Spread(_)))
            && class_name.as_ref().and_then(|class| self.classes.get(class)).is_some_and(|info| {
                info.static_methods.get(&method_key).or_else(|| info.methods.get(&method_key))
                    .is_some_and(|sig| crate::types::call_args::spread_reaches_by_ref_suffix(sig, args))
            });
        self.infer_static_method_call_type_with_options(
            receiver, method, args, expr, env, allow_by_ref_spread,
        )
    }

    /// Infers a static method call for descriptor-backed callback paths that can
    /// preserve by-reference spread arguments through runtime invoker metadata.
    pub(crate) fn infer_static_method_call_type_allowing_by_ref_spread(
        &mut self,
        receiver: &StaticReceiver,
        method: &str,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        self.infer_static_method_call_type_with_options(receiver, method, args, expr, env, true)
    }

    /// Shared implementation for static method call inference.
    ///
    /// A `static::method()` whose method only DESCENDANTS of the current class declare is checked
    /// against each of them, exactly as the `Child::method()` call the late-static dispatch ends
    /// in. PHP picks the target when the call runs and allows a protected method there, so the
    /// result is gradual.
    fn infer_static_method_call_type_with_options(
        &mut self,
        receiver: &StaticReceiver,
        method: &str,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
        allow_by_ref_spread: bool,
    ) -> Result<PhpType, CompileError> {
        let result = self.infer_static_method_call_type_dispatch(
            receiver,
            method,
            args,
            expr,
            env,
            allow_by_ref_spread,
        )?;
        let receiver_class = match receiver {
            StaticReceiver::Self_ | StaticReceiver::Static => self.current_class.clone(),
            StaticReceiver::Named(name) => Some(name.as_str().to_string()),
            _ => None,
        };
        Ok(match receiver_class {
            Some(class_name) => self.self_recursive_call_type(&class_name, method, true, result),
            None => result,
        })
    }

    fn infer_static_method_call_type_dispatch(
        &mut self,
        receiver: &StaticReceiver,
        method: &str,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
        allow_by_ref_spread: bool,
    ) -> Result<PhpType, CompileError> {
        if let Some(call) = crate::types::late_static_dispatch::static_receiver_instance_method_call(
            &self.classes,
            self.current_class.as_deref(),
            self.current_method_is_static,
            receiver,
            method,
            args,
            expr.span,
        ) {
            return self.infer_type(&call, env);
        }
        if matches!(receiver, StaticReceiver::Static) {
            if let Some(current) = self.current_class.clone() {
                let targets = crate::types::late_static_dispatch::child_only_static_targets(
                    &self.classes,
                    &current,
                    &php_symbol_key(method),
                );
                if !targets.is_empty() {
                    for target in &targets {
                        let named = StaticReceiver::Named(crate::names::Name::from_parts(
                            crate::names::NameKind::FullyQualified,
                            target.split('\\').map(str::to_string).collect(),
                        ));
                        self.infer_static_method_call_type_core(
                            &named,
                            method,
                            args,
                            expr,
                            env,
                            allow_by_ref_spread,
                        )?;
                    }
                    if self
                        .classes
                        .get(&current)
                        .is_some_and(|info| info.static_methods.contains_key("__callstatic"))
                    {
                        self.infer_static_method_call_type_core(
                            receiver,
                            method,
                            args,
                            expr,
                            env,
                            allow_by_ref_spread,
                        )?;
                    }
                    return Ok(PhpType::Mixed);
                }
            }
        }
        self.infer_static_method_call_type_core(
            receiver,
            method,
            args,
            expr,
            env,
            allow_by_ref_spread,
        )
    }

    /// Static method call inference proper, for one concrete receiver.
    fn infer_static_method_call_type_core(
        &mut self,
        receiver: &StaticReceiver,
        method: &str,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
        allow_by_ref_spread: bool,
    ) -> Result<PhpType, CompileError> {
        // `Box::of(5)` on a generic class, BEFORE the receiver is resolved against the class
        // table: a template is stripped from the program, so the table has no entry for it and
        // the ordinary path reports it as an undefined class.
        if let StaticReceiver::Named(class_name) = receiver {
            if let Some(inferred) =
                self.infer_generic_static_call(class_name.as_str(), method, args, expr, env)?
            {
                return Ok(inferred);
            }
        }
        let parent_call = matches!(receiver, StaticReceiver::Parent);
        let self_call = matches!(receiver, StaticReceiver::Self_);
        // A receiver written `Box<int>::of()` names `Box` until instantiation renames
        // it, and this pass can run on a generic function's template body — which is
        // walked and then stripped, never instantiated.
        let receiver = &receiver.written_class_receiver();
        let resolved_class_name = match receiver {
            // A generic receiver is instantiated into an ordinary named one before type checking;
            // a template has no class to reach through.
            StaticReceiver::Generic(_) => unreachable!(
                "StaticReceiver::Generic must be instantiated by generics::classes"
            ),
            StaticReceiver::Named(class_name) => class_name.as_str().to_string(),
            StaticReceiver::Self_ => self.current_class.as_ref().cloned().ok_or_else(|| {
                CompileError::new(expr.span, "Cannot use self:: outside class method scope")
            })?,
            StaticReceiver::Static => self.current_class.as_ref().cloned().ok_or_else(|| {
                CompileError::new(expr.span, "Cannot use static:: outside class method scope")
            })?,
            StaticReceiver::Parent => {
                let current_class = self.current_class.as_ref().ok_or_else(|| {
                    CompileError::new(expr.span, "Cannot use parent:: outside class method scope")
                })?;
                let current_info = self.classes.get(current_class).ok_or_else(|| {
                    CompileError::new(expr.span, &format!("Undefined class: {}", current_class))
                })?;
                // Inside a closure, `parent` is the parent of the scope the closure is BOUND to,
                // which `bindTo($object, $scope)` may only name at run time: Symfony's
                // `Descriptor::getEnvVarsFromDump()` writes `fn ($n) => parent::get($n)` in a
                // class with no parent and binds it to a parameter bag. php accepts it and checks
                // when the call runs; the lowering does the same
                // (`ir_lower::expr::static_method_calls`).
                if current_info.parent.is_none() && self.closure_depth > 0 {
                    for arg in args {
                        self.infer_type(arg, env)?;
                    }
                    return Ok(PhpType::Mixed);
                }
                current_info.parent.as_ref().cloned().ok_or_else(|| {
                    CompileError::new(
                        expr.span,
                        &format!("Class {} has no parent class", current_class),
                    )
                })?
            }
        };
        let class_name = resolved_class_name.as_str();
        let named_lexical_instance_call = matches!(receiver, StaticReceiver::Named(_))
            && !self.current_method_is_static
            && self.current_class.as_ref().is_some_and(|current| {
                current == class_name || self.is_subclass_of(current, class_name)
            });
        // `Closure::bind($closure, $newThis [, $scope])` is the static form of
        // `$closure->bindTo(...)`: it returns a new closure with `$this` rebound.
        if class_name.trim_start_matches('\\') == "Closure" && php_symbol_key(method) == "bind" {
            let args = &*crate::types::call_args::closure_bind_args_with_object_scope(args);
            let rebound_this = closure_bind_property_receiver_type(self, args, env);
            let rebound_scope = closure_bind_visibility_scope(self, args, env);
            for (index, arg) in args.iter().enumerate() {
                if index == 0 {
                    let saved_class = self.current_class.clone();
                    if rebound_scope.is_some() {
                        self.current_class = rebound_scope.clone();
                    }
                    let mut closure_env = env.clone();
                    if let Some(rebound_this) = &rebound_this {
                        closure_env.insert(
                            Self::narrowed_this_env_key().to_string(),
                            rebound_this.clone(),
                        );
                    }
                    let saved_bound_class = self.closure_bound_this_class.take();
                    if matches!(arg.kind, ExprKind::Closure { .. }) {
                        self.closure_bound_this_class = rebound_this
                            .as_ref()
                            .and_then(crate::types::checker::single_object_class_name);
                    }
                    let result = self.infer_type(arg, &closure_env);
                    self.closure_bound_this_class = saved_bound_class;
                    self.current_class = saved_class;
                    result?;
                } else {
                    self.infer_type(arg, env)?;
                }
            }
            return Ok(PhpType::Callable);
        }
        // `Closure::fromCallable($closure)` hands back the very Closure it was given. The literal
        // forms were rewritten to first-class callables by the name resolver; a value already
        // known to be a closure needs no conversion at all.
        if class_name.trim_start_matches('\\') == "Closure"
            && php_symbol_key(method) == "fromcallable"
            && args.len() == 1
            && !matches!(args[0].kind, ExprKind::NamedArg { .. } | ExprKind::Spread(_))
        {
            let arg_ty = self.infer_type(&args[0], env)?;
            if arg_ty.codegen_repr() == PhpType::Callable {
                return Ok(PhpType::Callable);
            }
        }
        if let Some(enum_info) = self.enums.get(class_name).cloned() {
            let has_signature = self.classes.get(class_name)
                .is_some_and(|info| info.static_methods.contains_key(&php_symbol_key(method)));
            if !has_signature {
                if let Some(return_ty) = self.infer_generic_method_call(class_name, method, args, expr, env)? {
                    return Ok(return_ty);
                }
            }
            return self
                .check_enum_static_call(&enum_info, class_name, method, args, env, expr.span);
        }
        let method_key = php_symbol_key(method);
        let late_static_receiver_type = if parent_call {
            self.current_class
                .clone()
                .unwrap_or_else(|| class_name.to_string())
        } else {
            class_name.to_string()
        };
        let late_static_static_return_type = self
            .static_method_late_static_return(class_name, &method_key)
            .map(|return_type| {
                self.resolve_late_static_return_type_hint(
                    &return_type,
                    &late_static_receiver_type,
                    expr.span,
                )
            })
            .transpose()?;
        let late_static_instance_return_type =
            if parent_call || self_call || named_lexical_instance_call {
            self.instance_method_late_static_return(class_name, &method_key)
                .map(|return_type| {
                    self.resolve_late_static_return_type_hint(
                        &return_type,
                        &late_static_receiver_type,
                        expr.span,
                    )
                })
                .transpose()?
            } else {
                None
            };
        let normalized_args: Vec<Expr>;
        let mut magic_return_ty = None;
        let mut magic_original_args = None;
        if let Some(class_info) = self.classes.get(class_name) {
            if let Some(sig) = class_info.static_methods.get(&method_key) {
                if let Some(reason) = sig.deprecation.clone() {
                    let message = if reason.is_empty() {
                        format!(
                            "Call to deprecated static method: {}::{}()",
                            class_name, method
                        )
                    } else {
                        format!(
                            "Call to deprecated static method: {}::{}() — {}",
                            class_name, method, reason
                        )
                    };
                    self.warnings
                        .push(crate::errors::CompileWarning::new(expr.span, &message));
                }
                if let Some(visibility) = class_info.static_method_visibilities.get(&method_key) {
                    let declaring_class = class_info
                        .static_method_declaring_classes
                        .get(&method_key)
                        .map(String::as_str)
                        .unwrap_or(class_name);
                    if !self.can_access_member(declaring_class, visibility)
                        && !self.can_access_pdo_exception_internal_factory(class_name, method)
                        && !self.can_access_mysqli_prelude_internal_factory(class_name, method)
                    {
                        return Err(CompileError::new(
                            expr.span,
                            &format!(
                                "Cannot access {} method: {}::{}",
                                Self::visibility_label(visibility),
                                class_name,
                                method
                            ),
                        ));
                    }
                }
                let declared_flags =
                    Self::declared_method_param_flags(class_info, &method_key, true);
                let mut effective_sig =
                    Self::callable_sig_for_declared_params(sig, &declared_flags);
                if method_key == "__callstatic" {
                    Self::relax_magic_call_validation_sig(&mut effective_sig);
                }
                normalized_args = self.normalize_method_call_args_with_options(
                    &effective_sig,
                    args,
                    expr.span,
                    &format!("Static method {}::{}", class_name, method),
                    env,
                    allow_by_ref_spread,
                )?;
                if class_name == "WeakReference" && method_key == "create" {
                    if let Some(argument) = normalized_args.first() {
                        let actual = self.infer_type(argument, env)?;
                        if !matches!(actual, PhpType::Object(_) | PhpType::Callable | PhpType::Mixed) {
                            return Err(CompileError::new(argument.span,
                                "WeakReference::create() argument must be an object"));
                        }
                    }
                }
                if allow_by_ref_spread {
                    self.check_user_declared_call_allowing_by_ref_spread(
                        &effective_sig,
                        args,
                        expr.span,
                        env,
                        &format!("Static method {}::{}", class_name, method),
                        class_name,
                    )?;
                } else {
                    self.check_user_declared_call(
                        &effective_sig,
                        args,
                        expr.span,
                        env,
                        &format!("Static method {}::{}", class_name, method),
                        class_name,
                    )?;
                }
            } else if parent_call || self_call || named_lexical_instance_call {
                if !class_info.methods.contains_key(&method_key)
                    && !class_info.methods.contains_key("__call")
                    && !class_info.static_methods.contains_key("__callstatic")
                {
                    for arg in args {
                        self.infer_type(arg, env)?;
                    }
                    crate::types::checker::record_throw_access_site(
                        &mut self.throw_access_sites,
                        self.current_loop_storage_scope.clone(),
                        expr.span,
                        crate::types::ThrowAccessInfo {
                            span: expr.span,
                            kind: crate::types::ThrowAccessKind::UndefinedMethod {
                                message: format!(
                                    "Call to undefined method {}::{}()",
                                    class_name, method
                                ),
                            },
                        },
                    );
                    return Ok(PhpType::Mixed);
                }
                if self.current_method_is_static {
                    // php raises this when the line RUNS, not when the class is compiled. Carbon's
                    // shared `Mixin` trait calls `self::copyStep()`, which only CarbonInterval
                    // declares, on a branch the other classes never take; refusing the class sent
                    // all of Carbon (and Laravel's request path) to the interpreter.
                    for arg in args {
                        self.infer_type(arg, env)?;
                    }
                    crate::types::checker::record_throw_access_site(
                        &mut self.throw_access_sites,
                        self.current_loop_storage_scope.clone(),
                        expr.span,
                        crate::types::ThrowAccessInfo {
                            span: expr.span,
                            kind: crate::types::ThrowAccessKind::UndefinedMethod {
                                message: format!(
                                    "Non-static method {}::{}() cannot be called statically",
                                    class_name, method
                                ),
                            },
                        },
                    );
                    return Ok(PhpType::Mixed);
                }
                let sig = class_info.methods.get(&method_key).ok_or_else(|| {
                    CompileError::new(
                        expr.span,
                        &format!("Undefined method: {}::{}", class_name, method),
                    )
                })?;
                if let Some(visibility) = class_info.method_visibilities.get(&method_key) {
                    let declaring_class = class_info
                        .method_declaring_classes
                        .get(&method_key)
                        .map(String::as_str)
                        .unwrap_or(class_name);
                    if !self.can_access_member(declaring_class, visibility) {
                        return Err(CompileError::new(
                            expr.span,
                            &format!(
                                "Cannot access {} method: {}::{}",
                                Self::visibility_label(visibility),
                                class_name,
                                method
                            ),
                        ));
                    }
                }
                let declared_flags =
                    Self::declared_method_param_flags(class_info, &method_key, false);
                let effective_sig = Self::callable_sig_for_declared_params(sig, &declared_flags);
                normalized_args = self.normalize_method_call_args_with_options(
                    &effective_sig,
                    args,
                    expr.span,
                    &format!(
                        "{} method {}::{}",
                        if parent_call {
                            "Parent"
                        } else if self_call {
                            "Self"
                        } else {
                            "Ancestor"
                        },
                        class_name,
                        method
                    ),
                    env,
                    allow_by_ref_spread,
                )?;
                if method_key == "__construct" {
                    // Cloned before the `&mut self` call below, so the borrow of `class_info`
                    // that produced `sig` ends here rather than spanning it.
                    let constructor_return_ty = sig.return_type.clone();
                    if let Some(message) = self.deferred_constructor_argument_type_error(
                        &effective_sig,
                        &normalized_args,
                        env,
                        class_name,
                    )? {
                        crate::types::checker::record_throw_access_site(
                            &mut self.throw_access_sites,
                            self.current_loop_storage_scope.clone(),
                            expr.span,
                            crate::types::ThrowAccessInfo {
                                span: expr.span,
                                kind: crate::types::ThrowAccessKind::ArgumentType { message },
                            },
                        );
                        return Ok(constructor_return_ty);
                    }
                }
                if allow_by_ref_spread {
                    self.check_user_declared_call_allowing_by_ref_spread(
                        &effective_sig,
                        args,
                        expr.span,
                        env,
                        &format!(
                            "{} method {}::{}",
                            if parent_call {
                                "Parent"
                            } else if self_call {
                                "Self"
                            } else {
                                "Ancestor"
                            },
                            class_name,
                            method
                        ),
                        class_name,
                    )?;
                } else {
                    self.check_user_declared_call(
                        &effective_sig,
                        args,
                        expr.span,
                        env,
                        &format!(
                            "{} method {}::{}",
                            if parent_call {
                                "Parent"
                            } else if self_call {
                                "Self"
                            } else {
                                "Ancestor"
                            },
                            class_name,
                            method
                        ),
                        class_name,
                    )?;
                }
            } else if class_info.methods.contains_key(&method_key) {
                return Err(CompileError::new(
                    expr.span,
                    &format!(
                        "Cannot call instance method statically: {}::{}",
                        class_name, method
                    ),
                ));
            } else if let Some(sig) = class_info.static_methods.get("__callstatic") {
                let magic_args = Self::magic_call_args(method, args, expr.span);
                let declared_flags =
                    Self::declared_method_param_flags(class_info, "__callstatic", true);
                let mut effective_sig =
                    Self::callable_sig_for_declared_params(sig, &declared_flags);
                Self::relax_magic_call_validation_sig(&mut effective_sig);
                normalized_args = self.normalize_method_call_args_with_options(
                    &effective_sig,
                    &magic_args,
                    expr.span,
                    &format!("Static method {}::__callStatic", class_name),
                    env,
                    allow_by_ref_spread,
                )?;
                if allow_by_ref_spread {
                    self.check_user_declared_call_allowing_by_ref_spread(
                        &effective_sig,
                        &normalized_args,
                        expr.span,
                        env,
                        &format!("Static method {}::__callStatic", class_name),
                        class_name,
                    )?;
                } else {
                    self.check_user_declared_call(
                        &effective_sig,
                        &normalized_args,
                        expr.span,
                        env,
                        &format!("Static method {}::__callStatic", class_name),
                        class_name,
                    )?;
                }
                magic_return_ty = Some(effective_sig.return_type.clone());
                magic_original_args = Some(args.to_vec());
            } else {
                // The class is in the compiled world and declares neither the method nor
                // `__callStatic()`: php's `Error` when the call runs.
                // A generic method called STATICALLY (`C::id(42)`). The generic-class attempt at
                // the top of this function answers for `Box<int>::of()`, where the CLASS carries
                // the type parameters; a method's own parameters are a different template, and it
                // is not in the class table either — so like the instance form, every ordinary
                // lookup has already missed by the time this runs.
                if let Some(inferred) =
                    self.infer_generic_method_call(class_name, method, args, expr, env)?
                {
                    return Ok(inferred);
                }
                return Err(CompileError::new(
                    expr.span,
                    &format!("Undefined method: {}::{}", class_name, method),
                )
                .with_runtime_fault(RuntimeFault::UndefinedStaticMethod {
                    class: class_name.to_string(),
                    method: method.to_string(),
                }));
            }
        } else if (self.eval_barrier_active || self.allows_absent_runtime_class())
            && matches!(receiver, StaticReceiver::Named(_))
        {
            // The class is not in the closed world (an `eval` fragment may declare it, or an
            // optional extension provides it at run time), so no signature governs the
            // arguments: alias them conservatively. Descriptor invocation
            // still owns its Traversable spread walk; ordinary static calls remain array-only.
            self.record_unresolved_callee_argument_aliases(args);
            for arg in args {
                if allow_by_ref_spread {
                    self.infer_descriptor_call_arg_type(arg, env)?;
                } else {
                    self.infer_type(arg, env)?;
                }
            }
            return Ok(PhpType::Mixed);
        } else {
            // Nothing in the closed world declares the class: php's `Class "X" not found` when
            // the call runs, if the pipeline can confirm no file supplies it either.
            return Err(CompileError::new(
                expr.span,
                &format!("Undefined class: {}", class_name),
            )
            .with_runtime_fault(RuntimeFault::UnknownClass {
                class: class_name.to_string(),
            }));
        }
        if let Some(return_ty) = magic_return_ty {
            if let Some(args) = magic_original_args {
                self.specialize_magic_static_call_signature(
                    class_name,
                    &args,
                    env,
                    allow_by_ref_spread,
                )?;
            }
            return Ok(return_ty);
        }
        let mut arg_types = Vec::new();
        for arg in &normalized_args {
            arg_types.push(if allow_by_ref_spread {
                self.infer_descriptor_call_arg_type(arg, env)?
            } else {
                self.infer_type(arg, env)?
            });
        }
        let tail_spread_keeps_keys = self.variadic_tail_spread_keeps_keys(&normalized_args, 0, env);

        // `AbstractPlatform::createSelectSQLBuilder()` from a descendant's instance method is
        // `parent::`'s non-virtual call on `$this`, aimed at the named ancestor (Doctrine DBAL's
        // `MySQL80Platform`); it fell through to the untyped default and returned `int`.
        let direct_impl_class_name = if parent_call || self_call || named_lexical_instance_call {
            self.classes
                .get(class_name)
                .and_then(|class_info| class_info.method_impl_classes.get(&method_key))
                .cloned()
                .unwrap_or_else(|| class_name.to_string())
        } else {
            String::new()
        };
        let static_declared_flags = self
            .classes
            .get(class_name)
            .map(|class_info| Self::declared_method_param_flags(class_info, &method_key, true))
            .unwrap_or_default();
        let static_tested_flags: Vec<bool> = (0..arg_types.len())
            .map(|i| self.instanceof_tested_param(class_name, &method_key, i))
            .collect();
        if let Some(class_info) = self.classes.get_mut(class_name) {
            if let Some(sig) = class_info.static_methods.get_mut(&method_key) {
                let regular_param_count = if sig.variadic.is_some() {
                    sig.params.len().saturating_sub(1)
                } else {
                    sig.params.len()
                };
                for (i, arg_ty) in arg_types.iter().enumerate() {
                    if i < regular_param_count
                        && !static_declared_flags.get(i).copied().unwrap_or(false)
                        && !static_tested_flags.get(i).copied().unwrap_or(false)
                        && !matches!(*arg_ty, PhpType::Void | PhpType::Never)
                    {
                        let key = (format!("static:{}::{}", class_name, method), i);
                        let seen = self.param_specialization_seen.contains(&key);
                        if sig.params[i].1 == PhpType::Int && !seen {
                            self.param_specialization_seen.insert(key);
                            sig.params[i].1 = arg_ty.clone();
                        } else {
                            sig.params[i].1 = Self::union_param_type(&sig.params[i].1, arg_ty);
                        }
                    }
                }
                let variadic_is_declared = static_declared_flags
                    .get(regular_param_count)
                    .copied()
                    .unwrap_or(false);
                if !variadic_is_declared
                    && method_variadic_tail_needs_iterable(
                        &normalized_args,
                        sig,
                        regular_param_count,
                        env,
                        tail_spread_keeps_keys,
                    )
                    && !method_variadic_param_is_by_ref(sig)
                {
                    if let Some((_, variadic_ty)) = sig.params.last_mut() {
                        *variadic_ty = PhpType::Iterable;
                    }
                } else if !variadic_is_declared
                    && sig.variadic.is_some()
                    && arg_types.len() > regular_param_count
                    && !method_variadic_param_is_by_ref(sig)
                    // Same rule as the instance-method path: a declared variadic element type is
                    // a contract to validate against, not a slot to narrow from the call site.
                    && !static_declared_flags
                        .get(regular_param_count)
                        .copied()
                        .unwrap_or(false)
                {
                    let mut elem_ty = arg_types[regular_param_count].clone();
                    for arg_ty in arg_types.iter().skip(regular_param_count + 1) {
                        elem_ty = wider_type_syntactic(&elem_ty, arg_ty);
                    }
                    if let Some((_, PhpType::Array(existing_elem_ty))) = sig.params.last_mut() {
                        **existing_elem_ty =
                            widen_method_variadic_element(existing_elem_ty.as_ref(), &elem_ty);
                    }
                }
                return Ok(late_static_static_return_type
                    .clone()
                    .unwrap_or_else(|| sig.return_type.clone()));
            }
        }
        if parent_call || self_call || named_lexical_instance_call {
            let instance_declared_flags = self
                .classes
                .get(&direct_impl_class_name)
                .map(|class_info| Self::declared_method_param_flags(class_info, &method_key, false))
                .unwrap_or_default();
            if let Some(sig) = self
                .classes
                .get_mut(&direct_impl_class_name)
                .and_then(|class_info| class_info.methods.get_mut(&method_key))
            {
                let regular_param_count = if sig.variadic.is_some() {
                    sig.params.len().saturating_sub(1)
                } else {
                    sig.params.len()
                };
                for (i, arg_ty) in arg_types.iter().enumerate() {
                    if i < regular_param_count
                        && !instance_declared_flags.get(i).copied().unwrap_or(false)
                        && !matches!(*arg_ty, PhpType::Void | PhpType::Never)
                    {
                        let key = (format!("{}::{}", direct_impl_class_name, method), i);
                        let seen = self.param_specialization_seen.contains(&key);
                        if sig.params[i].1 == PhpType::Int && !seen {
                            self.param_specialization_seen.insert(key);
                            sig.params[i].1 = arg_ty.clone();
                        } else {
                            sig.params[i].1 = Self::union_param_type(&sig.params[i].1, arg_ty);
                        }
                    }
                }
                let variadic_is_declared = instance_declared_flags
                    .get(regular_param_count)
                    .copied()
                    .unwrap_or(false);
                if !variadic_is_declared
                    && sig.variadic.is_some()
                    && arg_types.len() > regular_param_count
                    && !method_variadic_param_is_by_ref(sig)
                {
                    let mut elem_ty = arg_types[regular_param_count].clone();
                    for arg_ty in arg_types.iter().skip(regular_param_count + 1) {
                        elem_ty = wider_type_syntactic(&elem_ty, arg_ty);
                    }
                    if let Some((_, PhpType::Array(existing_elem_ty))) = sig.params.last_mut() {
                        **existing_elem_ty =
                            widen_method_variadic_element(existing_elem_ty.as_ref(), &elem_ty);
                    }
                }
                return Ok(late_static_instance_return_type
                    .clone()
                    .unwrap_or_else(|| sig.return_type.clone()));
            }
        }
        Ok(PhpType::Int)
    }

    /// Returns preserved late-static return syntax for a static method.
    fn static_method_late_static_return(
        &self,
        receiver_type: &str,
        method_key: &str,
    ) -> Option<TypeExpr> {
        self.classes
            .get(receiver_type)
            .and_then(|class_info| {
                class_info
                    .late_static_static_method_returns
                    .get(method_key)
            })
            .cloned()
    }

    /// Allows tightly-scoped private helper calls between compiler-generated PDO prelude classes.
    fn can_access_pdo_prelude_internal_method(&self, class_name: &str, method_key: &str) -> bool {
        let lazy_row_refresh = class_name == "PDORow"
            && method_key == "__elephcrefresh"
            && self.current_class.as_deref() == Some("PDOStatement")
            && self.current_method.as_deref() == Some("fetch");
        let pgsql_notice_drain = matches!(class_name, "PDO" | "Pdo\\Pgsql")
            && method_key == "__elephcdrainpgsqlnotices"
            && matches!(
                (self.current_class.as_deref(), self.current_method.as_deref()),
                (Some("PDOStatement"), Some("execute"))
                    | (Some("Pdo\\Pgsql"), Some("exec" | "query"))
            );
        lazy_row_refresh || pgsql_notice_drain
    }

    /// Allows compiler-generated PDO methods to construct exceptions with private driver state.
    fn can_access_pdo_exception_internal_factory(&self, class_name: &str, method: &str) -> bool {
        class_name == "PDOException"
            && php_symbol_key(method) == "__elephcfromerrorinfo"
            && matches!(self.current_class.as_deref(), Some("PDO" | "PDOStatement"))
    }

    /// Allows the mysqli prelude's internal static factories to stay private
    /// (they are not part of PHP's mysqli surface): the buffered-result drain
    /// factory is invoked by the connection's query/multi_query paths and by
    /// `mysqli_stmt::get_result`, the prepared-statement factory by
    /// `mysqli::prepare`, and the unprepared-statement factory by
    /// `mysqli::stmt_init`.
    fn can_access_mysqli_prelude_internal_factory(&self, class_name: &str, method: &str) -> bool {
        let drain_factory = class_name == "mysqli_result"
            && php_symbol_key(method) == "__elephcfromdrain"
            && matches!(self.current_class.as_deref(), Some("mysqli" | "mysqli_stmt"));
        let prepare_factory = class_name == "mysqli_stmt"
            && php_symbol_key(method) == "__elephcfromprepare"
            && self.current_class.as_deref() == Some("mysqli");
        let init_factory = class_name == "mysqli_stmt"
            && php_symbol_key(method) == "__elephcinit"
            && self.current_class.as_deref() == Some("mysqli");
        drain_factory || prepare_factory || init_factory
    }

    /// Allows tightly-scoped private helper calls within the mysqli prelude:
    /// `mysqli_stmt::requireLinkNotBusy` (the two-step prepare/execute
    /// commands-out-of-sync guard) reads the connection's private
    /// pending-result probe, and the procedural `mysqli_stmt_bind_param`
    /// alias — a compiler-owned free function, identified via
    /// `current_function` — reaches the statement's private bind-values
    /// helper (its by-ref variadic tail cannot be spread-forwarded into
    /// `bind_param`). Neither helper is part of PHP's mysqli surface.
    fn can_access_mysqli_prelude_internal_method(&self, class_name: &str, method_key: &str) -> bool {
        let pending_probe = class_name == "mysqli"
            && method_key == "__elephchaspendingresults"
            && self.current_class.as_deref() == Some("mysqli_stmt")
            && self.current_method.as_deref() == Some("requirelinknotbusy");
        let bind_values = class_name == "mysqli_stmt"
            && method_key == "__elephcbindparamvalues"
            && self
                .current_function
                .as_deref()
                .is_some_and(|name| php_symbol_key(name) == "mysqli_stmt_bind_param");
        pending_probe || bind_values
    }

    /// Return types of the concrete classes implementing `union_members` that declare `method`.
    ///
    /// Used when no union member declares the method itself. A union of INTERFACES describes a
    /// runtime object that may carry members none of them names — Symfony's `Router` holds a
    /// `UrlMatcherInterface|RequestMatcherInterface` and calls
    /// `addExpressionLanguageProvider()` on it behind `method_exists()`. Codegen already
    /// dispatches such a receiver on the runtime class id, so the implementors that declare the
    /// method are exactly the arms that can run, and their return types are what the call yields.
    ///
    /// Empty when nothing implements it, which leaves the caller's strict diagnostic in place.
    fn union_implementor_method_return_types(
        &mut self,
        union_members: &[String],
        method: &str,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<Vec<PhpType>, CompileError> {
        let method_key = php_symbol_key(method);
        let mut implementors: Vec<String> = self
            .classes
            .iter()
            .filter(|(class_name, class_info)| {
                !class_info.is_abstract
                    && class_info.methods.contains_key(&method_key)
                    && union_members.iter().any(|member| {
                        self.object_type_implements_interface(class_name, member)
                    })
            })
            .map(|(class_name, _)| class_name.clone())
            .collect();
        implementors.sort();
        let mut return_types = Vec::with_capacity(implementors.len());
        for class_name in &implementors {
            return_types.push(
                self.infer_method_call_on_class_type(class_name, method, args, expr, env)?,
            );
        }
        Ok(return_types)
    }
}

/// Resolves the visibility scope supplied to `Closure::bind()`.
///
/// PHP accepts a class name or an object as the third argument. When omitted, the closure keeps
/// its lexical class scope. Dynamic values whose class is not statically known remain gradual and
/// do not grant additional private or protected access during closure-body checking.
fn closure_bind_visibility_scope(
    checker: &mut Checker,
    args: &[Expr],
    env: &TypeEnv,
) -> Option<String> {
    let Some(scope) = args.get(2) else {
        return checker.current_class.clone();
    };
    match &scope.kind {
        ExprKind::StringLiteral(name) => Some(name.trim_start_matches('\\').to_string()),
        ExprKind::ClassConstant { receiver } => match receiver {
            // A generic receiver is instantiated into an ordinary named one before type checking;
            // a template has no class to reach through.
            StaticReceiver::Generic(_) => unreachable!(
                "StaticReceiver::Generic must be instantiated by generics::classes"
            ),
            StaticReceiver::Named(name) => {
                Some(name.as_str().trim_start_matches('\\').to_string())
            }
            StaticReceiver::Self_ | StaticReceiver::Static => checker.current_class.clone(),
            StaticReceiver::Parent => checker
                .current_class
                .as_ref()
                .and_then(|class| checker.classes.get(class))
                .and_then(|class| class.parent.clone()),
        },
        _ => checker
            .infer_type(scope, env)
            .ok()
            .as_ref()
            .and_then(crate::types::checker::single_object_class_name),
    }
}

/// Resolves the rebound `$this` type for the directly lowered bound-property closure shape.
///
/// The specialized lowering supports a closure whose body is exactly `return $this->property`;
/// all other bodies keep their existing gradual/runtime path.
fn closure_bind_property_receiver_type(
    checker: &mut Checker,
    args: &[Expr],
    env: &TypeEnv,
) -> Option<PhpType> {
    // MUST stay in step with `bound_this_is_gradual` in
    // `src/ir_lower/expr/static_method_calls.rs`, which asks the same question of the same two
    // arguments and types the closure's `this` capture from the answer. The two disagreeing is a
    // silent miscompile rather than a diagnostic: the checker would type `$this->p` against one
    // class and the backend emit it against another.
    //
    // Only a closure LITERAL has a body this can retype; a bind of an already-built closure value
    // cannot change how that body was compiled. What the body CONTAINS is not a condition —
    // `Closure::bind` rebinds `$this` for the whole of it.
    if !matches!(args.first()?.kind, ExprKind::Closure { .. }) {
        return None;
    }
    let new_this = checker.infer_type(args.get(1)?, env).ok()?;
    // A literal `null` target UNBINDS `$this`; the closure then has none, and answering `mixed`
    // would admit `$this` where PHP raises.
    if matches!(new_this, PhpType::Void) {
        return None;
    }
    if let Some(class_name) = crate::types::checker::single_object_class_name(&new_this) {
        return Some(PhpType::Object(class_name));
    }
    // A target this build cannot name is gradual, not the enclosing class — which is the one thing
    // a bound `$this` is definitely not. Symfony's `RedisTrait` line 70 binds to
    // `clone $redis->getOptions()` over a union of Redis classes none of which is installed, and
    // reading `$this->options` against `RedisAdapter` reported an undefined property on a class
    // that neither is the target nor owns it.
    matches!(new_this.codegen_repr(), PhpType::Mixed).then_some(PhpType::Mixed)
}

/// Returns true when a method variadic parameter must keep runtime key information.
fn method_variadic_tail_needs_iterable(
    args: &[Expr],
    sig: &FunctionSig,
    regular_param_count: usize,
    env: &TypeEnv,
    tail_spread_keeps_keys: bool,
) -> bool {
    if sig.variadic.is_none() {
        return false;
    }
    if tail_spread_keeps_keys {
        return true;
    }

    if args.iter().any(|arg| {
        matches!(
            &arg.kind,
            ExprKind::Spread(inner) if spread_source_keeps_runtime_keys(inner, env)
        )
    }) {
        return true;
    }

    args.iter().any(|arg| {
        matches!(
            &arg.kind,
            ExprKind::NamedArg { name, .. }
                if !sig
                    .params
                    .iter()
                    .take(regular_param_count)
                    .any(|(param_name, _)| param_name == name)
        )
    })
}

/// Joins a call site's argument type into a method's untyped variadic element type.
///
/// MONOTONE: the stored element only ever widens. `wider_type_syntactic` follows PHP's coercion
/// order, in which `Str` absorbs every other arm, so on its own it turned a `mixed` element back
/// into `string` whenever a string argument arrived. `mixed` is exactly what a first-class
/// callable or callable array promotes the collector to (the descriptor container,
/// `crate::types::signatures::descriptor_variadic_container`), so the method pass promoted it,
/// the top-level re-check of a string call site narrowed it again, and the outer fixpoint
/// alternated between the two until its pass budget ran out (Laravel's
/// `Illuminate\Http\Request::is(...$patterns)`: thousands of rounds). A `mixed` element holds any
/// argument already, so it stays.
fn widen_method_variadic_element(existing: &PhpType, incoming: &PhpType) -> PhpType {
    if *existing == PhpType::Mixed {
        return PhpType::Mixed;
    }
    wider_type_syntactic(existing, incoming)
}

/// Returns whether a method signature stores its variadic slot by reference.
fn method_variadic_param_is_by_ref(sig: &FunctionSig) -> bool {
    let Some(variadic_name) = sig.variadic.as_ref() else {
        return false;
    };
    sig.params
        .iter()
        .position(|(name, _)| name == variadic_name)
        .and_then(|index| sig.ref_params.get(index))
        .copied()
        .unwrap_or(false)
}

/// Returns true when a spread source can carry string keys into a variadic method tail.
fn spread_source_keeps_runtime_keys(expr: &Expr, env: &TypeEnv) -> bool {
    match &expr.kind {
        ExprKind::Variable(name) => matches!(
            env.get(name),
            Some(PhpType::AssocArray { .. } | PhpType::Iterable)
        ),
        ExprKind::ArrayLiteralAssoc(_) => true,
        _ => matches!(
            crate::types::checker::infer_expr_type_syntactic(expr),
            PhpType::AssocArray { .. } | PhpType::Iterable
        ),
    }
}


/// Spells a DECLARED parameter type the way php prints it in a `TypeError`.
///
/// Not `Display`: the compiler prints `array<mixed>` and `string|null` for shapes php calls
/// `array` and `?string`. A nullable union of one member is php's `?T`; a wider union keeps the
/// `A|B` form with `null` last, which is where php puts it.
pub(crate) fn php_declared_type_spelling(ty: &PhpType) -> String {
    match ty {
        PhpType::Array(_) | PhpType::AssocArray { .. } => "array".to_string(),
        PhpType::Void => "null".to_string(),
        PhpType::TaggedScalar => "?int".to_string(),
        PhpType::Union(members) => {
            let mut named: Vec<String> = members
                .iter()
                .filter(|member| !matches!(member, PhpType::Void))
                .map(php_declared_type_spelling)
                .collect();
            let nullable = members.iter().any(|member| matches!(member, PhpType::Void));
            match (named.len(), nullable) {
                (1, true) => format!("?{}", named.remove(0)),
                (_, true) => {
                    named.push("null".to_string());
                    named.join("|")
                }
                _ => named.join("|"),
            }
        }
        other => other.to_string(),
    }
}

/// Spells the type of a VALUE the way php names it after `given` in a `TypeError`.
fn php_value_type_spelling(ty: &PhpType) -> String {
    // A declared `array` is the packed-or-hash storage union; php names it `array`.
    if ty.is_php_array() {
        return "array".to_string();
    }
    match ty {
        PhpType::Array(_) | PhpType::AssocArray { .. } => "array".to_string(),
        PhpType::Void => "null".to_string(),
        PhpType::False => "bool".to_string(),
        PhpType::Resource(_) => "resource".to_string(),
        PhpType::Callable => "Closure".to_string(),
        PhpType::Object(name) if name.is_empty() => "object".to_string(),
        PhpType::Union(_) | PhpType::Mixed | PhpType::TaggedScalar => "mixed".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
std::thread_local! {
    /// Implementation signatures an interface-typed method call copied while being checked.
    pub(crate) static INTERFACE_IMPLEMENTATION_SIG_COPIES: std::cell::Cell<u64> =
        const { std::cell::Cell::new(0) };
}
