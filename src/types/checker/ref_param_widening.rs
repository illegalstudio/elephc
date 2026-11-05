//! Purpose:
//! Program-wide pre-pass that records every DECLARED by-reference parameter whose own body stores
//! a value the declaration cannot represent, so the CALLER and the CALLEE agree on one boxed cell
//! before either body is walked.
//!
//! Called from:
//! - `crate::types::checker::driver::check_types_impl`, once, after the function and class
//!   declarations are collected and the builtin signatures are patched.
//!
//! Key details:
//! - The fact is inter-procedural by nature and cannot be discovered while checking a body. The
//!   callee's cell IS the caller's slot, so a write the callee makes only reaches the caller if
//!   the CALLER's slot can hold it — and the checker's walk order does not guarantee the callee is
//!   checked first (a top-level call is checked before `resolve_unchecked_functions` ever reaches
//!   the callee, and `type_check_methods_until_stable` iterates classes in map order).
//! - Purely a STORAGE decision. The declared type stays in the signature and keeps validating
//!   arguments at every call site, so `function adv(int &$i) {…}; $s = "x"; adv($s);` is still the
//!   `parameter $i expects Int, got Str` diagnostic it is today — which matters, because `php -n`
//!   throws a TypeError there and silently accepting it would be a wrong answer, not a lost note.
//! - Missing a widening costs the pre-existing `cannot reassign` error, never a wrong
//!   representation, so every rule here fails towards saying nothing.

use std::collections::HashMap;

use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, ExprKind, Program, Stmt, StmtKind, TypeExpr};
use crate::span::Span;
use crate::types::traits::FlattenedClass;
use crate::types::PhpType;

use super::Checker;

impl Checker {
    /// Records the by-reference parameters whose bodies widen them, for every function and method
    /// in the program.
    ///
    /// PHP's reference cell is untyped: a declared `int &$i` gates what may be BOUND to it at the
    /// call, and nothing at all about what the body may later store through it. `php -n` runs
    /// `function adv(string $s, int &$i) { $i = strpos($s, ":", $i); }` and prints `int(2)` then
    /// `bool(false)` through the reference — `Symfony\Component\Yaml\Inline::parseMapping` is that
    /// exact shape, and `ProxyHelper::exportParameters` is its `?string &$args` twin, assigned an
    /// `array` by `explode`.
    ///
    /// elephc's cell is the caller's frame slot, so representing that needs BOTH sides to use a
    /// boxed one. The widened storage is published in two places, and they are the same statement
    /// read by two consumers:
    ///
    /// - [`Checker::widened_ref_params`], which the callee's environment seeding reads to bind the
    ///   parameter `mixed` for the body — so the incompatible store MERGES instead of reaching the
    ///   widening rule at all, exactly as a `mixed &$i` declaration already does today, and
    ///   `--strict-locals` accepts it for the same reason;
    /// - `by_ref_local_storage_types`, under the CALLEE's own scope key, which is the map
    ///   `LoweringContext::required_local_storage_type` consults when it declares the parameter's
    ///   slot. One entry gives the cell its boxed representation with no new plumbing, because the
    ///   scope keys the checker and EIR lowering use for a body are already the same string.
    ///
    /// The caller side is not recorded here: `prepare_by_ref_variable_storage` derives it per call
    /// site from the callee's effective storage, which is what makes an argument's own slot box.
    ///
    /// CLOSURES are deliberately out of scope. A closure's key is its SPAN inside an enclosing
    /// body (`nested_loop_storage_scope`), which nothing here can reconstruct without walking
    /// expressions in the same order the checker later does; a `function (&$x) { … }` that widens
    /// its parameter therefore keeps today's hard error rather than getting a decision the two
    /// sides might disagree about.
    pub(crate) fn scan_widened_ref_params(
        &mut self,
        program: &Program,
        classes: &[FlattenedClass],
    ) {
        let mut widened: Vec<(String, String)> = Vec::new();
        for stmt in program {
            let StmtKind::FunctionDecl { name, .. } = &stmt.kind else {
                continue;
            };
            let key = self
                .canonical_function_name_folded(name)
                .unwrap_or_else(|| name.clone());
            let Some(decl) = self.fn_decls.get(&key) else {
                continue;
            };
            let params: Vec<(String, Option<TypeExpr>, bool)> = decl
                .params
                .iter()
                .cloned()
                .zip(decl.param_types.iter().cloned())
                .zip(decl.ref_params.iter().copied())
                .map(|((name, type_expr), by_ref)| (name, type_expr, by_ref))
                .collect();
            self.collect_widened_ref_params(&key, &params, &decl.body.clone(), &mut widened);
        }
        for class in classes {
            for method in &class.methods {
                if !method.has_body {
                    continue;
                }
                let key = format!("{}::{}", class.name, method.name);
                let params: Vec<(String, Option<TypeExpr>, bool)> = method
                    .params
                    .iter()
                    .map(|(name, type_expr, _, by_ref)| {
                        (name.clone(), type_expr.clone(), *by_ref)
                    })
                    .collect();
                self.collect_widened_ref_params(&key, &params, &method.body, &mut widened);
            }
        }
        let (escaping, property_bound) =
            self.propagate_widened_ref_params(program, classes, &mut widened);
        // A parameter forwarded into a property-bound one carries the same cell, so its callers
        // must hand over a managed cell too.
        for (scope, param) in property_bound {
            self.property_bound_ref_params
                .insert((folded_scope_key(&scope), param));
        }
        // Codegen matches a call to its callee by NAME alone (a method call may reach several
        // implementations), so the escape fact is published as the bare lowercase name.
        for (scope, _) in &escaping {
            let name = scope.rsplit_once("::").map_or(scope.as_str(), |(_, method)| method);
            self.ref_param_escaping_callees.insert(php_symbol_key(name));
        }
        if std::env::var_os("ELEPHC_TRACE_REF_WIDEN").is_some() {
            for (scope, param) in &widened {
                eprintln!("[ref-widen] {scope} ${param}");
            }
            for (scope, param) in &escaping {
                eprintln!("[ref-escape] {scope} ${param}");
            }
            for (scope, param) in &self.property_bound_ref_params {
                eprintln!("[ref-property-bound] {scope} ${param}");
            }
        }
        for (scope, param) in widened {
            // EIR lowering looks the callee's own parameter slot up by the body scope key it
            // builds from the DECLARATION, so that entry keeps the exact spelling. The checker's
            // set is keyed by the case-FOLDED spelling instead, because a call site names the
            // callee the way the CALL spells it and PHP function and method names are
            // case-insensitive.
            self.by_ref_local_storage_types
                .insert((scope.clone(), param.clone()), PhpType::Mixed);
            self.widened_ref_params
                .insert((folded_scope_key(&scope), param.clone()));
            self.widened_ref_param_decls.push((scope, param));
        }
    }

