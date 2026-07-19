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
    /// Pushes a fake output buffer, mirroring the runtime's 64-level nesting cap.
    fn ob_start(&mut self) -> Result<bool, EvalStatus> {
        if self.ob_stack.len() >= 64 {
            return Ok(false);
        }
        self.ob_stack.push(String::new());
        Ok(true)
    }
    /// Returns the fake output-buffer nesting depth.
    fn ob_level(&mut self) -> Result<i64, EvalStatus> {
        Ok(self.ob_stack.len() as i64)
    }
    /// Returns the top fake output buffer's byte count.
    fn ob_length(&mut self) -> Result<Option<i64>, EvalStatus> {
        Ok(self.ob_stack.last().map(|buffer| buffer.len() as i64))
    }
    /// Returns a copy of the top fake output buffer's bytes.
    fn ob_contents(&mut self) -> Result<Option<Vec<u8>>, EvalStatus> {
        Ok(self.ob_stack.last().map(|buffer| buffer.clone().into_bytes()))
    }
    /// Truncates the top fake output buffer in place.
    fn ob_clean(&mut self) -> Result<bool, EvalStatus> {
        match self.ob_stack.last_mut() {
            Some(buffer) => {
                buffer.clear();
                Ok(true)
            }
            None => Ok(false),
        }
    }
    /// Flushes the top fake output buffer to its parent sink without popping it.
    fn ob_flush(&mut self) -> Result<bool, EvalStatus> {
        let Some(contents) = self.ob_stack.last_mut().map(std::mem::take) else {
            return Ok(false);
        };
        match self.ob_stack.len() {
            1 => self.output.push_str(&contents),
            depth => self.ob_stack[depth - 2].push_str(&contents),
        }
        Ok(true)
    }
    /// Pops (and optionally flushes) the top fake output buffer.
    fn ob_end(&mut self, flush: bool) -> Result<bool, EvalStatus> {
        let Some(contents) = self.ob_stack.pop() else {
            return Ok(false);
        };
        if flush {
            match self.ob_stack.last_mut() {
                Some(parent) => parent.push_str(&contents),
                None => self.output.push_str(&contents),
            }
        }
        Ok(true)
    }
    /// Reports `(used, size)` for one fake buffer with a fixed nominal capacity.
    fn ob_stats(&mut self, index: i64) -> Result<Option<(i64, i64)>, EvalStatus> {
        let index = usize::try_from(index).ok();
        Ok(index
            .and_then(|index| self.ob_stack.get(index))
            .map(|buffer| (buffer.len() as i64, 1024)))
    }
    /// Records the (inert) fake implicit-flush flag.
    fn ob_implicit_flush(&mut self, enable: bool) -> Result<(), EvalStatus> {
        self.ob_implicit_flush = enable;
        Ok(())
    }

    };
}

pub(super) use impl_fake_numeric_string_ops;
