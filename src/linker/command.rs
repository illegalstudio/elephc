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

use crate::codegen::platform::{AppleVariant, Platform, Target};
use crate::codegen::Emit;
use crate::link_plan::{LinkItem, LinkOrigin, LinkPlan, LinuxLinkMode};

/// ELF hardening options applied to every Linux output, in driver (`-Wl,`) form
/// so they reach `ld` verbatim for both the GCC and Clang drivers.
///
/// - `noexecstack`: elephc assembles its objects with `as`, which never emits a
///   `.note.GNU-stack` section, so GNU ld infers an **executable** stack
///   (`PT_GNU_STACK` `RWE`) and warns that the inference is deprecated. Nothing
///   elephc produces needs it: there is no JIT, and fiber stacks are `mmap`ed
///   `PROT_READ|PROT_WRITE` with a `PROT_NONE` guard page
///   (`codegen_support::runtime::fibers::alloc`).
/// - `relro` + `now`: resolve relocations eagerly and remap the relocated head
///   of the data segment read-only. This also covers the `-static-pie` output
///   that default-PIE drivers already produce from `-static`, where the RELRO
///   segment holds the self-relocated GOT.
///
/// None of the three can fail a link: `ld` accepts all of them for static,
/// dynamic, and `-shared` outputs, and silently ignores the ones that do not
/// apply to a given output kind.
const LINUX_HARDENING_FLAGS: [&str; 3] = ["-Wl,-z,noexecstack", "-Wl,-z,relro", "-Wl,-z,now"];

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

