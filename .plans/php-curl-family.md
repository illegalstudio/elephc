# PHP curl family

- [x] Freeze the PHP 8.2–8.5 curl surface and pin native library versions.
- [x] Add managed native packages for libcurl and its TLS backend (no system fallback).
- [x] Land `crates/elephc-curl` with a versioned C ABI and an i64 handle table.
- [x] Wire `--with-curl`, pay-for-use linking, and `extension_loaded('curl')`.
- [x] Inject a HashContext-style prelude for the PHP 8 handle/file classes.
- [x] Register the full `CURLOPT_*` / `CURLINFO_*` / `CURLE_*` constant tables.
- [x] Ship the easy interface (`curl_init` … `curl_version`) on every supported target.
- [x] Implement CURLOPT/CURLINFO option waves without inert acceptance.
- [x] Ship the multi interface, including PHP 8.5 `curl_multi_get_handles`.
- [x] Ship the share interface, including PHP 8.5 persistent shares.
- [x] Ship `CURLFile`, `CURLStringFile`, and `curl_file_create`.
- [x] Ship libcurl callbacks through the existing runtime callable invoker.
- [x] Mirror the same ABI in magician `eval_builtin!` homes.
- [x] Add local HTTP/HTTPS fixtures, example, generated docs, and a ROADMAP item.

This is a first-class PHP `ext/curl` implementation for AOT and magician. It is
**not** the v0.28 Zend-extension consumer PoC (`ROADMAP.md` “link against PHP
extension `.so`”). `extern "curl" { … }` stays a user-FFI example and must not
become the PHP-visible API.

Split across several green PRs is expected. Do not claim “full curl” until the
function/class/constant audit has no unexplained missing symbol and every
accepted option either works or fails with a PHP-shaped diagnostic.

## Goal

PHP programs can use the complete `ext/curl` function, class, and constant
surface. Compiled binaries and `eval()` call the same `elephc_curl_*` ABI.
Programs that never mention curl never declare the classes, never link the
bridge, and never require the native `curl` package.

## Architecture

```text
PHP  curl_init / curl_setopt / curl_exec / …
  ├─ AOT:      prelude wrappers
  │              → internal __elephc_curl_* builtins
  │              → RuntimeFnId::Curl*
  │              → target-aware lowering
  │              → elephc_curl_* C ABI
  │              → pinned libcurl
  └─ magician: eval_builtin! homes
                 → same elephc_curl_* C ABI
                 → same pinned libcurl
```

PHP 8 models sessions as objects (`CurlHandle`, `CurlMultiHandle`,
`CurlShareHandle`, `CurlSharePersistentHandle`), not resources. Follow
`src/hash_prelude.rs`:

- Raw work lives in `internal: true` `__elephc_curl_*` registry builtins.
- PHP names and classes live in an injected prelude so `get_class()`,
  `instanceof`, and type hints match PHP 8.
- The object stores an opaque Mixed/int handle in `$__elephc_handle`.
- Bind that property to a local before every internal call (same leak
  workaround documented in the hash prelude).
- No `__destruct` that double-frees. Object teardown releases the Mixed
  cell, which calls `elephc_curl_*_free`.
- `curl_close` / `curl_multi_close` / `curl_share_close` are no-ops, as
  in PHP 8.

libcurl is the only backend that can honour the option/info/error surface.
A pure-Rust HTTP client is rejected for this feature.

## Locked decisions

1. **Pinned libcurl:** catalog package `curl` **8.21.0** (upstream 2026-06-24).
   Task 1 records the exact tarball URL, byte size, and SHA-256 before any
   recipe lands. Constant numeric values come from this libcurl plus PHP's
   own extra options, not from the developer's `php -r`.
2. **TLS backend:** managed **OpenSSL 3.x LTS** used **only** as libcurl's
   SSL backend. PHP `openssl_encrypt` / `hash()` stay on `elephc-crypto`
   (RustCrypto). Task 1 pins the exact OpenSSL version/URL/size/SHA-256.
   Reuse catalog `zlib` **1.3.2**.
3. **No system fallback.** Homebrew/distro `-lcurl` / `-lssl` must not
   satisfy the requirement. Ordinary `elephc file.php` never downloads or
   builds native packages (same contract as PCRE2).
4. **Pay-for-use.** Detection of a curl function, class, or constant that
   requires the runtime links `elephc_curl` and requires native `curl`.
   `--with-curl` force-links the whole archive and force-injects the prelude.
5. **`extension_loaded('curl')`** is true only when the bridge is linked.
   Existing tests that assert curl is unloaded remain valid for curl-free
   programs.
6. **Protocol matrix for the first complete landing:**
   `FILE`, `HTTP`, `HTTPS`, `FTP`, `FTPS`. Other schemes fail at
   `curl_exec` / `curl_multi_exec` with `CURLE_UNSUPPORTED_PROTOCOL`
   (or `CURLE_NOT_BUILT_IN` when that is what this libcurl build returns).
   Never succeed silently. HTTP/2, HTTP/3, SSH, LDAP, SMTP, IMAP, MQTT,
   GOPHER, RTSP, SMB, TELNET, TFTP are explicit follow-ups.
7. **Unsupported option:** `curl_setopt` returns `false` and emits PHP's
   warning (`…: option … is not supported by this build` / type warning).
   No inert `true`.
8. **PHP version profile:** 8.2–8.4 omit `curl_multi_get_handles`,
   `curl_share_init_persistent`, and `CurlSharePersistentHandle`. 8.5
   (default) includes them.
9. **Callbacks** use the existing descriptor-based runtime callable
   invoker (`src/codegen/runtime_callable_invoker.rs`). Do not invent a
   second invoke ABI.
10. **Tests never hit the public internet** in the default suite. Use the
    local HTTP/HTTPS fixture pattern in `tests/codegen/io/streams.rs`.
    `#[ignore]` is allowed only for optional live smokes.
11. **All three supported targets** (`macos-aarch64`, `linux-aarch64`,
    `linux-x86_64`) ship in the same change as the runtime symbols they
    need. No ARM64-first stub.
12. **Home files must not import `crate::codegen`.** Typed
    `RuntimeFnId` / `RuntimeCallTarget` only.

## File map

