//! Purpose:
//! Computes php's two LINK-TIME diagnostics for a class that overrides a built-in method:
//! the `Return type of X::m() should either be compatible with ...` deprecation, and the
//! `Declaration of X::m(...) must be compatible with ...` fatal.
//!
//! Called from:
//! - `crate::types::checker::driver`, once the class map is complete.
//!
//! Key details:
//! - The fatal wins over the deprecation FOR THE WHOLE CLASS, not just for the offending method.
//!   MEASURED on `php -n` 8.5.6: a class whose `onClose()` deprecates and whose `filter()` is
//!   incompatible prints the fatal alone, even though `onClose` is declared first. An EARLIER
//!   class's deprecation does print, so the granularity is the class php is linking.
//! - php links every class in the file before it runs a single statement — an `echo` above the
//!   class declaration produces nothing when the class below it is incompatible — so the fatal
//!   belongs in the main prologue with the deprecations, not at any call site.
//! - php raises this while LINKING the class, so it fires before the script produces anything and
//!   whether or not the method is ever called. elephc's equivalent of "before any output" is the
//!   main prologue, which is where these land — the same place `$http_response_header`'s
//!   compile-time deprecation is emitted from.
//! - `php_user_filter` is the only built-in class in elephc's surface with tentative returns, and
//!   it has three: `filter(): int`, `onCreate(): bool` and `onClose(): void`. MEASURED on
//!   `php -n` 8.5.6 — a subclass declaring any of them WITHOUT a return type and WITHOUT
//!   `#[\ReturnTypeWillChange]` gets one notice per method, at the METHOD's line.
//! - php prints the child's parameter list exactly as declared — types, `&`, `...`, defaults —
//!   and its own canonical spelling of the parent's. Where this module cannot render the child's
//!   list with certainty it emits NOTHING rather than a message that would differ from php's.
//!   That is one shape today: a UNION, which php reorders into its own canonical order
//!   (`int|string` comes back as `string|int`). A union parameter or return type is therefore the
//!   known gap in both diagnostics.
//! - `static`, `abstract` and a non-public access level each get php's own separate message, and
//!   they win over the signature one in that order — MEASURED on methods where several apply at
//!   once. See `refusal_message`.

use std::collections::HashMap;

use crate::names::php_symbol_key;
use crate::parser::ast::{ClassMethod, Expr, ExprKind, TypeExpr, Visibility};
use crate::types::traits::FlattenedClass;

/// One parameter of a built-in method, as php declares it.
struct ParentParam {
    /// php's own spelling of the declared type, or `None` where php declares none — which php
    /// treats as `mixed`, so only an untyped or `mixed` override widens it.
    ty: Option<&'static str>,
    by_ref: bool,
}

/// One built-in method whose return type php declares tentatively.
struct TentativeMethod {
    /// The method name, compared case-insensitively as php does.
    name: &'static str,
    /// php's own spelling of the parent declaration, parameters and return type included.
    parent_signature: &'static str,
    /// php's tentative return type, which the child's must be a subtype of to stay silent.
    returns: &'static str,
    /// The parent parameters, in declaration order, for the compatibility check.
    params: &'static [ParentParam],
}

/// `php_user_filter`'s three tentative returns, in php's declaration order.
const USER_FILTER_TENTATIVES: &[TentativeMethod] = &[
    TentativeMethod {
        name: "filter",
        parent_signature: "filter($in, $out, &$consumed, bool $closing): int",
        returns: "int",
        params: &[
            ParentParam { ty: None, by_ref: false },
            ParentParam { ty: None, by_ref: false },
            ParentParam { ty: None, by_ref: true },
            ParentParam { ty: Some("bool"), by_ref: false },
        ],
    },
    TentativeMethod {
        name: "onCreate",
        parent_signature: "onCreate(): bool",
        returns: "bool",
        params: &[],
    },
    TentativeMethod {
        name: "onClose",
        parent_signature: "onClose(): void",
        returns: "void",
        params: &[],
    },
];

/// php's link-time diagnostics for one program, in the order php prints them.
pub(crate) struct LinkTimeDiagnostics {
    /// `(line, message)` for every tentative-return deprecation php reaches before it stops.
    pub deprecations: Vec<(u32, String)>,
    /// `(line, message)` for the incompatible declaration php stops on, when there is one.
    pub fatal: Option<(u32, String)>,
}

