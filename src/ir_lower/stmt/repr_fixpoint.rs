//! Purpose:
//! Lowers every statement at an array-representation FIXED POINT, so no instruction is ever
//! compiled against a runtime array layout that another instruction in the same statement replaces.
//!
//! Called from:
//! - `crate::ir_lower::stmt::lower_stmt`, for every statement of every body.
//!
//! Key details:
//! - Two ops rewrite a local array's storage in place: `Op::ArrayToMixed` (boxed element slots) and
//!   `Op::ArrayToHash` (packed vector -> hash table). Both are emitted MID-lowering, from a decision
//!   that depends on already-lowered value types, so the AST cannot predict them.
//! - The statement is the smallest region that both DOMINATES and PRECEDES every op it contains, so
//!   it is the smallest region a conversion can be hoisted to without corrupting an operand that was
//!   already emitted (see `lower_stmt_at_type_fixpoint`).

use std::collections::HashSet;

use crate::ir::{LocalKind, Op};
use crate::ir_lower::context::LoweringContext;
use crate::parser::ast::{
    BinOp, CallableTarget, CatchClause, Expr, ExprKind, InstanceOfTarget, Stmt, StmtKind,
};
use crate::span::Span;
use crate::types::{array_storage_conversion, PhpType};

use super::{local_slot_is_convertible_here, lower_stmt_once};

/// Lowers one statement against local types that its own conversions cannot invalidate.
///
/// An op is lowered against the local-type environment holding AT THAT POINT. A conversion
/// (`Op::ArrayToMixed`, `Op::ArrayToHash`) re-types a local mid-lowering AND, at runtime, rewrites
/// the array's storage. Any op lowered against the OLD representation but EXECUTED after the
/// conversion then misinterprets that storage: boxed cell pointers read as raw scalars (SIGSEGV), or
/// packed indices read out of a hash table (silent data loss). The conversion is emitted where the
/// write happens, but it is reachable — a `switch` case, a `catch` handler, a ternary arm, a loop
/// back edge — from code that was lowered before it.
///
/// The fix is to CANONICALIZE: discover what the statement converts, then convert it up front, at a
/// point that both DOMINATES and PRECEDES every op inside, and lower the statement again against
/// that. Only the statement is small enough to be that point and large enough to cover every
/// construct: no operand of a statement precedes the statement's entry, so re-lowering it re-lowers
/// the operands too.
///
/// Hoisting to a construct's own entry (a `switch`'s, a `match`'s) instead REGRESSES correct
/// programs: in `h($m, match ($c) { 1 => $m[0] = "s", default => "d" })` argument 0 is emitted
/// BEFORE the match is lowered, holds the array pointer with no refcount protection, and
/// `__rt_array_to_mixed` rewrites the slots IN PLACE at refcount 1 — so running the conversion on
/// the `default` path (where today it does not run) would corrupt an argument that is already
/// correct. Hoisting to the STATEMENT is safe precisely because argument 0 is then re-lowered too.
///
/// The discovery pass is what makes this cost anything, so it is gated three times: no convertible
/// array local in scope, or a statement that cannot syntactically touch one, skips it entirely; and
/// a statement lowered INSIDE a discovery pass runs no discovery of its own (`is_speculating`),
/// which keeps the total lowering cost linear in nesting depth instead of exponential.
pub(super) fn lower_stmt_at_type_fixpoint(ctx: &mut LoweringContext<'_, '_>, stmt: &Stmt) {
    lower_region_at_type_fixpoint(
        ctx,
        stmt.span,
        |candidates| may_convert_array_local(stmt, candidates),
        |ctx| lower_stmt_once(ctx, stmt),
    );
}

