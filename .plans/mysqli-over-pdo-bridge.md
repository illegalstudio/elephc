# mysqli over the elephc-pdo bridge

> **For agentic workers:** implement task-by-task. Do not wrap `new PDO()`.
> Do not add `pg_*` or `SQLite3` in this plan. Keep every supported target
> (`macos-aarch64`, `linux-aarch64`, `linux-x86_64`) working in the same change.

**Goal:** Ship a documented, PHP-compatible **mysqli subset** as its own
elephc-PHP prelude that talks to the existing `elephc_pdo` C ABI (MySQL
driver only). A mysqli-only program must not declare `PDO` / `PDOStatement`
or throw `PDOException`.

**Architecture:** Same three-layer stack as PDO, but a second PHP surface.
`mysqli` / `mysqli_stmt` / `mysqli_result` hold opaque `int` bridge handles
and buffer result rows in PHP so a later query cannot invalidate an earlier
result. Shared `extern "elephc_pdo"` declarations are injected at most once.
`extension_loaded('mysqli')` is surface-based, not “any `elephc_pdo` link”.

**Tech stack:** elephc-PHP prelude + `crates/elephc-pdo` MySQL client
(already linked as `libelephc_pdo.a`). No new crate. No new wire protocol.

## Global constraints

- PHP-derived behavior must match PHP. Fail loudly (`false`, `mysqli_sql_exception`,
  `ValueError`) rather than leaking PDO types or returning wrong data.
- Home files / new Rust modules need the standard `//! Purpose` / `Called from`
  / `Key details` preamble and `///` on every function.
- Never run `cargo fmt`. Never add `Co-Authored-By`.
- Local verification is focused (`cargo test --test codegen_tests mysqli`,
  `cargo test --test extension_loaded_tests`, `cargo build`). Do not run the
  full suite unless asked.
- Live MySQL fixtures are `#[ignore]` and use `ELEPHC_MY_DSN`, same as
  `tests/codegen/pdo_mysql.rs`.
- Magician / `eval()` is **out of scope**. Same documented divergence as PDO:
  `extension_loaded('mysqli')` is false under eval.
- `--strict-php` must keep mysqli visible (it is standard PHP, not an elephc extension).

---

## Task checklist

- [ ] 1. Split the `elephc_pdo` extern block so two PHP surfaces can share it
- [ ] 2. Surface-based `extension_loaded` / `get_loaded_extensions` for PDO vs mysqli
- [ ] 3. mysqli prelude skeleton, detection, `--with-mysqli`, injection
- [ ] 4. Connection, errors, escape, charset, ping, transactions
- [ ] 5. Buffered `query` / `mysqli_result` fetch + `foreach`
- [ ] 6. Prepared statements (`bind_param`, `execute`, `get_result`, `bind_result`)
- [ ] 7. `multi_query` / `next_result` / `more_results`
- [ ] 8. Docs, example, ROADMAP, compatibility table

---

## Locked design

### What this is

```
PHP  mysqli / mysqli_stmt / mysqli_result / mysqli_sql_exception / mysqli_*()
  ↓  extern "elephc_pdo"   (shared with PDO, injected at most once)
C ABI  elephc_pdo_* handles
  ↓
Rust  crates/elephc-pdo MySQL driver (pure-Rust mysql client)
```

### What this is not

- A facade over `class PDO`. No `new PDO`, no `PDOStatement`, no `PDOException`.
- A second MySQL client.
- Full 106/106 php-src mysqli. The locked subset below is v1; everything else
  is an explicit non-goal and must fail loudly.

### Surfaces vs the bridge

Today `linker::php_extension_for_lib("elephc_pdo")` is `"PDO"`, so any program
that declares `extern "elephc_pdo"` reports PDO as loaded. mysqli-only programs
will also declare those externs and would lie.

Change reporting to **injected PHP surface**, not linked archive:

| Program uses | Links | `extension_loaded('PDO')` | `extension_loaded('mysqli')` | `class_exists('PDO')` | `class_exists('mysqli')` |
|---|---|---|---|---|---|
| only PDO | `elephc_pdo` | true | false | true | false |
| only mysqli | `elephc_pdo` | false | true | false | true |
| both | `elephc_pdo` | true | true | true | true |
| neither | — | false | false | false | false |
| `--with-pdo` | `elephc_pdo` | true | false | true | false |
| `--with-mysqli` | `elephc_pdo` | false | true | false | true |

Do **not** report `mysqlnd`. elephc is not mysqlnd.

`--with-mysqli` is a **runtime capability** (same bucket as `--with-regex`),
not a new `BRIDGES` row. It force-injects the mysqli prelude and force-links
`elephc_pdo`. It must not inject the PDO classes.

### Result identity

`mysqli_query` returns a `mysqli_result` that **owns** its rows. After
`query()`, drain every row through `elephc_pdo_step` into a PHP array, then
`elephc_pdo_finalize` the statement. A later query on the same connection
must leave the earlier result intact (`data_seek`, `num_rows`, `foreach`).

`MYSQLI_USE_RESULT` is accepted and still buffered (documented divergence).
True unbuffered `use_result` is out of scope.

### Errors

Mirror PHP 8.1+ `mysqli_report` (default `MYSQLI_REPORT_ERROR | MYSQLI_REPORT_STRICT`
when `--php-version >= 8.1`; PHP 8.0 default is `MYSQLI_REPORT_OFF`).

| Mode | Failure |
|---|---|
| `STRICT` | throw `mysqli_sql_exception` (`extends RuntimeException`) |
| `ERROR` only | write the message to STDERR, return `false` |
| `OFF` | silent `false` |

Never throw `PDOException`. `connect_errno` / `connect_error` are distinct
from `errno` / `error` (connect-time vs last query).

### Escape vs quote

