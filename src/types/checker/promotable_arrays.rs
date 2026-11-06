//! Purpose:
//! Syntactic per-body pre-scan for the locals whose typed packed storage may be promoted to a
//! hash at run time instead of being widened to boxed `mixed` slots.
//!
//! Called from:
//! - `crate::types::checker::Checker::with_local_storage_context` (function/method/closure bodies)
//! - `crate::types::checker::Checker::check_top_level_program` (the top-level body)
//!
//! Key details:
//! - A candidate is a local the body writes by element (`$a[$k] = v`, `$a[] = v`) and that no
//!   construct can reach other than through the element operations codegen dispatches on the
//!   heap kind, or through a whole-value read lowered into the kind-aware `array<mixed>` view.
//! - Disqualified: references to the local (`=&`, `use (&$a)`, by-reference `foreach`), `unset`
//!   of it or of an element, `global` / `static` / `list()` / `foreach` / `catch` bindings, nested
//!   element writes, element writes used as expressions, increments, closure captures, and any bare
//!   argument to a call that may bind it by reference: a method, closure or dynamic call, a
//!   builtin outside the read-only list, or a user function whose declaration takes that position
//!   by reference (or is unknown). `eval()`, `extract()`, `compact()`,
//!   `get_defined_vars()` and `parse_str()` disqualify every local of the body.
//! - Syntactic apart from the callee declarations, which are fixed before any body is checked, so
//!   every repeated checker walk of a body gets the same answer.

use std::collections::HashSet;

use crate::parser::ast::{BinOp, CastType, Expr, ExprKind, Stmt, StmtKind};
use crate::span::Span;
use crate::types::PhpType;

use super::Checker;

/// Builtins that only read a bare array argument, so passing a candidate to them is a read.
const BY_VALUE_ARRAY_BUILTINS: &[&str] = &[
    "count",
    "sizeof",
    "implode",
    "join",
    "json_encode",
    "print_r",
    "var_dump",
    "var_export",
    "in_array",
    "array_search",
    "array_sum",
    "array_product",
    "max",
    "min",
    "array_keys",
    "array_values",
    "array_slice",
    "is_array",
    "array_key_exists",
    "array_reverse",
    "array_unique",
    "array_merge",
    "array_flip",
    "array_key_first",
    "array_key_last",
];

/// Builtins that can read or rebind arbitrary locals of the calling scope.
const SCOPE_BUILTINS: &[&str] = &["eval", "extract", "compact", "get_defined_vars", "parse_str"];

/// Returns whether an element type can stay raw in a run-time promoted hash.
///
/// Limited to `int`: its reads already produce the nullable integer representation, so a key
/// that misses still reads as PHP's `null`, and its slots are one word wide.
pub(crate) fn promotable_element(elem_ty: &PhpType) -> bool {
    elem_ty.codegen_repr() == PhpType::Int
}

/// Returns whether a boxed or counter key type reads a promotable local as its element type.
fn promotable_read_key(idx_ty: &PhpType) -> bool {
    matches!(idx_ty, PhpType::Mixed) || idx_ty.is_int_float_union()
}

impl Checker {
    /// Returns whether `$array[index] = value` keeps the candidate local's `array<int>` storage
    /// and lets codegen promote it to a hash only if the key skips a slot at run time.
    pub(crate) fn promotable_array_write(
        &self,
        array: &str,
        index: &Expr,
        elem_ty: &PhpType,
        idx_ty: &PhpType,
        normalized_idx_ty: &PhpType,
        index_is_foreach_key: bool,
        val_ty: &PhpType,
    ) -> bool {
        if !self.promotable_array_candidates.contains(array)
            || self.ref_aliased_locals.contains(array)
            || !promotable_element(elem_ty)
            || index_is_foreach_key
        {
            return false;
        }
        // An integer key the checker bounds needs no promotion, and a string key promotes the
        // static type as before; only an unbounded integer, boxed or counter key qualifies.
        let key_qualifies = (matches!(normalized_idx_ty, PhpType::Int)
            && !matches!(idx_ty, PhpType::Union(_))
            && !crate::types::nonempty_array_key_stays_in_range(index))
            || promotable_read_key(idx_ty);
        // A `?int` value keeps the slots typed too: codegen stores an integer in its slot and
        // PHP's `null` as a null entry of the promoted hash.
        let nullable_int = val_ty.codegen_repr() == PhpType::TaggedScalar;
        key_qualifies
            && (elem_ty == val_ty
                || nullable_int
                || self
                    .merge_array_element_type(elem_ty, val_ty)
                    .is_some_and(|merged| &merged == elem_ty))
    }

    /// Records a promotable local of the current function-like scope.
    pub(crate) fn record_promotable_array_local(&mut self, array: &str) {
        self.promotable_arrays
            .locals
            .insert((self.current_loop_storage_scope.clone(), array.to_string()));
    }