| Path | Responsibility |
|---|---|
| `scripts/curl/extract_php_curl_surface.php` | Dump PHP 8.2–8.5 functions, classes, constants, and option kinds |
| `scripts/docs/curl_surface.json` | Frozen committed matrix consumed by tests and constant generation |
| `src/native_deps/catalog.rs` | Catalog entries for `curl` and `openssl` |
| `src/native_deps/recipes/curl.rs` | Static libcurl recipe (HTTP/HTTPS/FILE/FTP/FTPS + OpenSSL + zlib) |
| `src/native_deps/recipes/openssl.rs` | Static OpenSSL recipe used only by libcurl |
| `src/native_deps/recipe.rs` | Dispatch the new recipe revisions |
| `crates/elephc-curl/` | Bridge staticlib: handle table + `elephc_curl_*` ABI |
| `Cargo.toml` | Workspace member + test/dev-dep like the other bridges |
| `src/linker/bridges.rs` | `elephc_curl` / `--with-curl` / `php_extension: Some("curl")` |
| `src/cli.rs` | Flag appears in the generated `--with-*` help list automatically |
| `src/pipeline.rs` + `src/pipeline/backend.rs` | Prelude inject + native package requirement |
| `src/curl_prelude.rs` + `src/curl_prelude/detect.rs` | PHP classes and wrappers |
| `src/builtins/spec.rs` | New `Area::Curl` |
| `src/builtins/curl/` | One home file per `__elephc_curl_*` or PHP-name builtin |
| `src/types/curl_constants.rs` | Frozen `(name, i64)` table |
| `src/types/checker/driver/init.rs` | Register constant types |
| `src/codegen_support/prescan.rs` | Materialize constant values |
| `src/name_resolver/names.rs` | `is_builtin_global_constant` chains `CURL_INT_CONSTANTS` |
| `src/ir/runtime_fn.rs` | `RuntimeFnId::Curl*` + `Bridge("elephc_curl")` |
| `src/codegen/lower_inst/runtime_functions/group_13.rs` | New dispatch group |
| `src/codegen/lower_inst/builtins/curl/` | Target-aware emitters |
| `src/codegen_support/runtime/curl/` | Thin `__rt_curl_*` wrappers if ABI packing needs assembly |
| `crates/elephc-magician/src/interpreter/builtins/curl/` | Eval homes |
| `tests/codegen/curl/` | End-to-end fixtures |
| `tests/error_tests/curl.rs` | Arity, type, closed-handle, unknown-option errors |
| `examples/curl-get/main.php` | Documented GET + error handling |
| `docs/php/curl.md` | User-facing contract, protocol matrix, SSL split |

Prefer many leaf files. `src/builtins/curl/mod.rs` only lists `pub mod` entries.
Do not dump the option matrix into `src/ir/runtime_fn.rs`.

## PHP surface (normative)

Normative language versions: PHP 8.2, 8.3, 8.4, 8.5 (elephc default 8.5).

### Functions

```php
curl_init(?string $url = null): CurlHandle|false
curl_setopt(CurlHandle $handle, int $option, mixed $value): bool
curl_setopt_array(CurlHandle $handle, array $options): bool
curl_exec(CurlHandle $handle): string|bool
curl_close(CurlHandle $handle): void
curl_copy_handle(CurlHandle $handle): CurlHandle|false
curl_errno(CurlHandle $handle): int
curl_error(CurlHandle $handle): string
curl_escape(CurlHandle $handle, string $string): string|false
curl_unescape(CurlHandle $handle, string $string): string|false
curl_getinfo(CurlHandle $handle, ?int $option = null): mixed
curl_pause(CurlHandle $handle, int $flags): int
curl_reset(CurlHandle $handle): void
curl_upkeep(CurlHandle $handle): bool
curl_version(): array|false
curl_strerror(int $error_code): ?string
curl_file_create(string $filename, ?string $mime_type = null, ?string $posted_filename = null): CURLFile

curl_multi_init(): CurlMultiHandle
curl_multi_add_handle(CurlMultiHandle $multi_handle, CurlHandle $handle): int
curl_multi_remove_handle(CurlMultiHandle $multi_handle, CurlHandle $handle): int
curl_multi_exec(CurlMultiHandle $multi_handle, int &$still_running): int
curl_multi_select(CurlMultiHandle $multi_handle, float $timeout = 1.0): int
curl_multi_info_read(CurlMultiHandle $multi_handle, int &$queued_messages = null): array|false
curl_multi_close(CurlMultiHandle $multi_handle): void
curl_multi_getcontent(CurlHandle $handle): ?string
curl_multi_setopt(CurlMultiHandle $multi_handle, int $option, mixed $value): bool
curl_multi_strerror(int $error_code): ?string
curl_multi_errno(CurlMultiHandle $multi_handle): int
curl_multi_get_handles(CurlMultiHandle $multi_handle): array   // PHP 8.5 only

curl_share_init(): CurlShareHandle
curl_share_setopt(CurlShareHandle $share_handle, int $option, mixed $value): bool
curl_share_close(CurlShareHandle $share_handle): void
curl_share_errno(CurlShareHandle $share_handle): int
curl_share_strerror(int $error_code): ?string
curl_share_init_persistent(array $share_options): CurlSharePersistentHandle  // PHP 8.5 only
```

`curl_file_create` is an alias of `CURLFile::__construct`.

### Classes

| Class | PHP | Notes |
|---|---|---|
| `CurlHandle` | 8.0+ | `final`, not user-constructible |
| `CurlMultiHandle` | 8.0+ | `final`, not user-constructible |
| `CurlShareHandle` | 8.0+ | `final`, not user-constructible |
| `CurlSharePersistentHandle` | 8.5+ | `final`, not user-constructible |
| `CURLFile` | 5.5+ | constructible; properties `name`, `mime`, `postname` |
| `CURLStringFile` | 8.1+ | constructible; properties `data`, `postname`, `mime` |

`CURLFile` methods: `__construct`, `getFilename`, `getMimeType`,
`getPostFilename`, `setMimeType`, `setPostFilename`.

### PHP-layer options (not raw libcurl)

Implement inside `elephc-curl`, not by forwarding blindly:

| Option | Behaviour |
|---|---|
| `CURLOPT_RETURNTRANSFER` | Capture body; `curl_exec` returns `string` |
| `CURLOPT_HEADER` | Prepend response headers to the body buffer |
| `CURLOPT_FILE` | Write body to a PHP stream resource |
| `CURLOPT_INFILE` / `CURLOPT_INFILESIZE` | Read upload body from a PHP stream |
| `CURLOPT_WRITEHEADER` | Write headers to a PHP stream |
| `CURLOPT_STDERR` | Redirect verbose output to a PHP stream |
| `CURLOPT_BINARYTRANSFER` | No-op, as in modern PHP |
| `CURLOPT_SAFE_UPLOAD` | Always on; `@file` upload strings stay literal |

