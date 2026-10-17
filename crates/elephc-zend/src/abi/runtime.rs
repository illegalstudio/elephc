//! Purpose:
//! Structures extensions take from PHP's main/ directory and its bundled
//! extensions: the process globals (`PG`, `SG`, `BG`, `FG`, `PS`), digest
//! contexts, hash algorithm descriptors, and the stream, output and session
//! shapes extensions register callbacks into.
//!
//! Called from:
//! - `crate::runtime` modules, and `build.rs`, which emits each struct as C.
//!
//! Key details:
//! - A compiled program has no PHP streams, output buffers or sessions. These
//!   structures exist so an extension that implements a stream wrapper or an
//!   output handler compiles and registers it; using one is refused at run
//!   time by the function reached (`crate::runtime::unsupported`).

#![allow(non_camel_case_types, non_snake_case)]

use std::ffi::{c_char, c_int, c_uchar, c_void};

use super::engine::*;
use super::scalars::*;
use super::values::*;

// ------------------------------------------------------------ process globals

/// `PG(...)`, php-cli's defaults.
#[repr(C)]
pub struct php_core_globals {
    pub serialize_precision: zend_long,
    pub memory_limit: zend_long,
    pub max_input_time: zend_long,
    pub display_errors: bool,
    pub display_startup_errors: bool,
    pub log_errors: bool,
    pub html_errors: bool,
    pub during_request_startup: bool,
    pub modules_activated: bool,
    pub file_uploads: bool,
    pub report_memleaks: bool,
    pub ignore_user_abort: bool,
    pub enable_dl: bool,
    pub implicit_flush: bool,
    pub unserialize_callback_func: *mut c_char,
    pub open_basedir: *mut c_char,
    pub error_log: *mut c_char,
    pub doc_root: *mut c_char,
    pub user_dir: *mut c_char,
    pub include_path: *mut c_char,
    pub extension_dir: *mut c_char,
    pub upload_tmp_dir: *mut c_char,
    pub sys_temp_dir: *mut c_char,
    pub http_globals: [zval; 6],
    pub max_input_nesting_level: zend_long,
    pub max_input_vars: zend_long,
    pub reserved: [*mut c_void; 8],
}

#[repr(C)]
pub struct sapi_header_struct {
    pub header: *mut c_char,
    pub header_len: usize,
}

#[repr(C)]
pub struct sapi_headers_struct {
    pub headers: zend_llist,
    pub http_response_code: c_int,
    pub send_default_content_type: c_uchar,
    pub mimetype: *mut c_char,
    pub http_status_line: *mut c_char,
}

#[repr(C)]
pub struct sapi_request_info {
    pub request_method: *const c_char,
    pub query_string: *mut c_char,
    pub cookie_data: *mut c_char,
    pub content_length: zend_long,
    pub path_translated: *mut c_char,
    pub request_uri: *mut c_char,
    pub content_type: *const c_char,
    pub headers_only: bool,
    pub no_headers: bool,
    pub headers_read: bool,
    pub argc: c_int,
    pub argv: *mut *mut c_char,
    pub proto_num: c_int,
}

/// `SG(...)`: the SAPI a hosted extension sees is the CLI one.
#[repr(C)]
pub struct sapi_globals_struct {
    pub server_context: *mut c_void,
    pub request_info: sapi_request_info,
    pub sapi_headers: sapi_headers_struct,
    pub read_post_bytes: i64,
    pub post_read: c_uchar,
    pub headers_sent: c_uchar,
    pub global_request_time: f64,
    pub reserved: [*mut c_void; 8],
}

#[repr(C)]
pub struct sapi_module_struct {
    pub name: *mut c_char,
    pub pretty_name: *mut c_char,
    pub reserved: [*mut c_void; 16],
}

#[repr(C)]
pub struct php_basic_globals_level {
    pub data: *mut c_void,
    pub level: u32,
}

/// `BG(...)`: ext/standard's state; the serializer nesting is what
/// extensions touch.
#[repr(C)]
pub struct php_basic_globals {
    pub serialize_lock: u32,
    pub serialize: php_basic_globals_level,
    pub unserialize: php_basic_globals_level,
    pub unserialize_max_depth: zend_long,
    pub unserialize_cur_depth: zend_long,
    pub ctype_string: *mut zend_string,
    pub locale_changed: bool,
    pub reserved: [*mut c_void; 8],
}

