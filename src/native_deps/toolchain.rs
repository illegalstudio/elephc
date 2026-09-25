//! Purpose:
//! Selects target C tools, validates target/libc identity, and computes cache fingerprints.
//!
//! Called from:
//! - Native install preflight, cache key construction, doctor, and compilation resolution.
//!
//! Key details:
//! - Cross targets require explicit tools and Linux GNU/musl identity is separate from Elephc `Target`.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crate::codegen_support::platform::{AppleVariant, Arch, Platform, Target};

use super::error::{NativeError, NativeErrorKind};
use super::receipt::ToolIdentity;
use super::util::{sha256_bytes, unique_sibling};

/// Fully selected native C toolchain and immutable fingerprint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeToolchain {
    pub cc: PathBuf,
    pub ar: PathBuf,
    pub ranlib: PathBuf,
    /// Canonical optional MinGW sysroot used for Windows C headers and archives.
    pub mingw_sysroot: Option<PathBuf>,
    pub target_tuple: String,
    pub abi: String,
    pub fingerprint: String,
    pub compiler: ToolIdentity,
    pub archiver: ToolIdentity,
    pub ranlib_identity: ToolIdentity,
}

/// Injectable target-toolchain selection boundary.
pub trait ToolchainProvider {
    /// Selects tools and computes identity without writing cache or temporary files.
    fn resolve(&self, target: Target) -> Result<NativeToolchain, NativeError>;
}

/// Production provider backed by process environment and tool version probes.
pub struct SystemToolchains;

impl ToolchainProvider for SystemToolchains {
    /// Resolves override precedence and validates the compiler-reported tuple.
    fn resolve(&self, target: Target) -> Result<NativeToolchain, NativeError> {
        resolve_toolchain(target)
    }
}

impl NativeToolchain {
    /// Returns the canonical `config.sub` host identity for Autoconf recipes.
    ///
    /// Apple clang reports SDK-qualified triples such as
    /// `arm64-apple-ios13.0-simulator`; those are valid compiler targets but invalid
    /// `config.sub` inputs. The normalized ABI deliberately removes the deployment
    /// version and simulator suffix, while the selected compiler wrapper still controls
    /// which Apple platform the emitted objects target. MinGW is the converse: Elephc's
    /// cache ABI is `x86_64-pc-windows-gnu`, but GNU `config.sub` requires the compiler's
    /// canonical `x86_64-w64-mingw32` tuple.
    pub(crate) fn autoconf_host(&self) -> &str {
        if self.abi == "x86_64-pc-windows-gnu" {
            &self.target_tuple
        } else {
            &self.abi
        }
    }

    /// Returns the minimal deterministic environment used by configure, Make, and C compilation.
    pub fn build_environment(&self) -> BTreeMap<OsString, OsString> {
        let mut environment = BTreeMap::new();
        for name in ["PATH", "TMPDIR", "TMP", "TEMP", "SYSTEMROOT"] {
            if let Some(value) = std::env::var_os(name) {
                environment.insert(OsString::from(name), value);
            }
        }
        environment.insert(OsString::from("LC_ALL"), OsString::from("C"));
        environment.insert(OsString::from("LANG"), OsString::from("C"));
        environment.insert(OsString::from("CC"), self.cc.as_os_str().to_os_string());
        environment.insert(OsString::from("AR"), self.ar.as_os_str().to_os_string());
        environment.insert(OsString::from("RANLIB"), self.ranlib.as_os_str().to_os_string());
        let mut cflags = OsString::from("-fPIC");
        if let Some(sysroot) = &self.mingw_sysroot {
            cflags.push(" --sysroot=");
            cflags.push(sysroot.as_os_str());
        }
        environment.insert(OsString::from("CFLAGS"), cflags);
        environment
    }

    /// Appends the target-independent PIC flag and the validated optional MinGW sysroot.
    ///
    /// Direct compiler invocations (the compatibility probe and PCRE2 shim) do not consume the
    /// `CFLAGS` environment variable themselves, so they must use this helper as well as the
    /// configure-driven upstream builds.
    pub fn append_compiler_flags(&self, command: &mut Command) {
        command.arg("-fPIC");
        if let Some(sysroot) = &self.mingw_sysroot {
            command.arg(format!("--sysroot={}", sysroot.display()));
        }
    }

