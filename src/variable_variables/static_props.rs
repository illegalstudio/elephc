//! Purpose:
//! Desugars static properties named at run time (`self::$$name`, `static::${'x' . $y}`,
//! `Foo::${$p}[$k]`) into dispatches over the statically known static properties of the
//! receiver class, so later passes only see ordinary `C::$prop` reads and writes.
//!
//! Called from:
//! - `crate::variable_variables::desugar()`, before the variable-variable rewrite.
//! - `crate::parser::variable_variables::parse_dynamic_static_property()` builds the access
//!   shape through [`dynamic_static_access`].
//!
//! Key details:
//! - Parser shape: `ArrayAccess { array: StaticPropertyAccess { receiver, MARKER }, index: name }`.
//!   The marker cannot be spelled in PHP (`#`), and the array-access shape lets every postfix,
//!   assignment and increment form parse exactly as for `C::$name`.
//! - Candidates are the static properties the receiver class can reach, collected from the AST:
//!   its own (any visibility), its traits', and its ancestors' non-private ones. `static::` uses
//!   the lexical class's set, so a property only a subclass declares is reported as undeclared.
//! - Reads, `isset`, `empty` and `??` are expression-level `match ((string) name)` dispatches,
//!   each arm boxed through the variable-variable mixed helper. Writes (`=`, compound, `??=`,
//!   `++`, element writes, `unset`, `=&`) are statement-level: the name is evaluated once into a
//!   temporary and an `if`/`elseif` chain runs the statement against the matching property.
//! - An unknown name throws PHP's `Error: Access to undeclared static property C::$name`; under
//!   `isset`/`empty`/`??` it is simply unset, as in PHP.
//! - An arm whose property cannot take the access (an element write on an `int` property, a
//!   whole-value write on a typed property) is not emitted as code the checker would refuse: it
//!   throws an explicit `Error` naming the elephc limit instead. Nothing is ever silently skipped.

use std::collections::HashMap;

use crate::errors::CompileError;
use crate::parser::ast::{
    BinOp, ClassProperty, Expr, ExprKind, StaticReceiver, Stmt, StmtKind, TypeExpr, Visibility,
};
use crate::span::Span;

use super::walk::{walk_expr_children, walk_stmt_children, Visitor};
use super::{binop, box_mixed, cast_string, str_lit, throw_error, var};

/// Unforgeable static-property name standing for "the property named by the index".
pub const DYNAMIC_STATIC_PROPERTY: &str = "__elephc_dynamic_static_property#gen";

/// Set once the parser has built a dynamic static access, so programs without one skip the pass.
static SEEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Builds the parser shape for `receiver::${name}`.
pub(crate) fn dynamic_static_access(receiver: StaticReceiver, name: Expr, span: Span) -> Expr {
    SEEN.store(true, std::sync::atomic::Ordering::Relaxed);
    Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(Expr::new(
                ExprKind::StaticPropertyAccess {
                    receiver,
                    property: DYNAMIC_STATIC_PROPERTY.to_string(),
                },
                span,
            )),
            index: Box::new(name),
        },
        span,
    )
}

/// Whether `expr` is a place rooted at a dynamic static-property access (`C::${n}`, `C::${n}[k]`).
pub(crate) fn is_dynamic_static_place(expr: &Expr) -> bool {
    place_root(expr).is_some()
}

/// Returns `(receiver, name)` when `expr` is exactly a dynamic static-property access.
fn dynamic_root(expr: &Expr) -> Option<(&StaticReceiver, &Expr)> {
    let ExprKind::ArrayAccess { array, index } = &expr.kind else {
        return None;
    };
    match &array.kind {
        ExprKind::StaticPropertyAccess { receiver, property }
            if property == DYNAMIC_STATIC_PROPERTY =>
        {
            Some((receiver, index))
        }
        _ => None,
    }
}

/// Returns the dynamic access at the root of a place chain (`C::${n}`, `C::${n}[k]->p`, ...).
fn place_root(expr: &Expr) -> Option<&Expr> {
    if dynamic_root(expr).is_some() {
        return Some(expr);
    }
    match &expr.kind {
        ExprKind::ArrayAccess { array: inner, .. }
        | ExprKind::PropertyAccess { object: inner, .. }
        | ExprKind::DynamicPropertyAccess { object: inner, .. }
        | ExprKind::NullsafePropertyAccess { object: inner, .. }
        | ExprKind::NullsafeDynamicPropertyAccess { object: inner, .. } => place_root(inner),
        _ => None,
    }
}

/// Finds the first dynamic access anywhere in `expr`.
fn find_root_in_expr(expr: &Expr) -> Option<Expr> {
    struct Finder(Option<Expr>);
    impl Visitor for Finder {
        fn stmt(&mut self, stmt: &mut Stmt) {
            if self.0.is_none() {
                walk_stmt_children(self, stmt);
            }
        }
        fn expr(&mut self, expr: &mut Expr) {
            if self.0.is_some() {
                return;
            }
            if dynamic_root(expr).is_some() {
                self.0 = Some(expr.clone());
                return;
            }
            walk_expr_children(self, expr);
        }
    }
    let mut finder = Finder(None);
    let mut copy = expr.clone();
    finder.expr(&mut copy);
    finder.0
}

