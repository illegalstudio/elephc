//! Purpose:
//! Runs method-body validation once class and interface schemas are available.
//! Checks instance/static context, declared returns, visibility-sensitive access, and inherited method contracts.
//!
//! Called from:
//! - `crate::types::checker::driver::functions`
//!
//! Key details:
//! - Method checking depends on flattened class metadata and must preserve `self`, `parent`, and `$this` context.

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::ClassMethod;
use crate::types::{traits::FlattenedClass, FunctionSig, PhpType, TypeEnv};

use super::Checker;

/// How many previously produced class tables the method-pass fixpoint remembers.
///
/// Long enough to catch the oscillations that actually occur (the one measured on Symfony has
/// period two), short enough that the memory is a handful of tables rather than a transcript of
/// the whole run. An oscillation longer than this still terminates on the pass budget, exactly as
/// it did before.
const PASS_CYCLE_WINDOW: usize = 4;

impl Checker {
    /// Gives untyped source interface parameters one boxed ABI in every implementing class.
    ///
    /// Interface callers have no implementation-specific call-site types. Keep their untyped
    /// parameters and the matching physical parameters as `Mixed`, including inherited copies,
    /// before direct calls can specialize a concrete method's signature.
    pub(super) fn box_untyped_interface_method_params(&mut self) {
        let mut boxed = crate::fast_hash::FastSet::default();
        for class_info in self.classes.values() {
            for interface_name in &class_info.interfaces {
                let Some(interface_info) = self.interfaces.get(interface_name) else {
                    continue;
                };
                for (method_name, interface_sig) in &interface_info.methods {
                    let source_owner = interface_info
                        .method_declaring_interfaces
                        .get(method_name)
                        .map(String::as_str)
                        .unwrap_or(interface_name);
                    if self
                        .interfaces
                        .get(source_owner)
                        .is_none_or(|owner| owner.declaration_span.line == 0)
                    {
                        continue;
                    }
                    let Some(impl_class) = class_info.method_impl_classes.get(method_name) else {
                        continue;
                    };
                    let Some(physical) = self
                        .classes
                        .get(impl_class)
                        .and_then(|info| info.methods.get(method_name))
                    else {
                        continue;
                    };
                    let regular_params = interface_sig.params.len()
                        - usize::from(interface_sig.variadic.is_some());
                    for (index, is_declared) in interface_sig
                        .declared_params
                        .iter()
                        .take(regular_params)
                        .enumerate()
                    {
                        if !is_declared
                            && !physical.declared_params.get(index).copied().unwrap_or(false)
                        {
                            boxed.insert((format!("{}::{}", impl_class, method_name), index));
                        }
                    }
                }
            }
        }
        for class_info in self.classes.values_mut() {
            for (method_name, sig) in &mut class_info.methods {
                let Some(owner) = class_info.method_impl_classes.get(method_name) else {
                    continue;
                };
                for (index, (_, ty)) in sig.params.iter_mut().enumerate() {
                    if boxed.contains(&(format!("{}::{}", owner, method_name), index)) {
                        *ty = PhpType::Mixed;
                    }
                }
            }
        }
        self.interface_method_boxed_params = boxed;
        self.keep_instanceof_tested_params_gradual();
    }

    /// Keeps an untyped parameter the method body tests with `instanceof` at `mixed`.
    ///
    /// Call-site specialization types an untyped parameter from the arguments the closed world
    /// passes. When the body asks `$p instanceof C`, that question is the point of the parameter:
    /// Carbon's `Creator::make($var)` is only ever called with strings in Laravel, the
    /// specialization made `$var` a `string`, and the `instanceof DateTimeInterface` branch then
    /// passed a string to `instance(DateTimeInterface $date)` -- a refusal that sent Carbon to
    /// the interpreter.
    fn keep_instanceof_tested_params_gradual(&mut self) {
        let mut tested = crate::fast_hash::FastSet::default();
        for (class_name, class_info) in &self.classes {
            for method in &class_info.method_decls {
                for (index, (name, type_expr, _, _)) in method.params.iter().enumerate() {
                    if type_expr.is_none() && body_tests_instanceof(&method.body, name) {
                        tested.insert((
                            php_symbol_key(class_name),
                            php_symbol_key(&method.name),
                            index,
                        ));
                    }
                }
            }
        }
        for (class_name, class_info) in self.classes.iter_mut() {
            let class_key = php_symbol_key(class_name);
            for (method_key, sig) in class_info
                .methods
                .iter_mut()
                .chain(class_info.static_methods.iter_mut())
            {
                for (index, (_, ty)) in sig.params.iter_mut().enumerate() {
                    if tested.contains(&(class_key.clone(), method_key.clone(), index)) {
                        *ty = PhpType::Mixed;
                    }
                }
            }
        }
        self.instanceof_tested_params = tested;
    }

    /// Whether a parameter is still gradual by the rule above.
    pub(crate) fn instanceof_tested_param(&self, class_name: &str, method_key: &str, index: usize) -> bool {
        self.instanceof_tested_params.contains(&(
            php_symbol_key(class_name),
            php_symbol_key(method_key),
            index,
        ))
    }

