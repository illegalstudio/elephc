//! Purpose:
//! Runs `elephc extension add/install/remove/list`: acquires extension sources,
//! builds them against Elephc's own PHP extension ABI, and keeps the
//! `[extension]` manifest section in step.
//!
//! Called from:
//! - `main`, through `crate::php_ext::cli`.
//!
//! Key details:
//! - Nothing is downloaded but the extension itself: the headers it compiles
//!   against are embedded in Elephc (`crate::php_ext::headers`) and the engine
//!   is the elephc-zend bridge archive (`libelephc_zend.a`).
//! - Built extensions live in the native cache under `extension/<name>/<key>`,
//!   where the key covers everything the build depends on: the build logic
//!   revision, the extension's source identity (pinned archive digest, or the
//!   content of a path source), the ABI headers' digest, the target, and the
//!   C toolchain's fingerprint (compiler, libc ABI, SDK, flags), so a GNU and
//!   a musl build, or two compilers, never share an archive.
//! - Manifest edits hold the project lock native commands use and replace
//!   `elephc.toml` atomically.
//!   Compilation recomputes the same key to find them, so it never builds
//!   anything itself.
//! - An artifact is built in a staging sibling and renamed into place, so a
//!   failed or interrupted build never leaves a directory that looks installed.

use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::codegen_support::platform::Target;
use crate::native_deps::cache::CacheLayout;
use crate::native_deps::cache::AdvisoryLock;
use crate::native_deps::project::{discover_for_native, discover_for_source, ProjectPaths};
use crate::native_deps::{atomic_write, NativeError, NativeErrorKind};

use super::build::{self, BuildRecord};
use super::headers;
use super::manifest::{validate_exact_version, validate_extension_name, ExtensionSource, PhpExtManifest};
use super::source;
use super::surface::ExtensionSurface;

/// Finds `libelephc_zend.a`, the engine an extension is introspected with.
/// Supplied by the `elephc` binary, which owns bridge discovery.
pub type EngineLocator = fn() -> Result<PathBuf, String>;

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

/// Builds a native error of kind Usage from a message.
fn usage(message: &str) -> NativeError {
    NativeError::new(NativeErrorKind::Usage, message.to_string())
}

/// Everything shared by the commands of one invocation.
struct Context {
    /// The directory the command was run from: a relative `--path` is relative to it.
    cwd: PathBuf,
    project: ProjectPaths,
    target: Target,
    cache: CacheLayout,
    offline: bool,
    output: String,
    locate_engine: EngineLocator,
}

/// The locator for commands that never build.
fn no_engine() -> Result<PathBuf, String> {
    Err("this command does not build extensions".to_string())
}

impl Context {
    /// Opens the command context for a working directory: discovers the project manifest (creating
    /// it when `create` is set), picks the requested or host target, and resolves the cache layout.
    /// Errors when no `elephc.toml` exists.
    fn open(cwd: &Path, manifest_path: Option<&Path>, target: Option<Target>, offline: bool, create: bool) -> Result<Self, NativeError> {
        let project = discover_for_native(cwd, manifest_path, create)?.ok_or_else(|| {
            NativeError::new(NativeErrorKind::Project, "no elephc.toml found; run `elephc extension add` in the project root")
        })?;
        Ok(Self {
            cwd: cwd.to_path_buf(),
            project,
            target: target.unwrap_or_else(Target::detect_host),
            cache: CacheLayout::from_environment(cwd)?,
            offline,
            output: String::new(),
            locate_engine: no_engine,
        })
    }

    /// Loads the project's extension manifest, or an empty one when `elephc.toml` does not exist
    /// yet.
    fn manifest(&self) -> Result<PhpExtManifest, NativeError> {
        if self.project.manifest.is_file() {
            PhpExtManifest::load(&self.project.manifest)
        } else {
            Ok(PhpExtManifest::new())
        }
    }