    /// Widens a by-reference parameter that is FORWARDED into a widened by-reference parameter.
    ///
    /// The forwarded argument is the caller's own cell, so the callee boxes exactly the storage the
    /// caller's declaration still calls concrete. `ContainerBuilder::doGet(?array &$inlineServices)`
    /// hands its cell to `createService(array &$inlineServices)`, which writes string keys and is
    /// widened; `doGet` kept its array contract and read the box back as the TypeError
    /// "Value must be of type array, int given" on every `lint:container`. Runs to a fixpoint,
    /// because a forwarding chain can be any length. A method call resolves by NAME among the
    /// classes it can reach (the caller's own chain for `$this`/`self`/`parent`/`static`, every
    /// class otherwise), which errs towards widening -- a boxed cell costs speed, a missed one
    /// costs a wrong value.
    fn propagate_widened_ref_params(
        &self,
        program: &Program,
        classes: &[FlattenedClass],
        widened: &mut Vec<(String, String)>,
    ) -> (
        std::collections::HashSet<(String, String)>,
        std::collections::HashSet<(String, String)>,
    ) {
        let mut bodies: Vec<RefBody<'_>> = Vec::new();
        // By-reference parameters whose DECLARED type is already a boxed cell -- `?array &`,
        // `mixed &`, a non-int union. They never enter `widened` (their representation is right
        // as declared), but a body forwarding its own concrete cell into one hands the callee a
        // raw pointer it reads as a box: `ContainerBuilder::createService(array &$inlineServices)`
        // recursing into `doGet(?array &$inlineServices)` read the array header's first word as
        // the type tag -- "Value must be of type array, int given" on every `lint:container`.
        let mut boxed_by_declaration: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();
        let mut note_boxed = |scope: &str, param: &str, type_expr: Option<&TypeExpr>| {
            let Some(type_expr) = type_expr else {
                return;
            };
            if self
                .resolve_type_expr(type_expr, Span::dummy())
                .is_ok_and(|declared| declared.codegen_repr() == PhpType::Mixed)
            {
                boxed_by_declaration.insert((scope.to_string(), param.to_string()));
            }
        };
        for stmt in program {
            let StmtKind::FunctionDecl { name, .. } = &stmt.kind else {
                continue;
            };
            let key = self
                .canonical_function_name_folded(name)
                .unwrap_or_else(|| name.clone());
            let Some(decl) = self.fn_decls.get(&key) else {
                continue;
            };
            for ((param, type_expr), by_ref) in decl
                .params
                .iter()
                .zip(decl.param_types.iter())
                .zip(decl.ref_params.iter())
            {
                if *by_ref {
                    note_boxed(&key, param, type_expr.as_ref());
                }
            }
            bodies.push(RefBody {
                scope: key,
                class: None,
                params: decl
                    .params
                    .iter()
                    .cloned()
                    .zip(decl.ref_params.iter().copied())
                    .collect(),
                body: std::borrow::Cow::Owned(decl.body.clone()),
            });
        }
        for class in classes {
            for method in &class.methods {
                if !method.has_body {
                    continue;
                }
                let scope = format!("{}::{}", class.name, method.name);
                for (param, type_expr, _, by_ref) in &method.params {
                    if *by_ref {
                        note_boxed(&scope, param, type_expr.as_ref());
                    }
                }
                bodies.push(RefBody {
                    scope,
                    class: Some(class.name.clone()),
                    params: method
                        .params
                        .iter()
                        .map(|(name, _, _, by_ref)| (name.clone(), *by_ref))
                        .collect(),
                    body: std::borrow::Cow::Borrowed(&method.body),
                });
            }
        }
        let parents: HashMap<String, String> = classes
            .iter()
            .filter_map(|class| {
                class.extends.as_ref().map(|parent| {
                    (
                        php_symbol_key(class.name.trim_start_matches('\\')),
                        php_symbol_key(parent.trim_start_matches('\\')),
                    )
                })
            })
            .collect();
        let chain = |class: &str| -> Vec<String> {
            let mut out = vec![php_symbol_key(class.trim_start_matches('\\'))];
            while let Some(parent) = parents.get(out.last().expect("chain starts non-empty")) {
                if out.contains(parent) {
                    break;
                }
                out.push(parent.clone());
            }
            out
        };
        let related = |a: &str, b: &str| {
            chain(a).contains(&php_symbol_key(b.trim_start_matches('\\')))
                || chain(b).contains(&php_symbol_key(a.trim_start_matches('\\')))
        };
        // Whether `body` hands its by-reference `param` to a callee parameter `is_target` names.
        let forwards_into = |body: &RefBody<'_>,
                             param: &str,
                             is_target: &dyn Fn(&(String, String)) -> bool|
         -> bool {
            let mut forwards = false;
            for_each_call(&body.body, &mut |call| {
                if forwards {
                    return;
                }
                for (position, arg) in call.args.iter().enumerate() {
                    if !matches!(&arg.kind, ExprKind::Variable(name) if name == param) {
                        continue;
                    }
                    forwards |= bodies.iter().any(|callee| {
                        let target_matches = match &call.target {
                            CallTarget::Function(name) => {
                                callee.class.is_none()
                                    && self
                                        .canonical_function_name_folded(name)
                                        .is_some_and(|key| key == callee.scope)
                            }
                            CallTarget::Method {
                                method,
                                receiver_class,
                            } => {
                                let Some(owner) = callee.class.as_deref() else {
                                    return false;
                                };
                                let method_matches = callee
                                    .scope
                                    .rsplit_once("::")
                                    .is_some_and(|(_, m)| m.eq_ignore_ascii_case(method));
                                method_matches
                                    && match (receiver_class, body.class.as_deref()) {
                                        (Some(ReceiverClass::Own), Some(own)) => {
                                            related(own, owner)
                                        }
                                        (Some(ReceiverClass::Named(named)), _) => {
                                            related(named, owner)
                                        }
                                        _ => true,
                                    }
                            }
                            CallTarget::RefCapture(_) => false,
                        };
                        target_matches
                            && callee.params.get(position).is_some_and(
                                |(callee_param, callee_by_ref)| {
                                    *callee_by_ref
                                        && is_target(&(callee.scope.clone(), callee_param.clone()))
                                },
                            )
                    });
                }
            });
            forwards
        };
        // Grows `set` to a fixpoint: every by-reference parameter forwarded into a member joins.
        let grow = |set: &mut std::collections::HashSet<(String, String)>,
                    extra_targets: &std::collections::HashSet<(String, String)>|
         -> Vec<(String, String)> {
            let mut grown = Vec::new();
            loop {
                let mut added = Vec::new();
                for body in &bodies {
                    for (param, by_ref) in &body.params {
                        let key = (body.scope.clone(), param.clone());
                        if !by_ref || set.contains(&key) {
                            continue;
                        }
                        if forwards_into(body, param, &|target| {
                            set.contains(target) || extra_targets.contains(target)
                        }) {
                            added.push(key);
                        }
                    }
                }
                if added.is_empty() {
                    return grown;
                }
                for entry in added {
                    if set.insert(entry.clone()) {
                        grown.push(entry);
                    }
                }
            }
        };
        // By-reference parameters a closure binds with `use (&$param)`. Two facts follow.
        //
        // WIDENED: EIR lowering gives a by-reference capture a boxed `Mixed` cell, and the body
        // reads the parameter through that representation -- while the declaration, and so every
        // caller, still said `array`. The closure's first string-keyed write stored a box the
        // caller then read as a raw array ("count(): Argument #1 must be of type
        // Countable|array, string given").
        //
        // ESCAPES: the closure can outlive the call, and the cell with it, so a caller omitting
        // that argument must give it heap storage rather than a stack temporary.
        // `ContainerBuilder::doGet($id)` omits `$inlineServices`, forwards it through
        // `createService` into `doResolveServices`, whose `RewindableGenerator` closure captures
        // it and runs long after `doGet` returned -- reading freed stack as the container.
        let mut captured: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();
        let mut property_bound: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();
        for body in &bodies {
            for (param, by_ref) in &body.params {
                if !by_ref {
                    continue;
                }
                // A property binding ESCAPES but is not widened: the property keeps its own
                // declared representation, which the caller's cell already has.
                if binds_property_to_ref_param(&body.body, param)
                    || binds_array_element_to_ref_param(&body.body, param)
                {
                    property_bound.insert((body.scope.clone(), param.clone()));
                }
                let mut is_captured = false;
                for_each_call(&body.body, &mut |call| {
                    if let CallTarget::RefCapture(names) = call.target {
                        is_captured |= names.iter().any(|name| name == param);
                    }
                });
                if is_captured {
                    captured.insert((body.scope.clone(), param.clone()));
                }
            }
        }

        let mut widened_set: std::collections::HashSet<(String, String)> =
            widened.iter().cloned().collect();
        for entry in &captured {
            if widened_set.insert(entry.clone()) {
                widened.push(entry.clone());
            }
        }
        widened.extend(grow(&mut widened_set, &boxed_by_declaration));

        let mut escaping = captured;
        grow(&mut escaping, &std::collections::HashSet::new());
        grow(&mut property_bound, &std::collections::HashSet::new());
        (escaping, property_bound)
    }

