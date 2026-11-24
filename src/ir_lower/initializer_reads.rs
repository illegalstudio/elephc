//! Purpose:
//! Tells EIR lowering whether an expression was written inside a declared initializer: a
//! class or interface constant value, an instance or static property default, or a parameter
//! default of a function, method, closure, or arrow function.
//!
//! Called from:
//! - `crate::ir_lower::program::lower()` installs the closure-default registry for one program
//!   lowering.
//! - `crate::ir_lower::expr::closures::lower_closure_with_context()` registers each closure
//!   literal's parameter defaults before its body or any call to it is lowered.
//! - `crate::ir_lower::expr::constants::emit_constant_read_deprecation()` asks whether a
//!   deprecated constant read sits in an initializer.
//! - `crate::ir_lower::function::lower_eval_native_default_helpers()` asks which parameter
//!   defaults reach a deprecated read (directly or through user class constants), so a dynamic
//!   call evaluates them through an EIR helper instead of a value folded at compile time.
//! - `crate::ir_lower::property_default_errors` asks which deprecated reads php-src reports
//!   when it initializes a class (first `new`, static-property access, or reflection).
//!
//! Key details:
//! - php-src evaluates such an initializer once and caches it, while elephc lowers it again at
//!   every use, so a read inside one is recognised by where it was WRITTEN (its span), not by
//!   where it is lowered.
//! - Named declarations come from the lowering context's class, interface, and signature
//!   tables. Closure signatures are built while their literal is lowered and never published
//!   there, so their defaults are recorded here, scoped to one program lowering; a nested
//!   lowering restores the outer registry on drop.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use crate::ir_lower::context::LoweringContext;
use crate::parser::ast::{ArrayEntry, BinOp, CastType, Expr, ExprKind, MagicConstant, StaticReceiver};
use crate::span::Span;

thread_local! {
    /// Parameter defaults of the closure literals lowered so far in the current program.
    static CLOSURE_DEFAULTS: RefCell<Option<Vec<(Expr, bool)>>> = const { RefCell::new(None) };
    /// Trait-collision defaults of the program's class and trait statements, by name.
    static TRAIT_COLLISION_DEFAULTS: RefCell<HashMap<String, Vec<crate::types::TraitCollisionDefault>>> = RefCell::new(HashMap::new());
    /// How the program's class-likes are bound (see `crate::types::ClassBindingFacts`).
    static CLASS_BINDING: RefCell<crate::types::ClassBindingFacts> = RefCell::new(crate::types::ClassBindingFacts::default());
    /// Each trait's own constants in declaration order, by trait name.
    static TRAIT_CONSTANTS: RefCell<HashMap<String, Vec<(String, Expr)>>> = RefCell::new(HashMap::new());
}

/// Publishes each trait's own constants (`names` gives the declaration order) for the current
/// program lowering.
pub(crate) fn set_trait_constants(
    names: &HashMap<String, Vec<String>>,
    constants: &HashMap<String, HashMap<String, Expr>>,
) {
    let ordered = constants
        .iter()
        .map(|(trait_name, values)| {
            let order = names.get(trait_name).cloned().unwrap_or_default();
            let mut list: Vec<(String, Expr)> = order
                .iter()
                .filter_map(|name| values.get(name).map(|value| (name.clone(), value.clone())))
                .collect();
            let mut rest: Vec<(&String, &Expr)> = values.iter().filter(|(name, _)| !order.contains(name)).collect();
            rest.sort_by(|left, right| left.0.cmp(right.0));
            list.extend(rest.into_iter().map(|(name, value)| (name.clone(), value.clone())));
            (trait_name.clone(), list)
        })
        .collect();
    TRAIT_CONSTANTS.with(|slot| slot.replace(ordered));
}

/// Returns trait `trait_name`'s own constants in declaration order.
pub(crate) fn trait_constants(trait_name: &str) -> Vec<(String, Expr)> {
    let trait_name = trait_name.trim_start_matches('\\');
    TRAIT_CONSTANTS.with(|slot| {
        slot.borrow()
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(trait_name))
            .map(|(_, constants)| constants.clone())
            .unwrap_or_default()
    })
}

/// Installs an empty closure-default registry and the program's trait-collision defaults for
/// the duration of the returned scope.
pub(crate) fn install(check_result: &crate::types::CheckResult) -> ClosureDefaultScope {
    let previous = CLOSURE_DEFAULTS.with(|slot| slot.replace(Some(Vec::new())));
    let previous_collisions =
        TRAIT_COLLISION_DEFAULTS.with(|slot| slot.replace(check_result.trait_collision_defaults.clone()));
    let previous_binding = CLASS_BINDING.with(|slot| slot.replace(check_result.class_binding.clone()));
    let previous_trait_constants = TRAIT_CONSTANTS.with(|slot| slot.replace(HashMap::new()));
    ClosureDefaultScope { previous, previous_collisions, previous_trait_constants, previous_binding }
}

/// Reports whether php-src binds class or trait `name` at its statement (see
/// `crate::types::ClassBindingFacts::runtime_bound`).
pub(crate) fn is_runtime_bound(name: &str) -> bool {
    let name = name.trim_start_matches('\\');
    CLASS_BINDING.with(|slot| slot.borrow().runtime_bound.iter().any(|bound| bound.eq_ignore_ascii_case(name)))
}

/// Reports whether class or trait `name` binds elsewhere than at its hoisted declaration (see
/// `crate::types::ClassBindingFacts::bound_elsewhere`).
pub(crate) fn is_bound_elsewhere(name: &str) -> bool {
    let name = name.trim_start_matches('\\');
    CLASS_BINDING.with(|slot| slot.borrow().bound_elsewhere.iter().any(|bound| bound.eq_ignore_ascii_case(name)))
}

/// Returns the trait class-like `holder` copied its imported property `property` from.
pub(crate) fn property_trait_origin(holder: &str, property: &str) -> Option<String> {
    let holder = holder.trim_start_matches('\\');
    CLASS_BINDING.with(|slot| {
        slot.borrow()
            .property_trait_origins
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(holder))
            .and_then(|(_, origins)| origins.get(property).cloned())
    })
}

/// Returns the traits class-like `holder` uses directly, in `use` order.
pub(crate) fn direct_trait_uses(holder: &str) -> Vec<String> {
    let holder = holder.trim_start_matches('\\');
    CLASS_BINDING.with(|slot| {
        slot.borrow()
            .trait_uses
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(holder))
            .map(|(_, uses)| uses.clone())
            .unwrap_or_default()
    })
}

/// Returns the defaults php-src evaluates at trait statement `trait_name`.
pub(crate) fn trait_statement_collision_defaults(trait_name: &str) -> Vec<crate::types::TraitCollisionDefault> {
    let trait_name = trait_name.trim_start_matches('\\');
    TRAIT_COLLISION_DEFAULTS.with(|slot| {
        slot.borrow()
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(trait_name))
            .map(|(_, defaults)| defaults.clone())
            .unwrap_or_default()
    })
}

/// Records the parameter defaults of one closure or arrow-function literal; `scoped` tells
/// whether the closure has a class scope.
pub(crate) fn register_closure_defaults<'a>(defaults: impl IntoIterator<Item = &'a Expr>, scoped: bool) {
    CLOSURE_DEFAULTS.with(|slot| {
        if let Some(registry) = slot.borrow_mut().as_mut() {
            registry.extend(defaults.into_iter().map(|default| (default.clone(), scoped)));
        }
    });
}