`mysqli_real_escape_string` returns the escaped payload **without** wrapping
quotes. Reuse the MySQL branch of `PDO::quote()` in
`src/pdo_prelude.rs` (around the `NO_BACKSLASH_ESCAPES` / `\\` / `'` / `"\` /
`\0` / `\n` / `\r` / `\Z` sequence) **minus** the surrounding `'…'` and the
`_binary` introducer. Implement it in the mysqli prelude in PHP. Do not add a
bridge entry point for this.

### DSN construction

`mysqli::__construct` / `real_connect` build a `mysql:` DSN and call
`elephc_pdo_open_persistent`. They never call `new PDO`.

```
mysql:host=<host>;port=<port>;dbname=<db>;user=<user>;password=<pass>;unix_socket=<sock>;connect_timeout=<sec>
```

Rules (php-src mysqli / pdo_mysql):

- Host starting with `p:` → persistent `1`, host is the remainder.
- `unix_socket` is honored only when host is empty or exactly `localhost`.
- Default port is `3306` when omitted.
- `MYSQLI_CLIENT_FOUND_ROWS` → `$my_found_rows = 1`.
- `MYSQLI_CLIENT_COMPRESS` → packed `$my_driver_config` `compress=1`.
- `MYSQLI_INIT_COMMAND` from `mysqli_options` before connect → `$my_init_command`.
- `MYSQLI_OPT_CONNECT_TIMEOUT` → DSN `connect_timeout`.
- `MYSQLI_SET_CHARSET_NAME` / `set_charset` after connect → `SET NAMES <ident>`.
- Empty constructor (`mysqli_init()` / `new mysqli()`) leaves `$conn = -1`
  until `real_connect`.

Reject a successful open whose `elephc_pdo_driver_name($conn)` is not `"mysql"`
(cannot happen if the DSN prefix is forced, but keep the guard).

### Procedural API

PHP 8.1+ requires the `mysqli` object. elephc always requires the explicit
link argument (including `--php-version=8.0`). No implicit last-link.
Document that 8.0 divergence.

Every public method has a `mysqli_*` procedural alias that forwards to the
object. Aliases live in the prelude as ordinary PHP functions so
`function_exists('mysqli_query')` is true once the prelude is injected.

### Version gates

| Symbol | Min `--php-version` |
|---|---|
| `mysqli_result::fetch_column` / `mysqli_fetch_column` | 8.1 |
| `mysqli_execute_query` / `mysqli::execute_query` | 8.2 |
| Default `mysqli_report` = `ERROR\|STRICT` | 8.1 |
| Default `mysqli_report` = `OFF` | 8.0 |

Use the same `-- elephc PHP >= X.Y … --` comment slices the PDO prelude already
uses, driven by `php_version` at injection time.

### Non-goals (must not silently pretend)

- `mysqli_poll`, `mysqli_reap_async_query`, async connect
- `change_user`, `kill`, `dump_debug_info`, `refresh`, embedded server
- `mysqli_ssl_set` / `MYSQLI_CLIENT_SSL` (document; fail with a clear error)
- True unbuffered `use_result`
- `mysqli_driver`, `mysqli_warning` objects
- Reporting `mysqlnd` or `pdo_mysql` as loaded because mysqli was used
- Magician / eval builtins
- `pg_*`, `SQLite3`, old `mysql_*`

Unsupported methods: if declared, they throw / return `false` with a message
that names the missing feature. Prefer **not declaring** them so
`method_exists` / `function_exists` stay honest. Detection must only match
declared names.

---

## Locked v1 PHP surface

### Classes

- `mysqli_sql_exception extends RuntimeException` with public `$sqlstate`
- `mysqli`
- `mysqli_result` implements `IteratorAggregate`
- `mysqli_stmt`

### `mysqli` methods / properties

Methods: `__construct`, `real_connect`, `connect` (alias of construct path),
`close`, `__destruct`, `ping`, `select_db`, `set_charset`, `character_set_name`,
`real_escape_string`, `escape_string`, `query`, `real_query`, `prepare`,
`begin_transaction`, `commit`, `rollback`, `autocommit`, `options` / `set_opt`,
`get_server_info`, `get_client_info`, `get_host_info`, `get_proto_info`,
`get_server_version`, `get_client_version`, `stat`, `thread_id`,
`multi_query`, `more_results`, `next_result`, `store_result`, `use_result`
(alias of store, buffered).

Public properties refreshed after operations (not `readonly`; not hooks):
`affected_rows`, `connect_errno`, `connect_error`, `errno`, `error`,
`error_list` (list of `['errno'=>, 'sqlstate'=>, 'error'=>]`),
`field_count`, `client_info`, `client_version`, `host_info`,
`protocol_version`, `server_info`, `server_version`, `info`, `insert_id`,
`sqlstate`, `thread_id`, `warning_count`.

Writes to those properties are a documented divergence (they stick). Do not
spend time on write barriers in v1.

### `mysqli_result`

`fetch_assoc`, `fetch_row`, `fetch_array($mode = MYSQLI_BOTH)`,
`fetch_object(?string $class = "stdClass", array $constructor_args = [])`,
`fetch_all($mode = MYSQLI_NUM)`, `fetch_column(int $column = 0)` (8.1+),
`data_seek`, `fetch_field`, `fetch_fields`, `fetch_field_direct`,
`num_rows` / `field_count` properties, `lengths`, `close` / `free` /
`free_result`, `getIterator()`.

`fetch_field*` returns `stdClass` with at least: `name`, `orgname`, `table`,
`orgtable`, `def`, `db`, `catalog`, `max_length`, `length`, `charsetnr`,
`flags`, `type`, `decimals`. Unknown metadata fields may be `0` / `""` when
the bridge does not expose them; `name`, `type`, `flags`, `length` must come
from `elephc_pdo_column_name` / `column_native_type` or `column_type` /
`column_flags` / `column_len`.

### `mysqli_stmt`

`prepare` is on `mysqli`. On the statement: `bind_param(string $types, mixed &...$vars)`,
`execute(?array $params = null)`, `get_result`, `bind_result(mixed &...$vars)`,
`fetch`, `store_result`, `reset`, `close`, `bind_param` type chars `i`/`d`/`s`/`b`
only. Properties: `affected_rows`, `errno`, `error`, `field_count`,
`insert_id`, `num_rows`, `param_count`, `sqlstate`.

`bind_param` evaluates the type string against the variadic count (`strlen($types)`
must equal the number of vars). Bind through `elephc_pdo_bind_int` / `bind_double`
/ `bind_text` / `bind_blob` / `bind_null`.

### Constants (declare only these)

```
MYSQLI_ASSOC = 1
MYSQLI_NUM = 2
MYSQLI_BOTH = 3
MYSQLI_STORE_RESULT = 0
MYSQLI_USE_RESULT = 1
MYSQLI_REPORT_OFF = 0
MYSQLI_REPORT_ERROR = 1
MYSQLI_REPORT_STRICT = 2
MYSQLI_REPORT_INDEX = 4
MYSQLI_REPORT_ALL = 255
MYSQLI_CLIENT_COMPRESS = 32
MYSQLI_CLIENT_FOUND_ROWS = 2
MYSQLI_CLIENT_IGNORE_SPACE = 256
MYSQLI_CLIENT_INTERACTIVE = 1024
MYSQLI_CLIENT_SSL = 2048          // accepted on connect flags → hard error
MYSQLI_OPT_CONNECT_TIMEOUT = 0
MYSQLI_INIT_COMMAND = 3
MYSQLI_SET_CHARSET_NAME = 7
MYSQLI_TRANS_START_WITH_CONSISTENT_SNAPSHOT = 1
MYSQLI_TRANS_START_READ_WRITE = 2
MYSQLI_TRANS_START_READ_ONLY = 4
```

`MYSQLI_TYPE_*` only if `fetch_field` needs them; map from
`elephc_pdo_column_native_type` (`LONG` → `MYSQLI_TYPE_LONG`, etc.) with a
small table and `MYSQLI_TYPE_STRING` as default.

### Procedural aliases (all required once the method exists)

`mysqli_connect`, `mysqli_init`, `mysqli_real_connect`, `mysqli_close`,
`mysqli_ping`, `mysqli_select_db`, `mysqli_set_charset`,
`mysqli_character_set_name`, `mysqli_real_escape_string`, `mysqli_escape_string`,
`mysqli_query`, `mysqli_real_query`, `mysqli_prepare`, `mysqli_begin_transaction`,
`mysqli_commit`, `mysqli_rollback`, `mysqli_autocommit`, `mysqli_options`,
`mysqli_set_opt`, `mysqli_get_server_info`, `mysqli_get_client_info`,
`mysqli_get_host_info`, `mysqli_get_proto_info`, `mysqli_get_server_version`,
`mysqli_get_client_version`, `mysqli_stat`, `mysqli_thread_id`,
`mysqli_connect_errno`, `mysqli_connect_error`, `mysqli_errno`, `mysqli_error`,
`mysqli_error_list`, `mysqli_sqlstate`, `mysqli_affected_rows`,
`mysqli_insert_id`, `mysqli_field_count`, `mysqli_warning_count`,
`mysqli_info`, `mysqli_fetch_assoc`, `mysqli_fetch_row`, `mysqli_fetch_array`,
`mysqli_fetch_object`, `mysqli_fetch_all`, `mysqli_fetch_column`,
`mysqli_num_rows`, `mysqli_num_fields`, `mysqli_data_seek`, `mysqli_fetch_field`,
`mysqli_fetch_fields`, `mysqli_fetch_field_direct`, `mysqli_free_result`,
`mysqli_stmt_bind_param`, `mysqli_stmt_execute`, `mysqli_stmt_get_result`,
`mysqli_stmt_bind_result`, `mysqli_stmt_fetch`, `mysqli_stmt_store_result`,
`mysqli_stmt_reset`, `mysqli_stmt_close`, `mysqli_stmt_affected_rows`,
`mysqli_stmt_errno`, `mysqli_stmt_error`, `mysqli_stmt_num_rows`,
`mysqli_stmt_param_count`, `mysqli_execute_query` (8.2+),
`mysqli_multi_query`, `mysqli_more_results`, `mysqli_next_result`,
`mysqli_store_result`, `mysqli_use_result`, `mysqli_report`.

`mysqli_connect_errno` / `mysqli_connect_error` without a link read a
process-static last-connect failure stored on `mysqli` as statics
(`public static int $lastConnectErrno`, etc.), updated by every construct /
`real_connect` attempt. That is how PHP’s no-arg `mysqli_connect_error()` works.

---

## File map

### Create

| File | Responsibility |
|---|---|
| `src/mysqli_prelude.rs` | Concatenate PHP fragments, `inject_if_used`, `program_uses_mysqli` re-export |
| `src/mysqli_prelude/detect.rs` | Exhaustive AST walk (copy `image_prelude/detect.rs` shape) |
| `src/mysqli_prelude/externs.rs` | Not used — externs live in the shared PDO fragment |
| `src/mysqli_prelude/fragments.rs` | `concat!` of the PHP pieces below |
| `src/mysqli_prelude/constants.rs` | `MYSQLI_*` const fragment (`pub const SRC`) |
| `src/mysqli_prelude/exception.rs` | `mysqli_sql_exception` + `mysqli_report` + static report flag |
| `src/mysqli_prelude/connection.rs` | `class mysqli` + connect/escape/txn/options |
| `src/mysqli_prelude/result.rs` | `class mysqli_result` |
| `src/mysqli_prelude/statement.rs` | `class mysqli_stmt` |
| `src/mysqli_prelude/multi.rs` | `multi_query` / `next_result` (added in task 7) |
| `src/mysqli_prelude/procedural.rs` | `mysqli_*` free functions |
| `tests/codegen/mysqli.rs` | Offline tests (injection, fail-to-connect, escape needs no live row) |
| `tests/codegen/mysqli_mysql.rs` | `#[ignore]` live MySQL, copy `pdo_mysql.rs` harness |
| `tests/error_tests/mysqli.rs` | Arity / empty query / bad bind types |
| `examples/mysqli-crud/main.php` | Small CRUD demo driven by env vars |
| `examples/mysqli-crud/.gitignore` | `*.s`, `*.o`, `main` |
| `docs/php/mysqli.md` | User-facing subset + divergences |

### Modify

| File | Why |
|---|---|
| `src/pdo_prelude.rs` | Move the `extern "elephc_pdo" { … }` block into a shared constant |
| `src/pdo_prelude/detect.rs` | Export `program_uses_pdo` as `pub` if not already public enough |
| `src/pipeline.rs` | Inject shared externs + mysqli; record surfaces |
| `src/pipeline/backend.rs` | Seed `linked_extensions` from surfaces, not `elephc_pdo → PDO` |
| `src/cli.rs` | `--with-mysqli` in `RUNTIME_CAPABILITY_FLAGS`; help text |
| `src/lib.rs`, `src/main.rs` | `mod mysqli_prelude` |
| `src/progress.rs` | `"mysqli-prelude" => "Configuring mysqli support"` |
| `src/codegen/lower_inst/builtins/scalar_metadata.rs` | Comment: surfaces, not just bridges |
| `tests/codegen/support/compiler.rs` | Inject mysqli + shared externs; seed linked extensions |
| `tests/codegen/support/projects.rs` | Same injection order as the pipeline |
| `src/ir_lower/tests/mod.rs` | Same |
| `tests/error_tests.rs` / helpers | Inject mysqli when compiling error fixtures that need it |
| `tests/extension_loaded_tests.rs` | mysqli vs PDO matrix |
| `tests/codegen/mod.rs` | `mod mysqli; mod mysqli_mysql;` |
| `docs/compiling/cli-reference.md` | `--with-mysqli` |
| `docs/php/compatibility.md` | Generated — run `gen_php_comparison.py` after surface exists |
| `docs/php/opcache.md` | Extension table |
| `docs/README.md` | Link `docs/php/mysqli.md` |
| `README.md` | One line next to the PDO bullet |
| `ROADMAP.md` | New planned item under a new 0.x section (do not edit completed PDO boxes) |
| `scripts/ci/run_pdo_codegen_shard.sh` | Or a sibling filter so live mysqli tests ride the existing MySQL job |

Do **not** add a new `BRIDGES` entry. Do **not** change `elephc-pdo` unless a
task below names a specific ABI gap.

---

## Shared bridge ABI the prelude may call

Declare the **full** existing `extern "elephc_pdo"` block (the one already in
`src/pdo_prelude.rs`). mysqli will actually call this subset:

```
elephc_pdo_open_persistent
elephc_pdo_last_open_error
elephc_pdo_last_open_sqlstate
elephc_pdo_last_open_native_code
elephc_pdo_close
elephc_pdo_release
elephc_pdo_exec
elephc_pdo_last_insert_id
elephc_pdo_last_insert_id_text
elephc_pdo_changes
elephc_pdo_begin
elephc_pdo_commit
elephc_pdo_rollback
elephc_pdo_errcode
elephc_pdo_errmsg
elephc_pdo_sqlstate
elephc_pdo_prepare
elephc_pdo_bind_int / bind_double / bind_text / bind_blob / bind_null / bind_bool
elephc_pdo_reset
elephc_pdo_clear_bindings
elephc_pdo_step
elephc_pdo_next_rowset
elephc_pdo_column_count / column_name / column_type / column_int / column_double
elephc_pdo_column_data_len / column_data_ptr / column_data_byte
elephc_pdo_column_native_type / column_flags / column_len / column_table_name
elephc_pdo_finalize
elephc_pdo_driver_name
elephc_pdo_server_version / client_version / server_info / connection_status
elephc_pdo_warning_count
elephc_pdo_no_backslash_escapes
elephc_pdo_in_transaction
elephc_pdo_set_autocommit / autocommit
```

No new `elephc_pdo_*` symbols in v1. `ping` is `elephc_pdo_exec($conn, "SELECT 1")`
(or a successful `server_info` read). `thread_id` is
`elephc_pdo_exec` + a one-row `SELECT CONNECTION_ID()` drained like a query.
`select_db` is `` USE `<escaped ident>` ``. `set_charset` is
`SET NAMES <ident>` with the same `[A-Za-z0-9_]` filter PDO already uses for
the `charset` DSN key.

If ping-via-`SELECT 1` is too slow or eats a pending result during
`multi_query`, add `elephc_pdo_ping(int $conn): int` in task 4 as a thin
wrapper over `MyConn::is_alive` (already in `crates/elephc-pdo/src/my.rs`)
and a `-1` stub for non-MySQL handles. Only add that ABI if the `SELECT 1`
approach breaks a fixture.

---

### Task 1: Shared `elephc_pdo` extern fragment

**Files:**
- Modify: `src/pdo_prelude.rs` (cut the `extern "elephc_pdo" { … }` block out of
  `PDO_PRELUDE_SRC`)
- Create: `src/pdo_prelude/bridge_externs.rs` holding `pub const SRC: &str`
- Modify: `src/pdo_prelude.rs` `inject_if_used_for_version` to prepend
  `bridge_externs::SRC` when injecting PDO
- Modify: later tasks will call a new
  `pdo_prelude::inject_bridge_externs(program)` that is idempotent

**Interfaces:**
- Produces: `pub fn bridge_externs_src() -> &'static str`
- Produces: `pub fn inject_bridge_externs(program: Program) -> Program`
  — tokenizes/parses `bridge_externs::SRC` and prepends it **only if** the
  program does not already declare `elephc_pdo_open_persistent` (scan
  top-level `StmtKind::ExternBlock` / function names). Idempotent so PDO
  injection and mysqli injection can both ask for it.

**Steps:**

- [ ] **1.1** Move the existing `extern "elephc_pdo" { … }` text verbatim into
  `src/pdo_prelude/bridge_externs.rs` as `pub const SRC: &str = r#"<?php
extern "elephc_pdo" {
    …
}
"#;`
  Keep every comment. Do not rename symbols.

- [ ] **1.2** Make `inject_if_used_for_version` concatenate
  `bridge_externs::SRC + remaining PDO class source` (or call
  `inject_bridge_externs` on the parsed class prelude before extending with
  user code). Existing PDO tests must see the same symbols.

- [ ] **1.3** Run:

```bash
cargo test --test codegen_tests test_pdo_exec_and_assoc_fetch
cargo test --test codegen_tests test_pdo_prepared_positional_bind
```

  Expected: PASS. This is a no-behavior refactor.

- [ ] **1.4** Commit `refactor: share elephc_pdo externs for a second PHP surface`

---

### Task 2: Surface-based extension reporting

**Files:**
- Modify: `src/pipeline.rs` — compute `pdo_used` / later `mysqli_used` **before**
  injection (`force || detect`)
- Modify: `src/pipeline/backend.rs` — take an extra
  `linked_php_surfaces: &[String]`
- Modify: `src/linker/bridges.rs` — stop treating `elephc_pdo` as an automatic
  `"PDO"` extension **or** ignore that mapping when surfaces are supplied
- Modify: `tests/extension_loaded_tests.rs`
- Modify: comments in `src/codegen/lower_inst/builtins/scalar_metadata.rs`

**Interfaces:**
- Pipeline passes `linked_php_surfaces` into backend.
- Backend builds `linked_extensions` as:
  1. every non-PDO bridge via `php_extension_for_lib` (unchanged)
  2. plus each name in `linked_php_surfaces` (`"PDO"`, later `"mysqli"`)
- `elephc_pdo` no longer implies `"PDO"` by itself.

**Steps:**

- [ ] **2.1** Add a failing CLI test in `tests/extension_loaded_tests.rs`:

```rust
#[test]
fn pdo_usage_still_reports_pdo_without_mysqli() {
    let dir = make_test_dir("ext_pdo_not_mysqli");
    let src = "<?php new PDO('sqlite::memory:'); \
        var_dump(extension_loaded('PDO')); \
        var_dump(extension_loaded('mysqli'));";
    let bin = compile_with_flags(&dir, src, "app", &[]);
    // after task 3 this also proves mysqli stays false
    assert!(run(&bin).contains("bool(true)"), "PDO loaded");
}
```

  Keep the existing `--with-pdo` tests green: they still inject the PDO
  surface, so they still report PDO.

- [ ] **2.2** Implement surface seeding. In `pipeline.rs`:

```rust
let pdo_force = with_crates.contains("pdo");
let pdo_used = pdo_force || pdo_prelude::program_uses_pdo(&ast);
let ast = pdo_prelude::inject_if_used_for_version(ast, pdo_force, php_version);
// …
let mut linked_php_surfaces = Vec::new();
if pdo_used {
    linked_php_surfaces.push("PDO".to_string());
}
```

  Export `program_uses_pdo` from `src/pdo_prelude.rs` as `pub fn`.

  In `backend.rs`, when iterating `planned_link_libraries`, **skip**
  `php_extension_for_lib` when the lib is `elephc_pdo`. Then extend with
  `linked_php_surfaces`.

- [ ] **2.3** Run:

```bash
cargo test --test extension_loaded_tests
cargo test --test codegen_tests test_pdo_exec_and_assoc_fetch
```

  Expected: PASS. A PDO program still reports PDO. A non-PDO program still
  does not.

- [ ] **2.4** Commit `fix: report PDO from the injected surface, not the archive`

---

### Task 3: Prelude skeleton, detection, `--with-mysqli`

**Files:**
- Create: `src/mysqli_prelude.rs`, `src/mysqli_prelude/detect.rs`,
  `src/mysqli_prelude/fragments.rs`, `src/mysqli_prelude/constants.rs`,
  `src/mysqli_prelude/exception.rs` (exception class + `mysqli_report` only)
- Modify: `src/lib.rs`, `src/main.rs`, `src/pipeline.rs`, `src/cli.rs`,
  `src/progress.rs`, `tests/codegen/support/compiler.rs`,
  `tests/codegen/support/projects.rs`, `src/ir_lower/tests/mod.rs`,
  `tests/codegen/mod.rs`, `tests/codegen/mysqli.rs`,
  `tests/extension_loaded_tests.rs`

**Interfaces:**
- `pub fn program_uses_mysqli(program: &[Stmt]) -> bool`
- `pub fn inject_if_used(program: Program, force: bool, php_version: PhpVersion) -> Program`
- Detection classes (case-insensitive last segment): `mysqli`,
  `mysqli_stmt`, `mysqli_result`, `mysqli_sql_exception`
- Detection functions: last segment equals `mysqli_report` or starts with
  `mysqli_` (ASCII case-insensitive). Do **not** use a `mysql` prefix
  (that would match ancient `mysql_query`).
- Exhaustive `match` on AST nodes, same soundness rule as
  `src/image_prelude/detect.rs`.
- `--with-mysqli` recorded in `with_crates` via `RUNTIME_CAPABILITY_FLAGS`.
- Pipeline: `mysqli_force = with_crates.contains("mysqli")`;
  `mysqli_used = mysqli_force || program_uses_mysqli(&ast)`;
  if `mysqli_used` { `inject_bridge_externs`; `mysqli_prelude::inject_if_used`;
  `linked_php_surfaces.push("mysqli")` }.
- Order: PDO inject, then mysqli inject. Shared externs first (idempotent).

**Steps:**

- [ ] **3.1** Write `tests/codegen/mysqli.rs` first:

```rust
//! Purpose:
//! Offline mysqli prelude tests that do not need a live MySQL server.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the test harness.
//!
//! Key details:
//! - Live query/fetch fixtures live in `mysqli_mysql.rs` and are `#[ignore]`d.

use crate::support::*;

#[test]
fn test_mysqli_class_exists_and_does_not_leak_pdo() {
    let out = compile_and_run(
        r#"<?php
echo class_exists('mysqli') ? '1' : '0';
echo class_exists('PDO') ? '1' : '0';
echo function_exists('mysqli_connect') ? '1' : '0';
echo defined('MYSQLI_ASSOC') ? '1' : '0';
"#,
    );
    assert_eq!(out, "1011");
}

#[test]
fn test_pdo_program_does_not_grow_mysqli() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
echo class_exists('mysqli') ? '1' : '0';
echo class_exists('PDO') ? '1' : '0';
"#,
    );
    assert_eq!(out, "01");
}
```

- [ ] **3.2** Run `cargo test --test codegen_tests test_mysqli_class_exists` —
  expected FAIL (undefined `mysqli` / prelude missing).

- [ ] **3.3** Implement detection + a minimal prelude that only declares:

```php
<?php
const MYSQLI_ASSOC = 1;
const MYSQLI_NUM = 2;
const MYSQLI_BOTH = 3;
const MYSQLI_REPORT_OFF = 0;
const MYSQLI_REPORT_ERROR = 1;
const MYSQLI_REPORT_STRICT = 2;
const MYSQLI_REPORT_INDEX = 4;
const MYSQLI_REPORT_ALL = 255;

