//! Purpose:
//! Defines numeric, bitwise, comparison, concatenation, output, byte, and
//! truthiness operations for the generated runtime adapter.
//!
//! Called from:
//! - The single `RuntimeValueOps for ElephcRuntimeOps` implementation in `super`.
//!
//! Key details:
//! - Binary and comparison op tags continue to use the shared target mappings.

macro_rules! impl_numeric_string_ops {
    () => {

    /// Computes PHP `abs()` for a boxed Mixed cell through the generated runtime wrapper.
    fn abs(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_abs(value.as_ptr()) })
    }

    /// Computes PHP `ceil()` for a boxed Mixed cell through the generated runtime wrapper.
    fn ceil(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_ceil(value.as_ptr()) })
    }

    /// Computes PHP `floor()` for a boxed Mixed cell through the generated runtime wrapper.
    fn floor(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_floor(value.as_ptr()) })
    }

    /// Computes PHP `sqrt()` for a boxed Mixed cell through the generated runtime wrapper.
    fn sqrt(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_sqrt(value.as_ptr()) })
    }

    /// Computes PHP `strrev()` for a boxed Mixed cell through the generated runtime wrapper.
    fn strrev(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_strrev(value.as_ptr()) })
    }

    /// Computes PHP `fdiv()` for boxed Mixed cells through the generated runtime wrapper.
    fn fdiv(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_fdiv(left.as_ptr(), right.as_ptr()) })
    }

    /// Computes PHP `fmod()` for boxed Mixed cells through the generated runtime wrapper.
    fn fmod(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_fmod(left.as_ptr(), right.as_ptr()) })
    }

    /// Adds two boxed Mixed cells using elephc runtime numeric semantics.
    fn add(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_add(left.as_ptr(), right.as_ptr()) })
    }

    /// Subtracts two boxed Mixed cells using elephc runtime numeric semantics.
    fn sub(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_sub(left.as_ptr(), right.as_ptr()) })
    }

    /// Multiplies two boxed Mixed cells using elephc runtime numeric semantics.
    fn mul(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_mul(left.as_ptr(), right.as_ptr()) })
    }

    /// Divides two boxed Mixed cells using elephc runtime numeric semantics.
    fn div(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_div(left.as_ptr(), right.as_ptr()) })
    }

    /// Computes modulo for two boxed Mixed cells using elephc runtime integer semantics.
    fn modulo(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_mod(left.as_ptr(), right.as_ptr()) })
    }

    /// Raises two boxed Mixed cells using elephc runtime numeric exponentiation semantics.
    fn pow(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_pow(left.as_ptr(), right.as_ptr()) })
    }

    /// Rounds a boxed Mixed cell through the generated runtime wrapper.
    fn round(
        &mut self,
        value: RuntimeCellHandle,
        precision: Option<RuntimeCellHandle>,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let (precision, has_precision) = if let Some(precision) = precision {
            (precision.as_ptr(), 1)
        } else {
            (core::ptr::null_mut(), 0)
        };
        Self::handle(unsafe { __elephc_eval_value_round(value.as_ptr(), precision, has_precision) })
    }

    /// Applies an integer bitwise or shift operation through the generated runtime wrapper.
    fn bitwise(
        &mut self,
        op: EvalBinOp,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe {
            __elephc_eval_value_bitwise(left.as_ptr(), right.as_ptr(), bitwise_op_tag(op))
        })
    }

    /// Applies integer bitwise NOT through the generated runtime wrapper.
    fn bit_not(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_bit_not(value.as_ptr()) })
    }

    /// Concatenates two boxed Mixed cells using elephc runtime string semantics.
    fn concat(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_concat(left.as_ptr(), right.as_ptr()) })
    }

    /// Compares two boxed Mixed cells through the generated runtime wrapper.
    fn compare(
        &mut self,
        op: EvalBinOp,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe {
            __elephc_eval_value_compare(left.as_ptr(), right.as_ptr(), compare_op_tag(op))
        })
    }

    /// Computes a PHP numeric spaceship result through the generated runtime wrapper.
    fn spaceship(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_spaceship(left.as_ptr(), right.as_ptr()) })
    }

    /// Emits one boxed Mixed cell to stdout through the generated runtime wrapper.
    fn echo(&mut self, value: RuntimeCellHandle) -> Result<(), EvalStatus> {
        unsafe {
            __elephc_eval_value_echo(value.as_ptr());
        }
        Ok(())
    }

    /// Casts one boxed Mixed cell to a PHP string and copies the bytes into Rust memory.
    fn string_bytes(&mut self, value: RuntimeCellHandle) -> Result<Vec<u8>, EvalStatus> {
        let mut ptr = std::ptr::null();
        let mut len = 0;
        let ok = unsafe { __elephc_eval_value_string_bytes(value.as_ptr(), &mut ptr, &mut len) };
        if ok == 0 || (len > 0 && ptr.is_null()) {
            return Err(EvalStatus::RuntimeFatal);
        }
        let len = usize::try_from(len).map_err(|_| EvalStatus::RuntimeFatal)?;
        let bytes = if len == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(ptr, len) }
        };
        Ok(bytes.to_vec())
    }

    /// Converts one boxed Mixed cell to PHP truthiness through the generated runtime wrapper.
    fn truthy(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus> {
        Ok(unsafe { __elephc_eval_value_truthy(value.as_ptr()) != 0 })
    }

    /// Pushes a new runtime output buffer through the generated ob bridge.
    fn ob_start(&mut self) -> Result<bool, EvalStatus> {
        Ok(unsafe { __elephc_eval_ob_start() } != 0)
    }

    /// Reads the runtime output-buffer nesting depth through the generated ob bridge.
    fn ob_level(&mut self) -> Result<i64, EvalStatus> {
        Ok(unsafe { __elephc_eval_ob_level() })
    }

    /// Reads the top runtime output buffer's byte count (-1 bridge sentinel → None).
    fn ob_length(&mut self) -> Result<Option<i64>, EvalStatus> {
        let length = unsafe { __elephc_eval_ob_length() };
        Ok((length >= 0).then_some(length))
    }

    /// Copies the top runtime output buffer's bytes into Rust memory.
    fn ob_contents(&mut self) -> Result<Option<Vec<u8>>, EvalStatus> {
        let mut ptr = std::ptr::null();
        let mut len = 0i64;
        let ok = unsafe { __elephc_eval_ob_contents(&mut ptr, &mut len) };
        if ok == 0 {
            return Ok(None);
        }
        if len > 0 && ptr.is_null() {
            return Err(EvalStatus::RuntimeFatal);
        }
        let len = usize::try_from(len).map_err(|_| EvalStatus::RuntimeFatal)?;
        let bytes = if len == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(ptr, len) }
        };
        Ok(Some(bytes.to_vec()))
    }

    /// Truncates the top runtime output buffer through the generated ob bridge.
    fn ob_clean(&mut self) -> Result<bool, EvalStatus> {
        Ok(unsafe { __elephc_eval_ob_clean() } != 0)
    }

    /// Flushes the top runtime output buffer to its parent sink through the ob bridge.
    fn ob_flush(&mut self) -> Result<bool, EvalStatus> {
        Ok(unsafe { __elephc_eval_ob_flush() } != 0)
    }

    /// Pops (and optionally flushes) the top runtime output buffer through the ob bridge.
    fn ob_end(&mut self, flush: bool) -> Result<bool, EvalStatus> {
        Ok(unsafe { __elephc_eval_ob_end(i64::from(flush)) } != 0)
    }

    /// Reads one buffer's `(used, size)` stats through the generated ob bridge.
    fn ob_stats(&mut self, index: i64) -> Result<Option<(i64, i64)>, EvalStatus> {
        let mut used = 0i64;
        let mut size = 0i64;
        let ok = unsafe { __elephc_eval_ob_stats(index, &mut used, &mut size) };
        Ok((ok != 0).then_some((used, size)))
    }

    /// Stores the (inert) implicit-flush flag through the generated ob bridge.
    fn ob_implicit_flush(&mut self, enable: bool) -> Result<(), EvalStatus> {
        unsafe { __elephc_eval_ob_implicit_flush(i64::from(enable)) };
        Ok(())
    }

    };
}

pub(super) use impl_numeric_string_ops;
