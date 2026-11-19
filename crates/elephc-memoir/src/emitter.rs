//! Purpose:
//! Defines the host assembly and ABI operations required by memory emitters.
//!
//! Called from:
//! - The elephc compiler memory subsystem.
//!
//! Key details:
//! - This code runs in the compiler and emits assembly; it is never a target-program library.

/// Compiler-owned PHP object layout consumed by pure graph visitors.
#[derive(Clone, Copy)]
pub struct GraphLayout {
    /// Byte layout fact supplied by the compiler.
    pub fiber_callable_offset: i32,
    /// Byte layout fact supplied by the compiler.
    pub fiber_object_size: i32,
    /// Byte layout fact supplied by the compiler.
    pub fiber_own_call_frame_offset: i32,
    /// Byte layout fact supplied by the compiler.
    pub fiber_pending_throw_offset: i32,
    /// Byte layout fact supplied by the compiler.
    pub fiber_stack_base_offset: i32,
    /// Byte layout fact supplied by the compiler.
    pub fiber_stack_top_offset: i32,
    /// Byte layout fact supplied by the compiler.
    pub fiber_start_args_max: i32,
    /// Byte layout fact supplied by the compiler.
    pub fiber_start_args_offset: i32,
    /// Byte layout fact supplied by the compiler.
    pub fiber_transfer_value_offset: i32,
    /// Byte layout fact supplied by the compiler.
    pub fiber_user_arg_max_offset: i32,
    /// Byte layout fact supplied by the compiler.
    pub gen_last_key_offset: i32,
    /// Byte layout fact supplied by the compiler.
    pub gen_last_value_offset: i32,
    /// Byte layout fact supplied by the compiler.
    pub gen_return_value_offset: i32,
    /// Compiler-owned special object storage offset.
    pub fiber_stack_size_offset: i32,
}

/// EIR GC metric selectors supplied without a dependency on the compiler IR.
#[derive(Clone, Copy)]
pub struct GcStatusSelectors {
    /// Stable EIR selector for this runtime metric.
    pub application_time: i64,
    /// Stable EIR selector for this runtime metric.
    pub collected: i64,
    /// Stable EIR selector for this runtime metric.
    pub collector_time: i64,
    /// Stable EIR selector for this runtime metric.
    pub destructor_time: i64,
    /// Stable EIR selector for this runtime metric.
    pub free_time: i64,
    /// Stable EIR selector for this runtime metric.
    pub protected: i64,
    /// Stable EIR selector for this runtime metric.
    pub roots: i64,
    /// Stable EIR selector for this runtime metric.
    pub running: i64,
    /// Stable EIR selector for this runtime metric.
    pub runs: i64,
}

mod output;
mod target;
mod context;
pub use output::AssemblyEmitter;
pub use target::TargetAbi;
pub use context::MemoryContext;

/// Composes output, target ABI and memory-context capabilities for complete emitters.
pub trait Emitter: AssemblyEmitter + TargetAbi + MemoryContext {}

impl<T: AssemblyEmitter + TargetAbi + MemoryContext> Emitter for T {}