    /// Runs method-body validation in passes until class type information stabilizes.
    ///
    /// Each pass type-checks every non-abstract method body, collecting return types and
    /// errors. If a pass changes `self.classes` (e.g., via inferred return types), another
    /// pass runs. Iteration stops when types stabilize or `2 * class_count + 1` passes
    /// are exhausted.
    ///
    /// For non-static methods, `$this` is inserted into the per-method `TypeEnv` as an
    /// `Object` of the declaring class. Parameters are resolved against declared type hints
    /// or inferred from the class signature. Untyped parameters without an observed call-site
    /// specialization are checked as gradual `Mixed`; variadic parameters use
    /// `PhpType::Array(Int)` as a fallback.
    ///
    /// Sets `self.current_class`, `self.current_method`, and `self.current_method_is_static`
    /// during body checking to enable context-sensitive diagnostics.
    pub(super) fn type_check_methods_until_stable(
        &mut self,
        flattened_classes: &[FlattenedClass],
        errors: &mut Vec<CompileError>,
    ) -> Result<(), CompileError> {
        // This loop is 32.61s of the 33.30s type-checking phase, and it is three things per
        // pass: a deep CLONE of the whole class table, the body checking, and up to five deep
        // COMPARISONS of that table (one for stability, four against the cycle window).
        // `ELEPHC_TYPECHECK_TIMES=1` says which, and how many passes it takes to get there.
        let trace = std::env::var("ELEPHC_TYPECHECK_TIMES").is_ok();
        let mut passes = 0usize;
        let (mut clone_secs, mut bodies_secs, mut compare_secs) = (0f64, 0f64, 0f64);
        let mut method_passes_remaining = (flattened_classes.len().max(1) * 2) + 1;
        // Tables this loop has already produced, newest last. A pass that repeats one has added
        // nothing and never will — the next table is a function of this one alone, so the
        // sequence is periodic from here. See `PASS_CYCLE_WINDOW`.
        let mut recent_tables = std::collections::VecDeque::with_capacity(PASS_CYCLE_WINDOW);
        self.seed_runtime_contract_param_types();
        let verify_worklist = std::env::var_os("ELEPHC_CHECK_VERIFY").is_some();
        let mut worklist = std::mem::take(&mut self.method_worklist);
        let mut verifying = None;
        loop {
            passes += 1;
            let mut mark = std::time::Instant::now();
            let classes_before_pass = self.classes.clone();
            if trace {
                clone_secs += mark.elapsed().as_secs_f64();
                mark = std::time::Instant::now();
            }
            let mut pass_errors = Vec::new();
            let recursive_mixed_before = self.recursive_mixed_methods.len();
            let recursive_mixed_snapshot = self.recursive_mixed_methods.clone();
            // Only the LAST round's answer counts: a seed that looked unrefined on an early round
            // can be specialized by a call site the same round walks later.
            self.unspecialized_seed_params.clear();
            // Rebuilt once per round, although no round adds a method: a stale owner list would
            // silently narrow a mixed receiver's candidates.
            *self.method_owners.borrow_mut() = None;
            self.subtype_dispatch_cache.borrow_mut().1.clear();
            self.descendant_classes_cache.borrow_mut().1.clear();
            self.interface_implementers_cache.borrow_mut().1.clear();
            self.method_impl_users_index = None;

            for (class_index, class) in flattened_classes.iter().enumerate() {
                for (method_index, method) in class.methods.iter().enumerate() {
                    if method.is_abstract {
                        continue;
                    }
                    // A generic method is a TEMPLATE: its parameter and return types name type
                    // parameters only a call site can bind, so checking its body here reports
                    // `Unknown type: U`. Its instantiations are ordinary methods on this class
                    // and this same loop checks them. Mirrors the generic-function skip in
                    // `resolve_unchecked_functions`.
                    if !method.type_params.is_empty() {
                        continue;
                    }
                    let method_key = php_symbol_key(&method.name);
                    let unit = (class_index, method_index);
                    if let Some(kept) = worklist.kept_result(unit, &method_key) {
                        pass_errors.extend(kept.errors.iter().cloned());
                        for parameter in &kept.seed_params {
                            self.unspecialized_seed_params.insert((
                                class.name.clone(),
                                method_key.clone(),
                                method.is_static,
                                *parameter,
                            ));
                        }
                        continue;
                    }
                    let first_method_error = pass_errors.len();
                    let mut method_seed_params = Vec::new();
                    let mut method_env = Self::seed_method_env();
                    if !method.is_static {
                        method_env.insert("this".to_string(), PhpType::Object(class.name.clone()));
                    }
                    let sig_params = if method.is_static {
                        self.classes
                            .get(&class.name)
                            .and_then(|c| c.static_methods.get(&method_key))
                            .map(|s| s.params.clone())
                    } else {
                        self.classes
                            .get(&class.name)
                            .and_then(|c| c.methods.get(&method_key))
                            .map(|s| s.params.clone())
                    };
                    for (i, (pname, type_ann, _, _)) in method.params.iter().enumerate() {
                        let ty = if let Some(type_ann) = type_ann {
                            let declared = self.resolve_declared_param_type_hint(
                                type_ann,
                                method.span,
                                &format!("Method parameter ${}", pname),
                            )?;
                            // A declared bare `array` remains generic. Call boundaries normalize
                            // concrete indexed/hash payloads while the method body dispatches on
                            // the runtime array kind, so one call site cannot narrow the contract.
                            // A `callable` read as a value keeps its value (`callable_param_storage`),
                            // the same answer the schema pass gave.
                            Self::callable_param_storage(declared, &method.body, pname)
                        } else {
                            let inferred = sig_params
                                .as_ref()
                                .and_then(|p| p.get(i))
                                .map(|(_, t)| t.clone())
                                .unwrap_or(PhpType::Int);
                            let was_seed = inferred == PhpType::Int;
                            let body_ty = self.method_body_param_type(class, method, i, inferred);
                            // The body is checked with `mixed` for a seed no call site refined,
                            // and the SIGNATURE has to say so too or the backend lowers the
                            // parameter as the raw `int` the seed left behind. Recorded here and
                            // published once the pass loop settles -- on any earlier round the
                            // call site that refines it may simply not have been walked yet.
                            if was_seed && body_ty == PhpType::Mixed {
                                method_seed_params.push(i);
                                self.unspecialized_seed_params.insert((
                                    class.name.clone(),
                                    method_key.clone(),
                                    method.is_static,
                                    i,
                                ));
                            }
                            body_ty
                        };
                        // Match build_method_sig and the runtime hydration adapter:
                        // user hooks consume boxed PHP arrays, synthetic SPL keeps
                        // the raw representation declared by its generated signature.
                        let ty = if method_key == "__unserialize" && i == 0 && method.span.line != 0 {
                            PhpType::php_array()
                        } else {
                            ty
                        };
                        // A DECLARED `callable(A): B` parameter also types its own invocation.
                        // Methods never populated this channel at all, so `$f($this->value)`
                        // inside a generic method answered `mixed` and the value it produced was
                        // boxed back into a field the instantiation had pinned — monomorphic on
                        // the outside, erased on the inside.
                        if let Some(sig) = type_ann
                            .as_ref()
                            .and_then(|t| self.declared_callable_signature(t, method.span))
                        {
                            self.closure_return_types
                                .insert(pname.clone(), sig.return_type.clone());
                            self.callable_sigs.insert(pname.clone(), sig);
                        }
                        // A by-reference parameter this body widens is bound `mixed` for the whole
                        // body — see `Checker::widened_ref_param_env_type`. Only the ENVIRONMENT
                        // moves; `self.classes`' signature keeps the declared type, so every call
                        // site still validates its argument against it.
                        let ty = self.widened_ref_param_env_type(
                            &format!("{}::{}", class.name, method.name),
                            pname,
                            &ty,
                        );
                        method_env.insert(pname.clone(), ty);
                    }
                    if let Some(variadic_name) = &method.variadic {
                        let fallback_ty = if method.variadic_by_ref {
                            PhpType::Array(Box::new(PhpType::Mixed))
                        } else {
                            PhpType::Array(Box::new(PhpType::Int))
                        };
                        let ty = sig_params
                            .as_ref()
                            .and_then(|p| p.get(method.params.len()))
                            .map(|(_, t)| t.clone())
                            .unwrap_or(fallback_ty);
                        method_env.insert(variadic_name.clone(), ty);
                    }
                    if method_key == "__construct" {
                        self.patch_constructor_method_env(class, method, &mut method_env);
                    }
                    // A local a body CREATES by writing an array element into it —
                    // `$keys[] = $k` against a name nothing has assigned — exists from that
                    // write onwards, because PHP auto-vivifies the array. Every other scope
                    // seeds those names at entry (`resolve_function_signature` for a function,
                    // `infer_closure_type_with_param_hints` for a closure,
                    // `check_top_level_program` for file scope) and the method pass did not, so
                    // a read after the loop that fills the array was "Undefined variable" here
                    // and lowered fine in the backend, which runs the same scan for every body.
                    // Seeded last: the scan treats everything already in the environment as
                    // bound, so the parameters, the variadic and the promoted constructor
                    // properties must be in place first.
                    Self::seed_vivified_array_locals(&mut method_env, &method.body);

                    self.current_class = Some(class.name.clone());
                    self.current_method = Some(method_key.clone());
                    self.current_method_is_static = method.is_static;
                    self.current_by_ref_return = method.by_ref_return;
                    let loop_storage_scope = format!("{}::{}", class.name, method.name);
                    self.clear_loop_storage_scope(&loop_storage_scope);
                    let previous_loop_storage_scope = std::mem::replace(
                        &mut self.current_loop_storage_scope,
                        loop_storage_scope,
                    );
                    let method_ref_params: Vec<String> = method
                        .params
                        .iter()
                        .filter(|(_, _, _, is_ref)| *is_ref)
                        .map(|(name, _, _, _)| name.clone())
                        .collect();
                    // Every parameter is bound unconditionally on entry, so all of them are
                    // recorded at binding depth 0 (a missing entry means "seeded, not bound
                    // here", which is not kill/retype eligible).
                    let method_param_names: Vec<String> = method
                        .params
                        .iter()
                        .map(|(name, _, _, _)| name.clone())
                        .chain(method.variadic.iter().cloned())
                        .collect();
                    let mut method_errors = Vec::new();
                    // The storage this frame already holds on entry: the parameters. `$this`,
                    // the superglobals and the seeded globals `method_env` also carries are not
                    // this frame's own storage, and none of them is markable anyway.
                    let pre_bound_own_storage: crate::fast_hash::FastMap<String, PhpType> =
                        method_param_names
                            .iter()
                            .filter_map(|name| {
                                method_env.get(name).map(|ty| (name.clone(), ty.clone()))
                            })
                            .collect();
                    // A parameter DECLARED `callable` carries no signature anyone can check
                    // against: its real one arrives with the argument. `resolve_function_signature`
                    // records such parameters so a spread call on one is not refused for the
                    // by-reference parameters of whatever signature inference happened to guess —
                    // and it only ever did it for plain FUNCTIONS, so the same parameter in a
                    // METHOD lost the allowance. Symfony's
                    // `PhpFileLoader::callConfigurator(callable $callback, ...)` is exactly that
                    // shape: `$callback(...$arguments)` was refused inside the method and accepted
                    // outside one.
                    let saved_callable_param_names = self.callable_param_names.clone();
                    for (pname, type_ann, _, _) in &method.params {
                        if type_ann.is_some()
                            && method_env.get(pname) == Some(&PhpType::Callable)
                        {
                            self.callable_param_names.insert(pname.clone());
                        }
                    }
                    // Every parameter is a fresh binding, and the callable records are keyed by
                    // NAME alone: whichever body last bound a `$callback` closure described this
                    // method's `$callback` too. `resolve_function_signature` isolates a function's
                    // parameters this way; methods did not, so Laravel's
                    // `EnvironmentDetector::detectWebEnvironment(Closure $callback)` typed
                    // `return $callback();` as some other body's int closure, cast the string the
                    // interpreted caller's closure returned to `int`, and every request ran in
                    // environment `0`. No call site records a METHOD's callable parameter, so
                    // the honest signature is none: the call returns `mixed`.
                    let hidden_param_callable_records: Vec<_> = method
                        .params
                        .iter()
                        .map(|(pname, type_ann, _, _)| {
                            // A DECLARED `callable(A): B` was recorded for this very body above
                            // and is the honest signature of the parameter: keep it in place.
                            if type_ann.as_ref().is_some_and(|t| {
                                self.declared_callable_signature(t, method.span).is_some()
                            }) {
                                return (
                                    pname.clone(),
                                    self.callable_sigs.get(pname).cloned(),
                                    self.closure_return_types.get(pname).cloned(),
                                    self.first_class_callable_targets.remove(pname),
                                    self.callable_array_targets.remove(pname),
                                );
                            }
                            (
                                pname.clone(),
                                self.callable_sigs.remove(pname),
                                self.closure_return_types.remove(pname),
                                self.first_class_callable_targets.remove(pname),
                                self.callable_array_targets.remove(pname),
                            )
                        })
                        .collect();
                    let body_result = self.with_local_storage_context(
                        method_ref_params,
                        method_param_names,
                        pre_bound_own_storage,
                        &method.body,
                        |checker| {
                            checker.boxed_parameter_locals.extend(method.params.iter()
                                .filter(|(_, annotation, _, _)| annotation.is_none())
                                .map(|(name, _, _, _)| name.clone()));
                            for s in &method.body {
                                if let Err(error) = checker.check_stmt(s, &mut method_env) {
                                    // Naming the enclosing CLASS is what lets `pipeline` recover
                                    // the FILE: after autoload expansion every spliced file
                                    // shares one line-number space, so `line:col` alone
                                    // identifies nothing. `resolve_function_signature` already
                                    // does this for a plain function; a method body reached the
                                    // reporter untagged, which is why a Symfony preload error
                                    // printed as a bare `error[213:13]`.
                                    method_errors.extend(
                                        error.within_declaration(class.name.as_str()).flatten(),
                                    );
                                }
                            }
                            Ok(())
                        },
                    );
                    self.callable_param_names = saved_callable_param_names;
                    for (pname, sig, return_ty, first_class_target, array_target) in
                        hidden_param_callable_records
                    {
                        match sig {
                            Some(sig) => {
                                self.callable_sigs.insert(pname.clone(), sig);
                            }
                            None => {
                                self.callable_sigs.remove(&pname);
                            }
                        }
                        match return_ty {
                            Some(return_ty) => {
                                self.closure_return_types.insert(pname.clone(), return_ty);
                            }
                            None => {
                                self.closure_return_types.remove(&pname);
                            }
                        }
                        match first_class_target {
                            Some(target) => {
                                self.first_class_callable_targets.insert(pname.clone(), target);
                            }
                            None => {
                                self.first_class_callable_targets.remove(&pname);
                            }
                        }
                        match array_target {
                            Some(target) => {
                                self.callable_array_targets.insert(pname, target);
                            }
                            None => {
                                self.callable_array_targets.remove(&pname);
                            }
                        }
                    }
                    body_result?;
                    if std::env::var("ELEPHC_BACKEND_INVENTORY").as_deref() == Ok("1") {
                        for error in &mut method_errors {
                            error.message = format!(
                                "{}::{}: {}",
                                class.name, method.name, error.message
                            );
                        }
                    }
                    let method_has_errors = !method_errors.is_empty();
                    // A refused untyped body keeps its `int` return seed, and a call to the method
                    // from inside itself is typed by that seed: retry with the call typed `mixed`.
                    if method_has_errors && method.return_type.is_none() {
                        self.recursive_mixed_methods.insert((
                            class.name.clone(),
                            method_key.clone(),
                            method.is_static,
                        ));
                    }
                    pass_errors.extend(method_errors);

                    if !method_has_errors {
                        let first_return_error = pass_errors.len();
                        self.update_method_return_type(class, method, &method_env, &mut pass_errors);
                        for error in &mut pass_errors[first_return_error..] {
                            if error.declaration.is_none() {
                                error.declaration = Some(class.name.clone());
                            }
                        }
                    }
                    self.current_class = None;
                    self.current_method = None;
                    self.current_method_is_static = false;
                    self.current_by_ref_return = false;
                    self.current_loop_storage_scope = previous_loop_storage_scope;
                    worklist.record(
                        unit,
                        method,
                        pass_errors[first_method_error..].to_vec(),
                        method_seed_params,
                    );
                }
            }

            if trace {
                bodies_secs += mark.elapsed().as_secs_f64();
                mark = std::time::Instant::now();
            }
            self.unify_override_family_return_types();
            self.sync_inherited_method_return_types();
            let stabilized = self.classes == classes_before_pass
                && self.recursive_mixed_methods.len() == recursive_mixed_before;
            if trace && !stabilized {
                // Which signatures actually moved this pass. The loop exits on CYCLE detection,
                // not convergence, so the last passes are re-checking 1000+ classes to chase
                // whatever this names -- and that is the thing worth fixing, not the loop.
                let mut moved: Vec<String> = Vec::new();
                for (class_name, after) in &self.classes {
                    let Some(before) = classes_before_pass.get(class_name) else {
                        moved.push(format!("{class_name}(new)"));
                        continue;
                    };
                    for (method, signature) in &after.methods {
                        if before.methods.get(method) != Some(signature) {
                            moved.push(format!("{class_name}::{method}"));
                        }
                    }
                    for (method, signature) in &after.static_methods {
                        if before.static_methods.get(method) != Some(signature) {
                            moved.push(format!("{class_name}::{method}(static)"));
                        }
                    }
                    if before.callable_method_return_sigs != after.callable_method_return_sigs {
                        moved.push(format!("{class_name}(callable_return_sigs)"));
                    }
                    if before != after && moved.last().map(|last| !last.starts_with(class_name.as_str())).unwrap_or(true) {
                        // A field OUTSIDE the three a pass writes here has moved, which means
                        // something deeper in the checker mutates `ClassInfo` too. Name it by
                        // diffing the debug renderings rather than enumerating forty fields:
                        // this runs only for the handful of classes that are still moving.
                        let (before_text, after_text) =
                            (format!("{before:?}"), format!("{after:?}"));
                        let at = before_text
                            .bytes()
                            .zip(after_text.bytes())
                            .position(|(left, right)| left != right)
                            .unwrap_or(before_text.len().min(after_text.len()));
                        let from = at.saturating_sub(70);
                        moved.push(format!(
                            "{class_name}(other @{at}: {:?} -> {:?})",
                            &before_text[from..before_text.len().min(at + 40)],
                            &after_text[from..after_text.len().min(at + 40)],
                        ));
                    }
                }
                moved.sort();
                eprintln!(
                    "[elephc-methodpass] pass={passes} moved={} first={:?}",
                    moved.len(),
                    &moved[..moved.len().min(12)]
                );
            }
            // A REPEAT is not progress. Symfony's 246-file preload is quiet for every class but
            // one after three passes and then oscillates forever on a single property, which used
            // to burn the whole budget — 1630 passes at ~2s each — to land on a table pass three
            // had already produced. Stopping here ends on a state the loop reached by itself.
            let cycling = !stabilized
                && recent_tables
                    .iter()
                    .any(|table| table == &self.classes);
            let out_of_passes = method_passes_remaining == 0;
            if trace {
                compare_secs += mark.elapsed().as_secs_f64();
            }
            // A round that kept bodies proves nothing on its own: the verification repeats the
            // full walk once and names anything the kept bodies would have moved.
            if let Some(kept_errors) = verifying.take() {
                if !stabilized || kept_errors != pass_errors.len() {
                    eprintln!(
                        "[elephc-methodpass] worklist diverged: stable={stabilized} errors {kept_errors} -> {}",
                        pass_errors.len()
                    );
                }
            } else if stabilized && verify_worklist && worklist.kept_any() {
                worklist.check_everything();
                verifying = Some(pass_errors.len());
                continue;
            }
            if stabilized || cycling || out_of_passes {
                errors.extend(pass_errors);
                if trace {
                    eprintln!(
                        "[elephc-methodpass] passes={passes} clone={clone_secs:.2}s \
                         bodies={bodies_secs:.2}s compare={compare_secs:.2}s \
                         classes={} stabilized={stabilized} cycling={cycling}",
                        self.classes.len()
                    );
                }
                break;
            }

            while recent_tables.len() >= PASS_CYCLE_WINDOW {
                recent_tables.pop_front();
            }
            worklist.prepare_next_pass(&classes_before_pass, &self.classes);
            // A method joining `recursive_mixed_methods` changes how calls to ITSELF inside its
            // own body are typed, and nothing else: its own name moves, so only its body (and the
            // bodies naming it) walk again. This used to walk every body of the program.
            let grown: Vec<&str> = self
                .recursive_mixed_methods
                .iter()
                .filter(|entry| !recursive_mixed_snapshot.contains(*entry))
                .map(|(_, method, _)| method.as_str())
                .collect();
            if trace && !grown.is_empty() {
                eprintln!("[elephc-worklist] recursive mixed methods grew by {}", grown.len());
            }
            worklist.also_moved(grown.into_iter());
            // The clone the stabilization check already made, reused rather than repeated.
            recent_tables.push_back(classes_before_pass);
            method_passes_remaining -= 1;
        }
        self.method_worklist = worklist;
        Ok(())
    }

