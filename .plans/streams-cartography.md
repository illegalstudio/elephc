# Streams support map against php-src 8.5.6

Snapshot of where elephc stands against the frozen baseline of
`streams-php-src-8.5-compliance.md` (php-src `php-8.5.6`,
`fcc29c8d6d6ee6f5ba2d941f0a2a6ea6aa6ee633`), taken on 2026-09-30 right after the streams
branch was synchronised with `origin/main`. Two independent measurements:

1. **Declarations**: the Gate 0 drift ledger, regenerated from the current tree
   (`tools/php_oracle/export_elephc_drift.py` against the committed `streams-full` manifest).
2. **Behaviour**: php-src's own PHPTs for the stream surface, run differentially: each test's
   `--FILE--` through the local `php -n` and through elephc, outputs compared after scrubbing
   paths. `--EXPECT--` is not used, so every difference is elephc against php on one machine.

## 1. Declarations (drift ledger, macos-aarch64)

| Category | July (Gate 0) | Now |
|---|---:|---:|
| function-signature | 125 | 128 |
| missing-function | 70 | 67 |
| class-surface | 11 | 12 |
| missing-class | 3 | 2 |
| missing-constant | 8 | 7 |
| extra-constant | 5 | 3 |
| constant-value | 3 | 0 |
| configured-capability | 3 | 0 |
| **total** | **228** | **219** |

What the numbers hide:

- **Capabilities are closed.** `stream_get_wrappers()`, `stream_get_transports()` and
  `stream_get_filters()` list exactly php's twelve wrappers, ten transports and nine filter
  families, in php's order. No constant carries a wrong value any more.
- **Most signature drift is representation, not behaviour.** Of the 128 entries, the bulk is
  (a) `allows_null`, which the exporter cannot read from elephc's model and emits as unknown,
  (b) resource parameters, which php leaves untyped and elephc types `mixed`, and (c) return
  unions such as `string|false`, which elephc's contract collapses to `mixed`. Closing them
  needs a `resource` and a union-return spelling in the shared contract, not runtime work.
