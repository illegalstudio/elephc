//! Purpose:
//! Owns assembler and linker process invocation for generated user and runtime objects.
//! Translates target metadata plus user link options into platform-specific tool commands.
//!
//! Called from:
//! - `crate::pipeline::compile()` after codegen writes assembly and prepares the runtime object.
//!
//! Key details:
//! - Target-specific command flags must stay aligned with `crate::codegen::platform::Target`.
//! - Non-system bridge staticlibs (TLS, PDO, PHAR, ...) are described once in `BRIDGES`;
//!   discovery, source-tree auto-build, and link flags are all driven from that table.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{self, Command};

use crate::codegen::platform::{Platform, Target};
use crate::codegen::Emit;

/// A non-system elephc bridge staticlib: a Rust `staticlib` crate linked into
/// compiled PHP programs that use a given feature (e.g. the `https://` TLS
/// wrapper or PDO). Each entry in [`BRIDGES`] fully describes how to locate and
/// link one bridge, so adding a new library is a single table entry rather than
/// another copy of the discovery/build/link logic.
struct BridgeStaticlib {
    /// Linker library name: `-l<lib_name>` resolves `lib<lib_name>.a`
    /// (e.g. `"elephc_tls"`). Also matched against `extra_link_libs`.
    lib_name: &'static str,
    /// Environment override pointing directly at the directory holding the
    /// staticlib (e.g. `"ELEPHC_TLS_LIB_DIR"`). Takes precedence over discovery.
    env_var: &'static str,
    /// Cargo package that produces the staticlib (e.g. `"elephc-tls"`), used for
    /// the source-checkout auto-build and workspace detection.
    crate_name: &'static str,
    /// User-facing short name for the `--with-<flag_name>` force flag (e.g.
    /// `"pdo"` → `--with-pdo`). Conventionally `crate_name` minus the `elephc-`
    /// prefix. `--with-<flag_name>` force-links this bridge (whole-archived so it
    /// survives dead-stripping) regardless of feature auto-detection.
    flag_name: &'static str,
    /// When true the whole archive is force-loaded so the staticlib's link-time
    /// side effects survive (e.g. rustls provider registration); when false a
    /// plain `-l` is enough.
    whole_archive: bool,
    /// Extra macOS frameworks required by the staticlib's transitive native
    /// dependencies (e.g. the PDO PostgreSQL driver pulls in `whoami`, which
    /// references CoreFoundation / SystemConfiguration).
    macos_frameworks: &'static [&'static str],
    /// Whether the staticlib needs the dynamic loader (`-ldl`) on Linux for its
    /// Rust runtime/unwinder symbols.
    needs_libdl: bool,
}

/// Every bridge staticlib elephc knows how to link. To support a new bridge,
/// add an entry here — `link()` and the discovery helpers are fully table-driven.
const BRIDGES: &[BridgeStaticlib] = &[
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
        // The PostgreSQL driver pulls in `whoami` (to default the connection
        // user), which references CoreFoundation / SystemConfiguration on macOS.
        macos_frameworks: &["CoreFoundation", "SystemConfiguration"],
        needs_libdl: true,
    },
    BridgeStaticlib {
        lib_name: "elephc_crypto",
        env_var: "ELEPHC_CRYPTO_LIB_DIR",
        crate_name: "elephc-crypto",
        flag_name: "crypto",
        // Pure-Rust hashing: no link-time side effects (unlike rustls' provider
        // registration), so a plain `-l elephc_crypto` is sufficient.
        whole_archive: false,
        // No native transitive deps.
        macos_frameworks: &[],
        // Rust runtime/unwinder symbols, like the other bridges.
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
        // Timezone-introspection tables baked from PHP and embedded with
        // include_str!: pure data lookup, no link-time side effects, so a plain
        // `-l elephc_tz` is sufficient.
        whole_archive: false,
        // Pure-std crate (the IANA tables are baked, not pulled from a tz crate),
        // so there are no native transitive deps.
        macos_frameworks: &[],
        // Rust runtime/unwinder symbols, like the other bridges.
        needs_libdl: true,
    },
    BridgeStaticlib {
        lib_name: "elephc_image",
        env_var: "ELEPHC_IMAGE_LIB_DIR",
        crate_name: "elephc-image",
        flag_name: "image",
        // Pure-Rust image codecs/drawing: no link-time side effects, so a plain
        // `-l elephc_image` suffices.
        whole_archive: false,
        // No native transitive deps (the `image` stack is pure Rust).
        macos_frameworks: &[],
        needs_libdl: true,
    },
    BridgeStaticlib {
        lib_name: "elephc_web",
        env_var: "ELEPHC_WEB_LIB_DIR",
        crate_name: "elephc-web",
        flag_name: "web",
        // The bridge owns the program entry (elephc_web_run) and Tokio/Hyper
        // link-time machinery. Windows is the sole exception: its GNU/COFF
        // archive must stay a plain link to avoid duplicate import members.
        whole_archive: true,
        macos_frameworks: &[],
        // Rust runtime/unwinder symbols, like the other bridges.
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

/// Resolves a `--with-<flag>` crate flag to its bridge `lib_name`, or `None`
/// when `flag` does not name a known bridge crate. Used by the CLI to validate
/// `--with-<crate>` and by the pipeline to force-link the matching staticlib.
pub(crate) fn bridge_lib_for_flag(flag: &str) -> Option<&'static str> {
    BRIDGES
        .iter()
        .find(|bridge| bridge.flag_name == flag)
        .map(|bridge| bridge.lib_name)
}

/// Returns every user-facing `--with-<flag>` crate flag name, in table order,
/// so the CLI can list the accepted crates in its error message.
pub(crate) fn crate_flag_names() -> Vec<&'static str> {
    BRIDGES.iter().map(|bridge| bridge.flag_name).collect()
}

/// Returns the `(lib_name, flag_name)` pair for every bridge staticlib present
/// in `extra_link_libs`, in `BRIDGES` table order. Used to report which bridge
/// libraries are actually being linked, distinct from the full `extra_link_libs`
/// list (which also holds plain system libraries like `pthread`).
pub(crate) fn bridges_in(extra_link_libs: &[String]) -> Vec<(&'static str, &'static str)> {
    BRIDGES
        .iter()
        .filter(|bridge| extra_link_libs.iter().any(|l| l.as_str() == bridge.lib_name))
        .map(|bridge| (bridge.lib_name, bridge.flag_name))
        .collect()
}