/// Lowers the post-init region of a `for` loop at the same fixed point.
///
/// `for ($a = [1, 2]; ...; ...) { $a[0] = "s"; }` needs a region whose entry is BELOW the init: at
/// the `for` statement's own entry `$a` does not exist yet, so it is not a candidate there and its
/// conversion cannot be hoisted. Below the init it is an ordinary loop-carried array, and the
/// conversion lands in the preheader like any other.
pub(super) fn lower_for_body_at_type_fixpoint(
    ctx: &mut LoweringContext<'_, '_>,
    span: Span,
    condition: Option<&Expr>,
    update: Option<&Stmt>,
    body: &[Stmt],
    lower_once: impl Fn(&mut LoweringContext<'_, '_>),
) {
    lower_region_at_type_fixpoint(
        ctx,
        span,
        |candidates| {
            // The for-body region is a LOOP body, so the back edge is the hiding construct — it
            // re-runs body ops lowered above a conversion regardless of what the body contains. The
            // `has_hiding` narrowing that skips straight-line STATEMENTS must not apply here, or a
            // loop with a straight-line widening body (`for ($a=[..];..) { echo $a[0]; $a[0]="s"; }`)
            // would never pre-widen `$a` in the preheader.
            let mut scan = ConversionScan::new_hiding_region(candidates);
            if let Some(condition) = condition {
                scan.expr(condition);
            }
            if let Some(update) = update {
                scan.stmt(update);
            }
            scan.block(body);
            scan.done()
        },
        lower_once,
    );
}

/// Lowers one region against local types that its own conversions cannot invalidate.
fn lower_region_at_type_fixpoint(
    ctx: &mut LoweringContext<'_, '_>,
    span: Span,
    may_convert: impl FnOnce(&[String]) -> bool,
    lower_once: impl Fn(&mut LoweringContext<'_, '_>),
) {
    if ctx.is_speculating() {
        lower_once(ctx);
        return;
    }
    let candidates = convertible_array_locals(ctx);
    if candidates.is_empty() || !may_convert(&candidates) {
        lower_once(ctx);
        return;
    }

    // Discover the conversions by LOWERING the region — the decision is a pure function of
    // already-lowered value types, so re-deriving it from the AST would duplicate (and eventually
    // desynchronize from) the lowering that makes it — then throw that lowering away completely.
    // The records are read BEFORE the rollback discards them, but they are matched against the
    // region's ENTRY types only AFTER it, in `canonicalize_array_locals`: read now, the entry types
    // are the speculation's EXIT types, which already carry the very conversions being looked for.
    let snapshot = ctx.snapshot();
    ctx.forget_array_conversions(&candidates);
    let outer = ctx.set_speculating(true);
    lower_once(ctx);
    ctx.set_speculating(outer);
    let conversions = discovered_conversions(ctx, &candidates);
    ctx.restore(snapshot);

    // Re-lower unconditionally, even with nothing to convert: the discovery pass suppressed the
    // fixed point of every NESTED region, so its output is not a lowering anyone may keep. A local
    // first assigned inside this region is not a candidate here and is canonicalized by its own
    // nested region during this second, non-speculative pass.
    canonicalize_array_locals(ctx, &conversions, span);
    lower_once(ctx);
}

/// Returns the locals whose array storage a statement could still convert, in a deterministic order.
///
/// A local already held in hash storage is excluded: no op converts a hash back, so its
/// representation is final. Everything else that is an indexed array — including an `Array(Mixed)`,
/// which a string-keyed write still promotes to a hash — can move.
fn convertible_array_locals(ctx: &LoweringContext<'_, '_>) -> Vec<String> {
    let mut names = ctx
        .local_types
        .iter()
        .filter(|(name, php_type)| {
            matches!(php_type.codegen_repr(), PhpType::Array(_))
                && local_slot_is_convertible_here(ctx, name)
        })
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    // `local_types` is a HashMap: sort so the conversion instructions are emitted in a stable order
    // regardless of hash seed.
    names.sort();
    names
}

/// Returns what the just-lowered region converted each candidate to, as `(local, representation)`.
///
/// The candidates come from the type environment and are filtered by the conversion record, not the
/// other way round: the record is function-scoped, so a set DIFFERENCE against a pre-region snapshot
/// would miss a local converted earlier in the function, rebound to a fresh concrete array since,
/// and converted again here. `forget_array_conversions` is what keeps that over-approximation from
/// hoisting conversions this region does not actually perform.
fn discovered_conversions(
    ctx: &LoweringContext<'_, '_>,
    candidates: &[String],
) -> Vec<(String, PhpType)> {
    candidates
        .iter()
        .filter_map(|name| Some((name.clone(), ctx.array_conversion(name)?.clone())))
        .collect()
}