/// Walks the classes php would link, in php's own order, and collects what it would print.
///
/// The walk stops at the first incompatible declaration because php's does: the fatal aborts the
/// script before any later class is linked and before any statement runs. Within one class the
/// compatibility check runs FIRST for every method, so a deprecation from that same class is
/// never reached — MEASURED, and the reason the two diagnostics share this one pass.
pub(crate) fn link_time_diagnostics(
    class_map: &HashMap<String, FlattenedClass>,
) -> LinkTimeDiagnostics {
    let mut classes: Vec<&FlattenedClass> = class_map
        .values()
        .filter(|class| descends_from_user_filter(class, class_map))
        .collect();
    // The map is a `HashMap`, so its iteration order is not the program's. php links classes as
    // the compiler reaches them, which is declaration order; the name breaks a tie so the output
    // does not depend on the hash seed.
    classes.sort_by(|a, b| a.span.line.cmp(&b.span.line).then_with(|| a.name.cmp(&b.name)));

    let mut deprecations = Vec::new();
    for class in classes {
        let mut methods: Vec<&ClassMethod> = class.methods.iter().collect();
        methods.sort_by_key(|method| method.span.line);

        for method in &methods {
            let Some(parent) = parent_method(method) else {
                continue;
            };
            let Some(message) = refusal_message(class, method, parent) else {
                continue;
            };
            return LinkTimeDiagnostics {
                deprecations,
                fatal: Some((method.span.line, message)),
            };
        }

        for method in &methods {
            let Some(parent) = parent_method(method) else {
                continue;
            };
            if has_return_type_will_change(method) {
                continue;
            }
            // php raises the notice unless the child's return type is a SUBTYPE of the tentative
            // one. Declaring none is the common case, and declaring the wrong one is the same
            // notice — MEASURED: `filter(): float` and `onClose(): mixed` both get it.
            if let Some(declared) = &method.return_type {
                match return_type_satisfies(parent.returns, declared) {
                    Some(false) => {}
                    // Compatible, or a shape `render_type` cannot judge: say nothing.
                    Some(true) | None => continue,
                }
            }
            let Some(declaration) = render_declaration(class, method) else {
                continue;
            };
            deprecations.push((
                method.span.line,
                format!(
                    "Deprecated: Return type of {} should either be compatible with \
                     php_user_filter::{}, or the #[\\ReturnTypeWillChange] attribute should be \
                     used to temporarily suppress the notice\n",
                    declaration, parent.parent_signature
                ),
            ));
        }
    }
    LinkTimeDiagnostics {
        deprecations,
        fatal: None,
    }
}

/// php's refusal for one overriding declaration, or `None` when php accepts it.
///
/// The order is php's own, MEASURED where several refusals apply to the same method at once:
/// `static` wins over `abstract`, which wins over the access level, which wins over the signature.
/// A method that is both `private` and `short` gets the access-level message, not the signature
/// one, so the checks cannot be reordered for convenience.
fn refusal_message(
    class: &FlattenedClass,
    method: &ClassMethod,
    parent: &TentativeMethod,
) -> Option<String> {
    if method.is_static {
        return Some(format!(
            "Fatal error: Cannot make non static method php_user_filter::{}() static in class {}\n",
            parent.name, class.name
        ));
    }
    if method.is_abstract {
        if method.visibility != Visibility::Public {
            // php answers a general declaration error here — `Abstract function X::m() cannot be
            // declared private` — which is not this family's to write.
            return None;
        }
        return Some(format!(
            "Fatal error: Cannot make non abstract method php_user_filter::{}() abstract in class {}\n",
            parent.name, class.name
        ));
    }
    if method.visibility != Visibility::Public {
        return Some(format!(
            "Fatal error: Access level to {}::{}() must be public (as in class php_user_filter)\n",
            class.name, method.name
        ));
    }
    if declaration_is_incompatible(method, parent) != Some(true) {
        return None;
    }
    // php stops here and this cannot say what it would print, so it says nothing rather than a
    // message that would differ. See `render_parameters`.
    let declaration = render_declaration(class, method)?;
    Some(format!(
        "Fatal error: Declaration of {} must be compatible with php_user_filter::{}\n",
        declaration, parent.parent_signature
    ))
}

/// Whether the child's declared return type is a subtype of the tentative one php declares.
///
/// MEASURED on `php -n` 8.5.6 against all three: the same type is silent, `never` is silent for
/// every parent because it is a subtype of everything, and `true`/`false` are silent for `bool`.
/// `float` and `?int` against `int`, and `mixed` against `void`, all get the notice.
///
/// `None` is "cannot tell" — a union, which `render_type` declines because php prints unions in
/// its own canonical order.
fn return_type_satisfies(parent: &str, child: &TypeExpr) -> Option<bool> {
    let rendered = render_type(child)?;
    if rendered.eq_ignore_ascii_case("never") {
        return Some(true);
    }
    if parent == "bool" {
        return Some(matches!(
            rendered.to_ascii_lowercase().as_str(),
            "bool" | "true" | "false"
        ));
    }
    Some(rendered.eq_ignore_ascii_case(parent))
}

