//! Purpose:
//! Groups the I/O integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for printing, files, streams, filesystem, misc, and related suites.

use crate::support::*;

#[path = "io/printing.rs"]
mod printing;
#[path = "io/output_buffering.rs"]
mod output_buffering;
#[path = "io/files.rs"]
mod files;
#[path = "io/streams.rs"]
pub(crate) mod streams;
#[path = "io/compress_wrapper.rs"]
mod compress_wrapper;
#[path = "io/gz_streams.rs"]
mod gz_streams;
#[path = "io/wrapper_read_buffer.rs"]
mod wrapper_read_buffer;
#[path = "io/wrapper_without_stream_eof.rs"]
mod wrapper_without_stream_eof;
#[path = "io/wrapper_stat_cache.rs"]
mod wrapper_stat_cache;
#[path = "io/wrapper_eof_conversation.rs"]
mod wrapper_eof_conversation;
#[path = "io/wrapper_chunk_reads.rs"]
mod wrapper_chunk_reads;
#[path = "io/read_buffer.rs"]
mod read_buffer;
#[path = "io/line_reads.rs"]
mod line_reads;
#[path = "io/temp_stream_eof.rs"]
mod temp_stream_eof;
#[path = "io/file_url_scheme.rs"]
mod file_url_scheme;
#[path = "io/path_failure_warnings.rs"]
mod path_failure_warnings;
#[path = "io/copy_dynamic_filter.rs"]
mod copy_dynamic_filter;
#[path = "io/null_stream_argument.rs"]
mod null_stream_argument;
#[path = "io/resource_id_numbering.rs"]
mod resource_id_numbering;
#[path = "io/directory_as_a_file.rs"]
mod directory_as_a_file;
#[path = "io/zlib_string_functions.rs"]
mod zlib_string_functions;
#[path = "io/filesystem.rs"]
mod filesystem;
#[path = "io/misc.rs"]
mod misc;
#[path = "io/stat_ext.rs"]
mod stat_ext;
#[path = "io/paths/mod.rs"]
mod paths;
#[path = "io/modify.rs"]
mod modify;
#[path = "io/streams_ext.rs"]
mod streams_ext;
#[path = "io/stream_context_propagation.rs"]
mod stream_context_propagation;
#[path = "io/symlinks.rs"]
mod symlinks;