/// Replaces every occurrence of the dynamic access `root` in `expr` by `replacement`.
fn replace_root_in_expr(expr: &mut Expr, root: &Expr, replacement: &Expr) {
    struct Replacer<'a> {
        root: &'a Expr,
        replacement: &'a Expr,
    }
    impl Visitor for Replacer<'_> {
        fn stmt(&mut self, stmt: &mut Stmt) {
            walk_stmt_children(self, stmt);
        }
        fn expr(&mut self, expr: &mut Expr) {
            if dynamic_root(expr).is_some() && expr == self.root {
                let span = expr.span;
                *expr = self.replacement.clone();
                expr.span = span;
                return;
            }
            walk_expr_children(self, expr);
        }
    }
    Replacer { root, replacement }.expr(expr);
}

/// Replaces every occurrence of `root` in the statement's own expressions.
fn replace_root_in_stmt(stmt: &mut Stmt, root: &Expr, replacement: &Expr) {
    struct Replacer<'a> {
        root: &'a Expr,
        replacement: &'a Expr,
    }
    impl Visitor for Replacer<'_> {
        fn stmt(&mut self, stmt: &mut Stmt) {
            walk_stmt_children(self, stmt);
        }
        fn expr(&mut self, expr: &mut Expr) {
            replace_root_in_expr(expr, self.root, self.replacement);
        }
    }
    Replacer { root, replacement }.stmt(stmt);
}

/// One static property as the AST declares it.
#[derive(Clone)]
struct StaticProp {
    name: String,
    visibility: Visibility,
    type_expr: Option<TypeExpr>,
    default: Option<Expr>,
}

/// The class-like facts the dispatch needs.
#[derive(Default, Clone)]
struct ClassFacts {
    name: String,
    parent: Option<String>,
    traits: Vec<String>,
    statics: Vec<StaticProp>,
}

/// Normalizes a class name for lookup.
fn class_key(name: &str) -> String {
    name.trim_start_matches('\\').to_ascii_lowercase()
}

/// Collects every class and trait declaration's static properties, recursively.
fn collect_classes(stmts: &[Stmt], out: &mut HashMap<String, ClassFacts>) {
    fn statics(properties: &[ClassProperty]) -> Vec<StaticProp> {
        properties
            .iter()
            .filter(|p| p.is_static)
            .map(|p| StaticProp {
                name: p.name.clone(),
                visibility: p.visibility.clone(),
                type_expr: p.type_expr.clone(),
                default: p.default.clone(),
            })
            .collect()
    }
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::ClassDecl { name, extends, trait_uses, properties, .. } => {
                out.insert(
                    class_key(name),
                    ClassFacts {
                        name: name.clone(),
                        parent: extends.as_ref().map(|n| n.as_str().to_string()),
                        traits: trait_uses
                            .iter()
                            .flat_map(|u| u.trait_names.iter().map(|n| n.as_str().to_string()))
                            .collect(),
                        statics: statics(properties),
                    },
                );
            }
            StmtKind::TraitDecl { name, trait_uses, properties, .. } => {
                out.insert(
                    class_key(name),
                    ClassFacts {
                        name: name.clone(),
                        parent: None,
                        traits: trait_uses
                            .iter()
                            .flat_map(|u| u.trait_names.iter().map(|n| n.as_str().to_string()))
                            .collect(),
                        statics: statics(properties),
                    },
                );
            }
            StmtKind::Synthetic(body) => collect_classes(body, out),
            _ => {}
        }
    }
}

/// Whether a property's declared type and default make an element write/read on it valid.
fn is_array_capable(prop: &StaticProp) -> bool {
    match &prop.type_expr {
        Some(TypeExpr::Named(name)) => {
            let n = name.as_str().trim_start_matches('\\').to_ascii_lowercase();
            n == "array" || n == "mixed"
        }
        Some(_) => false,
        None => match prop.default.as_ref().map(|d| &d.kind) {
            None | Some(ExprKind::Null) => true,
            Some(
                ExprKind::ArrayLiteral(_)
                | ExprKind::ArrayLiteralAssoc(_)
                | ExprKind::ArrayLiteralMixed(_),
            ) => true,
            Some(_) => false,
        },
    }
}

/// Whether the property always holds an array: declared `array`, or untyped with an array literal
/// default.
fn is_array_valued(prop: &StaticProp) -> bool {
    match &prop.type_expr {
        Some(TypeExpr::Named(name)) => {
            name.as_str().trim_start_matches('\\').eq_ignore_ascii_case("array")
        }
        Some(_) => false,
        None => matches!(
            prop.default.as_ref().map(|d| &d.kind),
            Some(
                ExprKind::ArrayLiteral(_)
                    | ExprKind::ArrayLiteralAssoc(_)
                    | ExprKind::ArrayLiteralMixed(_)
            )
        ),
    }
}