Default (no `RETURNTRANSFER`, no `FILE`): write the body to stdout, matching
PHP CLI.

### `curl_exec` return shape

| Situation | Return |
|---|---|
| Failure | `false` |
| Success + `RETURNTRANSFER` | body `string` (may be `""`) |
| Success + default / `FILE` | `true` |

### `curl_getinfo` return shape

- No `$option`: associative array of the PHP info keys (`url`, `http_code`,
  `content_type`, `total_time`, `redirect_count`, `primary_ip`, …).
- `CURLINFO_*` option: scalar/`array`/`false` as PHP documents for that key.
- `CURLINFO_HEADER_OUT` requires `CURLINFO_HEADER_OUT` to have been enabled.

### `curl_version()` keys

Match PHP's array: `version_number`, `age`, `features`, `ssl_version_number`,
`version`, `host`, `ssl_version`, `libz_version`, `protocols`,
`ares`, `ares_num`, `libidn`, `iconv_ver_num`, `libssh_version`,
`brotli_ver_num`, `brotli_version`, `feature_list` when the targeted PHP
version exposes it. Values reflect **our** pinned libcurl, not the host PHP.

---

### Task 1: Freeze the surface and pin native versions

**Files:**
- Create: `scripts/curl/extract_php_curl_surface.php`
- Create: `scripts/docs/curl_surface.json`
- Create: `crates/elephc-curl/tools/curl_constants.rs` generator notes inside the JSON (`generated_from`)
- Modify: `ROADMAP.md` — add a `v0.30.x — PHP curl extension` section with unchecked items copied from this checklist

**Interfaces:**
- Produces: committed JSON with `php_versions`, `functions`, `classes`, `constants` (`name` → integer), `php_only_options`, `option_kinds` (`long` / `string` / `blob` / `slist` / `off_t` / `callback` / `file` / `php_layer`), `libcurl` (`version`, `url`, `sha256`, `exact_size`), `openssl` (same fields)

- [x] **Step 1: Extract the PHP surface**

Run against every available local PHP 8.2–8.5 binary (skip a version if it
is not installed; still commit the union and mark the source version per
constant):

```php
<?php
$wanted = [];
foreach (get_defined_functions()['internal'] as $name) {
    if (str_starts_with($name, 'curl_')) {
        $wanted['functions'][] = $name;
    }
}
foreach (['CurlHandle', 'CurlMultiHandle', 'CurlShareHandle', 'CurlSharePersistentHandle', 'CURLFile', 'CURLStringFile'] as $class) {
    $wanted['classes'][$class] = class_exists($class);
}
$wanted['constants'] = get_defined_constants(true)['curl'] ?? [];
echo json_encode($wanted, JSON_PRETTY_PRINT);
```

Cross-check the function list against this plan. The committed JSON is the
audit source; later tasks may not add a PHP-visible name that is absent here
except for `internal: true` `__elephc_*` aliases.

- [x] **Step 2: Pin native archives**

Download the official source tarballs, record `exact_size` and SHA-256, and
write them into `curl_surface.json`. Do not invent checksums. Catalog code
in Task 2 copies these exact fields.

- [x] **Step 3: Add the ROADMAP section**

```markdown
## v0.30.x — PHP curl extension

- [ ] `ext/curl` function, class, and constant surface on AOT and magician
- [ ] Managed native `curl` 8.21.0 + OpenSSL TLS backend (no system fallback)
- [ ] Easy, multi, and share interfaces including PHP 8.5 additions
- [ ] `CURLFile` / `CURLStringFile` uploads
- [ ] libcurl callbacks via the runtime callable invoker
```

Do not tick the v0.28 “call a PHP curl `.so`” item. That is a different
product.

- [x] **Step 4: Commit**

```bash
git add scripts/curl/extract_php_curl_surface.php scripts/docs/curl_surface.json ROADMAP.md
git commit -m "docs: freeze PHP curl surface and native version pins"
```

---

### Task 2: Managed native `openssl` + `curl`

**Files:**
- Modify: `src/native_deps/catalog.rs`
- Create: `src/native_deps/recipes/openssl.rs`
- Create: `src/native_deps/recipes/curl.rs`
- Modify: `src/native_deps/recipes/mod.rs`
- Modify: `src/native_deps/recipe.rs` dispatcher
- Modify: `src/native_deps/lockfile.rs` only if transitive deps still fail closed — then implement transitive materialization here (curl is the first catalog package with non-empty `dependencies`)
- Test: `src/native_deps/recipe.rs` existing “current catalog recipe revisions have dispatchers” test

**Interfaces:**
- Consumes: checksums and versions from `scripts/docs/curl_surface.json`
- Produces: catalog names `openssl` and `curl`; `curl.dependencies = ["openssl", "zlib"]`; ordered link outputs for `curl`:

```text
lib/libcurl.a
```

and for `openssl`:

```text
lib/libssl.a
lib/libcrypto.a
```

zlib already emits `lib/libz.a`. Final PHP-program link order (later task):

```text
libelephc_curl.a
libcurl.a
libssl.a
libcrypto.a
libz.a
```

libcurl `configure` (all three targets):

```text
--disable-shared --enable-static
--with-openssl=<openssl prefix>
--with-zlib=<zlib prefix>
--disable-ldap --disable-ldaps
--disable-rtsp --disable-dict --disable-telnet --disable-tftp
--disable-pop3 --disable-imap --disable-smb --disable-smtp
--disable-gopher --disable-mqtt --disable-manual --disable-docs
--without-libpsl --without-libssh2 --without-nghttp2 --without-brotli --without-zstd
--without-librtmp --without-libidn2
```

Enable HTTP, HTTPS, FILE, FTP, FTPS. Use the existing toolchain discovery
(`src/native_deps/toolchain.rs`). No curl-sys, no CMake unless OpenSSL's
own build requires it; prefer OpenSSL's documented `Configure` + `make`
and curl's `configure` + `make`.

- [x] **Step 1: Write a recipe unit test that the dispatcher recognizes `curl` and `openssl`**

Extend `current_catalog_recipe_revisions_have_dispatchers` by adding the
packages. The test already walks `catalog::known_names()`.

- [x] **Step 2: Implement catalog entries and recipes**

Copy the exact URL/size/SHA-256 from Task 1. `supported_targets` is
`macos-aarch64`, `linux-aarch64`, `linux-x86_64`.