impl BridgeStaticlib {
    /// Returns whether this bridge must be force-loaded on `platform`.
    ///
    /// `elephc_web` owns the native web entry point on Linux and macOS, but its
    /// GNU/COFF archive must remain a plain link on Windows to avoid duplicate
    /// import-library members. An explicitly forced archive still wins for all
    /// other bridges.
    fn requires_whole_archive(&self, platform: Platform, forced: bool) -> bool {
        forced
            || (self.whole_archive
                && !(platform == Platform::Windows && self.lib_name == "elephc_web"))
    }

    /// Returns the `lib<name>.a` archive filename this bridge produces.
    fn archive_filename(&self) -> String {
        format!("lib{}.a", self.lib_name)
    }

    /// Locates the directory containing this bridge's staticlib.
    ///
    /// Searches explicit configuration (`env_var`), installed layouts
    /// (`bin/elephc` plus sibling `lib/` — the layout produced by the Homebrew
    /// formula), `CARGO_TARGET_DIR`, and local `target/{debug,release}`
    /// fallbacks. In a source checkout, builds the staticlib once when it is
    /// missing so `cargo run --` can compile examples without a manual
    /// `cargo build -p <crate>`. Returns `None` when it cannot be found or built.
    fn lib_dir(&self, target: Target) -> Option<String> {
        if let Ok(env_dir) = std::env::var(self.env_var) {
            if !env_dir.is_empty() {
                return Some(env_dir);
            }
        }
        if let Some(dir) = self.find_lib_dir(target) {
            return Some(dir);
        }
        let workspace = self.find_workspace()?;
        self.build_staticlib(&workspace, target);
        self.find_lib_dir(target)
    }

    /// Returns the first candidate directory that currently contains the staticlib.
    /// For a native target, order is the running binary's dir, its sibling
    /// `lib/`, `CARGO_TARGET_DIR` profiles, then in-tree profiles. For a cross
    /// target, only Cargo's target-triple subdirectories are considered: a host
    /// archive with the same filename must never shadow the PE/COFF archive.
    fn find_lib_dir(&self, target: Target) -> Option<String> {
        let archive = self.archive_filename();
        let exe = std::env::current_exe().ok()?;
        let dir = exe.parent()?;
        let native = target_is_native(target);
        let mut candidates = Vec::new();
        let mut executable_cross_candidate = None;
        if native {
            candidates.push(dir.to_path_buf());
            candidates.push(dir.parent().map(|parent| parent.join("lib")).unwrap_or_default());
        } else {
            // Derive Cargo's target root from either `target/<profile>/elephc`
            // or `target/<profile>/deps/<test-binary>`. PE compilation tests
            // execute the compiler from a temporary cwd, so cwd-relative
            // `target/...` alone cannot discover the already-built COFF bridge.
            let profile_dir = if dir.file_name().is_some_and(|name| name == "deps") {
                dir.parent()
            } else {
                Some(dir)
            };
            if let Some(profile_dir) = profile_dir {
                if let (Some(target_root), Some(profile)) =
                    (profile_dir.parent(), profile_dir.file_name())
                {
                    executable_cross_candidate = Some(
                        target_root
                            .join(cargo_target_triple(target))
                            .join(profile),
                    );
                }
            }
        }
        if let Ok(target_dir) = std::env::var("CARGO_TARGET_DIR") {
            if !target_dir.is_empty() {
                let base = PathBuf::from(target_dir);
                let base = if native {
                    base
                } else {
                    base.join(cargo_target_triple(target))
                };
                candidates.push(base.join("debug"));
                candidates.push(base.join("release"));
            }
        }
        if let Some(candidate) = executable_cross_candidate {
            candidates.push(candidate);
        }
        // Fallbacks for source-tree builds where the process cwd is the
        // workspace root or a path below it.
        let source_target = if native {
            PathBuf::from("target")
        } else {
            PathBuf::from("target").join(cargo_target_triple(target))
        };
        candidates.push(source_target.join("debug"));
        candidates.push(source_target.join("release"));

        candidates
            .into_iter()
            .find(|candidate| candidate.join(&archive).exists())
            .map(|candidate| candidate.display().to_string())
    }

    /// Finds the nearest ancestor that looks like the elephc workspace checkout
    /// providing this bridge's crate (`crates/<crate_name>/Cargo.toml`).
    fn find_workspace(&self) -> Option<PathBuf> {
        let manifest = format!("crates/{}/Cargo.toml", self.crate_name);
        let cwd = std::env::current_dir().ok()?;
        cwd.ancestors()
            .find(|dir| dir.join(&manifest).exists())
            .map(Path::to_path_buf)
    }

    /// Builds this bridge's staticlib in the current binary's debug/release
    /// profile (best-effort; failures are ignored so callers fall back to other
    /// discovery candidates).
    fn build_staticlib(&self, workspace: &Path, target: Target) {
        let release = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf))
            .is_some_and(|dir| dir.file_name().is_some_and(|name| name == "release"));
        let mut cmd = Command::new("cargo");
        cmd.args(["build", "-p", self.crate_name]);
        if !target_is_native(target) {
            cmd.args(["--target", cargo_target_triple(target)]);
        }
        if release {
            cmd.arg("--release");
        }
        let _ = cmd.current_dir(workspace).status();
    }
}

/// Returns whether `target` matches the compiler process OS, architecture, and
/// object ABI used by bridge archives.
///
/// Elephc's Windows x86_64 output always uses the GNU/MinGW ABI. A compiler
/// built with Rust's native MSVC host toolchain must therefore discover and
/// build bridges in Cargo's `x86_64-pc-windows-gnu` target directory instead
/// of treating incompatible host `.lib` archives as native output.
fn target_is_native(target: Target) -> bool {
    let platform_and_arch_match = target.platform == Platform::detect_host()
        && match target.arch {
            crate::codegen::platform::Arch::AArch64 => cfg!(target_arch = "aarch64"),
            crate::codegen::platform::Arch::X86_64 => cfg!(target_arch = "x86_64"),
        };
    let abi_matches = match (target.platform, target.arch) {
        (Platform::Windows, crate::codegen::platform::Arch::X86_64) => {
            cfg!(all(windows, target_env = "gnu"))
        }
        (Platform::Windows, crate::codegen::platform::Arch::AArch64) => {
            cfg!(all(windows, target_env = "msvc"))
        }
        _ => true,
    };
    platform_and_arch_match && abi_matches
}

