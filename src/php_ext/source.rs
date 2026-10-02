//! Purpose:
//! Acquires a hosted extension's source tree: a PECL release, a PIE package
//! from Packagist, or a directory on disk.
//!
//! Called from:
//! - `crate::php_ext::install`.
//!
//! Key details:
//! - PECL publishes no checksums. The archive `elephc extension add` downloads is
//!   hashed and the digest written to the manifest; every later download must
//!   match it, so a build cannot silently change under a pinned version.
//! - PIE packages are Packagist packages of type `php-ext`. Their metadata
//!   (`/p2/<vendor>/<name>.json`) is *minified*: each version lists only what
//!   changed from the one before it, and `"__unset"` removes a key. It is
//!   expanded here before any field is read.
//! - Packagist's dist for a GitHub-hosted package is a `zipball`; GitHub serves
//!   the same tree as a `tarball`, which the existing bounded tar extractor
//!   reads, so no zip reader is needed. Other hosts are refused by name.
//! - Only `php-ext` packages are accepted. `php-ext-zend` packages are Zend
//!   extensions, which hook the VM a compiled program does not have.

use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use ureq::{Agent, ResponseExt};

use crate::native_deps::archive::extract_archive_skipping;
use crate::native_deps::{ArchiveFormat, NativeError, NativeErrorKind};

/// Upper bound on one downloaded archive. The largest PECL extensions bundle
/// their C library (zstd, mongodb) and stay well under this.
const ARCHIVE_LIMIT: u64 = 64 * 1024 * 1024;
/// Upper bound on a metadata document.
const METADATA_LIMIT: u64 = 16 * 1024 * 1024;

fn network_error(message: impl Into<String>) -> NativeError {
    NativeError::new(NativeErrorKind::Network, message)
}

fn agent() -> Agent {
    Agent::config_builder()
        .https_only(true)
        .max_redirects(5)
        .timeout_connect(Some(Duration::from_secs(30)))
        .timeout_recv_body(Some(Duration::from_secs(60)))
        .timeout_global(Some(Duration::from_secs(5 * 60)))
        .user_agent("elephc")
        .build()
        .into()
}

/// Fetches a small text document over HTTPS.
fn fetch_text(url: &str) -> Result<String, NativeError> {
    let mut response = agent()
        .get(url)
        .call()
        .map_err(|error| network_error(format!("fetch {url}: {error}")))?;
    let mut body = String::new();
    response
        .body_mut()
        .as_reader()
        .take(METADATA_LIMIT)
        .read_to_string(&mut body)
        .map_err(|error| network_error(format!("read {url}: {error}")))?;
    Ok(body)
}

/// Streams `url` to `destination` and returns the SHA-256 of what was written.
pub fn download(url: &str, destination: &Path) -> Result<String, NativeError> {
    if !url.starts_with("https://") {
        return Err(network_error(format!("refusing a non-HTTPS source URL: {url}")));
    }
    let mut response = agent()
        .get(url)
        .call()
        .map_err(|error| network_error(format!("download {url}: {error}")))?;
    if response.get_uri().scheme_str() != Some("https") {
        return Err(network_error(format!("download of {url} ended at a non-HTTPS URL")));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| NativeError::io("create download directory", parent, error))?;
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(destination)
        .map_err(|error| NativeError::io("create download file", destination, error))?;
    let mut reader = response.body_mut().as_reader();
    let mut digest = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer).map_err(|error| network_error(format!("read {url}: {error}")))?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > ARCHIVE_LIMIT {
            return Err(NativeError::new(
                NativeErrorKind::Integrity,
                format!("{url} exceeds the {ARCHIVE_LIMIT}-byte source limit"),
            ));
        }
        digest.update(&buffer[..read]);
        output.write_all(&buffer[..read]).map_err(|error| NativeError::io("write download", destination, error))?;
    }
    output.sync_all().map_err(|error| NativeError::io("flush download", destination, error))?;
    Ok(format!("{:x}", digest.finalize()))
}

/// SHA-256 of a file on disk.
pub fn sha256_file(path: &Path) -> Result<String, NativeError> {
    let bytes = fs::read(path).map_err(|error| NativeError::io("read source archive", path, error))?;
    Ok(format!("{:x}", Sha256::digest(&bytes)))
}

