//! Purpose:
//! Verifies raw string gettype arguments use logical null classification on every ABI.
//!
//! Called from:
//! - Focused scalar-metadata backend tests.
//!
//! Key details:
//! - Exercises the unboxed argument path directly, independently of frontend Mixed boxing.
//! - String pointers use the target string pair rather than the integer result register.

use super::*;
use crate::codegen::{data_section::DataSection, emit::Emitter, platform::Target, shared_state::SharedCodegenState};

/// Every supported target checks the unboxed string pointer before selecting its PHP type name.
#[test]
fn raw_string_gettype_classifies_logical_null_on_all_targets() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let module = crate::ir::Module::new(target);
        let mut function = crate::codegen::shared_helper::helper_function("raw_string_gettype", PhpType::Void);
        function.params[0].php_type = PhpType::Str;
        function.values[0].php_type = PhpType::Str;
        function.values[0].ir_type = crate::ir::IrType::Str;
        {
            let entry = function.entry;
            let mut builder = crate::ir::Builder::new(&mut function);
            builder.position_at_end(entry);
            builder.emit(Op::RuntimeCall, vec![crate::codegen::shared_helper::helper_value()],
                Some(crate::ir::Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(crate::ir::RuntimeFnId::Gettype))),
                crate::ir::IrType::Str, PhpType::Str, crate::ir::Ownership::Borrowed);
            builder.terminate(crate::ir::Terminator::Return { value: None });
        }
        let instruction = function.instructions[0].clone();
        let layout = crate::codegen::frame::layout_for_function(&function, target, false, false, false);
        let mut emitter = Emitter::new(target);
        let mut data = DataSection::new();
        let mut shared = SharedCodegenState::default();
        {
            let mut ctx = FunctionContext::new(&module, &function, &mut emitter, &mut data,
                &mut shared, layout, false, false, false, None);
            lower_gettype(&mut ctx, &instruction).unwrap();
        }
        let (pointer, _) = abi::string_result_regs(&emitter);
        let scratch = abi::secondary_scratch_reg(&emitter);
        let assembly = emitter.output();
        assert!(assembly.contains(&format!("cmp {pointer}, {scratch}")), "{name}: {assembly}");
        assert!(assembly.contains("gettype_string_null"), "{name}: {assembly}");
        assert!(assembly.contains("gettype_string_done"), "{name}: {assembly}");
    }
}
