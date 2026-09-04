//! Purpose:
//! Defines the `PHP_OUTPUT_HANDLER_*` integer constants ext-standard's output buffering exposes.
//! Single source of truth for the phase bits a handler receives, the capability flags
//! `ob_start()` accepts, and the status bits `ob_get_status()` reports.
//!
//! Called from:
//! - `crate::types::checker::driver::init` when registering predefined constants.
//! - `crate::codegen_support::prescan` when materializing constant literal values.
//!
//! Key details:
//! - The runtime already used the VALUES — `ob_buffer` writes 112 for `STDFLAGS` — while the
//!   NAMES reached no program: `ob_start(null, 0, PHP_OUTPUT_HANDLER_REMOVABLE)` was a compile
//!   error. A value that is right and a name that is missing is the same catalogue gap as a
//!   registered class nothing can open.
//! - A domain table of its own rather than an entry in `stream_constants`: these name output
//!   BUFFERING behaviour, which no stream wrapper is party to.

/// Tuple of `(name, value)` pairs for the output-buffering handler constants.
pub(crate) use elephc_builtin_contract::php_constants::OUTPUT_HANDLER_INT_CONSTANTS;
