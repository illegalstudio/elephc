//! Purpose:
//! Interpreter unit tests for EvalIR execution against fake runtime values.
//! The tests exercise scope mutation, builtins, function calls, arrays, objects,
//! control flow, and error propagation without linking generated runtime hooks.
//!
//! Called from:
//! - `cargo test -p elephc-eval` through Rust's test harness.
//!
//! Key details:
//! - FakeOps owns opaque test cells and mirrors enough runtime behavior for eval.
//! - Test fixtures parse PHP eval fragments before executing them through EvalIR.

use std::collections::HashMap;
use std::ffi::c_void;

use crate::parser::parse_fragment;
use crate::value::RuntimeCell;

use super::*;

/// Test-only array key representation for fake indexed and associative arrays.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum FakeKey {
    Int(i64),
    String(String),
}

/// Test-only runtime value representation used behind opaque cell handles.
#[derive(Clone, Debug, PartialEq)]
enum FakeValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    Array(Vec<RuntimeCellHandle>),
    Assoc(Vec<(FakeKey, RuntimeCellHandle)>),
    Object(Vec<(String, RuntimeCellHandle)>),
    Iterator { len: i64, position: i64 },
    Resource(i64),
}

/// Test runtime hooks that allocate stable fake handles and record echo output.
#[derive(Default)]
struct FakeOps {
    next_id: usize,
    values: HashMap<usize, FakeValue>,
    output: String,
    releases: Vec<RuntimeCellHandle>,
    warnings: Vec<String>,
}

impl FakeOps {
    /// Allocates one fake runtime cell and returns its opaque handle.
    fn alloc(&mut self, value: FakeValue) -> RuntimeCellHandle {
        self.next_id += 1;
        let id = self.next_id;
        self.values.insert(id, value);
        RuntimeCellHandle::from_raw(id as *mut RuntimeCell)
    }

    /// Reads a fake runtime cell by opaque handle.
    fn get(&self, handle: RuntimeCellHandle) -> FakeValue {
        let id = handle.as_ptr() as usize;
        self.values.get(&id).cloned().expect("fake cell missing")
    }

    /// Converts a fake runtime cell into a normalized fake PHP array key.
    fn key(&self, handle: RuntimeCellHandle) -> Result<FakeKey, EvalStatus> {
        let value = self.get(handle);
        match value {
            FakeValue::Int(value) => Ok(FakeKey::Int(value)),
            FakeValue::String(value) => eval_numeric_string_array_key(value.as_bytes())
                .map(FakeKey::Int)
                .map_or_else(|| Ok(FakeKey::String(value)), Ok),
            FakeValue::Bytes(value) => eval_numeric_string_array_key(&value)
                .map(FakeKey::Int)
                .map_or_else(
                    || {
                        Ok(FakeKey::String(
                            String::from_utf8_lossy(&value).into_owned(),
                        ))
                    },
                    Ok,
                ),
            FakeValue::Null => Ok(FakeKey::String(String::new())),
            value => Ok(FakeKey::Int(self.fake_int(&value))),
        }
    }

    /// Allocates a fake runtime cell for an existing PHP array key.
    fn alloc_key(&mut self, key: &FakeKey) -> Result<RuntimeCellHandle, EvalStatus> {
        match key {
            FakeKey::Int(value) => self.int(*value),
            FakeKey::String(value) => self.string(value),
        }
    }

    /// Finds a fake object property by insertion-order name.
    fn object_property(
        properties: &[(String, RuntimeCellHandle)],
        name: &str,
    ) -> Option<RuntimeCellHandle> {
        properties
            .iter()
            .find_map(|(property, value)| (property == name).then_some(*value))
    }
}

impl RuntimeValueOps for FakeOps {
    /// Creates a fake indexed array cell.
    fn array_new(&mut self, capacity: usize) -> Result<RuntimeCellHandle, EvalStatus> {
        Ok(self.alloc(FakeValue::Array(Vec::with_capacity(capacity))))
    }

    /// Creates a fake associative array cell.
    fn assoc_new(&mut self, _capacity: usize) -> Result<RuntimeCellHandle, EvalStatus> {
        Ok(self.alloc(FakeValue::Assoc(Vec::new())))
    }

    /// Reads one fake indexed array element.
    fn array_get(
        &mut self,
        array: RuntimeCellHandle,
        index: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let key = self.key(index)?;
        match self.get(array) {
            FakeValue::Array(elements) => {
                let FakeKey::Int(index) = key else {
                    return self.null();
                };
                if index < 0 {
                    return self.null();
                }
                elements
                    .get(index as usize)
                    .copied()
                    .map_or_else(|| self.null(), Ok)
            }
            FakeValue::Assoc(entries) => entries
                .iter()
                .find_map(|(entry_key, value)| (entry_key == &key).then_some(*value))
                .map_or_else(|| self.null(), Ok),
            _ => self.null(),
        }
    }

    /// Checks whether a fake array has the requested key without reading its value.
    fn array_key_exists(
        &mut self,
        key: RuntimeCellHandle,
        array: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let key = self.key(key)?;
        let exists = match self.get(array) {
            FakeValue::Array(elements) => {
                matches!(key, FakeKey::Int(index) if index >= 0 && (index as usize) < elements.len())
            }
            FakeValue::Assoc(entries) => entries.iter().any(|(entry_key, _)| entry_key == &key),
            _ => return Err(EvalStatus::UnsupportedConstruct),
        };
        self.bool_value(exists)
    }

    /// Returns one fake foreach key by insertion-order position.
    fn array_iter_key(
        &mut self,
        array: RuntimeCellHandle,
        position: usize,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        match self.get(array) {
            FakeValue::Array(elements) if position < elements.len() => self.int(position as i64),
            FakeValue::Assoc(entries) => {
                let Some((key, _)) = entries.get(position) else {
                    return self.null();
                };
                self.alloc_key(key)
            }
            FakeValue::Array(_) => self.null(),
            _ => Err(EvalStatus::UnsupportedConstruct),
        }
    }

    /// Writes one fake indexed or associative array element.
    fn array_set(
        &mut self,
        array: RuntimeCellHandle,
        index: RuntimeCellHandle,
        value: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let key = self.key(index)?;
        let id = array.as_ptr() as usize;
        match self.values.get_mut(&id) {
            Some(FakeValue::Array(elements)) => {
                let FakeKey::Int(index) = key else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                if index < 0 {
                    return Err(EvalStatus::UnsupportedConstruct);
                }
                let index = index as usize;
                while elements.len() <= index {
                    elements.push(RuntimeCellHandle::from_raw(std::ptr::null_mut()));
                }
                elements[index] = value;
            }
            Some(FakeValue::Assoc(entries)) => {
                if let Some((_, existing_value)) =
                    entries.iter_mut().find(|(entry_key, _)| entry_key == &key)
                {
                    *existing_value = value;
                } else {
                    entries.push((key, value));
                }
            }
            _ => return Err(EvalStatus::UnsupportedConstruct),
        }
        Ok(array)
    }

    /// Reads one fake object property by name.
    fn property_get(
        &mut self,
        object: RuntimeCellHandle,
        property: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        match self.get(object) {
            FakeValue::Object(properties) => properties
                .iter()
                .find_map(|(name, value)| (name == property).then_some(*value))
                .map_or_else(|| self.null(), Ok),
            _ => Err(EvalStatus::UnsupportedConstruct),
        }
    }

    /// Writes one fake object property by name.
    fn property_set(
        &mut self,
        object: RuntimeCellHandle,
        property: &str,
        value: RuntimeCellHandle,
    ) -> Result<(), EvalStatus> {
        let id = object.as_ptr() as usize;
        let Some(FakeValue::Object(properties)) = self.values.get_mut(&id) else {
            return Err(EvalStatus::UnsupportedConstruct);
        };
        if let Some((_, existing_value)) = properties.iter_mut().find(|(name, _)| name == property)
        {
            *existing_value = value;
        } else {
            properties.push((property.to_string(), value));
        }
        Ok(())
    }

    /// Returns the number of fake object properties in insertion order.
    fn object_property_len(&mut self, object: RuntimeCellHandle) -> Result<usize, EvalStatus> {
        match self.get(object) {
            FakeValue::Object(properties) => Ok(properties.len()),
            FakeValue::Iterator { .. } => Ok(0),
            _ => Err(EvalStatus::UnsupportedConstruct),
        }
    }

    /// Returns one fake object property key by insertion-order position.
    fn object_property_iter_key(
        &mut self,
        object: RuntimeCellHandle,
        position: usize,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        match self.get(object) {
            FakeValue::Object(properties) => {
                let Some((name, _)) = properties.get(position) else {
                    return self.null();
                };
                self.string(name)
            }
            _ => Err(EvalStatus::UnsupportedConstruct),
        }
    }

    /// Calls one fake object method by name.
    fn method_call(
        &mut self,
        object: RuntimeCellHandle,
        method: &str,
        args: Vec<RuntimeCellHandle>,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        match (self.get(object), method) {
            (FakeValue::Iterator { .. }, "rewind") if args.is_empty() => {
                let id = object.as_ptr() as usize;
                let Some(FakeValue::Iterator { position, .. }) = self.values.get_mut(&id) else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                *position = 0;
                self.null()
            }
            (FakeValue::Iterator { len, position }, "valid") if args.is_empty() => {
                self.bool_value(position < len)
            }
            (FakeValue::Iterator { .. }, "next") if args.is_empty() => {
                let id = object.as_ptr() as usize;
                let Some(FakeValue::Iterator { position, .. }) = self.values.get_mut(&id) else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                *position += 1;
                self.null()
            }
            (FakeValue::Object(_), "answer") if args.is_empty() => self.int(42),
            (FakeValue::Object(properties), "read_x") => {
                if !args.is_empty() {
                    return Err(EvalStatus::UnsupportedConstruct);
                }
                Self::object_property(&properties, "x").map_or_else(|| self.null(), Ok)
            }
            (FakeValue::Object(properties), "add_x") => {
                let [arg] = args.as_slice() else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                let x = Self::object_property(&properties, "x").ok_or(EvalStatus::RuntimeFatal)?;
                let FakeValue::Int(x) = self.get(x) else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                let FakeValue::Int(arg) = self.get(*arg) else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                self.int(x + arg)
            }
            (FakeValue::Object(properties), "add2_x") => {
                let [left, right] = args.as_slice() else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                let x = Self::object_property(&properties, "x").ok_or(EvalStatus::RuntimeFatal)?;
                let FakeValue::Int(x) = self.get(x) else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                let FakeValue::Int(left) = self.get(*left) else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                let FakeValue::Int(right) = self.get(*right) else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                self.int(x + left + right)
            }
            _ => Err(EvalStatus::UnsupportedConstruct),
        }
    }

    /// Creates one fake object for eval `new` unit tests.
    fn new_object(&mut self, _class_name: &str) -> Result<RuntimeCellHandle, EvalStatus> {
        Ok(self.alloc(FakeValue::Object(Vec::new())))
    }

    /// Applies fake constructor side effects for eval `new` unit tests.
    fn construct_object(
        &mut self,
        object: RuntimeCellHandle,
        args: Vec<RuntimeCellHandle>,
    ) -> Result<(), EvalStatus> {
        let id = object.as_ptr() as usize;
        let Some(FakeValue::Object(properties)) = self.values.get_mut(&id) else {
            return Err(EvalStatus::UnsupportedConstruct);
        };
        if let Some(first) = args.first().copied() {
            if let Some((_, value)) = properties.iter_mut().find(|(name, _)| name == "x") {
                *value = first;
            } else {
                properties.push(("x".to_string(), first));
            }
        }
        Ok(())
    }

    /// Reports one fake AOT class for eval `class_exists` unit tests.
    fn class_exists(&mut self, name: &str) -> Result<bool, EvalStatus> {
        Ok(name.eq_ignore_ascii_case("KnownClass"))
    }

    /// Reports one fake AOT interface for eval `interface_exists` unit tests.
    fn interface_exists(&mut self, name: &str) -> Result<bool, EvalStatus> {
        Ok(name.eq_ignore_ascii_case("KnownInterface"))
    }

    /// Reports one fake AOT trait for eval `trait_exists` unit tests.
    fn trait_exists(&mut self, name: &str) -> Result<bool, EvalStatus> {
        Ok(name.eq_ignore_ascii_case("KnownTrait"))
    }

    /// Reports one fake AOT enum for eval `enum_exists` unit tests.
    fn enum_exists(&mut self, name: &str) -> Result<bool, EvalStatus> {
        Ok(name.eq_ignore_ascii_case("KnownEnum"))
    }

    /// Reports fake class relations for eval `is_a` and `is_subclass_of` unit tests.
    fn object_is_a(
        &mut self,
        object_or_class: RuntimeCellHandle,
        target_class: &str,
        exclude_self: bool,
    ) -> Result<bool, EvalStatus> {
        match self.get(object_or_class) {
            FakeValue::Object(_)
                if target_class.eq_ignore_ascii_case("Exception")
                    || target_class.eq_ignore_ascii_case("Throwable") =>
            {
                Ok(!exclude_self)
            }
            FakeValue::Object(_) if target_class.eq_ignore_ascii_case("KnownClass") => {
                Ok(!exclude_self)
            }
            FakeValue::Object(_) if target_class.eq_ignore_ascii_case("ParentClass") => Ok(true),
            _ => Ok(false),
        }
    }

    /// Returns a fake PHP class name for object-tagged test values.
    fn object_class_name(
        &mut self,
        object: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        match self.get(object) {
            FakeValue::Object(_) => self.string("stdClass"),
            FakeValue::Iterator { .. } => self.string("Iterator"),
            _ => Err(EvalStatus::RuntimeFatal),
        }
    }

    /// Returns fake parent-class names for eval introspection unit tests.
    fn parent_class_name(
        &mut self,
        object_or_class: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        match self.get(object_or_class) {
            FakeValue::Object(_) => self.string("ParentClass"),
            FakeValue::String(name) if name.eq_ignore_ascii_case("ChildClass") => {
                self.string("ParentClass")
            }
            _ => self.string(""),
        }
    }

    /// Returns the visible element count for fake array values.
    fn array_len(&mut self, array: RuntimeCellHandle) -> Result<usize, EvalStatus> {
        match self.get(array) {
            FakeValue::Array(elements) => Ok(elements.len()),
            FakeValue::Assoc(entries) => Ok(entries.len()),
            _ => Err(EvalStatus::UnsupportedConstruct),
        }
    }

