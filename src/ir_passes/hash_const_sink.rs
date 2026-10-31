//! Purpose:
//! Folds `hash("sha256", $literal)` and `hash_hmac("sha256", $literal, $key)` to the
//! lowercase hexadecimal digest at compile time.
//!
//! Called from:
//! - The EIR fixed-point pass driver after constant locals are available.
//!
//! Key details:
//! - Only the SHA-256 algorithm is folded; every other algorithm, dynamic input, or mutable
//!   local stays on the crypto bridge.
//! - The digest is interned as a persistent string, so a later `strlen()` folds and the loop
//!   no longer enters the bridge.
//! - HMAC uses SHA-256's 64-byte block with the standard inner/outer pads.

use sha2::{Digest, Sha256};

use crate::ir::{DataPool, Function, Immediate, InstId, IrType, Op, Ownership, RuntimeCallTarget, RuntimeFnId};
use crate::types::PhpType;

use super::driver::IrPass;
use super::string_len_sink::{const_local_string, const_string_value};

/// Folds literal SHA-256 and HMAC-SHA-256 calls.
pub struct HashConstSink;

impl IrPass for HashConstSink {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "hash_const_sink"
    }

    /// Skips functions without a hash call.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(|inst| {
            matches!(
                direct_target(inst),
                Some(RuntimeFnId::Hash | RuntimeFnId::HashHmac)
            )
        })
    }

    /// Folds every literal SHA-256 call.
    fn run(&self, function: &mut Function, data: &mut DataPool) -> bool {
        let candidates: Vec<InstId> = function
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| {
                matches!(
                    direct_target(inst),
                    Some(RuntimeFnId::Hash | RuntimeFnId::HashHmac)
                )
            })
            .map(|(raw, _)| InstId::from_raw(raw as u32))
            .collect();
        let mut changed = false;
        for candidate in candidates {
            if fold(function, data, candidate) {
                changed = true;
            }
        }
        changed
    }
}

/// Returns the direct or profiled runtime target for one call.
fn direct_target(inst: &crate::ir::Instruction) -> Option<RuntimeFnId> {
    match inst.immediate {
        Some(Immediate::RuntimeCall(
            RuntimeCallTarget::Function(target)
                | RuntimeCallTarget::ProfiledFunction { target, .. },
        )) => Some(target),
        _ => None,
    }
}

/// Folds one literal hash call when the algorithm is SHA-256.
fn fold(function: &mut Function, data: &mut DataPool, call_id: InstId) -> bool {
    let Some(call) = function.instruction(call_id) else {
        return false;
    };
    let Some(target) = direct_target(call) else {
        return false;
    };
    let (data_operand, key_operand) = match (target, call.operands.len()) {
        (RuntimeFnId::Hash, 2) => (call.operands[1], None),
        (RuntimeFnId::HashHmac, 3) => (call.operands[1], Some(call.operands[2])),
        _ => return false,
    };
    let Some(algorithm) = const_string_value(function, data, call.operands[0]) else {
        return false;
    };
    if !algorithm.eq_ignore_ascii_case("sha256") {
        return false;
    }
    let Some((subject, _)) = const_local_string(function, data, data_operand) else {
        return false;
    };
    let subject = crate::string_bytes::literal_bytes(&subject);
    let digest = if let Some(key_operand) = key_operand {
        let Some((key, _)) = const_local_string(function, data, key_operand) else {
            return false;
        };
        hmac_sha256(&crate::string_bytes::literal_bytes(&key), &subject)
    } else {
        Sha256::digest(&subject).as_slice().to_vec()
    };
    let hex = digest.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
    let data_id = data.intern_string(&hex);
    let Some(result) = call.result else {
        return false;
    };
    let Some(call) = function.instruction_mut(call_id) else {
        return false;
    };
    call.op = Op::ConstStr;
    call.operands.clear();
    call.immediate = Some(Immediate::Data(data_id));
    call.result_type = IrType::Str;
    call.result_php_type = PhpType::Str;
    call.result_ownership = Ownership::Persistent;
    call.effects = Op::ConstStr.default_effects();
    if let Some(value) = function.value_mut(result) {
        value.ir_type = IrType::Str;
        value.php_type = PhpType::Str;
        value.ownership = Ownership::Persistent;
    }
    true
}

/// Computes HMAC-SHA-256 over `data` with `key`.
fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut block = [0u8; 64];
    if key.len() > block.len() {
        block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    for byte in block {
        inner.update([byte ^ 0x36]);
    }
    inner.update(data);
    let inner = inner.finalize();
    let mut outer = Sha256::new();
    for byte in block {
        outer.update([byte ^ 0x5c]);
    }
    outer.update(inner);
    outer.finalize().as_slice().to_vec()
}
