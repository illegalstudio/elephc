//! Purpose:
//! Owns isolated CLI projects for strict semantic fixtures.
//!
//! Called from:
//! - Strict feature tests covering source rejection and native execution.
//!
//! Key details:
//! - Drop removes only this fixture's directory, including after assertion failures.

use super::*;
use std::path::PathBuf;
use std::process::Output;

/// Optional setup for staged runtime capabilities and native Number-class coverage.
#[derive(Default)]
struct StagedRuntimeOptions {
    eval_bridge: bool,
    bcmath: bool,
    source_mode: Option<elephc::source::SourceMode>,
}

/// A PHP entry with optional include or Composer/autoload companions.
pub(crate) struct StrictFixture {
    directory: PathBuf,
}

impl StrictFixture {
    /// Creates one task-owned fixture using the established unique CLI directory helper.
    pub(crate) fn new(source: &str) -> Self {
        let fixture = Self { directory: make_cli_test_dir("elephc_strict_semantics") };
        fixture.file("main.php", source);
        fixture
    }

    /// Adds a companion file and any directories contained within this fixture.
    pub(crate) fn file(&self, name: &str, contents: &str) {
        let path = self.directory.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    /// Compiles through the real CLI with explicit semantic options.
    fn compile(&self, flags: &[&str]) -> Output {
        elephc_cli_command(&self.directory).args(flags).arg(self.directory.join("main.php"))
            .output().expect("run strict fixture compiler")
    }

    /// Asserts a compile diagnostic and includes the source when a refusal is missing.
    pub(crate) fn assert_error(&self, flags: &[&str], expected: &str) {
        let output = self.compile(flags);
        let errors = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "strict fixture unexpectedly compiled: {}",
            fs::read_to_string(self.directory.join("main.php")).unwrap());
        assert!(errors.contains(expected), "expected {expected:?}, received: {errors}");
    }

    /// Compiles and executes a fixture, requiring successful compilation and execution.
    pub(crate) fn capture(&self, flags: &[&str]) -> Output {
        let output = self.compile(flags);
        assert!(output.status.success(), "strict fixture compilation failed: {}",
            String::from_utf8_lossy(&output.stderr));
        let output = run_binary(&self.directory.join("main"), &self.directory);
        assert!(output.status.success(), "strict fixture execution failed: status={} stdout={} stderr={}",
            output.status, String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        output
    }

    /// Returns stdout for fixtures whose successful execution needs no further inspection.
    pub(crate) fn run(&self, flags: &[&str]) -> String {
        String::from_utf8(self.capture(flags).stdout).unwrap()
    }

    /// Executes unfinished lowering policies without exposing them as accepted CLI guarantees.
    pub(crate) fn capture_staged_lowering(
        &self, profile: elephc_semantics::SemanticProfile, optimize: bool,
    ) -> ProgramOutput {
        self.capture_staged_runtime(profile, optimize, StagedRuntimeOptions::default())
    }

    /// Links the native eval callback ABI while its public semantic profile remains staged.
    pub(crate) fn capture_staged_eval_runtime(
        &self, profile: elephc_semantics::SemanticProfile, optimize: bool,
    ) -> ProgramOutput {
        self.capture_staged_runtime(profile, optimize, StagedRuntimeOptions { eval_bridge: true, ..Default::default() })
    }

    /// Exercises native decimal operators with the same prelude injection and names as the compiler.
    pub(crate) fn capture_staged_bcmath(
        &self, profile: elephc_semantics::SemanticProfile, optimize: bool,
    ) -> ProgramOutput {
        self.capture_staged_runtime(profile, optimize, StagedRuntimeOptions { bcmath: true, ..Default::default() })
    }

    /// Checks trusted implementation operators with native class dispatch in a strict module.
    pub(crate) fn capture_staged_internal_bcmath(
        &self, profile: elephc_semantics::SemanticProfile, optimize: bool,
    ) -> ProgramOutput {
        self.capture_staged_runtime(profile, optimize, StagedRuntimeOptions {
            bcmath: true, source_mode: Some(elephc::source::SourceMode::Internal), ..Default::default()
        })
    }

    /// Shares isolated assembly/link/heap validation with an explicitly requested eval runtime.
    fn capture_staged_runtime(
        &self, profile: elephc_semantics::SemanticProfile, optimize: bool, options: StagedRuntimeOptions,
    ) -> ProgramOutput {
        let source = fs::read_to_string(self.directory.join("main.php")).unwrap();
        let source_mode = options.source_mode.unwrap_or(elephc::source::SourceMode::Php);
        let source = if source_mode == elephc::source::SourceMode::Php { source.as_str() }
            else { source.strip_prefix("<?php").unwrap_or(&source) };
        let tokens = elephc::lexer::tokenize_with_semantics(&source, source_mode, 0, profile).unwrap();
        let mut program = elephc::parser::parse_with_mode(&tokens, source_mode).unwrap();
        if options.bcmath {
            let mut inventory = elephc::optimize::reachability::PreludeInventory::new();
            program = elephc::core_prelude::inject_if_used(program, &mut inventory);
            program = elephc::bcmath_prelude::inject_if_used(program, false, &mut inventory);
            program = elephc::name_resolver::resolve(program).unwrap();
        }
        let mut checked = elephc::types::check_with_target(&program, target()).unwrap();
        checked.semantic_profile = profile;
        let _semantics = elephc::optimize::scoped_semantic_profile(profile);
        if optimize {
            program = elephc::optimize::fold_constants_for_target(program, target());
            program = elephc::optimize::propagate_constants(program, checked.mixed_storage_local_names(), checked.buffer_read_sites.clone());
            program = elephc::optimize::prune_constant_control_flow(program, checked.local_binding_decision_spans());
            program = elephc::optimize::normalize_control_flow(program, checked.local_binding_decision_spans());
            program = elephc::optimize::eliminate_dead_code(program, checked.local_binding_decision_spans());
        }
        let mut module = elephc::ir_lower::lower_program(&program, &checked, target(), false).unwrap();
        module.required_runtime_features.eval_bridge |= options.eval_bridge;
        if optimize { elephc::ir_passes::optimize_module(&mut module); }
        elephc::ir::validate_module(&module).unwrap();
        let generated = elephc::codegen::generate_user_asm_from_ir_with_options_and_features(
            &module, false, false, elephc::codegen::Instrumentation::Off, false, true, false,
            elephc::codegen::Emit::Executable, &HashMap::new(), true, false,
            elephc::codegen::WebIsolation::Worker,
        ).unwrap();
        let mut features = module.required_runtime_features;
        features.handler_state |= generated.handler_state;
        features.object_clone |= generated.object_clone;
        let runtime = elephc::codegen_support::generate_runtime_with_semantics(
            8_388_608, target(), features, false, false, profile);
        let runtime_object = self.directory.join("runtime.o");
        assemble_from_stdin(&runtime, &runtime_object);
        let requirements = TestLinkRequirements::new(checked.required_libraries,
            elephc::codegen::link_requirements_for_runtime_features(features));
        assemble_and_run_capture(&generated.asm, &runtime_object, &self.directory,
            &requirements, &default_link_paths(), &[])
    }
}

impl Drop for StrictFixture {
    /// Removes the exact directory created by this fixture on success and unwinding.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}
