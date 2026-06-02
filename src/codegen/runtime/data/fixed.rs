//! Purpose:
//! Builds the cacheable fixed runtime data section as assembly text.
//! This owns heap globals, shared scratch buffers, fatal messages, lookup tables, and fixed runtime state.
//!
//! Called from:
//! - `crate::codegen::runtime::data::emit_runtime_data_fixed()`.
//!
//! Key details:
//! - Fixed symbols are cached across compilations, so only target-independent runtime data belongs here.

use super::{
    DIRNAME_LEVELS_MSG, PHP_UNAME_MODE_LEN_MSG, PHP_UNAME_MODE_VALUE_MSG,
    STR_REPEAT_TIMES_MSG,
};
use super::super::system;
use crate::types::checker::builtins::supported_builtin_function_names;

/// Emit the fixed runtime `.data` section as assembly text.
/// Cached across compilations because it contains only target-independent
/// runtime data: heap globals, concat buffers, exception/fiber state,
/// JSON/SPL error messages, base64 tables, PCRE regex patterns, and
/// lookup tables for builtins, file types, and `pathinfo` keys.
///
/// `heap_size` is the maximum heap bytes requested by the user program;
/// it is baked into `_heap_max` to enforce the heap limit at runtime.
pub(crate) fn emit_runtime_data_fixed(heap_size: usize) -> String {
    let mut out = String::new();
    out.push_str(".data\n");
    out.push_str(".comm _concat_buf, 65536, 3\n");
    out.push_str(".comm _concat_off, 8, 3\n");
    out.push_str(".comm _global_argc, 8, 3\n");
    out.push_str(".comm _global_argv, 8, 3\n");
    out.push_str(".comm _exc_handler_top, 8, 3\n");
    out.push_str(".comm _exc_call_frame_top, 8, 3\n");
    out.push_str(".comm _exc_value, 8, 3\n");
    out.push_str(".comm _fiber_current, 8, 3\n");
    out.push_str(".comm _fiber_main_saved_sp, 8, 3\n");
    out.push_str(".comm _fiber_main_saved_exc, 8, 3\n");
    out.push_str(".comm _fiber_main_saved_call_frame, 8, 3\n");
    out.push_str(".comm _rt_diag_suppression, 8, 3\n");
    out.push_str(&format!(".comm _heap_buf, {}, 3\n", heap_size));
    out.push_str(".comm _heap_off, 8, 3\n");
    out.push_str(".comm _heap_free_list, 8, 3\n");
    out.push_str(".comm _heap_small_bins, 32, 3\n");
    out.push_str(".comm _heap_debug_enabled, 8, 3\n");
    out.push_str(".comm _gc_collecting, 8, 3\n");
    out.push_str(".comm _gc_release_suppressed, 8, 3\n");
    out.push_str(".comm _json_last_error, 8, 3\n");
    out.push_str(".comm _json_active_flags, 8, 3\n");
    out.push_str(".comm _json_active_depth, 8, 3\n");
    out.push_str(".comm _json_indent_depth, 8, 3\n");
    out.push_str(".comm _json_depth_limit, 8, 3\n");
    out.push_str(".comm _json_validate_idx, 8, 3\n");
    out.push_str(".comm _json_validate_ptr, 8, 3\n");
    out.push_str(".comm _json_validate_len, 8, 3\n");
    out.push_str(".comm _json_decode_assoc, 8, 3\n");
    out.push_str(".comm _json_error_source_ptr, 8, 3\n");
    out.push_str(".comm _json_error_location_active, 8, 3\n");
    out.push_str(".comm _json_error_line, 8, 3\n");
    out.push_str(".comm _json_error_column, 8, 3\n");
    out.push_str(&format!(".globl _heap_max\n_heap_max:\n    .quad {}\n", heap_size));
    out.push_str(".globl _heap_err_msg\n_heap_err_msg:\n    .ascii \"Fatal error: heap memory exhausted\\n\"\n");
    out.push_str(".globl _heap_dbg_bad_refcount_msg\n_heap_dbg_bad_refcount_msg:\n    .ascii \"Fatal error: heap debug detected bad refcount\\n\"\n");
    out.push_str(".globl _heap_dbg_double_free_msg\n_heap_dbg_double_free_msg:\n    .ascii \"Fatal error: heap debug detected double free\\n\"\n");
    out.push_str(".globl _heap_dbg_free_list_msg\n_heap_dbg_free_list_msg:\n    .ascii \"Fatal error: heap debug detected free-list corruption\\n\"\n");
    out.push_str(".globl _arr_cap_err_msg\n_arr_cap_err_msg:\n    .ascii \"Fatal error: array capacity exceeded\\n\"\n");
    out.push_str(".globl _buffer_bounds_msg\n_buffer_bounds_msg:\n    .ascii \"Fatal error: buffer index out of bounds\\n\"\n");
    out.push_str(".globl _buffer_uaf_msg\n_buffer_uaf_msg:\n    .ascii \"Fatal error: use of buffer after buffer_free()\\n\"\n");
    out.push_str(".globl _iterable_unsupported_kind_msg\n_iterable_unsupported_kind_msg:\n    .ascii \"Fatal error: foreach over iterable with unsupported kind\\n\"\n");
    out.push_str(".globl _iterable_array_str\n_iterable_array_str:\n    .ascii \"Array\"\n");
    out.push_str(".globl _match_unhandled_msg\n_match_unhandled_msg:\n    .ascii \"Fatal error: unhandled match case\\n\"\n");
    out.push_str(".globl _static_prop_private_access_msg\n_static_prop_private_access_msg:\n    .ascii \"Fatal error: Cannot access private static property\\n\"\n");
    out.push_str(".globl _ptr_null_err_msg\n_ptr_null_err_msg:\n    .ascii \"Fatal error: null pointer dereference\\n\"\n");
    out.push_str(".globl _ptr_read_string_len_err_msg\n_ptr_read_string_len_err_msg:\n    .ascii \"Fatal error: ptr_read_string() length must be non-negative\\n\"\n");
    out.push_str(&format!(
        ".globl _str_repeat_times_msg\n_str_repeat_times_msg:\n    .ascii {:?}\n",
        STR_REPEAT_TIMES_MSG
    ));
    for (label, message) in [
        ("_spl_dll_pop_empty_msg", "Can't pop from an empty datastructure"),
        ("_spl_dll_shift_empty_msg", "Can't shift from an empty datastructure"),
        ("_spl_dll_peek_empty_msg", "Can't peek at an empty datastructure"),
        (
            "_spl_dll_add_range_msg",
            "SplDoublyLinkedList::add(): Argument #1 ($index) is out of range",
        ),
        (
            "_spl_dll_offset_get_range_msg",
            "SplDoublyLinkedList::offsetGet(): Argument #1 ($index) is out of range",
        ),
        (
            "_spl_dll_offset_set_range_msg",
            "SplDoublyLinkedList::offsetSet(): Argument #1 ($index) is out of range",
        ),
        (
            "_spl_dll_offset_unset_range_msg",
            "SplDoublyLinkedList::offsetUnset(): Argument #1 ($index) is out of range",
        ),
        (
            "_spl_dll_offset_exists_type_msg",
            "SplDoublyLinkedList::offsetExists(): Argument #1 ($index) must be of type int, non-int given",
        ),
        (
            "_spl_dll_offset_get_type_msg",
            "SplDoublyLinkedList::offsetGet(): Argument #1 ($index) must be of type int, non-int given",
        ),
        (
            "_spl_dll_offset_set_type_msg",
            "SplDoublyLinkedList::offsetSet(): Argument #1 ($index) must be of type ?int, non-int given",
        ),
        (
            "_spl_dll_offset_unset_type_msg",
            "SplDoublyLinkedList::offsetUnset(): Argument #1 ($index) must be of type int, non-int given",
        ),
        (
            "_spl_fixed_construct_size_msg",
            "SplFixedArray::__construct(): Argument #1 ($size) must be greater than or equal to 0",
        ),
        (
            "_spl_fixed_set_size_msg",
            "SplFixedArray::setSize(): Argument #1 ($size) must be greater than or equal to 0",
        ),
        (
            "_spl_fixed_offset_type_msg",
            "Cannot access offset of type non-int on SplFixedArray",
        ),
        ("_spl_fixed_offset_range_msg", "Index invalid or out of range"),
        (
            "_spl_fixed_from_array_keys_msg",
            "array must contain only positive integer keys",
        ),
        (
            "_array_filter_mode_msg",
            "array_filter(): Argument #3 ($mode) must be one of ARRAY_FILTER_USE_VALUE, ARRAY_FILTER_USE_KEY, or ARRAY_FILTER_USE_BOTH.",
        ),
        (
            "_iterator_iterator_downcast_msg",
            "Class to downcast to not found or not base class or does not implement Traversable",
        ),
    ] {
        out.push_str(&format!(".globl {label}\n{label}:\n    .ascii {message:?}\n"));
    }
    out.push_str(".globl _uncaught_exc_msg\n_uncaught_exc_msg:\n    .ascii \"Fatal error: uncaught exception\\n\"\n");
    out.push_str(".globl _instanceof_target_type_msg\n_instanceof_target_type_msg:\n    .ascii \"Fatal error: Class name must be a valid object or a string\\n\"\n");
    out.push_str(".globl _diag_file_get_contents_failed_msg\n_diag_file_get_contents_failed_msg:\n    .ascii \"Warning: file_get_contents(): Failed to open stream\\n\"\n");
    out.push_str(".globl _diag_fopen_failed_msg\n_diag_fopen_failed_msg:\n    .ascii \"Warning: fopen(): Failed to open stream\\n\"\n");
    out.push_str(".globl _diag_define_already_defined_msg\n_diag_define_already_defined_msg:\n    .ascii \"Warning: define(): Constant already defined\\n\"\n");
    out.push_str(".globl _diag_undefined_array_key_prefix\n_diag_undefined_array_key_prefix:\n    .ascii \"Warning: Undefined array key \"\n");
    out.push_str(".globl _diag_undefined_array_key_suffix\n_diag_undefined_array_key_suffix:\n    .ascii \"\\n\"\n");
    out.push_str(".globl _fiber_msg_already_started\n_fiber_msg_already_started:\n    .ascii \"Cannot start a fiber that has already been started\"\n");
    out.push_str(".globl _fiber_msg_not_suspended\n_fiber_msg_not_suspended:\n    .ascii \"Cannot resume a fiber that is not suspended\"\n");
    out.push_str(".globl _fiber_msg_throw_not_suspended\n_fiber_msg_throw_not_suspended:\n    .ascii \"Cannot resume a fiber that is not suspended\"\n");
    out.push_str(".globl _fiber_msg_not_terminated\n_fiber_msg_not_terminated:\n    .ascii \"Cannot get fiber return value: The fiber has not returned\"\n");
    out.push_str(".globl _fiber_msg_suspend_outside\n_fiber_msg_suspend_outside:\n    .ascii \"Cannot suspend outside of a fiber\"\n");
    out.push_str(".globl _fiber_msg_unsupported_callable\n_fiber_msg_unsupported_callable:\n    .ascii \"Fiber callable is not supported by this compiler\"\n");
    out.push_str(".globl _fiber_msg_stack_alloc_failed\n_fiber_msg_stack_alloc_failed:\n    .ascii \"Cannot allocate fiber stack\"\n");
    out.push_str(&emit_builtin_callable_data());
    out.push_str(".comm _gc_allocs, 8, 3\n");
    out.push_str(".comm _gc_frees, 8, 3\n");
    out.push_str(".comm _gc_live, 8, 3\n");
    out.push_str(".comm _gc_peak, 8, 3\n");
    out.push_str(".comm _cstr_buf, 4096, 3\n");
    out.push_str(".comm _cstr_buf2, 4096, 3\n");
    out.push_str(".comm _eof_flags, 256, 3\n");
    out.push_str(".comm _popen_files, 2048, 3\n");
    out.push_str(".comm _dir_handles, 2048, 3\n");
    // Per-fd glob:// state pointers (256 fds × 8B). Each slot is a pointer to
    // a heap-allocated glob_state struct (pathv ptr + pathc + index + the
    // libc glob_t whose lifetime globfree() needs at closedir time). The
    // readdir/closedir/rewinddir helpers probe this table first; a non-zero
    // entry routes them through the glob iterator instead of the libc DIR*.
    out.push_str(".comm _glob_handles, 2048, 3\n");
    out.push_str(".comm _stream_read_filters, 256, 3\n");
    out.push_str(".comm _stream_write_filters, 256, 3\n");
    out.push_str(".comm _stream_filter_buf, 65536, 3\n");
    // 64KB scratch used by length-growing stream filters (convert.base64-encode,
    // convert.quoted-printable-encode). The filter encodes into the scratch and
    // then memcpy()s back into the caller's buffer, capping input at 49152 bytes
    // so the 4/3 base64 expansion still fits the scratch.
    out.push_str(".comm _stream_grow_scratch, 65536, 3\n");
    out.push_str(".comm _zstream_handles, 2048, 3\n");
    out.push_str(".comm _zlib_fwrite_fn, 8, 3\n");
    out.push_str(".comm _zlib_close_fn, 8, 3\n");
    out.push_str(".globl _zlib_version\n_zlib_version:\n    .asciz \"1\"\n");
    // bzip2.compress write-filter state: per-fd bz_stream pointer table
    // (_bzstream_handles, indexed by fd) plus the indirect fn-pointer slots the
    // shared runtime calls through so non-bzip2 programs never link -lbz2.
    out.push_str(".comm _bzstream_handles, 2048, 3\n");
    out.push_str(".comm _bz2_fwrite_fn, 8, 3\n");
    out.push_str(".comm _bz2_close_fn, 8, 3\n");
    // convert.iconv.* WRITE-filter state: per-fd iconv_t descriptor table
    // (_iconv_handles) plus the indirect fn-pointer slots the shared runtime
    // calls through so it never names libc iconv (which needs -liconv on macOS).
    out.push_str(".comm _iconv_handles, 2048, 3\n");
    out.push_str(".comm _iconv_fwrite_fn, 8, 3\n");
    out.push_str(".comm _iconv_close_fn, 8, 3\n");
    out.push_str(".comm _ftp_resp_buf, 4096, 3\n");
    out.push_str(".comm _ftp_data_addr, 64, 3\n");
    // _ftp_use_tls: set to 1 by fopen("ftps://...") before __rt_ftp_open is
    // invoked. The handshake helper interprets it as "perform AUTH TLS on the
    // control connection, PBSZ 0 + PROT P after USER/PASS, and elephc-tls-
    // attach the PASV data connection". Reset to 0 at the end of __rt_ftp_open
    // so subsequent plain ftp:// opens are not contaminated.
    out.push_str(".comm _ftp_use_tls, 8, 3\n");
    out.push_str(".comm _http_resp_buf, 1048576, 3\n");
    // https:// goes through indirect function pointers so only programs that
    // actually open https URLs reference elephc-tls (and pull in -lelephc_tls
    // at link time); other programs keep the runtime libc-only.
    out.push_str(".comm _elephc_tls_connect_fn, 8, 3\n");
    // _elephc_tls_connect_insecure_fn: same shape as _elephc_tls_connect_fn
    // but dispatched when the caller has set ssl.verify_peer = false on the
    // stream context. The runtime picks one over the other at https_open
    // time so non-TLS programs still don't link elephc-tls.
    out.push_str(".comm _elephc_tls_connect_insecure_fn, 8, 3\n");
    // _elephc_tls_connect_cafile_fn: dispatched when the caller has set
    // ssl.cafile on the stream context. Same late-binding pattern; takes two
    // extra args (cafile path ptr/len) that the secure/insecure variants ignore.
    out.push_str(".comm _elephc_tls_connect_cafile_fn, 8, 3\n");
    // _elephc_tls_connect_capath_fn / _peer_name_fn: dispatched for ssl.capath
    // (a directory of CA certs) and ssl.peer_name (verify the cert for a name
    // other than the connection host). Same late-binding/extra-args pattern.
    out.push_str(".comm _elephc_tls_connect_capath_fn, 8, 3\n");
    out.push_str(".comm _elephc_tls_connect_peer_name_fn, 8, 3\n");
    out.push_str(".comm _elephc_tls_write_fn, 8, 3\n");
    out.push_str(".comm _elephc_tls_read_fn, 8, 3\n");
    out.push_str(".comm _elephc_tls_close_fn, 8, 3\n");
    // _elephc_tls_attach_fd_fn: indirect pointer to elephc_tls_attach_fd,
    // used by stream_socket_enable_crypto to promote an existing TCP fd to
    // a TLS session without re-establishing the TCP connection. Same
    // late-binding pattern as the other tls fn slots so non-TLS programs
    // do not pull in elephc-tls at link time.
    out.push_str(".comm _elephc_tls_attach_fd_fn, 8, 3\n");
    // _elephc_tls_attach_fd_client_cert_fn / _elephc_tls_connect_client_cert_fn:
    // mutual-TLS variants dispatched when the stream context carries both
    // ssl.local_cert and ssl.local_pk. The attach variant is used by
    // stream_socket_enable_crypto; both take the extra cert/key path ptr/len
    // pairs that the non-client-cert variants ignore. Same late-binding pattern.
    out.push_str(".comm _elephc_tls_attach_fd_client_cert_fn, 8, 3\n");
    out.push_str(".comm _elephc_tls_connect_client_cert_fn, 8, 3\n");
    // _tls_sessions: per-fd TLS handle (i64 returned by
    // elephc_tls_attach_fd or 0 when the fd is plain TCP). Indexed by raw
    // fd up to 256; the runtime fread/fwrite/fclose paths consult this
    // table and route through the elephc-tls helpers when an entry is
    // non-zero, falling back to read/write/close syscalls otherwise.
    out.push_str(".comm _tls_sessions, 2048, 3\n");
    // _stream_chunk_size: per-fd read/write chunk size set by
    // stream_set_chunk_size, indexed by raw fd up to 256 (8 bytes each). A zero
    // entry means "unset" and reports PHP's default of 8192. stream_set_chunk_size
    // returns the previous value (the PHP-observable contract); the size does not
    // currently change read granularity (reads return identical data).
    out.push_str(".comm _stream_chunk_size, 2048, 3\n");
    // _stream_connect_host: per-fd transport host string (ptr, len) captured by
    // stream_socket_client so stream_socket_enable_crypto can default the TLS
    // SNI / peer-name to the connection host when no ssl.peer_name context
    // option is set. 256 fds * 16 bytes (ptr + len). A zero len means "unset".
    out.push_str(".comm _stream_connect_host, 4096, 3\n");
    // _stream_notification_callback: the callable descriptor pointer for the
    // stream context's `notification` option, captured at codegen time by
    // stream_context_create / stream_context_set_params. __rt_http_open fires
    // it at the CONNECT, COMPLETED, and FAILURE transfer milestones. Zero when
    // no notification callback is registered (the fire shim is then a no-op).
    out.push_str(".comm _stream_notification_callback, 8, 3\n");
    // _tls_peer_name_default: hardcoded peer-name buffer used as the SNI
    // hint when stream_socket_enable_crypto is called without a context
    // peer_name. v1 limitation — production TLS needs real peer-name
    // passing via the stream context (deferred).
    out.push_str(
        ".globl _tls_peer_name_default\n_tls_peer_name_default:\n    .ascii \"localhost\"\n",
    );
    // Key literals used by __rt_get_ssl_peer_name for the
    // _stream_context_options["ssl"]["peer_name"] lookup.
    out.push_str(".globl _ssl_key_str\n_ssl_key_str:\n    .ascii \"ssl\"\n");
    out.push_str(
        ".globl _ssl_peer_name_key_str\n_ssl_peer_name_key_str:\n    .ascii \"peer_name\"\n",
    );
    out.push_str(
        ".globl _ssl_verify_peer_key_str\n_ssl_verify_peer_key_str:\n    .ascii \"verify_peer\"\n",
    );
    out.push_str(
        ".globl _ssl_cafile_key_str\n_ssl_cafile_key_str:\n    .ascii \"cafile\"\n",
    );
    out.push_str(
        ".globl _ssl_capath_key_str\n_ssl_capath_key_str:\n    .ascii \"capath\"\n",
    );
    // ssl.local_cert / ssl.local_pk: the client-certificate chain and private
    // key paths for mutual TLS, consumed by stream_socket_enable_crypto.
    out.push_str(
        ".globl _ssl_local_cert_key_str\n_ssl_local_cert_key_str:\n    .ascii \"local_cert\"\n",
    );
    out.push_str(
        ".globl _ssl_local_pk_key_str\n_ssl_local_pk_key_str:\n    .ascii \"local_pk\"\n",
    );
    // (_ssl_peer_name_key_str is already defined above for stream_context_get_ssl_peer_name)
    out.push_str(
        ".globl _ssl_allow_self_signed_key_str\n_ssl_allow_self_signed_key_str:\n    .ascii \"allow_self_signed\"\n",
    );
    out.push_str(
        ".globl _ssl_verify_peer_name_key_str\n_ssl_verify_peer_name_key_str:\n    .ascii \"verify_peer_name\"\n",
    );
    // Key literals + request fragments used by __rt_http_build_request.
    out.push_str(".globl _http_key_str\n_http_key_str:\n    .ascii \"http\"\n");
    out.push_str(
        ".globl _http_method_key_str\n_http_method_key_str:\n    .ascii \"method\"\n",
    );
    out.push_str(
        ".globl _http_header_key_str\n_http_header_key_str:\n    .ascii \"header\"\n",
    );
    out.push_str(
        ".globl _http_content_key_str\n_http_content_key_str:\n    .ascii \"content\"\n",
    );
    // "Content-Length: " — 16 bytes, written before the numeric length
    // when context supplies a body. The numeric length comes from
    // __rt_itoa, followed by a CRLF before the Connection header.
    out.push_str(
        ".globl _http_content_length_prefix\n_http_content_length_prefix:\n    .ascii \"Content-Length: \"\n",
    );
    // Phase B HTTP-context option keys + header prefixes used by
    // __rt_http_build_request when stream_context_set_option(... 'http' ...)
    // provides the corresponding value.
    out.push_str(
        ".globl _http_user_agent_key_str\n_http_user_agent_key_str:\n    .ascii \"user_agent\"\n",
    );
    out.push_str(
        ".globl _http_user_agent_prefix\n_http_user_agent_prefix:\n    .ascii \"User-Agent: \"\n",
    );
    out.push_str(
        ".globl _http_protocol_version_key_str\n_http_protocol_version_key_str:\n    .ascii \"protocol_version\"\n",
    );
    // 17-byte " HTTP/1.1\r\nHost: " variant used when [http][protocol_version]
    // is the literal string "1.1".
    out.push_str(
        ".globl _http_version_host_11\n_http_version_host_11:\n    .ascii \" HTTP/1.1\\r\\nHost: \"\n",
    );
    out.push_str(
        ".globl _http_proxy_key_str\n_http_proxy_key_str:\n    .ascii \"proxy\"\n",
    );
    // Socket-wrapper context option keys read by stream_socket_client /
    // stream_socket_server before / after their respective syscalls.
    // _empty_str: a guaranteed-readable 1-byte buffer used as the pointer for a
    // zero-length string null-fallback (out-of-bounds indexed read / assoc miss
    // on a Str-typed array). len 0 means no bytes are ever read; the valid
    // pointer keeps any echo/strlen path that still loads the pointer safe.
    out.push_str(".comm _empty_str, 1, 1\n");
    // _url_stat_matched: set to 1 by __rt_user_wrapper_url_stat when a path's
    // scheme matches a registered userspace wrapper, 0 otherwise. The path-based
    // stat builtins (file_exists/is_file/filesize) read it after the call to
    // decide between the wrapper's url_stat() result and the real filesystem.
    out.push_str(".comm _url_stat_matched, 1, 1\n");
    out.push_str(
        ".globl _socket_key_str\n_socket_key_str:\n    .ascii \"socket\"\n",
    );
    out.push_str(
        ".globl _socket_tcp_nodelay_key_str\n_socket_tcp_nodelay_key_str:\n    .ascii \"tcp_nodelay\"\n",
    );
    out.push_str(
        ".globl _socket_so_reuseport_key_str\n_socket_so_reuseport_key_str:\n    .ascii \"so_reuseport\"\n",
    );
    out.push_str(
        ".globl _socket_so_broadcast_key_str\n_socket_so_broadcast_key_str:\n    .ascii \"so_broadcast\"\n",
    );
    out.push_str(
        ".globl _socket_backlog_key_str\n_socket_backlog_key_str:\n    .ascii \"backlog\"\n",
    );
    out.push_str(
        ".globl _socket_ipv6_v6only_key_str\n_socket_ipv6_v6only_key_str:\n    .ascii \"ipv6_v6only\"\n",
    );
    out.push_str(
        ".globl _socket_bindto_key_str\n_socket_bindto_key_str:\n    .ascii \"bindto\"\n",
    );
    // "http://" — used as the scheme prefix when [http][request_fulluri] is truthy
    out.push_str(
        ".globl _http_scheme_prefix\n_http_scheme_prefix:\n    .ascii \"http://\"\n",
    );
    // Active HTTP context options written by __rt_http_build_request and
    // read by __rt_http_open. Lets the build-side (which performs the
    // context lookups) communicate enforcement-relevant values to the
    // socket-side without needing extra args.
    //   _http_active_ignore_errors : 1 = silently return body on 4xx/5xx;
    //                                0 = fail-open behavior (default in PHP).
    //   _http_active_max_redirects : count of remaining hops for
    //                                follow_location loops (0 disables).
    out.push_str(".comm _http_active_ignore_errors, 8, 3\n");
    out.push_str(".comm _http_active_max_redirects, 8, 3\n");
    out.push_str(".comm _http_active_timeout_seconds, 8, 3\n");
    // Proxy override for __rt_http_open: when non-zero, used as the TCP
    // connect target instead of the host extracted from the URL. Value
    // shape is "tcp://proxyhost:port" — the same format
    // __rt_stream_socket_client expects.
    out.push_str(".comm _http_active_proxy_ptr, 8, 3\n");
    out.push_str(".comm _http_active_proxy_len, 8, 3\n");
    // Host info written by __rt_http_build_request and consumed by
    // __rt_http_open when [http][follow_location] triggers an internal
    // redirect — we rebuild the request with the saved host + the
    // Location-header path.
    out.push_str(".comm _http_active_host_ptr, 8, 3\n");
    out.push_str(".comm _http_active_host_len, 8, 3\n");
    // 2 KiB scratch for the Location header's path component on
    // relative redirects (covers the vast majority of API redirects).
    out.push_str(".comm _http_redirect_path_buf, 2048, 3\n");
    out.push_str(".comm _http_redirect_path_len, 8, 3\n");
    out.push_str(
        ".globl _http_request_fulluri_key_str\n_http_request_fulluri_key_str:\n    .ascii \"request_fulluri\"\n",
    );
    out.push_str(
        ".globl _http_follow_location_key_str\n_http_follow_location_key_str:\n    .ascii \"follow_location\"\n",
    );
    out.push_str(
        ".globl _http_max_redirects_key_str\n_http_max_redirects_key_str:\n    .ascii \"max_redirects\"\n",
    );
    out.push_str(
        ".globl _http_ignore_errors_key_str\n_http_ignore_errors_key_str:\n    .ascii \"ignore_errors\"\n",
    );
    out.push_str(
        ".globl _http_timeout_key_str\n_http_timeout_key_str:\n    .ascii \"timeout\"\n",
    );
    // The PHAR stub terminator (`__HALT_COMPILER();`, 18 bytes) that
    // __rt_phar_read_entry scans for at runtime to locate the manifest start.
    out.push_str(
        ".globl _phar_halt_magic\n_phar_halt_magic:\n    .ascii \"__HALT_COMPILER();\"\n",
    );
    // FTP context keys + command fragments used by __rt_ftp_open when
    // ['ftp']['resume_pos'] is set in the active stream context. v1
    // limitation: the value is stored as a string by stream_context_set_option,
    // so callers pass `'1024'` rather than `1024`.
    out.push_str(".globl _ftp_key_str\n_ftp_key_str:\n    .ascii \"ftp\"\n");
    out.push_str(
        ".globl _ftp_resume_pos_key_str\n_ftp_resume_pos_key_str:\n    .ascii \"resume_pos\"\n",
    );
    out.push_str(".globl _ftp_rest_prefix\n_ftp_rest_prefix:\n    .ascii \"REST \"\n");
    // 64-byte scratch for the dynamically built REST command. The largest
    // PHP int (19 ascii digits) + "REST " (5) + "\r\n" (2) = 26 bytes, so
    // 64 leaves generous headroom for future extensions (auth, custom
    // commands).
    out.push_str(".comm _ftp_cmd_scratch, 64, 3\n");

    // Bucket-brigade property keys used by __rt_user_filter_brigade_invoke
    // to build and walk brigade-shaped argument data when the user's
    // filter() method uses the PHP-canonical 4-arg signature.
    out.push_str(".globl _brigade_buckets_key\n_brigade_buckets_key:\n    .ascii \"_buckets\"\n");
    out.push_str(".globl _brigade_data_key\n_brigade_data_key:\n    .ascii \"data\"\n");
    out.push_str(".globl _brigade_datalen_key\n_brigade_datalen_key:\n    .ascii \"datalen\"\n");
    out.push_str(".globl _http_default_method\n_http_default_method:\n    .ascii \"GET\"\n");
    // " HTTP/1.0\r\nHost: " — 17 bytes (space + version + CRLF + Host
    // header prefix). Inserted between the path and the host literal.
    out.push_str(
        ".globl _http_version_host\n_http_version_host:\n    .ascii \" HTTP/1.0\\r\\nHost: \"\n",
    );
    // "\r\n" — 2 bytes, CRLF separator written after the Host value
    // (and again after each context-supplied header line).
    out.push_str(".globl _http_crlf\n_http_crlf:\n    .ascii \"\\r\\n\"\n");
    // "Connection: close\r\n\r\n" — 21 bytes Connection header + blank
    // line separator that ends the request headers section.
    out.push_str(
        ".globl _http_trailer\n_http_trailer:\n    .ascii \"Connection: close\\r\\n\\r\\n\"\n",
    );
    // _http_req_scratch: 8 KB buffer for the dynamically-built HTTP/1.0
    // request. Comfortable headroom over (method 16 + path 4 KB + host
    // 253 + boilerplate 80) while keeping the BSS small. Populated by
    // `__rt_http_build_request` and consumed by `__rt_http_open` via
    // the http_stream lowering when context options can override the
    // default method.
    out.push_str(".comm _http_req_scratch, 8192, 3\n");
    out.push_str(".comm _https_resp_buf, 1048576, 3\n");
    out.push_str(".comm _fsockopen_addr, 512, 3\n");
    // _user_wrappers: USER_WRAPPER_REGISTRATIONS_CAP = 64 scheme→class
    // registrations, each entry 32 bytes (protocol_ptr/len + class_ptr/len).
    // Slot is free when protocol_ptr is null. 64 × 32 = 2048 bytes.
    out.push_str(".comm _user_wrappers, 2048, 3\n");
    // _user_wrapper_handles: USER_WRAPPER_HANDLES_CAP = 256 active stream-handle
    // slots, each storing the wrapper object pointer keyed by synthetic fd
    // `USER_WRAPPER_FD_BASE + slot_index`. Slot is free when the stored pointer
    // is null. 256 slots × 8 bytes = 2048 bytes.
    out.push_str(".comm _user_wrapper_handles, 2048, 3\n");
    // _user_wrapper_drain_buf: 1 MiB accumulation buffer for the codegen-level
    // feof-gated read loop emitted by stream_get_contents on a wrapper fd.
    // Each fread chunk is copied here, building one contiguous result. Drains
    // larger than 1 MiB are truncated (v1).
    out.push_str(".comm _user_wrapper_drain_buf, 1048576, 3\n");
    // phar:// write (Milestone-1) state. _phar_write_out is the 1 MiB in-memory
    // archive buffer (template prefix + entry content); _phar_write_len is the
    // bytes used; _phar_write_tpl_len is the template prefix length finalize uses
    // to locate the manifest size/crc fields and the content start; the
    // _phar_write_path_ptr/_len pair holds the on-disk archive path the
    // fopen("phar://...","w") emitter records for __rt_phar_write_finalize. One
    // phar-write stream at a time; the synthetic descriptor is 0x50000000.
    out.push_str(".comm _phar_write_out, 1048576, 3\n");
    out.push_str(".comm _phar_write_len, 8, 3\n");
    out.push_str(".comm _phar_write_tpl_len, 8, 3\n");
    out.push_str(".comm _phar_write_path_ptr, 8, 3\n");
    out.push_str(".comm _phar_write_path_len, 8, 3\n");
    // _stream_open_opened_path_scratch: 16-byte scratch backing the 5th
    // `?string &$opened_path` parameter of stream_open. The runtime passes
    // its address so wrappers that follow the PHP-faithful signature can
    // safely write to it; elephc v1 zeroes the slot before each call and
    // does not read the value back.
    out.push_str(".comm _stream_open_opened_path_scratch, 16, 3\n");
    // _user_filter_registry: 128 (filter_name, class_name) registrations,
    // each entry 32 bytes (filter_name_ptr/len + class_name_ptr/len). Slot
    // is free when filter_name_ptr is null. User filter IDs are slot_index
    // + USER_FILTER_ID_BASE (128) so they don't collide with the existing
    // u8 built-in filter IDs (1..=4). 128 × 32 = 4096 bytes.
    out.push_str(".comm _user_filter_registry, 4096, 3\n");
    // _user_filter_instances: one wrapper-class instance per attached
    // filter, keyed by (fd, direction). Slot = _user_filter_instances[fd*2
    // + dir] where dir=0 is read, dir=1 is write. Slot is null when no
    // user filter is attached. 256 fds × 2 dirs × 8 B = 4096 bytes.
    out.push_str(".comm _user_filter_instances, 4096, 3\n");
    // _stream_context_options: pointer to the current stream-context
    // options hash (nested array of `wrapper => option => value`).
    // stream_context_create() stores its options arg here; consumers
    // (http://, ftp://, fopen 4th arg) read it back through
    // __rt_hash_get. v1 limitation: only one active context at a time —
    // a fresh stream_context_create overwrites the slot.
    out.push_str(".comm _stream_context_options, 8, 3\n");
    // var_dump body literals (rodata): per-element prefix/suffix bytes
    // used by the array walkers __rt_var_dump_array_int / _str.
    out.push_str(".globl _vd_indent_open\n_vd_indent_open:\n    .ascii \"  [\"\n");
    out.push_str(".globl _vd_close_arrow\n_vd_close_arrow:\n    .ascii \"]=>\\n\"\n");
    out.push_str(".globl _vd_int_prefix\n_vd_int_prefix:\n    .ascii \"  int(\"\n");
    out.push_str(".globl _vd_close_paren\n_vd_close_paren:\n    .ascii \")\\n\"\n");
    out.push_str(".globl _vd_str_prefix\n_vd_str_prefix:\n    .ascii \"  string(\"\n");
    out.push_str(".globl _vd_close_paren_space\n_vd_close_paren_space:\n    .ascii \") \\\"\"\n");
    out.push_str(".globl _vd_close_quote\n_vd_close_quote:\n    .ascii \"\\\"\\n\"\n");
    // var_dump bool-array literals — preformatted lines (12 / 13 bytes) so
    // the bool walker is a single dispatch + write.
    out.push_str(".globl _vd_bool_true_line\n_vd_bool_true_line:\n    .ascii \"  bool(true)\\n\"\n");
    out.push_str(".globl _vd_bool_false_line\n_vd_bool_false_line:\n    .ascii \"  bool(false)\\n\"\n");
    out.push_str(".globl _vd_float_prefix\n_vd_float_prefix:\n    .ascii \"  float(\"\n");
    out.push_str(".globl _vd_null_line\n_vd_null_line:\n    .ascii \"  NULL\\n\"\n");
    out.push_str(".globl _ftp_user_cmd\n_ftp_user_cmd:\n    .ascii \"USER anonymous\\x0d\\n\"\n");
    out.push_str(".globl _ftp_pass_cmd\n_ftp_pass_cmd:\n    .ascii \"PASS anonymous@\\x0d\\n\"\n");
    out.push_str(".globl _ftp_type_cmd\n_ftp_type_cmd:\n    .ascii \"TYPE I\\x0d\\n\"\n");
    out.push_str(".globl _ftp_pasv_cmd\n_ftp_pasv_cmd:\n    .ascii \"PASV\\x0d\\n\"\n");
    out.push_str(".globl _ftp_tcp_prefix\n_ftp_tcp_prefix:\n    .ascii \"tcp://\"\n");
    // ftps:// commands (RFC 4217). AUTH TLS upgrades the control connection;
    // PBSZ 0 sets the protection buffer size (always 0 for TLS); PROT P
    // enables private (encrypted) data-channel protection.
    out.push_str(".globl _ftp_auth_tls_cmd\n_ftp_auth_tls_cmd:\n    .ascii \"AUTH TLS\\x0d\\n\"\n");
    out.push_str(".globl _ftp_pbsz_cmd\n_ftp_pbsz_cmd:\n    .ascii \"PBSZ 0\\x0d\\n\"\n");
    out.push_str(".globl _ftp_prot_p_cmd\n_ftp_prot_p_cmd:\n    .ascii \"PROT P\\x0d\\n\"\n");
    out.push_str(".comm _recvfrom_addr_ptr, 8, 3\n");
    out.push_str(".comm _recvfrom_addr_len, 8, 3\n");
    out.push_str(".comm _accept_peer_ptr, 8, 3\n");
    out.push_str(".comm _accept_peer_len, 8, 3\n");
    out.push_str(".comm _protoent_buf, 32768, 3\n");
    out.push_str(".globl _etc_protocols_path\n_etc_protocols_path:\n    .asciz \"/etc/protocols\"\n");
    out.push_str(".comm _servent_buf, 1048576, 3\n");
    out.push_str(".globl _etc_services_path\n_etc_services_path:\n    .asciz \"/etc/services\"\n");
    out.push_str(&emit_spl_autoload_extensions_data());
    out.push_str(".globl _heap_dbg_stats_prefix\n_heap_dbg_stats_prefix:\n    .ascii \"HEAP DEBUG: allocs=\"\n");
    out.push_str(".globl _heap_dbg_frees_label\n_heap_dbg_frees_label:\n    .ascii \" frees=\"\n");
    out.push_str(".globl _heap_dbg_live_blocks_label\n_heap_dbg_live_blocks_label:\n    .ascii \" live_blocks=\"\n");
    out.push_str(".globl _heap_dbg_live_bytes_label\n_heap_dbg_live_bytes_label:\n    .ascii \" live_bytes=\"\n");
    out.push_str(".globl _heap_dbg_peak_label\n_heap_dbg_peak_label:\n    .ascii \" peak_live_bytes=\"\n");
    out.push_str(".globl _heap_dbg_leak_prefix\n_heap_dbg_leak_prefix:\n    .ascii \"HEAP DEBUG: leak summary: \"\n");
    out.push_str(".globl _heap_dbg_live_blocks_short_label\n_heap_dbg_live_blocks_short_label:\n    .ascii \"live_blocks=\"\n");
    out.push_str(".globl _heap_dbg_clean_label\n_heap_dbg_clean_label:\n    .ascii \"clean\\n\"\n");
    out.push_str(".globl _heap_dbg_newline\n_heap_dbg_newline:\n    .ascii \"\\n\"\n");
    out.push_str(".globl _resource_id_prefix\n_resource_id_prefix:\n    .ascii \"Resource id #\"\n");
    out.push_str(".globl _fmt_g\n_fmt_g:\n    .asciz \"%.14G\"\n");
    out.push_str(".globl _b64_encode_tbl\n_b64_encode_tbl:\n    .ascii \"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/\"\n");
    out.push_str(".globl _b64_decode_tbl\n_b64_decode_tbl:\n");

    let mut decode_tbl = vec![0u8; 256];
    for (i, &c) in b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
        .iter()
        .enumerate()
    {
        decode_tbl[c as usize] = i as u8;
    }

    out.push_str("    .byte ");
    for (i, val) in decode_tbl.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&val.to_string());
    }
    out.push('\n');

    out.push_str(".globl _filetype_file\n_filetype_file:\n    .ascii \"file\"\n");
    out.push_str(".globl _filetype_dir\n_filetype_dir:\n    .ascii \"dir\"\n");
    out.push_str(".globl _filetype_link\n_filetype_link:\n    .ascii \"link\"\n");
    out.push_str(".globl _filetype_char\n_filetype_char:\n    .ascii \"char\"\n");
    out.push_str(".globl _filetype_block\n_filetype_block:\n    .ascii \"block\"\n");
    out.push_str(".globl _filetype_fifo\n_filetype_fifo:\n    .ascii \"fifo\"\n");
    out.push_str(".globl _filetype_socket\n_filetype_socket:\n    .ascii \"socket\"\n");
    out.push_str(".globl _filetype_unknown\n_filetype_unknown:\n    .ascii \"unknown\"\n");
    out.push_str(".globl _stat_key_dev\n_stat_key_dev:\n    .ascii \"dev\"\n");
    out.push_str(".globl _stat_key_ino\n_stat_key_ino:\n    .ascii \"ino\"\n");
    out.push_str(".globl _stat_key_mode\n_stat_key_mode:\n    .ascii \"mode\"\n");
    out.push_str(".globl _stat_key_nlink\n_stat_key_nlink:\n    .ascii \"nlink\"\n");
    out.push_str(".globl _stat_key_uid\n_stat_key_uid:\n    .ascii \"uid\"\n");
    out.push_str(".globl _stat_key_gid\n_stat_key_gid:\n    .ascii \"gid\"\n");
    out.push_str(".globl _stat_key_rdev\n_stat_key_rdev:\n    .ascii \"rdev\"\n");
    out.push_str(".globl _stat_key_size\n_stat_key_size:\n    .ascii \"size\"\n");
    out.push_str(".globl _stat_key_atime\n_stat_key_atime:\n    .ascii \"atime\"\n");
    out.push_str(".globl _stat_key_mtime\n_stat_key_mtime:\n    .ascii \"mtime\"\n");
    out.push_str(".globl _stat_key_ctime\n_stat_key_ctime:\n    .ascii \"ctime\"\n");
    out.push_str(".globl _stat_key_blksize\n_stat_key_blksize:\n    .ascii \"blksize\"\n");
    out.push_str(".globl _stat_key_blocks\n_stat_key_blocks:\n    .ascii \"blocks\"\n");
    out.push_str(".globl _dirname_dot\n_dirname_dot:\n    .ascii \".\"\n");
    out.push_str(".globl _dirname_slash\n_dirname_slash:\n    .ascii \"/\"\n");
    out.push_str(&format!(
        ".globl _dirname_levels_msg\n_dirname_levels_msg:\n    .ascii {:?}\n",
        DIRNAME_LEVELS_MSG
    ));
    out.push_str(".globl _pathinfo_key_dirname\n_pathinfo_key_dirname:\n    .ascii \"dirname\"\n");
    out.push_str(".globl _pathinfo_key_basename\n_pathinfo_key_basename:\n    .ascii \"basename\"\n");
    out.push_str(".globl _pathinfo_key_extension\n_pathinfo_key_extension:\n    .ascii \"extension\"\n");
    out.push_str(".globl _pathinfo_key_filename\n_pathinfo_key_filename:\n    .ascii \"filename\"\n");
    out.push_str(".globl _meta_key_timed_out\n_meta_key_timed_out:\n    .ascii \"timed_out\"\n");
    out.push_str(".globl _meta_key_blocked\n_meta_key_blocked:\n    .ascii \"blocked\"\n");
    out.push_str(".globl _meta_key_eof\n_meta_key_eof:\n    .ascii \"eof\"\n");
    out.push_str(".globl _meta_key_unread_bytes\n_meta_key_unread_bytes:\n    .ascii \"unread_bytes\"\n");
    out.push_str(".globl _meta_key_stream_type\n_meta_key_stream_type:\n    .ascii \"stream_type\"\n");
    out.push_str(".globl _meta_key_wrapper_type\n_meta_key_wrapper_type:\n    .ascii \"wrapper_type\"\n");
    out.push_str(".globl _meta_key_mode\n_meta_key_mode:\n    .ascii \"mode\"\n");
    out.push_str(".globl _meta_key_seekable\n_meta_key_seekable:\n    .ascii \"seekable\"\n");
    out.push_str(".globl _meta_key_uri\n_meta_key_uri:\n    .ascii \"uri\"\n");
    out.push_str(".globl _meta_stype_stdio\n_meta_stype_stdio:\n    .ascii \"STDIO\"\n");
    out.push_str(".globl _meta_stype_socket\n_meta_stype_socket:\n    .ascii \"tcp_socket\"\n");
    out.push_str(".globl _meta_wrapper_plainfile\n_meta_wrapper_plainfile:\n    .ascii \"plainfile\"\n");
    out.push_str(".globl _meta_mode_r\n_meta_mode_r:\n    .ascii \"r\"\n");
    out.push_str(".globl _meta_mode_w\n_meta_mode_w:\n    .ascii \"w\"\n");
    out.push_str(".globl _meta_mode_rw\n_meta_mode_rw:\n    .ascii \"r+\"\n");
    out.push_str(".p2align 3\n");
    out.push_str(".globl _tmpfile_template\n_tmpfile_template:\n    .ascii \"/tmp/elephc-XXXXXX\\0\"\n    .byte 0,0,0,0,0\n");
    out.push_str(".globl _locale_utf8_name\n_locale_utf8_name:\n    .asciz \"C.UTF-8\"\n");
    out.push_str(".globl _locale_env_name\n_locale_env_name:\n    .asciz \"\"\n");
    out.push_str(&system::emit_json_data());
    out.push_str(&system::emit_date_data());
    out.push_str(&system::emit_strtotime_data());
    out.push_str(&emit_php_uname_data());

    out
}

