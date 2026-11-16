//! Purpose:
//! Supplies anonymous private mapping flags for generated stacks.
//!
//! Called from:
//! - The compiler Memoir emitter adapter.
//!
//! Key details:
//! - Platform policy stays outside the memory emission crate.

use crate::codegen_support::platform::Platform;

/// Returns the platform-specific flag word for `MAP_PRIVATE | MAP_ANONYMOUS`.
/// - macOS: `0x1002` (MAP_PRIVATE | MAP_ANON)
/// - Linux: `0x22` (MAP_PRIVATE | MAP_ANONYMOUS)
pub(super) fn map_anon_private_flags(platform: Platform) -> i32 {
    match platform {
        Platform::MacOS => 0x1002, // MAP_PRIVATE | MAP_ANON
        Platform::Linux => 0x22,   // MAP_PRIVATE | MAP_ANONYMOUS
        Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
    }
}
