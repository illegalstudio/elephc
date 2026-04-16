#![allow(dead_code)]

use std::collections::HashSet;
pub(crate) use std::fs;
pub(crate) use std::path::Path;
pub(crate) use std::process::Command;
pub(crate) use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

pub(crate) use elephc::codegen::platform::{Arch, Platform, Target};

pub(crate) static TEST_ID: AtomicU64 = AtomicU64::new(0);
pub(crate) static SDK_PATH: OnceLock<String> = OnceLock::new();
pub(crate) static SDK_VERSION: OnceLock<String> = OnceLock::new();
pub(crate) static RUNTIME_OBJ: OnceLock<std::path::PathBuf> = OnceLock::new();
pub(crate) static QEMU_SYSROOT: OnceLock<Option<String>> = OnceLock::new();
pub(crate) static TEST_TARGET: OnceLock<Target> = OnceLock::new();

mod platform;
mod runner;
mod compiler;
mod projects;

pub(crate) use platform::*;
pub(crate) use runner::*;
pub(crate) use compiler::*;
pub(crate) use projects::*;
