//! Purpose:
//! Flow-sensitive type narrowing for `if`/`else` branches guarded by type predicates.
//! Narrows a union- or mixed-typed variable to the guarded type in the matching branch.
//!
//! Called from:
//! - `crate::types::checker::stmt_check::control_flow` when checking `StmtKind::If`.
//!
//! Key details:
//! - Recognizes scalar, null, array, and callable `is_*($var)` predicates (and aliases),
//!   `$var instanceof Class`, `=== null` / `=== false` and their `!==` forms, and single-operand
//!   `isset(...)`. `!==` and `isset` are self-negating guards, which combine with a leading `!`
//!   the same way two negations cancel. Narrowing is applied to each clause in an if/elseif*/else
//!   chain (each subsequent clause, and the else, see the accumulated complement from previous
//!   guards). For a chain with no else where *every* clause body cannot fall through to the
//!   following statement — via `src/termination.rs`'s structural analysis
//!   (return/throw/break/continue/exit/die, statically infinite loops, nested if/switch/try whose
//!   branches all terminate, or a terminal statement before unreachable code), extended
//!   recursively with checker-known `never` calls — the accumulated complement is applied to the
//!   statements after the entire if construct.
//! - Guarded places are locals, simple instance properties (`$var->p`, `$this->p`) and simple
//!   static properties (`self::$p`, `Cls::$p`); `static::$p` is excluded because late static
//!   binding can select a different storage. Property places are keyed under a `\x01` sigil, so
//!   `purge_property_narrowings` drops all of them after any call.
//! - A completed `$this->p = <non-null>` / `self::$p = <non-null>` write re-establishes the fact
//!   for that place (as the DECLARED type minus null), and `control_flow` joins that branch-exit
//!   fact with the guard complement. Together these make PHP's lazy-initialization idiom
//!   (`if (self::$p === null) { self::$p = new S(); } return self::$p;`) type-check.
//! - Conservative: a concrete (non-union, non-mixed) type is left unchanged, an empty narrowing
//!   result falls back to the original type, and guard detection never raises a diagnostic of its
//!   own — an un-typeable receiver simply is not narrowed.

use crate::errors::CompileError;
use crate::names::{php_symbol_key, property_hook_get_method};
use crate::parser::ast::{
    BinOp, Expr, ExprKind, InstanceOfTarget, StaticReceiver, Stmt, StmtKind,
};
use crate::termination::{block_terminal_effect_with_divergence, TerminalEffect};
use crate::types::{PhpType, TypeEnv};

use super::super::Checker;

/// A detected type-guard narrowing: the guarded binding's env key and the types it takes in the
/// then-branch (guard true) and else-branch (guard false).
pub(crate) struct GuardNarrowing {
    /// `TypeEnv` key of the guarded binding: a variable name (without the leading `$`) or the
    /// synthetic property key from `narrowed_property_env_key`.
    pub var: String,
    /// Type the binding has where the guard is true.
    pub then_ty: PhpType,
    /// Type the binding has where the guard is false.
    pub else_ty: PhpType,
}

/// Describes an exact guard type or the element-agnostic array family.
enum GuardTarget {
    /// An exact scalar, null, callable, or object target.
    Exact(PhpType),
    /// Any indexed or associative array, regardless of its element types.
    AnyArray,
}

impl GuardTarget {
    /// Returns the conservative type used when the current type has no matching union member.
    fn fallback_type(&self) -> PhpType {
        match self {
            Self::Exact(ty) => ty.clone(),
            Self::AnyArray => PhpType::Mixed,
        }
    }
}