/// Returns Cargo's canonical Rust target triple for a bridge staticlib build.
fn cargo_target_triple(target: Target) -> &'static str {
    use crate::codegen::platform::Arch;
    match (target.platform, target.arch) {
        (Platform::MacOS, Arch::AArch64) => "aarch64-apple-darwin",
        (Platform::MacOS, Arch::X86_64) => "x86_64-apple-darwin",
        (Platform::Linux, Arch::AArch64) => "aarch64-unknown-linux-gnu",
        (Platform::Linux, Arch::X86_64) => "x86_64-unknown-linux-gnu",
        (Platform::Windows, Arch::X86_64) => "x86_64-pc-windows-gnu",
        (Platform::Windows, Arch::AArch64) => "aarch64-pc-windows-msvc",
    }
}

/// Invokes the target assembler to produce an object file from assembly source.
/// - `target`: Compiler target (controls assembler command and flags).
/// - `asm_path`: Path to the generated `.s` assembly file.
/// - `obj_path`: Output path for the resulting `.o` object file.
/// Exits with status 1 if the assembler fails.
pub(crate) fn assemble(target: Target, asm_path: &Path, obj_path: &Path) {
    let mut as_cmd = if target.platform == Platform::Windows {
        crate::windows_toolchain::assembler_command(asm_path, obj_path)
            .unwrap_or_else(|message| fail_tool_configuration("Assembler", &message))
    } else {
        let mut command = Command::new(target.assembler_cmd());
        if target.platform == Platform::MacOS {
            command.args(["-arch", target.darwin_arch_name()]);
        }
        command.arg("-o").arg(obj_path).arg(asm_path);
        command
    };
    run_tool("Assembler", &mut as_cmd);
}

/// Bakes DWARF debug info into a standalone `.dSYM` bundle next to `bin_path`
/// via `dsymutil` on macOS (a no-op returning `true` on other platforms).
/// Returns `true` on success (or when nothing needs baking).
pub(crate) fn bake_debug_info(target: Target, bin_path: &Path) -> bool {
    if target.platform != Platform::MacOS {
        return true;
    }
    let status = Command::new("dsymutil").arg(bin_path).status();
    matches!(status, Ok(status) if status.success())
}

/// Returns the `-L` search paths derived from the `ELEPHC_MINGW_SYSROOT` env
/// var for the Windows MinGW link, when that variable is set and points at an
/// existing directory. CI sets it to a cross-built MinGW sysroot containing
/// PE/COFF static archives of PCRE2 (`libpcre2-8.a`, `libpcre2-posix.a`),
/// bzip2 (`libbz2.a`), zlib (`libz.a`), and libiconv (`libiconv.a`), so the
/// `x86_64-w64-mingw32-gcc` link resolves those C symbols. The variable is
/// unset on local non-CI builds, so this returns an empty `Vec` and the link
/// command emits no missing-directory warnings.
///
/// Both `$SYSROOT/lib` and `$SYSROOT/lib64` are added when present, so a
/// sysroot that installs either layout works without per-lib configuration.
fn mingw_sysroot_link_paths() -> Vec<String> {
    let Some(dir) = std::env::var_os("ELEPHC_MINGW_SYSROOT") else {
        return Vec::new();
    };
    mingw_sysroot_link_paths_from(&PathBuf::from(dir))
}

/// Pure core of [`mingw_sysroot_link_paths`]: returns the `-L` search paths for
/// a given sysroot base directory when it exists, or an empty `Vec` otherwise.
/// Split out so the gating logic can be unit-tested without mutating the
/// process environment (which is racy under parallel test execution).
fn mingw_sysroot_link_paths_from(base: &Path) -> Vec<String> {
    if !base.is_dir() {
        return Vec::new();
    }
    let mut paths = Vec::new();
    let lib = base.join("lib");
    if lib.is_dir() {
        paths.push(lib.to_string_lossy().into_owned());
    }
    let lib64 = base.join("lib64");
    if lib64.is_dir() {
        paths.push(lib64.to_string_lossy().into_owned());
    }
    paths
}

