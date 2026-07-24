//! Purpose:
//! Wires synthetic builtin class and interface declarations into checker setup.
//! Exposes patching and validation hooks for PHP runtime types such as Throwable, Error, Exception, and Fiber.
//!
//! Called from:
//! - `crate::types::checker::driver::init`
//!
//! Key details:
//! - Injected declarations must be present before schema validation and method signature checks run.

mod calendar;
mod date_period;
mod datetime;
mod declarations;
mod exception;
mod fiber;
mod magic_methods;
mod reflection;
mod timezone_ids;

/// Metadata for a builtin PHP interface declaration.
///
/// `name` is the fully-qualified interface name. `extends` lists parent interfaces.
/// `properties`, `methods`, and `constants` carry the type contract exposed to user code;
/// the checker consults these to validate member access without emitting runtime behavior.
/// Registers the builtin throwable hierarchy and Fiber declarations in
/// `interface_map` and `class_map`.
///
/// Checks for name collisions with user-declared types before inserting; returns
/// `CompileError` if any builtin name is already present. Insertion order sets
/// the inheritance chain: Error/Exception extend Throwable; TypeError/ValueError/
/// ArithmeticError/UnhandledMatchError extend Error; RuntimeException/
/// ReflectionException extend Exception; JsonException extends RuntimeException;
/// FiberError extends Error. Fiber is final with no parent.
pub(crate) use declarations::{InterfaceDeclInfo, inject_builtin_throwables};

/// Patches the checker metadata for the Throwable interface and all builtin exception classes.
/// Updates return types for getter methods and the `__construct` parameter types for Error, TypeError,
/// ValueError, ArithmeticError, UnhandledMatchError, Exception, RuntimeException,
/// ReflectionException, JsonException, and FiberError.
pub(crate) use exception::patch_builtin_exception_signatures;

/// Patches Fiber method signatures in the checker after initial class registration.
///
/// This function refines the parametric types of Fiber methods that were
/// registered with placeholder types.
pub(crate) use fiber::patch_builtin_fiber_signatures;

/// Patches the type signatures for magic methods `__get`, `__set`, and `__call`
/// on user-declared classes to enforce PHP-correct parameter types.
///
/// For `__get`: parameter 0 is `PhpType::Str`.
/// For `__set`: parameter 0 is `PhpType::Str`, parameter 1 is `PhpType::Mixed`.
/// For `__call`: parameter 0 is `PhpType::Str`, parameter 1 is `PhpType::Array` of `PhpType::Never`.
/// Does nothing for classes that do not declare these methods.
pub(crate) use magic_methods::{patch_magic_method_signatures, validate_magic_method_contracts};
pub(crate) use reflection::{inject_builtin_reflection, patch_builtin_reflection_signatures};

/// Injects the builtin `DateTimeInterface`, `DateTimeZone`, and `DateTimeImmutable` declarations.
pub(crate) use datetime::inject_builtin_datetime;

/// Injects the builtin `DatePeriod` Iterator class (the `(start, interval, end)` form).
pub(crate) use date_period::inject_builtin_date_period;
