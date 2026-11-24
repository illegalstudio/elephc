//! Purpose:
//! Registers stack-local exceptional owners used while native PHP callbacks run.
//!
//! Called from:
//! - Output handlers and mbregex callbacks when staging PHP argument owners.
//!
//! Key details:
//! - The shared exception activation protocol consumes owners only on an escaping throw.
//! - Normal cleanup detaches each guard before consuming its owner.

pub(crate) use elephc_memoir::guarded_owners::{
    emit_detach_in_frame as unguard,
    emit_publish_in_frame as guard,
};