    /// Rewrites every widened by-reference parameter's type in the resolved signatures, once all
    /// checking is done.
    ///
    /// The signature is the ABI, and by this point it is nothing else: every diagnostic that reads
    /// it has already run, so the DECLARED type has finished doing the one job only it can do —
    /// rejecting an argument PHP rejects, `parameter $i expects Int, got Str`. What EIR lowering
    /// needs from the same field afterwards is the cell's runtime REPRESENTATION, and for these
    /// parameters that is a boxed `Mixed` on both sides of the call. Publishing it here rather
    /// than threading a parallel table is what keeps the callee's slot, the call site's argument
    /// coercion and `ir_lower::expr::ref_place_args`'s adapter decision reading ONE fact: the
    /// adapter exists to bridge a boxed caller slot to a CONCRETE declared cell, and a cell that
    /// is no longer concrete must not get one — measured, the concrete `int` temporary it built
    /// made `adv("ab:cd", $p)` print a raw pointer where `php -n` prints `int(2)`.
    ///
    /// Runs only on the success path: a program with errors never reaches lowering, and leaving
    /// the signatures alone keeps every diagnostic quoting the declared type.
    pub(crate) fn publish_widened_ref_param_signatures(&mut self) {
        for (scope, param) in std::mem::take(&mut self.widened_ref_param_decls) {
            let Some((owner, method_name)) = scope.split_once("::") else {
                if let Some(signature) = self.functions.get_mut(&scope) {
                    widen_signature_param(signature, &param);
                }
                continue;
            };
            let key = php_symbol_key(method_name);
            // EVERY class that runs this body, not just the one that declares it. `ClassInfo` is
            // flattened per class, so a subclass carries its own copy of an inherited method's
            // signature — and a call site reads the RECEIVER's copy. Leaving a descendant's copy
            // at the declared type would hand `ir_lower::expr::ref_place_args` a concrete
            // `expected` for a cell the callee opens boxed, which is the adapter shape that prints
            // a raw pointer. `method_impl_classes` is the same map codegen uses to find a method's
            // implementation, so "runs this body" cannot mean two different things in two places.
            let targets: Vec<String> = self
                .classes
                .iter()
                .filter(|(class_name, info)| {
                    let implementer = info
                        .method_impl_classes
                        .get(&key)
                        .or_else(|| info.static_method_impl_classes.get(&key));
                    match implementer {
                        Some(implementer) => implementer == owner,
                        // No entry means the method is not inherited from anywhere: the class
                        // runs the body only if it IS the owner.
                        None => class_name.as_str() == owner,
                    }
                })
                .map(|(class_name, _)| class_name.clone())
                .collect();
            for class_name in targets {
                let Some(info) = self.classes.get_mut(&class_name) else {
                    continue;
                };
                for signature in info
                    .methods
                    .get_mut(&key)
                    .into_iter()
                    .chain(info.static_methods.get_mut(&key))
                {
                    widen_signature_param(signature, &param);
                }
            }
        }
    }