// ---------------------------------------------------------------- PECL

/// The release archive of a PECL package.
pub fn pecl_url(name: &str, version: &str) -> String {
    format!("https://pecl.php.net/get/{name}-{version}.tgz")
}

/// The newest stable release PECL lists for a package.
pub fn pecl_latest_stable(name: &str) -> Result<String, NativeError> {
    let url = format!("https://pecl.php.net/rest/r/{name}/stable.txt");
    let version = fetch_text(&url)?.trim().to_string();
    if version.is_empty() || version.contains('<') {
        return Err(network_error(format!("PECL has no stable release of '{name}'")));
    }
    Ok(version)
}

// ---------------------------------------------------------------- PIE

/// One release of a PIE package, resolved from Packagist.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PieRelease {
    pub package: String,
    pub version: String,
    /// The extension's name (`php-ext.extension-name`, or the package name).
    pub extension: String,
    /// A GitHub tarball URL for the release.
    pub url: String,
}

/// Resolves `package` at `version` (or its newest stable release).
pub fn pie_resolve(package: &str, version: Option<&str>) -> Result<PieRelease, NativeError> {
    let url = format!("https://repo.packagist.org/p2/{package}.json");
    let text = fetch_text(&url)?;
    pie_select(package, &text, version)
}

/// Expands Packagist's minified version list: every entry inherits the keys
/// of the one before it, and `"__unset"` removes one.
fn expand_minified(entries: &[Value]) -> Vec<Map<String, Value>> {
    let mut expanded = Vec::with_capacity(entries.len());
    let mut current = Map::new();
    for entry in entries {
        if let Some(object) = entry.as_object() {
            for (key, value) in object {
                if value.as_str() == Some("__unset") {
                    current.remove(key);
                } else {
                    current.insert(key.clone(), value.clone());
                }
            }
        }
        expanded.push(current.clone());
    }
    expanded
}

fn is_stable(version: &str) -> bool {
    let lower = version.to_ascii_lowercase();
    !["dev", "alpha", "beta", "rc"].iter().any(|marker| lower.contains(marker))
}

