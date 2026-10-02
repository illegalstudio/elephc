//! Purpose:
//! Runs `elephc extension add/install/remove/list`: acquires extension sources,
//! builds them against the `php-src` package, and keeps the `[extension]`
//! manifest section in step.
//!
//! Called from:
//! - `main`, through `crate::php_ext::cli`.
//!
//! Key details:
//! - The `php-src` package (PHP headers + the Zend engine archive) is an
//!   ordinary `[native]` dependency: `add` declares it through `elephc native
//!   add` when the project lacks it, so it is locked and cached like any other.
//! - Built extensions live in the native cache under `extension/<name>/<key>`,
//!   where the key covers everything the build depends on: the build logic
//!   revision, the extension's source identity (pinned archive digest, or the
//!   content of a path source), the exact `php-src` artifact (which already
//!   encodes target, ABI and toolchain) and the target. Compilation recomputes
//!   the same key to find them, so it never builds anything itself.
//! - An artifact is built in a staging sibling and renamed into place, so a
//!   failed or interrupted build never leaves a directory that looks installed.

use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::codegen_support::platform::Target;
use crate::native_deps::cache::CacheLayout;
use crate::native_deps::project::{discover_for_native, discover_for_source, ProjectPaths};
use crate::native_deps::{
    resolve_for_compilation, run_native_command, NativeCommand, NativeError, NativeErrorKind,
    NativeOptions, NativeRequirement,
};

use super::build::{self, BuildRecord};
use super::manifest::{validate_extension_name, ExtensionSource, PhpExtManifest};
use super::source;
use super::surface::ExtensionSurface;

/// The catalog package hosted extensions build and link against.
pub const PHP_SRC_PACKAGE: &str = "php-src";

/// An extension built and ready to link.
#[derive(Clone, Debug)]
pub struct InstalledExtension {
    pub name: String,
    pub artifact_dir: PathBuf,
    pub record: BuildRecord,
    pub surface: ExtensionSurface,
}

impl InstalledExtension {
    /// The extension's static archive.
    pub fn archive(&self) -> PathBuf {
        self.artifact_dir.join(&self.record.archive)
    }
}

/// Where the extension being added comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AddSpec {
    Pecl { name: String, version: Option<String> },
    Pie { package: String, version: Option<String> },
    Path { name: String, path: PathBuf },
}

/// Reads `simdjson`, `simdjson@4.0.0`, `vendor/pkg`, `vendor/pkg@1.2.3`, or a
/// name with `--path`.
pub fn parse_add_spec(spec: &str, path: Option<PathBuf>) -> Result<AddSpec, NativeError> {
    let (name, version) = match spec.split_once('@') {
        Some((name, version)) => (name.to_string(), Some(version.to_string())),
        None => (spec.to_string(), None),
    };
    if let Some(path) = path {
        if version.is_some() {
            return Err(usage("a --path extension has no version"));
        }
        return Ok(AddSpec::Path { name, path });
    }
    if name.contains('/') {
        Ok(AddSpec::Pie { package: name, version })
    } else {
        Ok(AddSpec::Pecl { name, version })
    }
}

fn usage(message: &str) -> NativeError {
    NativeError::new(NativeErrorKind::Usage, message.to_string())
}

/// Everything shared by the commands of one invocation.
struct Context {
    project: ProjectPaths,
    target: Target,
    cache: CacheLayout,
    offline: bool,
    output: String,
}

impl Context {
    fn open(cwd: &Path, manifest_path: Option<&Path>, target: Option<Target>, offline: bool, create: bool) -> Result<Self, NativeError> {
        let project = discover_for_native(cwd, manifest_path, create)?.ok_or_else(|| {
            NativeError::new(NativeErrorKind::Project, "no elephc.toml found; run `elephc extension add` in the project root")
        })?;
        Ok(Self {
            project,
            target: target.unwrap_or_else(Target::detect_host),
            cache: CacheLayout::from_environment(cwd)?,
            offline,
            output: String::new(),
        })
    }

    fn manifest(&self) -> Result<PhpExtManifest, NativeError> {
        if self.project.manifest.is_file() {
            PhpExtManifest::load(&self.project.manifest)
        } else {
            Ok(PhpExtManifest::new())
        }
    }

    fn save(&self, manifest: &PhpExtManifest) -> Result<(), NativeError> {
        fs::write(&self.project.manifest, manifest.render())
            .map_err(|error| NativeError::io("write elephc.toml", &self.project.manifest, error))
    }

    fn native_options(&self) -> NativeOptions {
        NativeOptions {
            target: Some(self.target),
            manifest_path: Some(self.project.manifest.clone()),
            offline: self.offline,
        }
    }