    /// Records every declared by-reference parameter of ONE body that the body cannot keep at its
    /// declared representation.
    fn collect_widened_ref_params(
        &self,
        scope: &str,
        params: &[(String, Option<TypeExpr>, bool)],
        body: &[Stmt],
        widened: &mut Vec<(String, String)>,
    ) {
        let by_ref_declared: Vec<(&str, PhpType)> = params
            .iter()
            .filter(|(_, type_expr, by_ref)| *by_ref && type_expr.is_some())
            .filter_map(|(name, type_expr, _)| {
                let declared = self
                    .resolve_type_expr(type_expr.as_ref()?, Span::dummy())
                    .ok()?;
                Some((name.as_str(), declared))
            })
            .collect();
        if by_ref_declared.is_empty() {
            return;
        }
        let element_keys: Vec<String> = by_ref_declared
            .iter()
            .map(|(name, _)| element_write_bucket(name))
            .collect();
        let mut stores: HashMap<&str, Vec<&Expr>> = HashMap::new();
        for (name, _) in &by_ref_declared {
            stores.insert(name, Vec::new());
        }
        for key in &element_keys {
            stores.insert(key.as_str(), Vec::new());
        }
        collect_local_stores(body, &mut stores);
        // A `array &$p` parameter's callers may hand it a HASH (`function f(array $params)` fed a
        // `['id' => 1]` route-parameter map hands it on). The indexed-only builtins below rebuild
        // or drop entries by POSITION and renumber integer keys, so over hash storage they read
        // the header as a list: `array_shift`/`array_pop` answered garbage and `array_splice`,
        // `array_unshift`, `sort`, `rsort` and `usort` took a bus error. With the cell boxed the
        // same calls dispatch on the runtime kind, which is what `mixed &$p` already does.
        // Laravel's `ResolvesRouteDependencies::spliceIntoParameters(array &$parameters, ...)`.
        let mut positional_receivers: Vec<&str> = Vec::new();
        for_each_call(body, &mut |call| {
            let CallTarget::Function(name) = &call.target else {
                return;
            };
            if !POSITIONAL_ARRAY_BUILTINS
                .contains(&name.trim_start_matches('\\').to_ascii_lowercase().as_str())
            {
                return;
            }
            if let Some(ExprKind::Variable(receiver)) = call.args.first().map(|arg| &arg.kind) {
                positional_receivers.push(receiver.as_str());
            }
        });
        // No parameter is filtered out up front, including one already declared `mixed`: the
        // predicate below answers `false` for it (`mixed` merges everything and keeps its own
        // representation), and a declared UNION must NOT be filtered out on the strength of
        // having a boxed representation — `?string &$args` assigned `explode`'s array is exactly
        // `ProxyHelper::exportParameters`, whose cell is boxed already and whose BINDING the
        // checker still refuses.
        for (name, declared) in by_ref_declared {
            let widens = stores
                .get(name)
                .into_iter()
                .flatten()
                .filter_map(|value| self.stored_value_type(value))
                .any(|stored| self.store_leaves_declared_representation(&declared, &stored));
            // A string-keyed ELEMENT write is a store too: it promotes an indexed array to a hash,
            // and the promoted hash is what the caller's cell receives. With the declaration still
            // saying "indexed", the next entry re-promoted a hash as if it were a list and
            // corrupted it -- Symfony's recursive
            // `DefinitionErrorExceptionPass::isErrorForRuntime(string $id, array &$visitedIds)`
            // writing `$visitedIds[$id]`.
            let promotes_to_hash = matches!(declared.codegen_repr(), PhpType::Array(_))
                && stores
                    .get(element_write_bucket(name).as_str())
                    .into_iter()
                    .flatten()
                    .any(|index| index_is_string_key(index, params));
            let positional_on_array = matches!(declared.codegen_repr(), PhpType::Array(_))
                && positional_receivers.contains(&name);
            if widens || promotes_to_hash || positional_on_array {
                widened.push((scope.to_string(), name.to_string()));
            }
        }
    }

    /// Returns whether storing `stored` into a `declared` cell takes it off the declaration's
    /// runtime REPRESENTATION.
    ///
    /// The merge is `Checker::merged_assignment_type`, the same one the assignment itself will
    /// run, so this cannot answer differently from the checker about what the name ends up
    /// holding. What is compared afterwards is not the type but the `codegen_repr`, because that
    /// is the only thing the cell has to be able to hold: a nullable `?string` merges `null` and
    /// `string` without leaving `Str` storage, an `int` merges `int` and stays, and both keep
    /// their raw cell. A merge that comes back `None` — `?string` against `explode`'s array — or
    /// one that lands on a different representation — `int` against `strpos`, whose registered
    /// return type is `mixed` — is what needs a boxed one.
    ///
    /// The acceptance predicates are deliberately NOT the test. `Checker::types_compatible` and
    /// `Checker::type_accepts` are gradual: they answer `true` for `mixed` against every scalar,
    /// which is right for a call boundary that will narrow at runtime and wrong for a reference
    /// cell that has to STORE the value.
    fn store_leaves_declared_representation(&self, declared: &PhpType, stored: &PhpType) -> bool {
        // An UNKNOWN value stored into a declared UNION. The union's cell is boxed already, so
        // this widens nothing at runtime — it only stops the body's binding rule from refusing a
        // store it cannot see the type of, which for a union `merged_assignment_type` does for
        // every value the union does not literally contain. `ProxyHelper::exportParameters`'
        // `?string &$args` is the shape: `$args = \explode(", ", $args, 2)` is
        // `cannot reassign $args from string|null to array<string>`, and the registered return
        // type of `explode` is the same coarse `mixed` every builtin whose real return type is a
        // union carries here (the precise one lives in a `check` hook that needs a call context
        // this pass has not got). A declared `mixed` is excluded because it already merges
        // everything, and a CONCRETE declaration is excluded because widening it really would
        // change the ABI — that needs the representation evidence below, not a shrug.
        if matches!(stored, PhpType::Mixed)
            && declared.codegen_repr() == PhpType::Mixed
            && !matches!(declared, PhpType::Mixed)
        {
            return true;
        }
        // A container pointer has no null: `array &$options` then `$options = null;`, which is how
        // `AmpResponse::generateResponse()` drops what the request no longer needs.
        if matches!(stored, PhpType::Void)
            && matches!(declared.codegen_repr(), PhpType::Array(_) | PhpType::AssocArray { .. })
        {
            return true;
        }
        match self.merged_assignment_type(declared, stored) {
            None => true,
            // An UNKNOWN value stored into a declared CONTAINER is not evidence. `mixed` is what
            // every builtin whose real return type this pass cannot reach carries here, and for a
            // container that is nearly always the same kind of container coming back:
            // `function __elephc_usort_mixed(array &$values, …) { $values = \array_values($values); }`
            // — the compiler's own sort prelude — was boxed on the strength of `array_values`'
            // registered `mixed`, which turned an array cell into a `Mixed` one under callers and
            // a runtime that both expect a container pointer, and `usort` over objects segfaulted
            // (`codegen::arrays::callbacks::test_usort_objects_typed_comparator`).
            //
            // A raw SCALAR cell is the opposite case and keeps the evidence: it is a register or a
            // pair with no room for a tag, so an unknown value genuinely cannot live in it, and
            // that is `int &$i` against `\strpos` — the shape this whole pass exists for. The
            // asymmetry is about what the representation can HOLD, not about how sure the pass is.
            Some(merged) => {
                merged.codegen_repr() != declared.codegen_repr()
                    && (!matches!(stored, PhpType::Mixed) || declared_is_raw_scalar(declared))
            }
        }
    }

