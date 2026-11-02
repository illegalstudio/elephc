//! Purpose:
//! SPL classes the eval interpreter implements itself rather than through an eval declaration.
//!
//! Called from:
//! - `crate::interpreter::builtins` re-exports, reached by object construction, method dispatch
//!   and the `instanceof` answer.
//!
//! Key details:
//! - These classes appear in `interpreter/constants.rs`'s known-class list, which makes them
//!   NAMEABLE. A name with nothing behind it builds an object with no members at all, which is
//!   the state this module exists to end, one class at a time.

pub(in crate::interpreter) mod array_iterator;
