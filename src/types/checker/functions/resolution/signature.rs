//! Purpose:
//! Handles function resolution signature details for call checking.
//! Materializes signatures or specialized metadata used by argument validation and return inference.
//!
//! Called from:
//! - `crate::types::checker::functions::resolution`
//!
//! Key details:
//! - Specialized and builtin signatures must expose the caller-visible parameter contract expected by call-argument planning.

use std::collections::HashMap;

use crate::errors::CompileError;
use crate::types::{FunctionSig, PhpType, TypeEnv};

use super::super::super::{Checker, FnDecl};

impl Checker {
    /// Resolves a function's signature given its declaration, parameter types, and body.
    ///
    /// Builds a `TypeEnv` from the provided parameter types, then type-checks the body
    /// while collecting return type information. Handles callable parameters by saving and
    /// restoring their metadata around the body check. Validates declared return types
    /// against inferred returns, and applies PHP's generator rules (functions containing
    /// `yield` implicitly return `Generator`). Stores the final signature in `self.functions`.
    ///
    /// Returns the resolved return type, or a `CompileError` if the body fails to type-check
    /// or return types are incompatible with any declared annotation.
    pub(crate) fn resolve_function_signature(
        &mut self,
        name: &str,
        decl: &FnDecl,
        param_types: Vec<(String, PhpType)>,
    ) -> Result<PhpType, CompileError> {
        let mut local_env: TypeEnv = HashMap::new();
        for (pname, pty) in &param_types {
            local_env.insert(pname.clone(), pty.clone());
        }
        // Seed the request superglobals so a function body can read/write
        // `$_SERVER`/`$_GET`/`$_POST` without a `global` declaration. `or_insert`
        // never clobbers a parameter that happens to share a superglobal name.
        for name in crate::superglobals::SUPERGLOBALS {
            local_env
                .entry((*name).to_string())
                .or_insert_with(crate::superglobals::superglobal_type);
        }
        let function_key = name.to_string();
        let callable_param_names: Vec<String> = param_types
            .iter()
            .filter(|(_, pty)| {
                pty == &PhpType::Callable || is_callable_array_return_type(pty)
            })
            .map(|(pname, _)| pname.clone())
            .collect();
        let declared_callable_param_names: Vec<String> = param_types
            .iter()
            .enumerate()
            .filter(|(idx, (_, pty))| {
                pty == &PhpType::Callable
                    && decl.param_types.get(*idx).is_some_and(|type_ann| type_ann.is_some())
            })
            .map(|(_, (pname, _))| pname.clone())
            .collect();
        let saved_callable_param_names = self.callable_param_names.clone();
        for pname in &declared_callable_param_names {
            self.callable_param_names.insert(pname.clone());
        }
        let saved_callable_metadata: Vec<_> = callable_param_names
            .iter()
            .map(|pname| {
                (
                    pname.clone(),
                    self.callable_sigs.get(pname).cloned(),
                    self.closure_return_types.get(pname).cloned(),
                )
            })
            .collect();
        for pname in &callable_param_names {
            if let Some(sig) = self
                .callable_param_sigs
                .get(&(function_key.clone(), pname.clone()))
                .cloned()
            {
                self.closure_return_types
                    .insert(pname.clone(), sig.return_type.clone());
                self.callable_sigs.insert(pname.clone(), sig);
            } else {
                self.closure_return_types.remove(pname);
                self.callable_sigs.remove(pname);
            }
        }

        let provisional_sig = FunctionSig {
            params: param_types.clone(),
            param_type_exprs: decl
                .param_types
                .iter()
                .cloned()
                .chain(decl.variadic.iter().map(|_| decl.variadic_type.clone()))
                .collect(),
            param_attributes: decl.param_attributes.clone(),
            defaults: decl.defaults.clone(),
            return_type: self.provisional_return_type(decl),
            declared_return: decl.return_type.is_some(),
            by_ref_return: decl.by_ref_return,
            ref_params: decl.ref_params.clone(),
            deprecation: None,
            declared_params: decl
                .param_types
                .iter()
                .map(|type_ann| type_ann.is_some())
                .chain(decl.variadic.iter().map(|_| decl.variadic_type.is_some()))
                .collect(),
            variadic: decl.variadic.clone(),
        };
        self.functions.insert(name.to_string(), provisional_sig);

        let mut return_type = PhpType::Void;
        let mut all_return_infos = Vec::new();
        let mut callable_return_sigs = Vec::new();
        let mut callable_array_return_sigs = Vec::new();
        let mut errors = Vec::new();
        let ref_param_names: Vec<String> = decl
            .params
            .iter()
            .zip(decl.ref_params.iter())
            .filter(|(_, is_ref)| **is_ref)
            .map(|(name, _)| name.clone())
            .collect();
        // Every parameter is bound unconditionally on entry, so all of them are recorded at
        // binding depth 0 (a missing entry means "seeded, not bound here", which is not
        // kill/retype eligible).
        let param_names: Vec<String> = decl
            .params
            .iter()
            .cloned()
            .chain(decl.variadic.iter().cloned())
            .collect();
        // A parameter with a declared type hint is a contract: it never becomes kill/retype
        // eligible inside the body, in either mode.
        let typed_param_names: Vec<String> = decl
            .params
            .iter()
            .zip(decl.param_types.iter())
            .filter(|(_, type_ann)| type_ann.is_some())
            .map(|(name, _)| name.clone())
            .chain(
                decl.variadic
                    .iter()
                    .filter(|_| decl.variadic_type.is_some())
                    .cloned(),
            )
            .collect();
        let prev_by_ref_return = self.current_by_ref_return;
        self.current_by_ref_return = decl.by_ref_return;
        self.loop_storage_types
            .retain(|(scope, _), _| scope != &function_key);
        let previous_loop_storage_scope =
            std::mem::replace(&mut self.current_loop_storage_scope, function_key.clone());
        self.resolving_functions.insert(function_key.clone());
        // Track the innermost function body being walked (nested resolution of
        // a called function saves and restores the enclosing name), so friend
        // channels can identify compiler-owned procedural aliases.
        let previous_function = self.current_function.replace(function_key.clone());
        // The storage this frame already holds on entry: the parameters, with the types this
        // call site specialized them to. The superglobals `local_env` also carries are not the
        // frame's own storage and are excluded from marking outright, so they are left out.
        let pre_bound_own_storage: std::collections::HashMap<String, PhpType> =
            param_types.iter().cloned().collect();
        let body_check_result = self.with_local_storage_context(
            ref_param_names,
            param_names,
            typed_param_names,
            pre_bound_own_storage,
            &decl.body,
            |checker| {
                for stmt in &decl.body {
                    if let Err(error) = checker.check_stmt(stmt, &mut local_env) {
                        errors.extend(error.flatten());
                    }
                    checker.collect_return_infos(stmt, &local_env, &mut all_return_infos);
                    checker.collect_return_callable_sigs(
                        stmt,
                        &local_env,
                        &mut callable_return_sigs,
                    );
                    checker.collect_return_callable_array_sigs(
                        stmt,
                        &local_env,
                        &mut callable_array_return_sigs,
                    );
                }
                Ok(())
            },
        );
        self.current_function = previous_function;
        self.resolving_functions.remove(&function_key);
        self.current_loop_storage_scope = previous_loop_storage_scope;
        self.current_by_ref_return = prev_by_ref_return;
        self.callable_param_names = saved_callable_param_names;
        body_check_result?;
        if let Some((widened, widened_indexes)) =
            self.by_ref_array_params_widened_by_body(decl, &param_types, &local_env)
        {
            // The body widened a BY-REFERENCE array parameter — a loop storage contract, an
            // element write of a `Mixed` value, an `$a[] =` of a wider one. That widening
            // rewrites the CALLER's storage to boxed slots, so the parameter type the caller
            // sees has to say so, and the body has to be compiled against it rather than
            // against the narrow type it started from. Re-resolving with the widened types is
            // a FIXED POINT, not a loop: `array<mixed>` is the top of this lattice, so the
            // second pass cannot widen again, and `resolving_by_ref_widening` makes that a
            // guarantee rather than a hope.
            if self.resolving_by_ref_widening.insert(function_key.clone()) {
                for index in widened_indexes {
                    self.by_ref_widened_params
                        .insert((function_key.clone(), index));
                }
                let resolved = self.resolve_function_signature(name, decl, widened);
                self.resolving_by_ref_widening.remove(&function_key);
                return resolved;
            }
        }
        for pname in &callable_param_names {
            if let Some(sig) = self.callable_sigs.get(pname).cloned() {
                self.callable_param_sigs
                    .insert((function_key.clone(), pname.clone()), sig);
            }
        }
        for (pname, saved_sig, saved_return) in saved_callable_metadata {
            if let Some(sig) = saved_sig {
                self.callable_sigs.insert(pname.clone(), sig);
            } else {
                self.callable_sigs.remove(&pname);
            }
            if let Some(return_ty) = saved_return {
                self.closure_return_types.insert(pname, return_ty);
            } else {
                self.closure_return_types.remove(&pname);
            }
        }
        if !errors.is_empty() {
            return Err(CompileError::from_many(errors));
        }

        let contains_yield = super::super::super::yield_validation::body_contains_yield(&decl.body);
        if contains_yield {
            let generator_ty = PhpType::Object("Generator".to_string());
            if let Some(type_ann) = decl.return_type.as_ref() {
                let declared_ret = self.resolve_declared_return_type_hint(
                    type_ann,
                    decl.span,
                    &format!("Function '{}'", name),
                )?;
                if !self.generator_return_type_accepts(&declared_ret) {
                    self.require_compatible_return_type(
                        &declared_ret,
                        &generator_ty,
                        true,
                        decl.span,
                        &format!("Function '{}' return type", name),
                    )?;
                }
            }
            return_type = generator_ty;
        } else if let Some(type_ann) = decl.return_type.as_ref() {
            let declared_ret = self.resolve_declared_return_type_hint(
                type_ann,
                decl.span,
                &format!("Function '{}'", name),
            )?;
            if matches!(declared_ret, PhpType::Never) && Self::body_contains_return(&decl.body) {
                return Err(CompileError::new(
                    decl.span,
                    &format!("Function '{}' declared never must not return", name),
                ));
            }
            self.require_declared_return_coverage(
                &declared_ret,
                &decl.body,
                decl.span,
                &format!("Function '{}'", name),
            )?;
            if !all_return_infos.is_empty() {
                for return_info in &all_return_infos {
                    self.require_compatible_return_type(
                        &declared_ret,
                        &return_info.ty,
                        return_info.has_value,
                        decl.span,
                        &format!("Function '{}' return type", name),
                    )?;
                }
            }
            return_type = if Self::is_generic_array_hint(&declared_ret)
                && matches!(inferred_specific_array_type_from_infos(&all_return_infos), Some(_))
            {
                inferred_specific_array_type_from_infos(&all_return_infos).unwrap()
            } else {
                declared_ret
            };
        } else if !all_return_infos.is_empty() {
            return_type = all_return_infos[0].ty.clone();
            for return_info in &all_return_infos[1..] {
                return_type = Self::wider_type(&return_type, &return_info.ty);
            }
        }

        let sig = FunctionSig {
            params: param_types,
            param_type_exprs: decl
                .param_types
                .iter()
                .cloned()
                .chain(decl.variadic.iter().map(|_| decl.variadic_type.clone()))
                .collect(),
            param_attributes: decl.param_attributes.clone(),
            defaults: decl.defaults.clone(),
            return_type: return_type.clone(),
            declared_return: decl.return_type.is_some(),
            by_ref_return: decl.by_ref_return,
            ref_params: decl.ref_params.clone(),
            declared_params: decl
                .param_types
                .iter()
                .map(|type_ann| type_ann.is_some())
                .chain(decl.variadic.iter().map(|_| decl.variadic_type.is_some()))
                .collect(),
            variadic: decl.variadic.clone(),
            deprecation: crate::types::checker::schema::validation::extract_deprecation(
                &decl.attributes,
            ),
        };
        self.functions.insert(name.to_string(), sig);
        if return_type == PhpType::Callable {
            if let Some(callable_sig) = matching_callable_sig(&callable_return_sigs) {
                self.callable_return_sigs
                    .insert(name.to_string(), callable_sig);
            } else {
                self.callable_return_sigs.remove(name);
            }
        } else {
            self.callable_return_sigs.remove(name);
        }
        if is_callable_array_return_type(&return_type) {
            if let Some(callable_sig) = matching_callable_sig(&callable_array_return_sigs) {
                self.callable_array_return_sigs
                    .insert(name.to_string(), callable_sig);
            } else {
                self.callable_array_return_sigs.remove(name);
            }
        } else {
            self.callable_array_return_sigs.remove(name);
        }

        Ok(return_type)
    }