/// Whether a whole-value write of an arbitrary value is valid for the property.
///
/// An untyped property whose default is an array literal is typed as an array by the checker,
/// so writing a scalar through a dispatch arm (`C::${$n}++`, `.= 'x'`) would fail to compile in
/// an arm the program may never take; that arm throws the explicit elephc `Error` instead.
fn accepts_any_value(prop: &StaticProp) -> bool {
    match &prop.type_expr {
        None => !is_array_valued(prop),
        Some(TypeExpr::Named(name)) => {
            name.as_str().trim_start_matches('\\').eq_ignore_ascii_case("mixed")
        }
        Some(_) => false,
    }
}

/// What a dispatch does with the matched property.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Access {
    /// Plain read, `isset`, `empty`, `??` on the whole property, or a reference to it.
    Whole,
    /// Read or write through an element / member of the property.
    Element,
    /// A write of a new value to the whole property.
    WholeWrite,
    /// `$r = &C::${n}`: binds a reference to the whole property.
    Reference,
}

impl Access {
    fn admits(self, prop: &StaticProp) -> bool {
        match self {
            Access::Whole => true,
            Access::Element => is_array_capable(prop),
            Access::WholeWrite => accepts_any_value(prop),
            Access::Reference => true,
        }
    }
}

/// The rewriter's state.
pub(super) struct StaticPropsRewriter {
    classes: HashMap<String, ClassFacts>,
    current_class: Option<String>,
    counter: usize,
    pub(super) used_box: bool,
    pub(super) errors: Vec<CompileError>,
}

impl StaticPropsRewriter {
    /// Static properties reachable through `receiver` from the current class.
    #[allow(clippy::type_complexity)]
    fn candidates(
        &self,
        receiver: &StaticReceiver,
    ) -> Result<(String, Vec<StaticProp>, Vec<(String, Visibility)>), String> {
        let current = self.current_class.clone();
        let (class, own_scope) = match receiver {
            StaticReceiver::Self_ | StaticReceiver::Static => match &current {
                Some(c) => (c.clone(), true),
                None => return Err("outside a class".to_string()),
            },
            StaticReceiver::Parent => {
                let Some(c) = &current else {
                    return Err("outside a class".to_string());
                };
                match self.classes.get(&class_key(c)).and_then(|f| f.parent.clone()) {
                    Some(p) => (p, false),
                    None => return Err("in a class without a parent".to_string()),
                }
            }
            StaticReceiver::Named(name) => {
                let name = name.as_str().to_string();
                let own = current.as_deref().is_some_and(|c| class_key(c) == class_key(&name));
                (name, own)
            }
            StaticReceiver::Generic(_) => return Err("on a generic receiver".to_string()),
        };
        if !self.classes.contains_key(&class_key(&class)) {
            return Err(format!("on class `{class}`, whose declaration is not visible to elephc"));
        }
        let mut out: Vec<StaticProp> = Vec::new();
        let mut hidden: Vec<(String, Visibility)> = Vec::new();
        let mut visit = Some(class.clone());
        let mut first = true;
        let mut guard = 0;
        while let Some(name) = visit.take() {
            guard += 1;
            if guard > 64 {
                break;
            }
            let Some(facts) = self.classes.get(&class_key(&name)) else {
                break;
            };
            let mut level = facts.statics.clone();
            let mut trait_stack = facts.traits.clone();
            let mut trait_guard = 0;
            while let Some(t) = trait_stack.pop() {
                trait_guard += 1;
                if trait_guard > 64 {
                    break;
                }
                if let Some(tf) = self.classes.get(&class_key(&t)) {
                    level.extend(tf.statics.iter().cloned());
                    trait_stack.extend(tf.traits.iter().cloned());
                }
            }
            for prop in level {
                let visible = if first {
                    own_scope || prop.visibility == Visibility::Public
                } else {
                    prop.visibility != Visibility::Private
                        && (own_scope || prop.visibility == Visibility::Public)
                };
                if visible && !out.iter().any(|p| p.name == prop.name) {
                    out.push(prop);
                } else if !visible
                    && !out.iter().any(|p| p.name == prop.name)
                    && !hidden.iter().any(|(name, _)| name == &prop.name)
                {
                    hidden.push((prop.name, prop.visibility));
                }
            }
            first = false;
            visit = facts.parent.clone();
        }
        hidden.retain(|(name, _)| !out.iter().any(|p| &p.name == name));
        if matches!(receiver, StaticReceiver::Static) {
            if let Some((sub, prop)) = self.subclass_only_static(&class, &out, &hidden) {
                return Err(format!(
                    "through `static::` while subclass `{}` declares static property `${}`, which late static binding could reach (elephc dispatches on the lexical class `{}` only); name the class explicitly or declare the property in `{}`",
                    sub.trim_start_matches('\\'),
                    prop,
                    class.trim_start_matches('\\'),
                    class.trim_start_matches('\\')
                ));
            }
        }
        Ok((class, out, hidden))
    }