/// Picks a release out of a `/p2/` document. Split from the fetch so it is
/// testable without the network.
fn pie_select(package: &str, document: &str, version: Option<&str>) -> Result<PieRelease, NativeError> {
    let root: Value = serde_json::from_str(document)
        .map_err(|error| network_error(format!("Packagist returned invalid JSON for {package}: {error}")))?;
    let entries = root
        .get("packages")
        .and_then(|packages| packages.get(package))
        .and_then(Value::as_array)
        .ok_or_else(|| network_error(format!("Packagist has no package '{package}'")))?;
    let releases = expand_minified(entries);
    let wanted = version.map(|v| v.trim_start_matches('v'));
    let release = releases
        .iter()
        .find(|release| {
            let name = release.get("version").and_then(Value::as_str).unwrap_or("");
            match wanted {
                Some(wanted) => name.trim_start_matches('v') == wanted,
                None => is_stable(name),
            }
        })
        .ok_or_else(|| match version {
            Some(version) => network_error(format!("PIE package '{package}' has no release '{version}'")),
            None => network_error(format!("PIE package '{package}' has no stable release")),
        })?;

    match release.get("type").and_then(Value::as_str) {
        Some("php-ext") => {}
        Some("php-ext-zend") => {
            return Err(NativeError::new(
                NativeErrorKind::Build,
                format!(
                    "'{package}' is a Zend extension: it hooks the PHP engine's executor, which a \
                     compiled program does not have"
                ),
            ))
        }
        other => {
            return Err(NativeError::new(
                NativeErrorKind::Build,
                format!("'{package}' is not a PIE extension (type {})", other.unwrap_or("none")),
            ))
        }
    }

    let release_version = release.get("version").and_then(Value::as_str).unwrap_or("").to_string();
    let extension = release
        .get("php-ext")
        .and_then(|meta| meta.get("extension-name"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| package.rsplit('/').next().unwrap_or(package).to_string());
    let extension = extension.trim_start_matches("ext-").to_string();
    let url = github_tarball(release).ok_or_else(|| {
        NativeError::new(
            NativeErrorKind::Build,
            format!("PIE package '{package}' is not hosted on GitHub; only GitHub-hosted PIE packages are supported"),
        )
    })?;
    Ok(PieRelease { package: package.to_string(), version: release_version, extension, url })
}

/// The GitHub tarball equivalent of a release's dist (or source) reference.
fn github_tarball(release: &Map<String, Value>) -> Option<String> {
    if let Some(url) = release.get("dist").and_then(|dist| dist.get("url")).and_then(Value::as_str) {
        if url.starts_with("https://api.github.com/repos/") && url.contains("/zipball/") {
            return Some(url.replacen("/zipball/", "/tarball/", 1));
        }
    }
    let source = release.get("source")?;
    let repository = source.get("url").and_then(Value::as_str)?;
    let reference = source.get("reference").and_then(Value::as_str)?;
    let path = repository
        .strip_prefix("https://github.com/")?
        .trim_end_matches(".git");
    Some(format!("https://api.github.com/repos/{path}/tarball/{reference}"))
}

// ---------------------------------------------------------------- trees

/// Extracts a downloaded archive and returns the directory holding
/// `config.m4`.
pub fn extract(archive: &Path, destination: &Path) -> Result<PathBuf, NativeError> {
    if destination.exists() {
        fs::remove_dir_all(destination)
            .map_err(|error| NativeError::io("clear extraction directory", destination, error))?;
    }
    extract_archive_skipping(archive, ArchiveFormat::TarGz, destination, &["package.xml", "package2.xml"])?;
    locate_config_m4(destination)
}

/// The directory holding `config.m4`: the root itself, or the single
/// subdirectory (at most two levels down) that has one, the way some PIE
/// repositories keep the extension beside other tooling.
pub fn locate_config_m4(root: &Path) -> Result<PathBuf, NativeError> {
    if root.join("config.m4").is_file() {
        return Ok(root.to_path_buf());
    }
    let mut found = Vec::new();
    collect_config_m4(root, 2, &mut found);
    match found.len() {
        1 => Ok(found.remove(0)),
        0 => Err(NativeError::new(NativeErrorKind::Build, "no config.m4 in the extension source").with_path(root)),
        _ => Err(NativeError::new(
            NativeErrorKind::Build,
            format!(
                "several config.m4 files in the extension source ({}); point a path source at the one to build",
                found.iter().map(|path| path.display().to_string()).collect::<Vec<_>>().join(", ")
            ),
        )),
    }
}

fn collect_config_m4(dir: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else { return };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();
    for sub in dirs {
        if sub.file_name().is_some_and(|name| name == "tests" || name == ".git") {
            continue;
        }
        if sub.join("config.m4").is_file() {
            found.push(sub);
        } else {
            collect_config_m4(&sub, depth - 1, found);
        }
    }
}

/// A content identity for a source tree on disk: the SHA-256 of every
/// regular file's relative path and bytes, in path order. Build leftovers a
/// phpize run may have left are ignored, as the build ignores them.
pub fn tree_identity(root: &Path) -> Result<String, NativeError> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    files.sort();
    let mut digest = Sha256::new();
    for relative in files {
        let bytes = fs::read(root.join(&relative))
            .map_err(|error| NativeError::io("read extension source", &root.join(&relative), error))?;
        digest.update(relative.to_string_lossy().as_bytes());
        digest.update([0u8]);
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(&bytes);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn collect_files(root: &Path, dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), NativeError> {
    let entries = fs::read_dir(dir).map_err(|error| NativeError::io("read extension source", dir, error))?;
    for entry in entries {
        let entry = entry.map_err(|error| NativeError::io("read extension source", dir, error))?;
        let name = entry.file_name();
        if matches!(name.to_str(), Some(".git" | ".libs" | "modules" | "autom4te.cache")) {
            continue;
        }
        let path = entry.path();
        let file_type = entry.file_type().map_err(|error| NativeError::io("inspect extension source", &path, error))?;
        if file_type.is_dir() {
            collect_files(root, &path, files)?;
        } else if file_type.is_file() {
            let name = name.to_string_lossy();
            if name.ends_with(".o") || name.ends_with(".lo") || name.ends_with(".la") {
                continue;
            }
            files.push(path.strip_prefix(root).unwrap_or(&path).to_path_buf());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A trimmed `/p2/` document in Packagist's minified shape: the second
    /// version inherits `type` and `php-ext` from the first, and unsets `dist`.
    const MINIFIED: &str = r#"{"minified":"composer/2.0","packages":{"acme/demo-ext":[
        {"name":"acme/demo-ext","version":"2.0.0-RC1","type":"php-ext",
         "php-ext":{"extension-name":"demo"},
         "dist":{"type":"zip","url":"https://api.github.com/repos/acme/demo-ext/zipball/aaa"},
         "source":{"type":"git","url":"https://github.com/acme/demo-ext.git","reference":"aaa"}},
        {"version":"1.2.3",
         "dist":"__unset",
         "source":{"type":"git","url":"https://github.com/acme/demo-ext.git","reference":"bbb"}}
    ]}}"#;

    #[test]
    fn picks_the_newest_stable_release_and_inherits_minified_keys() {
        let release = pie_select("acme/demo-ext", MINIFIED, None).expect("resolves");
        assert_eq!(release.version, "1.2.3", "2.0.0-RC1 is not stable");
        assert_eq!(release.extension, "demo", "php-ext is inherited from the entry before");
        assert_eq!(
            release.url,
            "https://api.github.com/repos/acme/demo-ext/tarball/bbb",
            "dist was unset, so the source reference is used"
        );
    }

    #[test]
    fn an_explicit_version_may_be_a_prerelease_and_turns_a_zipball_into_a_tarball() {
        let release = pie_select("acme/demo-ext", MINIFIED, Some("v2.0.0-RC1")).expect("resolves");
        assert_eq!(release.url, "https://api.github.com/repos/acme/demo-ext/tarball/aaa");
    }

    /// Zend extensions hook the executor; a compiled program has none.
    #[test]
    fn refuses_zend_extensions() {
        let document = r#"{"packages":{"x/debugger":[{"version":"1.0.0","type":"php-ext-zend",
            "source":{"url":"https://github.com/x/debugger.git","reference":"c"}}]}}"#;
        let error = pie_select("x/debugger", document, None).expect_err("zend extension");
        assert!(error.to_string().contains("Zend extension"), "{error}");
    }

    #[test]
    fn refuses_a_package_that_is_not_an_extension() {
        let document = r#"{"packages":{"x/lib":[{"version":"1.0.0","type":"library"}]}}"#;
        assert!(pie_select("x/lib", document, None).is_err());
    }

    #[test]
    fn builds_pecl_urls() {
        assert_eq!(pecl_url("apcu", "5.1.28"), "https://pecl.php.net/get/apcu-5.1.28.tgz");
    }

    #[test]
    fn locates_config_m4_in_a_single_subdirectory() {
        let root = std::env::temp_dir().join(format!("elephc-php-ext-locate-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("ext")).unwrap();
        fs::create_dir_all(root.join("tests").join("fixture")).unwrap();
        fs::write(root.join("ext").join("config.m4"), "").unwrap();
        fs::write(root.join("tests").join("fixture").join("config.m4"), "").unwrap();
        assert_eq!(locate_config_m4(&root).unwrap(), root.join("ext"), "tests/ is never the extension");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn tree_identity_follows_content_and_ignores_build_leftovers() {
        let root = std::env::temp_dir().join(format!("elephc-php-ext-tree-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join(".libs")).unwrap();
        fs::write(root.join("config.m4"), "a").unwrap();
        let first = tree_identity(&root).unwrap();
        fs::write(root.join(".libs").join("x.o"), "junk").unwrap();
        fs::write(root.join("demo.lo"), "junk").unwrap();
        assert_eq!(tree_identity(&root).unwrap(), first, "leftovers do not change the identity");
        fs::write(root.join("config.m4"), "b").unwrap();
        assert_ne!(tree_identity(&root).unwrap(), first, "content does");
        fs::remove_dir_all(&root).unwrap();
    }
}