/// Links object files and runtime objects into a final binary.
/// - `target`: Compiler target (controls platform, linker command, and flags).
/// - `emit`: Output kind. `Executable` produces a standalone binary; `Cdylib`
///   produces a loadable shared library (`.dylib` on macOS via `ld -dylib`,
///   `.so` on Linux via `gcc -shared`) with no `_main` entry point.
/// - `bin_path`: Output path for the final artifact.
/// - `obj_path`: Path to the user code object file.
/// - `runtime_object_path`: Path to the compiler runtime object file.
/// - `extra_link_libs`: Additional libraries to link against (e.g., `["m", "pthread"]`).
/// - `extra_link_paths`: Additional `-L` search paths for libraries.
/// - `extra_frameworks`: Additional macOS frameworks to link against.
/// On macOS, `-lSystem` is always added. On Linux, `-static` is used when no extra libs
/// are provided in executable mode; cdylib mode never goes static because shared
/// libraries cannot be statically linked.
/// Bridge staticlibs named in `extra_link_libs` are located, search-pathed, and
/// linked (whole-archived when required) via the [`BRIDGES`] table.
/// Exits with status 1 if linking fails.
pub(crate) fn link(
    target: Target,
    emit: Emit,
    bin_path: &Path,
    obj_path: &Path,
    runtime_object_path: &Path,
    extra_link_libs: &[String],
    extra_link_paths: &[String],
    extra_frameworks: &[String],
    forced_whole_archive: &[String],
) {
    // Bridge staticlibs this program actually links, paired with the directory
    // each one resolved to (`None` when it could not be located/built). Driven
    // by the `BRIDGES` table so a new library needs no changes in this function.
    let needed_bridges: Vec<(&BridgeStaticlib, Option<String>)> = BRIDGES
        .iter()
        .filter(|bridge| extra_link_libs.iter().any(|l| l.as_str() == bridge.lib_name))
        .map(|bridge| (bridge, bridge.lib_dir(target)))
        .collect();
    // A bridge is force-loaded either because its `BRIDGES` entry demands it
    // (link-time side effects / owned entry point) or because the user passed
    // `--with-<crate>` (`forced_whole_archive`), which guarantees the staticlib
    // is retained even when no program symbol references it.
    let needs_libdl = needed_bridges.iter().any(|(bridge, _)| bridge.needs_libdl);

    let mut ld_cmd = match target.platform {
        Platform::MacOS => {
            let sdk_path = macos_sdk_path();
            let sdk_version = macos_sdk_version();
            let mut cmd = Command::new("ld");
            cmd.args(["-arch", target.darwin_arch_name()]);
            match emit {
                Emit::Executable => {
                    cmd.args(["-e", "_main"]);
                    // The runtime object is emitted with `.subsections_via_symbols`
                    // and `L`-prefixed (assembler-local) internal labels, so
                    // `-dead_strip` drops whole unreferenced `__rt_*` helpers (the
                    // macOS analogue of the Linux `--gc-sections` path).
                    cmd.arg("-dead_strip");
                }
                Emit::Cdylib => {
                    // `-dylib` selects shared-library output and drops the executable
                    // entry-point requirement. `-install_name @rpath/<file>` lets
                    // hosts load us under an rpath-relative name instead of the
                    // build-time absolute path baked into the LC_ID_DYLIB record.
                    let install_name = bin_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| format!("@rpath/{}", n))
                        .unwrap_or_else(|| "@rpath/libelephc_module.dylib".to_string());
                    cmd.args(["-dylib", "-install_name", &install_name]);
                }
            }
            cmd.arg("-o");
            cmd.arg(bin_path);
            cmd.arg(obj_path);
            cmd.arg(runtime_object_path);
            cmd.args(["-lSystem", "-syslibroot"]);
            cmd.arg(&sdk_path);
            cmd.args(["-platform_version", "macos", &sdk_version, &sdk_version]);
            cmd
        }
        Platform::Linux => {
            let mut cmd = Command::new(target.linker_cmd());
            match emit {
                Emit::Cdylib => {
                    // `-shared` produces a `.so`. Static linking and shared
                    // output are mutually exclusive, so we never add `-static`
                    // here even when no extra libs are requested. User-code
                    // codegen routes cross-object data references through the
                    // GOT (`@GOTPCREL` on x86_64, `:got:`/`@got_lo12:` on AArch64)
                    // in PIC mode so the loader can fix them up at
                    // dlopen time without text-segment relocations.
                    cmd.arg("-shared");
                }
                Emit::Executable => {
                    cmd.arg("-Wl,--gc-sections");
                }
            }
            cmd.arg("-o").arg(bin_path).arg(obj_path).arg(runtime_object_path);
            if matches!(emit, Emit::Executable) && extra_link_libs.is_empty() {
                cmd.arg("-static");
            }
            if !extra_link_libs.is_empty() {
                cmd.arg("-Wl,--no-as-needed");
            }
            cmd.args(["-lm", "-lpthread"]);
            if needs_libdl {
                cmd.arg("-ldl");
            }
            cmd
        }
        Platform::Windows => {
            let mut cmd = crate::windows_toolchain::linker_command()
                .unwrap_or_else(|message| fail_tool_configuration("Linker", &message));
            cmd.args(windows_pe_hardening_linker_flags());
            if matches!(emit, Emit::Cdylib) {
                cmd.arg("-shared");
                cmd.arg(format!(
                    "-Wl,--out-implib,{}",
                    windows_import_library_path(bin_path).display()
                ));
            }
            cmd.arg("-o").arg(bin_path);
            cmd.arg(obj_path);
            cmd.arg(runtime_object_path);
            // Surface a CI-provided MinGW sysroot (cross-built PCRE2, bzip2,
            // zlib, libiconv) before the system import libs and any
            // `extra_link_libs` (`-lpcre2-8`, `-lbz2`, `-lz`, `-liconv`) so the
            // MinGW linker resolves those C symbols against PE/COFF archives
            // instead of the ELF dev packages the ubuntu runner also installs.
            // Gated on `ELEPHC_MINGW_SYSROOT` so local non-CI builds — which
            // never set the env var — see no missing-directory warnings.
            for path in mingw_sysroot_link_paths() {
                cmd.arg(format!("-L{}", path));
            }
            cmd
        }
    };
    // Search paths for the located bridge staticlibs.
    for (_, dir) in &needed_bridges {
        if let Some(dir) = dir.as_deref() {
            ld_cmd.arg(format!("-L{}", dir));
        }
    }
    if target.platform == Platform::MacOS && !extra_link_libs.is_empty() {
        for path in default_macos_library_paths() {
            ld_cmd.arg(format!("-L{}", path));
        }
    }
    for path in extra_link_paths {
        ld_cmd.arg(format!("-L{}", path));
    }
    // Two or more force-loaded (whole-archive) bridges each bundle their own
    // identical copy of the Rust std/core/allocator objects, which collide at
    // link time. Resolve it generally (any number of such bridges), per platform:
    //  - Linux: have the linker keep the first definition of each duplicate.
    //  - macOS (its ld has no equivalent flag): keep the first whole-archive
    //    bridge as the symbol provider and strip the already-provided members
    //    from the rest before force-loading them.
    let whole_archive_order: Vec<(&BridgeStaticlib, &str)> = extra_link_libs
        .iter()
        .filter_map(|lib| {
            needed_bridges
                .iter()
                .find(|(b, d)| {
                    b.lib_name == lib.as_str()
                        && b.requires_whole_archive(
                            target.platform,
                            forced_whole_archive.iter().any(|l| l.as_str() == b.lib_name),
                        )
                        && d.is_some()
                })
                .map(|(b, _)| (*b, lib.as_str()))
        })
        .collect();
    let mut deduped_archive: HashMap<&str, PathBuf> = HashMap::new();
    let mut dedup_scratch: Option<PathBuf> = None;
    if whole_archive_order.len() >= 2 {
        match target.platform {
            Platform::Linux => {
                ld_cmd.arg("-Wl,--allow-multiple-definition");
            }
            Platform::MacOS => {
                let scratch =
                    std::env::temp_dir().join(format!("elephc-link-dedup-{}", process::id()));
                let mut provider_names: HashSet<String> = HashSet::new();
                let mut provider_syms: HashSet<String> = HashSet::new();
                for (i, (bridge, lib)) in whole_archive_order.iter().enumerate() {
                    let dir = needed_bridges
                        .iter()
                        .find(|(b, _)| b.lib_name == *lib)
                        .and_then(|(_, d)| d.as_deref());
                    let Some(dir) = dir else { continue };
                    let archive = Path::new(dir).join(bridge.archive_filename());
                    if i == 0 {
                        // Provider: seed the member-name and symbol sets; its
                        // archive links unchanged. `ar t` gives every member name
                        // (robust); `nm` adds the symbols it can read.
                        if let Some(names) = ar_members(&archive) {
                            provider_names.extend(names);
                        }
                        for (_, syms) in nm_member_globals(&archive) {
                            provider_syms.extend(syms);
                        }
                    } else if let Some(stripped) = dedup_macos_archive(
                        &archive,
                        &mut provider_names,
                        &mut provider_syms,
                        &scratch,
                    ) {
                        deduped_archive.insert(*lib, stripped);
                    }
                }
                dedup_scratch = Some(scratch);
            }
            Platform::Windows => {
            }
        }
    }
    // MinGW Rust staticlibs can each carry std/core/allocator support. In
    // addition, rustls-native-certs embeds duplicate import members within the
    // TLS archive itself when it is force-loaded. Keep the first identical
    // definition only for either of those established COFF cases; ordinary
    // single-bridge links retain the linker's duplicate-symbol diagnostics.
    if windows_link_needs_duplicate_bridge_tolerance(
        target.platform,
        needed_bridges.iter().filter(|(_, dir)| dir.is_some()).count(),
        whole_archive_order
            .iter()
            .any(|(bridge, _)| bridge.lib_name == "elephc_tls"),
    ) {
        ld_cmd.arg("-Wl,--allow-multiple-definition");
    }
    for lib in extra_link_libs {
        if lib == "System" {
            continue;
        }
        // A bridge that must be whole-archived (and whose staticlib we located)
        // is force-loaded so its link-time side effects survive; everything else
        // links with a plain `-l`.
        let whole_archive_bridge = needed_bridges.iter().find(|(bridge, dir)| {
            bridge.lib_name == lib.as_str()
                && bridge.requires_whole_archive(
                    target.platform,
                    forced_whole_archive.iter().any(|l| l.as_str() == bridge.lib_name),
                )
                && dir.is_some()
        });
        match whole_archive_bridge {
            Some((bridge, dir)) => {
                let dir = dir.as_deref().expect("whole-archive bridge has a located dir");
                match target.platform {
                    Platform::MacOS => {
                        // Use the deduped copy when this bridge was stripped above.
                        let path = deduped_archive
                            .get(lib.as_str())
                            .cloned()
                            .unwrap_or_else(|| Path::new(dir).join(bridge.archive_filename()));
                        ld_cmd.arg("-force_load").arg(path);
                    }
                    Platform::Linux => {
                        ld_cmd.arg("-Wl,--whole-archive");
                        ld_cmd.arg(format!("-l{}", bridge.lib_name));
                        ld_cmd.arg("-Wl,--no-whole-archive");
                    }
                    Platform::Windows => {
                        ld_cmd.arg("-Wl,--whole-archive");
                        ld_cmd.arg(format!("-l{}", bridge.lib_name));
                        ld_cmd.arg("-Wl,--no-whole-archive");
                    }
                }
            }
            None => {
                ld_cmd.arg(format!("-l{}", lib));
            }
        }
    }
    if target.platform == Platform::Windows
        && extra_link_libs
            .iter()
            .any(|library| library == "elephc_magician")
    {
        // The eval staticlib contains direct PCRE2 and libiconv references. Rust
        // staticlib metadata is not propagated to this custom final link, and
        // GNU ld scans archives from left to right, so repeat these dependencies
        // after the whole-archived magician bridge that introduces the symbols.
        ld_cmd.args(windows_magician_transitive_libraries());
    }
    if target.platform == Platform::Linux && !extra_link_libs.is_empty() {
        ld_cmd.arg("-Wl,--as-needed");
    }
    if target.platform == Platform::MacOS {
        for fw in extra_frameworks {
            ld_cmd.args(["-framework", fw]);
        }
        // Frameworks required by the linked bridge staticlibs' transitive deps.
        for (bridge, _) in &needed_bridges {
            for fw in bridge.macos_frameworks {
                ld_cmd.args(["-framework", fw]);
            }
        }
    }
    if target.platform == Platform::Windows {
        // Keep import libraries AFTER every Rust bridge archive. GNU ld scans
        // static archives from left to right: placing these before elephc-tz
        // left Rust std's Nt*/WSA*/UserEnv references unresolved even though
        // the correct MinGW import libraries were named on the command line.
        // secur32/userenv/ntdll also cover the std-windows dependencies pulled
        // in by the PDO and image bridges.
        ld_cmd.args([
            "-lkernel32",
            "-lmsvcrt",
            "-lwinmm",
            "-lws2_32",
            "-lbcrypt",
            "-lshlwapi",
            "-lshell32",
            "-lsecur32",
            "-luserenv",
            "-lntdll",
        ]);
    }
    run_tool("Linker", &mut ld_cmd);
    // The deduped archive copies were only needed for the link command above.
    if let Some(scratch) = dedup_scratch {
        let _ = std::fs::remove_dir_all(scratch);
    }
}

