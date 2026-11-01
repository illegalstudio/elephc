//! Purpose:
//! Flow-sensitive type narrowing for `if`/`else` branches guarded by type predicates or truthiness.
//! Narrows a union- or mixed-typed variable to the guarded type in the matching branch.
//!
//! Called from:
//! - `crate::types::checker::stmt_check::control_flow` when checking `StmtKind::If`.
//!
//! Key details:
//! - Recognizes scalar, null, array, object, countable, and callable `is_*($var)` predicates (and aliases),
//!   `$var instanceof Class`, `=== null` / `=== false` and their `!==` forms, and single-operand
//!   `isset(...)`. Bare locals and simple local assignments also narrow representable null/false
//!   members on their truthy edge. `!==` and `isset` are self-negating guards, which combine with a leading `!`
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
//!   `purge_property_narrowings` drops all of them after any call, except a `readonly` property
//!   of `$this` in a hierarchy that declares no `__clone` (`purge_property_narrowings_after_call`).
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
    /// The guard is false for no value the binding can hold (`is_int($x)` over an `int` `$x`), so
    /// the false side cannot run. `else_ty` keeps the binding's type there all the same: only a
    /// consumer that knows the side is dead (a ternary branch) may treat it as such.
    pub else_dead: bool,
    /// The guard is true for no value the binding can hold.
    pub then_dead: bool,
}

/// Describes an exact guard type, an unresolved object target, or a structural PHP type family.
enum GuardTarget {
    /// An exact scalar, null, callable, or object target.
    Exact(PhpType),
    /// A one-sided proof that the truthy path cannot contain null.
    NonNull,
    /// An object-like target whose declaration is unavailable to the static checker.
    UnknownObject,
    /// Any indexed or associative array, regardless of its element types.
    AnyArray,
    /// Any object, regardless of its nominal class.
    AnyObject,
    /// Any value `foreach` accepts: an array of either storage, or a `Traversable` object.
    AnyIterable,
    /// Any value `is_callable()` accepts. Unlike `Exact(Callable)` — which `instanceof Closure`
    /// uses and which proves a closure object — this one also admits the plain function-name
    /// string, so an unconstrained receiver narrows to the boxed `Callable|string` union instead
    /// of the descriptor-shaped `Callable`.
    AnyCallable,
    /// Any indexed/associative array or object implementing `Countable`.
    Countable,
    /// A value accepted by `is_numeric()`, including runtime-validated numeric strings.
    Numeric,
}

impl GuardTarget {
    /// Returns the conservative type used when the current type has no matching union member.
    fn fallback_type(&self) -> PhpType {
        match self {
            Self::Exact(ty) => ty.clone(),
            Self::AnyArray => PhpType::Array(Box::new(PhpType::Mixed)),
            Self::AnyObject => PhpType::Object(String::new()),
            // `iterable` is `array|Traversable` and the guard proves exactly that pair. Keeping
            // the two members apart is what lets a following `is_array()` guard subtract one and
            // leave the other: `twig/twig`'s `CoreExtension::filter()` throws unless
            // `is_iterable($array)`, returns early when `is_array($array)`, and then hands what is
            // left to `new \IteratorIterator($array)` -- which needs a Traversable, not a Mixed.
            Self::AnyIterable => PhpType::Union(vec![
                PhpType::Array(Box::new(PhpType::Mixed)),
                PhpType::Object("Traversable".to_string()),
            ]),
            // `is_callable($x)` on an unconstrained value proves a callable, not a closure: PHP
            // accepts a function-name string there too, and Symfony's `ClassStub::wrapCallable`
            // feeds exactly that value to a `string` parameter. The union keeps the boxed Mixed
            // representation the storage already has, so the scalar funnel accepts it.
            Self::AnyCallable => PhpType::Union(vec![PhpType::Callable, PhpType::Str]),
            Self::NonNull | Self::UnknownObject => PhpType::Mixed,
            Self::Numeric => PhpType::Mixed,
            Self::Countable => PhpType::Union(vec![
                PhpType::Array(Box::new(PhpType::Mixed)),
                PhpType::Object("Countable".to_string()),
            ]),
        }
    }
}

