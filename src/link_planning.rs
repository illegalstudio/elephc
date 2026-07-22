//! Purpose:
//! Builds ordered typed final-link plans from compiler, runtime, user, and managed inputs.
//! Keeps provenance classification and archive ordering outside the compile orchestrator.
//!
//! Called from:
//! - `crate::pipeline::compile()` immediately before final assembly/linking.
//!
//! Key details:
//! - Managed archives retain catalog order and never become named-library fallbacks.
//! - Named inputs are deduplicated at first occurrence without losing their origin.

use std::collections::HashSet;
use std::path::PathBuf;

use crate::codegen::LinkRequirement;
use crate::link_plan::{LinkItem, LinkOrigin, LinkPlan};
use crate::linker;
use crate::native_deps::ResolvedNativePackage;

/// Inputs collected by the compile pipeline for one final native link.
pub(crate) struct LinkPlanningInputs<'a> {
    /// Libraries supplied explicitly by `--link`/`-l`.
    pub(crate) user_libraries: &'a [String],
    /// Search paths supplied explicitly by `--link-path`/`-L`.
    pub(crate) user_search_paths: &'a [String],
    /// Frameworks supplied explicitly by `--framework`.
    pub(crate) user_frameworks: &'a [String],
    /// Libraries discovered by extern declarations or builtin checking.
    pub(crate) checker_libraries: &'a [String],
    /// Typed requirements emitted by optional runtime feature detection.
    pub(crate) runtime_requirements: &'a [LinkRequirement],
    /// Verified exact managed artifacts returned by read-only project resolution.
    pub(crate) managed_packages: &'a [ResolvedNativePackage],
    /// Bridge names whose full archive must survive dead stripping.
    pub(crate) forced_bridges: &'a [String],
    /// Whether the full web bridge owns the program entrypoint.
    pub(crate) web: bool,
}

/// Builds the exact ordered final-link plan from all typed compiler inputs.
pub(crate) fn build(inputs: LinkPlanningInputs<'_>) -> LinkPlan {
    let mut plan = LinkPlan::new();
    let mut named = HashSet::new();

    for library in inputs.user_libraries {
        push_named_once(&mut plan, &mut named, library, LinkOrigin::User);
    }
    for path in inputs.user_search_paths {
        plan.push(LinkItem::SearchPath(PathBuf::from(path)));
    }
    for framework in inputs.user_frameworks {
        plan.push(LinkItem::Framework(framework.clone()));
    }
    if inputs.web {
        push_named_once(
            &mut plan,
            &mut named,
            "elephc_web",
            LinkOrigin::Bridge {
                name: "elephc_web".to_string(),
            },
        );
    }
    for bridge in inputs.forced_bridges {
        push_named_once(
            &mut plan,
            &mut named,
            bridge,
            LinkOrigin::Bridge {
                name: bridge.clone(),
            },
        );
    }
    for library in inputs.checker_libraries {
        let origin = if is_known_bridge(library) {
            LinkOrigin::Bridge {
                name: library.clone(),
            }
        } else {
            LinkOrigin::Extern
        };
        push_named_once(&mut plan, &mut named, library, origin);
    }
    for requirement in inputs.runtime_requirements {
        match requirement {
            LinkRequirement::NativePackage(_) => {}
            LinkRequirement::Bridge(bridge) => push_named_once(
                &mut plan,
                &mut named,
                bridge,
                LinkOrigin::Bridge {
                    name: (*bridge).to_string(),
                },
            ),
            LinkRequirement::SystemLibrary(library) => {
                push_named_once(&mut plan, &mut named, library, LinkOrigin::Runtime)
            }
        }
    }
    for package in inputs.managed_packages {
        for archive in &package.archives {
            plan.push(LinkItem::managed_archive(archive, &package.package));
        }
        for library in &package.system_libraries {
            push_named_once(&mut plan, &mut named, library, LinkOrigin::Runtime);
        }
        for framework in &package.frameworks {
            plan.push(LinkItem::Framework(framework.clone()));
        }
    }
    plan
}