class mysqli_sql_exception extends RuntimeException {
    public string $sqlstate = "00000";
}

class mysqli {
    public int $conn = -1;
}

function mysqli_connect(): mysqli {
    return new mysqli();
}

function mysqli_report(int $flags): bool {
    return true;
}
```

  Wire `inject_if_used` like `image_prelude::inject_if_used`. Add unit tests
  in `detect.rs` that parse `new mysqli`, `mysqli_query($db, $sql)`,
  `use mysqli as Db`, and reject `mysql_query`.

- [ ] **3.4** CLI: add `"mysqli"` to `RUNTIME_CAPABILITY_FLAGS`. Help line lists
  `mysqli`. Unknown `--with-mysqli` must stop being an error.

- [ ] **3.5** Extension tests (full CLI, because `compile_and_run` does not go
  through `pipeline::compile`’s backend seeding unless you also update the
  test compiler — **do both**):

```rust
#[test]
fn mysqli_usage_reports_mysqli_not_pdo() {
    let dir = make_test_dir("ext_mysqli_not_pdo");
    let src = "<?php new mysqli(); \
        var_dump(extension_loaded('mysqli')); \
        var_dump(extension_loaded('PDO'));";
    let bin = compile_with_flags(&dir, src, "app", &[]);
    let out = run(&bin);
    assert!(out.contains("bool(true)"), "mysqli loaded: {out}");
    // second dump is PDO
    assert!(out.matches("bool(false)").count() >= 1, "PDO not loaded: {out}");
}