If lock expansion still requires every transitive dep to be declared in
`elephc.toml`, implement catalog-driven transitive materialization so
`elephc native add curl` also installs `openssl` and `zlib`. Do not teach
users to list OpenSSL by hand.

- [x] **Step 3: Install once on the development machine**

```bash
elephc native add curl
elephc native list
```

Expected: `curl 8.21.0`, `openssl <pinned>`, `zlib 1.3.2` present for the
host target.

- [x] **Step 4: Commit**

```bash
git add src/native_deps
git commit -m "feat: add managed native curl and openssl packages"
```

---

### Task 3: `elephc-curl` crate and handle table

**Files:**
- Create: `crates/elephc-curl/Cargo.toml`
- Create: `crates/elephc-curl/src/lib.rs`
- Create: `crates/elephc-curl/src/abi.rs`
- Create: `crates/elephc-curl/src/handles.rs`
- Create: `crates/elephc-curl/src/easy.rs`
- Create: `crates/elephc-curl/src/php_layer.rs`
- Modify: root `Cargo.toml` workspace `members` / `default-members` and `[dev-dependencies]`
- Modify: `tests/codegen/support/runner.rs` `TEST_BRIDGE_STATICLIBS`

**Interfaces:**
- Produces: `crate-type = ["staticlib", "rlib"]`, library name `elephc_curl`
- Produces C ABI (names are normative):

```c
int32_t  elephc_curl_version_abi(void);              // returns 1
int64_t  elephc_curl_easy_init(void);                // 0 on failure
int32_t  elephc_curl_easy_set_url(int64_t id, const uint8_t *ptr, size_t len);
int32_t  elephc_curl_easy_setopt_long(int64_t id, int32_t opt, int64_t value);
int32_t  elephc_curl_easy_setopt_str(int64_t id, int32_t opt, const uint8_t *ptr, size_t len);
int32_t  elephc_curl_easy_perform(int64_t id);
int32_t  elephc_curl_easy_errno(int64_t id);
int32_t  elephc_curl_easy_error(int64_t id, uint8_t *out, size_t out_cap, size_t *out_len);
int32_t  elephc_curl_easy_take_body(int64_t id, uint8_t **ptr, size_t *len); // RETURNTRANSFER
void     elephc_curl_easy_free(int64_t id);
int32_t  elephc_curl_global_info(uint8_t *out_json, size_t cap, size_t *len); // curl_version
```

Task 3 only needs `init`, `set_url`, `setopt_long` for `RETURNTRANSFER`,
`perform`, `errno`, `error`, `take_body`, `free`, and `global_info`. Later
tasks add multi/share/info/slist/blob/callback entry points in the same
crate; do not put those symbols in generated assembly until their task.

Handle table: `Mutex<HashMap<i64, EasyEntry>>`, monotonic positive ids,
never reuse an id. `EasyEntry` owns `*mut CURL` plus PHP-layer buffers
(`return_transfer: bool`, `body: Vec<u8>`). `curl_easy_cleanup` runs in
`elephc_curl_easy_free`.

The crate **declares** libcurl `extern "C"` symbols. It does **not** link
libcurl at `cargo build -p elephc-curl` time. The PHP-program linker
supplies `libcurl.a` (Task 4).

- [x] **Step 1: Add a crate unit test that init/free is balanced**

`cargo test -p elephc-curl` can only run if the test binary links libcurl.
Gate those tests with an env var `ELEPHC_CURL_LIB_DIR` pointing at the
native artifact, or skip with a clear message when the archive is absent.
Do not make `cargo test -p elephc` require curl artifacts for unrelated
tests.

- [x] **Step 2: Implement the handle table and the Task 3 ABI**

Use a private write callback to fill `body` when `RETURNTRANSFER` is set.
Default write callback writes to stdout via `libc::write` on fd 1 so
`curl_exec` without `RETURNTRANSFER` matches PHP CLI.

- [x] **Step 3: Commit**

```bash
git add crates/elephc-curl Cargo.toml tests/codegen/support/runner.rs
git commit -m "feat: add elephc-curl bridge crate and easy handle table"
```

---

### Task 4: Linker, `--with-curl`, native requirement

**Files:**
- Modify: `src/linker/bridges.rs` — new `BridgeStaticlib`
- Modify: `src/linker/bridges.rs` unit tests (`php_extension_for_lib`)
- Modify: `src/pipeline/backend.rs` — when `elephc_curl` is planned, emit `NativeRequirement::package("curl")`
- Modify: `src/cli.rs` tests only if a new special-case is added (`--with-curl` should work via the existing table)
- Modify: `tests/extension_loaded_tests.rs` — keep curl-unloaded cases; add a linked case
- Modify: `docs/compiling/cli-reference.md` and `docs/compiling/linking-and-conditional-compilation.md` — document `--with-curl`
- Modify: `AGENTS.md` / `CONTRIBUTING.md` `--with-<crate>` lists if they enumerate crates

**Interfaces:**
- Produces:

```rust
BridgeStaticlib {
    lib_name: "elephc_curl",
    env_var: "ELEPHC_CURL_LIB_DIR",
    crate_name: "elephc-curl",
    flag_name: "curl",
    whole_archive: true,
    macos_frameworks: &[], // add Security/CoreFoundation only if the OpenSSL recipe requires them
    needs_libdl: true,
    php_extension: Some("curl"),
}
```

Link plan for a curl program must be:

```text
libelephc_curl.a → libcurl.a → libssl.a → libcrypto.a → libz.a
```

Missing native artifacts must print the same recovery style as PCRE2
(`elephc native add curl` + project path). Do not fall back to `-lcurl`.

- [x] **Step 1: Failing test — `extension_loaded('curl')` is false without usage**

Existing tests already cover this. Add:

```rust
#[test]
fn with_curl_reports_extension_loaded() {
    let out = compile_and_run(
        r#"<?php
        var_dump(extension_loaded('curl'));
        var_dump(function_exists('curl_version'));
        "#,
    );
    // After later tasks this becomes bool(true) + bool(true) when the
    // prelude exists. In this task, only the linked-bridge name matters:
    // compile a program that force-links via --with-curl once the prelude
    // exists. Until Task 5, assert php_extension_for_lib("elephc_curl") == Some("curl").
}
```

- [x] **Step 2: Implement the table entry and native requirement**

- [x] **Step 3: Commit**

```bash
git add src/linker/bridges.rs src/pipeline/backend.rs tests/extension_loaded_tests.rs docs
git commit -m "feat: wire --with-curl bridge and native curl requirement"
```

