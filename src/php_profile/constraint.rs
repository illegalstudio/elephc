//! Purpose:
//! Evaluates a Composer version constraint (`"^8.2"`, `"~8.3.0"`, `">=8.2 <8.5"`) against the
//! maintained profile set, so a project that declares only `require.php` can still narrow the
//! profile elephc compiles for.
//!
//! Called from:
//! - `crate::php_profile::resolve`, as the last source before the default.
//!
//! Key details:
//!
//! - THE `semver` CRATE CANNOT DO THIS. Composer and Cargo spell the same operators with
//!   different meanings: Composer's `~8.2` is `>=8.2 <9.0` while Cargo's is `>=8.2 <8.3`.
//!   Reusing a Cargo-semantics parser would silently misread the single most common
//!   narrowing operator, so the grammar below is Composer's.
//!
//! - PARSE FAILURE IS `None`, NEVER AN ERROR. An unsupported spelling (hyphen ranges,
//!   stability flags) leaves the profile at the default, which is what would have happened
//!   before this module existed. A constraint elephc cannot read must not fail a build or,
//!   worse, be half-read into a wrong answer.
//!
//! - COMPARISON IS ON THREE COMPONENTS because Composer's are: `8.2` normalizes to `8.2.0`,
//!   which is exactly how a profile is spelled (see `PhpVersion::version_string` for the
//!   patch-is-zero rule), so the two line up without special cases.

use crate::web_prelude::PhpVersion;

/// A version as Composer compares them.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct Version {
    /// Major component.
    major: u32,
    /// Minor component.
    minor: u32,
    /// Patch component.
    patch: u32,
}

impl Version {
    /// The version a profile presents to a constraint.
    fn of(profile: PhpVersion) -> Self {
        Self {
            major: profile.major(),
            minor: profile.minor(),
            patch: profile.release(),
        }
    }
}

/// A comparison operator.
#[derive(Clone, Copy, Debug)]
enum Op {
    /// `>=`
    Ge,
    /// `>`
    Gt,
    /// `<=`
    Le,
    /// `<`
    Lt,
    /// `=` / `==` / a bare version
    Eq,
    /// `!=` / `<>`
    Ne,
}

/// One comparison a version must satisfy.
#[derive(Clone, Copy, Debug)]
struct Clause {
    /// How to compare.
    op: Op,
    /// What to compare against.
    version: Version,
}

impl Clause {
    /// Returns whether `candidate` satisfies this clause.
    fn admits(self, candidate: Version) -> bool {
        match self.op {
            Op::Ge => candidate >= self.version,
            Op::Gt => candidate > self.version,
            Op::Le => candidate <= self.version,
            Op::Lt => candidate < self.version,
            Op::Eq => candidate == self.version,
            Op::Ne => candidate != self.version,
        }
    }
}

/// Parses a dotted numeric version, tolerating one or two components and a stability suffix.
///
/// Missing components are zero, matching Composer's normalization: `8.2` is `8.2.0`.
fn parse_version(raw: &str) -> Option<Version> {
    let raw = raw.split(['-', '+', '@']).next()?.trim();
    if raw.is_empty() {
        return None;
    }
    let mut parts = raw.split('.');
    let major = parts.next()?.parse::<u32>().ok()?;
    let minor = parts.next().unwrap_or("0").parse::<u32>().ok()?;
    let patch = parts.next().unwrap_or("0").parse::<u32>().ok()?;
    if parts.next().is_some() {
        // A fourth component is Composer-legal but never meaningful for a language profile.
        return Some(Version { major, minor, patch });
    }
    Some(Version { major, minor, patch })
}