/// Restores the previous registries when the enclosing lowering finishes.
pub(crate) struct ClosureDefaultScope {
    /// The closure-default registry that was active before this lowering scope.
    previous: Option<Vec<(Expr, bool)>>,
    /// The trait-collision defaults that were active before this lowering scope.
    previous_collisions: HashMap<String, Vec<crate::types::TraitCollisionDefault>>,
    /// The class binding facts that were active before this lowering scope.
    previous_binding: crate::types::ClassBindingFacts,
    /// The trait constants that were active before this lowering scope.
    previous_trait_constants: HashMap<String, Vec<(String, Expr)>>,
}

impl Drop for ClosureDefaultScope {
    /// Restores the registries that were active before this lowering scope.
    fn drop(&mut self) {
        let previous = self.previous.take();
        CLOSURE_DEFAULTS.with(|slot| {
            slot.replace(previous);
        });
        let previous_collisions = std::mem::take(&mut self.previous_collisions);
        TRAIT_COLLISION_DEFAULTS.with(|slot| {
            slot.replace(previous_collisions);
        });
        let previous_binding = std::mem::take(&mut self.previous_binding);
        CLASS_BINDING.with(|slot| {
            slot.replace(previous_binding);
        });
        let previous_trait_constants = std::mem::take(&mut self.previous_trait_constants);
        TRAIT_CONSTANTS.with(|slot| {
            slot.replace(previous_trait_constants);
        });
    }
}

/// Reports whether `span` is an expression inside a declared initializer.
pub(crate) fn is_declared_initializer_span(ctx: &LoweringContext<'_, '_>, span: Span) -> bool {
    if !span.identifies_a_node() {
        return false;
    }
    let signatures = ctx.functions.values().chain(ctx.classes.values().flat_map(|class| {
        class.methods.values().chain(class.static_methods.values())
    }));
    let named = ctx
        .classes
        .values()
        .flat_map(|class| {
            class
                .constants
                .values()
                .chain(class.defaults.iter().flatten())
                .chain(class.static_defaults.iter().flatten())
        })
        .chain(ctx.interfaces.values().flat_map(|interface| interface.constants.values()))
        .chain(signatures.flat_map(|signature| signature.defaults.iter().flatten()));
    if named.into_iter().any(|initializer| expression_contains_span(initializer, span)) {
        return true;
    }
    CLOSURE_DEFAULTS.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|defaults| defaults.iter().any(|(default, _)| expression_contains_span(default, span)))
    })
}

/// Returns the parameter default (of a function, method, or closure) that contains the read
/// at `span`, and whether its function has a class scope.
pub(crate) fn parameter_default_containing(ctx: &LoweringContext<'_, '_>, span: Span) -> Option<(Expr, bool)> {
    if !span.identifies_a_node() {
        return None;
    }
    let methods = ctx.classes.values().flat_map(|class| class.methods.values().chain(class.static_methods.values()));
    let named = ctx
        .functions
        .values()
        .flat_map(|signature| signature.defaults.iter().flatten().map(|default| (default, false)))
        .chain(methods.flat_map(|signature| signature.defaults.iter().flatten().map(|default| (default, true))));
    for (default, scoped) in named {
        if expression_contains_span(default, span) {
            return Some((default.clone(), scoped));
        }
    }
    CLOSURE_DEFAULTS.with(|slot| {
        slot.borrow().as_ref().and_then(|defaults| {
            defaults.iter().find(|(default, _)| expression_contains_span(default, span)).cloned()
        })
    })
}

/// Returns the line php-src reports for a deprecated read at `span` inside parameter default
/// `root`. `RECV_INIT` runs at the line of the last global constant name the default holds
/// (compilation leaves the line there), or of the default itself. Without a class scope every
/// read reports there; with one, every node reports its own line except a default that is one
/// bare global constant, which takes the fast path.
pub(crate) fn parameter_default_line(root: &Expr, scoped: bool, span: Span) -> Span {
    let lone = matches!(root.kind, ExprKind::ConstRef(_)) && root.span == span;
    if scoped && !lone {
        return span;
    }
    let mut recv_line = root.span;
    visit_constant_reads(root, &mut |read| {
        if matches!(read.kind, ExprKind::ConstRef(_)) {
            recv_line = read.span;
        }
    });
    recv_line
}

/// Reports whether evaluating parameter default `root` (in `scope`) yields a refcounted value,
/// which `RECV_INIT` does not cache, so such a default is evaluated again, and reports again,
/// on every call. A conditional yields the branch its condition selects.
pub(crate) fn parameter_default_is_refcounted(ctx: &LoweringContext<'_, '_>, scope: Option<&str>, root: &Expr) -> bool {
    // php-src never caches a default that constructs an object, whatever its result.
    constructs_object(root) || result_is_refcounted(ctx, scope, root, true)
}

/// Reports whether evaluating `expr` constructs an object (`new`).
fn constructs_object(expr: &Expr) -> bool {
    if matches!(expr.kind, ExprKind::NewObject { .. }) {
        return true;
    }
    let mut found = false;
    visit_direct_children(expr, &mut |child| found = found || constructs_object(child));
    found
}

/// Reports whether `expr` evaluates to a refcounted value in php-src: an object, a non-empty
/// array built at run time or read from a constant, an array cast of anything but an array
/// that is not refcounted, an array `+`, or a string built at run time (a concatenation of
/// two non-empty strings, a string cast of a number that is not a single digit, a constant
/// whose initializer built it). An interned string, an immutable array, and a scalar are not.
/// `folded` tells whether php-src folded `expr` at compile time where it could: it does not
/// fold the branches of a conditional whose condition does not fold (`zend_eval_const_expr`),
/// so a literal-only branch there is still evaluated at run time.
fn result_is_refcounted(ctx: &LoweringContext<'_, '_>, scope: Option<&str>, expr: &Expr, folded: bool) -> bool {
    match &expr.kind {
        ExprKind::ConstRef(_) | ExprKind::ScopedConstantAccess { .. } => return constant_is_refcounted(ctx, scope, expr, 0),
        _ => {}
    }
    if folded && compile_time_value(ctx, scope, expr).is_some_and(|value| is_fully_literal(&value)) {
        return false;
    }
    let refcounted = |inner: &Expr, folded: bool| result_is_refcounted(ctx, scope, inner, folded);
    match &expr.kind {
        ExprKind::ArrayLiteral(elements) => !elements.is_empty(),
        ExprKind::ArrayLiteralAssoc(pairs) => !pairs.is_empty(),
        ExprKind::ArrayLiteralMixed(entries) => !entries.is_empty(),
        ExprKind::NewObject { .. } | ExprKind::Cast { target: CastType::Object, .. } => true,
        ExprKind::BinaryOp { left, op: BinOp::Concat, right } => {
            let left_empty = static_string(ctx, scope, left).is_some_and(|text| text.is_empty());
            let right_empty = static_string(ctx, scope, right).is_some_and(|text| text.is_empty());
            if left_empty {
                string_result_is_refcounted(ctx, scope, right, folded)
            } else if right_empty {
                string_result_is_refcounted(ctx, scope, left, folded)
            } else {
                true
            }
        }
        ExprKind::BinaryOp { left, op: BinOp::Add, right } => {
            is_array_value(ctx, scope, left) || is_array_value(ctx, scope, right)
        }
        ExprKind::Cast { target: CastType::String, expr: inner } => string_result_is_refcounted(ctx, scope, inner, folded),
        ExprKind::Cast { target: CastType::Array, expr: inner } => {
            !is_array_value(ctx, scope, inner) || refcounted(inner, folded)
        }
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            let branches_folded = folded && compile_time_truthiness(ctx, scope, condition).is_some();
            match static_truthiness(ctx, scope, condition) {
                Some(true) => refcounted(then_expr, branches_folded),
                Some(false) => refcounted(else_expr, branches_folded),
                None => refcounted(then_expr, branches_folded) || refcounted(else_expr, branches_folded),
            }
        }
        ExprKind::ShortTernary { value, default } => {
            let branches_folded = folded && compile_time_truthiness(ctx, scope, value).is_some();
            match static_truthiness(ctx, scope, value) {
                Some(true) => refcounted(value, branches_folded),
                Some(false) => refcounted(default, branches_folded),
                None => refcounted(value, branches_folded) || refcounted(default, branches_folded),
            }
        }
        ExprKind::NullCoalesce { value, default } => {
            let branches_folded = folded && compile_time_value(ctx, scope, value).is_some();
            match static_value(ctx, scope, value).map(|value| value.kind) {
                Some(ExprKind::Null) => refcounted(default, branches_folded),
                Some(_) => refcounted(value, branches_folded),
                None => refcounted(value, branches_folded) || refcounted(default, branches_folded),
            }
        }
        _ => false,
    }
}

