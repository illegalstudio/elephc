//! Purpose:
//! Validates function returns semantics for the checker.
//! Keeps call diagnostics and return-flow analysis consistent with signatures and inferred expression types.
//!
//! Called from:
//! - `crate::types::checker::functions`
//!
//! Key details:
//! - Diagnostics should map shared planner errors back to source spans without duplicating call semantics.

use crate::errors::CompileError;
use crate::parser::ast::{Stmt, StmtKind, TypeExpr};
use crate::types::param_binding::param_accepts_weak_string_coercion;
use crate::types::checker::type_compat::unions::normalize_union_members;
use crate::types::{FunctionSig, PhpType, TypeEnv};

use super::super::Checker;

/// Holds the inferred type and whether a return statement provided a value.
/// Used by return-type checking to collect return type information across all paths.
#[derive(Clone)]
pub(crate) struct ReturnInfo {
    pub ty: PhpType,
    pub has_value: bool,
    pub strict_types: bool,
}

/// Adds one scalar member to an inferred return type, the way [`nullable_return_type`] adds
/// `Void` — the shared shape behind `string|false` and `?string`.
///
/// `Checker::resolve_type_expr` expands a declared `string|false` to `Union([Str, False])`,
/// so building the same shape here keeps a hint-less union return byte-identical to the
/// declared spelling, which already lowered correctly. `codegen_repr()` maps such a union to
/// a boxed representation, which is what carries `false` distinctly from `""` instead of
/// coercing it into the other arm's zero value.
///
/// `Mixed` already admits every value and is left alone; an existing union gains the member
/// rather than nesting, so folding three or more returns pairwise stays flat.
fn union_return_type(other: &PhpType, member: PhpType) -> PhpType {
    normalize_union_members(vec![other.clone(), member])
}

/// Makes an inferred return type nullable, the way a declared `?T` hint resolves.
///
/// `Checker::resolve_type_expr` expands `?T` to `Union([T, Void])`, and
/// `nullable_match_arm_type` builds the same shape for ternary/match joins, so this keeps a
/// hint-less nullable return byte-identical to both. `codegen_repr()` maps such a union to
/// `Mixed` (or `TaggedScalar` for `int|null`), which is why `return null` then boxes instead
/// of being coerced into the other arm's zero value.
///
/// `Mixed` already admits null and is left alone; an existing union gains `Void` as one more
/// member rather than nesting; `Void`/`Never` collapse to plain `Void` so a function whose
/// every path yields null does not become `null|null`.
/// Returns whether `ty` is a scalar the legacy `string`/`float` return fold absorbs.
///
/// KNOWN DIVERGENCE kept on purpose: php returns `5` as `int(5)` from a function whose other
/// return is a string, where this fold makes it `"5"`. Widening those to `mixed` too is correct but
/// boxes every such return; it is left for its own measured change.
fn scalar_absorbed_by_str_or_float(ty: &PhpType) -> bool {
    matches!(
        ty,
        PhpType::Int | PhpType::Bool | PhpType::False | PhpType::Str | PhpType::Float
    )
}

fn nullable_return_type(other: &PhpType) -> PhpType {
    match other {
        PhpType::Mixed => PhpType::Mixed,
        PhpType::Void | PhpType::Never => PhpType::Void,
        PhpType::Union(members) => {
            if members.iter().any(|member| matches!(member, PhpType::Void)) {
                PhpType::Union(members.clone())
            } else {
                let mut members = members.clone();
                members.push(PhpType::Void);
                PhpType::Union(members)
            }
        }
        other => PhpType::Union(vec![other.clone(), PhpType::Void]),
    }
}

