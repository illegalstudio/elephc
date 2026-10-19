//! Purpose:
//! Folds include-path conditions that only depend on the compile profile's PHP version, so
//! `require PHP_VERSION < 8.2 ? A : B` picks its branch at compile time.
//!
//! Called from:
//! - `crate::resolver::include_path::fold_include_path` for a ternary path.
//!
//! Key details:
//! - The values are the ones `codegen_support::prescan` bakes for the profile (`PHP_VERSION`,
//!   `PHP_VERSION_ID`, `PHP_MAJOR_VERSION`, `PHP_MINOR_VERSION`, `PHP_RELEASE_VERSION`), read from
//!   the same `compile_php_version()`.
//! - Comparisons replicate PHP 8 exactly: a non-numeric string against a number compares as
//!   strings (`"8.5.0" < 8.2` is `strcmp("8.5.0", "8.2") < 0`, false), numeric strings compare
//!   numerically. Anything this module cannot reproduce exactly (an odd float, an unknown
//!   function, an invalid `version_compare` operator) is NOT folded, so the site stays dynamic.
//! - A condition is only folded when it actually reads a version constant: `true ? A : B` keeps
//!   its "ternary path selection" diagnostic.

use std::cmp::Ordering;

use crate::names::Name;
use crate::parser::ast::{BinOp, Expr, ExprKind};

use super::super::state::{resolve_constant_ref, ResolveState};

/// A compile-time scalar.
#[derive(Clone, Debug, PartialEq)]
enum Value {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
}

/// Folds `condition` to a bool when it is a version check the profile decides.
///
/// `shadowed` lists the version names the site's namespace declares itself (functions or
/// constants); an UNQUALIFIED use of one of them is not folded, because PHP resolves it to the
/// namespaced declaration, not the global. A fully qualified `\\PHP_VERSION` still folds.
pub(in crate::resolver) fn fold_version_condition(
    condition: &Expr,
    state: &ResolveState,
    shadowed: &[String],
) -> Option<bool> {
    let php_version = crate::codegen_support::compile_php_version();
    let profile = Profile {
        shadowed: shadowed.to_vec(),
        version: php_version.version_string().to_string(),
        version_id: i64::from(php_version.version_id()),
        major: i64::from(php_version.major()),
        minor: i64::from(php_version.minor()),
        release: i64::from(php_version.release()),
    };
    let mut uses_version = false;
    let value = eval(condition, state, &profile, &mut uses_version)?;
    uses_version.then(|| truthy(&value))
}

/// The version surface of one profile.
struct Profile {
    shadowed: Vec<String>,
    version: String,
    version_id: i64,
    major: i64,
    minor: i64,
    release: i64,
}