/// Reports whether `expr` converted to a string is refcounted: a string as
/// [`result_is_refcounted`] says, `"Array"` (interned) for an array, a number's digits unless
/// they are one character (php-src interns single-character strings), and an interned `"1"`
/// or `""` for a bool or null.
fn string_result_is_refcounted(ctx: &LoweringContext<'_, '_>, scope: Option<&str>, expr: &Expr, folded: bool) -> bool {
    match static_value(ctx, scope, expr).map(|value| value.kind) {
        Some(ExprKind::IntLiteral(value)) => !(0..=9).contains(&value),
        Some(ExprKind::FloatLiteral(_)) => true,
        Some(ExprKind::BoolLiteral(_) | ExprKind::Null | ExprKind::ArrayLiteral(_)) => false,
        _ if is_array_value(ctx, scope, expr) => false,
        _ => result_is_refcounted(ctx, scope, expr, folded),
    }
}

/// Reports whether reading constant `expr` yields a refcounted value: an enum case object, a
/// non-empty array, or a string its initializer built at run time. A predefined constant holds
/// an interned or immutable value.
fn constant_is_refcounted(ctx: &LoweringContext<'_, '_>, scope: Option<&str>, expr: &Expr, depth: usize) -> bool {
    if depth >= MAX_CONSTANT_FOLLOW_DEPTH {
        return false;
    }
    let (initializer, owner) = match &expr.kind {
        ExprKind::ConstRef(name) => {
            if elephc_builtin_contract::lookup_constant(name.as_str()).is_some() {
                return false;
            }
            match ctx.constant_value(name.as_str()) {
                Some((value, _)) => (Expr::new(value, expr.span), None),
                None => return false,
            }
        }
        ExprKind::ScopedConstantAccess { receiver, name } => {
            let Some(class) = resolve_receiver(ctx, scope, receiver) else { return false };
            if ctx.enums.get(&class).is_some_and(|info| info.cases.iter().any(|case| case.name == *name)) {
                return true;
            }
            let (Some(owner), Some(value)) = (ctx.scoped_constant_owner(&class, name), ctx.scoped_constant_value(&class, name)) else {
                return false;
            };
            (value, Some(owner))
        }
        _ => return false,
    };
    match &initializer.kind {
        ExprKind::ConstRef(_) | ExprKind::ScopedConstantAccess { .. } => {
            constant_is_refcounted(ctx, owner.as_deref().or(scope), &initializer, depth + 1)
        }
        _ if is_array_value(ctx, owner.as_deref().or(scope), &initializer) => {
            !matches!(static_value(ctx, owner.as_deref().or(scope), &initializer).map(|value| value.kind),
                Some(ExprKind::ArrayLiteral(ref elements)) if elements.is_empty())
        }
        _ => result_is_refcounted(ctx, owner.as_deref().or(scope), &initializer, true),
    }
}

/// Reports whether `expr` is a literal all the way down (an array of literals included).
fn is_fully_literal(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::BoolLiteral(_) | ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_) | ExprKind::StringLiteral(_) | ExprKind::Null => true,
        ExprKind::Negate(inner) => is_fully_literal(inner),
        ExprKind::ArrayLiteral(elements) => elements.iter().all(is_fully_literal),
        ExprKind::ArrayLiteralAssoc(pairs) => pairs.iter().all(|(key, value)| is_fully_literal(key) && is_fully_literal(value)),
        _ => false,
    }
}

/// Returns the string `expr` statically evaluates to, if it is a string literal after folding.
fn static_string(ctx: &LoweringContext<'_, '_>, scope: Option<&str>, expr: &Expr) -> Option<String> {
    match static_value(ctx, scope, expr)?.kind {
        ExprKind::StringLiteral(text) => Some(text),
        _ => None,
    }
}

/// Reports whether `expr` evaluates to an array, judged from its shape and folded value.
fn is_array_value(ctx: &LoweringContext<'_, '_>, scope: Option<&str>, expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) | ExprKind::ArrayLiteralMixed(_) => true,
        ExprKind::Cast { target: CastType::Array, .. } => true,
        ExprKind::BinaryOp { left, op: BinOp::Add, right } => is_array_value(ctx, scope, left) || is_array_value(ctx, scope, right),
        ExprKind::Ternary { then_expr, else_expr, condition } => match static_truthiness(ctx, scope, condition) {
            Some(true) => is_array_value(ctx, scope, then_expr),
            Some(false) => is_array_value(ctx, scope, else_expr),
            None => false,
        },
        _ => matches!(static_value(ctx, scope, expr).map(|value| value.kind), Some(ExprKind::ArrayLiteral(_))),
    }
}

/// Reports whether `span` is the span of `expr` or of one of its subexpressions.
fn expression_contains_span(expr: &Expr, span: Span) -> bool {
    /// Walker pass that records whether one expression span occurs in the visited tree.
    struct SpanFinder {
        /// The read span being looked for.
        target: Span,
        /// Set once the walk has visited an expression with `target` as its span.
        found: Cell<bool>,
    }
    impl crate::magic_constants::walker::Pass for SpanFinder {
        /// Records a visited expression span and keeps it unchanged.
        fn transform_expr_span(&self, span: Span) -> Span {
            if span == self.target {
                self.found.set(true);
            }
            span
        }

        /// Keeps magic constants unchanged; the walk only observes spans.
        fn transform_magic(&self, _span: Span, constant: MagicConstant) -> ExprKind {
            ExprKind::MagicConstant(constant)
        }
    }
    // Only the source is compared up front: a binary node (`??`, `.`) is located at its
    // operator, after its left operand, so its span does not bound its subexpressions.
    if expr.span.source_id() != span.source_id() {
        return false;
    }
    let mut finder = SpanFinder { target: span, found: Cell::new(false) };
    crate::magic_constants::walker::walk_expression(expr.clone(), &mut finder);
    finder.found.get()
}