    /// Finds a static property a known subclass of `class` declares (itself or through its
    /// traits) that the lexical candidate set does not hold: `static::${$n}` called on that
    /// subclass would reach it in PHP.
    fn subclass_only_static(
        &self,
        class: &str,
        out: &[StaticProp],
        hidden: &[(String, Visibility)],
    ) -> Option<(String, String)> {
        let target = class_key(class);
        let mut names: Vec<&String> = self.classes.keys().collect();
        names.sort();
        for key in names {
            if *key == target {
                continue;
            }
            // Is `key` a descendant of `class`?
            let mut parent = self.classes.get(key).and_then(|f| f.parent.clone());
            let mut guard = 0;
            let mut descends = false;
            while let Some(p) = parent {
                guard += 1;
                if guard > 64 {
                    break;
                }
                if class_key(&p) == target {
                    descends = true;
                    break;
                }
                parent = self.classes.get(&class_key(&p)).and_then(|f| f.parent.clone());
            }
            if !descends {
                continue;
            }
            let facts = &self.classes[key];
            let mut level = facts.statics.clone();
            let mut trait_stack = facts.traits.clone();
            let mut trait_guard = 0;
            while let Some(t) = trait_stack.pop() {
                trait_guard += 1;
                if trait_guard > 64 {
                    break;
                }
                if let Some(tf) = self.classes.get(&class_key(&t)) {
                    level.extend(tf.statics.iter().cloned());
                    trait_stack.extend(tf.traits.iter().cloned());
                }
            }
            for prop in level {
                if prop.visibility == Visibility::Private {
                    continue;
                }
                let known = out.iter().any(|p| p.name == prop.name)
                    || hidden.iter().any(|(name, _)| name == &prop.name);
                if !known {
                    return Some((facts.name.clone(), prop.name));
                }
            }
        }
        None
    }

    fn fresh_temp(&mut self, span: Span) -> String {
        self.counter += 1;
        crate::names::generated_local_name(&format!(
            "__elephc_dynstatic_name_{}_{}_{}",
            span.line, span.col, self.counter
        ))
    }

    /// `"Access to undeclared static property " . R::class . "::$" . (string) name`.
    fn undeclared_message(receiver: &StaticReceiver, name: Expr, span: Span) -> Expr {
        let class = Expr::new(ExprKind::ClassConstant { receiver: receiver.clone() }, span);
        binop(
            binop(
                binop(str_lit("Access to undeclared static property ", span), BinOp::Concat, class),
                BinOp::Concat,
                str_lit("::$", span),
            ),
            BinOp::Concat,
            cast_string(name),
        )
    }

    /// `"Cannot access <private|protected> property " . R::class . "::$name"`.
    fn private_message(
        receiver: &StaticReceiver,
        prop: &str,
        visibility: &Visibility,
        span: Span,
    ) -> Expr {
        let class = Expr::new(ExprKind::ClassConstant { receiver: receiver.clone() }, span);
        let label = match visibility {
            Visibility::Protected => "Cannot access protected property ",
            _ => "Cannot access private property ",
        };
        binop(
            binop(str_lit(label, span), BinOp::Concat, class),
            BinOp::Concat,
            str_lit(&format!("::${prop}"), span),
        )
    }

    /// The explicit elephc-limit error for an arm the access cannot take.
    fn unsupported_message(class: &str, prop: &str, span: Span) -> Expr {
        str_lit(
            &format!(
                "elephc: this dynamic access to static property {}::${} is not supported for its declared type",
                class.trim_start_matches('\\'),
                prop
            ),
            span,
        )
    }

    /// `R::$prop` for a candidate.
    fn static_prop(receiver: &StaticReceiver, prop: &str, span: Span) -> Expr {
        Expr::new(
            ExprKind::StaticPropertyAccess { receiver: receiver.clone(), property: prop.to_string() },
            span,
        )
    }

    /// Returns the literal identifier when the name is a plain string literal.
    fn literal_name(name: &Expr) -> Option<String> {
        fn fold(expr: &Expr) -> Option<String> {
            match &expr.kind {
                ExprKind::StringLiteral(value) => Some(value.clone()),
                ExprKind::BinaryOp { left, op: BinOp::Concat, right } => {
                    Some(format!("{}{}", fold(left)?, fold(right)?))
                }
                _ => None,
            }
        }
        fold(name).filter(|value| super::is_identifier_name(value))
    }