---

### Task 5: Prelude and internal builtins for the easy handle

**Files:**
- Create: `src/curl_prelude.rs`
- Create: `src/curl_prelude/detect.rs`
- Modify: `src/lib.rs`, `src/main.rs`, `src/pipeline.rs` (inject after hash prelude, before name-resolve)
- Create: `src/builtins/curl/mod.rs` and the homes listed below
- Modify: `src/builtins/mod.rs` (`mod curl;`)
- Modify: `src/builtins/spec.rs` (`Area::Curl`)
- Modify: `src/ir/runtime_fn.rs` (ids + `Bridge("elephc_curl")` + `eir_name`)
- Create: `src/codegen/lower_inst/runtime_functions/group_13.rs`
- Modify: `src/codegen/lower_inst/runtime_functions.rs` (call group 13)
- Create: `src/codegen/lower_inst/builtins/curl/mod.rs` and one lowerer per id
- Test: `src/curl_prelude/detect.rs` (mirror hash-prelude detection tests)

**Internal builtins (PHP names are prelude wrappers):**

| Builtin | RuntimeFnId | Args | Returns |
|---|---|---|---|
| `__elephc_curl_easy_init` | `CurlEasyInit` | `url: Mixed` | `Mixed` (raw handle or false) |
| `__elephc_curl_easy_setopt` | `CurlEasySetopt` | `handle: Mixed, option: Int, value: Mixed` | `Bool` |
| `__elephc_curl_easy_exec` | `CurlEasyExec` | `handle: Mixed` | `Mixed` |
| `__elephc_curl_easy_errno` | `CurlEasyErrno` | `handle: Mixed` | `Int` |
| `__elephc_curl_easy_error` | `CurlEasyError` | `handle: Mixed` | `Str` |
| `__elephc_curl_easy_getinfo` | `CurlEasyGetinfo` | `handle: Mixed, option: Mixed` | `Mixed` |
| `__elephc_curl_easy_close` | `CurlEasyClose` | `handle: Mixed` | `Void` |
| `__elephc_curl_easy_reset` | `CurlEasyReset` | `handle: Mixed` | `Void` |
| `__elephc_curl_easy_copy` | `CurlEasyCopy` | `handle: Mixed` | `Mixed` |
| `__elephc_curl_easy_escape` | `CurlEasyEscape` | `handle: Mixed, string: Str` | `Mixed` |
| `__elephc_curl_easy_unescape` | `CurlEasyUnescape` | `handle: Mixed, string: Str` | `Mixed` |
| `__elephc_curl_easy_pause` | `CurlEasyPause` | `handle: Mixed, flags: Int` | `Int` |
| `__elephc_curl_easy_upkeep` | `CurlEasyUpkeep` | `handle: Mixed` | `Bool` |
| `__elephc_curl_strerror` | `CurlStrerror` | `code: Int` | `Mixed` |
| `__elephc_curl_version` | `CurlVersion` | none | `Mixed` |

Each home is `internal: true`. PHP-visible names come from the prelude so
`function_exists('curl_init')` is true once the prelude is injected.

Detection names (case-insensitive last segment):

```text
curl_init, curl_setopt, curl_setopt_array, curl_exec, curl_close,
curl_copy_handle, curl_errno, curl_error, curl_escape, curl_unescape,
curl_getinfo, curl_pause, curl_reset, curl_upkeep, curl_version,
curl_strerror, curl_file_create,
curl_multi_*, curl_share_*,
CurlHandle, CurlMultiHandle, CurlShareHandle, CurlSharePersistentHandle,
CURLFile, CURLStringFile
```

Also detect `CURLOPT_*` / `CURLINFO_*` / `CURLE_*` / `CURL_*` constants so a
program that only mentions `CURLOPT_URL` still injects the prelude and
links the bridge.

Prelude sketch (normative shape, not every method yet):

```php
<?php
final class CurlHandle {
    public mixed $__elephc_handle = null;
    private function __construct() {}
    public static function __elephc_wrap(mixed $raw): CurlHandle {
        $h = new self();
        $h->__elephc_handle = $raw;
        return $h;
    }
    public function __debugInfo(): array { return []; }
    public function __serialize(): array {
        throw new \Exception("Serialization of 'CurlHandle' is not allowed");
    }
}

function curl_init(?string $url = null): CurlHandle|false {
    $raw = __elephc_curl_easy_init($url);
    if ($raw === false) { return false; }
    return CurlHandle::__elephc_wrap($raw);
}

function curl_setopt(CurlHandle $handle, int $option, mixed $value): bool {
    $raw = $handle->__elephc_handle;
    return __elephc_curl_easy_setopt($raw, $option, $value);
}

function curl_exec(CurlHandle $handle): string|bool {
    $raw = $handle->__elephc_handle;
    return __elephc_curl_easy_exec($raw);
}

function curl_close(CurlHandle $handle): void {}
```

`curl_setopt_array` is a prelude loop over `curl_setopt` that stops on the
first `false`, matching PHP.

- [x] **Step 1: Write failing detection tests** (copy the hash-prelude table)

- [x] **Step 2: Write a failing codegen test that `curl_init()` returns an object**

```rust
#[test]
fn curl_init_returns_curlhandle() {
    let out = compile_and_run(
        r#"<?php
        $ch = curl_init();
        echo get_class($ch), "\n";
        echo ($ch instanceof CurlHandle) ? "yes\n" : "no\n";
        "#,
    );
    assert_eq!(out, "CurlHandle\nyes\n");
}
```

Expected first run: `Call to undefined function curl_init`.

- [x] **Step 3: Implement prelude, homes, RuntimeFnIds, group 13, lowerers**

Lowerers call the C ABI from Task 3 through the existing target-aware call
helpers. Do not hardcode ARM64 register names. `CurlEasyInit` result
ownership is `Fresh` (new handle cell). `CurlEasyExec` body string is
`Fresh`. `CurlEasyErrno` / `CurlEasyClose` are `NonHeap`.

- [x] **Step 4: Re-run the object test and a no-network `curl_version()` test**

```rust
#[test]
fn curl_version_reports_pinned_libcurl() {
    let out = compile_and_run(
        r#"<?php
        $v = curl_version();
        echo $v['version'], "\n";
        echo in_array('http', $v['protocols'], true) ? "http\n" : "no-http\n";
        "#,
    );
    assert!(out.starts_with("8.21.0\n"), "{out}");
    assert!(out.contains("http\n"), "{out}");
}
```