    /// Returns the parameter list re-typed for a body that WIDENED a by-reference array
    /// parameter, or `None` when none did.
    ///
    /// A by-reference parameter is compiled against the CALLER's storage. When the body ends up
    /// holding boxed element slots — `foreach ($a as $k => $v) { $a[$k] = $v * 2; }` does, through
    /// the loop storage contract — the caller's array is rewritten to boxed slots too, and a
    /// caller still typed `array<int>` reads those boxes back as ADDRESSES. Reporting the exit
    /// type here is what lets the call site convert its own local and re-type it.
    ///
    /// Only a WIDENING is reported. A body that merely reads the parameter, or one that sorts it
    /// in place, keeps its narrow element type — which is what keeps `sort($a)` compiling, since
    /// the backend has no Mixed-element sort.
    fn by_ref_array_params_widened_by_body(
        &self,
        decl: &FnDecl,
        param_types: &[(String, PhpType)],
        local_env: &TypeEnv,
    ) -> Option<(Vec<(String, PhpType)>, Vec<usize>)> {
        let mut widened = param_types.to_vec();
        let mut widened_indexes = Vec::new();
        let mut any = false;
        for (index, (pname, entry_ty)) in param_types.iter().enumerate() {
            if !decl.ref_params.get(index).copied().unwrap_or(false) {
                continue;
            }
            let Some(exit_ty) = local_env.get(pname) else {
                continue;
            };
            if let Some(widened_ty) = null_entry_by_ref_param_widens(entry_ty, exit_ty) {
                widened[index].1 = widened_ty;
                widened_indexes.push(index);
                any = true;
                continue;
            }
            if !array_element_representation_widens(entry_ty, exit_ty) {
                continue;
            }
            widened[index].1 = exit_ty.clone();
            widened_indexes.push(index);
            any = true;
        }
        any.then(|| (widened, widened_indexes))
    }