    /// Returns the type a body stores, or `None` when this pass cannot name it.
    ///
    /// Deliberately narrow: a value whose type is only known once the body is really checked is
    /// NOT evidence, because the cost of missing one is the pre-existing hard error while the cost
    /// of guessing one wrong is a boxed cell on both sides of every call. The shapes answered here
    /// are the ones whose type is a property of the callee's own declaration rather than of the
    /// surrounding flow — a literal, a cast, a concatenation, a container literal, a constructor,
    /// and a call whose signature is already registered (every builtin, and a user function or
    /// method that DECLARED its return type).
    fn stored_value_type(&self, value: &Expr) -> Option<PhpType> {
        match &value.kind {
            ExprKind::StringLiteral(_) => Some(PhpType::Str),
            ExprKind::IntLiteral(_) => Some(PhpType::Int),
            ExprKind::FloatLiteral(_) => Some(PhpType::Float),
            ExprKind::BoolLiteral(_) => Some(PhpType::Bool),
            ExprKind::Null => Some(PhpType::Void),
            ExprKind::ArrayLiteral(_) => Some(PhpType::Array(Box::new(PhpType::Mixed))),
            ExprKind::ArrayLiteralAssoc(_) => Some(PhpType::AssocArray {
                key: Box::new(PhpType::Mixed),
                value: Box::new(PhpType::Mixed),
            }),
            ExprKind::Cast { target, .. } => match target {
                crate::parser::ast::CastType::String => Some(PhpType::Str),
                crate::parser::ast::CastType::Int => Some(PhpType::Int),
                crate::parser::ast::CastType::Float => Some(PhpType::Float),
                crate::parser::ast::CastType::Bool => Some(PhpType::Bool),
                crate::parser::ast::CastType::Array => {
                    Some(PhpType::Array(Box::new(PhpType::Mixed)))
                }
                _ => None,
            },
            ExprKind::BinaryOp {
                op: crate::parser::ast::BinOp::Concat,
                ..
            } => Some(PhpType::Str),
            ExprKind::NewObject { class_name, .. } => {
                Some(PhpType::Object(class_name.as_str().to_string()))
            }
            ExprKind::Clone(inner) => self.stored_value_type(inner),
            ExprKind::ErrorSuppress(inner) => self.stored_value_type(inner),
            ExprKind::FunctionCall { name, .. } => {
                let key = php_symbol_key(name.trim_start_matches('\\'));
                if let Some(signature) = crate::types::builtin_call_sig(&key) {
                    return Some(signature.return_type);
                }
                let canonical = self
                    .canonical_function_name_folded(name)
                    .unwrap_or_else(|| name.to_string());
                // Only a DECLARED return type is a fact about the callee. An inferred one is a
                // fact about a walk that has not happened yet.
                let declaration = self.fn_decls.get(&canonical)?;
                let return_type = declaration.return_type.clone()?;
                self.resolve_type_expr(&return_type, value.span).ok()
            }
            _ => None,
        }
    }
}

/// Collects every statement-form assignment `$name = <expr>` for the tracked names, at any
/// conditional depth.
///
/// Depth is irrelevant to this question: a widening keeps the slot and boxes it, so it is sound on
/// the taken and the untaken path alike — the same reason `Checker::local_binding_is_widenable`
/// drops the conditional-depth condition the kill needs. Closure bodies are not descended into
/// (see [`Checker::scan_widened_ref_params`]), and neither are nested function or class
/// declarations, whose parameters are their own scope's.
fn collect_local_stores<'a>(stmts: &'a [Stmt], out: &mut HashMap<&str, Vec<&'a Expr>>) {
    for stmt in stmts {
        collect_local_stores_in_stmt(stmt, out);
    }
}

/// Walks one statement and the executable bodies nested inside it.
fn collect_local_stores_in_stmt<'a>(stmt: &'a Stmt, out: &mut HashMap<&str, Vec<&'a Expr>>) {
    match &stmt.kind {
        StmtKind::ArrayAssign { array, index, value } => {
            if let Some(indexes) = out.get_mut(element_write_bucket(array).as_str()) {
                indexes.push(index);
            }
            collect_local_stores_in_expr(index, out);
            collect_local_stores_in_expr(value, out);
        }
        StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
            if let Some(values) = out.get_mut(name.as_str()) {
                values.push(value);
            }
            collect_local_stores_in_expr(value, out);
        }
        StmtKind::ExprStmt(expr) | StmtKind::Echo(expr) | StmtKind::Throw(expr) => {
            collect_local_stores_in_expr(expr, out)
        }
        StmtKind::Return(Some(expr)) => collect_local_stores_in_expr(expr, out),
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            collect_local_stores_in_expr(condition, out);
            collect_local_stores(then_body, out);
            for (clause_condition, body) in elseif_clauses {
                collect_local_stores_in_expr(clause_condition, out);
                collect_local_stores(body, out);
            }
            if let Some(body) = else_body {
                collect_local_stores(body, out);
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            collect_local_stores_in_expr(condition, out);
            collect_local_stores(body, out);
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_local_stores_in_stmt(init, out);
            }
            if let Some(condition) = condition {
                collect_local_stores_in_expr(condition, out);
            }
            if let Some(update) = update {
                collect_local_stores_in_stmt(update, out);
            }
            collect_local_stores(body, out);
        }
        StmtKind::Foreach { array, body, .. } => {
            collect_local_stores_in_expr(array, out);
            collect_local_stores(body, out);
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            collect_local_stores_in_expr(subject, out);
            for (values, body) in cases {
                for value in values {
                    collect_local_stores_in_expr(value, out);
                }
                collect_local_stores(body, out);
            }
            if let Some(body) = default {
                collect_local_stores(body, out);
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            collect_local_stores(try_body, out);
            for catch in catches {
                collect_local_stores(&catch.body, out);
            }
            if let Some(body) = finally_body {
                collect_local_stores(body, out);
            }
        }
        StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. } => collect_local_stores(body, out),
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            collect_local_stores(then_body, out);
            if let Some(body) = else_body {
                collect_local_stores(body, out);
            }
        }
        _ => {}
    }
}

