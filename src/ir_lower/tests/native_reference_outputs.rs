//! Purpose:
//! Keeps output builtins addressable after native reference value views become boxed.
//!
//! Called from:
//! - The supported-target EIR and emitter regression tests in CI.
//!
//! Key details:
//! - An owned value snapshot still identifies the caller's original output place.
//! - PCNTL optional output must never be silently discarded as an omitted argument.

use crate::ir::{Immediate, Op, PcntlRuntime, RuntimeCallTarget, RuntimeFnId, ValueDef};

/// Regex, OpenSSL and PCNTL recover original places from owned native reference snapshots.
#[test]
fn native_reference_outputs_after_body_writes_remain_addressable_on_every_target() {
    let source = r#"<?php
function nativeReferenceOutputSnapshots(
    int &$status, string &$tag, array &$matches
): void {
    $status = 0;
    $tag = 'before';
    preg_match('/(a)/', 'a', $matches);
    preg_match_all('/(a)/', 'aa', $matches);
    openssl_encrypt('payload', 'aes-128-gcm', '0123456789abcdef', 0, '123456789012', $tag);
    pcntl_waitpid(-1, $status, 1, $matches);
}
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
            std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
        let function = module.functions.iter().find(|function|
            function.name == "nativeReferenceOutputSnapshots").unwrap();
        for (builtin, argument, local, snapshot) in [
            (RuntimeCallTarget::Function(RuntimeFnId::PregMatch), 2, "matches", false),
            (RuntimeCallTarget::Function(RuntimeFnId::PregMatchAll), 2, "matches", false),
            (RuntimeCallTarget::Function(RuntimeFnId::OpensslEncrypt), 5, "tag", true),
            (RuntimeCallTarget::Pcntl(PcntlRuntime::WaitPid), 1, "status", true),
            (RuntimeCallTarget::Pcntl(PcntlRuntime::WaitPid), 3, "matches", false),
        ] {
            let call = function.instructions.iter().find(|inst| inst.op == Op::RuntimeCall
                && match inst.immediate {
                    Some(Immediate::RuntimeCall(RuntimeCallTarget::ProfiledFunction { target, .. })) =>
                        builtin == RuntimeCallTarget::Function(target),
                    Some(Immediate::RuntimeCall(target)) => builtin == target,
                    _ => false,
                }).unwrap();
            let value = function.value(call.operands[argument]).unwrap();
            let ValueDef::Instruction { inst, .. } = value.def else { panic!("{target}: output load"); };
            let load = function.instruction(inst).unwrap();
            assert_eq!(load.op, if snapshot { Op::LoadRefCellValue } else { Op::LoadRefCell },
                "{target}: {builtin:?} {local}");
            let Some(Immediate::LocalSlot(slot)) = load.immediate else { panic!("{target}: original place"); };
            assert_eq!(function.locals[slot.as_raw() as usize].name.as_deref(), Some(local));
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: output reference snapshot: {error:?}"));
    }
}