/// Calls `visit` for every `ConstRef` and `ScopedConstantAccess` inside a constant expression.
/// The shapes walked are the ones PHP accepts in a constant expression.
pub(crate) fn visit_constant_reads(expr: &Expr, visit: &mut dyn FnMut(&Expr)) {
    match &expr.kind {
        ExprKind::ConstRef(_) | ExprKind::ScopedConstantAccess { .. } => visit(expr),
        _ => visit_direct_children(expr, &mut |inner| visit_constant_reads(inner, &mut *visit)),
    }
}

/// Class and interface tables a dynamic-call default is resolved against.
pub(crate) struct ConstantTables<'a> {
    /// Classes by name.
    pub(crate) classes: &'a std::collections::HashMap<String, crate::types::ClassInfo>,
    /// Interfaces by name.
    pub(crate) interfaces: &'a std::collections::HashMap<String, crate::types::InterfaceInfo>,
}

/// Reports whether a default evaluated in `scope` (the class `self::` names, if any) reads a
/// deprecated constant directly or through user class or interface constants, so a dynamic
/// call must evaluate it through an EIR helper instead of a value folded at compile time.
pub(crate) fn default_reaches_deprecated_read(
    tables: &ConstantTables<'_>,
    scope: Option<&str>,
    default: &Expr,
) -> bool {
    reaches_deprecated_read(tables, scope, default, 0)
}

/// Recursive body of [`default_reaches_deprecated_read`]; `depth` bounds constant cycles.
fn reaches_deprecated_read(
    tables: &ConstantTables<'_>,
    scope: Option<&str>,
    expr: &Expr,
    depth: usize,
) -> bool {
    let mut found = false;
    visit_constant_reads(expr, &mut |read| {
        if found {
            return;
        }
        match &read.kind {
            ExprKind::ConstRef(name) => {
                found = elephc_builtin_contract::global_constant_deprecation(name.as_str()).is_some();
            }
            ExprKind::ScopedConstantAccess { receiver, name } => {
                if elephc_builtin_contract::is_deprecated_class_constant_name(name) {
                    found = true;
                    return;
                }
                if depth >= MAX_CONSTANT_FOLLOW_DEPTH {
                    return;
                }
                let receiver_class = match receiver {
                    StaticReceiver::Named(written) => Some(written.as_str().trim_start_matches('\\').to_string()),
                    StaticReceiver::Self_ | StaticReceiver::Static => scope.map(str::to_string),
                    StaticReceiver::Parent => scope
                        .and_then(|scope| tables.classes.get(scope))
                        .and_then(|class| class.parent.clone()),
                    StaticReceiver::Generic(_) => None,
                };
                if let Some((owner, value)) = receiver_class
                    .and_then(|receiver| lookup_constant_value(tables, &receiver, name))
                {
                    found = reaches_deprecated_read(tables, Some(&owner), &value, depth + 1);
                }
            }
            _ => {}
        }
    });
    found
}

/// Finds `receiver::name` the way PHP resolves it: the class and its parents first, then the
/// interfaces they implement and those interfaces' parents, or the receiver itself when it is
/// an interface. Returns the declaring class-like and the initializer.
fn lookup_constant_value(
    tables: &ConstantTables<'_>,
    receiver: &str,
    name: &str,
) -> Option<(String, Expr)> {
    let mut interfaces = Vec::new();
    let mut current = Some(receiver.to_string());
    while let Some(class_name) = current {
        let Some(class) = tables.classes.get(&class_name) else { break };
        if let Some(value) = class.constants.get(name) {
            return Some((class_name, value.clone()));
        }
        interfaces.extend(class.interfaces.iter().cloned());
        current = class.parent.clone();
    }
    interfaces.push(receiver.to_string());
    let mut visited = std::collections::HashSet::new();
    while let Some(interface_name) = interfaces.pop() {
        if !visited.insert(interface_name.clone()) {
            continue;
        }
        let Some(interface) = tables.interfaces.get(&interface_name) else { continue };
        if let Some(value) = interface.constants.get(name) {
            return Some((interface_name, value.clone()));
        }
        interfaces.extend(interface.parents.iter().cloned());
    }
    None
}

/// Returns the deprecation message for reading the global constant `name` under the compile
/// profile.
pub(crate) fn global_read_deprecation(name: &str) -> Option<&'static str> {
    elephc_builtin_contract::global_constant_deprecation(name)
        .filter(|deprecation| deprecation.applies_to(crate::codegen_support::compile_php_version()))
        .map(|deprecation| deprecation.message)
}

/// Returns the deprecation message for reading `class_name::name` under the compile profile,
/// when the class hierarchy resolves the constant to a deprecated builtin declaration.
pub(crate) fn scoped_read_deprecation(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    name: &str,
) -> Option<&'static str> {
    let owner = ctx.scoped_constant_owner(class_name, name)?;
    // A user class reusing a builtin's name declares its own, silent constant.
    if ctx.classes.get(&owner).is_some_and(|info| !info.is_internal) {
        return None;
    }
    let deprecation = elephc_builtin_contract::class_constant_deprecation(&owner, name)?;
    deprecation
        .applies_to(crate::codegen_support::compile_php_version())
        .then_some(deprecation.message)
}

/// One property slot: the class that declared it and its default.
struct PropertySlot<'a> {
    /// Class whose declaration this slot holds; `self::`/`parent::` resolve against it.
    declaring: String,
    /// Property name.
    name: &'a str,
    /// Whether the slot is private, so a descendant's same-named property gets its own slot.
    private: bool,
    /// The declared default, if any.
    default: Option<&'a Expr>,
}

/// Returns the properties `class_name` declares itself, in php-src's initialization order:
/// its own declarations in source order, then the trait imports. Compiler-built classes have
/// no recorded order and keep their slot order.
fn own_property_names<'a>(ctx: &'a LoweringContext<'_, '_>, class_name: &str) -> Vec<&'a str> {
    let Some(class) = ctx.classes.get(class_name) else {
        return Vec::new();
    };
    if !class.property_initialization_order.is_empty() {
        return class.property_initialization_order.iter().map(String::as_str).collect();
    }
    class
        .properties
        .iter()
        .chain(&class.static_properties)
        .map(|(name, _)| name.as_str())
        .collect()
}

/// Returns the instance slots php-src lays out for `class_name`: the parent's slots, a
/// non-private redeclaration replacing the slot it overrides in place, and every other
/// property this class declares appended in initialization order. Each slot keeps the class
/// that declared it.
fn flattened_instance_slots<'a>(
    ctx: &'a LoweringContext<'_, '_>,
    class_name: &str,
) -> Vec<PropertySlot<'a>> {
    let Some(class) = ctx.classes.get(class_name) else {
        return Vec::new();
    };
    let mut slots = class
        .parent
        .as_deref()
        .map(|parent| flattened_instance_slots(ctx, parent.trim_start_matches('\\')))
        .unwrap_or_default();
    for name in own_property_names(ctx, class_name) {
        let declares = class
            .property_declaring_classes
            .get(name)
            .is_some_and(|declaring| declaring.eq_ignore_ascii_case(class_name));
        let Some(index) = class.properties.iter().rposition(|(property, _)| property == name) else {
            continue;
        };
        if !declares {
            continue;
        }
        let private = matches!(
            class.property_visibilities.get(name),
            Some(crate::parser::ast::Visibility::Private)
        );
        let slot = PropertySlot {
            declaring: class_name.to_string(),
            name,
            private,
            default: class.defaults.get(index).and_then(Option::as_ref),
        };
        match (!private)
            .then(|| slots.iter().position(|existing| !existing.private && existing.name == name))
            .flatten()
        {
            Some(position) => slots[position] = slot,
            None => slots.push(slot),
        }
    }
    slots
}

