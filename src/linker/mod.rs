//! Purpose:
//! Provides the assembler/linker facade used by the compiler pipeline.
//! Adapts legacy string options into a typed plan and orchestrates platform helpers.
//!
//! Called from:
//! - `crate::pipeline::compile()` after user and runtime object generation.
//! - `crate::cli` for table-driven bridge flag validation.
//!
//! Key details:
//! - The legacy `link()` API remains behavior-compatible while callers migrate to `LinkPlan`.
//! - Dependency resolution stays outside this module; the linker consumes typed paths only.

mod archive_dedup;
mod bridges;
mod command;
mod sdk;

use std::path::{Path, PathBuf};
use std::process::{self, Command};

use crate::codegen::platform::{AppleVariant, Platform, Target};
use crate::codegen::Emit;
use crate::link_plan::{LinkItem, LinkPlan};

use self::command::{LinkPaths, MacSdk};

/// Structured failure produced while preparing typed linker inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkError {
    /// A requested Elephc bridge archive could not be resolved to a regular file.
    MissingBridge {
        /// Authoritative bridge linker name that could not be materialized.
        name: String,
    },
}

impl std::fmt::Display for LinkError {
    /// Formats an actionable linker-preparation diagnostic.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingBridge { name } => {
                write!(formatter, "required Elephc bridge `{name}` could not be found")
            }
        }
    }
}

impl std::error::Error for LinkError {}

/// Resolves a `--with-<flag>` suffix to its bridge linker library name.
pub(crate) fn bridge_lib_for_flag(flag: &str) -> Option<&'static str> {
    bridges::bridge_lib_for_flag(flag)
}

/// Returns every accepted `--with-<flag>` suffix in bridge table order.
pub(crate) fn crate_flag_names() -> Vec<&'static str> {
    bridges::crate_flag_names()
}

/// Returns bridge library/flag pairs present in one planned named-library set.
pub(crate) fn bridges_in(
    link_libraries: &[String],
) -> Vec<(&'static str, &'static str)> {
    bridges::bridges_in(link_libraries)
}

/// Maps one bridge library name to its canonical PHP extension, when distinct.
pub(crate) fn php_extension_for_lib(lib_name: &str) -> Option<&'static str> {
    bridges::php_extension_for_lib(lib_name)
}

/// Builds the assembler invocation for a target, minus the input and output.
///
/// A Mach-O object records the platform it was built for, and `ld` refuses to
/// link a macOS-tagged object into an iOS image — the failure reads
/// *"building for 'iOS-simulator', but linking in object file built for
/// 'macOS'"*. Plain `as` has no way to say "iOS", so non-macOS Apple targets
/// assemble through `clang`, which stamps the platform from `-target` and needs
/// the matching SDK to resolve it.
///
/// Shared by the user object and the cached runtime object. Both must carry the
/// same platform or the link fails on whichever one disagrees, and they used to
/// build this command separately.
pub(crate) fn assembler_command(target: Target) -> Command {
    if target.platform == Platform::MacOS && target.apple_variant != AppleVariant::MacOS {
        let sdk_path = sdk::macos_sdk_path(target.apple_sdk_name());
        let mut assembler = Command::new("clang");
        assembler
            .arg("-c")
            .args(["-target", &target.apple_clang_triple()])
            .args(["-isysroot", &sdk_path]);
        return assembler;
    }
    let mut assembler = Command::new(target.assembler_cmd());
    if target.platform == Platform::MacOS {
        assembler.args(["-arch", target.darwin_arch_name()]);
    }
    assembler
}

/// Invokes the target assembler for one generated assembly source file.
pub(crate) fn assemble(target: Target, asm_path: &Path, obj_path: &Path) {
    let mut assembler = assembler_command(target);
    assembler.arg("-o").arg(obj_path).arg(asm_path);
    command::run_tool("Assembler", &mut assembler);
}