- [x] **Step 5: Commit**

```bash
git add src/curl_prelude.rs src/curl_prelude src/builtins/curl src/builtins/mod.rs \
  src/builtins/spec.rs src/ir/runtime_fn.rs src/codegen/lower_inst \
  src/pipeline.rs src/lib.rs src/main.rs tests/codegen/curl
git commit -m "feat: inject curl prelude and easy-handle builtins"
```

---

### Task 6: Constants

**Files:**
- Create: `src/types/curl_constants.rs` (generated from `scripts/docs/curl_surface.json`, committed)
- Modify: `src/types/checker/driver/init.rs`
- Modify: `src/codegen_support/prescan.rs`
- Modify: `src/name_resolver/names.rs` (`is_builtin_global_constant`)
- Modify: magician constant tables in the magician constants task, or here if magician already reads the same slice — do not fork values
- Test: `src/types/curl_constants.rs` (no duplicate names; `CURLOPT_RETURNTRANSFER` and `CURLOPT_URL` match the frozen JSON)

Constants are always visible (like `JSON_*`), even in programs that do not
link curl. Using a constant as a *value* does not need libcurl; *executing*
a curl function does.

- [x] **Step 1: Generate `CURL_INT_CONSTANTS` from the frozen JSON**

Keep a tiny generator or a checked-in table plus a unit test that
re-parses the JSON. Do not hand-type 300 values.

- [x] **Step 2: Codegen test**

```rust
#[test]
fn curlopt_url_is_defined_and_stable() {
    let out = compile_and_run("<?php echo CURLOPT_URL;");
    let expected = /* value from curl_surface.json */;
    assert_eq!(out, expected.to_string());
}
```

- [x] **Step 3: Commit**

```bash
git add src/types/curl_constants.rs src/types/checker/driver/init.rs \
  src/codegen_support/prescan.rs src/name_resolver/names.rs tests/codegen/curl
git commit -m "feat: register PHP curl constants from the frozen surface"
```

---

### Task 7: Local HTTP fixture and first real transfer

**Files:**
- Create: `tests/codegen/curl/http_fixture.rs` (loopback server helper)
- Create: `tests/codegen/curl/easy_http.rs`
- Create: `examples/curl-get/main.php` + `.gitignore` (`*.s`, `*.o`, `main`)
- Modify: `tests/codegen/mod.rs` (`mod curl;`)
- Modify: `tests/codegen/curl/mod.rs`

Fixture: bind `127.0.0.1:0`, serve a single GET `/hello` → `200` /
`text/plain` / `hello-curl`, and `/status` → `204`. Mirror the TLS helper
in `tests/codegen/io/streams.rs` for HTTPS in Task 8, not here.

- [x] **Step 1: Failing GET test**

```rust
#[test]
fn curl_get_returntransfer_localhost() {
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        echo curl_exec($ch);
        echo "\n", curl_getinfo($ch, CURLINFO_HTTP_CODE);
        "#
    ));
    assert_eq!(out, "hello-curl\n200");
}
```

- [x] **Step 2: Implement `CURLOPT_URL`, `CURLOPT_RETURNTRANSFER`, `CURLINFO_HTTP_CODE`, and `curl_getinfo` without an option (array form can wait one commit if the option form is enough for this test)**

- [x] **Step 3: Default-stdout test**

```rust
#[test]
fn curl_exec_writes_stdout_without_returntransfer() {
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        $ok = curl_exec($ch);
        echo $ok === true ? "T" : "F";
        "#
    ));
    assert_eq!(out, "hello-curlT");
}
```

- [x] **Step 4: Connection-refused error shape**

```rust
#[test]
fn curl_exec_connection_refused_returns_false() {
    let out = compile_and_run(
        r#"<?php
        $ch = curl_init("http://127.0.0.1:1/");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        $r = curl_exec($ch);
        echo $r === false ? "F" : "X";
        echo curl_errno($ch) === 0 ? "0" : "E";
        echo strlen(curl_error($ch)) > 0 ? "M" : "N";
        "#,
    );
    assert_eq!(out, "FEM");
}
```

- [x] **Step 5: Commit**

```bash
git add tests/codegen/curl examples/curl-get crates/elephc-curl src
git commit -m "feat: execute HTTP GET through curl_exec"
```

---

### Task 8: Easy-option waves

Implement `curl_setopt` kinds in this order. Each wave has its own commit
and at least one local-server codegen test. Reject unknown kinds with
PHP's warning + `false`.

**Wave A — longs / bools used by almost every app**

`CURLOPT_URL` (already), `CURLOPT_RETURNTRANSFER`, `CURLOPT_FOLLOWLOCATION`,
`CURLOPT_MAXREDIRS`, `CURLOPT_TIMEOUT`, `CURLOPT_TIMEOUT_MS`,
`CURLOPT_CONNECTTIMEOUT`, `CURLOPT_CONNECTTIMEOUT_MS`, `CURLOPT_PROTOCOLS`,
`CURLOPT_REDIR_PROTOCOLS`, `CURLOPT_SSL_VERIFYPEER`, `CURLOPT_SSL_VERIFYHOST`,
`CURLOPT_HTTP_VERSION`, `CURLOPT_NOBODY`, `CURLOPT_HEADER`, `CURLOPT_VERBOSE`,
`CURLOPT_FAILONERROR`, `CURLOPT_POST`, `CURLOPT_PUT`, `CURLOPT_CUSTOMREQUEST`,
`CURLOPT_USERAGENT`, `CURLOPT_REFERER`, `CURLOPT_COOKIE`, `CURLOPT_COOKIEFILE`,
`CURLOPT_COOKIEJAR`, `CURLOPT_USERPWD`, `CURLOPT_HTTPAUTH`, `CURLOPT_PROXY`,
`CURLOPT_PROXYPORT`, `CURLOPT_PROXYTYPE`, `CURLOPT_HTTPPROXYTUNNEL`,
`CURLOPT_FRESH_CONNECT`, `CURLOPT_FORBID_REUSE`, `CURLOPT_TCP_NODELAY`,
`CURLOPT_IPRESOLVE`, `CURLOPT_DNS_CACHE_TIMEOUT`, `CURLOPT_BUFFERSIZE`,
`CURLOPT_MAXFILESIZE`, `CURLOPT_LOW_SPEED_LIMIT`, `CURLOPT_LOW_SPEED_TIME`,
`CURLOPT_PORT`, `CURLOPT_UNRESTRICTED_AUTH`, `CURLOPT_AUTOREFERER`,
`CURLOPT_POSTREDIR`, `CURLOPT_PROTOCOLS_STR` / `CURLOPT_REDIR_PROTOCOLS_STR`
if present in the frozen PHP 8.5 table.

