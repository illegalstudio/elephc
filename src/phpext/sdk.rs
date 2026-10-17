//! Purpose:
//! Discovers and validates a native PHP 8.5 development SDK for extension output.
//!
//! Called from:
//! - Producer output integration and real-PHP SDK integration tests.
//!
//! Key details:
//! - Compiles against real headers and compares their build identity with the paired interpreter.
//! - No shell evaluation, libphp link, replacement Zend engine, or unverified cross-SDK build.
//! - Compiler outputs use private staging and atomic publication.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::codegen_support::platform::Target;
use super::{flags::split_words, probe, scratch::Scratch};

/// Header-derived host identity and value-layout facts, independent of emulated Zend constants.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct HostBuild {
    pub php_version_id: u32,
    pub module_api: u32,
    pub build_id: String,
    pub zts: bool,
    pub debug: bool,
    pub pointer_size: usize,
    pub integer_size: usize,
    pub target: String,
    pub layout: BTreeMap<String, usize>,
}

#[derive(Debug, Deserialize)]
struct InterpreterBuild {
    php_version_id: u32,
    module_api: u32,
    build_id: String,
    zts: bool,
    debug: bool,
    integer_size: usize,
}

/// A matched native SDK/interpreter pair; its module API and build ID come from host headers.
#[derive(Debug)]
pub struct PhpSdk {
    php_binary: PathBuf,
    includes: Vec<String>,
    compiler: Vec<String>,
    compiler_identity: String,
    build: HostBuild,
    target: Target,
}