    /// Picks the return type for the *provisional* signature published before a free
    /// function's body is walked.
    ///
    /// The provisional signature exists so that a call to the function from inside its own
    /// body — direct recursion, or a mutual-recursion cycle re-entering a function already
    /// on the resolution stack — can be typed at all. Whatever this returns is what such a
    /// self-call's expression type will be, since `resolving_functions` suppresses
    /// re-specialization for in-flight functions (see `specialization.rs`).
    ///
    /// Resolution order mirrors the authoritative pass that runs after the body walk:
    /// - a body containing `yield` always produces a `Generator`, whatever the annotation says;
    /// - otherwise an explicit return hint is trusted verbatim;
    /// - an unhinted function keeps the historical `Int` placeholder. That placeholder is not
    ///   observable for a function with a base case, because the final unhinted return type is
    ///   the `wider_type` merge of every `return` and the base case's real type absorbs it. It
    ///   only survives for a function whose *only* return is the recursive call, which is
    ///   unconditional infinite recursion and cannot produce a value anyway.
    ///
    /// A hint that fails to resolve (unknown class, misplaced `never`) falls back to the
    /// placeholder rather than raising here: the authoritative pass resolves the same hint
    /// again and reports that error with its original span and ordering.
    fn provisional_return_type(&self, decl: &FnDecl) -> PhpType {
        if super::super::super::yield_validation::body_contains_yield(&decl.body) {
            return PhpType::Object("Generator".to_string());
        }
        let Some(type_ann) = decl.return_type.as_ref() else {
            return PhpType::Int;
        };
        self.resolve_declared_return_type_hint(type_ann, decl.span, "")
            .unwrap_or(PhpType::Int)
    }