    /// Declares (or installs) the php-src package and returns its artifact root.
    fn ensure_php_src(&mut self) -> Result<PathBuf, NativeError> {
        let declared = fs::read_to_string(&self.project.manifest)
            .ok()
            .and_then(|text| text.parse::<toml_edit::DocumentMut>().ok())
            .is_some_and(|document| {
                document
                    .get("native")
                    .and_then(|native| native.get("dependencies"))
                    .and_then(|dependencies| dependencies.get(PHP_SRC_PACKAGE))
                    .is_some()
            });
        let command = if declared {
            NativeCommand::Install { locked: false, options: self.native_options() }
        } else {
            NativeCommand::Add { package: PHP_SRC_PACKAGE.to_string(), version: None, options: self.native_options() }
        };
        let output = run_native_command(&command, &self.project.root)?;
        self.output.push_str(&output.stdout);
        php_src_root(&self.project.root, self.target)
    }

    fn cache_dir(&self) -> PathBuf {
        self.cache.root.join("extension")
    }
}

/// The materialized php-src artifact root for a project, read-only.
pub fn php_src_root(project_root: &Path, target: Target) -> Result<PathBuf, NativeError> {
    // Resolution discovers the project from a source path; any path inside the
    // project root names it.
    let probe = project_root.join("elephc-extension.php");
    let resolved = resolve_for_compilation(&probe, target, &[NativeRequirement::package(PHP_SRC_PACKAGE)])?;
    resolved
        .into_iter()
        .find(|package| package.package == PHP_SRC_PACKAGE)
        .map(|package| package.artifact_root)
        .ok_or_else(|| NativeError::new(NativeErrorKind::Lock, "php-src is not materialized"))
}

/// `PHP_VERSION` of a php-src artifact, read from its headers.
fn php_version(php_src: &Path) -> Result<String, NativeError> {
    let header = php_src.join("include/main/php_version.h");
    let text = fs::read_to_string(&header).map_err(|error| NativeError::io("read php_version.h", &header, error))?;
    text.lines()
        .find_map(|line| line.strip_prefix("#define PHP_VERSION \""))
        .and_then(|rest| rest.split('"').next())
        .map(str::to_string)
        .ok_or_else(|| NativeError::new(NativeErrorKind::Build, "php_version.h has no PHP_VERSION").with_path(header))
}

/// The directory a built extension lives in. Computed identically at install
/// and at compile time.
pub fn artifact_dir(cache_root: &Path, name: &str, identity: &str, php_src: &Path, target: Target) -> PathBuf {
    let mut digest = Sha256::new();
    for part in [
        build::BUILD_REVISION.to_string().as_str(),
        name,
        identity,
        &php_src.display().to_string(),
        target.as_str(),
    ] {
        digest.update(part.as_bytes());
        digest.update([0u8]);
    }
    let key = format!("{:x}", digest.finalize());
    cache_root.join("extension").join(name).join(&key[..32])
}

/// The identity of an extension's source for the cache key.
pub fn source_identity(project_root: &Path, source: &ExtensionSource) -> Result<String, NativeError> {
    match source {
        ExtensionSource::Pecl { sha256, .. } | ExtensionSource::Pie { sha256, .. } => Ok(sha256.clone()),
        ExtensionSource::Path { path } => source::tree_identity(&extension_dir(project_root, path)?),
    }
}

/// The directory holding `config.m4` for a path source.
fn extension_dir(project_root: &Path, path: &Path) -> Result<PathBuf, NativeError> {
    let dir = if path.is_absolute() { path.to_path_buf() } else { project_root.join(path) };
    if !dir.is_dir() {
        return Err(NativeError::new(NativeErrorKind::Project, "php extension path is not a directory").with_path(&dir));
    }
    source::locate_config_m4(&dir)
}

/// Loads an installed extension from its artifact directory.
pub fn load_installed(name: &str, artifact_dir: &Path) -> Result<InstalledExtension, NativeError> {
    let read = |file: &str| -> Result<String, NativeError> {
        let path = artifact_dir.join(file);
        fs::read_to_string(&path).map_err(|error| NativeError::io("read installed php extension", &path, error))
    };
    let record: BuildRecord = serde_json::from_str(&read("build.json")?).map_err(|error| {
        NativeError::new(NativeErrorKind::Integrity, format!("installed php extension '{name}' has a corrupt build.json: {error}"))
    })?;
    let surface = ExtensionSurface::from_json(&read("surface.json")?)
        .map_err(|message| NativeError::new(NativeErrorKind::Integrity, message))?;
    let installed = InstalledExtension { name: name.to_string(), artifact_dir: artifact_dir.to_path_buf(), record, surface };
    if !installed.archive().is_file() {
        return Err(NativeError::new(NativeErrorKind::Integrity, format!("installed php extension '{name}' lost its archive"))
            .with_path(installed.archive()));
    }
    Ok(installed)
}

