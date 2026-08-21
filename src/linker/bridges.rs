//! Purpose:
//! Owns Elephc bridge metadata, archive discovery, and source-checkout auto-builds.
//! Resolves named bridge requirements into exact typed archive inputs when available.
//!
//! Called from:
//! - `crate::linker` before target-specific linker command rendering.
//! - `crate::cli` and `crate::pipeline` for `--with-<bridge>` validation and forcing.
//!
//! Key details:
//! - The bridge table remains the single source for flags, archives, macOS ABI inputs, and libdl needs.
//! - An unresolved, empty, non-file, or symlinked bridge fails before command rendering.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};

use crate::link_plan::{LinkItem, LinkOrigin, LinkPlan};

use super::LinkError;

/// A Rust `staticlib` bridge that can be linked into generated programs.
pub(super) struct BridgeStaticlib {
    /// Linker library name without the `lib` prefix or archive extension.
    pub(super) lib_name: &'static str,
    /// Environment override pointing at the directory containing the archive.
    pub(super) env_var: &'static str,
    /// Cargo package that produces the archive in a source checkout.
    pub(super) crate_name: &'static str,
    /// User-facing suffix accepted by `--with-<flag_name>`.
    pub(super) flag_name: &'static str,
    /// Whether link-time side effects require the whole archive by default.
    pub(super) whole_archive: bool,
    /// macOS frameworks required by this bridge's transitive dependencies.
    pub(super) macos_frameworks: &'static [&'static str],
    /// macOS ABI libraries required by vendored native objects in this bridge.
    pub(super) macos_libraries: &'static [&'static str],
    /// Whether the Linux link needs the dynamic loader library.
    pub(super) needs_libdl: bool,
    /// Canonical PHP extension reported when this bridge is linked, if distinct.
    pub(super) php_extension: Option<&'static str>,
}