    /// Widens the returns of untyped override-family members that disagree on their storage.
    ///
    /// A call through `$this->m()` or `static::m()` is typed by the class the CALL sits in, and
    /// the vtable then runs whichever override the receiver has. Every body therefore has to hand
    /// its value back in the storage the caller reads. Nothing made two untyped bodies agree:
    /// Laravel's `Facade::getFacadeAccessor()` only throws (so it was typed `void`) while every
    /// facade returns a string, and `static::getFacadeAccessor()` read the string as nothing --
    /// every facade resolved the empty accessor. A base returning a string with an override
    /// returning an int read garbage and crashed.
    ///
    /// Each untyped method is compared with the nearest ancestor that declares the same method;
    /// when their storage differs both become `mixed`, which every return boxes into. Recorded in
    /// `family_return_widenings` so the next pass's `update_method_return_type` keeps it, and
    /// repeated until stable by the enclosing fixpoint: a sibling or a grandparent that still
    /// disagrees with a newly widened member is widened on a later pass.
    fn unify_override_family_return_types(&mut self) {
        let mut widenings: Vec<(String, String, bool, PhpType)> = Vec::new();
        for (class_name, info) in &self.classes {
            for is_static in [false, true] {
                let (declaring, methods) = if is_static {
                    (&info.static_method_declaring_classes, &info.static_methods)
                } else {
                    (&info.method_declaring_classes, &info.methods)
                };
                for (method_key, owner) in declaring {
                    if owner != class_name || method_key == "__construct" {
                        continue;
                    }
                    let Some(own) = methods.get(method_key) else { continue };
                    if own.declared_return {
                        // A DECLARED override of an untyped ancestor: the ancestor's callers read
                        // its inferred storage, which the override does not return. Monolog's
                        // `LineFormatter::normalizeException(): string` overrides an untyped
                        // `NormalizerFormatter::normalizeException()` that returns an array, and
                        // `$this->normalizeException()` inside the base stored the string as an
                        // array. The ancestor widens to `mixed`; `ancestor_return_storage` then
                        // boxes the override's returns to match.
                        if let Some((ancestor, ancestor_sig)) =
                            self.nearest_ancestor_declaring(class_name, method_key, is_static)
                        {
                            if !ancestor_sig.declared_return
                                && ancestor_sig.return_type.codegen_repr() != PhpType::Mixed
                                && own.return_type.codegen_repr()
                                    != ancestor_sig.return_type.codegen_repr()
                            {
                                widenings.push((ancestor, method_key.clone(), is_static, PhpType::Mixed));
                            }
                        }
                        continue;
                    }
                    // A call through an interface that declares the method WITHOUT a return type
                    // reads the result boxed, so the implementation has to return it boxed too:
                    // `interface N { function name(); }` implemented by `function name() {
                    // return $this->name; }` handed `$this->name()` an unboxed string that the
                    // interface-typed caller read as "".
                    let untyped_in_an_interface = self.class_and_ancestor_interfaces(class_name).iter().any(|interface| {
                        self.interfaces
                            .get(interface.trim_start_matches('\\'))
                            .and_then(|interface| {
                                if is_static {
                                    interface.static_methods.get(method_key)
                                } else {
                                    interface.methods.get(method_key)
                                }
                            })
                            .is_some_and(|sig| !sig.declared_return)
                    });
                    if untyped_in_an_interface && own.return_type.codegen_repr() != PhpType::Mixed {
                        widenings.push((class_name.clone(), method_key.clone(), is_static, PhpType::Mixed));
                        continue;
                    }
                    let Some((ancestor, ancestor_sig)) =
                        self.nearest_ancestor_declaring(class_name, method_key, is_static)
                    else {
                        continue;
                    };
                    if ancestor_sig.declared_return
                        || own.return_type.codegen_repr() == ancestor_sig.return_type.codegen_repr()
                    {
                        continue;
                    }
                    widenings.push((class_name.clone(), method_key.clone(), is_static, PhpType::Mixed));
                    widenings.push((ancestor, method_key.clone(), is_static, PhpType::Mixed));
                }
            }
        }
        for (class_name, method_key, is_static, widened) in widenings {
            if let Some(info) = self.classes.get_mut(&class_name) {
                let methods = if is_static { &mut info.static_methods } else { &mut info.methods };
                if let Some(sig) = methods.get_mut(&method_key) {
                    sig.return_type = widened.clone();
                }
            }
            self.family_return_widenings
                .insert((class_name, method_key, is_static), widened);
        }
    }

