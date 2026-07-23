//! Purpose:
//! Defines the ordered, typed inputs consumed by the final native linker.
//! Separates exact archives from named-library lookup and records their provenance.
//!
//! Called from:
//! - `crate::linker` when adapting compile options and rendering a linker command.
//! - Native dependency resolution when managed artifacts become exact archive paths.
//!
//! Key details:
//! - Exact managed archives remain compatible with Elephc's preferred static Linux link.
//! - Named libraries and bridge archives conservatively select dynamic Linux linking.
//! - Item order is preserved because static archive order affects symbol resolution.

use std::path::PathBuf;

/// Identifies which compiler surface contributed a linker input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkOrigin {
    /// An exact artifact produced by Elephc's managed native dependency workflow.
    ManagedNative {
        /// Catalog package whose receipt owns the exact archive.
        package: String,
    },
    /// An Elephc Rust bridge static library.
    Bridge {
        /// Authoritative bridge linker name from Elephc's bridge table.
        name: String,
    },
    /// A library supplied explicitly by the user-facing compile CLI.
    User,
    /// A library required by an `extern` declaration in the PHP program.
    Extern,
    /// A library selected by compiler/runtime feature detection.
    Runtime,
}

/// One ordered linker input with enough type information to render it safely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkItem {
    /// A static archive selected by exact filesystem path.
    StaticArchive {
        /// Exact path passed to the platform linker without `-l` lookup.
        path: PathBuf,
        /// Whether this archive must be retained in full around bounded linker markers.
        whole_archive: bool,
        /// Compiler surface that selected this archive.
        origin: LinkOrigin,
    },
    /// A library resolved by the platform linker's `-l<name>` search.
    NamedLibrary {
        /// Name passed to the platform linker after `-l`.
        name: String,
        /// Compiler surface that requested this named lookup.
        origin: LinkOrigin,
    },
    /// A directory added to the platform library search path.
    SearchPath(PathBuf),
    /// A macOS framework name.
    Framework(String),
}

impl LinkItem {
    /// Creates an exact managed archive that remains static-link compatible on Linux.
    pub fn managed_archive(path: impl Into<PathBuf>, package: impl Into<String>) -> Self {
        Self::StaticArchive {
            path: path.into(),
            whole_archive: false,
            origin: LinkOrigin::ManagedNative {
                package: package.into(),
            },
        }
    }

    /// Creates an exact bridge archive with optional whole-archive retention.
    pub fn bridge_archive(
        path: impl Into<PathBuf>,
        name: impl Into<String>,
        whole_archive: bool,
    ) -> Self {
        Self::StaticArchive {
            path: path.into(),
            whole_archive,
            origin: LinkOrigin::Bridge { name: name.into() },
        }
    }

    /// Creates a named library supplied by the compile CLI.
    pub fn named_user(name: impl Into<String>) -> Self {
        Self::NamedLibrary {
            name: name.into(),
            origin: LinkOrigin::User,
        }
    }

    /// Creates a named library required by an `extern` declaration.
    pub fn named_extern(name: impl Into<String>) -> Self {
        Self::NamedLibrary {
            name: name.into(),
            origin: LinkOrigin::Extern,
        }
    }

    /// Creates a named library selected by compiler/runtime feature detection.
    pub fn named_runtime(name: impl Into<String>) -> Self {
        Self::NamedLibrary {
            name: name.into(),
            origin: LinkOrigin::Runtime,
        }
    }

    /// Returns a stable diagnostic reason when this item requires dynamic Linux linking.
    fn dynamic_reason(&self) -> Option<String> {
        match self {
            Self::NamedLibrary { name, origin } => {
                Some(format!("named {} library `{name}`", origin.description()))
            }
            Self::StaticArchive {
                origin: LinkOrigin::Bridge { name },
                ..
            } => Some(format!("Elephc bridge `{name}`")),
            Self::StaticArchive { .. } | Self::SearchPath(_) | Self::Framework(_) => None,
        }
    }
}

impl LinkOrigin {
    /// Returns the concise provenance label used in Linux-mode diagnostics.
    fn description(&self) -> String {
        match self {
            Self::ManagedNative { package } => format!("managed native `{package}`"),
            Self::Bridge { name } => format!("bridge `{name}`"),
            Self::User => "user".to_string(),
            Self::Extern => "extern".to_string(),
            Self::Runtime => "runtime".to_string(),
        }
    }
}

/// Records whether an executable link may use the Linux static-link preference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinuxLinkMode {
    /// Every planned input is compatible with a fully static executable link.
    Static,
    /// At least one planned input requires the existing dynamic-link path.
    Dynamic {
        /// Stable provenance messages explaining which inputs disabled static mode.
        reasons: Vec<String>,
    },
}

/// Ordered linker inputs plus the Linux mode derived from their typed provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkPlan {
    ordered: Vec<LinkItem>,
    linux_mode: LinuxLinkMode,
}

