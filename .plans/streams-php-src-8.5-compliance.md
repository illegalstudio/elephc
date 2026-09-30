# PHP 8.5 Streams and CSV Compliance Specification

## Checklist

- [x] Freeze the normative php-src revision and the Elephc audit revision.
- [x] Inventory the public stream, stream-backed file, CSV, wrapper, transport, filter, context, and socket surfaces.
- [x] Record verified semantic contradictions in the current PR.
- [x] Define the runtime architecture required to represent PHP stream state.
- [x] Define implementation gates and per-gate acceptance evidence.
- [x] Define differential, PHPT, failure-injection, and supported-target test policy.
- [x] Define exact three-reviewer consensus and companion-evidence protocol.
- [x] Keep implementation outside this specification phase.

## Status

This document is an audit and implementation specification only. It does not
authorize implementation. The current phase ends only when GLM 5.2, Kimi K2.7,
and MiniMax M3 independently return `LOCK` for the same SHA-256 of this file.

The earlier model-review claim in PR #638 is not an acceptance artifact for
this specification. It reviewed a different and demonstrably incomplete state.

## Frozen references

### Normative PHP baseline

- PHP release: 8.5.6.
- php-src tag: `php-8.5.6`.
- php-src commit: `fcc29c8d6d6ee6f5ba2d941f0a2a6ea6aa6ee633`.
- Primary source tree:
  <https://github.com/php/php-src/tree/fcc29c8d6d6ee6f5ba2d941f0a2a6ea6aa6ee633>.
- The `PHP-8.5` branch tip may be monitored for drift, but it is not allowed to
  silently change this acceptance baseline.
- Elephc's default PHP profile is 8.5. Where Elephc exposes its existing PHP
  8.2, 8.3, or 8.4 profiles, version-specific signatures, deprecations, errors,
  constants, and behavior must follow the corresponding php-src release branch.

The authoritative php-src inputs are:

- `ext/standard/basic_functions.stub.php`
- `ext/standard/file.stub.php`
- `ext/standard/dir.stub.php`
- `ext/standard/user_filters.stub.php`
- `ext/standard/file.c`
- `ext/standard/dir.c`
- `ext/standard/streamsfuncs.c`
- `ext/standard/streamsfuncs.h`
- `ext/standard/php_fopen_wrapper.c`
- `ext/standard/http_fopen_wrapper.c`
- `ext/standard/ftp_fopen_wrapper.c`
- `main/php_streams.h`
- `main/streams/streams.c`
- `main/streams/filter.c`
- `main/streams/userspace.c`
- `main/streams/plain_wrapper.c`
- `main/streams/memory.c`
- `main/streams/xp_socket.c`
- the applicable stream-wrapper sources under `ext/openssl`, `ext/zlib`,
  `ext/bz2`, `ext/phar`, `ext/ftp`, and stream-consuming sources under
  `ext/spl`
- applicable PHPTs under `ext/standard/tests/file`,
  `ext/standard/tests/dir`, `ext/standard/tests/directory`,
  `ext/standard/tests/streams`, `ext/standard/tests/filters`,
  `ext/standard/tests/http`, and `ext/standard/tests/network`, plus
  extension-specific wrapper, filter, and stream-consumer PHPTs

PHP CLI 8.5.6 may be used as an executable oracle. When a local oracle and the
frozen source appear to disagree, the source, its PHPTs, build configuration,
platform guards, and the exact CLI build information must be reconciled before
acceptance.

### Reproducible baseline evidence

The release and dereferenced commit were verified on 2026-07-29 with:

```text
$ git ls-remote --tags https://github.com/php/php-src.git \
    refs/tags/php-8.5.6 'refs/tags/php-8.5.6^{}'
b208d6046a61da2f2dfdd45883ce02d5299d1b22  refs/tags/php-8.5.6
fcc29c8d6d6ee6f5ba2d941f0a2a6ea6aa6ee633  refs/tags/php-8.5.6^{}
```

The following frozen-source anchors resolve common historical-version
ambiguities:

- `fscanf($stream, string $format, mixed &...$vars):
  array|int|false|null` is declared in
  [`basic_functions.stub.php`](https://github.com/php/php-src/blob/fcc29c8d6d6ee6f5ba2d941f0a2a6ea6aa6ee633/ext/standard/basic_functions.stub.php#L2778).
- `stream_filter_prepend()` and `stream_filter_append()` use parameter names
  `$filter_name`, `$mode`, and `$params`, with `$mode = 0`, in
  [`basic_functions.stub.php`](https://github.com/php/php-src/blob/fcc29c8d6d6ee6f5ba2d941f0a2a6ea6aa6ee633/ext/standard/basic_functions.stub.php#L3398-L3412).
- `STREAM_PF_INET6` is a PHP-visible, build-conditional constant in
  [`file.stub.php`](https://github.com/php/php-src/blob/fcc29c8d6d6ee6f5ba2d941f0a2a6ea6aa6ee633/ext/standard/file.stub.php#L289-L305);
  its value is the target's `PF_INET6` or `AF_INET6`.
- `FILE_TEXT = 0` and `FILE_BINARY = 0` are unconditional PHP-visible
  constants, deprecated since PHP 8.1, in
  [`file.stub.php`](https://github.com/php/php-src/blob/fcc29c8d6d6ee6f5ba2d941f0a2a6ea6aa6ee633/ext/standard/file.stub.php#L446-L451).

These anchors are illustrative evidence. The generated manifests remain
authoritative for the complete surface.

### Audited Elephc state

- PR: <https://github.com/illegalstudio/elephc/pull/638>.
- Audited PR head:
  `239ea4aeaac4b1779ec475f455db23002c9da3dd`.
- Audited base:
  `9f74f5300d81007b11099cd93cc0604504b23f75`.
- Worktree:
  `.claude/worktrees/streams-csv-signatures`.
- The worktree is intentionally detached at the PR head.
- The local `feat/streams-csv-signatures` branch points at divergent commit
  `f133b173e75c8b457c969745c26548aa56aba51f`; it must not be reset, switched,
  deleted, or treated as the audited PR head.

Any implementation must first resolve this provenance intentionally and must
show that every integrated commit is on the active implementation branch.

## Meaning of "100% compliant"

Compliance is behavioral parity with the frozen php-src baseline for every
surface within scope. Counts, successful focused tests, generated declarations,
or the absence of compiler diagnostics are evidence, not completion.

For each public symbol and observable operation, parity includes:

1. Symbol existence, case-insensitive function lookup, aliases, classes, public
   properties, methods, and configured availability.
2. Exact parameter order, names for named arguments, optionality, defaults,
   by-reference flags, variadic flags, accepted unions, coercions, and return
   unions.
3. Exact public constant existence and values for the selected PHP version,
   operating system, target, and enabled extensions. Internal php-src constants
   must not become PHP-visible.
4. Bytes returned or written, array shape and keys, metadata fields, resource
   identity, cursor position, buffered state, EOF state, and side effects.
5. Warning, deprecation, notice, `ValueError`, `TypeError`, and fatal behavior,
   including message text where php-src PHPTs assert it and the point at which
   side effects become observable.
6. Reference writes, source evaluation order, error-output ordering, partial
   I/O, retry behavior, blocking behavior, timeouts, and cleanup.
7. Scope and lifetime of variables such as `$http_response_header`.
8. Wrapper, context, filter, transport, and resource lifecycle callbacks.
9. Identical PHP-visible behavior on `macos-aarch64`, `linux-aarch64`, and
   `linux-x86_64`, except for differences that php-src itself makes for the same
   target or build configuration.
10. No Elephc-only behavior under PHP names. Compiler extensions must remain
    explicitly distinguishable from the PHP surface.

An implementation may internally use different syscalls or data structures,
but differences must not leak through PHP-visible behavior.

## Scope

### In scope

- The complete core `stream_*` function surface and its aliases.
- Stream-backed file APIs: `fopen`, `fclose`, `fread`, `fwrite`, `fputs`,
  `fgets`, `fgetc`, `feof`, `fflush`, `fsync`, `fdatasync`, `rewind`, `fseek`,
  `ftell`, `ftruncate`, `fstat`, `flock`, `fpassthru`, `fprintf`, `vfprintf`,
  `fscanf`, `file`, `file_get_contents`, `file_put_contents`, `readfile`,
  `copy`, `rename`, `unlink`, `mkdir`, `rmdir`, `tempnam`, `tmpfile`, `popen`,
  and `pclose`.
- Stream-backed directory and path operations: `opendir`, `readdir`,
  `rewinddir`, `closedir`, `scandir`, `stat`, `lstat`, the `file*` stat
  helpers, `touch`, `chmod`, `chown`, `chgrp`, `link`, `symlink`, `readlink`,
  `realpath`, `glob`, and php-src aliases.
- CSV stream behavior: `fgetcsv` and `fputcsv`.
- SPL consumers whose frozen implementation is backed by the stream engine,
  including the complete `SplFileObject` and `SplTempFileObject` stream/CSV
  contracts. The Gate 0 reachability manifest decides whether other SPL
  classes or methods are included; class names alone do not.
- Stream contexts, default context state, context parameters, notification
  callbacks, and per-wrapper options.
- Built-in wrappers that Elephc advertises, including every supported
  `php://` form, `file://`, `data://`, HTTP/HTTPS, FTP/FTPS, glob, Phar, ZIP,
  zlib, and bzip2 as applicable to the enabled build.
- User wrappers and all callbacks supported by php-src 8.5.6.
- Built-in and user filters, buckets, brigades, chains, flush behavior, and
  filter resources.
- Socket streams, transports, `stream_select`, blocking, timeouts, TLS, and
  socket metadata, including `fsockopen`, `pfsockopen`, and the stream aliases
  `socket_get_status`, `socket_set_blocking`, and `socket_set_timeout`.
- Core aliases including `set_file_buffer` and `stream_register_wrapper`.
- HTTP-stream state exposed by `http_get_last_response_headers()` and
  `http_clear_last_response_headers()`.
- Interaction with include paths, URL policy, web request state, ownership,
  copy-on-write, cleanup, and compiler optimizations.
- Every other frozen php-src API that reaches `php_stream`, a stream wrapper,
  a stream context, or a stream filter, even when its public name does not
  contain `stream` or `file`. Gate 0 must generate this source-reachability
  inventory; the lists above are minimum named surfaces, not an escape hatch.

### Configuration-scoped surfaces

php-src lists wrappers, transports, and filters according to its build. Elephc
must define an explicit build/profile manifest at
`tests/php_oracle/manifests/streams/php-8.5.6/<target>/<build-profile>.json`.
The manifest records the exact target, feature/bridge set, php-src build flags,
PHP-visible symbols, constants, wrappers, transports, filters, crypto methods,
and source commit. A wrapper, transport, filter, or crypto method that is
advertised is fully in scope; an unavailable optional extension must not be
advertised. Merely accepting options as no-ops is not compliance.

### Out of scope

- Windows-only `sapi_windows_vt100_support()` while Windows is not in Elephc's
  supported target matrix.
- Unrelated parsing performed after bytes have been read, such as the HTML
  extraction semantics of `get_meta_tags()`. Its use of streams, wrappers,
  contexts, errors, and lifecycle must still go through the compliant stream
  layer if the function exists.
- Compiler-specific stream extensions that use non-PHP names and cannot be
  mistaken for PHP behavior.

These exclusions do not permit a public PHP symbol to be present with partial
or incompatible behavior.

## Frozen public prototype inventory

The implementation must generate and test an exact machine-readable manifest
from the frozen stubs. At minimum, the following audited differences must be
fixed:

| Function | php-src 8.5.6 requirement | Audited PR problem |
|---|---|---|
| `copy` | third `$context = null` | context absent |
| `fgets` | `?int $length = null` | length absent |
| `file` | `$flags = 0, $context = null` | both absent |
| `fwrite` / `fputs` | `?int $length = null`, `int|false` | optional length and/or alias missing |
| `fscanf` | `mixed &...$vars`, `array|int|false|null` | variadic-reference and return contract differ |
| `mkdir` | permissions, recursive, context | incomplete |
| `rename`, `rmdir`, `unlink` | context | context absent |
| `stream_context_set_option` | array overload or four-argument form; returns `true` | declaration/default/type mismatch |
| `stream_context_set_options` | public two-argument alias; returns `true` | missing |
| `stream_copy_to_stream` | `?int $length = null, int $offset = 0` | offset default differs |
| `stream_filter_append` / `prepend` | `$mode = 0`, exact parameter names | default is `3`; names differ |
| `stream_select` | nullable by-ref arrays, nullable seconds, nullable microseconds | nullability/defaults differ |
| `stream_socket_client` | six parameters; timeout `?float`; flags default `4`; context last | bogus seventh peer-name parameter; types/defaults differ |
| `stream_socket_server` | five parameters; flags default `12`; context last | bogus sixth peer-name parameter |
| `stream_socket_recvfrom` | address by reference, default `null` | type/default differ |
| `socket_set_block` | ext/sockets function takes one `Socket`; it is not a stream alias | audited Elephc declaration conflicts with the PHP symbol |
| `socket_set_blocking` | stream alias takes stream plus boolean | must remain coherent with `stream_set_blocking` |

The machine-readable prototype gate must also cover all false/null/true unions
and resource-like parameters that are imprecise in the current compiler model.
If Elephc's internal type system cannot express a PHP stub type, it must gain an
equivalent checker and lowering contract; silently widening or narrowing is not
accepted.

Named-argument behavior must use the exact stub parameter names. Direct calls,
namespaced fallback, first-class callable syntax, aliases, and runtime-callable
dispatch must share the same signature.

The same manifest gate applies to stream-facing classes. It includes the exact
methods, properties, visibility, defaults, aliases, inheritance, and behavior
of `php_user_filter`, `StreamBucket`, `SplFileObject`, and
`SplTempFileObject` wherever the frozen build exposes them.

## Verified blockers in PR #638

The following are source- or oracle-verified contradictions, not speculative
implementation suggestions.

### Constants

- php-src 8.5.6 defines `STREAM_CLIENT_PERSISTENT = 1`,
  `STREAM_CLIENT_ASYNC_CONNECT = 2`, and `STREAM_CLIENT_CONNECT = 4`.
  The audited PR uses `2`, `4`, and `1` respectively.
- The PR exposes internal-only names such as `STREAM_FROM_START`,
  `STREAM_FROM_CUR`, `STREAM_FROM_END`, `STREAM_META_MODIFIED`, and
  `STREAM_OPTION_CHUNK_SIZE`. They are not public PHP constants.
- Public `FILE_BINARY = 0` and `FILE_TEXT = 0` must be included where php-src
  exposes them.
- Target-dependent constants, including `STREAM_PF_INET6`, must follow php-src
  on each supported operating system.

The acceptance gate is exact set equality and exact value equality for the
selected profile and build, not a hand-maintained subset.

### Generated signatures without behavior

- `file_get_contents()` lowering consumes only the filename and ignores
  include-path, context, offset, and length behavior.
- `file_put_contents()` lowering consumes path and data while ignoring flags and
  context, including `FILE_APPEND` and `LOCK_EX`.
- `stream_socket_client()` consumes only the address; error references, timeout,
  flags, and context are ignored.
- Broadened declarations therefore do not establish compliance.

Every parameter must have an observable php-src-equivalent implementation and
tests proving normal, boundary, and error paths.

### Resource identity and fixed tables

- Current resources frequently expose raw OS file descriptors as PHP resource
  identity.
- Stream metadata, EOF, timeouts, filters, wrappers, and contexts are stored in
  parallel fixed-size tables, commonly capped at 256 entries.
- Context handles wrap after 16 allocations and can overwrite live state.
- User-wrapper handles and raw descriptors use separate ad hoc ranges.
- Metadata mode is reconstructed from descriptor access flags, losing the
  original PHP mode.
- URI storage and table slots do not establish safe ownership, complete cleanup,
  or stale-handle protection.

This representation cannot satisfy arbitrary filter chains, non-FD streams,
descriptor reuse, exact metadata, or normal process descriptor counts.

### Contexts

- `stream_context_get_params()` currently lowers to an empty associative array.
- `stream_context_set_params()` accepts an update without applying the complete
  parameter state.
- Only a small subset of options is operational. Tests that accept TLS or HTTP
  options as no-ops contradict the compliance target.
- Default-context merging, resource identity, copy/replace rules, notification
  callbacks, and `$this->context` on user wrappers are incomplete.

### Open modes and wrapper dispatch

- The raw file opener implements only first-character `r`, `w`, and `a` with a
  limited second-character `+` check. PHP modes `x`, `x+`, `c`, `c+`, and valid
  modifier orderings are not represented completely.
- Several built-in wrapper paths are selected only when the filename is a
  compile-time literal. Dynamic strings must select the same runtime wrapper.
- Literal `php://memory` and `php://temp` are treated as the same temporary-file
  path, although php-src has distinct memory and spill-to-file semantics.
- `php://filter` parsing is literal-only and does not represent arbitrary
  dynamic chains.
- Advertised HTTPS, FTPS, compression, Phar, ZIP, glob, and data behavior must
  be executable through the generic runtime wrapper registry.

### Metadata and buffering

`stream_get_meta_data()` currently synthesizes a small descriptor-based view:

- `timed_out` is effectively constant.
- `unread_bytes` is not driven by the actual read buffer.
- `stream_type`, `wrapper_type`, `wrapper_data`, `crypto`, `seekable`, `mode`,
  `uri`, `blocked`, and `eof` are not sourced from a complete stream object.
- Non-FD, memory, temporary, HTTP, compression, TLS, and user-wrapper streams
  cannot be described correctly.

Buffer-size, chunk-size, blocking, timeout, cursor, EOF, and read-ahead changes
must affect subsequent operations and metadata exactly as in php-src.

`stream_is_local()` must accept and distinguish both a stream resource and a
string path as php-src does; a resource-only checker contract is not compliant.

### CSV

`fgetcsv()` currently reads one physical line, ignores the length contract, and
uses low bytes from delimiter arguments. This fails php-src behavior for:

- quoted fields spanning physical lines;
- EOF versus blank-line distinction;
- blank lines, which produce `[null]` in PHP 8.5.6;
- CR, LF, and CRLF handling;
- NUL and arbitrary binary bytes;
- empty or multi-byte separator/enclosure/escape validation;
- malformed or unterminated quoted fields;
- array-or-false return typing.

`fputcsv()` currently assumes an indexed string array and uses a non-php-src
escaping algorithm. PHP field coercion, warnings, `Stringable` values, enclosure
selection, escape-state handling, partial write/failure behavior, and return
length must match php-src. Verified PHP 8.5.6 behavior includes:

- a quote in an enclosed field is doubled unless php-src's escape-state rule
  says it is already escaped;
- escape characters are not generically doubled;
- an explicitly empty `$eol` writes no line terminator;
- omitting `$escape` emits the PHP 8.4+ deprecation:
  `fgetcsv(): the $escape parameter must be provided as its default value will change`
  or the corresponding `fputcsv()` message;
- separator and enclosure must be exactly one byte; escape must be empty or
  exactly one byte, with php-src-equivalent `ValueError` behavior.

CSV tests must be byte-exact and must run through plain files, memory/temp
streams, user wrappers, filtered streams, and failure-injected writers.

### Filters

- The current stream holds at most two user-filter slots rather than linked,
  arbitrarily sized read and write chains.
- `PSFS_ERR_FATAL` and `PSFS_FEED_ME` are treated as successful pass-through in
  branch tests. php-src stops the chain for either non-`PSFS_PASS_ON` status;
  `FEED_ME` buffers/requests more data and `ERR_FATAL` fails the operation.
- Bucket brigades, `$consumed`, append/prepend order, mode auto-selection,
  flush flags, filter removal, `onCreate`, `filter`, and `onClose` lifecycle are
  incomplete.
- The advertised filter list differs from the configured php-src list and
  includes names whose behavior is not established.

The runtime must support arbitrary chain length and opaque filter resources.
All built-in and user filters use the same bucket/brigade protocol.

### `stream_select`

The current implementation uses a fixed-capacity `poll` translation and:

- assumes three non-null arrays;
- returns zero when no descriptors are passed instead of raising the php-src
  `ValueError`;
- truncates timeout precision to milliseconds;
- does not model already-buffered readable data;
- has a 256-descriptor cap;
- does not establish php-src key preservation, reference mutation, warning, and
  false-return behavior.

Using `poll`, `select`, `kqueue`, or another facility is permitted only if the
PHP-visible contract, including buffered streams and microsecond behavior,
matches php-src.

### Socket streams and TLS

- Client/server flags and argument lists are wrong, and by-reference
  `$error_code`/`$error_message` outputs are not implemented.
- Persistent and asynchronous client flags, connect/listen combinations,
  timeouts, nonblocking progress, address parsing, peer-name formatting,
  datagrams, Unix sockets, IPv4, and IPv6 require differential coverage.
- Receive/send flags and reference outputs, shutdown validation, socket pairs,
  partial I/O, and transport error mapping require parity.
- `stream_socket_enable_crypto()` must preserve php-src's `true`, `false`, and
  `0` outcomes, context-driven crypto methods, verification behavior, session
  streams, warnings, and metadata.

No transport or crypto method may be advertised when it is a stub or accepted
no-op.

### User wrappers

The visible method-name inventory is not sufficient. The implementation must
match the php-src callback protocol for:

- `stream_open`, `stream_close`, `stream_read`, `stream_write`, `stream_flush`,
  `stream_seek`, `stream_tell`, `stream_eof`, `stream_stat`, `url_stat`;
- `unlink`, `rename`, `mkdir`, `rmdir`;
- `dir_opendir`, `dir_readdir`, `dir_rewinddir`, `dir_closedir`;
- `stream_lock`, `stream_cast`, `stream_set_option`, `stream_truncate`, and
  `stream_metadata`.

This includes exact callback arguments and reference behavior, method-absence
fallbacks and warnings, return coercion, state lifetime, `$this->context`,
opened-path updates, stat arrays, directory iteration, protocol validation,
`STREAM_IS_URL`, registration/unregistration/restore behavior, and wrapper
replacement rules.

Hard-coded limits on wrapper registrations or live handles are not accepted.

### HTTP state

- php-src assigns `$http_response_header` in the local scope that initiated the
  HTTP wrapper operation. It is not a superglobal.
- PHP 8.5.6 also exposes request-local
  `http_get_last_response_headers(): ?array` and
  `http_clear_last_response_headers(): void`; neither symbol is present in the
  audited Elephc graph.
- The PR stores it in global program state and initializes it independently of a
  successful HTTP wrapper operation.
- Only a subset of literal HTTP opens populates it.

All stream-opening HTTP APIs must implement local-scope creation, undefined
state before use, redirect response chains, status lines, repeated headers,
folding rules, failure paths, nested calls, and request isolation. There must be
no global or cross-request leakage.

HTTP context behavior must include applicable method, protocol version, header,
user agent, content, proxy, request-full-URI, timeout, redirect, max-redirect,
ignore-errors, authentication, notification, decoding, and TLS options.

### Registry introspection

`stream_get_wrappers()`, `stream_get_transports()`, and
`stream_get_filters()` must expose exactly the names registered by the active
build, in php-src-equivalent order where observable. Runtime registration,
unregistration, and restoration must update introspection immediately.

Concrete names must not replace php-src wildcard families, and unsupported
names such as a non-public or removed filter must not be added for
"completeness".

## Required runtime architecture

Implementation must replace raw-descriptor identity and parallel bounded tables
with one authoritative, dynamically allocated stream registry.

Each PHP stream resource resolves through an opaque, generational handle to a
`StreamHandle`-equivalent object containing at least:

- backend kind and backend-owned state;
- optional OS descriptor or socket, never used as PHP resource identity;
- exact original mode and read/write/append capabilities;
- wrapper identity, URI, wrapper data, and wrapper-owned state;
- context resource/reference, effective options, parameters, and notifier;
- read buffer, write buffer, positions, unread byte count, EOF, blocked state,
  timeout state, timeout values, and chunk size;
- separate arbitrary-length read and write filter chains;
- metadata and TLS/crypto session state;
- seek, stat, cast, lock, truncate, and option capabilities;
- ownership, reference count, close state, cleanup hooks, and request lifetime.

The registry must provide:

- dynamic capacity without PHP-visible 16/64/256 limits;
- stale-handle rejection through generations or an equivalent mechanism;
- deterministic close and request-shutdown cleanup;
- safe descriptor reuse;
- balanced ownership on success, early return, warning, fatal, and partial
  construction;
- one dispatch path for literal and dynamic URIs;
- target-independent semantics with target-aware backend operations.

Memory streams, temporary streams, user wrappers, filters, and contexts must not
pretend to be ordinary file descriptors. `php://temp/maxmemory:<bytes>` must
spill according to php-src behavior, whereas `php://memory` remains memory
backed.

`Mixed` in this specification means Elephc's internal boxed heterogeneous PHP
value cell, including its tag, payload, ownership, and copy-on-write contract;
it does not mean an unconstrained raw machine scalar.

## Audit closure map

| Audited blocker family | Primary closing gates |
|---|---|
| public symbols, signatures, aliases, constants, and version deltas | Gates 0 and 2 |
| raw-FD identity, fixed tables, stale handles, and cleanup | Gates 1 and 13 |
| core I/O parameters, modes, buffering, metadata, and partial failures | Gates 3 and 12 |
| context handles, options, params, defaults, and notifications | Gate 4 |
| literal-only dispatch, `php://`, and built-in wrapper introspection | Gate 5 |
| user-wrapper callbacks, limits, and lifecycle | Gate 6 |
| two-slot filters, status handling, buckets, and advertised filters | Gate 7 |
| CSV parsing, formatting, coercion, validation, and deprecation | Gate 8 |
| socket signatures, references, transports, and `stream_select` | Gate 9 |
| HTTP/FTP contexts and response-header scope/state | Gate 10 |
| TLS and compression behavior advertised by optional bridges | Gate 11 |
| optimizer, ownership, GC, and request isolation | Gate 13 |
| PHPT completeness, supported targets, docs, and PR hygiene | Gate 14 |

## Implementation gates

Gates are sequential integration boundaries. A later gate may be researched in
parallel, but it cannot be declared accepted on top of an unaccepted dependency.
Each gate needs a regression that fails at the audited PR head.

### Gate 0 — frozen manifests and oracle harness

- Generate checked-in manifests for functions, aliases, parameters, constants,
  classes, wrapper callbacks, configured wrappers, transports, filters, and
  crypto methods from the frozen source/build.
- Generate a checked-in source-reachability manifest for every public php-src
  API that calls or reaches `php_stream`, wrapper, context, transport, or filter
  operations, including extension classes and methods, so non-obvious consumers
  cannot be silently omitted.
- Add a differential harness that captures stdout, stderr, exit status,
  exception class/message, warning/deprecation order, return serialization,
  reference outputs, metadata, and output bytes.
- Record PHP version, build configuration, target, locale, timezone, INI, and
  enabled extensions in every oracle artifact.
- Fail on unclassified additions, removals, or value/signature drift.

Acceptance: exact manifest parity for the selected profile and a reproducible
oracle corpus with no hand-copied expected values where PHP can generate them.

### Gate 1 — authoritative resource registry and lifecycle

- Introduce the opaque dynamic stream/context/filter resource model.
- Route close, request shutdown, descriptor reuse, metadata, and ownership
  through it.
- Remove all PHP-visible fixed capacities and raw-FD resource identity.
- Add stale-handle, double-close, close-on-error, more-than-256-live-stream,
  more-than-16-context, descriptor-reuse, and web-request-reset tests.

Acceptance: no stream semantics depend on an external parallel table indexed by
an OS descriptor.

### Gate 2 — declarations, constants, aliases, and diagnostics

- Make the complete public manifest exact for PHP 8.5.6.
- Implement version-profile deltas.
- Make named arguments, aliases, first-class callables, and runtime-callable
  dispatch coherent.
- Remove non-public constants and add missing public constants.
- Add argument count/type/value, invalid-resource, invalid-mode, and named-
  argument differential tests.

Acceptance: manifest equality plus zero differential mismatches for declaration
and validation probes.

### Gate 3 — core open, I/O, positioning, buffering, and metadata

- Implement all PHP file modes and modifier validation.
- Implement exact read/write/line/contents/copy semantics, offsets, lengths,
  partial operations, EOF, seek, tell, flush, truncate, locks, stat, chunk
  sizes, read/write buffers, timeouts, and blocking state.
- Preserve exact mode, URI, cursor, buffer, and metadata.
- Inject short reads/writes, EINTR-equivalent retry cases, EAGAIN, EOF, close,
  and backend errors.

Acceptance: core file/stream differential corpus and applicable file PHPT
manifest are green on the host target.

### Gate 4 — contexts, defaults, parameters, and notifications

- Implement independent context resources, merge/replace rules, default-context
  state, options, parameters, notification callbacks, and wrapper attachment.
- Make every supported option behavioral; reject or omit unsupported advertised
  options according to php-src.
- Verify callback event, severity, message, code, transferred bytes, maximum
  bytes, ordering, reentrancy, and cleanup.

Acceptance: context APIs round-trip exact nested arrays and all consumers observe
the same effective context.

### Gate 5 — built-in wrapper registry and `php://`

- Route literal and dynamic URIs through the same registry.
- Implement configured `file://`, `php://memory`, `php://temp`,
  `php://temp/maxmemory`, `php://input`, `php://output`, standard streams,
  `php://fd`, and `php://filter`.
- Enforce mode, seekability, spill, duplication, ownership, close semantics,
  process-stream behavior, `stream_isatty()`, and target-equivalent TTY
  behavior.
- Implement `data://`, glob, Phar, ZIP, and other advertised built-ins according
  to their frozen source and enabled-extension manifest.

Acceptance: introspection matches the build and every advertised wrapper passes
its wrapper-specific differential/PHPT corpus for literal and dynamic paths.

### Gate 6 — user wrappers

- Implement the full php-src user-wrapper protocol listed above.
- Support arbitrary registrations and live handles.
- Integrate context, stat/cache invalidation, directory operations, metadata,
  locking, truncation, casting, and generic stream consumers.

Acceptance: applicable php-src userspace-wrapper PHPTs and adversarial lifecycle
tests pass without wrapper-specific shortcuts in file/CSV functions.

### Gate 7 — filters, buckets, and brigades

- Implement arbitrary read/write chains, append/prepend, auto mode, lifecycle,
  resources, removal, buckets, brigades, `$consumed`, and flush flags.
- Implement configured built-in filters through the same protocol.
- Correctly distinguish `PSFS_PASS_ON`, `PSFS_FEED_ME`, and
  `PSFS_ERR_FATAL`.
- Support filters on all compatible backends and generic stream consumers.

Acceptance: applicable filter PHPTs pass, including chains longer than two,
incremental feeds, fatal filters, flush-on-close, removal, and binary buckets.

### Gate 8 — CSV

- Port or independently reproduce `php_fgetcsv()` and `php_fputcsv()` byte
  semantics from the frozen `ext/standard/file.c`.
- Implement exact parameter validation, versioned deprecations, coercions,
  multiline parsing, binary behavior, EOL behavior, errors, partial writes,
  cursor updates, and return values.
- Exercise CSV through the generic stream layer.

Acceptance: a table-driven PHP oracle matrix plus applicable CSV PHPTs have zero
byte, return, diagnostic, cursor, or lifecycle mismatches.

### Gate 9 — sockets, transports, `stream_select`, and names

- Implement complete client/server/pair/send/receive/accept/shutdown/name
  contracts and by-reference outputs.
- Support the configured TCP, UDP, Unix, UDG, IPv4, and IPv6 surfaces.
- Implement persistent/async flags or do not expose incompatible support.
- Implement `stream_select` over nullable arrays, buffered data, exact mutation,
  keys, timeout validation, microsecond behavior, warnings, and false returns
  without arbitrary descriptor limits.

Acceptance: local deterministic network fixtures and applicable network PHPTs
pass, including nonblocking and timeout cases.

### Gate 10 — HTTP, HTTPS, FTP, and response state

- Implement configured protocol wrappers through the generic registry.
- Apply all supported context options and notification events.
- Implement redirects, chunked transfer, decoding, ranges/offsets as applicable,
  authentication, proxies, status/error handling, metadata, and cleanup.
- Implement local-scope `$http_response_header` exactly.

Acceptance: deterministic local HTTP/FTP fixtures cover success, redirects,
errors, malformed responses, repeated calls, nested calls, and request reset;
applicable PHPTs pass without public-network dependencies.

### Gate 11 — TLS and compression

- Implement every advertised TLS transport and zlib/bzip2 wrapper/filter.
- Apply certificate verification, peer names, crypto methods, session streams,
  handshake progress, shutdown, metadata, and errors.
- Implement streaming compression flush, concatenation, corruption, truncation,
  seek restrictions, partial I/O, and cleanup.

Acceptance: local certificates and binary fixtures prove all advertised
configuration behavior; disabled bridge/extension builds advertise nothing
unavailable.

### Gate 12 — stream-backed file API integration

- Route all in-scope file APIs through the generic wrapper/context/filter layer.
- Implement include-path, context, flags, atomicity/locking, recursive directory,
  directory iteration, rename/copy/unlink, metadata operations, links, stat
  cache, formatting writes, sync, passthrough, and error semantics.
- Apply stream-affecting INI and process state, including `open_basedir`,
  `allow_url_fopen`, `allow_url_include` where applicable,
  `default_socket_timeout`, include path, current directory, umask, and default
  HTTP user-agent/from settings.
- Route `SplFileObject` and `SplTempFileObject` through the same engine and CSV
  implementation without a divergent compatibility shortcut.
- Ensure optimization and literal fast paths cannot bypass runtime behavior.

Acceptance: the same operation produces the same result for a literal path,
dynamic path, built-in wrapper, and user wrapper whenever php-src permits it.

### Gate 13 — ownership, GC, optimization, and web isolation

- Prove resource, string, array, Mixed, context, callback, bucket, and wrapper
  ownership across normal, partial, and failing paths.
- Preserve copy-on-write and reference writes.
- Keep observable behavior identical with IR optimization on and off.
- Reset request-local stream globals, defaults, registrations, response headers,
  buffers, and persistent/nonpersistent resources according to php-src.

Acceptance: focused runtime-GC, aliasing, cycle, failure, optimization, and
repeated-web-request tests pass with heap diagnostics enabled where available.

### Gate 14 — exhaustive parity closure

- Classify every relevant PHPT from the frozen directories in a checked-in
  manifest as `pass`, `not-applicable`, or `blocked-by-non-stream-capability`.
- `not-applicable` requires a source/build/target reason.
- `blocked-by-non-stream-capability` requires a minimal reproducer proving the
  blocker is outside stream semantics; it does not count as stream acceptance
  when the same semantics can be tested another way.
- Add equivalent reduced tests for PHPTs blocked only by unrelated syntax.
- Run focused supported-target tests during implementation and the complete
  stream manifest on macOS ARM64, Linux ARM64, and Linux x86_64 before closure.
- Reconcile generated docs and remove unrelated generated artifacts from the PR.

Acceptance: zero unexplained skips, zero known behavioral mismatches, exact
manifest parity, all applicable PHPTs green, and all supported targets green.

## Test and evidence requirements

### Differential case schema

Each case records:

- frozen php-src revision and PHP build information;
- Elephc revision and target;
- source program and auxiliary fixture bytes;
- INI, locale, timezone, environment, and enabled bridges;
- stdout and stderr as bytes;
- exit status and exception class/message;
- serialized return value and type;
- by-reference outputs;
- stream position and metadata before and after;
- filesystem/network side effects;
- cleanup state and heap diagnostics where relevant.

Unstable platform text must be normalized only when php-src PHPT itself
normalizes it. Normalization must not erase a semantic difference.

### Required adversarial dimensions

- zero, one, boundary, and very large lengths;
- negative values wherever validation is observable;
- empty strings, NUL bytes, invalid UTF-8, and arbitrary binary bytes;
- literal and runtime-computed URIs, modes, filter names, and context options;
- seekable and nonseekable backends;
- blocking and nonblocking streams;
- partial reads/writes and injected failures;
- buffered and unbuffered data;
- more than 256 live streams and chains longer than two filters;
- descriptor reuse and stale resources;
- nested callbacks and reentrancy;
- repeated web requests in one process;
- optimizer enabled and disabled;
- every supported target.

### PHPT inventory

The initial audit found approximately 897 tests under
`ext/standard/tests/file`, 158 under `ext/standard/tests/streams`, 85 under
`ext/standard/tests/http`, 64 under `ext/standard/tests/network`, 73 under
`ext/standard/tests/dir`, 12 under `ext/standard/tests/directory`, and 39 under
`ext/standard/tests/filters` at the frozen revision. Directory counts are
discovery data only. The checked-in manifest must select relevant tests by
inspected behavior rather than filename alone and must include
extension-specific wrapper/filter tests and stream-backed SPL tests.

### No false acceptance

The following do not close a gate:

- a matching function or constant count;
- a declaration with ignored parameters;
- a test that asserts an accepted no-op;
- a literal-only lowering path;
- a fixed-capacity implementation whose tests stay below the cap;
- returning a plausible empty value on an error path;
- target-specific success on only one supported target;
- an old reviewer verdict for a different spec or commit;
- a reviewer comment that is not an explicit `LOCK`.

## Implementation collaboration after authorization

After unanimous spec consensus and explicit user authorization, gates may be
researched and implemented with dedicated Codex instances corresponding to GLM
5.2, Kimi K2.7, and MiniMax M3 review roles, subject to these rules:

- one writer owns the active integration gate;
- other instances are read-only researchers/reviewers unless assigned an
  isolated, non-overlapping gate branch;
- no instance edits the PHP/framework oracle fixtures;
- integration, builds, and mutable acceptance runs are serialized;
- every gate records provenance, focused tests, target evidence, open
  mismatches, and explicit reviews;
- later-gate work never disguises an unaccepted dependency;
- no implementation starts from this document alone.

## Reviewer protocol

Each reviewer receives the full bytes of this file and its SHA-256. It must
review completeness against the frozen php-src baseline, internal consistency,
testability, target policy, and whether every currently verified blocker has a
closure gate.

The response format is:

```text
VERDICT: LOCK | BLOCK
SPEC_SHA256: <exact SHA-256>
BLOCKERS:
- <normative omission, contradiction, or untestable requirement>
NON_BLOCKING_NOTES:
- <optional improvement that does not prevent implementation>
```

`LOCK` means:

- no missing normative stream/CSV surface was found;
- no internal contradiction or ambiguous completion rule was found;
- every requirement is testable against the frozen source/oracle;
- the architecture can represent the required semantics;
- the gate sequence can reach literal 100% compliance;
- the reported SHA-256 exactly matches the reviewed bytes.

Any `BLOCK` invalidates the round. The specification is amended, a new SHA-256
is computed, and all three reviewers restart independently on the same new
bytes. Consensus is only three explicit `LOCK` verdicts on one exact SHA-256.

The required Ollama identities for this campaign are:

- `glm-5.2:cloud`, model ID `ce8fd6f94793`;
- `kimi-k2.7-code:cloud`, model ID `eda07a659237`;
- `minimax-m3:cloud`, model ID `d03a959f45c0`.

Each transcript records the exact tag and model ID returned by `ollama list`,
the prompt protocol, the spec SHA-256, and the verdict. Model availability is
therefore an execution prerequisite and an auditable fact, not a php-src
normative assumption.

Review transcripts are stored under:

`.plans/reviews/streams-php-src-8.5-compliance/<sha-prefix>/`

The immutable consensus result is recorded in that directory's `CONSENSUS.md`;
review status is deliberately not edited back into this file after review,
because doing so would invalidate the reviewed SHA-256.

## Definition of done for this specification phase

This phase is complete only when:

1. this document has no unrecorded audit blocker known to the author;
2. its SHA-256 is recorded;
3. GLM 5.2 returns `LOCK` for that SHA-256;
4. Kimi K2.7 returns `LOCK` for that SHA-256;
5. MiniMax M3 returns `LOCK` for that SHA-256;
6. the three exact transcripts are preserved;
7. no implementation code was changed;
8. the user receives the spec, consensus evidence, branch/worktree provenance,
   and a concise list of the largest implementation risks.
