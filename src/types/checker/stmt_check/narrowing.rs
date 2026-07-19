//! Purpose:
//! Flow-sensitive type narrowing for `if`/`else` branches guarded by type predicates.
//! Narrows a union- or mixed-typed variable to the guarded type in the matching branch.
//!
//! Called from:
//! - `crate::types::checker::stmt_check::control_flow` when checking `StmtKind::If`.
//!
//! Key details:
//! - Recognizes `is_int`/`is_float`/`is_string`/`is_bool`/`is_null`/`is_array`/`is_callable($var)` (and aliases),
//!   `$var instanceof Class`, and the strict comparisons `$var === null`/`$var === false` (and their
//!   `!==` / operand-swapped forms), optionally negated with a leading `!`. Narrowing is applied to
//!   each clause in an if/elseif*/else chain (each subsequent clause, and the else, see the
//!   accumulated complement from previous guards). For a chain with no else where *every* clause
//!   body always diverges (return/throw/exit/die/never-function), the accumulated complement is
//!   applied to the statements after the entire if construct.
//! - `=== false` narrows against the literal `False` subtype and `=== null` against `void`
//!   (elephc represents `null` as `void`), preserving a full `bool` member when only literal false
//!   is excluded.
//! - Conservative: a concrete (non-union, non-mixed) type is left unchanged, and an empty narrowing
//!   result falls back to the original type, so valid code is never narrowed away to `Never`.

use crate::errors::CompileError;
use crate::names::{php_symbol_key, property_hook_get_method};
use crate::parser::ast::{BinOp, Expr, ExprKind, InstanceOfTarget, Stmt, StmtKind};
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

/// The type a guard narrows toward. Most guards map to an exact `PhpType` variant, but `is_array`
/// needs to match any array member of a union regardless of its element type, and the `PhpType`
/// model has no bare, element-agnostic array variant — so it is kept as a distinct case.
enum GuardTarget {
    /// An exact scalar, `void` (null), or object target, matched by variant equality (an object by
    /// class name). Covers `is_int`/`is_float`/`is_string`/`is_bool`/`is_null`, `instanceof`, and
    /// the `=== null` / `=== false` comparisons.
    Exact(PhpType),
    /// `is_array($x)`: matches any indexed (`Array`) or associative (`AssocArray`) member.
    AnyArray,
}

impl GuardTarget {
    /// The concrete type to use when the guard is known to hold but the current type carries no
    /// matching member to keep. Exact scalar/object targets can safely select their target type;
    /// a bare `mixed` array must stay `Mixed` because the type model cannot represent the union of
    /// packed and associative runtime array layouts without changing their ABI representation.
    fn fallback_type(&self) -> PhpType {
        match self {
            GuardTarget::Exact(ty) => ty.clone(),
            GuardTarget::AnyArray => PhpType::Mixed,
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
        let Some(key) = Self::guard_env_key(receiver) else {
            return Ok(None);
        };
        if self.property_guard_receiver_is_unstable(receiver, env)? {
            return Ok(None);
        }
        // A prior narrowing (or a variable binding) wins; otherwise a property receiver falls back
        // to its declared field type. An unbound plain variable stays un-narrowed.
        let current = match env.get(&key) {
            Some(ty) => ty.clone(),
            None if matches!(receiver.kind, ExprKind::PropertyAccess { .. }) => {
                self.infer_type(receiver, env)?
            }
            None => return Ok(None),
        };
        let matched = self.narrow_to(&current, &target);
        let complement = self.narrow_complement(&current, &target);
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

    /// `TypeEnv` key for a guard receiver: a variable's name, or the synthetic property key for a
    /// simple property access. `None` for receivers narrowing can't key (complex chains).
    fn guard_env_key(receiver: &Expr) -> Option<String> {
        match &receiver.kind {
            ExprKind::Variable(var) => Some(var.clone()),
            ExprKind::PropertyAccess { object, property } => {
                Self::narrowed_property_env_key(object, property)
            }
            _ => None,
        }
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
    /// so `Mixed` and any incompatible concrete type become the target's fallback type; a `Union`
    /// keeps only its matching members (falling back if none match); a concrete type already
    /// matching the guard is kept as-is (preserving a more specific class for `instanceof` or the
    /// element type for `is_array`).
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

    /// Returns true when a statement body always diverges.
    ///
    /// A body is considered diverging if its last statement is:
    /// - `return` or `throw`
    /// - a call to `exit()` or `die()`
    /// - a call to a user function whose declared return type is `never`
    ///
    /// This is used by type narrowing so that an `if (guard) { ... diverging ... }` (with no else)
    /// allows the statements *after* the if to be narrowed to the complement type.
    pub(crate) fn body_always_diverges(&self, body: &[Stmt]) -> bool {
        let Some(last) = body.last() else {
            return false;
        };

        match &last.kind {
            StmtKind::Return(_) | StmtKind::Throw(_) => true,
            StmtKind::ExprStmt(expr) => self.expr_always_diverges(expr),
            _ => false,
        }
    }

    /// Returns true if the expression is known to never return normally: a call to `exit()` or
    /// `die()` (recognized by name), or a call to a user function whose declared return type is
    /// `never`. The function name is resolved case-insensitively against the checker's function
    /// table, matching PHP's call semantics.
    fn expr_always_diverges(&self, expr: &Expr) -> bool {
        let ExprKind::FunctionCall { name, .. } = &expr.kind else {
            return false;
        };
        let lowered = name.to_ascii_lowercase();
        if lowered == "exit" || lowered == "die" {
            return true;
        }
        self.canonical_function_name_folded(name)
            .and_then(|canonical| self.functions.get(&canonical))
            .map(|sig| sig.return_type == PhpType::Never)
            .unwrap_or(false)
    }
}

/// Extracts the guarded receiver, target, and comparison negation from a guard
/// expression. Recognizes the scalar `is_*` predicates, `is_null`, `instanceof <Name>`, and
/// `===` / `!==` comparisons with false or null. The receiver may be any expression here;
/// `guard_env_key` decides which receivers narrowing can actually key.
fn guard_receiver_and_target(cond: &Expr) -> Option<(&Expr, GuardTarget, bool)> {
    match &cond.kind {
        ExprKind::FunctionCall { name, args } if args.len() == 1 => {
            let target = match name.as_str().to_ascii_lowercase().as_str() {
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
        ExprKind::BinaryOp { left, op, right }
            if matches!(op, BinOp::StrictEq | BinOp::StrictNotEq) =>
        {
            let (receiver, lit) = match (&left.kind, &right.kind) {
                (ExprKind::Variable(_) | ExprKind::PropertyAccess { .. }, _) => {
                    (left.as_ref(), &right.kind)
                }
                (_, ExprKind::Variable(_) | ExprKind::PropertyAccess { .. }) => {
                    (right.as_ref(), &left.kind)
                }
                _ => return None,
            };
            match lit {
                ExprKind::BoolLiteral(false) => Some((
                    receiver,
                    GuardTarget::Exact(PhpType::False),
                    matches!(op, BinOp::StrictNotEq),
                )),
                // `$x === null`: strip the null-ish member (elephc models a `?T` value's null as
                // Void), e.g. `?self` / self|null → self after `if ($x === null) { throw; }`.
                ExprKind::Null => Some((
                    receiver,
                    GuardTarget::Exact(PhpType::Void),
                    matches!(op, BinOp::StrictNotEq),
                )),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Returns true when a union member is compatible with a guard target, used to keep (then) or drop
/// (else) members. `AnyArray` matches indexed and associative arrays; exact object targets match
/// by class name, and exact `bool` also accepts the literal `False` subtype.
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