    /// Proves before download that compiler objects are accepted by the selected archiver and ranlib.
    pub fn verify_compatibility(&self, cache_root: &Path) -> Result<(), NativeError> {
        fs::create_dir_all(cache_root).map_err(|error| NativeError::io("create toolchain probe root", cache_root, error))?;
        let probe = unique_sibling(&cache_root.join("toolchain-probe"), "stage");
        fs::create_dir(&probe).map_err(|error| NativeError::io("create toolchain probe staging", &probe, error))?;
        let result = (|| {
            let source = probe.join("probe.c");
            let object = probe.join("probe.o");
            let archive = probe.join("libprobe.a");
            fs::write(&source, b"int elephc_native_toolchain_probe(void) { return 0; }\n")
                .map_err(|error| NativeError::io("write toolchain probe", &source, error))?;
            let mut compile = self.command(&self.cc);
            self.append_compiler_flags(&mut compile);
            compile
                .arg(OsStr::new("-c"))
                .arg(&source)
                .arg("-o")
                .arg(&object);
            run_checked(&mut compile, "compile native toolchain probe")?;
            require_nonempty_regular(&object, "compiler did not produce a regular object")?;
            run_checked(self.command(&self.ar).arg("crs").arg(&archive).arg(&object), "archive native toolchain probe")?;
            run_checked(self.command(&self.ranlib).arg(&archive), "index native toolchain probe")?;
            require_nonempty_regular(&archive, "archiver did not produce a regular archive")?;
            Ok(())
        })();
        let _ = fs::remove_dir_all(&probe);
        result
    }

    /// Creates a scrubbed command with only the allowlisted recipe environment.
    pub fn command(&self, executable: &Path) -> Command {
        let mut command = Command::new(executable);
        command.env_clear();
        command.envs(self.build_environment());
        command
    }
}

/// Resolves the effective commands, tuple, ABI, versions, SDK, and fingerprint.
pub fn resolve_toolchain(target: Target) -> Result<NativeToolchain, NativeError> {
    if !target.supports_current_backend() {
        return Err(NativeError::new(NativeErrorKind::Toolchain, format!("unsupported native target '{}'", target.as_str())));
    }
    let host = Target::detect_host();
    let suffix = target.as_str().replace('-', "_").to_ascii_uppercase();
    let (cc, cc_overridden) = select_command_for_target("CC", &suffix, target, target == host)?;
    let (ar, ar_overridden) = select_command_for_target("AR", &suffix, target, target == host)?;
    let (ranlib, ranlib_overridden) =
        select_command_for_target("RANLIB", &suffix, target, target == host)?;
    if target != host
        && target.platform != Platform::Windows
        && !(cc_overridden && ar_overridden && ranlib_overridden)
    {
        return Err(NativeError::new(NativeErrorKind::Toolchain, format!("cross target '{}' requires explicit ELEPHC_NATIVE_CC/AR/RANLIB overrides", target.as_str())));
    }
    let tuple = normalized_output(run_output(Command::new(&cc).arg("-dumpmachine"), "query target C compiler tuple")?);
    let abi = validate_tuple(target, &tuple, target == host)?;
    let cc_version = normalized_output(run_output(Command::new(&cc).arg("--version"), "query target C compiler version")?);
    let ar_version = version_output(&ar, "archiver")?;
    let ranlib_version = version_output(&ranlib, "ranlib")?;
    // The SDK version is a fingerprint input, so it must be the SDK the target
    // actually builds against. Hardcoding `macosx` recorded the host SDK for an
    // iOS build, which meant bumping the iOS SDK never invalidated the cache.
    let sdk = if target.platform == Platform::MacOS {
        normalized_output(run_output(Command::new("xcrun").args(["--sdk", target.apple_sdk_name(), "--show-sdk-version"]), "query selected Apple SDK")?)
    } else {
        String::new()
    };
    let cc_name = cc.to_string_lossy().into_owned();
    let ar_name = ar.to_string_lossy().into_owned();
    let ranlib_name = ranlib.to_string_lossy().into_owned();
    let mingw_sysroot = resolve_mingw_sysroot(target)?;
    let sysroot_fingerprint = mingw_sysroot
        .as_ref()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    let environment = fingerprinted_environment();
    let fingerprint_payload = format!("target={}\nabi={}\ntuple={}\ncc={}\ncc-version={}\nar={}\nar-version={}\nranlib={}\nranlib-version={}\nsdk={}\nmingw-sysroot={}\nCFLAGS=-fPIC\n{}", target.as_str(), abi, tuple, cc_name, cc_version, ar_name, ar_version, ranlib_name, ranlib_version, sdk, sysroot_fingerprint, environment);
    let fingerprint = sha256_bytes(fingerprint_payload.as_bytes());
    Ok(NativeToolchain {
        cc, ar, ranlib, mingw_sysroot, target_tuple: tuple, abi, fingerprint,
        compiler: ToolIdentity { command: cc_name, version: cc_version },
        archiver: ToolIdentity { command: ar_name, version: ar_version },
        ranlib_identity: ToolIdentity { command: ranlib_name, version: ranlib_version },
    })
}