/// Returns whether MinGW must tolerate identical Rust runtime definitions from
/// multiple located bridge staticlibs.
fn windows_link_needs_duplicate_bridge_tolerance(
    platform: Platform,
    located_bridge_count: usize,
    force_loaded_tls: bool,
) -> bool {
    platform == Platform::Windows && (located_bridge_count >= 2 || force_loaded_tls)
}

/// Returns MinGW libraries that must follow the whole-archived eval bridge.
fn windows_magician_transitive_libraries() -> &'static [&'static str] {
    &["-lpcre2-posix", "-lpcre2-8", "-liconv"]
}

/// Returns the MinGW linker flags that opt generated PE images into supported
/// loader mitigations.
///
/// GNU ld supports ASLR, 64-bit high-entropy address selection, and DEP through
/// PE DLL-characteristic bits. It does not currently emit a Guard CF load
/// configuration for elephc's hand-written assembly, so CFG is deliberately not
/// advertised here; setting the bit without a valid guard table would be unsafe.
fn windows_pe_hardening_linker_flags() -> &'static [&'static str] {
    &[
        "-Wl,--dynamicbase",
        "-Wl,--high-entropy-va",
        "-Wl,--nxcompat",
    ]
}

/// Returns the conventional MinGW import-library path paired with a Windows DLL.
fn windows_import_library_path(dll_path: &Path) -> PathBuf {
    let stem = dll_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("elephc_module");
    dll_path.with_file_name(format!("lib{}.dll.a", stem))
}

/// Lists the member (object file) names in `archive` via `ar t`. Member-name
/// based deduplication does not parse object contents, so it is robust even when
/// `nm` cannot read newer-toolchain objects.
fn ar_members(archive: &Path) -> Option<Vec<String>> {
    let out = Command::new("ar").arg("t").arg(archive).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty() && l != "__.SYMDEF" && l != "__.SYMDEF SORTED")
            .collect(),
    )
}

