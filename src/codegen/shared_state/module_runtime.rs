//! Purpose:
//! Caches immutable source-module runtime flags used by function emission.
//!
//! Called from:
//! - Function context construction and block emission frame setup.
//!
//! Key details:
//! - One SharedCodegenState belongs to one immutable source-body graph.
//! - Wrapper snapshots only extend literals and preserve the same body facts.
//! - Generated runtime requirements remain separately tracked during emission.

use super::SharedCodegenState;
use crate::ir::{Immediate, Module, Op, PcntlRuntime};

/// Read-only module properties that must agree for every emitted function frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::codegen) struct ModuleRuntimeFacts {
    pub(in crate::codegen) pcntl_async_signals: bool,
    pub(in crate::codegen) pcntl_signal_handlers: bool,
    pub(in crate::codegen) backtrace: bool,
}

impl SharedCodegenState {
    /// Discovers borrowed global addresses once, including every emitted function table.
    pub(in crate::codegen) fn global_has_reference(&self, module: &Module, name: &str) -> bool {
        self.global_reference_names.get_or_init(|| {
            module.functions.iter().chain(&module.class_methods).chain(&module.closures)
                .chain(&module.fiber_wrappers).chain(&module.callback_wrappers)
                .chain(&module.extern_callback_trampolines).chain(&module.runtime_callable_invokers)
                .flat_map(|function| &function.instructions)
                .filter(|inst| inst.op == Op::LoadGlobalRefCell)
                .filter_map(|inst| match inst.immediate {
                    Some(Immediate::GlobalName(id)) => module.data.global_names.get(id.as_raw() as usize).cloned(),
                    _ => None,
                }).collect()
        }).contains(name)
    }

    /// Computes source-body flags once, preserving the existing complete-module predicates.
    pub(in crate::codegen) fn module_runtime_facts(&self, module: &Module) -> ModuleRuntimeFacts {
        *self.module_runtime_facts.get_or_init(|| ModuleRuntimeFacts {
            pcntl_async_signals: module.uses_pcntl_runtime(PcntlRuntime::AsyncSignals),
            pcntl_signal_handlers: module.uses_pcntl_runtime(PcntlRuntime::Signal),
            backtrace: crate::codegen::frame::module_uses_backtrace(module),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::Target;
    use crate::ir::{Builder, Function, Immediate, IrType, Op, Ownership, RuntimeCallTarget, Terminator};
    use crate::types::PhpType;

    /// Creates a metadata-only function with both typed signal operations.
    fn signal_body() -> Function {
        let mut function = Function::new("signal_feature".into(), IrType::Void, PhpType::Void);
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        for target in [PcntlRuntime::AsyncSignals, PcntlRuntime::Signal] {
            builder.emit(Op::RuntimeCall, Vec::new(),
                Some(Immediate::RuntimeCall(RuntimeCallTarget::Pcntl(target))),
                IrType::Void, PhpType::Void, Ownership::NonHeap);
        }
        builder.terminate(Terminator::Return { value: None });
        function
    }

    /// Signal discovery includes all seven emitted function-like tables on every target.
    #[test]
    fn module_runtime_facts_include_every_function_table() {
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for table in 0..7 {
                let mut module = Module::new(Target::parse(target).unwrap());
                let body = signal_body();
                match table {
                    0 => module.functions.push(body),
                    1 => module.class_methods.push(body),
                    2 => module.closures.push(body),
                    3 => module.fiber_wrappers.push(body),
                    4 => module.callback_wrappers.push(body),
                    5 => module.extern_callback_trampolines.push(body),
                    _ => module.runtime_callable_invokers.push(body),
                }
                let shared = SharedCodegenState::default();
                let facts = shared.module_runtime_facts(&module);
                assert!(facts.pcntl_async_signals, "{target}, table {table}");
                assert!(facts.pcntl_signal_handlers, "{target}, table {table}");
                assert_eq!(facts.backtrace, crate::codegen::frame::module_uses_backtrace(&module));
            }
        }
    }

    /// An exception-only module keeps frame readers without enabling signal polls.
    #[test]
    fn module_runtime_facts_preserve_exception_backtraces() {
        let mut module = Module::new(Target::parse("linux-x86_64").unwrap());
        let mut function = Function::new("throw_feature".into(), IrType::Void, PhpType::Void);
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        builder.emit(Op::ThrowError, Vec::new(), None, IrType::Void, PhpType::Void, Ownership::NonHeap);
        builder.terminate(Terminator::Return { value: None });
        module.functions.push(function);
        let shared = SharedCodegenState::default();
        let facts = shared.module_runtime_facts(&module);
        assert!(facts.backtrace);
        assert!(!facts.pcntl_async_signals);
        assert!(!facts.pcntl_signal_handlers);
        // Literal-only wrapper snapshots must reuse the original source-body facts.
        let mut wrapper = module.clone();
        wrapper.data.intern_string("wrapper literal");
        assert_eq!(shared.module_runtime_facts(&wrapper), facts);
    }

    /// Separate module emissions never inherit another program's signal requirements.
    #[test]
    fn module_runtime_facts_are_scoped_to_one_emission() {
        let plain = Module::new(Target::parse("linux-x86_64").unwrap());
        let plain_shared = SharedCodegenState::default();
        assert_eq!(plain_shared.module_runtime_facts(&plain), ModuleRuntimeFacts {
            pcntl_async_signals: false, pcntl_signal_handlers: false, backtrace: false,
        });
        let mut signal = plain.clone();
        signal.functions.push(signal_body());
        let signal_shared = SharedCodegenState::default();
        assert!(signal_shared.module_runtime_facts(&signal).pcntl_async_signals);
        assert!(!plain_shared.module_runtime_facts(&plain).pcntl_async_signals);
    }
}