/// Evaluates a pure expression over literals, version constants and `version_compare`.
fn eval(expr: &Expr, state: &ResolveState, profile: &Profile, uses_version: &mut bool) -> Option<Value> {
    match &expr.kind {
        ExprKind::BoolLiteral(b) => Some(Value::Bool(*b)),
        ExprKind::IntLiteral(i) => Some(Value::Int(*i)),
        ExprKind::FloatLiteral(f) => Some(Value::Float(*f)),
        ExprKind::StringLiteral(s) => Some(Value::Str(s.clone())),
        ExprKind::ConstRef(name) => {
            if let Some(value) = resolve_constant_ref(name, state) {
                return Some(Value::Str(value));
            }
            let value = version_constant(name, profile)?;
            *uses_version = true;
            Some(value)
        }
        ExprKind::Not(inner) => Some(Value::Bool(!truthy(&eval(inner, state, profile, uses_version)?))),
        ExprKind::BinaryOp { left, op, right } => {
            let l = eval(left, state, profile, uses_version)?;
            match op {
                BinOp::And => {
                    if !truthy(&l) {
                        return Some(Value::Bool(false));
                    }
                    Some(Value::Bool(truthy(&eval(right, state, profile, uses_version)?)))
                }
                BinOp::Or => {
                    if truthy(&l) {
                        return Some(Value::Bool(true));
                    }
                    Some(Value::Bool(truthy(&eval(right, state, profile, uses_version)?)))
                }
                _ => {
                    let r = eval(right, state, profile, uses_version)?;
                    compare_op(op.clone(), &l, &r)
                }
            }
        }
        ExprKind::FunctionCall { name, args }
            if is_global_function(name, "version_compare")
                && !(name.is_unqualified() && profile.shadowed.iter().any(|s| s == "version_compare")) =>
        {
            if args.len() < 2 || args.len() > 3 {
                return None;
            }
            let Value::Str(a) = eval(&args[0], state, profile, uses_version)? else {
                return None;
            };
            let Value::Str(b) = eval(&args[1], state, profile, uses_version)? else {
                return None;
            };
            let cmp = version_compare(&a, &b);
            if args.len() == 2 {
                return Some(Value::Int(cmp));
            }
            let Value::Str(op) = eval(&args[2], state, profile, uses_version)? else {
                return None;
            };
            let result = match op.as_str() {
                "<" | "lt" => cmp == -1,
                "<=" | "le" => cmp != 1,
                ">" | "gt" => cmp == 1,
                ">=" | "ge" => cmp != -1,
                "==" | "eq" => cmp == 0,
                "!=" | "<>" | "ne" => cmp != 0,
                // PHP 8 throws a ValueError: not a fold.
                _ => return None,
            };
            Some(Value::Bool(result))
        }
        _ => None,
    }
}

/// Whether `name` designates the global function `function` (unqualified falls back to global).
fn is_global_function(name: &Name, function: &str) -> bool {
    (name.is_unqualified() || name.is_fully_qualified())
        && name.parts.len() == 1
        && name.parts[0].eq_ignore_ascii_case(function)
}

/// The profile value of a version constant.
fn version_constant(name: &Name, profile: &Profile) -> Option<Value> {
    if !(name.is_unqualified() || name.is_fully_qualified()) || name.parts.len() != 1 {
        return None;
    }
    if name.is_unqualified() && profile.shadowed.iter().any(|s| s == &name.parts[0]) {
        return None;
    }
    match name.parts[0].as_str() {
        "PHP_VERSION" => Some(Value::Str(profile.version.clone())),
        "PHP_VERSION_ID" => Some(Value::Int(profile.version_id)),
        "PHP_MAJOR_VERSION" => Some(Value::Int(profile.major)),
        "PHP_MINOR_VERSION" => Some(Value::Int(profile.minor)),
        "PHP_RELEASE_VERSION" => Some(Value::Int(profile.release)),
        _ => None,
    }
}

/// PHP truthiness of a scalar.
fn truthy(value: &Value) -> bool {
    match value {
        Value::Bool(b) => *b,
        Value::Int(i) => *i != 0,
        Value::Float(f) => *f != 0.0,
        Value::Str(s) => !(s.is_empty() || s == "0"),
    }
}

/// Applies a comparison operator with PHP 8 semantics.
fn compare_op(op: BinOp, l: &Value, r: &Value) -> Option<Value> {
    let result = match op {
        BinOp::StrictEq => strict_eq(l, r),
        BinOp::StrictNotEq => !strict_eq(l, r),
        BinOp::Eq => loose_compare(l, r)? == Ordering::Equal,
        BinOp::NotEq => loose_compare(l, r)? != Ordering::Equal,
        BinOp::Lt => loose_compare(l, r)? == Ordering::Less,
        BinOp::LtEq => loose_compare(l, r)? != Ordering::Greater,
        BinOp::Gt => loose_compare(l, r)? == Ordering::Greater,
        BinOp::GtEq => loose_compare(l, r)? != Ordering::Less,
        BinOp::Spaceship => {
            return Some(Value::Int(match loose_compare(l, r)? {
                Ordering::Less => -1,
                Ordering::Equal => 0,
                Ordering::Greater => 1,
            }))
        }
        _ => return None,
    };
    Some(Value::Bool(result))
}

/// `===` between scalars.
fn strict_eq(l: &Value, r: &Value) -> bool {
    match (l, r) {
        (Value::Float(a), Value::Float(b)) => a == b,
        _ => l == r,
    }
}

