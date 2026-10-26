//! Purpose:
//! End-to-end coverage for the PHP `random` extension: the global procedural surface,
//! the `Random\Engine\*` implementations, and `Random\Randomizer`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Fixtures are ported from php-src `ext/random/tests` and assert the same
//!   deterministic reference values. Tests that only assert ranges/permutations keep
//!   their original shape; tests that required `pack()`/Reflection/32-bit hosts are
//!   adapted as noted in their docblocks.
//! - Deprecation notices are written to stderr by php-src and by elephc, so stdout
//!   assertions remain stable for fixtures that trigger `MT_RAND_PHP`.

#[path = "random/functions.rs"]
mod functions;

#[path = "random/engines.rs"]
mod engines;

#[path = "random/randomizer.rs"]
mod randomizer;

#[path = "random/user_engine.rs"]
mod user_engine;