    /// Returns every interface `class_name` implements, including those an ancestor implements and
    /// those an implemented interface extends.
    fn class_and_ancestor_interfaces(&self, class_name: &str) -> Vec<String> {
        let mut pending: Vec<String> = Vec::new();
        let mut current = Some(class_name.to_string());
        let mut depth = 0;
        while let Some(name) = current {
            depth += 1;
            let Some(info) = self.classes.get(&name).filter(|_| depth <= 256) else {
                break;
            };
            pending.extend(info.interfaces.iter().map(|i| i.trim_start_matches('\\').to_string()));
            current = info.parent.clone();
        }
        let mut seen = crate::fast_hash::FastSet::default();
        let mut all = Vec::new();
        while let Some(interface) = pending.pop() {
            if !seen.insert(interface.clone()) {
                continue;
            }
            if let Some(info) = self.interfaces.get(&interface) {
                pending.extend(info.parents.iter().map(|p| p.trim_start_matches('\\').to_string()));
            }
            all.push(interface);
        }
        all
    }

    /// Returns the nearest ancestor that declares `method_key` itself, with its signature, unless
    /// that declaration is private (a private method is not overridden).
    fn nearest_ancestor_declaring(
        &self,
        class_name: &str,
        method_key: &str,
        is_static: bool,
    ) -> Option<(String, FunctionSig)> {
        let mut current = self.classes.get(class_name)?.parent.clone();
        let mut depth = 0;
        while let Some(parent) = current {
            depth += 1;
            if depth > 256 {
                return None;
            }
            let info = self.classes.get(&parent)?;
            let (declaring, methods, visibilities) = if is_static {
                (
                    &info.static_method_declaring_classes,
                    &info.static_methods,
                    &info.static_method_visibilities,
                )
            } else {
                (&info.method_declaring_classes, &info.methods, &info.method_visibilities)
            };
            if declaring.get(method_key).is_some_and(|owner| owner == &parent) {
                if visibilities.get(method_key) == Some(&crate::parser::ast::Visibility::Private) {
                    return None;
                }
                return methods.get(method_key).map(|sig| (parent.clone(), sig.clone()));
            }
            current = info.parent.clone();
        }
        None
    }