**Wave B — strings / slists / POST body**

`CURLOPT_POSTFIELDS` (string → raw body; array → `application/x-www-form-urlencoded`
or multipart when a `CURLFile` is present — `CURLFile` lands in Task 11;
until then arrays of scalars encode as form fields),
`CURLOPT_HTTPHEADER` (string list → `curl_slist`),
`CURLOPT_HTTP200ALIASES`, `CURLOPT_QUOTE`, `CURLOPT_POSTQUOTE`,
`CURLOPT_PREQUOTE`, `CURLOPT_RESOLVE`, `CURLOPT_CONNECT_TO`,
`CURLOPT_MAIL_RCPT` (accept the option, fail later if SMTP is disabled),
`CURLOPT_ACCEPT_ENCODING`, `CURLOPT_ENCODING`, `CURLOPT_RANGE`,
`CURLOPT_SSLCERT`, `CURLOPT_SSLKEY`, `CURLOPT_CAINFO`, `CURLOPT_CAPATH`,
`CURLOPT_COOKIESESSION`, `CURLOPT_KEYPASSWD`, `CURLOPT_SSH_…` (not built
in → `false` + warning).

**Wave C — info keys**

Every `CURLINFO_*` in the frozen table. Missing keys return `false` or
PHP's documented empty/zero, never invent fields. Array-form `curl_getinfo`
keys must match PHP's names (`http_code` not `CURLINFO_HTTP_CODE`).

**Wave D — copy/reset/escape/pause/upkeep**

`curl_copy_handle` duplicates the easy handle **and** PHP-layer flags
(RETURNTRANSFER, captured body). `curl_reset` clears libcurl options and
PHP-layer flags. `curl_escape` / `curl_unescape` use `curl_easy_escape`.
`curl_pause` uses `CURLPAUSE_*`. `curl_upkeep` forwards `curl_easy_upkeep`.

**Wave E — HTTPS against the local TLS fixture**

Reuse `tests/codegen/io/streams.rs`'s self-signed server. Test
`CURLOPT_SSL_VERIFYPEER=false` success and `true` failure against that
cert. A `#[ignore]` live `https://example.com` smoke is optional.

- [x] **Step 1: Add a table-driven “option is accepted or rejected” unit test in `elephc-curl`**

Every frozen `CURLOPT_*` is either implemented or returns the documented
unsupported status. The test fails if a new JSON constant is neither.

- [x] **Step 2: Implement waves A–E with one commit per wave**

- [x] **Step 3: Error tests**

```rust
#[test]
fn curl_setopt_rejects_wrong_handle_type() {
    let err = compile_and_run_expect_failure(
        r#"<?php curl_setopt(1, CURLOPT_URL, "http://127.0.0.1/");"#,
    );
    assert!(err.contains("CurlHandle"), "{err}");
}
```

---

### Task 9: Multi interface

**Files:**
- Create: `crates/elephc-curl/src/multi.rs`
- Create internal builtins `__elephc_curl_multi_*`
- Extend prelude with `CurlMultiHandle` and the `curl_multi_*` wrappers
- `curl_multi_exec` `$still_running` is `ref` (same check as `fsockopen`)
- `curl_multi_info_read` `$queued_messages` is optional `ref`

**Interfaces:**
- New ABI: `elephc_curl_multi_init/add/remove/perform/select/info_read/setopt/errno/strerror/free`
- `info_read` returns a packed result the lowerer turns into
  `['msg' => CURLMSG_DONE, 'result' => CURLE_*, 'handle' => CurlHandle]`

PHP 8.5 `curl_multi_get_handles`: return the easy handles currently
attached, in add order. Gate the prelude function with `--php-version`.

- [x] **Step 1: Failing parallel GET test**

Two local paths `/a` and `/b`. Add both handles, loop `curl_multi_exec`
until `$running == 0`, assert both bodies via `curl_multi_getcontent`.

- [x] **Step 2: Implement and commit**

```bash
git commit -m "feat: add curl_multi interface"
```

---

### Task 10: Share interface

**Files:**
- Create: `crates/elephc-curl/src/share.rs`
- Prelude: `CurlShareHandle`, PHP 8.5 `CurlSharePersistentHandle`
- Options: `CURLSHOPT_SHARE` / `CURLSHOPT_UNSHARE` with
  `CURL_LOCK_DATA_COOKIE`, `CURL_LOCK_DATA_DNS`, `CURL_LOCK_DATA_SSL_SESSION`,
  `CURL_LOCK_DATA_CONNECT`, `CURL_LOCK_DATA_PSL`, `CURL_LOCK_DATA_HSTS`
- `curl_share_init_persistent` (8.5): process-lifetime share keyed by the
  option set. Document that elephc has no PHP-FPM worker restart; the
  handle lives until process exit.

- [x] **Step 1: Failing DNS-share test**

Two sequential GETs to the same host on one share with
`CURL_LOCK_DATA_DNS`. Assert both succeed. A stricter “second lookup is
cached” assertion is optional if `CURLINFO_NAMELOOKUP_TIME` is stable
enough; do not flake CI on timing.

- [x] **Step 2: Implement and commit**

```bash
git commit -m "feat: add curl_share interface"
```

---

### Task 11: `CURLFile` / `CURLStringFile`

**Files:**
- Prelude classes with public properties and the six `CURLFile` methods
- `curl_file_create` prelude wrapper
- `CURLOPT_POSTFIELDS` array walker in `elephc-curl`:
  - scalar → form field
  - `CURLFile` → `curl_mime` / `CURLFORM_FILE` file part
  - `CURLStringFile` → in-memory mime part

- [x] **Step 1: Local upload fixture**

Server echoes `Content-Type` and the uploaded field filename. Compile:

```php
$ch = curl_init($url);
$cfile = new CURLFile($path, 'text/plain', 'hello.txt');
curl_setopt($ch, CURLOPT_POST, true);
curl_setopt($ch, CURLOPT_POSTFIELDS, ['f' => $cfile]);
curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
echo curl_exec($ch);
```

- [x] **Step 2: Implement and commit**

```bash
git commit -m "feat: add CURLFile and CURLStringFile uploads"
```

---

### Task 12: Callbacks

**Files:**
- `crates/elephc-curl/src/callbacks.rs`
- Runtime trampolines that box arguments and call
  `callable_descriptor_invoke`
