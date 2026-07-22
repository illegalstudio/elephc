//! Purpose:
//! Resolves logical native requirements to verified exact archive paths without mutating state.
//!
//! Called from:
//! - Compilation pipeline only when a final link and native package requirement are present.
//!
//! Key details:
//! - Resolution never downloads/builds/writes and never falls back to named system libraries.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::codegen_support::platform::Target;

use super::cache::{ArtifactKey, CacheLayout};
use super::catalog;
use super::error::{NativeError, NativeErrorKind};
use super::lockfile::NativeLock;
use super::manifest::ManifestDocument;
use super::project::discover_for_source;
use super::receipt::{ArtifactReceipt, ReceiptIdentity};
use super::requirements::NativeRequirement;
use super::toolchain::{SystemToolchains, ToolchainProvider};

/// One verified native package and its exact ordered link inputs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedNativePackage {
    pub package: String,
    pub artifact_root: PathBuf,
    pub archives: Vec<PathBuf>,
    pub system_libraries: Vec<String>,
    pub frameworks: Vec<String>,
}

/// Resolves compilation requirements using process environment toolchain/cache selection.
pub fn resolve_for_compilation(
    source: &Path,
    target: Target,
    requirements: &[NativeRequirement],
) -> Result<Vec<ResolvedNativePackage>, NativeError> {
    if requirements.is_empty() {
        return Ok(Vec::new());
    }
    let cwd = std::env::current_dir().map_err(|error| NativeError::io("read current directory", Path::new("."), error))?;
    let cache = CacheLayout::from_environment(&cwd)?;
    resolve_for_compilation_with(source, target, requirements, &cache, &SystemToolchains)
}

/// Pure read-only resolver with injected cache and toolchain identity for tests and integration.
pub(crate) fn resolve_for_compilation_with(
    source: &Path,
    target: Target,
    requirements: &[NativeRequirement],
    cache: &CacheLayout,
    toolchains: &dyn ToolchainProvider,
) -> Result<Vec<ResolvedNativePackage>, NativeError> {
    if requirements.is_empty() {
        return Ok(Vec::new());
    }
    let first_package = requirements[0].package_name();
    let feature = if first_package == "pcre2" { "regex" } else { first_package };
    let project = discover_for_source(source)?.ok_or_else(|| NativeError::new(
        NativeErrorKind::Project,
        format!("{feature} support requires managed native package {first_package}; run elephc native add {first_package}"),
    ))?;
    let manifest = ManifestDocument::load(&project.manifest)?;
    let lock = NativeLock::load(&project.lock).map_err(|_| NativeError::new(
        NativeErrorKind::Lock,
        "native lock is missing or stale; run elephc native install (CI should use install --locked)",
    ).with_path(&project.lock))?;
    lock.validate_current(&manifest)?;
    let toolchain = toolchains.resolve(target)?;
    let mut seen = BTreeSet::new();
    let mut resolved = Vec::new();
    for requirement in requirements {
        let name = requirement.package_name();
        if !seen.insert(name.to_string()) { continue; }
        let selected = manifest.dependencies().get(name).ok_or_else(|| NativeError::new(
            NativeErrorKind::Manifest,
            format!("project does not declare required native package {name}; run elephc native add {name}"),
        ).with_path(&project.manifest))?;
        let version = catalog::version(name, Some(selected))?;
        catalog::ensure_target(version, target)?;
        let locked = lock.package(name).ok_or_else(|| NativeError::new(NativeErrorKind::Lock, "native lock is missing or stale; run elephc native install"))?;
        let key = ArtifactKey {
            package: name,
            version: version.version,
            recipe: version.recipe_revision,
            source_sha256: version.source.sha256,
            target: target.as_str(),
            abi: &toolchain.abi,
            toolchain_fingerprint: &toolchain.fingerprint,
        };
        let root = cache.artifact_path(&key)?;
        let receipt = ArtifactReceipt::load(&root).map_err(|error| missing_artifact(error, target, &toolchain.abi))?;
        let retained = version.retained_headers.iter().chain(version.ordered_link_outputs.iter()).copied().collect::<Vec<_>>();
        let identity = ReceiptIdentity {
            package: name, version: version.version, recipe: version.recipe_revision,
            source_sha256: version.source.sha256, target: target.as_str(), abi: &toolchain.abi,
            toolchain_fingerprint: &toolchain.fingerprint,
            required_outputs: &retained,
        };
        receipt.verify(&root, &identity).map_err(|error| missing_artifact(error, target, &toolchain.abi))?;
        let target_plan = locked.target.iter().find(|plan| plan.name == target.as_str()).ok_or_else(|| NativeError::new(NativeErrorKind::Lock, "native lock has no selected target plan"))?;
        let mut archives = Vec::new();
        for relative in &target_plan.archives {
            if !receipt.outputs.iter().any(|output| output.path == *relative) {
                return Err(missing_artifact(NativeError::new(NativeErrorKind::Integrity, format!("receipt omits required output '{relative}'")), target, &toolchain.abi));
            }
            archives.push(root.join(relative));
        }
        resolved.push(ResolvedNativePackage {
            package: name.to_string(), artifact_root: root, archives,
            system_libraries: target_plan.system_libraries.clone(), frameworks: target_plan.frameworks.clone(),
        });
    }
    Ok(resolved)
}

/// Adds the exact selected target/ABI recovery command to an artifact failure.
fn missing_artifact(error: NativeError, target: Target, abi: &str) -> NativeError {
    NativeError::new(
        NativeErrorKind::Integrity,
        format!("native artifact is missing or corrupt for target '{}' ABI '{}': {}; run elephc native install --locked --target {}", target.as_str(), abi, error, target.as_str()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies no requirement avoids all project/cache/toolchain access.
    #[test]
    fn empty_requirements_resolve_without_project() {
        struct PanicToolchains;
        impl ToolchainProvider for PanicToolchains {
            /// Fails the test if an empty requirement set probes tools.
            fn resolve(&self, _target: Target) -> Result<super::super::toolchain::NativeToolchain, NativeError> { panic!("toolchain should not be queried") }
        }
        let cache = CacheLayout::from_values(Path::new("/"), Some(std::ffi::OsStr::new("/missing-cache")), None, None).unwrap();
        assert!(resolve_for_compilation_with(Path::new("/missing/main.php"), Target::detect_host(), &[], &cache, &PanicToolchains).unwrap().is_empty());
    }

    /// Verifies a regex-style requirement without a manifest returns the exact recovery action.
    #[test]
    fn missing_project_is_actionable() {
        struct PanicToolchains;
        impl ToolchainProvider for PanicToolchains {
            /// Fails the test if project discovery does not stop first.
            fn resolve(&self, _target: Target) -> Result<super::super::toolchain::NativeToolchain, NativeError> { panic!("toolchain should not be queried") }
        }
        let cache = CacheLayout::from_values(Path::new("/"), Some(std::ffi::OsStr::new("/missing-cache")), None, None).unwrap();
        let fixture = std::env::temp_dir().join(format!("elephc-no-project-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&fixture);
        std::fs::create_dir_all(&fixture).unwrap();
        let error = resolve_for_compilation_with(&fixture.join("main.php"), Target::detect_host(), &[NativeRequirement::package("pcre2")], &cache, &PanicToolchains).unwrap_err();
        assert!(error.to_string().contains("regex support requires managed native package pcre2"));
        assert!(error.to_string().contains("elephc native add pcre2"));
        std::fs::remove_dir_all(fixture).unwrap();
    }
}