    /// Copies each inherited method's return type from the class whose body implements it.
    ///
    /// `update_method_return_type` writes the type a body returns into its DECLARING class only,
    /// yet every subclass carries its own copy of the signature, and a call on a subclass
    /// receiver reads that copy. A copy taken on an earlier pass keeps whatever the body returned
    /// then: `class Section extends Out` kept `getStream(): resource` from the pass on which
    /// `parent::__construct(fopen(...))` had typed the untyped property, while `Out::getStream()`
    /// had since widened to the boxed Mixed it is emitted with. A caller then re-boxed that box
    /// pointer as an `int`, and Symfony's `ConsoleOutput::section()` handed `int(4316502720)` to
    /// `StreamOutput::__construct`, which refused it as "not a stream".
    ///
    /// One sweep per pass over every inherited entry: cheaper than a search per updated method,
    /// and the fixpoint then sees the copies move in the same pass as the body.
    /// A late-static return (`: static`) is resolved per receiver class, so its copy is skipped.
    fn sync_inherited_method_return_types(&mut self) {
        let mut updates: Vec<(String, String, PhpType)> = Vec::new();
        for (class_name, info) in &self.classes {
            for (method_key, impl_class) in &info.method_impl_classes {
                if impl_class == class_name || info.late_static_method_returns.contains_key(method_key) {
                    continue;
                }
                let Some(own) = info.methods.get(method_key) else { continue };
                if own.declared_return && info.abstract_methods.contains(method_key) {
                    continue;
                }
                let Some(body) = self
                    .classes
                    .get(impl_class)
                    .and_then(|owner| owner.methods.get(method_key))
                else {
                    continue;
                };
                if own.return_type != body.return_type {
                    updates.push((class_name.clone(), method_key.clone(), body.return_type.clone()));
                }
            }
        }
        for (class_name, method_key, return_type) in updates {
            if let Some(sig) = self
                .classes
                .get_mut(&class_name)
                .and_then(|info| info.methods.get_mut(&method_key))
            {
                sig.return_type = return_type;
            }
        }
    }

    /// Returns the body-checking type for an untyped method parameter.
    ///
    /// Class schemas retain `Int` as the unspecialized legacy sentinel. A real call records the
    /// parameter in `param_specialization_seen`, including the homogeneous integer case where the
    /// stored type remains `Int`. Without that evidence PHP's untyped parameter is gradual
    /// `Mixed`; using the sentinel in its body creates false diagnostics and disagrees with the
    /// boxed PHP parameter contract used by EIR lowering.
    fn method_body_param_type(
        &self,
        class: &FlattenedClass,
        method: &ClassMethod,
        index: usize,
        inferred: PhpType,
    ) -> PhpType {
        if inferred != PhpType::Int
            || self.is_runtime_contract_method(&class.name, &php_symbol_key(&method.name), method.is_static)
        {
            return inferred;
        }
        let owner = if method.is_static {
            format!("static:{}::{}", class.name, method.name)
        } else {
            format!("{}::{}", class.name, php_symbol_key(&method.name))
        };
        if self.param_specialization_seen.contains(&(owner, index)) {
            inferred
        } else {
            PhpType::Mixed
        }
    }


    /// Publishes `mixed` for every undeclared parameter whose `int` inference seed no call site
    /// refined, so the signature says what the body was checked with.
    ///
    /// `method_body_param_type` already answers `mixed` for such a parameter -- PHP reads an
    /// undeclared parameter as `mixed` -- but it only moved the ENVIRONMENT. The signature kept
    /// the seed, and the signature is what EIR lowers the parameter as, so the body reasoned about
    /// a boxed value while the backend read a raw integer register. `twig/twig`'s
    /// `CoreExtension::filter($env, $isSandboxed, $array, $arrow)` has no compiled caller: its
    /// `$array` reached `new \IteratorIterator($array)` as `Int` and its `$arrow` reached
    /// `new \CallbackFilterIterator(..., $arrow)` as `Int` against a `callable` property, on code
    /// php runs. The free-function half of this rule is
    /// `widen_unrefined_params_of_an_uncalled_function`.
    pub(super) fn publish_unspecialized_method_param_seeds(&mut self) {
        let slots: Vec<(String, String, bool, usize)> =
            self.unspecialized_seed_params.iter().cloned().collect();
        for (class_name, method_key, is_static, index) in slots {
            if self.is_runtime_contract_method(&class_name, &method_key, is_static) {
                continue;
            }
            let Some(class_info) = self.classes.get_mut(&class_name) else {
                continue;
            };
            let table = if is_static {
                &mut class_info.static_methods
            } else {
                &mut class_info.methods
            };
            let Some(sig) = table.get_mut(&method_key) else {
                continue;
            };
            let Some(param) = sig.params.get_mut(index) else {
                continue;
            };
            // Only the seed itself: a later round may have refined it after all.
            if param.1 == PhpType::Int {
                param.1 = PhpType::Mixed;
            }
        }
        self.sync_inherited_method_param_types();
    }