impl Checker {
    /// Resolves the array contract a declared bare `array` hint must carry, from what the body
    /// actually returns, or `None` to keep the declared hint.
    ///
    /// PHP has ONE array type; elephc splits it into an indexed vector (`Array`) and a hash
    /// (`AssocArray`). A bare `array` hint resolves to `Array(Mixed)` -- the INDEXED repr -- so when a
    /// body returns a hash on one path and an indexed array on another, the contract has to say which
    /// storage the caller will read. Answering indexed is not an option: `coerce_container_to_return_type`
    /// then retypes the hash as indexed WITHOUT converting it (converting would discard string keys),
    /// and every consumer that trusts the static type misreads the container -- `json_encode` prints raw
    /// slot words, `serialize` writes integer keys, `in_array` faults. Measured on a 36-consumer survey,
    /// 18 diverged from `php -n` 8.5.10 and 5 of those segfaulted.
    ///
    /// So the join of an indexed target and a hash target is the HASH, for the same reason
    /// `types::array_storage::join_array_storage_conversion` gives: a hash represents any PHP array, a
    /// packed vector cannot. It costs nothing in observable behavior -- a hash holding sequential
    /// integer keys is indistinguishable from a list to every consumer, measured against `php -n`:
    /// `json_encode` prints `["a","b"]`, `serialize` writes `a:2:{i:0;s:1:"a";i:1;s:1:"b";}`.
    ///
    /// Returning `None` leaves the declared hint in place, which is right when no return says otherwise
    /// (an empty body, or a path returning something that is not an array at all -- that disagreement
    /// is a return-type error the compatibility check reports, not something to widen here).
    pub(crate) fn generic_array_return_contract(return_infos: &[ReturnInfo]) -> Option<PhpType> {
        let mut specific: Option<PhpType> = None;
        let mut empty_array: Option<PhpType> = None;
        let mut saw_hash = false;
        let mut saw_unshaped = false;
        let mut disagreed = false;
        for return_info in return_infos {
            let return_ty = &return_info.ty;
            if matches!(return_ty, PhpType::Void) {
                continue;                                   // a `return null` path says nothing about storage
            }
            if !matches!(return_ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
                // A `Mixed` return -- `$this->h[$k] ?? []` boxes into one -- names no storage. It
                // cannot argue AGAINST the hash, though: whatever it holds is unboxed into the
                // contract, while a hash handed to an indexed contract is misread outright.
                saw_unshaped = true;
                continue;
            }
            if matches!(return_ty, PhpType::AssocArray { .. }) {
                saw_hash = true;
            } else if matches!(return_ty, PhpType::Array(elem) if elem.as_ref() == &PhpType::Never) {
                // `return []` commits to no storage at all: it fits whichever shape the others pick.
                empty_array = Some(return_ty.clone());
                continue;
            }
            match &specific {
                None => specific = Some(return_ty.clone()),
                Some(existing) if existing == return_ty => {}
                _ => disagreed = true,
            }
        }
        if saw_hash && (disagreed || saw_unshaped) {
            return Some(PhpType::AssocArray {
                key: Box::new(PhpType::Mixed),
                value: Box::new(PhpType::Mixed),
            });
        }
        if disagreed || saw_unshaped {
            return None;                                    // no shape to prefer over the declared hint
        }
        match specific {
            Some(ty) => Some(Self::generic_array_contract_payload(ty)),
            None => empty_array,
        }
    }

    /// Strips a CONTAINER payload from a resolved bare-`array` contract, keeping the storage kind.
    ///
    /// The contract exists to name the storage a caller will read -- packed slots or a hash -- and
    /// that is ALL a bare `array` hint can promise. A container payload promises more: it says the
    /// slots hold raw pointers, which only a writer of that same element type produces. Every
    /// other writer boxes, and the two cannot be told apart at the call site.
    ///
    /// Both halves of the disagreement were live. `twig/twig`'s `Template::getBlocks(): array`
    /// returns `$this->blocks`, whose base default is `[]` and whose subclasses assign a hash of
    /// arrays: the contract said `array<string, array<mixed>>` over a value the lowering had as
    /// `array<mixed>`, and the backend refused the conversion outright. Symfony's
    /// `BufferingLogger::cleanLogs(): array` is the silent half -- `$this->logs[] = [$level,
    /// $message, $context]` boxes each row into a `Mixed` slot, the contract said
    /// `array<array<mixed>>`, and `$log[1]` / `$log[2]` then read a boxed cell as a raw array:
    /// `php -S` prints the page, the compiled binary printed `Warning: Undefined array key 1` and
    /// a row of `count() == 4`.
    ///
    /// A SCALAR payload keeps its precision: an `int` or `string` slot has one representation, so
    /// the contract and the writer cannot drift apart, and widening it would cost every consumer
    /// an unbox.
    fn generic_array_contract_payload(ty: PhpType) -> PhpType {
        fn payload_is_container(ty: &PhpType) -> bool {
            // An OBJECT payload is deliberately absent: an object slot is a pointer whose class is
            // read from the object itself, so every writer stores the same representation and the
            // contract cannot drift from it. Widening it would also cost real behaviour --
            // Symfony's `ArgumentMetadataFactory::createArgumentMetadata(): array` returns
            // `ArgumentMetadata[]`, and `ArgumentResolver` reads `$metadata::IS_INSTANCEOF` off the
            // loop variable: as `mixed` that class constant has no static receiver, so it went to
            // the eval bridge as `dynamic::IS_INSTANCEOF` and every route with a controller
            // argument died on it.
            matches!(
                ty.codegen_repr(),
                PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable
            )
        }
        match ty {
            PhpType::Array(elem) if payload_is_container(&elem) => {
                PhpType::Array(Box::new(PhpType::Mixed))
            }
            PhpType::AssocArray { key, value } if payload_is_container(&value) => {
                PhpType::AssocArray {
                    key,
                    value: Box::new(PhpType::Mixed),
                }
            }
            other => other,
        }
    }

