//! Purpose:
//! Platform fixture helpers for selecting assembler, linker, SDK, and target settings in codegen tests.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Includes a focused target-selection test plus host and cross-target command assumptions.

use super::*;

/// Returns the current test target, initialized from `ELEPHC_TEST_TARGET` env var
/// or auto-detected from the host platform. Cached across the test run.
pub(crate) fn target() -> Target {
    *TEST_TARGET.get_or_init(|| {
        std::env::var("ELEPHC_TEST_TARGET")
            .ok()
            .map(|value| Target::parse(&value).expect("invalid ELEPHC_TEST_TARGET"))
            .unwrap_or_else(Target::detect_host)
    })
}

/// Returns the macOS SDK path by running `xcrun --show-sdk-path`.
/// Falls back to an empty string if the command fails.
pub(crate) fn get_sdk_path() -> &'static str {
    SDK_PATH.get_or_init(|| {
        Command::new("xcrun")
            .args(["--show-sdk-path"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    })
}

/// Returns the macOS SDK version by running `xcrun --sdk macosx --show-sdk-version`.
/// Falls back to "15.0" if the command fails or returns an empty string.
pub(crate) fn get_sdk_version() -> &'static str {
    SDK_VERSION.get_or_init(|| {
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
    })
}

/// Returns the platform-specific assembler command (e.g., `as` on Linux, or `as` with
/// `-arch arm64` on macOS). Delegates to `target().assembler_cmd()`.
pub(crate) fn assembler_cmd() -> &'static str {
    target().assembler_cmd()
}

/// Returns the platform-specific linker/gcc command (e.g., `gcc` on Linux).
/// Delegates to `target().linker_cmd()`.
pub(crate) fn gcc_cmd() -> &'static str {
    target().linker_cmd()
}

/// Returns platform-specific library search paths used during linking.
/// On macOS checks `/opt/homebrew/lib` and `/usr/local/lib`.
/// On Linux checks aarch64 sysroot paths.
pub(crate) fn default_link_paths() -> Vec<String> {
    let mut paths = Vec::new();
    match target().platform {
        Platform::MacOS => {
            for candidate in ["/opt/homebrew/lib", "/usr/local/lib"] {
                if std::path::Path::new(candidate).exists() {
                    paths.push(candidate.to_string());
                }
            }
        }
        Platform::Linux => {
            for candidate in ["/usr/aarch64-linux-gnu/lib", "/usr/lib/aarch64-linux-gnu"] {
                if std::path::Path::new(candidate).exists() {
                    paths.push(candidate.to_string());
                }
            }
        }
        Platform::Windows => {
            // MinGW's `x86_64-w64-mingw32-gcc` resolves its own import libraries
            // (kernel32, msvcrt, ...), so the windows-x86_64 measurement target
            // needs no extra `-L` search paths threaded through here.
        }
    }
    // The elephc-tls / elephc-pdo bridge staticlib directory is added directly by
    // `link_binary` (an absolute, manifest-anchored `-L` keyed on the program
    // actually linking a bridge), so it does not need to be threaded through here.
    paths
}

/// Filters a list of library names, removing "System" which is handled
/// separately by the macOS linker. Returns the remaining library names as string slices.
pub(crate) fn effective_link_libs(extra_link_libs: &[String]) -> Vec<&str> {
    extra_link_libs
        .iter()
        .map(String::as_str)
        .filter(|lib| *lib != "System")
        .collect()
}

/// Returns the sysroot path for qemu-aarch64 when running ARM64 binaries
/// on a host cross-compiler. Queries the gcc sysroot or scans known aarch64
/// linux GNU paths. Returns `None` on macOS or when no sysroot is found.
pub(crate) fn qemu_sysroot() -> Option<&'static str> {
    QEMU_SYSROOT
        .get_or_init(|| match target().platform {
            Platform::Linux => {
                let compiler = gcc_cmd();
                if let Ok(output) = Command::new(compiler).arg("-print-sysroot").output() {
                    let sysroot = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if !sysroot.is_empty()
                        && sysroot != "/"
                        && std::path::Path::new(&sysroot).exists()
                    {
                        return Some(sysroot);
                    }
                }
                for candidate in ["/usr/aarch64-linux-gnu", "/usr/local/aarch64-linux-gnu"] {
                    if std::path::Path::new(candidate)
                        .join("lib/ld-linux-aarch64.so.1")
                        .exists()
                        || std::path::Path::new(candidate)
                            .join("lib64/ld-linux-aarch64.so.1")
                            .exists()
                    {
                        return Some(candidate.to_string());
                    }
                }
                None
            }
            Platform::MacOS => None,
            // Windows binaries run under Wine, not qemu, so there is no sysroot.
            Platform::Windows => None,
        })
        .as_deref()
}