impl LinkPlan {
    /// Creates an empty plan that is statically linkable on Linux.
    pub fn new() -> Self {
        Self {
            ordered: Vec::new(),
            linux_mode: LinuxLinkMode::Static,
        }
    }

    /// Creates a plan from ordered items and derives its Linux link mode.
    pub fn from_items(ordered: Vec<LinkItem>) -> Self {
        let linux_mode = Self::derive_linux_mode(&ordered);
        Self {
            ordered,
            linux_mode,
        }
    }

    /// Appends one item without disturbing the order of existing static archives.
    pub fn push(&mut self, item: LinkItem) {
        self.ordered.push(item);
        self.linux_mode = Self::derive_linux_mode(&self.ordered);
    }

    /// Prepends infrastructure items while retaining their supplied order.
    pub fn prepend(&mut self, mut items: Vec<LinkItem>) {
        items.append(&mut self.ordered);
        self.ordered = items;
        self.linux_mode = Self::derive_linux_mode(&self.ordered);
    }

    /// Returns the typed linker items in their semantic order.
    pub fn items(&self) -> &[LinkItem] {
        &self.ordered
    }

    /// Returns the Linux link mode derived from every current item.
    pub fn linux_mode(&self) -> &LinuxLinkMode {
        &self.linux_mode
    }

    /// Returns whether the plan contains at least one named-library lookup.
    pub fn has_named_libraries(&self) -> bool {
        self.ordered
            .iter()
            .any(|item| matches!(item, LinkItem::NamedLibrary { .. }))
    }

    /// Returns whether legacy macOS library search paths remain relevant to this plan.
    pub fn needs_default_macos_library_paths(&self) -> bool {
        self.ordered.iter().any(|item| {
            matches!(
                item,
                LinkItem::NamedLibrary { .. }
                    | LinkItem::StaticArchive {
                        origin: LinkOrigin::Bridge { .. },
                        ..
                    }
            )
        })
    }

    /// Recomputes Linux mode and diagnostic provenance from ordered typed items.
    fn derive_linux_mode(items: &[LinkItem]) -> LinuxLinkMode {
        let reasons: Vec<String> = items
            .iter()
            .filter_map(LinkItem::dynamic_reason)
            .collect();
        if reasons.is_empty() {
            LinuxLinkMode::Static
        } else {
            LinuxLinkMode::Dynamic { reasons }
        }
    }
}

impl Default for LinkPlan {
    /// Creates the default empty, statically linkable plan.
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies exact managed archives preserve caller order and static Linux mode.
    #[test]
    fn managed_archive_order_remains_static() {
        let plan = LinkPlan::from_items(vec![
            LinkItem::managed_archive("shim.a", "pcre2"),
            LinkItem::managed_archive("posix.a", "pcre2"),
            LinkItem::managed_archive("pcre2.a", "pcre2"),
        ]);

        assert_eq!(plan.linux_mode(), &LinuxLinkMode::Static);
        let paths: Vec<&str> = plan
            .items()
            .iter()
            .filter_map(|item| match item {
                LinkItem::StaticArchive { path, .. } => path.to_str(),
                _ => None,
            })
            .collect();
        assert_eq!(paths, vec!["shim.a", "posix.a", "pcre2.a"]);
    }

    /// Verifies every named-library origin selects dynamic Linux mode with provenance.
    #[test]
    fn named_libraries_select_dynamic_mode() {
        let plan = LinkPlan::from_items(vec![
            LinkItem::named_user("sqlite3"),
            LinkItem::named_extern("curl"),
            LinkItem::named_runtime("z"),
        ]);

        let LinuxLinkMode::Dynamic { reasons } = plan.linux_mode() else {
            panic!("named libraries must select dynamic mode");
        };
        assert_eq!(reasons.len(), 3);
        assert!(reasons[0].contains("user"));
        assert!(reasons[1].contains("extern"));
        assert!(reasons[2].contains("runtime"));
    }

    /// Verifies bridge archives remain dynamically conservative while paths and frameworks do not.
    #[test]
    fn bridge_archive_is_dynamic_but_metadata_items_are_neutral() {
        let neutral = LinkPlan::from_items(vec![
            LinkItem::SearchPath(PathBuf::from("/native/lib")),
            LinkItem::Framework("Security".to_string()),
        ]);
        assert_eq!(neutral.linux_mode(), &LinuxLinkMode::Static);

        let bridge = LinkPlan::from_items(vec![LinkItem::bridge_archive(
            "/native/lib/libelephc_tls.a",
            "elephc_tls",
            true,
        )]);
        assert!(matches!(
            bridge.linux_mode(),
            LinuxLinkMode::Dynamic { reasons } if reasons == &vec!["Elephc bridge `elephc_tls`".to_string()]
        ));
        assert!(bridge.needs_default_macos_library_paths());
        assert!(!LinkPlan::from_items(vec![LinkItem::managed_archive(
            "/native/lib/libpcre2.a",
            "pcre2",
        )])
        .needs_default_macos_library_paths());
    }
}