/// Encodes every inherited allowlisted environment value that can influence tool resolution or outputs.
fn fingerprinted_environment() -> String {
    let mut output = String::new();
    for name in ["PATH", "TMPDIR", "TMP", "TEMP", "SYSTEMROOT"] {
        let value = std::env::var_os(name).map(|value| value.to_string_lossy().into_owned()).unwrap_or_default();
        output.push_str(&format!("{name}={value}\n"));
    }
    output.push_str("LC_ALL=C\nLANG=C\n");
    output
}

/// Requires a non-empty, non-symlink regular output from a toolchain compatibility probe.
fn require_nonempty_regular(path: &Path, message: &str) -> Result<(), NativeError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| NativeError::io("inspect toolchain probe output", path, error))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() || metadata.len() == 0 {
        return Err(NativeError::new(NativeErrorKind::Toolchain, message).with_path(path));
    }
    Ok(())
}

/// Selects a target-specific or shared override, with validated MinGW defaults for Windows.
///
/// Non-Windows cross targets still require an explicit override. Windows gets the conventional
/// prefixed MinGW command as a candidate, but the subsequent tuple and archive probes remain
/// mandatory before a cache key can be published.
fn select_command_for_target(
    tool: &str,
    suffix: &str,
    target: Target,
    host: bool,
) -> Result<(PathBuf, bool), NativeError> {
    let targeted = format!("ELEPHC_NATIVE_{tool}_{suffix}");
    if let Some(value) = nonempty_env(&targeted)? {
        return Ok((PathBuf::from(value), true));
    }
    if target.platform == Platform::Windows {
        let windows_name = match tool {
            "CC" => "ELEPHC_WINDOWS_GCC",
            "AR" => "ELEPHC_WINDOWS_AR",
            "RANLIB" => "ELEPHC_WINDOWS_RANLIB",
            _ => unreachable!("native tools are fixed to CC/AR/RANLIB"),
        };
        if let Some(value) = nonempty_env(windows_name)? {
            return Ok((PathBuf::from(value), true));
        }
    }
    let generic = format!("ELEPHC_NATIVE_{tool}");
    if let Some(value) = nonempty_env(&generic)? {
        return Ok((PathBuf::from(value), true));
    }
    if target.platform == Platform::Windows {
        let command = match tool {
            "CC" => "x86_64-w64-mingw32-gcc",
            "AR" => "x86_64-w64-mingw32-ar",
            "RANLIB" => "x86_64-w64-mingw32-ranlib",
            _ => unreachable!("native tools are fixed to CC/AR/RANLIB"),
        };
        return Ok((PathBuf::from(command), false));
    }
    if host {
        return Ok((PathBuf::from(tool.to_ascii_lowercase()), false));
    }
    Err(NativeError::new(NativeErrorKind::Toolchain, format!("missing {targeted} or {generic} for cross target")))
}

/// Reads and canonicalizes the optional shared MinGW sysroot for a Windows recipe.
///
/// The same `ELEPHC_MINGW_SYSROOT` convention is used by bridge and final-link support. An
/// invalid explicit value is rejected before download so it cannot publish a host-contaminated
/// archive under a Windows cache key.
fn resolve_mingw_sysroot(target: Target) -> Result<Option<PathBuf>, NativeError> {
    if target.platform != Platform::Windows {
        return Ok(None);
    }
    let Some(value) = nonempty_env("ELEPHC_MINGW_SYSROOT")? else {
        return Ok(None);
    };
    canonical_mingw_sysroot(Path::new(&value)).map(Some)
}