impl PhpSdk {
    /// Discovers the selected SDK, compares headers and interpreter, and verifies real module loading.
    ///
    /// `work_dir` must be an existing caller-owned build directory. All probe artifacts are
    /// removed on return; no module is installed into PHP's extension directory.
    pub fn discover(php_config: &Path, target: Target, work_dir: &Path) -> Result<Self, String> {
        validate_target(target)?;
        let config = |flag: &str| output_text(Command::new(php_config).arg(flag), "query php-config");
        let configured_version = config("--vernum")?.parse::<u32>()
            .map_err(|_| "php-config --vernum did not return a PHP version ID".to_string())?;
        if configured_version / 100 != 805 {
            return Err("PHP extension output requires a PHP 8.5 development SDK".to_string());
        }
        let includes = split_words(&config("--includes")?)?;
        if includes.is_empty() {
            return Err("php-config --includes returned no host header paths".to_string());
        }
        let configured_binary = config("--php-binary")?;
        let php_binary = std::env::var_os("ELEPHC_PHPEXT_PHP_BINARY").map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(configured_binary));
        if php_binary.as_os_str().is_empty() || php_binary == Path::new("NONE") {
            return Err("php-config does not identify a PHP CLI interpreter; set ELEPHC_PHPEXT_PHP_BINARY to the matching executable".to_string());
        }
        let compiler = split_words(&std::env::var("CC").unwrap_or_else(|_| "cc".to_string()))?;
        if compiler.is_empty() {
            return Err("CC must identify a C compiler".to_string());
        }
        let scratch = Scratch::create(work_dir)?;
        let executable = scratch.join("host-probe");
        compile_c(&compiler, &includes, &probe::source(), &executable, &[])?;
        let text = output_text(&mut Command::new(&executable), "run PHP header probe")?;
        let build: HostBuild = serde_json::from_str(&text)
            .map_err(|error| format!("invalid PHP header probe result: {error}"))?;
        if build.php_version_id != configured_version || build.target != target.as_str()
            || build.pointer_size != 8 || build.integer_size != 8
        {
            return Err(format!("PHP headers/compiler do not match the selected native SDK target {}", target.as_str()));
        }
        validate_value_layout(&build.layout)?;
        let runtime = output_text(Command::new(&php_binary).args(["-n", "-r", INTERPRETER_PROBE]),
            "inspect matching PHP interpreter")?;
        let runtime: InterpreterBuild = serde_json::from_str(&runtime)
            .map_err(|error| format!("invalid PHP interpreter build result: {error}"))?;
        validate_build(&build, &runtime)?;
        let compiler_identity = output_text(compiler_command(&compiler).arg("--version"), "identify C compiler")?;
        let sdk = Self { php_binary, includes, compiler, compiler_identity, build, target };
        let module = scratch.join("sdk-probe.so");
        let source = super::render_registration("elephc_sdk_probe", "0.0.0")?;
        sdk.compile_extension(&source, &module)?;
        let mut load = Command::new(&sdk.php_binary);
        load.args(["-n", "-d"]).arg(extension_option(&module));
        load.args(["-r", "if (!extension_loaded('elephc_sdk_probe')) { exit(1); }"]);
        let loaded = output_text(&mut load, "load PHP SDK validation module")?;
        if !loaded.is_empty() {
            return Err(format!("PHP SDK validation module produced unexpected output: {loaded}"));
        }
        Ok(sdk)
    }

    /// Returns the paired interpreter, rather than whichever PHP binary is first on PATH.
    pub fn php_binary(&self) -> &Path {
        &self.php_binary
    }

    /// Returns the verified header-derived ABI metadata and shared value-layout facts.
    pub fn build(&self) -> &HostBuild {
        &self.build
    }

    /// Hashes the checked ABI and compiler invocation; adapter cache keys must additionally hash sources.
    pub fn abi_identity(&self) -> String {
        let mut digest = Sha256::new();
        digest.update(serde_json::to_vec(&self.build).expect("host build is JSON serializable"));
        for word in self.includes.iter().chain(&self.compiler).chain(std::iter::once(&self.compiler_identity)) {
            digest.update((word.len() as u64).to_le_bytes());
            digest.update(word.as_bytes());
        }
        format!("{:x}", digest.finalize())
    }

    /// Builds a host module, exporting only get_module and publishing it without following symlinks.
    pub fn compile_extension(&self, source: &str, destination: &Path) -> Result<(), String> {
        self.compile_extension_with_objects(source, destination, &[])
    }

    /// Links native producer objects without introducing a dependency on libphp.
    pub(crate) fn compile_extension_with_objects(&self, source: &str, destination: &Path, objects: &[PathBuf]) -> Result<(), String> {
        reject_unsafe_destination(destination)?;
        let parent = destination.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
        let scratch = Scratch::create(parent)?;
        let staged = scratch.join("module.so");
        let mut args = vec![OsString::from("-fvisibility=hidden")];
        match self.target.as_str() {
            "linux-x86_64" | "linux-aarch64" => {
                let exports = scratch.join("exports.map");
                std::fs::write(&exports, "{ global: get_module; local: *; };\n")
                    .map_err(|error| format!("write PHP extension export map: {error}"))?;
                args.extend(["-shared", "-Wl,--exclude-libs,ALL", "-Wl,-z,noexecstack",
                    "-Wl,-z,relro", "-Wl,-z,now"].map(OsString::from));
                args.extend([OsString::from("-Xlinker"), OsString::from("--version-script"),
                    OsString::from("-Xlinker"), exports.into_os_string()]);
            }
            "macos-aarch64" => args.extend(["-bundle", "-undefined", "dynamic_lookup",
                "-Wl,-exported_symbol,_get_module"].map(OsString::from)),
            _ => return Err("unsupported PHP extension target".to_string()),
        }
        args.extend(objects.iter().map(|path| path.as_os_str().to_os_string()));
        args.extend([OsString::from("-pthread"), OsString::from("-lm")]);
        if !objects.is_empty() {
            args.push(OsString::from(if self.target.as_str() == "macos-aarch64" {
                "-Wl,-dead_strip"
            } else { "-Wl,--gc-sections" }));
        }
        compile_c(&self.compiler, &self.includes, source, &staged, &args)?;
        // rename replaces a symlink introduced after the initial check, rather than following it.
        std::fs::rename(&staged, destination).map_err(|error| format!("publish PHP extension: {error}"))
    }
}