    /// Replaces `elephc.toml` atomically; callers hold `lock_project`.
    fn save(&self, manifest: &PhpExtManifest) -> Result<(), NativeError> {
        atomic_write(&self.project.manifest, manifest.render().as_bytes())
    }

    /// The project lock `elephc native` commands take, held across a manifest
    /// read, edit and save so concurrent commands never lose each other's edits.
    fn lock_project(&self, command: &str) -> Result<AdvisoryLock, NativeError> {
        self.cache.lock(&self.cache.project_lock_path(&self.project.manifest), command)
    }

    /// The fingerprint of the C toolchain extensions are built with.
    fn toolchain_fingerprint(&self) -> Result<String, NativeError> {
        Ok(crate::native_deps::toolchain::resolve_toolchain(self.target)?.fingerprint)
    }

    /// The cache directory for hosted extensions, under the cache root.
    fn cache_dir(&self) -> PathBuf {
        self.cache.root.join("extension")
    }
}

/// The directory a built extension lives in. Computed identically at install
/// and at compile time.
pub fn artifact_dir(
    cache_root: &Path,
    name: &str,
    identity: &str,
    abi_identity: &str,
    target: Target,
    toolchain_fingerprint: &str,
) -> PathBuf {
    let mut digest = Sha256::new();
    for part in [
        build::BUILD_REVISION.to_string().as_str(),
        name,
        identity,
        abi_identity,
        target.as_str(),
        toolchain_fingerprint,
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
        ExtensionSource::Pecl { version, sha256 } => {
            let url = if cached_archive(ctx, sha256).is_file() || ctx.offline {
                String::new()
            } else {
                source::pecl_archive_url(name, version)?
            };
            (sha256, url)
        }
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
        let actual = download_or_discard(&url, &temporary)?;
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
        publish_download(&temporary, &cached)?;
    } else if &source::sha256_file(&cached)? != sha256 {
        return Err(NativeError::new(NativeErrorKind::Integrity, "cached php extension source is corrupt").with_path(&cached));
    }
    source::extract(&cached, &work.join("tree"))
}

/// The cache path of a downloaded source archive, named by its SHA-256 digest.
fn cached_archive(ctx: &Context, sha256: &str) -> PathBuf {
    ctx.cache_dir().join("sources").join(format!("{sha256}.tar.gz"))
}

/// Builds the Network error reported when offline mode is on and an extension's source archive is
/// not cached.
fn offline_missing(name: &str) -> NativeError {
    NativeError::new(NativeErrorKind::Network, format!("offline mode: php extension '{name}' is not cached"))
}

/// Builds (or reuses) one declared extension.
fn ensure_built(ctx: &mut Context, name: &str, source: &ExtensionSource) -> Result<InstalledExtension, NativeError> {
    let identity = source_identity(&ctx.project.root, source)?;
    let toolchain = crate::native_deps::toolchain::resolve_toolchain(ctx.target)?;
    let final_dir =
        artifact_dir(&ctx.cache.root, name, &identity, &headers::identity(), ctx.target, &toolchain.fingerprint);
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
        let include_dir = headers::materialize(&ctx.cache_dir())?;
        let engine_archive = (ctx.locate_engine)()
            .map_err(|error| NativeError::new(NativeErrorKind::Build, format!("locate the PHP extension engine: {error}")))?;
        // Created last, so every early return above leaves nothing behind.
        let staging = final_dir.with_extension(format!("stage.{}", std::process::id()));
        let _ = fs::remove_dir_all(&staging);
        fs::create_dir_all(&staging).map_err(|error| NativeError::io("create php extension staging", &staging, error))?;
        let inputs = build::BuildInputs {
            name,
            source_dir: &tree,
            include_dir: &include_dir,
            engine_archive: &engine_archive,
            toolchain: &toolchain,
            target: ctx.target,
            work_dir: &work.join("build"),
            output_dir: &staging,
        };
        let published = build::build(&inputs).and_then(|_| publish(ctx, name, &staging, &final_dir));
        // Whatever happened, the staging directory is spent: renamed into
        // place, superseded by another process's artifact, or abandoned by an
        // error (a failed build, lock or cache directory creation).
        let _ = fs::remove_dir_all(&staging);
        published
    })();
    let _ = fs::remove_dir_all(&work);
    result
}