    /// Returns true when a declared generator return annotation accepts
    /// the actual `Generator` object returned when the body contains `yield`.
    ///
    /// Shared with the method pass (`crate::types::checker::method_pass`) so a
    /// generator method's hint is validated by exactly the same rule as a
    /// generator function's.
    pub(crate) fn generator_return_type_accepts(&self, declared_ret: &PhpType) -> bool {
        if matches!(declared_ret, PhpType::Object(name) if name == "Traversable") {
            return true;
        }
        self.type_accepts(declared_ret, &PhpType::Object("Generator".to_string()))
    }
}

/// Infers a concrete array type from return info when the declared return type is a generic `array` hint.
///
/// Returns `Some(PhpType)` only when every non-void, non-empty return in
/// `return_types` is the same array type (including `array<T>` or `assocArray`
/// shapes). An empty indexed array is neutral because it can be materialized in
/// either concrete storage family at the return boundary. Returns `None` if
/// non-empty returns differ, include non-array types, or are all `void`.
/// Returns whether an array-ish type's ELEMENT representation WIDENED from `entry` to `exit`.
///
/// Two widenings matter, and only those two. An element representation that became `Mixed` means
/// boxed slots where there were raw ones. An empty `array<never>` that gained an element type
/// means slots where there were none. The direction is load-bearing: a callee compiled for
/// `array<int>` must never re-type a caller holding `array<mixed>`, because it cannot unbox
/// storage it does not own.
///
/// A change of container KIND counts too when the body turned a list into a hash — writing a
/// string key into an empty `array<never>` does exactly that, and the caller reading it back as
/// a packed list is the same defect one layer out: `$h = []; fill_keyed($h);` printed `[10, 13]`
/// where php printed `{"k":1,"j":2}`.
pub(in crate::types) fn array_element_representation_widens(entry: &PhpType, exit: &PhpType) -> bool {
    match (entry.codegen_repr(), exit.codegen_repr()) {
        (PhpType::Array(entry_elem), PhpType::Array(exit_elem)) => {
            element_representation_widens(&entry_elem, &exit_elem)
        }
        (
            PhpType::AssocArray {
                value: entry_value, ..
            },
            PhpType::AssocArray {
                value: exit_value, ..
            },
        ) => element_representation_widens(&entry_value, &exit_value),
        (PhpType::Array(entry_elem), PhpType::AssocArray { .. }) => {
            entry_elem.codegen_repr() == PhpType::Void || matches!(*entry_elem, PhpType::Never)
        }
        _ => false,
    }
}

