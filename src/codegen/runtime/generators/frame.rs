//! Purpose:
//! Layout constants for the heap-allocated `GeneratorFrame` struct backing
//! every PHP `Generator` object. Defines the 80-byte fixed header plus the
//! generator-specific param/local slots that follow it.
//!
//! Called from:
//!  - `crate::codegen::runtime::generators` (frame slot reads/writes in the
//!    `__rt_gen_*` helpers).
//!  - `crate::codegen::functions::generator::emit` (wrapper allocation and
//!    resume-function slot accesses).
//!
//! Key details:
//!  - Mixed-typed slots store **pre-boxed Mixed pointers** (8 bytes each), not
//!    raw 16-byte payloads. Yield codegen invokes `__rt_mixed_from_value` and
//!    stores the resulting pointer; the runtime helpers just load and return.
//!  - Layout (offsets in bytes):
//!    ```text
//!    +0   class_id : u64           synthetic Generator class id
//!    +8   resume_fn_ptr : u64      `<f>__resume` address
//!    +16  state_idx : u32          resume label index (0 = entry)
//!    +20  flags : u32              bit 0 = rewound, bit 1 = terminated
//!    +24  auto_key_counter : u64   for `yield $v` without an explicit key
//!    +32  last_key : u64           boxed Mixed cell pointer
//!    +40  last_value : u64         boxed Mixed cell pointer
//!    +48  return_value : u64       boxed Mixed cell pointer
//!    +56  sent_value : u64         boxed Mixed cell pointer (Generator::send)
//!    +64  delegated_iter : u64     active inner iterator for `yield from`
//!    +72  layout_id : u64          index into `_gen_frame_layouts`
//!    +80  <params then locals, sized per generator function>
//!    ```

#![allow(dead_code)] // Some layout constants are only consumed by feature-specific generator paths.

/// Generator heap kind marker.
pub const HEAP_KIND_GENERATOR: u8 = 4;

/// Generator frame offset: class ID.
pub const OFF_CLASS_ID: usize = 0;
/// Generator frame offset: resume function pointer.
pub const OFF_RESUME_FN: usize = 8;
/// Generator frame offset: state index.
pub const OFF_STATE_IDX: usize = 16;
/// Generator frame offset: flags.
pub const OFF_FLAGS: usize = 20;
/// Generator frame offset: auto key counter.
pub const OFF_AUTO_KEY_COUNTER: usize = 24;
/// Generator frame offset: last key.
pub const OFF_LAST_KEY: usize = 32;
/// Generator frame offset: last value.
pub const OFF_LAST_VALUE: usize = 40;
/// Generator frame offset: return value.
pub const OFF_RETURN_VALUE: usize = 48;
/// Generator frame offset: sent value.
pub const OFF_SENT_VALUE: usize = 56;
/// Generator frame offset: delegated iterator.
pub const OFF_DELEGATED_ITER: usize = 64;
/// Generator frame offset: layout ID.
pub const OFF_LAYOUT_ID: usize = 72;
/// Generator frame fixed header size in bytes.
pub const FIXED_HEADER_BYTES: usize = 80;

/// Generator flag: rewound.
pub const FLAG_REWOUND: u32 = 1;
/// Generator flag: terminated.
pub const FLAG_TERMINATED: u32 = 2;

/// 16-byte alignment is required because `__rt_heap_alloc` returns
/// 16-byte-aligned pointers and the frame is treated as an object body.
pub fn aligned_frame_size(params_and_locals_bytes: usize) -> usize {
    let unaligned = FIXED_HEADER_BYTES + params_and_locals_bytes;
    (unaligned + 15) & !15
}