/// Publishes a built `staging` directory as `final_dir`, or adopts an
/// equivalent artifact another process published first.
fn publish(ctx: &Context, name: &str, staging: &Path, final_dir: &Path) -> Result<InstalledExtension, NativeError> {
    // Another process building the same key may have published first: its
    // artifact is equivalent, so it wins and this one is dropped. Only a
    // directory that does not load (an interrupted older layout) is replaced.
    // The check, the removal and the rename hold the key's lock, so no
    // process can delete a build another one has just published.
    let key = final_dir.file_name().map(|key| key.to_string_lossy().into_owned()).unwrap_or_default();
    let lock_path = ctx.cache.locks.join("extension").join(format!("{name}-{key}.lock"));
    let _publish_lock = ctx.cache.lock(&lock_path, "extension install")?;
    if let Ok(installed) = load_installed(name, final_dir) {
        return Ok(installed);
    }
    if let Some(parent) = final_dir.parent() {
        fs::create_dir_all(parent).map_err(|error| NativeError::io("create php extension cache", parent, error))?;
    }
    if let Err(error) = replace_dir(staging, final_dir) {
        return load_installed(name, final_dir).map_err(|_| NativeError::io("publish php extension", final_dir, error));
    }
    load_installed(name, final_dir)
}

/// Puts `staging` at `final_dir` so a reader that takes no lock never finds the
/// path missing: a rename when nothing is there, otherwise an atomic exchange of
/// the two directories (the caller then deletes `staging`, which holds the
/// directory that did not load). Where the platform has no exchange, the old
/// directory is renamed aside first, which narrows the gap to two renames
/// instead of a recursive delete.
fn replace_dir(staging: &Path, final_dir: &Path) -> std::io::Result<()> {
    if !final_dir.exists() {
        return fs::rename(staging, final_dir);
    }
    if exchange_dirs(staging, final_dir).is_ok() {
        return Ok(());
    }
    let aside = final_dir.with_extension(format!("old.{}", std::process::id()));
    let _ = fs::remove_dir_all(&aside);
    fs::rename(final_dir, &aside)?;
    let renamed = fs::rename(staging, final_dir);
    let _ = fs::remove_dir_all(&aside);
    renamed
}

/// Atomically swaps two directories (`renameat2(RENAME_EXCHANGE)` on Linux,
/// `renamex_np(RENAME_SWAP)` on macOS); an error on any other platform.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn exchange_dirs(first: &Path, second: &Path) -> std::io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let to_c = |path: &Path| CString::new(path.as_os_str().as_bytes()).map_err(|_| std::io::ErrorKind::InvalidInput);
    let (first, second) = (to_c(first)?, to_c(second)?);
    #[cfg(target_os = "linux")]
    // SAFETY: both paths are NUL-terminated C strings that outlive the call.
    let status = unsafe {
        libc::syscall(libc::SYS_renameat2, libc::AT_FDCWD, first.as_ptr(), libc::AT_FDCWD, second.as_ptr(), 2 as libc::c_uint)
    };
    #[cfg(target_os = "macos")]
    // SAFETY: both paths are NUL-terminated C strings that outlive the call.
    let status = unsafe { libc::renamex_np(first.as_ptr(), second.as_ptr(), libc::RENAME_SWAP) };
    if status == 0 { Ok(()) } else { Err(std::io::Error::last_os_error()) }
}

