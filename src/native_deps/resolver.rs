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
    let first_package = requirements[0].package_name();
    let cache = CacheLayout::from_environment(&cwd).map_err(|error| {
        let search_root = source
            .parent()
            .and_then(|parent| std::fs::canonicalize(parent).ok())
            .unwrap_or_else(|| cwd.clone());
        match discover_for_source(source).ok().flatten() {
            Some(project) => error
                .with_project(project.root)
                .with_recovery(format!(
                    "elephc native install --locked --target {}",
                    target.as_str()
                )),
            None => error
                .with_missing_project(search_root)
                .with_recovery(format!("elephc native add {first_package}")),
        }
    })?;
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
    let search_root = source
        .parent()
        .and_then(|parent| std::fs::canonicalize(parent).ok())
        .unwrap_or_else(|| source.parent().unwrap_or_else(|| Path::new(".")).to_path_buf());
    let project = discover_for_source(source)?.ok_or_else(|| {
        NativeError::new(
            NativeErrorKind::Project,
            format!("{feature} support requires managed native package {first_package}"),
        )
        .with_missing_project(&search_root)
        .with_recovery(format!("elephc native add {first_package}"))
    })?;
    let manifest = ManifestDocument::load(&project.manifest)?;
    let lock_recovery = format!("elephc native install --target {}", target.as_str());
    let artifact_recovery =
        format!("elephc native install --locked --target {}", target.as_str());
    let lock = NativeLock::load(&project.lock).map_err(|_| {
        NativeError::new(NativeErrorKind::Lock, "native lock is missing or stale")
            .with_path(&project.lock)
            .with_project(&project.root)
            .with_recovery(&lock_recovery)
    })?;
    lock.validate_current(&manifest).map_err(|error| {
        error
            .with_path(&project.lock)
            .with_project(&project.root)
            .with_default_recovery(&lock_recovery)
    })?;
    let toolchain = toolchains.resolve(target).map_err(|error| {
        error
            .with_project(&project.root)
            .with_default_recovery(&artifact_recovery)
    })?;
    let mut seen = BTreeSet::new();
    let mut resolved = Vec::new();
    // A flat worklist, not a `for` loop over `requirements`: a package's own
    // catalog/lock-declared dependencies (e.g. `curl` -> `["openssl", "zlib"]`,
    // the first non-empty `PackageVersion::dependencies`) are spliced in right
    // after it resolves, so a single top-level requirement such as `curl` alone
    // still yields every archive its final link needs — the caller only ever
    // names the leaf feature (see `crate::pipeline::backend`), never the
    // transitive chain. Splicing at `cursor` (not appending) preserves the
    // pre-order dependent-before-dependency shape the lock already recorded
    // (`curl, openssl, zlib`), which is also the exact static link order Task 2
    // verified (`libcurl.a -> libssl.a -> libcrypto.a -> libz.a`).
    let mut queue: Vec<String> = requirements
        .iter()
        .map(|requirement| requirement.package_name().to_string())
        .collect();
    let mut cursor = 0;
    while cursor < queue.len() {
        let name = queue[cursor].clone();
        cursor += 1;
        if !seen.insert(name.clone()) { continue; }
        let selected = manifest.dependencies().get(name.as_str()).ok_or_else(|| NativeError::new(
            NativeErrorKind::Manifest,
            format!("project does not declare required native package {name}"),
        )
        .with_path(&project.manifest)
        .with_project(&project.root)
        .with_recovery(format!("elephc native add {name}")))?;
        let version = catalog::version(&name, Some(selected))?;
        catalog::ensure_target(version, target)?;
        let locked = lock.package(&name).ok_or_else(|| {
            NativeError::new(NativeErrorKind::Lock, "native lock is missing or stale")
                .with_path(&project.lock)
                .with_project(&project.root)
                .with_recovery(&lock_recovery)
        })?;
        let key = ArtifactKey {
            package: &name,
            version: version.version,
            recipe: version.recipe_revision,
            source_sha256: version.source.sha256,
            target: target.as_str(),
            abi: &toolchain.abi,
            toolchain_fingerprint: &toolchain.fingerprint,
        };
        let root = cache.artifact_path(&key)?;
        let receipt = ArtifactReceipt::load(&root).map_err(|error| {
            missing_artifact(error, target, &toolchain.abi, &project.root)
        })?;
        let retained = version.retained_headers.iter().chain(version.ordered_link_outputs.iter()).copied().collect::<Vec<_>>();
        let identity = ReceiptIdentity {
            package: &name, version: version.version, recipe: version.recipe_revision,
            source_sha256: version.source.sha256, target: target.as_str(), abi: &toolchain.abi,
            toolchain_fingerprint: &toolchain.fingerprint,
            required_outputs: &retained,
        };
        receipt.verify(&root, &identity).map_err(|error| {
            missing_artifact(error, target, &toolchain.abi, &project.root)
        })?;
        let target_plan = locked
            .target
            .iter()
            .find(|plan| plan.name == target.as_str())
            .ok_or_else(|| {
                NativeError::new(NativeErrorKind::Lock, "native lock has no selected target plan")
                    .with_path(&project.lock)
                    .with_project(&project.root)
                    .with_recovery(&lock_recovery)
            })?;
        let mut archives = Vec::new();
        for relative in &target_plan.archives {
            if !receipt.outputs.iter().any(|output| output.path == *relative) {
                return Err(missing_artifact(
                    NativeError::new(
                        NativeErrorKind::Integrity,
                        format!("receipt omits required output '{relative}'"),
                    ),
                    target,
                    &toolchain.abi,
                    &project.root,
                ));
            }
            archives.push(root.join(relative));
        }
        resolved.push(ResolvedNativePackage {
            package: name.clone(), artifact_root: root, archives,
            system_libraries: target_plan.system_libraries.clone(), frameworks: target_plan.frameworks.clone(),
        });
        for (offset, dependency) in locked.dependencies.iter().enumerate() {
            queue.insert(cursor + offset, dependency.clone());
        }
    }
    Ok(resolved)
}

