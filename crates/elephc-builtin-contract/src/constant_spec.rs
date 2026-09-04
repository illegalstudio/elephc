//! Purpose:
//! Defines the backend-neutral contract for one PHP global constant elephc predefines.
//!
//! Called from:
//! - Shared constant catalog declarations (`crate::catalog_constants`,
//!   `crate::catalog_constants_curl`).
//! - The compiler's checker, prescan, and name resolver, and Magician's constant evaluator,
//!   which all read this single table instead of private copies.
//!
//! Key details:
//! - The catalog carries the VALUE, so both backends resolve a predefined constant from the
//!   same data. Values that depend on the compile target or the selected PHP profile are
//!   marked `TargetDependent` and computed by each backend under the catalogued name.
//! - Class constants (`PDO::ATTR_*`) are not global constants and do not belong here; PHP's
//!   own `ReflectionExtension::getConstants()` agrees.

use crate::{BuiltinId, PhpModule, PhpVersion};

/// Value of a predefined global constant.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ConstValue {
    /// PHP `int`.
    Int(i64),
    /// PHP `float`.
    Float(f64),
    /// PHP `string`.
    Str(&'static str),
    /// PHP `bool`.
    Bool(bool),
    /// PHP `null`.
    Null,
    /// A stream resource wrapping a fixed file descriptor (`STDIN`, `STDOUT`, `STDERR`).
    StreamResource(i64),
    /// Computed per compile target or PHP profile (`PHP_OS`, `PHP_VERSION_ID`,
    /// `DIRECTORY_SEPARATOR`, `FNM_NOESCAPE`, `ICONV_IMPL`, ...). The backend owns the value;
    /// the catalog owns the name and the type.
    TargetDependent(ConstType),
}

/// PHP type of a constant whose value the backend computes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ConstType {
    /// PHP `int`.
    Int,
    /// PHP `float`.
    Float,
    /// PHP `string`.
    Str,
    /// PHP `bool`.
    Bool,
}

impl ConstValue {
    /// Returns the PHP type this value has on every backend.
    pub const fn php_type(self) -> ConstType {
        match self {
            Self::Int(_) | Self::StreamResource(_) => ConstType::Int,
            Self::Float(_) => ConstType::Float,
            Self::Str(_) => ConstType::Str,
            Self::Bool(_) => ConstType::Bool,
            Self::Null => ConstType::Str,
            Self::TargetDependent(ty) => ty,
        }
    }
}

/// How the compiler provides a global constant.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ConstantRoute {
    /// Registered unconditionally by the checker, prescan, and name resolver from this catalog.
    Predefined,
    /// Declared by an injected elephc prelude (`MYSQLI_*`, `IMG_*`); present only when that
    /// prelude is injected.
    Prelude,
    /// Defined at runtime by an elephc mechanism rather than predefined (`SID`).
    Dynamic,
}

/// Canonical backend-neutral contract for one PHP global constant.
#[derive(Clone, Copy, Debug)]
pub struct ConstantContract {
    /// Stable identity derived from the exact constant name (constants are case-sensitive).
    pub id: BuiltinId,
    /// Constant name as PHP spells it, e.g. `JSON_PRETTY_PRINT`.
    pub name: &'static str,
    /// PHP module (php-src extension) that owns this name, or `Elephc`.
    pub module: PhpModule,
    /// First PHP minor release that ships this name; `None` means every supported profile.
    pub since: Option<PhpVersion>,
    /// Value on every backend, or the backend-computed marker.
    pub value: ConstValue,
    /// How the compiler provides the constant.
    pub route: ConstantRoute,
    /// Whether strict-PHP hides this elephc-only surface.
    pub extension: bool,
    /// Whether this constant is compiler/runtime-internal, never PHP-visible.
    pub internal: bool,
}