/// Every Elephc bridge known to discovery and CLI flag validation.
pub(super) const BRIDGES: &[BridgeStaticlib] = &[
    BridgeStaticlib {
        lib_name: "elephc_tls",
        env_var: "ELEPHC_TLS_LIB_DIR",
        crate_name: "elephc-tls",
        flag_name: "tls",
        whole_archive: false,
        macos_frameworks: &[],
        macos_libraries: &[],
        needs_libdl: true,
        // The TLS bridge implements PHP's OpenSSL-backed stream crypto surface.
        php_extension: Some("openssl"),
    },
    BridgeStaticlib {
        lib_name: "elephc_pdo",
        env_var: "ELEPHC_PDO_LIB_DIR",
        crate_name: "elephc-pdo",
        flag_name: "pdo",
        whole_archive: false,
        macos_frameworks: &["CoreFoundation", "SystemConfiguration"],
        macos_libraries: &[],
        needs_libdl: true,
        // The bridge exposes the core PDO database-access surface.
        php_extension: Some("PDO"),
    },
    BridgeStaticlib {
        lib_name: "elephc_dom",
        env_var: "ELEPHC_DOM_LIB_DIR",
        crate_name: "elephc-dom",
        flag_name: "dom",
        whole_archive: false,
        macos_frameworks: &[],
        macos_libraries: &["iconv"],
        needs_libdl: true,
        // The bridge implements PHP's DOM, libxml, and SimpleXML surfaces.
        php_extension: Some("dom"),
    },
    BridgeStaticlib {
        lib_name: "elephc_crypto",
        env_var: "ELEPHC_CRYPTO_LIB_DIR",
        crate_name: "elephc-crypto",
        flag_name: "crypto",
        whole_archive: false,
        macos_frameworks: &[],
        macos_libraries: &[],
        needs_libdl: true,
        // The crypto bridge implements PHP's digest/HMAC `hash` extension.
        php_extension: Some("hash"),
    },
    BridgeStaticlib {
        lib_name: "elephc_bcmath",
        env_var: "ELEPHC_BCMATH_LIB_DIR",
        crate_name: "elephc-bcmath",
        flag_name: "bcmath",
        whole_archive: false,
        macos_frameworks: &[],
        macos_libraries: &[],
        needs_libdl: true,
        // The decimal bridge implements PHP's procedural `bcmath` extension.
        php_extension: Some("bcmath"),
    },
    BridgeStaticlib {
        lib_name: "elephc_phar",
        env_var: "ELEPHC_PHAR_LIB_DIR",
        crate_name: "elephc-phar",
        flag_name: "phar",
        whole_archive: false,
        macos_frameworks: &[],
        macos_libraries: &[],
        needs_libdl: true,
        // The archive reader/writer is exposed by PHP as `Phar`.
        php_extension: Some("Phar"),
    },
    BridgeStaticlib {
        lib_name: "elephc_tz",
        env_var: "ELEPHC_TZ_LIB_DIR",
        crate_name: "elephc-tz",
        flag_name: "tz",
        whole_archive: false,
        macos_frameworks: &[],
        macos_libraries: &[],
        needs_libdl: true,
        // Timezone support folds into the always-present `date` extension.
        php_extension: None,
    },
    BridgeStaticlib {
        lib_name: "elephc_image",
        env_var: "ELEPHC_IMAGE_LIB_DIR",
        crate_name: "elephc-image",
        flag_name: "image",
        whole_archive: false,
        macos_frameworks: &[],
        macos_libraries: &[],
        needs_libdl: true,
        // The image codec/drawing surface maps to PHP's `gd` extension.
        php_extension: Some("gd"),
    },
    BridgeStaticlib {
        lib_name: "elephc_web",
        env_var: "ELEPHC_WEB_LIB_DIR",
        crate_name: "elephc-web",
        flag_name: "web",
        whole_archive: true,
        macos_frameworks: &[],
        macos_libraries: &[],
        needs_libdl: true,
        // The web bridge owns the PHP `session` extension surface.
        php_extension: Some("session"),
    },
    BridgeStaticlib {
        lib_name: "elephc_magician",
        env_var: "ELEPHC_MAGICIAN_LIB_DIR",
        crate_name: "elephc-magician",
        flag_name: "eval",
        whole_archive: false,
        macos_frameworks: &[],
        macos_libraries: &[],
        needs_libdl: true,
        // The eval interpreter is an internal compiler facility, not an extension.
        php_extension: None,
    },
];

/// A typed plan after known bridge names have been resolved as far as possible.
#[derive(Debug)]
pub(super) struct BridgeResolution {
    /// Plan with located bridges converted to exact archive paths.
    pub(super) plan: LinkPlan,
    /// Whether any requested bridge needs `libdl` on Linux.
    pub(super) needs_libdl: bool,
    /// macOS-only ABI libraries required by the resolved bridge set.
    pub(super) macos_libraries: Vec<String>,
}

/// Resolves a `--with-<flag>` name to its bridge linker library name.
pub(super) fn bridge_lib_for_flag(flag: &str) -> Option<&'static str> {
    BRIDGES
        .iter()
        .find(|bridge| bridge.flag_name == flag)
        .map(|bridge| bridge.lib_name)
}

/// Returns all accepted `--with-<flag>` suffixes in stable table order.
pub(super) fn crate_flag_names() -> Vec<&'static str> {
    BRIDGES.iter().map(|bridge| bridge.flag_name).collect()
}

/// Returns bridge library/flag pairs present in one planned named-library set.
pub(super) fn bridges_in(
    link_libraries: &[String],
) -> Vec<(&'static str, &'static str)> {
    BRIDGES
        .iter()
        .filter(|bridge| {
            link_libraries
                .iter()
                .any(|library| library == bridge.lib_name)
        })
        .map(|bridge| (bridge.lib_name, bridge.flag_name))
        .collect()
}

/// Maps one bridge library name to its canonical PHP extension, when distinct.
pub(super) fn php_extension_for_lib(lib_name: &str) -> Option<&'static str> {
    bridge_for_library(lib_name).and_then(|bridge| bridge.php_extension)
}

