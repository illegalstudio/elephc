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
function nativeReferenceOutputStatus(int &$status): void { $status = 0; }
function nativeReferenceOutputSnapshots(
    int &$status, string &$tag, array &$matches
): void {
    $status = 0;
    nativeReferenceOutputStatus($status);
    $tag = 'before';
    preg_match('/(a)/', 'a', $matches);
    preg_match_all('/(a)/', 'aa', $matches);
    openssl_encrypt('payload', 'aes-128-gcm', '0123456789abcdef', 0, '123456789012', $tag);
    openssl_encrypt(data: 'payload', cipher_algo: 'aes-128-gcm',
        passphrase: '0123456789abcdef', options: 0, iv: '123456789012', tag: $tag);
    pcntl_waitpid(-1, $status, 1, $matches);
}
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let supports_pcntl = !target.starts_with("ios-");
        let source = if supports_pcntl { source.to_owned() } else {
            source.replace("    pcntl_waitpid(-1, $status, 1, $matches);", "")
        };
        let module = super::lower_source_at_for_target(&source, std::path::Path::new("main.php"),
            std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
        let function = module.functions.iter().find(|function|
            function.name == "nativeReferenceOutputSnapshots").unwrap();
        let forward = function.instructions.iter().find(|inst| inst.op == Op::Call).unwrap();
        let forwarded = forward.operands[0];
        let ValueDef::Instruction { inst, .. } = function.value(forwarded).unwrap().def
            else { panic!("{target}: forwarded output place"); };
        assert_eq!(function.instruction(inst).unwrap().op, Op::LoadRefCellValue);
        assert_eq!(function.instructions.iter().filter(|inst|
            inst.op == Op::Release && inst.operands == [forwarded]).count(), 1,
            "{target}: source-evaluation cleanup already consumed the forwarded snapshot");
        for (builtin, argument, local, snapshot) in [
            (RuntimeCallTarget::Function(RuntimeFnId::PregMatch), 2, "matches", false),
            (RuntimeCallTarget::Function(RuntimeFnId::PregMatchAll), 2, "matches", false),
            (RuntimeCallTarget::Function(RuntimeFnId::OpensslEncrypt), 5, "tag", true),
            (RuntimeCallTarget::Pcntl(PcntlRuntime::WaitPid), 1, "status", true),
            (RuntimeCallTarget::Pcntl(PcntlRuntime::WaitPid), 3, "matches", false),
        ] {
            if !supports_pcntl && matches!(builtin, RuntimeCallTarget::Pcntl(_)) { continue; }
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
            if snapshot {
                assert_eq!(function.instructions.iter().filter(|inst|
                    inst.op == Op::Release && inst.operands == [call.operands[argument]]).count(), 1,
                    "{target}: builtin cleanup cannot consume an already-rooted reference snapshot twice");
            }
        }
        let encrypts = function.instructions.iter().filter(|inst| inst.op == Op::RuntimeCall
            && matches!(inst.immediate,
                Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(RuntimeFnId::OpensslEncrypt)
                    | RuntimeCallTarget::ProfiledFunction { target: RuntimeFnId::OpensslEncrypt, .. }))))
            .collect::<Vec<_>>();
        assert_eq!(encrypts.len(), 2, "{target}: positional and named last-argument coverage");
        for call in encrypts {
            let snapshot = call.operands[5];
            let ValueDef::Instruction { inst, .. } = function.value(snapshot).unwrap().def
                else { panic!("{target}: last output snapshot"); };
            assert_eq!(function.instruction(inst).unwrap().op, Op::LoadRefCellValue);
            assert_eq!(function.instructions.iter().filter(|inst|
                inst.op == Op::Acquire && inst.operands == [snapshot]).count(), 1,
                "{target}: each incidental output view is protected once");
            assert_eq!(function.instructions.iter().filter(|inst|
                inst.op == Op::Release && inst.operands == [snapshot]).count(), 1,
                "{target}: the last reference snapshot retires its original lease");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: output reference snapshot: {error:?}"));
    }
}
