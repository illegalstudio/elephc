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
    /// Whether the PHP internal object handler permits cloning, including subclasses.
    pub cloneable: bool,
    /// Reviewed internal object handlers; absent for ordinary PHP/userland classes.
    pub operators: Option<&'static crate::ClassOperators>,
    /// Whether creating dynamic properties on this class is allowed but deprecated.
    ///
    /// PHP 8.2+ allows the write and reports `Creation of dynamic property ... is
    /// deprecated`; the compiler usually models that with its storage flags. GMP is a
    /// prelude class that needs the storage WITHOUT the `AllowDynamicProperties` exemption,
    /// so its contract carries the deprecation explicitly.
    pub dynamic_properties_deprecated: bool,
    /// Whether the dump walkers render dynamic properties BEFORE a foldable
    /// `__debugInfo()` projection. PHP's internal GMP debug info dups the normal
    /// property table (dynamic properties included) and then appends `num`, so the
    /// projection's row must come last.
    pub dump_dynamic_properties_first: bool,
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
    /// Supported compiler targets when the PHP class exists only on some platforms.
    /// `None` means the declaration is available on every supported target.
    pub target_support: Option<&'static [&'static str]>,
    /// Whether strict-PHP hides this elephc-only surface.
    pub extension: bool,
    /// Whether this is a compiler/runtime-internal helper class, never PHP-visible.
    pub internal: bool,
    /// Declared property names that are compiler storage and never PHP-visible.
    ///
    /// Property enumeration, JSON, object casts and serialization omit these names;
    /// the class's own internal handlers and debug projection can still read them.
    pub hidden_properties: &'static [&'static str],
    /// PHP-visible method allowlist for internal classes whose compiler adapters are hidden.
    ///
    /// `None` exposes every declared method, matching userland classes. `Some` is a reviewed
    /// php-src surface; compiler lowering can still call every declared adapter internally.
    pub php_visible_methods: Option<&'static [&'static str]>,
    /// PHP manual fragment (`class.arrayiterator`), when one exists.
    pub php_manual: Option<&'static str>,
}

impl ClassContract {
    /// Returns the canonical lowercase lookup key.
    pub fn canonical_name(&self) -> String {
        self.name.to_ascii_lowercase()
    }

    /// Returns true when a declared property is compiler storage rather than PHP state.
    pub fn property_is_hidden(&self, property: &str) -> bool {
        self.hidden_properties
            .iter()
            .any(|hidden| *hidden == property)
    }

    /// Returns true when a declared method is part of the PHP-visible surface.
    pub fn method_is_php_visible(&self, method: &str) -> bool {
        self.php_visible_methods.is_none_or(|methods| {
            methods
                .iter()
                .any(|visible| visible.eq_ignore_ascii_case(method))
        })
    }
}
