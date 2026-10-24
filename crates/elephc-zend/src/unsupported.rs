//! Purpose:
//! The PHP runtime services a compiled program does not have: PHP streams and
//! stream filters, output buffering, sessions, PCRE, JSON encoding,
//! `var_dump()`.
//!
//! Called from:
//! - Extension code. Mostly MINIT registrations (a stream wrapper, a filter
//!   factory, an output handler, a session serializer) and the occasional
//!   operation on one.
//!
//! Key details:
//! - Three kinds of entry, deliberately different:
//!   - STATE an extension reads (no output buffer is active, no stream
//!     context carries options) answers truthfully;
//!   - REGISTRATIONS succeed and are dropped, because an extension that fails
//!     its MINIT is unusable even for the functions that never touch them
//!     (zstd registers `compress.zstd://`, and `zstd_compress()` needs no
//!     stream);
//!   - OPERATIONS stop the program with their name. Returning something
//!     plausible would be a silent miscompile; failing where the gap is keeps
//!     it visible.

use std::ffi::{c_char, c_int, c_void};
use std::ptr;

use crate::abi::*;
use crate::error::unsupported;

unsafe impl Sync for php_stream_ops {}
unsafe impl Sync for php_stream_wrapper {}

#[no_mangle]
pub static php_stream_stdio_ops: php_stream_ops = php_stream_ops {
    write: None,
    read: None,
    close: None,
    flush: None,
    label: c"STDIO".as_ptr(),
    seek: None,
    cast: None,
    stat: None,
    set_option: None,
};

#[no_mangle]
pub static php_plain_files_wrapper: php_stream_wrapper =
    php_stream_wrapper { wops: ptr::null(), r#abstract: ptr::null_mut(), is_url: 0 };

// ------------------------------------------------------------ streams: registrations and state

/// Registration stub: accepts a URL stream wrapper for `protocol` and drops it, so an extension's
/// MINIT succeeds; returns `SUCCESS`.
#[no_mangle]
pub unsafe extern "C" fn php_register_url_stream_wrapper(_protocol: *const c_char, _wrapper: *const php_stream_wrapper) -> zend_result {
    SUCCESS
}

/// Registration stub: pretends to unregister a stream wrapper by name; returns `SUCCESS`.
#[no_mangle]
pub unsafe extern "C" fn php_unregister_url_stream_wrapper(_protocol: *const c_char) -> zend_result {
    SUCCESS
}

/// Registration stub for the `zend_string`-named, request-scoped wrapper form; drops it and returns
/// `SUCCESS`.
#[no_mangle]
pub unsafe extern "C" fn php_register_url_stream_wrapper_volatile(_protocol: *mut zend_string, _wrapper: *mut php_stream_wrapper) -> zend_result {
    SUCCESS
}

/// Registration stub for unregistering a request-scoped wrapper; returns `SUCCESS`.
#[no_mangle]
pub unsafe extern "C" fn php_unregister_url_stream_wrapper_volatile(_protocol: *mut zend_string) -> zend_result {
    SUCCESS
}

/// Registration stub: accepts a stream filter factory for a pattern and drops it; returns
/// `SUCCESS`.
#[no_mangle]
pub unsafe extern "C" fn php_stream_filter_register_factory(_filterpattern: *const c_char, _factory: *const php_stream_filter_factory) -> zend_result {
    SUCCESS
}

/// Registration stub: pretends to unregister a filter factory; returns `SUCCESS`.
#[no_mangle]
pub unsafe extern "C" fn php_stream_filter_unregister_factory(_filterpattern: *const c_char) -> zend_result {
    SUCCESS
}

/// Registration stub for the `zend_string`-named, request-scoped filter factory form; returns
/// `SUCCESS`.
#[no_mangle]
pub unsafe extern "C" fn php_stream_filter_register_factory_volatile(
    _filterpattern: *mut zend_string,
    _factory: *const php_stream_filter_factory,
) -> zend_result {
    SUCCESS
}

/// No stream context ever carries options.
#[no_mangle]
pub unsafe extern "C" fn php_stream_context_get_option(
    _context: *const php_stream_context,
    _wrappername: *const c_char,
    _optionname: *const c_char,
) -> *mut zval {
    ptr::null_mut()
}

/// Always returns null: no stream context exists, whatever the zval holds.
#[no_mangle]
pub unsafe extern "C" fn php_stream_context_from_zval(_zcontext: *mut zval, _nocontext: bool) -> *mut php_stream_context {
    ptr::null_mut()
}

/// Resource type numbers no resource ever carries.
#[no_mangle]
pub extern "C" fn php_file_le_stream() -> c_int {
    -2
}

/// Returns the resource type number (-3) for persistent streams, which no resource ever carries.
#[no_mangle]
pub extern "C" fn php_file_le_pstream() -> c_int {
    -3
}

/// Returns the resource type number (-4) for stream filters, which no resource ever carries.
#[no_mangle]
pub extern "C" fn php_file_le_stream_filter() -> c_int {
    -4
}

// ------------------------------------------------------------ streams: operations

macro_rules! refuse {
    ($what:literal) => {
        unsupported($what)
    };
}

/// Unsupported operation: `zend_create_internal_iterator_zval`, the PHP object an internal
/// `getIterator()` method returns, stops the program with the API name. Elephc's wrappers never
/// call that method: `foreach` and `getIterator()` drive the class's `get_iterator` handler
/// directly, so an extension that uses it still links and iterates.
#[no_mangle]
pub unsafe extern "C" fn zend_create_internal_iterator_zval(_return_value: *mut zval, _obj: *mut zval) -> zend_result {
    refuse!("zend_create_internal_iterator_zval (a PHP-visible internal iterator object)")
}

/// Unsupported operation: allocating a PHP stream stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn _php_stream_alloc(_ops: *const php_stream_ops, _abstract: *mut c_void, _persistent_id: *const c_char, _mode: *const c_char) -> *mut php_stream {
    refuse!("_php_stream_alloc (PHP streams)")
}