/// Acquires the source tree for a pinned remote source, verifying its digest.
fn remote_tree(ctx: &Context, name: &str, source: &ExtensionSource, work: &Path) -> Result<PathBuf, NativeError> {
    let (sha256, url) = match source {
        ExtensionSource::Pecl { version, sha256 } => (sha256, source::pecl_url(name, version)),
        ExtensionSource::Pie { package, version, sha256 } => {
            if ctx.offline && !cached_archive(ctx, sha256).is_file() {
                return Err(offline_missing(name));
            }
            let url = if cached_archive(ctx, sha256).is_file() {
                String::new()
            } else {
                source::pie_resolve(package, Some(version))?.url
            };
            (sha256, url)
        }
        ExtensionSource::Path { .. } => unreachable!("path sources have no archive"),
    };
    let cached = cached_archive(ctx, sha256);
    if !cached.is_file() {
        if ctx.offline {
            return Err(offline_missing(name));
        }
        let temporary = cached.with_extension(format!("download.{}", std::process::id()));
        let actual = source::download(&url, &temporary)?;
        if &actual != sha256 {
            let _ = fs::remove_file(&temporary);
            return Err(NativeError::new(
                NativeErrorKind::Integrity,
                format!(
                    "php extension '{name}' changed upstream: the manifest pins sha256 {sha256}, the download \
                     is {actual}. If the new archive is expected, run `elephc extension add` again to re-pin it"
                ),
            ));
        }
        fs::rename(&temporary, &cached).map_err(|error| NativeError::io("publish php extension source", &cached, error))?;
    } else if &source::sha256_file(&cached)? != sha256 {
        return Err(NativeError::new(NativeErrorKind::Integrity, "cached php extension source is corrupt").with_path(&cached));
    }
    source::extract(&cached, &work.join("tree"))
}

fn cached_archive(ctx: &Context, sha256: &str) -> PathBuf {
    ctx.cache_dir().join("sources").join(format!("{sha256}.tar.gz"))
}

fn offline_missing(name: &str) -> NativeError {
    NativeError::new(NativeErrorKind::Network, format!("offline mode: php extension '{name}' is not cached"))
}