    /// Returns whether a fake runtime cell is an indexed or associative array.
    fn is_array_like(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus> {
        Ok(matches!(
            self.get(value),
            FakeValue::Array(_) | FakeValue::Assoc(_)
        ))
    }

    /// Returns whether a fake runtime cell is null.
    fn is_null(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus> {
        Ok(matches!(self.get(value), FakeValue::Null))
    }

    /// Returns the fake runtime tag corresponding to a test value.
    fn type_tag(&mut self, value: RuntimeCellHandle) -> Result<u64, EvalStatus> {
        Ok(match self.get(value) {
            FakeValue::Int(_) => EVAL_TAG_INT,
            FakeValue::String(_) | FakeValue::Bytes(_) => EVAL_TAG_STRING,
            FakeValue::Float(_) => EVAL_TAG_FLOAT,
            FakeValue::Bool(_) => EVAL_TAG_BOOL,
            FakeValue::Array(_) => EVAL_TAG_ARRAY,
            FakeValue::Assoc(_) => EVAL_TAG_ASSOC,
            FakeValue::Object(_) | FakeValue::Iterator { .. } => EVAL_TAG_OBJECT,
            FakeValue::Resource(_) => EVAL_TAG_RESOURCE,
            FakeValue::Null => EVAL_TAG_NULL,
        })
    }

    /// Returns the fake object handle as a stable object identity.
    fn object_identity(&mut self, object: RuntimeCellHandle) -> Result<u64, EvalStatus> {
        match self.get(object) {
            FakeValue::Object(_) | FakeValue::Iterator { .. } => Ok(object.as_ptr() as u64),
            _ => Err(EvalStatus::RuntimeFatal),
        }
    }

    /// Records fake releases without freeing handles needed for assertions.
    fn release(&mut self, value: RuntimeCellHandle) -> Result<(), EvalStatus> {
        self.releases.push(value);
        Ok(())
    }

    /// Returns the same fake handle because fake cells do not refcount.
    fn retain(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        Ok(value)
    }

    /// Records fake PHP warnings without writing to stderr.
    fn warning(&mut self, message: &str) -> Result<(), EvalStatus> {
        self.warnings.push(message.to_string());
        Ok(())
    }

    /// Creates a fake null cell.
    fn null(&mut self) -> Result<RuntimeCellHandle, EvalStatus> {
        Ok(self.alloc(FakeValue::Null))
    }

    /// Creates a fake bool cell.
    fn bool_value(&mut self, value: bool) -> Result<RuntimeCellHandle, EvalStatus> {
        Ok(self.alloc(FakeValue::Bool(value)))
    }

    /// Creates a fake int cell.
    fn int(&mut self, value: i64) -> Result<RuntimeCellHandle, EvalStatus> {
        Ok(self.alloc(FakeValue::Int(value)))
    }

    /// Creates a fake float cell.
    fn float(&mut self, value: f64) -> Result<RuntimeCellHandle, EvalStatus> {
        Ok(self.alloc(FakeValue::Float(value)))
    }

    /// Creates a fake string cell.
    fn string(&mut self, value: &str) -> Result<RuntimeCellHandle, EvalStatus> {
        Ok(self.alloc(FakeValue::String(value.to_string())))
    }

    /// Creates a fake string cell from raw PHP bytes.
    fn string_bytes_value(&mut self, value: &[u8]) -> Result<RuntimeCellHandle, EvalStatus> {
        match std::str::from_utf8(value) {
            Ok(value) => self.string(value),
            Err(_) => Ok(self.alloc(FakeValue::Bytes(value.to_vec()))),
        }
    }

    /// Casts a fake runtime cell to a fake integer cell.
    fn cast_int(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        let value = self.get(value);
        let value = self.fake_int(&value);
        self.int(value)
    }

    /// Casts a fake runtime cell to a fake float cell.
    fn cast_float(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        let value = self.get(value);
        let value = self.fake_numeric(&value);
        self.float(value)
    }

    /// Casts a fake runtime cell to a fake string cell.
    fn cast_string(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        let value = self.stringify(value);
        self.string(&value)
    }

    /// Casts a fake runtime cell to a fake boolean cell.
    fn cast_bool(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        let value = self.get(value);
        let value = self.fake_truthy(&value);
        self.bool_value(value)
    }

    /// Computes fake PHP absolute value while preserving float payloads.
    fn abs(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        match self.get(value) {
            FakeValue::Float(value) => self.float(value.abs()),
            value => self.int(self.fake_int(&value).wrapping_abs()),
        }
    }

    /// Computes fake PHP ceiling through numeric conversion as a float result.
    fn ceil(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        let value = self.get(value);
        self.float(self.fake_numeric(&value).ceil())
    }

    /// Computes fake PHP floor through numeric conversion as a float result.
    fn floor(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        let value = self.get(value);
        self.float(self.fake_numeric(&value).floor())
    }

    /// Computes fake PHP square root through numeric conversion as a float result.
    fn sqrt(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        let value = self.get(value);
        self.float(self.fake_numeric(&value).sqrt())
    }

    /// Reverses a fake string byte-wise for interpreter tests.
    fn strrev(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        let mut bytes = self.stringify(value).into_bytes();
        bytes.reverse();
        let value = String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
        self.string(&value)
    }

    /// Divides fake numeric cells with PHP `fdiv()` zero handling.
    fn fdiv(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let left = self.fake_numeric(&self.get(left));
        let right = self.fake_numeric(&self.get(right));
        self.float(left / right)
    }

    /// Computes fake floating-point modulo for interpreter tests.
    fn fmod(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let left = self.fake_numeric(&self.get(left));
        let right = self.fake_numeric(&self.get(right));
        self.float(left % right)
    }

    /// Adds fake numeric cells for interpreter tests.
    fn add(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        match (self.get(left), self.get(right)) {
            (FakeValue::Int(left), FakeValue::Int(right)) => self.int(left + right),
            (left, right) => self.float(self.fake_numeric(&left) + self.fake_numeric(&right)),
        }
    }

    /// Subtracts fake numeric cells for interpreter tests.
    fn sub(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        match (self.get(left), self.get(right)) {
            (FakeValue::Int(left), FakeValue::Int(right)) => self.int(left - right),
            (left, right) => self.float(self.fake_numeric(&left) - self.fake_numeric(&right)),
        }
    }

    /// Multiplies fake numeric cells for interpreter tests.
    fn mul(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        match (self.get(left), self.get(right)) {
            (FakeValue::Int(left), FakeValue::Int(right)) => self.int(left * right),
            (left, right) => self.float(self.fake_numeric(&left) * self.fake_numeric(&right)),
        }
    }

    /// Divides fake numeric cells for interpreter tests.
    fn div(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let right = self.fake_numeric(&self.get(right));
        if right == 0.0 {
            return Err(EvalStatus::RuntimeFatal);
        }
        let left = self.fake_numeric(&self.get(left));
        self.float(left / right)
    }

    /// Computes fake integer modulo for interpreter tests.
    fn modulo(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let right = self.fake_int(&self.get(right));
        if right == 0 {
            return Err(EvalStatus::RuntimeFatal);
        }
        let left = self.fake_int(&self.get(left));
        self.int(left % right)
    }

    /// Raises fake numeric cells for interpreter tests.
    fn pow(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let left = self.fake_numeric(&self.get(left));
        let right = self.fake_numeric(&self.get(right));
        self.float(left.powf(right))
    }

    /// Rounds fake numeric cells with PHP's optional decimal precision.
    fn round(
        &mut self,
        value: RuntimeCellHandle,
        precision: Option<RuntimeCellHandle>,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let value = self.fake_numeric(&self.get(value));
        let precision = precision
            .map(|precision| self.fake_int(&self.get(precision)))
            .unwrap_or(0);
        let multiplier = 10_f64.powf(precision as f64);
        self.float((value * multiplier).round() / multiplier)
    }

    /// Applies fake integer bitwise and shift operations for interpreter tests.
    fn bitwise(
        &mut self,
        op: EvalBinOp,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let left = self.fake_int(&self.get(left));
        let right = self.fake_int(&self.get(right));
        let value = match op {
            EvalBinOp::BitAnd => left & right,
            EvalBinOp::BitOr => left | right,
            EvalBinOp::BitXor => left ^ right,
            EvalBinOp::ShiftLeft => {
                if right < 0 {
                    return Err(EvalStatus::RuntimeFatal);
                }
                left.wrapping_shl(right as u32)
            }
            EvalBinOp::ShiftRight => {
                if right < 0 {
                    return Err(EvalStatus::RuntimeFatal);
                }
                left.wrapping_shr(right as u32)
            }
            _ => return Err(EvalStatus::UnsupportedConstruct),
        };
        self.int(value)
    }

    /// Applies fake integer bitwise NOT for interpreter tests.
    fn bit_not(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        let value = self.fake_int(&self.get(value));
        self.int(!value)
    }

    /// Concatenates fake cells with byte-preserving string conversion for interpreter tests.
    fn concat(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let mut left = self.string_bytes_for_value(&self.get(left));
        let right = self.string_bytes_for_value(&self.get(right));
        left.extend_from_slice(&right);
        self.string_bytes_value(&left)
    }

    /// Compares fake scalar cells and returns a fake PHP boolean.
    fn compare(
        &mut self,
        op: EvalBinOp,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let result = match op {
            EvalBinOp::LooseEq => self.loose_eq(left, right),
            EvalBinOp::LooseNotEq => !self.loose_eq(left, right),
            EvalBinOp::StrictEq => self.strict_eq(left, right),
            EvalBinOp::StrictNotEq => !self.strict_eq(left, right),
            EvalBinOp::Lt => self.numeric(left)? < self.numeric(right)?,
            EvalBinOp::LtEq => self.numeric(left)? <= self.numeric(right)?,
            EvalBinOp::Gt => self.numeric(left)? > self.numeric(right)?,
            EvalBinOp::GtEq => self.numeric(left)? >= self.numeric(right)?,
            EvalBinOp::Add
            | EvalBinOp::Sub
            | EvalBinOp::Mul
            | EvalBinOp::Div
            | EvalBinOp::Mod
            | EvalBinOp::Pow
            | EvalBinOp::BitAnd
            | EvalBinOp::BitOr
            | EvalBinOp::BitXor
            | EvalBinOp::ShiftLeft
            | EvalBinOp::ShiftRight
            | EvalBinOp::Concat
            | EvalBinOp::Spaceship
            | EvalBinOp::LogicalAnd
            | EvalBinOp::LogicalOr
            | EvalBinOp::LogicalXor => {
                return Err(EvalStatus::UnsupportedConstruct);
            }
        };
        self.bool_value(result)
    }

    /// Compares fake numeric cells and returns a PHP spaceship integer.
    fn spaceship(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let left = self.numeric(left)?;
        let right = self.numeric(right)?;
        let value = if left < right {
            -1
        } else if left > right {
            1
        } else {
            0
        };
        self.int(value)
    }

    /// Appends fake echo output for interpreter tests.
    fn echo(&mut self, value: RuntimeCellHandle) -> Result<(), EvalStatus> {
        let value = self.stringify(value);
        self.output.push_str(&value);
        Ok(())
    }

    /// Casts one fake runtime cell to bytes for nested eval parsing.
    fn string_bytes(&mut self, value: RuntimeCellHandle) -> Result<Vec<u8>, EvalStatus> {
        Ok(self.string_bytes_for_value(&self.get(value)))
    }

    /// Returns PHP-like truthiness for fake runtime cells.
    fn truthy(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus> {
        Ok(match self.get(value) {
            FakeValue::Null => false,
            FakeValue::Bool(value) => value,
            FakeValue::Int(value) => value != 0,
            FakeValue::Float(value) => value != 0.0,
            FakeValue::String(value) => !value.is_empty() && value != "0",
            FakeValue::Bytes(value) => !value.is_empty() && value.as_slice() != b"0",
            FakeValue::Array(value) => !value.is_empty(),
            FakeValue::Assoc(value) => !value.is_empty(),
            FakeValue::Object(_) | FakeValue::Iterator { .. } => true,
            FakeValue::Resource(_) => true,
        })
    }
}

impl FakeOps {
    /// Compares fake scalar values with the same loose rules covered by eval tests.
    fn loose_eq(&self, left: RuntimeCellHandle, right: RuntimeCellHandle) -> bool {
        match (self.get(left), self.get(right)) {
            (FakeValue::Bool(left), right) => left == self.fake_truthy(&right),
            (left, FakeValue::Bool(right)) => self.fake_truthy(&left) == right,
            (FakeValue::Null, FakeValue::Null) => true,
            (FakeValue::Null, FakeValue::String(value))
            | (FakeValue::String(value), FakeValue::Null) => value.is_empty(),
            (FakeValue::Null, FakeValue::Bytes(value))
            | (FakeValue::Bytes(value), FakeValue::Null) => value.is_empty(),
            (FakeValue::String(left), FakeValue::String(right)) => {
                match (left.parse::<f64>(), right.parse::<f64>()) {
                    (Ok(left), Ok(right)) => left == right,
                    _ => left == right,
                }
            }
            (FakeValue::Bytes(left), FakeValue::Bytes(right)) => left == right,
            (FakeValue::String(left), FakeValue::Bytes(right))
            | (FakeValue::Bytes(right), FakeValue::String(left)) => left.as_bytes() == right,
            (FakeValue::String(left), right) => left
                .parse::<f64>()
                .is_ok_and(|left| left == self.fake_numeric(&right)),
            (FakeValue::Bytes(left), right) => std::str::from_utf8(&left)
                .ok()
                .and_then(|left| left.parse::<f64>().ok())
                .is_some_and(|left| left == self.fake_numeric(&right)),
            (left, FakeValue::String(right)) => right
                .parse::<f64>()
                .is_ok_and(|right| self.fake_numeric(&left) == right),
            (left, FakeValue::Bytes(right)) => std::str::from_utf8(&right)
                .ok()
                .and_then(|right| right.parse::<f64>().ok())
                .is_some_and(|right| self.fake_numeric(&left) == right),
            (left, right) => self.fake_numeric(&left) == self.fake_numeric(&right),
        }
    }

    /// Compares fake scalar values by PHP strict tag and payload equality.
    fn strict_eq(&self, left: RuntimeCellHandle, right: RuntimeCellHandle) -> bool {
        match (self.get(left), self.get(right)) {
            (FakeValue::Null, FakeValue::Null) => true,
            (FakeValue::Bool(left), FakeValue::Bool(right)) => left == right,
            (FakeValue::Int(left), FakeValue::Int(right)) => left == right,
            (FakeValue::Float(left), FakeValue::Float(right)) => left == right,
            (FakeValue::String(left), FakeValue::String(right)) => left == right,
            (FakeValue::Bytes(left), FakeValue::Bytes(right)) => left == right,
            (FakeValue::String(left), FakeValue::Bytes(right))
            | (FakeValue::Bytes(right), FakeValue::String(left)) => left.as_bytes() == right,
            (FakeValue::Resource(left), FakeValue::Resource(right)) => left == right,
            _ => false,
        }
    }

    /// Converts one fake scalar cell to a numeric value for comparison tests.
    fn numeric(&self, handle: RuntimeCellHandle) -> Result<f64, EvalStatus> {
        Ok(self.fake_numeric(&self.get(handle)))
    }

    /// Converts a fake value to the numeric scalar used by comparison tests.
    fn fake_numeric(&self, value: &FakeValue) -> f64 {
        match value {
            FakeValue::Null => 0.0,
            FakeValue::Bool(false) => 0.0,
            FakeValue::Bool(true) => 1.0,
            FakeValue::Int(value) => *value as f64,
            FakeValue::Float(value) => *value,
            FakeValue::String(value) => value.parse::<f64>().unwrap_or(0.0),
            FakeValue::Bytes(value) => std::str::from_utf8(value)
                .ok()
                .and_then(|value| value.parse::<f64>().ok())
                .unwrap_or(0.0),
            FakeValue::Array(value) => value.len() as f64,
            FakeValue::Assoc(value) => value.len() as f64,
            FakeValue::Object(_) | FakeValue::Iterator { .. } => 1.0,
            FakeValue::Resource(value) => (*value + 1) as f64,
        }
    }

    /// Converts a fake value to the integer scalar used by modulo tests.
    fn fake_int(&self, value: &FakeValue) -> i64 {
        self.fake_numeric(value) as i64
    }

    /// Returns fake PHP truthiness for already-loaded test values.
    fn fake_truthy(&self, value: &FakeValue) -> bool {
        match value {
            FakeValue::Null => false,
            FakeValue::Bool(value) => *value,
            FakeValue::Int(value) => *value != 0,
            FakeValue::Float(value) => *value != 0.0,
            FakeValue::String(value) => !value.is_empty() && value != "0",
            FakeValue::Bytes(value) => !value.is_empty() && value.as_slice() != b"0",
            FakeValue::Array(value) => !value.is_empty(),
            FakeValue::Assoc(value) => !value.is_empty(),
            FakeValue::Object(_) | FakeValue::Iterator { .. } => true,
            FakeValue::Resource(_) => true,
        }
    }

    /// Converts a fake runtime cell to a PHP-like string for test echo/concat.
    fn stringify(&self, handle: RuntimeCellHandle) -> String {
        match self.get(handle) {
            FakeValue::Null => String::new(),
            FakeValue::Bool(false) => String::new(),
            FakeValue::Bool(true) => "1".to_string(),
            FakeValue::Int(value) => value.to_string(),
            FakeValue::Float(value) => value.to_string(),
            FakeValue::String(value) => value,
            FakeValue::Bytes(value) => String::from_utf8_lossy(&value).into_owned(),
            FakeValue::Array(_) => "Array".to_string(),
            FakeValue::Assoc(_) => "Array".to_string(),
            FakeValue::Object(_) | FakeValue::Iterator { .. } => "Object".to_string(),
            FakeValue::Resource(value) => format!("Resource id #{}", value + 1),
        }
    }

    /// Converts a fake PHP value to string bytes while preserving binary strings.
    fn string_bytes_for_value(&self, value: &FakeValue) -> Vec<u8> {
        match value {
            FakeValue::String(value) => value.as_bytes().to_vec(),
            FakeValue::Bytes(value) => value.clone(),
            value => self.stringify_value(value).into_bytes(),
        }
    }

    /// Converts one loaded fake PHP value to display text for byte coercions.
    fn stringify_value(&self, value: &FakeValue) -> String {
        match value {
            FakeValue::Null => String::new(),
            FakeValue::Bool(false) => String::new(),
            FakeValue::Bool(true) => "1".to_string(),
            FakeValue::Int(value) => value.to_string(),
            FakeValue::Float(value) => value.to_string(),
            FakeValue::String(value) => value.clone(),
            FakeValue::Bytes(value) => String::from_utf8_lossy(value).into_owned(),
            FakeValue::Array(_) | FakeValue::Assoc(_) => "Array".to_string(),
            FakeValue::Object(_) | FakeValue::Iterator { .. } => "Object".to_string(),
            FakeValue::Resource(value) => format!("Resource id #{}", value + 1),
        }
    }
}

/// Test native invoker that returns the descriptor pointer as a runtime cell.
unsafe extern "C" fn fake_native_return_descriptor(
    descriptor: *mut c_void,
    _args: *mut RuntimeCell,
) -> *mut RuntimeCell {
    descriptor.cast()
}

/// Verifies assignment writes a named scope entry and return reads it back.
#[test]
fn execute_program_stores_and_returns_scope_value() {
    let program = parse_fragment(b"$x = 3; return $x + 4;").expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let x = scope.visible_cell("x").expect("scope should contain x");

    assert_eq!(values.get(x), FakeValue::Int(3));
    assert_eq!(values.get(result), FakeValue::Int(7));
}

/// Verifies reference assignment aliases variable names and writes through the alias.
#[test]
fn execute_program_reference_assignment_updates_source_variable() {
    let program = parse_fragment(b"$x = 1; $alias =& $x; $alias = 5; return $x;")
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let x = scope.visible_cell("x").expect("scope should contain x");
    let alias = scope
        .visible_cell("alias")
        .expect("scope should contain alias");

    assert_eq!(x, alias);
    assert_eq!(values.get(x), FakeValue::Int(5));
    assert_eq!(values.get(result), FakeValue::Int(5));
}

/// Verifies eval `throw` exits the program with a retained Throwable cell.
#[test]
fn execute_program_propagates_throw_as_uncaught_outcome() {
    let program =
        parse_fragment(br#"throw new Exception("eval boom");"#).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let outcome =
        execute_program_outcome_with_context(&mut context, &program, &mut scope, &mut values)
            .expect("throw should be an eval outcome");

    match outcome {
        EvalOutcome::Throwable(value) => {
            assert_eq!(values.type_tag(value), Ok(EVAL_TAG_OBJECT));
        }
        EvalOutcome::Value(value) => panic!("expected Throwable, got {:?}", values.get(value)),
    }
}

/// Verifies eval `try/catch` catches a thrown object and binds the catch variable.
#[test]
fn execute_program_catches_throwable_inside_eval() {
    let program = parse_fragment(
        br#"try {
    throw new Exception("eval boom");
} catch (Throwable $caught) {
    return $caught->answer();
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let caught = scope
        .visible_cell("caught")
        .expect("scope should contain catch variable");

    assert_eq!(values.type_tag(caught), Ok(EVAL_TAG_OBJECT));
    assert_eq!(values.get(result), FakeValue::Int(42));
}

/// Verifies eval `catch (Throwable)` can handle a throw without binding a variable.
#[test]
fn execute_program_catches_throwable_without_variable_inside_eval() {
    let program = parse_fragment(
        br#"try {
    throw new Exception("eval boom");
} catch (Throwable) {
    return 9;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let released = values
        .releases
        .first()
        .copied()
        .expect("unbound catch should release the thrown object");

    assert_eq!(scope.visible_cell("caught"), None);
    assert_eq!(values.type_tag(released), Ok(EVAL_TAG_OBJECT));
    assert_eq!(values.get(result), FakeValue::Int(9));
}

/// Verifies eval `catch (Exception)` matches thrown exception objects.
#[test]
fn execute_program_catches_specific_exception_inside_eval() {
    let program = parse_fragment(
        br#"try {
    throw new Exception("eval boom");
} catch (Exception $caught) {
    return $caught->answer();
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let caught = scope
        .visible_cell("caught")
        .expect("scope should contain catch variable");

    assert_eq!(values.type_tag(caught), Ok(EVAL_TAG_OBJECT));
    assert_eq!(values.get(result), FakeValue::Int(42));
}

/// Verifies eval catch clauses keep source order and skip non-matching types.
#[test]
fn execute_program_skips_non_matching_specific_catch_inside_eval() {
    let program = parse_fragment(
        br#"try {
    throw new Exception("eval boom");
} catch (RuntimeException $wrong) {
    return 1;
} catch (Exception $caught) {
    return 2;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(scope.visible_cell("wrong"), None);
    assert_eq!(values.get(result), FakeValue::Int(2));
}

/// Verifies union catch clauses test later types in the same catch clause.
#[test]
fn execute_program_catches_union_type_inside_eval() {
    let program = parse_fragment(
        br#"try {
    throw new Exception("eval boom");
} catch (RuntimeException|Exception $caught) {
    return $caught->answer();
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let caught = scope
        .visible_cell("caught")
        .expect("scope should contain catch variable");

    assert_eq!(values.type_tag(caught), Ok(EVAL_TAG_OBJECT));
    assert_eq!(values.get(result), FakeValue::Int(42));
}

/// Verifies eval `finally` runs before a pending try-body return is observed.
#[test]
fn execute_program_runs_finally_before_returning_try_value() {
    let program = parse_fragment(
        br#"try {
    return 1;
} finally {
    echo "finally";
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "finally");
    assert_eq!(values.get(result), FakeValue::Int(1));
}

/// Verifies eval `finally` return values replace pending try-body returns.
#[test]
fn execute_program_finally_return_overrides_try_return() {
    let program = parse_fragment(
        br#"try {
    return 1;
} finally {
    return 2;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(2));
    assert_eq!(values.releases.len(), 1);
}

/// Verifies eval `finally` return values replace pending uncaught throws.
#[test]
fn execute_program_finally_return_overrides_uncaught_throw() {
    let program = parse_fragment(
        br#"try {
    throw new Exception("eval boom");
} finally {
    return 2;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let released = values
        .releases
        .first()
        .copied()
        .expect("overridden throw should be released");

    assert_eq!(values.get(result), FakeValue::Int(2));
    assert_eq!(values.type_tag(released), Ok(EVAL_TAG_OBJECT));
}

/// Verifies eval `finally` runs before an uncaught throw leaves the fragment.
#[test]
fn execute_program_runs_finally_before_uncaught_throw_outcome() {
    let program = parse_fragment(
        br#"try {
    throw new Exception("eval boom");
} finally {
    echo "finally";
}"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let outcome =
        execute_program_outcome_with_context(&mut context, &program, &mut scope, &mut values)
            .expect("throw should be an eval outcome");

    match outcome {
        EvalOutcome::Throwable(value) => {
            assert_eq!(values.type_tag(value), Ok(EVAL_TAG_OBJECT))
        }
        EvalOutcome::Value(value) => panic!("expected Throwable, got {:?}", values.get(value)),
    }
    assert_eq!(values.output, "finally");
}

/// Verifies static locals declared inside eval catch blocks persist per function context.
#[test]
fn execute_context_function_persists_static_local_inside_catch() {
    let program = parse_fragment(
        br#"function dyn($e) {
    try {
        throw $e;
    } catch (Throwable $caught) {
        static $n = 0;
        $n++;
        return $caught->answer() + $n;
    }
}"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("declare dynamic function");
    let first_thrown = values
        .new_object("Exception")
        .expect("allocate first fake exception");
    let second_thrown = values
        .new_object("Exception")
        .expect("allocate second fake exception");

    let first = execute_context_function(&mut context, "dyn", vec![first_thrown], &mut values)
        .expect("execute first dynamic function call");
    let second = execute_context_function(&mut context, "dyn", vec![second_thrown], &mut values)
        .expect("execute second dynamic function call");

    assert_eq!(values.get(first), FakeValue::Int(43));
    assert_eq!(values.get(second), FakeValue::Int(44));
}

/// Verifies static locals declared inside eval finally blocks persist per function context.
#[test]
fn execute_context_function_persists_static_local_inside_finally() {
    let program = parse_fragment(
        br#"function dyn() {
    try {
        return 0;
    } finally {
        static $n = 0;
        $n++;
        return $n;
    }
}"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("declare dynamic function");

    let first = execute_context_function_zero_args(&mut context, "dyn", &mut values)
        .expect("execute first dynamic function call");
    let second = execute_context_function_zero_args(&mut context, "dyn", &mut values)
        .expect("execute second dynamic function call");

    assert_eq!(values.get(first), FakeValue::Int(1));
    assert_eq!(values.get(second), FakeValue::Int(2));
}

/// Verifies throws from eval-declared functions escape through the shared context.
#[test]
fn execute_context_function_propagates_throw_as_uncaught_outcome() {
    let program =
        parse_fragment(br#"function dyn($e) { throw $e; }"#).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("declare dynamic function");
    let thrown = values
        .new_object("Exception")
        .expect("allocate fake exception");

    let outcome = execute_context_function_outcome(&mut context, "dyn", vec![thrown], &mut values)
        .expect("throw should be an eval function outcome");

    match outcome {
        EvalOutcome::Throwable(value) => assert_eq!(value, thrown),
        EvalOutcome::Value(value) => panic!("expected Throwable, got {:?}", values.get(value)),
    }
}

/// Verifies nested eval preserves the thrown cell while returning an uncaught status.
#[test]
fn execute_program_nested_eval_propagates_throw_as_uncaught_outcome() {
    let program = parse_fragment(br#"eval("throw $e;");"#).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let thrown = values
        .new_object("Exception")
        .expect("allocate fake exception");
    scope.set("e", thrown, ScopeCellOwnership::Borrowed);

    let outcome =
        execute_program_outcome_with_context(&mut context, &program, &mut scope, &mut values)
            .expect("nested throw should be an eval outcome");

    match outcome {
        EvalOutcome::Throwable(value) => assert_eq!(value, thrown),
        EvalOutcome::Value(value) => panic!("expected Throwable, got {:?}", values.get(value)),
    }
}

/// Verifies eval include resolves caller-relative paths, shares scope, and returns file values.
#[test]
fn execute_program_include_uses_call_site_and_returns_file_result() {
    let dir = std::env::temp_dir().join(format!(
        "elephc-eval-include-{}-call-site",
        std::process::id()
    ));
    let path = dir.join("piece.php");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create include fixture directory");
    std::fs::write(
            &path,
            format!(
                r#"<?php echo (__DIR__ === "{}" ? "D" : "d"); echo (__FILE__ === "{}" ? "F" : "f"); $x = $x + 1; return $x;"#,
                dir.to_string_lossy(),
                path.to_string_lossy()
            ),
        )
        .expect("write include fixture");
    let program = parse_fragment(br#"return include "piece.php";"#).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    context.set_call_site(
        dir.join("main.php").to_string_lossy().into_owned(),
        dir.to_string_lossy().into_owned(),
        1,
    );
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let x = values.int(2).expect("allocate fake int");
    scope.set("x", x, ScopeCellOwnership::Owned);

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval include");

    assert_eq!(values.output, "DF");
    assert_eq!(values.get(result), FakeValue::Int(3));
    assert_eq!(
        values.get(scope.visible_cell("x").expect("scope should contain x")),
        FakeValue::Int(3)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies regular include marks a file so later include_once skips it and returns true.
#[test]
fn execute_program_include_once_skips_regularly_included_file() {
    let dir = std::env::temp_dir().join(format!("elephc-eval-include-{}-once", std::process::id()));
    let path = dir.join("once.php");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create include_once fixture directory");
    std::fs::write(&path, br#"<?php echo "O";"#).expect("write include_once fixture");
    let source = format!(
        r#"include "{}"; return include_once "{}";"#,
        path.to_string_lossy(),
        path.to_string_lossy()
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute include_once");

    assert_eq!(values.output, "O");
    assert_eq!(values.get(result), FakeValue::Bool(true));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies missing include warns and returns false without aborting the eval program.
#[test]
fn execute_program_missing_include_warns_and_returns_false() {
    let missing = std::env::temp_dir().join(format!(
        "elephc-eval-missing-{}-include.php",
        std::process::id()
    ));
    let source = format!(r#"return include "{}";"#, missing.to_string_lossy());
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("missing include returns false");

    assert_eq!(values.get(result), FakeValue::Bool(false));
    assert_eq!(values.warnings.len(), 2);
}

/// Verifies missing require emits warnings and aborts the eval program.
#[test]
fn execute_program_missing_require_is_runtime_fatal() {
    let missing = std::env::temp_dir().join(format!(
        "elephc-eval-missing-{}-require.php",
        std::process::id()
    ));
    let source = format!(r#"require "{}";"#, missing.to_string_lossy());
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect_err("missing require should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
    assert_eq!(values.warnings.len(), 2);
}

/// Verifies simple variable compound assignments read, compute, and write the scope value.
#[test]
fn execute_program_evaluates_compound_assignments() {
    let program =
        parse_fragment(br#"$x = 2; $x += 3; $x *= 4; $x -= 5; $s = "v"; $s .= $x; echo $s;"#)
            .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let x = scope.visible_cell("x").expect("scope should contain x");

    assert_eq!(values.output, "v15");
    assert_eq!(values.get(x), FakeValue::Int(15));
}

/// Verifies division and modulo evaluate through fake runtime numeric hooks.
#[test]
fn execute_program_evaluates_division_and_modulo() {
    let program = parse_fragment(br#"$x = 20; $x /= 2; $x %= 6; echo $x; return 9 / 2;"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let x = scope.visible_cell("x").expect("scope should contain x");

    assert_eq!(values.output, "4");
    assert_eq!(values.get(x), FakeValue::Int(4));
    assert_eq!(values.get(result), FakeValue::Float(4.5));
}

/// Verifies exponentiation evaluates through fake runtime numeric hooks.
#[test]
fn execute_program_evaluates_exponentiation() {
    let program = parse_fragment(
        br#"$x = 2; $x **= 3; echo $x; echo ":"; echo -2 ** 2; return 2 ** 3 ** 2;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let x = scope.visible_cell("x").expect("scope should contain x");

    assert_eq!(values.output, "8:-4");
    assert_eq!(values.get(x), FakeValue::Float(8.0));
    assert_eq!(values.get(result), FakeValue::Float(512.0));
}

/// Verifies bitwise and shift operators evaluate through fake runtime hooks.
#[test]
fn execute_program_evaluates_bitwise_and_shift_ops() {
    let program = parse_fragment(
        br#"$x = 6; $x &= 3; echo $x; echo ":";
$x = 4; $x |= 1; echo $x; echo ":";
$x = 7; $x ^= 3; echo $x; echo ":";
$x = 1; $x <<= 5; echo $x; echo ":";
$x = 64; $x >>= 3; echo $x; echo ":";
echo ~0; echo ":"; echo -16 >> 2;
return (1 << 4) | ((16 >> 2) ^ (3 & 1));"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "2:5:4:32:8:-1:-4");
    assert_eq!(values.get(result), FakeValue::Int(21));
}

/// Verifies simple variable increment and decrement statements update the scope value.
#[test]
fn execute_program_evaluates_inc_dec_statements() {
    let program = parse_fragment(br#"$i = 1; $i++; ++$i; $i--; --$i; echo $i;"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let i = scope.visible_cell("i").expect("scope should contain i");

    assert_eq!(values.output, "1");
    assert_eq!(values.get(i), FakeValue::Int(1));
}

/// Verifies echo and unset operate through runtime hooks and scope metadata.
#[test]
fn execute_program_echoes_and_unsets_scope_value() {
    let program =
        parse_fragment(br#"echo "hi" . $name; unset($name);"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let name = values.string(" Ada").expect("create fake string");
    scope.set("name", name, ScopeCellOwnership::Owned);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "hi Ada");
    assert_eq!(values.get(result), FakeValue::Null);
    assert!(scope.entry("name").expect("unset marker").flags().unset);
}

/// Verifies comma-separated echo expressions are executed in source order.
#[test]
fn execute_program_echoes_comma_list() {
    let program = parse_fragment(br#"echo "a", $b, "c";"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let b = values.string("b").expect("create fake string");
    scope.set("b", b, ScopeCellOwnership::Owned);

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "abc");
}

/// Verifies print writes output and returns integer 1.
#[test]
fn execute_program_print_returns_one() {
    let program = parse_fragment(br#"return print "p";"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "p");
    assert_eq!(values.get(result), FakeValue::Int(1));
}

/// Verifies eval `print_r()` emits supported values and returns true.
#[test]
fn execute_program_dispatches_print_r_builtin() {
    let program = parse_fragment(
        br#"print_r("x"); echo ":";
print_r(value: false); echo ":";
print_r([1, 2]); echo ":";
$call = call_user_func("print_r", true);
$spread = call_user_func_array("print_r", ["value" => "z"]);
echo ":" . ($call ? "call" : "bad") . ":" . ($spread ? "spread" : "bad") . ":";
return function_exists("print_r");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "x::Array\n:1z:call:spread:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `var_dump()` emits scalar and array diagnostics and returns null.
#[test]
fn execute_program_dispatches_var_dump_builtin() {
    let program = parse_fragment(
            br#"var_dump(42);
var_dump("hi");
var_dump(false);
var_dump(null);
var_dump([10, 20]);
var_dump(["x" => true]);
$call = call_user_func("var_dump", 3.5);
$spread = call_user_func_array("var_dump", ["value" => "z"]);
echo ($call === null ? "call-null" : "bad") . ":" . ($spread === null ? "spread-null" : "bad") . ":";
return function_exists("var_dump");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        concat!(
            "int(42)\n",
            "string(2) \"hi\"\n",
            "bool(false)\n",
            "NULL\n",
            "array(2) {\n",
            "  [0]=>\n",
            "  int(10)\n",
            "  [1]=>\n",
            "  int(20)\n",
            "}\n",
            "array(1) {\n",
            "  [\"x\"]=>\n",
            "  bool(true)\n",
            "}\n",
            "float(3.5)\n",
            "string(1) \"z\"\n",
            "call-null:spread-null:",
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval property reads and writes dispatch through runtime hooks.
#[test]
fn execute_program_reads_and_writes_object_property() {
    let program = parse_fragment(br#"$this->x = $this->x + 1; return $this->x;"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let x = values.int(1).expect("create fake int");
    let properties = vec![("x".to_string(), x)];
    let object = values.alloc(FakeValue::Object(properties));
    scope.set("this", object, ScopeCellOwnership::Borrowed);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(2));
    assert_eq!(
        values
            .property_get(object, "x")
            .map(|value| values.get(value))
            .expect("property should be readable"),
        FakeValue::Int(2)
    );
}

/// Verifies eval method calls dispatch through the runtime method hook.
#[test]
fn execute_program_calls_object_method() {
    let program = parse_fragment(br#"return $this->answer();"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let object = values.alloc(FakeValue::Object(Vec::new()));
    scope.set("this", object, ScopeCellOwnership::Borrowed);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(42));
}

/// Verifies eval method calls forward evaluated arguments to the runtime hook.
#[test]
fn execute_program_calls_object_method_with_argument() {
    let program = parse_fragment(br#"return $this->add_x(5);"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let x = values.int(7).expect("create fake int");
    let properties = vec![("x".to_string(), x)];
    let object = values.alloc(FakeValue::Object(properties));
    scope.set("this", object, ScopeCellOwnership::Borrowed);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(12));
}

/// Verifies eval method calls forward multiple evaluated arguments to the runtime hook.
#[test]
fn execute_program_calls_object_method_with_two_arguments() {
    let program = parse_fragment(br#"return $this->add2_x(5, 6);"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let x = values.int(7).expect("create fake int");
    let properties = vec![("x".to_string(), x)];
    let object = values.alloc(FakeValue::Object(properties));
    scope.set("this", object, ScopeCellOwnership::Borrowed);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(18));
}

/// Verifies eval method calls forward numerically unpacked arguments.
#[test]
fn execute_program_calls_object_method_with_spread_arguments() {
    let program =
        parse_fragment(br#"return $this->add2_x(...[5, 6]);"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let x = values.int(7).expect("create fake int");
    let properties = vec![("x".to_string(), x)];
    let object = values.alloc(FakeValue::Object(properties));
    scope.set("this", object, ScopeCellOwnership::Borrowed);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(18));
}

/// Verifies eval object construction dispatches through runtime hooks.
#[test]
fn execute_program_constructs_named_object() {
    let program = parse_fragment(br#"return new Box();"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Object(Vec::new()));
}

/// Verifies eval object construction passes constructor arguments through runtime hooks.
#[test]
fn execute_program_constructs_named_object_with_args() {
    let program = parse_fragment(br#"return new Box(1);"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let FakeValue::Object(properties) = values.get(result) else {
        panic!("expected fake object");
    };
    let x = FakeOps::object_property(&properties, "x").expect("constructor should set x");

    assert_eq!(values.get(x), FakeValue::Int(1));
}

/// Verifies eval-declared classes create objects with properties and methods.
#[test]
fn execute_program_constructs_eval_declared_class_with_method() {
    let program = parse_fragment(
        br#"class DynBox {
    public int $x = 1;
    public function __construct($x) { $this->x = $x; }
    public function bump($n) { $this->x = $this->x + $n; return $this->x; }
}
$box = new DynBox(4);
echo get_class($box);
echo ":";
echo $box->bump(3);
echo ":";
echo is_a($box, "DynBox") ? "Y" : "N";
$call = [$box, "bump"];
echo call_user_func($call, 1);
echo ":";
echo call_user_func_array($call, [2]);
echo ":";
return $box->x;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "DynBox:7:Y8:10:");
    assert_eq!(values.get(result), FakeValue::Int(10));
}

/// Verifies if/else executes only the PHP-truthy branch.
#[test]
fn execute_program_if_else_uses_php_truthiness() {
    let program = parse_fragment(br#"if ($flag) { $x = "then"; } else { $x = "else"; }"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let flag = values.int(0).expect("create fake int");
    scope.set("flag", flag, ScopeCellOwnership::Owned);

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let x = scope.visible_cell("x").expect("scope should contain x");

    assert_eq!(values.get(x), FakeValue::String("else".to_string()));
}

/// Verifies elseif chains execute the first truthy branch and skip later branches.
#[test]
fn execute_program_elseif_uses_first_truthy_branch() {
    let program =
        parse_fragment(br#"if ($a) { $x = "a"; } elseif ($b) { $x = "b"; } else { $x = "c"; }"#)
            .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let a = values.bool_value(false).expect("create fake bool");
    let b = values.bool_value(true).expect("create fake bool");
    scope.set("a", a, ScopeCellOwnership::Owned);
    scope.set("b", b, ScopeCellOwnership::Owned);

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let x = scope.visible_cell("x").expect("scope should contain x");

    assert_eq!(values.get(x), FakeValue::String("b".to_string()));
}

/// Verifies while repeats while the condition remains truthy and propagates writes.
#[test]
fn execute_program_while_uses_php_truthiness() {
    let program = parse_fragment(br#"while ($flag) { echo $flag; $flag = false; }"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let flag = values.int(2).expect("create fake int");
    scope.set("flag", flag, ScopeCellOwnership::Owned);

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let flag = scope
        .visible_cell("flag")
        .expect("scope should contain flag");

    assert_eq!(values.output, "2");
    assert_eq!(values.get(flag), FakeValue::Bool(false));
}

/// Verifies do/while runs the body before testing the condition.
#[test]
fn execute_program_do_while_runs_body_before_condition() {
    let program = parse_fragment(br#"do { echo $i; $i = $i + 1; } while (false);"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let i = values.int(0).expect("create fake int");
    scope.set("i", i, ScopeCellOwnership::Owned);

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let i = scope.visible_cell("i").expect("scope should contain i");

    assert_eq!(values.output, "0");
    assert_eq!(values.get(i), FakeValue::Int(1));
}

/// Verifies switch uses loose matching and falls through after the matching case.
#[test]
fn execute_program_switch_matches_and_falls_through() {
    let program =
            parse_fragment(br#"switch ($x) { case 1: echo "one"; break; case 2: echo "two"; default: echo "default"; }"#)
                .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let x = values.int(2).expect("create fake int");
    scope.set("x", x, ScopeCellOwnership::Owned);

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "twodefault");
}

/// Verifies for loops run init, condition, update, and body in PHP order.
#[test]
fn execute_program_for_loop_updates_after_body() {
    let program = parse_fragment(br#"for ($i = 3; $i; $i = $i - 1) { echo $i; }"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let i = scope.visible_cell("i").expect("scope should contain i");

    assert_eq!(values.output, "321");
    assert_eq!(values.get(i), FakeValue::Int(0));
}

/// Verifies `continue` in a for loop still runs the update clause.
#[test]
fn execute_program_for_continue_runs_update_clause() {
    let program = parse_fragment(
        br#"for ($i = 3; $i; $i = $i - 1) { if ($i - 1) { continue; } echo "done"; }"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let i = scope.visible_cell("i").expect("scope should contain i");

    assert_eq!(values.output, "done");
    assert_eq!(values.get(i), FakeValue::Int(0));
}

/// Verifies comparison operators return boolean cells usable by echo and branches.
#[test]
fn execute_program_comparisons_return_bool_cells() {
    let program = parse_fragment(
            br#"echo 2 < 3; echo 3 <= 3; echo 4 > 3; echo 4 >= 4; if ("10" == 10) { echo "n"; } if ("a" != "b") { echo "s"; }"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "1111ns");
}

/// Verifies spaceship comparisons return PHP -1/0/1 integer cells.
#[test]
fn execute_program_spaceship_returns_int_cells() {
    let program =
        parse_fragment(br#"echo 1 <=> 2; echo ":"; echo 2 <=> 2; echo ":"; echo 3 <=> 2;"#)
            .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "-1:0:1");
}

/// Verifies strict equality keeps PHP type identity distinct from loose equality.
#[test]
fn execute_program_strict_equality_uses_type_identity() {
    let program = parse_fragment(
        br#"echo "10" == 10; echo "10" === 10; echo "10" === "10"; echo "10" !== 10;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "111");
}

/// Verifies logical AND skips an unsupported right-hand expression after a false left side.
#[test]
fn execute_program_short_circuits_logical_and() {
    let program = parse_fragment(br#"return false && missing();"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Bool(false));
}

/// Verifies logical OR skips an unsupported right-hand expression after a true left side.
#[test]
fn execute_program_short_circuits_logical_or() {
    let program = parse_fragment(br#"return true || missing();"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies match expressions use strict comparison across comma-separated patterns.
#[test]
fn execute_program_match_uses_strict_pattern_comparison() {
    let program =
        parse_fragment(br#"return match ($x) { 1, "1" => "string", default => "other" };"#)
            .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let x = values.string("1").expect("create fake string");
    scope.set("x", x, ScopeCellOwnership::Owned);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("string".to_string()));
}

/// Verifies match expressions evaluate only the selected arm result.
#[test]
fn execute_program_match_skips_unselected_results() {
    let program = parse_fragment(
        br#"return match (2) { 1 => missing(), 2 => "two", default => missing() };"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("two".to_string()));
}

/// Verifies match expressions without a matching arm or default fail at runtime.
#[test]
fn execute_program_match_without_default_fails_on_miss() {
    let program = parse_fragment(br#"return match (3) { 1 => "one", 2 => "two" };"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values);

    assert_eq!(result, Err(EvalStatus::RuntimeFatal));
}

/// Verifies PHP keyword logical operators use PHP precedence and short-circuiting.
#[test]
fn execute_program_evaluates_keyword_logical_operators() {
    let program =
        parse_fragment(br#"echo (false || true and false) ? "T" : "F"; return true or missing();"#)
            .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "F");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies PHP keyword `xor` evaluates both operands and returns a boolean cell.
#[test]
fn execute_program_evaluates_keyword_xor() {
    let program =
        parse_fragment(br#"echo (true xor false) ? "T" : "F"; echo (true xor true) ? "T" : "F";"#)
            .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "TF");
}

/// Verifies ternary expressions evaluate only the selected branch.
#[test]
fn execute_program_ternary_short_circuits_unselected_branch() {
    let program =
        parse_fragment(br#"echo true ? "yes" : missing(); echo false ? missing() : "no";"#)
            .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "yesno");
}

/// Verifies the short ternary form returns the condition value when it is truthy.
#[test]
fn execute_program_short_ternary_reuses_truthy_condition() {
    let program = parse_fragment(br#"echo "x" ?: "fallback"; echo false ?: "fallback";"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "xfallback");
}

/// Verifies null coalescing uses the default for missing or null values.
#[test]
fn execute_program_null_coalesce_uses_default_for_missing_or_null() {
    let program = parse_fragment(br#"echo $missing ?? "fallback"; echo $x ?? "null-fallback";"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let x = values.null().expect("create fake null");
    scope.set("x", x, ScopeCellOwnership::Owned);

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "fallbacknull-fallback");
}

/// Verifies null coalescing skips the default expression for non-null values.
#[test]
fn execute_program_null_coalesce_short_circuits_non_null_value() {
    let program = parse_fragment(br#"echo "set" ?? missing();"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "set");
}

/// Verifies logical negation returns boolean cells using PHP truthiness.
#[test]
fn execute_program_evaluates_logical_not() {
    let program = parse_fragment(br#"echo !false; echo !"x";"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "1");
}

/// Verifies unary numeric operators delegate to PHP numeric runtime operations.
#[test]
fn execute_program_evaluates_unary_numeric_ops() {
    let program = parse_fragment(br#"return -$x + +2;"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let x = values.int(5).expect("create fake int");
    scope.set("x", x, ScopeCellOwnership::Owned);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(-3));
}

/// Verifies foreach assigns each indexed element to the value variable.
#[test]
fn execute_program_foreach_iterates_indexed_values() {
    let program = parse_fragment(br#"foreach (["a", "b"] as $item) { echo $item; }"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let item = scope
        .visible_cell("item")
        .expect("scope should contain last foreach item");

    assert_eq!(values.output, "ab");
    assert_eq!(values.get(item), FakeValue::String("b".to_string()));
}

/// Verifies foreach key-value targets receive indexed integer keys and values.
#[test]
fn execute_program_foreach_assigns_indexed_keys() {
    let program =
        parse_fragment(br#"foreach (["a", "b"] as $key => $item) { echo $key . $item; }"#)
            .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let key = scope.visible_cell("key").expect("scope should contain key");
    let item = scope
        .visible_cell("item")
        .expect("scope should contain last foreach item");

    assert_eq!(values.output, "0a1b");
    assert_eq!(values.get(key), FakeValue::Int(1));
    assert_eq!(values.get(item), FakeValue::String("b".to_string()));
}

/// Verifies foreach over associative arrays preserves insertion-order keys and values.
#[test]
fn execute_program_foreach_iterates_assoc_keys_and_values() {
    let program = parse_fragment(
        br#"foreach (["a" => 1, "b" => 2] as $key => $item) { echo $key . ":" . $item . ";"; }"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "a:1;b:2;");
}

/// Verifies value-only foreach over associative arrays still yields values in insertion order.
#[test]
fn execute_program_foreach_iterates_assoc_values_only() {
    let program = parse_fragment(br#"foreach (["a" => 1, "b" => 2] as $item) { echo $item; }"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "12");
}

/// Verifies break and continue control foreach execution inside eval.
#[test]
fn execute_program_foreach_honors_break_and_continue() {
    let program = parse_fragment(
        br#"foreach ([1, 2, 3] as $item) { if ($item == 1) { continue; } echo $item; break; }"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "2");
}

/// Verifies indexed array literals and reads execute through runtime hooks.
#[test]
fn execute_program_reads_indexed_array_literal() {
    let program = parse_fragment(br#"return ["a", "b"][1];"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("b".to_string()));
}

/// Verifies legacy `array(...)` literals execute through the existing array runtime hooks.
#[test]
fn execute_program_reads_legacy_array_literal() {
    let program =
        parse_fragment(br#"return array("a", "b" => "bee",)[0];"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("a".to_string()));
}

/// Verifies associative array literals and string-key reads execute through runtime hooks.
#[test]
fn execute_program_reads_assoc_array_literal() {
    let program =
        parse_fragment(br#"return ["name" => "Ada"]["name"];"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("Ada".to_string()));
}

/// Verifies unkeyed assoc literal entries start at zero after string keys.
#[test]
fn execute_program_assoc_array_literal_unkeyed_after_string_key_starts_at_zero() {
    let program =
        parse_fragment(br#"return ["name" => "Ada", "Grace"][0];"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("Grace".to_string()));
}

/// Verifies unkeyed assoc literal entries use one plus the largest integer key.
#[test]
fn execute_program_assoc_array_literal_unkeyed_after_positive_int_key() {
    let program =
        parse_fragment(br#"return [2 => "two", "tail"][3];"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
}

/// Verifies unkeyed assoc literal entries preserve PHP's negative-key rule.
#[test]
fn execute_program_assoc_array_literal_unkeyed_after_negative_int_key() {
    let program =
        parse_fragment(br#"return [-2 => "minus", "tail"][-1];"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
}

/// Verifies numeric string literal keys update the next automatic key.
#[test]
fn execute_program_assoc_array_literal_unkeyed_after_numeric_string_key() {
    let program =
        parse_fragment(br#"return ["2" => "two", "tail"][3];"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
}

/// Verifies leading-zero string literal keys do not update the automatic key.
#[test]
fn execute_program_assoc_array_literal_unkeyed_after_leading_zero_string_key() {
    let program =
        parse_fragment(br#"return ["02" => "two", "tail"][0];"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
}

/// Verifies null literal keys normalize to empty strings without advancing automatic keys.
#[test]
fn execute_program_assoc_array_literal_unkeyed_after_null_key() {
    let program =
        parse_fragment(br#"return [null => "empty", "tail"][0];"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
}

/// Verifies null literal keys are readable through the empty-string key.
#[test]
fn execute_program_assoc_array_literal_reads_null_key_as_empty_string() {
    let program = parse_fragment(br#"return [null => "empty"][""];"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("empty".to_string()));
}

/// Verifies boolean literal keys update the next automatic key after integer normalization.
#[test]
fn execute_program_assoc_array_literal_unkeyed_after_bool_key() {
    let program =
        parse_fragment(br#"return [true => "yes", "tail"][2];"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
}

/// Verifies false literal keys update the next automatic key from zero.
#[test]
fn execute_program_assoc_array_literal_unkeyed_after_false_key() {
    let program =
        parse_fragment(br#"return [false => "no", "tail"][1];"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
}

/// Verifies float literal keys update the next automatic key after truncation.
#[test]
fn execute_program_assoc_array_literal_unkeyed_after_float_key() {
    let program =
        parse_fragment(br#"return [2.7 => "two", "tail"][3];"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
}

/// Verifies nested eval calls parse and execute against the same dynamic scope.
#[test]
fn execute_program_nested_eval_uses_same_scope() {
    let program =
        parse_fragment(br#"eval("$x = $x + 4;"); return $x;"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let x = values.int(1).expect("create fake int");
    scope.set("x", x, ScopeCellOwnership::Owned);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(5));
}

/// Verifies `__LINE__` inside eval uses the source line within the fragment.
#[test]
fn execute_program_magic_line_uses_fragment_line() {
    let program = parse_fragment(b"\nreturn __LINE__;").expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(2));
}

/// Verifies file-dependent eval magic constants use call-site metadata from the context.
#[test]
fn execute_program_magic_file_and_dir_use_context_call_site() {
    let program =
        parse_fragment(br#"return __FILE__ . "|" . __DIR__;"#).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    context.set_call_site("/tmp/main.php", "/tmp", 17);
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(
        values.get(result),
        FakeValue::String("/tmp/main.php(17) : eval()'d code|/tmp".to_string())
    );
}

/// Verifies eval class, namespace, and trait magic constants are empty in eval scope.
#[test]
fn execute_program_scope_magic_constants_are_empty_strings() {
    let program =
        parse_fragment(br#"return "[" . __CLASS__ . "|" . __NAMESPACE__ . "|" . __TRAIT__ . "]";"#)
            .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("[||]".to_string()));
}

/// Verifies eval-declared functions can be called by the same fragment.
#[test]
fn execute_program_calls_declared_function() {
    let program = parse_fragment(br#"function dyn($x) { return $x + 1; } return dyn(4);"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(5));
}

/// Verifies eval namespace declarations qualify functions and namespace magic values.
#[test]
fn execute_program_namespace_qualifies_declared_function() {
    let program = parse_fragment(
        br#"namespace Eval\Ns;
function dyn() { return __NAMESPACE__ . ":" . __FUNCTION__; }
return dyn();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.get(result),
        FakeValue::String("Eval\\Ns:Eval\\Ns\\dyn".to_string())
    );
}

/// Verifies unqualified namespaced calls fall back to global builtins when needed.
#[test]
fn execute_program_namespace_call_falls_back_to_builtin() {
    let program = parse_fragment(br#"namespace Eval\Ns; return strlen("abcd");"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(4));
}

/// Verifies namespaced dynamic functions take precedence over global builtin fallback.
#[test]
fn execute_program_namespace_function_overrides_builtin_fallback() {
    let program = parse_fragment(
        br#"namespace Eval\Ns;
function strlen($value) { return 99; }
return strlen("abcd");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(99));
}

/// Verifies unqualified namespaced constants fall back to global predefined constants.
#[test]
fn execute_program_namespace_const_fetch_falls_back_to_global() {
    let program =
        parse_fragment(br#"namespace Eval\Ns; return PHP_EOL;"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("\n".to_string()));
}

/// Verifies namespaced dynamic constants take precedence over global fallback.
#[test]
fn execute_program_namespace_const_fetch_reads_dynamic_constant_first() {
    let program =
        parse_fragment(br#"namespace Eval\Ns; return LOCAL;"#).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let value = values.int(7).expect("create fake int");
    assert!(context.define_constant("Eval\\Ns\\LOCAL", value));

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(7));
}

/// Verifies eval namespace `use function` imports dispatch to qualified dynamic functions.
#[test]
fn execute_program_namespace_use_function_import_dispatches() {
    let program = parse_fragment(
        br#"namespace Eval\Lib;
function target($x) { return $x + 1; }
namespace Eval\App;
use function Eval\Lib\target as AliasTarget;
return aliastarget(6);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(7));
}

/// Verifies eval namespace `use const` imports fetch qualified dynamic constants.
#[test]
fn execute_program_namespace_use_const_import_fetches_dynamic_constant() {
    let program = parse_fragment(
        br#"namespace Eval\App;
use const Eval\Lib\VALUE as LocalValue;
return LocalValue;"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let value = values.int(11).expect("create fake int");
    assert!(context.define_constant("Eval\\Lib\\VALUE", value));

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(11));
}

/// Verifies eval grouped namespace imports dispatch dynamic functions and constants.
#[test]
fn execute_program_grouped_namespace_use_imports_dispatch() {
    let program = parse_fragment(
        br#"namespace Eval\Lib;
function target($x) { return $x + 2; }
namespace Eval\App;
use function Eval\Lib\{target as AliasTarget};
use const Eval\Lib\{VALUE as LocalValue};
return AliasTarget(LocalValue);"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let value = values.int(5).expect("create fake int");
    assert!(context.define_constant("Eval\\Lib\\VALUE", value));

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(7));
}

/// Verifies eval-declared functions bind named arguments by parameter name.
#[test]
fn execute_program_calls_declared_function_with_named_args() {
    let program = parse_fragment(
        br#"function dyn($x, $y) { return ($x * 10) + $y; } return dyn(y: 2, x: 1);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(12));
}

/// Verifies eval-declared functions unpack indexed arrays as positional arguments.
#[test]
fn execute_program_calls_declared_function_with_spread_args() {
    let program = parse_fragment(
        br#"function dyn($x, $y) { return ($x * 10) + $y; } return dyn(...[1, 2]);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(12));
}

/// Verifies string keys unpack as named arguments for eval-declared functions.
#[test]
fn execute_program_calls_declared_function_with_named_spread_args() {
    let program = parse_fragment(
        br#"function dyn($x, $y) { return ($x * 10) + $y; } return dyn(...["y" => 2], x: 1);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(12));
}

/// Verifies eval-declared function static locals persist between calls.
#[test]
fn execute_program_static_var_persists_in_declared_function() {
    let program = parse_fragment(
        br#"function dyn() { for ($i = 0; $i < 2; $i++) { static $n = 0; $n++; } return $n; }
return (dyn() * 10) + dyn();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(24));
}

/// Verifies top-level eval static declarations reinitialize on each eval execution.
#[test]
fn execute_program_top_level_static_var_reinitializes_per_eval() {
    let program =
        parse_fragment(br#"static $n = 0; $n++; return $n;"#).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let first = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute first eval ir");
    let second = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute second eval ir");

    assert_eq!(values.get(first), FakeValue::Int(1));
    assert_eq!(values.get(second), FakeValue::Int(1));
}

/// Verifies `global` declarations read and write the context global scope.
#[test]
fn execute_program_global_alias_writes_context_global_scope() {
    let program =
        parse_fragment(br#"global $g; $g = $g + 1; return $g;"#).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut global_scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let initial = values.int(1).expect("allocate initial global");
    global_scope.set("g", initial, ScopeCellOwnership::Owned);
    context.set_global_scope(&mut global_scope);

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    let global = global_scope
        .visible_cell("g")
        .expect("global scope should contain g");
    assert_eq!(values.get(result), FakeValue::Int(2));
    assert_eq!(values.get(global), FakeValue::Int(2));
}

/// Verifies references to global aliases write the source global variable.
#[test]
fn execute_program_reference_alias_to_global_updates_source_global() {
    let program = parse_fragment(br#"global $g; $alias =& $g; $alias = 4; return $g;"#)
        .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut global_scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let initial = values.int(1).expect("allocate initial global");
    global_scope.set("g", initial, ScopeCellOwnership::Owned);
    context.set_global_scope(&mut global_scope);

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    let global = global_scope
        .visible_cell("g")
        .expect("global scope should contain g");
    assert_eq!(values.get(result), FakeValue::Int(4));
    assert_eq!(values.get(global), FakeValue::Int(4));
    assert!(global_scope.visible_cell("alias").is_none());
}

/// Verifies named calls reject positional arguments that follow named arguments.
#[test]
fn execute_program_rejects_positional_after_named_arg() {
    let program = parse_fragment(
        br#"function dyn($x, $y) { return $x + $y; } return dyn(x: 1, print "late");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values);

    assert_eq!(result, Err(EvalStatus::RuntimeFatal));
    assert_eq!(values.output, "");
}

/// Verifies named calls reject argument unpacking after named arguments.
#[test]
fn execute_program_rejects_spread_after_named_arg() {
    let program =
        parse_fragment(br#"function dyn($x, $y) { return $x + $y; } return dyn(x: 1, ...[2]);"#)
            .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values);

    assert_eq!(result, Err(EvalStatus::RuntimeFatal));
}

/// Verifies function-scope magic constants keep the eval declaration spelling.
#[test]
fn execute_program_magic_function_and_method_use_eval_declared_name() {
    let program = parse_fragment(
            br#"function DynMagicCase() { return __FUNCTION__ . ":" . __METHOD__; } return dynmagiccase();"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.get(result),
        FakeValue::String("DynMagicCase:DynMagicCase".to_string())
    );
}

/// Verifies eval-declared functions persist in a shared eval context.
#[test]
fn execute_program_context_keeps_declared_function() {
    let define =
        parse_fragment(br#"function dyn($x) { return $x + 1; }"#).expect("parse eval fragment");
    let call = parse_fragment(br#"return dyn(4);"#).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program_with_context(&mut context, &define, &mut scope, &mut values)
        .expect("execute eval ir");
    let result = execute_program_with_context(&mut context, &call, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(5));
}

/// Verifies `call_user_func` inside eval can dispatch an eval-declared function.
#[test]
fn execute_program_call_user_func_dispatches_declared_function() {
    let program = parse_fragment(
        br#"function dyn($x) { return $x + 1; }
return call_user_func("dyn", 4);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(5));
}

/// Verifies `call_user_func` inside eval can dispatch a supported builtin.
#[test]
fn execute_program_call_user_func_dispatches_builtin() {
    let program = parse_fragment(br#"return call_user_func("strlen", "abcd");"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(4));
}

/// Verifies `call_user_func` inside eval can dispatch a registered native function.
#[test]
fn execute_program_call_user_func_dispatches_registered_native_function() {
    let program =
        parse_fragment(br#"return call_user_func("native_answer");"#).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let expected = values.int(42).expect("allocate fake result");
    let native = NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 0);
    assert!(context
        .define_native_function("native_answer", native)
        .is_ok());

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(result, expected);
}

/// Verifies string variable calls inside eval can dispatch a supported builtin.
#[test]
fn execute_program_variable_call_dispatches_builtin() {
    let program = parse_fragment(
        br#"$fn = "strlen";
return $fn("abcd");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(4));
}

/// Verifies callable array entries can be invoked through postfix dynamic calls.
#[test]
fn execute_program_postfix_variable_call_dispatches_builtin() {
    let program = parse_fragment(
        br#"$callbacks = ["strlen"];
return $callbacks[0]("abc");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(3));
}

/// Verifies variable calls bind eval-declared function arguments by name.
#[test]
fn execute_program_variable_call_binds_declared_named_args() {
    let program = parse_fragment(
        br#"function dyn($x, $y) { return ($x * 10) + $y; }
$fn = "dyn";
return $fn(y: 2, x: 1);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(12));
}

/// Verifies variable calls can dispatch registered native functions with named args.
#[test]
fn execute_program_variable_call_binds_registered_native_named_args() {
    let program = parse_fragment(
        br#"$fn = "native_answer";
return $fn(right: 2, left: 1);"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let expected = values.int(42).expect("allocate fake result");
    let mut native =
        NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 2);
    assert!(native.set_param_name(0, "left"));
    assert!(native.set_param_name(1, "right"));
    assert!(context
        .define_native_function("native_answer", native)
        .is_ok());

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(result, expected);
}

/// Verifies direct callable-array variable calls dispatch object methods.
#[test]
fn execute_program_callable_array_variable_dispatches_object_method() {
    let program = parse_fragment(
        br#"$box = new Box(41);
$cb = [$box, "add_x"];
return $cb(1);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(42));
}

/// Verifies `call_user_func` dispatches callable arrays with object receivers.
#[test]
fn execute_program_call_user_func_dispatches_object_method_array() {
    let program = parse_fragment(
        br#"$box = new Box(42);
$cb = [$box, "read_x"];
return call_user_func($cb);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(42));
}

/// Verifies `call_user_func_array` dispatches callable arrays with positional args.
#[test]
fn execute_program_call_user_func_array_dispatches_object_method_array() {
    let program = parse_fragment(
        br#"$box = new Box(39);
return call_user_func_array([$box, "add2_x"], [1, 2]);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(42));
}

/// Verifies `call_user_func_array` inside eval can dispatch an eval-declared function.
#[test]
fn execute_program_call_user_func_array_dispatches_declared_function() {
    let program = parse_fragment(
        br#"function dyn($x, $y) { return $x + $y; }
return call_user_func_array("dyn", [4, 5]);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(9));
}

/// Verifies `call_user_func_array` string keys bind eval-declared parameters by name.
#[test]
fn execute_program_call_user_func_array_binds_declared_named_args() {
    let program = parse_fragment(
        br#"function dyn($x, $y) { return ($x * 10) + $y; }
return call_user_func_array("dyn", ["y" => 2, "x" => 1]);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(12));
}

/// Verifies context-level `call_user_func_array` dispatch binds eval-declared named args.
#[test]
fn execute_context_function_call_array_binds_declared_named_args() {
    let program = parse_fragment(br#"function dyn($x, $y) { return ($x * 10) + $y; }"#)
        .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let _ = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");
    let arg_array = values.assoc_new(2).expect("allocate argument array");
    let key_y = values.string("y").expect("allocate y key");
    let value_y = values.int(2).expect("allocate y value");
    let _ = values
        .array_set(arg_array, key_y, value_y)
        .expect("store y argument");
    let key_x = values.string("x").expect("allocate x key");
    let value_x = values.int(1).expect("allocate x value");
    let _ = values
        .array_set(arg_array, key_x, value_x)
        .expect("store x argument");

    let result = execute_context_function_call_array(&mut context, "dyn", arg_array, &mut values)
        .expect("execute context function call array");

    assert_eq!(values.get(result), FakeValue::Int(12));
}

/// Verifies `call_user_func_array` rejects positional values after named keys.
#[test]
fn execute_program_call_user_func_array_rejects_positional_after_named_arg() {
    let program = parse_fragment(
        br#"function dyn($x, $y) { return $x + $y; }
return call_user_func_array("dyn", ["y" => 2, 1]);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values);

    assert_eq!(result, Err(EvalStatus::RuntimeFatal));
}

/// Verifies `call_user_func_array` inside eval can dispatch a supported builtin.
#[test]
fn execute_program_call_user_func_array_dispatches_builtin() {
    let program = parse_fragment(br#"return call_user_func_array("strlen", ["abcd"]);"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(4));
}

/// Verifies `call_user_func_array` inside eval can dispatch a registered native function.
#[test]
fn execute_program_call_user_func_array_dispatches_registered_native_function() {
    let program = parse_fragment(br#"return call_user_func_array("native_answer", [4, 5]);"#)
        .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let expected = values.int(42).expect("allocate fake result");
    let native = NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 2);
    assert!(context
        .define_native_function("native_answer", native)
        .is_ok());

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(result, expected);
}

/// Verifies `call_user_func_array` named keys can bind registered native parameters.
#[test]
fn execute_program_call_user_func_array_binds_registered_native_named_args() {
    let program = parse_fragment(
        br#"return call_user_func_array("native_answer", ["right" => 2, "left" => 1]);"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let expected = values.int(42).expect("allocate fake result");
    let mut native =
        NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 2);
    assert!(native.set_param_name(0, "left"));
    assert!(native.set_param_name(1, "right"));
    assert!(context
        .define_native_function("native_answer", native)
        .is_ok());

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(result, expected);
}

/// Verifies duplicate eval-declared function names fail in a shared context.
#[test]
fn execute_program_rejects_duplicate_declared_function() {
    let define = parse_fragment(br#"function dyn() { return 1; }"#).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program_with_context(&mut context, &define, &mut scope, &mut values)
        .expect("execute first declaration");
    let err = execute_program_with_context(&mut context, &define, &mut scope, &mut values)
        .expect_err("duplicate function declaration should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies dynamic builtin calls inside eval dispatch through runtime value hooks.
#[test]
fn execute_program_dispatches_simple_builtins() {
    let program = parse_fragment(
        br#"echo strlen("abc") . ":" . count([1, [2, 3], [4]]) . ":";
echo count([1, [2, 3], [4]], COUNT_RECURSIVE) . ":";
echo call_user_func("count", [1, [2]]) . ":";
echo call_user_func_array("count", ["value" => [1, [2]], "mode" => COUNT_RECURSIVE]) . ":";
return defined("COUNT_RECURSIVE");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "3:3:6:2:3:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `json_encode()` serializes scalar, indexed, and associative values.
#[test]
fn execute_program_dispatches_json_encode_builtin() {
    let program = parse_fragment(
            br#"echo json_encode(["a" => 1, "b" => "x/y"]) . ":";
echo json_encode([1, "q", true, null]) . ":";
echo call_user_func("json_encode", "a/b\"c") . ":";
echo call_user_func_array("json_encode", ["value" => ["k" => false], "flags" => 0, "depth" => 4]) . ":";
echo json_encode("a/b", JSON_UNESCAPED_SLASHES) . ":";
echo call_user_func_array("json_encode", ["value" => "x/y", "flags" => JSON_UNESCAPED_SLASHES]) . ":";
$accent = json_decode("\"\\u00e9\"");
$emoji = json_decode("\"\\ud83d\\ude00\"");
echo bin2hex(json_encode($accent . "/" . $emoji)) . ":";
echo bin2hex(json_encode($accent . "/" . $emoji, JSON_UNESCAPED_UNICODE)) . ":";
echo bin2hex(json_encode([$accent => $emoji])) . ":";
echo bin2hex(json_encode([$accent => $emoji], JSON_UNESCAPED_UNICODE)) . ":";
echo json_encode([1, 2], JSON_FORCE_OBJECT) . ":";
echo json_encode([], JSON_FORCE_OBJECT) . ":";
echo call_user_func_array("json_encode", ["value" => [1, 2], "flags" => JSON_FORCE_OBJECT]) . ":";
echo json_encode("<>&\"" . chr(39), JSON_HEX_TAG | JSON_HEX_AMP | JSON_HEX_APOS | JSON_HEX_QUOT) . ":";
echo json_encode(["01", "+12", "1e3", " 7", "7x"], JSON_NUMERIC_CHECK) . ":";
echo json_encode([1.0, 2.5, -3.0], JSON_PRESERVE_ZERO_FRACTION) . ":";
echo (json_encode(INF) === false ? "false" : "json") . ":";
echo json_last_error() . ":" . json_last_error_msg() . ":";
echo json_encode([1.5, INF, NAN], JSON_PARTIAL_OUTPUT_ON_ERROR) . ":";
echo json_last_error() . ":" . json_last_error_msg() . ":";
$bad = "a" . hex2bin("80") . "b";
echo (json_encode($bad) === false ? "utf8-false" : "bad") . ":";
echo json_last_error() . ":";
echo bin2hex(json_encode($bad, JSON_PARTIAL_OUTPUT_ON_ERROR)) . ":";
echo json_last_error() . ":";
echo json_encode($bad, JSON_INVALID_UTF8_IGNORE) . ":";
echo json_last_error() . ":";
echo bin2hex(json_encode($bad, JSON_INVALID_UTF8_SUBSTITUTE)) . ":";
echo json_last_error() . ":";
echo bin2hex(json_encode($bad, JSON_INVALID_UTF8_SUBSTITUTE | JSON_UNESCAPED_UNICODE)) . ":";
echo json_last_error() . ":";
echo json_encode([hex2bin("6b80") => hex2bin("7680")], JSON_PARTIAL_OUTPUT_ON_ERROR) . ":";
echo json_last_error() . ":";
json_encode(3.5);
echo json_last_error() . ":" . json_last_error_msg() . ":";
echo str_replace("\n", "|", json_encode(["a" => [1, 2]], JSON_PRETTY_PRINT)) . ":";
return function_exists("json_encode") && defined("INF") && defined("NAN") && defined("JSON_UNESCAPED_SLASHES") && defined("JSON_UNESCAPED_UNICODE") && defined("JSON_FORCE_OBJECT") && defined("JSON_HEX_TAG") && defined("JSON_HEX_AMP") && defined("JSON_HEX_APOS") && defined("JSON_HEX_QUOT") && defined("JSON_NUMERIC_CHECK") && defined("JSON_PARTIAL_OUTPUT_ON_ERROR") && defined("JSON_PRETTY_PRINT") && defined("JSON_PRESERVE_ZERO_FRACTION") && defined("JSON_INVALID_UTF8_IGNORE") && defined("JSON_INVALID_UTF8_SUBSTITUTE");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        r#"{"a":1,"b":"x\/y"}:[1,"q",true,null]:"a\/b\"c":{"k":false}:"a/b":"x/y":225c75303065395c2f5c75643833645c756465303022:22c3a95c2ff09f988022:7b225c7530306539223a225c75643833645c7564653030227d:7b22c3a9223a22f09f9880227d:{"0":1,"1":2}:{}:{"0":1,"1":2}:"\u003C\u003E\u0026\u0022\u0027":[1,12,1000,7,"7x"]:[1.0,2.5,-3.0]:false:7:Inf and NaN cannot be JSON encoded:[1.5,0,0]:7:Inf and NaN cannot be JSON encoded:utf8-false:5:6e756c6c:5:"ab":0:22615c75666666646222:0:2261efbfbd6222:0:{"k\ufffd":null}:5:0:No error:{|    "a": [|        1,|        2|    ]|}:"#
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `json_decode()` materializes scalars, arrays, and associative arrays.
#[test]
fn execute_program_dispatches_json_decode_builtin() {
    let program = parse_fragment(
            br#"echo json_decode("\"hello\"") . ":";
echo json_decode("42") . ":";
echo (json_decode("true") ? "T" : "bad") . ":";
echo (is_null(json_decode("null")) ? "NULL" : "bad") . ":";
$decoded = json_decode("{\"a\":1,\"b\":[\"x\",false]}", true);
echo $decoded["a"] . ":" . $decoded["b"][0] . ":" . ($decoded["b"][1] ? "bad" : "F") . ":";
$call = call_user_func("json_decode", "[3,4]");
echo $call[1] . ":";
$named = call_user_func_array("json_decode", ["json" => "{\"k\":\"v\"}", "associative" => true, "depth" => 4, "flags" => 0]);
echo $named["k"] . ":";
$badJson = "\"a" . hex2bin("80") . "b\"";
echo (is_null(json_decode($badJson)) ? "utf8-null" : "bad") . ":";
echo json_last_error() . ":";
echo bin2hex(json_decode($badJson, true, 512, JSON_INVALID_UTF8_IGNORE)) . ":";
echo json_last_error() . ":";
echo bin2hex(json_decode($badJson, true, 512, JSON_INVALID_UTF8_SUBSTITUTE)) . ":";
echo json_last_error() . ":";
$objSub = json_decode("{\"k" . hex2bin("80") . "\":\"v" . hex2bin("80") . "\"}", true, 512, JSON_INVALID_UTF8_SUBSTITUTE);
$objSubKeys = array_keys($objSub);
echo bin2hex($objSubKeys[0]) . "=" . bin2hex($objSub[$objSubKeys[0]]) . ":";
$objIgnore = json_decode("{\"k" . hex2bin("80") . "\":\"v" . hex2bin("80") . "\"}", true, 512, JSON_INVALID_UTF8_IGNORE);
$objIgnoreKeys = array_keys($objIgnore);
echo bin2hex($objIgnoreKeys[0]) . "=" . bin2hex($objIgnore[$objIgnoreKeys[0]]) . ":";
echo (is_null(json_decode("bad")) ? "BAD" : "wrong") . ":";
$big = json_decode("[9223372036854775808]", true, 512, JSON_BIGINT_AS_STRING);
echo json_decode("9223372036854775808", true, 512, JSON_BIGINT_AS_STRING) . ":";
echo json_decode("-9223372036854775809", true, 512, JSON_BIGINT_AS_STRING) . ":";
echo gettype($big[0]) . ":" . $big[0] . ":";
echo call_user_func_array("json_decode", ["json" => "9223372036854775808", "associative" => true, "depth" => 512, "flags" => JSON_BIGINT_AS_STRING]) . ":";
return function_exists("json_decode") && defined("JSON_BIGINT_AS_STRING") && defined("JSON_INVALID_UTF8_IGNORE") && defined("JSON_INVALID_UTF8_SUBSTITUTE");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
            values.output,
            "hello:42:T:NULL:1:x:F:4:v:utf8-null:5:6162:0:61efbfbd62:0:6befbfbd=76efbfbd:6b=76:BAD:9223372036854775808:-9223372036854775809:string:9223372036854775808:9223372036854775808:"
        );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `json_decode()` returns `stdClass` objects unless assoc is true.
#[test]
fn execute_program_dispatches_json_decode_stdclass_default() {
    let program = parse_fragment(
        br#"$object = json_decode("{\"a\":1,\"b\":{\"c\":\"x\"}}");
echo $object->a . ":" . $object->b->c . ":";
$objectFalse = json_decode("{\"z\":2}", false);
echo $objectFalse->z . ":";
$objectNull = json_decode("{\"n\":{\"m\":3}}", null);
echo $objectNull->n->m . ":";
$assoc = json_decode("{\"b\":{\"c\":\"y\"}}", true);
echo $assoc["b"]["c"] . ":";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "1:x:2:3:y:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `json_encode()` serializes stdClass dynamic properties.
#[test]
fn execute_program_dispatches_json_encode_stdclass_object() {
    let program = parse_fragment(
        br#"$object = json_decode("{\"a\":1,\"b\":{\"c\":\"x\"}}");
echo json_encode($object) . ":";
echo str_replace("\n", "|", json_encode($object, JSON_PRETTY_PRINT)) . ":";
$empty = json_decode("{}");
echo json_encode($empty) . ":";
$empty->a = 7;
echo json_encode($empty);
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        r#"{"a":1,"b":{"c":"x"}}:{|    "a": 1,|    "b": {|        "c": "x"|    }|}:{}:{"a":7}"#
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `json_last_error*()` track JSON parse failures and success resets.
#[test]
fn execute_program_dispatches_json_last_error_builtins() {
    let program = parse_fragment(
            br#"echo json_last_error() . ":" . json_last_error_msg() . ":";
json_decode("bad");
echo json_last_error() . ":" . (json_last_error() === JSON_ERROR_SYNTAX ? "syntax" : "bad") . ":" . json_last_error_msg() . ":";
json_validate("[1]", 1);
echo json_last_error() . ":" . json_last_error_msg() . ":";
json_validate("\"ok\"");
echo json_last_error() . ":" . json_last_error_msg() . ":";
json_validate("\"a" . chr(10) . "b\"");
echo json_last_error() . ":" . json_last_error_msg() . ":";
json_decode("\"\\uD83D\"");
echo json_last_error() . ":" . json_last_error_msg() . ":";
json_decode("\"a" . chr(128) . "b\"");
echo json_last_error() . ":" . json_last_error_msg() . ":";
json_validate("[0]");
echo call_user_func("json_last_error") . ":" . call_user_func_array("json_last_error_msg", []) . ":";
return function_exists("json_last_error") && function_exists("json_last_error_msg") && defined("JSON_ERROR_SYNTAX");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
            values.output,
            "0:No error:4:syntax:Syntax error near location 1:1:1:Maximum stack depth exceeded near location 1:1:0:No error:3:Control character error, possibly incorrectly encoded near location 1:3:10:Single unpaired UTF-16 surrogate in unicode escape near location 1:8:5:Malformed UTF-8 characters, possibly incorrectly encoded near location 1:3:0:No error:"
        );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval JSON throw flags raise catchable Throwable objects.
#[test]
fn execute_program_dispatches_json_throw_on_error() {
    let program = parse_fragment(
        br#"try {
    json_decode("bad", true, 512, JSON_THROW_ON_ERROR);
    echo "bad";
} catch (Throwable) {
    echo "decode:";
}
try {
    json_encode(INF, JSON_THROW_ON_ERROR);
    echo "bad";
} catch (Throwable) {
    echo "encode:";
}
echo json_encode(INF, JSON_THROW_ON_ERROR | JSON_PARTIAL_OUTPUT_ON_ERROR) . ":";
return defined("JSON_THROW_ON_ERROR");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "decode:encode:0:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `json_validate()` validates documents, depth, and dynamic calls.
#[test]
fn execute_program_dispatches_json_validate_builtin() {
    let program = parse_fragment(
            br#"echo (json_validate("{\"a\":[1,true,null,\"caf\\u00e9\"]}") ? "Y" : "N") . ":";
echo (json_validate("bad") ? "bad" : "N") . ":";
echo (json_validate("[1]", 1) ? "bad" : "D") . ":";
echo (call_user_func("json_validate", "\"x\"") ? "C" : "bad") . ":";
echo (call_user_func_array("json_validate", ["json" => "[[1]]", "depth" => 3, "flags" => 0]) ? "A" : "bad") . ":";
echo (json_validate("\"a" . chr(128) . "b\"", 512, JSON_INVALID_UTF8_IGNORE) ? "I" : "bad") . ":";
echo json_last_error() . ":";
echo (json_validate("bad", 512, JSON_INVALID_UTF8_IGNORE) ? "bad" : "S") . ":";
echo json_last_error() . ":";
return function_exists("json_validate") && defined("JSON_INVALID_UTF8_IGNORE");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Y:N:D:C:A:I:0:S:4:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies direct eval builtin calls bind named and unpacked arguments.
#[test]
fn execute_program_dispatches_named_and_spread_builtins() {
    let program = parse_fragment(
        br#"echo strlen(string: "abcd");
echo ":" . (array_key_exists(array: ["name" => 1], key: "name") ? "Y" : "N");
echo ":" . (str_contains(...["haystack" => "abc", "needle" => "b"]) ? "Y" : "N");
return round(precision: 1, num: 3.14);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "4:Y:Y");
    assert_eq!(values.get(result), FakeValue::Float(3.1));
}

/// Verifies eval `ord()` returns the first byte and supports callable dispatch.
#[test]
fn execute_program_dispatches_ord_builtin() {
    let program = parse_fragment(
        br#"echo ord("A");
echo ":" . ord("");
echo ":" . call_user_func("ord", "B");
echo ":" . call_user_func_array("ord", ["C"]);
echo ":"; echo function_exists("ord");
return ord("Z");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "65:0:66:67:1");
    assert_eq!(values.get(result), FakeValue::Int(90));
}

/// Verifies eval array aggregate builtins iterate array values and support callable dispatch.
#[test]
fn execute_program_dispatches_array_aggregate_builtins() {
    let program = parse_fragment(
        br#"echo array_sum([1, 2, 3]);
echo ":" . array_product([2, 3, 4]);
echo ":" . array_sum([]);
echo ":" . array_product([]);
echo ":" . array_sum(["a" => 2, "b" => 5]);
echo ":" . call_user_func("array_sum", [3, 4]);
echo ":" . call_user_func_array("array_product", [[2, 5]]);
echo ":"; echo function_exists("array_sum");
return function_exists("array_product");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "6:24:0:1:7:7:10:1");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `array_map()` applies callbacks and preserves source keys.
#[test]
fn execute_program_dispatches_array_map_builtin() {
    let program = parse_fragment(
        br#"function eval_map_double($value) { return $value * 2; }
$mapped = array_map("eval_map_double", [1, 2, 3]);
echo $mapped[0] . ":" . $mapped[2] . ":";
$assoc = array_map("strtoupper", ["a" => "x", "b" => "y"]);
echo $assoc["a"] . ":" . $assoc["b"] . ":";
$identity = array_map(null, ["k" => "v"]);
echo $identity["k"] . ":";
function eval_map_pair($left, $right) { return $left . "-" . ($right ?? "N"); }
$pairs = array_map("eval_map_pair", ["a" => "L", "b" => "R"], ["x" => "1"]);
echo $pairs[0] . ":" . $pairs[1] . ":";
$zipped = array_map(null, [1, 2], [3, 4]);
echo $zipped[0][0] . $zipped[0][1] . ":" . $zipped[1][0] . $zipped[1][1] . ":";
$call = call_user_func("array_map", "intval", ["7"]);
echo $call[0] . ":";
$multi_call = call_user_func("array_map", "eval_map_pair", ["Q"], ["9"]);
echo $multi_call[0] . ":";
$spread = call_user_func_array("array_map", ["callback" => "strval", "array" => [8]]);
echo $spread[0] . ":";
return function_exists("array_map");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "2:6:X:Y:v:L-1:R-N:13:24:7:Q-9:8:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `array_reduce()` folds values through a string callback.
#[test]
fn execute_program_dispatches_array_reduce_builtin() {
    let program = parse_fragment(
            br#"function eval_reduce_sum($carry, $item) { return $carry + $item; }
echo array_reduce([1, 2, 3], "eval_reduce_sum", 10) . ":";
function eval_reduce_join($carry, $item) { return $carry . $item; }
echo array_reduce([4, 5], "eval_reduce_sum") . ":";
echo array_reduce(["a", "b"], "eval_reduce_join", "") . ":";
$named = array_reduce(array: [6, 7], callback: "eval_reduce_sum");
echo $named . ":";
$call = call_user_func("array_reduce", [4, 5], "eval_reduce_sum");
echo $call . ":";
$spread = call_user_func_array("array_reduce", ["array" => [2, 3], "callback" => "eval_reduce_sum", "initial" => 4]);
echo $spread . ":";
return function_exists("array_reduce");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "16:9:ab:13:9:9:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `array_walk()` invokes string callbacks with value and key cells.
#[test]
fn execute_program_dispatches_array_walk_builtin() {
    let program = parse_fragment(
            br#"function eval_walk_show($value, $key) { echo $key . "=" . $value . ";"; }
echo array_walk(["a" => 2, "b" => 3], "eval_walk_show") ? "T:" : "F:";
$call = call_user_func("array_walk", [4, 5], "eval_walk_show");
$spread = call_user_func_array("array_walk", ["array" => ["z" => 6], "callback" => "eval_walk_show"]);
return function_exists("array_walk");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "a=2;b=3;T:0=4;1=5;z=6;");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `array_pop()` and `array_shift()` write back only for direct variable calls.
#[test]
fn execute_program_dispatches_array_pop_shift_builtins() {
    let program = parse_fragment(
        br#"$a = [1, 2, 3];
echo array_pop($a) . ":" . count($a) . ":" . $a[1] . ":";
$b = ["x" => 1, 10 => 2, "y" => 3, 11 => 4];
echo array_shift(array: $b) . ":" . $b[0] . ":" . $b["y"] . ":" . $b[1] . ":";
$c = [4, 5];
echo call_user_func("array_pop", $c) . ":" . count($c) . ":" . $c[1] . ":";
$d = [6, 7];
echo call_user_func_array("array_shift", ["array" => $d]) . ":" . count($d) . ":" . $d[0] . ":";
return function_exists("array_pop") && function_exists("array_shift");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "3:2:2:1:2:3:4:5:2:5:6:2:6:");
    assert_eq!(
        values.warnings,
        vec![
            "array_pop(): Argument #1 ($array) must be passed by reference, value given",
            "array_shift(): Argument #1 ($array) must be passed by reference, value given",
        ]
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `array_push()` and `array_unshift()` write back direct variable calls.
#[test]
fn execute_program_dispatches_array_push_unshift_builtins() {
    let program = parse_fragment(
        br#"$a = [1];
echo array_push($a, 2, 3) . ":" . $a[2] . ":";
$b = ["x" => 1, 10 => 2];
echo array_push($b, "A") . ":" . $b["x"] . ":" . $b[11] . ":";
$c = [2, 3];
echo array_unshift($c, 0, 1) . ":" . $c[0] . ":" . $c[3] . ":";
$d = ["x" => 1, 10 => 2, "y" => 3];
echo array_unshift($d, "A") . ":" . $d[0] . ":" . $d["x"] . ":" . $d[1] . ":" . $d["y"] . ":";
$e = [5];
echo call_user_func("array_push", $e, 6) . ":" . count($e) . ":" . $e[0] . ":";
$f = [7];
echo call_user_func_array("array_unshift", [$f, 6]) . ":" . count($f) . ":" . $f[0] . ":";
return function_exists("array_push") && function_exists("array_unshift");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "3:3:3:1:A:4:0:3:4:A:1:2:3:2:1:5:2:1:7:");
    assert_eq!(
        values.warnings,
        vec![
            "array_push(): Argument #1 ($array) must be passed by reference, value given",
            "array_unshift(): Argument #1 ($array) must be passed by reference, value given",
        ]
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `array_splice()` returns removed values and writes back direct variable calls.
#[test]
fn execute_program_dispatches_array_splice_builtin() {
    let program = parse_fragment(
            br#"$a = [10, 20, 30, 40];
$removed = array_splice($a, 1, 2);
echo count($removed) . ":" . $removed[0] . ":" . $removed[1] . ":" . count($a) . ":" . $a[1] . ":";
$b = ["x" => 1, 10 => 2, "y" => 3, 11 => 4];
$cut = array_splice(array: $b, offset: 1, length: 2);
echo $cut[0] . ":" . $cut["y"] . ":" . $b["x"] . ":" . $b[0] . ":";
$c = [1, 2, 3, 4];
$tail = call_user_func("array_splice", $c, -2, 1);
echo $tail[0] . ":" . count($c) . ":" . $c[2] . ":";
$d = [5, 6, 7];
$all = call_user_func_array("array_splice", ["array" => $d, "offset" => 1]);
echo count($all) . ":" . $all[0] . ":" . $all[1] . ":" . count($d) . ":";
$e = [1, 2, 3, 4];
$rep = array_splice($e, 1, 2, ["A", "B"]);
echo count($rep) . ":" . $rep[0] . ":" . $rep[1] . ":" . $e[0] . ":" . $e[1] . ":" . $e[2] . ":" . $e[3] . ":";
$f = ["x" => 1, 10 => 2, "y" => 3, 11 => 4];
$rep2 = array_splice(array: $f, offset: 1, length: 2, replacement: ["s" => "S", 9 => "N"]);
echo $rep2[0] . ":" . $rep2["y"] . ":" . $f["x"] . ":" . $f[0] . ":" . $f[1] . ":" . $f[2] . ":";
$g = [1, 2, 3];
$rep3 = array_splice($g, offset: 1, replacement: [9]);
echo count($rep3) . ":" . $rep3[0] . ":" . $rep3[1] . ":" . count($g) . ":" . $g[1] . ":";
$h = [1, 2, 3];
$removed2 = call_user_func_array("array_splice", ["array" => $h, "offset" => 1, "replacement" => [9]]);
echo count($removed2) . ":" . $removed2[0] . ":" . $removed2[1] . ":" . count($h) . ":" . $h[1] . ":";
return function_exists("array_splice");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "2:20:30:2:40:2:3:1:4:3:4:3:2:6:7:3:2:2:3:1:A:B:4:2:3:1:S:N:4:2:2:3:2:9:2:2:3:3:2:"
    );
    assert_eq!(
        values.warnings,
        vec![
            "array_splice(): Argument #1 ($array) must be passed by reference, value given",
            "array_splice(): Argument #1 ($array) must be passed by reference, value given",
            "array_splice(): Argument #1 ($array) must be passed by reference, value given",
        ]
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `sort()` and `rsort()` reindex direct variable arrays only.
#[test]
fn execute_program_dispatches_sort_builtins() {
    let program = parse_fragment(
        br#"$a = [3, 1, 2];
echo sort($a) . ":" . $a[0] . $a[1] . $a[2] . ":";
$b = ["banana", "apple", "cherry"];
echo rsort(array: $b) . ":" . $b[0] . ":" . $b[2] . ":";
$c = ["x" => 3, "y" => 1, "z" => 2];
sort($c);
echo $c[0] . $c[1] . $c[2] . ":";
$d = [3, 1, 2];
echo call_user_func("sort", $d) . ":" . $d[0] . $d[1] . $d[2] . ":";
$e = [1, 2, 3];
echo call_user_func_array("rsort", ["array" => $e]) . ":" . $e[0] . ":" . $e[2] . ":";
return function_exists("sort") && function_exists("rsort");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "1:123:1:cherry:apple:123:1:312:1:1:3:");
    assert_eq!(
        values.warnings,
        vec![
            "sort(): Argument #1 ($array) must be passed by reference, value given",
            "rsort(): Argument #1 ($array) must be passed by reference, value given",
        ]
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval key-preserving array ordering builtins write back direct variable calls.
#[test]
fn execute_program_dispatches_key_preserving_sort_builtins() {
    let program = parse_fragment(
            br#"$a = ["x" => 3, "y" => 1, "z" => 2];
echo asort($a) . ":";
foreach ($a as $key => $value) { echo $key . $value; }
echo ":";
$b = ["x" => 1, "y" => 3, "z" => 2];
echo arsort(array: $b) . ":";
foreach ($b as $key => $value) { echo $key . $value; }
echo ":";
$c = ["b" => 1, "a" => 2, 3 => 4];
echo ksort($c) . ":";
foreach ($c as $key => $value) { echo $key . $value; }
echo ":";
$d = ["b" => 1, "a" => 2, 3 => 4];
echo krsort($d) . ":";
foreach ($d as $key => $value) { echo $key . $value; }
echo ":";
$e = ["x" => 2, "y" => 1];
echo call_user_func("asort", $e) . ":" . $e["x"] . $e["y"] . ":";
$f = ["b" => 1, "a" => 2];
echo call_user_func_array("krsort", ["array" => $f]) . ":" . $f["b"] . $f["a"] . ":";
return function_exists("asort") && function_exists("arsort") && function_exists("ksort") && function_exists("krsort");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "1:y1z2x3:1:y3z2x1:1:34a2b1:1:b1a234:1:21:1:12:"
    );
    assert_eq!(
        values.warnings,
        vec![
            "asort(): Argument #1 ($array) must be passed by reference, value given",
            "krsort(): Argument #1 ($array) must be passed by reference, value given",
        ]
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval natural sort builtins preserve keys and use natural string order.
#[test]
fn execute_program_dispatches_natural_sort_builtins() {
    let program = parse_fragment(
        br#"$a = ["img10", "img2", "img1"];
echo natsort($a) . ":";
foreach ($a as $key => $value) { echo $key . $value . ";"; }
echo ":";
$b = ["b" => "Img10", "a" => "img2", "c" => "IMG1"];
echo natcasesort(array: $b) . ":";
foreach ($b as $key => $value) { echo $key . $value . ";"; }
echo ":";
$c = ["x" => "b", "y" => "a"];
echo call_user_func("natsort", $c) . ":" . $c["x"] . $c["y"] . ":";
return function_exists("natsort") && function_exists("natcasesort");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "1:2img1;1img2;0img10;:1:cIMG1;aimg2;bImg10;:1:ba:"
    );
    assert_eq!(
        values.warnings,
        vec!["natsort(): Argument #1 ($array) must be passed by reference, value given"]
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `shuffle()` reindexes direct variable arrays only.
#[test]
fn execute_program_dispatches_shuffle_builtin() {
    let program = parse_fragment(
            br#"$a = ["x" => 1, "y" => 2];
echo shuffle($a) . ":" . (isset($a["x"]) ? "bad" : "reindexed") . ":" . count($a) . ":" . array_sum($a) . ":";
$b = ["x" => 1, "y" => 2];
echo call_user_func("shuffle", $b) . ":" . $b["x"] . $b["y"] . ":";
return function_exists("shuffle");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "1:reindexed:2:3:1:12:");
    assert_eq!(
        values.warnings,
        vec!["shuffle(): Argument #1 ($array) must be passed by reference, value given"]
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval user-comparator sort builtins call callbacks and write back direct arrays.
#[test]
fn execute_program_dispatches_user_sort_builtins() {
    let program = parse_fragment(
        br#"function eval_sort_cmp($left, $right) { echo "c"; return $left <=> $right; }
function eval_key_cmp($left, $right) { return strcmp($left, $right); }
$a = [3, 1, 2];
echo usort($a, "eval_sort_cmp") . ":";
foreach ($a as $value) { echo $value; }
echo ":";
$b = ["b" => 1, "a" => 3, "c" => 2];
echo uasort(array: $b, callback: "eval_sort_cmp") . ":";
foreach ($b as $key => $value) { echo $key . $value; }
echo ":";
$c = ["b" => 1, "a" => 2];
echo uksort($c, "eval_key_cmp") . ":";
foreach ($c as $key => $value) { echo $key . $value; }
echo ":";
$d = [2, 1];
echo call_user_func("usort", $d, "eval_sort_cmp") . ":" . $d[0] . $d[1] . ":";
return function_exists("usort") && function_exists("uasort") && function_exists("uksort");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "ccc1:123:ccc1:b1c2a3:1:a2b1:c1:21:");
    assert_eq!(
        values.warnings,
        vec!["usort(): Argument #1 ($array) must be passed by reference, value given"]
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval iterator array helpers support direct and dynamic builtin calls.
#[test]
fn execute_program_dispatches_iterator_array_builtins() {
    let program = parse_fragment(
            br#"$items = ["x" => 1, "y" => 2];
$copy = iterator_to_array($items);
echo iterator_count($items) . ":" . $copy["x"] . $copy["y"] . ":";
$values = iterator_to_array($items, false);
echo (isset($values["x"]) ? "bad" : "reindexed") . ":" . $values[0] . $values[1] . ":";
echo call_user_func("iterator_count", $items) . ":";
$spread = call_user_func_array("iterator_to_array", ["iterator" => $items, "preserve_keys" => false]);
echo $spread[0] . $spread[1] . ":";
return function_exists("iterator_count") && function_exists("iterator_to_array");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "2:12:reindexed:12:2:12:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `iterator_apply()` drives Iterator objects and callback args.
#[test]
fn execute_program_dispatches_iterator_apply_object_builtin() {
    let program = parse_fragment(
        br#"function eval_apply($prefix) { echo $prefix; return true; }
echo iterator_apply($it, "eval_apply", ["prefix" => "x"]) . ":";
echo call_user_func("iterator_apply", $it, "eval_apply", ["y"]) . ":";
return function_exists("iterator_apply");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let iterator = values.alloc(FakeValue::Iterator {
        len: 3,
        position: 0,
    });
    scope.set("it", iterator, ScopeCellOwnership::Borrowed);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "xxx3:yyy3:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `iterator_apply()` accepts object-method callable arrays.
#[test]
fn execute_program_iterator_apply_dispatches_object_method_array() {
    let program = parse_fragment(
        br#"$box = new Box(5);
echo iterator_apply($it, [$box, "add_x"], [1]) . ":";
return call_user_func("iterator_apply", $it, [$box, "add_x"], [1]);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let iterator = values.alloc(FakeValue::Iterator {
        len: 3,
        position: 0,
    });
    scope.set("it", iterator, ScopeCellOwnership::Borrowed);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "3:");
    assert_eq!(values.get(result), FakeValue::Int(3));
}

/// Verifies eval `iterator_apply()` counts the position where the callback stops.
#[test]
fn execute_program_iterator_apply_stops_on_falsey_callback() {
    let program = parse_fragment(
        br#"function eval_stop() { echo "s"; return false; }
return iterator_apply($it, "eval_stop");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let iterator = values.alloc(FakeValue::Iterator {
        len: 3,
        position: 0,
    });
    scope.set("it", iterator, ScopeCellOwnership::Borrowed);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "s");
    assert_eq!(values.get(result), FakeValue::Int(1));
}

/// Verifies eval `array_filter()` removes falsey values while preserving original keys.
#[test]
fn execute_program_dispatches_array_filter_builtin() {
    let program = parse_fragment(
        br#"$filtered = array_filter([0, 1, 2, "", false, null, "0", "ok"]);
echo count($filtered) . ":" . $filtered[1] . ":" . $filtered[2] . ":" . $filtered[7] . ":";
$assoc = array_filter(["a" => 0, "b" => 2, "c" => ""]);
echo (array_key_exists("a", $assoc) ? "bad" : "drop") . ":" . $assoc["b"] . ":";
$null = array_filter([0, 3], null, 1);
echo count($null) . ":" . $null[1] . ":";
$call = call_user_func("array_filter", [0, 4]);
echo count($call) . ":" . $call[1] . ":";
$spread = call_user_func_array("array_filter", ["array" => [0, 5], "callback" => null]);
echo count($spread) . ":" . $spread[1] . ":";
function eval_keep_even($value) { return $value % 2 == 0; }
$evens = array_filter([1, 2, 3, 4], "eval_keep_even");
echo count($evens) . ":" . $evens[1] . ":" . $evens[3] . ":";
function eval_keep_key($key) { return $key === "b"; }
$keyed = array_filter(["a" => 10, "b" => 20], "eval_keep_key", ARRAY_FILTER_USE_KEY);
echo count($keyed) . ":" . $keyed["b"] . ":";
function eval_keep_both($value, $key) { return $key === "c" || $value === 1; }
$both = array_filter(["a" => 1, "b" => 2, "c" => 3], "eval_keep_both", ARRAY_FILTER_USE_BOTH);
echo count($both) . ":" . $both["a"] . ":" . $both["c"] . ":";
$ints = array_filter([1, "x", 2], "is_int");
echo count($ints) . ":" . $ints[0] . ":" . $ints[2] . ":";
return function_exists("array_filter");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "3:1:2:ok:drop:2:1:3:1:4:1:5:2:2:4:1:20:2:1:3:2:1:2:"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `array_combine()` converts key values through PHP string-key rules.
#[test]
fn execute_program_dispatches_array_combine_builtin() {
    let program = parse_fragment(
        br#"$pairs = array_combine(["a", "b"], [10, 20]);
echo $pairs["a"] . ":" . $pairs["b"];
$numeric = array_combine(["1", "01"], ["n", "z"]);
echo ":" . $numeric[1] . $numeric["01"];
$scalar = array_combine([null, true, false, 2.8], ["n", "t", "f", "d"]);
echo ":" . $scalar[""] . $scalar[1] . $scalar["2.8"];
$named = array_combine(keys: ["k"], values: ["v"]);
echo ":" . $named["k"];
$call = call_user_func("array_combine", ["x"], [7]);
echo ":" . $call["x"];
$spread = call_user_func_array("array_combine", [["y"], [8]]);
echo ":" . $spread["y"] . ":";
return function_exists("array_combine");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "10:20:nz:ftd:v:7:8:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `array_column()` extracts present row columns and reindexes them.
#[test]
fn execute_program_dispatches_array_column_builtin() {
    let program = parse_fragment(
            br#"$rows = [["name" => "Ada", "score" => 10], ["score" => 20], ["name" => "Lin", "score" => 30], 42];
$names = array_column($rows, "name");
echo count($names) . ":" . $names[0] . ":" . $names[1];
$scores = array_column($rows, "score");
echo ":" . count($scores) . ":" . $scores[0] . $scores[2];
$numeric = array_column([[0 => "zero", 1 => "one"], [1 => "uno"]], 1);
echo ":" . count($numeric) . ":" . $numeric[0] . ":" . $numeric[1];
$named = array_column(array: $rows, column_key: "score");
echo ":" . $named[1];
$call = call_user_func("array_column", [["x" => 5], ["x" => 6]], "x");
echo ":" . $call[1];
$spread = call_user_func_array("array_column", [[["y" => 7], ["z" => 0], ["y" => 9]], "y"]);
echo ":" . count($spread) . ":" . $spread[1] . ":";
return function_exists("array_column");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "2:Ada:Lin:3:1030:2:one:uno:20:6:2:9:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `array_pad()` and `array_chunk()` build reindexed array shapes.
#[test]
fn execute_program_dispatches_array_shape_builtins() {
    let program = parse_fragment(
        br#"$right = array_pad([1, 2], 5, 0);
echo count($right) . ":" . $right[0] . $right[1] . $right[2] . $right[4];
$left = array_pad([1, 2], -4, 9);
echo ":" . $left[0] . $left[1] . $left[2] . $left[3];
$copy = array_pad([7, 8], 1, 0);
echo ":" . count($copy) . ":" . $copy[0] . $copy[1];
$chunks = array_chunk([1, 2, 3, 4, 5], 2);
echo ":" . count($chunks) . ":" . $chunks[0][1] . $chunks[2][0];
$named = array_pad(array: ["a"], length: 2, value: "b");
echo ":" . $named[1];
$call = call_user_func("array_chunk", [6, 7, 8], 2);
echo ":" . $call[1][0];
$spread = call_user_func_array("array_pad", [[1], 3, 2]);
echo ":" . $spread[2] . ":";
return function_exists("array_pad") && function_exists("array_chunk");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "5:1200:9912:2:78:3:25:b:8:2:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `array_slice()` observes PHP offset and length bounds.
#[test]
fn execute_program_dispatches_array_slice_builtin() {
    let program = parse_fragment(
        br#"$mid = array_slice([10, 20, 30, 40, 50], 1, 3);
echo count($mid) . ":" . $mid[0] . $mid[1] . $mid[2];
$tail = array_slice([10, 20, 30, 40], -2, 1);
echo ":" . $tail[0];
$open = array_slice([10, 20, 30, 40, 50], 2);
echo ":" . count($open) . ":" . $open[0] . $open[2];
$null_len = array_slice([5, 6, 7], 1, null);
echo ":" . $null_len[0] . $null_len[1];
$negative_len = array_slice([10, 20, 30, 40, 50], 1, -1);
echo ":" . count($negative_len) . ":" . $negative_len[0] . $negative_len[2];
$named = array_slice(array: [1, 2, 3], offset: 1, length: 1);
echo ":" . $named[0];
$call = call_user_func("array_slice", [6, 7, 8], 1, 2);
echo ":" . $call[1];
$spread = call_user_func_array("array_slice", [[9, 10, 11], 1]);
echo ":" . $spread[0] . ":";
return function_exists("array_slice");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "3:203040:30:3:3050:67:3:2040:2:8:10:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `array_merge()` appends numeric keys and overwrites string keys.
#[test]
fn execute_program_dispatches_array_merge_builtin() {
    let program = parse_fragment(
        br#"$merged = array_merge([1, 2], [3, 4]);
echo count($merged) . ":" . $merged[0] . $merged[1] . $merged[2] . $merged[3];
$left = [1, 2];
$right = [3];
$copy = array_merge($left, $right);
echo ":" . count($left) . ":" . $left[0] . ":" . $copy[2];
$assoc = array_merge(["a" => 1, 2 => "x"], ["a" => 9, 5 => "y", "b" => 3]);
echo ":" . $assoc["a"] . ":" . $assoc[0] . ":" . $assoc[1] . ":" . $assoc["b"];
$call = call_user_func("array_merge", [6], [7, 8]);
echo ":" . $call[2];
$spread = call_user_func_array("array_merge", [[9], [10]]);
echo ":" . $spread[1] . ":";
return function_exists("array_merge");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "4:1234:2:1:3:9:x:y:3:8:10:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `array_diff()` and `array_intersect()` compare values as strings.
#[test]
fn execute_program_dispatches_array_value_set_builtins() {
    let program = parse_fragment(
        br#"$diff = array_diff(["a" => 1, "b" => 2, "c" => "2", "d" => 3], [2]);
echo count($diff) . ":" . $diff["a"] . ":" . $diff["d"];
echo ":" . (array_key_exists("b", $diff) ? "bad" : "no-b");
echo ":" . (array_key_exists("c", $diff) ? "bad" : "no-c");
$inter = array_intersect(["a" => 1, "b" => 2, "c" => "2", "d" => 3], ["2", 4]);
echo ":" . count($inter) . ":" . $inter["b"] . ":" . $inter["c"];
$call = call_user_func("array_diff", [1, 2, 3], [2]);
echo ":" . count($call) . ":" . $call[0] . $call[2];
$spread = call_user_func_array("array_intersect", [[1, 2, 3], [3]]);
echo ":" . count($spread) . ":" . $spread[2] . ":";
return function_exists("array_diff") && function_exists("array_intersect");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "2:1:3:no-b:no-c:2:2:2:2:13:1:3:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `array_diff_key()` and `array_intersect_key()` preserve first-array keys.
#[test]
fn execute_program_dispatches_array_key_set_builtins() {
    let program = parse_fragment(
        br#"$diff = array_diff_key(["a" => 1, "b" => 2, 4 => 3], ["a" => 0, 5 => 0]);
echo count($diff) . ":" . $diff["b"] . ":" . $diff[4];
echo ":" . (array_key_exists("a", $diff) ? "bad" : "no-a");
$inter = array_intersect_key(["a" => 1, "b" => 2, 4 => 3], ["b" => 0, 4 => 0]);
echo ":" . count($inter) . ":" . $inter["b"] . ":" . $inter[4];
$call = call_user_func("array_diff_key", [10, 20, 30], [1 => 0]);
echo ":" . count($call) . ":" . $call[0] . $call[2];
$spread = call_user_func_array("array_intersect_key", [["x" => 7, "y" => 8], ["y" => 0]]);
echo ":" . count($spread) . ":" . $spread["y"] . ":";
return function_exists("array_diff_key") && function_exists("array_intersect_key");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "2:2:3:no-a:2:2:3:2:1030:1:8:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `range()` builds inclusive ascending and descending integer arrays.
#[test]
fn execute_program_dispatches_range_builtin() {
    let program = parse_fragment(
        br#"$up = range(1, 4);
echo count($up) . ":" . $up[0] . $up[3];
$down = range(4, 1);
echo ":" . count($down) . ":" . $down[0] . $down[3];
$single = range(3, 3);
echo ":" . count($single) . ":" . $single[0];
$named = range(start: 2, end: 4);
echo ":" . $named[0] . $named[2];
$call = call_user_func("range", 5, 7);
echo ":" . $call[2];
$spread = call_user_func_array("range", [8, 6]);
echo ":" . count($spread) . ":" . $spread[0] . $spread[2] . ":";
return function_exists("range");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "4:14:4:41:1:3:24:7:3:86:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `array_rand()` returns a key that exists in the source array.
#[test]
fn execute_program_dispatches_array_rand_builtin() {
    let program = parse_fragment(
        br#"$nums = [10, 20, 30];
$idx = array_rand($nums);
echo ($idx >= 0 && $idx < 3 && array_key_exists($idx, $nums)) ? "idx" : "bad";
$assoc = ["a" => 1, "b" => 2];
$key = array_rand($assoc);
echo ":" . (array_key_exists($key, $assoc) ? "assoc" : "bad");
$named = array_rand(array: [5, 6]);
echo ":" . (($named >= 0 && $named < 2) ? "named" : "bad");
$call = call_user_func("array_rand", [7, 8]);
echo ":" . (($call >= 0 && $call < 2) ? "call" : "bad");
$spread = call_user_func_array("array_rand", [["x" => 1, "y" => 2]]);
echo ":" . (array_key_exists($spread, ["x" => 1, "y" => 2]) ? "spread" : "bad") . ":";
return function_exists("array_rand");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "idx:assoc:named:call:spread:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval random builtins return values inside PHP inclusive ranges.
#[test]
fn execute_program_dispatches_rand_builtins() {
    let program = parse_fragment(
        br#"$plain = rand();
echo ($plain >= 0 && $plain <= 2147483647) ? "plain" : "bad";
$bounded = rand(2, 4);
echo ":" . (($bounded >= 2 && $bounded <= 4) ? "range" : "bad");
$same = mt_rand(max: 6, min: 6);
echo ":" . ($same === 6 ? "same" : "bad");
$swapped = rand(10, 1);
echo ":" . (($swapped >= 1 && $swapped <= 10) ? "swap" : "bad");
$call = call_user_func("mt_rand", 1, 1);
echo ":" . ($call === 1 ? "call" : "bad");
$spread = call_user_func_array("rand", ["min" => 3, "max" => 3]);
echo ":" . ($spread === 3 ? "spread" : "bad") . ":";
$secure = random_int(max: 4, min: 4);
echo ($secure === 4 ? "random" : "bad") . ":";
$secureCall = call_user_func("random_int", 5, 5);
echo ($secureCall === 5 ? "random-call" : "bad") . ":";
$secureSpread = call_user_func_array("random_int", ["min" => 6, "max" => 6]);
echo ($secureSpread === 6 ? "random-spread" : "bad") . ":";
echo function_exists("rand");
echo function_exists("mt_rand");
return function_exists("random_int");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "plain:range:same:swap:call:spread:random:random-call:random-spread:11"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `array_fill()` and `array_fill_keys()` create arrays with PHP key rules.
#[test]
fn execute_program_dispatches_array_fill_builtins() {
    let program = parse_fragment(
        br#"$filled = array_fill(2, 3, "x");
echo count($filled) . ":" . $filled[2] . $filled[4];
$negative = array_fill(-2, 3, 7);
echo ":" . $negative[-2] . $negative[-1] . $negative[0];
$empty = array_fill(5, 0, "x");
echo ":" . count($empty);
$map = array_fill_keys(["a", "1", "01"], 8);
echo ":" . $map["a"] . ":" . $map[1] . ":" . $map["01"];
$named = array_fill(start_index: 1, count: 2, value: "n");
echo ":" . $named[1] . $named[2];
$call = call_user_func("array_fill", 0, 2, "c");
echo ":" . $call[0] . $call[1];
$spread = call_user_func_array("array_fill_keys", [["x", "y"], "z"]);
echo ":" . $spread["x"] . $spread["y"] . ":";
return function_exists("array_fill") && function_exists("array_fill_keys");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "3:xx:777:0:8:8:8:nn:cc:zz:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `array_flip()` swaps valid values into PHP-normalized keys.
#[test]
fn execute_program_dispatches_array_flip_builtin() {
    let program = parse_fragment(
            br#"$flipped = array_flip(["a" => "x", "b" => "y", "c" => "x", "d" => 1, "e" => "01", "skip" => null, "truth" => true]);
echo $flipped["x"] . ":" . $flipped["y"] . ":" . $flipped[1] . ":" . $flipped["01"] . ":" . count($flipped);
$named = array_flip(array: ["k" => "v"]);
echo ":" . $named["v"];
$call = call_user_func("array_flip", ["left" => "right"]);
echo ":" . $call["right"];
$spread = call_user_func_array("array_flip", [["n" => 9]]);
echo ":" . $spread[9] . ":";
return function_exists("array_flip");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "c:b:d:e:4:k:left:n:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `array_unique()` preserves first keys using default string comparison.
#[test]
fn execute_program_dispatches_array_unique_builtin() {
    let program = parse_fragment(
        br#"$unique = array_unique(["a", "b", "a", "2", 2]);
echo count($unique) . ":" . $unique[0] . $unique[1] . $unique[3];
$assoc = array_unique(["x" => "a", "y" => "b", "z" => "a"]);
echo ":" . count($assoc) . ":" . $assoc["x"] . $assoc["y"];
$scalar = array_unique([1, "1", 1.0, true, false, null, ""]);
echo ":" . count($scalar) . ":" . $scalar[0] . ":";
echo $scalar[4] ? "bad" : "F";
$named = array_unique(array: ["k" => "v", "l" => "v"]);
echo ":" . $named["k"] . ":" . count($named);
$call = call_user_func("array_unique", ["q", "q", "r"]);
echo ":" . $call[0] . $call[2];
$spread = call_user_func_array("array_unique", [["s", "s", "t"]]);
echo ":" . $spread[0] . $spread[2] . ":";
return function_exists("array_unique");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "3:ab2:2:ab:2:1:F:v:1:qr:st:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval array projection builtins produce indexed key/value arrays.
#[test]
fn execute_program_dispatches_array_projection_builtins() {
    let program = parse_fragment(
        br#"$values = array_values(["a" => 10, "b" => 20]);
echo $values[0] . ":" . $values[1];
$keys = array_keys(["a" => 10, "b" => 20]);
echo ":" . $keys[0] . ":" . $keys[1];
echo ":" . count(array_values([]));
$call_keys = call_user_func("array_keys", ["z" => 7]);
echo ":" . $call_keys[0];
$call_values = call_user_func_array("array_values", [["q" => 8]]);
echo ":" . $call_values[0];
echo ":"; echo function_exists("array_keys");
return function_exists("array_values");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "10:20:a:b:0:z:8:1");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `array_reverse()` handles PHP key preservation rules.
#[test]
fn execute_program_dispatches_array_reverse_builtin() {
    let program = parse_fragment(
        br#"$indexed = array_reverse([1, 2, 3]);
echo $indexed[0]; echo $indexed[1]; echo $indexed[2]; echo ":";
$mixed = array_reverse([2 => "a", "k" => "b", 5 => "c"]);
echo $mixed[0]; echo $mixed["k"]; echo $mixed[1]; echo ":";
$preserved = array_reverse([2 => "a", "k" => "b", 5 => "c"], true);
echo $preserved[5]; echo $preserved["k"]; echo $preserved[2]; echo ":";
$named = array_reverse(array: ["x", "y"], preserve_keys: true);
echo $named[1]; echo $named[0]; echo ":";
$call = call_user_func("array_reverse", [4, 5]);
echo $call[0]; echo $call[1]; echo ":";
$spread = call_user_func_array("array_reverse", [[6, 7]]);
echo $spread[0]; echo $spread[1]; echo ":";
return function_exists("array_reverse");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "321:cba:cba:yx:54:76:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `array_key_exists()` distinguishes present null values from missing keys.
#[test]
fn execute_program_dispatches_array_key_exists_builtin() {
    let program = parse_fragment(
        br#"$map = ["name" => null, "age" => 30];
echo array_key_exists("name", $map) ? "Y" : "N"; echo ":";
echo array_key_exists("missing", $map) ? "bad" : "N"; echo ":";
echo array_key_exists(1, [10, null]) ? "Y" : "N"; echo ":";
echo array_key_exists(2, [10, null]) ? "bad" : "N"; echo ":";
echo call_user_func("array_key_exists", "age", $map) ? "Y" : "N"; echo ":";
echo call_user_func_array("array_key_exists", ["age", $map]) ? "Y" : "N"; echo ":";
return function_exists("array_key_exists");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Y:N:Y:N:Y:Y:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval array search builtins use loose comparison and return keys or booleans.
#[test]
fn execute_program_dispatches_array_search_builtins() {
    let program = parse_fragment(
        br#"echo in_array(2, [1, 2, 3]) ? "Y" : "bad";
echo ":"; echo in_array(4, [1, 2, 3]) ? "bad" : "N";
echo ":" . array_search(20, [10, 20, 30]);
echo ":" . array_search("Grace", ["name" => "Grace"]);
echo ":"; echo array_search("x", ["name" => "Grace"]) === false ? "F" : "bad";
echo ":"; echo call_user_func("in_array", "b", ["a", "b"]) ? "C" : "bad";
$found = call_user_func_array("array_search", ["v", ["k" => "v"]]);
echo ":" . $found;
echo ":"; echo function_exists("in_array");
return function_exists("array_search");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Y:N:1:name:F:C:k:1");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `explode()` and `implode()` bridge byte strings and arrays.
#[test]
fn execute_program_dispatches_explode_implode_builtins() {
    let program = parse_fragment(
        br#"$parts = explode(",", "a,b,");
echo count($parts); echo ":" . $parts[0] . ":" . $parts[1] . ":" . $parts[2];
echo ":" . implode("|", $parts);
echo ":" . implode(separator: "-", array: ["x", 2, true, null]);
$call_parts = call_user_func("explode", ":", "m:n");
echo ":" . $call_parts[1];
echo ":" . call_user_func_array("implode", ["separator" => "/", "array" => ["p", "q"]]);
echo ":"; echo function_exists("explode");
return function_exists("implode");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "3:a:b::a|b|:x-2-1-:n:p/q:1");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `str_split()` builds indexed arrays of fixed-width chunks.
#[test]
fn execute_program_dispatches_str_split_builtin() {
    let program = parse_fragment(
        br#"$letters = str_split("abc");
echo count($letters) . ":" . $letters[0] . $letters[1] . $letters[2]; echo ":";
$pairs = str_split(string: "abcd", length: 2);
echo $pairs[0] . "-" . $pairs[1]; echo ":";
$empty = str_split("");
echo count($empty); echo ":";
$call = call_user_func("str_split", "xyz", 2);
echo $call[0] . "-" . $call[1]; echo ":";
$named = call_user_func_array("str_split", ["string" => "pqrs", "length" => 3]);
echo $named[0] . "-" . $named[1]; echo ":";
return function_exists("str_split");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "3:abc:ab-cd:0:xy-z:pqr-s:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `str_pad()` supports PHP left, right, and both-side padding modes.
#[test]
fn execute_program_dispatches_str_pad_builtin() {
    let program = parse_fragment(
            br#"echo "[" . str_pad("hi", 5) . "]"; echo ":";
echo "[" . str_pad(string: "hi", length: 5, pad_string: "_", pad_type: 0) . "]"; echo ":";
echo "[" . str_pad("x", 6, "ab", 2) . "]"; echo ":";
echo call_user_func("str_pad", "42", 5, "0", 0); echo ":";
echo call_user_func_array("str_pad", ["string" => "x", "length" => 3, "pad_string" => "."]); echo ":";
return function_exists("str_pad");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "[hi   ]:[___hi]:[abxaba]:00042:x..:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval string replacement builtins support direct, named, and callable dispatch.
#[test]
fn execute_program_dispatches_string_replace_builtins() {
    let program = parse_fragment(
            br#"echo str_replace("o", "0", "Hello World"); echo ":";
echo str_replace(search: "aa", replace: "b", subject: "aaaa"); echo ":";
echo str_replace("", "x", "abc"); echo ":";
echo str_ireplace("HE", "ye", "Hello he"); echo ":";
echo call_user_func("str_replace", "l", "L", "hello"); echo ":";
echo call_user_func_array("str_ireplace", ["search" => "x", "replace" => "Y", "subject" => "xX"]); echo ":";
echo function_exists("str_replace");
return function_exists("str_ireplace");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Hell0 W0rld:bb:abc:yello ye:heLLo:YY:1");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval regex builtins handle captures, replacement, callbacks, and splitting.
#[test]
fn execute_program_dispatches_preg_builtins() {
    let program = parse_fragment(
            br#"$ok = preg_match("/([a-z]+)([0-9]+)/", "id42", $matches);
echo $ok . ":" . count($matches) . ":" . $matches[0] . ":" . $matches[1] . ":" . $matches[2] . ":";
echo preg_match("/xyz/", "id42") . ":";
echo preg_match_all("/[0-9]+/", "a1b22c333") . ":";
$allCount = preg_match_all("/([a-z]+)([0-9]+)/", "a1 b22", $all);
echo $allCount . ":" . count($all) . ":" . $all[0][1] . ":" . $all[1][0] . ":" . $all[2][1] . ":";
$setCount = preg_match_all("/([a-z]+)([0-9]+)/", "a1 b22", $set, PREG_SET_ORDER);
echo $setCount . ":" . count($set) . ":" . $set[0][0] . ":" . $set[0][1] . ":" . $set[1][2] . ":";
preg_match("/(a)?(b)/", "b", $offsetOne, PREG_OFFSET_CAPTURE);
echo $offsetOne[0][0] . ":" . $offsetOne[0][1] . ":" . $offsetOne[1][0] . ":" . $offsetOne[1][1] . ":" . $offsetOne[2][0] . ":" . $offsetOne[2][1] . ":";
preg_match_all("/([a-z]+)([0-9]+)/", "a1 b22", $offsetAll, PREG_OFFSET_CAPTURE);
echo $offsetAll[0][1][0] . ":" . $offsetAll[0][1][1] . ":" . $offsetAll[1][0][1] . ":" . $offsetAll[2][1][1] . ":";
preg_match_all("/([a-z]+)([0-9]+)/", "a1 b22", $offsetSet, PREG_SET_ORDER | PREG_OFFSET_CAPTURE);
echo $offsetSet[1][0][0] . ":" . $offsetSet[1][0][1] . ":" . $offsetSet[0][2][1] . ":";
preg_match("/(a)?(b)(c)?/", "b", $nullOne, PREG_UNMATCHED_AS_NULL);
echo count($nullOne) . ":" . ($nullOne[1] === null ? "n" : "bad") . ":" . $nullOne[2] . ":" . ($nullOne[3] === null ? "n" : "bad") . ":";
preg_match("/(a)?(b)(c)?/", "b", $nullOffset, PREG_UNMATCHED_AS_NULL | PREG_OFFSET_CAPTURE);
echo ($nullOffset[1][0] === null ? "n" : "bad") . ":" . $nullOffset[1][1] . ":" . ($nullOffset[3][0] === null ? "n" : "bad") . ":" . $nullOffset[3][1] . ":";
preg_match_all("/(a)?(b)(c)?/", "b", $nullAll, PREG_UNMATCHED_AS_NULL);
echo ($nullAll[1][0] === null ? "n" : "bad") . ":" . $nullAll[2][0] . ":" . ($nullAll[3][0] === null ? "n" : "bad") . ":";
preg_match_all("/(a)?(b)(c)?/", "b", $nullSet, PREG_SET_ORDER | PREG_UNMATCHED_AS_NULL | PREG_OFFSET_CAPTURE);
echo ($nullSet[0][1][0] === null ? "n" : "bad") . ":" . $nullSet[0][1][1] . ":" . ($nullSet[0][3][0] === null ? "n" : "bad") . ":" . $nullSet[0][3][1] . ":";
preg_match_all("/(x)(y)/", "abc", $none);
echo count($none) . ":" . count($none[0]) . ":" . count($none[1]) . ":" . count($none[2]) . ":";
echo preg_replace("/([a-z])([0-9])/", "$2-$1", "a1 b2") . ":";
function eval_regex_wrap($matches) { return "[" . $matches[0] . "]"; }
echo preg_replace_callback("/[A-Z]/", "eval_regex_wrap", "AB") . ":";
$limited = preg_split("/,/", "a,b,c", 2);
echo count($limited) . ":" . $limited[0] . ":" . $limited[1] . ":";
$kept = preg_split("/,/", "a,,b", 0, PREG_SPLIT_NO_EMPTY);
echo count($kept) . ":" . $kept[1] . ":";
echo call_user_func("preg_match", "/x/", "x") . ":";
$replaced = call_user_func_array("preg_replace", ["pattern" => "/[0-9]+/", "replacement" => "N", "subject" => "a12"]);
echo $replaced . ":";
$captured = preg_split("/(,)/", "a,b", 0, PREG_SPLIT_DELIM_CAPTURE);
echo count($captured) . ":" . $captured[1] . ":";
$splitOffsets = preg_split("/,/", "a,b,c", 2, PREG_SPLIT_OFFSET_CAPTURE);
echo $splitOffsets[0][0] . ":" . $splitOffsets[0][1] . ":" . $splitOffsets[1][0] . ":" . $splitOffsets[1][1] . ":";
$splitBoth = preg_split("/(,)/", "a,b", 0, PREG_SPLIT_DELIM_CAPTURE | PREG_SPLIT_OFFSET_CAPTURE);
echo count($splitBoth) . ":" . $splitBoth[1][0] . ":" . $splitBoth[1][1] . ":";
$splitNoEmpty = preg_split("/,/", "a,,b", 0, PREG_SPLIT_NO_EMPTY | PREG_SPLIT_OFFSET_CAPTURE);
echo $splitNoEmpty[1][0] . ":" . $splitNoEmpty[1][1] . ":";
return function_exists("preg_match") && function_exists("preg_match_all") && function_exists("preg_replace") && function_exists("preg_replace_callback") && function_exists("preg_split") && defined("PREG_SPLIT_NO_EMPTY") && defined("PREG_SET_ORDER") && defined("PREG_OFFSET_CAPTURE") && defined("PREG_SPLIT_OFFSET_CAPTURE") && defined("PREG_UNMATCHED_AS_NULL");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
            values.output,
            "1:3:id42:id:42:0:3:2:3:b22:a:22:2:2:a1:a:22:b:0::-1:b:0:b22:3:0:4:b22:3:1:4:n:b:n:n:-1:n:-1:n:b:n:n:-1:n:-1:3:0:0:0:1-a 2-b:[A][B]:2:a:b,c:2:b:1:aN:3:,:a:0:b,c:2:3:,:1:b:3:"
        );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval HTML entity builtins encode, decode, and dispatch as callables.
#[test]
fn execute_program_dispatches_html_entity_builtins() {
    let program = parse_fragment(
        br#"echo htmlspecialchars("<b>\"Hi\" & 'bye'</b>"); echo ":";
echo htmlentities(string: "<a>"); echo ":";
echo html_entity_decode("&lt;b&gt;hi&lt;/b&gt;"); echo ":";
echo call_user_func("htmlspecialchars", "<x>"); echo ":";
echo call_user_func_array("html_entity_decode", ["string" => "&quot;q&quot;"]); echo ":";
echo function_exists("htmlspecialchars"); echo function_exists("htmlentities");
return function_exists("html_entity_decode");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
            values.output,
            "&lt;b&gt;&quot;Hi&quot; &amp; &#039;bye&#039;&lt;/b&gt;:&lt;a&gt;:<b>hi</b>:&lt;x&gt;:\"q\":11"
        );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval URL codec builtins dispatch through direct, named, and callable paths.
#[test]
fn execute_program_dispatches_url_codec_builtins() {
    let program = parse_fragment(
        br#"echo urlencode("a b&=~"); echo ":";
echo rawurlencode(string: "a b&=~"); echo ":";
echo urldecode("a+b%26%3D%7E"); echo ":";
echo rawurldecode("a+b%26%3D%7E"); echo ":";
echo call_user_func("urlencode", "%zz"); echo ":";
echo call_user_func_array("rawurldecode", ["string" => "x%2By%zz"]); echo ":";
echo function_exists("urlencode"); echo function_exists("rawurlencode");
echo function_exists("urldecode");
return function_exists("rawurldecode");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "a+b%26%3D%7E:a%20b%26%3D~:a b&=~:a+b&=~:%25zz:x+y%zz:111"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `ctype_*` predicates dispatch through direct, named, and callable paths.
#[test]
fn execute_program_dispatches_ctype_builtins() {
    let program = parse_fragment(
        br#"echo ctype_alpha("abc") ? "A" : "-"; echo ":";
echo ctype_digit(text: "123") ? "D" : "-"; echo ":";
echo ctype_alnum("a1") ? "N" : "-"; echo ":";
echo ctype_space(" \t\n" . chr(11) . chr(12) . "\r") ? "S" : "-"; echo ":";
echo ctype_alpha("") ? "bad" : "empty"; echo ":";
echo call_user_func("ctype_digit", "12x") ? "bad" : "not-digit"; echo ":";
echo call_user_func_array("ctype_space", ["text" => " x"]) ? "bad" : "not-space"; echo ":";
echo function_exists("ctype_alpha"); echo function_exists("ctype_digit");
echo function_exists("ctype_alnum");
return function_exists("ctype_space");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "A:D:N:S:empty:not-digit:not-space:111");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `crc32()` returns PHP-compatible non-negative checksums.
#[test]
fn execute_program_dispatches_crc32_builtin() {
    let program = parse_fragment(
            br#"echo crc32(""); echo ":";
echo crc32(string: "123456789"); echo ":";
echo call_user_func("crc32", "hello"); echo ":";
echo call_user_func_array("crc32", ["string" => "The quick brown fox jumps over the lazy dog"]); echo ":";
return function_exists("crc32");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "0:3421780262:907060870:1095738169:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `hash_algos()` returns supported hash names through callable dispatch too.
#[test]
fn execute_program_dispatches_hash_algos_builtin() {
    let program = parse_fragment(
        br#"$algos = hash_algos();
echo count($algos) . ":" . $algos[0] . ":" . $algos[5] . ":";
echo in_array("crc32c", $algos) ? "crc" : "bad";
$call = call_user_func("hash_algos");
echo ":" . $call[18];
$spread = call_user_func_array("hash_algos", []);
echo ":" . $spread[27] . ":";
echo function_exists("hash_algos") ? "exists" : "missing";
return count($algos);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "28:md2:sha256:crc:whirlpool:joaat:exists");
    assert_eq!(values.get(result), FakeValue::Int(28));
}

/// Verifies eval one-shot hash digest builtins use the crypto bridge and dispatch dynamically.
#[test]
fn execute_program_dispatches_hash_digest_builtins() {
    let filename = format!("elephc_eval_hash_file_{}.txt", std::process::id());
    let source = format!(
        r#"echo md5("abc"); echo ":";
echo sha1(string: "abc"); echo ":";
echo hash("sha256", "abc"); echo ":";
echo hash_hmac(algo: "sha256", data: "data", key: "key"); echo ":";
echo bin2hex(md5("abc", true)); echo ":";
echo bin2hex(call_user_func("sha1", "abc", true)); echo ":";
echo call_user_func_array("hash", ["algo" => "md5", "data" => "abc"]); echo ":";
echo call_user_func_array("hash_hmac", ["algo" => "sha256", "data" => "data", "key" => "key"]); echo ":";
file_put_contents("{filename}", "abc");
echo hash_file("sha256", "{filename}"); echo ":";
echo bin2hex(hash_file(algo: "md5", filename: "{filename}", binary: true)); echo ":";
echo call_user_func_array("hash_file", ["algo" => "md5", "filename" => "{filename}"]); echo ":";
echo hash_file("sha256", "{filename}.missing") === false ? "missing" : "bad"; echo ":";
unlink("{filename}");
echo function_exists("md5"); echo function_exists("sha1"); echo function_exists("hash"); echo function_exists("hash_file");
return function_exists("hash_hmac");"#,
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        concat!(
            "900150983cd24fb0d6963f7d28e17f72:",
            "a9993e364706816aba3e25717850c26c9cd0d89d:",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad:",
            "5031fe3d989c6d1537a013fa6e739da23463fdaec3b70137d828e36ace221bd0:",
            "900150983cd24fb0d6963f7d28e17f72:",
            "a9993e364706816aba3e25717850c26c9cd0d89d:",
            "900150983cd24fb0d6963f7d28e17f72:",
            "5031fe3d989c6d1537a013fa6e739da23463fdaec3b70137d828e36ace221bd0:",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad:",
            "900150983cd24fb0d6963f7d28e17f72:",
            "900150983cd24fb0d6963f7d28e17f72:",
            "missing:",
            "1111"
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval zero-argument system builtins return native-compatible values.
#[test]
fn execute_program_dispatches_zero_arg_system_builtins() {
    let program = parse_fragment(
        br#"echo time() > 1000000000 ? "time" : "bad"; echo ":";
echo phpversion(); echo ":";
echo sys_get_temp_dir(); echo ":";
echo strlen(getcwd()) > 0 ? "cwd" : "bad"; echo ":";
echo call_user_func("time") > 1000000000 ? "call-time" : "bad"; echo ":";
echo call_user_func("phpversion"); echo ":";
echo call_user_func_array("getcwd", []) !== "" ? "call-cwd" : "bad"; echo ":";
echo call_user_func_array("sys_get_temp_dir", []); echo ":";
echo function_exists("time"); echo function_exists("phpversion"); echo function_exists("getcwd");
return function_exists("sys_get_temp_dir");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        format!(
            "time:{}:/tmp:cwd:call-time:{}:call-cwd:/tmp:111",
            eval_compiler_php_version(),
            eval_compiler_php_version()
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `date()` formats libc local timestamps and `mktime()` builds them.
#[test]
fn execute_program_dispatches_date_mktime_builtins() {
    let program = parse_fragment(
            br#"$ts = mktime(13, 2, 3, 1, 2, 2024);
echo date("Y-m-d H:i:s", $ts);
echo ":" . date("j-n-G-g-A-a-N-D-M-l-F", $ts);
echo ":" . (date("U", $ts) === strval($ts) ? "U" : "bad");
echo ":" . call_user_func("date", "Y", $ts);
$named = call_user_func_array("mktime", ["hour" => 0, "minute" => 0, "second" => 0, "month" => 1, "day" => 1, "year" => 2000]);
echo ":" . date(format: "Y", timestamp: $named);
echo ":"; echo function_exists("date");
return function_exists("mktime");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "2024-01-02 13:02:03:2-1-13-1-PM-pm-2-Tue-Jan-Tuesday-January:U:2024:2000:1"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `strtotime()` parses supported ISO date strings and rejects others.
#[test]
fn execute_program_dispatches_strtotime_builtin() {
    let program = parse_fragment(
        br#"$date = strtotime("2024-06-15");
echo date("Y-m-d H:i:s", $date);
$full = strtotime("2024-06-15 12:30:45");
echo ":" . date("Y-m-d H:i:s", $full);
$short = strtotime("2024-06-15T12:30");
echo ":" . date("Y-m-d H:i:s", $short);
echo ":" . (strtotime("2024/06/15") === -1 ? "bad" : "wrong");
$call = call_user_func("strtotime", "2024-01-02 03:04:05");
echo ":" . date("Y-m-d H:i:s", $call);
$spread = call_user_func_array("strtotime", ["datetime" => "2024-01-02"]);
echo ":" . date("Y-m-d", $spread) . ":";
return function_exists("strtotime");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
            values.output,
            "2024-06-15 00:00:00:2024-06-15 12:30:45:2024-06-15 12:30:00:bad:2024-01-02 03:04:05:2024-01-02:"
        );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `microtime()` returns a plausible float timestamp by all call paths.
#[test]
fn execute_program_dispatches_microtime_builtin() {
    let program = parse_fragment(
        br#"echo microtime() > 1000000000 ? "now" : "bad"; echo ":";
echo microtime(as_float: false) > 1000000000 ? "named" : "bad"; echo ":";
echo call_user_func("microtime", true) > 1000000000 ? "call" : "bad"; echo ":";
echo call_user_func_array("microtime", ["as_float" => true]) > 1000000000 ? "array" : "bad";
echo ":";
return function_exists("microtime");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "now:named:call:array:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval realpath-cache stubs match elephc's empty-cache runtime view.
#[test]
fn execute_program_dispatches_realpath_cache_builtins() {
    let program = parse_fragment(
        br#"$cache = realpath_cache_get();
echo count($cache) . ":" . realpath_cache_size() . ":";
$call_cache = call_user_func("realpath_cache_get");
echo count($call_cache) . ":";
echo call_user_func_array("realpath_cache_size", []) . ":";
echo function_exists("realpath_cache_get");
return function_exists("realpath_cache_size");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "0:0:0:0:1");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval stream introspection builtins return native-compatible static lists.
#[test]
fn execute_program_dispatches_stream_introspection_builtins() {
    let program = parse_fragment(
        br#"$wrappers = stream_get_wrappers();
$transports = stream_get_transports();
$filters = stream_get_filters();
echo count($wrappers) . ":" . $wrappers[0] . ":" . $wrappers[5] . ":";
echo count($transports) . ":" . $transports[0] . ":" . $transports[8] . ":";
echo count($filters) . ":" . $filters[2] . ":";
$call_wrappers = call_user_func("stream_get_wrappers");
echo $call_wrappers[10] . ":";
$call_transports = call_user_func_array("stream_get_transports", []);
echo $call_transports[11] . ":";
$call_filters = call_user_func_array("stream_get_filters", []);
echo $call_filters[13] . ":";
echo function_exists("stream_get_wrappers"); echo function_exists("stream_get_transports");
return function_exists("stream_get_filters");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "11:file:https:12:tcp:tlsv1.0:14:string.rot13:glob:tlsv1.3:bzip2.decompress:11"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `spl_classes()` returns the native-compatible SPL type snapshot.
#[test]
fn execute_program_dispatches_spl_classes_builtin() {
    let program = parse_fragment(
        br#"$names = spl_classes();
echo count($names) . ":" . $names[0] . ":" . $names[55] . ":";
echo (in_array("Exception", $names) ? "exception" : "bad") . ":";
echo (in_array("SplDoublyLinkedList", $names) ? "list" : "bad") . ":";
$call = call_user_func("spl_classes");
echo (in_array("Throwable", $call) ? "call" : "bad") . ":";
$spread = call_user_func_array("spl_classes", []);
echo (count($spread) === count($names) ? "spread" : "bad") . ":";
echo function_exists("spl_classes");
return is_callable("spl_classes");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "61:AppendIterator:Throwable:exception:list:call:spread:1"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval SPL object identity builtins are stable, unique, and callable.
#[test]
fn execute_program_dispatches_spl_object_identity_builtins() {
    let program = parse_fragment(
            br#"$a = new KnownClass();
$b = new KnownClass();
echo (spl_object_id($a) === spl_object_id($a)) ? "stable" : "drift";
echo ":";
echo (spl_object_id($a) !== spl_object_id($b)) ? "unique" : "same";
echo ":";
echo (spl_object_hash(object: $a) === spl_object_hash($a)) ? "hash" : "bad";
echo ":";
echo (call_user_func("spl_object_id", $a) === spl_object_id($a)) ? "call" : "bad";
echo ":";
echo (call_user_func_array("spl_object_hash", ["object" => $b]) === spl_object_hash($b)) ? "array" : "bad";
echo ":";
echo function_exists("spl_object_id");
return function_exists("spl_object_hash");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "stable:unique:hash:call:array:1");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval environment builtins read, write, unset, and dispatch dynamically.
#[test]
fn execute_program_dispatches_environment_builtins() {
    let program = parse_fragment(
            br#"putenv("ELEPHC_EVAL_ENV_TEST=direct");
echo getenv("ELEPHC_EVAL_ENV_TEST") . ":";
putenv(assignment: "ELEPHC_EVAL_ENV_TEST=named");
echo getenv(name: "ELEPHC_EVAL_ENV_TEST") . ":";
echo call_user_func("getenv", "ELEPHC_EVAL_ENV_TEST") . ":";
echo call_user_func_array("putenv", ["assignment" => "ELEPHC_EVAL_ENV_TEST=spread"]) ? "set" : "bad";
echo ":" . getenv("ELEPHC_EVAL_ENV_TEST") . ":";
putenv("ELEPHC_EVAL_ENV_TEST");
echo getenv("ELEPHC_EVAL_ENV_TEST") === "" ? "empty" : "bad";
echo ":"; echo function_exists("getenv");
return function_exists("putenv");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "direct:named:named:set:spread:empty:1");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval sleep builtins dispatch without delaying focused tests.
#[test]
fn execute_program_dispatches_sleep_builtins() {
    let program = parse_fragment(
        br#"echo sleep(0) . ":";
echo sleep(seconds: 0) . ":";
usleep(0);
echo "u:";
echo call_user_func("sleep", 0) . ":";
echo call_user_func_array("usleep", ["microseconds" => 0]) === null ? "null" : "bad";
echo ":"; echo function_exists("sleep");
return function_exists("usleep");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "0:0:u:0:null:1");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `php_uname()` dispatches default, named, mode, and callable calls.
#[test]
fn execute_program_dispatches_php_uname_builtin() {
    let program = parse_fragment(
        br#"echo strlen(php_uname()) > 0 ? "all" : "empty"; echo ":";
echo php_uname() === php_uname("a") ? "same" : "different"; echo ":";
echo strlen(php_uname(mode: "s")) > 0 ? "sys" : "empty"; echo ":";
echo strlen(php_uname("n")) > 0 ? "node" : "empty"; echo ":";
echo strlen(php_uname("r")) > 0 ? "release" : "empty"; echo ":";
echo strlen(php_uname("v")) > 0 ? "version" : "empty"; echo ":";
echo strlen(php_uname("m")) > 0 ? "machine" : "empty"; echo ":";
echo strlen(call_user_func("php_uname", "m")) > 0 ? "call" : "empty"; echo ":";
echo strlen(call_user_func_array("php_uname", ["mode" => "n"])) > 0 ? "spread" : "empty"; echo ":";
return function_exists("php_uname");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "all:same:sys:node:release:version:machine:call:spread:"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `gethostbyname()` handles IPv4 literals and failed lookups.
#[test]
fn execute_program_dispatches_gethostbyname_builtin() {
    let program = parse_fragment(
        br#"echo gethostbyname("127.0.0.1") . ":";
echo gethostbyname(hostname: "not a host") . ":";
echo call_user_func("gethostbyname", "127.0.0.1") . ":";
echo call_user_func_array("gethostbyname", ["hostname" => "not a host"]) . ":";
return function_exists("gethostbyname");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "127.0.0.1:not a host:127.0.0.1:not a host:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `gethostname()` dispatches direct and callable zero-arg calls.
#[test]
fn execute_program_dispatches_gethostname_builtin() {
    let program = parse_fragment(
        br#"echo strlen(gethostname()) > 0 ? "host" : "empty"; echo ":";
echo strlen(call_user_func("gethostname")) > 0 ? "call" : "empty"; echo ":";
echo strlen(call_user_func_array("gethostname", [])) > 0 ? "spread" : "empty"; echo ":";
return function_exists("gethostname");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "host:call:spread:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `gethostbyaddr()` handles valid, malformed, and callable calls.
#[test]
fn execute_program_dispatches_gethostbyaddr_builtin() {
    let program = parse_fragment(
            br#"echo strlen(gethostbyaddr("127.0.0.1")) > 0 ? "direct" : "empty"; echo ":";
echo strlen(gethostbyaddr(ip: "127.0.0.1")) > 0 ? "named" : "empty"; echo ":";
echo gethostbyaddr("not-an-ip-address") === false ? "false" : "bad"; echo ":";
echo strlen(call_user_func("gethostbyaddr", "127.0.0.1")) > 0 ? "call" : "empty"; echo ":";
echo call_user_func_array("gethostbyaddr", ["ip" => "not-an-ip-address"]) === false ? "spread" : "bad"; echo ":";
return function_exists("gethostbyaddr");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "direct:named:false:call:spread:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval protocol and service database lookups dispatch dynamically.
#[test]
fn execute_program_dispatches_protocol_service_builtins() {
    let program = parse_fragment(
            br#"echo getprotobyname("TCP") . ":";
echo getprotobynumber(6) . ":";
echo getprotobyname("no_such_protocol") === false ? "missing-proto" : "bad"; echo ":";
echo getprotobynumber(999) === false ? "missing-number" : "bad"; echo ":";
echo getservbyname("www", "tcp") . ":";
echo getservbyport(80, "tcp") . ":";
echo getservbyname("no_such_service", "tcp") === false ? "missing-service" : "bad"; echo ":";
echo getservbyport(80, "no_such_proto") === false ? "missing-port" : "bad"; echo ":";
echo call_user_func("getprotobyname", "udp") . ":";
echo call_user_func_array("getprotobynumber", ["protocol" => 17]) . ":";
echo call_user_func("getservbyname", "https", "tcp") . ":";
echo call_user_func_array("getservbyport", ["port" => 443, "protocol" => "tcp"]) . ":";
echo function_exists("getprotobyname"); echo function_exists("getprotobynumber"); echo function_exists("getservbyname");
return function_exists("getservbyport");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
            values.output,
            "6:tcp:missing-proto:missing-number:80:http:missing-service:missing-port:17:udp:443:https:111"
        );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval IPv4 conversion builtins handle scalar and raw-byte paths.
#[test]
fn execute_program_dispatches_ip_conversion_builtins() {
    let program = parse_fragment(
        br#"echo long2ip(3232235777) . ":";
echo long2ip(ip: 4294967295) . ":";
echo ip2long("192.168.1.1") . ":";
echo ip2long(ip: "1.2.3") === false ? "bad-ip" : "bad"; echo ":";
$packed = inet_pton("1.2.3.4");
echo bin2hex($packed) . ":";
echo inet_pton(ip: "nonsense") === false ? "bad-pton" : "bad"; echo ":";
echo inet_ntop($packed) . ":";
echo inet_ntop(ip: "xx") === false ? "bad-ntop" : "bad"; echo ":";
echo call_user_func("long2ip", 2130706433) . ":";
echo call_user_func_array("ip2long", ["ip" => "0.0.0.0"]) . ":";
echo function_exists("long2ip"); echo function_exists("ip2long");
echo function_exists("inet_pton");
return function_exists("inet_ntop");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
            values.output,
            "192.168.1.1:255.255.255.255:3232235777:bad-ip:01020304:bad-pton:1.2.3.4:bad-ntop:127.0.0.1:0:111"
        );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval path component builtins mirror static basename/dirname edge cases.
#[test]
fn execute_program_dispatches_path_component_builtins() {
    let program = parse_fragment(
        br#"echo basename("/var/log/syslog.log", ".log") . ":";
echo basename(path: "/usr///") . ":";
echo basename("/", "x") === "" ? "root" : "bad"; echo ":";
echo dirname("/usr/local/bin/tool", 2) . ":";
echo dirname(path: "/usr///local///bin") . ":";
echo call_user_func("basename", "foo.tar.gz", ".bz2") . ":";
echo call_user_func_array("dirname", ["path" => "/usr", "levels" => 3]) . ":";
echo function_exists("basename");
return function_exists("dirname");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "syslog:usr:root:/usr/local:/usr///local:foo.tar.gz:/:1"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `realpath()` resolves existing paths and returns false for misses.
#[test]
fn execute_program_dispatches_realpath_builtin() {
    let program = parse_fragment(
            br#"echo realpath(".") !== false ? "resolved" : "bad"; echo ":";
echo realpath(path: "elephc-eval-missing-path") === false ? "false" : "bad"; echo ":";
echo call_user_func("realpath", ".") !== false ? "call" : "bad"; echo ":";
echo call_user_func_array("realpath", ["path" => "elephc-eval-missing-path"]) === false ? "array-false" : "bad";
echo ":";
return function_exists("realpath");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "resolved:false:call:array-false:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `fnmatch()` supports wildcards, classes, flags, constants, and callables.
#[test]
fn execute_program_dispatches_fnmatch_builtin() {
    let program = parse_fragment(
            br#"echo fnmatch("*.log", "system.log") ? "match" : "bad"; echo ":";
echo fnmatch("*.log", "logs/system.log", FNM_PATHNAME) ? "bad" : "path"; echo ":";
echo fnmatch("*.LOG", "system.log", FNM_CASEFOLD) ? "case" : "bad"; echo ":";
echo fnmatch("*", ".env", FNM_PERIOD) ? "bad" : "period"; echo ":";
echo fnmatch("[!abc]oo", "doo") && !fnmatch("[!abc]oo", "boo") ? "class" : "bad"; echo ":";
echo fnmatch('a\\*b', 'a*b') ? "escape" : "bad"; echo ":";
echo fnmatch('a\\*b', 'a\\xxb', FNM_NOESCAPE) ? "noescape" : "bad"; echo ":";
$flags = FNM_PATHNAME | FNM_CASEFOLD;
echo fnmatch("dir/*.TXT", "dir/file.txt", $flags) ? "flags" : "bad"; echo ":";
echo call_user_func("fnmatch", "*.txt", "report.txt") ? "call" : "bad"; echo ":";
echo call_user_func_array("fnmatch", ["pattern" => "*.TXT", "filename" => "report.txt", "flags" => FNM_CASEFOLD]) ? "callarray" : "bad"; echo ":";
echo function_exists("fnmatch"); echo defined("FNM_CASEFOLD");
return FNM_CASEFOLD;"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "match:path:case:period:class:escape:noescape:flags:call:callarray:11"
    );
    assert_eq!(values.get(result), FakeValue::Int(EVAL_FNM_CASEFOLD));
}

/// Verifies eval `pathinfo()` handles arrays, component flags, constants, and callables.
#[test]
fn execute_program_dispatches_pathinfo_builtin() {
    let program = parse_fragment(
            br#"$info = pathinfo("/var/log/syslog.log");
echo $info["dirname"] . "|" . $info["basename"] . "|" . $info["extension"] . "|" . $info["filename"] . ":";
echo pathinfo("archive.tar.gz", PATHINFO_EXTENSION) . ":";
echo pathinfo(".bashrc", PATHINFO_FILENAME) === "" ? "dotfile" : "bad"; echo ":";
echo pathinfo("file.", PATHINFO_EXTENSION) === "" ? "trail" : "bad"; echo ":";
echo pathinfo("", PATHINFO_DIRNAME) === "" ? "empty-dir" : "bad"; echo ":";
$plain = pathinfo("/etc/hosts");
echo array_key_exists("extension", $plain) ? "bad" : "no-ext"; echo ":";
echo pathinfo("/a/b.php", PATHINFO_BASENAME | PATHINFO_FILENAME) . ":";
$call = call_user_func("pathinfo", "foo.txt", PATHINFO_ALL);
echo $call["basename"] . ":";
echo call_user_func_array("pathinfo", ["path" => "foo.txt", "flags" => 0]) === "" ? "zero" : "bad";
echo ":"; echo function_exists("pathinfo"); echo defined("PATHINFO_ALL");
return PATHINFO_ALL;"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "/var/log|syslog.log|log|syslog:gz:dotfile:trail:empty-dir:no-ext:b.php:foo.txt:zero:11"
    );
    assert_eq!(values.get(result), FakeValue::Int(EVAL_PATHINFO_ALL));
}

/// Verifies eval local filesystem builtins read, write, stat, delete, and dispatch.
#[test]
fn execute_program_dispatches_filesystem_builtins() {
    let filename = format!("elephc_eval_fs_probe_{}.txt", std::process::id());
    let missing = format!("elephc_eval_fs_missing_{}.txt", std::process::id());
    let source = format!(
        r#"echo file_put_contents("{filename}", "hello") . ":";
echo file_get_contents("{filename}") . ":";
echo file_exists("{filename}") ? "exists" : "missing"; echo ":";
echo is_file(filename: "{filename}") ? "file" : "bad"; echo ":";
echo is_dir(".") ? "dir" : "bad"; echo ":";
echo is_readable("{filename}") ? "readable" : "bad"; echo ":";
echo is_writable("{filename}") ? "writable" : "bad"; echo ":";
echo is_writeable("{filename}") ? "writeable" : "bad"; echo ":";
echo filesize("{filename}") . ":";
echo file_get_contents("{missing}") === false ? "missing-false" : "bad"; echo ":";
echo call_user_func("file_exists", "{filename}") ? "call-exists" : "bad"; echo ":";
echo call_user_func_array("filesize", ["filename" => "{filename}"]) . ":";
echo unlink("{filename}") ? "unlinked" : "bad"; echo ":";
echo file_exists("{filename}") ? "bad" : "gone"; echo ":";
echo function_exists("file_get_contents"); echo function_exists("file_put_contents");
echo function_exists("file_exists"); echo function_exists("is_file"); echo function_exists("is_dir");
echo function_exists("is_readable"); echo function_exists("is_writable"); echo function_exists("is_writeable");
echo function_exists("filesize");
return function_exists("unlink");"#
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    let _ = std::fs::remove_file(&filename);
    assert_eq!(
            values.output,
            "5:hello:exists:file:dir:readable:writable:writeable:5:missing-false:call-exists:5:unlinked:gone:111111111"
        );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval disk-space builtins query local filesystem capacity and dispatch dynamically.
#[test]
fn execute_program_dispatches_disk_space_builtins() {
    let program = parse_fragment(
            br#"echo disk_free_space(".") > 0 ? "free" : "bad"; echo ":";
echo disk_total_space(directory: ".") > 0 ? "total" : "bad"; echo ":";
echo disk_total_space(".") >= disk_free_space(".") ? "ordered" : "bad"; echo ":";
echo disk_free_space("no/such/path/elephc-eval") === 0.0 ? "missing" : "bad"; echo ":";
echo call_user_func("disk_free_space", ".") > 0 ? "call" : "bad"; echo ":";
echo call_user_func_array("disk_total_space", ["directory" => "."]) > 0 ? "spread" : "bad"; echo ":";
echo function_exists("disk_free_space");
return function_exists("disk_total_space");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "free:total:ordered:missing:call:spread:1");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval stat metadata builtins expose scalar file metadata and link probes.
#[test]
fn execute_program_dispatches_stat_metadata_builtins() {
    let filename = format!("elephc_eval_stat_probe_{}.txt", std::process::id());
    let missing = format!("elephc_eval_stat_missing_{}.txt", std::process::id());
    let link = format!("elephc_eval_stat_link_{}.txt", std::process::id());
    let source = format!(
        r#"echo filemtime("{filename}") > 0 ? "mtime" : "bad"; echo ":";
echo fileatime("{filename}") > 0 ? "atime" : "bad"; echo ":";
echo filectime("{filename}") > 0 ? "ctime" : "bad"; echo ":";
echo fileperms("{filename}") > 0 ? "perms" : "bad"; echo ":";
echo fileowner("{filename}") >= 0 ? "owner" : "bad"; echo ":";
echo filegroup("{filename}") >= 0 ? "group" : "bad"; echo ":";
echo fileinode("{filename}") > 0 ? "inode" : "bad"; echo ":";
echo filetype("{filename}") . ":";
echo filetype(".") . ":";
echo filetype("{link}") . ":";
echo is_executable("{filename}") ? "bad" : "noexec"; echo ":";
echo is_link("{link}") ? "link" : "bad"; echo ":";
echo fileatime("{missing}") === false ? "missing-atime" : "bad"; echo ":";
echo filectime("{missing}") === false ? "missing-ctime" : "bad"; echo ":";
echo fileperms("{missing}") === false ? "missing-perms" : "bad"; echo ":";
echo fileowner("{missing}") === false ? "missing-owner" : "bad"; echo ":";
echo filegroup("{missing}") === false ? "missing-group" : "bad"; echo ":";
echo fileinode("{missing}") === false ? "missing-inode" : "bad"; echo ":";
echo filetype("{missing}") === false ? "missing-type" : "bad"; echo ":";
echo filemtime("{missing}") === 0 ? "missing-mtime" : "bad"; echo ":";
echo call_user_func("filetype", "{filename}") . ":";
echo call_user_func_array("fileinode", ["filename" => "{filename}"]) > 0 ? "callinode" : "bad"; echo ":";
echo function_exists("filemtime"); echo function_exists("fileatime");
echo function_exists("filectime"); echo function_exists("fileperms");
echo function_exists("fileowner"); echo function_exists("filegroup");
echo function_exists("fileinode"); echo function_exists("filetype");
echo function_exists("is_executable"); echo function_exists("is_link");
return true;"#
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let _ = std::fs::remove_file(&filename);
    let _ = std::fs::remove_file(&link);
    std::fs::write(&filename, b"hello").expect("write stat fixture");
    std::os::unix::fs::symlink(&filename, &link).expect("create stat symlink");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    let _ = std::fs::remove_file(&filename);
    let _ = std::fs::remove_file(&link);
    assert_eq!(
            values.output,
            "mtime:atime:ctime:perms:owner:group:inode:file:dir:link:noexec:link:missing-atime:missing-ctime:missing-perms:missing-owner:missing-group:missing-inode:missing-type:missing-mtime:file:callinode:1111111111"
        );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `stat()` and `lstat()` build PHP-compatible metadata arrays.
#[test]
fn execute_program_dispatches_stat_array_builtins() {
    let pid = std::process::id();
    let filename = format!("elephc_eval_stat_array_{pid}.txt");
    let link = format!("elephc_eval_lstat_array_{pid}.txt");
    let missing = format!("elephc_eval_stat_array_missing_{pid}.txt");
    let source = format!(
        r#"$stat = stat("{filename}");
$lstat = lstat("{link}");
echo $stat["size"] === 5 && $stat[7] === $stat["size"] ? "stat" : "bad"; echo ":";
echo ($stat["mode"] & 61440) === 32768 ? "mode" : "bad"; echo ":";
echo ($lstat["mode"] & 61440) === 40960 ? "lstat" : "bad"; echo ":";
echo stat("{missing}") === false && lstat("{missing}") === false ? "missing" : "bad"; echo ":";
$call = call_user_func("stat", "{filename}");
echo $call["mtime"] === filemtime("{filename}") ? "callstat" : "bad"; echo ":";
$call_lstat = call_user_func_array("lstat", ["filename" => "{link}"]);
echo $call_lstat["ino"] > 0 ? "calllstat" : "bad"; echo ":";
echo unlink("{link}") && unlink("{filename}") ? "cleanup" : "bad"; echo ":";
echo function_exists("stat"); echo function_exists("lstat");
return true;"#
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let _ = std::fs::remove_file(&filename);
    let _ = std::fs::remove_file(&link);
    std::fs::write(&filename, b"hello").expect("write stat array fixture");
    std::os::unix::fs::symlink(&filename, &link).expect("create stat array symlink");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    let _ = std::fs::remove_file(&filename);
    let _ = std::fs::remove_file(&link);
    assert_eq!(
        values.output,
        "stat:mode:lstat:missing:callstat:calllstat:cleanup:11"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval local path operation builtins mutate filesystem state.
#[test]
fn execute_program_dispatches_path_operation_builtins() {
    let pid = std::process::id();
    let dir = format!("elephc_eval_ops_dir_{pid}");
    let call_dir = format!("elephc_eval_ops_call_dir_{pid}");
    let src = format!("elephc_eval_ops_src_{pid}.txt");
    let copy = format!("elephc_eval_ops_copy_{pid}.txt");
    let moved = format!("elephc_eval_ops_moved_{pid}.txt");
    let symlink = format!("elephc_eval_ops_symlink_{pid}.txt");
    let hardlink = format!("elephc_eval_ops_hardlink_{pid}.txt");
    let source = format!(
        r#"file_put_contents("{src}", "hello");
echo mkdir("{dir}") ? "mkdir" : "bad"; echo ":";
echo is_dir("{dir}") ? "dir" : "bad"; echo ":";
echo copy("{src}", "{copy}") && file_get_contents("{copy}") === "hello" ? "copy" : "bad"; echo ":";
echo rename("{copy}", "{moved}") && file_exists("{moved}") && !file_exists("{copy}") ? "rename" : "bad"; echo ":";
echo symlink("{src}", "{symlink}") ? "symlink" : "bad"; echo ":";
echo readlink("{symlink}") === "{src}" ? "readlink" : "bad"; echo ":";
echo linkinfo("{symlink}") >= 0 ? "linkinfo" : "bad"; echo ":";
echo readlink("{src}") === false ? "readlink-false" : "bad"; echo ":";
echo linkinfo("{missing}") === -1 ? "linkinfo-missing" : "bad"; echo ":";
echo link("{src}", "{hardlink}") && file_get_contents("{hardlink}") === "hello" ? "hardlink" : "bad"; echo ":";
echo clearstatcache() === null ? "cache" : "bad"; echo ":";
echo unlink("{symlink}") && unlink("{hardlink}") && unlink("{moved}") && unlink("{src}") && rmdir("{dir}") ? "cleanup" : "bad"; echo ":";
echo call_user_func("mkdir", "{call_dir}") ? "callmkdir" : "bad"; echo ":";
echo call_user_func_array("rmdir", ["directory" => "{call_dir}"]) ? "callrmdir" : "bad"; echo ":";
echo function_exists("mkdir"); echo function_exists("rmdir"); echo function_exists("copy");
echo function_exists("rename"); echo function_exists("symlink"); echo function_exists("link");
echo function_exists("readlink"); echo function_exists("linkinfo"); echo function_exists("clearstatcache");
return true;"#,
        missing = format!("elephc_eval_ops_missing_{pid}.txt"),
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    for path in [&symlink, &hardlink, &moved, &copy, &src] {
        let _ = std::fs::remove_file(path);
    }
    for path in [&call_dir, &dir] {
        let _ = std::fs::remove_dir(path);
    }
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    for path in [&symlink, &hardlink, &moved, &copy, &src] {
        let _ = std::fs::remove_file(path);
    }
    for path in [&call_dir, &dir] {
        let _ = std::fs::remove_dir(path);
    }
    assert_eq!(
            values.output,
            "mkdir:dir:copy:rename:symlink:readlink:linkinfo:readlink-false:linkinfo-missing:hardlink:cache:cleanup:callmkdir:callrmdir:111111111"
        );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval file-listing builtins build arrays, stream files, and dispatch dynamically.
#[test]
fn execute_program_dispatches_file_listing_builtins() {
    let pid = std::process::id();
    let lines = format!("elephc_eval_listing_lines_{pid}.txt");
    let empty = format!("elephc_eval_listing_empty_{pid}.txt");
    let missing = format!("elephc_eval_listing_missing_{pid}.txt");
    let dir = format!("elephc_eval_listing_dir_{pid}");
    let source = format!(
        r#"file_put_contents("{lines}", "one\ntwo");
file_put_contents("{empty}", "");
$lines = file("{lines}");
echo count($lines) . ":";
echo $lines[0] === "one\n" ? "line0" : "bad"; echo ":";
echo $lines[1] === "two" ? "line1" : "bad"; echo ":";
echo "[";
$bytes = readfile(filename: "{empty}");
echo "]" . $bytes . ":";
echo readfile("{missing}") === false ? "missing-readfile" : "bad"; echo ":";
echo count(file("{missing}")) === 0 ? "missing-file" : "bad"; echo ":";
mkdir("{dir}");
file_put_contents("{dir}/a.txt", "a");
file_put_contents("{dir}/b.txt", "b");
$scan = scandir(directory: "{dir}");
echo count($scan) . ":";
echo in_array(".", $scan) && in_array("..", $scan) && in_array("a.txt", $scan) && in_array("b.txt", $scan) ? "scan" : "bad"; echo ":";
$call_lines = call_user_func("file", "{lines}");
echo $call_lines[0] === "one\n" ? "callfile" : "bad"; echo ":";
$call_scan = call_user_func_array("scandir", ["directory" => "{dir}"]);
echo count($call_scan) . ":";
echo unlink("{dir}/a.txt") && unlink("{dir}/b.txt") && rmdir("{dir}") && unlink("{lines}") && unlink("{empty}") ? "cleanup" : "bad"; echo ":";
echo function_exists("file"); echo function_exists("readfile"); echo function_exists("scandir");
return true;"#
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    for path in [&lines, &empty, &missing] {
        let _ = std::fs::remove_file(path);
    }
    let _ = std::fs::remove_file(format!("{dir}/a.txt"));
    let _ = std::fs::remove_file(format!("{dir}/b.txt"));
    let _ = std::fs::remove_dir(&dir);
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    for path in [&lines, &empty, &missing] {
        let _ = std::fs::remove_file(path);
    }
    let _ = std::fs::remove_file(format!("{dir}/a.txt"));
    let _ = std::fs::remove_file(format!("{dir}/b.txt"));
    let _ = std::fs::remove_dir(&dir);
    assert_eq!(
        values.output,
        "2:line0:line1:[]0:missing-readfile:missing-file:4:scan:callfile:4:cleanup:111"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `glob()` expands local patterns and dispatches dynamically.
#[test]
fn execute_program_dispatches_glob_builtin() {
    let pid = std::process::id();
    let dir = format!("elephc_eval_glob_dir_{pid}");
    let source = format!(
        r#"mkdir("{dir}");
file_put_contents("{dir}/a.txt", "a");
file_put_contents("{dir}/b.log", "b");
file_put_contents("{dir}/c.txt", "c");
file_put_contents("{dir}/.hidden.txt", "h");
$matches = glob("{dir}/*.txt");
echo count($matches) === 2 && basename($matches[0]) === "a.txt" && basename($matches[1]) === "c.txt" ? "glob" : "bad"; echo ":";
echo count(glob("{dir}/*.none")) === 0 ? "empty" : "bad"; echo ":";
$literal = glob("{dir}/a.txt");
echo count($literal) === 1 && $literal[0] === "{dir}/a.txt" ? "literal" : "bad"; echo ":";
$all = glob("{dir}/*");
echo in_array("{dir}/.hidden.txt", $all) ? "bad" : "hidden"; echo ":";
$call = call_user_func("glob", "{dir}/*.log");
echo count($call) === 1 && basename($call[0]) === "b.log" ? "callglob" : "bad"; echo ":";
$call_array = call_user_func_array("glob", ["pattern" => "{dir}/*.txt"]);
echo count($call_array) === 2 ? "callarray" : "bad"; echo ":";
unlink("{dir}/.hidden.txt");
unlink("{dir}/c.txt");
unlink("{dir}/b.log");
unlink("{dir}/a.txt");
echo rmdir("{dir}") ? "cleanup" : "bad"; echo ":";
echo function_exists("glob");
return true;"#
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let _ = std::fs::remove_file(format!("{dir}/.hidden.txt"));
    let _ = std::fs::remove_file(format!("{dir}/c.txt"));
    let _ = std::fs::remove_file(format!("{dir}/b.log"));
    let _ = std::fs::remove_file(format!("{dir}/a.txt"));
    let _ = std::fs::remove_dir(&dir);
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    let _ = std::fs::remove_file(format!("{dir}/.hidden.txt"));
    let _ = std::fs::remove_file(format!("{dir}/c.txt"));
    let _ = std::fs::remove_file(format!("{dir}/b.log"));
    let _ = std::fs::remove_file(format!("{dir}/a.txt"));
    let _ = std::fs::remove_dir(&dir);
    assert_eq!(
        values.output,
        "glob:empty:literal:hidden:callglob:callarray:cleanup:1"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval file-modification builtins update modes, masks, temp files, and dispatch.
#[test]
fn execute_program_dispatches_file_modify_builtins() {
    let pid = std::process::id();
    let filename = format!("elephc_eval_modify_{pid}.txt");
    let missing = format!("elephc_eval_modify_missing_{pid}.txt");
    let prefix = format!("evm{pid}_");
    let call_prefix = format!("evc{pid}_");
    let source = format!(
        r#"file_put_contents("{filename}", "x");
echo chmod(filename: "{filename}", permissions: 384) ? "chmod" : "bad"; echo ":";
echo (fileperms("{filename}") & 511) === 384 ? "mode" : "bad"; echo ":";
echo chmod("{missing}", 384) ? "bad" : "chmod-false"; echo ":";
$tmp = tempnam(directory: ".", prefix: "{prefix}");
echo file_exists($tmp) && str_starts_with(basename($tmp), "{prefix}") ? "tempnam" : "bad"; echo ":";
unlink($tmp);
$previous = umask(mask: 18);
$set = umask($previous);
echo $set === 18 ? "umask" : "bad"; echo ":";
$before = umask(18);
$probe = umask();
$restore = umask($before);
echo $probe === 18 && $restore === 18 ? "probe" : "bad"; echo ":";
echo call_user_func("chmod", "{filename}", 420) ? "callchmod" : "bad"; echo ":";
$call_tmp = call_user_func_array("tempnam", ["directory" => ".", "prefix" => "{call_prefix}"]);
echo file_exists($call_tmp) && str_starts_with(basename($call_tmp), "{call_prefix}") ? "calltempnam" : "bad"; echo ":";
unlink($call_tmp);
echo unlink("{filename}") ? "cleanup" : "bad"; echo ":";
echo function_exists("chmod"); echo function_exists("tempnam"); echo function_exists("umask");
return true;"#
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let _ = std::fs::remove_file(&filename);
    let _ = std::fs::remove_file(&missing);
    for entry in std::fs::read_dir(".").expect("read eval test cwd") {
        let entry = entry.expect("read eval temp entry");
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(&prefix) || name.starts_with(&call_prefix) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    let _ = std::fs::remove_file(&filename);
    let _ = std::fs::remove_file(&missing);
    for entry in std::fs::read_dir(".").expect("read eval test cwd") {
        let entry = entry.expect("read eval temp entry");
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(&prefix) || name.starts_with(&call_prefix) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    assert_eq!(
        values.output,
        "chmod:mode:chmod-false:tempnam:umask:probe:callchmod:calltempnam:cleanup:111"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `touch()` creates files, stamps mtimes, and dispatches dynamically.
#[test]
fn execute_program_dispatches_touch_builtin() {
    let pid = std::process::id();
    let created = format!("elephc_eval_touch_created_{pid}.txt");
    let stamped = format!("elephc_eval_touch_stamped_{pid}.txt");
    let missing = format!("elephc_eval_touch_missing_{pid}/x.txt");
    let source = format!(
        r#"echo touch(filename: "{created}") && file_exists("{created}") ? "create" : "bad"; echo ":";
file_put_contents("{stamped}", "x");
echo touch("{stamped}", 1000000000) ? "mtime" : "bad"; echo ":";
echo filemtime("{stamped}") === 1000000000 ? "readmtime" : "bad"; echo ":";
echo touch("{stamped}", 1000000001, null) && filemtime("{stamped}") === 1000000001 ? "nullatime" : "bad"; echo ":";
echo touch("{stamped}", 1000000002, 1000000003) && filemtime("{stamped}") === 1000000002 ? "both" : "bad"; echo ":";
echo touch("{missing}") ? "bad" : "touch-false"; echo ":";
echo call_user_func("touch", "{created}", 1000000004) ? "calltouch" : "bad"; echo ":";
echo call_user_func_array("touch", ["filename" => "{stamped}", "mtime" => 1000000005]) ? "callarray" : "bad"; echo ":";
echo unlink("{created}") && unlink("{stamped}") ? "cleanup" : "bad"; echo ":";
echo function_exists("touch");
return true;"#
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let _ = std::fs::remove_file(&created);
    let _ = std::fs::remove_file(&stamped);
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    let _ = std::fs::remove_file(&created);
    let _ = std::fs::remove_file(&stamped);
    assert_eq!(
        values.output,
        "create:mtime:readmtime:nullatime:both:touch-false:calltouch:callarray:cleanup:1"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval ASCII string case builtins work directly and through callable dispatch.
#[test]
fn execute_program_dispatches_string_case_builtins() {
    let program = parse_fragment(
            br#"echo strtoupper("Hello World"); echo ":";
echo strtolower("LOUD"); echo ":";
echo ucfirst("eval"); echo ":";
echo lcfirst("LOUD"); echo ":";
echo call_user_func("strtoupper", "xy"); echo ":";
echo call_user_func_array("strtolower", ["ZZ"]); echo ":";
echo call_user_func("ucfirst", "case"); echo ":";
echo call_user_func_array("lcfirst", ["CASE"]);
echo ":"; echo function_exists("strtoupper"); echo function_exists("strtolower"); echo function_exists("ucfirst");
return function_exists("lcfirst");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "HELLO WORLD:loud:Eval:lOUD:XY:zz:Case:cASE:111"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `ucwords()` capitalizes word starts directly and by callable dispatch.
#[test]
fn execute_program_dispatches_ucwords_builtin() {
    let program = parse_fragment(
        br#"echo ucwords("hello world"); echo ":";
echo ucwords(string: "hello-world", separators: "-"); echo ":";
echo ucwords("hello\tworld"); echo ":";
echo call_user_func("ucwords", "a b"); echo ":";
echo call_user_func_array("ucwords", ["string" => "a-b", "separators" => "-"]); echo ":";
return function_exists("ucwords");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "Hello World:Hello-World:Hello\tWorld:A B:A-B:"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `wordwrap()` wraps at word boundaries and can cut long words.
#[test]
fn execute_program_dispatches_wordwrap_builtin() {
    let program = parse_fragment(
        br#"echo wordwrap("The quick brown fox", 10, "|"); echo ":";
echo wordwrap(string: "A verylongword here", width: 8, break: "|"); echo ":";
echo wordwrap("abcdefghij", 4, "|", true); echo ":";
echo wordwrap("preserve\nnewlines here ok", 10, "|"); echo ":";
echo call_user_func("wordwrap", "aaa bbb ccc", 3, "<br>"); echo ":";
echo call_user_func_array("wordwrap", ["string" => "hello world", "width" => 5, "break" => "|"]);
echo ":";
return function_exists("wordwrap");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
            values.output,
            "The quick|brown fox:A|verylongword|here:abcd|efgh|ij:preserve\nnewlines|here ok:aaa<br>bbb<br>ccc:hello|world:"
        );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `str_contains()` uses byte-string search and supports callable dispatch.
#[test]
fn execute_program_dispatches_str_contains_builtin() {
    let program = parse_fragment(
        br#"echo str_contains("Hello World", "World") ? "Y" : "N";
echo str_contains("Hello", "z") ? "bad" : ":N";
echo str_contains("Hello", "") ? ":E" : "bad";
echo call_user_func("str_contains", "abc", "b") ? ":C" : "bad";
echo call_user_func_array("str_contains", ["abc", "x"]) ? "bad" : ":A";
return function_exists("str_contains");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Y:N:E:C:A");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval string position builtins return byte offsets or PHP false.
#[test]
fn execute_program_dispatches_string_position_builtins() {
    let program = parse_fragment(
        br#"echo strpos("banana", "na");
echo ":" . strrpos("banana", "na");
echo ":"; echo strpos("abc", "z") === false ? "F" : "bad";
echo ":" . strpos("abc", "");
echo ":" . strrpos("abc", "");
echo ":" . call_user_func("strpos", "abc", "b");
echo ":" . call_user_func_array("strrpos", ["ababa", "ba"]);
echo ":"; echo function_exists("strpos");
return function_exists("strrpos");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "2:4:F:0:3:1:3:1");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `strstr()` returns suffixes, prefixes, or false for misses.
#[test]
fn execute_program_dispatches_strstr_builtin() {
    let program = parse_fragment(
            br#"echo strstr("user@example.com", "@"); echo ":";
echo strstr(haystack: "hello world", needle: "lo", before_needle: true); echo ":";
echo strstr("hello", "x") === false ? "F" : "bad"; echo ":";
echo strstr("hello", ""); echo ":";
echo call_user_func("strstr", "abcabc", "bc"); echo ":";
echo call_user_func_array("strstr", ["haystack" => "abcabc", "needle" => "bc", "before_needle" => true]); echo ":";
return function_exists("strstr");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "@example.com:hel:F:hello:bcabc:a:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval prefix/suffix string search builtins use byte-string semantics.
#[test]
fn execute_program_dispatches_string_boundary_builtins() {
    let program = parse_fragment(
        br#"echo str_starts_with("Hello World", "Hello") ? "S" : "bad";
echo str_starts_with("Hello", "World") ? "bad" : ":s";
echo str_starts_with("Hello", "") ? ":se" : "bad";
echo str_ends_with("Hello World", "World") ? ":E" : "bad";
echo str_ends_with("Hello", "World") ? "bad" : ":e";
echo str_ends_with("Hello", "") ? ":ee" : "bad";
echo call_user_func("str_starts_with", "abc", "a") ? ":CS" : "bad";
echo call_user_func_array("str_ends_with", ["abc", "c"]) ? ":CE" : "bad";
echo ":"; echo function_exists("str_starts_with");
return function_exists("str_ends_with");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "S:s:se:E:e:ee:CS:CE:1");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval string comparison builtins return PHP-compatible scalar results.
#[test]
fn execute_program_dispatches_string_compare_builtins() {
    let program = parse_fragment(
        br#"echo strcmp("abc", "abc");
echo ":"; echo strcmp("abc", "abd") < 0 ? "lt" : "bad";
echo ":"; echo strcasecmp("Hello", "hello");
echo ":"; echo call_user_func("strcmp", "b", "a") > 0 ? "gt" : "bad";
echo ":"; echo call_user_func_array("strcasecmp", ["A", "a"]) === 0 ? "ci" : "bad";
echo ":"; echo hash_equals("abc", "abc") ? "heq" : "bad";
echo ":"; echo hash_equals("abc", "abcd") ? "bad" : "hlen";
echo ":"; echo call_user_func("hash_equals", "abc", "abd") ? "bad" : "hneq";
echo ":"; echo function_exists("strcmp"); echo function_exists("strcasecmp");
return function_exists("hash_equals");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "0:lt:0:gt:ci:heq:hlen:hneq:11");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval trim-like builtins strip default and explicit byte masks.
#[test]
fn execute_program_dispatches_trim_like_builtins() {
    let program = parse_fragment(
            br#"echo "[" . trim("  hello  ") . "]";
echo ":[" . ltrim("  left") . "]";
echo ":[" . rtrim("right  ") . "]";
echo ":[" . chop("tail... ", " .") . "]";
echo ":[" . trim("**boxed**", "*") . "]";
echo ":[" . call_user_func("trim", "  cuf  ") . "]";
echo ":[" . call_user_func_array("ltrim", ["0007", "0"]) . "]";
echo ":"; echo function_exists("trim"); echo function_exists("ltrim"); echo function_exists("rtrim");
return function_exists("chop");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "[hello]:[left]:[right]:[tail]:[boxed]:[cuf]:[7]:111"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval type-predicate builtins inspect boxed runtime tags directly and by callable.
#[test]
fn execute_program_dispatches_type_predicate_builtins() {
    let program = parse_fragment(
            br#"echo is_int(1); echo is_integer(1); echo is_long(1);
echo is_float(1.5); echo is_double(1.5); echo is_real(1.5);
echo is_string("x"); echo is_bool(false); echo is_null(null);
echo is_array([1]); echo is_array(["a" => 1]);
echo is_iterable([1]); echo is_iterable(["a" => 1]);
echo is_iterable(1) ? "bad" : "T";
echo is_array(1) ? "bad" : "ok";
echo is_numeric(42); echo is_numeric(3.14); echo is_numeric("42");
echo is_numeric("-5"); echo is_numeric("3.14");
echo is_numeric("abc") ? "bad" : "N";
echo is_numeric(true) ? "bad" : "B";
echo is_resource(1) ? "bad" : "R";
echo is_object($object) ? "O" : "bad";
echo is_object([1]) ? "bad" : "o";
echo is_nan(fdiv(0, 0)) ? "N" : "bad";
echo is_infinite(fdiv(1, 0)) ? "I" : "bad";
echo is_infinite(fdiv(-1, 0)) ? "i" : "bad";
echo is_finite(42) ? "F" : "bad";
echo is_finite(fdiv(1, 0)) ? "bad" : "f";
echo ":"; echo call_user_func("is_string", "x");
echo call_user_func_array("is_array", [[1]]);
echo call_user_func("is_numeric", "12");
echo call_user_func("is_iterable", [1]);
echo call_user_func_array("is_iterable", ["value" => 1]) ? "bad" : "t";
echo call_user_func("is_object", $object) ? "O" : "bad";
echo call_user_func_array("is_object", ["value" => 1]) ? "bad" : "o";
echo function_exists("is_numeric"); echo function_exists("is_object"); echo function_exists("is_resource");
echo function_exists("is_double"); echo function_exists("is_nan"); echo function_exists("is_finite");
echo function_exists("is_iterable");
return function_exists("is_infinite");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let object = values.alloc(FakeValue::Object(Vec::new()));
    scope.set("object".to_string(), object, ScopeCellOwnership::Borrowed);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "1111111111111Tok11111NBROoNIiFf:1111tOo1111111"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `is_resource()` recognizes resource-tagged runtime cells from scope.
#[test]
fn execute_program_dispatches_is_resource_true() {
    let program = parse_fragment(
        br#"echo is_resource($handle) ? "R" : "bad";
echo ":" . gettype($handle);
return call_user_func("is_resource", $handle);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let handle = values.alloc(FakeValue::Resource(6));
    scope.set("handle".to_string(), handle, ScopeCellOwnership::Borrowed);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "R:resource");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval resource introspection builtins expose stream type and one-based id.
#[test]
fn execute_program_dispatches_resource_introspection_builtins() {
    let program = parse_fragment(
        br#"echo get_resource_type($handle);
echo ":" . get_resource_id($handle);
echo ":" . call_user_func("get_resource_type", $handle);
echo ":" . call_user_func_array("get_resource_id", ["resource" => $handle]);
echo ":" . function_exists("get_resource_type");
return function_exists("get_resource_id");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let handle = values.alloc(FakeValue::Resource(6));
    scope.set("handle".to_string(), handle, ScopeCellOwnership::Borrowed);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "stream:7:stream:7:1");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval cast builtins return boxed scalar cells directly and by callable.
#[test]
fn execute_program_dispatches_cast_builtins() {
    let program = parse_fragment(
        br#"echo intval("42"); echo ":";
echo floatval("3.5"); echo ":";
echo strval(12); echo ":";
echo boolval("0") ? "bad" : "false";
echo ":"; echo call_user_func("strval", 7);
return call_user_func_array("intval", ["9"]);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "42:3.5:12:false:7");
    assert_eq!(values.get(result), FakeValue::Int(9));
}

/// Verifies eval `settype()` mutates direct variables and warns for callable by-value dispatch.
#[test]
fn execute_program_dispatches_settype_builtin() {
    let program = parse_fragment(
        br#"$x = 42;
echo settype($x, "string") ? gettype($x) . ":" . $x : "bad";
echo ":";
$y = "0";
echo settype(type: "bool", var: $y) ? gettype($y) . ":" . ($y ? "true" : "false") : "bad";
echo ":";
echo settype($missing, "integer") ? gettype($missing) . ":" . $missing : "bad";
echo ":";
$z = 3.8;
echo call_user_func("settype", $z, "integer") ? gettype($z) . ":" . $z : "bad";
echo ":";
return function_exists("settype");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "string:42:boolean:false:integer:0:double:3.8:"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
    assert_eq!(
        values.warnings,
        ["settype(): Argument #1 ($var) must be passed by reference, value given"]
    );
}

/// Verifies eval `gettype()` maps runtime tags to PHP type names directly and by callable.
#[test]
fn execute_program_dispatches_gettype_builtin() {
    let program = parse_fragment(
        br#"echo gettype(1); echo ":";
echo gettype(1.5); echo ":";
echo gettype("x"); echo ":";
echo gettype(false); echo ":";
echo gettype(null); echo ":";
echo gettype([1]); echo ":";
echo gettype(["a" => 1]); echo ":";
echo call_user_func("gettype", true); echo ":";
echo call_user_func_array("gettype", [null]);
return function_exists("gettype");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "integer:double:string:boolean:NULL:array:array:boolean:NULL"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `get_class()` reads object class names directly and by callable.
#[test]
fn execute_program_dispatches_get_class_builtin() {
    let program = parse_fragment(
        br#"echo get_class($object); echo ":";
echo call_user_func("get_class", $object); echo ":";
return call_user_func_array("get_class", ["object" => $object]);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let object = values.alloc(FakeValue::Object(Vec::new()));
    scope.set("object".to_string(), object, ScopeCellOwnership::Borrowed);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "stdClass:stdClass:");
    assert_eq!(
        values.get(result),
        FakeValue::String("stdClass".to_string())
    );
}

/// Verifies eval `get_parent_class()` reads object and class-string parents by callable.
#[test]
fn execute_program_dispatches_get_parent_class_builtin() {
    let program = parse_fragment(
        br#"echo get_parent_class($object); echo ":";
echo get_parent_class("ChildClass"); echo ":";
echo call_user_func("get_parent_class", $object); echo ":";
return call_user_func_array("get_parent_class", ["object_or_class" => "ChildClass"]);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let object = values.alloc(FakeValue::Object(Vec::new()));
    scope.set("object".to_string(), object, ScopeCellOwnership::Borrowed);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "ParentClass:ParentClass:ParentClass:");
    assert_eq!(
        values.get(result),
        FakeValue::String("ParentClass".to_string())
    );
}

/// Verifies eval `abs()` dispatches through runtime numeric hooks directly and by callable.
#[test]
fn execute_program_dispatches_abs_builtin() {
    let program = parse_fragment(
        br#"echo abs(-5); echo ":";
echo abs(-2.5); echo ":";
echo gettype(abs(-2.5)); echo ":";
echo call_user_func("abs", -7); echo ":";
echo call_user_func_array("abs", [-9]);
return function_exists("abs");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "5:2.5:double:7:9");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `floor()` and `ceil()` dispatch as double-returning math builtins.
#[test]
fn execute_program_dispatches_floor_and_ceil_builtins() {
    let program = parse_fragment(
        br#"echo floor(3.7); echo ":";
echo gettype(floor(3)); echo ":";
echo ceil(3.2); echo ":";
echo gettype(ceil(3)); echo ":";
echo call_user_func("floor", 4.9); echo ":";
echo call_user_func_array("ceil", [4.1]);
echo ":"; echo function_exists("floor");
return function_exists("ceil");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "3:double:4:double:4:5:1");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `fdiv()` and `fmod()` dispatch as floating-point binary builtins.
#[test]
fn execute_program_dispatches_float_binary_builtins() {
    let program = parse_fragment(
        br#"echo round(fdiv(10, 4), 2); echo ":";
echo gettype(fdiv(10, 4)); echo ":";
echo round(fmod(10.5, 3.2), 1); echo ":";
echo round(call_user_func("fdiv", 9, 2), 1); echo ":";
echo round(call_user_func_array("fmod", [10.5, 3.2]), 1); echo ":";
echo function_exists("fdiv");
return function_exists("fmod");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    assert_eq!(values.output, "2.5:double:0.9:4.5:0.9:1");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval extended scalar math builtins support direct, named, callable, and probe paths.
#[test]
fn execute_program_dispatches_extended_math_builtins() {
    let program = parse_fragment(
        br#"echo sin(0); echo ":";
echo cos(0); echo ":";
echo tan(0); echo ":";
echo round(asin(1), 2); echo ":";
echo acos(1); echo ":";
echo round(atan(1), 2); echo ":";
echo sinh(0); echo ":";
echo cosh(0); echo ":";
echo tanh(0); echo ":";
echo log2(8); echo ":";
echo log10(100); echo ":";
echo exp(0); echo ":";
echo round(deg2rad(180), 2); echo ":";
echo round(rad2deg(pi()), 0); echo ":";
echo log(num: 8, base: 2); echo ":";
echo atan2(y: 0, x: 1); echo ":";
echo hypot(3, 4); echo ":";
echo intdiv(7, 2); echo ":";
echo round(call_user_func("sin", pi() / 2), 0); echo ":";
echo call_user_func_array("intdiv", ["num1" => 9, "num2" => 2]); echo ":";
echo function_exists("sin"); echo function_exists("log"); echo function_exists("intdiv");
return function_exists("hypot");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "0:1:0:1.57:0:0.79:0:1:0:3:2:1:3.14:180:3:0:5:3:1:4:111"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `pow()` dispatches through the existing exponentiation runtime hook.
#[test]
fn execute_program_dispatches_pow_builtin() {
    let program = parse_fragment(
        br#"echo pow(2, 3); echo ":";
echo gettype(pow(2, 3)); echo ":";
echo call_user_func("pow", 2, 5); echo ":";
echo call_user_func_array("pow", [3, 3]);
return function_exists("pow");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "8:double:32:27");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `round()` supports default and explicit precision through callable paths.
#[test]
fn execute_program_dispatches_round_builtin() {
    let program = parse_fragment(
        br#"echo round(3.5); echo ":";
echo round(3.14159, 2); echo ":";
echo gettype(round(3)); echo ":";
echo call_user_func("round", 2.5); echo ":";
echo call_user_func_array("round", [1.55, 1]);
return function_exists("round");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "4:3.14:double:3:1.6");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `number_format()` groups and rounds numbers through callable paths.
#[test]
fn execute_program_dispatches_number_format_builtin() {
    let program = parse_fragment(
            br#"echo number_format(1234567); echo ":";
echo number_format(1234.5678, 2); echo ":";
echo number_format(num: 1234567.89, decimals: 2, decimal_separator: ",", thousands_separator: "."); echo ":";
echo number_format(1234567.89, 2, ".", ""); echo ":";
echo call_user_func("number_format", -1234.5, 1); echo ":";
echo call_user_func_array("number_format", ["num" => 1234, "decimals" => 0, "decimal_separator" => ".", "thousands_separator" => " "]); echo ":";
return function_exists("number_format");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "1,234,567:1,234.57:1.234.567,89:1234567.89:-1,234.5:1 234:"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval printf-family builtins format, print, and dispatch through callables.
#[test]
fn execute_program_dispatches_printf_family_builtins() {
    let program = parse_fragment(
        br#"echo sprintf("Hello %s", "World"); echo ":";
echo sprintf("%05d", 42); echo ":";
echo sprintf("%.2f", 3.14159); echo ":";
echo sprintf("%-6s|", "hi"); echo ":";
$printed = printf("%s=%d", "n", 42);
echo ":" . $printed . ":";
echo vsprintf("%s/%d/%.1f", ["age", 42, 3]); echo ":";
$vprinted = vprintf("%s-%d", ["v", 7]);
echo ":" . $vprinted . ":";
echo call_user_func("sprintf", "%+d", 42); echo ":";
echo call_user_func_array("vsprintf", ["format" => "%s", "values" => ["spread"]]); echo ":";
echo function_exists("sprintf"); echo is_callable("printf"); echo function_exists("vsprintf");
return is_callable("vprintf");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "Hello World:00042:3.14:hi    |:n=42:4:age/42/3.0:v-7:3:+42:spread:111"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `sscanf()` returns indexed string matches through callable paths.
#[test]
fn execute_program_dispatches_sscanf_builtin() {
    let program = parse_fragment(
        br#"$result = sscanf("John 1.5 30", "%s %f %d");
echo $result[0] . ":" . $result[1] . ":" . $result[2] . ":";
$named = sscanf(string: "Age: -25", format: "Age: %d");
echo $named[0] . ":";
$call = call_user_func("sscanf", "-2.5e3", "%f");
echo $call[0] . ":";
$spread = call_user_func_array("sscanf", ["string" => "ok %", "format" => "%s %%"]);
echo $spread[0] . ":";
return function_exists("sscanf");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "John:1.5:30:-25:-2.5e3:ok:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `min()` and `max()` select numeric values directly and by callable.
#[test]
fn execute_program_dispatches_min_max_builtins() {
    let program = parse_fragment(
        br#"echo min(3, 1, 2); echo ":";
echo max(1, 3, 2); echo ":";
echo min(2.5, 1.5); echo ":";
echo max(1.5, 2.5); echo ":";
echo call_user_func("min", 9, 4, 7); echo ":";
echo call_user_func_array("max", [4, 8, 6]); echo ":";
echo function_exists("min");
return function_exists("max");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "1:3:1.5:2.5:4:8:1");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `clamp()` selects numeric values through direct, named, and callable paths.
#[test]
fn execute_program_dispatches_clamp_builtin() {
    let program = parse_fragment(
        br#"echo clamp(5, 0, 10); echo ":";
echo clamp(15, 0, 10); echo ":";
echo clamp(-5, 0, 10); echo ":";
echo clamp(2.75, 1.5, 2.5); echo ":";
echo clamp(value: 8, min: 0, max: 5); echo ":";
echo call_user_func("clamp", -1, 0, 10); echo ":";
echo call_user_func_array("clamp", ["value" => 9, "min" => 0, "max" => 7]); echo ":";
echo function_exists("clamp");
return is_callable("clamp");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "5:10:0:2.5:5:0:7:1");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `clamp()` rejects a lower bound greater than the upper bound.
#[test]
fn execute_program_rejects_clamp_invalid_bounds() {
    let program = parse_fragment(br#"return clamp(5, 10, 0);"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("invalid clamp bounds should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies eval `pi()` returns a double constant directly and through callable paths.
#[test]
fn execute_program_dispatches_pi_builtin() {
    let program = parse_fragment(
        br#"echo round(pi(), 2); echo ":";
echo gettype(pi()); echo ":";
echo round(call_user_func("pi"), 3); echo ":";
echo round(call_user_func_array("pi", []), 4); echo ":";
return function_exists("pi");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "3.14:double:3.142:3.1416:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `sqrt()` dispatches through runtime float hooks directly and by callable.
#[test]
fn execute_program_dispatches_sqrt_builtin() {
    let program = parse_fragment(
        br#"echo sqrt(16); echo ":";
echo gettype(sqrt(9)); echo ":";
echo call_user_func("sqrt", 25); echo ":";
echo call_user_func_array("sqrt", [36]);
return function_exists("sqrt");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "4:double:5:6");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `strrev()` dispatches through direct and callable paths.
#[test]
fn execute_program_dispatches_strrev_builtin() {
    let program = parse_fragment(
        br#"echo strrev("Hello"); echo ":";
echo strrev(123); echo ":";
echo call_user_func("strrev", "ABC"); echo ":";
echo call_user_func_array("strrev", ["def"]); echo ":";
return function_exists("strrev");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    assert_eq!(values.output, "olleH:321:CBA:fed:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `chr()` dispatches through direct, named, and callable paths.
#[test]
fn execute_program_dispatches_chr_builtin() {
    let program = parse_fragment(
        br#"echo chr(65); echo ":";
echo bin2hex(chr(codepoint: 256)); echo ":";
echo bin2hex(call_user_func("chr", 257)); echo ":";
echo call_user_func_array("chr", ["codepoint" => 321]); echo ":";
return function_exists("chr");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "A:00:01:A:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `str_repeat()` dispatches through direct, named, and callable paths.
#[test]
fn execute_program_dispatches_str_repeat_builtin() {
    let program = parse_fragment(
        br#"echo str_repeat("ha", 3); echo ":";
echo strlen(str_repeat(string: "x", times: 0)); echo ":";
echo call_user_func("str_repeat", "ab", 2); echo ":";
echo call_user_func_array("str_repeat", ["string" => "z", "times" => 3]); echo ":";
return function_exists("str_repeat");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "hahaha:0:abab:zzz:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `substr()` dispatches through direct, named, and callable paths.
#[test]
fn execute_program_dispatches_substr_builtin() {
    let program = parse_fragment(
            br#"echo substr("abcdef", 2); echo ":";
echo substr(string: "abcdef", offset: 1, length: -1); echo ":";
echo substr("abcdef", -2); echo ":";
echo call_user_func("substr", "abcdef", 2, -2); echo ":";
echo call_user_func_array("substr", ["string" => "abcdef", "offset" => -4, "length" => 2]); echo ":";
return function_exists("substr");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "cdef:bcde:ef:cd:cd:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `substr_replace()` dispatches through direct, named, and callable paths.
#[test]
fn execute_program_dispatches_substr_replace_builtin() {
    let program = parse_fragment(
            br#"echo substr_replace("hello world", "PHP", 6, 5); echo ":";
echo substr_replace(string: "abcdef", replace: "X", offset: 1, length: -1); echo ":";
echo substr_replace("abcdef", "X", -2); echo ":";
echo call_user_func("substr_replace", "abcdef", "X", 99, 1); echo ":";
echo call_user_func_array("substr_replace", ["string" => "abcdef", "replace" => "X", "offset" => -99, "length" => 2]); echo ":";
return function_exists("substr_replace");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "hello PHP:aXf:abcdX:abcdefX:Xcdef:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `nl2br()` dispatches through direct, named, and callable paths.
#[test]
fn execute_program_dispatches_nl2br_builtin() {
    let program = parse_fragment(
        br#"echo bin2hex(nl2br("a\nb")); echo ":";
echo bin2hex(nl2br(string: "a\nb", use_xhtml: false)); echo ":";
echo bin2hex(call_user_func("nl2br", "a\r\nb")); echo ":";
echo bin2hex(call_user_func_array("nl2br", ["string" => "a\n\rb", "use_xhtml" => false])); echo ":";
return function_exists("nl2br");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "613c6272202f3e0a62:613c62723e0a62:613c6272202f3e0d0a62:613c62723e0a0d62:"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `bin2hex()` dispatches through direct, named, and callable paths.
#[test]
fn execute_program_dispatches_bin2hex_builtin() {
    let program = parse_fragment(
        br#"echo bin2hex("Az"); echo ":";
echo bin2hex(string: "A\n"); echo ":";
echo call_user_func("bin2hex", "!?"); echo ":";
echo call_user_func_array("bin2hex", ["string" => "ok"]); echo ":";
return function_exists("bin2hex");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "417a:410a:213f:6f6b:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `hex2bin()` dispatches through direct, named, and callable paths.
#[test]
fn execute_program_dispatches_hex2bin_builtin() {
    let program = parse_fragment(
        br#"echo hex2bin("417a"); echo ":";
echo bin2hex(hex2bin(string: "410a")); echo ":";
echo call_user_func("hex2bin", "213f"); echo ":";
echo call_user_func_array("hex2bin", ["string" => "6f6b"]); echo ":";
echo hex2bin("4") ? "bad" : "false"; echo ":";
return function_exists("hex2bin");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Az:410a:!?:ok:false:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
    assert_eq!(
        values.warnings,
        vec![HEX2BIN_ODD_LENGTH_WARNING.to_string()]
    );
}

/// Verifies eval slash escaping builtins use PHP byte-string semantics.
#[test]
fn execute_program_dispatches_slash_escape_builtins() {
    let program = parse_fragment(
        br#"$escaped = addslashes($source);
echo bin2hex($escaped); echo ":";
echo bin2hex(stripslashes($escaped)); echo ":";
echo call_user_func("addslashes", "x\"y"); echo ":";
echo call_user_func_array("stripslashes", [addslashes("o\"k")]); echo ":";
return function_exists("addslashes") && function_exists("stripslashes");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let source = values.string("a\0b\\c\"d'").expect("create source");
    scope.set("source", source, ScopeCellOwnership::Owned);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "615c30625c5c635c22645c27:6100625c63226427:x\\\"y:o\"k:"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `base64_encode()` dispatches through direct, named, and callable paths.
#[test]
fn execute_program_dispatches_base64_encode_builtin() {
    let program = parse_fragment(
        br#"echo base64_encode("Hello"); echo ":";
echo base64_encode(string: "Hi"); echo ":";
echo call_user_func("base64_encode", "Test 123!"); echo ":";
echo call_user_func_array("base64_encode", ["string" => ""]); echo ":";
return function_exists("base64_encode");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "SGVsbG8=:SGk=:VGVzdCAxMjMh::");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `base64_decode()` dispatches through direct, named, and callable paths.
#[test]
fn execute_program_dispatches_base64_decode_builtin() {
    let program = parse_fragment(
        br#"echo base64_decode("SGVsbG8="); echo ":";
echo base64_decode(string: "SGk="); echo ":";
echo call_user_func("base64_decode", "VGVzdCAxMjMh"); echo ":";
echo call_user_func_array("base64_decode", ["string" => ""]); echo ":";
return function_exists("base64_decode");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Hello:Hi:Test 123!::");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies `isset` distinguishes missing, null, and other falsey values.
#[test]
fn execute_program_isset_distinguishes_missing_null_and_falsey_values() {
    let program = parse_fragment(
        br#"if (isset($missing)) { echo "1"; } else { echo "0"; }
if (isset($nullish)) { echo "1"; } else { echo "0"; }
if (isset($zero)) { echo "1"; } else { echo "0"; }
if (isset($empty)) { echo "1"; } else { echo "0"; }
if (isset($zero, $empty)) { echo "1"; } else { echo "0"; }
if (isset($zero, $nullish)) { echo "1"; } else { echo "0"; }"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let nullish = values.null().expect("create fake null");
    let zero = values.int(0).expect("create fake int");
    let empty = values.string("").expect("create fake string");
    scope.set("nullish", nullish, ScopeCellOwnership::Owned);
    scope.set("zero", zero, ScopeCellOwnership::Owned);
    scope.set("empty", empty, ScopeCellOwnership::Owned);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "001110");
    assert_eq!(values.get(result), FakeValue::Null);
}

/// Verifies `empty` treats missing, null, and falsey values as empty.
#[test]
fn execute_program_empty_uses_php_truthiness_without_missing_warnings() {
    let program = parse_fragment(
        br#"if (empty($missing)) { echo "1"; } else { echo "0"; }
if (empty($nullish)) { echo "1"; } else { echo "0"; }
if (empty($zero)) { echo "1"; } else { echo "0"; }
if (empty($empty_string)) { echo "1"; } else { echo "0"; }
if (empty($zero_string)) { echo "1"; } else { echo "0"; }
if (empty($value)) { echo "1"; } else { echo "0"; }"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let nullish = values.null().expect("create fake null");
    let zero = values.int(0).expect("create fake int");
    let empty_string = values.string("").expect("create fake empty string");
    let zero_string = values.string("0").expect("create fake zero string");
    let value = values.string("x").expect("create fake non-empty string");
    scope.set("nullish", nullish, ScopeCellOwnership::Owned);
    scope.set("zero", zero, ScopeCellOwnership::Owned);
    scope.set("empty_string", empty_string, ScopeCellOwnership::Owned);
    scope.set("zero_string", zero_string, ScopeCellOwnership::Owned);
    scope.set("value", value, ScopeCellOwnership::Owned);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "111110");
    assert_eq!(values.get(result), FakeValue::Null);
}

/// Verifies `isset` and `empty` use PHP offset semantics for array reads.
#[test]
fn execute_program_isset_and_empty_support_array_offsets() {
    let program = parse_fragment(
        br#"$map = [
    "present" => "x",
    "nullish" => null,
    "zero" => 0,
    "empty" => "",
    "child" => ["leaf" => "ok", "null" => null],
];
echo isset($map["present"]) ? "1" : "0";
echo isset($map["nullish"]) ? "1" : "0";
echo isset($map["missing"]) ? "1" : "0";
echo isset($map["zero"]) ? "1" : "0";
echo isset($map["child"]["leaf"]) ? "1" : "0";
echo isset($map["child"]["null"]) ? "1" : "0";
echo isset($map["missing"]["leaf"]) ? "1" : "0";
echo ":";
echo empty($map["present"]) ? "1" : "0";
echo empty($map["nullish"]) ? "1" : "0";
echo empty($map["missing"]) ? "1" : "0";
echo empty($map["zero"]) ? "1" : "0";
echo empty($map["empty"]) ? "1" : "0";
echo empty($map["child"]["leaf"]) ? "1" : "0";
echo empty($map["child"]["null"]) ? "1" : "0";
echo empty($map["missing"]["leaf"]) ? "1" : "0";"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "1001100:01111011");
    assert_eq!(values.get(result), FakeValue::Null);
}

/// Verifies eval builtin probes see dynamic functions and supported PHP-visible builtins.
#[test]
fn execute_program_function_probes_use_eval_context() {
    let program = parse_fragment(
        br#"function dyn_probe() { return 1; }
echo function_exists("DYN_PROBE") . "x";
echo is_callable("dyn_probe") . "x";
echo function_exists("strlen") . "x";
echo function_exists("native_probe") . "x";
echo function_exists("eval") . "x";
echo function_exists("missing_probe") . "x";"#,
    )
    .expect("parse eval fragment");
    let native = NativeFunction::new(1usize as *mut c_void, fake_native_return_descriptor, 0);
    let mut context = ElephcEvalContext::new();
    assert!(context
        .define_native_function("native_probe", native)
        .is_ok());
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(values.output, "1x1x1x1xxx");
}

/// Verifies eval `interface_exists()` probes generated interface metadata by callable.
#[test]
fn execute_program_interface_exists_uses_runtime_probe() {
    let program = parse_fragment(
        br#"echo interface_exists("KnownInterface") ? "Y" : "N";
echo interface_exists("knowninterface") ? "Y" : "N";
echo interface_exists("KnownClass") ? "Y" : "N";
echo call_user_func("interface_exists", "KnownInterface") ? "Y" : "N";
echo call_user_func_array("interface_exists", ["interface" => "KnownInterface"]) ? "Y" : "N";
echo interface_exists(interface: "MissingInterface", autoload: false) ? "Y" : "N";"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "YYNYYN");
}

/// Verifies eval `trait_exists()` and `enum_exists()` probe generated metadata.
#[test]
fn execute_program_class_like_exists_uses_runtime_probe() {
    let program = parse_fragment(
        br#"echo trait_exists("KnownTrait") ? "T" : "t";
echo trait_exists("knowntrait") ? "T" : "t";
echo trait_exists("KnownEnum") ? "T" : "t";
echo enum_exists("KnownEnum") ? "E" : "e";
echo enum_exists("\knownenum") ? "E" : "e";
echo enum_exists("KnownTrait") ? "E" : "e";
echo call_user_func("trait_exists", "KnownTrait") ? "T" : "t";
echo call_user_func_array("enum_exists", ["enum" => "KnownEnum"]) ? "E" : "e";
echo trait_exists(trait: "MissingTrait", autoload: false) ? "T" : "t";
echo enum_exists(enum: "MissingEnum", autoload: false) ? "E" : "e";"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "TTtEEeTEte");
}

/// Verifies eval `is_a()` and `is_subclass_of()` dispatch through runtime class metadata.
#[test]
fn execute_program_is_a_relation_uses_runtime_probe() {
    let program = parse_fragment(
            br#"$object = new KnownClass();
echo is_a($object, "KnownClass") ? "Y" : "N";
echo is_subclass_of($object, "KnownClass") ? "Y" : "N";
echo is_subclass_of($object, "ParentClass") ? "Y" : "N";
echo call_user_func("is_a", $object, "ParentClass") ? "Y" : "N";
echo call_user_func_array("is_subclass_of", ["object_or_class" => $object, "class" => "ParentClass"]) ? "Y" : "N";
echo is_a(object_or_class: $object, class: "MissingClass", allow_string: false) ? "Y" : "N";"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "YNYYYN");
}

/// Verifies eval `define()` and `defined()` share a dynamic constant-name table.
#[test]
fn execute_program_define_and_defined_use_dynamic_constant_table() {
    let program = parse_fragment(
        br#"echo define("DynEvalConst", "ok") ? "Y" : "N";
echo DynEvalConst;
echo \DynEvalConst;
echo defined("DynEvalConst") ? "Y" : "N";
echo defined("\\DynEvalConst") ? "Y" : "N";
echo defined("dynevalconst") ? "Y" : "N";
echo define("DynEvalConst", 2) ? "Y" : "N";
echo call_user_func("defined", "DynEvalConst") ? "Y" : "N";
echo call_user_func_array("defined", ["constant_name" => "\\DynEvalConst"]) ? "Y" : "N";"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "YokokYYNNYY");
    assert_eq!(
        values.warnings,
        vec![DEFINE_ALREADY_DEFINED_WARNING.to_string()]
    );
}

/// Verifies eval predefined runtime constants are fetchable and cannot be redefined.
#[test]
fn execute_program_reads_predefined_runtime_constants() {
    let program = parse_fragment(
        br#"echo PHP_EOL === "\n" ? "eol" : "bad"; echo ":";
echo (PHP_OS === "Darwin" || PHP_OS === "Linux") ? "os" : "bad"; echo ":";
echo DIRECTORY_SEPARATOR; echo ":";
echo PHP_INT_MAX > 9000000000000000000 ? "int" : "bad"; echo ":";
echo defined("PHP_OS") ? "defined" : "bad"; echo ":";
echo defined("\\PHP_OS") ? "root" : "bad"; echo ":";
echo defined("php_os") ? "bad" : "case"; echo ":";
echo define("PHP_OS", "x") ? "bad" : "locked"; echo ":";
return PHP_INT_MAX;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "eol:os:/:int:defined:root:case:locked:");
    assert_eq!(values.get(result), FakeValue::Int(i64::MAX));
    assert_eq!(
        values.warnings,
        vec![DEFINE_ALREADY_DEFINED_WARNING.to_string()]
    );
}

/// Verifies missing eval dynamic constants fail through runtime status.
#[test]
fn execute_program_missing_constant_fetch_fails() {
    let program = parse_fragment(br#"return MissingEvalConst;"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("missing constant should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies eval class probes use the runtime class-name table.
#[test]
fn execute_program_class_exists_uses_runtime_probe() {
    let program = parse_fragment(
        br#"class DynProbe {}
echo class_exists("DynProbe") ? "Y" : "N";
echo class_exists("\dynprobe") ? "Y" : "N";
echo class_exists("KnownClass") ? "Y" : "N";
echo class_exists("\knownclass") ? "Y" : "N";
echo class_exists(class: "MissingClass", autoload: false) ? "Y" : "N";"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "YYYYN");
}

/// Verifies duplicate eval-declared class names fail through runtime status.
#[test]
fn execute_program_duplicate_class_declaration_fails() {
    let program = parse_fragment(
        br#"class DynProbeDup {}
class dynprobedup {}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values).expect_err("duplicate fails");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies eval fragments can dispatch registered native AOT functions.
#[test]
fn execute_program_calls_registered_native_function() {
    let program = parse_fragment(br#"return native_answer();"#).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let expected = values.int(42).expect("allocate fake result");
    let native = NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 0);
    assert!(context
        .define_native_function("native_answer", native)
        .is_ok());

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(result, expected);
}

/// Verifies direct eval calls can bind registered native parameters by name.
#[test]
fn execute_program_calls_registered_native_function_with_named_args() {
    let program = parse_fragment(br#"return native_answer(right: 2, left: 1);"#)
        .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let expected = values.int(42).expect("allocate fake result");
    let mut native =
        NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 2);
    assert!(native.set_param_name(0, "left"));
    assert!(native.set_param_name(1, "right"));
    assert!(context
        .define_native_function("native_answer", native)
        .is_ok());

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(result, expected);
}

/// Verifies direct eval calls can unpack arrays into registered native parameters.
#[test]
fn execute_program_calls_registered_native_function_with_spread_args() {
    let program =
        parse_fragment(br#"return native_answer(...[1, 2]);"#).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let expected = values.int(42).expect("allocate fake result");
    let mut native =
        NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 2);
    assert!(native.set_param_name(0, "left"));
    assert!(native.set_param_name(1, "right"));
    assert!(context
        .define_native_function("native_answer", native)
        .is_ok());

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(result, expected);
}

/// Verifies indexed array writes mutate an existing scope array.
#[test]
fn execute_program_writes_indexed_scope_array() {
    let program = parse_fragment(br#"$items = ["a"]; $items[1] = "b"; return $items[1];"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("b".to_string()));
}

/// Verifies indexed array append writes use the next visible index.
#[test]
fn execute_program_appends_indexed_scope_array() {
    let program = parse_fragment(br#"$items = ["a"]; $items[] = "b"; return $items[1];"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("b".to_string()));
}

/// Verifies associative append starts at key zero when only string keys exist.
#[test]
fn execute_program_appends_assoc_scope_array_with_string_keys() {
    let program =
        parse_fragment(br#"$items = ["name" => "Ada"]; $items[] = "Grace"; return $items[0];"#)
            .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("Grace".to_string()));
}

/// Verifies associative append uses one plus the largest existing integer key.
#[test]
fn execute_program_appends_assoc_scope_array_after_positive_int_key() {
    let program = parse_fragment(
        br#"$items = [2 => "two", "name" => "Ada"]; $items[] = "tail"; return $items[3];"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
}

/// Verifies associative append preserves PHP's largest-negative-key behavior.
#[test]
fn execute_program_appends_assoc_scope_array_after_negative_int_key() {
    let program =
        parse_fragment(br#"$items = [-2 => "minus"]; $items[] = "tail"; return $items[-1];"#)
            .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
}

/// Verifies mutating a borrowed scope array does not make the eval scope own it.
#[test]
fn execute_program_preserves_borrowed_array_ownership() {
    let program = parse_fragment(br#"$items[0] = "b";"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let array = values.array_new(1).expect("create fake array");
    scope.set("items", array, ScopeCellOwnership::Borrowed);

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let entry = scope.entry("items").expect("scope should contain items");

    assert_eq!(entry.cell(), array);
    assert_eq!(entry.flags().ownership, ScopeCellOwnership::Borrowed);
    assert!(values.releases.is_empty());
}

/// Verifies replacing an eval-owned scope value releases the old cell.
#[test]
fn execute_program_releases_replaced_scope_value() {
    let program = parse_fragment(br#"$x = "old"; $x = "new";"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.releases.len(), 1);
    assert_eq!(
        values.get(values.releases[0]),
        FakeValue::String("old".to_string())
    );
}

/// Verifies unsetting an eval-owned scope value releases the old cell.
#[test]
fn execute_program_releases_unset_scope_value() {
    let program = parse_fragment(br#"$x = "old"; unset($x);"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.releases.len(), 1);
    assert_eq!(
        values.get(values.releases[0]),
        FakeValue::String("old".to_string())
    );
}

/// Verifies break exits a runtime eval loop before later statements run.
#[test]
fn execute_program_break_exits_loop() {
    let program = parse_fragment(br#"while ($flag) { echo "a"; break; echo "b"; }"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let flag = values.bool_value(true).expect("create fake bool");
    scope.set("flag", flag, ScopeCellOwnership::Owned);

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "a");
}

/// Verifies continue restarts a runtime eval loop and observes later scope updates.
#[test]
fn execute_program_continue_restarts_loop() {
    let program = parse_fragment(
        br#"while ($flag) { $flag = false; continue; echo "unreachable"; } echo "done";"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let flag = values.bool_value(true).expect("create fake bool");
    scope.set("flag", flag, ScopeCellOwnership::Owned);

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "done");
}
