//! Purpose:
//! Defines the backend-neutral contract for one PHP builtin class-like symbol (class,
//! interface, enum, or trait) that elephc ships.
//!
//! Called from:
//! - Shared class catalog declarations (`crate::catalog_classes`).
//! - Compiler and Magician class registries when proving every catalogued class-like is
//!   reachable, and the documentation exporter.
//!
//! Key details:
//! - Every PHP-visible builtin class-like elephc provides must have exactly one contract here,
//!   with its owning PHP module. Standalone class-name lists in the compiler are derived from
//!   this catalog, never maintained beside it.
//! - The contract records HOW the compiler provides the declaration (`ClassRoute`) so the
//!   backend joins know which mechanism to audit.

use crate::{BuiltinId, PhpModule, PhpVersion};

/// PHP class-like flavour of a catalogued symbol.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClassKind {
    /// Ordinary or abstract/final class.
    Class,
    /// Interface.
    Interface,
    /// Pure or backed enum.
    Enum,
    /// Trait.
    Trait,
}

impl ClassKind {
    /// Returns the lowercase PHP keyword for this kind.
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::Class => "class",
            Self::Interface => "interface",
            Self::Enum => "enum",
            Self::Trait => "trait",
        }
    }
}

/// How the compiler provides a builtin class-like declaration.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClassRoute {
    /// Synthetic declaration injected by the type checker and materialized by runtime
    /// metadata (SPL, the Throwable hierarchy, date/time, Reflection, builtin enums).
    CheckerInjected,
    /// Declared in PHP by an injected elephc prelude (PDO, mysqli, curl handles, GD, sessions).
    Prelude,
    /// Engine-level type the front end knows natively (`Closure`, `Generator`, `Fiber`,
    /// `stdClass`).
    LanguageIntrinsic,
}

/// Canonical backend-neutral contract for one PHP builtin class-like.
#[derive(Clone, Copy, Debug)]
pub struct ClassContract {
    /// Stable identity derived from the canonical lowercase name (namespace included).
    pub id: BuiltinId,
    /// PHP spelling as `get_class()` reports it, e.g. `ArrayIterator` or `Pdo\Mysql`.
    pub name: &'static str,
    /// Class, interface, enum, or trait.
    pub kind: ClassKind,
    /// PHP module (php-src extension) that owns this name, or `Elephc`.
    pub module: PhpModule,
    /// First PHP minor release that ships this name; `None` means every supported profile.
    pub since: Option<PhpVersion>,
    /// How the compiler provides the declaration.
    pub aot: ClassRoute,
    /// Whether strict-PHP hides this elephc-only surface.
    pub extension: bool,
    /// Whether this is a compiler/runtime-internal helper class, never PHP-visible.
    pub internal: bool,
    /// PHP manual fragment (`class.arrayiterator`), when one exists.
    pub php_manual: Option<&'static str>,
}

impl ClassContract {
    /// Returns the canonical lowercase lookup key.
    pub fn canonical_name(&self) -> String {
        self.name.to_ascii_lowercase()
    }
}
