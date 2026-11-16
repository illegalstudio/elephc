//! Purpose:
//! Folds `openssl_cipher_iv_length("literal")` to a boxed integer using the pinned
//! OpenSSL 3.6.4 default-provider table.
//!
//! Called from:
//! - The EIR fixed-point pass driver after constant locals and call staging are lowered.
//!
//! Key details:
//! - The table is the exact 135-name default-provider catalog; the managed
//!   `openssl-crypto` package is pinned, so these lengths are compile-time constants.
//! - Dynamic or unknown names keep the libcrypto bridge, which preserves PHP's
//!   `false`, alias, and case-insensitive behavior.
//! - The replacement keeps the original result value and boxes the `ConstI64` with
//!   `MixedBox`, matching the runtime helper's `int|false` storage contract.

use crate::ir::{
    BlockId, DataPool, Function, Immediate, InstId, IrHeapKind, IrType, Op, Ownership,
    RuntimeCallTarget, RuntimeFnId, Value, ValueDef, ValueId,
};
use crate::types::PhpType;

use super::driver::IrPass;
use super::string_len_sink::const_string_value;

/// Folds literal cipher IV lengths.
pub struct OpensslCipherIvLengthConstSink;

impl IrPass for OpensslCipherIvLengthConstSink {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "openssl_cipher_iv_length_const_sink"
    }

    /// Skips functions without an IV-length call.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(|inst| {
            matches!(direct_target(inst), Some(RuntimeFnId::OpensslCipherIvLength))
        })
    }

    /// Folds every literal cipher whose length is in the pinned table.
    fn run(&self, function: &mut Function, data: &mut DataPool) -> bool {
        let candidates: Vec<InstId> = function
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| {
                matches!(direct_target(inst), Some(RuntimeFnId::OpensslCipherIvLength))
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

/// Folds one literal `openssl_cipher_iv_length()` call.
fn fold(function: &mut Function, data: &DataPool, call_id: InstId) -> bool {
    let Some(call) = function.instruction(call_id) else {
        return false;
    };
    if call.operands.len() != 1 {
        return false;
    }
    let Some(name) = const_string_value(function, data, call.operands[0]) else {
        return false;
    };
    let name = crate::string_bytes::literal_bytes(&name);
    let Some(length) = iv_length(&name) else {
        return false;
    };
    let Some(result) = call.result else {
        return false;
    };
    let Some(ValueDef::Instruction { block, index, .. }) = function.value(result).map(|value| value.def)
    else {
        return false;
    };
    let literal = insert_const_i64(function, block, index as usize, length as i64);
    let Some(call) = function.instruction_mut(call_id) else {
        return false;
    };
    call.op = Op::MixedBox;
    call.operands = vec![literal];
    call.immediate = None;
    call.result_type = IrType::Heap(IrHeapKind::Mixed);
    call.result_php_type = PhpType::Mixed;
    call.result_ownership = Ownership::Owned;
    call.effects = Op::MixedBox.default_effects();
    if let Some(value) = function.value_mut(result) {
        value.ir_type = IrType::Heap(IrHeapKind::Mixed);
        value.php_type = PhpType::Mixed;
        value.ownership = Ownership::Owned;
    }
    true
}

/// Returns the pinned default-provider IV length for a lowercase or uppercase name.
fn iv_length(name: &[u8]) -> Option<usize> {
    if name.is_empty() {
        return None;
    }
    if name.iter().any(|byte| byte.is_ascii_uppercase()) {
        let lowered: Vec<u8> = name.iter().map(|byte| byte.to_ascii_lowercase()).collect();
        return table(&lowered);
    }
    table(name)
}

/// Looks up one lowercase name in the pinned OpenSSL 3.6.4 table.
fn table(name: &[u8]) -> Option<usize> {
    Some(match name {
        b"aes-128-cbc" => 16,
        b"aes-128-cbc-cts" => 16,
        b"aes-128-cbc-hmac-sha1-etm" => 16,
        b"aes-128-cbc-hmac-sha256-etm" => 16,
        b"aes-128-cbc-hmac-sha512-etm" => 16,
        b"aes-128-ccm" => 12,
        b"aes-128-cfb" => 16,
        b"aes-128-cfb1" => 16,
        b"aes-128-cfb8" => 16,
        b"aes-128-ctr" => 16,
        b"aes-128-ecb" => 0,
        b"aes-128-gcm" => 12,
        b"aes-128-gcm-siv" => 12,
        b"aes-128-ocb" => 12,
        b"aes-128-ofb" => 16,
        b"aes-128-siv" => 0,
        b"aes-128-wrap" => 8,
        b"aes-128-wrap-inv" => 8,
        b"aes-128-wrap-pad" => 4,
        b"aes-128-wrap-pad-inv" => 4,
        b"aes-128-xts" => 16,
        b"aes-192-cbc" => 16,
        b"aes-192-cbc-cts" => 16,
        b"aes-192-cbc-hmac-sha1-etm" => 16,
        b"aes-192-cbc-hmac-sha256-etm" => 16,
        b"aes-192-cbc-hmac-sha512-etm" => 16,
        b"aes-192-ccm" => 12,
        b"aes-192-cfb" => 16,
        b"aes-192-cfb1" => 16,
        b"aes-192-cfb8" => 16,
        b"aes-192-ctr" => 16,
        b"aes-192-ecb" => 0,
        b"aes-192-gcm" => 12,
        b"aes-192-gcm-siv" => 12,
        b"aes-192-ocb" => 12,
        b"aes-192-ofb" => 16,
        b"aes-192-siv" => 0,
        b"aes-192-wrap" => 8,
        b"aes-192-wrap-inv" => 8,
        b"aes-192-wrap-pad" => 4,
        b"aes-192-wrap-pad-inv" => 4,
        b"aes-256-cbc" => 16,
        b"aes-256-cbc-cts" => 16,
        b"aes-256-cbc-hmac-sha1-etm" => 16,
        b"aes-256-cbc-hmac-sha256-etm" => 16,
        b"aes-256-cbc-hmac-sha512-etm" => 16,
        b"aes-256-ccm" => 12,
        b"aes-256-cfb" => 16,
        b"aes-256-cfb1" => 16,
        b"aes-256-cfb8" => 16,
        b"aes-256-ctr" => 16,
        b"aes-256-ecb" => 0,
        b"aes-256-gcm" => 12,
        b"aes-256-gcm-siv" => 12,
        b"aes-256-ocb" => 12,
        b"aes-256-ofb" => 16,
        b"aes-256-siv" => 0,
        b"aes-256-wrap" => 8,
        b"aes-256-wrap-inv" => 8,
        b"aes-256-wrap-pad" => 4,
        b"aes-256-wrap-pad-inv" => 4,
        b"aes-256-xts" => 16,
        b"aria-128-cbc" => 16,
        b"aria-128-ccm" => 12,
        b"aria-128-cfb" => 16,
        b"aria-128-cfb1" => 16,
        b"aria-128-cfb8" => 16,
        b"aria-128-ctr" => 16,
        b"aria-128-ecb" => 0,
        b"aria-128-gcm" => 12,
        b"aria-128-ofb" => 16,
        b"aria-192-cbc" => 16,
        b"aria-192-ccm" => 12,
        b"aria-192-cfb" => 16,
        b"aria-192-cfb1" => 16,
        b"aria-192-cfb8" => 16,
        b"aria-192-ctr" => 16,
        b"aria-192-ecb" => 0,
        b"aria-192-gcm" => 12,
        b"aria-192-ofb" => 16,
        b"aria-256-cbc" => 16,
        b"aria-256-ccm" => 12,
        b"aria-256-cfb" => 16,
        b"aria-256-cfb1" => 16,
        b"aria-256-cfb8" => 16,
        b"aria-256-ctr" => 16,
        b"aria-256-ecb" => 0,
        b"aria-256-gcm" => 12,
        b"aria-256-ofb" => 16,
        b"camellia-128-cbc" => 16,
        b"camellia-128-cbc-cts" => 16,
        b"camellia-128-cfb" => 16,
        b"camellia-128-cfb1" => 16,
        b"camellia-128-cfb8" => 16,
        b"camellia-128-ctr" => 16,
        b"camellia-128-ecb" => 0,
        b"camellia-128-ofb" => 16,
        b"camellia-192-cbc" => 16,
        b"camellia-192-cbc-cts" => 16,
        b"camellia-192-cfb" => 16,
        b"camellia-192-cfb1" => 16,
        b"camellia-192-cfb8" => 16,
        b"camellia-192-ctr" => 16,
        b"camellia-192-ecb" => 0,
        b"camellia-192-ofb" => 16,
        b"camellia-256-cbc" => 16,
        b"camellia-256-cbc-cts" => 16,
        b"camellia-256-cfb" => 16,
        b"camellia-256-cfb1" => 16,
        b"camellia-256-cfb8" => 16,
        b"camellia-256-ctr" => 16,
        b"camellia-256-ecb" => 0,
        b"camellia-256-ofb" => 16,
        b"chacha20" => 16,
        b"chacha20-poly1305" => 12,
        b"des-ede-cbc" => 8,
        b"des-ede-cfb" => 8,
        b"des-ede-ecb" => 0,
        b"des-ede-ofb" => 8,
        b"des-ede3-cbc" => 8,
        b"des-ede3-cfb" => 8,
        b"des-ede3-cfb1" => 8,
        b"des-ede3-cfb8" => 8,
        b"des-ede3-ecb" => 0,
        b"des-ede3-ofb" => 8,
        b"des3-wrap" => 0,
        b"null" => 0,
        b"sm4-cbc" => 16,
        b"sm4-ccm" => 12,
        b"sm4-cfb" => 16,
        b"sm4-ctr" => 16,
        b"sm4-ecb" => 0,
        b"sm4-gcm" => 12,
        b"sm4-ofb" => 16,
        b"sm4-xts" => 16,
        _ => return None,
    })
}

/// Inserts a `ConstI64` immediately before `position` in `block`.
fn insert_const_i64(function: &mut Function, block: BlockId, position: usize, value: i64) -> ValueId {
    let inst_id = InstId::from_raw(function.instructions.len() as u32);
    let result = ValueId::from_raw(function.values.len() as u32);
    function.values.push(Value {
        ir_type: IrType::I64,
        php_type: PhpType::Int,
        def: ValueDef::Instruction { block, index: position as u32, inst: inst_id },
        ownership: Ownership::NonHeap,
    });
    let mut inst = crate::ir::Instruction::new(
        Op::ConstI64,
        Vec::new(),
        Some(Immediate::I64(value)),
        Some(result),
        IrType::I64,
        PhpType::Int,
        Ownership::NonHeap,
        Op::ConstI64.default_effects(),
        None,
    );
    inst.origin = Some(crate::ir::PassOrigin::ConstFold);
    function.instructions.push(inst);
    function.blocks[block.as_raw() as usize].instructions.insert(position, inst_id);
    let shifted: Vec<InstId> =
        function.blocks[block.as_raw() as usize].instructions[position + 1..].to_vec();
    for (offset, shifted_id) in shifted.into_iter().enumerate() {
        if let Some(value) = function.instructions[shifted_id.as_raw() as usize].result {
            function.values[value.as_raw() as usize].def = ValueDef::Instruction {
                block,
                index: (position + 1 + offset) as u32,
                inst: shifted_id,
            };
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins representative table entries across block, stream, AEAD, wrap, and XTS modes.
    #[test]
    fn pinned_table_matches_php_catalog() {
        assert_eq!(iv_length(b"aes-128-cbc"), Some(16));
        assert_eq!(iv_length(b"AES-256-GCM"), Some(12));
        assert_eq!(iv_length(b"aes-192-ecb"), Some(0));
        assert_eq!(iv_length(b"aes-128-wrap"), Some(8));
        assert_eq!(iv_length(b"aes-128-xts"), Some(16));
        assert_eq!(iv_length(b"aria-128-ccm"), Some(12));
        assert_eq!(iv_length(b"camellia-256-cbc-cfb"), None);
        assert_eq!(iv_length(b"not-a-cipher"), None);
    }
}