/// Unsupported operation: opening a path through a stream wrapper stops the program with the API
/// name.
#[no_mangle]
pub unsafe extern "C" fn _php_stream_open_wrapper_ex(
    _path: *const c_char,
    _mode: *const c_char,
    _options: c_int,
    _opened_path: *mut *mut zend_string,
    _context: *mut php_stream_context,
) -> *mut php_stream {
    refuse!("_php_stream_open_wrapper_ex (PHP streams)")
}

/// Unsupported operation: freeing a PHP stream stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn _php_stream_free(_stream: *mut php_stream, _close_options: c_int) -> c_int {
    refuse!("_php_stream_free (PHP streams)")
}

/// Unsupported operation: reading from a PHP stream stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn _php_stream_read(_stream: *mut php_stream, _buf: *mut c_char, _count: usize) -> isize {
    refuse!("_php_stream_read (PHP streams)")
}

/// Unsupported operation: writing to a PHP stream stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn _php_stream_write(_stream: *mut php_stream, _buf: *const c_char, _count: usize) -> isize {
    refuse!("_php_stream_write (PHP streams)")
}

/// Unsupported operation: testing a PHP stream for EOF stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn _php_stream_eof(_stream: *mut php_stream) -> bool {
    refuse!("_php_stream_eof (PHP streams)")
}

/// Unsupported operation: reading a character from a PHP stream stops the program with the API
/// name.
#[no_mangle]
pub unsafe extern "C" fn _php_stream_getc(_stream: *mut php_stream) -> c_int {
    refuse!("_php_stream_getc (PHP streams)")
}

/// Unsupported operation: writing a character to a PHP stream stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn _php_stream_putc(_stream: *mut php_stream, _c: c_int) -> c_int {
    refuse!("_php_stream_putc (PHP streams)")
}

/// Unsupported operation: flushing a PHP stream stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn _php_stream_flush(_stream: *mut php_stream, _closing: c_int) -> c_int {
    refuse!("_php_stream_flush (PHP streams)")
}

/// Unsupported operation: seeking in a PHP stream stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn _php_stream_seek(_stream: *mut php_stream, _offset: zend_off_t, _whence: c_int) -> c_int {
    refuse!("_php_stream_seek (PHP streams)")
}

/// Unsupported operation: querying a PHP stream position stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn _php_stream_tell(_stream: *mut php_stream) -> zend_off_t {
    refuse!("_php_stream_tell (PHP streams)")
}

/// Unsupported operation: setting a PHP stream option stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn _php_stream_set_option(_stream: *mut php_stream, _option: c_int, _value: c_int, _ptrparam: *mut c_void) -> c_int {
    refuse!("_php_stream_set_option (PHP streams)")
}