/// Appends one named input at its first semantic occurrence while retaining provenance.
fn push_named_once(
    plan: &mut LinkPlan,
    seen: &mut HashSet<String>,
    library: &str,
    origin: LinkOrigin,
) {
    if seen.insert(library.to_string()) {
        plan.push(LinkItem::NamedLibrary {
            name: library.to_string(),
            origin,
        });
    }
}

/// Returns whether a library name belongs to the authoritative Elephc bridge table.
fn is_known_bridge(library: &str) -> bool {
    linker::crate_flag_names()
        .into_iter()
        .filter_map(linker::bridge_lib_for_flag)
        .any(|known| known == library)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::link_plan::LinuxLinkMode;

    /// Returns empty inputs with only supplied managed/runtime requirements.
    fn managed_inputs<'a>(
        runtime_requirements: &'a [LinkRequirement],
        managed_packages: &'a [ResolvedNativePackage],
    ) -> LinkPlanningInputs<'a> {
        LinkPlanningInputs {
            user_libraries: &[],
            user_search_paths: &[],
            user_frameworks: &[],
            checker_libraries: &[],
            runtime_requirements,
            managed_packages,
            forced_bridges: &[],
            web: false,
        }
    }

    /// Verifies managed PCRE2 archives retain exact shim/POSIX/8-bit order and static mode.
    #[test]
    fn managed_pcre2_plan_preserves_catalog_archive_order() {
        let package = ResolvedNativePackage {
            package: "pcre2".to_string(),
            artifact_root: PathBuf::from("/cache/pcre2"),
            archives: vec![
                PathBuf::from("/cache/pcre2/lib/libelephc_pcre2_shim.a"),
                PathBuf::from("/cache/pcre2/lib/libpcre2-posix.a"),
                PathBuf::from("/cache/pcre2/lib/libpcre2-8.a"),
            ],
            system_libraries: Vec::new(),
            frameworks: Vec::new(),
        };
        let requirements = [LinkRequirement::NativePackage("pcre2")];
        let packages = [package];
        let plan = build(managed_inputs(&requirements, &packages));

        let archives: Vec<&Path> = plan
            .items()
            .iter()
            .filter_map(|item| match item {
                LinkItem::StaticArchive { path, origin, .. }
                    if matches!(origin, LinkOrigin::ManagedNative { package } if package == "pcre2") =>
                {
                    Some(path.as_path())
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            archives,
            vec![
                Path::new("/cache/pcre2/lib/libelephc_pcre2_shim.a"),
                Path::new("/cache/pcre2/lib/libpcre2-posix.a"),
                Path::new("/cache/pcre2/lib/libpcre2-8.a"),
            ]
        );
        assert_eq!(plan.linux_mode(), &LinuxLinkMode::Static);
    }

    /// Verifies user, extern, bridge, and runtime named inputs keep distinct provenance.
    #[test]
    fn link_plan_classifies_non_managed_origins() {
        let user_libraries = ["sqlite3".to_string()];
        let checker_libraries = ["curl".to_string(), "elephc_crypto".to_string()];
        let runtime_requirements = [
            LinkRequirement::Bridge("elephc_phar"),
            LinkRequirement::SystemLibrary("z".to_string()),
        ];
        let plan = build(LinkPlanningInputs {
            user_libraries: &user_libraries,
            user_search_paths: &[],
            user_frameworks: &[],
            checker_libraries: &checker_libraries,
            runtime_requirements: &runtime_requirements,
            managed_packages: &[],
            forced_bridges: &[],
            web: false,
        });
        let origins: Vec<&LinkOrigin> = plan
            .items()
            .iter()
            .filter_map(|item| match item {
                LinkItem::NamedLibrary { origin, .. } => Some(origin),
                _ => None,
            })
            .collect();

        assert!(matches!(origins[0], LinkOrigin::User));
        assert!(matches!(origins[1], LinkOrigin::Extern));
        assert!(matches!(origins[2], LinkOrigin::Bridge { name } if name == "elephc_crypto"));
        assert!(matches!(origins[3], LinkOrigin::Bridge { name } if name == "elephc_phar"));
        assert!(matches!(origins[4], LinkOrigin::Runtime));
        assert!(matches!(plan.linux_mode(), LinuxLinkMode::Dynamic { .. }));
    }
}