    /// Copies each inherited method's parameter types from the class whose body implements it.
    ///
    /// The parameter twin of `sync_inherited_method_return_types`: a call site specializes the
    /// IMPLEMENTING class's signature, and the body is emitted with it, but a call on a subclass
    /// receiver plans its arguments against the subclass's own copy. That copy kept the seed,
    /// which the publish above turned into `mixed`, so `$child->bind('k', fn () => 1)` boxed the
    /// closure while `Container::bind()` read its `callable` parameter raw -- Laravel's
    /// `Application` (a `Container` subclass) lost every `$app['env'] = ...` binding that way.
    fn sync_inherited_method_param_types(&mut self) {
        let mut updates: Vec<(String, String, bool, Vec<PhpType>)> = Vec::new();
        for (class_name, info) in &self.classes {
            for (is_static, impl_classes, table) in [
                (false, &info.method_impl_classes, &info.methods),
                (true, &info.static_method_impl_classes, &info.static_methods),
            ] {
                for (method_key, impl_class) in impl_classes.iter() {
                    if impl_class == class_name {
                        continue;
                    }
                    let Some(own) = table.get(method_key) else { continue };
                    let Some(body) = self.classes.get(impl_class).and_then(|owner| {
                        if is_static {
                            owner.static_methods.get(method_key)
                        } else {
                            owner.methods.get(method_key)
                        }
                    }) else {
                        continue;
                    };
                    if own.params.len() != body.params.len() {
                        continue;
                    }
                    if own.params.iter().zip(&body.params).any(|(a, b)| a.1 != b.1) {
                        let types = body.params.iter().map(|(_, ty)| ty.clone()).collect();
                        updates.push((class_name.clone(), method_key.clone(), is_static, types));
                    }
                }
            }
        }
        for (class_name, method_key, is_static, types) in updates {
            let Some(info) = self.classes.get_mut(&class_name) else { continue };
            let table = if is_static { &mut info.static_methods } else { &mut info.methods };
            if let Some(sig) = table.get_mut(&method_key) {
                for (param, ty) in sig.params.iter_mut().zip(types) {
                    param.1 = ty;
                }
            }
        }
    }

    /// Gives every undeclared parameter of a stream-wrapper contract method php's prototype type.
    ///
    /// The runtime passes these arguments raw, so the seed the body and the signature keep for a
    /// contract method (`is_runtime_contract_method`) must BE the contract type, not the `int`
    /// placeholder every undeclared parameter starts from. See
    /// `codegen_support::runtime::user_wrapper_contract_param_type`.
    fn seed_runtime_contract_param_types(&mut self) {
        let mut updates = Vec::new();
        for (class_name, class_info) in &self.classes {
            for (method_key, sig) in &class_info.methods {
                if !self.is_runtime_contract_method(class_name, method_key, false) {
                    continue;
                }
                for index in 0..sig.params.len() {
                    if sig.declared_params.get(index).copied().unwrap_or(false) {
                        continue;
                    }
                    if let Some(ty) =
                        crate::codegen_support::runtime::user_wrapper_contract_param_type(method_key, index)
                    {
                        updates.push((class_name.clone(), method_key.clone(), index, ty));
                    }
                }
            }
        }
        for (class_name, method_key, index, ty) in updates {
            if let Some(param) = self
                .classes
                .get_mut(&class_name)
                .and_then(|info| info.methods.get_mut(&method_key))
                .and_then(|sig| sig.params.get_mut(index))
            {
                param.1 = ty;
            }
        }
    }

    /// Returns whether the runtime calls this method through a fixed raw-argument vtable: a
    /// stream wrapper's or a user filter's contract method. The EIR metadata keeps their seeds
    /// (`normalize_method_map_for_eir`), so the body and the signature must keep them too: a
    /// `mixed` parameter read the raw path pointer `fopen()` passed as a box and crashed in
    /// `stream_open($path, $mode, $options, &$opened)`.
    fn is_runtime_contract_method(&self, class_name: &str, method_key: &str, is_static: bool) -> bool {
        if is_static {
            return false;
        }
        let Some(class_info) = self.classes.get(class_name) else {
            return false;
        };
        let is_wrapper_class = class_info.methods.keys().any(|method| {
            crate::codegen_support::runtime::is_user_wrapper_contract_method(method)
        });
        // phpoption's `Some::filter($callable)` is no stream filter: in a world that registers
        // one (Laravel's does), its `$callable` kept the filter ABI's raw `int` seed and the call
        // `$callable($this->value)` was refused. See `types::is_user_filter_class`.
        let is_filter_class = self.user_filters_reachable
            && class_info.methods.contains_key("filter")
            && crate::types::is_user_filter_class(
                &self.classes,
                self.user_filter_registered.as_ref(),
                class_name,
            );
        (is_wrapper_class && crate::codegen_support::runtime::is_user_wrapper_contract_method(method_key))
            || (is_filter_class
                && crate::codegen_support::runtime::is_user_filter_contract_method(method_key))
    }

    /// Builds the PHP-local base environment shared by all method bodies.
    ///
    /// Methods can read request superglobals without a `global` declaration, but
    /// ordinary top-level locals belong to file scope and must not leak into a
    /// method. Explicit `global` statements resolve through `self.top_level_env`.
    fn seed_method_env() -> TypeEnv {
        crate::superglobals::SUPERGLOBALS
            .iter()
            .map(|name| ((*name).to_string(), crate::superglobals::superglobal_type()))
            .collect()
    }