    /// Returns whether `name` is a promotable local of the current function-like scope.
    pub(crate) fn is_promotable_array_local(&self, name: &str) -> bool {
        self.promotable_arrays
            .locals
            .contains(&(self.current_loop_storage_scope.clone(), name.to_string()))
    }

    /// Returns whether an element read of the candidate `array` reads as `?int` under a boxed or
    /// counter key, which would otherwise read as `mixed`.
    ///
    /// Only a local that can really hold a promotable array takes that view: one the body writes
    /// under a key that may skip a slot (`PromotableCandidates::keyed`), or one already recorded
    /// promotable (an activated parameter, a promotable value
    /// assigned to it). A packed local nothing writes keeps the `mixed` read it has without this
    /// pass, so a buffer index, an `int` local it is reassigned into, and every other consumer
    /// that accepts `mixed` but not `?int` keeps compiling.
    pub(crate) fn promotable_array_element_read(&self, array: &str, elem_ty: &PhpType, idx_ty: &PhpType) -> bool {
        (self.promotable_array_candidates.keyed(array) || self.is_promotable_array_local(array))
            && promotable_element(elem_ty)
            && promotable_read_key(idx_ty)
    }

    /// Returns the type a whole-value read of `name` has: a promotable `array<int>` local may be
    /// hash storage at run time, so outside its element ops it reads as the boxed `array<mixed>`
    /// every consumer accepts in either storage kind, and the read is recorded for lowering. A
    /// read a promotable sink receives raw keeps the typed view.
    pub(crate) fn promotable_array_value_view(&mut self, name: &str, span: Span, ty: PhpType) -> PhpType {
        let promotable = self.is_promotable_array_local(name)
            && matches!(&ty, PhpType::Array(elem) if promotable_element(elem));
        if !promotable || !span.identifies_a_node() || self.promotable_raw_sites.contains(&span) {
            return ty;
        }
        self.record_promotable_value_read(name, span);
        PhpType::Array(Box::new(PhpType::Mixed))
    }

    /// Returns whether `expr` evaluates to a value that may be a promotable `array<int>` hash:
    /// a promotable local, or a call to a function returning one.
    pub(crate) fn is_promotable_value_expr(&self, expr: &Expr, env: &crate::types::TypeEnv) -> bool {
        match &expr.kind {
            ExprKind::Variable(name) => {
                self.is_promotable_array_local(name)
                    && matches!(env.get(name), Some(PhpType::Array(elem)) if promotable_element(elem))
            }
            ExprKind::FunctionCall { name, .. } => {
                let callee = self
                    .canonical_function_name_folded(name.as_str())
                    .unwrap_or_else(|| name.as_str().to_string());
                self.promotable_returning.contains(&callee)
            }
            _ => false,
        }
    }

    /// Returns whether `expr` may evaluate to a promotable `array<int>` once it is checked: a
    /// promotable local, or a call to a function declared to return `array<int>` (whether it
    /// really returns a promotable value is known only after that call is checked; handing a
    /// packed result on raw is always correct).
    pub(crate) fn may_be_promotable_value_expr(&self, expr: &Expr, env: &crate::types::TypeEnv) -> bool {
        match &expr.kind {
            ExprKind::FunctionCall { name, .. } => {
                let callee = self
                    .canonical_function_name_folded(name.as_str())
                    .unwrap_or_else(|| name.as_str().to_string());
                self.fn_decls
                    .get(&callee)
                    .and_then(|decl| decl.return_type.clone().map(|ty| (ty, decl.span)))
                    .is_some_and(|(ty, span)| self.resolves_to_promotable_array(&ty, span))
            }
            _ => self.is_promotable_value_expr(expr, env),
        }
    }

    /// Returns the parameter of user function `callee` at `position` that can receive a
    /// promotable `array<int>` raw: declared `array<int>`, by value, and a candidate in the body.
    pub(crate) fn promotable_callee_param(&mut self, callee: &str, position: usize) -> Option<String> {
        let decl = self.fn_decls.get(callee)?.clone();
        if !decl.type_params.is_empty() || decl.ref_params.get(position).copied().unwrap_or(true) {
            return None;
        }
        let param = decl.params.get(position)?.clone();
        let declared = decl.param_types.get(position)?.as_ref()?;
        if !self.resolves_to_promotable_array(declared, decl.span) {
            return None;
        }
        let candidates = self.callee_promotable_candidates(callee)?;
        candidates.contains(&param).then_some(param)
    }

    /// Returns whether a declared type is exactly `array<int>`.
    pub(crate) fn resolves_to_promotable_array(
        &self,
        declared: &crate::parser::ast::TypeExpr,
        span: Span,
    ) -> bool {
        self.resolve_type_expr(declared, span)
            .is_ok_and(|ty| matches!(&ty, PhpType::Array(elem) if promotable_element(elem)))
    }