/// Reports whether the MinGW-w64 x86_64 cross toolchain is installed, by probing
/// `x86_64-w64-mingw32-gcc --version`. Required to assemble and link the
/// windows-x86_64 measurement target's `.exe`. Mirrors the probe used by the
/// dedicated `windows_pe` tests.
pub(crate) fn has_mingw() -> bool {
    Command::new("x86_64-w64-mingw32-gcc")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Reports whether Wine is installed, by probing `wine64` first (the native 64-bit
/// loader) then falling back to `wine`. Required to execute a cross-compiled
/// windows-x86_64 `.exe`. Mirrors the probe used by the `windows_pe` tests.
pub(crate) fn has_wine() -> bool {
    Command::new("wine64")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
        || Command::new("wine")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
}

/// Returns the preferred Wine binary name: `wine64` when present, else `wine`.
/// Both run PE32+ binaries on modern distros; `wine64` is tried first to match the
/// selection the `windows_pe` execution tests use.
pub(crate) fn wine_binary() -> &'static str {
    if Command::new("wine64")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        "wine64"
    } else {
        "wine"
    }
}

/// Reports whether the windows-x86_64 test toolchain can build and execute PE files.
///
/// MinGW-w64 is always required to assemble/link the GNU-target executable.
/// A native Windows host executes that PE directly; cross-host runs additionally
/// require Wine. The result is cached because every fixture probes this guard.
pub(crate) fn windows_toolchain_available() -> bool {
    static WINDOWS_TOOLCHAIN_AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *WINDOWS_TOOLCHAIN_AVAILABLE.get_or_init(|| has_mingw() && (cfg!(windows) || has_wine()))
}

/// Reports whether the current job has declared that windows-x86_64 fixtures must
/// really execute, through `ELEPHC_REQUIRE_WINDOWS_TOOLCHAIN`.
///
/// `ensure_windows_runnable_or_skip` treats a missing toolchain as a graceful
/// skip that nextest records as a pass. That is right on a developer machine,
/// but it is the one way the strict native gate can go green while proving
/// nothing: `windows-codegen-gate` compares testcase *names* against the
/// runnable inventory, so a shard whose MinGW dropped off `PATH` would emit the
/// same complete, failure-free JUnit report as a shard that executed every
/// fixture. CI jobs that install the toolchain deliberately therefore set this
/// variable, which converts the skip into a hard failure.
pub(crate) fn windows_toolchain_is_required() -> bool {
    windows_toolchain_requirement_from(
        std::env::var("ELEPHC_REQUIRE_WINDOWS_TOOLCHAIN").ok().as_deref(),
    )
}

/// Decides the toolchain requirement from a raw variable value. Split out as a
/// pure helper so the gating can be unit-tested without mutating process
/// environment, which is racy under parallel test execution.
fn windows_toolchain_requirement_from(value: Option<&str>) -> bool {
    value.is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
}

/// Gracefully skips the current codegen fixture when it targets windows-x86_64 but
/// its host-appropriate execution toolchain is missing (e.g. a macOS dev host
/// without Wine, or a native Windows host without MinGW-w64).
///
/// Exits the test process with success. Under `cargo nextest` each test runs in its
/// own process, so this reports the individual test as passed/skipped rather than
/// failing it — the same "guard and return" outcome the dedicated `windows_pe`
/// tests use, adapted to helpers that cannot early-return through the caller's
/// assertion. On every non-Windows target this is a no-op, so the native suite is
/// completely unaffected.
///
/// When `ELEPHC_REQUIRE_WINDOWS_TOOLCHAIN` is set the skip is refused outright:
/// see `windows_toolchain_is_required`.
pub(crate) fn ensure_windows_runnable_or_skip() {
    if target().platform != Platform::Windows {
        return;
    }
    if !windows_toolchain_available() {
        assert!(
            !windows_toolchain_is_required(),
            "windows-x86_64 execution toolchain is unavailable, but this job set \
             ELEPHC_REQUIRE_WINDOWS_TOOLCHAIN: it declared that every Windows fixture \
             really executes, so skipping here would report a vacuous pass to the \
             strict native codegen gate"
        );
        eprintln!(
            "skipping windows-x86_64 codegen fixture: MinGW-w64/native-or-Wine execution toolchain unavailable"
        );
        std::process::exit(0);
    }
}