    /// Builds `match ((string) name) { 'p' => arm(p), ..., default => fallback }`.
    #[allow(clippy::too_many_arguments)]
    fn dispatch_expr(
        &mut self,
        root: &Expr,
        access: Access,
        span: Span,
        arm: &dyn Fn(&Expr) -> Expr,
        excluded: &dyn Fn(&str, &str) -> Expr,
        private: &dyn Fn(&str, &Visibility) -> Expr,
        fallback: Expr,
    ) -> Option<Expr> {
        let (receiver, name) = dynamic_root(root)?;
        let (receiver, name) = (receiver.clone(), name.clone());
        let (class, props, hidden) = match self.candidates(&receiver) {
            Ok(found) => found,
            Err(reason) => {
                self.errors.push(CompileError::new(
                    span,
                    &format!("Dynamic static property access (`::${{...}}`) {reason} is not supported (elephc)"),
                ));
                return None;
            }
        };
        if let Some(literal) = Self::literal_name(&name) {
            // A literal name is exactly `C::$name`: the checker validates it like one.
            let _ = (&class, excluded, access);
            return Some(match props.iter().find(|p| p.name == literal) {
                Some(p) => arm(&Self::static_prop(&receiver, &p.name, span)),
                None => match hidden.iter().find(|(name, _)| name == &literal) {
                    Some((name, visibility)) => private(name, visibility),
                    None => fallback,
                },
            });
        }
        let mut arms: Vec<(Vec<Expr>, Expr)> = props
            .iter()
            .map(|p| {
                let value = if access.admits(p) {
                    arm(&Self::static_prop(&receiver, &p.name, span))
                } else {
                    excluded(&class, &p.name)
                };
                (vec![str_lit(&p.name, span)], value)
            })
            .collect();
        arms.extend(
            hidden.iter().map(|(name, visibility)| (vec![str_lit(name, span)], private(name, visibility))),
        );
        Some(Expr::new(
            ExprKind::Match {
                subject: Box::new(cast_string(name)),
                arms,
                default: Some(Box::new(fallback)),
            },
            span,
        ))
    }

    /// Rewrites a read of the place `expr`, rooted at a dynamic access.
    fn rewrite_read(&mut self, expr: &mut Expr) {
        let span = expr.span;
        let Some(root) = place_root(expr).cloned() else {
            return;
        };
        let (receiver, name) = dynamic_root(&root).map(|(r, n)| (r.clone(), n.clone())).unwrap();
        let access = if dynamic_root(expr).is_some() { Access::Whole } else { Access::Element };
        let place = expr.clone();
        self.used_box = true;
        let arm = |prop: &Expr| {
            let mut read = place.clone();
            replace_root_in_expr(&mut read, &root, prop);
            box_mixed(read)
        };
        let excluded = |class: &str, prop: &str| {
            box_mixed(throw_error(Self::unsupported_message(class, prop, span), span))
        };
        let private = |prop: &str, visibility: &Visibility| {
            box_mixed(throw_error(Self::private_message(&receiver, prop, visibility, span), span))
        };
        let fallback = box_mixed(throw_error(Self::undeclared_message(&receiver, name, span), span));
        if let Some(rewritten) =
            self.dispatch_expr(&root, access, span, &arm, &excluded, &private, fallback)
        {
            *expr = rewritten;
        }
    }

    /// Rewrites `isset(...)` / `empty(...)` whose arguments include dynamic places.
    fn rewrite_isset_empty(&mut self, expr: &mut Expr, is_isset: bool) {
        let span = expr.span;
        let ExprKind::FunctionCall { name: fn_name, args } = &expr.kind else {
            return;
        };
        let fn_name = fn_name.clone();
        let mut plain = Vec::new();
        let mut parts = Vec::new();
        for arg in args.clone() {
            let Some(root) = place_root(&arg).cloned() else {
                plain.push(arg);
                continue;
            };
            let access = if dynamic_root(&arg).is_some() { Access::Whole } else { Access::Element };
            let builtin = fn_name.clone();
            let arm = |prop: &Expr| {
                let mut probed = arg.clone();
                replace_root_in_expr(&mut probed, &root, prop);
                Expr::new(ExprKind::FunctionCall { name: builtin.clone(), args: vec![probed] }, span)
            };
            let unset = Expr::new(ExprKind::BoolLiteral(!is_isset), span);
            let excluded = |_: &str, _: &str| Expr::new(ExprKind::BoolLiteral(!is_isset), span);
            let private = |_: &str, _: &Visibility| Expr::new(ExprKind::BoolLiteral(!is_isset), span);
            match self.dispatch_expr(&root, access, span, &arm, &excluded, &private, unset) {
                Some(part) => parts.push(part),
                None => return,
            }
        }
        if !plain.is_empty() {
            parts.insert(0, Expr::new(ExprKind::FunctionCall { name: fn_name, args: plain }, span));
        }
        let mut iter = parts.into_iter();
        let Some(mut combined) = iter.next() else {
            return;
        };
        for part in iter {
            combined = binop(combined, BinOp::And, part);
        }
        *expr = combined;
    }

    /// Rewrites `place ?? default` whose place is dynamic.
    fn rewrite_null_coalesce(&mut self, expr: &mut Expr) {
        let span = expr.span;
        let ExprKind::NullCoalesce { value, default } = &expr.kind else {
            return;
        };
        let (value, mut default) = ((**value).clone(), (**default).clone());
        self.expr(&mut default);
        let Some(root) = place_root(&value).cloned() else {
            return;
        };
        let access = if dynamic_root(&value).is_some() { Access::Whole } else { Access::Element };
        self.used_box = true;
        let arm = |prop: &Expr| {
            let mut read = value.clone();
            replace_root_in_expr(&mut read, &root, prop);
            box_mixed(Expr::new(
                ExprKind::NullCoalesce { value: Box::new(read), default: Box::new(default.clone()) },
                span,
            ))
        };
        let excluded = |_: &str, _: &str| box_mixed(default.clone());
        let private = |_: &str, _: &Visibility| box_mixed(default.clone());
        let fallback = box_mixed(default.clone());
        if let Some(rewritten) =
            self.dispatch_expr(&root, access, span, &arm, &excluded, &private, fallback)
        {
            *expr = rewritten;
        }
    }