/// Adds the exact selected target/ABI recovery command to an artifact failure.
fn missing_artifact(
    error: NativeError,
    target: Target,
    abi: &str,
    project_root: &Path,
) -> NativeError {
    NativeError::new(
        NativeErrorKind::Integrity,
        format!(
            "native artifact is missing or corrupt for target '{}' ABI '{}': {}",
            target.as_str(),
            abi,
            error
        ),
    )
    .with_project(project_root)
    .with_recovery(format!(
        "elephc native install --locked --target {}",
        target.as_str()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::super::receipt::ToolIdentity;
    use super::super::toolchain::NativeToolchain;

    /// Toolchain provider with one deterministic selected identity.
    struct FixedToolchains;

    impl ToolchainProvider for FixedToolchains {
        /// Returns the stable identity used to address missing fixture artifacts.
        fn resolve(&self, _target: Target) -> Result<NativeToolchain, NativeError> {
            let identity = ToolIdentity {
                command: "fixture".into(),
                version: "fixture".into(),
            };
            Ok(NativeToolchain {
                cc: "cc".into(),
                ar: "ar".into(),
                ranlib: "ranlib".into(),
                target_tuple: "fixture".into(),
                abi: "fixture-abi".into(),
                fingerprint: "fixture-fingerprint".into(),
                compiler: identity.clone(),
                archiver: identity.clone(),
                ranlib_identity: identity,
            })
        }
    }

    /// Toolchain provider that simulates an incompatible or missing selected toolchain.
    struct BrokenToolchains;

    impl ToolchainProvider for BrokenToolchains {
        /// Returns the deterministic wrong-toolchain diagnostic.
        fn resolve(&self, _target: Target) -> Result<NativeToolchain, NativeError> {
            Err(NativeError::new(
                NativeErrorKind::Toolchain,
                "compiler tuple does not match target",
            ))
        }
    }

    /// Creates one project fixture and optionally writes its current deterministic lock.
    fn project_fixture(label: &str, manifest_text: &str, write_lock: bool) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "elephc-resolver-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("elephc.toml"), manifest_text).unwrap();
        if write_lock {
            let manifest = ManifestDocument::load(&root.join("elephc.toml")).unwrap();
            let lock = NativeLock::from_manifest(&manifest).unwrap();
            fs::write(root.join("elephc.lock"), lock.render().unwrap()).unwrap();
        }
        fs::canonicalize(root).unwrap()
    }

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
        assert!(error.to_string().contains("project: not found"));
        assert!(error.to_string().contains("recovery: cd --"));
        std::fs::remove_dir_all(fixture).unwrap();
    }

    /// Verifies absent lock state carries the discovered project and non-locked repair command.
    #[test]
    fn missing_lock_is_actionable() {
        let root = project_fixture(
            "missing-lock",
            "[native]\nschema = 1\n[native.dependencies]\npcre2 = \"10.47\"\n",
            false,
        );
        let cache = CacheLayout::from_values(
            &root,
            Some(root.join("cache").as_os_str()),
            None,
            None,
        )
        .unwrap();
        let error = resolve_for_compilation_with(
            &root.join("main.php"),
            Target::detect_host(),
            &[NativeRequirement::package("pcre2")],
            &cache,
            &FixedToolchains,
        )
        .unwrap_err();
        let rendered = error.to_string();
        assert!(rendered.contains("native lock is missing or stale"));
        assert!(rendered.contains(&format!("project: {}", root.display())));
        assert!(rendered.contains("elephc native install --target"));
        fs::remove_dir_all(root).unwrap();
    }

    /// Verifies undeclared requirements use the same project/recovery diagnostic shape.
    #[test]
    fn undeclared_package_is_actionable() {
        let root = project_fixture(
            "undeclared",
            "[native]\nschema = 1\n[native.dependencies]\n",
            true,
        );
        let cache = CacheLayout::from_values(
            &root,
            Some(root.join("cache").as_os_str()),
            None,
            None,
        )
        .unwrap();
        let error = resolve_for_compilation_with(
            &root.join("main.php"),
            Target::detect_host(),
            &[NativeRequirement::package("pcre2")],
            &cache,
            &FixedToolchains,
        )
        .unwrap_err();
        let rendered = error.to_string();
        assert!(rendered.contains("does not declare required native package pcre2"));
        assert!(rendered.contains(&format!("project: {}", root.display())));
        assert!(rendered.contains("elephc native add pcre2"));
        fs::remove_dir_all(root).unwrap();
    }

    /// Verifies wrong toolchains and missing artifacts share exact-target recovery diagnostics.
    #[test]
    fn toolchain_and_artifact_misses_are_actionable() {
        let root = project_fixture(
            "toolchain-artifact",
            "[native]\nschema = 1\n[native.dependencies]\npcre2 = \"10.47\"\n",
            true,
        );
        let cache = CacheLayout::from_values(
            &root,
            Some(root.join("cache").as_os_str()),
            None,
            None,
        )
        .unwrap();
        for provider in [
            &BrokenToolchains as &dyn ToolchainProvider,
            &FixedToolchains as &dyn ToolchainProvider,
        ] {
            let error = resolve_for_compilation_with(
                &root.join("main.php"),
                Target::detect_host(),
                &[NativeRequirement::package("pcre2")],
                &cache,
                provider,
            )
            .unwrap_err();
            let rendered = error.to_string();
            assert!(rendered.contains(&format!("project: {}", root.display())));
            assert!(rendered.contains("recovery: cd --"));
            assert!(rendered.contains("elephc native install --locked --target"));
        }
        fs::remove_dir_all(root).unwrap();
    }

    /// Fabricates one complete, verifiable local artifact for a catalog package: every
    /// catalog-declared retained header/archive as a tiny non-empty dummy file, plus a
    /// receipt whose recorded hashes match what was just written. Mirrors the shape a
    /// real recipe publishes without invoking any actual build, using the catalog's own
    /// `retained_headers`/`ordered_link_outputs` lists so the fixture cannot drift from
    /// what `receipt.verify()` actually requires.
    fn fabricate_artifact(cache: &CacheLayout, name: &str, toolchain: &NativeToolchain, target: Target) {
        let version = catalog::version(name, None).expect("catalog package");
        let key = ArtifactKey {
            package: name,
            version: version.version,
            recipe: version.recipe_revision,
            source_sha256: version.source.sha256,
            target: target.as_str(),
            abi: &toolchain.abi,
            toolchain_fingerprint: &toolchain.fingerprint,
        };
        let root = cache.artifact_path(&key).expect("artifact path");
        let retained: Vec<&str> = version
            .retained_headers
            .iter()
            .chain(version.ordered_link_outputs.iter())
            .copied()
            .collect();
        for relative in &retained {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().expect("catalog output has a parent directory"))
                .unwrap();
            fs::write(&path, b"fixture").unwrap();
        }
        let outputs = super::super::receipt::collect_outputs(&root, &retained).unwrap();
        let identity = ToolIdentity { command: "fixture".into(), version: "fixture".into() };
        let receipt = ArtifactReceipt {
            schema: 1,
            package: name.to_string(),
            version: version.version.to_string(),
            recipe: version.recipe_revision,
            source_sha256: version.source.sha256.to_string(),
            target: target.as_str().to_string(),
            abi: toolchain.abi.clone(),
            compiler: identity.clone(),
            archiver: identity.clone(),
            ranlib: identity,
            toolchain_fingerprint: toolchain.fingerprint.clone(),
            outputs,
            created_by: "test".to_string(),
        };
        receipt.write(&root).unwrap();
    }

    /// Verifies a single top-level `curl` requirement resolves its full catalog-declared
    /// transitive chain (`openssl`, `zlib` — `curl` is the first catalog package with
    /// non-empty `PackageVersion::dependencies`) without the caller ever naming them, in
    /// exactly the dependent-before-dependency order Task 2's real static link required:
    /// `libcurl.a -> libssl.a -> libcrypto.a -> libz.a`. Before this test, `resolve_for_
    /// compilation_with` only ever iterated the exact `requirements` slice a caller passed
    /// in, so a lone `NativeRequirement::package("curl")` (what `crate::pipeline::backend`
    /// emits) resolved `curl`'s own archive only, silently dropping `libssl.a`/`libcrypto.a`/
    /// `libz.a` from the final link plan.
    #[test]
    fn curl_requirement_resolves_its_transitive_native_chain() {
        let root = project_fixture(
            "curl-transitive",
            "[native]\nschema = 1\n[native.dependencies]\ncurl = \"8.21.0\"\nopenssl = \"3.5.7\"\nzlib = \"1.3.2\"\n",
            true,
        );
        let cache = CacheLayout::from_values(
            &root,
            Some(root.join("cache").as_os_str()),
            None,
            None,
        )
        .unwrap();
        let target = Target::detect_host();
        let toolchain = FixedToolchains.resolve(target).expect("fixed toolchain");
        for package in ["curl", "openssl", "zlib"] {
            fabricate_artifact(&cache, package, &toolchain, target);
        }

        let resolved = resolve_for_compilation_with(
            &root.join("main.php"),
            target,
            &[NativeRequirement::package("curl")],
            &cache,
            &FixedToolchains,
        )
        .expect("curl requirement resolves its transitive chain");

        let names: Vec<&str> = resolved.iter().map(|package| package.package.as_str()).collect();
        assert_eq!(
            names,
            vec!["curl", "openssl", "zlib"],
            "curl's own dependency order must be preserved, not re-sorted alphabetically"
        );
        let archive_filenames: Vec<String> = resolved
            .iter()
            .flat_map(|package| package.archives.iter())
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            archive_filenames,
            vec!["libcurl.a", "libssl.a", "libcrypto.a", "libz.a"],
            "final link order must match Task 2's verified static link"
        );

        fs::remove_dir_all(root).unwrap();
    }
}