/// Returns whether one array element representation is wider than another.
///
/// `never` is compared on the RAW type, not on `codegen_repr()`: an empty array's element type
/// normalizes to `Void`, so asking `codegen_repr() == Never` is always false and the empty-array
/// case silently never fired.
fn element_representation_widens(entry: &PhpType, exit: &PhpType) -> bool {
    let entry_repr = entry.codegen_repr();
    let exit_repr = exit.codegen_repr();
    if entry_repr == exit_repr {
        return false;
    }
    matches!(entry, PhpType::Never)
        || entry_repr == PhpType::Void
        || exit_repr == PhpType::Mixed
}

fn inferred_specific_array_type_from_infos(
    return_types: &[super::super::returns::ReturnInfo],
) -> Option<PhpType> {
    let mut specific: Option<PhpType> = None;
    let mut empty_array: Option<PhpType> = None;
    for return_info in return_types {
        let return_ty = &return_info.ty;
        if matches!(return_ty, PhpType::Void) {
            continue;
        }
        if !matches!(return_ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
            return None;
        }
        if matches!(return_ty, PhpType::Array(elem) if elem.as_ref() == &PhpType::Never) {
            empty_array = Some(return_ty.clone());
            continue;
        }
        match &specific {
            None => specific = Some(return_ty.clone()),
            Some(existing) if existing == return_ty => {}
            _ => return None,
        }
    }
    specific.or(empty_array)
}