/// Returns the minimum-OS version recorded in `-platform_version`.
///
/// macOS keeps its long-standing behaviour of reporting the SDK version as the
/// deployment floor, so existing binaries are unaffected. iOS cannot: its SDK
/// versions run far ahead of any sensible floor, and recording one would refuse
/// to load on every device below it. `13.0` is the oldest release the arm64-only
/// backend can target anyway.
fn apple_min_os_version<'a>(target: Target, sdk_version: &'a str) -> &'a str {
    match target.apple_variant {
        AppleVariant::MacOS => sdk_version,
        AppleVariant::IOS | AppleVariant::IOSSimulator => {
            crate::codegen::platform::APPLE_IOS_MIN_OS
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
        Emit::Staticlib => unreachable!("a static library is archived with ar, never linked"),
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
                // Collectable for the same reason as the Linux shared library: every symbol
                // outside the export allowlist is `.private_extern`, so it is no longer an export
                // and no longer a root. Mach-O needed the marking more than ELF did — there every
                // `.globl` is an export by definition, so an unmarked dylib has no dead code at
                // all from the linker's point of view.
                OsString::from("-dead_strip"),
            ]);
        }
    }
    args.extend([
        OsString::from("-o"),
        paths.bin.as_os_str().to_owned(),
        paths.object.as_os_str().to_owned(),
        paths.runtime.as_os_str().to_owned(),
        // The cached runtime can reference the optional timelib bridge from a dead subsection.
        OsString::from("-U"),
        OsString::from("_elephc_tz_format"),
        OsString::from("-syslibroot"),
        OsString::from(sdk.path),
        OsString::from("-platform_version"),
        OsString::from(target.apple_platform_name()),
        OsString::from(apple_min_os_version(target, sdk.version)),
        OsString::from(sdk.version),
    ]);

    append_search_paths(&mut args, plan);
    if plan.needs_default_macos_library_paths() {
        for path in homebrew_paths {
            args.push(OsString::from(format!("-L{path}")));
        }
    }
    append_link_inputs(&mut args, plan, Platform::MacOS);
    // Keep native dependencies before the platform runtime in the final link order.
    args.push(OsString::from("-lSystem"));
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
        Emit::Staticlib => unreachable!("a static library is archived with ar, never linked"),
        Emit::Cdylib => {
            args.push(OsString::from("-shared"));
            // A shared library collects the same unreachable helpers an executable does. The
            // prerequisite is already in place and was the reason this was withheld: every symbol
            // outside the export allowlist is marked `.hidden`, so it is not a dynsym root and is
            // collectable. Without that marking `--gc-sections` would be inert here, since every
            // `.globl __rt_*` would be an export and therefore a root.
            //
            // A helper reached only through a data pointer — the runtime `.data` holds vtables of
            // `.quad __rt_*` — stays alive: that is a relocation from a retained section, which
            // the collector follows like any other reference.
            args.push(OsString::from("-Wl,--gc-sections"));
        }
    }
    args.extend(LINUX_HARDENING_FLAGS.iter().copied().map(OsString::from));
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
    if bridge_archive_count(plan) >= 2 {
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

/// Counts bridge staticlib archives that can duplicate Rust runtime members.
///
/// Every Rust `staticlib` bundles the allocator shims, std rcgu objects, and any
/// shared dependency (e.g. rustls in both `elephc_pdo` and `elephc_tls`), so as
/// soon as TWO bridge archives each contribute at least one member, GNU ld sees
/// multiple definitions — with or without `--whole-archive`. A program that
/// auto-detects several bridges (PDO's prelude alone plans pdo+tls+phar+crypto)
/// therefore needs `--allow-multiple-definition` exactly like a forced
/// whole-archive pair; the duplicates are identical objects from one workspace
/// build, so first-definition-wins is sound.
fn bridge_archive_count(plan: &LinkPlan) -> usize {
    plan.items()
        .iter()
        .filter(|item| {
            matches!(
                item,
                LinkItem::StaticArchive {
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

    use crate::codegen::platform::{AppleVariant, Arch, Platform};

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

    /// Renders one Linux shared-library command with no host probes.
    fn render_linux_cdylib(plan: &LinkPlan) -> Vec<String> {
        render_link_command(
            Target::new(Platform::Linux, Arch::X86_64),
            Emit::Cdylib,
            paths(),
            plan,
            false,
            None,
            &[],
        )
        .arguments_lossy()
    }

    /// Verifies a Linux shared library is section-collected like an executable.
    ///
    /// The flag is only meaningful because every symbol outside the export allowlist is already
    /// marked `.hidden`: without that, each `.globl __rt_*` would be an export, therefore a
    /// dynsym root, and the collector would have nothing to drop. A regression removing either
    /// half leaves a shared library carrying the whole runtime while still linking and passing
    /// its behaviour tests, which is why the flag is asserted rather than left to review.
    #[test]
    fn linux_shared_library_collects_unreachable_sections() {
        let args = render_linux_cdylib(&LinkPlan::new());
        assert!(args.iter().any(|arg| arg == "-shared"));
        assert!(
            args.iter().any(|arg| arg == "-Wl,--gc-sections"),
            "a shared library must collect unreachable sections: {args:?}"
        );
    }

    /// Verifies the executable path did not lose its collection flag while the cdylib gained one.
    #[test]
    fn linux_executable_still_collects_unreachable_sections() {
        let args = render_linux(&LinkPlan::new());
        assert!(args.iter().any(|arg| arg == "-Wl,--gc-sections"));
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

    /// Verifies iOS device and simulator links carry distinct platform tokens
    /// while sharing the compiler's fixed deployment floor.
    #[test]
    fn ios_link_commands_record_variant_and_deployment_floor() {
        for (variant, platform_name) in [
            (AppleVariant::IOS, "ios"),
            (AppleVariant::IOSSimulator, "ios-simulator"),
        ] {
            let args = render_link_command(
                Target::new_apple(Arch::AArch64, variant),
                Emit::Executable,
                paths(),
                &LinkPlan::new(),
                false,
                Some(MacSdk {
                    path: "/SDK",
                    version: "18.2",
                }),
                &[],
            )
            .arguments_lossy();
            let flag = args
                .iter()
                .position(|argument| argument == "-platform_version")
                .expect("Apple link must carry -platform_version");
            assert_eq!(args[flag + 1], platform_name);
            assert_eq!(args[flag + 2], crate::codegen::platform::APPLE_IOS_MIN_OS);
            assert_eq!(args[flag + 3], "18.2");
        }
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

    /// Verifies two bridge archives enable `--allow-multiple-definition` even
    /// without whole-archiving: every Rust staticlib duplicates the allocator
    /// shims and shared dependency rcgu objects, so an auto-detected
    /// multi-bridge link (e.g. PDO's pdo+tls+phar+crypto plan) collides on GNU
    /// ld exactly like a forced whole-archive pair.
    #[test]
    fn linux_two_bridge_archives_allow_multiple_definition() {
        let plan = LinkPlan::from_items(vec![
            LinkItem::bridge_archive("pdo.a", "elephc_pdo", false),
            LinkItem::bridge_archive("tls.a", "elephc_tls", false),
        ]);
        let args = render_linux(&plan);
        assert!(args.iter().any(|argument| argument == "-Wl,--allow-multiple-definition"));
    }

    /// Verifies a single bridge archive does not relax linker duplicate checks.
    #[test]
    fn linux_single_bridge_archive_keeps_strict_definitions() {
        let plan = LinkPlan::from_items(vec![
            LinkItem::bridge_archive("pdo.a", "elephc_pdo", false),
            LinkItem::managed_archive("pcre2.a", "pcre2"),
        ]);
        let args = render_linux(&plan);
        assert!(!args.iter().any(|argument| argument == "-Wl,--allow-multiple-definition"));
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

    /// Verifies every Linux output carries the ELF hardening options, on both
    /// supported architectures, in static and dynamic mode, and for shared
    /// libraries. Without `-z noexecstack` the assembler objects (which have no
    /// `.note.GNU-stack`) make GNU ld mark the stack `RWE`.
    #[test]
    fn linux_outputs_carry_elf_hardening_flags() {
        let static_plan = LinkPlan::from_items(vec![LinkItem::managed_archive("pcre2.a", "pcre2")]);
        let dynamic_plan = LinkPlan::from_items(vec![LinkItem::named_user("sqlite3")]);
        let aarch64 = render_link_command(
            Target::new(Platform::Linux, Arch::AArch64),
            Emit::Executable,
            paths(),
            &static_plan,
            false,
            None,
            &[],
        )
        .arguments_lossy();

        let commands = [
            render_linux(&static_plan),
            render_linux(&dynamic_plan),
            render_linux_cdylib(&dynamic_plan),
            aarch64,
        ];
        for args in commands {
            for flag in LINUX_HARDENING_FLAGS {
                assert!(
                    args.contains(&flag.to_string()),
                    "missing {flag} in Linux link command: {args:?}"
                );
            }
        }
    }

    /// Verifies macOS link commands stay free of the ELF-only hardening options:
    /// `ld64` rejects `-z` entirely, and macOS binaries are already PIE with a
    /// platform-enforced non-executable stack.
    #[test]
    fn macos_command_omits_elf_hardening_flags() {
        let args = render_macos(&LinkPlan::from_items(vec![LinkItem::named_extern("pcre2-8")]));
        for flag in LINUX_HARDENING_FLAGS {
            assert!(
                !args.contains(&flag.to_string()),
                "macOS link command must not carry {flag}: {args:?}"
            );
        }
        assert!(
            !args.iter().any(|argument| argument.contains("-z")),
            "macOS link command must not carry any -z option: {args:?}"
        );
    }

    /// Verifies the test fixture uses ordinary path values accepted by all hosts.
    #[test]
    fn renderer_fixture_paths_are_stable() {
        assert_eq!(PathBuf::from("out"), paths().bin);
    }
}