/// Atomically swaps two directories: unsupported on this platform.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn exchange_dirs(_first: &Path, _second: &Path) -> std::io::Result<()> {
    Err(std::io::ErrorKind::Unsupported.into())
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
        AddSpec::Path { name, path } => {
            Ok((name.clone(), ExtensionSource::Path { path: manifest_path_for(&ctx.cwd, &ctx.project.root, path) }))
        }
        AddSpec::Pecl { name, version } => {
            let version = match version {
                Some(version) => version.clone(),
                None => source::pecl_latest_stable(name)?,
            };
            // The version becomes part of the download URL: refuse a range or
            // a malformed one before anything is fetched.
            validate_exact_version(name, &version)?;
            let sha256 = fetch_for_pin(ctx, &source::pecl_archive_url(name, &version)?)?;
            Ok((name.clone(), ExtensionSource::Pecl { version, sha256 }))
        }
        AddSpec::Pie { package, version } => {
            if let Some(version) = version {
                validate_exact_version(package, version)?;
            }
            let release = source::pie_resolve(package, version.as_deref())?;
            validate_extension_name(&release.extension)?;
            validate_exact_version(&release.extension, &release.version)?;
            let sha256 = fetch_for_pin(ctx, &release.url)?;
            Ok((
                release.extension.clone(),
                ExtensionSource::Pie { package: release.package, version: release.version, sha256 },
            ))
        }
    }
}

/// The path to record for a `--path` source. The user wrote it relative to the
/// directory they ran the command from, but the manifest resolves a relative path
/// against the project root, and the two differ when the command runs from a
/// subdirectory or with `--manifest-path`. So the path is resolved against `cwd`
/// first: recorded relative to the project root when it lies inside it (the
/// manifest stays portable), absolute otherwise.
fn manifest_path_for(cwd: &Path, project_root: &Path, path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() { path.to_path_buf() } else { cwd.join(path) };
    let (Ok(resolved), Ok(root)) = (fs::canonicalize(&absolute), fs::canonicalize(project_root)) else {
        // Not a directory yet: keep the lexical answer, which the build then reports.
        return absolute;
    };
    match resolved.strip_prefix(&root) {
        Ok(relative) if !relative.as_os_str().is_empty() => relative.to_path_buf(),
        Ok(_) => PathBuf::from("."),
        Err(_) => resolved,
    }
}

/// Downloads an archive for the first time and files it under its digest.
fn fetch_for_pin(ctx: &Context, url: &str) -> Result<String, NativeError> {
    if ctx.offline {
        return Err(NativeError::new(NativeErrorKind::Network, "offline mode: `extension add` must download the source to pin it"));
    }
    let sources = ctx.cache_dir().join("sources");
    let temporary = sources.join(format!("pin.{}.tar.gz", std::process::id()));
    let sha256 = download_or_discard(url, &temporary)?;
    let cached = sources.join(format!("{sha256}.tar.gz"));
    publish_download(&temporary, &cached)?;
    Ok(sha256)
}

/// Downloads `url` into `temporary` and returns its SHA-256, removing the
/// partial file when the download fails (refused, interrupted, oversized).
fn download_or_discard(url: &str, temporary: &Path) -> Result<String, NativeError> {
    source::download(url, temporary).inspect_err(|_| {
        let _ = fs::remove_file(temporary);
    })
}

/// Moves a verified download to its digest-named cache path, removing the
/// temporary file when the move fails.
fn publish_download(temporary: &Path, cached: &Path) -> Result<(), NativeError> {
    fs::rename(temporary, cached).map_err(|error| {
        let _ = fs::remove_file(temporary);
        NativeError::io("publish php extension source", cached, error)
    })
}

