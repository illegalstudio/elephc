//! Purpose:
//! The frozen `CURLOPT_*` classification table: for every option number PHP's `ext/curl`
//! recognizes, how `curl_setopt()` must carry its value (`long`, `char *`,
//! `struct curl_slist *`, `curl_off_t`, a PHP-layer pseudo-option), or that this build
//! cannot carry it at all. This is the ONLY place that decides whether a `curl_setopt()`
//! call reaches libcurl.
//!
//! Called from:
//! - `crate::abi::elephc_curl_option_kind`, which the curl prelude's `curl_setopt()`
//!   consults before it picks which setter to call.
//! - `crate::tests`' wave-completeness ratchet, which cross-checks this table against
//!   `scripts/docs/curl_surface.json`.
//!
//! Key details:
//! - THE TABLE IS A SAFETY BOUNDARY, NOT A CONVENIENCE. `curl_easy_setopt` is variadic and
//!   reads its third argument according to the option's numeric RANGE (libcurl 8.21.0,
//!   `lib/setopt.c`): below 10000 a `long`, 10000-19999 a `char *`/`struct curl_slist *`,
//!   20000-29999 a function pointer, 30000-39999 a `curl_off_t`, 40000+ a
//!   `struct curl_blob *`. Handing a PHP string to a slist-typed option makes libcurl walk
//!   the string as a linked list. Every option that reaches libcurl does so through the
//!   setter this table names, and nothing else.
//! - THE NUMBERS COME FROM THE FROZEN SURFACE, never from a developer's `php -r`: the rows
//!   below are generated from `scripts/docs/curl_surface.json`'s `constants` +
//!   `option_kinds` (extracted against the pinned libcurl 8.21.0), and
//!   `crate::tests::option_table_matches_the_frozen_surface` fails if the two ever drift.
//! - EIGHT ROWS DELIBERATELY DIVERGE FROM THE JSON'S `option_kinds` BUCKET, all inside its
//!   `php_layer`/`file` catch-alls, which describe how PHP *models* an option rather than
//!   what libcurl needs:
//!   - `CURLOPT_HEADER` (42) and `CURLOPT_INFILESIZE` (14) are ordinary libcurl `long`
//!     options that php-src forwards verbatim from its own long switch, so they are
//!     `KIND_LONG` here, not PHP-layer.
//!   - `CURLOPT_PRIVATE` (10103) stores an arbitrary PHP value that
//!     `curl_getinfo(..., CURLINFO_PRIVATE)` reads back; libcurl never sees it, so it is
//!     `KIND_PHP_LAYER` and lives on the `CurlHandle` object.
//!   - `CURLOPT_FILE`/`INFILE`/`WRITEHEADER`/`STDERR`/`READDATA` (four rows — `INFILE` and
//!     `READDATA` share one option number) take a PHP STREAM, which
//!     is neither a scalar nor a `FILE *`, so they get their own kind, `KIND_STREAM` —
//!     the curl prelude implements them by composing this crate's callback slots rather
//!     than by forwarding anything to libcurl. They were `KIND_UNSUPPORTED` (`false` +
//!     PHP's warning) until WP-C; see `KIND_STREAM`'s own doc comment.
//!   - `CURLOPT_SHARE` diverges the same way, with its own kind, `KIND_SHARE`,
//!     because it needs a share-handle id, not a scalar — see that constant's doc comment.
//! - `CURLINFO_HEADER_OUT` (2) is in the table even though it is not a `CURLOPT_*`
//!   constant: php-src's `curl_setopt()` switch accepts it (it turns on request-header
//!   tracking through the debug callback), so answering `ValueError` for it would be
//!   wrong. Capturing the request header still needs machinery this build does not have,
//!   so it stays `KIND_UNSUPPORTED`.
//! - ANYTHING NOT IN THIS TABLE IS `KIND_INVALID`, which the prelude turns into php-src's
//!   own `ValueError: curl_setopt(): Argument #2 ($option) is not a valid cURL option`.
//!   That is the real php-src behaviour for an unrecognized option number, and it is a
//!   different answer from `KIND_UNSUPPORTED`'s `false` + warning, which is reserved for
//!   options php-src *does* recognize but this build cannot apply.

