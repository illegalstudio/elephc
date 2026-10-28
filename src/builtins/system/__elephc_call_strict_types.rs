//! Purpose:
//! Exposes the nearest call site's `strict_types` state to compiler-provided preludes.
//!
//! Called from:
//! - `Random\Engine\Mt19937::__construct`, whose seed binding follows php-src's
//!   `Z_PARAM_LONG_OR_NULL` rule and must reject coercions in strict files.
//!
//! Key details:
//! - The value is published by `PublishCallStrictness` at construction sites, where the
//!   compiler still knows the constructing file's profile.
//! - E_DEPRECATED-style engine semantics; never PHP-visible (`internal: true`).

use super::core_support::core_builtin_home;
core_builtin_home!("__elephc_call_strict_types", CallStrictTypes);