    /// Hands promotable arguments of a call to `callee` on raw where its parameter can take them,
    /// and activates those parameters, re-resolving an already checked callee.
    ///
    /// A promotable local passed to an undeclared parameter redirects the call to a typed clone of
    /// `callee` (`generics::promotable_clones`), whose name is returned; the arguments are then
    /// handed to the clone's `array<int>` parameters exactly as to declared ones.
    pub(crate) fn prepare_promotable_call_args(
        &mut self,
        callee: &str,
        args: &[Expr],
        env: &crate::types::TypeEnv,
        call_span: Span,
    ) -> Result<Option<String>, crate::errors::CompileError> {
        self.flush_promotable_reresolutions()?;
        let clone = self.promotable_clone_for_call(callee, args, env, call_span);
        let callee = clone.as_deref().unwrap_or(callee);
        let mut activated = false;
        for (position, arg) in args.iter().enumerate() {
            if matches!(arg.kind, ExprKind::NamedArg { .. } | ExprKind::Spread(_)) {
                break;
            }
            if !self.may_be_promotable_value_expr(arg, env) {
                continue;
            }
            let Some(param) = self.promotable_callee_param(callee, position) else {
                continue;
            };
            self.insert_promotable_raw_site(arg.span);
            if self.is_promotable_value_expr(arg, env) {
                activated |= self
                    .promotable_params
                    .insert((callee.to_string(), param));
            } else {
                // A call argument is known to return a promotable value only once it is checked.
                self.pending_promotable_call_args
                    .push((callee.to_string(), param, arg.clone()));
            }
        }
        if activated && self.functions.contains_key(callee) {
            self.reresolve_for_promotion(callee)?;
        }
        Ok(clone)
    }

    /// Returns the typed clone a direct call to `callee` is redirected to, creating it on first
    /// use: the undeclared by-value parameters that receive a promotable local are declared
    /// `array<int>` in the clone. `None` keeps the call on the original.
    ///
    /// Only an ordinary top-level function qualifies (no template, include variant, attribute or
    /// by-reference return), and only parameters the body never reassigns as a whole, in a body
    /// with no `static` local, nested declaration or `include` a second copy would duplicate.
    fn promotable_clone_for_call(
        &mut self,
        callee: &str,
        args: &[Expr],
        env: &crate::types::TypeEnv,
        call_span: Span,
    ) -> Option<String> {
        if !call_span.identifies_a_node()
            || args
                .iter()
                .any(|arg| matches!(arg.kind, ExprKind::NamedArg { .. } | ExprKind::Spread(_)))
        {
            return None;
        }
        let decl = self.fn_decls.get(callee)?;
        let in_variant_group = self.function_variant_groups.contains_key(callee)
            || self
                .function_variant_groups
                .values()
                .any(|variants| variants.iter().any(|variant| variant == callee));
        if !decl.type_params.is_empty()
            || decl.by_ref_return
            || !decl.attributes.is_empty()
            || callee.contains('<')
            || crate::generics::promotable_clones::promotable_clone_source_name(callee).is_some()
            || in_variant_group
            || crate::types::checker::yield_validation::body_contains_yield(&decl.body)
        {
            return None;
        }
        let candidates = self.callee_promotable_candidates(callee)?;
        let decl = self.fn_decls.get(callee)?;
        let positions: Vec<usize> = args
            .iter()
            .enumerate()
            .filter(|(position, arg)| {
                let position = *position;
                position < decl.params.len()
                    && decl.param_types.get(position).is_some_and(Option::is_none)
                    && !decl.ref_params.get(position).copied().unwrap_or(true)
                    && candidates.clonable_param(&decl.params[position])
                    && self.is_promotable_value_expr(arg, env)
            })
            .map(|(position, _)| position)
            .collect();
        if positions.is_empty() {
            return None;
        }
        let clone = crate::generics::promotable_clones::promotable_clone_name(callee, &positions);
        if !self.fn_decls.contains_key(&clone) {
            let decl = self.fn_decls.get(callee)?.clone();
            let source_id = crate::generics::promotable_clones::fresh_clone_source_id(decl.span);
            let clone_decl = promotable_clone_decl(decl, &positions, source_id);
            self.publish_function_declaration(clone.clone(), clone_decl);
            self.requested_promotable_clones.push(
                crate::generics::promotable_clones::PromotableCloneRequest {
                    base: callee.to_string(),
                    clone: clone.clone(),
                    positions,
                    source_id,
                },
            );
        } else if !self
            .requested_promotable_clones
            .iter()
            .any(|request| request.clone == clone)
        {
            // Spliced by an earlier round: listed again so the result names it as a root.
            self.requested_promotable_clones.push(
                crate::generics::promotable_clones::PromotableCloneRequest {
                    base: callee.to_string(),
                    clone: clone.clone(),
                    positions,
                    source_id: 0,
                },
            );
        }
        self.generic_call_sites
            .insert((self.current_loop_storage_scope.clone(), call_span), clone.clone());
        Some(clone)
    }