/// Returns the op that converts a local from its region-entry representation to `target`.
///
/// `None` means the storage is already in the target representation (or in one no op moves it out
/// of), and hoisting anything would be worse than hoisting nothing: emitting `Op::ArrayToMixed`
/// where a hash is what the region actually builds would box the slots of an array the region then
/// re-reads as a hash. The decision is `array_storage_conversion` — the SAME predicate the checker
/// applies to the type environment — so the element type a callee is compiled for cannot drift from
/// the one the caller actually passes.
fn conversion_op(entry: &PhpType, target: &PhpType) -> Option<Op> {
    match array_storage_conversion(Some(entry), target)? {
        PhpType::Array(_) => Some(Op::ArrayToMixed),
        PhpType::AssocArray { .. } => Some(Op::ArrayToHash),
        _ => None,
    }
}

/// Converts local arrays to the representation the region ahead was lowered against.
///
/// This is the same pair of conversions the element writes themselves perform
/// (`prepare_indexed_array_local_set` and `lower_string_key_array_promotion`), emitted where control
/// flow needs them instead of where the write happens, and with the same ownership pairing: the
/// helpers take the loaded array as an owned reference and `store_mutated_local` puts the result
/// back without re-acquiring it.
///
/// Both helpers are idempotent on an already-converted array — `__rt_array_to_mixed` re-stamps a
/// Mixed array without re-boxing it, and `Op::ArrayToHash` reuses a hash payload as-is — so a
/// canonicalization inside an outer loop stays correct on every iteration.
fn canonicalize_array_locals(
    ctx: &mut LoweringContext<'_, '_>,
    conversions: &[(String, PhpType)],
    span: Span,
) {
    for (name, target) in conversions {
        let Some(op) = conversion_op(&ctx.local_type(name), target) else {
            continue;
        };
        let array = ctx.load_local(name, Some(span));
        let converted = ctx.emit_value(
            op,
            vec![array.value],
            None,
            target.clone(),
            op.default_effects(),
            Some(span),
        );
        ctx.store_mutated_local(name, converted, target.clone(), Some(span));
    }
}

/// Returns true when a statement could possibly convert the storage of one of `candidates` ON A
/// PATH THAT AN ALREADY-LOWERED OP OF THE SAME STATEMENT DEPENDS ON.
///
/// Purely syntactic and deliberately coarse: it never re-derives the lowering's decision (which
/// depends on types the AST does not carry), it only proves the ABSENCE of a hazard. THREE facts
/// have to hold together:
///
/// - it names a candidate, and
/// - it contains a node that can mutate a local — because a conversion is performed BY a mutation ON
///   a named local (an assignment, any call — a by-ref array param converts its arg, `unset($a[$k])`
///   promotes to a hash, `array_push` widens — all count), and
/// - it contains a CONVERSION-HIDING construct: any BRANCHING — an `if`/`elseif`/`else`, a loop
///   (back edge re-runs ops lowered above a conversion), a `switch` (fall-through gives a case two
///   predecessors), a `try` (the handler is reachable from mid-body) — or a conditionally-evaluated
///   expression (`match`, ternary, `?:`, `??`, `&&`, `||`) whose arm can convert an array a SIBLING
///   operand already loaded.
///
/// The third fact is what makes a plain STRAIGHT-LINE statement cheap and is the ONLY narrowing that
/// is provably sound: `$a[0] = "s";` or `h($a);` with no branching converts inline, and every op
/// after it is lowered after it, so nothing it converts is read through a stale view. A plain `if`
/// is NOT narrowed away — its own arm-tail join reconciles only the Array->Mixed axis, so an
/// Array->Hash conversion in one arm must still be canonicalized at the `if`'s entry (a call or loop
/// after the `if` reads the local through a single lowering the per-path continuation does not always
/// duplicate). A false positive costs one speculative lowering; a false negative is a silent
/// miscompile, so the hiding set is deliberately broad — every branching form is in it.
///
/// A local reached only through an ALIAS (`$r = &$m`) is not covered, and does not need to be:
/// `local_slot_is_convertible_here` excludes ref-bound and global-storage locals from the candidates
/// in the first place.
fn may_convert_array_local(stmt: &Stmt, candidates: &[String]) -> bool {
    let mut scan = ConversionScan::new(candidates);
    scan.stmt(stmt);
    scan.done()
}

