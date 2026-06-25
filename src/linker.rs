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
        whole_archive: true,
        macos_frameworks: &[],
        needs_libdl: true,
    },
    BridgeStaticlib {
        lib_name: "elephc_pdo",
        env_var: "ELEPHC_PDO_LIB_DIR",
        crate_name: "elephc-pdo",
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
        whole_archive: false,
        macos_frameworks: &[],
        needs_libdl: true,
    },
    BridgeStaticlib {
        lib_name: "elephc_tz",
        env_var: "ELEPHC_TZ_LIB_DIR",
        crate_name: "elephc-tz",
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
        // The bridge owns the program entry (elephc_web_run) and tokio/hyper
        // link-time machinery, so the whole archive is force-loaded.
        whole_archive: true,
        macos_frameworks: &[],
        // Rust runtime/unwinder symbols, like the other bridges.
        needs_libdl: true,
    },
];

impl BridgeStaticlib {
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
    fn lib_dir(&self) -> Option<String> {
        if let Ok(env_dir) = std::env::var(self.env_var) {
            if !env_dir.is_empty() {
                return Some(env_dir);
            }
        }
        if let Some(dir) = self.find_lib_dir() {
            return Some(dir);
        }
        let workspace = self.find_workspace()?;
        self.build_staticlib(&workspace);
        self.find_lib_dir()
    }

    /// Returns the first candidate directory that currently contains the staticlib.
    /// Order: the running binary's dir, its sibling `lib/`, `CARGO_TARGET_DIR`
    /// profiles, then in-tree `target/{debug,release}`.
    fn find_lib_dir(&self) -> Option<String> {
        let archive = self.archive_filename();
        let exe = std::env::current_exe().ok()?;
        let dir = exe.parent()?;
        let mut candidates = vec![
            dir.to_path_buf(),
            dir.parent().map(|parent| parent.join("lib")).unwrap_or_default(),
        ];
        if let Ok(target_dir) = std::env::var("CARGO_TARGET_DIR") {
            if !target_dir.is_empty() {
                candidates.push(PathBuf::from(&target_dir).join("debug"));
                candidates.push(PathBuf::from(target_dir).join("release"));
            }
        }
        // Fallbacks for source-tree builds where the process cwd is the
        // workspace root or a path below it.
        candidates.push(PathBuf::from("target/debug"));
        candidates.push(PathBuf::from("target/release"));

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
    fn build_staticlib(&self, workspace: &Path) {
        let release = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf))
            .is_some_and(|dir| dir.file_name().is_some_and(|name| name == "release"));
        let mut cmd = Command::new("cargo");
        cmd.args(["build", "-p", self.crate_name]);
        if release {
            cmd.arg("--release");
        }
        let _ = cmd.current_dir(workspace).status();
    }
}

/// Invokes the target assembler to produce an object file from assembly source.
/// - `target`: Compiler target (controls assembler command and flags).
/// - `asm_path`: Path to the generated `.s` assembly file.
/// - `obj_path`: Output path for the resulting `.o` object file.
/// Exits with status 1 if the assembler fails.
pub(crate) fn assemble(target: Target, asm_path: &Path, obj_path: &Path) {
    let mut as_cmd = Command::new(target.assembler_cmd());
    if target.platform == Platform::MacOS {
        as_cmd.args(["-arch", target.darwin_arch_name()]);
    }
    as_cmd.arg("-o").arg(obj_path).arg(asm_path);
    run_tool("Assembler", &mut as_cmd);
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
) {
    // Bridge staticlibs this program actually links, paired with the directory
    // each one resolved to (`None` when it could not be located/built). Driven
    // by the `BRIDGES` table so a new library needs no changes in this function.
    let needed_bridges: Vec<(&BridgeStaticlib, Option<String>)> = BRIDGES
        .iter()
        .filter(|bridge| extra_link_libs.iter().any(|l| l.as_str() == bridge.lib_name))
        .map(|bridge| (bridge, bridge.lib_dir()))
        .collect();
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
                    // GOT (`@GOTPCREL` on x86_64, `:got:`/`:got_lo12:` on
                    // AArch64) in PIC mode so the loader can fix them up at
                    // dlopen time without text-segment relocations.
                    cmd.arg("-shared");
                }
                Emit::Executable => {}
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
                .find(|(b, d)| b.lib_name == lib.as_str() && b.whole_archive && d.is_some())
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
        }
    }
    for lib in extra_link_libs {
        if lib == "System" {
            continue;
        }
        // A bridge that must be whole-archived (and whose staticlib we located)
        // is force-loaded so its link-time side effects survive; everything else
        // links with a plain `-l`.
        let whole_archive_bridge = needed_bridges.iter().find(|(bridge, dir)| {
            bridge.lib_name == lib.as_str() && bridge.whole_archive && dir.is_some()
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
                }
            }
            None => {
                ld_cmd.arg(format!("-l{}", lib));
            }
        }
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
    run_tool("Linker", &mut ld_cmd);
    // The deduped archive copies were only needed for the link command above.
    if let Some(scratch) = dedup_scratch {
        let _ = std::fs::remove_dir_all(scratch);
    }
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
}