#[test]
fn with_mysqli_force_injects_without_static_new() {
    let dir = make_test_dir("ext_with_mysqli");
    let src = "<?php var_dump(class_exists('mysqli')); \
        var_dump(extension_loaded('mysqli')); \
        var_dump(extension_loaded('PDO'));";
    let bin = compile_with_flags(&dir, src, "app", &["--with-mysqli"]);
    let out = run(&bin);
    assert!(out.contains("bool(true)"));
}
```

  Update `tests/codegen/support/compiler.rs` immediately after the PDO
  inject:

```rust
let resolved = elephc::pdo_prelude::inject_bridge_externs_if_needed(resolved);
let resolved = elephc::mysqli_prelude::inject_if_used(resolved, false, php_version);
```

  And seed `elephc::codegen::set_linked_extensions` from the same
  `pdo_used` / `mysqli_used` bits so `compile_and_run` and the CLI agree.

- [ ] **3.6** Run:

```bash
cargo build
cargo test --test codegen_tests test_mysqli_class_exists
cargo test --test codegen_tests test_pdo_program_does_not_grow_mysqli
cargo test --test extension_loaded_tests
cargo test --test codegen_tests test_pdo_exec_and_assoc_fetch
```

  Expected: PASS.

- [ ] **3.7** Commit `feat: inject a mysqli prelude without leaking PDO`

---

### Task 4: Connection, errors, escape, charset, ping, transactions

**Files:**
- Modify: `src/mysqli_prelude/connection.rs`, `exception.rs`, `procedural.rs`
- Modify: `tests/codegen/mysqli.rs`
- Create: first live fixtures in `tests/codegen/mysqli_mysql.rs`

**Interfaces (PHP, locked):**

```php
class mysqli {
    public function __construct(
        ?string $hostname = null,
        ?string $username = null,
        ?string $password = null,
        ?string $database = null,
        ?int $port = null,
        ?string $socket = null
    ) { /* if any arg non-null, real_connect; else leave unconnected */ }

