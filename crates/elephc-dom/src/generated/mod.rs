//! Purpose:
//! Exposes generated, version-locked native dispatch metadata to the DOM bridge.
//! Separates reproducible surface artifacts from handwritten ABI and engine code.
//!
//! Called from:
//! - `crate::exports` during opcode dispatch.
//!
//! Key details:
//! - Child modules are overwritten only by checked-in generators with `--check` support.

pub(crate) mod opcodes;