    /// Finds the dynamic access a statement WRITES through, with the access kind.
    fn written_root(stmt: &Stmt) -> Option<(Expr, Access)> {
        match &stmt.kind {
            StmtKind::StaticPropertyArrayAssign { receiver, property, index, .. }
                if property == DYNAMIC_STATIC_PROPERTY =>
            {
                Some((dynamic_static_access(receiver.clone(), index.clone(), stmt.span), Access::WholeWrite))
            }
            StmtKind::StaticPropertyArrayPush { property, .. } if property == DYNAMIC_STATIC_PROPERTY => {
                None
            }
            StmtKind::NestedArrayAssign { target, .. } => {
                place_root(target).map(|root| (root.clone(), Access::Element))
            }
            StmtKind::ExprStmt(expr) => match &expr.kind {
                ExprKind::Assignment { target, .. } => place_root(target).map(|root| {
                    let access =
                        if dynamic_root(target).is_some() { Access::WholeWrite } else { Access::Element };
                    (root.clone(), access)
                }),
                ExprKind::FunctionCall { name, args }
                    if super::rewrite::builtin_call_name(name).as_deref() == Some("unset") =>
                {
                    args.iter().find_map(|arg| {
                        place_root(arg).map(|root| {
                            let access = if dynamic_root(arg).is_some() {
                                Access::WholeWrite
                            } else {
                                Access::Element
                            };
                            (root.clone(), access)
                        })
                    })
                }
                _ => None,
            },
            StmtKind::RefAssign { source, .. } => {
                place_root(source).map(|root| {
                    let access =
                        if dynamic_root(source).is_some() { Access::Reference } else { Access::Element };
                    (root.clone(), access)
                })
            }
            // `C::${$n}[] = v` / `C::${$n}[k][] = v`: the parser's nested-append desugar reads
            // the property into a hidden temporary, appends, and writes the temporary back. The
            // whole block is one ELEMENT write of the property, dispatched as a unit so each arm
            // reads, appends and writes back the same named property.
            StmtKind::Synthetic(stmts) => stmts.iter().find_map(|inner| match &inner.kind {
                StmtKind::StaticPropertyArrayAssign { receiver, property, index, value }
                    if property == DYNAMIC_STATIC_PROPERTY
                        && matches!(&value.kind, ExprKind::Variable(name)
                            if name.starts_with(crate::parser::ast::NESTED_APPEND_TEMP_PREFIX)) =>
                {
                    Some((
                        dynamic_static_access(receiver.clone(), index.clone(), inner.span),
                        Access::Element,
                    ))
                }
                StmtKind::NestedArrayAssign { target, value }
                    if matches!(&value.kind, ExprKind::Variable(name)
                        if name.starts_with(crate::parser::ast::NESTED_APPEND_TEMP_PREFIX)) =>
                {
                    place_root(target).map(|root| (root.clone(), Access::Element))
                }
                _ => None,
            }),
            _ => None,
        }
    }

    /// Normalizes a statement whose dynamic root was replaced by a named static property.
    fn normalize_written(stmt: Stmt, root: &Expr, prop: &str) -> Result<Stmt, CompileError> {
        let span = stmt.span;
        if let StmtKind::Synthetic(stmts) = stmt.kind {
            let stmts = stmts
                .into_iter()
                .map(|inner| Self::normalize_written(inner, root, prop))
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(Stmt::new(StmtKind::Synthetic(stmts), span));
        }
        let (receiver, _) = dynamic_root(root).unwrap();
        let replacement = Self::static_prop(receiver, prop, span);
        let mut stmt = stmt;
        if let StmtKind::StaticPropertyArrayAssign { receiver, property, value, index } = &stmt.kind {
            if property == DYNAMIC_STATIC_PROPERTY
                && dynamic_root(root).is_some_and(|(_, name)| name == index)
            {
                let mut value = value.clone();
                replace_root_in_expr(&mut value, root, &replacement);
                stmt.kind = StmtKind::StaticPropertyAssign {
                    receiver: receiver.clone(),
                    property: prop.to_string(),
                    value,
                };
                return Ok(stmt);
            }
        }
        replace_root_in_stmt(&mut stmt, root, &replacement);
        if let StmtKind::NestedArrayAssign { target, value } = &stmt.kind {
            let kind = crate::parser::stmt::assignment_target_store_stmt(
                target.clone(),
                value.clone(),
                span,
            )?;
            stmt.kind = kind;
        }
        Ok(stmt)
    }