    /// Returns the cached candidate scan of user function `callee`'s body.
    fn callee_promotable_candidates(&mut self, callee: &str) -> Option<PromotableCandidates> {
        if let Some(candidates) = self.promotable_callee_candidates.get(callee) {
            return Some(candidates.clone());
        }
        let body = self.fn_decls.get(callee)?.body.clone();
        let candidates = find_candidates(self, &body);
        self.promotable_callee_candidates
            .insert(callee.to_string(), candidates.clone());
        Some(candidates)
    }

    /// Activates the parameters whose argument calls turned out to return a promotable value.
    pub(crate) fn confirm_promotable_call_args(
        &mut self,
        env: &crate::types::TypeEnv,
    ) -> Result<(), crate::errors::CompileError> {
        let pending = std::mem::take(&mut self.pending_promotable_call_args);
        for (callee, param, arg) in pending {
            if self.is_promotable_value_expr(&arg, env)
                && self.promotable_params.insert((callee.clone(), param))
                && self.functions.contains_key(&callee)
            {
                self.reresolve_for_promotion(&callee)?;
            }
        }
        Ok(())
    }

    /// Re-checks `callee` now, or once its current resolution ends.
    fn reresolve_for_promotion(&mut self, callee: &str) -> Result<(), crate::errors::CompileError> {
        if self.resolving_functions.contains(callee) {
            if !self.pending_promotable_reresolutions.iter().any(|name| name == callee) {
                self.pending_promotable_reresolutions.push(callee.to_string());
            }
            return Ok(());
        }
        let Some(decl) = self.fn_decls.get(callee).cloned() else {
            return Ok(());
        };
        let Some(params) = self.functions.get(callee).map(|sig| sig.params.clone()) else {
            return Ok(());
        };
        self.resolve_function_signature(callee, &decl, params).map(|_| ())
    }

    /// Re-resolves every pending function whose resolution has ended.
    pub(crate) fn flush_promotable_reresolutions(&mut self) -> Result<(), crate::errors::CompileError> {
        let ready: Vec<String> = self
            .pending_promotable_reresolutions
            .iter()
            .filter(|name| !self.resolving_functions.contains(*name))
            .cloned()
            .collect();
        if ready.is_empty() {
            return Ok(());
        }
        self.pending_promotable_reresolutions
            .retain(|name| self.resolving_functions.contains(name));
        for name in ready {
            self.reresolve_for_promotion(&name)?;
        }
        Ok(())
    }

    /// Marks the parameters a caller activated as promotable from the body's entry.
    pub(crate) fn activate_promotable_params(
        &mut self,
        params: &std::collections::HashMap<String, PhpType>,
    ) {
        let Some(function) = self.current_function.clone() else {
            return;
        };
        let activated: Vec<String> = params
            .iter()
            .filter(|(param, ty)| {
                self.promotable_array_candidates.contains(param)
                    && self.promotable_params.contains(&(function.clone(), (*param).clone()))
                    && matches!(ty, PhpType::Array(elem) if promotable_element(elem))
            })
            .map(|(param, _)| param.clone())
            .collect();
        for param in activated {
            self.record_promotable_array_local(&param);
        }
    }

    /// Returns the current function when it is declared to return `array<int>`.
    fn current_function_returning_promotable_type(&self) -> Option<String> {
        let function = self.current_function.clone()?;
        let (declared, span) = self
            .fn_decls
            .get(&function)
            .and_then(|decl| decl.return_type.clone().map(|ty| (ty, decl.span)))?;
        self.resolves_to_promotable_array(&declared, span).then_some(function)
    }

    /// Hands a maybe-promotable `return` on raw when the function is declared to return
    /// `array<int>`; `confirm_promotable_return` records it once the value is checked.
    pub(crate) fn prepare_promotable_return(&mut self, expr: &Expr, env: &crate::types::TypeEnv) {
        if self.current_function_returning_promotable_type().is_some()
            && self.may_be_promotable_value_expr(expr, env)
        {
            self.insert_promotable_raw_site(expr.span);
        }
    }

    /// Records the current function as returning a promotable value when this `return` does.
    pub(crate) fn confirm_promotable_return(&mut self, expr: &Expr, env: &crate::types::TypeEnv) {
        let Some(function) = self.current_function_returning_promotable_type() else {
            return;
        };
        if !self.is_promotable_value_expr(expr, env) {
            return;
        }
        if self.promotable_returning.insert(function) {
            // Every body on the resolution stack may already have used this function's result
            // as a plain packed array (recursion), so each is checked again once it ends.
            let resolving: Vec<String> = self.resolving_functions.iter().cloned().collect();
            for name in resolving {
                if !self.pending_promotable_reresolutions.contains(&name) {
                    self.pending_promotable_reresolutions.push(name);
                }
            }
        }
    }

