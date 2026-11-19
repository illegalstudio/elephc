//! Purpose:
//! Collects owned string and scratch allocation machinery.
//!
//! Called from:
//! - Compiler memory emission and its integration tests.
//!
//! Key details:
//! - Runs only in the compiler and preserves the generated target assembly.

pub mod str_persist;
pub mod str_new_owned;
pub mod concat_scratch;
pub mod concat_cursor;
pub mod strbuf;
pub mod scoped_stream;
pub mod str_append;

pub mod concat;
pub mod capture_buffer;

pub mod strcopy;

pub mod guarded_snapshot;
pub mod owner_pair;
pub mod byte_view;

pub mod concat_window;

pub mod export_scratch;
pub mod implode_buffer;