    /// Rewrites a statement that writes through a dynamic access into an `if` chain.
    fn rewrite_write_stmt(&mut self, stmt: &mut Stmt, root: Expr, access: Access) {
        let span = stmt.span;
        let (receiver, name) = dynamic_root(&root).map(|(r, n)| (r.clone(), n.clone())).unwrap();
        let (class, props, hidden) = match self.candidates(&receiver) {
            Ok(found) => found,
            Err(reason) => {
                self.errors.push(CompileError::new(
                    span,
                    &format!("Dynamic static property access (`::${{...}}`) {reason} is not supported (elephc)"),
                ));
                return;
            }
        };
        // `$r = &C::${$n}` lowers to one `$r = &C::$p;` per branch of an `if` chain, and a
        // reference bound to a static property inside a conditional branch does not survive the
        // branch unless every candidate shares one representation (elephc limit: the arms of
        // DebugClassLoader are all arrays). Mixed candidates would silently bind null or crash,
        // so they are refused here.
        if access == Access::Reference
            && Self::literal_name(&name).is_none()
            && props.len() > 1
            && !props.iter().all(is_array_valued)
        {
            self.errors.push(CompileError::new(
                span,
                &format!(
                    "A reference to a static property named at run time (`$r = &C::${{...}}`) is only supported when every candidate property of `{}` is an array (or there is a single candidate) (elephc)",
                    class.trim_start_matches('\\')
                ),
            ));
            return;
        }
        let throw_stmt = |message: Expr| Stmt::new(StmtKind::Throw(super::new_error(message, span)), span);
        let arm_body = |this: &mut Self, prop: &StaticProp, checked: bool| -> Vec<Stmt> {
            if checked && !access.admits(prop) {
                return vec![throw_stmt(Self::unsupported_message(&class, &prop.name, span))];
            }
            match Self::normalize_written(stmt.clone(), &root, &prop.name) {
                Ok(mut written) => {
                    this.stmt(&mut written);
                    vec![written]
                }
                Err(error) => {
                    this.errors.push(error);
                    Vec::new()
                }
            }
        };
        if let Some(literal) = Self::literal_name(&name) {
            let body = match props.iter().find(|p| p.name == literal) {
                Some(p) => arm_body(self, &p.clone(), false),
                None if hidden.iter().any(|(name, _)| name == &literal) => {
                    let visibility = hidden
                        .iter()
                        .find(|(name, _)| name == &literal)
                        .map(|(_, visibility)| visibility.clone())
                        .unwrap_or(Visibility::Private);
                    vec![throw_stmt(Self::private_message(&receiver, &literal, &visibility, span))]
                }
                None => vec![throw_stmt(Self::undeclared_message(&receiver, name.clone(), span))],
            };
            stmt.kind = StmtKind::Synthetic(body);
            return;
        }
        let temp = self.fresh_temp(span);
        let mut out = vec![Stmt::new(
            StmtKind::Assign { name: temp.clone(), value: cast_string(name.clone()) },
            span,
        )];
        let mut clauses: Vec<(Expr, Vec<Stmt>)> = Vec::new();
        for prop in props.clone() {
            let test = binop(var(&temp, span), BinOp::StrictEq, str_lit(&prop.name, span));
            let body = arm_body(self, &prop, true);
            clauses.push((test, body));
        }
        for (name, visibility) in &hidden {
            let test = binop(var(&temp, span), BinOp::StrictEq, str_lit(name, span));
            clauses.push((
                test,
                vec![throw_stmt(Self::private_message(&receiver, name, visibility, span))],
            ));
        }
        let fallback = vec![throw_stmt(Self::undeclared_message(&receiver, var(&temp, span), span))];
        if clauses.is_empty() {
            out.extend(fallback);
        } else {
            let (condition, then_body) = clauses.remove(0);
            out.push(Stmt::new(
                StmtKind::If {
                    condition,
                    then_body,
                    elseif_clauses: clauses,
                    else_body: Some(fallback),
                },
                span,
            ));
        }
        stmt.kind = StmtKind::Synthetic(out);
    }

    /// Visits a function-like body with `class` as the lexical class.
    fn with_class(&mut self, class: Option<String>, body: &mut Vec<Stmt>) {
        let saved = std::mem::replace(&mut self.current_class, class);
        self.block(body);
        self.current_class = saved;
    }
}

impl Visitor for StaticPropsRewriter {
    fn stmt(&mut self, stmt: &mut Stmt) {
        match &mut stmt.kind {
            StmtKind::FunctionDecl { body, .. } => {
                self.with_class(None, body);
                return;
            }
            StmtKind::ClassDecl { name, methods, .. }
            | StmtKind::TraitDecl { name, methods, .. }
            | StmtKind::EnumDecl { name, methods, .. }
            | StmtKind::InterfaceDecl { name, methods, .. } => {
                let name = name.clone();
                for method in methods.iter_mut() {
                    self.with_class(Some(name.clone()), &mut method.body);
                }
                return;
            }
            _ => {}
        }
        if let Some((root, access)) = Self::written_root(stmt) {
            self.rewrite_write_stmt(stmt, root, access);
            return;
        }
        walk_stmt_children(self, stmt);
    }