/// Returns the static properties `class_name` declares itself, in initialization order.
fn own_static_slots<'a>(ctx: &'a LoweringContext<'_, '_>, class_name: &str) -> Vec<PropertySlot<'a>> {
    let Some(class) = ctx.classes.get(class_name) else {
        return Vec::new();
    };
    own_property_names(ctx, class_name)
        .into_iter()
        .filter_map(|name| {
            let declares = class
                .static_property_declaring_classes
                .get(name)
                .is_none_or(|declaring| declaring.eq_ignore_ascii_case(class_name));
            let index = class.static_properties.iter().rposition(|(property, _)| property == name)?;
            declares.then(|| PropertySlot {
                declaring: class_name.to_string(),
                name,
                private: matches!(
                    class.static_property_visibilities.get(name),
                    Some(crate::parser::ast::Visibility::Private)
                ),
                default: class.static_defaults.get(index).and_then(Option::as_ref),
            })
        })
        .collect()
}

/// Returns the property defaults php-src evaluates when it initializes `class_name` itself
/// (its ancestors initialize separately, first): its flattened instance slots, inherited and
/// private parent slots included, then the static properties it declares itself. Each default
/// comes with the class whose declaration it belongs to. php-src evaluates the inherited
/// instance defaults again for every class it initializes.
pub(crate) fn class_own_initialization_defaults(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
) -> Vec<InitializationDefault> {
    let class_name = class_name.trim_start_matches('\\');
    let instance = flattened_instance_slots(ctx, class_name).into_iter().map(|slot| (slot, false));
    let statics = own_static_slots(ctx, class_name).into_iter().map(|slot| (slot, true));
    instance
        .chain(statics)
        .filter_map(|(slot, is_static)| {
            slot.default.map(|default| InitializationDefault {
                declaring: slot.declaring.clone(),
                name: slot.name.to_string(),
                is_static,
                default: default.clone(),
            })
        })
        .collect()
}

/// One property default a class initialization evaluates.
pub(crate) struct InitializationDefault {
    /// The class whose declaration holds the default.
    pub(crate) declaring: String,
    /// The property name.
    pub(crate) name: String,
    /// Whether the property is static.
    pub(crate) is_static: bool,
    /// The default expression.
    pub(crate) default: Expr,
}

/// Returns the static defaults `ReflectionClass::getDefaultProperties()` evaluates again on
/// every call, each with the class that declared it (see [`class_static_default_names`]).
pub(crate) fn class_static_default_slots(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
) -> Vec<(String, Expr)> {
    visible_static_slots(ctx, class_name)
        .into_iter()
        .filter_map(|slot| slot.default.map(|default| (slot.declaring, default.clone())))
        .collect()
}

/// Returns the names of the statics visible on `class_name`, in the order php-src lists them:
/// walking from the class toward the root, each class's own statics in initialization order,
/// a name a descendant already covered skipped, and an ancestor's private static left out.
pub(crate) fn class_static_default_names(ctx: &LoweringContext<'_, '_>, class_name: &str) -> Vec<String> {
    visible_static_slots(ctx, class_name).into_iter().map(|slot| slot.name.to_string()).collect()
}

/// Shared body of [`class_static_default_slots`] and [`class_static_default_names`].
fn visible_static_slots<'a>(ctx: &'a LoweringContext<'_, '_>, class_name: &str) -> Vec<PropertySlot<'a>> {
    let class_name = class_name.trim_start_matches('\\');
    let mut chain = class_initialization_chain(ctx, class_name);
    chain.reverse();
    let mut slots: Vec<PropertySlot<'a>> = Vec::new();
    for owner in &chain {
        for slot in own_static_slots(ctx, owner) {
            if slots.iter().any(|seen| seen.name == slot.name)
                || (slot.private && !owner.eq_ignore_ascii_case(class_name))
            {
                continue;
            }
            slots.push(slot);
        }
    }
    slots
}

/// Names and declaration spans of `class_name`'s int enum cases written `MT_RAND_PHP`, which the
/// checker folds to `1` and flags, in declaration order.
pub(crate) fn flagged_enum_cases(ctx: &LoweringContext<'_, '_>, class_name: &str) -> Vec<(String, Span)> {
    let Some(info) = ctx.enums.get(class_name.trim_start_matches('\\')) else { return Vec::new() };
    let mut cases: Vec<(String, Span)> = info
        .cases
        .iter()
        .filter(|case| case.deprecated_mt_rand_php)
        .filter_map(|case| Some((case.name.clone(), case.span?)))
        .collect();
    cases.sort_by_key(|(_, span)| (span.line, span.col));
    cases
}

/// Evaluates `class_name`'s constant table the way php-src does on a class initialization or a
/// reflection `getConstant()`/`getConstants()`: in table order, each constant's first
/// evaluation reporting at `line`. An enum's flagged `MT_RAND_PHP` cases are constants of that
/// table too, so their notices (at the case declaration, once per case) come in declaration
/// order among the enum's own constants, all before the inherited ones (an interface's).
pub(crate) fn emit_constant_table_evaluation(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &str,
    line: crate::ir_lower::initializer_evaluation::LineSource,
) {
    let class_name = class_name.trim_start_matches('\\');
    let mut cases = flagged_enum_cases(ctx, class_name);
    for (owner, name) in class_constant_table(ctx, class_name) {
        let own = owner.eq_ignore_ascii_case(class_name);
        // A case's own slot in the table (the enum's declarations, before imported trait
        // constants) evaluates it; the first inherited constant follows every case.
        if own {
            if let Some(position) = cases.iter().position(|(case, _)| *case == name) {
                let (_, span) = cases.remove(position);
                crate::ir_lower::expr::emit_mt_rand_php_deprecation(ctx, span, span);
                continue;
            }
        } else {
            for (_, span) in std::mem::take(&mut cases) {
                crate::ir_lower::expr::emit_mt_rand_php_deprecation(ctx, span, span);
            }
        }
        emit_class_constant_evaluation(ctx, &owner, &name, line);
    }
    for (_, span) in cases {
        crate::ir_lower::expr::emit_mt_rand_php_deprecation(ctx, span, span);
    }
}

/// Reports whether evaluating `class_name`'s constant table can raise a deprecation.
pub(crate) fn constant_table_reports(ctx: &LoweringContext<'_, '_>, class_name: &str) -> bool {
    !flagged_enum_cases(ctx, class_name).is_empty()
        || class_constant_table(ctx, class_name).iter().any(|(owner, name)| {
            ctx.scoped_constant_value(owner, name).is_some_and(|value| reaches_deprecated_read_in(ctx, Some(owner), &value))
        })
}

/// Returns the class constants in `class_name`'s constant table, in php-src's order, each with
/// the class-like that holds it: the class's own constants (trait constants after them), the
/// parent's whole table, then each implemented interface's table (its own constants, then its
/// parents'). A name appears once, at its first position.
pub(crate) fn class_constant_table(ctx: &LoweringContext<'_, '_>, class_name: &str) -> Vec<(String, String)> {
    let mut table = Vec::new();
    push_class_constant_table(ctx, class_name.trim_start_matches('\\'), &mut table, 0);
    table
}