    /// Hands a promotable value assigned to a candidate local on raw, returning whether the local
    /// becomes promotable once the assignment is checked.
    pub(crate) fn prepare_promotable_assignment(
        &mut self,
        target: &str,
        value: &Expr,
        env: &crate::types::TypeEnv,
    ) -> bool {
        if !self.promotable_array_candidates.contains(target)
            || self.ref_aliased_locals.contains(target)
            || !self.may_be_promotable_value_expr(value, env)
        {
            return false;
        }
        self.insert_promotable_raw_site(value.span);
        true
    }

    /// Returns the type a call to `callee` has where it is used: a function returning a
    /// promotable `array<int>` reads as the boxed `array<mixed>` view unless a sink takes it raw.
    pub(crate) fn promotable_call_result_view(&mut self, callee: &str, span: Span, ty: PhpType) -> PhpType {
        if !self.promotable_returning.contains(callee)
            || !matches!(&ty, PhpType::Array(elem) if promotable_element(elem))
            || self.promotable_raw_sites.contains(&span)
            || !span.identifies_a_node()
        {
            return ty;
        }
        self.promotable_arrays
            .call_views
            .insert((self.current_loop_storage_scope.clone(), span));
        PhpType::Array(Box::new(PhpType::Mixed))
    }

    /// Records a span a promotable sink takes raw, for this checker and for lowering.
    fn insert_promotable_raw_site(&mut self, span: Span) {
        self.promotable_raw_sites.insert(span);
        self.promotable_arrays
            .raw_sites
            .insert((self.current_loop_storage_scope.clone(), span));
    }

    /// Records a whole-value read of a promotable local, which lowering views as `array<mixed>`.
    pub(crate) fn record_promotable_value_read(&mut self, name: &str, span: Span) {
        self.promotable_arrays.value_reads.insert((
            self.current_loop_storage_scope.clone(),
            span,
            name.to_string(),
        ));
    }
}

/// Builds the checker's copy of a typed clone, matching what
/// `generics::promotable_clones::splice_promotable_clones` splices from the same request.
fn promotable_clone_decl(mut decl: super::FnDecl, positions: &[usize], source_id: u32) -> super::FnDecl {
    use crate::generics::promotable_clones::{promotable_param_type, retag_body, retag_expr, retag_span};
    for position in positions {
        if let Some(declared) = decl.param_types.get_mut(*position) {
            *declared = Some(promotable_param_type());
        }
    }
    decl.defaults = decl
        .defaults
        .into_iter()
        .map(|default| default.map(|value| retag_expr(value, source_id)))
        .collect();
    decl.body = retag_body(decl.body, source_id);
    decl.span = retag_span(decl.span, source_id);
    decl
}

/// The locals of one body that may hold a promotable `array<int>`: every local no construct can
/// reach outside the heap-kind-dispatching element ops and the whole-value view.
///
/// A local qualifies whether it is written by element, receives a promotable value (a parameter,
/// `$b = f()`), or both: only the checker's decisions make one of them promotable.
#[derive(Clone, Debug, Default)]
pub(crate) struct PromotableCandidates {
    /// Whether a scan installed this set (an unscanned body has no candidates).
    active: bool,
    /// Set when a builtin can read or rebind every local of the body.
    scope_escape: bool,
    /// Locals some construct can reach outside the dispatched element operations.
    disqualified: HashSet<String>,
    /// Locals the body assigns as a whole.
    assigned: HashSet<String>,
    /// Locals the body writes under a key that may skip a slot (`Scan::keyed`).
    keyed: HashSet<String>,
    /// Whether the body holds a construct a typed clone would duplicate.
    clone_blocked: bool,
}

impl PromotableCandidates {
    /// Returns whether `name` may hold a promotable array in this body.
    pub(crate) fn contains(&self, name: &str) -> bool {
        self.active && !self.scope_escape && !self.disqualified.contains(name)
    }

    /// Returns whether candidate `name` has an element write in this body that can make it
    /// promotable. The set is syntactic, so a read checked before that write gets the same answer.
    pub(crate) fn keyed(&self, name: &str) -> bool {
        self.contains(name) && self.keyed.contains(name)
    }

    /// Returns whether a typed clone of this body may declare parameter `name` as `array<int>`:
    /// a candidate the body never reassigns as a whole, in a body a clone can duplicate.
    fn clonable_param(&self, name: &str) -> bool {
        self.contains(name) && !self.clone_blocked && !self.assigned.contains(name)
    }