    /// Adds php's implicit `null` to the type inferred for an UNDECLARED return type whose body
    /// can run off its end after some `return <value>;`.
    ///
    /// `function f($a) { if ($a) { return new X; } }` answers `null` when the end is reached, but
    /// the returns alone inferred a non-null `X`: the default return then had no value of that
    /// type to produce (the backend refused the whole file -- Laravel's `getGenericUser()`,
    /// `TableGuesser::guess()`, `ProviderRepository::loadManifest()`), and a caller was free to
    /// fold `=== null` on a result that can be null. A body whose end is unreachable (every path
    /// returns or throws) keeps its exact type. `Mixed` already carries null.
    pub(crate) fn widen_return_for_fall_off(&self, body: &[Stmt], inferred: PhpType) -> PhpType {
        if matches!(inferred, PhpType::Void | PhpType::Never | PhpType::Mixed)
            || self.body_cannot_fall_through(body)
        {
            return inferred;
        }
        Self::wider_type(&inferred, &PhpType::Void)
    }

    /// Recursively collects ReturnInfo from all return statements in `stmt` and its
    /// nested blocks (if/while/try/etc.), appending each to `returns`. Untyped or unresolvable
    /// expressions are skipped silently — only well-typed returns contribute to the vector.
    pub(crate) fn collect_return_infos(
        &mut self,
        stmt: &Stmt,
        env: &TypeEnv,
        returns: &mut Vec<ReturnInfo>,
    ) {
        match &stmt.kind {
            StmtKind::Return(Some(expr)) => {
                // Prefer the type recorded while this exact statement was checked: it reflects
                // the environment at the return site, whereas `env` here is the body's final
                // environment and would leak a later narrowing backwards. Falls back to
                // re-inference when nothing was recorded (e.g. an unchecked body).
                let recorded = self
                    .flow_typed_returns
                    .get(&(
                        self.current_loop_storage_scope.clone(),
                        stmt as *const Stmt as usize,
                    ))
                    .filter(|(span, _)| *span == stmt.span)
                    .map(|(_, ty)| ty.clone());
                if let Some(ty) = recorded.or_else(|| self.infer_type(expr, env).ok()) {
                    returns.push(ReturnInfo {
                        ty,
                        has_value: true,
                        strict_types: stmt.strict_types,
                    });
                }
            }
            StmtKind::Return(None) => {
                returns.push(ReturnInfo {
                    ty: PhpType::Void,
                    has_value: false,
                    strict_types: stmt.strict_types,
                });
            }
            // A body folded to a plain block (`if (true) { return false; }` becomes one
            // before checking) still returns: skipping it dropped that return from the
            // fold, and the function's type became the other exit's alone.
            StmtKind::Synthetic(body) => {
                for s in body {
                    self.collect_return_infos(s, env, returns);
                }
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                for s in then_body {
                    self.collect_return_infos(s, env, returns);
                }
                for (_, body) in elseif_clauses {
                    for s in body {
                        self.collect_return_infos(s, env, returns);
                    }
                }
                if let Some(body) = else_body {
                    for s in body {
                        self.collect_return_infos(s, env, returns);
                    }
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => {
                for s in body {
                    self.collect_return_infos(s, env, returns);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                for s in try_body {
                    self.collect_return_infos(s, env, returns);
                }
                for catch_clause in catches {
                    for s in &catch_clause.body {
                        self.collect_return_infos(s, env, returns);
                    }
                }
                if let Some(body) = finally_body {
                    for s in body {
                        self.collect_return_infos(s, env, returns);
                    }
                }
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    for s in body {
                        self.collect_return_infos(s, env, returns);
                    }
                }
                if let Some(body) = default {
                    for s in body {
                        self.collect_return_infos(s, env, returns);
                    }
                }
            }
            _ => {}
        }
    }

    /// Collects return callable sigs for the surrounding analysis or metadata result.
    pub(crate) fn collect_return_callable_sigs(
        &mut self,
        stmt: &Stmt,
        env: &TypeEnv,
        returns: &mut Vec<FunctionSig>,
    ) {
        match &stmt.kind {
            StmtKind::Return(Some(expr)) => {
                if let Ok(Some(sig)) = self.resolve_expr_callable_sig(expr, env) {
                    returns.push(sig);
                }
            }
            // A body folded to a plain block (`if (true) { return false; }` becomes one
            // before checking) still returns: skipping it dropped that return from the
            // fold, and the function's type became the other exit's alone.
            StmtKind::Synthetic(body) => {
                for s in body {
                    self.collect_return_callable_sigs(s, env, returns);
                }
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                for s in then_body {
                    self.collect_return_callable_sigs(s, env, returns);
                }
                for (_, body) in elseif_clauses {
                    for s in body {
                        self.collect_return_callable_sigs(s, env, returns);
                    }
                }
                if let Some(body) = else_body {
                    for s in body {
                        self.collect_return_callable_sigs(s, env, returns);
                    }
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => {
                for s in body {
                    self.collect_return_callable_sigs(s, env, returns);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                for s in try_body {
                    self.collect_return_callable_sigs(s, env, returns);
                }
                for catch_clause in catches {
                    for s in &catch_clause.body {
                        self.collect_return_callable_sigs(s, env, returns);
                    }
                }
                if let Some(body) = finally_body {
                    for s in body {
                        self.collect_return_callable_sigs(s, env, returns);
                    }
                }
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    for s in body {
                        self.collect_return_callable_sigs(s, env, returns);
                    }
                }
                if let Some(body) = default {
                    for s in body {
                        self.collect_return_callable_sigs(s, env, returns);
                    }
                }
            }
            _ => {}
        }
    }

    /// Collects callable element signatures from array-returning statements.
    ///
    /// This records homogeneous `array<callable>` return metadata separately from
    /// direct callable returns so callers can propagate element signatures without
    /// treating the function call expression itself as a callable.
    pub(crate) fn collect_return_callable_array_sigs(
        &mut self,
        stmt: &Stmt,
        env: &TypeEnv,
        returns: &mut Vec<FunctionSig>,
    ) {
        match &stmt.kind {
            StmtKind::Return(Some(expr)) => {
                if let Ok(Some(sig)) = self.resolve_expr_callable_array_sig(expr, env) {
                    returns.push(sig);
                }
            }
            // A body folded to a plain block (`if (true) { return false; }` becomes one
            // before checking) still returns: skipping it dropped that return from the
            // fold, and the function's type became the other exit's alone.
            StmtKind::Synthetic(body) => {
                for s in body {
                    self.collect_return_callable_array_sigs(s, env, returns);
                }
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                for s in then_body {
                    self.collect_return_callable_array_sigs(s, env, returns);
                }
                for (_, body) in elseif_clauses {
                    for s in body {
                        self.collect_return_callable_array_sigs(s, env, returns);
                    }
                }
                if let Some(body) = else_body {
                    for s in body {
                        self.collect_return_callable_array_sigs(s, env, returns);
                    }
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => {
                for s in body {
                    self.collect_return_callable_array_sigs(s, env, returns);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                for s in try_body {
                    self.collect_return_callable_array_sigs(s, env, returns);
                }
                for catch_clause in catches {
                    for s in &catch_clause.body {
                        self.collect_return_callable_array_sigs(s, env, returns);
                    }
                }
                if let Some(body) = finally_body {
                    for s in body {
                        self.collect_return_callable_array_sigs(s, env, returns);
                    }
                }
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    for s in body {
                        self.collect_return_callable_array_sigs(s, env, returns);
                    }
                }
                if let Some(body) = default {
                    for s in body {
                        self.collect_return_callable_array_sigs(s, env, returns);
                    }
                }
            }
            _ => {}
        }
    }

    /// Returns true if `body` contains at least one Return statement at any nesting depth,
    /// including inside conditionals, loops, try/catch, switch, or synthetic blocks.
    pub(crate) fn body_contains_return(body: &[Stmt]) -> bool {
        body.iter().any(Self::stmt_contains_return)
    }

    /// Accepts a body that can fall off the end of a declared non-void return type.
    ///
    /// php compiles such a body and raises `TypeError: f(): Return value must be of type T, none
    /// returned` only when a call really falls off the end, so refusing it here rejected programs
    /// php runs: Symfony's `TagAwareAdapter::getItem(): CacheItem` returns from inside a `foreach`
    /// and has nothing after it. EIR lowering raises that `TypeError` on the fall-off path
    /// (`crate::ir_lower::function::terminate_open_block`); this stays the single place the
    /// three declaration kinds consult, so a future compile-time warning has one home.
    pub(crate) fn require_declared_return_coverage(
        &self,
        _declared_ret: &PhpType,
        _body: &[Stmt],
        _span: crate::span::Span,
        _context: &str,
    ) -> Result<(), CompileError> {
        Ok(())
    }

    /// Null-only declarations require an explicit value return despite sharing void storage.
    pub(crate) fn require_return_hint_coverage(
        &self, hint: &TypeExpr, expected: &PhpType, body: &[Stmt],
        span: crate::span::Span, context: &str,
    ) -> Result<(), CompileError> {
        let null_only = matches!(hint, TypeExpr::Named(name) if name.as_str().eq_ignore_ascii_case("null"));
        let coverage_type = if null_only { &PhpType::Mixed } else { expected };
        self.require_declared_return_coverage(coverage_type, body, span, context)
    }

    /// Keeps source null and void return rules separate while retaining their shared runtime value.
    pub(crate) fn require_compatible_return_hint(
        &self, hint: &TypeExpr, expected: &PhpType, actual: &PhpType, has_value: bool,
        strict_types: bool, span: crate::span::Span, context: &str,
    ) -> Result<(), CompileError> {
        if matches!(hint, TypeExpr::Named(name) if name.as_str().eq_ignore_ascii_case("null")) {
            if has_value && *actual == PhpType::Void { return Ok(()); }
            return Err(CompileError::new(span, &format!("{context} must return a null value")));
        }
        self.require_compatible_return_type(expected, actual, has_value, strict_types, span, context)
    }

    /// Checks that an actual return type is compatible with the declared return type.
    /// Handles three cases: void-returning functions (no value allowed), value-returning
    /// functions (value required and must be assignable to `expected`), and nullability
    /// via `declared_type_accepts_null`. In coercive source, a `Stringable` object may satisfy a
    /// string return boundary; `strict_types` disables that PHP conversion. Delegates to
    /// `require_compatible_arg_type` for the final assignability check.
    pub(crate) fn require_compatible_return_type(
        &self,
        expected: &PhpType,
        actual: &PhpType,
        has_value: bool,
        strict_types: bool,
        span: crate::span::Span,
        context: &str,
    ) -> Result<(), CompileError> {
        if !has_value {
            if matches!(expected, PhpType::Void) {
                return Ok(());
            }
            return Err(CompileError::new(
                span,
                &format!("{} must return a value of type {:?}", context, expected),
            ));
        }

        // A standalone `null` return type parses to the same `Void` as `void`, and its body says
        // `return null;`. PHP refuses that statement only under `void`, at compile time, so no
        // valid program reaches here with it: accepting it keeps `: null` methods compiled
        // (Symfony's `NullCoalescedNameNode::evaluate(): null`).
        if matches!(expected, PhpType::Void) && !matches!(actual, PhpType::Void) {
            return Err(CompileError::new(
                span,
                &format!("{} must not return a value", context),
            ));
        }

        if matches!(actual, PhpType::Void) && !Self::declared_type_accepts_null(expected) {
            return Err(CompileError::new(
                span,
                &format!("{} expects {}, got null", context, expected),
            ));
        }

        Self::require_matching_array_element_storage(expected, actual, span, context)?;

        if !self.type_accepts(expected, actual)
            && (crate::types::param_binding::object_requires_runtime_nominal_guard(
                expected, actual,
            ) || crate::types::param_binding::gradual_object_requires_runtime_nominal_guard(
                expected, actual,
            ))
        {
            return Ok(());
        }

        if !strict_types && param_accepts_weak_string_coercion(expected) {
            if let PhpType::Object(class_name) = actual.codegen_repr() {
                if self.object_supports_weak_string_coercion(&class_name) {
                    return Ok(());
                }
            }
        }

        // A string name or indexed receiver/method pair can satisfy PHP's `callable` return
        // contract. Lowering resolves the runtime value to a descriptor and raises at the
        // boundary when no callable target exists, so these values must reach that validation.
        if matches!(expected.codegen_repr(), PhpType::Callable)
            && matches!(actual.codegen_repr(), PhpType::Str | PhpType::Array(_))
        {
            return Ok(());
        }

        if !strict_types
            && crate::types::param_binding::scalar_param_cast(expected, actual).is_some()
        {
            return Ok(());
        }

        // Return lowering already normalizes a boxed gradual value into iterable storage.
        // Parameter binding has a distinct ABI path and deliberately remains stricter.
        if matches!((expected, actual), (PhpType::Iterable, PhpType::Mixed)) {
            return Ok(());
        }

        // `?int` returned from a declared `int` is PHP's runtime TypeError, not a compile error,
        // and `declared_int_return_boundary` already emits the check that raises it.
        if Self::nullable_int_reaches_declared_int_return(expected, actual) {
            return Ok(());
        }

        self.require_compatible_arg_type(expected, actual, span, context)
    }

    /// Rejects a declared `array<T>` return whose body produces an array with a DIFFERENT
    /// element representation.
    ///
    /// Assignability alone cannot catch this: `Mixed` is compatible with every type, so
    /// `require_compatible_arg_type` accepts an `array<mixed>` body for a declared
    /// `array<int>`. Their STORAGE is not interchangeable, though — an `array<int>` holds raw
    /// 64-bit values while an `array<mixed>` holds pointers to boxed cells, and the element
    /// layout lives in the array header tag rather than in the static type. Letting the
    /// declaration through unchecked makes every caller read the callee's boxed pointers as
    /// integers; measured, `function squares(int $n): array<int>` built with
    /// `$out = []; $out[] = $i * $i;` returned pointer values instead of squares.
    ///
    /// A declared bare `array` is `Array(Mixed)` and imposes no element contract, so it is
    /// exempt. Only a CONCRETE declared element type is enforced here.
    fn require_matching_array_element_storage(
        expected: &PhpType,
        actual: &PhpType,
        span: crate::span::Span,
        context: &str,
    ) -> Result<(), CompileError> {
        // A declared bare `array` is `Array(Mixed)` and imposes no element contract.
        if Self::is_generic_array_hint(expected) {
            return Ok(());
        }
        let (expected_elem, actual_elem) = match (expected, actual) {
            (PhpType::Array(expected_elem), PhpType::Array(actual_elem)) => {
                (expected_elem, actual_elem)
            }
            // Hash storage: only the VALUE is compared. The key is a runtime-tagged hash key
            // either way, so `array<string, int>` and `array<int, int>` share a layout, while
            // `array<string, int>` and `array<string, mixed>` do not.
            (
                PhpType::AssocArray {
                    value: expected_value,
                    ..
                },
                PhpType::AssocArray {
                    value: actual_value,
                    ..
                },
            ) => (expected_value, actual_value),
            // A packed element vector and a hash table are not interchangeable at all, so a
            // declared form that disagrees with the body's is rejected outright — unless the
            // body returns an empty literal, which carries no storage to disagree with.
            (PhpType::Array(_), PhpType::AssocArray { .. })
            | (PhpType::AssocArray { .. }, PhpType::Array(_))
                if !Self::returns_empty_array_literal_inner(actual) =>
            {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "{} declares {} but returns {}; the storage differs",
                        context, expected, actual
                    ),
                ));
            }
            _ => return Ok(()),
        };
        let expected_repr = expected_elem.codegen_repr();
        let actual_repr = actual_elem.codegen_repr();
        if expected_repr == actual_repr || matches!(**actual_elem, PhpType::Never) {
            return Ok(());
        }
        // A `null` element of a pointer array is the null pointer word, so `return [null]` from
        // `: array<ptr<T>|null>` shares the declared pointer storage.
        if matches!(expected_repr, PhpType::Pointer(_))
            && crate::types::merge_pointer_elements(expected_elem, actual_elem).is_some()
        {
            return Ok(());
        }
        Err(CompileError::new(
            span,
            &format!(
                "{} declares {} but returns {}; the element storage differs{}",
                context,
                expected,
                actual,
                Self::element_storage_hint(expected_elem, actual_elem)
            ),
        ))
    }

    /// Returns the way out of an element-storage mismatch, when there is a known one.
    ///
    /// `int|float` elements are almost always a loop counter: PHP promotes an integer at the
    /// overflow boundary, so `for ($i = 0; …) { $out[] = $i; }` builds a list of numbers that
    /// are each an int OR a float, and that is boxed storage rather than a packed `int` vector.
    /// A cast at the append pins the element type and costs nothing at runtime for a value that
    /// is already an int, which is the fix a reader would otherwise have to guess.
    fn element_storage_hint(expected_elem: &PhpType, actual_elem: &PhpType) -> String {
        if actual_elem.is_int_float_union() && matches!(expected_elem, PhpType::Int) {
            return ". A loop counter is `int|float` after `++`, because PHP promotes it at the \
                    overflow boundary; write `(int) $counter` at the append to pin the element \
                    type"
                .to_string();
        }
        String::new()
    }

    /// Returns true when `ty` is the element type an EMPTY array literal infers.
    ///
    /// `return [];` carries no element storage, so it satisfies any declared container form,
    /// indexed or associative. The checker types it `Array(Never)`.
    fn returns_empty_array_literal_inner(ty: &PhpType) -> bool {
        matches!(ty, PhpType::Array(elem) if matches!(**elem, PhpType::Never))
    }

    /// Reports whether this is the nullable-int-into-declared-`int` return the backend verifies.
    ///
    /// Deliberately narrow: `ReturnBoundaryMixedToInt` is the only return boundary that TESTS the
    /// value and raises PHP's message. A declared `float` or `string` return converts instead, so
    /// admitting the same shape there would answer `0.0` or `""` where PHP throws.
    fn nullable_int_reaches_declared_int_return(expected: &PhpType, actual: &PhpType) -> bool {
        if !matches!(expected, PhpType::Int) {
            return false;
        }
        let PhpType::Union(members) = actual else {
            return false;
        };
        members.iter().any(|member| matches!(member, PhpType::Void))
            && members
                .iter()
                .all(|member| matches!(member, PhpType::Int | PhpType::Void))
    }

    /// Returns true if `ty` can accept a null/void value — covers PhpType::Mixed,
    /// PhpType::Void, and PhpType::Union types where any member accepts null.
    ///
    /// Shared with the by-reference PARAMETER position
    /// (`Checker::require_by_ref_argument_storage`), which asks the identical question of a
    /// declared type: PHP's `?int`/`mixed` accept null in both positions and a bare `int`
    /// accepts it in neither. Kept as one predicate rather than two so the two sites cannot
    /// drift on a type either of them later learns about.
    pub(crate) fn declared_type_accepts_null(ty: &PhpType) -> bool {
        match ty {
            PhpType::Mixed => true,
            PhpType::Union(members) => members.iter().any(Self::declared_type_accepts_null),
            PhpType::Void => true,
            _ => false,
        }
    }

    /// Returns true if `stmt` or any nested statement within it contains a Return.
    /// Recurses through If, While, DoWhile, For, Foreach, Try, Switch, Synthetic,
    /// NamespaceBlock, and IfDef. Used by `body_contains_return` for control-flow analysis.
    fn stmt_contains_return(stmt: &Stmt) -> bool {
        match &stmt.kind {
            StmtKind::Return(_) => true,
            StmtKind::Synthetic(stmts) | StmtKind::NamespaceBlock { body: stmts, .. } => {
                Self::body_contains_return(stmts)
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                Self::body_contains_return(then_body)
                    || elseif_clauses
                        .iter()
                        .any(|(_, body)| Self::body_contains_return(body))
                    || else_body
                        .as_ref()
                        .is_some_and(|body| Self::body_contains_return(body))
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::Foreach { body, .. } => Self::body_contains_return(body),
            StmtKind::For {
                init, update, body, ..
            } => {
                init.as_ref()
                    .is_some_and(|stmt| Self::stmt_contains_return(stmt))
                    || update
                        .as_ref()
                        .is_some_and(|stmt| Self::stmt_contains_return(stmt))
                    || Self::body_contains_return(body)
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                Self::body_contains_return(try_body)
                    || catches
                        .iter()
                        .any(|catch_clause| Self::body_contains_return(&catch_clause.body))
                    || finally_body
                        .as_ref()
                        .is_some_and(|body| Self::body_contains_return(body))
            }
            StmtKind::Switch { cases, default, .. } => {
                cases
                    .iter()
                    .any(|(_, body)| Self::body_contains_return(body))
                    || default
                        .as_ref()
                        .is_some_and(|body| Self::body_contains_return(body))
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                Self::body_contains_return(then_body)
                    || else_body
                        .as_ref()
                        .is_some_and(|body| Self::body_contains_return(body))
            }
            _ => false,
        }
    }

    /// Computes the wider of two PHP types for return-type merging:
    /// - If equal, returns a clone.
    /// - `Never` is absorbed by the other type (it materializes no value).
    /// - `Void` (elephc's spelling of PHP `null`) makes the other type NULLABLE.
    /// - Str + anything → Str; Float + anything → Float; otherwise → Mixed.
    ///
    /// The `Void` arm must be tested BEFORE `Str`/`Float`, and `Never` before `Void`.
    /// `Void` used to resolve to the other type outright, which silently deleted the null
    /// arm of a hint-less union return: `function f($x) { if ($x) { return "s"; } return null; }`
    /// inferred `Str`, so `return null` was lowered as `i_to_str(const_null)` and the caller
    /// saw `""` instead of `NULL`. Writing `: ?string` was already correct, and the ternary
    /// spelling of the same function already inferred `string|null` through
    /// `nullable_match_arm_type` — this makes the multi-`return` fold agree with both.
    /// `False` and `Bool` are subject to the SAME rule as `Void`, for the same reason, and
    /// their arms must likewise precede `Str`/`Float` (issue #398).
    ///
    /// `false` is a VALUE, not a width. A function that returns `false` on failure and a
    /// string otherwise returns `string|false`, and every PHP caller tests that with
    /// `=== false`. Letting `Str` absorb it below inferred plain `Str`, so the failure arm
    /// was lowered as a string and the caller saw `""` — `image_type_to_extension()` on an
    /// unknown type returned `""` instead of `false`, and so did any hint-less user function
    /// of that shape. `Float` did the same with `0.0`, and `Bool` was absorbed into `"1"`.
    ///
    /// `int|false` never had the bug only because it fell through to `Mixed`, which carries
    /// the value distinctly — the accident that made the defect look image-specific.
    ///
    /// Writing `: string|false` was already correct; this makes the hint-less fold agree
    /// with the declared spelling, exactly as the `Void` arm above does for `?string`.
    pub(crate) fn wider_type(a: &PhpType, b: &PhpType) -> PhpType {
        match (a, b) {
            _ if a == b => a.clone(),
            (PhpType::Never, other) | (other, PhpType::Never) => other.clone(),
            (PhpType::Void, other) | (other, PhpType::Void) => nullable_return_type(other),
            // `false` is the literal subtype of `bool`, so together they are just `bool`.
            (PhpType::Bool, PhpType::False) | (PhpType::False, PhpType::Bool) => PhpType::Bool,
            (PhpType::False, other @ (PhpType::Str | PhpType::Float))
            | (other @ (PhpType::Str | PhpType::Float), PhpType::False) => {
                union_return_type(other, PhpType::False)
            }
            (PhpType::Bool, other @ (PhpType::Str | PhpType::Float))
            | (other @ (PhpType::Str | PhpType::Float), PhpType::Bool) => {
                union_return_type(other, PhpType::Bool)
            }
            // A union already carries its members distinctly, so `Str`/`Float` below must
            // not absorb one back into a single scalar. Without this the fold only survived
            // TWO returns: a third collapsed the union the first two had correctly built.
            // That applied to the `Void` arm above as well — `null`, `"one"`, `"two"`
            // inferred plain `Str`, and the null arm came back as `""`.
            (PhpType::Mixed, _) | (_, PhpType::Mixed) => PhpType::Mixed,
            (PhpType::Union(_), _) | (_, PhpType::Union(_)) => {
                normalize_union_members(vec![a.clone(), b.clone()])
            }
            // `string`/`float` absorb only the SCALARS they can stand for. An array, object,
            // callable, `mixed` or union is not a string: `if (is_array($x)) { return [1]; }
            // return "x";` inferred `string` and returned "Array" (with a conversion warning) for
            // every array -- Twig's `first`/`last` filters over `slice()` among them.
            (PhpType::Str | PhpType::Float, other) | (other, PhpType::Str | PhpType::Float)
                if !scalar_absorbed_by_str_or_float(other) =>
            {
                PhpType::Mixed
            }
            (PhpType::Str, _) | (_, PhpType::Str) => PhpType::Str,
            (PhpType::Float, _) | (_, PhpType::Float) => PhpType::Float,
            _ => PhpType::Mixed,
        }
    }

    /// Unions two inferred parameter types for call-site specialization: identical
    /// types stay, `Void`/`Never` are absorbed by the other type, and any genuine
    /// disagreement widens to `Mixed`. Unlike `wider_type` (which lets `Str`/`Float`
    /// absorb other scalars for coercion), this preserves the distinction between
    /// scalar tags, so a parameter called with both an int and a string is `Mixed`
    /// (boxed) rather than collapsed to one type and mis-tagged at runtime.
    pub(crate) fn union_param_type(a: &PhpType, b: &PhpType) -> PhpType {
        match (a, b) {
            _ if a == b => a.clone(),
            // Under the tagged null representation a null call-site argument makes the
            // parameter genuinely nullable: widen scalar params to int|null instead of
            // riding null in as the in-band sentinel of a plain Int.
            (PhpType::Void, PhpType::Int) | (PhpType::Int, PhpType::Void)
                if crate::codegen::sentinels::null_repr_is_tagged() =>
            {
                PhpType::Union(vec![PhpType::Int, PhpType::Void])
            }
            // A `callable` slot has no null representation a call can pass through: the argument
            // is normalized to a descriptor, and null fatals ("mixed value is not callable").
            // Laravel's `Arr::sort($array, $callback = null)` forwards its null to
            // `Collection::sortBy($callback)`, whose other callers pass closures.
            (PhpType::Void, PhpType::Callable) | (PhpType::Callable, PhpType::Void) => {
                PhpType::Union(vec![PhpType::Callable, PhpType::Void])
            }
            (PhpType::Void | PhpType::Never, other) | (other, PhpType::Void | PhpType::Never) => {
                other.clone()
            }
            // An object parameter called with various concrete subtypes keeps its
            // object type (e.g. a `Throwable` param invoked with a concrete
            // exception) instead of widening to `Mixed`, which would break
            // object-typed dispatch (`Fiber::throw` / `Generator::throw`).
            (PhpType::Object(_), PhpType::Object(_)) => a.clone(),
            (PhpType::Array(left), PhpType::Array(right)) => {
                PhpType::Array(Box::new(Self::union_array_payload_type(left, right)))
            }
            (
                PhpType::AssocArray {
                    key: left_key,
                    value: left_value,
                },
                PhpType::AssocArray {
                    key: right_key,
                    value: right_value,
                },
            ) => PhpType::AssocArray {
                key: Box::new(Self::union_array_payload_type(left_key, right_key)),
                value: Box::new(Self::union_array_payload_type(left_value, right_value)),
            },
            _ => PhpType::Mixed,
        }
    }

    /// Joins one indexed/associative array payload position without discarding container shape.
    fn union_array_payload_type(left: &PhpType, right: &PhpType) -> PhpType {
        if left == right || left.codegen_repr() == right.codegen_repr() {
            return left.clone();
        }
        match (left, right) {
            (PhpType::Never, other) | (other, PhpType::Never) => other.clone(),
            // Pointers and `null` share the pointer word, so `[null]` passed to an
            // `array<?ptr<T>>` keeps pointer storage instead of boxing every element.
            _ => crate::types::merge_pointer_elements(left, right).unwrap_or(PhpType::Mixed),
        }
    }
}