/// Renders a one-line summary of an installed extension (name, version, counts of functions,
/// classes and constants, a C++ marker) followed by a line per function that cannot be called yet
/// and why.
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
pub fn add(
    cwd: &Path,
    spec: &AddSpec,
    manifest_path: Option<&Path>,
    target: Option<Target>,
    offline: bool,
    locate_engine: EngineLocator,
) -> Result<String, NativeError> {
    let mut ctx = Context::open(cwd, manifest_path, target, offline, true)?;
    ctx.locate_engine = locate_engine;
    let (name, source) = pin(&ctx, spec)?;
    let installed = ensure_built(&mut ctx, &name, &source)?;
    let _project_lock = ctx.lock_project("extension add")?;
    let mut manifest = ctx.manifest()?;
    manifest.set_extension(&name, source)?;
    ctx.save(&manifest)?;
    ctx.output.push_str(&format!("added php extension {}\n", describe(&installed)));
    Ok(ctx.output)
}

/// `elephc extension install`: builds every declared extension that is missing.
pub fn install(
    cwd: &Path,
    manifest_path: Option<&Path>,
    target: Option<Target>,
    offline: bool,
    locate_engine: EngineLocator,
) -> Result<String, NativeError> {
    let mut ctx = Context::open(cwd, manifest_path, target, offline, false)?;
    ctx.locate_engine = locate_engine;
    let manifest = ctx.manifest()?;
    if manifest.extensions().is_empty() {
        return Ok("no php extensions declared\n".to_string());
    }
    for (name, source) in manifest.extensions() {
        let installed = ensure_built(&mut ctx, name, source)?;
        ctx.output.push_str(&format!("installed php extension {}\n", describe(&installed)));
    }
    Ok(ctx.output)
}