/// Unsupported operation: reading a line from a PHP stream stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn _php_stream_get_line(_stream: *mut php_stream, _buf: *mut c_char, _maxlen: usize, _returned_len: *mut usize) -> *mut c_char {
    refuse!("_php_stream_get_line (PHP streams)")
}

/// Unsupported operation: reading a PHP stream into memory stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn _php_stream_copy_to_mem(_src: *mut php_stream, _maxlen: usize, _persistent: bool) -> *mut zend_string {
    refuse!("_php_stream_copy_to_mem (PHP streams)")
}

/// Unsupported operation: copying between PHP streams stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn _php_stream_copy_to_stream_ex(_src: *mut php_stream, _dest: *mut php_stream, _maxlen: usize, _len: *mut usize) -> zend_result {
    refuse!("_php_stream_copy_to_stream_ex (PHP streams)")
}

/// Unsupported operation: stat on a PHP stream stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn _php_stream_stat(_stream: *mut php_stream, _ssb: *mut php_stream_statbuf) -> c_int {
    refuse!("_php_stream_stat (PHP streams)")
}

/// Unsupported operation: casting a PHP stream to a file descriptor or `FILE*` stops the program
/// with the API name.
#[no_mangle]
pub unsafe extern "C" fn _php_stream_cast(_stream: *mut php_stream, _castas: c_int, _ret: *mut *mut c_void, _show_err: c_int) -> c_int {
    refuse!("_php_stream_cast (PHP streams)")
}

/// Unsupported operation: wrapping a file descriptor in a PHP stream stops the program with the API
/// name.
#[no_mangle]
pub unsafe extern "C" fn _php_stream_fopen_from_fd(_fd: c_int, _mode: *const c_char, _persistent_id: *const c_char, _zero_position: bool) -> *mut php_stream {
    refuse!("_php_stream_fopen_from_fd (PHP streams)")
}

/// Unsupported operation: wrapping a `FILE*` in a PHP stream stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn _php_stream_fopen_from_file(_file: *mut FILE, _mode: *const c_char) -> *mut php_stream {
    refuse!("_php_stream_fopen_from_file (PHP streams)")
}

/// Unsupported operation: creating a memory stream stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn _php_stream_memory_create(_mode: c_int) -> *mut php_stream {
    refuse!("_php_stream_memory_create (PHP streams)")
}

/// Unsupported operation: opening a memory stream over a string stops the program with the API
/// name.
#[no_mangle]
pub unsafe extern "C" fn _php_stream_memory_open(_mode: c_int, _buf: *mut zend_string) -> *mut php_stream {
    refuse!("_php_stream_memory_open (PHP streams)")
}

/// Unsupported operation: reading a memory stream's buffer stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn _php_stream_memory_get_buffer(_stream: *mut php_stream) -> *mut zend_string {
    refuse!("_php_stream_memory_get_buffer (PHP streams)")
}

/// Unsupported operation: creating a temp stream stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn _php_stream_temp_create(_mode: c_int, _max_memory_usage: usize) -> *mut php_stream {
    refuse!("_php_stream_temp_create (PHP streams)")
}

/// Unsupported operation: allocating a stream context stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn php_stream_context_alloc() -> *mut php_stream_context {
    refuse!("php_stream_context_alloc (PHP streams)")
}

/// Unsupported operation: allocating a stream filter stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn _php_stream_filter_alloc(_fops: *const php_stream_filter_ops, _abstract: *mut c_void, _persistent: u8) -> *mut php_stream_filter {
    refuse!("_php_stream_filter_alloc (PHP stream filters)")
}

/// Unsupported operation: instantiating a stream filter by name stops the program with the API
/// name.
#[no_mangle]
pub unsafe extern "C" fn php_stream_filter_create(_filtername: *const c_char, _filterparams: *mut zval, _persistent: u8) -> *mut php_stream_filter {
    refuse!("php_stream_filter_create (PHP stream filters)")
}

/// Unsupported operation: freeing a stream filter stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn php_stream_filter_free(_filter: *mut php_stream_filter) {
    refuse!("php_stream_filter_free (PHP stream filters)")
}

/// Unsupported operation: removing a filter from a stream stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn php_stream_filter_remove(_filter: *mut php_stream_filter, _call_dtor: bool) -> *mut php_stream_filter {
    refuse!("php_stream_filter_remove (PHP stream filters)")
}

/// Unsupported operation: appending a filter to a chain stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn _php_stream_filter_append(_chain: *mut c_void, _filter: *mut php_stream_filter) {
    refuse!("_php_stream_filter_append (PHP stream filters)")
}