    /// Disqualifies more locals, such as those a reference reaches.
    pub(crate) fn disqualify<'a>(&mut self, names: impl IntoIterator<Item = &'a String>) {
        self.disqualified.extend(names.into_iter().cloned());
    }
}

/// Returns the candidate locals of `body`.
///
/// A bare argument to a user function stays a candidate only when that function's declaration
/// takes the position by value: a by-reference parameter, an unknown or conditionally declared
/// function, and a builtin outside the read-only list all disqualify it.
pub(crate) fn find_candidates(checker: &Checker, body: &[Stmt]) -> PromotableCandidates {
    let mut scan = Scan::default();
    scan.stmts(body);
    let mut disqualified = scan.disqualified;
    for (callee, position, local) in scan.user_call_args {
        if !checker.user_function_takes_by_value(&callee, position) {
            disqualified.insert(local);
        }
    }
    PromotableCandidates {
        active: true,
        scope_escape: scan.scope_escape,
        disqualified,
        assigned: scan.assigned,
        keyed: scan.keyed,
        clone_blocked: scan.clone_blocked,
    }
}

impl Checker {
    /// Returns whether the user function `callee` binds argument `position` by value.
    fn user_function_takes_by_value(&self, callee: &str, position: usize) -> bool {
        // A conditionally declared function picks one of several declarations at run time.
        let conditional = self.function_variant_groups.get(callee).is_some_and(|variants| {
            variants.len() != 1 || variants[0] != callee
        });
        if crate::builtins::registry::lookup(callee).is_some() || conditional {
            return false;
        }
        let Some(decl) = self.fn_decls.get(callee) else {
            return false;
        };
        match decl.ref_params.get(position) {
            Some(by_ref) => !by_ref,
            None => decl.variadic.is_some() && !decl.variadic_by_ref,
        }
    }
}

/// Returns whether an element write's value may be an integer, the only value a promotable
/// `array<int>` write stores: a string, float, bool, `null` or array literal, a concatenation and
/// a cast to anything but `int` never are. Every other expression stays eligible, because its
/// type is not syntactic and an element copy (`$a[$j + 1] = $a[$j]`) must keep the view.
fn value_may_stay_int(value: &Expr) -> bool {
    !matches!(
        &value.kind,
        ExprKind::StringLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::Null
            | ExprKind::ArrayLiteral(_)
            | ExprKind::ArrayLiteralAssoc(_)
            | ExprKind::ArrayLiteralMixed(_)
            | ExprKind::BinaryOp { op: BinOp::Concat, .. }
            | ExprKind::Cast {
                target: CastType::String | CastType::Float | CastType::Bool | CastType::Array | CastType::Object,
                ..
            }
    )
}

/// What one walk of a body collects.
#[derive(Default)]
struct Scan {
    /// Locals written through a statement-form element write or append.
    written: HashSet<String>,
    /// Locals with an element write that can make a candidate promotable: its key may skip a
    /// slot (`Scan::key_may_skip`) and its value may stay an integer (`value_may_stay_int`).
    keyed: HashSet<String>,
    /// `foreach` key variables, in source order: a name stays here after its loop until it is
    /// assigned again, exactly as `Checker::is_foreach_key` keeps it.
    foreach_keys: HashSet<String>,
    /// Locals some construct can reach outside the dispatched element operations.
    disqualified: HashSet<String>,
    /// Set when a builtin can read or rebind every local of the body.
    scope_escape: bool,
    /// `(callee, argument position, local)` for each bare local passed to a function call that is
    /// not a known read-only builtin; resolved against the callee's declaration afterwards.
    user_call_args: Vec<(String, usize, String)>,
    /// Locals the body assigns as a whole (`$a = ...`), outside closures.
    assigned: HashSet<String>,
    /// Set when the body holds a construct a typed clone would duplicate: `static` locals,
    /// nested declarations, or an `include`.
    clone_blocked: bool,
}

impl Scan {
    /// Returns whether an element write under `index` may skip a slot and so make its local
    /// promotable, mirroring the key gate of `Checker::promotable_array_write`: not a key provably
    /// in range (literal `0`), not a syntactic string key (a string literal, `null`, a string cast
    /// or a concatenation, which always take a hash), and not a `foreach` key (routed to the
    /// tagged writer).
    fn key_may_skip(&self, index: &Expr) -> bool {
        !crate::types::nonempty_array_key_stays_in_range(index)
            && !matches!(
                &index.kind,
                ExprKind::StringLiteral(_)
                    | ExprKind::Null
                    | ExprKind::Cast { target: CastType::String, .. }
                    | ExprKind::BinaryOp { op: BinOp::Concat, .. }
            )
            && !matches!(&index.kind, ExprKind::Variable(name) if self.foreach_keys.contains(name))
    }