- First-wave callbacks only:

| Option | PHP callable signature |
|---|---|
| `CURLOPT_WRITEFUNCTION` | `fn(CurlHandle $ch, string $data): int` |
| `CURLOPT_HEADERFUNCTION` | `fn(CurlHandle $ch, string $header): int` |
| `CURLOPT_READFUNCTION` | `fn(CurlHandle $ch, $fd, int $length): string` |
| `CURLOPT_PROGRESSFUNCTION` | `fn($ch, $dltotal, $dlnow, $ultotal, $ulnow): int` |
| `CURLOPT_XFERINFOFUNCTION` | same as progress |
| `CURLOPT_DEBUGFUNCTION` | `fn($ch, int $type, string $data): int` |

Return-length rules match PHP (write/header must return `strlen($data)`
or the transfer is aborted). Store the callable on `EasyEntry` so it
outlives `curl_setopt`.

Remaining libcurl callbacks (`OPENSOCKETFUNCTION`, `SOCKOPTFUNCTION`,
`FNMATCH_FUNCTION`, `TRAILERFUNCTION`, `PREREQFUNCTION`, HSTS read/write,
`SSL_CTX_FUNCTION`) stay rejected with the unsupported-option warning
until a follow-up. List them in `docs/php/curl.md`.

- [x] **Step 1: Failing WRITEFUNCTION test**

```php
$buf = '';
curl_setopt($ch, CURLOPT_WRITEFUNCTION, function ($ch, $data) use (&$buf) {
    $buf .= $data;
    return strlen($data);
});
curl_exec($ch);
echo $buf;
```

- [x] **Step 2: Implement trampolines on both targets and commit**

```bash
git commit -m "feat: invoke PHP callables from libcurl callbacks"
```

---

### Task 13: Magician

**Files:**
- Create: `crates/elephc-magician/src/interpreter/builtins/curl/` (one `eval_builtin!` home per PHP-visible function)
- Modify: `crates/elephc-magician/src/interpreter/builtins/mod.rs`
- Extend: magician object/handle storage analogously to `HashContext` — **do not** consume stream resource ids
- Call the same `elephc_curl_*` symbols; do not reimplement HTTP in the interpreter
- Magician `extension_loaded('curl')` stays `false` unless the eval host linked the bridge (document if eval cannot load it). Prefer linking the bridge into magician when curl builtins are compiled in, so AOT and eval do not diverge

- [x] **Step 1: Eval parity test**

`tests/codegen/eval_builtin_parity.rs` (or a curl sibling): `eval('return curl_version();')['version']` equals the AOT `curl_version()` string.

- [x] **Step 2: Implement and commit**

```bash
git commit -m "feat: add magician curl builtins on the shared ABI"
```

---

### Task 14: Docs, audit, and hygiene

**Files:**
- Create: `docs/php/curl.md` (Astro frontmatter, no top-level `#`)
- Modify: `docs/README.md` index
- Modify: `docs/php/compatibility.md` if any intentional divergence remains
- Run the builtin-docs skill sequence after PHP-visible names exist
- Modify: `src/builtins/parity_tests.rs` only if a prelude accidentally calls a PHP-visible extension builtin — preludes must call `__elephc_*`
- Modify: `CHANGELOG.md` is **not** updated here; release skill owns `[Unreleased]`

`docs/php/curl.md` must state:

- pinned libcurl 8.21.0
- protocol matrix
- OpenSSL is only libcurl's TLS backend
- PHP `openssl_*` is still RustCrypto
- which callbacks are implemented
- `elephc native add curl`
- `--with-curl`

- [x] **Step 1: Generate builtin docs**

```bash
cargo build --example gen_builtins
python3 scripts/docs/extract_builtins.py --render --force
python3 scripts/docs/audit_builtins.py
python3 scripts/docs/elephc_builtins/validate_site_compat.py
```

- [x] **Step 2: Focused verification**

```bash
cargo build
cargo test --test codegen_tests curl
cargo test --test extension_loaded_tests
git diff --check
```

Do not run the full suite locally unless asked.

- [x] **Step 3: Final audit against `scripts/docs/curl_surface.json`**

Every function, class, and constant has a home. Every `CURLOPT_*` is
implemented or documented as rejected. Tick the checklist at the top of
this plan only when that audit is clean.

- [x] **Step 4: Commit**

```bash
git add docs scripts/docs src/builtins/parity_tests.rs
git commit -m "docs: document the PHP curl extension surface"
```

## Testing rules

| Kind | Where |
|---|---|
| Detection / prelude | `src/curl_prelude/detect.rs` |
| Constants | `src/types/curl_constants.rs` + codegen |
| Easy / multi / share / file / callbacks | `tests/codegen/curl/` |
| Errors | `tests/error_tests/curl.rs` |
| `extension_loaded` | `tests/extension_loaded_tests.rs` |
| Magician | eval parity + magician unit tests |
| Native recipe | `src/native_deps/` existing dispatcher tests |
| Ownership | `tests/codegen/runtime_gc/` — CurlHandle free is balanced (`--heap-debug`) |
| Case-insensitive PHP names | one codegen test (`CURL_INIT` / `\\curl_init`) |

Network: loopback only. HTTPS: local self-signed fixture. PHP cross-check
(`ELEPHC_PHP_CHECK=1`) is optional and only for option/error wording, never
for `curl_version()['version']` (ours is 8.21.0, the host PHP may differ).

## Explicit non-goals

- Linking a Zend `curl.so` (v0.28 consumer PoC)
- Replacing `file_get_contents('https://…')` / `fopen` HTTP(S) wrappers
  (those stay on `elephc-tls`)
- HTTP/2, HTTP/3, HTTP/3 QUIC, SSH/SFTP, LDAP, SMTP, IMAP, MQTT as required
  protocols in the first landing
- System libcurl / Homebrew curl
- Using `ureq` / `reqwest` as the curl backend
- Making `extern "curl"` the supported PHP API
- Serializing `CurlHandle` (throw, like an honest HashContext)

## Suggested PR cuts

1. Tasks 1–4 (pins, native packages, crate, linker) — no PHP functions yet
   except maybe a hidden smoke.
2. Tasks 5–7 (prelude, constants, HTTP GET).
3. Task 8 waves (options / info / HTTPS).
4. Tasks 9–11 (multi, share, files).
5. Tasks 12–14 (callbacks, magician, docs).

Each PR must keep `cargo build` warning-free and the focused curl tests green.
)
