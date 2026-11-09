//! Purpose:
//! Checks the complete host register contract around generated web handlers.
//!
//! Called from:
//! - `cargo test --lib` through `crate::codegen::frame`.
//!
//! Key details:
//! - Every target and both register-allocation modes must preserve host state.
//! - Request reset can call runtime helpers before the PHP body starts.

use super::*;
use crate::codegen::{generate_user_asm_from_ir_with_options, Emit, Instrumentation, WebIsolation};
use crate::ir::{Builder, IrType};

/// Host registers are saved before reset and restored after cleanup on every target.
#[test]
fn web_handler_preserves_complete_host_abi_before_request_reset() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        for regalloc in [false, true] {
            let mut module = Module::new(target);
            let mut main = Function::new("main".into(), IrType::Void, PhpType::Void);
            main.flags.is_main = true;
            {
                let mut builder = Builder::new(&mut main);
                let entry = builder.create_named_block("entry", Vec::new());
                builder.set_entry(entry);
                builder.position_at_end(entry);
                builder.terminate(Terminator::Return { value: None });
            }
            module.add_function(main);
            let asm = generate_user_asm_from_ir_with_options(
                &module, false, false, Instrumentation::Off, false, false, false,
                Emit::Executable, &HashMap::default(), regalloc, true, WebIsolation::Pool, None,
            ).unwrap();
            let asm = asm.into_string();
            let handler = asm.split_once("_elephc_web_handler:\n").unwrap().1;
            let (before_reset, after_reset) = handler.split_once("__rt_web_reset").unwrap();
            let before_return = after_reset.lines().take_while(|line| line.trim() != "ret")
                .collect::<Vec<_>>().join("\n");
            let registers: &[&str] = match target.arch {
                Arch::X86_64 => &["rbx", "r12", "r13", "r14", "r15"],
                Arch::AArch64 => &["x19", "x20", "x21", "x22", "x23", "x24", "x25",
                    "x26", "x27", "x28", "d8", "d9", "d10", "d11", "d12", "d13", "d14", "d15"],
            };
            for register in registers {
                let (saved, restored) = match target.arch {
                    Arch::X86_64 => (format!("], {register}"), format!("mov {register}, QWORD PTR")),
                    Arch::AArch64 => (format!("stur {register},"), format!("ldur {register},")),
                };
                assert!(before_reset.contains(&saved), "{name}, regalloc={regalloc}: missing {register} save before reset:\n{handler}");
                assert!(before_return.contains(&restored), "{name}, regalloc={regalloc}: missing {register} restore before return:\n{handler}");
            }
        }
    }
}
