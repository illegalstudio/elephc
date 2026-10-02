//! Purpose:
//! Hosting real PHP extensions — PECL releases, PIE packages, or a local
//! source tree — inside compiled Elephc programs.
//!
//! Called from:
//! - `elephc extension …` (install side) and the compile pipeline (use side).
//!
//! Key details:
//! - An extension is built from source by evaluating its own `config.m4`
//!   (`config_m4`, `build`), against the `php-src` catalog package: the PHP
//!   headers plus `libelephc_zend.a`, which is the Zend engine's real
//!   data-structure code with a small host layer standing in for the VM.
//! - Its surface is read by running its MINIT and walking what it registered
//!   (`surface`), then declared to the compiler as ordinary PHP functions,
//!   classes and constants that call through the engine (`prelude`).
//! - `manifest` owns the `[extension]` section of `elephc.toml`; `install` and
//!   `cli` run `elephc extension add/install/remove/list`; `source` fetches PECL
//!   and PIE archives; `admission` refuses extensions that hook the VM.

pub mod admission;
pub mod build;
pub mod cli;
pub mod config_m4;
pub mod install;
pub mod manifest;
pub mod prelude;
pub mod source;
pub mod surface;