/// Walks the statement lists an expression carries, for the assignment forms that live there.
///
/// An `$x = …` written in EXPRESSION position (`$a = $x = f();`, `while ($x = next())`) is an
/// `ExprKind::Assignment`, not a `StmtKind::Assign`, and it stores through the reference just the
/// same.
fn collect_local_stores_in_expr<'a>(expr: &'a Expr, out: &mut HashMap<&str, Vec<&'a Expr>>) {
    match &expr.kind {
        ExprKind::Assignment {
            target,
            value,
            prelude,
            ..
        } => {
            collect_local_stores(prelude, out);
            if let ExprKind::Variable(name) = &target.kind {
                if let Some(values) = out.get_mut(name.as_str()) {
                    values.push(value);
                }
            }
            if let ExprKind::ArrayAccess { array, index } = &target.kind {
                if let ExprKind::Variable(name) = &array.kind {
                    if let Some(indexes) = out.get_mut(element_write_bucket(name).as_str()) {
                        indexes.push(index);
                    }
                }
            }
            collect_local_stores_in_expr(value, out);
        }
        ExprKind::BinaryOp { left, right, .. } => {
            collect_local_stores_in_expr(left, out);
            collect_local_stores_in_expr(right, out);
        }
        // `$v[$k] ?? $v[$k] = f()` keeps its write in the DEFAULT arm, and Symfony writes its
        // memoized recursion exactly that way.
        ExprKind::NullCoalesce { value, default } | ExprKind::ShortTernary { value, default } => {
            collect_local_stores_in_expr(value, out);
            collect_local_stores_in_expr(default, out);
        }
        ExprKind::ArrayAccess { array, index } => {
            collect_local_stores_in_expr(array, out);
            collect_local_stores_in_expr(index, out);
        }
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                collect_local_stores_in_expr(item, out);
            }
        }
        ExprKind::ArrayLiteralAssoc(pairs) => {
            for (key, value) in pairs {
                collect_local_stores_in_expr(key, out);
                collect_local_stores_in_expr(value, out);
            }
        }
        ExprKind::NewObject { args, .. } => {
            for arg in args {
                collect_local_stores_in_expr(arg, out);
            }
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Cast { expr: inner, .. } => collect_local_stores_in_expr(inner, out),
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_local_stores_in_expr(condition, out);
            collect_local_stores_in_expr(then_expr, out);
            collect_local_stores_in_expr(else_expr, out);
        }
        ExprKind::FunctionCall { args, .. } => {
            for arg in args {
                collect_local_stores_in_expr(arg, out);
            }
        }
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            collect_local_stores_in_expr(object, out);
            for arg in args {
                collect_local_stores_in_expr(arg, out);
            }
        }
        ExprKind::StaticMethodCall { args, .. } => {
            for arg in args {
                collect_local_stores_in_expr(arg, out);
            }
        }
        _ => {}
    }
}

impl Checker {
    /// Returns the type a body's environment binds `param` to on entry.
    ///
    /// `PhpType::Mixed` for a by-reference parameter [`Checker::scan_widened_ref_params`] widened,
    /// and `declared` unchanged for everything else. Applied to the ENVIRONMENT only: the
    /// signature keeps the declared type, so a call site still reports
    /// `parameter $i expects Int, got Str` for an argument PHP rejects with a TypeError.
    pub(crate) fn widened_ref_param_env_type(
        &self,
        scope: &str,
        param: &str,
        declared: &PhpType,
    ) -> PhpType {
        if self.ref_param_is_widened(scope, param) {
            PhpType::Mixed
        } else {
            declared.clone()
        }
    }

    /// Whether some body binds a property to by-reference parameter `param` of `scope`, or,
    /// with no resolved callee, of any user callee.
    pub(crate) fn ref_param_is_property_bound(&self, scope: Option<&str>, param: &str) -> bool {
        match scope {
            Some(scope) => self
                .property_bound_ref_params
                .contains(&(folded_scope_key(scope), param.to_string())),
            None => self
                .property_bound_ref_params
                .iter()
                .any(|(_, bound)| bound == param),
        }
    }

    /// Returns whether `param`'s by-reference cell in `scope` was widened to boxed storage.
    pub(crate) fn ref_param_is_widened(&self, scope: &str, param: &str) -> bool {
        self.widened_ref_params
            .contains(&(folded_scope_key(scope), param.to_string()))
    }

    /// Returns the storage a CALL SITE must give the caller's variable for one by-reference
    /// parameter of `callee`.
    ///
    /// The declared type, except where the callee's body widens the parameter — there the cell is
    /// boxed and the caller's slot has to be boxed with it, or the callee's write lands in a slot
    /// that cannot hold it and the caller reads the box back as whatever its declared
    /// representation says. `None` for a callee this pass cannot name (a closure, a runtime
    /// callable, a `$f(...)`), which leaves the call on its pre-existing storage decision.
    pub(crate) fn effective_ref_param_type(
        &self,
        callee_scope: Option<&str>,
        param: &str,
        declared: &PhpType,
    ) -> PhpType {
        match callee_scope {
            Some(scope) => self.widened_ref_param_env_type(scope, param, declared),
            None => declared.clone(),
        }
    }

    /// Names the BODY that a `Class::method` call actually runs, for the widening lookup.
    ///
    /// An inherited method's body belongs to the class that DECLARED it, and that is the scope the
    /// pre-pass scanned — `$child->m($x)` where `m` comes from `Parent` has to ask about
    /// `Parent::m`. `method_impl_classes` is the same map codegen uses to find a method's
    /// implementation, so the two cannot drift; a method with no entry is answered against the
    /// receiver's own class, which is the non-inherited case.
    pub(crate) fn method_body_scope(&self, class_name: &str, method: &str) -> Option<String> {
        let class_name = class_name.trim_start_matches('\\');
        let info = self.classes.get(class_name)?;
        let key = php_symbol_key(method);
        let owner = info
            .method_impl_classes
            .get(&key)
            .or_else(|| info.static_method_impl_classes.get(&key))
            .map(String::as_str)
            .unwrap_or(class_name);
        Some(format!("{}::{}", owner, method))
    }
}

