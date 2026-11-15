//! Purpose:
//! Executes an EIR module checked under a staged profile without weakening public admission.
//!
//! Called from:
//! - Host-native library regressions that need the real selected-profile checker metadata.
//!
//! Key details:
//! - The enclosing Cargo test runs through the machine-wide build queue.
//! - An owned directory removes both assembly inputs and the executable on success or panic.
//! - Canonical reference modules link the exact static archive reported by Cargo.

use std::{collections::HashMap, path::PathBuf, process::{Command, Output}};

/// Owns only the unique directory this helper created successfully.
struct NativeDirectory(PathBuf);

impl NativeDirectory {
    /// Creates an isolated module fixture without claiming an existing path.
    fn new() -> Self {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("elephc-strict-native-{}-{stamp}", std::process::id()));
        std::fs::create_dir(&path).expect("create isolated native module fixture");
        Self(path)
    }
}

impl Drop for NativeDirectory {
    /// Removes exactly the fixture directory owned by this test, including assertion failures.
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
}

/// Links user and runtime assembly as separate objects through the host C toolchain and runs them.
pub(super) fn run(module: &crate::ir::Module) -> Output {
    let directory = NativeDirectory::new();
    let generated = crate::codegen::generate_user_asm_from_ir_with_options_and_features(
        module, false, false, crate::codegen::Instrumentation::Off, false, true, false,
        crate::codegen::Emit::Executable, &HashMap::new(), true, false,
        crate::codegen::WebIsolation::Worker,
    ).unwrap();
    let mut features = module.required_runtime_features;
    features.handler_state |= generated.handler_state;
    features.object_clone |= generated.object_clone;
    let runtime = crate::codegen_support::generate_runtime_with_semantics(
        8_388_608, module.target, features, false, false, module.semantic_profile);
    let user_path = directory.0.join("user.s"); let runtime_path = directory.0.join("runtime.s");
    let executable = directory.0.join("main");
    std::fs::write(&user_path, generated.asm).unwrap(); std::fs::write(&runtime_path, runtime).unwrap();
    let mut compiler = Command::new("cc");
    if cfg!(target_os = "linux") { compiler.arg("-no-pie"); }
    compiler.arg(&user_path).arg(&runtime_path);
    if features.native_reference_validation {
        compiler.arg(reference_validation_archive());
        if cfg!(target_os = "linux") { compiler.arg("-ldl"); }
    }
    let linked = compiler.arg("-lm").arg("-o").arg(&executable)
        .output().expect("link native module with host C toolchain");
    assert!(linked.status.success(), "native module link: {}", String::from_utf8_lossy(&linked.stderr));
    Command::new(&executable).current_dir(&directory.0).output().expect("run native module fixture")
}

/// Builds the host reference bridge once inside the enclosing machine-wide queued test job.
fn reference_validation_archive() -> &'static std::path::Path {
    static ARCHIVE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    ARCHIVE.get_or_init(|| {
        let output = Command::new("cargo").args(["build", "-j4", "--package", "elephc-reference",
            "--lib", "--message-format=json"])
            .current_dir(env!("CARGO_MANIFEST_DIR")).output().expect("build native reference bridge");
        assert!(output.status.success(), "reference bridge build: {}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8_lossy(&output.stdout).lines().filter_map(|line|
            serde_json::from_str::<serde_json::Value>(line).ok()).find_map(|artifact| {
                if artifact["reason"] != "compiler-artifact" || artifact["target"]["name"] != "elephc_reference" {
                    return None;
                }
                artifact["filenames"].as_array()?.iter().filter_map(|file| file.as_str())
                    .find(|file| file.ends_with(".a")).map(PathBuf::from)
            }).expect("Cargo reports the reference static archive")
    }).as_path()
}