/// Accumulates the three facts `may_convert_array_local` needs from one AST subtree.
struct ConversionScan<'a> {
    candidates: HashSet<&'a str>,
    names_candidate: bool,
    mutates: bool,
    has_hiding: bool,
}

impl<'a> ConversionScan<'a> {
    /// Starts a scan over the locals a region could still convert.
    fn new(candidates: &'a [String]) -> Self {
        Self {
            candidates: candidates.iter().map(String::as_str).collect(),
            names_candidate: false,
            mutates: false,
            has_hiding: false,
        }
    }

    /// Starts a scan for a region that is ALREADY a hiding context (a loop body): the `has_hiding`
    /// fact is pre-satisfied, so the region fixpoints on names + mutation alone, exactly like the
    /// pre-narrowing gate. Used for the `for`-body region, whose enclosing loop the body scan does
    /// not itself contain.
    fn new_hiding_region(candidates: &'a [String]) -> Self {
        let mut scan = Self::new(candidates);
        scan.has_hiding = true;
        scan
    }

    /// Returns true once all three facts hold and the rest of the subtree cannot change the answer.
    fn done(&self) -> bool {
        self.names_candidate && self.mutates && self.has_hiding
    }

    /// Records a reference to a local by name.
    fn name(&mut self, name: &str) {
        if self.candidates.contains(name) {
            self.names_candidate = true;
        }
    }

    /// Records a node that can mutate a local's value or type.
    fn mutation(&mut self) {
        self.mutates = true;
    }

    /// Records a construct that can hide a conversion from an already-lowered, later-executing op.
    fn hiding(&mut self) {
        self.has_hiding = true;
    }

    /// Walks a block of statements.
    fn block(&mut self, body: &[Stmt]) {
        for stmt in body {
            if self.done() {
                return;
            }
            self.stmt(stmt);
        }
    }