/// Recursive body of [`class_constant_table`]; `depth` bounds a malformed parent cycle.
fn push_class_constant_table(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    table: &mut Vec<(String, String)>,
    depth: usize,
) {
    let Some(class) = ctx.classes.get(class_name) else {
        push_interface_constant_table(ctx, class_name, table);
        return;
    };
    if depth > ctx.classes.len() {
        return;
    }
    for name in declared_then_remaining(&class.constant_order, class.constants.keys()) {
        push_constant_entry(table, class_name, name);
    }
    if let Some(parent) = class.parent.as_deref() {
        push_class_constant_table(ctx, parent.trim_start_matches('\\'), table, depth + 1);
    }
    for interface in &class.interfaces {
        push_interface_constant_table(ctx, interface, table);
    }
}

/// Appends an interface's constant table: its own constants, then its parents'.
fn push_interface_constant_table(
    ctx: &LoweringContext<'_, '_>,
    interface_name: &str,
    table: &mut Vec<(String, String)>,
) {
    let Some(interface) = ctx.interfaces.get(interface_name) else { return };
    for name in declared_then_remaining(&interface.constant_order, interface.constants.keys()) {
        let owner = interface.constant_declaring_interfaces.get(name).map(String::as_str).unwrap_or(interface_name);
        push_constant_entry(table, owner, name);
    }
}

/// Appends `owner::name` unless the table already holds a constant of that name.
fn push_constant_entry(table: &mut Vec<(String, String)>, owner: &str, name: &str) {
    if !table.iter().any(|(_, existing)| existing == name) {
        table.push((owner.to_string(), name.to_string()));
    }
}

/// Yields the names of `order` first, then any other key in sorted order.
fn declared_then_remaining<'a>(order: &'a [String], keys: impl Iterator<Item = &'a String>) -> Vec<&'a String> {
    let mut remaining: Vec<&String> = keys.filter(|key| !order.contains(key)).collect();
    remaining.sort();
    order.iter().chain(remaining).collect()
}

/// Reports whether `expr`, evaluated in `scope`, reads a deprecated constant directly or
/// through user class or interface constants.
pub(crate) fn reaches_deprecated_read_in(ctx: &LoweringContext<'_, '_>, scope: Option<&str>, expr: &Expr) -> bool {
    let tables = ConstantTables { classes: ctx.classes, interfaces: ctx.interfaces };
    default_reaches_deprecated_read(&tables, scope, expr)
}

/// Emits the deprecations of the first evaluation of class constant `owner::name`, started at
/// `base` (a read, an initialization, a reflection call): php-src evaluates it once, so each
/// read reports behind the constant's once flag.
pub(crate) fn emit_class_constant_evaluation(
    ctx: &mut LoweringContext<'_, '_>,
    owner: &str,
    name: &str,
    base: crate::ir_lower::initializer_evaluation::LineSource,
) {
    let Some(value) = ctx.scoped_constant_value(owner, name) else { return };
    if !reaches_deprecated_read_in(ctx, Some(owner), &value) {
        return;
    }
    let frame = crate::ir_lower::initializer_evaluation::InitializerFrame::top(
        true,
        base,
        lone_global_read(ctx, Some(owner), &value),
        Some(crate::ir_lower::initializer_evaluation::class_constant_once_key(owner, name)),
    );
    emit_initializer_reads(ctx, &value, Some(owner), &frame);
}

/// Emits the deprecations php-src raises while it evaluates `expr` in `scope` (the class
/// `self::` names) as `frame`, without lowering its value. A user class constant it reads is
/// followed into that constant's first evaluation. A conditional evaluates only the branch
/// its condition selects when that condition folds at compile time, both otherwise.
pub(crate) fn emit_initializer_reads(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
    scope: Option<&str>,
    frame: &crate::ir_lower::initializer_evaluation::InitializerFrame,
) {
    walk_initializer_reads(ctx, expr, scope, frame, 0);
}

/// Recursive body of [`emit_initializer_reads`]; `depth` bounds constant cycles.
fn walk_initializer_reads(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
    scope: Option<&str>,
    frame: &crate::ir_lower::initializer_evaluation::InitializerFrame,
    depth: usize,
) {
    use crate::ir_lower::initializer_evaluation::{emit_global_read, emit_node_read, InitializerFrame};
    let walk = |ctx: &mut LoweringContext<'_, '_>, inner: &Expr| {
        walk_initializer_reads(ctx, inner, scope, frame, depth);
    };
    match &expr.kind {
        ExprKind::ConstRef(name) => {
            if let Some(message) = global_read_deprecation(name.as_str()) {
                emit_global_read(ctx, frame, message, expr.span);
            }
        }
        ExprKind::ScopedConstantAccess { receiver, name } => {
            let Some(receiver_class) = resolve_receiver(ctx, scope, receiver) else { return };
            if let Some(message) = scoped_read_deprecation(ctx, &receiver_class, name) {
                emit_node_read(ctx, frame, message, expr.span);
                return;
            }
            if depth >= MAX_CONSTANT_FOLLOW_DEPTH {
                return;
            }
            let Some(owner) = ctx.scoped_constant_owner(&receiver_class, name) else { return };
            let Some(value) = ctx.scoped_constant_value(&receiver_class, name) else { return };
            let lone = lone_global_read(ctx, Some(&owner), &value);
            let nested = InitializerFrame::class_constant(Some(frame), expr.span, &owner, name, lone);
            walk_initializer_reads(ctx, &value, Some(&owner), &nested, depth + 1);
        }
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            walk(ctx, condition);
            match static_truthiness(ctx, scope, condition) {
                Some(true) => walk(ctx, then_expr),
                Some(false) => walk(ctx, else_expr),
                None => walk_runtime_branch(ctx, condition, scope, |ctx| walk(ctx, then_expr), |ctx| walk(ctx, else_expr)),
            }
        }
        ExprKind::ShortTernary { value, default } => {
            walk(ctx, value);
            match static_truthiness(ctx, scope, value) {
                Some(true) => {}
                Some(false) => walk(ctx, default),
                None => walk_runtime_branch(ctx, value, scope, |_| {}, |ctx| walk(ctx, default)),
            }
        }
        ExprKind::NullCoalesce { value, default } => {
            walk(ctx, value);
            match static_value(ctx, scope, value).map(|value| matches!(value.kind, ExprKind::Null)) {
                Some(false) => {}
                Some(true) => walk(ctx, default),
                None => {
                    // `walk_runtime_branch` probes the condition quietly, as `??` does.
                    let not_null = Expr::new(
                        ExprKind::BinaryOp { left: value.clone(), op: BinOp::StrictNotEq, right: Box::new(Expr::new(ExprKind::Null, value.span)) },
                        value.span,
                    );
                    walk_runtime_branch(ctx, &not_null, scope, |_| {}, |ctx| walk(ctx, default));
                }
            }
        }
        ExprKind::BinaryOp { left, op: op @ (BinOp::And | BinOp::Or), right } => {
            walk(ctx, left);
            match static_truthiness(ctx, scope, left) {
                Some(truth) if truth == matches!(op, BinOp::Or) => {}
                Some(_) => walk(ctx, right),
                // `&&` evaluates the right operand when the left is truthy, `||` when falsy.
                None if matches!(op, BinOp::And) => walk_runtime_branch(ctx, left, scope, |ctx| walk(ctx, right), |_| {}),
                None => walk_runtime_branch(ctx, left, scope, |_| {}, |ctx| walk(ctx, right)),
            }
        }
        _ => {
            let mut children = Vec::new();
            visit_direct_children(expr, &mut |child| children.push(child.clone()));
            for child in &children {
                walk(ctx, child);
            }
        }
    }
}