/// Expands one constraint term into the clauses it stands for.
///
/// Returns `None` for anything this parser does not understand, which propagates all the way
/// out as "no answer" rather than a partial one.
fn parse_term(term: &str) -> Option<Vec<Clause>> {
    let term = term.trim();
    if term.is_empty() {
        return None;
    }
    // `*` and `8.*`-style wildcards.
    if term == "*" {
        return Some(Vec::new());
    }
    if let Some(prefix) = term.strip_suffix(".*") {
        let base = parse_version(prefix)?;
        let components = prefix.split('.').count();
        let upper = match components {
            1 => Version { major: base.major + 1, minor: 0, patch: 0 },
            2 => Version { major: base.major, minor: base.minor + 1, patch: 0 },
            _ => return None,
        };
        return Some(vec![
            Clause { op: Op::Ge, version: base },
            Clause { op: Op::Lt, version: upper },
        ]);
    }
    // `^8.2` — up to but excluding the next MAJOR.
    if let Some(rest) = term.strip_prefix('^') {
        let base = parse_version(rest)?;
        let upper = Version { major: base.major + 1, minor: 0, patch: 0 };
        return Some(vec![
            Clause { op: Op::Ge, version: base },
            Clause { op: Op::Lt, version: upper },
        ]);
    }
    // `~8.2` — up to the next MAJOR with two components, next MINOR with three. This is the
    // operator whose meaning differs from Cargo's, and the reason this parser exists.
    if let Some(rest) = term.strip_prefix('~') {
        let base = parse_version(rest)?;
        let components = rest.split(['-', '+', '@']).next()?.split('.').count();
        let upper = match components {
            1 => Version { major: base.major + 1, minor: 0, patch: 0 },
            2 => Version { major: base.major + 1, minor: 0, patch: 0 },
            _ => Version { major: base.major, minor: base.minor + 1, patch: 0 },
        };
        return Some(vec![
            Clause { op: Op::Ge, version: base },
            Clause { op: Op::Lt, version: upper },
        ]);
    }
    for (prefix, op) in [
        (">=", Op::Ge),
        ("<=", Op::Le),
        ("!=", Op::Ne),
        ("<>", Op::Ne),
        ("==", Op::Eq),
        (">", Op::Gt),
        ("<", Op::Lt),
        ("=", Op::Eq),
    ] {
        if let Some(rest) = term.strip_prefix(prefix) {
            return Some(vec![Clause {
                op,
                version: parse_version(rest)?,
            }]);
        }
    }
    // A bare version is an exact match in Composer.
    Some(vec![Clause {
        op: Op::Eq,
        version: parse_version(term)?,
    }])
}

/// Parses an inclusive hyphen range (`"8.2 - 8.4"`), or returns `None` when the text is not
/// one.
///
/// The upper bound's arity decides how inclusive it is, which is Composer's rule rather than
/// an approximation: a PARTIAL upper bound admits everything carrying that prefix, so
/// `8.2 - 8.4` is `>=8.2.0 <8.5.0`, while a complete `8.2.0 - 8.4.0` is `>=8.2.0 <=8.4.0`.
///
/// The separator must be a SPACED hyphen. `8.2-dev` is one version with a stability suffix,
/// not a range, and splitting it here would turn a legal constraint into a nonsensical one.
fn parse_hyphen_range(alternative: &str) -> Option<Vec<Clause>> {
    let (low, high) = alternative.split_once(" - ")?;
    let lower = parse_version(low)?;
    let upper_raw = high.trim();
    let upper = parse_version(upper_raw)?;
    let components = upper_raw
        .split(['-', '+', '@'])
        .next()?
        .split('.')
        .count();
    let upper_clause = match components {
        1 => Clause {
            op: Op::Lt,
            version: Version { major: upper.major + 1, minor: 0, patch: 0 },
        },
        2 => Clause {
            op: Op::Lt,
            version: Version { major: upper.major, minor: upper.minor + 1, patch: 0 },
        },
        _ => Clause {
            op: Op::Le,
            version: upper,
        },
    };
    Some(vec![
        Clause {
            op: Op::Ge,
            version: lower,
        },
        upper_clause,
    ])
}

/// Parses a full constraint into alternatives, each a conjunction of clauses.
///
/// `||` separates alternatives; a comma or whitespace conjoins terms within one.
fn parse(constraint: &str) -> Option<Vec<Vec<Clause>>> {
    let mut alternatives = Vec::new();
    for alternative in constraint.split("||") {
        if let Some(clauses) = parse_hyphen_range(alternative) {
            alternatives.push(clauses);
            continue;
        }
        let mut clauses = Vec::new();
        let terms = alternative
            .split([',', ' ', '\t'])
            .filter(|term| !term.trim().is_empty());
        let mut seen = false;
        for term in terms {
            seen = true;
            clauses.extend(parse_term(term)?);
        }
        if !seen {
            return None;
        }
        alternatives.push(clauses);
    }
    if alternatives.is_empty() {
        return None;
    }
    Some(alternatives)
}