impl Checker {
    /// Detects a type-predicate guard in an `if`/ternary condition and computes the then/else
    /// narrowing for the guarded binding against the current environment. Handles the scalar
    /// `is_*` predicates, `is_null`, `instanceof Class`, and `=== false` / `=== null`, each with an
    /// optional leading `!` that swaps the branches. The guarded receiver may be a variable
    /// (narrowed under its name) or a simple property access `$var->prop` / `$this->prop`
    /// (narrowed under a synthetic key that `infer_property_access_type` consults). Returns
    /// `Ok(None)` when the condition is not a recognized guard or the receiver's current type is
    /// unknown.
    pub(crate) fn guard_narrowing(
        &mut self,
        condition: &Expr,
        env: &TypeEnv,
    ) -> Result<Option<GuardNarrowing>, CompileError> {
        let (cond, prefix_negated) = match &condition.kind {
            ExprKind::Not(inner) => (inner.as_ref(), true),
            _ => (condition, false),
        };
        let Some((receiver, target, comparison_negated)) = guard_receiver_and_target(cond) else {
            return Ok(None);
        };
        let negated = prefix_negated ^ comparison_negated;
        let Some(key) = self.guard_env_key(receiver) else {
            return Ok(None);
        };
        // An un-typeable receiver is simply not narrowed. Guard detection must never be the
        // thing that raises a diagnostic: the caller already inferred the condition through the
        // normal path, which owns the real semantics — `isset($o->virtual)` is legal through
        // `__isset` even though *reading* `$o->virtual` is not.
        if self
            .property_guard_receiver_is_unstable(receiver, env)
            .unwrap_or(true)
        {
            return Ok(None);
        }
        // A prior narrowing (or a variable binding) wins; otherwise a property receiver falls back
        // to its declared field type. An unbound plain variable stays un-narrowed.
        let current = match env.get(&key) {
            Some(ty) => ty.clone(),
            None
                if matches!(
                    receiver.kind,
                    ExprKind::PropertyAccess { .. } | ExprKind::StaticPropertyAccess { .. }
                ) =>
            {
                match self.infer_type(receiver, env) {
                    Ok(ty) => ty,
                    Err(_) => return Ok(None),
                }
            }
            None => return Ok(None),
        };
        let matched = self.narrow_to(&current, &target);
        let complement = self.narrow_complement(&current, &target);
        // `!` on the condition and a self-negating guard (`isset(...)`, `!== null`) each swap the
        // branches, so two negations cancel out — `negated` is already that XOR.
        let (then_ty, else_ty) = if negated {
            (complement, matched)
        } else {
            (matched, complement)
        };
        Ok(Some(GuardNarrowing { var: key, then_ty, else_ty }))
    }

    /// Synthetic `TypeEnv` key for a narrowed simple property access `$var->prop` (`None` for a
    /// more complex receiver). The `\x01` sigil bytes cannot appear in a real variable name, so
    /// this key never collides with a variable binding — a normal property read only picks it up
    /// when a narrowing has explicitly inserted it.
    pub(crate) fn narrowed_property_env_key(object: &Expr, property: &str) -> Option<String> {
        match &object.kind {
            ExprKind::Variable(var) => Some(format!("\u{1}prop\u{1}{var}->{property}")),
            ExprKind::This => Some(format!("\u{1}prop\u{1}$this->{property}")),
            _ => None,
        }
    }

    /// Synthetic `TypeEnv` key for a narrowed static property access (`self::$p`, `Cls::$p`).
    ///
    /// The receiver is resolved to its declaring class first, so `self::$p` and `Cls::$p` inside
    /// `Cls` share one fact. `static::$p` is deliberately not keyed: late static binding can
    /// select a subclass that redeclares the property, so the storage a guard observed is not
    /// necessarily the storage a later read reaches. The key shares the `\x01` sigil with
    /// instance-property keys, so `purge_property_narrowings` drops both after any call.
    pub(crate) fn narrowed_static_property_env_key(
        &self,
        receiver: &StaticReceiver,
        property: &str,
        expr: &Expr,
    ) -> Option<String> {
        if matches!(receiver, StaticReceiver::Static) {
            return None;
        }
        let class_name = self.resolve_static_property_receiver(receiver, expr).ok()?;
        Some(format!("\u{1}sprop\u{1}{class_name}::${property}"))
    }

    /// Returns whether a narrowing key names a property place rather than a plain local.
    ///
    /// Synthetic property keys carry the `\x01` sigil, which no PHP variable name can contain.
    /// Only these places can be re-established by a write inside a guarded branch, so the
    /// post-`if` join in `control_flow` is limited to them.
    pub(crate) fn narrowed_place_key_is_property(key: &str) -> bool {
        key.starts_with('\u{1}')
    }

    /// `TypeEnv` key for a guard receiver: a variable's name, or the synthetic property key for a
    /// simple instance/static property access. `None` for receivers narrowing can't key
    /// (complex chains, `static::$p`).
    fn guard_env_key(&self, receiver: &Expr) -> Option<String> {
        match &receiver.kind {
            ExprKind::Variable(var) => Some(var.clone()),
            ExprKind::PropertyAccess { object, property } => {
                Self::narrowed_property_env_key(object, property)
            }
            ExprKind::StaticPropertyAccess {
                receiver: static_receiver,
                property,
            } => self.narrowed_static_property_env_key(static_receiver, property, receiver),
            _ => None,
        }
    }