/// Unsupported operation: prepending a filter to a chain stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn _php_stream_filter_prepend(_chain: *mut c_void, _filter: *mut php_stream_filter) {
    refuse!("_php_stream_filter_prepend (PHP stream filters)")
}

/// Unsupported operation: creating a filter bucket stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn php_stream_bucket_new(
    _stream: *const php_stream,
    _buf: *mut c_char,
    _buflen: usize,
    _own_buf: u8,
    _buf_persistent: u8,
) -> *mut php_stream_bucket {
    refuse!("php_stream_bucket_new (PHP stream filters)")
}

/// Unsupported operation: splitting a filter bucket stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn php_stream_bucket_split(
    _in: *mut php_stream_bucket,
    _left: *mut *mut php_stream_bucket,
    _right: *mut *mut php_stream_bucket,
    _length: usize,
) -> c_int {
    refuse!("php_stream_bucket_split (PHP stream filters)")
}

/// Unsupported operation: releasing a filter bucket stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn php_stream_bucket_delref(_bucket: *mut php_stream_bucket) {
    refuse!("php_stream_bucket_delref (PHP stream filters)")
}

/// Unsupported operation: prepending a bucket to a brigade stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn php_stream_bucket_prepend(_brigade: *mut php_stream_bucket_brigade, _bucket: *mut php_stream_bucket) {
    refuse!("php_stream_bucket_prepend (PHP stream filters)")
}

/// Unsupported operation: appending a bucket to a brigade stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn php_stream_bucket_append(_brigade: *mut php_stream_bucket_brigade, _bucket: *mut php_stream_bucket) {
    refuse!("php_stream_bucket_append (PHP stream filters)")
}

/// Unsupported operation: unlinking a bucket from its brigade stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn php_stream_bucket_unlink(_bucket: *mut php_stream_bucket) {
    refuse!("php_stream_bucket_unlink (PHP stream filters)")
}

/// Unsupported operation: making a bucket writable stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn php_stream_bucket_make_writeable(_bucket: *mut php_stream_bucket) -> *mut php_stream_bucket {
    refuse!("php_stream_bucket_make_writeable (PHP stream filters)")
}

// ------------------------------------------------------------ output buffering

/// No PHP output buffer is ever active in a compiled program.
#[no_mangle]
pub extern "C" fn php_output_get_level() -> c_int {
    0
}

/// Returns 0: no output-buffering status flags are ever set because no PHP output buffer is active.
#[no_mangle]
pub extern "C" fn php_output_get_status() -> c_int {
    0
}

/// Always false: no output handler is ever started.
#[no_mangle]
pub unsafe extern "C" fn php_output_handler_started(_name: *const c_char, _name_len: usize) -> bool {
    false
}

/// Always false: with no handlers running there is no conflict to report.
#[no_mangle]
pub unsafe extern "C" fn php_output_handler_conflict(
    _handler_new: *const c_char,
    _handler_new_len: usize,
    _handler_set: *const c_char,
    _handler_set_len: usize,
) -> bool {
    false
}

/// Registration stub: accepts a handler conflict checker and drops it; returns `SUCCESS`.
#[no_mangle]
pub unsafe extern "C" fn php_output_handler_conflict_register(
    _handler_name: *const c_char,
    _handler_name_len: usize,
    _check_func: php_output_handler_conflict_check_t,
) -> zend_result {
    SUCCESS
}

/// Registration stub: accepts an output handler alias constructor and drops it; returns `SUCCESS`.
#[no_mangle]
pub unsafe extern "C" fn php_output_handler_alias_register(
    _handler_name: *const c_char,
    _handler_name_len: usize,
    _func: php_output_handler_alias_ctor_t,
) -> zend_result {
    SUCCESS
}

/// Unsupported operation: creating an output handler stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn php_output_handler_create_internal(
    _name: *const c_char,
    _name_len: usize,
    _handler: php_output_handler_context_func_t,
    _chunk_size: usize,
    _flags: c_int,
) -> *mut php_output_handler {
    refuse!("php_output_handler_create_internal (PHP output buffering)")
}

/// Unsupported operation: attaching a context to an output handler stops the program with the API
/// name.
#[no_mangle]
pub unsafe extern "C" fn php_output_handler_set_context(
    _handler: *mut php_output_handler,
    _opaq: *mut c_void,
    _dtor: Option<unsafe extern "C" fn(*mut c_void)>,
) {
    refuse!("php_output_handler_set_context (PHP output buffering)")
}

