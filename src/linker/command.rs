//! Purpose:
//! Renders typed link plans into deterministic macOS and Linux tool invocations.
//! Executes prepared assembler/linker commands without owning dependency discovery.
//!
//! Called from:
//! - `crate::linker` after bridge resolution and optional archive deduplication.
//!
//! Key details:
//! - Rendering is pure and unit-testable; SDK and Homebrew probes are injected as data.
//! - Whole-archive flags are scoped to exactly one archive and item order is preserved.

use std::ffi::OsString;
use std::path::Path;
use std::process::{self, Command};

use crate::codegen::platform::{Platform, Target};
use crate::codegen::Emit;
use crate::link_plan::{LinkItem, LinkOrigin, LinkPlan, LinuxLinkMode};

/// Paths for the final output and its two required input objects.
pub(super) struct LinkPaths<'a> {
    /// Final executable or shared-library path.
    pub(super) bin: &'a Path,
    /// Generated user-code object path.
    pub(super) object: &'a Path,
    /// Cached runtime object path.
    pub(super) runtime: &'a Path,
}

/// macOS SDK values resolved before pure command rendering begins.
pub(super) struct MacSdk<'a> {
    /// Absolute SDK root passed through `-syslibroot`.
    pub(super) path: &'a str,
    /// Minimum and SDK version passed through `-platform_version`.
    pub(super) version: &'a str,
}

/// A fully rendered tool program and argument vector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RenderedCommand {
    program: OsString,
    args: Vec<OsString>,
}

impl RenderedCommand {
    /// Converts this inert representation into a process command.
    fn into_command(self) -> Command {
        let mut command = Command::new(self.program);
        command.args(self.args);
        command
    }

    /// Returns arguments as lossy strings for focused renderer tests.
    #[cfg(test)]
    fn arguments_lossy(&self) -> Vec<String> {
        self.args
            .iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect()
    }
}

/// Renders one target linker command from typed inputs without spawning a process.
pub(super) fn render_link_command(
    target: Target,
    emit: Emit,
    paths: LinkPaths<'_>,
    plan: &LinkPlan,
    needs_libdl: bool,
    mac_sdk: Option<MacSdk<'_>>,
    homebrew_paths: &[&str],
) -> RenderedCommand {
    match target.platform {
        Platform::MacOS => render_macos_command(
            target,
            emit,
            paths,
            plan,
            mac_sdk.expect("macOS link rendering requires an SDK"),
            homebrew_paths,
        ),
        Platform::Linux => render_linux_command(target, emit, paths, plan, needs_libdl),
        Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
    }
}

/// Executes a rendered linker command and exits on failure.
pub(super) fn execute_link_command(rendered: RenderedCommand) {
    let mut command = rendered.into_command();
    run_tool("Linker", &mut command);
}

/// Executes a prepared external tool and exits with a concise failure diagnostic.
pub(super) fn run_tool(name: &str, command: &mut Command) {
    match command.status() {
        Ok(status) if status.success() => {}
        Ok(status) => {
            eprintln!("{name} failed with exit code {status}");
            process::exit(1);
        }
        Err(error) => {
            eprintln!("Failed to run {name}: {error}");
            process::exit(1);
        }
    }
}

/// Renders the existing direct-`ld` macOS command shape from a typed plan.
fn render_macos_command(
    target: Target,
    emit: Emit,
    paths: LinkPaths<'_>,
    plan: &LinkPlan,
    sdk: MacSdk<'_>,
    homebrew_paths: &[&str],
) -> RenderedCommand {
    let mut args = vec![OsString::from("-arch"), OsString::from(target.darwin_arch_name())];
    match emit {
        Emit::Executable => {
            args.extend([OsString::from("-e"), OsString::from("_main")]);
            args.push(OsString::from("-dead_strip"));
        }
        Emit::Cdylib => {
            let install_name = paths
                .bin
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| format!("@rpath/{name}"))
                .unwrap_or_else(|| "@rpath/libelephc_module.dylib".to_string());
            args.extend([
                OsString::from("-dylib"),
                OsString::from("-install_name"),
                OsString::from(install_name),
            ]);
        }
    }
    args.extend([
        OsString::from("-o"),
        paths.bin.as_os_str().to_owned(),
        paths.object.as_os_str().to_owned(),
        paths.runtime.as_os_str().to_owned(),
        OsString::from("-lSystem"),
        OsString::from("-syslibroot"),
        OsString::from(sdk.path),
        OsString::from("-platform_version"),
        OsString::from("macos"),
        OsString::from(sdk.version),
        OsString::from(sdk.version),
    ]);

    append_search_paths(&mut args, plan);
    if plan.needs_default_macos_library_paths() {
        for path in homebrew_paths {
            args.push(OsString::from(format!("-L{path}")));
        }
    }
    append_link_inputs(&mut args, plan, Platform::MacOS);
    append_frameworks(&mut args, plan);

    RenderedCommand {
        program: OsString::from("ld"),
        args,
    }
}

