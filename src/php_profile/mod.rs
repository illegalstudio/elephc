//! Purpose:
//! Everything about WHICH PHP language profile a compilation targets, and about whether that
//! choice is observable in the program being compiled.
//!
//! Called from:
//! - `crate::pipeline::compile()`, to report profile dependence alongside the resolved
//!   profile.
//!
//! Key details:
//! - `--php-version` selects a *semantics profile*, not a compatibility floor. An elephc
//!   binary IS its own runtime: there is no target machine's PHP for it to be compatible
//!   with, so the only question the profile answers is "which PHP's observable behavior
//!   should this binary emulate?".
//! - [`sensitivity`] answers the follow-up question that makes the choice actionable: does
//!   the profile actually change anything *for this program*? For the overwhelming majority
//!   of programs the answer is no, and the compiler stays silent.

pub mod constraint;
pub mod floor;
pub mod resolve;
pub mod sensitivity;

use crate::parser::ast::Stmt;
use crate::web_prelude::PhpVersion;

/// Where the compile profile came from.
///
/// The variant set is deliberately open to growth: the resolution ladder this feature is
/// designed around (an explicit flag, then a Composer platform pin, then `.php-version`, then
/// a `require.php` constraint, then the newest maintained profile) adds variants here without
/// touching the reporting logic, which only distinguishes "the user chose" from "elephc
/// chose".
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Provenance {
    /// An explicit `--php-version` on the command line.
    Flag,
    /// `composer.lock`'s `platform-overrides.php` — what the project actually installed
    /// against, and the highest-confidence declaration available.
    ComposerLock,
    /// `composer.json`'s `config.platform.php` — Composer's own way of saying "resolve as if
    /// PHP were exactly this".
    ComposerPlatform,
    /// A `.php-version` file, the phpenv/asdf toolchain convention.
    PhpVersionFile,
    /// `composer.json`'s `require.php` CONSTRAINT, honored only where it excludes the newest
    /// maintained profile — see `resolve::resolve_in` for why that restriction is what makes
    /// reading a range defensible.
    ComposerRequire,
    /// Nothing selected it; the newest maintained profile was assumed.
    Default,
}

impl Provenance {
    /// A short parenthetical naming the source, for the profile line.
    fn label(self) -> &'static str {
        match self {
            Self::Flag => "--php-version",
            Self::ComposerLock => "composer.lock",
            Self::ComposerPlatform => "composer.json",
            Self::PhpVersionFile => ".php-version",
            Self::ComposerRequire => "composer.json require.php",
            Self::Default => "default",
        }
    }

    /// Whether elephc picked the profile rather than the user, which is the case worth
    /// suggesting a pin for.
    fn is_assumed(self) -> bool {
        matches!(self, Self::Default)
    }
}

/// Returns the error to report when `profile` is too old to run this program.
///
/// `None` — the overwhelming majority — means the program's own constructs are all available
/// in the selected profile, so the version surface the binary reports is one its source could
/// genuinely have run under.
///
/// This can only fail for an explicitly selected profile. The default is the newest
/// maintained one and [`floor::floor`] can never exceed it, so a default compile is never
/// rejected here; the check fires only on a build that was already claiming a version its own
/// source contradicts. See `floor`'s preamble for why every judgement call in computing the
/// floor is made toward under-reporting.
///
/// Runs under `crate::compiler_stack::with_compiler_stack` (issue #1149): the construct scan walks
/// the whole program, so an embedder calling it without the CLI driver's wrapper gets the same
/// stack budget.
pub fn floor_violation(program: &[Stmt], profile: PhpVersion) -> Option<crate::errors::CompileError> {
    crate::compiler_stack::with_compiler_stack(|| {
        floor_violation_on_compiler_stack(program, profile)
    })
}

/// The body of [`floor_violation`], run on the stack `with_compiler_stack` sized.
fn floor_violation_on_compiler_stack(
    program: &[Stmt],
    profile: PhpVersion,
) -> Option<crate::errors::CompileError> {
    let required = floor::floor(program)?;
    if required.profile.version_id() <= profile.version_id() {
        return None;
    }
    // Phrased so the construct name sits in a noun slot: entries are a mix of plurals
    // ("property hooks"), singulars ("typed class constant") and bare function names
    // ("json_validate"), and no single verb agrees with all three.
    Some(crate::errors::CompileError::new(
        required.span,
        &format!(
            "this program needs PHP {} ({}), but --php-version selected {}; \
             a binary built for {} could not have run this source",
            required.profile.spelling(),
            required.construct,
            profile.spelling(),
            profile.spelling(),
        ),
    ))
}

/// Reports how the profile choice is observable in this program, if it is at all.
///
/// Emits NOTHING for a program whose behavior does not depend on the profile, which is the
/// overwhelming majority: a program has to go out of its way — by asking the runtime about
/// its own version, by querying OPcache, or by driving sessions — to notice which profile it
/// was built for. That silence is the point of the feature, not a limitation of it: the
/// compiler speaks only when the user has a real choice to make.
///
/// Reported under BOTH provenances. When the profile was assumed, the report says so and
/// suggests pinning it; when the user passed `--php-version`, the report confirms which
/// constructs that flag is actually governing, so an explicit choice is still visible rather
/// than silently doing work the user cannot see.
///
/// # Why these are `note[…]` lines and not warnings
///
/// Nothing here is wrong with the program. Under an explicit `--php-version` the user made a
/// deliberate choice and this only says what that choice governs; even under the default it
/// reports a fact, not a defect. Routing it through `errors::report_warning` would call a
/// correct program's deliberate decision a warning, and would mix these lines into the
/// `warning[line:col]:` stream that test harnesses and users scan for actual problems — see
/// `tests/opcache_ini_tests::elephc_diagnostics`, which deliberately refuses an allow-list so
/// that an UNEXPECTED warning is never hidden. A distinct prefix keeps that property intact.
///
/// `program` must be the USER program — after include resolution but BEFORE any compiler
/// prelude is injected. The `--web` prelude calls `__elephc_php_version_id()` and defines the
/// session surface itself, so scanning after injection would report every `--web` build as
/// profile-dependent on the strength of elephc's own generated code.
///
/// Runs under `crate::compiler_stack::with_compiler_stack` (issue #1149): the sensitivity scan
/// walks the whole program, so an embedder calling it without the CLI driver's wrapper gets the
/// same stack budget.
pub fn report(program: &[Stmt], web: bool, profile: PhpVersion, provenance: Provenance) {
    crate::compiler_stack::with_compiler_stack(|| {
        report_on_compiler_stack(program, web, profile, provenance)
    })
}

/// The body of [`report`], run on the stack `with_compiler_stack` sized.
fn report_on_compiler_stack(
    program: &[Stmt],
    web: bool,
    profile: PhpVersion,
    provenance: Provenance,
) {
    let found = sensitivity::scan(program, web);
    if found.is_empty() {
        return;
    }

    let plural = if found.len() == 1 { "" } else { "s" };
    eprintln!(
        "php profile {} ({}); {} construct{} depend{} on it{}",
        profile.spelling(),
        provenance.label(),
        found.len(),
        plural,
        if found.len() == 1 { "s" } else { "" },
        if provenance.is_assumed() {
            " — pin it with --php-version to make the choice explicit"
        } else {
            ""
        },
    );
    for item in &found {
        let call = if item.is_function { "()" } else { "" };
        eprintln!(
            "  note[{}:{}]: {}{} {}",
            item.span.line, item.span.col, item.symbol, call, item.detail
        );
    }
}
