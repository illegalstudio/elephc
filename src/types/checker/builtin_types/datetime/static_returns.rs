//! Purpose:
//! Late-static (`static`) return contract of the DateTime/DateTimeImmutable fluent methods.
//!
//! Called from:
//! - DateTime setter construction (`setters.rs`, `setter_helpers.rs`, `factories.rs`).
//! - Class schema override validation (`schema::classes::methods`).
//!
//! Key details:
//! - PHP returns the receiver's class from every fluent method, so a subclass gets itself back:
//!   mutable `DateTime` returns `$this`, `DateTimeImmutable` returns a clone of `$this`.
//! - php-src types most of them as the concrete class only *tentatively* (`@return static` in
//!   the stub), so an override may still declare the concrete class; the override check asks
//!   `is_tentative_late_static_return` before demanding `static` from the child.

use super::*;

/// Fluent instance methods php-src types tentatively as the concrete class while returning
/// `static` at runtime. `setMicrosecond()` is absent: PHP declares it `static` outright.
const TENTATIVE_LATE_STATIC_METHODS: &[&str] = &[
    "modify",
    "add",
    "sub",
    "settimezone",
    "settime",
    "setdate",
    "setisodate",
    "settimestamp",
];

/// Builds the late-static `static` return type declared by the fluent methods.
pub(super) fn late_static_return() -> TypeExpr {
    TypeExpr::Named(Name::unqualified("static"))
}

/// Builds `clone $this` — the receiver-class copy an immutable fluent method mutates and returns,
/// so a subclass instance (and its own properties) survives the call exactly as PHP's clone does.
pub(super) fn clone_this() -> Expr {
    Expr::new(ExprKind::Clone(Box::new(Expr::new(ExprKind::This, dummy()))), dummy())
}

/// Returns whether `owner::method` is a compiler-injected DateTime fluent method whose `static`
/// return PHP only declares tentatively as the class, so an override may narrow it to the
/// concrete class instead of repeating `static`.
pub(crate) fn is_tentative_late_static_return(owner: &str, method: &str) -> bool {
    let owner = owner.trim_start_matches('\\');
    (owner.eq_ignore_ascii_case("DateTime") || owner.eq_ignore_ascii_case("DateTimeImmutable"))
        && TENTATIVE_LATE_STATIC_METHODS.contains(&method.to_ascii_lowercase().as_str())
}
