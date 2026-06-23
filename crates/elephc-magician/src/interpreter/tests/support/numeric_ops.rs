//! Purpose:
//! Numeric, bitwise, comparison, concatenation, cast, and string-reversal fake runtime operations.
//!
//! Called from:
//! - `crate::interpreter::tests::support::runtime_ops`.
//!
//! Key details:
//! - These helpers implement PHP-like scalar coercions used by expression and builtin tests.

use super::*;

impl FakeOps {
    /// Casts a fake runtime cell to a fake integer cell.
    pub(super) fn runtime_cast_int(
        &mut self,
        value: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let value = self.get(value);
        let value = self.fake_int(&value);
        self.int(value)
    }
    /// Casts a fake runtime cell to a fake float cell.
    pub(super) fn runtime_cast_float(
        &mut self,
        value: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let value = self.get(value);
        let value = self.fake_numeric(&value);
        self.float(value)
    }
    /// Casts a fake runtime cell to a fake string cell.
    pub(super) fn runtime_cast_string(
        &mut self,
        value: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let value = self.stringify(value);
        self.string(&value)
    }
    /// Casts a fake runtime cell to a fake boolean cell.
    pub(super) fn runtime_cast_bool(
        &mut self,
        value: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let value = self.get(value);
        let value = self.fake_truthy(&value);
        self.bool_value(value)
    }
    /// Computes fake PHP absolute value while preserving float payloads.
    pub(super) fn runtime_abs(
        &mut self,
        value: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        match self.get(value) {
            FakeValue::Float(value) => self.float(value.abs()),
            value => self.int(self.fake_int(&value).wrapping_abs()),
        }
    }
    /// Computes fake PHP ceiling through numeric conversion as a float result.
    pub(super) fn runtime_ceil(
        &mut self,
        value: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let value = self.get(value);
        self.float(self.fake_numeric(&value).ceil())
    }
    /// Computes fake PHP floor through numeric conversion as a float result.
    pub(super) fn runtime_floor(
        &mut self,
        value: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let value = self.get(value);
        self.float(self.fake_numeric(&value).floor())
    }
    /// Computes fake PHP square root through numeric conversion as a float result.
    pub(super) fn runtime_sqrt(
        &mut self,
        value: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let value = self.get(value);
        self.float(self.fake_numeric(&value).sqrt())
    }
    /// Reverses a fake string byte-wise for interpreter tests.
    pub(super) fn runtime_strrev(
        &mut self,
        value: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let mut bytes = self.stringify(value).into_bytes();
        bytes.reverse();
        let value = String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
        self.string(&value)
    }
    /// Divides fake numeric cells with PHP `fdiv()` zero handling.
    pub(super) fn runtime_fdiv(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let left = self.fake_numeric(&self.get(left));
        let right = self.fake_numeric(&self.get(right));
        self.float(left / right)
    }
    /// Computes fake floating-point modulo for interpreter tests.
    pub(super) fn runtime_fmod(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let left = self.fake_numeric(&self.get(left));
        let right = self.fake_numeric(&self.get(right));
        self.float(left % right)
    }
    /// Adds fake numeric cells for interpreter tests.
    pub(super) fn runtime_add(
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
    pub(super) fn runtime_sub(
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
    pub(super) fn runtime_mul(
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
    pub(super) fn runtime_div(
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
    pub(super) fn runtime_modulo(
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
    pub(super) fn runtime_pow(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let left = self.fake_numeric(&self.get(left));
        let right = self.fake_numeric(&self.get(right));
        self.float(left.powf(right))
    }
    /// Rounds fake numeric cells with PHP's optional decimal precision.
    pub(super) fn runtime_round(
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
    pub(super) fn runtime_bitwise(
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
    pub(super) fn runtime_bit_not(
        &mut self,
        value: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let value = self.fake_int(&self.get(value));
        self.int(!value)
    }
    /// Concatenates fake cells with byte-preserving string conversion for interpreter tests.
    pub(super) fn runtime_concat(
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
    pub(super) fn runtime_compare(
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
    pub(super) fn runtime_spaceship(
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
}