/// Parses `nm -gU <archive>` (macOS) into `(member name, defined global symbols)`
/// pairs. TOLERANT of a non-zero exit: an older Xcode `nm` errors on objects from
/// a newer rustc LLVM ("Unknown attribute kind") yet still prints usable output
/// for the members it can read, so we parse stdout regardless. Member headers look
/// like `member.o:` (or `libfoo.a(member.o):`); symbol lines are `<value> <type>
/// <name>` (the name is the last whitespace token).
fn nm_member_globals(archive: &Path) -> Vec<(String, Vec<String>)> {
    let Ok(out) = Command::new("nm").args(["-gU"]).arg(archive).output() else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut members: Vec<(String, Vec<String>)> = Vec::new();
    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        // A member header is a single token ending in ':' — either `member.o:`
        // or, on some nm builds, `libfoo.a(member.o):`. A symbol line always has
        // whitespace (`<value> <type> <name>`), so the no-space test separates them.
        if line.ends_with(':') && !line.contains(char::is_whitespace) {
            let inner = &line[..line.len() - 1];
            let name = match inner.rfind('(') {
                Some(open) => inner[open + 1..].strip_suffix(')').unwrap_or(&inner[open + 1..]),
                None => inner,
            };
            members.push((name.to_string(), Vec::new()));
            continue;
        }
        if let Some(sym) = line.split_whitespace().last() {
            if let Some(last) = members.last_mut() {
                last.1.push(sym.to_string());
            }
        }
    }
    members
}

/// Copies `archive` into `scratch` and removes every member already supplied by an
/// earlier whole-archive bridge, then re-indexes the copy and returns its path. A
/// member is redundant when its name matches one in `provider_names` (catches the
/// identical std/core/etc. CGUs without parsing objects) OR all of its defined
/// global symbols are in `provider_syms` (catches differently-named generated
/// members like the allocator shim). Updates `provider_names`/`provider_syms` with
/// the members it keeps so a third bridge dedups against the union. Best-effort:
/// returns `None` (caller falls back to the original archive) on tool failure or
/// when there is nothing to strip.
fn dedup_macos_archive(
    archive: &Path,
    provider_names: &mut HashSet<String>,
    provider_syms: &mut HashSet<String>,
    scratch: &Path,
) -> Option<PathBuf> {
    let names = ar_members(archive)?;
    let per_member = nm_member_globals(archive);
    let readable: HashMap<&str, &Vec<String>> =
        per_member.iter().map(|(n, s)| (n.as_str(), s)).collect();
    let mut strip: HashSet<String> = HashSet::new();
    for name in &names {
        let name_dup = provider_names.contains(name);
        let sym_dup = readable
            .get(name.as_str())
            .map(|syms| !syms.is_empty() && syms.iter().all(|s| provider_syms.contains(s)))
            .unwrap_or(false);
        if name_dup || sym_dup {
            strip.insert(name.clone());
        }
    }
    if strip.is_empty() {
        return None;
    }
    // Members we keep extend the provider sets for any later bridge.
    for name in &names {
        if !strip.contains(name) {
            provider_names.insert(name.clone());
            if let Some(syms) = readable.get(name.as_str()) {
                for s in *syms {
                    provider_syms.insert(s.clone());
                }
            }
        }
    }
    let copy = scratch.join(archive.file_name()?);
    std::fs::create_dir_all(scratch).ok()?;
    std::fs::copy(archive, &copy).ok()?;
    // `ar d` in batches to stay clear of argument-length limits, then re-index.
    let strip: Vec<&String> = strip.iter().collect();
    for chunk in strip.chunks(256) {
        let ok = Command::new("ar")
            .arg("d")
            .arg(&copy)
            .args(chunk.iter().map(|s| s.as_str()))
            .status()
            .ok()?
            .success();
        if !ok {
            return None;
        }
    }
    if !Command::new("ranlib").arg(&copy).status().ok()?.success() {
        return None;
    }
    Some(copy)
}

/// Executes a tool command and exits the process if the command fails.
/// - `name`: Human-readable name for error messages (e.g., "Assembler", "Linker").
/// - `cmd`: Prepared `Command` to execute.
/// Prints an error message and exits with status 1 on failure.
fn run_tool(name: &str, cmd: &mut Command) {
    match cmd.status() {
        Ok(s) if s.success() => {}
        Ok(s) => {
            eprintln!("{} failed with exit code {}", name, s);
            process::exit(1);
        }
        Err(e) => {
            eprintln!("Failed to run {}: {}", name, e);
            process::exit(1);
        }
    }
}

/// Prints an actionable toolchain configuration error and terminates compilation.
fn fail_tool_configuration(name: &str, message: &str) -> ! {
    eprintln!("{name} configuration failed: {message}");
    process::exit(1);
}