    /// Walks one statement. The match is exhaustive so a new `StmtKind` cannot silently default to
    /// "cannot convert anything", which would be a miscompile rather than a missed optimization.
    fn stmt(&mut self, stmt: &Stmt) {
        if self.done() {
            return;
        }
        match &stmt.kind {
            StmtKind::Assign { name, value } => {
                self.mutation();
                self.name(name);
                self.expr(value);
            }
            StmtKind::RefAssign { target, source } => {
                self.mutation();
                self.name(target);
                self.expr(source);
            }
            StmtKind::ArrayAssign { array, index, value } => {
                self.mutation();
                self.name(array);
                self.expr(index);
                self.expr(value);
            }
            StmtKind::NestedArrayAssign { target, value } => {
                self.mutation();
                self.expr(target);
                self.expr(value);
            }
            StmtKind::ArrayPush { array, value } => {
                self.mutation();
                self.name(array);
                self.expr(value);
            }
            StmtKind::TypedAssign { type_expr: _, name, value } => {
                self.mutation();
                self.name(name);
                self.expr(value);
            }
            StmtKind::ListUnpack { vars, value } => {
                self.mutation();
                for var in vars {
                    self.name(var);
                }
                self.expr(value);
            }
            StmtKind::Global { vars } => {
                self.mutation();
                for var in vars {
                    self.name(var);
                }
            }
            StmtKind::StaticVar { name, init } => {
                self.mutation();
                self.name(name);
                self.expr(init);
            }
            StmtKind::Foreach { array, key_var, value_var, value_by_ref: _, body } => {
                // A loop back edge re-runs body ops lowered above a conversion point.
                self.hiding();
                self.mutation();
                self.expr(array);
                if let Some(key_var) = key_var {
                    self.name(key_var);
                }
                self.name(value_var);
                self.block(body);
            }
            StmtKind::PropertyAssign { object, property: _, value } => {
                self.mutation();
                self.expr(object);
                self.expr(value);
            }
            StmtKind::PropertyArrayPush { object, property: _, value } => {
                self.mutation();
                self.expr(object);
                self.expr(value);
            }
            StmtKind::PropertyArrayAssign { object, property: _, index, value } => {
                self.mutation();
                self.expr(object);
                self.expr(index);
                self.expr(value);
            }
            StmtKind::StaticPropertyAssign { receiver: _, property: _, value } => {
                self.mutation();
                self.expr(value);
            }
            StmtKind::StaticPropertyArrayPush { receiver: _, property: _, value } => {
                self.mutation();
                self.expr(value);
            }
            StmtKind::StaticPropertyArrayAssign { receiver: _, property: _, index, value } => {
                self.mutation();
                self.expr(index);
                self.expr(value);
            }
            StmtKind::Include { path, once: _, required: _ } => {
                // A residual include runs arbitrary code in the caller's scope; treat conservatively.
                self.hiding();
                self.mutation();
                self.expr(path);
            }
            StmtKind::IncludeOnceGuard { label: _, body } => {
                // A once-guard executes its body conditionally.
                self.hiding();
                self.mutation();
                self.block(body);
            }
            // A synthetic block (e.g. the nested-append desugaring) is straight-line in itself; any
            // branching it contains is discovered by walking its body.
            StmtKind::Synthetic(body) => {
                self.mutation();
                self.block(body);
            }
            StmtKind::Echo(expr) | StmtKind::Throw(expr) | StmtKind::ExprStmt(expr) => {
                self.expr(expr);
            }
            StmtKind::ConstDecl { name: _, value } => self.expr(value),
            StmtKind::Return(value) => {
                if let Some(value) = value {
                    self.expr(value);
                }
            }
            StmtKind::If { condition, then_body, elseif_clauses, else_body } => {
                // The arm-tail join reconciles the merge only on the Array->Mixed axis
                // (`join_arm_types`), not Array->Hash, and a call or a loop after the `if` reads the
                // local through ONE lowering that a per-path continuation does not always duplicate
                // (e.g. `if ($c) { $m["k"]=1; } h($m);`). Entry canonicalization is what fixes that,
                // so an `if` chain is a hiding region.
                self.hiding();
                self.expr(condition);
                self.block(then_body);
                for (condition, body) in elseif_clauses {
                    self.expr(condition);
                    self.block(body);
                }
                if let Some(else_body) = else_body {
                    self.block(else_body);
                }
            }
            StmtKind::IfDef { symbol: _, then_body, else_body } => {
                self.hiding();
                self.block(then_body);
                if let Some(else_body) = else_body {
                    self.block(else_body);
                }
            }
            StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
                // A loop back edge re-runs body ops lowered above a conversion point.
                self.hiding();
                self.expr(condition);
                self.block(body);
            }
            StmtKind::For { init, condition, update, body } => {
                // The init runs before the loop, but the condition/update/body re-run on the back
                // edge, so the whole statement is a hiding region.
                self.hiding();
                if let Some(init) = init {
                    self.stmt(init);
                }
                if let Some(condition) = condition {
                    self.expr(condition);
                }
                if let Some(update) = update {
                    self.stmt(update);
                }
                self.block(body);
            }
            StmtKind::Switch { subject, cases, default } => {
                // PHP fall-through gives a case two predecessors; a case body is lowered against the
                // previous body's exit env but is also entered directly.
                self.hiding();
                self.expr(subject);
                for (patterns, body) in cases {
                    for pattern in patterns {
                        self.expr(pattern);
                    }
                    self.block(body);
                }
                if let Some(default) = default {
                    self.block(default);
                }
            }
            StmtKind::Try { try_body, catches, finally_body } => {
                // A handler is reachable from every point in the try, including above a conversion.
                self.hiding();
                self.block(try_body);
                for CatchClause { exception_types: _, variable, body } in catches {
                    if let Some(variable) = variable {
                        self.name(variable);
                    }
                    self.block(body);
                }
                if let Some(finally_body) = finally_body {
                    self.block(finally_body);
                }
            }
            StmtKind::NamespaceBlock { name: _, body } => self.block(body),
            // Leaves and declarations: a declaration's body is lowered as its own function, with its
            // own locals, so it cannot convert one of ours.
            StmtKind::Break(_)
            | StmtKind::Continue(_)
            | StmtKind::IncludeOnceMark { .. }
            | StmtKind::NamespaceDecl { .. }
            | StmtKind::UseDecl { .. }
            | StmtKind::FunctionDecl { .. }
            | StmtKind::FunctionVariantGroup { .. }
            | StmtKind::FunctionVariantMark { .. }
            | StmtKind::ClassDecl { .. }
            | StmtKind::EnumDecl { .. }
            | StmtKind::PackedClassDecl { .. }
            | StmtKind::InterfaceDecl { .. }
            | StmtKind::TraitDecl { .. }
            | StmtKind::ExternFunctionDecl { .. }
            | StmtKind::ExternClassDecl { .. }
            | StmtKind::ExternGlobalDecl { .. } => {}
        }
    }

    /// Walks one expression. Exhaustive for the same reason as `stmt`.
    fn expr(&mut self, expr: &Expr) {
        if self.done() {
            return;
        }
        match &expr.kind {
            ExprKind::Variable(name) => self.name(name),
            ExprKind::PreIncrement(name)
            | ExprKind::PostIncrement(name)
            | ExprKind::PreDecrement(name)
            | ExprKind::PostDecrement(name) => {
                self.mutation();
                self.name(name);
            }
            ExprKind::Assignment {
                target,
                value,
                result_target,
                prelude,
                conditional_value_temp,
            } => {
                // A conditional assignment (`??=`, `?:` desugaring) evaluates its value only on one
                // path, so a conversion in it is path-dependent.
                if conditional_value_temp.is_some() {
                    self.hiding();
                }
                self.mutation();
                self.expr(target);
                self.expr(value);
                if let Some(result_target) = result_target {
                    self.expr(result_target);
                }
                self.block(prelude);
                if let Some(temp) = conditional_value_temp {
                    self.name(temp);
                }
            }
            ExprKind::FunctionCall { name: _, args } => {
                self.mutation();
                self.exprs(args);
            }
            ExprKind::MethodCall { object, method: _, args } => {
                self.mutation();
                self.expr(object);
                self.exprs(args);
            }
            ExprKind::NullsafeMethodCall { object, method: _, args } => {
                // `?->` evaluates the arguments only when the receiver is non-null: conditional.
                self.hiding();
                self.mutation();
                self.expr(object);
                self.exprs(args);
            }
            ExprKind::NullsafeDynamicMethodCall { object, method, args } => {
                // Both the dynamic method name and arguments are skipped for a null receiver.
                self.hiding();
                self.mutation();
                self.expr(object);
                self.expr(method);
                self.exprs(args);
            }
            ExprKind::StaticMethodCall { receiver: _, method: _, args }
            | ExprKind::NewScopedObject { receiver: _, args } => {
                self.mutation();
                self.exprs(args);
            }
            ExprKind::ClosureCall { var, args } => {
                self.mutation();
                self.name(var);
                self.exprs(args);
            }
            ExprKind::ExprCall { callee, args } => {
                self.mutation();
                self.expr(callee);
                self.exprs(args);
            }
            ExprKind::Pipe { value, callable } => {
                self.mutation();
                self.expr(value);
                self.expr(callable);
            }
            ExprKind::NewObject { class_name: _, args } => {
                self.mutation();
                self.exprs(args);
            }
            ExprKind::NewDynamic { name_expr, args } => {
                self.mutation();
                self.expr(name_expr);
                self.exprs(args);
            }
            ExprKind::NewDynamicObject {
                class_name,
                fallback_class: _,
                required_parent: _,
                args,
            } => {
                self.mutation();
                self.expr(class_name);
                self.exprs(args);
            }
            ExprKind::IncludeValue { path, once: _, required: _ } => {
                // An include in expression position runs arbitrary code; treat conservatively.
                self.hiding();
                self.mutation();
                self.expr(path);
            }
            ExprKind::Yield { key, value } => {
                self.mutation();
                if let Some(key) = key {
                    self.expr(key);
                }
                if let Some(value) = value {
                    self.expr(value);
                }
            }
            ExprKind::YieldFrom(value) => {
                self.mutation();
                self.expr(value);
            }
            ExprKind::Clone(value) => {
                // Cloning can execute a user-defined `__clone` method.
                self.mutation();
                self.expr(value);
            }
            ExprKind::Closure {
                params,
                variadic: _,
                variadic_type: _,
                return_type: _,
                body,
                is_arrow: _,
                is_static: _,
                by_ref_return: _,
                variadic_by_ref: _,
                captures,
                capture_refs,
            } => {
                // The body is lowered as its own function against its own locals; only the capture
                // list touches ours, and a by-reference capture makes the local ref-bound (never a
                // candidate). The default-value expressions are still ours.
                for (_, _, default, _) in params {
                    if let Some(default) = default {
                        self.expr(default);
                    }
                }
                self.block(body);
                for capture in captures.iter().chain(capture_refs) {
                    self.name(capture);
                }
            }
            ExprKind::BinaryOp { left, op, right } => {
                // `&&`/`||` evaluate the right operand only conditionally, so a conversion in it is
                // as path-dependent as a ternary arm; other binary ops evaluate both operands.
                if matches!(op, BinOp::And | BinOp::Or) {
                    self.hiding();
                }
                self.expr(left);
                self.expr(right);
            }
            ExprKind::InstanceOf { value, target } => {
                self.expr(value);
                if let InstanceOfTarget::Expr(target) = target {
                    self.expr(target);
                }
            }
            ExprKind::Negate(inner)
            | ExprKind::Not(inner)
            | ExprKind::BitNot(inner)
            | ExprKind::Throw(inner)
            | ExprKind::ErrorSuppress(inner)
            | ExprKind::Print(inner)
            | ExprKind::Spread(inner)
            | ExprKind::Cast { target: _, expr: inner }
            | ExprKind::PtrCast { target_type: _, expr: inner }
            | ExprKind::NamedArg { name: _, value: inner } => self.expr(inner),
            ExprKind::NullCoalesce { value, default } | ExprKind::ShortTernary { value, default } => {
                // The default is evaluated only when the value is null/absent — conditional.
                self.hiding();
                self.expr(value);
                self.expr(default);
            }
            ExprKind::Ternary { condition, then_expr, else_expr } => {
                self.hiding();
                self.expr(condition);
                self.expr(then_expr);
                self.expr(else_expr);
            }
            ExprKind::Match { subject, arms, default } => {
                self.hiding();
                self.expr(subject);
                for (patterns, body) in arms {
                    self.exprs(patterns);
                    self.expr(body);
                }
                if let Some(default) = default {
                    self.expr(default);
                }
            }
            ExprKind::ArrayLiteral(items) => self.exprs(items),
            ExprKind::ArrayLiteralAssoc(items) => {
                for (key, value) in items {
                    self.expr(key);
                    self.expr(value);
                }
            }
            ExprKind::ArrayAccess { array, index } => {
                self.expr(array);
                self.expr(index);
            }
            // A nullsafe property READ has no convertible sub-expression (a static property name),
            // so it is not a hiding construct even though the read itself is conditional.
            ExprKind::PropertyAccess { object, property: _ }
            | ExprKind::NullsafePropertyAccess { object, property: _ }
            | ExprKind::ObjectClassName { object } => self.expr(object),
            ExprKind::DynamicPropertyAccess { object, property } => {
                self.expr(object);
                self.expr(property);
            }
            ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
                // `$o?->{$expr}` evaluates `$expr` only when the receiver is non-null: conditional.
                self.hiding();
                self.expr(object);
                self.expr(property);
            }
            ExprKind::BufferNew { element_type: _, len } => self.expr(len),
            ExprKind::FirstClassCallable(target) => {
                if let CallableTarget::Method { object, method: _ } = target {
                    self.expr(object);
                }
            }
            ExprKind::StringLiteral(_)
            | ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::Null
            | ExprKind::ConstRef(_)
            | ExprKind::This
            | ExprKind::StaticPropertyAccess { .. }
            | ExprKind::ClassConstant { .. }
            | ExprKind::ScopedConstantAccess { .. }
            | ExprKind::MagicConstant(_) => {}
        }
    }

    /// Walks a list of expressions.
    fn exprs(&mut self, exprs: &[Expr]) {
        for expr in exprs {
            if self.done() {
                return;
            }
            self.expr(expr);
        }
    }
}

/// Returns true when a local can be converted in place at the CURRENT program point.
///
/// Split out of `local_slot_is_convertible` so the per-statement candidate scan does not clone the
/// definitely-initialized slot set once per statement.
pub(super) fn local_slot_kind_is_convertible(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
) -> bool {
    matches!(
        ctx.local_kinds.get(name).copied().unwrap_or(LocalKind::PhpLocal),
        LocalKind::PhpLocal | LocalKind::StaticLocal
    ) && !ctx.is_ref_bound_local(name)
        && !ctx.local_uses_global_storage(name)
}