/// Returns the NEWEST maintained profile the constraint admits, or `None` when the constraint
/// cannot be read or admits none of them.
///
/// Newest-satisfying is the right end of the range for elephc specifically: a compiled binary
/// IS its own runtime, so the low end of `"^8.2"` is a portability promise with nothing left
/// to be portable to, while the high end is the behavior the project is most likely actually
/// tested against.
pub fn newest_admitted(constraint: &str) -> Option<PhpVersion> {
    let alternatives = parse(constraint)?;
    PhpVersion::MAINTAINED
        .iter()
        .rev()
        .copied()
        .find(|profile| {
            let candidate = Version::of(*profile);
            alternatives
                .iter()
                .any(|clauses| clauses.iter().all(|clause| clause.admits(candidate)))
        })
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for Composer constraint evaluation, with particular attention to the
    //! operators whose meaning differs from Cargo's.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.

    use super::*;

    /// `^8.2` spans to the next major, so the newest maintained profile is admitted.
    #[test]
    fn caret_spans_to_the_next_major() {
        assert_eq!(newest_admitted("^8.2"), Some(PhpVersion::Php85));
    }

    /// COMPOSER's `~8.2` is `>=8.2 <9.0` — NOT Cargo's `>=8.2 <8.3`. Getting this backwards
    /// is the single most likely way to misread a real `composer.json`.
    #[test]
    fn two_component_tilde_spans_to_the_next_major() {
        assert_eq!(newest_admitted("~8.2"), Some(PhpVersion::Php85));
    }

    /// With three components, `~` stops at the next MINOR.
    #[test]
    fn three_component_tilde_stops_at_the_next_minor() {
        assert_eq!(newest_admitted("~8.3.0"), Some(PhpVersion::Php83));
    }

    /// An explicit upper bound is honored.
    #[test]
    fn explicit_range_is_honored() {
        assert_eq!(newest_admitted(">=8.2 <8.5"), Some(PhpVersion::Php84));
        assert_eq!(newest_admitted(">=8.2,<8.4"), Some(PhpVersion::Php83));
    }

    /// A `.*` wildcard bounds the minor.
    #[test]
    fn wildcard_bounds_the_minor() {
        assert_eq!(newest_admitted("8.3.*"), Some(PhpVersion::Php83));
    }

    /// A bare `*` admits everything, so the newest wins.
    #[test]
    fn star_admits_everything() {
        assert_eq!(newest_admitted("*"), Some(PhpVersion::Php85));
    }

    /// Alternatives are unioned, and the newest admitted across all of them wins.
    #[test]
    fn alternatives_are_unioned() {
        assert_eq!(newest_admitted("~8.2.0 || ~8.4.0"), Some(PhpVersion::Php84));
    }

    /// A bare version is an exact match.
    #[test]
    fn bare_version_is_exact() {
        assert_eq!(newest_admitted("8.3"), Some(PhpVersion::Php83));
        assert_eq!(newest_admitted("8.3.0"), Some(PhpVersion::Php83));
    }

    /// `!=` excludes a profile without excluding the rest.
    #[test]
    fn not_equal_excludes_one_profile() {
        assert_eq!(
            newest_admitted(">=8.2 !=8.5.10"),
            Some(PhpVersion::Php84)
        );
    }

    /// A constraint admitting no maintained profile yields `None`, so the caller falls back
    /// rather than inventing an answer.
    #[test]
    fn constraint_below_the_range_admits_nothing() {
        assert_eq!(newest_admitted("~7.4.0"), None);
    }

    /// An unreadable constraint yields `None` rather than a partial reading.
    #[test]
    fn unreadable_constraint_yields_none() {
        assert_eq!(newest_admitted("not a constraint"), None);
        assert_eq!(newest_admitted(""), None);
    }

    /// A hyphen range with a PARTIAL upper bound admits everything carrying that prefix, so
    /// `8.2 - 8.4` reaches 8.4 — Composer's rule, and the trap in reading one as `<8.4.0`.
    #[test]
    fn hyphen_range_with_partial_upper_bound_is_inclusive() {
        assert_eq!(newest_admitted("8.2 - 8.4"), Some(PhpVersion::Php84));
        assert_eq!(newest_admitted("8.2 - 8.3"), Some(PhpVersion::Php83));
    }

    /// A hyphen range with a COMPLETE upper bound is inclusive of exactly that version.
    #[test]
    fn hyphen_range_with_complete_upper_bound_stops_there() {
        assert_eq!(newest_admitted("8.2.0 - 8.3.0"), Some(PhpVersion::Php83));
        // 8.3.1 is above every profile's `.0`, so 8.3 is still the newest admitted.
        assert_eq!(newest_admitted("8.2.0 - 8.3.1"), Some(PhpVersion::Php83));
    }

    /// A hyphen range composes with alternatives.
    #[test]
    fn hyphen_range_works_inside_an_alternative() {
        assert_eq!(
            newest_admitted("8.2.0 - 8.2.0 || 8.3 - 8.3"),
            Some(PhpVersion::Php83)
        );
    }

    /// A stability suffix is NOT a hyphen range: splitting `8.2-dev` on its hyphen would turn
    /// a legal constraint into a nonsensical one, which is why the separator is a SPACED
    /// hyphen.
    #[test]
    fn stability_suffix_is_not_a_hyphen_range() {
        assert_eq!(newest_admitted("^8.3-dev"), Some(PhpVersion::Php85));
        assert_eq!(newest_admitted("~8.3.0-stable"), Some(PhpVersion::Php83));
        assert_eq!(newest_admitted("^8.2@stable"), Some(PhpVersion::Php85));
    }
}