/// Walks `when_true` or `when_false` on the branch `condition` takes at run time, for a
/// condition compile-time folding cannot decide: php-src evaluates only the branch it takes,
/// so only that branch's reads report. The condition's own reads were walked already, so it
/// is evaluated here with their deprecations suppressed, after its class constants are
/// replaced by their values (a `self::` read resolves against `scope`, not the code lowering
/// this walk). The value lowering evaluates the condition for real and raises its warnings, so
/// this probe evaluates it under `?? null`, which reads a missing key quietly; `null` is falsy,
/// so the branch taken does not change.
fn walk_runtime_branch(
    ctx: &mut LoweringContext<'_, '_>,
    condition: &Expr,
    scope: Option<&str>,
    when_true: impl FnOnce(&mut LoweringContext<'_, '_>),
    when_false: impl FnOnce(&mut LoweringContext<'_, '_>),
) {
    let condition = quiet_probe(substitute_constants(ctx, scope, condition, 0, true));
    let then_block = ctx.builder.create_named_block("initializer.branch.then", Vec::new());
    let else_block = ctx.builder.create_named_block("initializer.branch.else", Vec::new());
    let after = ctx.builder.create_named_block("initializer.branch.after", Vec::new());
    crate::ir_lower::expr::with_constant_deprecations_suppressed(|| {
        crate::ir_lower::expr::lower_condition(ctx, &condition, then_block, else_block);
    });
    ctx.builder.position_at_end(then_block);
    when_true(ctx);
    ctx.builder.terminate(crate::ir::Terminator::Br { target: after, args: Vec::new() });
    ctx.builder.position_at_end(else_block);
    when_false(ctx);
    ctx.builder.terminate(crate::ir::Terminator::Br { target: after, args: Vec::new() });
    ctx.builder.position_at_end(after);
}

/// Rewrites `expr` so the dimension reads in it probe quietly (`read ?? null`), keeping its value
/// otherwise.
fn quiet_probe(expr: Expr) -> Expr {
    let span = expr.span;
    let quiet = |inner: Box<Expr>| Box::new(quiet_probe(*inner));
    let kind = match expr.kind {
        ExprKind::ArrayAccess { .. } => {
            return Expr::new(ExprKind::NullCoalesce { value: Box::new(expr), default: Box::new(Expr::new(ExprKind::Null, span)) }, span);
        }
        ExprKind::BinaryOp { left, op, right } => ExprKind::BinaryOp { left: quiet(left), op, right: quiet(right) },
        ExprKind::Not(inner) => ExprKind::Not(quiet(inner)),
        ExprKind::Negate(inner) => ExprKind::Negate(quiet(inner)),
        ExprKind::BitNot(inner) => ExprKind::BitNot(quiet(inner)),
        ExprKind::Cast { target, expr: inner } => ExprKind::Cast { target, expr: quiet(inner) },
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            ExprKind::Ternary { condition: quiet(condition), then_expr: quiet(then_expr), else_expr: quiet(else_expr) }
        }
        ExprKind::ShortTernary { value, default } => ExprKind::ShortTernary { value: quiet(value), default: quiet(default) },
        other => other,
    };
    Expr::new(kind, span)
}