    /// Walks a statement list.
    fn stmts(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            self.stmt(stmt);
        }
    }

    /// Records one statement's writes and disqualifications, then walks its nested bodies.
    fn stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::ArrayAssign { array, index, value } => {
                self.written.insert(array.clone());
                if self.key_may_skip(index) && value_may_stay_int(value) {
                    self.keyed.insert(array.clone());
                }
                self.expr(index);
                self.expr(value);
            }
            StmtKind::ArrayPush { array, value } => {
                self.written.insert(array.clone());
                self.expr(value);
            }
            StmtKind::NestedArrayAssign { target, value } => {
                self.disqualify_root(target);
                self.expr(target);
                self.expr(value);
            }
            StmtKind::RefAssign { target, source } => {
                self.disqualified.insert(target.clone());
                self.disqualify_root(source);
                self.expr(source);
            }
            StmtKind::ListUnpack { vars, value } => {
                self.disqualified.extend(vars.iter().cloned());
                self.expr(value);
            }
            StmtKind::Global { vars } => self.disqualified.extend(vars.iter().cloned()),
            StmtKind::StaticVar { name, init } => {
                self.disqualified.insert(name.clone());
                self.clone_blocked = true;
                self.expr(init);
            }
            StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
                self.assigned.insert(name.clone());
                self.foreach_keys.remove(name);
                self.expr(value);
            }
            StmtKind::Include { path, .. } => {
                self.clone_blocked = true;
                self.expr(path);
            }
            StmtKind::IfDef { .. }
            | StmtKind::IncludeOnceMark { .. }
            | StmtKind::NamespaceDecl { .. }
            | StmtKind::UseDecl { .. }
            | StmtKind::FunctionDecl { .. }
            | StmtKind::FunctionVariantGroup { .. }
            | StmtKind::FunctionVariantMark { .. }
            | StmtKind::ConstDecl { .. }
            | StmtKind::ClassDecl { .. }
            | StmtKind::EnumDecl { .. }
            | StmtKind::PackedClassDecl { .. }
            | StmtKind::InterfaceDecl { .. }
            | StmtKind::TraitDecl { .. }
            | StmtKind::ExternFunctionDecl { .. }
            | StmtKind::ExternClassDecl { .. }
            | StmtKind::ExternGlobalDecl { .. } => self.clone_blocked = true,
            StmtKind::Foreach {
                array,
                key_var,
                value_var,
                value_by_ref,
                body,
            } => {
                if *value_by_ref {
                    self.disqualify_root(array);
                }
                self.disqualified.extend(key_var.iter().cloned());
                self.disqualified.insert(value_var.clone());
                self.expr(array);
                self.foreach_keys.extend(key_var.iter().cloned());
                self.stmts(body);
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                self.stmts(try_body);
                for clause in catches {
                    self.disqualified.extend(clause.variable.iter().cloned());
                    self.stmts(&clause.body);
                }
                if let Some(finally_body) = finally_body {
                    self.stmts(finally_body);
                }
            }
            StmtKind::Echo(expr)
            | StmtKind::Throw(expr)
            | StmtKind::ExprStmt(expr)
            | StmtKind::Return(Some(expr))
            | StmtKind::PropertyAssign { value: expr, .. }
            | StmtKind::StaticPropertyAssign { value: expr, .. }
            | StmtKind::StaticPropertyArrayPush { value: expr, .. }
            | StmtKind::PropertyArrayPush { value: expr, .. } => self.expr(expr),
            StmtKind::StaticPropertyArrayAssign { index, value, .. }
            | StmtKind::PropertyArrayAssign { index, value, .. } => {
                self.expr(index);
                self.expr(value);
            }
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses,
                else_body,
            } => {
                self.expr(condition);
                self.stmts(then_body);
                for (condition, clause) in elseif_clauses {
                    self.expr(condition);
                    self.stmts(clause);
                }
                if let Some(else_body) = else_body {
                    self.stmts(else_body);
                }
            }
            StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
                self.expr(condition);
                self.stmts(body);
            }
            StmtKind::For {
                init,
                condition,
                update,
                body,
            } => {
                if let Some(init) = init {
                    self.stmt(init);
                }
                if let Some(condition) = condition {
                    self.expr(condition);
                }
                if let Some(update) = update {
                    self.stmt(update);
                }
                self.stmts(body);
            }
            StmtKind::Switch {
                subject,
                cases,
                default,
            } => {
                self.expr(subject);
                for (labels, case) in cases {
                    labels.iter().for_each(|label| self.expr(label));
                    self.stmts(case);
                }
                if let Some(default) = default {
                    self.stmts(default);
                }
            }
            StmtKind::IncludeOnceGuard { body, .. } | StmtKind::NamespaceBlock { body, .. } => {
                self.clone_blocked = true;
                self.stmts(body);
            }
            StmtKind::Synthetic(body) => self.stmts(body),
            // Function, class and other declarations are separate scopes with their own scan.
            _ => {}
        }
    }

    /// Records the disqualifications an expression causes, at any depth.
    fn expr(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::FunctionCall { name, args } => {
                let callee = name.as_str().trim_start_matches('\\');
                if SCOPE_BUILTINS.iter().any(|builtin| callee.eq_ignore_ascii_case(builtin)) {
                    self.scope_escape = true;
                }
                if super::is_unset_call(callee) {
                    args.iter().for_each(|arg| self.disqualify_root(arg));
                } else if !BY_VALUE_ARRAY_BUILTINS
                    .iter()
                    .any(|builtin| callee.eq_ignore_ascii_case(builtin))
                {
                    for (position, arg) in args.iter().enumerate() {
                        match &arg.kind {
                            ExprKind::Variable(local) => self.user_call_args.push((
                                name.as_str().to_string(),
                                position,
                                local.clone(),
                            )),
                            _ => self.disqualify_bare_argument(arg),
                        }
                    }
                }
            }
            ExprKind::ClosureCall { args, .. }
            | ExprKind::ExprCall { args, .. }
            | ExprKind::MethodCall { args, .. }
            | ExprKind::NullsafeMethodCall { args, .. }
            | ExprKind::NullsafeDynamicMethodCall { args, .. }
            | ExprKind::StaticMethodCall { args, .. }
            | ExprKind::NewObject { args, .. }
            | ExprKind::NewGeneric { args, .. }
            | ExprKind::NewDynamic { args, .. }
            | ExprKind::NewDynamicObject { args, .. }
            | ExprKind::NewScopedObject { args, .. } => {
                args.iter().for_each(|arg| self.disqualify_bare_argument(arg));
            }
            ExprKind::Closure {
                body,
                is_arrow,
                captures,
                capture_refs,
                ..
            } => {
                self.disqualified.extend(captures.iter().cloned());
                self.disqualified.extend(capture_refs.iter().cloned());
                if *is_arrow {
                    // An arrow function captures every local its body names, by value.
                    let mut named = HashSet::new();
                    collect_named_locals(body, &mut named);
                    self.disqualified.extend(named);
                }
                // A closure body is its own scope with its own scan.
                return;
            }
            ExprKind::Assignment { target, .. } => match &target.kind {
                ExprKind::Variable(name) => {
                    self.assigned.insert(name.clone());
                    self.foreach_keys.remove(name);
                }
                _ => self.disqualify_root(target),
            },
            ExprKind::PreIncrement(name)
            | ExprKind::PostIncrement(name)
            | ExprKind::PreDecrement(name)
            | ExprKind::PostDecrement(name) => {
                self.disqualified.insert(name.clone());
            }
            _ => {}
        }
        super::loop_storage::visit_child_expressions(expr, &mut |child| self.expr(child));
    }

    /// Disqualifies a bare variable argument, which a callee can bind by reference.
    fn disqualify_bare_argument(&mut self, arg: &Expr) {
        match &arg.kind {
            ExprKind::NamedArg { value, .. } => self.disqualify_bare_argument(value),
            ExprKind::Spread(inner) => self.disqualify_root(inner),
            ExprKind::Variable(name) => {
                self.disqualified.insert(name.clone());
            }
            _ => {}
        }
    }

    /// Disqualifies the local an element or variable expression is rooted at.
    fn disqualify_root(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Variable(name) => {
                self.disqualified.insert(name.clone());
            }
            ExprKind::ArrayAccess { array, .. } => self.disqualify_root(array),
            _ => {}
        }
    }
}

/// Collects every local a statement list names, for an arrow function's implicit captures.
fn collect_named_locals(body: &[Stmt], out: &mut HashSet<String>) {
    let mut scan = Scan::default();
    scan.stmts(body);
    out.extend(scan.written);
    for stmt in body {
        visit_stmt_variables(stmt, out);
    }
}

/// Collects the variables named by the expressions of one statement.
fn visit_stmt_variables(stmt: &Stmt, out: &mut HashSet<String>) {
    let mut collect = |expr: &Expr| collect_expr_variables(expr, out);
    match &stmt.kind {
        StmtKind::Return(Some(expr)) | StmtKind::ExprStmt(expr) | StmtKind::Echo(expr) => {
            collect(expr)
        }
        _ => {}
    }
}

/// Collects the variables an expression names, at any depth.
fn collect_expr_variables(expr: &Expr, out: &mut HashSet<String>) {
    if let ExprKind::Variable(name) = &expr.kind {
        out.insert(name.clone());
    }
    super::loop_storage::visit_child_expressions(expr, &mut |child| {
        collect_expr_variables(child, out)
    });
}