/// Rejects a non-directory MinGW root and returns its canonical cache-identity path.
fn canonical_mingw_sysroot(path: &Path) -> Result<PathBuf, NativeError> {
    if !path.is_dir() {
        return Err(NativeError::new(
            NativeErrorKind::Toolchain,
            format!("ELEPHC_MINGW_SYSROOT='{}' is not a directory", path.display()),
        ));
    }
    fs::canonicalize(&path)
        .map_err(|error| NativeError::io("canonicalize ELEPHC_MINGW_SYSROOT", &path, error))
}

/// Reads an override and rejects a present empty command.
fn nonempty_env(name: &str) -> Result<Option<OsString>, NativeError> {
    match std::env::var_os(name) {
        Some(value) if value.is_empty() => Err(NativeError::new(NativeErrorKind::Toolchain, format!("{name} must not be empty"))),
        value => Ok(value),
    }
}

/// Validates compiler-reported architecture, OS, and Linux libc identity.
fn validate_tuple(target: Target, tuple: &str, host: bool) -> Result<String, NativeError> {
    let lower = tuple.to_ascii_lowercase();
    let arch_ok = match target.arch {
        Arch::AArch64 => lower.starts_with("aarch64-") || lower.starts_with("arm64-"),
        Arch::X86_64 => lower.starts_with("x86_64-"),
    };
    let os_ok = match target.platform {
        // An iOS cross compiler reports `arm64-apple-ios13.0`, never `darwin`,
        // so matching on a fixed "darwin" here rejected every iOS toolchain.
        //
        // Device and simulator both carry `ios`, and differ only by a trailing
        // `-simulator`. Checking the substring alone would accept a simulator
        // compiler for a device build: same triple prefix, incompatible objects.
        Platform::MacOS => {
            let compiler_targets_simulator = lower.contains("-simulator");
            let want_simulator = matches!(target.apple_variant, AppleVariant::IOSSimulator);
            lower.contains("apple")
                && lower.contains(target.apple_triple_os())
                && compiler_targets_simulator == want_simulator
        }
        Platform::Linux => lower.contains("linux"),
        Platform::Windows => {
            lower == "x86_64-w64-mingw32" || lower == "x86_64-pc-windows-gnu"
        }
    };
    if !arch_ok || !os_ok {
        return Err(NativeError::new(NativeErrorKind::Toolchain, format!("compiler tuple '{tuple}' does not match target '{}'", target.as_str())));
    }
    let arch = match target.arch { Arch::AArch64 => "aarch64", Arch::X86_64 => "x86_64" };
    match target.platform {
        Platform::MacOS => Ok(format!("{arch}-apple-{}", target.apple_triple_os())),
        Platform::Linux => {
            let environment = if lower.contains("musl") {
                "musl"
            } else if lower.contains("gnu") {
                "gnu"
            } else if host {
                host_linux_environment()?
            } else {
                return Err(NativeError::new(NativeErrorKind::Toolchain, format!("cross compiler tuple '{tuple}' does not identify GNU or musl")));
            };
            Ok(format!("{arch}-unknown-linux-{environment}"))
        }
        Platform::Windows => Ok("x86_64-pc-windows-gnu".to_string()),
    }
}

/// Returns the running compiler's Linux libc environment without runtime guessing.
fn host_linux_environment() -> Result<&'static str, NativeError> {
    if cfg!(target_env = "musl") {
        Ok("musl")
    } else if cfg!(target_env = "gnu") {
        Ok("gnu")
    } else {
        Err(NativeError::new(NativeErrorKind::Toolchain, "host compiler tuple omits libc and this Elephc build has no GNU/musl target_env"))
    }
}

/// Queries `--version`, falling back to `-V` for BSD tools that reject the long option.
fn version_output(command: &Path, label: &str) -> Result<String, NativeError> {
    for flag in ["--version", "-V"] {
        let output = Command::new(command).arg(flag).output();
        if let Ok(output) = output {
            if !output.status.success() {
                continue;
            }
            let normalized = normalized_output(output);
            if !normalized.is_empty() {
                return Ok(normalized);
            }
        }
    }
    let executable = resolve_executable(command).ok_or_else(|| NativeError::new(NativeErrorKind::Toolchain, format!("cannot resolve {label} executable '{}'", command.display())))?;
    let (_, sha256) = super::util::hash_file(&executable)?;
    Ok(format!("binary-sha256={sha256}"))
}