/// Replaces located named bridge libraries with exact archive items and adds metadata.
pub(super) fn resolve(
    plan: &LinkPlan,
    forced_whole_archive: &[String],
) -> Result<BridgeResolution, LinkError> {
    resolve_with(plan, forced_whole_archive, BridgeStaticlib::archive_path)
}

/// Resolves bridges through an injected locator so missing-path behavior is deterministic in tests.
fn resolve_with<F>(
    plan: &LinkPlan,
    forced_whole_archive: &[String],
    mut locate: F,
) -> Result<BridgeResolution, LinkError>
where
    F: FnMut(&BridgeStaticlib) -> Result<PathBuf, LinkError>,
{
    let plan = plan.without_redundant_embedded_bridges();
    let mut located: HashMap<&'static str, PathBuf> = HashMap::new();
    let mut bridge_paths = Vec::new();
    let mut seen_paths = HashSet::new();
    let mut frameworks = Vec::new();
    let mut seen_frameworks = HashSet::new();
    let mut macos_libraries = Vec::new();
    let mut seen_macos_libraries = HashSet::new();
    let mut needs_libdl = false;
    let mut ordered = Vec::with_capacity(plan.items().len());

    for item in plan.items() {
        if let LinkItem::StaticArchive {
            path,
            origin: LinkOrigin::Bridge { name },
            ..
        } = item
        {
            if let Some(bridge) = bridge_for_library(name) {
                bridge.validate_archive(path.clone())?;
                record_bridge_metadata(
                    bridge,
                    &mut needs_libdl,
                    &mut frameworks,
                    &mut seen_frameworks,
                    &mut macos_libraries,
                    &mut seen_macos_libraries,
                );
            } else {
                validate_archive_path(name, path.clone())?;
            }
            ordered.push(item.clone());
            continue;
        }
        let LinkItem::NamedLibrary { name, .. } = item else {
            ordered.push(item.clone());
            continue;
        };
        let Some(bridge) = bridge_for_library(name) else {
            ordered.push(item.clone());
            continue;
        };

        record_bridge_metadata(
            bridge,
            &mut needs_libdl,
            &mut frameworks,
            &mut seen_frameworks,
            &mut macos_libraries,
            &mut seen_macos_libraries,
        );

        let archive = match located.get(bridge.lib_name) {
            Some(archive) => archive.clone(),
            None => {
                let archive = locate(bridge)?;
                located.insert(bridge.lib_name, archive.clone());
                archive
            }
        };
        if let Some(parent) = archive.parent() {
            let parent = parent.to_path_buf();
            if seen_paths.insert(parent.clone()) {
                bridge_paths.push(LinkItem::SearchPath(parent));
            }
        }
        let forced = forced_whole_archive
            .iter()
            .any(|forced| forced == bridge.lib_name);
        ordered.push(LinkItem::bridge_archive(
            archive,
            bridge.lib_name,
            bridge.whole_archive || forced,
        ));
    }

    ordered.extend(frameworks);
    let mut plan = LinkPlan::from_items(ordered);
    plan.prepend(bridge_paths);
    Ok(BridgeResolution {
        plan,
        needs_libdl,
        macos_libraries,
    })
}

/// Accumulates table-driven runtime, framework, and macOS ABI-library metadata.
fn record_bridge_metadata(
    bridge: &BridgeStaticlib,
    needs_libdl: &mut bool,
    frameworks: &mut Vec<LinkItem>,
    seen_frameworks: &mut HashSet<&'static str>,
    macos_libraries: &mut Vec<String>,
    seen_macos_libraries: &mut HashSet<&'static str>,
) {
    *needs_libdl |= bridge.needs_libdl;
    for framework in bridge.macos_frameworks {
        if seen_frameworks.insert(*framework) {
            frameworks.push(LinkItem::Framework((*framework).to_string()));
        }
    }
    for library in bridge.macos_libraries {
        if seen_macos_libraries.insert(*library) {
            macos_libraries.push((*library).to_string());
        }
    }
}