    /// Patches untyped constructor parameters with property types when the constructor
    /// property-promotion rule applies.
    ///
    /// For each constructor parameter without an explicit type hint, if the class has a
    /// matching promoted property (`constructor_param_to_prop`), that property's declared
    /// type is injected into `method_env` for the parameter and also propagated back into
    /// the class signature's `params[i].1`. Skips parameters that have explicit type
    /// annotations or whose promoted property is redeclared as a normal property.
    fn patch_constructor_method_env(
        &mut self,
        class: &FlattenedClass,
        method: &ClassMethod,
        method_env: &mut TypeEnv,
    ) {
        if let Some(ci) = self.classes.get(&class.name).cloned() {
            for (i, (pname, type_ann, _, is_ref)) in method.params.iter().enumerate() {
                if type_ann.is_some() {
                    continue;
                }
                if let Some(Some(prop_name)) = ci.constructor_param_to_prop.get(i) {
                    if ci.visible_property_is_declared(prop_name) {
                        continue;
                    }
                    if let Some((_, (_, ty))) = ci.visible_property(prop_name) {
                        let param_ty = if *is_ref { PhpType::Mixed } else { ty.clone() };
                        method_env.insert(pname.clone(), param_ty.clone());
                        if let Some(ci_mut) = self.classes.get_mut(&class.name) {
                            if let Some(sig) = ci_mut.methods.get_mut("__construct") {
                                if i < sig.params.len() {
                                    sig.params[i].1 = param_ty;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Infers the return type from method body `return` statements, validates it against
    /// any declared return type hint, and writes the effective return type back into
    /// `self.classes`.
    ///
    /// Return type inference scans `method.body` for `return` statements, widens all
    /// observed types to the common supertype, and falls back to `PhpType::Void` when
    /// the body is empty. If a declared hint exists, `require_declared_return_coverage`
    /// checks for unreachable returns and `require_compatible_return_type` checks each
    /// observed return for assignability to the declared type. A `Never` declared return
    /// suppresses the compatibility check (the body is allowed to have no returns when
    /// it always throws/exits/loops). `Never` combined with a body that *does* contain
    /// return statements produces a compile error. Generic array hints are passed
    /// through as-is to preserve inference.
    ///
    /// A method body containing `yield` is a generator: calling it produces a
    /// `Generator` object regardless of what the body's `return` statements say, so
    /// generator detection short-circuits the whole inference/validation chain the
    /// same way the free-function path in `functions::resolution::signature` does.
    /// Without that short-circuit an unhinted generator method infers `void` (the
    /// body has no value return) and a `: Generator` hint trips the
    /// "must return a value on every path" coverage check.
    fn update_method_return_type(
        &mut self,
        class: &FlattenedClass,
        method: &ClassMethod,
        method_env: &TypeEnv,
        pass_errors: &mut Vec<CompileError>,
    ) {
        let mut return_infos = Vec::new();
        let mut callable_return_sigs = Vec::new();
        let mut callable_array_return_sigs = Vec::new();
        for stmt in &method.body {
            self.collect_return_infos(stmt, method_env, &mut return_infos);
            self.collect_return_callable_sigs(stmt, method_env, &mut callable_return_sigs);
            self.collect_return_callable_array_sigs(
                stmt,
                method_env,
                &mut callable_array_return_sigs,
            );
        }
        let raw_inferred = if return_infos.is_empty() {
            None
        } else {
            let mut widest = return_infos[0].ty.clone();
            for return_info in &return_infos[1..] {
                widest = Self::wider_type(&widest, &return_info.ty);
            }
            Some(widest)
        };
        let inferred_return = match raw_inferred.clone() {
            Some(inferred) if method.return_type.is_none() => {
                self.widen_return_for_fall_off(&method.body, inferred)
            }
            Some(inferred) => inferred,
            None => PhpType::Void,
        };
        let inferred_php_array = return_infos
            .iter()
            .any(|return_info| return_info.ty.is_php_array())
            && return_infos.iter().all(|return_info| {
                return_info.ty.is_php_array()
                    || matches!(
                        return_info.ty,
                        PhpType::Void | PhpType::Array(_) | PhpType::AssocArray { .. }
                    )
            });
        let effective_return = if crate::types::checker::yield_validation::body_contains_yield(
            &method.body,
        ) {
            match self.generator_method_return_type(class, method) {
                Ok(generator_ty) => generator_ty,
                Err(error) => {
                    pass_errors.extend(error.flatten());
                    self.current_class = None;
                    self.current_method = None;
                    self.current_method_is_static = false;
                    return;
                }
            }
        } else if let Some(type_ann) = method.return_type.as_ref() {
            match self.resolve_declared_return_type_hint(
                type_ann,
                method.span,
                &format!("Method '{}::{}'", class.name, method.name),
            ) {
                Ok(declared) => {
                    if matches!(declared, PhpType::Never)
                        && Self::body_contains_return(&method.body)
                    {
                        pass_errors.push(CompileError::new(
                            method.span,
                            &format!(
                                "Method '{}::{}' declared never must not return",
                                class.name, method.name
                            ),
                        ));
                        self.current_class = None;
                        self.current_method = None;
                        self.current_method_is_static = false;
                        return;
                    }
                    if let Err(error) = self.require_return_hint_coverage(
                        type_ann,
                        &declared,
                        &method.body,
                        method.span,
                        &format!("Method '{}::{}'", class.name, method.name),
                    ) {
                        pass_errors.extend(error.flatten());
                        self.current_class = None;
                        self.current_method = None;
                        self.current_method_is_static = false;
                        return;
                    }
                    // :never methods are allowed to have no return statements (they always throw/exit/loop).
                    let skip_compat_check = matches!(declared, PhpType::Never);
                    if !skip_compat_check {
                        for return_info in &return_infos {
                            if let Err(error) = self.require_compatible_return_hint(
                                type_ann,
                                &declared,
                                &return_info.ty,
                                return_info.has_value,
                                return_info.strict_types,
                                method.span,
                                &format!("Method '{}::{}' return type", class.name, method.name),
                            ) {
                                pass_errors.extend(error.flatten());
                                self.current_class = None;
                                self.current_method = None;
                                self.current_method_is_static = false;
                                return;
                            }
                        }
                    }
                    if Self::is_generic_array_hint(&declared) && inferred_php_array {
                        PhpType::php_array()
                    } else if Self::is_generic_array_hint(&declared)
                        && !self.method_overridden_by_descendant(
                            &class.name,
                            &php_symbol_key(&method.name),
                            method.is_static,
                        )
                    {
                        // Resolved from the individual returns, not from `inferred_return`:
                        // `wider_type` collapses an indexed return joined with a hash one to
                        // `Mixed`, which this guard used to reject, leaving the declared INDEXED
                        // hint to describe a container that is a hash at runtime.
                        Self::generic_array_return_contract(&return_infos)
                        .unwrap_or(declared)
                    } else {
                        declared
                    }
                }
                Err(error) => {
                    pass_errors.extend(error.flatten());
                    self.current_class = None;
                    self.current_method = None;
                    self.current_method_is_static = false;
                    return;
                }
            }
        } else {
            // An untyped member of an override family keeps the storage the family agreed on
            // (`unify_override_family_return_types`), or this pass would narrow it straight back.
            self.family_return_widenings
                .get(&(class.name.clone(), php_symbol_key(&method.name), method.is_static))
                .cloned()
                .unwrap_or_else(|| {
                    if self.diverging_placeholder_may_be_overridden_elsewhere(
                        class,
                        method,
                        raw_inferred.is_some(),
                    ) {
                        PhpType::Mixed
                    } else {
                        inferred_return
                    }
                })
        };
        let method_key = php_symbol_key(&method.name);
        // An override keeps the storage its ancestors agreed on.
        let effective_return = if method.is_static {
            effective_return
        } else {
            self.ancestor_return_storage(&class.name, &method_key, effective_return.clone())
        };
        // A subclass copies the method signature before body inference runs. Calls through
        // that subclass must use the implementing method's return ABI, or a boxed result can
        // be read as a scalar even though both names dispatch to the same body.
        let mut receivers = self.method_impl_users(&class.name, &method_key, method.is_static);
        if !receivers.iter().any(|name| name == &class.name) {
            receivers.push(class.name.clone());
        }
        for name in receivers {
            let Some(ci) = self.classes.get_mut(&name) else {
                continue;
            };
            let implementation = if method.is_static {
                ci.static_method_impl_classes.get(&method_key)
            } else {
                ci.method_impl_classes.get(&method_key)
            };
            if name != class.name && implementation != Some(&class.name) {
                continue;
            }
            // An ABSTRACT declaration keeps the signature it declared. Its implementors narrow the
            // return covariantly (`create(): SQLiteSchemaManager` over `create(): AbstractSchemaManager`),
            // and reflection and the eval bridge's declared-signature registration read this copy.
            let declared_abstract = if method.is_static {
                ci.abstract_static_methods.contains(&method_key)
                    && ci.static_methods.get(&method_key).is_some_and(|sig| sig.declared_return)
            } else {
                ci.abstract_methods.contains(&method_key)
                    && ci.methods.get(&method_key).is_some_and(|sig| sig.declared_return)
            };
            if name != class.name && declared_abstract {
                continue;
            }
            let sig = if method.is_static {
                ci.static_methods.get_mut(&method_key)
            } else {
                ci.methods.get_mut(&method_key)
            };
            if let Some(sig) = sig {
                sig.return_type = effective_return.clone();
            }
        }
        self.update_method_callable_return_metadata(
            &class.name,
            &method_key,
            method.is_static,
            &effective_return,
            &callable_return_sigs,
            &callable_array_return_sigs,
        );
    }

    /// The classes whose `method_key` dispatches to `impl_class`'s body (see
    /// `Checker::method_impl_users_index`), from an index rebuilt whenever the class count
    /// changed or a pass round starts.
    ///
    /// The index is a candidate list: the caller re-checks each entry against the live maps, so a
    /// map edited since the build only costs a miss, never a wrong write.
    fn method_impl_users(
        &mut self,
        impl_class: &str,
        method_key: &str,
        is_static: bool,
    ) -> Vec<String> {
        let class_count = self.classes.len();
        if self
            .method_impl_users_index
            .as_ref()
            .is_none_or(|(built_for, _)| *built_for != class_count)
        {
            let mut index: crate::fast_hash::FastMap<(String, String, bool), Vec<String>> =
                Default::default();
            for (name, info) in &self.classes {
                for (key, implementation) in &info.method_impl_classes {
                    index
                        .entry((implementation.clone(), key.clone(), false))
                        .or_default()
                        .push(name.clone());
                }
                for (key, implementation) in &info.static_method_impl_classes {
                    index
                        .entry((implementation.clone(), key.clone(), true))
                        .or_default()
                        .push(name.clone());
                }
            }
            self.method_impl_users_index = Some((class_count, index));
        }
        self.method_impl_users_index
            .as_ref()
            .and_then(|(_, index)| {
                index.get(&(impl_class.to_string(), method_key.to_string(), is_static))
            })
            .cloned()
            .unwrap_or_default()
    }

    /// Returns whether an untyped method that never returns normally -- a placeholder that
    /// throws, like Laravel's `Facade::getFacadeAccessor()` -- must keep a value-carrying return
    /// storage because code the compiler cannot see may override it.
    ///
    /// Its body alone infers `void`, so every compiled call through it (`static::accessor()`)
    /// was typed `void` and discarded what an interpreted subclass's override returned: every
    /// runtime-loaded facade resolved the empty accessor name, and `Date::now()` failed with
    /// "Target class [date] does not exist". A compiled override is already covered by
    /// `unify_override_family_return_types`; this is for the ones only eval can declare.
    fn diverging_placeholder_may_be_overridden_elsewhere(
        &self,
        class: &FlattenedClass,
        method: &ClassMethod,
        returns_a_value: bool,
    ) -> bool {
        self.program_has_interpreted_callers
            && !returns_a_value
            && method.visibility != crate::parser::ast::Visibility::Private
            && !method.is_final
            && !class.is_final
            && !method.name.eq_ignore_ascii_case("__construct")
            && crate::termination::block_guarantees_function_exit_with_divergence(
                &method.body,
                &|_| false,
            )
    }

    /// Returns whether a descendant class declares its own body for this method.
    ///
    /// A generic `array` hint is narrowed to the element type the body returns, and a call typed
    /// at this class reads elements with that narrowed type. An override narrows independently,
    /// so a family member must keep the declared contract, whose element reads follow the
    /// array's runtime value type -- the reading an interface-typed call already uses. Twig's
    /// `AbstractExpressionParser::getAliases(): array { return []; }` is the shape: narrowed to
    /// `array<never>`, `[$this->getName(), ...$this->getAliases()]` read DotExpressionParser's
    /// `['?.']` as null, the operator regex gained an empty alternative, and the lexer looped.
    fn method_overridden_by_descendant(&self, class_name: &str, method_key: &str, is_static: bool) -> bool {
        self.classes.iter().any(|(descendant, info)| {
            let declaring = if is_static {
                info.static_method_declaring_classes.get(method_key)
            } else {
                info.method_declaring_classes.get(method_key)
            };
            descendant != class_name
                && declaring.is_some_and(|declaring| declaring == descendant)
                && self.is_subclass_of(descendant, class_name)
        })
    }

    /// Keeps an override's return in the storage its parent's vtable slot promises.
    ///
    /// The schema adopts the ancestor's return storage for an override that narrows it to a
    /// different representation (`adopt_ancestor_return_storage`); resolving the declared hint
    /// here would narrow it straight back, and a call through the parent-typed slot would again
    /// read a raw object where a boxed cell belongs. A parent's PRIVATE method is not overridden,
    /// and a constructor is never entered through an ancestor's slot.
    fn ancestor_return_storage(
        &self,
        class_name: &str,
        method_key: &str,
        effective: PhpType,
    ) -> PhpType {
        if method_key == "__construct" {
            return effective;
        }
        let Some(parent) = self.classes.get(class_name).and_then(|info| info.parent.clone()) else {
            return effective;
        };
        let Some(parent_info) = self.classes.get(&parent) else {
            return effective;
        };
        if parent_info.method_visibilities.get(method_key) == Some(&crate::parser::ast::Visibility::Private) {
            return effective;
        }
        let Some(ancestor) = parent_info.methods.get(method_key) else {
            return effective;
        };
        // An untyped ancestor whose storage is boxed binds its overrides the same way a declared
        // `mixed` does: the ancestor's own callers read a box from every body the vtable can run.
        let ancestor_repr = ancestor.return_type.codegen_repr();
        if !matches!(ancestor_repr, PhpType::Mixed | PhpType::Union(_))
            || effective.codegen_repr() == ancestor_repr
        {
            return effective;
        }
        ancestor.return_type.clone()
    }

    /// Resolves the return type of a method whose body contains `yield`.
    ///
    /// The result is always `Generator`, because that is the object PHP hands back when
    /// the generator method is called. A declared return hint is still resolved and
    /// validated: hints that accept a `Generator` (`Generator`, `Traversable`,
    /// `iterable`, `mixed`, …) pass through, anything else is reported as an
    /// incompatible return type. Unlike the non-generator path there is no
    /// return-coverage check — a generator body legitimately has no `return` at all.
    fn generator_method_return_type(
        &mut self,
        class: &FlattenedClass,
        method: &ClassMethod,
    ) -> Result<PhpType, CompileError> {
        let generator_ty = PhpType::Object("Generator".to_string());
        if let Some(type_ann) = method.return_type.as_ref() {
            let declared = self.resolve_declared_return_type_hint(
                type_ann,
                method.span,
                &format!("Method '{}::{}'", class.name, method.name),
            )?;
            if !self.generator_return_type_accepts(&declared) {
                self.require_compatible_return_hint(
                    type_ann,
                    &declared,
                    &generator_ty,
                    true,
                    true,
                    method.span,
                    &format!("Method '{}::{}' return type", class.name, method.name),
                )?;
            }
        }
        Ok(generator_ty)
    }

    /// Updates callable-return metadata on the implementing class and its inheriting views.
    fn update_method_callable_return_metadata(
        &mut self,
        class_name: &str,
        method_key: &str,
        is_static: bool,
        return_type: &PhpType,
        callable_return_sigs: &[FunctionSig],
        callable_array_return_sigs: &[FunctionSig],
    ) {
        let callable_sig = (return_type == &PhpType::Callable)
            .then(|| matching_callable_sig(callable_return_sigs))
            .flatten();
        let callable_array_sig = is_callable_array_return_type(return_type)
            .then(|| matching_callable_sig(callable_array_return_sigs))
            .flatten();
        let mut receivers = self.method_impl_users(class_name, method_key, is_static);
        if !receivers.iter().any(|name| name == class_name) {
            receivers.push(class_name.to_string());
        }
        for name in receivers {
            let Some(class_info) = self.classes.get_mut(&name) else {
                continue;
            };
            if let Some(sig) = &callable_sig {
                class_info
                    .callable_method_return_sigs
                    .insert(method_key.to_string(), sig.clone());
            } else {
                class_info.callable_method_return_sigs.remove(method_key);
            }
            if let Some(sig) = &callable_array_sig {
                class_info
                    .callable_array_method_return_sigs
                    .insert(method_key.to_string(), sig.clone());
            } else {
                class_info.callable_array_method_return_sigs.remove(method_key);
            }
        }
    }
}

/// Returns true when a method return type is a homogeneous array of callables.
fn is_callable_array_return_type(return_type: &PhpType) -> bool {
    match return_type {
        PhpType::Array(elem_ty) => elem_ty.as_ref() == &PhpType::Callable,
        PhpType::AssocArray { value, .. } => value.as_ref() == &PhpType::Callable,
        _ => false,
    }
}

/// Returns one callable signature only when every return path has the same contract.
fn matching_callable_sig(return_sigs: &[FunctionSig]) -> Option<FunctionSig> {
    let first = return_sigs.first()?.clone();
    if return_sigs.iter().all(|sig| sig == &first) {
        Some(callable_return_codegen_sig(first))
    } else {
        None
    }
}

/// Returns the callable-return metadata for one method's returned closure, unchanged.
///
/// An undeclared parameter stays `mixed`, the type the closure is compiled with; see the twin
/// in `functions::resolution::signature`, which used to rewrite it to a fabricated `int`.
fn callable_return_codegen_sig(sig: FunctionSig) -> FunctionSig {
    sig
}

/// Whether `body` (closures included) tests the variable `name` with `instanceof`.
fn body_tests_instanceof(body: &[crate::parser::ast::Stmt], name: &str) -> bool {
    let mut found = false;
    crate::parser::ast::visit::walk_nodes(body, &mut |node| {
        if let crate::parser::ast::visit::Node::Expr(expr) = node {
            if let crate::parser::ast::ExprKind::InstanceOf { value, .. } = &expr.kind {
                if matches!(&value.kind, crate::parser::ast::ExprKind::Variable(var) if var == name) {
                    found = true;
                }
            }
        }
    });
    found
}