#[repr(C)]
pub struct php_file_globals {
    pub pclose_ret: c_int,
    pub def_chunk_size: usize,
    pub auto_detect_line_endings: bool,
    pub default_socket_timeout: zend_long,
    pub user_agent: *mut c_char,
    pub from_address: *mut c_char,
    pub user_stream_current_filename: *const c_char,
    pub default_context: *mut c_void,
    pub stream_wrappers: *mut HashTable,
    pub stream_filters: *mut HashTable,
    pub wrapper_errors: *mut HashTable,
    pub pclose_wait: c_int,
}

// ------------------------------------------------------------ serialize

/// State of one `serialize()` run: objects already written, for back-references.
/// c: tag php_serialize_data
/// c: opaque
#[repr(C)]
pub struct php_serialize_data {
    _private: [u8; 0],
}
/// c: tag php_unserialize_data
/// c: opaque
#[repr(C)]
pub struct php_unserialize_data {
    _private: [u8; 0],
}
pub type php_serialize_data_t = *mut php_serialize_data;
pub type php_unserialize_data_t = *mut php_unserialize_data;

// ------------------------------------------------------------ digests

#[repr(C)]
pub struct PHP_MD5_CTX {
    pub state: [u32; 4],
    pub count: u64,
    pub buffer: [c_uchar; 64],
}

#[repr(C)]
pub struct PHP_SHA1_CTX {
    pub state: [u32; 5],
    pub count: u64,
    pub buffer: [c_uchar; 64],
}

#[repr(C)]
pub struct PHP_SHA256_CTX {
    pub state: [u32; 8],
    pub count: u64,
    pub buffer: [c_uchar; 64],
}
pub type PHP_SHA224_CTX = PHP_SHA256_CTX;

/// c: opaque
#[repr(C)]
pub struct php_hashcontext_object {
    _private: [u8; 0],
}

/// A hash algorithm, as ext/hash describes one.
#[repr(C)]
pub struct php_hash_ops {
    pub algo: *const c_char,
    pub hash_init: Option<unsafe extern "C" fn(context: *mut c_void, args: *mut HashTable)>,
    pub hash_update: Option<unsafe extern "C" fn(context: *mut c_void, buf: *const c_uchar, count: usize)>,
    pub hash_final: Option<unsafe extern "C" fn(digest: *mut c_uchar, context: *mut c_void)>,
    pub hash_copy:
        Option<unsafe extern "C" fn(ops: *const c_void, orig_context: *const c_void, dest_context: *mut c_void) -> zend_result>,
    pub hash_serialize:
        Option<unsafe extern "C" fn(hash: *const php_hashcontext_object, magic: *mut zend_long, zv: *mut zval) -> c_int>,
    pub hash_unserialize:
        Option<unsafe extern "C" fn(hash: *mut php_hashcontext_object, magic: zend_long, zv: *const zval) -> c_int>,
    pub serialize_spec: *const c_char,
    pub digest_size: usize,
    pub block_size: usize,
    pub context_size: usize,
    pub is_crypto: u32,
}

// ------------------------------------------------------------ PCRE (declarations only)

/// c: opaque
#[repr(C)]
pub struct pcre_cache_entry {
    _private: [u8; 0],
}
/// c: tag pcre2_real_code_8
/// c: opaque
#[repr(C)]
pub struct pcre2_code {
    _private: [u8; 0],
}
/// c: tag pcre2_real_match_data_8
/// c: opaque
#[repr(C)]
pub struct pcre2_match_data {
    _private: [u8; 0],
}
/// c: tag pcre2_real_match_context_8
/// c: opaque
#[repr(C)]
pub struct pcre2_match_context {
    _private: [u8; 0],
}
/// c: tag pcre2_real_general_context_8
/// c: opaque
#[repr(C)]
pub struct pcre2_general_context {
    _private: [u8; 0],
}
pub type PCRE2_SPTR = *const c_uchar;
pub type PCRE2_SIZE = usize;

// ------------------------------------------------------------ streams

/// c: opaque
#[repr(C)]
pub struct php_stream_context {
    _private: [u8; 0],
}
/// c: opaque
#[repr(C)]
pub struct php_stream_notifier {
    _private: [u8; 0],
}