/// A number parsed from a numeric string.
#[derive(Clone, Copy)]
enum Number {
    Int(i64),
    Float(f64),
}

impl Number {
    fn as_f64(self) -> f64 {
        match self {
            Number::Int(i) => i as f64,
            Number::Float(f) => f,
        }
    }
}

/// PHP 8 `<=>` between two scalars, `None` when it cannot be reproduced exactly.
fn loose_compare(l: &Value, r: &Value) -> Option<Ordering> {
    match (l, r) {
        (Value::Bool(_), _) | (_, Value::Bool(_)) => Some(truthy(l).cmp(&truthy(r))),
        (Value::Int(a), Value::Int(b)) => Some(a.cmp(b)),
        (Value::Int(_) | Value::Float(_), Value::Int(_) | Value::Float(_)) => {
            compare_numbers(number_of(l)?, number_of(r)?)
        }
        (Value::Str(a), Value::Str(b)) => match (numeric_string(a), numeric_string(b)) {
            (Some(x), Some(y)) => compare_numbers(x, y),
            _ => Some(php_strcmp(a, b)),
        },
        (Value::Str(s), number) | (number, Value::Str(s)) => {
            let flipped = matches!(l, Value::Int(_) | Value::Float(_));
            let n = number_of(number)?;
            let ordering = match numeric_string(s) {
                Some(parsed) => compare_numbers(parsed, n)?,
                None => php_strcmp(s, &number_to_string(n)?),
            };
            Some(if flipped { ordering.reverse() } else { ordering })
        }
    }
}

fn number_of(value: &Value) -> Option<Number> {
    match value {
        Value::Int(i) => Some(Number::Int(*i)),
        Value::Float(f) => Some(Number::Float(*f)),
        _ => None,
    }
}

fn compare_numbers(a: Number, b: Number) -> Option<Ordering> {
    match (a, b) {
        (Number::Int(x), Number::Int(y)) => Some(x.cmp(&y)),
        _ => a.as_f64().partial_cmp(&b.as_f64()),
    }
}

/// `zend_binary_strcmp`: bytes first, then length.
fn php_strcmp(a: &str, b: &str) -> Ordering {
    a.as_bytes().cmp(b.as_bytes())
}

/// PHP's string form of a number (`precision = 14`), only where Rust's shortest form is
/// guaranteed to be the same text.
fn number_to_string(n: Number) -> Option<String> {
    match n {
        Number::Int(i) => Some(i.to_string()),
        Number::Float(f) => {
            if !f.is_finite() {
                return None;
            }
            if f == f.trunc() && f.abs() < 1e15 {
                return Some(format!("{}", f as i64));
            }
            let text = format!("{}", f);
            let digits = text.bytes().filter(u8::is_ascii_digit).count();
            let leading_zeros = text
                .trim_start_matches('-')
                .trim_start_matches("0.")
                .bytes()
                .take_while(|b| *b == b'0')
                .count();
            if text.contains('e') || digits - leading_zeros > 14 || f.abs() < 1e-4 || f.abs() >= 1e15 {
                return None;
            }
            Some(text)
        }
    }
}

/// PHP 8 `is_numeric_string`: optional surrounding whitespace, sign, digits, fraction, exponent.
fn numeric_string(s: &str) -> Option<Number> {
    let is_ws = |c: char| matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c');
    let t = s.trim_start_matches(is_ws).trim_end_matches(is_ws);
    if t.is_empty() {
        return None;
    }
    let bytes = t.as_bytes();
    let mut i = 0;
    if bytes[i] == b'+' || bytes[i] == b'-' {
        i += 1;
    }
    let int_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    let int_digits = i - int_start;
    let mut is_float = false;
    let mut frac_digits = 0;
    if i < bytes.len() && bytes[i] == b'.' {
        is_float = true;
        i += 1;
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        frac_digits = i - start;
    }
    if int_digits == 0 && frac_digits == 0 {
        return None;
    }
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        let mut j = i + 1;
        if j < bytes.len() && (bytes[j] == b'+' || bytes[j] == b'-') {
            j += 1;
        }
        let start = j;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
        if j > start {
            is_float = true;
            i = j;
        }
    }
    if i != bytes.len() {
        return None;
    }
    if !is_float {
        if let Ok(v) = t.parse::<i64>() {
            return Some(Number::Int(v));
        }
    }
    t.parse::<f64>().ok().map(Number::Float)
}

