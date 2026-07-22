//! Purpose:
//! Owns Elephc bridge metadata, archive discovery, and source-checkout auto-builds.
//! Resolves named bridge requirements into exact typed archive inputs when available.
//!
//! Called from:
//! - `crate::linker` before target-specific linker command rendering.
//! - `crate::cli` and `crate::pipeline` for `--with-<bridge>` validation and forcing.
//!
//! Key details:
//! - The bridge table remains the single source for flags, archives, frameworks, and libdl needs.
//! - An unresolved, empty, non-file, or symlinked bridge fails before command rendering.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

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
    /// Whether the Linux link needs the dynamic loader library.
    pub(super) needs_libdl: bool,
}

/// Every Elephc bridge known to discovery and CLI flag validation.
pub(super) const BRIDGES: &[BridgeStaticlib] = &[
    BridgeStaticlib {
        lib_name: "elephc_tls",
        env_var: "ELEPHC_TLS_LIB_DIR",
        crate_name: "elephc-tls",
        flag_name: "tls",
        whole_archive: true,
        macos_frameworks: &[],
        needs_libdl: true,
    },
    BridgeStaticlib {
        lib_name: "elephc_pdo",
        env_var: "ELEPHC_PDO_LIB_DIR",
        crate_name: "elephc-pdo",
        flag_name: "pdo",
        whole_archive: false,
        macos_frameworks: &["CoreFoundation", "SystemConfiguration"],
        needs_libdl: true,
    },
    BridgeStaticlib {
        lib_name: "elephc_crypto",
        env_var: "ELEPHC_CRYPTO_LIB_DIR",
        crate_name: "elephc-crypto",
        flag_name: "crypto",
        whole_archive: false,
        macos_frameworks: &[],
        needs_libdl: true,
    },
    BridgeStaticlib {
        lib_name: "elephc_phar",
        env_var: "ELEPHC_PHAR_LIB_DIR",
        crate_name: "elephc-phar",
        flag_name: "phar",
        whole_archive: false,
        macos_frameworks: &[],
        needs_libdl: true,
    },
    BridgeStaticlib {
        lib_name: "elephc_tz",
        env_var: "ELEPHC_TZ_LIB_DIR",
        crate_name: "elephc-tz",
        flag_name: "tz",
        whole_archive: false,
        macos_frameworks: &[],
        needs_libdl: true,
    },
    BridgeStaticlib {
        lib_name: "elephc_image",
        env_var: "ELEPHC_IMAGE_LIB_DIR",
        crate_name: "elephc-image",
        flag_name: "image",
        whole_archive: false,
        macos_frameworks: &[],
        needs_libdl: true,
    },
    BridgeStaticlib {
        lib_name: "elephc_web",
        env_var: "ELEPHC_WEB_LIB_DIR",
        crate_name: "elephc-web",
        flag_name: "web",
        whole_archive: true,
        macos_frameworks: &[],
        needs_libdl: true,
    },
    BridgeStaticlib {
        lib_name: "elephc_magician",
        env_var: "ELEPHC_MAGICIAN_LIB_DIR",
        crate_name: "elephc-magician",
        flag_name: "eval",
        whole_archive: false,
        macos_frameworks: &[],
        needs_libdl: true,
    },
];

/// A typed plan after known bridge names have been resolved as far as possible.
#[derive(Debug)]
pub(super) struct BridgeResolution {
    /// Plan with located bridges converted to exact archive paths.
    pub(super) plan: LinkPlan,
    /// Whether any requested bridge needs `libdl` on Linux.
    pub(super) needs_libdl: bool,
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
    let mut located: HashMap<&'static str, PathBuf> = HashMap::new();
    let mut bridge_paths = Vec::new();
    let mut seen_paths = HashSet::new();
    let mut frameworks = Vec::new();
    let mut seen_frameworks = HashSet::new();
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
    Ok(BridgeResolution { plan, needs_libdl })
}

/// Accumulates table-driven runtime and framework metadata for one requested bridge.
fn record_bridge_metadata(
    bridge: &BridgeStaticlib,
    needs_libdl: &mut bool,
    frameworks: &mut Vec<LinkItem>,
    seen_frameworks: &mut HashSet<&'static str>,
) {
    *needs_libdl |= bridge.needs_libdl;
    for framework in bridge.macos_frameworks {
        if seen_frameworks.insert(*framework) {
            frameworks.push(LinkItem::Framework((*framework).to_string()));
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
            return self.validate_archive(archive);
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

    /// Finds the nearest ancestor containing this bridge's Cargo package.
    fn find_workspace(&self) -> Option<PathBuf> {
        let manifest = format!("crates/{}/Cargo.toml", self.crate_name);
        let cwd = std::env::current_dir().ok()?;
        cwd.ancestors()
            .find(|directory| directory.join(&manifest).exists())
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
        if release {
            command.arg("--release");
        }
        let _ = command.current_dir(workspace).status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let magician = bridge_for_library("elephc_magician").expect("eval bridge");
        assert_eq!(magician.crate_name, "elephc-magician");
        assert_eq!(magician.env_var, "ELEPHC_MAGICIAN_LIB_DIR");
        assert_eq!(magician.archive_filename(), "libelephc_magician.a");
        assert!(!magician.whole_archive);
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