/// Rejects a host whose values cannot be consumed by the native transport helpers.
fn validate_value_layout(actual: &BTreeMap<String, usize>) -> Result<(), String> {
    let facts = include!(concat!(env!("OUT_DIR"), "/elephc_zend_value_layout.rs"));
    let expected = facts.into_iter().map(|(key, value)| (key.to_string(), value))
        .collect::<BTreeMap<_, _>>();
    if actual != &expected {
        return Err("PHP SDK value layouts do not match the native producer transport ABI".into());
    }
    Ok(())
}

/// Rejects unsupported and cross-SDK targets before executing any SDK tool.
fn validate_target(target: Target) -> Result<(), String> {
    if !matches!(target.as_str(), "linux-x86_64" | "linux-aarch64" | "macos-aarch64") {
        return Err(format!("PHP extension output is unsupported for {}; supported: linux-x86_64, linux-aarch64, macos-aarch64", target.as_str()));
    }
    if target != Target::detect_host() {
        return Err("PHP extension output currently requires a native matching PHP SDK".to_string());
    }
    Ok(())
}

/// Rejects mixed header/interpreter builds, including API-compatible NTS/ZTS mismatches.
fn validate_build(headers: &HostBuild, php: &InterpreterBuild) -> Result<(), String> {
    if headers.php_version_id != php.php_version_id || headers.module_api != php.module_api
        || headers.build_id != php.build_id || headers.zts != php.zts
        || headers.debug != php.debug || headers.integer_size != php.integer_size
    {
        return Err(format!("PHP SDK headers do not match its interpreter: headers PHP {} {}, ZTS={}, debug={}; interpreter PHP {} {}, ZTS={}, debug={}",
            headers.php_version_id, headers.build_id, headers.zts, headers.debug,
            php.php_version_id, php.build_id, php.zts, php.debug));
    }
    Ok(())
}

/// Builds a compiler command from already parsed words, without shell interpretation.
fn compiler_command(words: &[String]) -> Command {
    let mut command = Command::new(&words[0]);
    command.args(&words[1..]);
    command
}

/// Feeds generated C through stdin, waits for the compiler, and retains its diagnostic on failure.
fn compile_c(words: &[String], includes: &[String], source: &str, output: &Path, args: &[OsString]) -> Result<(), String> {
    let mut command = compiler_command(words);
    command.args(includes).args(["-std=gnu11", "-O2", "-fPIC", "-Wall", "-Wextra", "-Werror"]);
    command.args(args).args(["-x", "c", "-", "-o"]).arg(output);
    command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|error| format!("start PHP SDK C compiler: {error}"))?;
    let input = child.stdin.take().expect("compiler stdin is piped").write_all(source.as_bytes());
    let result = child.wait_with_output().map_err(|error| format!("wait for PHP SDK C compiler: {error}"))?;
    if !result.status.success() {
        return Err(format!("PHP SDK C compilation failed:\n{}", String::from_utf8_lossy(&result.stderr)));
    }
    input.map_err(|error| format!("write generated PHP adapter source: {error}"))
}

/// Executes a build tool and requires successful UTF-8 output, preserving stderr for diagnostics.
fn output_text(command: &mut Command, action: &str) -> Result<String, String> {
    let output = command.output().map_err(|error| format!("{action}: {error}"))?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err(format!("{action} failed ({:?}):\n{}\n{}", output.status.code(),
            String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr)));
    }
    String::from_utf8(output.stdout).map(|text| text.trim().to_string())
        .map_err(|error| format!("{action} returned invalid UTF-8: {error}"))
}

/// Builds one PHP -d option while preserving non-UTF-8 path bytes.
fn extension_option(module: &Path) -> OsString {
    let mut option = OsString::from("extension=");
    option.push(module.as_os_str());
    option
}