    public function real_connect(
        ?string $hostname = null,
        ?string $username = null,
        ?string $password = null,
        ?string $database = null,
        ?int $port = null,
        ?string $socket = null,
        int $flags = 0
    ): bool { /* build DSN; elephc_pdo_open_persistent; on fail record connect_* and report() */ }

    public function close(): true { /* elephc_pdo_close; $conn = -1 */ }
    public function ping(): bool;
    public function select_db(string $database): bool;
    public function set_charset(string $charset): bool;
    public function character_set_name(): string;
    public function real_escape_string(string $string): string;
    public function begin_transaction(int $flags = 0, ?string $name = null): bool;
    public function commit(int $flags = 0, ?string $name = null): bool;
    public function rollback(int $flags = 0, ?string $name = null): bool;
    public function autocommit(bool $enable): bool;
    public function options(int $option, $value): bool;
}

function mysqli_report(int $flags): bool { /* store process-static flags */ }
```

`report()` helper on `mysqli`: if last op failed, apply `mysqli_report` flags.
Savepoints (`$name` non-null) are `SAVEPOINT` / `RELEASE SAVEPOINT` /
`ROLLBACK TO` via `elephc_pdo_exec`. `MYSQLI_TRANS_START_READ_ONLY` is
`SET TRANSACTION READ ONLY` then `BEGIN` (best-effort; if `exec` fails, report).

`MYSQLI_CLIENT_SSL` → `mysqli_sql_exception` / `false`:
`"elephc mysqli does not support MYSQLI_CLIENT_SSL; use PDO MySQL TLS attributes"`.

**Steps:**

- [ ] **4.1** Offline fail-to-connect test (no server):

```rust
#[test]
fn test_mysqli_connect_failure_sets_connect_errno() {
    let out = compile_and_run(
        r#"<?php
mysqli_report(MYSQLI_REPORT_OFF);
$db = @new mysqli("127.0.0.1", "nope", "nope", "nope", 1);
echo $db->connect_errno > 0 ? "err" : "ok";
echo "|";
echo $db->connect_error !== "" ? "msg" : "empty";
"#,
    );
    assert_eq!(out, "err|msg");
}
```

  Put `connect_timeout=1` in the DSN builder so this cannot hang 30s.

- [ ] **4.2** Escape unit test can run once a connection exists; until then
  test the helper by connecting to live MySQL. Add `#[ignore]` live test:

```rust
#[test]
#[ignore]
fn test_mysqli_escape_and_roundtrip() {
    let out = compile_and_run(&my_program(
        r#"
mysqli_report(MYSQLI_REPORT_ERROR | MYSQLI_REPORT_STRICT);
echo $db->real_escape_string("a'b\\c");
"#,
    ));
    assert_eq!(out, r"a\'b\\c");
}
```

  `my_program` parses `ELEPHC_MY_DSN` (`mysql:host=…`) into `new mysqli(...)`
  **or** opens via a small helper that accepts the same env var. Prefer
  parsing the PDO DSN in the PHP fixture so CI can keep one env var:

```php
$dsn = (string) getenv("ELEPHC_MY_DSN");
// parse host/port/dbname/user/password from the mysql: DSN
$db = new mysqli($host, $user, $pass, $dbname, $port);
```

- [ ] **4.3** Implement DSN builder, `real_connect`, `close`, `__destruct`,
  `report()`, escape (copy PDO MySQL quoting minus wrapping quotes),
  `set_charset`, `select_db`, `autocommit`, `begin/commit/rollback`,
  `ping`, `options` for the three locked option ids.

  Method-locals use the `$_` prefix (same checker clash as PDO).

- [ ] **4.4** Run:

```bash
cargo test --test codegen_tests test_mysqli_connect_failure
cargo test --test codegen_tests test_mysqli_class_exists
```

  Live (optional locally):

```bash
cargo test --test codegen_tests test_mysqli_escape_and_roundtrip -- --ignored
```

- [ ] **4.5** Commit `feat: connect, escape, and transactions for mysqli`

---

### Task 5: Buffered query and `mysqli_result`