/// Case-folds a body scope key so a call site's spelling of the callee matches the declaration's.
///
/// PHP function and class/method names are case-insensitive, and the scope key is built from the
/// DECLARATION on one side and from the CALL on the other. `php_symbol_key` is applied per segment
/// so `Foo::Bar` and `foo::bar` agree without `Foo::bar` colliding with a function named `foo::bar`
/// (which cannot exist).
pub(crate) fn folded_scope_key(scope: &str) -> String {
    match scope.split_once("::") {
        Some((class, method)) => {
            format!("{}::{}", php_symbol_key(class), php_symbol_key(method))
        }
        None => php_symbol_key(scope),
    }
}

/// Replaces one named parameter's type with boxed `Mixed` in a resolved signature.
fn widen_signature_param(signature: &mut crate::types::FunctionSig, param: &str) {
    for (name, ty) in &mut signature.params {
        if name == param {
            *ty = PhpType::Mixed;
        }
    }
}

/// Returns whether a declared type's cell is a raw scalar — a register or a register pair with no
/// room for a runtime tag.
///
/// `TaggedScalar` is included: a nullable integer carries its tag in a second word rather than in a
/// box, so it can no more hold an arbitrary value than a bare `int` can.
/// Names the bucket that collects the INDEX expressions of `$name[...] = …` writes.
///
/// Shares the store map with the whole-variable stores; a NUL cannot appear in a PHP variable
/// name, so the bucket can never collide with a real one.
fn element_write_bucket(name: &str) -> String {
    format!("{name}\u{0}[]")
}

/// Returns whether an element write's key is a string this pass can see without checking the body.
///
/// A literal, a concatenation, or a parameter declared `string`. A numeric string literal is an
/// integer key in PHP (`$a["1"]` writes key 1), so it keeps a list a list and is not counted.
fn index_is_string_key(index: &Expr, params: &[(String, Option<TypeExpr>, bool)]) -> bool {
    match &index.kind {
        ExprKind::StringLiteral(value) => value.parse::<i64>().is_err(),
        ExprKind::BinaryOp {
            op: crate::parser::ast::BinOp::Concat,
            ..
        } => true,
        ExprKind::Variable(name) => params
            .iter()
            .any(|(param, type_expr, _)| param == name && matches!(type_expr, Some(TypeExpr::Str))),
        _ => false,
    }
}

fn declared_is_raw_scalar(declared: &PhpType) -> bool {
    matches!(
        declared.codegen_repr(),
        PhpType::Int
            | PhpType::Float
            | PhpType::Bool
            | PhpType::False
            | PhpType::Str
            | PhpType::TaggedScalar
    )
}

/// Returns whether a body binds a property to by-reference parameter `param`'s cell:
/// `$this->p = &$param`, which the parser spells as the pair `$this->p = $param;
/// $param = &$this->p;` (`parse_complex_ref_assignment`).
///
/// The property keeps the CALLER's reference for the object's whole life, exactly like a closure
/// capturing `use (&$param)`, so the parameter escapes: the caller must hand over a managed cell
/// that outlives the call, and the callee binds the property to that cell
/// (`ir_lower::stmt::param_property_reference`). Symfony's `ContainerConfigurator` and
/// `ServicesConfigurator` constructors share the loader's `$instanceof` this way.
pub(crate) fn binds_property_to_ref_param(body: &[Stmt], param: &str) -> bool {
    let mut found = false;
    crate::parser::ast::visit::walk_stmts(body, &mut |stmt| {
        found |= crate::parser::ast::visit::param_property_reference_pair(stmt).is_some_and(|(_, _, name)| name == param);
    });
    found
}

/// Returns whether a body stores a reference to by-reference parameter `param` into an array:
/// `$store[] = &$param`, `$store['k'] = &$param` or `[&$param]`.
///
/// The array keeps the CALLER's reference for as long as it lives, exactly like a property bound
/// to the parameter, so the caller must hand over a managed cell. A variable lent by its frame
/// address was released into a marker that incremented and later decremented a "header" in the
/// caller's stack: `keep($v)` with `function keep(&$r) { global $refs; $refs[] = &$r; }` ended
/// in `heap debug detected bad refcount` and a corrupted stack neighbour.
pub(crate) fn binds_array_element_to_ref_param(body: &[Stmt], param: &str) -> bool {
    let mut found = false;
    crate::parser::ast::visit::walk_nodes(body, &mut |node| {
        if let crate::parser::ast::visit::Node::Expr(expr) = node {
            if let ExprKind::ArrayReference(inner) = &expr.kind {
                found |= matches!(&inner.kind, ExprKind::Variable(name) if name == param);
            }
        }
    });
    found
}

/// One function or method body with its parameters, for the forwarding fixpoint.
struct RefBody<'a> {
    scope: String,
    class: Option<String>,
    params: Vec<(String, bool)>,
    body: std::borrow::Cow<'a, [Stmt]>,
}

/// Whose class a method call's receiver names, when the call says.
enum ReceiverClass {
    /// `$this->m()`, `self::m()`, `static::m()`, `parent::m()`: the calling body's own chain.
    Own,
    /// `Foo::m()`.
    Named(String),
}

/// What a call reaches, as far as this pass can tell from syntax.
enum CallTarget<'a> {
    Function(String),
    Method {
        method: String,
        receiver_class: Option<ReceiverClass>,
    },
    /// Not a call: a closure literal binding these enclosing variables by reference. Reported
    /// through the same walk because the escape analysis needs both from one body.
    RefCapture(&'a [String]),
}

/// One call site: its target and its positional arguments.
struct CallSite<'a> {
    target: CallTarget<'a>,
    args: &'a [Expr],
}

/// By-reference array builtins that work by position and renumber integer keys, and so cannot
/// run over hash storage when their receiver's representation is the dense indexed one.
const POSITIONAL_ARRAY_BUILTINS: &[&str] = &[
    "array_shift",
    "array_unshift",
    "array_pop",
    "array_splice",
    "sort",
    "rsort",
    "usort",
];

/// Visits every call in a body, including calls nested in expressions and in nested statements.
///
/// Closure bodies are not entered, for the reason `collect_local_stores` gives.
fn for_each_call<'a>(stmts: &'a [Stmt], visit: &mut dyn FnMut(CallSite<'a>)) {
    for stmt in stmts {
        for_each_call_in_stmt(stmt, visit);
    }
}