/// `elephc extension remove`.
pub fn remove(cwd: &Path, name: &str, manifest_path: Option<&Path>) -> Result<String, NativeError> {
    let ctx = Context::open(cwd, manifest_path, None, false, false)?;
    let _project_lock = ctx.lock_project("extension remove")?;
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
    let mut out = String::new();
    let fingerprint = if manifest.extensions().is_empty() { String::new() } else { ctx.toolchain_fingerprint()? };
    for (name, source) in manifest.extensions() {
        let identity = source_identity(&ctx.project.root, source)?;
        let dir = artifact_dir(&ctx.cache.root, name, &identity, &headers::identity(), ctx.target, &fingerprint);
        let state = match load_installed(name, &dir) {
            Ok(installed) => describe(&installed),
            Err(_) => format!("{name} — not built; run `elephc extension install`"),
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
    let cwd = std::env::current_dir().map_err(|error| NativeError::io("read current directory", Path::new("."), error))?;
    let cache = CacheLayout::from_environment(&cwd)?;
    let fingerprint = crate::native_deps::toolchain::resolve_toolchain(target)?.fingerprint;
    let mut extensions = Vec::new();
    for (name, source) in manifest.extensions() {
        let identity = source_identity(&project.root, source)?;
        let dir = artifact_dir(&cache.root, name, &identity, &headers::identity(), target, &fingerprint);
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

    /// Verifies `add` specifications parse to the right source: a bare PECL name, a PECL name with
    /// a version, a `vendor/package` PIE name with a version, and a name with a path.
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
        let abi = "abi-digest";
        let target = Target::detect_host();
        let cc = "toolchain";
        let base = artifact_dir(root, "apcu", "aaa", abi, target, cc);
        assert_eq!(base, artifact_dir(root, "apcu", "aaa", abi, target, cc), "deterministic");
        assert_ne!(base, artifact_dir(root, "apcu", "bbb", abi, target, cc), "source identity");
        assert_ne!(base, artifact_dir(root, "apcu", "aaa", "other-abi", target, cc), "ABI headers");
        assert_ne!(base, artifact_dir(root, "igbinary", "aaa", abi, target, cc), "extension name");
        assert_ne!(base, artifact_dir(root, "apcu", "aaa", abi, target, "musl-cc"), "toolchain and libc");
        assert!(base.starts_with("/cache/extension/apcu"));
    }

    /// Verifies `pin` refuses a version range for PECL and PIE before any
    /// URL is built or anything is fetched: offline, a range used to reach
    /// the download and fail there with an offline error instead.
    #[test]
    fn pin_validates_the_version_before_fetching() {
        let dir = std::env::temp_dir().join(format!("elephc-pin-version-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch project");
        fs::write(dir.join("elephc.toml"), "").expect("manifest");
        let ctx = Context::open(&dir, None, None, true, false).expect("context");
        let pecl = pin(&ctx, &AddSpec::Pecl { name: "apcu".into(), version: Some("^5.1".into()) });
        let pie = pin(&ctx, &AddSpec::Pie { package: "apcu/apcu".into(), version: Some(">=5".into()) });
        let _ = fs::remove_dir_all(&dir);
        for result in [pecl, pie] {
            let error = result.expect_err("a range is refused").to_string();
            assert!(error.contains("must be exact"), "{error}");
        }
    }

    /// Verifies a `--path` given relative to a subdirectory is recorded relative to the project
    /// root (so the manifest, which resolves against the root, finds it), and one outside the
    /// project is recorded absolute.
    #[test]
    fn a_relative_path_source_is_resolved_against_the_working_directory() {
        let base = std::env::temp_dir().join(format!("elephc-path-source-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let project = base.join("project");
        let sub = project.join("sub");
        fs::create_dir_all(project.join("ext/demo")).expect("extension dir");
        fs::create_dir_all(&sub).expect("working dir");
        fs::create_dir_all(base.join("elsewhere/other")).expect("outside dir");
        let inside = manifest_path_for(&sub, &project, Path::new("../ext/demo"));
        let from_root = manifest_path_for(&project, &project, Path::new("ext/demo"));
        let outside = manifest_path_for(&sub, &project, Path::new("../../elsewhere/other"));
        let expected_outside = fs::canonicalize(base.join("elsewhere/other")).expect("canonical");
        let _ = fs::remove_dir_all(&base);
        assert_eq!(inside, PathBuf::from("ext/demo"));
        assert_eq!(from_root, PathBuf::from("ext/demo"));
        assert_eq!(outside, expected_outside);
    }

    /// Verifies publication replaces a directory that does not load without ever leaving the
    /// final path missing: the staged tree is at the path afterwards, and the old content is
    /// left in the staging path for the caller to delete.
    #[test]
    fn replacing_a_published_directory_keeps_the_path_present() {
        let base = std::env::temp_dir().join(format!("elephc-replace-dir-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let (staging, published) = (base.join("stage"), base.join("final"));
        fs::create_dir_all(&staging).expect("staging");
        fs::create_dir_all(&published).expect("published");
        fs::write(staging.join("new"), "new").expect("staged file");
        fs::write(published.join("old"), "old").expect("old file");
        replace_dir(&staging, &published).expect("replace");
        let (has_new, has_old) = (published.join("new").is_file(), published.join("old").exists());
        // Nothing there yet: a plain rename.
        let (fresh_stage, fresh) = (base.join("stage2"), base.join("fresh"));
        fs::create_dir_all(&fresh_stage).expect("second staging");
        replace_dir(&fresh_stage, &fresh).expect("first publish");
        let fresh_present = fresh.is_dir();
        let _ = fs::remove_dir_all(&base);
        assert!(has_new && !has_old, "the staged tree replaced the old one");
        assert!(fresh_present);
    }

    /// Verifies a download that cannot be moved into the cache does not
    /// leave its temporary file behind.
    #[test]
    fn a_failed_publish_removes_the_temporary_download() {
        let dir = std::env::temp_dir().join(format!("elephc-download-cleanup-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch dir");
        let temporary = dir.join("pin.1.tar.gz");
        fs::write(&temporary, "partial").expect("temporary download");
        let error = publish_download(&temporary, &dir.join("missing").join("x.tar.gz"));
        let leftover = temporary.exists();
        let _ = fs::remove_dir_all(&dir);
        assert!(error.is_err());
        assert!(!leftover, "the temporary download must be removed");
    }
}
