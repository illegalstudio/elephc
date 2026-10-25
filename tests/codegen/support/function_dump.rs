//! Purpose:
//! Captures selected fixture functions as EIR for remote test diagnosis.
//!
//! Called from:
//! - Compiler fixture stages and tests opting into `with_function_dump`.
//!
//! Key details:
//! - The selection is thread local, restored on panic, and uses the existing compilation.
//! - The test harness displays captured diagnostics only for failing tests.

use std::cell::RefCell;

thread_local! {
    static FUNCTIONS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Selects function-name fragments for diagnostics within this test thread only.
pub(crate) fn with_function_dump<T>(names: &[&str], body: impl FnOnce() -> T) -> T {
    struct Restore(Vec<String>);
    impl Drop for Restore {
        /// Restores the selection displaced by this scope, including on panic.
        fn drop(&mut self) { FUNCTIONS.with(|names| { names.replace(std::mem::take(&mut self.0)); }); }
    }
    let selected = names.iter().map(|name| name.to_ascii_lowercase()).collect();
    let _restore = Restore(FUNCTIONS.with(|names| names.replace(selected)));
    body()
}

/// Prints selected optimized EIR bodies with resolved data-pool immediates.
pub(super) fn dump_fixture_ir(module: &elephc::ir::Module) {
    FUNCTIONS.with(|names| {
        let names = names.borrow();
        if names.is_empty() { return; }
        let mut selected = false;
        for line in elephc::ir::print_module(module).lines() {
            if line.starts_with("  function ") {
                let header = line.to_ascii_lowercase();
                selected = names.iter().any(|name| header.contains(name));
            }
            if selected { eprintln!("EIR: {line}"); }
        }
    });
}