fn for_each_call_in_stmt<'a>(stmt: &'a Stmt, visit: &mut dyn FnMut(CallSite<'a>)) {
    match &stmt.kind {
        StmtKind::Assign { value, .. }
        | StmtKind::TypedAssign { value, .. }
        | StmtKind::ExprStmt(value)
        | StmtKind::Echo(value)
        | StmtKind::Throw(value)
        | StmtKind::Return(Some(value))
        | StmtKind::StaticPropertyAssign { value, .. } => for_each_call_in_expr(value, visit),
        StmtKind::ArrayAssign { index, value, .. } => {
            for_each_call_in_expr(index, visit);
            for_each_call_in_expr(value, visit);
        }
        StmtKind::ArrayPush { value, .. } => for_each_call_in_expr(value, visit),
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            for_each_call_in_expr(object, visit);
            for_each_call_in_expr(value, visit);
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            for_each_call_in_expr(condition, visit);
            for_each_call(then_body, visit);
            for (clause_condition, body) in elseif_clauses {
                for_each_call_in_expr(clause_condition, visit);
                for_each_call(body, visit);
            }
            if let Some(body) = else_body {
                for_each_call(body, visit);
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            for_each_call_in_expr(condition, visit);
            for_each_call(body, visit);
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                for_each_call_in_stmt(init, visit);
            }
            if let Some(condition) = condition {
                for_each_call_in_expr(condition, visit);
            }
            if let Some(update) = update {
                for_each_call_in_stmt(update, visit);
            }
            for_each_call(body, visit);
        }
        StmtKind::Foreach { array, body, .. } => {
            for_each_call_in_expr(array, visit);
            for_each_call(body, visit);
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            for_each_call_in_expr(subject, visit);
            for (values, body) in cases {
                for value in values {
                    for_each_call_in_expr(value, visit);
                }
                for_each_call(body, visit);
            }
            if let Some(body) = default {
                for_each_call(body, visit);
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            for_each_call(try_body, visit);
            for catch in catches {
                for_each_call(&catch.body, visit);
            }
            if let Some(body) = finally_body {
                for_each_call(body, visit);
            }
        }
        StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. } => for_each_call(body, visit),
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            for_each_call(then_body, visit);
            if let Some(body) = else_body {
                for_each_call(body, visit);
            }
        }
        _ => {}
    }
}

fn for_each_call_in_expr<'a>(expr: &'a Expr, visit: &mut dyn FnMut(CallSite<'a>)) {
    use crate::parser::ast::StaticReceiver;
    match &expr.kind {
        ExprKind::FunctionCall { name, args } => {
            visit(CallSite {
                target: CallTarget::Function(name.as_str().to_string()),
                args,
            });
            for arg in args {
                for_each_call_in_expr(arg, visit);
            }
        }
        ExprKind::MethodCall {
            object,
            method,
            args,
        }
        | ExprKind::NullsafeMethodCall {
            object,
            method,
            args,
        } => {
            let receiver_class =
                matches!(object.kind, ExprKind::This).then_some(ReceiverClass::Own);
            visit(CallSite {
                target: CallTarget::Method {
                    method: method.clone(),
                    receiver_class,
                },
                args,
            });
            for_each_call_in_expr(object, visit);
            for arg in args {
                for_each_call_in_expr(arg, visit);
            }
        }
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => {
            let receiver_class = Some(match receiver {
                // A generic receiver is instantiated into an ordinary named one before type checking;
                // a template has no class to reach through.
                StaticReceiver::Generic(_) => unreachable!(
                    "StaticReceiver::Generic must be instantiated by generics::classes"
                ),
                StaticReceiver::Named(name) => {
                    ReceiverClass::Named(name.as_str().trim_start_matches('\\').to_string())
                }
                StaticReceiver::Self_ | StaticReceiver::Static | StaticReceiver::Parent => {
                    ReceiverClass::Own
                }
            });
            visit(CallSite {
                target: CallTarget::Method {
                    method: method.clone(),
                    receiver_class,
                },
                args,
            });
            for arg in args {
                for_each_call_in_expr(arg, visit);
            }
        }
        // `new Foo(...)` runs `Foo::__construct`, and a constructor is where a by-reference
        // parameter is most often kept (`$this->p = &$param`).
        ExprKind::NewObject { class_name, args } => {
            visit(CallSite {
                target: CallTarget::Method {
                    method: "__construct".to_string(),
                    receiver_class: Some(ReceiverClass::Named(
                        class_name.as_str().trim_start_matches('\\').to_string(),
                    )),
                },
                args,
            });
            for arg in args {
                for_each_call_in_expr(arg, visit);
            }
        }
        ExprKind::Assignment {
            target,
            value,
            prelude,
            ..
        } => {
            for_each_call(prelude, visit);
            for_each_call_in_expr(target, visit);
            for_each_call_in_expr(value, visit);
        }
        ExprKind::BinaryOp { left, right, .. } => {
            for_each_call_in_expr(left, visit);
            for_each_call_in_expr(right, visit);
        }
        ExprKind::NullCoalesce { value, default } | ExprKind::ShortTernary { value, default } => {
            for_each_call_in_expr(value, visit);
            for_each_call_in_expr(default, visit);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            for_each_call_in_expr(condition, visit);
            for_each_call_in_expr(then_expr, visit);
            for_each_call_in_expr(else_expr, visit);
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Cast { expr: inner, .. } => for_each_call_in_expr(inner, visit),
        ExprKind::ArrayAccess { array, index } => {
            for_each_call_in_expr(array, visit);
            for_each_call_in_expr(index, visit);
        }
        ExprKind::PropertyAccess { object, .. } => for_each_call_in_expr(object, visit),
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                for_each_call_in_expr(item, visit);
            }
        }
        ExprKind::ArrayLiteralAssoc(pairs) => {
            for (key, value) in pairs {
                for_each_call_in_expr(key, visit);
                for_each_call_in_expr(value, visit);
            }
        }
        // The body is not entered (see `for_each_call`), but the capture list belongs to the
        // enclosing body: `use (&$x)` binds the enclosing `$x`'s cell into the closure.
        ExprKind::Closure { capture_refs, .. } if !capture_refs.is_empty() => {
            visit(CallSite {
                target: CallTarget::RefCapture(capture_refs),
                args: &[],
            });
        }
        _ => {}
    }
}