/// Not a `curl_setopt()` option at all -> php-src's `ValueError`.
pub(crate) const KIND_INVALID: i32 = 0;
/// `CURLOPTTYPE_LONG`/`CURLOPTTYPE_VALUES` -> `curl_easy_setopt(curl, opt, long)`.
pub(crate) const KIND_LONG: i32 = 1;
/// `CURLOPTTYPE_STRINGPOINT` -> `curl_easy_setopt(curl, opt, const char *)`.
pub(crate) const KIND_STRING: i32 = 2;
/// `CURLOPTTYPE_SLISTPOINT` -> `curl_easy_setopt(curl, opt, struct curl_slist *)`.
pub(crate) const KIND_SLIST: i32 = 3;
/// `CURLOPTTYPE_OFF_T` -> `curl_easy_setopt(curl, opt, curl_off_t)`.
pub(crate) const KIND_OFF_T: i32 = 4;
/// A PHP-only pseudo-option libcurl never sees (`CURLOPT_RETURNTRANSFER`,
/// `CURLOPT_BINARYTRANSFER`, `CURLOPT_SAFE_UPLOAD`, `CURLOPT_PRIVATE`).
pub(crate) const KIND_PHP_LAYER: i32 = 5;
/// Recognized by php-src's `curl_setopt()` but not carryable by this build (blobs,
/// callbacks, PHP stream options) -> `false` + PHP's warning.
pub(crate) const KIND_UNSUPPORTED: i32 = 6;
/// `CURLOPT_SHARE` (10100) ONLY: the value is a `CurlShareHandle`/`CurlSharePersistentHandle`
/// object, not a scalar libcurl setter can carry, so it is routed to
/// `elephc_curl_easy_set_share` (`crate::share`) instead of any of `setopt_long`/`_str`/
/// `_slist`. A distinct kind from every other row in this table; before it existed, this
/// option was `KIND_UNSUPPORTED` like its `file`-bucket siblings.
pub(crate) const KIND_SHARE: i32 = 7;
/// A `curl_setopt()` option whose value is a PHP STREAM RESOURCE (what `fopen()` returns),
/// or `null` to clear it: `CURLOPT_FILE` (10001), `CURLOPT_INFILE`/`CURLOPT_READDATA`
/// (10009), `CURLOPT_WRITEHEADER` (10029), `CURLOPT_STDERR` (10037).
///
/// NOTHING IS FORWARDED TO libcurl FOR THESE. php-src hands libcurl a `FILE *`/stream
/// pointer; elephc streams are not `FILE *`, so the curl prelude implements all four by
/// COMPOSING the callback options this crate already carries — an internal PHP closure in
/// the write/header/read/debug slot that `fwrite()`s to (or `fread()`s from) the stream.
/// The bridge therefore never sees a stream, and `setopt_str`/`_slist`/`_long` are never
/// reachable for these numbers. See `src/curl_prelude.rs`'s `$kind === 9` branch for the
/// precedence rules, all measured against PHP 8.4.20.
pub(crate) const KIND_STREAM: i32 = 9;
/// A `curl_setopt()` callback option whose value is a PHP `callable` (or `null` to
/// restore the default). Routed to `elephc_curl_easy_set_callback` (`crate::callbacks`)
/// with the option's slot index, never to any of `setopt_long`/`_str`/`_slist`. Covers
/// the first wave only — `CURLOPT_WRITEFUNCTION`, `CURLOPT_HEADERFUNCTION`,
/// `CURLOPT_READFUNCTION`, `CURLOPT_PROGRESSFUNCTION`, `CURLOPT_XFERINFOFUNCTION`, and
/// `CURLOPT_DEBUGFUNCTION`. The remaining callback options stay `KIND_UNSUPPORTED`
/// (see the rows below).
pub(crate) const KIND_CALLBACK: i32 = 8;

