//! Purpose:
//! Wires shared codegen test support helpers and cached target/runtime state.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Exports platform, compiler, runner, bundle, C fixture, and project helpers used by end-to-end codegen fixtures.

#![allow(dead_code)]

use elephc::fast_hash::{FastMap as HashMap, FastSet as HashSet};
pub(crate) use std::fs;
pub(crate) use std::path::Path;
pub(crate) use std::process::Command;
pub(crate) use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

pub(crate) use elephc::codegen::platform::{Arch, Platform, Target};

/// Removes a codegen test's temporary directory even when the test panics.
///
/// The directory used to be deleted only on the way out of `compile_and_run`, so ANY panic before
/// that -- a compile refusal, an assembler or linker failure, an assertion inside the harness --
/// leaked the `.s`, the `.o` and the linked binary it held. Measured on one full `codegen::eval`
/// run: 3092 leftover `elephc_test_*` directories in TMPDIR and free disk down from 20 GiB to 7.
///
/// That is a feedback loop, not just untidiness. Once the disk is short, compiles that would have
/// passed start failing for want of space, and every one of THOSE leaks another directory. The
/// same run's failure rate climbed from 3% to 49% as it went, which is what a contaminated census
/// looks like -- and it is why a number from such a run cannot be trusted as a branch-tip
/// measurement.
///
/// `ELEPHC_TEST_KEEP=1` keeps the directory and prints where it is, which is how to inspect the
/// assembly of a case that is genuinely failing.
pub(crate) struct TestDirGuard {
    path: std::path::PathBuf,
}

impl TestDirGuard {
    pub(crate) fn new(path: std::path::PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for TestDirGuard {
    fn drop(&mut self) {
        if std::env::var_os("ELEPHC_TEST_KEEP").is_some() {
            eprintln!("[elephc-test] kept {}", self.path.display());
            return;
        }
        let _ = fs::remove_dir_all(&self.path);
    }
}

impl AsRef<std::path::Path> for TestDirGuard {
    /// Needed as well as `Deref`: a generic bound like `fs::remove_dir_all(impl AsRef<Path>)`
    /// does not go through deref coercion, and 155 call sites already pass `&dir` to one.
    fn as_ref(&self) -> &std::path::Path {
        &self.path
    }
}

impl std::ops::Deref for TestDirGuard {
    type Target = std::path::Path;

    /// Lets a guard stand in for the `PathBuf` a helper used to hand back.
    ///
    /// `compile_and_run_in_dir` RETURNS its directory for the caller to inspect, so it cannot use
    /// a guard that drops at the end of the helper -- the guard has to travel to the caller. With
    /// this, the 155 existing call sites keep writing `&dir` and `dir.join(..)` unchanged, and
    /// their explicit `fs::remove_dir_all(&dir)` still works and simply becomes redundant: the
    /// guard removes the directory when the caller's frame ends, including when an assertion
    /// panics before that line is ever reached.
    fn deref(&self) -> &Self::Target {
        &self.path
    }
}

pub(crate) static TEST_ID: AtomicU64 = AtomicU64::new(0);
pub(crate) static SDK_PATH: OnceLock<String> = OnceLock::new();
pub(crate) static SDK_VERSION: OnceLock<String> = OnceLock::new();
pub(crate) static RUNTIME_OBJ: OnceLock<std::path::PathBuf> = OnceLock::new();
pub(crate) static RUNTIME_OBJS_BY_ASM: OnceLock<Mutex<HashMap<u64, std::path::PathBuf>>> =
    OnceLock::new();
pub(crate) static BRIDGE_STATICLIB_BUILD_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
pub(crate) static LIBPQ_BRIDGE_BUILT: OnceLock<()> = OnceLock::new();
pub(crate) static DBLIB_BRIDGE_BUILT: OnceLock<()> = OnceLock::new();
pub(crate) static FIREBIRD_BRIDGE_BUILT: OnceLock<()> = OnceLock::new();
pub(crate) static ODBC_BRIDGE_BUILT: OnceLock<()> = OnceLock::new();
pub(crate) static OCI_BRIDGE_BUILT: OnceLock<()> = OnceLock::new();
pub(crate) static CUBRID_BRIDGE_BUILT: OnceLock<()> = OnceLock::new();
pub(crate) static QEMU_SYSROOT: OnceLock<Option<String>> = OnceLock::new();
pub(crate) static TEST_TARGET: OnceLock<Target> = OnceLock::new();

mod platform;
mod runner;
mod magician_link;
mod bundle;
mod c_fixture;
mod compiler;
mod curl_native;
mod gmp_native;
mod native_cache;
pub(crate) mod fileinfo_native;
mod native_projects;
mod oniguruma;
mod projects;
pub(crate) mod xml_native;

pub(crate) use platform::*;
pub(crate) use runner::*;
pub(crate) use bundle::*;
pub(crate) use c_fixture::*;
pub(crate) use compiler::*;
pub(crate) use curl_native::*;
pub(crate) use native_projects::*;
pub(crate) use oniguruma::*;
pub(crate) use projects::*;
pub(crate) use gmp_native::{gmp_native_package, skip_without_gmp_native, GmpNativePackage, GMP_NATIVE_PACKAGE};
// Named rather than globbed: `xml_native::available()` would otherwise collide with
// `curl_native::available()` under two `*` imports. Reach it as
// `support::xml_native::available()`.
pub(crate) use xml_native::{
    skip_without_xml_native, xml_native_archives, xml_native_package, XmlNativePackage,
    XML_APPLE_LIBRARIES, XML_NATIVE_ARCHIVES, XML_NATIVE_PACKAGE,
};
