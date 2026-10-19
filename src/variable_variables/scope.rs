//! Purpose:
//! Collects the compile-time frame names of one PHP scope and bounds the runtime values a
//! variable-variable name expression can take.
//!
//! Called from:
//! - `super::rewrite::Rewriter` when it enters a function, method, closure or arrow function.
//!
//! Key details:
//! - A frame name is any variable the scope spells out: parameters, assigned, read, captured
//!   (`use`), `global` / `static`, `foreach` / `catch` / `list()` targets, and the free variables
//!   of nested arrow functions (PHP compiles those as the parent's locals to capture them).
//! - `known_values` is a best-effort bound used only to narrow write dispatch. It may be wrong in
//!   either direction without miscompiling, because every dispatch keeps an explicit `Error` arm
//!   for names outside the set it chose.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::parser::ast::{ArrayEntry, BinOp, CastType, Expr, ExprKind, Stmt, StmtKind};

use super::walk::{walk_expr_children, walk_stmt_children, Visitor};
use super::{is_frame_reachable_name, FRAME_LOCAL};

/// Maximum number of names a narrowed bound may hold before it is treated as unbounded.
const MAX_KNOWN_VALUES: usize = 64;

/// Narrowing-stack marker that disables the self-write check while it is being evaluated.
const SELF_CHECK_MARKER: &str = "#self-check";

/// Maximum definition-chasing depth for `known_values`.
const MAX_KNOWN_DEPTH: usize = 4;

/// Compile-time facts about one scope's frame.
#[derive(Debug, Default)]
pub(super) struct FrameNames {
    /// Every variable name a variable variable in this scope can reach, sorted.
    pub(super) names: BTreeSet<String>,
    /// Declared parameter names (always defined on entry).
    pub(super) params: HashSet<String>,
    /// Closure `use` captures, by value or by reference (always defined on entry: PHP binds an
    /// undefined capture as null).
    pub(super) captures: HashSet<String>,
    /// Plain `$x = <expr>;` definitions per variable, used to bound name expressions.
    pub(super) defs: HashMap<String, Vec<Expr>>,
    /// Variables written by some form other than a plain definition (unbounded values).
    pub(super) opaque: HashSet<String>,
    /// Name expressions of every variable-variable write site (`$$n = v`, `unset($$n)`,
    /// `foreach ($$n as &$v)`, `= &$$n`, destructuring into `$$n`) in this frame.
    pub(super) frame_writes: Vec<Expr>,
    /// Frame names spelled by something other than a `compact()` string.
    pub(super) spelled: BTreeSet<String>,
    /// The frame calls `extract()`, which may define any frame name.
    pub(super) has_extract: bool,
    /// Locals some statement `unset()`s by name; never definitely assigned.
    pub(super) unset_names: HashSet<String>,
    /// Declared types of the typed parameters.
    pub(super) param_types: HashMap<String, crate::parser::ast::TypeExpr>,
    /// The scope reads its frame by runtime name (`$$name`, `get_defined_vars()`).
    pub(super) reads_dynamically: bool,
}

impl FrameNames {
    fn add(&mut self, name: &str) {
        if is_frame_reachable_name(name) {
            self.names.insert(name.to_string());
            self.spelled.insert(name.to_string());
        }
    }

    fn add_compact_name(&mut self, name: &str) {
        if is_frame_reachable_name(name) {
            self.names.insert(name.to_string());
        }
    }

    fn add_opaque(&mut self, name: &str) {
        self.add(name);
        self.opaque.insert(name.to_string());
    }
}

/// Collects the frame names of a function-like scope with `params` and `body`.
pub(super) fn collect_frame_names(params: &[String], body: &mut Vec<Stmt>) -> FrameNames {
    collect_frame_names_with_captures(params, &[], body)
}