/// `ssb->sb` is the platform's `struct stat`; Rust only ever holds a pointer
/// to one, so its own view of this struct stops short.
#[repr(C)]
pub struct php_stream_statbuf {
    pub sb: stat,
}

#[repr(C)]
pub struct php_stream_dirent {
    pub d_name: [c_char; 1024],
    pub d_type: c_uchar,
}

#[repr(C)]
pub struct php_stream_ops {
    pub write: Option<unsafe extern "C" fn(stream: *mut php_stream, buf: *const c_char, count: usize) -> isize>,
    pub read: Option<unsafe extern "C" fn(stream: *mut php_stream, buf: *mut c_char, count: usize) -> isize>,
    pub close: Option<unsafe extern "C" fn(stream: *mut php_stream, close_handle: c_int) -> c_int>,
    pub flush: Option<unsafe extern "C" fn(stream: *mut php_stream) -> c_int>,
    pub label: *const c_char,
    pub seek: Option<
        unsafe extern "C" fn(stream: *mut php_stream, offset: zend_off_t, whence: c_int, newoffset: *mut zend_off_t) -> c_int,
    >,
    pub cast: Option<unsafe extern "C" fn(stream: *mut php_stream, castas: c_int, ret: *mut *mut c_void) -> c_int>,
    pub stat: Option<unsafe extern "C" fn(stream: *mut php_stream, ssb: *mut php_stream_statbuf) -> c_int>,
    pub set_option:
        Option<unsafe extern "C" fn(stream: *mut php_stream, option: c_int, value: c_int, ptrparam: *mut c_void) -> c_int>,
}

pub type php_stream_opener_t = Option<
    unsafe extern "C" fn(
        wrapper: *mut php_stream_wrapper,
        filename: *const c_char,
        mode: *const c_char,
        options: c_int,
        opened_path: *mut *mut zend_string,
        context: *mut php_stream_context,
    ) -> *mut php_stream,
>;

#[repr(C)]
pub struct php_stream_wrapper_ops {
    pub stream_opener: php_stream_opener_t,
    pub stream_closer: Option<unsafe extern "C" fn(wrapper: *mut php_stream_wrapper, stream: *mut php_stream) -> c_int>,
    pub stream_stat: Option<
        unsafe extern "C" fn(wrapper: *mut php_stream_wrapper, stream: *mut php_stream, ssb: *mut php_stream_statbuf) -> c_int,
    >,
    pub url_stat: Option<
        unsafe extern "C" fn(
            wrapper: *mut php_stream_wrapper,
            url: *const c_char,
            flags: c_int,
            ssb: *mut php_stream_statbuf,
            context: *mut php_stream_context,
        ) -> c_int,
    >,
    pub dir_opener: php_stream_opener_t,
    pub label: *const c_char,
    pub unlink: Option<
        unsafe extern "C" fn(
            wrapper: *mut php_stream_wrapper,
            url: *const c_char,
            options: c_int,
            context: *mut php_stream_context,
        ) -> c_int,
    >,
    pub rename: Option<
        unsafe extern "C" fn(
            wrapper: *mut php_stream_wrapper,
            url_from: *const c_char,
            url_to: *const c_char,
            options: c_int,
            context: *mut php_stream_context,
        ) -> c_int,
    >,
    pub stream_mkdir: Option<
        unsafe extern "C" fn(
            wrapper: *mut php_stream_wrapper,
            url: *const c_char,
            mode: c_int,
            options: c_int,
            context: *mut php_stream_context,
        ) -> c_int,
    >,
    pub stream_rmdir: Option<
        unsafe extern "C" fn(
            wrapper: *mut php_stream_wrapper,
            url: *const c_char,
            options: c_int,
            context: *mut php_stream_context,
        ) -> c_int,
    >,
    pub stream_metadata: Option<
        unsafe extern "C" fn(
            wrapper: *mut php_stream_wrapper,
            url: *const c_char,
            options: c_int,
            value: *mut c_void,
            context: *mut php_stream_context,
        ) -> c_int,
    >,
}

#[repr(C)]
pub struct php_stream_wrapper {
    pub wops: *const php_stream_wrapper_ops,
    pub r#abstract: *mut c_void,
    pub is_url: c_int,
}