/// Renders a Linux GCC-driver link command and honors the plan's static preference.
fn render_linux_command(
    target: Target,
    emit: Emit,
    paths: LinkPaths<'_>,
    plan: &LinkPlan,
    needs_libdl: bool,
) -> RenderedCommand {
    let mut args = Vec::new();
    match emit {
        Emit::Executable => args.push(OsString::from("-Wl,--gc-sections")),
        Emit::Cdylib => args.push(OsString::from("-shared")),
    }
    args.extend([
        OsString::from("-o"),
        paths.bin.as_os_str().to_owned(),
        paths.object.as_os_str().to_owned(),
        paths.runtime.as_os_str().to_owned(),
    ]);
    if matches!(emit, Emit::Executable) && matches!(plan.linux_mode(), LinuxLinkMode::Static) {
        args.push(OsString::from("-static"));
    }
    let has_link_inputs = has_link_inputs(plan);
    if has_link_inputs {
        args.push(OsString::from("-Wl,--no-as-needed"));
    }
    args.extend([OsString::from("-lm"), OsString::from("-lpthread")]);
    if needs_libdl {
        args.push(OsString::from("-ldl"));
    }
    append_search_paths(&mut args, plan);
    if whole_bridge_count(plan) >= 2 {
        args.push(OsString::from("-Wl,--allow-multiple-definition"));
    }
    append_link_inputs(&mut args, plan, Platform::Linux);
    if has_link_inputs {
        args.push(OsString::from("-Wl,--as-needed"));
    }

    RenderedCommand {
        program: OsString::from(target.linker_cmd()),
        args,
    }
}

/// Appends every typed search path before archive and named-library inputs.
fn append_search_paths(args: &mut Vec<OsString>, plan: &LinkPlan) {
    for item in plan.items() {
        if let LinkItem::SearchPath(path) = item {
            let mut argument = OsString::from("-L");
            argument.push(path);
            args.push(argument);
        }
    }
}

/// Appends ordered archives and named libraries using target whole-archive syntax.
fn append_link_inputs(args: &mut Vec<OsString>, plan: &LinkPlan, platform: Platform) {
    for item in plan.items() {
        match item {
            LinkItem::StaticArchive {
                path,
                whole_archive,
                ..
            } => match (platform, whole_archive) {
                (Platform::MacOS, true) => {
                    args.push(OsString::from("-force_load"));
                    args.push(path.as_os_str().to_owned());
                }
                (Platform::Linux, true) => {
                    args.push(OsString::from("-Wl,--whole-archive"));
                    args.push(path.as_os_str().to_owned());
                    args.push(OsString::from("-Wl,--no-whole-archive"));
                }
                (Platform::MacOS | Platform::Linux, false) => {
                    args.push(path.as_os_str().to_owned());
                }
                (Platform::Windows, _) => {
                    panic!("Windows target is not yet supported (see issue #379)")
                }
            },
            LinkItem::NamedLibrary { name, .. } if name != "System" => {
                args.push(OsString::from(format!("-l{name}")));
            }
            LinkItem::NamedLibrary { .. }
            | LinkItem::SearchPath(_)
            | LinkItem::Framework(_) => {}
        }
    }
}

/// Appends macOS framework pairs in their typed plan order.
fn append_frameworks(args: &mut Vec<OsString>, plan: &LinkPlan) {
    for item in plan.items() {
        if let LinkItem::Framework(framework) = item {
            args.extend([OsString::from("-framework"), OsString::from(framework)]);
        }
    }
}

/// Returns whether a plan contains an archive or non-System named library.
fn has_link_inputs(plan: &LinkPlan) -> bool {
    plan.items().iter().any(|item| match item {
        LinkItem::StaticArchive { .. } => true,
        LinkItem::NamedLibrary { name, .. } => name != "System",
        LinkItem::SearchPath(_) | LinkItem::Framework(_) => false,
    })
}