/// Unsupported operation: starting an output handler stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn php_output_handler_start(_handler: *mut php_output_handler) -> zend_result {
    refuse!("php_output_handler_start (PHP output buffering)")
}

/// Unsupported operation: output handler hooks stop the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn php_output_handler_hook(_type: php_output_handler_hook_t, _arg: *mut c_void) -> zend_result {
    refuse!("php_output_handler_hook (PHP output buffering)")
}

/// Unsupported operation: starting the default output buffer stops the program with the API name.
#[no_mangle]
pub extern "C" fn php_output_start_default() -> zend_result {
    refuse!("php_output_start_default (PHP output buffering)")
}

/// Always `FAILURE`: there is no active output buffer to end.
#[no_mangle]
pub extern "C" fn php_output_end() -> zend_result {
    FAILURE
}

/// Always `FAILURE`: there is no active output buffer to discard.
#[no_mangle]
pub extern "C" fn php_output_discard() -> zend_result {
    FAILURE
}

/// Always `FAILURE`: there is no output buffer whose contents could be read.
#[no_mangle]
pub unsafe extern "C" fn php_output_get_contents(_p: *mut zval) -> zend_result {
    FAILURE
}

// ------------------------------------------------------------ sessions

/// A serializer registration is accepted; no session ever uses it.
#[no_mangle]
pub unsafe extern "C" fn php_session_register_serializer(
    _name: *const c_char,
    _encode: Option<unsafe extern "C" fn() -> *mut zend_string>,
    _decode: Option<unsafe extern "C" fn(*const c_char, usize) -> zend_result>,
) -> zend_result {
    SUCCESS
}

/// Unsupported operation: adding a session variable stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn php_add_session_var(_name: *mut zend_string) {
    refuse!("php_add_session_var (PHP sessions)")
}

/// Unsupported operation: setting a session variable stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn php_set_session_var(_name: *mut zend_string, _state_val: *mut zval, _var_hash: *mut php_unserialize_data_t) -> *mut zval {
    refuse!("php_set_session_var (PHP sessions)")
}

/// Always returns null: there is no session, so no session variable exists.
#[no_mangle]
pub unsafe extern "C" fn php_get_session_var(_name: *mut zend_string) -> *mut zval {
    ptr::null_mut()
}

/// C-string-name form of `php_get_session_var`; always null.
#[no_mangle]
pub unsafe extern "C" fn php_get_session_var_str(_name: *const c_char, _name_len: usize) -> *mut zval {
    ptr::null_mut()
}

// ------------------------------------------------------------ PCRE, iconv, JSON, var_dump()

/// Unsupported operation: compiling a PCRE pattern stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn pcre_get_compiled_regex_cache(_regex: *mut zend_string) -> *mut pcre_cache_entry {
    refuse!("pcre_get_compiled_regex_cache (PHP regular expressions)")
}

/// Unsupported operation: compiling a (possibly locale-aware) PCRE pattern stops the program with
/// the API name.
#[no_mangle]
pub unsafe extern "C" fn pcre_get_compiled_regex_cache_ex(_regex: *mut zend_string, _locale_aware: bool) -> *mut pcre_cache_entry {
    refuse!("pcre_get_compiled_regex_cache_ex (PHP regular expressions)")
}

/// Unsupported operation: reading a compiled PCRE code object stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn php_pcre_pce_re(_pce: *mut pcre_cache_entry) -> *mut pcre2_code {
    refuse!("php_pcre_pce_re (PHP regular expressions)")
}

/// Unsupported operation: fetching the PCRE match context stops the program with the API name.
#[no_mangle]
pub extern "C" fn php_pcre_mctx() -> *mut pcre2_match_context {
    refuse!("php_pcre_mctx (PHP regular expressions)")
}

/// Unsupported operation: fetching the PCRE general context stops the program with the API name.
#[no_mangle]
pub extern "C" fn php_pcre_gctx() -> *mut pcre2_general_context {
    refuse!("php_pcre_gctx (PHP regular expressions)")
}

/// No-op: no PCRE cache entry can exist, so there is no count to raise.
#[no_mangle]
pub unsafe extern "C" fn php_pcre_pce_incref(_pce: *mut pcre_cache_entry) {}

/// No-op: no PCRE cache entry can exist, so there is no count to lower.
#[no_mangle]
pub unsafe extern "C" fn php_pcre_pce_decref(_pce: *mut pcre_cache_entry) {}