/// Returns the macOS SDK path by running `xcrun --show-sdk-path`.
///
/// Exits with an actionable diagnostic when no SDK path can be resolved (xcrun missing,
/// or returning empty output because the Xcode Command Line Tools are not installed /
/// `xcode-select` points at a bad directory) instead of passing an empty `-syslibroot`
/// argument to `ld`, which fails with a cryptic `ld: -syslibroot missing <path>`.
fn macos_sdk_path() -> String {
    let resolved = Command::new("xcrun")
        .args(["--show-sdk-path"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    match validate_macos_sdk_path(&resolved) {
        Ok(path) => path,
        Err(message) => {
            eprintln!("{}", message);
            process::exit(1);
        }
    }
}

/// Validates a resolved macOS SDK path, returning the trimmed path or an actionable
/// error message when `xcrun` produced no path. Kept pure (no IO/exit) so the
/// empty-path diagnostic can be unit-tested.
fn validate_macos_sdk_path(resolved: &str) -> Result<String, String> {
    let trimmed = resolved.trim();
    if trimmed.is_empty() {
        return Err(
            "Could not locate the macOS SDK. Install the Xcode Command Line Tools \
             (run: xcode-select --install) and make sure `xcrun --show-sdk-path` prints a valid path."
                .to_string(),
        );
    }
    Ok(trimmed.to_string())
}

/// Returns common Homebrew library directories used for optional native deps on macOS.
fn default_macos_library_paths() -> Vec<&'static str> {
    ["/opt/homebrew/lib", "/usr/local/lib"]
        .into_iter()
        .filter(|path| Path::new(path).exists())
        .collect()
}

/// Returns the macOS SDK version string by running `xcrun --sdk macosx --show-sdk-version`.
/// Returns `"15.0"` as a fallback if the command fails or returns an empty version.
fn macos_sdk_version() -> String {
    match Command::new("xcrun")
        .args(["--sdk", "macosx", "--show-sdk-version"])
        .output()
    {
        Ok(output) => {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if version.is_empty() {
                "15.0".to_string()
            } else {
                version
            }
        }
        Err(_) => "15.0".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    /// Serializes tests that temporarily modify process environment variables.
    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    /// Verifies an empty or whitespace-only SDK path (xcrun missing or misconfigured)
    /// yields an actionable Xcode Command Line Tools hint instead of being silently
    /// passed to `ld` as an empty `-syslibroot` argument.
    #[test]
    fn empty_sdk_path_produces_actionable_error() {
        let err = validate_macos_sdk_path("   ").expect_err("empty path must error");
        assert!(err.contains("xcode-select --install"), "got: {err}");
    }

    /// Verifies a real SDK path is returned trimmed and otherwise unchanged.
    #[test]
    fn valid_sdk_path_is_returned_trimmed() {
        let ok = validate_macos_sdk_path("  /Library/Dev/MacOSX.sdk\n").expect("valid path");
        assert_eq!(ok, "/Library/Dev/MacOSX.sdk");
    }

    /// Verifies GNU Windows bridge output is native only for a GNU Windows
    /// compiler process, never for the MSVC host used by native Windows CI.
    #[test]
    fn windows_gnu_bridge_target_requires_a_matching_host_abi() {
        let target = Target::new(
            Platform::Windows,
            crate::codegen::platform::Arch::X86_64,
        );
        assert_eq!(
            target_is_native(target),
            cfg!(all(
                windows,
                target_arch = "x86_64",
                target_env = "gnu"
            ))
        );
    }

    /// Verifies the elephc-crypto bridge is registered and produces the expected
    /// archive filename, so compiled programs that use hashing can link it.
    #[test]
    fn bridges_includes_elephc_crypto() {
        let entry = BRIDGES
            .iter()
            .find(|b| b.lib_name == "elephc_crypto")
            .expect("elephc_crypto must be a registered bridge");
        assert_eq!(entry.crate_name, "elephc-crypto");
        assert_eq!(entry.env_var, "ELEPHC_CRYPTO_LIB_DIR");
        assert_eq!(entry.archive_filename(), "libelephc_crypto.a");
        assert!(!entry.whole_archive, "crypto bridge must not force-load (no link-time side effects)");
    }

    /// Verifies the elephc-phar bridge is registered for runtime archive reads.
    #[test]
    fn bridges_includes_elephc_phar() {
        let entry = BRIDGES
            .iter()
            .find(|b| b.lib_name == "elephc_phar")
            .expect("elephc_phar must be a registered bridge");
        assert_eq!(entry.crate_name, "elephc-phar");
        assert_eq!(entry.env_var, "ELEPHC_PHAR_LIB_DIR");
        assert_eq!(entry.archive_filename(), "libelephc_phar.a");
        assert!(!entry.whole_archive, "phar bridge must not force-load");
    }

    /// Verifies the elephc-tz bridge is registered and produces the expected
    /// archive filename, so compiled programs that use timezone introspection
    /// (getLocation/getTransitions/listAbbreviations) can link it.
    #[test]
    fn bridges_includes_elephc_tz() {
        let entry = BRIDGES
            .iter()
            .find(|b| b.lib_name == "elephc_tz")
            .expect("elephc_tz must be a registered bridge");
        assert_eq!(entry.crate_name, "elephc-tz");
        assert_eq!(entry.env_var, "ELEPHC_TZ_LIB_DIR");
        assert_eq!(entry.archive_filename(), "libelephc_tz.a");
        assert!(!entry.whole_archive, "tz bridge must not force-load (no link-time side effects)");
    }

    /// Verifies the optional eval bridge is registered for programs that use `eval()`.
    #[test]
    fn bridges_includes_elephc_magician() {
        let entry = BRIDGES
            .iter()
            .find(|b| b.lib_name == "elephc_magician")
            .expect("elephc_magician must be a registered bridge");
        assert_eq!(entry.crate_name, "elephc-magician");
        assert_eq!(entry.env_var, "ELEPHC_MAGICIAN_LIB_DIR");
        assert_eq!(entry.archive_filename(), "libelephc_magician.a");
        assert!(!entry.whole_archive, "eval bridge must not force-load");
    }

    /// Verifies every bridge exposes a non-empty `--with-<flag>` name and that
    /// `bridge_lib_for_flag` maps each one back to its `lib_name`, so the CLI's
    /// `--with-<crate>` validation stays in lockstep with the `BRIDGES` table.
    #[test]
    fn crate_flags_map_back_to_bridge_lib_names() {
        for bridge in BRIDGES {
            assert!(!bridge.flag_name.is_empty(), "{} has no flag_name", bridge.lib_name);
            assert_eq!(
                bridge_lib_for_flag(bridge.flag_name),
                Some(bridge.lib_name),
                "flag {} must resolve to {}",
                bridge.flag_name,
                bridge.lib_name
            );
        }
        assert_eq!(bridge_lib_for_flag("pdo"), Some("elephc_pdo"));
        assert_eq!(bridge_lib_for_flag("web"), Some("elephc_web"));
    }

    /// Verifies the web bridge is force-loaded on Unix targets that need its
    /// entry-point machinery.
    #[test]
    fn web_bridge_force_loads_on_linux_and_macos() {
        let bridge = BRIDGES
            .iter()
            .find(|bridge| bridge.lib_name == "elephc_web")
            .expect("web bridge entry");
        assert!(bridge.requires_whole_archive(Platform::Linux, false));
        assert!(bridge.requires_whole_archive(Platform::MacOS, false));
    }

    /// Verifies the web bridge remains a plain archive link on Windows so GNU/COFF
    /// linking does not force-load duplicate import members from its Rust staticlib.
    #[test]
    fn web_bridge_does_not_force_whole_archive_on_windows() {
        let bridge = BRIDGES
            .iter()
            .find(|bridge| bridge.lib_name == "elephc_web")
            .expect("web bridge entry");
        assert!(!bridge.requires_whole_archive(Platform::Windows, false));
    }

    /// Verifies MinGW tolerates duplicate bridge runtime members and the
    /// duplicate import members introduced by a force-loaded TLS archive.
    #[test]
    fn windows_duplicate_bridge_or_tls_import_members_enable_tolerance() {
        assert!(!windows_link_needs_duplicate_bridge_tolerance(
            Platform::Windows,
            1,
            false,
        ));
        assert!(windows_link_needs_duplicate_bridge_tolerance(
            Platform::Windows,
            2,
            false,
        ));
        assert!(windows_link_needs_duplicate_bridge_tolerance(
            Platform::Windows,
            1,
            true,
        ));
        assert!(!windows_link_needs_duplicate_bridge_tolerance(
            Platform::Linux,
            2,
            true,
        ));
    }

    /// Verifies an unknown crate flag resolves to `None` so the CLI rejects
    /// `--with-<bogus>` instead of silently ignoring it.
    #[test]
    fn unknown_crate_flag_resolves_to_none() {
        assert_eq!(bridge_lib_for_flag("bogus"), None);
        assert_eq!(bridge_lib_for_flag("elephc_pdo"), None);
        assert!(crate_flag_names().contains(&"pdo"));
        assert_eq!(crate_flag_names().len(), BRIDGES.len());
    }

    /// Verifies the eval bridge honors `ELEPHC_MAGICIAN_LIB_DIR` before filesystem discovery.
    #[test]
    fn eval_bridge_lib_dir_uses_env_override() {
        let _guard = ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("env lock should not be poisoned");
        let previous = std::env::var_os("ELEPHC_MAGICIAN_LIB_DIR");
        let override_dir = "/tmp/elephc-magician-lib-dir-override";
        std::env::set_var("ELEPHC_MAGICIAN_LIB_DIR", override_dir);
        let entry = BRIDGES
            .iter()
            .find(|b| b.lib_name == "elephc_magician")
            .expect("elephc_magician must be a registered bridge");

        let resolved = entry.lib_dir(Target::new(
            Platform::detect_host(),
            if cfg!(target_arch = "aarch64") {
                crate::codegen::platform::Arch::AArch64
            } else {
                crate::codegen::platform::Arch::X86_64
            },
        ));

        match previous {
            Some(value) => std::env::set_var("ELEPHC_MAGICIAN_LIB_DIR", value),
            None => std::env::remove_var("ELEPHC_MAGICIAN_LIB_DIR"),
        }
        assert_eq!(resolved.as_deref(), Some(override_dir));
    }

    /// Verifies Windows cross-discovery ignores a same-named host archive in
    /// `CARGO_TARGET_DIR/debug` and selects the PE/COFF archive under Cargo's
    /// target-triple directory instead.
    #[test]
    fn windows_bridge_discovery_prefers_cross_target_archive() {
        let _guard = ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("env lock should not be poisoned");
        let previous_target_dir = std::env::var_os("CARGO_TARGET_DIR");
        let previous_override = std::env::var_os("ELEPHC_TZ_LIB_DIR");
        let tmp = std::env::temp_dir().join(format!(
            "elephc-bridge-target-discovery-{}",
            std::process::id()
        ));
        let host_dir = tmp.join("debug");
        let coff_dir = tmp.join("x86_64-pc-windows-gnu").join("debug");
        std::fs::create_dir_all(&host_dir).expect("create host archive directory");
        std::fs::create_dir_all(&coff_dir).expect("create COFF archive directory");
        std::fs::write(host_dir.join("libelephc_tz.a"), b"host").expect("write host archive");
        std::fs::write(coff_dir.join("libelephc_tz.a"), b"coff").expect("write COFF archive");
        std::env::set_var("CARGO_TARGET_DIR", &tmp);
        std::env::remove_var("ELEPHC_TZ_LIB_DIR");

        let entry = BRIDGES
            .iter()
            .find(|bridge| bridge.lib_name == "elephc_tz")
            .expect("elephc_tz must be registered");
        let resolved = entry.find_lib_dir(Target::new(
            Platform::Windows,
            crate::codegen::platform::Arch::X86_64,
        ));

        match previous_target_dir {
            Some(value) => std::env::set_var("CARGO_TARGET_DIR", value),
            None => std::env::remove_var("CARGO_TARGET_DIR"),
        }
        match previous_override {
            Some(value) => std::env::set_var("ELEPHC_TZ_LIB_DIR", value),
            None => std::env::remove_var("ELEPHC_TZ_LIB_DIR"),
        }
        let _ = std::fs::remove_dir_all(&tmp);

        assert_eq!(resolved.as_deref(), Some(coff_dir.to_string_lossy().as_ref()));
    }

    /// Verifies a non-existent sysroot base produces no search paths, so a
    /// stray `ELEPHC_MINGW_SYSROOT` value can never emit a missing-directory
    /// linker warning.
    #[test]
    fn mingw_sysroot_paths_empty_for_missing_dir() {
        let paths = mingw_sysroot_link_paths_from(Path::new("/nonexistent/elephc-mingw-sysroot-123"));
        assert!(paths.is_empty(), "got: {paths:?}");
    }

    /// Verifies a real sysroot with a `lib` directory is surfaced as a `-L`
    /// path, and that `lib64` is also added when present, so a CI cross-built
    /// sysroot is picked up regardless of which layout the libs installed into.
    #[test]
    fn mingw_sysroot_paths_from_real_dir() {
        let tmp = std::env::temp_dir().join(format!("elephc-mingw-sysroot-test-{}", std::process::id()));
        std::fs::remove_dir_all(&tmp).ok();
        std::fs::create_dir_all(tmp.join("lib")).unwrap();
        std::fs::create_dir_all(tmp.join("lib64")).unwrap();
        let paths = mingw_sysroot_link_paths_from(&tmp);
        assert_eq!(paths.len(), 2);
        assert!(paths[0].ends_with("lib"), "got: {paths:?}");
        assert!(paths[1].ends_with("lib64"), "got: {paths:?}");
        std::fs::remove_dir_all(&tmp).ok();
    }

    /// Verifies only `lib` is returned when `lib64` is absent, so sysroots
    /// that install solely into `lib` do not produce a phantom `lib64` entry.
    #[test]
    fn mingw_sysroot_paths_lib_only() {
        let tmp = std::env::temp_dir().join(format!("elephc-mingw-sysroot-lib-only-{}", std::process::id()));
        std::fs::remove_dir_all(&tmp).ok();
        std::fs::create_dir_all(tmp.join("lib")).unwrap();
        let paths = mingw_sysroot_link_paths_from(&tmp);
        assert_eq!(paths.len(), 1);
        assert!(paths[0].ends_with("lib"), "got: {paths:?}");
        std::fs::remove_dir_all(&tmp).ok();
    }

    /// Verifies Windows PE links explicitly request every loader mitigation
    /// supported by the MinGW linker instead of relying on toolchain defaults.
    #[test]
    fn windows_pe_hardening_flags_enable_aslr_dep_and_high_entropy() {
        assert_eq!(
            windows_pe_hardening_linker_flags(),
            &[
                "-Wl,--dynamicbase",
                "-Wl,--high-entropy-va",
                "-Wl,--nxcompat",
            ]
        );
    }

    /// Verifies the eval bridge's native dependencies retain GNU archive scan order.
    #[test]
    fn windows_magician_dependencies_follow_the_bridge() {
        assert_eq!(
            windows_magician_transitive_libraries(),
            &["-lpcre2-posix", "-lpcre2-8", "-liconv"]
        );
    }
}
