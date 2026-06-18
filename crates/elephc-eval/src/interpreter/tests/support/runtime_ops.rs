//! Purpose:
//! RuntimeValueOps implementation for interpreter test fake values.
//! This keeps the large trait surface separate from test fixture type
//! declarations and assertion-only conversion helpers.
//!
//! Called from:
//! - `crate::interpreter::tests::support::FakeOps` through trait dispatch.
//!
//! Key details:
//! - Methods intentionally model only the runtime behavior covered by eval tests.
//! - Handles are fake stable cells and must not be freed by this implementation.

use super::*;

impl RuntimeValueOps for FakeOps {
    /// Creates a fake indexed array cell.
    fn array_new(&mut self, capacity: usize) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_array_new(capacity)
    }
    /// Creates a fake associative array cell.
    fn assoc_new(&mut self, _capacity: usize) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_assoc_new(_capacity)
    }
    /// Reads one fake indexed array element.
    fn array_get(
        &mut self,
        array: RuntimeCellHandle,
        index: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_array_get(array, index)
    }
    /// Checks whether a fake array has the requested key without reading its value.
    fn array_key_exists(
        &mut self,
        key: RuntimeCellHandle,
        array: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_array_key_exists(key, array)
    }
    /// Returns one fake foreach key by insertion-order position.
    fn array_iter_key(
        &mut self,
        array: RuntimeCellHandle,
        position: usize,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_array_iter_key(array, position)
    }
    /// Writes one fake indexed or associative array element.
    fn array_set(
        &mut self,
        array: RuntimeCellHandle,
        index: RuntimeCellHandle,
        value: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_array_set(array, index, value)
    }
    /// Reads one fake object property by name.
    fn property_get(
        &mut self,
        object: RuntimeCellHandle,
        property: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_property_get(object, property)
    }
    /// Writes one fake object property by name.
    fn property_set(
        &mut self,
        object: RuntimeCellHandle,
        property: &str,
        value: RuntimeCellHandle,
    ) -> Result<(), EvalStatus> {
        self.runtime_property_set(object, property, value)
    }
    /// Returns the number of fake object properties in insertion order.
    fn object_property_len(&mut self, object: RuntimeCellHandle) -> Result<usize, EvalStatus> {
        self.runtime_object_property_len(object)
    }
    /// Returns one fake object property key by insertion-order position.
    fn object_property_iter_key(
        &mut self,
        object: RuntimeCellHandle,
        position: usize,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_object_property_iter_key(object, position)
    }
    /// Calls one fake object method by name.
    fn method_call(
        &mut self,
        object: RuntimeCellHandle,
        method: &str,
        args: Vec<RuntimeCellHandle>,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_method_call(object, method, args)
    }
    /// Creates one fake object for eval `new` unit tests.
    fn new_object(&mut self, _class_name: &str) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_new_object(_class_name)
    }
    /// Applies fake constructor side effects for eval `new` unit tests.
    fn construct_object(
        &mut self,
        object: RuntimeCellHandle,
        args: Vec<RuntimeCellHandle>,
    ) -> Result<(), EvalStatus> {
        self.runtime_construct_object(object, args)
    }
    /// Reports one fake AOT class for eval `class_exists` unit tests.
    fn class_exists(&mut self, name: &str) -> Result<bool, EvalStatus> {
        self.runtime_class_exists(name)
    }
    /// Reports one fake AOT interface for eval `interface_exists` unit tests.
    fn interface_exists(&mut self, name: &str) -> Result<bool, EvalStatus> {
        self.runtime_interface_exists(name)
    }
    /// Reports one fake AOT trait for eval `trait_exists` unit tests.
    fn trait_exists(&mut self, name: &str) -> Result<bool, EvalStatus> {
        self.runtime_trait_exists(name)
    }
    /// Reports one fake AOT enum for eval `enum_exists` unit tests.
    fn enum_exists(&mut self, name: &str) -> Result<bool, EvalStatus> {
        self.runtime_enum_exists(name)
    }
    /// Reports fake class relations for eval `is_a` and `is_subclass_of` unit tests.
    fn object_is_a(
        &mut self,
        object_or_class: RuntimeCellHandle,
        target_class: &str,
        exclude_self: bool,
    ) -> Result<bool, EvalStatus> {
        self.runtime_object_is_a(object_or_class, target_class, exclude_self)
    }
    /// Returns a fake PHP class name for object-tagged test values.
    fn object_class_name(
        &mut self,
        object: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_object_class_name(object)
    }
    /// Returns fake parent-class names for eval introspection unit tests.
    fn parent_class_name(
        &mut self,
        object_or_class: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_parent_class_name(object_or_class)
    }
    /// Returns the visible element count for fake array values.
    fn array_len(&mut self, array: RuntimeCellHandle) -> Result<usize, EvalStatus> {
        self.runtime_array_len(array)
    }
    /// Returns whether a fake runtime cell is an indexed or associative array.
    fn is_array_like(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus> {
        self.runtime_is_array_like(value)
    }
    /// Returns whether a fake runtime cell is null.
    fn is_null(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus> {
        self.runtime_is_null(value)
    }
    /// Returns the fake runtime tag corresponding to a test value.
    fn type_tag(&mut self, value: RuntimeCellHandle) -> Result<u64, EvalStatus> {
        self.runtime_type_tag(value)
    }
    /// Returns the fake object handle as a stable object identity.
    fn object_identity(&mut self, object: RuntimeCellHandle) -> Result<u64, EvalStatus> {
        self.runtime_object_identity(object)
    }
    /// Records fake releases without freeing handles needed for assertions.
    fn release(&mut self, value: RuntimeCellHandle) -> Result<(), EvalStatus> {
        self.runtime_release(value)
    }
    /// Returns the same fake handle because fake cells do not refcount.
    fn retain(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_retain(value)
    }
    /// Records fake PHP warnings without writing to stderr.
    fn warning(&mut self, message: &str) -> Result<(), EvalStatus> {
        self.runtime_warning(message)
    }
    /// Creates a fake null cell.
    fn null(&mut self) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_null()
    }
    /// Creates a fake bool cell.
    fn bool_value(&mut self, value: bool) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_bool_value(value)
    }
    /// Creates a fake int cell.
    fn int(&mut self, value: i64) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_int(value)
    }
    /// Creates a fake float cell.
    fn float(&mut self, value: f64) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_float(value)
    }
    /// Creates a fake string cell.
    fn string(&mut self, value: &str) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_string(value)
    }
    /// Creates a fake string cell from raw PHP bytes.
    fn string_bytes_value(&mut self, value: &[u8]) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_string_bytes_value(value)
    }
    /// Casts a fake runtime cell to a fake integer cell.
    fn cast_int(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_cast_int(value)
    }
    /// Casts a fake runtime cell to a fake float cell.
    fn cast_float(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_cast_float(value)
    }
    /// Casts a fake runtime cell to a fake string cell.
    fn cast_string(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_cast_string(value)
    }
    /// Casts a fake runtime cell to a fake boolean cell.
    fn cast_bool(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_cast_bool(value)
    }
    /// Computes fake PHP absolute value while preserving float payloads.
    fn abs(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_abs(value)
    }
    /// Computes fake PHP ceiling through numeric conversion as a float result.
    fn ceil(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_ceil(value)
    }
    /// Computes fake PHP floor through numeric conversion as a float result.
    fn floor(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_floor(value)
    }
    /// Computes fake PHP square root through numeric conversion as a float result.
    fn sqrt(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_sqrt(value)
    }
    /// Reverses a fake string byte-wise for interpreter tests.
    fn strrev(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_strrev(value)
    }
    /// Divides fake numeric cells with PHP `fdiv()` zero handling.
    fn fdiv(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_fdiv(left, right)
    }
    /// Computes fake floating-point modulo for interpreter tests.
    fn fmod(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_fmod(left, right)
    }
    /// Adds fake numeric cells for interpreter tests.
    fn add(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_add(left, right)
    }
    /// Subtracts fake numeric cells for interpreter tests.
    fn sub(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_sub(left, right)
    }
    /// Multiplies fake numeric cells for interpreter tests.
    fn mul(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_mul(left, right)
    }
    /// Divides fake numeric cells for interpreter tests.
    fn div(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_div(left, right)
    }
    /// Computes fake integer modulo for interpreter tests.
    fn modulo(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_modulo(left, right)
    }
    /// Raises fake numeric cells for interpreter tests.
    fn pow(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_pow(left, right)
    }
    /// Rounds fake numeric cells with PHP's optional decimal precision.
    fn round(
        &mut self,
        value: RuntimeCellHandle,
        precision: Option<RuntimeCellHandle>,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_round(value, precision)
    }
    /// Applies fake integer bitwise and shift operations for interpreter tests.
    fn bitwise(
        &mut self,
        op: EvalBinOp,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_bitwise(op, left, right)
    }
    /// Applies fake integer bitwise NOT for interpreter tests.
    fn bit_not(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_bit_not(value)
    }
    /// Concatenates fake cells with byte-preserving string conversion for interpreter tests.
    fn concat(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_concat(left, right)
    }
    /// Compares fake scalar cells and returns a fake PHP boolean.
    fn compare(
        &mut self,
        op: EvalBinOp,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_compare(op, left, right)
    }
    /// Compares fake numeric cells and returns a PHP spaceship integer.
    fn spaceship(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_spaceship(left, right)
    }
    /// Appends fake echo output for interpreter tests.
    fn echo(&mut self, value: RuntimeCellHandle) -> Result<(), EvalStatus> {
        self.runtime_echo(value)
    }
    /// Casts one fake runtime cell to bytes for nested eval parsing.
    fn string_bytes(&mut self, value: RuntimeCellHandle) -> Result<Vec<u8>, EvalStatus> {
        self.runtime_string_bytes(value)
    }
    /// Returns PHP-like truthiness for fake runtime cells.
    fn truthy(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus> {
        self.runtime_truthy(value)
    }
}
