//! Purpose:
//! Folds `strlen(openssl_encrypt(...))` and a matching `openssl_decrypt(...)` length
//! over literal AES-CBC inputs.
//!
//! Called from:
//! - The EIR fixed-point pass driver after constant locals and call staging are lowered.
//!
//! Key details:
//! - Encryption uses options `0`, so PHP returns base64: PKCS#7 always pads to the next
//!   16-byte block, then the text length is `4 * ceil(padded / 3)`.
//! - Decryption is folded only when the ciphertext local is assigned once from an
//!   `openssl_encrypt()` call with the same cipher, key, options and 16-byte IV, and a
//!   literal plaintext. The round trip then returns that plaintext length.
//! - Unknown ciphers, raw-data options, bad IV lengths, observed ciphertext, and dynamic
//!   inputs keep the crypto bridge.

use crate::ir::{
    DataPool, Function, Immediate, InstId, LocalSlotId, Op, Ownership, RuntimeCallTarget,
    RuntimeFnId, ValueId,
};
use crate::types::PhpType;

use super::driver::IrPass;
use super::iconv_mime_literal_sink::{cleanup_dead_loads, neutralize_call_staging};
use super::rewrite::{defining_instruction, neutralize_to_nop};
use super::string_len_sink::const_local_string;

/// Folds literal AES-CBC encrypt/decrypt length observations.
pub struct OpensslCbcLenSink;