#[repr(C)]
pub struct php_stream {
    pub ops: *const php_stream_ops,
    pub r#abstract: *mut c_void,
    pub readfilters_head: *mut php_stream_filter,
    pub writefilters_head: *mut php_stream_filter,
    pub wrapper: *mut php_stream_wrapper,
    pub wrapperthis: *mut c_void,
    pub wrapperdata: zval,
    pub is_persistent: u8,
    pub in_free: u8,
    pub eof: u8,
    pub flags: u8,
    pub res: *mut zend_resource,
    pub mode: [c_char; 16],
    pub flags2: u32,
    pub orig_path: *mut zend_string,
    pub ctx: *mut zend_resource,
    pub fclose_stdiocast: c_int,
    pub readbuf: *mut c_char,
    pub readbuflen: usize,
    pub readpos: zend_off_t,
    pub writepos: zend_off_t,
    pub position: zend_off_t,
    pub chunk_size: usize,
    pub __exposed: u8,
}

#[repr(C)]
pub struct php_stream_filter_ops {
    pub filter: Option<
        unsafe extern "C" fn(
            stream: *mut php_stream,
            thisfilter: *mut php_stream_filter,
            buckets_in: *mut php_stream_bucket_brigade,
            buckets_out: *mut php_stream_bucket_brigade,
            bytes_consumed: *mut usize,
            flags: c_int,
        ) -> php_stream_filter_status_t,
    >,
    pub dtor: Option<unsafe extern "C" fn(thisfilter: *mut php_stream_filter)>,
    pub label: *const c_char,
}

#[repr(C)]
pub struct php_stream_filter {
    pub fops: *const php_stream_filter_ops,
    pub r#abstract: zval,
    pub next: *mut php_stream_filter,
    pub prev: *mut php_stream_filter,
    pub is_persistent: c_int,
    pub chain: *mut c_void,
    pub buffer_dummy: *mut php_stream_bucket_brigade,
    pub stream: *mut php_stream,
    pub res: *mut zend_resource,
}

#[repr(C)]
pub struct php_stream_bucket {
    pub next: *mut php_stream_bucket,
    pub prev: *mut php_stream_bucket,
    pub brigade: *mut php_stream_bucket_brigade,
    pub buf: *mut c_char,
    pub buflen: usize,
    pub own_buf: u8,
    pub is_persistent: u8,
    pub refcount: c_int,
}

#[repr(C)]
pub struct php_stream_bucket_brigade {
    pub head: *mut php_stream_bucket,
    pub tail: *mut php_stream_bucket,
}

#[repr(C)]
pub struct php_stream_filter_factory {
    pub create_filter: Option<
        unsafe extern "C" fn(filtername: *const c_char, filterparams: *mut zval, persistent: u8) -> *mut php_stream_filter,
    >,
}

// ------------------------------------------------------------ output

#[repr(C)]
pub struct php_output_buffer {
    pub data: *mut c_char,
    pub size: usize,
    pub used: usize,
    pub free: u32,
}

#[repr(C)]
pub struct php_output_context {
    pub op: c_int,
    pub r#in: php_output_buffer,
    pub out: php_output_buffer,
}

/// c: opaque
#[repr(C)]
pub struct php_output_handler {
    _private: [u8; 0],
}

pub type php_output_handler_context_func_t =
    Option<unsafe extern "C" fn(handler_context: *mut *mut c_void, output_context: *mut php_output_context) -> zend_result>;
pub type php_output_handler_conflict_check_t =
    Option<unsafe extern "C" fn(handler_name: *const c_char, handler_name_len: usize) -> zend_result>;
pub type php_output_handler_alias_ctor_t = Option<
    unsafe extern "C" fn(
        handler_name: *const c_char,
        handler_name_len: usize,
        chunk_size: usize,
        flags: c_int,
    ) -> *mut php_output_handler,
>;

// ------------------------------------------------------------ session

#[repr(C)]
pub struct ps_serializer {
    pub name: *const c_char,
    pub encode: Option<unsafe extern "C" fn() -> *mut zend_string>,
    pub decode: Option<unsafe extern "C" fn(val: *const c_char, vallen: usize) -> zend_result>,
}

/// `PS(...)`: there is never an active session.
#[repr(C)]
pub struct php_ps_globals {
    pub http_session_vars: zval,
    pub session_status: c_int,
    pub reserved: [*mut c_void; 8],
}

pub type spl_iterator_apply_func_t =
    Option<unsafe extern "C" fn(iter: *mut zend_object_iterator, puser: *mut c_void) -> c_int>;