/// Emit symbol data for all first-class-callable builtin functions.
///
/// Produces per-name labels (`_callable_builtin_name_N`), a null-terminated
/// `"__invoke"` string for `__invoke` lookups, `_callable_builtin_count`
/// holding the total count, and `_callable_builtin_table` containing
/// pointer/length pairs for each builtin. Used by the `is_callable()` runtime
/// routine and callable-invoke paths.
fn emit_builtin_callable_data() -> String {
    let mut out = String::new();
    let builtins = supported_builtin_function_names();
    for (idx, name) in builtins.iter().enumerate() {
        out.push_str(&format!(
            ".globl _callable_builtin_name_{0}\n_callable_builtin_name_{0}:\n    .ascii \"{1}\"\n",
            idx, name
        ));
    }
    out.push_str(".p2align 3\n");
    out.push_str(".globl _callable_invoke_name\n_callable_invoke_name:\n");
    out.push_str("    .ascii \"__invoke\"\n");
    out.push_str(".p2align 3\n");
    out.push_str(".globl _callable_builtin_count\n_callable_builtin_count:\n");
    out.push_str(&format!("    .quad {}\n", builtins.len()));
    out.push_str(".globl _callable_builtin_table\n_callable_builtin_table:\n");
    for (idx, name) in builtins.iter().enumerate() {
        out.push_str(&format!("    .quad _callable_builtin_name_{}\n", idx));
        out.push_str(&format!("    .quad {}\n", name.len()));
    }
    out
}

