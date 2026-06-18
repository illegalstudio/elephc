//! Purpose:
//! Tests for the fixed-point EIR pass driver: convergence, the change protocol,
//! the non-convergence cap, and the debug-build post-pass validation gate.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Functions are built by hand with `crate::ir::Builder`. Synthetic passes
//!   stand in for real transforms so the driver mechanics are tested in
//!   isolation. The cap-panic and validation-panic tests are debug-only because
//!   those guards compile out of `--release`.

use crate::ir::{Builder, DataPool, Function, IrType, Terminator};
use crate::ir_passes::driver::{run_function_passes, IrPass};
use crate::types::PhpType;

/// Runs the driver over `function` with a throwaway literal pool, mirroring the
/// real `optimize_module` call but for synthetic-pass tests that intern nothing.
fn drive(function: &mut Function, passes: &[Box<dyn IrPass>]) {
    let mut data = DataPool::default();
    run_function_passes(function, passes, &mut data);
}

/// Builds a minimal valid function: an entry block returning a constant.
fn sample_function() -> Function {
    let mut function = Function::new("sample".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let value = builder.emit_const_i64(7);
        builder.terminate(Terminator::Return { value: Some(value) });
    }
    function
}

/// A pass that never changes anything.
struct NoopPass;
impl IrPass for NoopPass {
    fn name(&self) -> &'static str {
        "noop"
    }
    fn run(&self, _function: &mut Function, _data: &mut DataPool) -> bool {
        false
    }
}

/// A pass that mutates once (appends `!` to the name) then reports stable.
struct AppendBangPass;
impl IrPass for AppendBangPass {
    fn name(&self) -> &'static str {
        "append-bang"
    }
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        if function.name.ends_with('!') {
            false
        } else {
            function.name.push('!');
            true
        }
    }
}

/// A pass that always reports a change, so the driver can never converge.
struct AlwaysChangePass;
impl IrPass for AlwaysChangePass {
    fn name(&self) -> &'static str {
        "always-change"
    }
    fn run(&self, _function: &mut Function, _data: &mut DataPool) -> bool {
        true
    }
}

/// A pass that corrupts the IR by removing the entry block's terminator.
struct DropTerminatorPass;
impl IrPass for DropTerminatorPass {
    fn name(&self) -> &'static str {
        "drop-terminator"
    }
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        let entry = function.entry;
        if let Some(block) = function.block_mut(entry) {
            block.terminator = None;
        }
        true
    }
}

/// A no-op pass leaves the function untouched and the driver returns immediately.
#[test]
fn noop_pass_converges_without_change() {
    let mut function = sample_function();
    let passes: Vec<Box<dyn IrPass>> = vec![Box::new(NoopPass)];
    drive(&mut function, &passes);
    assert_eq!(function.name, "sample");
}

/// A pass that changes once then reports stable drives the loop to a fixed point.
#[test]
fn change_once_pass_converges() {
    let mut function = sample_function();
    let passes: Vec<Box<dyn IrPass>> = vec![Box::new(AppendBangPass)];
    drive(&mut function, &passes);
    assert_eq!(function.name, "sample!", "applied exactly once and converged");
}

/// Several passes in one pipeline converge together without over-applying.
#[test]
fn multiple_passes_converge_together() {
    let mut function = sample_function();
    let passes: Vec<Box<dyn IrPass>> = vec![Box::new(AppendBangPass), Box::new(NoopPass)];
    drive(&mut function, &passes);
    assert_eq!(function.name, "sample!");
}

/// A pass that never converges trips the iteration-cap panic in debug builds.
#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "did not reach a fixed point")]
fn non_convergent_pass_panics_in_debug() {
    let mut function = sample_function();
    let passes: Vec<Box<dyn IrPass>> = vec![Box::new(AlwaysChangePass)];
    drive(&mut function, &passes);
}

/// A pass that produces malformed IR trips the post-pass validation gate in
/// debug builds, with the panic naming the offending pass.
#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "produced invalid IR")]
fn malformed_ir_pass_trips_validation_in_debug() {
    let mut function = sample_function();
    let passes: Vec<Box<dyn IrPass>> = vec![Box::new(DropTerminatorPass)];
    drive(&mut function, &passes);
}
