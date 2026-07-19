//! Purpose:
//! Defines fake numeric, bitwise, comparison, string, echo, byte, and truthiness
//! trait methods.
//!
//! Called from:
//! - The single `RuntimeValueOps for FakeOps` implementation in `super`.
//!
//! Key details:
//! - PHP-like fake behavior remains in the existing `runtime_*` helpers.

macro_rules! impl_fake_numeric_string_ops {
    () => {

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

    };
}

pub(super) use impl_fake_numeric_string_ops;