/// Unsupported operation: running a PCRE match stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn php_pcre_match_impl(
    _pce: *mut pcre_cache_entry,
    _subject_str: *mut zend_string,
    _return_value: *mut zval,
    _subpats: *mut zval,
    _global: c_int,
    _flags: zend_long,
    _start_offset: zend_off_t,
) {
    refuse!("php_pcre_match_impl (PHP regular expressions)")
}

/// Unsupported operation: a PCRE replace stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn php_pcre_replace(
    _regex: *mut zend_string,
    _subject_str: *mut zend_string,
    _subject: *const c_char,
    _subject_len: usize,
    _replace_str: *mut zend_string,
    _limit: usize,
    _replace_count: *mut usize,
) -> *mut zend_string {
    refuse!("php_pcre_replace (PHP regular expressions)")
}

/// Unsupported operation: calling `pcre2_match` stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn pcre2_match_8(
    _code: *const pcre2_code,
    _subject: PCRE2_SPTR,
    _length: PCRE2_SIZE,
    _start: PCRE2_SIZE,
    _options: u32,
    _match_data: *mut pcre2_match_data,
    _mcontext: *mut pcre2_match_context,
) -> c_int {
    refuse!("pcre2_match (PHP regular expressions)")
}

/// Unsupported operation: creating PCRE2 match data stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn pcre2_match_data_create_from_pattern_8(
    _code: *const pcre2_code,
    _gcontext: *mut pcre2_general_context,
) -> *mut pcre2_match_data {
    refuse!("pcre2_match_data_create_from_pattern (PHP regular expressions)")
}

/// Freeing is reachable from cleanup paths that never created match data.
#[no_mangle]
pub unsafe extern "C" fn pcre2_match_data_free_8(_match_data: *mut pcre2_match_data) {}

/// Unsupported operation: reading the PCRE2 offset vector stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn pcre2_get_ovector_pointer_8(_match_data: *mut pcre2_match_data) -> *mut PCRE2_SIZE {
    refuse!("pcre2_get_ovector_pointer (PHP regular expressions)")
}


/// Unsupported operation: JSON-encoding a value stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn php_json_encode(_buf: *mut smart_str, _val: *mut zval, _options: c_int) -> zend_result {
    refuse!("php_json_encode (PHP JSON)")
}

/// Unsupported operation: JSON-decoding a string stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn php_json_decode_ex(
    _return_value: *mut zval,
    _str: *const c_char,
    _str_len: usize,
    _options: zend_long,
    _depth: zend_long,
) -> zend_result {
    refuse!("php_json_decode_ex (PHP JSON)")
}

/// Unsupported operation: `var_dump()` of a value stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn php_var_dump(_struc: *mut zval, _level: c_int) {
    refuse!("php_var_dump (PHP var_dump())")
}

/// Unsupported operation: `var_export()` to output stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn php_var_export(_struc: *mut zval, _level: c_int) {
    refuse!("php_var_export (PHP var_export())")
}

/// Unsupported operation: `var_export()` into a `smart_str` stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn php_var_export_ex(_struc: *mut zval, _level: c_int, _buf: *mut smart_str) {
    refuse!("php_var_export_ex (PHP var_export())")
}

/// Unsupported operation: `debug_zval_dump()` stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn php_debug_zval_dump(_struc: *mut zval, _level: c_int) {
    refuse!("php_debug_zval_dump (PHP debug_zval_dump())")
}

/// Unsupported operation: `unserialize()` stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn php_unserialize_with_options(
    _return_value: *mut zval,
    _buf: *const c_char,
    _buf_len: usize,
    _options: *mut HashTable,
    _function_name: *const c_char,
) {
    refuse!("php_unserialize_with_options (PHP unserialize())")
}

/// Unsupported operation: turning an object into a lazy object stops the program with the API name.
#[no_mangle]
pub unsafe extern "C" fn zend_object_make_lazy(
    _obj: *mut zend_object,
    _class_type: *mut zend_class_entry,
    _initializer_zv: *mut zval,
    _initializer_fcc: *mut zend_fcall_info_cache,
    _flags: u32,
) -> *mut zend_object {
    refuse!("zend_object_make_lazy (PHP lazy objects)")
}

/// Always true: a compiled program behaves as the CLI SAPI.
#[no_mangle]
pub extern "C" fn sapi_cli_is_cli() -> bool {
    true
}