/// Calls `visit` for the direct children of `expr` in evaluation order.
fn visit_direct_children(expr: &Expr, visit: &mut dyn FnMut(&Expr)) {
    match &expr.kind {
        ExprKind::BinaryOp { left, right, .. } => {
            visit(left);
            visit(right);
        }
        ExprKind::NullCoalesce { value, default } | ExprKind::ShortTernary { value, default } => {
            visit(value);
            visit(default);
        }
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            visit(condition);
            visit(then_expr);
            visit(else_expr);
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Spread(inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::NamedArg { value: inner, .. } => visit(inner),
        ExprKind::ArrayAccess { array, index } => {
            visit(array);
            visit(index);
        }
        ExprKind::ArrayLiteral(elements) | ExprKind::NewObject { args: elements, .. } => {
            elements.iter().for_each(|element| visit(element));
        }
        ExprKind::ArrayLiteralAssoc(pairs) => {
            for (key, value) in pairs {
                visit(key);
                visit(value);
            }
        }
        ExprKind::ArrayLiteralMixed(entries) => {
            for entry in entries {
                match entry {
                    ArrayEntry::Spread(value) | ArrayEntry::Value(value) | ArrayEntry::Reference(value) => visit(value),
                    ArrayEntry::Keyed(key, value) | ArrayEntry::KeyReference(key, value) => {
                        visit(key);
                        visit(value);
                    }
                }
            }
        }
        ExprKind::Match { subject, arms, default } => {
            visit(subject);
            for (conditions, value) in arms {
                conditions.iter().for_each(|condition| visit(condition));
                visit(value);
            }
            if let Some(default) = default {
                visit(default);
            }
        }
        _ => {}
    }
}

/// Returns the span of the bare global constant `expr` reduces to once php-src folds it at
/// compile time (a conditional whose condition is a compile-time constant keeps one branch),
/// if it reduces to one. Such an initializer takes php-src's fast path, which reports at the
/// enclosing line instead of the constant's own.
pub(crate) fn lone_global_read(ctx: &LoweringContext<'_, '_>, scope: Option<&str>, expr: &Expr) -> Option<Span> {
    match &expr.kind {
        ExprKind::ConstRef(_) => Some(expr.span),
        ExprKind::Ternary { condition, then_expr, else_expr } => match compile_time_truthiness(ctx, scope, condition)? {
            true => lone_global_read(ctx, scope, then_expr),
            false => lone_global_read(ctx, scope, else_expr),
        },
        ExprKind::ShortTernary { value, default } => {
            (compile_time_truthiness(ctx, scope, value)? == false).then(|| lone_global_read(ctx, scope, default)).flatten()
        }
        ExprKind::NullCoalesce { value, default } => compile_time_value(ctx, scope, value)
            .filter(|value| matches!(value.kind, ExprKind::Null))
            .and_then(|_| lone_global_read(ctx, scope, default)),
        _ => None,
    }
}

/// Returns the spans of the deprecated reads a constant initializer holds itself (global
/// constants and deprecated builtin class constants), not those of the user class constants it
/// reads, which keep their own once flags.
pub(crate) fn direct_read_spans(expr: &Expr) -> Vec<Span> {
    let mut spans = Vec::new();
    visit_constant_reads(expr, &mut |read| match &read.kind {
        ExprKind::ConstRef(name) if elephc_builtin_contract::global_constant_deprecation(name.as_str()).is_some() => {
            spans.push(read.span);
        }
        ExprKind::ScopedConstantAccess { name, .. } if elephc_builtin_contract::is_deprecated_class_constant_name(name) => {
            spans.push(read.span);
        }
        _ => {}
    });
    spans
}

/// Resolves the class a scoped constant receiver names inside an initializer of `scope`.
fn resolve_receiver(ctx: &LoweringContext<'_, '_>, scope: Option<&str>, receiver: &StaticReceiver) -> Option<String> {
    match receiver {
        StaticReceiver::Named(written) => Some(written.as_str().trim_start_matches('\\').to_string()),
        StaticReceiver::Self_ | StaticReceiver::Static => scope.map(str::to_string),
        StaticReceiver::Parent => scope
            .and_then(|scope| ctx.classes.get(scope))
            .and_then(|class| class.parent.clone()),
        StaticReceiver::Generic(_) => None,
    }
}

/// Returns PHP's truthiness of `expr` when it folds to a literal at compile time, reading
/// every constant's value (what the expression yields at run time).
fn static_truthiness(ctx: &LoweringContext<'_, '_>, scope: Option<&str>, expr: &Expr) -> Option<bool> {
    literal_truthiness(static_value(ctx, scope, expr)?)
}

/// [`static_truthiness`] as php-src's compiler folds it: a deprecated constant is never
/// substituted at compile time.
fn compile_time_truthiness(ctx: &LoweringContext<'_, '_>, scope: Option<&str>, expr: &Expr) -> Option<bool> {
    literal_truthiness(compile_time_value(ctx, scope, expr)?)
}

/// Returns PHP's truthiness of a folded literal.
fn literal_truthiness(value: Expr) -> Option<bool> {
    match value.kind {
        ExprKind::BoolLiteral(value) => Some(value),
        ExprKind::IntLiteral(value) => Some(value != 0),
        ExprKind::FloatLiteral(value) => Some(value != 0.0),
        ExprKind::StringLiteral(value) => Some(!(value.is_empty() || value == "0")),
        ExprKind::Null => Some(false),
        ExprKind::ArrayLiteral(elements) => Some(!elements.is_empty()),
        _ => None,
    }
}

/// Folds `expr` to a scalar literal or `null` at compile time, substituting constants.
fn static_value(ctx: &LoweringContext<'_, '_>, scope: Option<&str>, expr: &Expr) -> Option<Expr> {
    folded_literal(substitute_constants(ctx, scope, expr, 0, true))
}

/// [`static_value`] without substituting deprecated constants, as php-src's compiler folds.
fn compile_time_value(ctx: &LoweringContext<'_, '_>, scope: Option<&str>, expr: &Expr) -> Option<Expr> {
    folded_literal(substitute_constants(ctx, scope, expr, 0, false))
}

/// Folds an expression whose constants were substituted, keeping it only as a literal.
fn folded_literal(expr: Expr) -> Option<Expr> {
    let folded = crate::optimize::fold_constant_expression(expr);
    matches!(
        folded.kind,
        ExprKind::BoolLiteral(_)
            | ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::StringLiteral(_)
            | ExprKind::Null
            | ExprKind::ArrayLiteral(_)
    )
    .then_some(folded)
}

/// Returns `expr` with the constants it reads (user class constants, `define()`d or `const`
/// constants, predefined ones) replaced by their values, for compile-time folding. A deprecated
/// constant is substituted only when `deprecated` is set.
fn substitute_constants(
    ctx: &LoweringContext<'_, '_>,
    scope: Option<&str>,
    expr: &Expr,
    depth: usize,
    deprecated: bool,
) -> Expr {
    let sub = |inner: &Expr| Box::new(substitute_constants(ctx, scope, inner, depth, deprecated));
    let kind = match &expr.kind {
        ExprKind::ConstRef(name)
            if !deprecated && elephc_builtin_contract::global_constant_deprecation(name.as_str()).is_some() =>
        {
            return expr.clone();
        }
        // A predefined constant's catalog value comes first: the lowering context may hold it
        // as an expression that does not fold.
        ExprKind::ConstRef(name) => match elephc_builtin_contract::lookup_constant(name.as_str()).map(|constant| &constant.value) {
            Some(elephc_builtin_contract::ConstValue::Int(value)) => ExprKind::IntLiteral(*value),
            Some(elephc_builtin_contract::ConstValue::Float(value)) => ExprKind::FloatLiteral(*value),
            Some(elephc_builtin_contract::ConstValue::Str(value)) => ExprKind::StringLiteral((*value).to_string()),
            Some(elephc_builtin_contract::ConstValue::Bool(value)) => ExprKind::BoolLiteral(*value),
            Some(elephc_builtin_contract::ConstValue::Null) => ExprKind::Null,
            _ => match ctx.constant_value(name.as_str()) {
                Some((value, _)) => value,
                None => return expr.clone(),
            },
        },
        ExprKind::ScopedConstantAccess { receiver, name } if depth < MAX_CONSTANT_FOLLOW_DEPTH => {
            let resolved = resolve_receiver(ctx, scope, receiver).and_then(|receiver| {
                let owner = ctx.scoped_constant_owner(&receiver, name)?;
                let value = ctx.scoped_constant_value(&receiver, name)?;
                Some(substitute_constants(ctx, Some(&owner), &value, depth + 1, deprecated))
            });
            return resolved.unwrap_or_else(|| expr.clone());
        }
        ExprKind::BinaryOp { left, op, right } => ExprKind::BinaryOp { left: sub(left), op: op.clone(), right: sub(right) },
        ExprKind::Not(inner) => ExprKind::Not(sub(inner)),
        ExprKind::Negate(inner) => ExprKind::Negate(sub(inner)),
        ExprKind::BitNot(inner) => ExprKind::BitNot(sub(inner)),
        ExprKind::Ternary { condition, then_expr, else_expr } => ExprKind::Ternary {
            condition: sub(condition),
            then_expr: sub(then_expr),
            else_expr: sub(else_expr),
        },
        ExprKind::ShortTernary { value, default } => ExprKind::ShortTernary { value: sub(value), default: sub(default) },
        ExprKind::NullCoalesce { value, default } => ExprKind::NullCoalesce { value: sub(value), default: sub(default) },
        // php-src's compiler folds a constant array and a dimension read of one; a cast folds
        // only when the value is computed at run time (`deprecated`).
        ExprKind::ArrayAccess { array, index } => ExprKind::ArrayAccess { array: sub(array), index: sub(index) },
        ExprKind::ArrayLiteral(elements) => ExprKind::ArrayLiteral(elements.iter().map(|element| *sub(element)).collect()),
        ExprKind::ArrayLiteralAssoc(pairs) => {
            ExprKind::ArrayLiteralAssoc(pairs.iter().map(|(key, value)| (*sub(key), *sub(value))).collect())
        }
        ExprKind::Cast { target, expr: inner } if deprecated => ExprKind::Cast { target: target.clone(), expr: sub(inner) },
        _ => return expr.clone(),
    };
    Expr::new(kind, expr.span)
}

/// Bound on following class constants that read other class constants.
const MAX_CONSTANT_FOLLOW_DEPTH: usize = 16;

/// Returns `class_name` and its ancestors in the order php-src initializes them: root first.
pub(crate) fn class_initialization_chain(ctx: &LoweringContext<'_, '_>, class_name: &str) -> Vec<String> {
    let mut chain = Vec::new();
    let mut current = Some(class_name.trim_start_matches('\\').to_string());
    while let Some(name) = current {
        if chain.contains(&name) {
            break;
        }
        current = ctx.classes.get(&name).and_then(|class| class.parent.clone());
        chain.push(name);
    }
    chain.reverse();
    chain
}