/// Like [`collect_frame_names`] for a closure whose `use` list binds `captures` on entry.
pub(super) fn collect_frame_names_with_captures(
    params: &[String],
    captures: &[String],
    body: &mut Vec<Stmt>,
) -> FrameNames {
    let mut collector = Collector {
        frame: FrameNames::default(),
    };
    for param in params {
        collector.frame.add_opaque(param);
        collector.frame.params.insert(param.clone());
    }
    for capture in captures {
        // A capture is a local of the closure frame even when its body never spells it
        // (`extract()` can still write through a `use (&$x)` binding).
        collector.frame.add_opaque(capture);
        collector.frame.captures.insert(capture.clone());
    }
    collector.block(body);
    collector.frame
}

struct Collector {
    frame: FrameNames,
}

impl Visitor for Collector {
    fn stmt(&mut self, stmt: &mut Stmt) {
        match &stmt.kind {
            StmtKind::FunctionDecl { .. }
            | StmtKind::ClassDecl { .. }
            | StmtKind::TraitDecl { .. }
            | StmtKind::InterfaceDecl { .. }
            | StmtKind::EnumDecl { .. } => return,
            StmtKind::Assign { name, value } => {
                self.frame.add(name);
                self.frame
                    .defs
                    .entry(name.clone())
                    .or_default()
                    .push(value.clone());
            }
            StmtKind::RefAssign { target, source } => {
                self.frame.add_opaque(target);
                // `$r = &$n`: later writes through `$r` change `$n`.
                if let ExprKind::Variable(name) = &source.kind {
                    let name = name.clone();
                    self.frame.add_opaque(&name);
                }
                if let Some(root) = super::place_frame_root(source) {
                    if let Some(name) = super::frame_access_name(root) {
                        self.frame.frame_writes.push(name.clone());
                    }
                }
            }
            StmtKind::ArrayAssign { array, index, .. } if array == FRAME_LOCAL => {
                self.frame.frame_writes.push(index.clone());
            }
            StmtKind::RefAssignPlace { target, source } => {
                if let ExprKind::Variable(name) = &source.kind {
                    let name = name.clone();
                    self.frame.add_opaque(&name);
                }
                for place in [target, source] {
                    if let Some(root) = super::place_frame_root(place) {
                        if let Some(name) = super::frame_access_name(root) {
                            self.frame.frame_writes.push(name.clone());
                        }
                    }
                }
            }
            StmtKind::NestedArrayAssign { target, .. } => {
                if let Some(root) = super::place_frame_root(target) {
                    if let Some(name) = super::frame_access_name(root) {
                        self.frame.frame_writes.push(name.clone());
                    }
                }
            }
            StmtKind::TypedAssign { name, .. }
            | StmtKind::ArrayAssign { array: name, .. }
            | StmtKind::ArrayPush { array: name, .. }
            | StmtKind::StaticVar { name, .. } => self.frame.add_opaque(name),
            StmtKind::Foreach {
                key_var,
                value_var,
                array,
                value_by_ref,
                ..
            } => {
                if *value_by_ref {
                    if let Some(root) = super::place_frame_root(array) {
                        if let Some(name) = super::frame_access_name(root) {
                            self.frame.frame_writes.push(name.clone());
                        }
                    }
                }
                self.frame.add_opaque(value_var);
                if let Some(key) = key_var {
                    self.frame.add_opaque(key);
                }
            }
            StmtKind::ListUnpack { vars, .. } | StmtKind::Global { vars } => {
                for var in vars {
                    self.frame.add_opaque(var);
                }
            }
            StmtKind::Try { catches, .. } => {
                for catch in catches {
                    if let Some(var) = &catch.variable {
                        self.frame.add_opaque(var);
                    }
                }
            }
            _ => {}
        }
        walk_stmt_children(self, stmt);
    }