/// Packs the compiled objects into a static library with `ar`.
///
/// This deliberately does not go through the link plan. An archive is not a
/// link: bridge staticlibs and managed native packages stay separate `.a` files
/// for the consuming project to link alongside this one, exactly as a C library
/// would leave its own dependencies to its consumer. Rolling them in would
/// duplicate every symbol the host also links directly.
///
/// `rcs` replaces the archive, creates it when absent, and writes the symbol
/// index in one step -- no separate `ranlib` pass is needed on either platform.
pub(crate) fn archive(archive_path: &Path, object: &Path, runtime_object: &Path) {
    // A stale archive would otherwise keep members from the previous build,
    // because `r` replaces matching names but never removes vanished ones.
    let _ = std::fs::remove_file(archive_path);

    let mut ar = Command::new("ar");
    ar.arg("rcs").arg(archive_path).arg(object).arg(runtime_object);
    command::run_tool("ar", &mut ar);
}

/// Bakes macOS debug maps into a dSYM before temporary objects are removed.
pub(crate) fn bake_debug_info(target: Target, bin_path: &Path) -> bool {
    if target.platform != Platform::MacOS {
        return true;
    }
    let status = Command::new("dsymutil").arg(bin_path).status();
    matches!(status, Ok(status) if status.success())
}

/// Adapts the existing raw linker arguments into a typed plan and links the output.
///
/// Raw libraries retain their legacy dynamic behavior. New managed dependency
/// integration should call [`link_with_plan`] with exact archive items instead.
#[allow(dead_code)] // Retained as a compatibility adapter while non-pipeline callers migrate.
pub(crate) fn link(
    target: Target,
    emit: Emit,
    bin_path: &Path,
    obj_path: &Path,
    runtime_object_path: &Path,
    extra_link_libs: &[String],
    extra_link_paths: &[String],
    extra_frameworks: &[String],
    forced_whole_archive: &[String],
) {
    let mut plan = LinkPlan::new();
    for library in extra_link_libs {
        plan.push(LinkItem::named_user(library));
    }
    for path in extra_link_paths {
        plan.push(LinkItem::SearchPath(PathBuf::from(path)));
    }
    for framework in extra_frameworks {
        plan.push(LinkItem::Framework(framework.clone()));
    }
    if let Err(error) = link_with_plan(
        target,
        emit,
        bin_path,
        obj_path,
        runtime_object_path,
        &plan,
        forced_whole_archive,
    ) {
        eprintln!("Linker error: {error}");
        process::exit(1);
    }
}

/// Resolves bridge inputs and executes a linker command from an already typed plan.
pub(crate) fn link_with_plan(
    target: Target,
    emit: Emit,
    bin_path: &Path,
    obj_path: &Path,
    runtime_object_path: &Path,
    plan: &LinkPlan,
    forced_whole_archive: &[String],
) -> Result<(), LinkError> {
    let resolved = bridges::resolve(plan, forced_whole_archive)?;
    let prepared = (target.platform == Platform::MacOS)
        .then(|| archive_dedup::prepare(&resolved.plan));
    let render_plan = prepared
        .as_ref()
        .map(|prepared| &prepared.plan)
        .unwrap_or(&resolved.plan);

    // The SDK is selected by the target's Apple variant, not by the host: an
    // iOS build resolves the iOS SDK even though it runs on macOS.
    let apple_sdk = target.apple_sdk_name();
    let sdk_path = (target.platform == Platform::MacOS).then(|| sdk::macos_sdk_path(apple_sdk));
    let sdk_version =
        (target.platform == Platform::MacOS).then(|| sdk::macos_sdk_version(apple_sdk));
    let mac_sdk = sdk_path
        .as_deref()
        .zip(sdk_version.as_deref())
        .map(|(path, version)| MacSdk { path, version });
    let homebrew_paths = if target.platform == Platform::MacOS
        && render_plan.needs_default_macos_library_paths()
    {
        sdk::default_macos_library_paths()
    } else {
        Vec::new()
    };

    let rendered = command::render_link_command(
        target,
        emit,
        LinkPaths {
            bin: bin_path,
            object: obj_path,
            runtime: runtime_object_path,
        },
        render_plan,
        resolved.needs_libdl,
        mac_sdk,
        &homebrew_paths,
    );
    command::execute_link_command(rendered);

    if let Some(prepared) = prepared {
        prepared.cleanup();
    }
    Ok(())
}