/// Accepts only non-empty regular archive files without following symbolic links.
fn validate_archive_path(name: &str, archive: PathBuf) -> Result<PathBuf, LinkError> {
    let valid = std::fs::symlink_metadata(&archive)
        .map(|metadata| metadata.file_type().is_file() && metadata.len() > 0)
        .unwrap_or(false);
    if valid {
        Ok(archive)
    } else {
        Err(LinkError::MissingBridge {
            name: name.to_string(),
        })
    }
}

/// Returns bridge metadata for one linker library name.
fn bridge_for_library(name: &str) -> Option<&'static BridgeStaticlib> {
    BRIDGES.iter().find(|bridge| bridge.lib_name == name)
}

/// Returns whether any file under `directory` was modified after `instant`.
///
/// Stops at the first one, so an up-to-date tree costs a full walk and a stale one usually
/// costs much less. An unreadable entry is skipped rather than treated as newer: this decides
/// whether to SPAWN CARGO, and a directory elephc cannot read is not evidence of an edit.
fn any_file_newer_than(directory: &Path, instant: std::time::SystemTime) -> bool {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        // A nested `target/` is this bridge's own build output, never its input.
        if metadata.is_dir() {
            if path.file_name().is_some_and(|name| name == "target") {
                continue;
            }
            if any_file_newer_than(&path, instant) {
                return true;
            }
        } else if metadata.modified().is_ok_and(|modified| modified > instant) {
            return true;
        }
    }
    false
}

impl BridgeStaticlib {
    /// Returns the archive filename produced by this bridge's Cargo package.
    pub(super) fn archive_filename(&self) -> String {
        format!("lib{}.a", self.lib_name)
    }

    /// Locates this bridge archive, auto-building it in a source checkout if needed.
    fn archive_path(&self) -> Result<PathBuf, LinkError> {
        if let Ok(env_dir) = std::env::var(self.env_var) {
            if !env_dir.is_empty() {
                return self.validate_archive(PathBuf::from(env_dir).join(self.archive_filename()));
            }
        }
        if let Some(archive) = self.find_archive() {
            if self.lib_name == "elephc_pdo"
                && super::pdo::profile_selected()
                && self.claim_rebuild_attempt()
            {
                if let Some(workspace) = self.find_workspace() {
                    self.build_staticlib(&workspace);
                    if let Some(rebuilt) = self.find_archive() {
                        return self.validate_archive(rebuilt);
                    }
                }
            }
            return self.validate_archive(self.refreshed_if_stale(archive));
        }
        if let Some(workspace) = self.find_workspace() {
            self.build_staticlib(&workspace);
            if let Some(archive) = self.find_archive() {
                return self.validate_archive(archive);
            }
        }
        Err(self.missing_error())
    }

    /// Validates that a configured bridge archive path names a regular file.
    fn validate_archive(&self, archive: PathBuf) -> Result<PathBuf, LinkError> {
        validate_archive_path(self.lib_name, archive)
    }

    /// Creates the structured error used by discovery and invalid environment overrides.
    fn missing_error(&self) -> LinkError {
        LinkError::MissingBridge {
            name: self.lib_name.to_string(),
        }
    }

    /// Returns the first installed or source-tree candidate containing this archive.
    fn find_archive(&self) -> Option<PathBuf> {
        let archive = self.archive_filename();
        let executable = std::env::current_exe().ok()?;
        let executable_dir = executable.parent()?;
        let mut candidates = vec![
            executable_dir.to_path_buf(),
            executable_dir
                .parent()
                .map(|parent| parent.join("lib"))
                .unwrap_or_default(),
        ];
        if let Ok(target_dir) = std::env::var("CARGO_TARGET_DIR") {
            if !target_dir.is_empty() {
                candidates.push(PathBuf::from(&target_dir).join("debug"));
                candidates.push(PathBuf::from(target_dir).join("release"));
            }
        }
        candidates.push(PathBuf::from("target/debug"));
        candidates.push(PathBuf::from("target/release"));

        candidates
            .into_iter()
            .map(|candidate| candidate.join(&archive))
            .find(|candidate| candidate.exists())
    }