/// The built-in method this one overrides, if it overrides one at all.
fn parent_method(method: &ClassMethod) -> Option<&'static TentativeMethod> {
    USER_FILTER_TENTATIVES
        .iter()
        .find(|candidate| php_symbol_key(candidate.name) == php_symbol_key(&method.name))
}

/// Whether php would refuse this declaration, or `None` when the answer cannot be certain.
///
/// MEASURED on `php -n` 8.5.6, one shape per run, over `filter` and `onCreate`/`onClose`:
/// - a parameter position the parent has and the child cannot be called with is a fatal, whether
///   the child is simply shorter (`filter($in, $out)`) or ends early with no variadic;
/// - by-reference must match EXACTLY at every position the parent declares — losing the `&` on
///   `$consumed` and adding one to `$in` are both fatals, and a by-value `...$args` covering the
///   by-reference `$consumed` is a fatal for the same reason;
/// - the child's declared type must accept everything the parent's does (see `child_type_accepts`);
/// - a parameter BEYOND the parent's list must be optional, because php calls the method with
///   exactly the parent's arguments: `onCreate($var)` is a fatal and `onCreate($var = 1)` is not.
///
/// A wrong RETURN type is NOT part of this: php declares these three tentatively, so it answers
/// with the deprecation above instead.
fn declaration_is_incompatible(method: &ClassMethod, parent: &TentativeMethod) -> Option<bool> {
    for (index, expected) in parent.params.iter().enumerate() {
        let Some((child_type, child_by_ref)) = covering_parameter(method, index) else {
            return Some(true);
        };
        if child_by_ref != expected.by_ref {
            return Some(true);
        }
        if !child_type_accepts(expected.ty, child_type)? {
            return Some(true);
        }
    }
    for (_, _, default, _) in method.params.iter().skip(parent.params.len()) {
        if default.is_none() {
            return Some(true);
        }
    }
    Some(false)
}

/// The child parameter that receives the parent's argument at `index`: a declared one, else the
/// variadic that collects everything past them.
fn covering_parameter(method: &ClassMethod, index: usize) -> Option<(Option<&TypeExpr>, bool)> {
    if let Some((_, type_expr, _, by_ref)) = method.params.get(index) {
        return Some((type_expr.as_ref(), *by_ref));
    }
    method
        .variadic
        .as_ref()
        .map(|_| (method.variadic_type.as_ref(), method.variadic_by_ref))
}

/// Whether the child's declared type accepts everything the parent's does.
///
/// MEASURED against `php_user_filter::filter`'s two shapes. Where php declares no type the
/// parameter is `mixed`, and only an absent or `mixed` child type widens to it — `string $in` is
/// a fatal. Where php declares `bool`, an absent, `mixed`, `bool` or `?bool` child type is
/// accepted and `int` is a fatal.
///
/// `None` means "cannot be certain", which today is exactly a union: php prints unions in its own
/// canonical order (`int|string` comes back as `string|int`), so a message about one could not be
/// reproduced from the source anyway. `bool|int $closing` is therefore accepted in silence, and
/// `int|string $closing` — a fatal in php — goes unreported. A union in a stream filter's
/// signature is the known gap here.
fn child_type_accepts(parent: Option<&str>, child: Option<&TypeExpr>) -> Option<bool> {
    let Some(child) = child else {
        return Some(true);
    };
    // `?bool` accepts every `bool`, so the nullable wrapper is transparent to this question.
    let rendered = match child {
        TypeExpr::Nullable(inner) => render_type(inner)?,
        other => render_type(other)?,
    };
    // php's type names are case-insensitive: `Mixed $in` and `BOOL $closing` link exactly as
    // their lowercase spellings do, so the comparison must not be the rendering's.
    if rendered.eq_ignore_ascii_case("mixed") {
        return Some(true);
    }
    match parent {
        None => Some(false),
        Some(name) => Some(rendered.eq_ignore_ascii_case(name)),
    }
}

/// Renders `Class::method(params): ret` the way php names the child in the fatal.
///
/// php omits the return type when the child declares none — MEASURED on a body written
/// `function filter($in, $out, &$consumed) {}`, whose fatal ends at the closing parenthesis.
fn render_declaration(class: &FlattenedClass, method: &ClassMethod) -> Option<String> {
    let params = render_parameters(method)?;
    let returns = match &method.return_type {
        None => String::new(),
        Some(type_expr) => format!(": {}", render_type(type_expr)?),
    };
    Some(format!(
        "{}::{}({}){}",
        class.name, method.name, params, returns
    ))
}

