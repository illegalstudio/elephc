//! Purpose:
//! Groups the math integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for BCMath, functions, and PHP float->int conversion.

#[path = "math/bcmath.rs"]
mod bcmath;
#[path = "math/bcmath_number.rs"]
mod bcmath_number;
#[path = "math/functions.rs"]
mod functions;
#[path = "math/random_bytes.rs"]
mod random_bytes;
#[path = "math/php_float_to_int.rs"]
mod php_float_to_int;
#[path = "math/rounding_modes.rs"]
mod rounding_modes;