/// Every option number php-src's `curl_setopt()` recognizes, paired with how this build
/// carries it. Sorted by option number so [`option_kind`] can binary-search it; the sort
/// order is asserted by `crate::tests::option_table_is_sorted_and_unique`.
pub(crate) const OPTION_KINDS: &[(i32, i32)] = &[
    (-1, KIND_PHP_LAYER), // CURLOPT_SAFE_UPLOAD
    (2, KIND_UNSUPPORTED), // CURLINFO_HEADER_OUT
    (3, KIND_LONG), // CURLOPT_PORT
    (13, KIND_LONG), // CURLOPT_TIMEOUT
    (14, KIND_LONG), // CURLOPT_INFILESIZE
    (19, KIND_LONG), // CURLOPT_LOW_SPEED_LIMIT
    (20, KIND_LONG), // CURLOPT_LOW_SPEED_TIME
    (21, KIND_LONG), // CURLOPT_RESUME_FROM
    (27, KIND_LONG), // CURLOPT_CRLF
    (32, KIND_LONG), // CURLOPT_SSLVERSION
    (33, KIND_LONG), // CURLOPT_TIMECONDITION
    (34, KIND_LONG), // CURLOPT_TIMEVALUE
    (41, KIND_LONG), // CURLOPT_VERBOSE
    (42, KIND_LONG), // CURLOPT_HEADER
    (43, KIND_LONG), // CURLOPT_NOPROGRESS
    (44, KIND_LONG), // CURLOPT_NOBODY
    (45, KIND_LONG), // CURLOPT_FAILONERROR
    (46, KIND_LONG), // CURLOPT_UPLOAD
    (47, KIND_LONG), // CURLOPT_POST
    (48, KIND_LONG), // CURLOPT_DIRLISTONLY/CURLOPT_FTPLISTONLY
    (50, KIND_LONG), // CURLOPT_APPEND/CURLOPT_FTPAPPEND
    (51, KIND_LONG), // CURLOPT_NETRC
    (52, KIND_LONG), // CURLOPT_FOLLOWLOCATION
    (53, KIND_LONG), // CURLOPT_TRANSFERTEXT
    (54, KIND_LONG), // CURLOPT_PUT
    (58, KIND_LONG), // CURLOPT_AUTOREFERER
    (59, KIND_LONG), // CURLOPT_PROXYPORT
    (61, KIND_LONG), // CURLOPT_HTTPPROXYTUNNEL
    (64, KIND_LONG), // CURLOPT_SSL_VERIFYPEER
    (68, KIND_LONG), // CURLOPT_MAXREDIRS
    (69, KIND_LONG), // CURLOPT_FILETIME
    (71, KIND_LONG), // CURLOPT_MAXCONNECTS
    (74, KIND_LONG), // CURLOPT_FRESH_CONNECT
    (75, KIND_LONG), // CURLOPT_FORBID_REUSE
    (78, KIND_LONG), // CURLOPT_CONNECTTIMEOUT
    (80, KIND_LONG), // CURLOPT_HTTPGET
    (81, KIND_LONG), // CURLOPT_SSL_VERIFYHOST
    (84, KIND_LONG), // CURLOPT_HTTP_VERSION
    (85, KIND_LONG), // CURLOPT_FTP_USE_EPSV
    (90, KIND_LONG), // CURLOPT_SSLENGINE_DEFAULT
    (91, KIND_LONG), // CURLOPT_DNS_USE_GLOBAL_CACHE
    (92, KIND_LONG), // CURLOPT_DNS_CACHE_TIMEOUT
    (96, KIND_LONG), // CURLOPT_COOKIESESSION
    (98, KIND_LONG), // CURLOPT_BUFFERSIZE
    (99, KIND_LONG), // CURLOPT_NOSIGNAL
    (101, KIND_LONG), // CURLOPT_PROXYTYPE
    (105, KIND_LONG), // CURLOPT_UNRESTRICTED_AUTH
    (106, KIND_LONG), // CURLOPT_FTP_USE_EPRT
    (107, KIND_LONG), // CURLOPT_HTTPAUTH
    (110, KIND_LONG), // CURLOPT_FTP_CREATE_MISSING_DIRS
    (111, KIND_LONG), // CURLOPT_PROXYAUTH
    (112, KIND_LONG), // CURLOPT_FTP_RESPONSE_TIMEOUT/CURLOPT_SERVER_RESPONSE_TIMEOUT
    (113, KIND_LONG), // CURLOPT_IPRESOLVE
    (114, KIND_LONG), // CURLOPT_MAXFILESIZE
    (119, KIND_LONG), // CURLOPT_FTP_SSL/CURLOPT_USE_SSL
    (121, KIND_LONG), // CURLOPT_TCP_NODELAY
    (129, KIND_LONG), // CURLOPT_FTPSSLAUTH
    (136, KIND_LONG), // CURLOPT_IGNORE_CONTENT_LENGTH
    (137, KIND_LONG), // CURLOPT_FTP_SKIP_PASV_IP
    (138, KIND_LONG), // CURLOPT_FTP_FILEMETHOD
    (139, KIND_LONG), // CURLOPT_LOCALPORT
    (140, KIND_LONG), // CURLOPT_LOCALPORTRANGE
    (141, KIND_LONG), // CURLOPT_CONNECT_ONLY
    (150, KIND_LONG), // CURLOPT_SSL_SESSIONID_CACHE
    (151, KIND_LONG), // CURLOPT_SSH_AUTH_TYPES
    (154, KIND_LONG), // CURLOPT_FTP_SSL_CCC
    (155, KIND_LONG), // CURLOPT_TIMEOUT_MS
    (156, KIND_LONG), // CURLOPT_CONNECTTIMEOUT_MS
    (157, KIND_LONG), // CURLOPT_HTTP_TRANSFER_DECODING
    (158, KIND_LONG), // CURLOPT_HTTP_CONTENT_DECODING
    (159, KIND_LONG), // CURLOPT_NEW_FILE_PERMS
    (160, KIND_LONG), // CURLOPT_NEW_DIRECTORY_PERMS
    (161, KIND_LONG), // CURLOPT_POSTREDIR
    (166, KIND_LONG), // CURLOPT_PROXY_TRANSFER_MODE
    (171, KIND_LONG), // CURLOPT_ADDRESS_SCOPE
    (172, KIND_LONG), // CURLOPT_CERTINFO
    (178, KIND_LONG), // CURLOPT_TFTP_BLKSIZE
    (180, KIND_LONG), // CURLOPT_SOCKS5_GSSAPI_NEC
    (181, KIND_LONG), // CURLOPT_PROTOCOLS
    (182, KIND_LONG), // CURLOPT_REDIR_PROTOCOLS
    (188, KIND_LONG), // CURLOPT_FTP_USE_PRET
    (189, KIND_LONG), // CURLOPT_RTSP_REQUEST
    (193, KIND_LONG), // CURLOPT_RTSP_CLIENT_CSEQ
    (194, KIND_LONG), // CURLOPT_RTSP_SERVER_CSEQ
    (197, KIND_LONG), // CURLOPT_WILDCARDMATCH
    (207, KIND_LONG), // CURLOPT_TRANSFER_ENCODING
    (210, KIND_LONG), // CURLOPT_GSSAPI_DELEGATION
    (212, KIND_LONG), // CURLOPT_ACCEPTTIMEOUT_MS
    (213, KIND_LONG), // CURLOPT_TCP_KEEPALIVE
    (214, KIND_LONG), // CURLOPT_TCP_KEEPIDLE
    (215, KIND_LONG), // CURLOPT_TCP_KEEPINTVL
    (216, KIND_LONG), // CURLOPT_SSL_OPTIONS
    (218, KIND_LONG), // CURLOPT_SASL_IR
    (225, KIND_LONG), // CURLOPT_SSL_ENABLE_NPN
    (226, KIND_LONG), // CURLOPT_SSL_ENABLE_ALPN
    (227, KIND_LONG), // CURLOPT_EXPECT_100_TIMEOUT_MS
    (229, KIND_LONG), // CURLOPT_HEADEROPT
    (232, KIND_LONG), // CURLOPT_SSL_VERIFYSTATUS
    (233, KIND_LONG), // CURLOPT_SSL_FALSESTART
    (234, KIND_LONG), // CURLOPT_PATH_AS_IS
    (237, KIND_LONG), // CURLOPT_PIPEWAIT
    (239, KIND_LONG), // CURLOPT_STREAM_WEIGHT
    (242, KIND_LONG), // CURLOPT_TFTP_NO_OPTIONS
    (244, KIND_LONG), // CURLOPT_TCP_FASTOPEN
    (245, KIND_LONG), // CURLOPT_KEEP_SENDING_ON_ERROR
    (248, KIND_LONG), // CURLOPT_PROXY_SSL_VERIFYPEER
    (249, KIND_LONG), // CURLOPT_PROXY_SSL_VERIFYHOST
    (250, KIND_LONG), // CURLOPT_PROXY_SSLVERSION
    (261, KIND_LONG), // CURLOPT_PROXY_SSL_OPTIONS
    (265, KIND_LONG), // CURLOPT_SUPPRESS_CONNECT_HEADERS
    (267, KIND_LONG), // CURLOPT_SOCKS5_AUTH
    (268, KIND_LONG), // CURLOPT_SSH_COMPRESSION
    (271, KIND_LONG), // CURLOPT_HAPPY_EYEBALLS_TIMEOUT_MS
    (274, KIND_LONG), // CURLOPT_HAPROXYPROTOCOL
    (275, KIND_LONG), // CURLOPT_DNS_SHUFFLE_ADDRESSES
    (278, KIND_LONG), // CURLOPT_DISALLOW_USERNAME_IN_URL
    (280, KIND_LONG), // CURLOPT_UPLOAD_BUFFERSIZE
    (281, KIND_LONG), // CURLOPT_UPKEEP_INTERVAL_MS
    (285, KIND_LONG), // CURLOPT_HTTP09_ALLOWED
    (286, KIND_LONG), // CURLOPT_ALTSVC_CTRL
    (288, KIND_LONG), // CURLOPT_MAXAGE_CONN
    (290, KIND_LONG), // CURLOPT_MAIL_RCPT_ALLLOWFAILS
    (299, KIND_LONG), // CURLOPT_HSTS_CTRL
    (306, KIND_LONG), // CURLOPT_DOH_SSL_VERIFYPEER
    (307, KIND_LONG), // CURLOPT_DOH_SSL_VERIFYHOST
    (308, KIND_LONG), // CURLOPT_DOH_SSL_VERIFYSTATUS
    (314, KIND_LONG), // CURLOPT_MAXLIFETIME_CONN
    (315, KIND_LONG), // CURLOPT_MIME_OPTIONS
    (320, KIND_LONG), // CURLOPT_WS_OPTIONS
    (321, KIND_LONG), // CURLOPT_CA_CACHE_TIMEOUT
    (322, KIND_LONG), // CURLOPT_QUICK_EXIT
    (326, KIND_LONG), // CURLOPT_TCP_KEEPCNT
    (10001, KIND_STREAM), // CURLOPT_FILE
    (10002, KIND_STRING), // CURLOPT_URL
    (10004, KIND_STRING), // CURLOPT_PROXY
    (10005, KIND_STRING), // CURLOPT_USERPWD
    (10006, KIND_STRING), // CURLOPT_PROXYUSERPWD
    (10007, KIND_STRING), // CURLOPT_RANGE
    (10009, KIND_STREAM), // CURLOPT_INFILE/CURLOPT_READDATA
    (10015, KIND_STRING), // CURLOPT_POSTFIELDS
    (10016, KIND_STRING), // CURLOPT_REFERER
    (10017, KIND_STRING), // CURLOPT_FTPPORT
    (10018, KIND_STRING), // CURLOPT_USERAGENT
    (10022, KIND_STRING), // CURLOPT_COOKIE
    (10023, KIND_SLIST), // CURLOPT_HTTPHEADER
    (10025, KIND_STRING), // CURLOPT_SSLCERT
    (10026, KIND_STRING), // CURLOPT_KEYPASSWD/CURLOPT_SSLCERTPASSWD/CURLOPT_SSLKEYPASSWD
    (10028, KIND_SLIST), // CURLOPT_QUOTE
    (10029, KIND_STREAM), // CURLOPT_WRITEHEADER
    (10031, KIND_STRING), // CURLOPT_COOKIEFILE
    (10036, KIND_STRING), // CURLOPT_CUSTOMREQUEST
    (10037, KIND_STREAM), // CURLOPT_STDERR
    (10039, KIND_SLIST), // CURLOPT_POSTQUOTE
    (10062, KIND_STRING), // CURLOPT_INTERFACE
    (10063, KIND_STRING), // CURLOPT_KRB4LEVEL/CURLOPT_KRBLEVEL
    (10065, KIND_STRING), // CURLOPT_CAINFO
    (10070, KIND_SLIST), // CURLOPT_TELNETOPTIONS
    (10076, KIND_STRING), // CURLOPT_RANDOM_FILE
    (10077, KIND_STRING), // CURLOPT_EGDSOCKET
    (10082, KIND_STRING), // CURLOPT_COOKIEJAR
    (10083, KIND_STRING), // CURLOPT_SSL_CIPHER_LIST
    (10086, KIND_STRING), // CURLOPT_SSLCERTTYPE
    (10087, KIND_STRING), // CURLOPT_SSLKEY
    (10088, KIND_STRING), // CURLOPT_SSLKEYTYPE
    (10089, KIND_STRING), // CURLOPT_SSLENGINE
    (10093, KIND_SLIST), // CURLOPT_PREQUOTE
    (10097, KIND_STRING), // CURLOPT_CAPATH
    (10100, KIND_SHARE), // CURLOPT_SHARE
    (10102, KIND_STRING), // CURLOPT_ACCEPT_ENCODING/CURLOPT_ENCODING
    (10103, KIND_PHP_LAYER), // CURLOPT_PRIVATE
    (10104, KIND_SLIST), // CURLOPT_HTTP200ALIASES
    (10118, KIND_STRING), // CURLOPT_NETRC_FILE
    (10134, KIND_STRING), // CURLOPT_FTP_ACCOUNT
    (10135, KIND_STRING), // CURLOPT_COOKIELIST
    (10147, KIND_STRING), // CURLOPT_FTP_ALTERNATIVE_TO_USER
    (10152, KIND_STRING), // CURLOPT_SSH_PUBLIC_KEYFILE
    (10153, KIND_STRING), // CURLOPT_SSH_PRIVATE_KEYFILE
    (10162, KIND_STRING), // CURLOPT_SSH_HOST_PUBLIC_KEY_MD5
    (10169, KIND_STRING), // CURLOPT_CRLFILE
    (10170, KIND_STRING), // CURLOPT_ISSUERCERT
    (10173, KIND_STRING), // CURLOPT_USERNAME
    (10174, KIND_STRING), // CURLOPT_PASSWORD
    (10175, KIND_STRING), // CURLOPT_PROXYUSERNAME
    (10176, KIND_STRING), // CURLOPT_PROXYPASSWORD
    (10177, KIND_STRING), // CURLOPT_NOPROXY
    (10179, KIND_STRING), // CURLOPT_SOCKS5_GSSAPI_SERVICE
    (10183, KIND_STRING), // CURLOPT_SSH_KNOWNHOSTS
    (10186, KIND_STRING), // CURLOPT_MAIL_FROM
    (10187, KIND_SLIST), // CURLOPT_MAIL_RCPT
    (10190, KIND_STRING), // CURLOPT_RTSP_SESSION_ID
    (10191, KIND_STRING), // CURLOPT_RTSP_STREAM_URI
    (10192, KIND_STRING), // CURLOPT_RTSP_TRANSPORT
    (10203, KIND_SLIST), // CURLOPT_RESOLVE
    (10204, KIND_STRING), // CURLOPT_TLSAUTH_USERNAME
    (10205, KIND_STRING), // CURLOPT_TLSAUTH_PASSWORD
    (10206, KIND_STRING), // CURLOPT_TLSAUTH_TYPE
    (10211, KIND_STRING), // CURLOPT_DNS_SERVERS
    (10217, KIND_STRING), // CURLOPT_MAIL_AUTH
    (10220, KIND_STRING), // CURLOPT_XOAUTH2_BEARER
    (10221, KIND_STRING), // CURLOPT_DNS_INTERFACE
    (10222, KIND_STRING), // CURLOPT_DNS_LOCAL_IP4
    (10223, KIND_STRING), // CURLOPT_DNS_LOCAL_IP6
    (10224, KIND_STRING), // CURLOPT_LOGIN_OPTIONS
    (10228, KIND_SLIST), // CURLOPT_PROXYHEADER
    (10230, KIND_STRING), // CURLOPT_PINNEDPUBLICKEY
    (10231, KIND_STRING), // CURLOPT_UNIX_SOCKET_PATH
    (10235, KIND_STRING), // CURLOPT_PROXY_SERVICE_NAME
    (10236, KIND_STRING), // CURLOPT_SERVICE_NAME
    (10238, KIND_STRING), // CURLOPT_DEFAULT_PROTOCOL
    (10243, KIND_SLIST), // CURLOPT_CONNECT_TO
    (10246, KIND_STRING), // CURLOPT_PROXY_CAINFO
    (10247, KIND_STRING), // CURLOPT_PROXY_CAPATH
    (10251, KIND_STRING), // CURLOPT_PROXY_TLSAUTH_USERNAME
    (10252, KIND_STRING), // CURLOPT_PROXY_TLSAUTH_PASSWORD
    (10253, KIND_STRING), // CURLOPT_PROXY_TLSAUTH_TYPE
    (10254, KIND_STRING), // CURLOPT_PROXY_SSLCERT
    (10255, KIND_STRING), // CURLOPT_PROXY_SSLCERTTYPE
    (10256, KIND_STRING), // CURLOPT_PROXY_SSLKEY
    (10257, KIND_STRING), // CURLOPT_PROXY_SSLKEYTYPE
    (10258, KIND_STRING), // CURLOPT_PROXY_KEYPASSWD
    (10259, KIND_STRING), // CURLOPT_PROXY_SSL_CIPHER_LIST
    (10260, KIND_STRING), // CURLOPT_PROXY_CRLFILE
    (10262, KIND_STRING), // CURLOPT_PRE_PROXY
    (10263, KIND_STRING), // CURLOPT_PROXY_PINNEDPUBLICKEY
    (10264, KIND_STRING), // CURLOPT_ABSTRACT_UNIX_SOCKET
    (10266, KIND_STRING), // CURLOPT_REQUEST_TARGET
    (10276, KIND_STRING), // CURLOPT_TLS13_CIPHERS
    (10277, KIND_STRING), // CURLOPT_PROXY_TLS13_CIPHERS
    (10279, KIND_STRING), // CURLOPT_DOH_URL
    (10287, KIND_STRING), // CURLOPT_ALTSVC
    (10289, KIND_STRING), // CURLOPT_SASL_AUTHZID
    (10296, KIND_STRING), // CURLOPT_PROXY_ISSUERCERT
    (10298, KIND_STRING), // CURLOPT_SSL_EC_CURVES
    (10300, KIND_STRING), // CURLOPT_HSTS
    (10305, KIND_STRING), // CURLOPT_AWS_SIGV4
    (10311, KIND_STRING), // CURLOPT_SSH_HOST_PUBLIC_KEY_SHA256
    (10318, KIND_STRING), // CURLOPT_PROTOCOLS_STR
    (10319, KIND_STRING), // CURLOPT_REDIR_PROTOCOLS_STR
    (10328, KIND_STRING), // CURLOPT_SSL_SIGNATURE_ALGORITHMS
    (19913, KIND_PHP_LAYER), // CURLOPT_RETURNTRANSFER
    (19914, KIND_PHP_LAYER), // CURLOPT_BINARYTRANSFER
    (20011, KIND_CALLBACK), // CURLOPT_WRITEFUNCTION
    (20012, KIND_CALLBACK), // CURLOPT_READFUNCTION
    (20056, KIND_CALLBACK), // CURLOPT_PROGRESSFUNCTION
    (20079, KIND_CALLBACK), // CURLOPT_HEADERFUNCTION
    (20094, KIND_CALLBACK), // CURLOPT_DEBUGFUNCTION
    // STILL REJECTED (a deliberate second wave, not an oversight): each of these needs
    // machinery the first wave does not build. FNMATCH/PREREQ/SSH_HOSTKEY/TRAILER/
    // HSTS read+write need transfer phases outside the landed callback matrix, and
    // SSL_CTX_FUNCTION/OPENSOCKET/SOCKOPT hand PHP a raw native pointer (an
    // `SSL_CTX *`, a socket) that this build has no PHP-visible type for.
    // NOTE ON SSH_HOSTKEY: it is the CALLBACK that is unbuilt, not the protocol — since
    // curl recipe revision 2 this build carries libssh2, so `scp://`/`sftp://` and the
    // whole `CURLOPT_SSH_*` option family below are live.
    (20200, KIND_UNSUPPORTED), // CURLOPT_FNMATCH_FUNCTION
    (20219, KIND_CALLBACK), // CURLOPT_XFERINFOFUNCTION
    (20312, KIND_UNSUPPORTED), // CURLOPT_PREREQFUNCTION
    (20316, KIND_UNSUPPORTED), // CURLOPT_SSH_HOSTKEYFUNCTION
    (30115, KIND_OFF_T), // CURLOPT_INFILESIZE_LARGE
    (30117, KIND_OFF_T), // CURLOPT_MAXFILESIZE_LARGE
    (30145, KIND_OFF_T), // CURLOPT_MAX_SEND_SPEED_LARGE
    (30146, KIND_OFF_T), // CURLOPT_MAX_RECV_SPEED_LARGE
    (30270, KIND_OFF_T), // CURLOPT_TIMEVALUE_LARGE
    (40291, KIND_UNSUPPORTED), // CURLOPT_SSLCERT_BLOB
    (40292, KIND_UNSUPPORTED), // CURLOPT_SSLKEY_BLOB
    (40293, KIND_UNSUPPORTED), // CURLOPT_PROXY_SSLCERT_BLOB
    (40294, KIND_UNSUPPORTED), // CURLOPT_PROXY_SSLKEY_BLOB
    (40295, KIND_UNSUPPORTED), // CURLOPT_ISSUERCERT_BLOB
    (40297, KIND_UNSUPPORTED), // CURLOPT_PROXY_ISSUERCERT_BLOB
    (40309, KIND_UNSUPPORTED), // CURLOPT_CAINFO_BLOB
    (40310, KIND_UNSUPPORTED), // CURLOPT_PROXY_CAINFO_BLOB
];

/// Classifies `opt` for `curl_setopt()`. Returns [`KIND_INVALID`] for any number not in
/// [`OPTION_KINDS`].
pub(crate) fn option_kind(opt: i32) -> i32 {
    match OPTION_KINDS.binary_search_by_key(&opt, |&(number, _)| number) {
        Ok(index) => OPTION_KINDS[index].1,
        Err(_) => KIND_INVALID,
    }
}