/// Builds (or reuses) one declared extension.
fn ensure_built(ctx: &mut Context, php_src: &Path, name: &str, source: &ExtensionSource) -> Result<InstalledExtension, NativeError> {
    let identity = source_identity(&ctx.project.root, source)?;
    let final_dir = artifact_dir(&ctx.cache.root, name, &identity, php_src, ctx.target);
    if let Ok(installed) = load_installed(name, &final_dir) {
        return Ok(installed);
    }
    let work = ctx.cache_dir().join("work").join(format!("{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work).map_err(|error| NativeError::io("create php extension work directory", &work, error))?;
    let result = (|| {
        let tree = match source {
            ExtensionSource::Path { path } => extension_dir(&ctx.project.root, path)?,
            _ => remote_tree(ctx, name, source, &work)?,
        };
        let staging = final_dir.with_extension(format!("stage.{}", std::process::id()));
        let _ = fs::remove_dir_all(&staging);
        fs::create_dir_all(&staging).map_err(|error| NativeError::io("create php extension staging", &staging, error))?;
        let toolchain = crate::native_deps::toolchain::resolve_toolchain(ctx.target)?;
        let version = php_version(php_src)?;
        let inputs = build::BuildInputs {
            name,
            source_dir: &tree,
            php_src,
            php_version: &version,
            toolchain: &toolchain,
            target: ctx.target,
            work_dir: &work.join("build"),
            output_dir: &staging,
        };
        if let Err(error) = build::build(&inputs) {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
        let _ = fs::remove_dir_all(&final_dir);
        if let Some(parent) = final_dir.parent() {
            fs::create_dir_all(parent).map_err(|error| NativeError::io("create php extension cache", parent, error))?;
        }
        fs::rename(&staging, &final_dir).map_err(|error| NativeError::io("publish php extension", &final_dir, error))?;
        load_installed(name, &final_dir)
    })();
    let _ = fs::remove_dir_all(&work);
    result
}

/// Resolves what `add` should pin: the manifest key and the source to record.
///
/// The name becomes a cache path (built, staged, and removed on failure), so it
/// is validated before anything is fetched or built: the one given on the
/// command line at once, and a PIE package's, which its own metadata chooses,
/// as soon as it is known.
fn pin(ctx: &Context, spec: &AddSpec) -> Result<(String, ExtensionSource), NativeError> {
    if let AddSpec::Pecl { name, .. } | AddSpec::Path { name, .. } = spec {
        validate_extension_name(name)?;
    }
    match spec {
        AddSpec::Path { name, path } => Ok((name.clone(), ExtensionSource::Path { path: path.clone() })),
        AddSpec::Pecl { name, version } => {
            let version = match version {
                Some(version) => version.clone(),
                None => source::pecl_latest_stable(name)?,
            };
            let sha256 = fetch_for_pin(ctx, &source::pecl_url(name, &version))?;
            Ok((name.clone(), ExtensionSource::Pecl { version, sha256 }))
        }
        AddSpec::Pie { package, version } => {
            let release = source::pie_resolve(package, version.as_deref())?;
            validate_extension_name(&release.extension)?;
            let sha256 = fetch_for_pin(ctx, &release.url)?;
            Ok((
                release.extension.clone(),
                ExtensionSource::Pie { package: release.package, version: release.version, sha256 },
            ))
        }
    }
}

/// Downloads an archive for the first time and files it under its digest.
fn fetch_for_pin(ctx: &Context, url: &str) -> Result<String, NativeError> {
    if ctx.offline {
        return Err(NativeError::new(NativeErrorKind::Network, "offline mode: `extension add` must download the source to pin it"));
    }
    let sources = ctx.cache_dir().join("sources");
    let temporary = sources.join(format!("pin.{}.tar.gz", std::process::id()));
    let sha256 = source::download(url, &temporary)?;
    let cached = sources.join(format!("{sha256}.tar.gz"));
    fs::rename(&temporary, &cached).map_err(|error| NativeError::io("publish php extension source", &cached, error))?;
    Ok(sha256)
}

fn describe(installed: &InstalledExtension) -> String {
    let surface = &installed.surface;
    let mut text = format!(
        "{} {} — {} functions, {} classes, {} constants{}",
        installed.name,
        surface.version,
        surface.functions.len(),
        surface.classes.len(),
        surface.constants.len(),
        if installed.record.cxx { " (C++)" } else { "" }
    );
    for (function, reason) in super::prelude::unsupported_functions(installed) {
        text.push_str(&format!("\n  not callable yet: {function}() — {reason}"));
    }
    text
}

/// `elephc extension add`.
pub fn add(cwd: &Path, spec: &AddSpec, manifest_path: Option<&Path>, target: Option<Target>, offline: bool) -> Result<String, NativeError> {
    let mut ctx = Context::open(cwd, manifest_path, target, offline, true)?;
    let php_src = ctx.ensure_php_src()?;
    let (name, source) = pin(&ctx, spec)?;
    let installed = ensure_built(&mut ctx, &php_src, &name, &source)?;
    let mut manifest = ctx.manifest()?;
    manifest.set_extension(&name, source)?;
    ctx.save(&manifest)?;
    ctx.output.push_str(&format!("added php extension {}\n", describe(&installed)));
    Ok(ctx.output)
}

/// `elephc extension install`: builds every declared extension that is missing.
pub fn install(cwd: &Path, manifest_path: Option<&Path>, target: Option<Target>, offline: bool) -> Result<String, NativeError> {
    let mut ctx = Context::open(cwd, manifest_path, target, offline, false)?;
    let manifest = ctx.manifest()?;
    if manifest.extensions().is_empty() {
        return Ok("no php extensions declared\n".to_string());
    }
    let php_src = ctx.ensure_php_src()?;
    for (name, source) in manifest.extensions() {
        let installed = ensure_built(&mut ctx, &php_src, name, source)?;
        ctx.output.push_str(&format!("installed php extension {}\n", describe(&installed)));
    }
    Ok(ctx.output)
}

/// `elephc extension remove`.
pub fn remove(cwd: &Path, name: &str, manifest_path: Option<&Path>) -> Result<String, NativeError> {
    let ctx = Context::open(cwd, manifest_path, None, false, false)?;
    let mut manifest = ctx.manifest()?;
    if !manifest.remove_extension(name) {
        return Err(NativeError::new(NativeErrorKind::Manifest, format!("php extension '{name}' is not declared")));
    }
    ctx.save(&manifest)?;
    Ok(format!("removed php extension {name}\n"))
}

/// `elephc extension list`: each declared extension and whether it is built.
pub fn list(cwd: &Path, manifest_path: Option<&Path>, target: Option<Target>) -> Result<String, NativeError> {
    let ctx = Context::open(cwd, manifest_path, target, false, false)?;
    let manifest = ctx.manifest()?;
    let php_src = php_src_root(&ctx.project.root, ctx.target).ok();
    let mut out = String::new();
    for (name, source) in manifest.extensions() {
        let state = match &php_src {
            Some(php_src) => {
                let identity = source_identity(&ctx.project.root, source)?;
                match load_installed(name, &artifact_dir(&ctx.cache.root, name, &identity, php_src, ctx.target)) {
                    Ok(installed) => describe(&installed),
                    Err(_) => format!("{name} — not built; run `elephc extension install`"),
                }
            }
            None => format!("{name} — php-src is not installed; run `elephc extension install`"),
        };
        out.push_str(&format!("{state}  [{}]\n", source.describe()));
    }
    for (directive, value) in manifest.ini() {
        out.push_str(&format!("ini {directive} = {value}\n"));
    }
    if out.is_empty() {
        out.push_str("no php extensions declared\n");
    }
    Ok(out)
}

/// What compilation needs to host a project's extensions.
#[derive(Clone, Debug)]
pub struct HostedExtensions {
    pub extensions: Vec<InstalledExtension>,
    pub ini: Vec<(String, String)>,
}

/// Finds the extensions a program's project hosts, already built. Returns
/// `None` when the project declares none. Never builds: a missing artifact is
/// an error that names the command which builds it.
pub fn resolve_hosted(source_file: &Path, target: Target) -> Result<Option<HostedExtensions>, NativeError> {
    let Some(project) = discover_for_source(source_file)? else { return Ok(None) };
    if !project.manifest.is_file() {
        return Ok(None);
    }
    let manifest = PhpExtManifest::load(&project.manifest)?;
    if manifest.extensions().is_empty() {
        return Ok(None);
    }
    let php_src = php_src_root(&project.root, target).map_err(|error| {
        error.with_recovery(format!("elephc extension install --target {}", target.as_str()))
    })?;
    let cwd = std::env::current_dir().map_err(|error| NativeError::io("read current directory", Path::new("."), error))?;
    let cache = CacheLayout::from_environment(&cwd)?;
    let mut extensions = Vec::new();
    for (name, source) in manifest.extensions() {
        let identity = source_identity(&project.root, source)?;
        let dir = artifact_dir(&cache.root, name, &identity, &php_src, target);
        let installed = load_installed(name, &dir).map_err(|_| {
            NativeError::new(
                NativeErrorKind::Lock,
                format!("php extension '{name}' is not built for {}", target.as_str()),
            )
            .with_project(project.root.clone())
            .with_recovery(format!("elephc extension install --target {}", target.as_str()))
        })?;
        extensions.push(installed);
    }
    Ok(Some(HostedExtensions {
        extensions,
        ini: manifest.ini().iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_add_spec_form() {
        assert_eq!(
            parse_add_spec("simdjson", None).unwrap(),
            AddSpec::Pecl { name: "simdjson".into(), version: None }
        );
        assert_eq!(
            parse_add_spec("apcu@5.1.28", None).unwrap(),
            AddSpec::Pecl { name: "apcu".into(), version: Some("5.1.28".into()) }
        );
        assert_eq!(
            parse_add_spec("acme/demo-ext@1.2.3", None).unwrap(),
            AddSpec::Pie { package: "acme/demo-ext".into(), version: Some("1.2.3".into()) }
        );
        assert_eq!(
            parse_add_spec("demo", Some(PathBuf::from("ext/demo"))).unwrap(),
            AddSpec::Path { name: "demo".into(), path: PathBuf::from("ext/demo") }
        );
        assert!(parse_add_spec("demo@1.0", Some(PathBuf::from("ext"))).is_err());
    }

    /// Compilation finds an artifact by recomputing its key, so every input
    /// the build depends on must move it, and nothing else may.
    #[test]
    fn the_artifact_key_covers_every_build_input() {
        let root = Path::new("/cache");
        let php_src = Path::new("/cache/artifacts/php-src/8.5.6/r1/x");
        let target = Target::detect_host();
        let base = artifact_dir(root, "apcu", "aaa", php_src, target);
        assert_eq!(base, artifact_dir(root, "apcu", "aaa", php_src, target), "deterministic");
        assert_ne!(base, artifact_dir(root, "apcu", "bbb", php_src, target), "source identity");
        assert_ne!(base, artifact_dir(root, "apcu", "aaa", Path::new("/other"), target), "engine artifact");
        assert_ne!(base, artifact_dir(root, "igbinary", "aaa", php_src, target), "extension name");
        assert!(base.starts_with("/cache/extension/apcu"));
    }
}