- **Real signature gaps** (behaviour a program can observe):
  `exec`/`system` (1 of 3/2 parameters), `passthru` (1 of 2), `shell_exec` return,
  `hash`/`hash_file` (missing `$options`), `rename` (missing `$context`),
  `socket_set_block` (declared as a stream alias; php's takes an ext/sockets `Socket`),
  `fscanf` (the variadic is not flagged by-reference), `stream_select` (`?int $seconds`),
  the missing `default_constant` spellings (`SEEK_SET`, `SCANDIR_SORT_ASCENDING`,
  `STREAM_CLIENT_CONNECT`), and seven aliases not recorded as aliases (`fputs`,
  `is_writeable`, `set_file_buffer`, `socket_get_status`, `socket_set_blocking`,
  `socket_set_timeout`, `stream_register_wrapper`).
- **Missing functions**: 67 in the ledger, of which 20 are an exporter blind spot (prelude
  functions it does not read: the `gz*` family, `readgzfile()`, `dir()`, plus the opcache,
  image and `error_log()` preludes). The genuinely absent ones:
  - ext/bz2: `bzopen`, `bzread`, `bzwrite`, `bzflush`, `bzclose`, `bzerrno`, `bzerror`,
    `bzerrstr`;
  - ext/ftp: `ftp_*` (15 stream-reaching functions);
  - ext/standard: `md5_file`, `sha1_file`, `parse_ini_file`, `get_headers`,
    `get_meta_tags`, `get_browser`, `highlight_file`/`show_source`, `php_strip_whitespace`,
    `phpinfo`, `mail`, `move_uploaded_file`, `proc_open`;
  - ext/hash: `hash_hmac_file`, `hash_update_file`, `hash_update_stream`;
  - ext/sockets: `socket_import_stream`, `socket_export_stream`, `socket_close`,
    `socket_set_nonblock`;
  - ext/openssl: `openssl_cms_encrypt`, `openssl_cms_sign`.
- **Missing classes**: `StreamBucket` (buckets are still plain objects) and `Directory` (the
  class exists through the `dir()` prelude, which the exporter does not see).
- **Constants**: `LOCK_*`, `FILE_TEXT`/`FILE_BINARY` and `STREAM_PF_INET6` are declared but
  outside the table the exporter reads; `SCANDIR_SORT_*` sit in the stream table although php
  files them under ext/standard's directory functions.

## 2. Behaviour (php-src PHPTs, differential)

1,330 PHPTs from the 8.5.6 tree, compared against the local `php -n` (8.5.10). `SAME` is
byte-identical output after path scrubbing; `BUILD` is a compile refusal; `INI` is a
difference in a test that needs an ini setting elephc does not read; `SKIP` is php's own
`--SKIPIF--` (or a section the runner cannot drive, such as `--ARGS--` or `--POST--`).

| Directory | Total | SKIP | SAME | DIFF | INI | BUILD | SAME / runnable |
|---|---:|---:|---:|---:|---:|---:|---:|
| ext/standard/tests/file | 785 | 177 | 293 | 109 | 9 | 197 | 48% |
| ext/standard/tests/streams | 158 | 10 | 82 | 16 | 2 | 48 | 55% |
| ext/standard/tests/filters | 39 | 0 | 22 | 13 | 0 | 4 | 56% |
| ext/standard/tests/dir | 73 | 29 | 23 | 9 | 0 | 12 | 52% |
| ext/standard/tests/directory | 12 | 2 | 3 | 2 | 0 | 5 | 30% |
| ext/standard/tests/network | 64 | 10 | 21 | 4 | 1 | 28 | 39% |
| ext/standard/tests/http | 49 | 0 | 0 | 3 | 0 | 46 | 0% |
| ext/zlib/tests | 150 | 22 | 21 | 71 | 1 | 35 | 16% |
| **total** | **1,330** | **250** | **465** | **227** | **13** | **375** | **43%** |

### Why tests do not compile (375)

| Cause | Tests | Stream semantics? |
|---|---:|---|
| `include`/`require` of a runtime path (`$file_path."/file.inc"`, `sprintf(...)`) | 66 | no: closed-world compilation; also drags in the helpers `file.inc` defines (`create_files`, 10) |
| `count()` refused on a value the checker cannot prove countable | 45 | no: typing |
| `posix_kill`, `posix_getuid`, `posix_getgid`, `posix_mkfifo` | 26 | no: ext/posix (the HTTP tests fork a server with it) |
| `current()` on a non-variable | 11 | no: array pointer model |
| `set_include_path()` | 11 | yes: include path is a Gate 12 item |
| `fwrite()` refusing an int that is a resource at run time | 10 | yes: declaration typing |
| `preg_*` over a `mixed` subject | 10 | no |
| `proc_open`, `parse_ini_file`, `md5_file`, `deflate_init`/`inflate_init`, `escapeshellarg`, `setcookie`, `uniqid`, `strchr`, ... | ~60 | partly: `proc_open`, `parse_ini_file`, `md5_file` reach `php_stream` |
| `exec()`/`rename()` arity, `ftruncate`/`fseek`/`fread` over `mixed`, by-ref string output into a null slot | ~15 | yes: signature and lowering gaps |

### Largest behavioural differences

- The `gz*` prelude opens through `fopen()`, so its warnings name `fopen` where php names
  `gzopen` (21 zlib tests); `dir()` likewise names `opendir` (5 tests).
- `Directory` objects are cloneable and serializable; php refuses both.
- The directory family (`closedir`, `readdir`, `rewinddir`) answers `false`/`NULL` for a closed
  or foreign handle where php throws `TypeError: ... must be a valid Directory resource`.
- `copy()` into a directory reports the failed open instead of php's dedicated warning.
- Validation `ValueError`s still missing on `file()` flags, `fgetcsv()` length and
  `ftruncate()` size; `stream_context_set_params()` accepts a non-resource.
- `StreamBucket` is still a `stdClass` (visible in `var_dump`).
- `fgetcsv()` ignores `$length` when it truncates the line.
- HTTP: every test needs the CLI web server (`php_cli_server_start`) or `posix_kill`; none runs.

## Plan to resume, by impact

1. **Merge health first.** Get every suite green on the synchronised branch: migrate main's
   warning assertions from `stderr` to `diagnostics`, fix the regressions the merge exposes,
   land the monitor port, regenerate the builtin docs and the drift ledgers (three targets).
2. **Cheap behavioural wins** (each a handful of PHPTs): the function php names in `gz*` and
   `dir()` warnings; `Directory` refusing clone and serialization; the directory family's
   `TypeError` for an invalid handle; the missing `ValueError`s (`file()` flags,
   `fgetcsv()` length, `ftruncate()` size, `stream_context_set_params()`); `copy()` into a
   directory; a real `StreamBucket` class; `fgetcsv()` honouring `$length`.
3. **Include path** (Gate 12): resolve `include` paths built from `__DIR__` and locals that
   hold compile-time strings, and implement `set_include_path()`/`get_include_path()`. This
   alone unblocks about 80 PHPTs that currently do not compile.
4. **Missing stream-reaching functions**: `md5_file`, `sha1_file`, `hash_*_file`,
   `hash_update_stream`, `parse_ini_file`, `get_headers`, `get_meta_tags`, `proc_open`, then
   ext/bz2's `bz*` and ext/sockets' `socket_import_stream`/`socket_export_stream`.
5. **Signature gaps** (Gate 2): `exec`/`system`/`passthru`, `rename($context)`,
   `hash($options)`, `fscanf`'s by-reference variadic, `stream_select`'s `?int`, aliases and
   default constants recorded as such, `socket_set_block` removed from the stream aliases.
6. **zlib wrapper parity** (Gate 11): the 71 differing zlib tests, starting with the shared
   warning and `gzseek`/`gztell` cases.
7. **HTTP fixtures** (Gate 10): the tests need php's CLI server and ext/posix; a local fixture
   server in the harness is the prerequisite for measuring any of them.
8. **Declaration model** (Gate 2): a `resource` spelling and union returns in the shared
   contract, so the remaining signature drift becomes real signal.

## 3. Where the branch's work went

Own commits by plan gate (518 branch commits, subject keywords):

| Gate | Commits |
|---|---:|
| G12 stream-backed file API | 76 |
| G7 filters, buckets, brigades | 74 |
| G6 user wrappers | 73 |
| G3 core I/O, buffering, metadata | 54 |
| G5 built-in wrappers and `php://` | 50 |
| G2 declarations and constants | 25 |
| G13 ownership, web isolation | 25 |
| G8 CSV | 26 |
| G9 sockets, `stream_select` | 23 |
| G4 contexts | 20 |
| G11 TLS and compression | 20 |
| G1 registry and lifecycle | 17 |
| G10 HTTP/FTP and response state | 5 |

## 4. State after the synchronisation with main

- The previous merge of main into the branch (`209c680d83`) had resolved its conflicts by
  keeping one side wholesale in about sixty files: it dropped upstream code and branch code
  alike, and left the branch tip unable to compile. The new merge re-merged those files
  against that merge's own base and rebuilt `catalog_data.rs` contract by contract.
- The branch writes php's diagnostics to stdout, as the php CLI does; main's tests still
  assert them on stderr, and are being migrated to the harness's `diagnostics` field.
- The monitor's stream-operation counters are being re-ported onto main's rewritten
  monitoring subsystem as separate commits.