/// Reports whether a dedicated `windows_pe` fixture may proceed, given the parts
/// of the toolchain it needs: MinGW-w64 always, plus a way to execute the produced
/// PE (a native Windows host, else Wine) when `needs_execution` is set.
///
/// Returns `true` to proceed and `false` to skip, and panics instead of returning
/// `false` when the job set `ELEPHC_REQUIRE_WINDOWS_TOOLCHAIN`. The `windows_pe`
/// module cannot use `ensure_windows_runnable_or_skip`: those tests run with the
/// host target selected (so `target().platform` is not Windows on the Ubuntu Wine
/// jobs) and they guard on MinGW and Wine separately. Without this, a job whose
/// MinGW or Wine install silently degraded would report every `windows_pe` test as
/// passed while compiling and executing nothing — the same vacuous-pass hole the
/// strict codegen gate closes, on the four jobs the gate does not cover
/// (`windows-pe-cross-compile`, `windows-pe-llvm-lld`, `windows-bridge-native-build`).
pub(crate) fn windows_pe_fixture_may_run(what: &str, needs_execution: bool) -> bool {
    let mingw = has_mingw();
    let can_execute = !needs_execution || cfg!(windows) || has_wine();
    if mingw && can_execute {
        return true;
    }
    let missing = if !mingw {
        "MinGW-w64 (x86_64-w64-mingw32-gcc)"
    } else {
        "a PE execution host (native Windows or Wine)"
    };
    assert!(
        !windows_toolchain_is_required(),
        "skipping {what}: {missing} is unavailable, but this job set \
         ELEPHC_REQUIRE_WINDOWS_TOOLCHAIN — it declared that Windows PE fixtures \
         really compile and run, so skipping here would report a vacuous pass"
    );
    eprintln!("skipping {what}: {missing} not found");
    false
}

/// Applies the target's final assembly rewrite before assembling. For
/// windows-x86_64 this rewrites the shared x86_64 backend's raw Linux syscall
/// sequences into `__rt_sys_*` shim calls, exactly as the CLI pipeline
/// (`src/pipeline.rs`) and runtime cache (`src/runtime_cache.rs`) do before
/// assembling. For every other target it returns the assembly unchanged, so the
/// native suite stays byte-identical.
pub(crate) fn finalize_asm_for_target(asm: &str) -> String {
    if target().platform == Platform::Windows {
        elephc::codegen::platform::transform_for_windows(asm)
    } else {
        asm.to_string()
    }
}

/// Verifies `effective_link_libs` filters out "System" from the library list.
/// The macOS linker handles "System" specially and does not accept it as a
/// normal `-l` argument. Input fixture: ["System", "crypto"] → ["crypto"].
#[test]
fn test_effective_link_libs_ignores_system() {
    let libs = vec!["System".to_string(), "crypto".to_string()];
    assert_eq!(effective_link_libs(&libs), vec!["crypto"]);
}

/// Verifies the strict-gate opt-in accepts exactly the affirmative spellings and
/// stays off when the variable is absent, empty, or negative. An accidental
/// "always true" here would break every developer host without Wine; an
/// accidental "always false" would silently restore the vacuous-pass path the
/// native Windows gate depends on being closed.
#[test]
fn test_windows_toolchain_requirement_parsing() {
    assert!(windows_toolchain_requirement_from(Some("1")));
    assert!(windows_toolchain_requirement_from(Some("true")));
    assert!(windows_toolchain_requirement_from(Some("TRUE")));
    assert!(!windows_toolchain_requirement_from(None));
    assert!(!windows_toolchain_requirement_from(Some("")));
    assert!(!windows_toolchain_requirement_from(Some("0")));
    assert!(!windows_toolchain_requirement_from(Some("false")));
}