impl Checker {
    /// Returns the environment for a single-condition arm of `match (true)` or `match (false)`.
    ///
    /// PHP commonly expresses type dispatch as `match (true) { $value instanceof T => ... }`.
    /// The arm result executes on the guard's matching edge, so it receives the same narrowing
    /// as an `if` body. Multi-condition arms remain conservative because their conditions are
    /// alternatives and may narrow different bindings or types.
    pub(crate) fn match_arm_narrowed_env(
        &mut self,
        subject: &Expr,
        conditions: &[Expr],
        env: &TypeEnv,
    ) -> Result<TypeEnv, CompileError> {
        let ExprKind::BoolLiteral(subject_value) = &subject.kind else {
            return Ok(env.clone());
        };
        let [condition] = conditions else {
            return Ok(env.clone());
        };
        let Some(narrowing) = self.guard_narrowing(condition, env)? else {
            return Ok(env.clone());
        };
        let mut narrowed = env.clone();
        narrowed.insert(
            narrowing.var,
            if *subject_value {
                narrowing.then_ty
            } else {
                narrowing.else_ty
            },
        );
        Ok(narrowed)
    }

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
        if let Some(narrowing) = self.truthy_binding_guard_narrowing(cond, prefix_negated, env)? {
            return Ok(Some(narrowing));
        }
        let Some((receiver, target, comparison_negated)) =
            guard_receiver_and_target(self, cond)
        else {
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
        // A prior narrowing (or a variable binding) wins; otherwise a property or `$this`
        // receiver falls back to its declared type. An unbound plain variable stays un-narrowed.
        let current = match env.get(&key) {
            Some(ty) => ty.clone(),
            None
                if matches!(
                    receiver.kind,
                    ExprKind::PropertyAccess { .. }
                        | ExprKind::StaticPropertyAccess { .. }
                        | ExprKind::This
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
        let complement_dead = self.guard_covers_every_member(&current, &target);
        // `!` on the condition and a self-negating guard (`isset(...)`, `!== null`) each swap the
        // branches, so two negations cancel out — `negated` is already that XOR.
        let (then_ty, else_ty, then_dead, else_dead) = if negated {
            (complement, matched, complement_dead, false)
        } else {
            (matched, complement, false, complement_dead)
        };
        Ok(Some(GuardNarrowing { var: key, then_ty, else_ty, else_dead, then_dead }))
    }

    /// Narrows a local truthiness guard, including a simple assignment used as the condition.
    ///
    /// Assignment conditions use the local type installed by the condition's already-completed
    /// assignment-effects pass. Re-inferring the right-hand side here would execute its type
    /// effects twice and can make a self-referential assignment resolve its receiver through the
    /// newly assigned type. Ordinary locals therefore retain the precise assigned type, while an
    /// explicitly typed local conservatively retains its declaration. Only fully representable
    /// falsey members (`null` and literal `false`) are removed; zero and empty strings/arrays
    /// remain conservative.
    fn truthy_binding_guard_narrowing(
        &mut self,
        condition: &Expr,
        negated: bool,
        env: &TypeEnv,
    ) -> Result<Option<GuardNarrowing>, CompileError> {
        let (var, current, overwrites) = match &condition.kind {
            ExprKind::Variable(var) => {
                let Some(current) = env.get(var) else {
                    return Ok(None);
                };
                (var.clone(), current.clone(), false)
            }
            ExprKind::Assignment {
                target,
                value: _,
                result_target: None,
                prelude,
                conditional_value_temp: None,
                ..
            } if prelude.is_empty() => {
                let ExprKind::Variable(var) = &target.kind else {
                    return Ok(None);
                };
                let Some(current) = env.get(var) else {
                    return Ok(None);
                };
                (var.clone(), current.clone(), true)
            }
            _ => return Ok(None),
        };
        let truthy = match &current {
            PhpType::Union(members) => {
                let kept: Vec<PhpType> = members
                    .iter()
                    .filter(|member| !matches!(member, PhpType::Void | PhpType::False))
                    .cloned()
                    .collect();
                if kept.is_empty() || kept.len() == members.len() {
                    current.clone()
                } else {
                    self.normalize_union_type(kept)
                }
            }
            // A local known to hold ONLY null cannot be truthy, so the guarded branch is dead.
            // Saying so is what lets `if ($e) { throw $e; }` compile after a `goto` cloned the
            // live copy elsewhere -- twig's `CoreExtension::getAttribute()` leaves exactly that
            // tail behind, with `$propertyNotAllowedError` still at its `= null` initialiser.
            PhpType::Void | PhpType::False => PhpType::Never,
            _ => current.clone(),
        };
        if !overwrites && truthy == current {
            return Ok(None);
        }
        let (then_ty, else_ty) = if negated {
            (current, truthy)
        } else {
            (truthy, current)
        };
        Ok(Some(GuardNarrowing {
            var,
            then_ty,
            else_ty,
            else_dead: false,
            then_dead: false,
        }))
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

    /// Synthetic `TypeEnv` key for a narrowed bare `$this` receiver.
    ///
    /// The leading control-byte sigil keeps the fact outside the PHP variable namespace and lets
    /// the existing property-effect invalidation discard it after calls that may mutate state.
    pub(crate) fn narrowed_this_env_key() -> &'static str {
        "\u{1}this\u{1}$this"
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

    /// `TypeEnv` key for a guard receiver: a variable's name, bare `$this`, or the synthetic
    /// property key for a simple instance/static property access. `None` for receivers narrowing
    /// cannot key (complex chains, `static::$p`).
    fn guard_env_key(&self, receiver: &Expr) -> Option<String> {
        match &receiver.kind {
            ExprKind::Variable(var) => Some(var.clone()),
            ExprKind::Assignment {
                target,
                result_target: None,
                prelude,
                conditional_value_temp: None,
                ..
            } if prelude.is_empty() => self.guard_env_key(target),
            ExprKind::PropertyAccess { object, property } => {
                Self::narrowed_property_env_key(object, property)
            }
            ExprKind::StaticPropertyAccess {
                receiver: static_receiver,
                property,
            } => self.narrowed_static_property_env_key(static_receiver, property, receiver),
            ExprKind::This => Some(Self::narrowed_this_env_key().to_string()),
            _ => None,
        }
    }

    /// Returns the stable environment key for a method receiver used by flow-effect inference.
    ///
    /// Short-circuit inference uses this to retain an already-proven nominal type while invoking
    /// a zero-argument method through that exact property. The wrapper deliberately exposes only
    /// the key construction; guard recognition and stability validation remain owned here.
    pub(crate) fn flow_guard_env_key(&self, receiver: &Expr) -> Option<String> {
        self.guard_env_key(receiver)
    }

    /// Records that this body TESTED whether a receiver has a method, so a call it guards is
    /// admitted even when no class in the closed world declares that method.
    ///
    /// Only a literal method name counts: a computed one names no method to admit. The receiver
    /// uses the same place key the type guards use, so `method_exists($a, 'm')` says nothing about
    /// `$b->m()`.
    pub(crate) fn record_method_exists_capability_guard(&mut self, name: &str, args: &[Expr]) {
        let name = name.trim_start_matches('\\');
        // `is_callable([$x, 'm'])` asks the same question: Laravel's `PendingRequest` tests
        // `is_callable([$e, 'hasResponse'])` before calling it, a Guzzle 7 method Guzzle 8 dropped.
        let (receiver, method) = if name.eq_ignore_ascii_case("method_exists") && args.len() == 2 {
            (&args[0], &args[1])
        } else if name.eq_ignore_ascii_case("is_callable") && !args.is_empty() {
            let ExprKind::ArrayLiteral(pair) = &args[0].kind else {
                return;
            };
            let [receiver, method] = pair.as_slice() else {
                return;
            };
            (receiver, method)
        } else {
            return;
        };
        let ExprKind::StringLiteral(method) = &method.kind else {
            return;
        };
        let Some(receiver_key) = self.guard_env_key(receiver) else {
            return;
        };
        let scope = self.current_loop_storage_scope.clone();
        self.method_exists_guards
            .insert((scope, receiver_key, php_symbol_key(method)));
    }

    /// Reports whether this body tested `receiver` for `method_key` with `method_exists()`.
    pub(crate) fn method_exists_capability_guarded(&self, receiver: &Expr, method_key: &str) -> bool {
        let Some(receiver_key) = self.guard_env_key(receiver) else {
            return false;
        };
        self.method_exists_guards.contains(&(
            self.current_loop_storage_scope.clone(),
            receiver_key,
            method_key.to_string(),
        ))
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

    /// Records that a property holding an array is non-null after a completed element write.
    ///
    /// `$this->p[] = $v` and `$this->p[$k] = $v` turn a null property into an array, so the
    /// storage is an array afterwards whatever it held before: `$this->languages = [];
    /// $this->languages[] = $l; return $this->languages;` must not read back as `?array`. Only
    /// array-family declarations are narrowed; an `ArrayAccess` property is left to `offsetSet`.
    pub(crate) fn record_property_array_write_narrowing(&mut self, stmt: &Stmt, env: &mut TypeEnv) {
        let place = match &stmt.kind {
            StmtKind::PropertyArrayPush {
                object, property, ..
            }
            | StmtKind::PropertyArrayAssign {
                object, property, ..
            } => Expr::new(
                ExprKind::PropertyAccess {
                    object: object.clone(),
                    property: property.clone(),
                },
                stmt.span,
            ),
            StmtKind::StaticPropertyArrayPush {
                receiver, property, ..
            }
            | StmtKind::StaticPropertyArrayAssign {
                receiver, property, ..
            } => Expr::new(
                ExprKind::StaticPropertyAccess {
                    receiver: receiver.clone(),
                    property: property.clone(),
                },
                stmt.span,
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
        let Ok(declared) = self.infer_type(&place, env) else {
            return;
        };
        let PhpType::Union(members) = &declared else {
            return;
        };
        if !members.iter().any(|member| matches!(member, PhpType::Void))
            || !members.iter().all(|member| {
                matches!(
                    member,
                    PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Void
                )
            })
        {
            return;
        }
        let non_null = self.narrow_complement(&declared, &GuardTarget::Exact(PhpType::Void));
        env.insert(key, non_null);
    }

    /// Drops every synthetic property narrowing from the environment. Called after effects that
    /// may write a property (property assignments, any call — a callee can mutate the object),
    /// and at loop-body entry (a later iteration may observe an earlier iteration's write), so a
    /// stale narrowing never survives a potential mutation. Variable narrowings are unaffected —
    /// visible assignments already update those bindings directly.
    ///
    /// A narrowed `$this` survives: no call can change the class of the object a running body
    /// is bound to. Symfony's `CompiledUrlMatcherTrait::match()` guards
    /// `if (!$this instanceof RedirectableUrlMatcherInterface) { throw ... }` and then returns
    /// `$this->redirect(...) + $ret`; the `+` re-infers its operands after the first call purged
    /// the fact, and `redirect()` was refused on the bare `CompiledUrlMatcher`.
    pub(crate) fn purge_property_narrowings(env: &mut TypeEnv) {
        env.retain(|key, _| !key.starts_with('\u{1}') || key == Self::narrowed_this_env_key());
    }

    /// Drops the property narrowings a call may invalidate. A call can reassign any reachable
    /// property, except an initialized `readonly` one reached through `$this`: the property
    /// cannot be written again and `$this` cannot be rebound, so a guard on `$this->plugin`
    /// still holds after the call (`$this->plugin->name() . $this->plugin->count()` inside
    /// `instanceof Sold`). Any other root can be rebound by the call (a by-reference parameter,
    /// a `=&` alias, a global, a by-reference closure capture), so its facts are dropped.
    pub(crate) fn purge_property_narrowings_after_call(&self, env: &mut TypeEnv) {
        let kept: Vec<String> = env
            .keys()
            .filter(|key| self.narrowed_property_survives_calls(key))
            .cloned()
            .collect();
        env.retain(|key, _| !key.starts_with('\u{1}') || kept.contains(key));
    }

    /// Returns whether an instance-property narrowing key names a `readonly` property of `$this`
    /// that no call can change. PHP 8.3+ lets a readonly property be reinitialized while an
    /// object is cloned, from `__clone` or any method it calls, so a hierarchy that declares
    /// `__clone` anywhere keeps no fact across a call. Constructors keep none either: lowering
    /// carries no receiver fact there, although a second write would be an `Error` in PHP.
    fn narrowed_property_survives_calls(&self, key: &str) -> bool {
        let Some(property) = key.strip_prefix("\u{1}prop\u{1}$this->") else {
            return false;
        };
        if self.current_method.as_deref().is_some_and(|method| method.eq_ignore_ascii_case("__construct")) {
            return false;
        }
        let Some(class_name) = self.current_class.as_deref() else {
            return false;
        };
        let class_name = class_name.trim_start_matches('\\');
        self.classes.get(class_name).is_some_and(|info| {
            info.is_readonly_class || info.readonly_properties.contains(property)
        }) && !self.hierarchy_declares_clone(class_name)
    }

    /// Returns whether `class_name`, one of its ancestors or one of its descendants has a
    /// `__clone` method, any of which may run while an object of that class is cloned.
    fn hierarchy_declares_clone(&self, class_name: &str) -> bool {
        self.classes.iter().any(|(name, info)| {
            (name == class_name
                || self.is_subclass_of(name, class_name)
                || self.is_subclass_of(class_name, name))
                && info.methods.contains_key("__clone")
        })
    }

    /// Drops synthetic property narrowings rooted at one local variable after that local is
    /// rebound. Other receivers remain valid and keep their precision.
    pub(crate) fn purge_property_narrowings_for_root(env: &mut TypeEnv, root: &str) {
        let prefix = format!("\u{1}prop\u{1}{root}->");
        env.retain(|key, _| !key.starts_with(&prefix));
    }

    /// Drops instance/static property facts for one written property name across all aliases.
    ///
    /// A direct write to `$a->p` can invalidate a fact for `$b->p` when both locals alias the
    /// same object, but it cannot change the storage named `q`. Calls and computed writes still
    /// use the all-property invalidation path because they may execute arbitrary user code.
    pub(crate) fn purge_property_narrowings_for_property(env: &mut TypeEnv, property: &str) {
        let instance_suffix = format!("->{property}");
        let static_suffix = format!("::${property}");
        env.retain(|key, _| {
            !key.starts_with('\u{1}')
                || (!key.ends_with(&instance_suffix) && !key.ends_with(&static_suffix))
        });
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

    /// Returns true when a value already typed `member_class` necessarily passes
    /// `instanceof target_class`.
    ///
    /// `instanceof` is an INTERSECTION, not a replacement: a value that already satisfies the
    /// guard is still everything it was. Only name equality counted before, so such a guard
    /// replaced the class with the INTERFACE -- and an interface declares no properties, so the
    /// very next `$this->prop` read was typed from nothing.
    fn object_type_satisfies(&self, member_class: &str, target_class: &str) -> bool {
        crate::types::instanceof_intersection::object_type_satisfies(
            &self.classes,
            &self.interfaces,
            member_class,
            target_class,
        )
    }

    /// Narrows `current` to the guard-true type. Inside the branch the guard guarantees the target,
    /// so `Mixed` and incompatible concrete types use the target fallback; a `Union` keeps matching
    /// members; a concrete match is preserved, including its array element or object class type.
    fn narrow_to(&self, current: &PhpType, target: &GuardTarget) -> PhpType {
        if matches!(target, GuardTarget::Numeric) {
            return match current {
                PhpType::Int | PhpType::Float => current.clone(),
                _ => PhpType::Mixed,
            };
        }
        match current {
            PhpType::Union(members) => {
                let kept: Vec<PhpType> = members
                    .iter()
                    .filter(|member| self.guard_matches(member, target))
                    .cloned()
                    .collect();
                if kept.is_empty() {
                    target.fallback_type()
                } else {
                    self.normalize_union_type(kept)
                }
            }
            _ if self.guard_matches(current, target) => current.clone(),
            // `$x instanceof I` is an INTERSECTION with what `$x` already is, and an interface
            // declares METHODS, never properties. Replacing the class with the interface throws
            // away every property the value has, and the next `$this->prop` read is then typed
            // from an empty set -- it came out `Int`, and the backend refused the method call on
            // it with `method call receiver for PHP type Int`.
            //
            // MEASURED on Symfony's `CompiledUrlMatcherTrait::match()`, which guards
            // `if (!$this instanceof RedirectableUrlMatcherInterface) { throw … }` seven lines
            // before `$this->context->getScheme()`.
            //
            // The closed world knows which classes satisfy both halves, and the IR lowering asks
            // the same question through the same module -- the two narrowed independently before,
            // and disagreeing about `$this` is what made the SECOND read of a property fail where
            // the first had succeeded.
            PhpType::Object(class_name) => match target {
                GuardTarget::Exact(PhpType::Object(target_name)) => {
                    crate::types::instanceof_intersection::narrow_object_to_instanceof(
                        &self.classes,
                        &self.interfaces,
                        class_name,
                        target_name,
                    )
                    .unwrap_or_else(|| target.fallback_type())
                }
                _ => target.fallback_type(),
            },
            // A scalar or array can never be an instance of a class, so the guard's true edge is
            // unreachable. Keeping the current type there keeps the local's storage honest:
            // narrowing a `bool` slot to `Markup` made the backend refuse to load it (Twig's
            // compiled `($tmp = !$x) && $tmp instanceof Markup ? (string) $tmp : $tmp`).
            PhpType::Int
            | PhpType::Float
            | PhpType::Bool
            | PhpType::False
            | PhpType::Str
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
                if matches!(
                    target,
                    GuardTarget::Exact(PhpType::Object(_)) | GuardTarget::UnknownObject
                ) =>
            {
                current.clone()
            }
            _ => target.fallback_type(),
        }
    }

    /// Narrows `current` to the subset incompatible with `target` (the guard-false type): a `Union`
    /// drops its matching members, while `Mixed` and concrete types are returned unchanged (the
    /// complement of `Mixed` is not representable). An empty result falls back to `current`.
    /// Whether every value `current` can hold satisfies `target`, so the guard's complement is empty.
    ///
    /// `narrow_complement` answers `current` itself for an empty complement; this says it was empty.
    /// A structural guard that never matches (`GuardTarget::UnknownObject`) and the null probe are
    /// never complete.
    fn guard_covers_every_member(&self, current: &PhpType, target: &GuardTarget) -> bool {
        if matches!(target, GuardTarget::NonNull | GuardTarget::UnknownObject) {
            return false;
        }
        match current {
            PhpType::Union(members) => {
                !members.is_empty() && members.iter().all(|member| self.guard_matches(member, target))
            }
            PhpType::Mixed => false,
            other => self.guard_matches(other, target),
        }
    }

    fn narrow_complement(&self, current: &PhpType, target: &GuardTarget) -> PhpType {
        if matches!(target, GuardTarget::NonNull) {
            return current.clone();
        }
        match current {
            PhpType::Union(members) => {
                let kept: Vec<PhpType> = members
                    .iter()
                    .filter(|member| !self.guard_matches(member, target))
                    .cloned()
                    .collect();
                if kept.is_empty() {
                    current.clone()
                } else {
                    self.normalize_union_type(kept)
                }
            }
            // An `iterable` that is not an array is a `Traversable`: guzzle's `Create::iterFor()`
            // returns early for `is_array($value)` and hands the rest to `new \IteratorIterator()`.
            PhpType::Iterable if matches!(target, GuardTarget::AnyArray) => {
                PhpType::Object("Traversable".to_string())
            }
            _ => current.clone(),
        }
    }

    /// Returns whether `member` satisfies the structural family represented by `target`.
    ///
    /// Countability is inheritance-aware for known object types; an unresolved value falls back
    /// to the generic array-or-Countable union only on the guard's true edge.
    fn guard_matches(&self, member: &PhpType, target: &GuardTarget) -> bool {
        match target {
            GuardTarget::NonNull => !matches!(member, PhpType::Void),
            GuardTarget::UnknownObject => false,
            GuardTarget::AnyArray => {
                matches!(member, PhpType::Array(_) | PhpType::AssocArray { .. })
            }
            GuardTarget::AnyObject => matches!(member, PhpType::Object(_)),
            GuardTarget::AnyIterable => match member {
                PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable => true,
                // An UNRESOLVED class name is not proof of anything, but it is not a
                // counter-example either: keep it, exactly as the generic-object guard does.
                PhpType::Object(class_name) => {
                    class_name.is_empty()
                        || class_name
                            .trim_start_matches('\\')
                            .eq_ignore_ascii_case("Traversable")
                        || self.class_implements_interface(class_name, "Traversable")
                        || self.class_implements_interface(class_name, "Iterator")
                        || self.class_implements_interface(class_name, "IteratorAggregate")
                }
                _ => false,
            },
            // Deliberately the same member test as `Exact(Callable)`: `is_callable` is a value
            // predicate for strings and arrays, so its false edge must not strip a `string`
            // member — a non-callable string is still a string.
            GuardTarget::AnyCallable => matches!(member, PhpType::Callable),
            GuardTarget::Countable => match member {
                PhpType::Array(_) | PhpType::AssocArray { .. } => true,
                PhpType::Object(class_name) => {
                    class_name.trim_start_matches('\\').eq_ignore_ascii_case("Countable")
                        || self.class_implements_interface(class_name, "Countable")
                }
                _ => false,
            },
            GuardTarget::Numeric => {
                matches!(member, PhpType::Int | PhpType::Float | PhpType::Str | PhpType::Mixed)
            }
            GuardTarget::Exact(PhpType::Object(target_class)) => {
                matches!(member, PhpType::Object(member_class)
                    if php_symbol_key(member_class) == php_symbol_key(target_class)
                        || self.object_type_satisfies(member_class, target_class))
                    || (matches!(member, PhpType::Callable)
                        && target_class
                            .trim_start_matches('\\')
                            .eq_ignore_ascii_case("Closure"))
            }
            GuardTarget::Exact(PhpType::Bool) => {
                matches!(member, PhpType::Bool | PhpType::False)
            }
            GuardTarget::Exact(target) => member == target,
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
/// which is what made converting those builtins to a union look like a bad trade. The same
/// shape appears written the other way round (`false === $value = call()`), which is why both
/// operands are offered to this.
///
/// The SYNTHETIC assignment forms are excluded on purpose: a `result_target`, a lowering
/// `prelude` or a `conditional_value_temp` means the expression's result may be a stabilized
/// temporary rather than the written place, so narrowing the place would narrow something the
/// comparison never looked at.
fn guard_receiver_place(expr: &Expr) -> Option<&Expr> {
    if is_guard_receiver_shape(&expr.kind) {
        return Some(expr);
    }
    let ExprKind::Assignment {
        target,
        result_target: None,
        prelude,
        conditional_value_temp: None,
        ..
    } = &expr.kind
    else {
        return None;
    };
    if prelude.is_empty() && is_guard_receiver_shape(&target.kind) {
        Some(target.as_ref())
    } else {
        None
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
fn guard_receiver_and_target<'a>(
    checker: &Checker,
    cond: &'a Expr,
) -> Option<(&'a Expr, GuardTarget, bool)> {
    match &cond.kind {
        ExprKind::FunctionCall { name, args } if args.len() == 1 => {
            let function_key = php_symbol_key(name.trim_start_matches('\\'));
            if function_key == "is_string" {
                if let ExprKind::NullCoalesce { value, .. } = &args[0].kind {
                    if let ExprKind::Assignment {
                        target,
                        prelude,
                        ..
                    } = &value.kind
                    {
                        if prelude.is_empty() && matches!(target.kind, ExprKind::Variable(_)) {
                            return Some((
                                target,
                                GuardTarget::Exact(PhpType::Union(vec![
                                    PhpType::Str,
                                    PhpType::Void,
                                ])),
                                false,
                            ));
                        }
                    }
                }
            }
            // `php_symbol_key` rather than a plain lowercase: it also folds the leading `\` and
            // the namespace qualification, so `\is_int($x)` and `Ns\is_int($x)` narrow too.
            let target = match function_key.as_str() {
                "is_int" | "is_integer" | "is_long" => GuardTarget::Exact(PhpType::Int),
                "is_float" | "is_double" => GuardTarget::Exact(PhpType::Float),
                "is_real" if !crate::strict_php::is_enabled() => GuardTarget::Exact(PhpType::Float),
                "is_string" => GuardTarget::Exact(PhpType::Str),
                "is_bool" => GuardTarget::Exact(PhpType::Bool),
                // `is_null($x)`: same narrowing as `$x === null` — elephc models a `?T` value's
                // null as Void, so the complement strips it (`if (is_null($x)) { throw; }` leaves
                // ?int as int on the fall-through path).
                "is_null" => GuardTarget::Exact(PhpType::Void),
                "is_callable" => GuardTarget::AnyCallable,
                "is_array" => GuardTarget::AnyArray,
                // `is_iterable($x)` proves `array|Traversable`, which is what makes the
                // `is_array()` guard that usually follows it able to leave a Traversable behind.
                "is_iterable" => GuardTarget::AnyIterable,
                "is_object" => GuardTarget::AnyObject,
                // A true result proves exactly the two families accepted by `count()`. Keep
                // unguarded `iterable` strict because it may be a non-Countable Traversable.
                "is_countable" => GuardTarget::Countable,
                "is_numeric" => GuardTarget::Numeric,
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
            let class_name = checker
                .resolve_instanceof_target_name(class, cond.span)
                .ok()?;
            if class_name
                .trim_start_matches('\\')
                .eq_ignore_ascii_case("Closure")
            {
                return Some((
                    value,
                    GuardTarget::Exact(PhpType::Callable),
                    false,
                ));
            }
            let target_is_known = checker.classes.contains_key(&class_name)
                || checker.interfaces.contains_key(&class_name)
                || checker.enums.contains_key(&class_name)
                || checker.declared_traits.contains(&class_name);
            if !target_is_known {
                return Some((value, GuardTarget::UnknownObject, false));
            }
            let narrowed_type = PhpType::Object(class_name);
            Some((
                value,
                GuardTarget::Exact(narrowed_type),
                false,
            ))
        }
        // A true numeric lower-bound comparison proves the receiver is not null: PHP evaluates
        // both `1 < null` and `null > 1` as false. The inverse path remains unconstrained
        // because a false comparison may hold for either null or a smaller numeric value.
        ExprKind::BinaryOp { left, op, right }
            if matches!(&left.kind, ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_))
                && is_guard_receiver_shape(&right.kind)
                && matches!(op, BinOp::Lt | BinOp::LtEq) =>
        {
            Some((right, GuardTarget::NonNull, false))
        }
        ExprKind::BinaryOp { left, op, right }
            if is_guard_receiver_shape(&left.kind)
                && matches!(&right.kind, ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_))
                && matches!(op, BinOp::Gt | BinOp::GtEq) =>
        {
            Some((left, GuardTarget::NonNull, false))
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
            // `guard_receiver_place` rather than an inline `Variable | PropertyAccess` match:
            // it also accepts a static property — which is what lets the singleton shape
            // `if (self::$inst === null) { self::$inst = new S(); }` narrow — and a place bound
            // by the comparison itself (`false === $value = call()`).
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