    fn expr(&mut self, expr: &mut Expr) {
        match &mut expr.kind {
            ExprKind::Closure { body, .. } => {
                let class = self.current_class.clone();
                self.with_class(class, body);
                return;
            }
            ExprKind::FunctionCall { name, args } => {
                let builtin = super::rewrite::builtin_call_name(name);
                if matches!(builtin.as_deref(), Some("isset") | Some("empty"))
                    && args.iter().any(|arg| place_root(arg).is_some())
                {
                    for arg in args.iter_mut() {
                        if place_root(arg).is_none() {
                            self.expr(arg);
                        } else {
                            // Only the dimensions inside the place are ordinary reads.
                            walk_place_dimensions(self, arg);
                        }
                    }
                    let is_isset = builtin.as_deref() == Some("isset");
                    self.rewrite_isset_empty(expr, is_isset);
                    return;
                }
            }
            ExprKind::NullCoalesce { value, .. } if place_root(value).is_some() => {
                walk_place_dimensions(self, value);
                self.rewrite_null_coalesce(expr);
                return;
            }
            ExprKind::Assignment { target, .. } if place_root(target).is_some() => {
                let span = expr.span;
                self.errors.push(CompileError::new(
                    span,
                    "Assigning to a dynamic static property (`C::${...}`) inside an expression is not supported (elephc); use a separate assignment statement",
                ));
                return;
            }
            _ => {}
        }
        if place_root(expr).is_some() {
            walk_place_dimensions(self, expr);
            self.rewrite_read(expr);
            return;
        }
        walk_expr_children(self, expr);
    }
}

/// Rewrites the dimension expressions inside a dynamic place (not the place itself).
fn walk_place_dimensions(v: &mut StaticPropsRewriter, place: &mut Expr) {
    if dynamic_root(place).is_some() {
        if let ExprKind::ArrayAccess { index, .. } = &mut place.kind {
            if find_root_in_expr(index).is_some() {
                v.errors.push(CompileError::new(
                    place.span,
                    "A dynamic static property name (`C::${...}`) cannot itself read a dynamic static property (elephc)",
                ));
            } else {
                v.expr(index);
            }
        }
        return;
    }
    match &mut place.kind {
        ExprKind::ArrayAccess { array, index } => {
            walk_place_dimensions(v, array);
            v.expr(index);
        }
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            walk_place_dimensions(v, object);
            v.expr(property);
        }
        ExprKind::PropertyAccess { object, .. } | ExprKind::NullsafePropertyAccess { object, .. } => {
            walk_place_dimensions(v, object)
        }
        _ => {}
    }
}

/// Desugars every dynamic static-property access in `program`.
pub(super) fn desugar(program: &mut Vec<Stmt>) -> (bool, Vec<CompileError>) {
    if !SEEN.load(std::sync::atomic::Ordering::Relaxed)
        || !program_has_dynamic_static_access(program)
    {
        return (false, Vec::new());
    }
    let mut classes = HashMap::new();
    collect_classes(program, &mut classes);
    let mut rewriter = StaticPropsRewriter {
        classes,
        current_class: None,
        counter: 0,
        used_box: false,
        errors: Vec::new(),
    };
    rewriter.block(program);
    let mut errors = rewriter.errors;
    if errors.is_empty() {
        let leftover = program_has_dynamic_static_access(program);
        if leftover {
            errors.push(CompileError::new(
                Span::dummy(),
                "Unsupported dynamic static property form (elephc): this position cannot take `C::${expr}`",
            ));
        }
    }
    (rewriter.used_box, errors)
}

/// Cheap scan: does any statement mention the marker property?
fn program_has_dynamic_static_access(program: &mut Vec<Stmt>) -> bool {
    struct Scan(bool);
    impl Visitor for Scan {
        fn stmt(&mut self, stmt: &mut Stmt) {
            if self.0 {
                return;
            }
            match &mut stmt.kind {
                StmtKind::FunctionDecl { body, .. } => self.block(body),
                StmtKind::ClassDecl { methods, .. }
                | StmtKind::TraitDecl { methods, .. }
                | StmtKind::EnumDecl { methods, .. }
                | StmtKind::InterfaceDecl { methods, .. } => {
                    for method in methods.iter_mut() {
                        self.block(&mut method.body);
                    }
                }
                StmtKind::StaticPropertyArrayAssign { property, .. }
                | StmtKind::StaticPropertyArrayPush { property, .. }
                | StmtKind::StaticPropertyAssign { property, .. }
                    if property == DYNAMIC_STATIC_PROPERTY =>
                {
                    self.0 = true;
                }
                _ => walk_stmt_children(self, stmt),
            }
        }
        fn expr(&mut self, expr: &mut Expr) {
            if self.0 {
                return;
            }
            match &mut expr.kind {
                ExprKind::StaticPropertyAccess { property, .. } if property == DYNAMIC_STATIC_PROPERTY => {
                    self.0 = true;
                }
                ExprKind::Closure { body, .. } => self.block(body),
                _ => walk_expr_children(self, expr),
            }
        }
    }
    let mut scan = Scan(false);
    scan.block(program);
    scan.0
}