    fn expr(&mut self, expr: &mut Expr) {
        if super::builtins::reads_frame_dynamically(expr) {
            self.frame.reads_dynamically = true;
        }
        match &mut expr.kind {
            ExprKind::Variable(name) => {
                if name != FRAME_LOCAL {
                    let name = name.clone();
                    self.frame.add(&name);
                }
            }
            ExprKind::PreIncrement(name)
            | ExprKind::PostIncrement(name)
            | ExprKind::PreDecrement(name)
            | ExprKind::PostDecrement(name) => {
                let name = name.clone();
                self.frame.add_opaque(&name);
            }
            ExprKind::Assignment { target, .. } => {
                if let ExprKind::Variable(name) = &target.kind {
                    let name = name.clone();
                    self.frame.add_opaque(&name);
                }
                if let Some(root) = super::place_frame_root(target) {
                    if let Some(name) = super::frame_access_name(root) {
                        self.frame.frame_writes.push(name.clone());
                    }
                }
            }
            ExprKind::FunctionCall { name, args } => {
                let builtin = super::rewrite::builtin_call_name(name);
                match builtin.as_deref() {
                    Some("compact") => {
                        // `compact('x')` reads `$x` by name: it is a frame name.
                        let mut names = Vec::new();
                        for arg in args.iter() {
                            super::builtins::collect_literal_names(arg, &mut names);
                        }
                        for name in names {
                            self.frame.add_compact_name(&name);
                        }
                    }
                    Some("extract") => self.frame.has_extract = true,
                    Some("unset") => {
                        for arg in args.iter() {
                            if let ExprKind::Variable(name) = &arg.kind {
                                self.frame.unset_names.insert(name.clone());
                            }
                            if let Some(root) = super::place_frame_root(arg) {
                                if let Some(name) = super::frame_access_name(root) {
                                    self.frame.frame_writes.push(name.clone());
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            ExprKind::ClosureCall { .. } => {
                if let ExprKind::ClosureCall { var, .. } = &expr.kind {
                    let var = var.clone();
                    self.frame.add(&var);
                }
            }
            ExprKind::Closure {
                params,
                variadic,
                body,
                is_arrow,
                captures,
                capture_refs,
                ..
            } => {
                if *is_arrow {
                    // An arrow function's free variables are captured from this frame.
                    let mut own: HashSet<String> =
                        params.iter().map(|(name, ..)| name.clone()).collect();
                    if let Some(variadic) = variadic {
                        own.insert(variadic.clone());
                    }
                    let inner = collect_frame_names(&[], body);
                    for name in inner.names {
                        if !own.contains(&name) {
                            self.frame.add_opaque(&name);
                        }
                    }
                } else {
                    for name in captures.iter() {
                        self.frame.add(name);
                    }
                    for name in capture_refs.iter() {
                        self.frame.add_opaque(name);
                    }
                }
                return;
            }
            _ => {}
        }
        walk_expr_children(self, expr);
    }
}

/// Bounds the string values `expr` can evaluate to, or `None` when it is unbounded.
///
/// `narrow` holds the innermost-last bindings established by enclosing `foreach` loops over
/// literal arrays and `switch` case groups.
pub(super) fn known_values(
    expr: &Expr,
    frame: &FrameNames,
    narrow: &[(String, BTreeSet<String>)],
) -> Option<BTreeSet<String>> {
    known_values_at(expr, frame, narrow, 0)
}

fn known_values_at(
    expr: &Expr,
    frame: &FrameNames,
    narrow: &[(String, BTreeSet<String>)],
    depth: usize,
) -> Option<BTreeSet<String>> {
    if depth > MAX_KNOWN_DEPTH {
        return None;
    }
    match &expr.kind {
        ExprKind::StringLiteral(value) => Some(BTreeSet::from([value.clone()])),
        ExprKind::IntLiteral(value) => Some(BTreeSet::from([value.to_string()])),
        ExprKind::Cast {
            target: CastType::String,
            expr,
        } => known_values_at(expr, frame, narrow, depth),
        ExprKind::Variable(name) => {
            if let Some((_, values)) = narrow.iter().rev().find(|(var, _)| var == name) {
                return Some(values.clone());
            }
            if frame.opaque.contains(name) || frame.params.contains(name) {
                return None;
            }
            // `$n = 'n'; $$n = 'a';` rewrites `$n` itself: a write site named by `$n` whose
            // own bound includes "n" makes the definitions unreliable.
            if !narrow.iter().any(|(marker, _)| marker == SELF_CHECK_MARKER) {
                let mut guarded: Vec<(String, BTreeSet<String>)> = narrow.to_vec();
                guarded.push((SELF_CHECK_MARKER.to_string(), BTreeSet::new()));
                for site in &frame.frame_writes {
                    if matches!(&site.kind, ExprKind::Variable(v) if v == name) {
                        let bound = known_values_at(site, frame, &guarded, depth + 1);
                        if bound.is_none_or(|bound| bound.contains(name)) {
                            return None;
                        }
                    }
                }
            }
            let defs = frame.defs.get(name)?;
            let mut out = BTreeSet::new();
            for def in defs {
                out.extend(known_values_at(def, frame, narrow, depth + 1)?);
                if out.len() > MAX_KNOWN_VALUES {
                    return None;
                }
            }
            Some(out)
        }
        ExprKind::BinaryOp {
            left,
            op: BinOp::Concat,
            right,
        } => {
            let left = known_values_at(left, frame, narrow, depth + 1)?;
            let right = known_values_at(right, frame, narrow, depth + 1)?;
            if left.len() * right.len() > MAX_KNOWN_VALUES {
                return None;
            }
            let mut out = BTreeSet::new();
            for l in &left {
                for r in &right {
                    out.insert(format!("{l}{r}"));
                }
            }
            Some(out)
        }
        _ => None,
    }
}

/// Returns whether a narrowing of `var` established at the head of `region` may be invalidated
/// inside it: `var` is reassigned, aliased, passed to a possibly by-reference call, or named by a
/// variable-variable write.
///
/// A variable-variable write named by `var` itself only rewrites `var` when "var" is one of the
/// `bound` names it can resolve to.
pub(super) fn region_may_rewrite(region: &[Stmt], var: &str, bound: &BTreeSet<String>) -> bool {
    let mut body = region.to_vec();
    let inner = collect_frame_names(&[], &mut body);
    inner.opaque.contains(var)
        || inner.defs.contains_key(var)
        || inner.unset_names.contains(var)
        || inner
            .frame_writes
            .iter()
            .any(|site| matches!(&site.kind, ExprKind::Variable(v) if v == var) && bound.contains(var))
}

/// Returns the string values a `foreach` over `array` binds to its value (or key) variable when
/// the array is a literal of string/int scalars.
pub(super) fn literal_foreach_values(array: &Expr, keys: bool) -> Option<BTreeSet<String>> {
    let scalar = |expr: &Expr| match &expr.kind {
        ExprKind::StringLiteral(value) => Some(value.clone()),
        ExprKind::IntLiteral(value) => Some(value.to_string()),
        _ => None,
    };
    let mut out = BTreeSet::new();
    match &array.kind {
        ExprKind::ArrayLiteral(items) => {
            if keys {
                for index in 0..items.len() {
                    out.insert(index.to_string());
                }
            } else {
                for item in items {
                    out.insert(scalar(item)?);
                }
            }
        }
        ExprKind::ArrayLiteralAssoc(pairs) => {
            for (key, value) in pairs {
                out.insert(scalar(if keys { key } else { value })?);
            }
        }
        ExprKind::ArrayLiteralMixed(entries) => {
            for entry in entries {
                match entry {
                    ArrayEntry::Keyed(key, value) => {
                        out.insert(scalar(if keys { key } else { value })?);
                    }
                    _ => return None,
                }
            }
        }
        _ => return None,
    }
    (!out.is_empty() && out.len() <= MAX_KNOWN_VALUES).then_some(out)
}