**Files:**
- Create/modify: `src/mysqli_prelude/result.rs`, `connection.rs` (`query` /
  `real_query`), `procedural.rs`
- Modify: `tests/codegen/mysqli_mysql.rs`

**Interfaces:**

`mysqli::query(string $query, int $resultmode = MYSQLI_STORE_RESULT): mysqli_result|bool`

- Empty `$query` → `ValueError` (`mysqli::query(): Argument #1 ($query) must not be empty`).
- `elephc_pdo_prepare($conn, $query, 1)` (emulated is fine for ad-hoc SQL)
  then `step` until `0`. Each row: for `i in 0..column_count`, switch on
  `elephc_pdo_column_type` the same way `PDOStatement::fetch` does
  (int / float / text via `column_data_*` so embedded NULs survive, null).
  Store `array<int, mixed>` rows plus a name map.
- `finalize` the statement immediately.
- Non-select (`column_count == 0`): return `true`, set `affected_rows` /
  `insert_id` from `elephc_pdo_changes` / `last_insert_id_text`.
- Failure: `report()` and `false`.

`mysqli_result` holds `$_rows`, `$_names`, `$_pos`, `$_lengths`.
`data_seek($i)` sets `$_pos`. Fetch advances `$_pos`. `num_rows` is
`count($_rows)`.

`fetch_object`: `new $class(...$constructor_args)` then assign public
properties by column name. Default class `stdClass`.

`getIterator()` yields `fetch_assoc()` rows until exhausted (PHP’s
`mysqli_result` foreach is assoc).

**Steps:**

- [ ] **5.1** Live fixture (mirror `test_pdo_exec_and_assoc_fetch`):

```rust
#[test]
#[ignore]
fn test_mysqli_query_assoc_and_independent_result() {
    let out = compile_and_run(&my_program(
        r#"
$db->query("DROP TABLE IF EXISTS mj");
$db->query("CREATE TABLE mj (id INT PRIMARY KEY AUTO_INCREMENT, name VARCHAR(32))");
$db->query("INSERT INTO mj (name) VALUES ('Ada'), ('Ben')");
$r1 = $db->query("SELECT name FROM mj ORDER BY id");
$r2 = $db->query("SELECT COUNT(*) AS c FROM mj");
$row = $r2->fetch_assoc();
echo $row["c"], "|";
$r1->data_seek(1);
$second = $r1->fetch_assoc();
echo $second["name"], "|";
echo $r1->num_rows;
$db->query("DROP TABLE mj");
"#,
    ));
    assert_eq!(out, "2|Ben|2");
}
```

  This fixture is the whole point of not wrapping `PDOStatement`.

- [ ] **5.2** Implement drain + fetch family + procedural aliases +
  `foreach ($result as $row)`.

- [ ] **5.3** Offline error test in `tests/error_tests/mysqli.rs`:

```php
<?php
$db = new mysqli();
$db->query("");
```

  Expected compile-time or runtime `ValueError`. If the empty string is only
  a runtime check, use `compile_and_run` and catch it:

```php
try { $db->query(""); echo "no"; } catch (ValueError $e) { echo "ve"; }
```

  An unconnected `$db->query("SELECT 1")` with `MYSQLI_REPORT_OFF` returns
  `false` and sets `errno`.

- [ ] **5.4** Run focused live + offline tests. Commit
  `feat: buffered mysqli_query and mysqli_result`

---

### Task 6: Prepared statements

**Files:**
- Modify: `src/mysqli_prelude/statement.rs`, `connection.rs` (`prepare`),
  `procedural.rs`
- Modify: `tests/codegen/mysqli_mysql.rs`, `tests/error_tests/mysqli.rs`

**Interfaces:**

```php
public function prepare(string $query): mysqli_stmt|false;
public function bind_param(string $types, mixed &...$vars): bool;
public function execute(?array $params = null): bool;
public function get_result(): mysqli_result|false;
public function bind_result(mixed &...$vars): bool;
public function fetch(): bool|null;
```

- `prepare` → `elephc_pdo_prepare($conn, $query, 0)` (native). Failure reports
  and returns `false`. Store the stmt handle. `param_count` = `strlen` of
  `?` placeholders after a scan that skips quotes/comments (or count binds
  the user supplies; PHP’s `param_count` is the server count — if the
  bridge has no counter, count `?` in the source with the existing MySQL
  placeholder scanner **in PHP**, conservative).
- `bind_param`: require `strlen($types) === count($vars)`. Unknown type char
  → `ValueError` / `false`. Store references; apply at `execute` time
  (PHP reads current values).
- `execute($params)`: if `$params` is non-null, bind them as all-`s` in
  order (PHP 8.1+). Then `elephc_pdo_reset` if re-executing, bind, `step`
  once to know if there is a result set. If columns: do **not** finalize;
  `get_result` drains the rest. If no columns: snapshot `affected_rows` /
  `insert_id` and reset for the next execute.
- `get_result`: drain remaining rows into a new `mysqli_result`, finalize
  or reset the stmt so it can be re-executed. After `get_result`,
  `bind_result`/`fetch` is invalid (`false`).
- `bind_result` + `fetch`: on `fetch`, write the next buffered-or-stepped
  row into the bound references. Prefer draining into an internal row
  buffer on `store_result` / first fetch so `num_rows` works.

`mysqli::execute_query(string $query, ?array $params = null)` (8.2+):
`prepare` + `execute($params)` + `get_result`.

**Steps:**

- [ ] **6.1** Live tests:

```rust
#[test]
#[ignore]
fn test_mysqli_stmt_bind_param_and_get_result() {
    // INSERT + SELECT ? bind, fetch_assoc name
}

#[test]
#[ignore]
fn test_mysqli_stmt_bind_result() {
    // SELECT id, name; bind_result($id, $name); fetch(); echo "$id:$name"
}
```

- [ ] **6.2** Implement. Variadic by-ref is already parsed
  (`variadic_by_ref` in `src/parser/stmt/params.rs`). Use
  `function bind_param(string $types, mixed &...$vars): bool`.

- [ ] **6.3** Error: `bind_param("is", $a)` (count mismatch) returns `false`
  or throws under STRICT.

- [ ] **6.4** Commit `feat: mysqli prepared statements on the pdo bridge`

---

### Task 7: `multi_query`

**Files:**
- Create: `src/mysqli_prelude/multi.rs`
- Modify: connection + procedural + `tests/codegen/mysqli_mysql.rs`

**Interfaces:**