/// Returns true when a function return type is a homogeneous array of callables.
fn is_callable_array_return_type(return_type: &PhpType) -> bool {
    match return_type {
        PhpType::Array(elem_ty) => elem_ty.as_ref() == &PhpType::Callable,
        PhpType::AssocArray { value, .. } => value.as_ref() == &PhpType::Callable,
        _ => false,
    }
}

/// Computes the callable signature metadata for matching callable.
fn matching_callable_sig(return_sigs: &[FunctionSig]) -> Option<FunctionSig> {
    let first = return_sigs.first()?.clone();
    if return_sigs.iter().all(|sig| sig == &first) {
        Some(callable_return_codegen_sig(first))
    } else {
        None
    }
}

/// Computes the callable signature metadata for callable return codegen.
fn callable_return_codegen_sig(mut sig: FunctionSig) -> FunctionSig {
    for (idx, (_, ty)) in sig.params.iter_mut().enumerate() {
        if !sig.declared_params.get(idx).copied().unwrap_or(false)
            && matches!(ty, PhpType::Mixed)
        {
            *ty = PhpType::Int;
        }
    }
    sig
}

/// Widens a by-reference parameter whose caller passed NULL and whose body writes something else.
///
/// `function f(&$a) { $a = 5; }` called as `$x = null; f($x);` is php's out-parameter idiom, and
/// the whole point of it is that `$x` is `int(5)` afterwards. elephc typed the parameter from the
/// caller — `null` — and left it there, so the body's write went through the pointer while the
/// CALLER kept reading its slot as null-typed. Every later read constant-folded to `NULL`:
/// `var_dump($x)` printed NULL and `if ($x === null)` took the wrong branch, with no diagnostic
/// anywhere. The call site's EIR was byte-identical to the case that works, because the
/// difference was never in the EIR.
///
/// The widened type is `mixed`, not the narrower `<written>|null` union it might look like it
/// should be. Both are boxed, but a nullable SCALAR union has its own inline representation — a
/// payload word plus a tag word, sixteen bytes — while the caller's slot was laid out for the
/// eight-byte null it was holding, so the callee's write ran past the end of it and the program
/// segfaulted. `mixed` is one pointer whatever the payload, which is the only representation
/// both sides of the boundary agree on here.
///
/// Only a `null` entry widens. A parameter the caller passed a real value to keeps elephc's
/// monomorphized contract: `$x = 1; f($x)` with an `$a = "s"` body is still the reassignment
/// error it has always been, because widening THAT silently would change what the backend
/// compiles for every existing by-reference call.
fn null_entry_by_ref_param_widens(entry_ty: &PhpType, exit_ty: &PhpType) -> Option<PhpType> {
    if !matches!(entry_ty, PhpType::Void) {
        return None;
    }
    if matches!(exit_ty, PhpType::Void | PhpType::Never) {
        return None;
    }
    Some(PhpType::Mixed)
}
