//! Purpose:
//! Defines fake identity, retain/release, warning, scalar construction, and cast
//! trait methods.
//!
//! Called from:
//! - The single `RuntimeValueOps for FakeOps` implementation in `super`.
//!
//! Key details:
//! - Operations retain the fake runtime's stable-cell ownership model.

macro_rules! impl_fake_lifecycle_scalar_ops {
    () => {

    /// Returns the fake object handle as a stable object identity.
    fn object_identity(&mut self, object: RuntimeCellHandle) -> Result<u64, EvalStatus> {
        self.runtime_object_identity(object)
    }
    /// Returns the fake object handle as the PHP object handle: the fake runtime
    /// mints one small stable integer per object, which is the same shape PHP's
    /// handles have, so identity-comparison tests behave identically.
    fn php_object_handle(&mut self, object: RuntimeCellHandle) -> Result<u64, EvalStatus> {
        self.runtime_object_identity(object)
    }
    /// Returns fake object identity for releases that target object cells.
    fn final_object_identity_for_release(
        &mut self,
        value: RuntimeCellHandle,
    ) -> Result<Option<u64>, EvalStatus> {
        if self.runtime_type_tag(value)? == EVAL_TAG_OBJECT {
            self.runtime_object_identity(value).map(Some)
        } else {
            Ok(None)
        }
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
    ///
    /// Gated on the `@` depth exactly like the real bridge, so a test asserting the
    /// suppression sees the same silence a compiled program produces.
    fn warning(&mut self, message: &str) -> Result<(), EvalStatus> {
        if self.suppress_depth > 0 {
            return Ok(());
        }
        self.runtime_warning(message)
    }

    fn suppress_begin(&mut self) {
        self.suppress_depth = self.suppress_depth.saturating_add(1);
    }

    fn suppress_end(&mut self) {
        self.suppress_depth = self.suppress_depth.saturating_sub(1);
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
    /// Creates a fake resource cell.
    fn resource(&mut self, value: i64) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_resource(value)
    }
    /// Creates a fake inert hash-context cell, consuming no fake PHP resource id.
    fn hash_context(&mut self, value: i64) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_hash_context(value)
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

    };
}

pub(super) use impl_fake_lifecycle_scalar_ops;