    /// Records the flow fact produced by a completed property or static-property write.
    ///
    /// Runs after `purge_property_narrowings` has dropped every fact the write (or the calls
    /// inside its right-hand side) could have invalidated, so this only ever re-establishes a
    /// fact for the exact storage that was just written. The recorded type is the property's
    /// DECLARED type minus `null`, never the assigned expression's type: a declared property
    /// coerces what it stores (`public ?int $x; $x = 1.0;` reads back as `int`), so narrowing
    /// to "declared, definitely not null" is the strongest statement that stays sound.
    ///
    /// Nothing is recorded when the assigned value may itself be null, when the receiver is not
    /// a simple keyable place, or when a read could run user code (`__get` / a `get` hook).
    pub(crate) fn record_property_assignment_narrowing(&mut self, stmt: &Stmt, env: &mut TypeEnv) {
        let (place, value) = match &stmt.kind {
            StmtKind::PropertyAssign {
                object,
                property,
                value,
            } => (
                Expr::new(
                    ExprKind::PropertyAccess {
                        object: object.clone(),
                        property: property.clone(),
                    },
                    stmt.span,
                ),
                value,
            ),
            StmtKind::StaticPropertyAssign {
                receiver,
                property,
                value,
            } => (
                Expr::new(
                    ExprKind::StaticPropertyAccess {
                        receiver: receiver.clone(),
                        property: property.clone(),
                    },
                    stmt.span,
                ),
                value,
            ),
            _ => return,
        };
        let Some(key) = self.guard_env_key(&place) else {
            return;
        };
        if self
            .property_guard_receiver_is_unstable(&place, env)
            .unwrap_or(true)
        {
            return;
        }
        let Ok(assigned) = self.infer_type(value, env) else {
            return;
        };
        if !type_is_definitely_non_null(&assigned) {
            return;
        }
        let Ok(declared) = self.infer_type(&place, env) else {
            return;
        };
        let non_null = self.narrow_complement(&declared, &GuardTarget::Exact(PhpType::Void));
        env.insert(key, non_null);
    }

    /// Drops every synthetic property narrowing from the environment. Called after effects that
    /// may write a property (property assignments, any call — a callee can mutate the object),
    /// and at loop-body entry (a later iteration may observe an earlier iteration's write), so a
    /// stale narrowing never survives a potential mutation. Variable narrowings are unaffected —
    /// visible assignments already update those bindings directly.
    pub(crate) fn purge_property_narrowings(env: &mut TypeEnv) {
        env.retain(|key, _| !key.starts_with('\u{1}'));
    }

    /// Drops synthetic property narrowings rooted at one local variable after that local is
    /// rebound. Other receivers remain valid and keep their precision.
    pub(crate) fn purge_property_narrowings_for_root(env: &mut TypeEnv, root: &str) {
        let prefix = format!("\u{1}prop\u{1}{root}->");
        env.retain(|key, _| !key.starts_with(&prefix));
    }

    /// Returns whether a property guard can invoke user code on either read. Hooked or magic
    /// properties are not stable flow bindings because two reads may produce different values.
    fn property_guard_receiver_is_unstable(
        &mut self,
        receiver: &Expr,
        env: &TypeEnv,
    ) -> Result<bool, CompileError> {
        let ExprKind::PropertyAccess { object, property } = &receiver.kind else {
            return Ok(false);
        };
        let object_ty = self.infer_type(object, env)?;
        let classes = match object_ty {
            PhpType::Object(class) => vec![class],
            PhpType::Union(_) => self.union_object_classes(&object_ty),
            _ => return Ok(false),
        };
        let get_hook = php_symbol_key(&property_hook_get_method(property));
        Ok(classes.iter().any(|class| {
            self.classes.get(class).is_some_and(|info| {
                info.methods.contains_key(&get_hook)
                    || (!info.properties.iter().any(|(name, _)| name == property)
                        && info.methods.contains_key("__get"))
            })
        }))
    }

    /// Narrows `current` to the guard-true type. Inside the branch the guard guarantees the target,
    /// so `Mixed` and incompatible concrete types use the target fallback; a `Union` keeps matching
    /// members; a concrete match is preserved, including its array element or object class type.
    fn narrow_to(&self, current: &PhpType, target: &GuardTarget) -> PhpType {
        match current {
            PhpType::Union(members) => {
                let kept: Vec<PhpType> =
                    members.iter().filter(|m| guard_matches(m, target)).cloned().collect();
                if kept.is_empty() {
                    target.fallback_type()
                } else {
                    self.normalize_union_type(kept)
                }
            }
            _ if guard_matches(current, target) => current.clone(),
            _ => target.fallback_type(),
        }
    }