/// Refuses destinations that would follow a symlink or replace a non-file artifact.
fn reject_unsafe_destination(destination: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(destination) {
        Ok(metadata) if !metadata.file_type().is_file() => Err(format!("refusing unsafe PHP extension destination '{}'", destination.display())),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("inspect PHP extension destination: {error}")),
    }
}

/// Queries the interpreter build independently of SDK-generated constants or php-config path names.
const INTERPRETER_PROBE: &str = r#"
ob_start(); phpinfo(INFO_GENERAL); $info = ob_get_clean();
if (!preg_match('/^PHP Extension => ([0-9]+)$/m', $info, $api)
    || !preg_match('/^PHP Extension Build => (.+)$/m', $info, $build)) { exit(1); }
echo json_encode(['php_version_id' => PHP_VERSION_ID, 'module_api' => (int)$api[1],
    'build_id' => trim($build[1]), 'zts' => (bool)PHP_ZTS, 'debug' => (bool)PHP_DEBUG,
    'integer_size' => PHP_INT_SIZE], JSON_THROW_ON_ERROR);
"#;

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a matching metadata pair without requiring an installed PHP SDK.
    fn builds() -> (HostBuild, InterpreterBuild) {
        (HostBuild { php_version_id: 80500, module_api: 123, build_id: "API123,NTS".to_string(),
            zts: false, debug: false, pointer_size: 8, integer_size: 8,
            target: "linux-x86_64".to_string(), layout: BTreeMap::new() },
        InterpreterBuild { php_version_id: 80500, module_api: 123, build_id: "API123,NTS".to_string(),
            zts: false, debug: false, integer_size: 8 })
    }

    /// Rejects each ABI mismatch independently, even when both sides report PHP 8.5.
    #[test]
    fn rejects_mixed_host_builds() {
        let (headers, php) = builds();
        assert!(validate_build(&headers, &php).is_ok());
        for field in 0..6 {
            let (mut headers, php) = builds();
            match field {
                0 => headers.zts = true,
                1 => headers.debug = true,
                2 => headers.module_api += 1,
                3 => headers.build_id.push_str(",custom"),
                4 => headers.php_version_id += 1,
                _ => headers.integer_size = 4,
            }
            assert!(validate_build(&headers, &php).is_err(), "mismatch {field}");
        }
    }

    /// Refuses a custom SDK layout before native transport helpers can access its values.
    #[test]
    fn rejects_changed_value_layout() {
        let facts = include!(concat!(env!("OUT_DIR"), "/elephc_zend_value_layout.rs"));
        let mut layout = facts.into_iter().map(|(key, value)| (key.to_string(), value))
            .collect::<BTreeMap<_, _>>();
        assert!(validate_value_layout(&layout).is_ok());
        *layout.get_mut("sizeof(zval)").unwrap() += 8;
        assert!(validate_value_layout(&layout).is_err());
    }

    /// Refuses iOS device and Simulator before querying a PHP toolchain.
    #[test]
    fn rejects_ios_sdk_targets() {
        for name in ["ios-arm64", "ios-sim-arm64"] {
            let target = Target::parse(name).unwrap();
            assert!(validate_target(target).unwrap_err().contains("unsupported"));
        }
    }

    /// Includes ZTS and debug state in ABI identities rather than sharing NTS metadata.
    #[test]
    fn abi_identity_distinguishes_thread_and_debug_builds() {
        let (build, _) = builds();
        let mut sdk = PhpSdk { php_binary: PathBuf::from("php"), includes: vec![], compiler: vec!["cc".to_string()],
            compiler_identity: "cc test".to_string(), build, target: Target::detect_host() };
        let nts = sdk.abi_identity();
        sdk.build.zts = true;
        let zts = sdk.abi_identity();
        sdk.build.debug = true;
        assert_ne!(nts, zts);
        assert_ne!(zts, sdk.abi_identity());
    }
}
