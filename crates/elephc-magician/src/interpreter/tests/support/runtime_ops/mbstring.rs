//! Purpose:
//! Adapts fake interpreter values to the real mbstring bridge for focused eval tests.
//!
//! Called from:
//! - FakeOps::runtime_builtin_call for mbstring runtime identities.
//!
//! Key details:
//! - Codec algorithms and mutable lookup behavior come from the actual C ABI.
//! - Only fake value materialization and exception ownership are implemented here.

use super::*;
use elephc_builtin_contract::{mbstring_abi::*, RuntimeBuiltinId};
use elephc_builtin_contract::mbstring_abi::array::{ArrayGraph, Key, Value};
use elephc_mbstring::abi::{elephc_mbstring_call_v1, elephc_mbstring_release_v1};

impl FakeOps {
    /// Calls the actual bridge with fake string values, then transfers its result into fake cells.
    pub(super) fn mbstring_builtin_call(&mut self, id: RuntimeBuiltinId, args: &[RuntimeCellHandle]) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
        if !id.supports_arity(args.len()) { return Ok(None); }
        let contract = elephc_builtin_contract::lookup_id(id.builtin_id()).expect("mbstring contract");
        let mut storage = Vec::with_capacity(args.len());
        let mut slots = Vec::with_capacity(args.len());
        for (&value, parameter) in args.iter().zip(contract.params) {
            let scalar = match parameter.ty { elephc_builtin_contract::TypeSpec::Nullable(inner) => *inner, ty => ty };
            let slot = if accepts_argument_kind(parameter.ty, ARG_NULL) && self.is_null(value)? {
                MbArgV1::null()
            } else if accepts_argument_kind(parameter.ty, ARG_ARRAY)
                && matches!(self.get(value), FakeValue::Array(_) | FakeValue::Assoc(_)) {
                storage.push(self.mbstring_array_graph(value)?.encode());
                MbArgV1::array(storage.last().unwrap())
            } else if scalar == elephc_builtin_contract::TypeSpec::Int
                || accepts_argument_kind(parameter.ty, ARG_INT)
                    && matches!(self.get(value), FakeValue::Int(_) | FakeValue::Float(_) | FakeValue::Bool(_)) {
                MbArgV1::integer(self.fake_int(&self.get(value)))
            } else if scalar == elephc_builtin_contract::TypeSpec::Bool {
                MbArgV1::boolean(self.truthy(value)?)
            } else {
                storage.push(self.string_bytes(value)?);
                MbArgV1::string(storage.last().unwrap())
            };
            slots.push(slot);
        }
        let mut result = MbResultV1::default();
        unsafe { elephc_mbstring_call_v1(id.as_u32(), slots.as_ptr(), slots.len() as u64, &mut result); }
        let kind = result.kind;
        let value = result.value;
        let message = unsafe { copy(result.bytes, result.len) };
        let diagnostics = unsafe { copy(result.diagnostics, result.diagnostics_len) };
        unsafe { elephc_mbstring_release_v1(&mut result); }
        if !diagnostics.is_empty() { self.warning(&String::from_utf8_lossy(&diagnostics))?; }
        match kind {
            RESULT_INT => self.int(value).map(Some),
            RESULT_BOOL => self.bool_value(value != 0).map(Some),
            RESULT_STRING => self.string_bytes_value(&message).map(Some),
            RESULT_STRING_ARRAY => {
                let count = usize::try_from(value).map_err(|_| EvalStatus::RuntimeFatal)?;
                let values = decode_string_array(&message, count).ok_or(EvalStatus::RuntimeFatal)?;
                let cells = values.into_iter().map(|bytes| self.string_bytes_value(bytes)).collect::<Result<Vec<_>, _>>()?;
                Ok(Some(self.alloc(FakeValue::Array(cells))))
            }
            RESULT_VALUE_ERROR | RESULT_TYPE_ERROR | RESULT_ERROR => {
                let class = match kind { RESULT_VALUE_ERROR => "ValueError", RESULT_TYPE_ERROR => "TypeError", _ => "Error" };
                let object = self.new_object(class)?;
                let message = self.string_bytes_value(&message)?;
                let code = self.int(0)?;
                self.construct_object(object, vec![message, code])?;
                self.pending_runtime_throwable = Some(object);
                Err(EvalStatus::UncaughtThrowable)
            }
            _ => Err(EvalStatus::RuntimeFatal),
        }
    }

    /// Copies fake array identities and scalar bits into the neutral graph used by real dispatch.
    fn mbstring_array_graph(&self, root: RuntimeCellHandle) -> Result<ArrayGraph, EvalStatus> {
        let mut identities = HashMap::from([(root.as_ptr() as usize, 0)]);
        let mut pending = vec![root];
        let mut arrays = Vec::new();
        while arrays.len() < pending.len() {
            let entries: Vec<_> = match self.get(pending[arrays.len()]) {
                FakeValue::Array(values) => values.into_iter().enumerate()
                    .map(|(index, value)| (Key::Int(index as i64), value)).collect(),
                FakeValue::Assoc(entries) => entries.into_iter().map(|(key, value)| {
                    let key = match key { FakeKey::Int(value) => Key::Int(value),
                        FakeKey::String(value) => Key::String(value) };
                    (key, value)
                }).collect(),
                _ => return Err(EvalStatus::RuntimeFatal),
            };
            let mut output = Vec::with_capacity(entries.len());
            for (key, cell) in entries {
                let value = match self.get(cell) {
                    FakeValue::Null => Value::Null,
                    FakeValue::Bool(value) => Value::Bool(value),
                    FakeValue::Int(value) => Value::Int(value),
                    FakeValue::Float(value) => Value::Float(value.to_bits()),
                    FakeValue::String(value) => Value::String(value.into_bytes()),
                    FakeValue::Bytes(value) => Value::String(value),
                    FakeValue::Array(_) | FakeValue::Assoc(_) => {
                        let index = *identities.entry(cell.as_ptr() as usize).or_insert_with(|| {
                            let index = pending.len(); pending.push(cell); index
                        });
                        Value::Array(index)
                    }
                    _ => Value::Unsupported,
                };
                output.push((key, value));
            }
            arrays.push(output);
        }
        ArrayGraph::new(0, arrays).ok_or(EvalStatus::RuntimeFatal)
    }

}

/// Copies a bridge-owned byte range before returning its storage to Rust.
unsafe fn copy(bytes: *const u8, len: u64) -> Vec<u8> {
    if len == 0 { Vec::new() } else { unsafe { std::slice::from_raw_parts(bytes, len as usize).to_vec() } }
}
