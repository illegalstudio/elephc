//! Purpose:
//! Public facade for elephc's pure-Rust PHAR archive bridge.
//! Keeps the C ABI and Rust archive API stable while focused modules own parsing,
//! writing, compression, signatures, stream state, and binary helpers.
//!
//! Called from:
//! - Compiled PHP program assembly through the exported `elephc_phar_*` symbols.
//! - `src/codegen/builtins/io/phar_stream.rs` for literal compile-time reads.
//!
//! Key details:
//! - Returned FFI pointers remain backed by the process-global extraction buffer.
//! - Existing archive families, compression, metadata, and signatures are preserved.

use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

mod archive_api;
mod archive_core;
mod binary;
mod compression;
mod ffi_archive;
mod ffi_stream;
mod model;
mod native;
mod signature;
mod streams;
mod tar_read;
mod tar_write;
mod zip_crypto;
mod zip_read;
mod zip_write;

#[allow(unused_imports)]
use archive_api::*;
#[allow(unused_imports)]
use archive_core::*;
#[allow(unused_imports)]
use binary::*;
#[allow(unused_imports)]
use compression::*;
#[allow(unused_imports)]
use ffi_archive::*;
#[allow(unused_imports)]
use ffi_stream::*;
#[allow(unused_imports)]
use model::*;
#[allow(unused_imports)]
use native::*;
#[allow(unused_imports)]
use signature::*;
#[allow(unused_imports)]
use streams::*;
#[allow(unused_imports)]
use tar_read::*;
#[allow(unused_imports)]
use tar_write::*;
#[allow(unused_imports)]
use zip_crypto::*;
#[allow(unused_imports)]
use zip_read::*;
#[allow(unused_imports)]
use zip_write::*;

pub use archive_api::{
    delete_entry_bytes, delete_url_bytes, entry_names_bytes, extract_entry_bytes,
    extract_url_bytes, put_entry_bytes, put_url_bytes, set_archive_compression,
    zip_extract_url_bytes, zip_stat_entries_bytes,
};
pub use ffi_archive::*;
pub use ffi_stream::*;

#[cfg(test)]
mod tests;