`multi_query($sql)` sends the whole string. Enable multi-statements for
that connection by including `multi=1` in `$my_driver_config` at open
**or** by documenting that `multi_query` requires connecting with
`MYSQLI_CLIENT_*` — prefer turning multi on in `real_connect` when we can
(PDO already packs `multi=` in `$my_driver_config`). Always open mysqli
connections with `multi=1` so `multi_query` works without a second
handshake. Single `query()` still executes one statement; extra statements
in `query()` should fail like PHP (`Commands out of sync` / error) if the
server leftover-results. If matching that is hard, document:
`query()` sends the string as one prepared statement (no multi),
`multi_query()` is the only multi path.

Implementation sketch:

1. `elephc_pdo_prepare` / `exec` of the full string on a connection that
   has `multi=1`.
2. First result set drained into a pending `mysqli_result` held on `mysqli`.
3. `store_result()` returns that pending result.
4. `more_results()` / `next_result()` call `elephc_pdo_next_rowset` and
   drain the next set.

If `next_rowset` cannot be used without a live statement handle, keep the
statement alive on `mysqli` until `next_result` returns false, then
finalize.

**Steps:**

- [ ] **7.1** Live fixture: `multi_query("SELECT 1 AS a; SELECT 2 AS b")`,
  `store_result` → `1`, `next_result`, `store_result` → `2`.

- [ ] **7.2** Implement. If the bridge cannot represent this without a new
  ABI, stop and add `elephc_pdo_exec_multi` rather than faking it with two
  PHP `query()` calls (that would not be one server round-trip and would
  break `mysqli_insert_id` across a mixed batch).

- [ ] **7.3** Commit `feat: mysqli_multi_query via next_rowset`

---

### Task 8: Docs, example, roadmap, compatibility

**Files:**
- Create: `docs/php/mysqli.md` (Astro frontmatter, no top-level `# Title`)
- Create: `examples/mysqli-crud/main.php`, `.gitignore`
- Modify: `docs/README.md`, `README.md`, `docs/compiling/cli-reference.md`,
  `docs/php/opcache.md`, `ROADMAP.md`
- Run: `python3 scripts/docs/gen_php_comparison.py` so
  `docs/php/compatibility.md` picks up prelude functions if the catalog
  counts them. If the comparison catalog only sees registry builtins,
  add a `†` note like PDO: real support is the Extensions table.

**`docs/php/mysqli.md` required sections:**

- Connecting (`new mysqli`, `p:` persistent, `real_connect` flags)
- Queries and `mysqli_result` (independent buffered results)
- Prepared statements
- `multi_query`
- Errors / `mysqli_report` / `mysqli_sql_exception`
- Divergences: no `mysqlnd`, no SSL flags, `USE_RESULT` buffered, no
  implicit 8.0 last-link, no eval, `MYSQLI_CLIENT_SSL` rejected, property
  writes stick, ping via `SELECT 1` unless `elephc_pdo_ping` was added
- Locked non-goals list

**Example:** `examples/mysqli-crud/main.php` reads
`ELEPHC_MY_HOST` / `USER` / `PASSWORD` / `DB` (or `ELEPHC_MY_DSN`),
creates a table, inserts, selects, prints rows, drops the table. Skip
gracefully with a message if env is unset so `cargo run -- build examples/mysqli-crud/main.php`
does not require MySQL to compile.

**ROADMAP:** add a **new** 0.x section (do not reopen completed PDO
checkboxes), e.g. under upcoming 0.x:

```
### mysqli (MySQL / MariaDB) — subset over the elephc-pdo bridge

- [ ] Prelude + shared `elephc_pdo` externs; no PDO class leak
- [ ] Connect / escape / transactions / buffered query / result
- [ ] Prepared statements
- [ ] multi_query
```

Mark items `[x]` only when the matching task is merged.

**CLI docs:** `--with-mysqli` force-injects the mysqli prelude and
force-links `elephc_pdo`. It does not inject PDO.

- [ ] **8.1** Write docs + example + ROADMAP + README.
- [ ] **8.2** `git diff --check` and `cargo build`.
- [ ] **8.3** Commit `docs: document the mysqli subset and --with-mysqli`

---

## Test plan (what “done” means)

| Layer | Command | What it proves |
|---|---|---|
| Refactor | `cargo test --test codegen_tests test_pdo_exec_and_assoc_fetch` | PDO still works |
| Offline mysqli | `cargo test --test codegen_tests mysqli` | injection, no PDO leak, connect fail, empty query |
| Extensions | `cargo test --test extension_loaded_tests` | surface matrix |
| Errors | `cargo test --test error_tests mysqli` | arity / ValueError |
| Live MySQL | `ELEPHC_MY_DSN=… cargo test --test codegen_tests mysqli_mysql -- --ignored` | real protocol |
| Build | `cargo build` | no warnings |

Do not run the full local suite.

CI already has a live MySQL job for PDO. Add the `mysqli_mysql` filter to
that job so ignored tests run there. Do not require Docker in the
implementation worktree.

## PHP cross-check

When a semantic is ambiguous, run `php -r '…'` (if `php` is installed) and
lock the fixture to that output. In particular:

- `mysqli_report` default per 8.0 vs 8.1
- `real_escape_string("a'b\\c")` with and without `NO_BACKSLASH_ESCAPES`
- `foreach ($result as $row)` key shape
- `bind_param` count mismatch exception vs warning
- `new mysqli(...)` on failure with STRICT vs OFF (throws vs object with
  `connect_errno`)

## Risks

1. **Duplicate externs** if both preludes declare the block. Task 1’s
   idempotent `inject_bridge_externs` is mandatory before task 3.
2. **`extension_loaded('PDO')` false positive** if task 2 is skipped.
3. **Result clobber** if `query()` returns a live statement handle instead
   of a drained `mysqli_result`.
4. **`quote` used as `escape`** — never call `PDO::quote` or wrap quotes.
5. **Test harness vs CLI** injection drift. Every prelude inject site
   listed in the file map must be updated in the same PR that adds
   `inject_if_used`.
6. **`multi_query` vs prepare** — do not implement task 7 by splitting on
   `;` in PHP.

## Suggested PR split

1. Tasks 1–3 (plumbing, no live MySQL)
2. Tasks 4–5 (usable CRUD)
3. Task 6 (statements)
4. Task 7 (multi_query)
5. Task 8 can land with PR 2 so the subset is documented as soon as it
   runs, then be updated in 3–4