    /// Narrows `current` to the subset incompatible with `target` (the guard-false type): a `Union`
    /// drops its matching members, while `Mixed` and concrete types are returned unchanged (the
    /// complement of `Mixed` is not representable). An empty result falls back to `current`.
    fn narrow_complement(&self, current: &PhpType, target: &GuardTarget) -> PhpType {
        match current {
            PhpType::Union(members) => {
                let kept: Vec<PhpType> =
                    members.iter().filter(|m| !guard_matches(m, target)).cloned().collect();
                if kept.is_empty() {
                    current.clone()
                } else {
                    self.normalize_union_type(kept)
                }
            }
            _ => current.clone(),
        }
    }

    /// Returns true when a statement body cannot fall through to the statement that textually
    /// follows it.
    ///
    /// The structural control-flow analysis in [`crate::termination`] recognizes
    /// `return`/`throw`/`break`/`continue`/`exit`/`die`, statically infinite loops, and nested
    /// `if`/`switch`/`try` forms whose branches all terminate, including a terminal statement placed
    /// before unreachable trailing code. `break`/`continue` count as non-fallthrough here — they
    /// prevent reaching the following statement even though they do not exit the function, which
    /// is exactly the distinction post-guard narrowing needs.
    ///
    /// User functions declared `never` need the checker's function table. The checker supplies that
    /// one semantic predicate to the shared traversal, so it applies at every nested structural
    /// level rather than as a separate shallow scan.
    ///
    /// Used by type narrowing so that an `if (guard) { ... non-fallthrough ... }` (with no else)
    /// allows the statements *after* the if to keep the complement type.
    pub(crate) fn body_cannot_fall_through(&self, body: &[Stmt]) -> bool {
        block_terminal_effect_with_divergence(body, &|expr| {
            self.expr_is_declared_never_call(expr)
        })
            != TerminalEffect::FallsThrough
    }

    /// Returns true if the expression calls a user function whose declared return type is `never`.
    /// The function name is resolved case-insensitively against the checker's function table,
    /// matching PHP's call semantics. Error suppression preserves the call's divergence.
    pub(in crate::types::checker) fn expr_is_declared_never_call(&self, expr: &Expr) -> bool {
        if let ExprKind::ErrorSuppress(inner) = &expr.kind {
            return self.expr_is_declared_never_call(inner);
        }
        let ExprKind::FunctionCall { name, .. } = &expr.kind else {
            return false;
        };
        self.canonical_function_name_folded(name)
            .and_then(|canonical| self.functions.get(&canonical))
            .map(|sig| sig.return_type == PhpType::Never)
            .unwrap_or(false)
    }
}

/// Returns whether an expression shape can be the keyed receiver of a type guard.
///
/// Variables and simple instance/static property accesses are the places `guard_env_key`
/// can name; everything else is rejected here so a comparison against a complex chain is not
/// mistaken for a guard.
fn is_guard_receiver_shape(kind: &ExprKind) -> bool {
    matches!(
        kind,
        ExprKind::Variable(_)
            | ExprKind::PropertyAccess { .. }
            | ExprKind::StaticPropertyAccess { .. }
    )
}

/// Returns the place a guard narrows, seeing through an assignment made inside the guard.
///
/// `while (($row = fgetcsv($h)) !== false)` is the shape the PHP manual uses for every
/// `T|false` reader, and it narrows exactly like `$row !== false` would: the assignment
/// completes before the comparison, so the compared value IS the variable's new value.
/// Without this the union survives into the loop body and `count($row)` stops compiling —
/// which is what made converting those builtins to a union look like a bad trade.
fn guard_receiver_place(expr: &Expr) -> Option<&Expr> {
    if is_guard_receiver_shape(&expr.kind) {
        return Some(expr);
    }
    match &expr.kind {
        ExprKind::Assignment { target, .. } if is_guard_receiver_shape(&target.kind) => {
            Some(target.as_ref())
        }
        _ => None,
    }
}