/// Resolves a bare tool name through the exact inherited PATH or canonicalizes an explicit path.
fn resolve_executable(command: &Path) -> Option<PathBuf> {
    if command.components().count() > 1 {
        return fs::canonicalize(command).ok();
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|directory| {
        let candidate = directory.join(command);
        fs::metadata(&candidate).is_ok_and(|metadata| metadata.is_file()).then(|| fs::canonicalize(candidate).ok()).flatten()
    })
}

/// Executes a probe command and requires successful status.
fn run_output(command: &mut Command, action: &str) -> Result<Output, NativeError> {
    let output = command.output().map_err(|error| NativeError::new(NativeErrorKind::Toolchain, format!("{action}: {error}")))?;
    if !output.status.success() {
        return Err(NativeError::new(NativeErrorKind::Toolchain, format!("{action} failed: {}", String::from_utf8_lossy(&output.stderr).trim())));
    }
    Ok(output)
}

/// Executes a scrubbed build command and includes captured output in failures.
pub(crate) fn run_checked(command: &mut Command, action: &str) -> Result<(), NativeError> {
    let output = command.output().map_err(|error| NativeError::new(NativeErrorKind::Build, format!("{action}: {error}")))?;
    if !output.status.success() {
        return Err(NativeError::new(NativeErrorKind::Build, format!("{action} failed\nstdout:\n{}\nstderr:\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))));
    }
    Ok(())
}

/// Normalizes combined stdout/stderr to stable single-newline text.
fn normalized_output(output: Output) -> String {
    let bytes = if output.stdout.is_empty() { output.stderr } else { output.stdout };
    String::from_utf8_lossy(&bytes).split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies public targets normalize GNU and musl into distinct ABI identities.
    #[test]
    fn validates_linux_abi_identity() {
        let target = Target::new(Platform::Linux, Arch::X86_64);
        assert_eq!(validate_tuple(target, "x86_64-unknown-linux-gnu", false).unwrap(), "x86_64-unknown-linux-gnu");
        assert_eq!(validate_tuple(target, "x86_64-linux-musl", false).unwrap(), "x86_64-unknown-linux-musl");
        assert!(validate_tuple(target, "aarch64-linux-gnu", false).is_err());
        assert!(validate_tuple(target, "x86_64-linux", false).is_err());
    }

    /// Verifies an iOS compiler tuple is accepted where a fixed "darwin" match
    /// used to reject it, and that macOS still demands a darwin tuple.
    ///
    /// The accepted spellings are measured, not assumed: `clang -dumpmachine`
    /// reports `arm64-apple-ios13.0` under `-target arm64-apple-ios13.0`, and
    /// `arm64-apple-darwin25.5.0` with no target flag.
    #[test]
    fn validates_ios_compiler_tuples() {
        let device = Target::new_apple(Arch::AArch64, AppleVariant::IOS);
        assert_eq!(
            validate_tuple(device, "arm64-apple-ios13.0", false).unwrap(),
            "aarch64-apple-ios"
        );
        assert!(
            validate_tuple(device, "arm64-apple-darwin25.5.0", false).is_err(),
            "a host macOS compiler must not satisfy an iOS target"
        );

        let macos = Target::new(Platform::MacOS, Arch::AArch64);
        assert_eq!(
            validate_tuple(macos, "arm64-apple-darwin25.5.0", true).unwrap(),
            "aarch64-apple-darwin"
        );
        assert!(
            validate_tuple(macos, "arm64-apple-ios13.0", false).is_err(),
            "an iOS compiler must not satisfy a macOS target"
        );
    }

    /// Device and simulator triples differ only by a trailing `-simulator`, so a
    /// plain substring match would accept either for either: same prefix,
    /// incompatible objects. Each must reject the other's compiler.
    #[test]
    fn ios_device_and_simulator_toolchains_are_not_interchangeable() {
        let device = Target::new_apple(Arch::AArch64, AppleVariant::IOS);
        let simulator = Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator);

        assert!(validate_tuple(device, "arm64-apple-ios13.0", false).is_ok());
        assert!(validate_tuple(simulator, "arm64-apple-ios13.0-simulator", false).is_ok());

        assert!(
            validate_tuple(device, "arm64-apple-ios13.0-simulator", false).is_err(),
            "a simulator compiler must not build for a device"
        );
        assert!(
            validate_tuple(simulator, "arm64-apple-ios13.0", false).is_err(),
            "a device compiler must not build for the simulator"
        );
    }

    /// Verifies Autoconf receives the normalized Apple ABI instead of clang's
    /// SDK-qualified simulator triple, which `config.sub` rejects.
    #[test]
    fn autoconf_host_normalizes_ios_simulator_compiler_triples() {
        let toolchain = NativeToolchain {
            cc: "cc".into(),
            ar: "ar".into(),
            ranlib: "ranlib".into(),
            target_tuple: "arm64-apple-ios13.0-simulator".into(),
            abi: "aarch64-apple-ios".into(),
            fingerprint: "fp".into(),
            compiler: ToolIdentity {
                command: "cc".into(),
                version: "v".into(),
            },
            archiver: ToolIdentity {
                command: "ar".into(),
                version: "v".into(),
            },
            ranlib_identity: ToolIdentity {
                command: "ranlib".into(),
                version: "v".into(),
            },
            mingw_sysroot: None,
        };

        assert_eq!(toolchain.autoconf_host(), "aarch64-apple-ios");
        assert_ne!(toolchain.autoconf_host(), toolchain.target_tuple);
    }

    /// MinGW's config.sub identity differs from the stable Elephc cache ABI.
    #[test]
    fn autoconf_host_uses_mingw_compiler_tuple_for_windows() {
        let toolchain = NativeToolchain {
            cc: "x86_64-w64-mingw32-gcc".into(),
            ar: "x86_64-w64-mingw32-ar".into(),
            ranlib: "x86_64-w64-mingw32-ranlib".into(),
            mingw_sysroot: None,
            target_tuple: "x86_64-w64-mingw32".into(),
            abi: "x86_64-pc-windows-gnu".into(),
            fingerprint: "fp".into(),
            compiler: ToolIdentity { command: "cc".into(), version: "v".into() },
            archiver: ToolIdentity { command: "ar".into(), version: "v".into() },
            ranlib_identity: ToolIdentity { command: "ranlib".into(), version: "v".into() },
        };

        assert_eq!(toolchain.autoconf_host(), "x86_64-w64-mingw32");
        assert_ne!(toolchain.autoconf_host(), toolchain.abi);
    }

    /// Verifies recipe environment excludes inherited compiler and linker flags.
    #[test]
    fn build_environment_is_allowlisted() {
        let toolchain = NativeToolchain {
            cc: "cc".into(), ar: "ar".into(), ranlib: "ranlib".into(), mingw_sysroot: None, target_tuple: "aarch64-apple-darwin".into(), abi: "aarch64-apple-darwin".into(), fingerprint: "fp".into(),
            compiler: ToolIdentity { command: "cc".into(), version: "v".into() }, archiver: ToolIdentity { command: "ar".into(), version: "v".into() }, ranlib_identity: ToolIdentity { command: "ranlib".into(), version: "v".into() },
        };
        let environment = toolchain.build_environment();
        assert_eq!(environment.get(OsStr::new("CFLAGS")), Some(&OsString::from("-fPIC")));
        assert!(!environment.contains_key(OsStr::new("LDFLAGS")));
        assert!(!environment.contains_key(OsStr::new("MAKEFLAGS")));
        assert!(fingerprinted_environment().contains("PATH="));
    }

    /// Verifies an explicit MinGW sysroot is canonicalized and rejects a file or missing path.
    #[test]
    fn mingw_sysroot_requires_a_directory() {
        let root = unique_sibling(&std::env::temp_dir().join("elephc-native-mingw-sysroot"), "test");
        fs::create_dir(&root).unwrap();
        let canonical = canonical_mingw_sysroot(&root).unwrap();
        assert_eq!(canonical, fs::canonicalize(&root).unwrap());
        fs::remove_dir_all(&root).unwrap();
        assert!(canonical_mingw_sysroot(&root).is_err());
    }
}
