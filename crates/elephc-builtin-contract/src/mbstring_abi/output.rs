//! Purpose:
//! Defines host construction callbacks for completed mbstring array results.
//!
//! Called from:
//! - The shared graph restorer and target-aware native result materializer.
//!
//! Key details:
//! - Inputs borrow validated scalar/string descriptors or already completed child arrays.
//! - Construction acquires independent ownership of keys and values before returning.
//! - These callbacks cannot execute PHP, mutate inputs, or unwind across Rust frames.

use std::ffi::c_void;
use super::host::MbHostValueV1;

/// Builds one fresh array from insertion-ordered borrowed key/value pairs.
/// A nonzero return transfers one owned array handle; zero reports failure after local cleanup.
/// Arrays in value descriptors are completed and immutable for the remainder of restoration.
/// Keys retain their exact integer/string identity, including numeric string keys.
/// `indexed` is one exactly for consecutive integer keys starting at zero, otherwise zero.
/// The result must use that indexed/associative representation, including for empty lists.
pub type MbArrayBuildV1 = unsafe extern "C" fn(
    context: *mut c_void, count: u64, keys: *const MbHostValueV1, values: *const MbHostValueV1, indexed: u64,
) -> u64;

/// Builds one INI array under the same ownership and representation rules as MbArrayBuildV1.
/// `identities` borrows `count` tokens: a live identity for each string value, zero for other values.
/// Keys have no identity token. The host binds each copied string to its token before returning,
/// acquiring a lease independent of the wire result and releasing it with the final string owner.
/// All descriptor and token storage is borrowed only for this callback; it must not escape.
pub type MbIniArrayBuildV1 = unsafe extern "C" fn(
    context: *mut c_void, count: u64, keys: *const MbHostValueV1, values: *const MbHostValueV1,
    indexed: u64, identities: *const u64,
) -> u64;

/// Consumes one owned handle returned by MbArrayBuildV1, without throwing or executing PHP.
/// Every completed array contains only scalars, strings, and other completed arrays.
pub type MbArrayReleaseV1 = unsafe extern "C" fn(context: *mut c_void, array: u64);