/// Extracts the guarded receiver, the target, and whether the guard is self-negating from a
/// (syntactically non-negated) guard expression.
///
/// Recognizes the scalar `is_*` predicates, `is_null`, `is_array`, `is_callable`,
/// `instanceof <Name>`, `=== false` / `=== null`, their `!==` counterparts, and single-operand
/// `isset()`. The third tuple element is `true` for guards that are true when the target does
/// NOT match (`isset`, `!==`), so `guard_narrowing` can combine it with a leading `!`. The
/// receiver may be any expression here — `guard_env_key` decides which receivers narrowing can
/// actually key.
fn guard_receiver_and_target(cond: &Expr) -> Option<(&Expr, GuardTarget, bool)> {
    match &cond.kind {
        ExprKind::FunctionCall { name, args } if args.len() == 1 => {
            // `php_symbol_key` rather than a plain lowercase: it also folds the leading `\` and
            // the namespace qualification, so `\is_int($x)` and `Ns\is_int($x)` narrow too.
            let target = match php_symbol_key(name.trim_start_matches('\\')).as_str() {
                "is_int" | "is_integer" | "is_long" => GuardTarget::Exact(PhpType::Int),
                "is_float" | "is_double" | "is_real" => GuardTarget::Exact(PhpType::Float),
                "is_string" => GuardTarget::Exact(PhpType::Str),
                "is_bool" => GuardTarget::Exact(PhpType::Bool),
                // `is_null($x)`: same narrowing as `$x === null` — elephc models a `?T` value's
                // null as Void, so the complement strips it (`if (is_null($x)) { throw; }` leaves
                // ?int as int on the fall-through path).
                "is_null" => GuardTarget::Exact(PhpType::Void),
                "is_callable" => GuardTarget::Exact(PhpType::Callable),
                "is_array" => GuardTarget::AnyArray,
                // `isset($x)` is the exact negation of `$x === null` for a keyable place: true
                // exactly when the storage holds a non-null value. This is what makes
                // `if (!isset(self::$inst)) { self::$inst = new S(); }` narrow.
                "isset" if is_guard_receiver_shape(&args[0].kind) => {
                    return Some((&args[0], GuardTarget::Exact(PhpType::Void), true))
                }
                _ => return None,
            };
            Some((&args[0], target, false))
        }
        ExprKind::InstanceOf { value, target } => {
            let InstanceOfTarget::Name(class) = target else {
                return None;
            };
            Some((
                value,
                GuardTarget::Exact(PhpType::Object(class.as_str().to_string())),
                false,
            ))
        }
        // `$var === false` / `false === $var`: narrow to the literal False subtype in the
        // then-branch; the else-branch strips only that member (e.g. int|false → int) while a full
        // `bool` member remains. Enables the common
        // `if ($x === false) { throw; } return $x;` guard (ward-http StreamGuards::requireInt etc.).
        // `!==` is the same guard with the branches swapped.
        ExprKind::BinaryOp {
            left,
            op: op @ (BinOp::StrictEq | BinOp::StrictNotEq),
            right,
        } => {
            let negates = matches!(op, BinOp::StrictNotEq);
            // `is_guard_receiver_shape` rather than an inline `Variable | PropertyAccess`
            // match: it also accepts a static property, which is what lets the singleton
            // shape `if (self::$inst === null) { self::$inst = new S(); }` narrow.
            let (receiver, lit) = match guard_receiver_place(left) {
                Some(place) => (place, &right.kind),
                None => (guard_receiver_place(right)?, &left.kind),
            };
            match lit {
                ExprKind::BoolLiteral(false) => {
                    Some((receiver, GuardTarget::Exact(PhpType::False), negates))
                }
                // `$x === null`: strip the null-ish member (elephc models a `?T` value's null as
                // Void), e.g. `?self` / self|null → self after `if ($x === null) { throw; }`.
                ExprKind::Null => Some((receiver, GuardTarget::Exact(PhpType::Void), negates)),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Returns whether a type can never hold `null` on any path.
///
/// `Mixed` and `Never` are treated as possibly-null because neither carries enough information
/// to prove otherwise; a union is non-null only when every member is.
fn type_is_definitely_non_null(ty: &PhpType) -> bool {
    match ty {
        PhpType::Void | PhpType::Never | PhpType::Mixed => false,
        PhpType::Union(members) => members.iter().all(type_is_definitely_non_null),
        _ => true,
    }
}

/// Returns true when a union member is compatible with a guard target, used to keep (then) or drop
/// (else) members. Exact targets require a matching variant; an `Object` target matches an object
/// member with the same class name (inheritance-aware narrowing is left for the future), and
/// `AnyArray` matches either array shape.
fn guard_matches(member: &PhpType, target: &GuardTarget) -> bool {
    match target {
        GuardTarget::AnyArray => matches!(member, PhpType::Array(_) | PhpType::AssocArray { .. }),
        GuardTarget::Exact(PhpType::Object(target_class)) => {
            matches!(member, PhpType::Object(member_class) if member_class == target_class)
        }
        GuardTarget::Exact(PhpType::Bool) => matches!(member, PhpType::Bool | PhpType::False),
        GuardTarget::Exact(target) => member == target,
    }
}
