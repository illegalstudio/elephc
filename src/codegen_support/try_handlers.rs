//! Purpose:
//! Defines the stack layout constants for EIR exception-handler slots.
//! Keeps try/catch frame offsets available to codegen and runtime emitters.
//!
//! Called from:
//! - `crate::codegen::frame` when reserving handler slots.
//! - `crate::codegen::lower_inst` when writing handler metadata.
//!
//! Key details:
//! - Offsets must stay synchronized with the runtime exception handler ABI.

/// Size of the pre-allocated try handler slot.
///
/// The `jmp_buf` runs from [`TRY_HANDLER_JMP_BUF_OFFSET`] to 224 — glibc's is 200 bytes and Apple
/// AArch64's 192, so 24 + 200 is exactly the old size — and the filter counters are appended
/// ABOVE it. Growing the slot upwards is deliberate: the four PDO callback firewalls build a
/// record of this shape BY HAND and assert `next@0, survivor@8, diag@16, jmp_buf@24` at compile
/// time, so moving the `jmp_buf` to make room lower down would have rewritten four hand-rolled
/// frames to buy nothing. They simply do not save the fields past the `jmp_buf`.
pub(crate) const TRY_HANDLER_SLOT_SIZE: usize = 256;

/// Offset within the try handler slot for the diagnostic depth field.
pub(crate) const TRY_HANDLER_DIAG_DEPTH_OFFSET: usize = 16;

/// Offset within the try handler slot for the `jmp_buf` field.
pub(crate) const TRY_HANDLER_JMP_BUF_OFFSET: usize = 24;

/// Offset within the try handler slot for the `php://filter` suppression depth.
pub(crate) const TRY_HANDLER_FILTER_SUPPRESSION_OFFSET: usize = 224;

/// Offset within the try handler slot for the filtered-open depth.
pub(crate) const TRY_HANDLER_FILTER_OPEN_DEPTH_OFFSET: usize = 232;

/// Offset within the try handler slot for the parked-hand-off depth.
pub(crate) const TRY_HANDLER_FILTER_PENDING_DEPTH_OFFSET: usize = 240;

/// The runtime depth counters a try handler saves on entry and republishes when it is left.
///
/// Every one of these is a counter some construct raises on the way in and gives back on the way
/// out, through a pop the `longjmp` out of a `throw` never reaches. `_rt_diag_suppression` has
/// been parked here since `@` learned to nest; the three filter counters leak by exactly the same
/// mechanism and were not, so an exception thrown out of a user wrapper's `stream_open` spent one
/// filtered-open frame and one parked hand-off per throw. Ten throws exhausted the eight-frame
/// bound, and past it `__rt_php_filter_suppress_begin` parks nothing and so opens no suppression
/// scope: the next FAILED filtered open let its inner opener speak, printing
/// `fopen(absent.txt): ... No such file or directory` where php prints one line naming the whole
/// `php://filter/...` URL.
///
/// Kept as a TABLE because four call sites save and restore this set — the compiled `try`, the
/// two eval helper boundaries and the runtime callable invoker — and a counter added to the set
/// at three of them would be this defect again, silent at the fourth.
pub(crate) const TRY_HANDLER_SAVED_DEPTHS: [(&str, usize); 4] = [
    ("_rt_diag_suppression", TRY_HANDLER_DIAG_DEPTH_OFFSET),
    ("_php_filter_suppression", TRY_HANDLER_FILTER_SUPPRESSION_OFFSET),
    ("_php_filter_open_depth", TRY_HANDLER_FILTER_OPEN_DEPTH_OFFSET),
    ("_php_filter_pending_depth", TRY_HANDLER_FILTER_PENDING_DEPTH_OFFSET),
];
