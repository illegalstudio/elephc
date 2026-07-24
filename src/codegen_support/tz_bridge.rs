//! Purpose:
//! Call-site support for routing windows-x86_64 IANA-offset resolution through
//! the elephc-tz staticlib. Publishes the offset and abbreviation C entry points
//! into runtime function-pointer slots immediately before a date/mktime/strtotime
//! call site whose target is windows-x86_64, mirroring
//! `hash_crypto::publish_elephc_crypto_function_pointers`.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::system` date/gmdate, mktime/gmmktime,
//!   and strtotime lowerers (each publishes the fn pointer immediately before its
//!   `__rt_date`/`__rt_mktime`/`__rt_strtotime` call, windows-x86_64 only).
//!
//! Key details:
//! - The fn pointer is published indirectly (mirroring the `_elephc_crypto_hash_fn`
//!   pattern) so only windows programs that actually call a date/time builtin
//!   reference the elephc-tz offset entry point and therefore pull in
//!   `-lelephc_tz` at link time (pay-for-use). The shared `__rt_sys_localtime` /
//!   `__rt_sys_mktime` Win32 shims that consume the slot
//!   (`codegen_support/runtime/win32/shims_time.rs`) gracefully fall back to raw
//!   msvcrt `localtime()`/`mktime()` when the slot is unpublished (null) —
//!   exactly today's behavior — so a missed call site degrades, it never breaks.
//! - x86_64-only: elephc's Windows PE target is x86_64-only (see
//!   `Emitter::entry_symbol`'s `panic!` for Windows ARM64), so unlike
//!   `hash_crypto` (which targets every platform/arch) there is no AArch64
//!   branch to mirror.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::{Arch, Platform};

/// Publishes the elephc-tz offset and abbreviation entry points into their
/// runtime slots so the Windows time shims can synthesize complete `struct tm`
/// timezone fields from the baked IANA transition tables.
///
/// Callers must guard this with a `target.platform == Platform::Windows` check
/// before emitting it (mirroring every other late-bound Windows-only entry
/// point): emitting it unconditionally would make every target reference the
/// elephc-tz staticlib, defeating pay-for-use linking.
pub(crate) fn publish_elephc_tz_offset_function_pointer(emitter: &mut Emitter) {
    debug_assert_eq!(
        (emitter.target.platform, emitter.target.arch),
        (Platform::Windows, Arch::X86_64),
        "the elephc-tz offset fn pointer is windows-x86_64 only"
    );
    let extern_sym = emitter.target.extern_symbol("elephc_tz_offset");
    abi::emit_extern_symbol_address(emitter, "r9", &extern_sym);
    abi::emit_store_reg_to_symbol(emitter, "r9", "_elephc_tz_offset_fn", 0); // publish the elephc-tz offset entry into its runtime slot
    let abbreviation_sym = emitter.target.extern_symbol("elephc_tz_abbreviation");
    abi::emit_extern_symbol_address(emitter, "r9", &abbreviation_sym);
    abi::emit_store_reg_to_symbol(emitter, "r9", "_elephc_tz_abbreviation_fn", 0); // publish stable transition-abbreviation lookup
}
