//! Purpose:
//! Validates borrowed wire arguments and resolves neutral-contract defaults.
//!
//! Called from:
//! - The versioned bridge before operation-specific validation and encoding lookup.
//!
//! Key details:
//! - Missing arguments use contract defaults; explicit null remains distinguishable.
//! - Byte slices borrow only metadata validated at the unsafe C ABI boundary.

use super::*;
use elephc_builtin_contract::{BuiltinContract, DefaultSpec};
use elephc_builtin_contract::mbstring_abi::array::ArrayGraph;

/// Validated borrowed arguments with their authoritative PHP parameter contract.
pub(super) struct Arguments<'a> {
    pub contract: &'static BuiltinContract,
    slots: &'a [MbArgV1],
    arrays: Vec<Option<ArrayGraph>>,
}

impl<'a> Arguments<'a> {
    /// Rejects invalid wire tags or byte ranges before any PHP operation observes them.
    pub unsafe fn new(operation: RuntimeBuiltinId, slots: &'a [MbArgV1]) -> Option<Self> {
        let contract = elephc_builtin_contract::lookup_id(operation.builtin_id())?;
        let mut arrays = Vec::with_capacity(slots.len());
        for (slot, parameter) in slots.iter().zip(contract.params) {
            if !accepts_argument_kind(parameter.ty, slot.kind) { return None; }
            let array = match slot.kind {
                ARG_NULL | ARG_INT => None,
                ARG_BOOL if (0..=1).contains(&slot.value) => None,
                ARG_STRING => { unsafe { bytes(slot) }?; None }
                ARG_ARRAY if slot.value & !ARRAY_ENCODING_CATALOG == 0 => Some(ArrayGraph::decode(unsafe { bytes(slot) }?)?),
                _ => return None,
            };
            arrays.push(array);
        }
        Some(Self { contract, slots, arrays })
    }

    /// Distinguishes omitted optional parameters from explicitly supplied default values.
    pub fn supplied(&self, index: usize) -> bool { index < self.slots.len() }

    /// Reads the host-proven cached catalog identity independently of array contents.
    pub fn encoding_catalog(&self, index: usize) -> bool {
        self.slots.get(index).is_some_and(|slot| slot.kind == ARG_ARRAY && slot.value & ARRAY_ENCODING_CATALOG != 0)
    }

    /// Borrows an already decoded graph without repeating its structural validation.
    pub fn array(&self, index: usize) -> Option<&ArrayGraph> {
        self.arrays.get(index).and_then(Option::as_ref)
    }

    /// Copies a canonical encoding list after the host has completed every element conversion.
    pub fn encoding_names(&self, index: usize) -> Option<Vec<Vec<u8>>> {
        use elephc_builtin_contract::mbstring_abi::array::Value;
        let graph = self.array(index)?;
        graph.arrays()[graph.root()].iter().map(|(_, value)| match value {
            Value::String(bytes) => Some(bytes.clone()), _ => None,
        }).collect()
    }

    /// Distinguishes an explicit array/string/null and resolves an omitted null default.
    pub fn kind(&self, index: usize) -> u64 {
        self.slots.get(index).map_or_else(|| {
            assert_eq!(self.contract.params[index].default, Some(DefaultSpec::Null));
            ARG_NULL
        }, |slot| slot.kind)
    }

    /// Reads a required or defaulted string after the wire metadata has been validated.
    pub fn string(&self, index: usize) -> &'a [u8] {
        self.nullable_string(index).expect("required string argument")
    }

    /// Preserves explicit null and obtains omitted string defaults from the shared contract.
    pub fn nullable_string(&self, index: usize) -> Option<&'a [u8]> {
        match self.slots.get(index) {
            Some(slot) if slot.kind == ARG_NULL => None,
            Some(slot) if slot.kind == ARG_STRING => unsafe { bytes(slot) },
            Some(_) => None,
            None => match self.contract.params[index].default {
                Some(DefaultSpec::Str(value)) => Some(value.as_bytes()),
                Some(DefaultSpec::Null) => None,
                _ => panic!("missing string default"),
            },
        }
    }

    /// Reads a required or defaulted integer without treating explicit null as zero.
    pub fn integer(&self, index: usize) -> i64 {
        self.nullable_integer(index).expect("required integer argument")
    }

    /// Preserves explicit null and obtains omitted integer defaults from the shared contract.
    pub fn nullable_integer(&self, index: usize) -> Option<i64> {
        match self.slots.get(index) {
            Some(slot) if slot.kind == ARG_NULL => None,
            Some(slot) => Some(slot.value),
            None => match self.contract.params[index].default {
                Some(DefaultSpec::Int(value)) => Some(value),
                Some(DefaultSpec::Null) => None,
                _ => panic!("missing integer default"),
            },
        }
    }

    /// Reads a validated boolean or its omitted default from the shared contract.
    pub fn boolean(&self, index: usize) -> bool {
        match self.slots.get(index) {
            Some(slot) => slot.value != 0,
            None => match self.contract.params[index].default {
                Some(DefaultSpec::Bool(value)) => value,
                _ => panic!("missing boolean default"),
            },
        }
    }
}