    /// Rebuilds `archive` when this checkout's sources have moved past it.
    ///
    /// An EXISTING archive used to be trusted unconditionally, so a bridge edited after the
    /// last `cargo build` was linked in its old form. That fails as `Undefined symbols` naming
    /// a symbol the source plainly defines — a message that accuses the code rather than the
    /// stale file, and `cargo test` never refreshes a staticlib because it only needs the
    /// crate's rlib.
    ///
    /// Cargo is the real staleness oracle, but asking it costs a process spawn on every link,
    /// and elephc is spawned once per compiled program. The mtime comparison is the cheap
    /// pre-filter that decides whether that spawn is worth making: it stays silent on the
    /// overwhelmingly common up-to-date path, and the rebuild it does trigger makes the
    /// archive newer than the sources, so the next link goes quiet again.
    ///
    /// Every failure here — no workspace, unreadable metadata, a build that does not help —
    /// falls through to the archive we already had. This can only improve on the old
    /// behaviour, never replace a working link with an error.
    /// The attempt is made AT MOST ONCE per process, and that bound is load-bearing rather
    /// than an optimisation. Cargo, not this check, decides whether to rebuild; when it
    /// declines, the archive keeps its old timestamp and still looks stale. Without the bound
    /// every link would spawn cargo again forever, turning a rare staleness into a permanent
    /// tax on a compiler's hot path.
    fn refreshed_if_stale(&self, archive: PathBuf) -> PathBuf {
        let Some(workspace) = self.find_workspace() else {
            return archive;
        };
        if !self.sources_are_newer_than(&workspace, &archive) {
            return archive;
        }
        if !self.claim_rebuild_attempt() {
            return archive;
        }
        self.build_staticlib(&workspace);
        self.find_archive().unwrap_or(archive)
    }

    /// Registers this bridge as rebuild-attempted, returning whether the caller won the claim.
    fn claim_rebuild_attempt(&self) -> bool {
        static ATTEMPTED: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
        ATTEMPTED
            .get_or_init(|| Mutex::new(HashSet::new()))
            .lock()
            .is_ok_and(|mut attempted| attempted.insert(self.crate_name))
    }

    /// Returns whether any file under this bridge's crate is newer than `archive`.
    ///
    /// The whole crate directory is walked, not just `src`, because `Cargo.toml` and build
    /// scripts change what the archive contains too. The walk stops at the first newer file.
    fn sources_are_newer_than(&self, workspace: &Path, archive: &Path) -> bool {
        let Ok(built_at) = std::fs::metadata(archive).and_then(|meta| meta.modified()) else {
            return false;
        };
        let crate_dir = workspace.join("crates").join(self.crate_name);
        any_file_newer_than(&crate_dir, built_at)
    }

    /// Finds the checkout this elephc was built from, if it was built from one.
    ///
    /// The compile-time manifest directory identifies the exact checkout that produced this
    /// compiler, including worktrees whose Cargo target directory is shared with another
    /// checkout. The working directory and executable ancestry remain fallbacks for relocated
    /// or installed binaries where that original source tree no longer exists.
    ///
    /// An installed binary has neither, and correctly gets `None`: `/usr/local/bin/elephc` has
    /// no ancestor carrying elephc's crates, so nothing tries to run cargo on a user's machine.
    fn find_workspace(&self) -> Option<PathBuf> {
        let manifest = format!("crates/{}/Cargo.toml", self.crate_name);
        let compiled_from = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        if compiled_from.join(&manifest).exists() {
            return Some(compiled_from);
        }
        let from_cwd = std::env::current_dir()
            .ok()
            .and_then(|cwd| Self::ancestor_carrying(&cwd, &manifest));
        if from_cwd.is_some() {
            return from_cwd;
        }
        let from_executable = std::env::current_exe()
            .ok()
            .and_then(|executable| Self::ancestor_carrying(&executable, &manifest));
        from_executable
    }

    /// Returns the nearest ancestor of `start` that carries `manifest`.
    fn ancestor_carrying(start: &Path, manifest: &str) -> Option<PathBuf> {
        start
            .ancestors()
            .find(|directory| directory.join(manifest).exists())
            .map(Path::to_path_buf)
    }

