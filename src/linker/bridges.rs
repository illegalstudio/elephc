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
use std::sync::{Mutex, OnceLock};

use elephc_monitoring_contract::{
    IoKind, MonitoringPolicy, TraceContextPolicy, WaitPolicy,
};

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
    /// Apple frameworks required by this bridge's transitive dependencies.
    pub(super) apple_frameworks: &'static [&'static str],
    /// Whether the Linux link needs the dynamic loader library.
    pub(super) needs_libdl: bool,
    /// Canonical PHP extension reported when this bridge is linked, if distinct.
    pub(super) php_extension: Option<&'static str>,
    /// Required monitoring decision for work crossing this bridge boundary.
    pub(super) monitoring: MonitoringPolicy,
}

/// Every Elephc bridge known to discovery and CLI flag validation.
pub(super) const BRIDGES: &[BridgeStaticlib] = &[
    BridgeStaticlib {
        lib_name: "elephc_tls",
        env_var: "ELEPHC_TLS_LIB_DIR",
        crate_name: "elephc-tls",
        flag_name: "tls",
        whole_archive: false,
        apple_frameworks: &[],
        needs_libdl: true,
        // The TLS bridge implements PHP's OpenSSL-backed stream crypto surface.
        php_extension: Some("openssl"),
        monitoring: MonitoringPolicy::GenericTiming,
    },
    BridgeStaticlib {
        lib_name: "elephc_pdo",
        env_var: "ELEPHC_PDO_LIB_DIR",
        crate_name: "elephc-pdo",
        flag_name: "pdo",
        whole_archive: false,
        apple_frameworks: &["CoreFoundation", "SystemConfiguration"],
        needs_libdl: true,
        // The archive backs MORE THAN ONE PHP surface (PDO and mysqli), so the
        // linked staticlib alone cannot identify a PHP extension. Reporting comes
        // from the injected PHP surface(s) instead: `pipeline::compile` passes
        // `linked_php_surfaces` ("PDO" / "mysqli") to the backend seeding.
        php_extension: None,
        monitoring: MonitoringPolicy::Io {
            kind: IoKind::Database,
            wait: WaitPolicy::Measured,
            trace_context: TraceContextPolicy::NotApplicable,
        },
    },
    BridgeStaticlib {
        lib_name: "elephc_crypto",
        env_var: "ELEPHC_CRYPTO_LIB_DIR",
        crate_name: "elephc-crypto",
        flag_name: "crypto",
        whole_archive: false,
        apple_frameworks: &[],
        needs_libdl: true,
        // The crypto bridge implements PHP's digest/HMAC `hash` extension.
        php_extension: Some("hash"),
        monitoring: MonitoringPolicy::GenericTiming,
    },
    BridgeStaticlib {
        lib_name: "elephc_bcmath",
        env_var: "ELEPHC_BCMATH_LIB_DIR",
        crate_name: "elephc-bcmath",
        flag_name: "bcmath",
        whole_archive: false,
        apple_frameworks: &[],
        needs_libdl: true,
        // The decimal bridge implements PHP's procedural `bcmath` extension.
        php_extension: Some("bcmath"),
        monitoring: MonitoringPolicy::GenericTiming,
    },
    BridgeStaticlib {
        lib_name: "elephc_iconv",
        env_var: "ELEPHC_ICONV_LIB_DIR",
        crate_name: "elephc-iconv",
        flag_name: "iconv",
        whole_archive: false,
        apple_frameworks: &[],
        needs_libdl: true,
        // The charset bridge implements PHP's procedural `iconv` extension.
        php_extension: Some("iconv"),
        monitoring: MonitoringPolicy::GenericTiming,
    },
    BridgeStaticlib {
        lib_name: "elephc_phar",
        env_var: "ELEPHC_PHAR_LIB_DIR",
        crate_name: "elephc-phar",
        flag_name: "phar",
        whole_archive: false,
        apple_frameworks: &[],
        needs_libdl: true,
        // The archive reader/writer is exposed by PHP as `Phar`.
        php_extension: Some("Phar"),
        monitoring: MonitoringPolicy::GenericTiming,
    },
    BridgeStaticlib {
        lib_name: "elephc_tz",
        env_var: "ELEPHC_TZ_LIB_DIR",
        crate_name: "elephc-tz",
        flag_name: "tz",
        whole_archive: false,
        apple_frameworks: &[],
        needs_libdl: true,
        // Timezone support folds into the always-present `date` extension.
        php_extension: None,
        monitoring: MonitoringPolicy::GenericTiming,
    },
    BridgeStaticlib {
        lib_name: "elephc_image",
        env_var: "ELEPHC_IMAGE_LIB_DIR",
        crate_name: "elephc-image",
        flag_name: "image",
        whole_archive: false,
        apple_frameworks: &[],
        needs_libdl: true,
        // The image codec/drawing surface maps to PHP's `gd` extension.
        php_extension: Some("gd"),
        monitoring: MonitoringPolicy::GenericTiming,
    },
    BridgeStaticlib {
        lib_name: "elephc_probe",
        env_var: "ELEPHC_PROBE_LIB_DIR",
        crate_name: "elephc-probe",
        flag_name: "probe",
        whole_archive: false,
        apple_frameworks: &[],
        needs_libdl: true,
        // The sampling probe is an elephc-native diagnostic, not a PHP extension.
        php_extension: None,
        monitoring: MonitoringPolicy::Infrastructure {
            reason: "sampling monitor implementation",
        },
    },
    BridgeStaticlib {
        lib_name: "elephc_instr",
        env_var: "ELEPHC_INSTR_LIB_DIR",
        crate_name: "elephc-instr",
        flag_name: "instrument",
        whole_archive: false,
        apple_frameworks: &[],
        needs_libdl: true,
        // Exact per-function instrumentation is an elephc-native diagnostic.
        php_extension: None,
        monitoring: MonitoringPolicy::Infrastructure {
            reason: "exact monitor implementation",
        },
    },
    BridgeStaticlib {
        lib_name: "elephc_web",
        env_var: "ELEPHC_WEB_LIB_DIR",
        crate_name: "elephc-web",
        flag_name: "web",
        whole_archive: true,
        apple_frameworks: &[],
        needs_libdl: true,
        // The web bridge owns the PHP `session` extension surface.
        php_extension: Some("session"),
        monitoring: MonitoringPolicy::Infrastructure {
            reason: "request boundary already opens monitoring slices and trace context",
        },
    },
    BridgeStaticlib {
        lib_name: "elephc_magician",
        env_var: "ELEPHC_MAGICIAN_LIB_DIR",
        crate_name: "elephc-magician",
        flag_name: "eval",
        whole_archive: false,
        apple_frameworks: &[],
        needs_libdl: true,
        // The eval interpreter is an internal compiler facility, not an extension.
        php_extension: None,
        monitoring: MonitoringPolicy::GenericTiming,
    },
    BridgeStaticlib {
        lib_name: "elephc_curl",
        env_var: "ELEPHC_CURL_LIB_DIR",
        crate_name: "elephc-curl",
        flag_name: "curl",
        whole_archive: true,
        apple_frameworks: &[
            "Security",
            "CoreFoundation",
            "CoreServices",
            "SystemConfiguration",
        ],
        needs_libdl: true,
        // The curl bridge implements PHP's libcurl-backed `curl` extension surface.
        php_extension: Some("curl"),
        monitoring: MonitoringPolicy::Io {
            kind: IoKind::Network,
            wait: WaitPolicy::Measured,
            trace_context: TraceContextPolicy::Automatic,
        },
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

/// Maps a `--with-<flag>` suffix to the archive filename it resolves to.
///
/// Exists so a shipped compiler can name the archives it needs without anyone
/// writing that list down a second time: `--print-capabilities` reports this
/// projection of the table, and the release probe checks the tarball against
/// what the binary inside it says. A bridge added to `BRIDGES` is therefore
/// carried into the packaging check by the same edit that declares it.
pub(super) fn archive_filename_for_flag(flag: &str) -> Option<String> {
    BRIDGES
        .iter()
        .find(|bridge| bridge.flag_name == flag)
        .map(BridgeStaticlib::archive_filename)
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
    // Computed ONCE, ahead of the per-bridge loop below: whether THIS program's link plan
    // needs `elephc_curl` at all, regardless of which bridge in the plan is being resolved
    // right now. Only `elephc_magician`'s locator branch below reads it — see
    // `BridgeStaticlib::magician_curl_archive_path`'s own doc for why linking `eval()`
    // together with curl needs a build of that bridge distinct from the plain one.
    let needs_curl = plan_requires_library(plan, "elephc_curl");
    resolve_with(plan, forced_whole_archive, |bridge| {
        if bridge.lib_name == "elephc_magician" && needs_curl {
            bridge.magician_curl_archive_path()
        } else {
            bridge.archive_path()
        }
    })
}

/// Returns whether `plan` names `library` anywhere, as either an already-resolved
/// `StaticArchive` (a codegen test harness supplies these directly) or a `NamedLibrary`
/// still waiting on bridge resolution (the production compile path).
fn plan_requires_library(plan: &LinkPlan, library: &str) -> bool {
    plan.items().iter().any(|item| match item {
        LinkItem::NamedLibrary { name, .. } => name == library,
        LinkItem::StaticArchive {
            origin: LinkOrigin::Bridge { name },
            ..
        } => name == library,
        _ => false,
    })
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
    for framework in bridge.apple_frameworks {
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
    let bridge = BRIDGES.iter().find(|bridge| bridge.lib_name == name);
    if let Some(metadata) = bridge {
        assert!(
            metadata.monitoring.is_reviewed(),
            "{} reached linker resolution without a monitoring policy",
            metadata.lib_name
        );
    }
    bridge
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

    /// Locates (auto-building if needed) a curl-aware `libelephc_magician_curl.a` — a
    /// SEPARATE archive file from the plain `libelephc_magician.a` the ordinary
    /// `archive_path()` above locates, never the same slot re-purposed. This is called
    /// ONLY for the `elephc_magician` bridge, and only when this compiled program's link
    /// plan requires BOTH `elephc_magician` (it calls `eval()`) and `elephc_curl` (it —
    /// outside eval, or via `--with-curl` — uses the curl surface); see
    /// `crate::interpreter::builtins::curl`'s module doc in `elephc-magician` (mirrored
    /// here) for why a curl-aware magician build cannot be the SAME artifact a curl-free
    /// program links.
    ///
    /// A DISTINCT FILENAME, NOT a rebuild-in-place of `libelephc_magician.a`, is load-
    /// bearing: `cargo build --features curl` overwrites the very same
    /// `target/<profile>/libelephc_magician.a` a plain `cargo build` produces (Cargo does
    /// not vary a staticlib's output filename by feature set), and this `elephc`
    /// invocation is not the only one that will ever run against this checkout/install —
    /// an EARLIER program that only used `eval()` may have left a curl-free archive in
    /// place, and a LATER one will expect it back. Reusing one slot for both would make
    /// whichever program compiled most recently silently pick the OTHER program's variant:
    /// a curl-free program picking up a stale curl-aware archive would fail to link (it
    /// references `elephc_curl_*` symbols its own plan never supplies `libelephc_curl.a`
    /// for); a curl-in-eval program picking up a stale curl-free archive would fail to
    /// link for the opposite reason. A second, separately named archive makes the two
    /// coexist on disk with no risk of clobbering each other.
    ///
    /// Honors `ELEPHC_MAGICIAN_LIB_DIR` exactly like `archive_path()` does, so an
    /// installed binary (no workspace checkout to build from) can still resolve the
    /// curl-aware archive from an operator-supplied directory — and, when no override is
    /// set and no workspace can be found either, fails with a message that names the
    /// curl-aware archive specifically (`libelephc_magician_curl.a`) rather than the
    /// generic "elephc_magician missing", which would misdirect anyone troubleshooting a
    /// build where the PLAIN magician archive is present and only the curl-aware one is
    /// not.
    fn magician_curl_archive_path(&self) -> Result<PathBuf, LinkError> {
        debug_assert_eq!(self.lib_name, "elephc_magician");
        let filename = self.magician_curl_archive_filename();

        if let Ok(env_dir) = std::env::var(self.env_var) {
            if !env_dir.is_empty() {
                return self.validate_archive(PathBuf::from(env_dir).join(&filename));
            }
        }

        if let Some(archive) = self.find_named_archive(&filename) {
            // STALENESS, mirroring `refreshed_if_stale`: skip the `cargo build` spawn
            // entirely when the archive is already newer than every magician source file.
            // Without this check every `elephc` invocation that compiles an eval()+curl
            // program pays a full `cargo build -p elephc-magician --features curl`
            // subprocess, even when nothing changed since the last one.
            let Some(workspace) = self.find_workspace() else {
                return self.validate_archive(archive);
            };
            if !self.sources_are_newer_than(&workspace, &archive) {
                return self.validate_archive(archive);
            }
            if !self.claim_curl_rebuild_attempt() {
                // Another resolution already attempted the rebuild this process; reuse
                // whatever is on disk now rather than spawning a second concurrent build.
                return self.validate_archive(archive);
            }
            self.build_magician_curl_staticlib(&workspace, &filename)?;
            return self.validate_archive(self.magician_curl_missing_error_if_absent(&filename)?);
        }

        let Some(workspace) = self.find_workspace() else {
            return Err(self.magician_curl_missing_error());
        };
        self.build_magician_curl_staticlib(&workspace, &filename)?;
        self.validate_archive(self.magician_curl_missing_error_if_absent(&filename)?)
    }

    /// The curl-aware magician archive's own filename — distinct from
    /// `archive_filename()`'s plain `libelephc_magician.a`.
    fn magician_curl_archive_filename(&self) -> String {
        format!("lib{}_curl.a", self.lib_name)
    }

    /// A `LinkError` that names the curl-aware archive specifically, not just
    /// `elephc_magician` (which `missing_error()` would report, and which is misleading
    /// here: the PLAIN magician archive may be present and fine — only the curl-aware
    /// build is missing, so the message should say so and suggest the fix).
    fn magician_curl_missing_error(&self) -> LinkError {
        LinkError::MissingBridge {
            name: format!(
                "{} (curl-aware build for eval()+curl programs — set {} to a directory \
                 containing {}, or run elephc from an elephc source checkout so it can be \
                 built automatically)",
                self.lib_name,
                self.env_var,
                self.magician_curl_archive_filename()
            ),
        }
    }

    /// Re-locates `filename` after a build attempt, or returns the same accurate
    /// curl-aware-archive error `magician_curl_archive_path` uses everywhere else —
    /// never silently falling back to a stale/previous archive when the build (or the
    /// copy inside it) did not actually produce a fresh one at this path.
    fn magician_curl_missing_error_if_absent(&self, filename: &str) -> Result<PathBuf, LinkError> {
        self.find_named_archive(filename)
            .ok_or_else(|| self.magician_curl_missing_error())
    }

    /// `find_archive()`'s search, generalized to an arbitrary archive filename so the
    /// curl-aware variant can reuse the same candidate-directory list.
    fn find_named_archive(&self, filename: &str) -> Option<PathBuf> {
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
            .map(|candidate| candidate.join(filename))
            .find(|candidate| candidate.exists())
    }

    /// Registers a curl-aware-magician rebuild as attempted, separately from
    /// `claim_rebuild_attempt()`'s plain-archive claim (the two builds are independent and
    /// both may legitimately run once in the same `elephc` process).
    fn claim_curl_rebuild_attempt(&self) -> bool {
        static ATTEMPTED: OnceLock<Mutex<bool>> = OnceLock::new();
        let cell = ATTEMPTED.get_or_init(|| Mutex::new(false));
        let Ok(mut attempted) = cell.lock() else {
            return false;
        };
        if *attempted {
            return false;
        }
        *attempted = true;
        true
    }

    /// Returns the directory a curl-aware archive build should COPY its result into: the
    /// same directory `find_named_archive`'s (and `find_archive`'s) FIRST candidate
    /// checks, `current_exe()`'s own parent — the directory this running `elephc` binary
    /// itself lives in, which is where every other bridge archive it already resolved
    /// came from. Falling back to `CARGO_TARGET_DIR`/`<profile>` and then
    /// `workspace/target/<profile>` mirrors `find_named_archive`'s remaining candidates in
    /// the same priority order, so whatever this returns is always a location the SAME
    /// discovery function that will look for the result would actually find it at.
    fn magician_curl_destination_dir(&self, workspace: &Path, release: bool) -> PathBuf {
        if let Some(dir) = std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf)) {
            return dir;
        }
        if let Ok(target_dir) = std::env::var("CARGO_TARGET_DIR") {
            if !target_dir.is_empty() {
                return PathBuf::from(target_dir).join(if release { "release" } else { "debug" });
            }
        }
        workspace.join(if release { "target/release" } else { "target/debug" })
    }

    /// Builds `elephc-magician` with `--features curl` and copies the resulting
    /// `libelephc_magician.a` to `filename` (the curl-aware archive's own name) in
    /// `magician_curl_destination_dir`'s directory. Cargo's own `libelephc_magician.a`
    /// output slot inside the ISOLATED `--target-dir` below is never the shared workspace
    /// one, so this never overwrites the plain archive `archive_path()` locates.
    ///
    /// SURFACES BOTH THE BUILD AND THE COPY AS ERRORS, never silently swallowed: an
    /// earlier version of this function discarded both (`let _ = command.status()`, `let _
    /// = fs::copy(...)`), which meant a failed `cargo build` OR a failed copy (e.g. the
    /// destination directory not existing, which it never created either) left
    /// `find_named_archive` reporting a generic, misleading `MissingBridge("elephc_magician")`
    /// for what was actually a link-preparation failure with a real, discoverable cause.
    fn build_magician_curl_staticlib(&self, workspace: &Path, filename: &str) -> Result<(), LinkError> {
        let release = std::env::current_exe()
            .ok()
            .and_then(|executable| executable.parent().map(Path::to_path_buf))
            .is_some_and(|directory| directory.file_name().is_some_and(|name| name == "release"));
        // `--target-dir` ISOLATION IS LOAD-BEARING, not an optimization. Cargo names a
        // staticlib's output `lib<crate>.a` regardless of which features built it — it
        // does NOT vary the filename by feature set — so a PLAIN `cargo build -p
        // elephc-magician --features curl` run against the WORKSPACE's ordinary
        // `target/<profile>/` would overwrite the very same `libelephc_magician.a` a
        // curl-free consumer expects to keep finding there. An earlier version of this
        // function did exactly that (build into the shared target dir, then copy the
        // result aside) and left a curl-aware `libelephc_magician.a` behind as a side
        // effect: the NEXT `elephc` invocation for a curl-free eval program picked up
        // that contaminated archive, which references `elephc_curl_*` symbols its own
        // link plan never supplies — a link failure for a program that used to compile
        // cleanly. Building into a SEPARATE `target-dir` (this bridge's own, not the
        // shared one) means this command never writes to the shared
        // `target/<profile>/libelephc_magician.a` path at all, so the plain archive
        // `archive_path()` locates is never at risk from this build running at any point.
        let curl_target_dir = workspace.join("target/elephc-magician-curl-build");
        let mut command = Command::new("cargo");
        command.args([
            "build",
            "-p",
            self.crate_name,
            "--features",
            "curl",
            "--target-dir",
        ]);
        command.arg(&curl_target_dir);
        if release {
            command.arg("--release");
        }
        let status = command
            .current_dir(workspace)
            .status()
            .map_err(|_| self.magician_curl_missing_error())?;
        if !status.success() {
            return Err(self.magician_curl_missing_error());
        }
        let profile_dir = curl_target_dir.join(if release { "release" } else { "debug" });
        let built = profile_dir.join(self.archive_filename());
        let destination_dir = self.magician_curl_destination_dir(workspace, release);
        std::fs::create_dir_all(&destination_dir).map_err(|_| self.magician_curl_missing_error())?;
        std::fs::copy(&built, destination_dir.join(filename))
            .map_err(|_| self.magician_curl_missing_error())?;
        Ok(())
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

    /// Every bridge must be built and archived by CI, or its shards cannot link.
    ///
    /// A shard runs from a nextest archive with no source tree, so a bridge missing from
    /// either list fails at link time on CI while passing locally, where the compiler
    /// builds bridges on demand.
    #[test]
    fn every_bridge_is_built_and_archived_by_ci() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let workflow = std::fs::read_to_string(root.join(".github/workflows/ci.yml"))
            .expect("read ci workflow");
        let archive = std::fs::read_to_string(root.join(".config/nextest.toml"))
            .expect("read nextest config");
        for bridge in BRIDGES {
            assert!(
                workflow.contains(&format!("-p {}", bridge.crate_name)),
                "{} is missing from BRIDGE_CRATES in .github/workflows/ci.yml",
                bridge.crate_name
            );
            assert!(
                archive.contains(&format!("debug/lib{}.a", bridge.lib_name)),
                "lib{}.a is missing from the archive include list in .config/nextest.toml",
                bridge.lib_name
            );
        }
    }

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

    /// Every bridge crate must appear in the lists CI, the Docker scripts and the
    /// release and nightly workflows build.
    ///
    /// The archives are produced by explicit `cargo build -p …` lists that live
    /// outside Rust, and `cargo test` alone never emits a staticlib. So a bridge
    /// added to the table above but not to those lists compiles fine, passes
    /// review, and then fails only in CI with `required Elephc bridge X could not be
    /// found` — which is exactly how `elephc-instr` and `elephc-probe` shipped
    /// unbuildable. Deriving the expectation from the table is the point: the next
    /// bridge is covered without anyone remembering this test exists.
    ///
    /// `release.yml` was the list nobody checked, and it is the one users meet:
    /// it built eight of eleven, so every published tarball carried a compiler
    /// that refused `--with-monitoring`. `nightly.yml` ships to users too, and
    /// nobody watches an unattended 03:00 build, so it is held to the same list.
    #[test]
    fn every_bridge_crate_is_in_the_build_lists() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let lists = [
            ".github/workflows/ci.yml",
            ".github/workflows/release.yml",
            ".github/workflows/nightly.yml",
            "scripts/test-linux-arm64.sh",
            "scripts/test-linux-x86_64.sh",
        ];
        for rel in lists {
            let path = root.join(rel);
            let Ok(body) = std::fs::read_to_string(&path) else {
                panic!("cannot read {rel}; the build lists moved");
            };
            for bridge in BRIDGES {
                assert!(
                    body.contains(&format!("-p {}", bridge.crate_name)),
                    "{rel} never builds `{}`, so linking with --{} fails there",
                    bridge.crate_name,
                    bridge.flag_name
                );
            }
        }
    }

    /// Every bridge's staticlib must also be listed in the nextest archive.
    ///
    /// Building a bridge and shipping it to the machine that runs the tests are
    /// two different lists, and having the first without the second is the worse
    /// half: the archive job goes green, and the failure lands in a sharded test
    /// job as `required Elephc bridge X could not be found` — nowhere near the
    /// file that forgot it. That is exactly how `elephc_instr` reached CI:
    /// present in `BRIDGE_CRATES`, absent from the archive, so every shard that
    /// compiled a monitored program failed on a platform-shaped error message
    /// for a config-shaped mistake.
    ///
    /// Derived from `BRIDGES` rather than pinned, so a twelfth bridge cannot be
    /// half-registered.
    #[test]
    fn every_bridge_staticlib_is_in_the_nextest_archive() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let rel = ".config/nextest.toml";
        let Ok(body) = std::fs::read_to_string(root.join(rel)) else {
            panic!("cannot read {rel}; the archive list moved");
        };
        for bridge in BRIDGES {
            let entry = format!("debug/{}", bridge.archive_filename());
            assert!(
                body.contains(&entry),
                "{rel} never archives `{}`, so a sharded run compiling with --{} \
                 fails with `required Elephc bridge {} could not be found`",
                bridge.archive_filename(),
                bridge.flag_name,
                bridge.lib_name
            );
        }
    }

    /// Every bridge's staticlib must be PACKED into both shipping channels.
    ///
    /// Building it and shipping it are two lists again, and here the second one
    /// is what a user receives: the archive is resolved from the directory the
    /// compiler lives in (or its sibling `lib/`, which is the Homebrew layout),
    /// so an installed elephc finds only what was packed. Eight of eleven were —
    /// `elephc_probe`, `elephc_instr` and `elephc_magician` were in none of the
    /// three lists — so every published release answered `elephc
    /// --with-monitoring app.php` with `required Elephc bridge elephc_instr could
    /// not be found`: a whole feature that worked in every checkout and
    /// throughout CI, and in no release at all. `elephc_magician` had been
    /// missing since before v0.26.4.
    ///
    /// Nothing that runs inside this repository can notice that, because the
    /// archives are always present in `target/`. Only the shipped artifact is
    /// short, so the packing lists themselves are what have to be checked — and
    /// checked SEPARATELY: the tarball and the Homebrew formula are two lists in
    /// one file, and a name present in one reads as present to any test that
    /// searches the whole file.
    #[test]
    fn every_bridge_staticlib_ships_in_the_release_tarball() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let rel = ".github/workflows/release.yml";
        let Ok(body) = std::fs::read_to_string(root.join(rel)) else {
            panic!("cannot read {rel}; the release packaging moved");
        };
        let marker = "Update Homebrew tap";
        let split = body
            .find(marker)
            .unwrap_or_else(|| panic!("{rel} no longer has an `{marker}` step to split on"));
        let (tarball, formula) = body.split_at(split);
        for bridge in BRIDGES {
            let archive = bridge.archive_filename();
            assert!(
                tarball.contains(&archive),
                "{rel} never packs `{archive}` into the tarball, so an elephc \
                 installed from a release fails --{} with `required Elephc bridge \
                 {} could not be found`",
                bridge.flag_name,
                bridge.lib_name
            );
            assert!(
                formula.contains(&format!("lib.install \"{archive}\"")),
                "{rel} never installs `{archive}` in the Homebrew formula, so an \
                 elephc installed with brew fails --{} with `required Elephc \
                 bridge {} could not be found`",
                bridge.flag_name,
                bridge.lib_name
            );
        }
    }

    /// The workflows that publish an artifact must actually run the packaging probe.
    ///
    /// Every other packaging test here checks one list against the bridge
    /// table, and all of them pass on a tarball nobody ever unpacks: they prove
    /// the packing list NAMES each archive, not that the archive arrived beside
    /// the binary. `scripts/verify-release-artifact.sh` is what closes that —
    /// it unpacks the tarball, asks the compiler inside it for its capabilities
    /// and holds it to them — and it closes nothing if the step that runs it is
    /// dropped from the workflow that publishes.
    #[test]
    fn the_publishing_workflows_run_the_packaging_probe() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let probe = "scripts/verify-release-artifact.sh";
        assert!(
            root.join(probe).is_file(),
            "{probe} is missing; the shipped artifact is checked by nothing"
        );
        for rel in [
            ".github/workflows/release.yml",
            ".github/workflows/nightly.yml",
        ] {
            let Ok(body) = std::fs::read_to_string(root.join(rel)) else {
                panic!("cannot read {rel}; the publishing workflows moved");
            };
            assert!(
                body.contains(probe),
                "{rel} publishes an artifact without running {probe}, so a \
                 bridge missing from the tarball ships unnoticed again"
            );
            assert!(
                body.contains("~/.cache/elephc/native"),
                "{rel} runs {probe} without caching managed native packages, so \
                 a cold native add curl rebuilds openssl/libcurl from source \
                 on every nightly instead of reusing a verified cache"
            );
        }
        let script = std::fs::read_to_string(root.join(probe))
            .unwrap_or_else(|_| panic!("cannot read {probe}"));
        assert!(
            script.contains("[ \"$name\" = \"curl\" ]") && script.contains("native add curl"),
            "{probe} must run native add curl before --with-curl; curl is \
             the packed-archive capability that also needs a catalog package"
        );
        assert!(
            script.contains("adding managed native package curl before --with-curl"),
            "{probe} must native-add curl first, not after a failed compile"
        );
        assert!(
            !script.contains("missing_native_package")
                && !script.contains("requires managed native package")
                && !script.contains("after native add"),
            "{probe} must not retry from a compiler FAIL / recovery line"
        );
        assert!(
            script.contains("needs no archive from this tarball"),
            "{probe} must keep the empty-archive skip for regex/mysqli"
        );
        let after_curl = script
            .split_once("[ \"$name\" = \"curl\" ]")
            .map(|(_, rest)| rest)
            .unwrap_or("");
        assert!(
            after_curl.contains("native add curl")
                && !after_curl.contains("needs no archive from this tarball"),
            "{probe} must not skip curl the way regex is skipped"
        );
    }

    /// Verifies release artifacts also ship the curl-aware Magician variant.
    ///
    /// The ordinary bridge table names only `libelephc_magician.a`, but a program that
    /// combines eval with curl resolves Magician to the separately feature-built
    /// `libelephc_magician_curl.a`. Both the tarball and Homebrew layout must carry it.
    #[test]
    fn curl_aware_magician_staticlib_ships_in_both_release_channels() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let rel = ".github/workflows/release.yml";
        let body = std::fs::read_to_string(root.join(rel)).expect("read release workflow");
        let marker = "Update Homebrew tap";
        let split = body
            .find(marker)
            .unwrap_or_else(|| panic!("{rel} no longer has an `{marker}` step to split on"));
        let (tarball, formula) = body.split_at(split);

        assert!(
            tarball.contains("-p elephc-magician --features curl")
                && tarball.contains("libelephc_magician_curl.a"),
            "{rel} must build and pack the curl-aware Magician archive"
        );
        assert!(
            formula.contains("lib.install \"libelephc_magician_curl.a\""),
            "{rel} must install the curl-aware Magician archive with Homebrew"
        );
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

    /// Verifies adding a bridge requires an explicit, machine-audited monitoring decision.
    #[test]
    fn every_bridge_declares_a_monitoring_policy() {
        for bridge in BRIDGES {
            assert!(
                bridge.monitoring.is_reviewed(),
                "{} has no monitoring policy",
                bridge.lib_name
            );
            if let MonitoringPolicy::Infrastructure { reason } = bridge.monitoring {
                assert!(
                    !reason.trim().is_empty(),
                    "{} has an empty monitoring-policy reason",
                    bridge.lib_name
                );
            }
        }

        let pdo = bridge_for_library("elephc_pdo").expect("PDO bridge");
        assert!(matches!(
            pdo.monitoring,
            MonitoringPolicy::Io {
                kind: IoKind::Database,
                wait: WaitPolicy::Measured,
                trace_context: TraceContextPolicy::NotApplicable,
            }
        ));
        let curl = bridge_for_library("elephc_curl").expect("curl bridge");
        assert!(matches!(
            curl.monitoring,
            MonitoringPolicy::Io {
                kind: IoKind::Network,
                wait: WaitPolicy::Measured,
                trace_context: TraceContextPolicy::Automatic,
            }
        ));
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
            pdo.apple_frameworks,
            &["CoreFoundation", "SystemConfiguration"]
        );

        let magician = bridge_for_library("elephc_magician").expect("eval bridge");
        assert_eq!(magician.crate_name, "elephc-magician");
        assert_eq!(magician.env_var, "ELEPHC_MAGICIAN_LIB_DIR");
        assert_eq!(magician.archive_filename(), "libelephc_magician.a");
        assert!(!magician.whole_archive);

        // `--with-curl` force-links the whole archive: a program that names no
        // `curl_*` function/class/constant (the ordinary pay-for-use detection path,
        // `src/curl_prelude/detect.rs`) but is compiled with the flag anyway references
        // no `elephc_curl_*` symbol at all, so a selective (non-whole-archive) link
        // would pull in nothing from the archive. The Apple link also needs the same
        // framework set as the crate's gated native tests (`crates/elephc-curl/build.rs`):
        // Security/CoreFoundation/CoreServices mirror curl 8.21's upstream
        // `APPLE_SECTRUST_LDFLAGS`, while SystemConfiguration satisfies macOS's
        // `SCDynamicStoreCopyProxies` reference (and is harmless on iOS, where that
        // source path is compiled out).
        let curl = bridge_for_library("elephc_curl").expect("curl bridge");
        assert_eq!(curl.crate_name, "elephc-curl");
        assert_eq!(curl.env_var, "ELEPHC_CURL_LIB_DIR");
        assert_eq!(curl.archive_filename(), "libelephc_curl.a");
        assert!(curl.whole_archive);
        assert!(curl.needs_libdl);
        assert_eq!(
            curl.apple_frameworks,
            &[
                "Security",
                "CoreFoundation",
                "CoreServices",
                "SystemConfiguration"
            ]
        );
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
        // elephc_pdo backs two PHP surfaces (PDO, mysqli); reporting is
        // surface-based via `linked_php_surfaces`, never archive-based.
        assert_eq!(php_extension_for_lib("elephc_pdo"), None);
        assert_eq!(php_extension_for_lib("elephc_crypto"), Some("hash"));
        assert_eq!(php_extension_for_lib("elephc_bcmath"), Some("bcmath"));
        assert_eq!(php_extension_for_lib("elephc_phar"), Some("Phar"));
        assert_eq!(php_extension_for_lib("elephc_image"), Some("gd"));
        assert_eq!(php_extension_for_lib("elephc_web"), Some("session"));
        assert_eq!(php_extension_for_lib("elephc_tz"), None);
        assert_eq!(php_extension_for_lib("elephc_magician"), None);
        assert_eq!(php_extension_for_lib("elephc_curl"), Some("curl"));
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

    /// `resolve()` only chooses the curl-aware `elephc_magician` locator when the SAME
    /// plan also names `elephc_curl` — an eval-only plan (no curl anywhere in it) must
    /// keep resolving the plain `libelephc_magician.a` slot, so a curl-free program never
    /// pays any curl-aware-build cost.
    #[test]
    fn plan_requires_library_finds_named_and_resolved_curl_bridge() {
        let magician_only = LinkPlan::from_items(vec![LinkItem::NamedLibrary {
            name: "elephc_magician".to_string(),
            origin: LinkOrigin::Runtime,
        }]);
        assert!(!plan_requires_library(&magician_only, "elephc_curl"));

        let both_named = LinkPlan::from_items(vec![
            LinkItem::NamedLibrary {
                name: "elephc_magician".to_string(),
                origin: LinkOrigin::Runtime,
            },
            LinkItem::NamedLibrary {
                name: "elephc_curl".to_string(),
                origin: LinkOrigin::Runtime,
            },
        ]);
        assert!(plan_requires_library(&both_named, "elephc_curl"));

        // A test harness that hands `resolve()` an already-located `StaticArchive` (rather
        // than a `NamedLibrary` still waiting on bridge resolution) must be recognized too.
        let already_resolved = LinkPlan::from_items(vec![LinkItem::bridge_archive(
            PathBuf::from("/tmp/libelephc_curl.a"),
            "elephc_curl",
            true,
        )]);
        assert!(plan_requires_library(&already_resolved, "elephc_curl"));
    }

    /// The curl-aware magician archive is a DISTINCT filename from the plain one — this is
    /// the invariant `magician_curl_archive_path`'s doc comment depends on: the two builds
    /// must never share (and therefore never clobber) one file on disk.
    #[test]
    fn magician_curl_archive_filename_is_distinct_from_the_plain_one() {
        let magician = bridge_for_library("elephc_magician").expect("magician bridge");
        assert_eq!(magician.archive_filename(), "libelephc_magician.a");
        assert_eq!(
            magician.magician_curl_archive_filename(),
            "libelephc_magician_curl.a"
        );
        assert_ne!(
            magician.archive_filename(),
            magician.magician_curl_archive_filename()
        );
    }

    /// The exact routing decision `resolve()` makes (per-bridge, once per plan): the
    /// `elephc_magician` bridge resolves to a `_curl`-suffixed archive only when the SAME
    /// plan also names `elephc_curl`; every other bridge, and a magician-only plan, must
    /// keep resolving to the plain archive. Exercised through `resolve_with`'s injected
    /// locator (mirroring `resolve()`'s own closure shape exactly) with sentinel paths, so
    /// this needs no real bridge archives on disk — only `plan_requires_library`'s real
    /// (already separately tested) answer drives which branch each fake locator call
    /// takes.
    #[test]
    fn resolve_routes_magician_to_the_curl_aware_locator_only_when_the_plan_needs_curl() {
        /// Resolves a plan with sentinel archives while preserving the real routing decision.
        fn resolve_with_fake_locators(plan: &LinkPlan) -> BridgeResolution {
            let needs_curl = plan_requires_library(plan, "elephc_curl");
            resolve_with(plan, &[], |bridge| {
                if bridge.lib_name == "elephc_magician" && needs_curl {
                    Ok(PathBuf::from("/fake/libelephc_magician_curl.a"))
                } else {
                    Ok(PathBuf::from(format!("/fake/lib{}.a", bridge.lib_name)))
                }
            })
            .expect("fake locators never fail")
        }

        /// Extracts the resolved Magician archive path from a synthetic link plan.
        fn magician_archive_path(resolution: &BridgeResolution) -> PathBuf {
            resolution
                .plan
                .items()
                .iter()
                .find_map(|item| match item {
                    LinkItem::StaticArchive {
                        path,
                        origin: LinkOrigin::Bridge { name },
                        ..
                    } if name == "elephc_magician" => Some(path.clone()),
                    _ => None,
                })
                .expect("elephc_magician resolved to a static archive")
        }

        let magician_and_curl = LinkPlan::from_items(vec![
            LinkItem::NamedLibrary {
                name: "elephc_magician".to_string(),
                origin: LinkOrigin::Runtime,
            },
            LinkItem::NamedLibrary {
                name: "elephc_curl".to_string(),
                origin: LinkOrigin::Runtime,
            },
        ]);
        let resolution = resolve_with_fake_locators(&magician_and_curl);
        assert_eq!(
            magician_archive_path(&resolution),
            PathBuf::from("/fake/libelephc_magician_curl.a")
        );

        let magician_only = LinkPlan::from_items(vec![LinkItem::NamedLibrary {
            name: "elephc_magician".to_string(),
            origin: LinkOrigin::Runtime,
        }]);
        let resolution = resolve_with_fake_locators(&magician_only);
        assert_eq!(
            magician_archive_path(&resolution),
            PathBuf::from("/fake/libelephc_magician.a")
        );
    }
}