/// Emit the `php_uname_mode_len_msg` and `php_uname_mode_value_msg`
/// error message strings used when `php_uname()` mode argument validation fails.
fn emit_php_uname_data() -> String {
    format!(
        ".globl _php_uname_mode_len_msg\n_php_uname_mode_len_msg:\n    .ascii {:?}\n\
         .globl _php_uname_mode_value_msg\n_php_uname_mode_value_msg:\n    .ascii {:?}\n",
        PHP_UNAME_MODE_LEN_MSG, PHP_UNAME_MODE_VALUE_MSG
    )
}

/// Emit the mutable globals backing `spl_autoload_extensions` runtime
/// read/write. Initialised to point at the default ".inc,.php" string so
/// PHP programs see PHP's documented default before any explicit set.
fn emit_spl_autoload_extensions_data() -> String {
    let default = ".inc,.php";
    let mut out = String::new();
    out.push_str(".globl _spl_autoload_exts_default\n");
    out.push_str("_spl_autoload_exts_default:\n");
    out.push_str(&format!("    .ascii \"{}\"\n", default));
    out.push_str(".p2align 3\n");
    out.push_str(".globl _spl_autoload_exts_ptr\n");
    out.push_str("_spl_autoload_exts_ptr:\n");
    out.push_str("    .quad _spl_autoload_exts_default\n");
    out.push_str(".globl _spl_autoload_exts_len\n");
    out.push_str("_spl_autoload_exts_len:\n");
    out.push_str(&format!("    .quad {}\n", default.len()));
    out
}