impl IrPass for OpensslCbcLenSink {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "openssl_cbc_len_sink"
    }

    /// Skips functions without an OpenSSL encryption call.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(|inst| {
            matches!(
                direct_target(inst),
                Some(RuntimeFnId::OpensslEncrypt | RuntimeFnId::OpensslDecrypt)
            )
        })
    }

    /// Folds every independently proven encrypt/decrypt length.
    fn run(&self, function: &mut Function, data: &mut DataPool) -> bool {
        let mut changed = false;
        let candidates: Vec<InstId> = function
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| inst.op == Op::StrLen && inst.operands.len() == 1)
            .map(|(raw, _)| InstId::from_raw(raw as u32))
            .collect();
        for candidate in candidates {
            if fold_length(function, data, candidate) {
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

/// Returns a literal `i64` value produced by `ConstI64`.
fn const_i64(function: &Function, value: ValueId) -> Option<i64> {
    let inst = defining_instruction(function, value)?;
    match (inst.op, &inst.immediate) {
        (Op::ConstI64, Some(Immediate::I64(value))) => Some(*value),
        _ => None,
    }
}

/// Returns the literal bytes of an immutable string local.
fn const_local_bytes(function: &Function, data: &DataPool, value: ValueId) -> Option<Vec<u8>> {
    const_local_string(function, data, value)
        .map(|(value, _)| crate::string_bytes::literal_bytes(&value))
}

/// Returns whether the cipher name is AES-CBC.
fn is_aes_cbc(cipher: &[u8]) -> bool {
    matches!(
        cipher.to_ascii_lowercase().as_slice(),
        b"aes-128-cbc" | b"aes-192-cbc" | b"aes-256-cbc"
    )
}

/// Folds one encrypt or matching decrypt length observation.
fn fold_length(function: &mut Function, data: &DataPool, length_id: InstId) -> bool {
    let Some(length) = function.instruction(length_id) else {
        return false;
    };
    let Some(cast_value) = length.operands.first().copied() else {
        return false;
    };
    let Some(cast) = defining_instruction(function, cast_value) else {
        return false;
    };
    if cast.op != Op::Cast || cast.operands.len() != 1 {
        return false;
    }
    let call_value = cast.operands[0];
    let Some(call_id) = defining_id(function, call_value) else {
        return false;
    };
    let Some(call) = function.instruction(call_id) else {
        return false;
    };
    let Some(target) = direct_target(call) else {
        return false;
    };
    if call.operands.len() != 5 {
        return false;
    }
    let Some(cipher) = const_local_bytes(function, data, call.operands[1]) else {
        return false;
    };
    if !is_aes_cbc(&cipher) || const_i64(function, call.operands[3]) != Some(0) {
        return false;
    }
    let Some(iv) = const_local_bytes(function, data, call.operands[4]) else {
        return false;
    };
    if iv.len() != 16 {
        return false;
    }
    let computed = match target {
        RuntimeFnId::OpensslEncrypt => {
            let Some(plaintext) = const_local_bytes(function, data, call.operands[0]) else {
                return false;
            };
            let padded = (plaintext.len() / 16 + 1) * 16;
            ((padded + 2) / 3 * 4) as i64
        }
        RuntimeFnId::OpensslDecrypt => {
            let Some(plaintext_len) = matching_encrypt_plaintext_len(function, data, call) else {
                return false;
            };
            plaintext_len as i64
        }
        _ => return false,
    };
    if count_uses_outside(function, call_value, &[Op::Cast, Op::Release]) > 0
        || count_uses_outside(function, cast_value, &[Op::StrLen, Op::Release]) > 0
    {
        return false;
    }
    let Some(result) = length.result else {
        return false;
    };
    neutralize_call_staging(function, call_id);
    let Some(length) = function.instruction_mut(length_id) else {
        return false;
    };
    length.op = Op::ConstI64;
    length.operands.clear();
    length.immediate = Some(Immediate::I64(computed));
    length.result_type = crate::ir::IrType::I64;
    length.result_php_type = PhpType::Int;
    length.result_ownership = Ownership::NonHeap;
    length.effects = Op::ConstI64.default_effects();
    if let Some(value) = function.value_mut(result) {
        value.ir_type = crate::ir::IrType::I64;
        value.php_type = PhpType::Int;
        value.ownership = Ownership::NonHeap;
    }
    let Some(cast_id) = defining_id(function, cast_value) else {
        return false;
    };
    for id in [call_id, cast_id]
        .into_iter()
        .chain(releases_of(function, call_value))
        .chain(releases_of(function, cast_value))
    {
        if let Some(inst) = function.instruction_mut(id) {
            neutralize_to_nop(inst);
        }
    }
    cleanup_dead_loads(function);
    true
}

/// Returns the plaintext length of the matching encrypt call behind a decrypt ciphertext.
fn matching_encrypt_plaintext_len(
    function: &Function,
    data: &DataPool,
    decrypt: &crate::ir::Instruction,
) -> Option<usize> {
    let ciphertext = decrypt.operands[0];
    let load = through_identity(function, ciphertext)?;
    let cast = defining_instruction(function, load)?;
    let (load_value, load) = if cast.op == Op::Cast && cast.operands.len() == 1 {
        (cast.operands[0], defining_instruction(function, cast.operands[0])?)
    } else {
        (load, cast)
    };
    if load.op != Op::LoadLocal {
        return None;
    }
    let Some(Immediate::LocalSlot(slot)) = load.immediate else {
        return None;
    };
    let stored = single_entry_store(function, slot)?;
    let encrypt_value = through_identity(function, stored)?;
    let Some(encrypt_id) = defining_id(function, encrypt_value) else {
        return None;
    };
    let Some(encrypt) = function.instruction(encrypt_id) else {
        return None;
    };
    if direct_target(encrypt) != Some(RuntimeFnId::OpensslEncrypt) || encrypt.operands.len() != 5 {
        return None;
    }
    if const_i64(function, encrypt.operands[3]) != Some(0) {
        return None;
    }
    let encrypt_cipher = const_local_bytes(function, data, encrypt.operands[1])?;
    let decrypt_cipher = const_local_bytes(function, data, decrypt.operands[1])?;
    let encrypt_key = const_local_bytes(function, data, encrypt.operands[2])?;
    let decrypt_key = const_local_bytes(function, data, decrypt.operands[2])?;
    let encrypt_iv = const_local_bytes(function, data, encrypt.operands[4])?;
    let decrypt_iv = const_local_bytes(function, data, decrypt.operands[4])?;
    if !encrypt_cipher.eq_ignore_ascii_case(&decrypt_cipher)
        || encrypt_key != decrypt_key
        || encrypt_iv != decrypt_iv
        || encrypt_iv.len() != 16
    {
        return None;
    }
    let plaintext = const_local_bytes(function, data, encrypt.operands[0])?;
    let _ = load_value;
    Some(plaintext.len())
}

/// Returns the single entry store value for a plain local, rejecting any mutation or reads.
fn single_entry_store(function: &Function, slot: LocalSlotId) -> Option<ValueId> {
    let entry = function.blocks.first()?;
    let mut stored = None;
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !matches!(inst.immediate, Some(Immediate::LocalSlot(found)) if found == slot) {
            continue;
        }
        match inst.op {
            Op::StoreLocal if inst.operands.len() == 1 => {
                let id = InstId::from_raw(raw as u32);
                if !entry.instructions.contains(&id) || stored.replace(inst.operands[0]).is_some() {
                    return None;
                }
            }
            Op::LoadLocal | Op::ReleaseLocalSlot => {}
            _ => return None,
        }
    }
    stored
}

/// Follows identity-preserving ownership wrappers to the producer value.
fn through_identity(function: &Function, mut value: ValueId) -> Option<ValueId> {
    for _ in 0..8 {
        let inst = defining_instruction(function, value)?;
        if matches!(inst.op, Op::Acquire | Op::MixedClone | Op::Move | Op::Borrow | Op::EnsureOwned)
            && inst.operands.len() == 1
        {
            value = inst.operands[0];
        } else {
            return Some(value);
        }
    }
    None
}

/// Counts uses of `value` whose opcode is not in `allowed`.
fn count_uses_outside(function: &Function, value: ValueId, allowed: &[Op]) -> usize {
    function
        .instructions
        .iter()
        .filter(|inst| inst.operands.contains(&value) && !allowed.contains(&inst.op))
        .count()
}

/// Returns every release of `value`.
fn releases_of(function: &Function, value: ValueId) -> Vec<InstId> {
    function
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(raw, inst)| {
            (inst.op == Op::Release && inst.operands.len() == 1 && inst.operands[0] == value)
                .then_some(InstId::from_raw(raw as u32))
        })
        .collect()
}

/// Returns the defining instruction identifier for one SSA value.
fn defining_id(function: &Function, value: ValueId) -> Option<InstId> {
    let definition = function.value(value)?;
    let crate::ir::ValueDef::Instruction { inst, .. } = definition.def else {
        return None;
    };
    Some(inst)
}
