//! Purpose:
//! Defines shared fixtures for ABI helper unit tests.
//! Hosts target-specific emitter construction and includes focused ABI test modules.
//!
//! Called from:
//! - `crate::codegen::abi` test modules through Rust test harness
//!
//! Key details:
//! - Fixtures must reflect the default and Linux x86_64 targets used by ABI regression tests.

use super::*;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::{Arch, Platform, Target};
use crate::types::PhpType;

fn test_emitter() -> Emitter {
    Emitter::new(Target::new(Platform::MacOS, Arch::AArch64))
}

fn test_emitter_x86() -> Emitter {
    Emitter::new(Target::new(Platform::Linux, Arch::X86_64))
}

mod basics;
mod arguments;
mod symbols;
mod linux_x86_64;