/// Whether a class reaches `php_user_filter` through its parent chain.
///
/// A class that merely INHERITS an untyped override gets no notice: php raises it where the method
/// is DECLARED, so only the declaring class is walked here.
fn descends_from_user_filter(
    class: &FlattenedClass,
    class_map: &HashMap<String, FlattenedClass>,
) -> bool {
    let mut current = class;
    // The chain is finite and short; the bound only stops a cycle a malformed map could carry.
    for _ in 0..64 {
        let Some(parent) = current.extends.as_deref() else {
            return false;
        };
        let parent = parent.trim_start_matches('\\');
        if php_symbol_key(parent) == php_symbol_key("php_user_filter") {
            return true;
        }
        let Some(next) = class_map
            .values()
            .find(|candidate| php_symbol_key(&candidate.name) == php_symbol_key(parent))
        else {
            return false;
        };
        current = next;
    }
    false
}

/// Whether the method carries `#[\ReturnTypeWillChange]`, php's opt-out from this notice.
fn has_return_type_will_change(method: &ClassMethod) -> bool {
    method.attributes.iter().any(|group| {
        group.attributes.iter().any(|attribute| {
            php_symbol_key(attribute.name.to_string().trim_start_matches('\\'))
                == php_symbol_key("ReturnTypeWillChange")
        })
    })
}

/// Renders a method's parameter list the way php prints it, or `None` when it cannot be certain.
///
/// php prints what was DECLARED: the type if there is one, `&` for by-reference, ` = <value>` for
/// a default, and `...` before the name of a variadic. Returning `None` is deliberate — a message
/// that differs from php's would be worse than none — and what it declines is the shape php
/// renders differently from the source: a union, which php reorders into its own canonical order.
fn render_parameters(method: &ClassMethod) -> Option<String> {
    let mut rendered = Vec::with_capacity(method.params.len() + 1);
    for (name, type_expr, default, by_ref) in &method.params {
        let mut piece = String::new();
        if let Some(type_expr) = type_expr {
            piece.push_str(&render_type(type_expr)?);
            piece.push(' ');
        }
        if *by_ref {
            piece.push('&');
        }
        piece.push('$');
        piece.push_str(name);
        if let Some(default) = default {
            piece.push_str(" = ");
            piece.push_str(&render_default(default)?);
        }
        rendered.push(piece);
    }
    // MEASURED: php writes the element type first, then `&`, then `...$name` — `bool ...$rest`
    // and `&...$rest`, the same order it uses for a fixed parameter.
    if let Some(name) = &method.variadic {
        let mut piece = String::new();
        if let Some(type_expr) = &method.variadic_type {
            piece.push_str(&render_type(type_expr)?);
            piece.push(' ');
        }
        if method.variadic_by_ref {
            piece.push('&');
        }
        piece.push_str("...$");
        piece.push_str(name);
        rendered.push(piece);
    }
    Some(rendered.join(", "))
}

/// Renders a declared type the way php prints it in this message.
fn render_type(type_expr: &TypeExpr) -> Option<String> {
    Some(match type_expr {
        TypeExpr::Int => "int".to_string(),
        TypeExpr::Float => "float".to_string(),
        TypeExpr::Bool => "bool".to_string(),
        TypeExpr::False => "false".to_string(),
        TypeExpr::Str => "string".to_string(),
        TypeExpr::Void => "void".to_string(),
        TypeExpr::Never => "never".to_string(),
        TypeExpr::Iterable => "iterable".to_string(),
        TypeExpr::Array(_) => "array".to_string(),
        TypeExpr::Named(name) => name.to_string().trim_start_matches('\\').to_string(),
        TypeExpr::Nullable(inner) => format!("?{}", render_type(inner)?),
        // php reorders union members into its own canonical order, which this cannot reproduce
        // from the source alone: `int|string` prints as `string|int`.
        _ => return None,
    })
}

/// Renders a default value the way php prints it, for the literals it can be certain of.
fn render_default(default: &Expr) -> Option<String> {
    Some(match &default.kind {
        ExprKind::Null => "null".to_string(),
        ExprKind::BoolLiteral(true) => "true".to_string(),
        ExprKind::BoolLiteral(false) => "false".to_string(),
        ExprKind::IntLiteral(value) => value.to_string(),
        ExprKind::StringLiteral(value) => format!("'{}'", value),
        ExprKind::ArrayLiteral(items) if items.is_empty() => "[]".to_string(),
        _ => return None,
    })
}