/// php-src `php_version_compare`, returning -1, 0 or 1.
pub(super) fn version_compare(a: &str, b: &str) -> i64 {
    if a.is_empty() || b.is_empty() {
        return match (a.is_empty(), b.is_empty()) {
            (true, true) => 0,
            (false, true) => 1,
            _ => -1,
        };
    }
    let v1 = if a.starts_with('#') { a.to_string() } else { canonicalize_version(a) };
    let v2 = if b.starts_with('#') { b.to_string() } else { canonicalize_version(b) };
    let p1: Vec<&str> = v1.split('.').collect();
    let p2: Vec<&str> = v2.split('.').collect();
    let starts_digit = |s: &str| s.bytes().next().is_some_and(|c| c.is_ascii_digit());
    let mut compare = 0;
    let mut i = 0;
    while i < p1.len() && i < p2.len() && compare == 0 {
        let (x, y) = (p1[i], p2[i]);
        compare = if starts_digit(x) && starts_digit(y) {
            sign(strtol(x).cmp(&strtol(y)))
        } else if !starts_digit(x) && !starts_digit(y) {
            compare_special_forms(x, y)
        } else if starts_digit(x) {
            compare_special_forms("#N#", y)
        } else {
            compare_special_forms(x, "#N#")
        };
        i += 1;
    }
    if compare == 0 {
        if i < p1.len() {
            compare = if starts_digit(p1[i]) {
                1
            } else {
                version_compare(&p1[i..].join("."), "#N#")
            };
        } else if i < p2.len() {
            compare = if starts_digit(p2[i]) {
                -1
            } else {
                version_compare("#N#", &p2[i..].join("."))
            };
        }
    }
    compare
}

