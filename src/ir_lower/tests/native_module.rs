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
    run_with_optional_c_source(module, None)
}

/// Adds a host C caller to native ABI fixtures that do not need the interpreter bridge.
pub(super) fn run_with_c_source(module: &crate::ir::Module, source: &str) -> Output {
    run_with_optional_c_source(module, Some(source))
}

/// Shares native assembly, runtime, linking and isolated-directory ownership across caller kinds.
fn run_with_optional_c_source(module: &crate::ir::Module, c_source: Option<&str>) -> Output {
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
    if features.eval_bridge {
        assert!(c_source.is_none(), "C caller fixture requires a native-only runtime");
        link_eval_fixture(module.target, features, &directory.0, &user_path, &runtime_path, &executable);
        return Command::new(&executable).current_dir(&directory.0).output().expect("run native eval fixture");
    }
    let mut compiler = Command::new("cc");
    if cfg!(target_os = "linux") { compiler.arg("-no-pie"); }
    compiler.arg(&user_path).arg(&runtime_path);
    if let Some(source) = c_source {
        let path = directory.0.join("caller.c");
        std::fs::write(&path, source).unwrap();
        compiler.arg(path);
    }
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
        if std::env::var("ELEPHC_TEST_PREBUILT_BRIDGES").is_ok_and(|value|
            value == "1" || value.eq_ignore_ascii_case("true")) {
            let archive = std::env::var_os("ELEPHC_REFERENCE_LIB_DIR")
                .map(PathBuf::from).map(|root| root.join("libelephc_reference.a"))
                .or_else(|| std::env::var_os("CARGO_TARGET_DIR")
                    .map(PathBuf::from).map(|root| root.join("debug/libelephc_reference.a")))
                .or_else(|| std::env::var_os("ELEPHC_CI_TEST_ROOT")
                    .map(PathBuf::from).map(|root| root.join("target/debug/libelephc_reference.a")))
                .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("target/debug/libelephc_reference.a"));
            assert!(archive.is_file(),
                "ELEPHC_TEST_PREBUILT_BRIDGES is set but {} is missing from the nextest archive",
                archive.display());
            return archive;
        }
        let output = Command::new("cargo").args(["build", "-j1", "--package", "elephc-reference",
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

/// Uses the production runtime requirement and linker planners for fixtures that execute eval.
fn link_eval_fixture(
    target: crate::codegen::platform::Target, features: crate::codegen::RuntimeFeatures,
    directory: &std::path::Path, user: &std::path::Path, runtime: &std::path::Path,
    executable: &std::path::Path,
) {
    let requirements = crate::codegen_support::link_requirements_for_runtime_features(features);
    assert!(!requirements.iter().any(|item| matches!(item, crate::codegen::LinkRequirement::NativePackage(_))),
        "staged eval fixture needs explicit managed native package inputs: {requirements:?}");
    let plan = crate::link_planning::build(crate::link_planning::LinkPlanningInputs {
        user_libraries: &[], user_search_paths: &[], user_frameworks: &[], checker_libraries: &[],
        runtime_requirements: &requirements, managed_packages: &[], forced_bridges: &[], web: false,
    });
    let objects = [directory.join("user.o"), directory.join("runtime.o")];
    for (source, object) in [user, runtime].into_iter().zip(&objects) {
        let result = crate::linker::assembler_command(target).arg("-o").arg(object).arg(source)
            .output().expect("assemble staged eval fixture");
        assert!(result.status.success(), "native eval assembly: {}", String::from_utf8_lossy(&result.stderr));
    }
    let result = crate::linker::link_test_fixture(target, executable, &objects[0], &objects[1], &plan)
        .expect("prepare staged native eval link inputs");
    assert!(result.status.success(), "native eval link: {}",
        String::from_utf8_lossy(&result.stderr).lines().take(35).collect::<Vec<_>>().join("\n"));
}