/// Counts whole-archived bridge items that can duplicate Rust runtime members.
fn whole_bridge_count(plan: &LinkPlan) -> usize {
    plan.items()
        .iter()
        .filter(|item| {
            matches!(
                item,
                LinkItem::StaticArchive {
                    whole_archive: true,
                    origin: LinkOrigin::Bridge { .. },
                    ..
                }
            )
        })
        .count()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::codegen::platform::{Arch, Platform};

    use super::*;

    /// Returns fixed paths used by pure renderer tests.
    fn paths() -> LinkPaths<'static> {
        LinkPaths {
            bin: Path::new("out"),
            object: Path::new("user.o"),
            runtime: Path::new("runtime.o"),
        }
    }

    /// Renders one Linux executable command with no host probes.
    fn render_linux(plan: &LinkPlan) -> Vec<String> {
        render_link_command(
            Target::new(Platform::Linux, Arch::X86_64),
            Emit::Executable,
            paths(),
            plan,
            false,
            None,
            &[],
        )
        .arguments_lossy()
    }

    /// Renders one macOS executable command with injected SDK and Homebrew paths.
    fn render_macos(plan: &LinkPlan) -> Vec<String> {
        render_link_command(
            Target::new(Platform::MacOS, Arch::AArch64),
            Emit::Executable,
            paths(),
            plan,
            false,
            Some(MacSdk {
                path: "/SDK",
                version: "15.0",
            }),
            &["/brew/lib"],
        )
        .arguments_lossy()
    }

    /// Verifies exact managed archives keep static mode and catalog order on both Linux architectures.
    #[test]
    fn linux_exact_archives_keep_static_and_order_on_both_architectures() {
        let plan = LinkPlan::from_items(vec![
            LinkItem::managed_archive("shim.a", "pcre2"),
            LinkItem::managed_archive("posix.a", "pcre2"),
            LinkItem::managed_archive("pcre2.a", "pcre2"),
        ]);
        let commands = [
            render_linux(&plan),
            render_link_command(
                Target::new(Platform::Linux, Arch::AArch64),
                Emit::Executable,
                paths(),
                &plan,
                false,
                None,
                &[],
            )
            .arguments_lossy(),
        ];

        for args in commands {
            assert!(args.contains(&"-static".to_string()));
            let shim = args.iter().position(|argument| argument == "shim.a").unwrap();
            let posix = args.iter().position(|argument| argument == "posix.a").unwrap();
            let pcre2 = args.iter().position(|argument| argument == "pcre2.a").unwrap();
            assert!(shim < posix && posix < pcre2);
        }
    }

    /// Verifies a named user library selects the dynamic Linux rendering path.
    #[test]
    fn linux_named_library_omits_static() {
        let args = render_linux(&LinkPlan::from_items(vec![LinkItem::named_user(
            "sqlite3",
        )]));
        assert!(!args.contains(&"-static".to_string()));
        assert!(args.contains(&"-lsqlite3".to_string()));
    }

    /// Verifies Linux whole-archive markers surround only their bridge archive.
    #[test]
    fn linux_whole_archive_markers_are_bounded() {
        let plan = LinkPlan::from_items(vec![
            LinkItem::bridge_archive("tls.a", "elephc_tls", true),
            LinkItem::managed_archive("pcre2.a", "pcre2"),
        ]);
        let args = render_linux(&plan);
        let open = args
            .iter()
            .position(|argument| argument == "-Wl,--whole-archive")
            .unwrap();
        let archive = args.iter().position(|argument| argument == "tls.a").unwrap();
        let close = args
            .iter()
            .position(|argument| argument == "-Wl,--no-whole-archive")
            .unwrap();
        let managed = args
            .iter()
            .position(|argument| argument == "pcre2.a")
            .unwrap();
        assert_eq!((archive, close), (open + 1, open + 2));
        assert!(close < managed);
    }

    /// Verifies exact macOS archives do not trigger implicit Homebrew search paths.
    #[test]
    fn macos_exact_archive_does_not_add_homebrew_paths() {
        let plan = LinkPlan::from_items(vec![LinkItem::managed_archive(
            "/cache/libpcre2.a",
            "pcre2",
        )]);
        let args = render_macos(&plan);
        assert!(!args.contains(&"-L/brew/lib".to_string()));
        assert!(args.contains(&"/cache/libpcre2.a".to_string()));
    }

    /// Verifies macOS keeps legacy Homebrew search paths for named libraries only.
    #[test]
    fn macos_named_library_adds_homebrew_paths() {
        let args = render_macos(&LinkPlan::from_items(vec![LinkItem::named_extern(
            "pcre2-8",
        )]));
        assert!(args.contains(&"-L/brew/lib".to_string()));
        assert!(args.contains(&"-lpcre2-8".to_string()));
    }

    /// Verifies resolved bridge archives preserve the legacy Homebrew search-path behavior.
    #[test]
    fn macos_bridge_archive_adds_homebrew_paths() {
        let args = render_macos(&LinkPlan::from_items(vec![LinkItem::bridge_archive(
            "/cache/libelephc_pdo.a",
            "elephc_pdo",
            false,
        )]));
        assert!(args.contains(&"-L/brew/lib".to_string()));
        assert!(args.contains(&"/cache/libelephc_pdo.a".to_string()));
    }

    /// Verifies the test fixture uses ordinary path values accepted by all hosts.
    #[test]
    fn renderer_fixture_paths_are_stable() {
        assert_eq!(PathBuf::from("out"), paths().bin);
    }
}