    /// Best-effort builds this bridge in the active debug or release profile.
    fn build_staticlib(&self, workspace: &Path) {
        let release = std::env::current_exe()
            .ok()
            .and_then(|executable| executable.parent().map(Path::to_path_buf))
            .is_some_and(|directory| directory.file_name().is_some_and(|name| name == "release"));
        let mut command = Command::new("cargo");
        command.args(["build", "-p", self.crate_name]);
        if self.lib_name == "elephc_pdo" {
            let features = super::pdo::cargo_features();
            if !features.is_empty() {
                command.args(["--features", &features.join(",")]);
            }
        }
        if release {
            command.arg("--release");
        }
        let _ = command.current_dir(workspace).status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Creates an empty directory unique across parallel test threads.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "elephc_bridges_{}_{}_{:?}",
            name,
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    /// The checkout root is found from a nested starting point, and absent trees answer `None`.
    ///
    /// This is what lets an elephc invoked from a temp directory — every integration test —
    /// still locate the crates it was built from, by asking its own executable path.
    #[test]
    fn ancestor_carrying_finds_the_checkout_root() {
        let root = scratch("workspace");
        let manifest = "crates/elephc-magician/Cargo.toml";
        std::fs::create_dir_all(root.join("crates/elephc-magician")).expect("create crate dir");
        std::fs::write(root.join(manifest), "[package]").expect("write manifest");
        let nested = root.join("deep/deeper");
        std::fs::create_dir_all(&nested).expect("create nested dir");

        assert_eq!(
            BridgeStaticlib::ancestor_carrying(&nested, manifest).as_deref(),
            Some(root.as_path())
        );
        assert!(BridgeStaticlib::ancestor_carrying(&scratch("bare"), manifest).is_none());

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Verifies shared Cargo target directories cannot redirect bridge discovery to another worktree.
    #[test]
    fn bridge_workspace_prefers_the_compiler_manifest_checkout() {
        let bridge = bridge_for_library("elephc_web").expect("web bridge");
        assert_eq!(
            bridge.find_workspace().as_deref(),
            Some(Path::new(env!("CARGO_MANIFEST_DIR")))
        );
    }

    /// Staleness is decided by modification time, and a nested `target/` never counts.
    ///
    /// The `target/` exclusion is what keeps the check from seeing the bridge's own build
    /// output as a reason to rebuild it, which would make every link rebuild forever.
    #[test]
    fn any_file_newer_than_ignores_build_output() {
        let root = scratch("staleness");
        std::fs::write(root.join("lib.rs"), "// source").expect("write source");
        let source_time = std::fs::metadata(root.join("lib.rs"))
            .and_then(|meta| meta.modified())
            .expect("read source mtime");

        assert!(any_file_newer_than(&root, std::time::SystemTime::UNIX_EPOCH));
        assert!(!any_file_newer_than(&root, source_time));

        let output = scratch("staleness_output");
        std::fs::create_dir_all(output.join("target")).expect("create target dir");
        std::fs::write(output.join("target/libx.a"), "archive").expect("write archive");
        assert!(!any_file_newer_than(
            &output,
            std::time::SystemTime::UNIX_EPOCH
        ));

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&output);
    }

    /// Verifies every bridge flag maps back to the table's linker library name.
    #[test]
    fn crate_flags_map_back_to_bridge_names() {
        for bridge in BRIDGES {
            assert_eq!(bridge_lib_for_flag(bridge.flag_name), Some(bridge.lib_name));
        }
        assert_eq!(bridge_lib_for_flag("bogus"), None);
        assert_eq!(crate_flag_names().len(), BRIDGES.len());
    }

    /// Verifies representative bridge metadata and archive naming remain registered.
    #[test]
    fn representative_bridge_metadata_is_preserved() {
        let tls = bridge_for_library("elephc_tls").expect("TLS bridge");
        assert!(!tls.whole_archive);

        let crypto = bridge_for_library("elephc_crypto").expect("crypto bridge");
        assert_eq!(crypto.crate_name, "elephc-crypto");
        assert_eq!(crypto.env_var, "ELEPHC_CRYPTO_LIB_DIR");
        assert_eq!(crypto.archive_filename(), "libelephc_crypto.a");
        assert!(!crypto.whole_archive);

        let pdo = bridge_for_library("elephc_pdo").expect("pdo bridge");
        assert_eq!(
            pdo.macos_frameworks,
            &["CoreFoundation", "SystemConfiguration"]
        );

        let dom = bridge_for_library("elephc_dom").expect("DOM bridge");
        assert_eq!(dom.crate_name, "elephc-dom");
        assert_eq!(dom.env_var, "ELEPHC_DOM_LIB_DIR");
        assert_eq!(dom.archive_filename(), "libelephc_dom.a");
        assert_eq!(dom.macos_libraries, &["iconv"]);
        assert!(!dom.whole_archive);

        let magician = bridge_for_library("elephc_magician").expect("eval bridge");
        assert_eq!(magician.crate_name, "elephc-magician");
        assert_eq!(magician.env_var, "ELEPHC_MAGICIAN_LIB_DIR");
        assert_eq!(magician.archive_filename(), "libelephc_magician.a");
        assert!(!magician.whole_archive);
    }

    /// Verifies automatic TLS linking stays lazy while `--with-tls` force-loads the archive.
    #[test]
    fn tls_whole_archive_is_reserved_for_explicit_forcing() {
        let archive = std::env::current_exe().expect("test executable path");
        let plan = LinkPlan::from_items(vec![LinkItem::named_runtime("elephc_tls")]);

        for (forced, expected_whole_archive) in [(&[][..], false), (&["elephc_tls".to_string()][..], true)] {
            let resolution = resolve_with(&plan, forced, |_| Ok(archive.clone()))
                .expect("TLS bridge must resolve");
            assert!(resolution.plan.items().iter().any(|item| matches!(
                item,
                LinkItem::StaticArchive {
                    whole_archive,
                    origin: LinkOrigin::Bridge { name },
                    ..
                } if name == "elephc_tls" && *whole_archive == expected_whole_archive
            )));
        }
    }

    /// Verifies bridge progress selection and PHP extension reporting share the bridge table.
    #[test]
    fn bridge_reporting_metadata_matches_php_surface() {
        let libraries = vec![
            "pthread".to_string(),
            "elephc_tls".to_string(),
            "elephc_magician".to_string(),
        ];
        assert_eq!(
            bridges_in(&libraries),
            vec![("elephc_tls", "tls"), ("elephc_magician", "eval")]
        );
        assert_eq!(php_extension_for_lib("elephc_tls"), Some("openssl"));
        assert_eq!(php_extension_for_lib("elephc_pdo"), Some("PDO"));
        assert_eq!(php_extension_for_lib("elephc_dom"), Some("dom"));
        assert_eq!(php_extension_for_lib("elephc_crypto"), Some("hash"));
        assert_eq!(php_extension_for_lib("elephc_bcmath"), Some("bcmath"));
        assert_eq!(php_extension_for_lib("elephc_phar"), Some("Phar"));
        assert_eq!(php_extension_for_lib("elephc_image"), Some("gd"));
        assert_eq!(php_extension_for_lib("elephc_web"), Some("session"));
        assert_eq!(php_extension_for_lib("elephc_tz"), None);
        assert_eq!(php_extension_for_lib("elephc_magician"), None);
        assert_eq!(php_extension_for_lib("elephc_bogus"), None);
    }

    /// Verifies an already-resolved bridge archive still receives libdl and framework metadata.
    #[test]
    fn exact_bridge_archive_retains_table_driven_metadata() {
        let executable = std::env::current_exe().expect("test executable path");
        let archive = LinkItem::bridge_archive(executable, "elephc_pdo", false);
        let resolution = resolve_with(
            &LinkPlan::from_items(vec![archive.clone()]),
            &[],
            |_| panic!("an exact bridge archive must not trigger discovery"),
        )
        .expect("exact bridge metadata must resolve");

        assert!(resolution.needs_libdl);
        assert_eq!(resolution.plan.items()[0], archive);
        assert!(resolution
            .plan
            .items()
            .contains(&LinkItem::Framework("CoreFoundation".to_string())));
        assert!(resolution
            .plan
            .items()
            .contains(&LinkItem::Framework("SystemConfiguration".to_string())));
    }

    /// Verifies bridge resolution keeps Magician as the sole provider of its embedded crates.
    #[test]
    fn magician_replaces_standalone_embedded_bridge_archives() {
        let plan = LinkPlan::from_items(vec![
            LinkItem::named_runtime("elephc_crypto"),
            LinkItem::named_runtime("elephc_phar"),
            LinkItem::named_runtime("elephc_magician"),
        ]);
        let executable = std::env::current_exe().expect("test executable path");
        let resolution = resolve_with(&plan, &[], |_| Ok(executable.clone()))
            .expect("embedded bridge plan must resolve");
        let bridge_names: Vec<&str> = resolution
            .plan
            .items()
            .iter()
            .filter_map(|item| match item {
                LinkItem::StaticArchive {
                    origin: LinkOrigin::Bridge { name },
                    ..
                } => Some(name.as_str()),
                LinkItem::StaticArchive { .. }
                | LinkItem::NamedLibrary { .. }
                | LinkItem::SearchPath(_)
                | LinkItem::Framework(_) => None,
            })
            .collect();

        assert_eq!(bridge_names, vec!["elephc_magician"]);
    }

    /// Verifies a missing named bridge returns a structured error instead of a `-l` fallback.
    #[test]
    fn missing_named_bridge_is_structured_error() {
        let plan = LinkPlan::from_items(vec![LinkItem::named_runtime("elephc_tls")]);
        let error = resolve_with(&plan, &[], |bridge| Err(bridge.missing_error()))
            .expect_err("missing bridge must fail before command rendering");

        assert_eq!(
            error,
            LinkError::MissingBridge {
                name: "elephc_tls".to_string()
            }
        );
    }

    /// Verifies nonexistent and non-file override targets use the same structured bridge error.
    #[test]
    fn invalid_override_archive_is_structured_error() {
        let bridge = bridge_for_library("elephc_tls").expect("tls bridge");
        let nonexistent = std::env::temp_dir().join(format!(
            "elephc-missing-bridge-{}/libelephc_tls.a",
            std::process::id()
        ));
        assert_eq!(
            bridge.validate_archive(nonexistent),
            Err(LinkError::MissingBridge {
                name: "elephc_tls".to_string()
            })
        );
        assert_eq!(
            bridge.validate_archive(std::env::temp_dir()),
            Err(LinkError::MissingBridge {
                name: "elephc_tls".to_string()
            })
        );
    }

    /// Verifies exact bridge items reject empty files and symbolic links before rendering.
    #[test]
    fn exact_bridge_requires_nonempty_regular_nonsymlink_file() {
        let base = std::env::temp_dir().join(format!(
            "elephc-linker-bridge-validation-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&base).expect("create bridge validation fixture");
        let empty = base.join("empty.a");
        std::fs::write(&empty, b"").expect("create empty archive fixture");
        let empty_plan = LinkPlan::from_items(vec![LinkItem::bridge_archive(
            &empty,
            "elephc_tls",
            false,
        )]);
        assert!(matches!(
            resolve_with(&empty_plan, &[], |_| panic!("exact path must not invoke locator")),
            Err(LinkError::MissingBridge { name }) if name == "elephc_tls"
        ));

        let symlink = base.join("symlink.a");
        let _ = std::fs::remove_file(&symlink);
        std::os::unix::fs::symlink(std::env::current_exe().expect("test executable"), &symlink)
            .expect("create archive symlink fixture");
        assert!(matches!(
            validate_archive_path("elephc_tls", symlink.clone()),
            Err(LinkError::MissingBridge { name }) if name == "elephc_tls"
        ));

        let _ = std::fs::remove_file(empty);
        let _ = std::fs::remove_file(symlink);
        let _ = std::fs::remove_dir(base);
    }
}