fn sign(ordering: Ordering) -> i64 {
    match ordering {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
}

/// C `strtol` on a leading run of digits.
fn strtol(s: &str) -> i64 {
    let digits: String = s.chars().take_while(char::is_ascii_digit).collect();
    digits.parse::<i64>().unwrap_or(i64::MAX)
}

/// php-src `compare_special_version_forms`.
fn compare_special_forms(a: &str, b: &str) -> i64 {
    const FORMS: [(&str, i64); 10] = [
        ("dev", 0),
        ("alpha", 1),
        ("a", 1),
        ("beta", 2),
        ("b", 2),
        ("RC", 3),
        ("rc", 3),
        ("#", 4),
        ("pl", 5),
        ("p", 5),
    ];
    let order = |form: &str| {
        FORMS
            .iter()
            .find(|(name, _)| form.as_bytes().starts_with(name.as_bytes()))
            .map(|(_, order)| *order)
            .unwrap_or(-1)
    };
    sign(order(a).cmp(&order(b)))
}

/// php-src `php_canonicalize_version`.
fn canonicalize_version(version: &str) -> String {
    let bytes = version.as_bytes();
    let is_dig = |c: u8| c.is_ascii_digit();
    let is_ndig = |c: u8| !c.is_ascii_digit() && c != b'.';
    let is_special = |c: u8| matches!(c, b'-' | b'_' | b'+');
    let mut out: Vec<u8> = vec![bytes[0]];
    let mut lp = bytes[0];
    for &c in &bytes[1..] {
        let last = *out.last().unwrap();
        if is_special(c) {
            if last != b'.' {
                out.push(b'.');
            }
        } else if (is_ndig(lp) && is_dig(c)) || (is_dig(lp) && is_ndig(c)) {
            if last != b'.' {
                out.push(b'.');
            }
            out.push(c);
        } else if !c.is_ascii_alphanumeric() {
            if last != b'.' {
                out.push(b'.');
            }
        } else {
            out.push(c);
        }
        lp = c;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_numeric_version_string_against_float_compares_as_strings() {
        // MEASURED with php 8.5: var_dump("8.5.0" < 8.2) => false, ("8.0.0" < 8.2) => true,
        // ("8.2.0" < 8.2) => false, ("8.10.0" < 8.2) => true.
        let lt = |s: &str, f: f64| {
            compare_op(BinOp::Lt, &Value::Str(s.into()), &Value::Float(f)) == Some(Value::Bool(true))
        };
        assert!(!lt("8.5.0", 8.2));
        assert!(lt("8.0.0", 8.2));
        assert!(!lt("8.2.0", 8.2));
        assert!(lt("8.10.0", 8.2));
        assert!(lt("8.1.0", 8.2));
    }

    #[test]
    fn numeric_strings_compare_numerically() {
        let eq = compare_op(BinOp::Eq, &Value::Str("8.20".into()), &Value::Float(8.2));
        assert_eq!(eq, Some(Value::Bool(true)));
        let gt = compare_op(BinOp::Gt, &Value::Str("10".into()), &Value::Str("9".into()));
        assert_eq!(gt, Some(Value::Bool(true)));
    }

    #[test]
    fn version_compare_matches_php() {
        assert_eq!(version_compare("8.5.0", "8.2"), 1);
        assert_eq!(version_compare("8.2.0", "8.2"), 1);
        assert_eq!(version_compare("8.2.0", "8.2.0"), 0);
        assert_eq!(version_compare("8.1.0", "8.2.0"), -1);
        assert_eq!(version_compare("8.2.0", "8.2.0-dev"), 1);
        assert_eq!(version_compare("8.2.0RC1", "8.2.0"), -1);
        assert_eq!(version_compare("1.0rc1", "1.0"), -1);
        assert_eq!(version_compare("1.0pl1", "1.0"), 1);
        assert_eq!(version_compare("5.2", "5.10"), -1);
    }

    /// The CarbonPeriod condition and its `version_compare` / `PHP_*_VERSION` spellings, at the
    /// 8.2 border, under every compile profile.
    #[test]
    fn version_conditions_fold_per_profile() {
        use crate::names::Name;
        use crate::span::Span;
        use crate::web_prelude::PhpVersion;
        let constant = |name: &str| Expr::new(ExprKind::ConstRef(Name::unqualified(name)), Span::dummy());
        let binary = |left: Expr, op: BinOp, right: Expr| {
            Expr::new(ExprKind::BinaryOp { left: Box::new(left), op, right: Box::new(right) }, Span::dummy())
        };
        let string = |s: &str| Expr::new(ExprKind::StringLiteral(s.to_string()), Span::dummy());
        let lt_float = binary(constant("PHP_VERSION"), BinOp::Lt, Expr::new(ExprKind::FloatLiteral(8.2), Span::dummy()));
        let compare = Expr::new(
            ExprKind::FunctionCall {
                name: Name::unqualified("version_compare"),
                args: vec![constant("PHP_VERSION"), string("8.2.0"), string("<")],
            },
            Span::dummy(),
        );
        let minor = binary(constant("PHP_MINOR_VERSION"), BinOp::Lt, Expr::new(ExprKind::IntLiteral(2), Span::dummy()));
        let id = binary(constant("PHP_VERSION_ID"), BinOp::Lt, Expr::new(ExprKind::IntLiteral(80200), Span::dummy()));
        let state = ResolveState::default();
        for version in PhpVersion::ALL {
            crate::codegen::set_compile_profile(version, false);
            let below = version.version_id() < 80_200;
            for condition in [&lt_float, &compare, &minor, &id] {
                assert_eq!(fold_version_condition(condition, &state, &[]), Some(below), "{:?}", version);
            }
        }
        crate::codegen::set_compile_profile(PhpVersion::default(), false);
        // A condition that reads no version constant is not folded.
        assert_eq!(fold_version_condition(&Expr::new(ExprKind::BoolLiteral(true), Span::dummy()), &state, &[]), None);
    }
}
