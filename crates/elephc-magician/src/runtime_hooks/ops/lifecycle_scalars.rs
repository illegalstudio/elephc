//! Purpose:
//! Defines object identity, retain/release, warning, scalar construction, and
//! scalar-cast methods for the generated runtime adapter.
//!
//! Called from:
//! - The single `RuntimeValueOps for ElephcRuntimeOps` implementation in `super`.
//!
//! Key details:
//! - Every runtime pointer is validated before it becomes a handle.

macro_rules! impl_lifecycle_scalar_ops {
    () => {

    /// Returns the unboxed object payload pointer for SPL object identity builtins.
    fn object_identity(&mut self, object: RuntimeCellHandle) -> Result<u64, EvalStatus> {
        let identity = unsafe { __elephc_eval_value_object_identity(object.as_ptr()) };
        if identity == 0 {
            Err(EvalStatus::RuntimeFatal)
        } else {
            Ok(identity)
        }
    }

    /// Returns the PHP object handle reported by `spl_object_id()`.
    fn php_object_handle(&mut self, object: RuntimeCellHandle) -> Result<u64, EvalStatus> {
        let handle = unsafe { __elephc_eval_value_object_handle(object.as_ptr()) };
        if handle == 0 {
            Err(EvalStatus::RuntimeFatal)
        } else {
            Ok(handle)
        }
    }

    /// Returns the object payload that the next release would destroy, when known.
    fn final_object_identity_for_release(
        &mut self,
        value: RuntimeCellHandle,
    ) -> Result<Option<u64>, EvalStatus> {
        let identity = unsafe { __elephc_eval_value_final_object_identity(value.as_ptr()) };
        Ok((identity != 0).then_some(identity))
    }

    /// Releases a Mixed owner and schedules any contained native destructor exception for eval.
    fn release(&mut self, value: RuntimeCellHandle) -> Result<(), EvalStatus> {
        let mut throwable = std::ptr::null_mut();
        unsafe {
            __elephc_eval_value_release_v2(value.as_ptr(), &mut throwable);
        }
        if !throwable.is_null() {
            self.schedule_pending_throw(RuntimeCellHandle::from_raw(throwable))?;
            return Err(EvalStatus::UncaughtThrowable);
        }
        Ok(())
    }

    /// Forces collection and schedules a bounded native Throwable for eval's catch machinery.
    fn gc_collect_cycles(&mut self) -> Result<i64, EvalStatus> {
        let mut throwable = std::ptr::null_mut();
        let count = unsafe { __elephc_eval_gc_collect_cycles(&mut throwable) };
        if throwable.is_null() {
            return Ok(count);
        }
        let throwable = RuntimeCellHandle::from_raw(throwable);
        if let Err(status) = self.schedule_pending_throw(throwable) {
            self.release(throwable)?;
            return Err(status);
        }
        Err(EvalStatus::UncaughtThrowable)
    }

    /// Disables generated-runtime automatic collection safe points.
    fn gc_disable(&mut self) -> Result<(), EvalStatus> {
        unsafe {
            __elephc_eval_gc_disable();
        }
        Ok(())
    }

    /// Enables generated-runtime automatic collection safe points.
    fn gc_enable(&mut self) -> Result<(), EvalStatus> {
        unsafe {
            __elephc_eval_gc_enable();
        }
        Ok(())
    }

    /// Reads the generated-runtime automatic collection flag.
    fn gc_enabled(&mut self) -> Result<bool, EvalStatus> {
        Ok(unsafe { __elephc_eval_gc_enabled() } != 0)
    }

    /// Flushes generated-runtime allocator caches and returns reclaimed bytes.
    fn gc_mem_caches(&mut self) -> Result<i64, EvalStatus> {
        Ok(unsafe { __elephc_eval_gc_mem_caches() })
    }

    /// Reads one generated-runtime GC status metric.
    fn gc_status_metric(&mut self, metric: u64) -> Result<i64, EvalStatus> {
        Ok(unsafe { __elephc_eval_gc_status_metric(metric) })
    }

    /// Reads one generated-runtime GC timing metric from its scalar ABI bit pattern.
    fn gc_status_time(&mut self, metric: u64) -> Result<f64, EvalStatus> {
        let bits = unsafe { __elephc_eval_gc_status_metric(metric) } as u64;
        Ok(f64::from_bits(bits))
    }

    /// Retains one boxed Mixed cell through the generated runtime wrapper.
    fn retain(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        Ok(RuntimeCellHandle::from_raw(unsafe {
            __elephc_eval_value_retain(value.as_ptr())
        }))
    }

    /// Attaches retained receiver cells to the native object's GC graph and final-release callback.
    fn retain_object_children(
        &mut self,
        object: RuntimeCellHandle,
        children: &[RuntimeCellHandle],
    ) -> Result<(), EvalStatus> {
        crate::runtime_hooks::object_owners::retain_object_children(self, object, children)
    }

    /// Emits one PHP warning through the generated runtime diagnostic helper.
    fn warning(&mut self, message: &str) -> Result<(), EvalStatus> {
        // Magician submits complete diagnostics, unlike native fragment producers.
        let terminated;
        let message = if message.ends_with('\n') { message } else {
            terminated = format!("{message}\n");
            &terminated
        };
        unsafe {
            __elephc_eval_warning(message.as_ptr(), message.len() as u64);
        }
        Ok(())
    }

    /// Emits one unsuppressible PHP fatal through the generated runtime and terminates.
    fn fatal(&mut self, message: &str) -> Result<(), EvalStatus> {
        unsafe {
            __elephc_eval_fatal(message.as_ptr(), message.len() as u64);
        }
        Err(EvalStatus::RuntimeFatal)
    }

    /// Mirrors eval handler execution into the generated runtime Fiber-switch guard.
    fn set_pcntl_dispatching(&mut self, active: bool) -> Result<(), EvalStatus> {
        crate::context::pcntl_runtime::set_fiber_dispatching(active);
        unsafe {
            __elephc_eval_set_pcntl_dispatching(u64::from(active));
        }
        Ok(())
    }

    /// Creates a boxed null Mixed cell through the generated runtime wrapper.
    fn null(&mut self) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_null() })
    }

    /// Creates a boxed bool Mixed cell through the generated runtime wrapper.
    fn bool_value(&mut self, value: bool) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_bool(u64::from(value)) })
    }

    /// Creates a boxed int Mixed cell through the generated runtime wrapper.
    fn int(&mut self, value: i64) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_int(value) })
    }

    /// Creates a boxed resource Mixed cell through the generated runtime wrapper.
    fn resource(&mut self, value: i64) -> Result<RuntimeCellHandle, EvalStatus> {
        let resource = Self::handle(unsafe { __elephc_eval_value_resource(value) })?;
        if let Some(context) = unsafe { self.context.as_ref() } {
            let subtype = match context.stream_resources().resource_type(value) {
                Some("stream-context") => Some(10),
                Some("stream filter") => Some(9),
                Some("Unknown") => Some(-1),
                _ => None,
            };
            if let Some(subtype) = subtype {
                unsafe { __elephc_eval_resource_state(resource.as_ptr(), subtype); }
            }
        }
        Ok(resource)
    }

    /// Creates a boxed inert hash-context Mixed cell through the generated runtime wrapper.
    ///
    /// The wrapper stamps resource kind 5, so `__rt_mixed_from_value` skips PHP id
    /// binding and `__rt_mixed_free_deep` runs no destructor: PHP counts a
    /// `HashContext` in the object-handle space, and the native context behind this key
    /// is owned by `crate::stream_resources::EvalHashContext`.
    fn hash_context(&mut self, value: i64) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_hash_context(value) })
    }

    /// Creates a boxed float Mixed cell through the generated runtime wrapper.
    fn float(&mut self, value: f64) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_float(value) })
    }

    /// Creates a boxed string Mixed cell through the generated runtime wrapper.
    fn string(&mut self, value: &str) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_string(value.as_ptr(), value.len() as u64) })
    }

    /// Creates a boxed string Mixed cell from raw PHP bytes through the generated runtime wrapper.
    fn string_bytes_value(&mut self, value: &[u8]) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_string(value.as_ptr(), value.len() as u64) })
    }

    /// Casts a boxed Mixed cell to a boxed integer Mixed cell through the generated runtime wrapper.
    fn cast_int(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_cast_int(value.as_ptr()) })
    }

    /// Casts a boxed Mixed cell to a boxed float Mixed cell through the generated runtime wrapper.
    fn cast_float(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_cast_float(value.as_ptr()) })
    }

    /// Casts a boxed Mixed cell to a boxed string Mixed cell through the generated runtime wrapper.
    fn cast_string(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_cast_string(value.as_ptr()) })
    }

    /// Casts a boxed Mixed cell to a boxed boolean Mixed cell through the generated runtime wrapper.
    fn cast_bool(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_cast_bool(value.as_ptr()) })
    }

    };
}

pub(super) use impl_lifecycle_scalar_ops;
