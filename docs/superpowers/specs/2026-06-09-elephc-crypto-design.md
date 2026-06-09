# elephc-crypto — Design Spec

**Date:** 2026-06-09
**Branch:** `feat/elephc-crypto`
**Status:** Approved design, pending implementation plan

## Goal

Extract all hashing/crypto into a dedicated pure-Rust runtime staticlib, `crates/elephc-crypto`, following the established `elephc-tls` / `elephc-pdo` bridge model. This:

1. **Closes the PHP hash gap.** Today elephc supports only `md5`/`sha1`/`sha256` via `hash()`, plus standalone `md5()`/`sha1()`/`crc32()`. PHP's `hash_algos()` lists ~50 algorithms and a full hash function family (`hash_hmac`, `hash_file`, `hash_equals`, `hash_init`/`update`/`final`/`copy`). We implement the full module.
2. **Removes the system-crypto fork.** Today macOS uses CommonCrypto (`CC_SHA1`, …) and Linux uses libcrypto/OpenSSL (`.weak MD5/SHA1/SHA256` + `-lcrypto` + a `CC_*→*` platform transform). RustCrypto is a single source of truth across all supported targets, musl-friendly, no system crypto dependency.
3. **Fixes latent/compat bugs.** `md5()`/`sha1()` already declare a `$binary` parameter in `signatures.rs` but codegen ignores it (always returns hex). Unknown algorithms currently return an empty string; PHP 8 throws `ValueError`.

### Supported-target policy

This is a target-sensitive change (new ABI, link path, removal of libcrypto/weak symbols/platform transform). It must land for `macos-aarch64`, `linux-aarch64`, and `linux-x86_64` together, verified with the Docker Linux scripts plus local macOS tests. RustCrypto is pure Rust and builds static under musl.

## Non-goals

- Chasing 1:1 parity with every PHP algorithm. We cover every algorithm that has a maintained pure-Rust implementation and **document the gaps** with a coherent `ValueError`.
- Compile-time validation of unknown algorithm literals (PHP validates at runtime). The ABI passes the algorithm name string so dynamic `$algo` works and Rust stays the single source of truth. An optional compile-time literal allow-list check can be added later by mirroring the name list.
- New phar signature types (MD5/SHA256/SHA512). Phar keeps SHA1 signing; only the implementation moves off CommonCrypto/libcrypto.

## Chosen approach: "thin Rust, fat runtime" (Approach A)

elephc-crypto is a pure "bytes in → raw digest out" engine. Everything elephc already does well stays in the existing runtime/codegen: hex formatting, the `$binary` flag, file reading for `hash_file`, the timing-safe compare for `hash_equals`, ownership/GC of returned strings, and `ValueError`. RustCrypto is the single source of truth for **algorithms only**.

Rejected alternatives:
- **Approach B (fat Rust):** Rust returns ready hex/binary strings and reads files. Rejected — it duplicates capabilities elephc has, and a Rust-allocated result string crossing the ABI breaks the runtime string-heap/COW ownership contract.
- **Approach C (compile-time integer id enum):** resolve algorithm to a stable id, enabling compile-time `ValueError` for literals. Reasonable fallback, but adds an extra ABI function and an id enum to keep in sync. Passing the name string keeps a single source of truth with no id table.

## Component 1 — `crates/elephc-crypto`

Mirrors `elephc-tls`: `crate-type = ["staticlib", "rlib"]`, one `src/lib.rs` (split into `algos.rs` + `lib.rs` if it grows past the cohesion threshold).

### Dependencies (pure-Rust, musl-friendly)

`digest` (for `DynDigest` + `box_clone`), `md-5`, `md2`, `md4`, `sha1`, `sha2`, `sha3`, `ripemd`, `whirlpool`, plus the small non-crypto crates `crc32fast` (crc32b), `crc` (the non-`b` crc32 variant and crc32c), `adler2`. No `hmac` crate dependency for the dynamic path (see below).

### Algorithm table (single source of truth)

A map `name → (constructor: fn() -> Box<dyn DynDigest>, output_size, block_size)`.

**Covered:**
- crypto: `md2`, `md4`, `md5`, `sha1`, `sha224`, `sha256`, `sha384`, `sha512`, `sha512/224`, `sha512/256`, `sha3-224`, `sha3-256`, `sha3-384`, `sha3-512`, `ripemd128`, `ripemd160`, `ripemd256`, `ripemd320`, `whirlpool`
- non-crypto: `crc32`, `crc32b`, `crc32c`, `adler32`, `fnv1a32`, `fnv1a64`, `fnv132`, `fnv164`, `joaat`

  Note: `blake2` (`blake2b-512`, `blake2s-256`) is **not** in PHP's `hash_algos()` and is intentionally excluded from elephc-crypto's algorithm table. It is not a gap — it is simply outside PHP's supported set.

  **crc32 vs crc32b:** PHP exposes two distinct CRC-32 variants with different polynomials/reflection. `crc32b` (CRC-32/ISO-HDLC, reflected) is what `crc32fast` computes and what PHP's `crc32()` *function* returns; `crc32` (PHP's non-`b` `hash()` algorithm) is a different variant and needs the `crc` crate with the matching parameterization. Both must be cross-checked against `php -r 'echo hash("crc32", ...);'` / `hash("crc32b", ...)` to match byte-for-byte. The existing `__rt_crc32` (used by `crc32()` and phar entry checksums) already computes the `crc32b` value and stays as-is.

**Documented gaps** (no maintained pure-Rust impl): `tiger`/`tiger128`/`tiger160`, `snefru`/`snefru256`, `gost`/`gost-crypto`, `haval*`, `murmur3*`, `xxh*`. These produce a coherent `ValueError` and are listed in the docs. Added individually later if needed.

Max raw digest size across the covered set is 64 bytes (sha512 / sha3-512 / whirlpool / blake2b-512), so the caller-provided output buffer is fixed at 64 bytes.

### HMAC

Implemented **generically over `DynDigest`** rather than via a per-type `Hmac<D>` match:
`HMAC(key, msg) = H((k' ⊕ opad) ‖ H((k' ⊕ ipad) ‖ msg))`, where the key is hashed down if longer than `block_size` and zero-padded to `block_size`. The required `block_size` comes from the algorithm table.

### C ABI (raw digests, keyed by algorithm name)

```
// one-shot: write raw digest into out, return byte count, -1 = unknown algorithm
elephc_crypto_hash(name_ptr, name_len, data_ptr, data_len, out_ptr) -> isize
elephc_crypto_hmac(name_ptr, name_len, key_ptr, key_len, data_ptr, data_len, out_ptr) -> isize

// incremental (HashContext)
elephc_crypto_init(name_ptr, name_len) -> *mut ctx              // plain digest; null = unknown algorithm
elephc_crypto_init_hmac(name_ptr, name_len, key_ptr, key_len) -> *mut ctx  // HASH_HMAC mode; null = unknown algorithm
elephc_crypto_update(ctx, data_ptr, data_len)
elephc_crypto_final(ctx, out_ptr) -> isize           // consumes + frees ctx (PHP semantics)
elephc_crypto_clone(ctx) -> *mut ctx                 // backs hash_copy / clone $ctx
elephc_crypto_free(ctx)                              // cleanup on error/scope-exit paths
```

`ctx` is an opaque 8-byte pointer (the `PhpType::Resource(Some("hash"))` handle) to a heap enum: a plain `Box<dyn DynDigest>`, or an HMAC-streaming state (an inner `DynDigest` already fed with `key ⊕ ipad`, plus the stored `key ⊕ opad` and algorithm so `final` can compute the outer hash). `elephc_crypto_init_hmac` backs `hash_init($algo, HASH_HMAC, $key)`. `update`/`final`/`clone`/`free` are agnostic to which variant the `ctx` holds. All functions are `#[no_mangle] pub extern "C"`.

## Component 2 — codegen & runtime integration (target-aware ARM64 + x86_64)

- **BSS slots** in `src/codegen/runtime/data/fixed.rs`: `_elephc_crypto_hash_fn`, `_elephc_crypto_hmac_fn`, `_elephc_crypto_init_fn`, `_elephc_crypto_init_hmac_fn`, `_elephc_crypto_update_fn`, `_elephc_crypto_final_fn`, `_elephc_crypto_clone_fn`, `_elephc_crypto_free_fn` (zero-initialized).
- **`publish_elephc_crypto_function_pointers()`** in a new `src/codegen/builtins/strings/hash_crypto.rs` (mirrors `publish_tls_function_pointers`): writes the C function addresses into the slots before first use. Fail-closed indirection (`cbz`/`test` before each `blr`/`call`) so programs without hashing never link the crate and a missing runtime fails closed.
- **Runtime helpers** (`src/codegen/runtime/strings/` and a new hash module):
  - `__rt_hash_generic` — evaluates the algorithm string + data, calls the indirected `_elephc_crypto_hash_fn`, gets the raw digest, then hex-formats (reusing the existing bin2hex/hex path) **or** returns raw bytes per `$binary`. Replaces the current md5/sha1/sha256-only `__rt_hash`.
  - `__rt_md5` / `__rt_sha1` rerouted through elephc-crypto and now honoring `$binary` (closes the latent gap).
  - `__rt_hash_hmac`, `__rt_hash_file` (file read via the existing file-get-contents path, then hash), `__rt_hash_equals` (**pure timing-safe compare — does NOT touch the crate**; returns false immediately on length mismatch, otherwise XOR-accumulates over the full length).
  - Incremental `__rt_hash_init` / `__rt_hash_update` / `__rt_hash_final` — thin wrappers over the indirected slots, producing/consuming `Resource("hash")` handles; `__rt_hash_copy` over `_clone_fn`. `hash_init` with the `HASH_HMAC` flag and a key routes to `_elephc_crypto_init_hmac_fn` instead of `_init_fn`; flag `0` uses `_init_fn`.
- **`$binary` flag** honored for `hash`, `md5`, `sha1`, `hash_hmac`, `hash_file`.

## Component 3 — type system / checker / catalog / signatures

- **`src/types/checker/builtins/catalog.rs`**: add `hash_hmac`, `hash_file`, `hash_equals`, `hash_init`, `hash_update`, `hash_final`, `hash_copy`, `hash_algos` (existing `hash`/`md5`/`sha1`/`crc32` stay). This drives `function_exists()`, case-insensitive lookup, and namespace fallback automatically.
- **`src/types/signatures.rs`**:
  - Fix `hash` → `optional(["algo","data","binary"], 2, [false])`.
  - `hash_hmac` → `optional(["algo","key","data","binary"], 3, [false])`
  - `hash_file` → `optional(["algo","filename","binary"], 2, [false])`
  - `hash_equals` → `fixed(["known_string","user_string"])`
  - `hash_init` → `optional(["algo","flags","key"], 1, [int 0, ""])`
  - `hash_update` → `fixed(["context","data"])`
  - `hash_final` → `optional(["context","binary"], 1, [false])`
  - `hash_copy` → `fixed(["context"])`
  - `hash_algos` → no args
- **`src/types/checker/builtins/` hash category** (new file or extend `strings.rs`): argument validation, return types, and the **conditional-link swap**: replace `require_linux_builtin_library("crypto")` with `require_builtin_library("elephc_crypto")` for every hashing builtin (and for phar write mode). This is what moves the link from libcrypto to the new crate.
- **`Resource(Some("hash"))`** for `HashContext`: `hash_init`/`hash_copy` return it; `hash_update`/`hash_final` accept it; `hash_final` consumes it.
- **Optimizer effects** (`src/optimize/effects/builtins.rs`): `hash`/`md5`/`sha1`/`hash_hmac`/`hash_equals`/`hash_algos` are pure (read args, allocate a string); `hash_file` reads the filesystem (not pure); `hash_init`/`hash_update`/`hash_final`/`hash_copy` carry state/effects.
- **Codegen return-type table** (`src/codegen/functions/types/builtins.rs`): `hash`/`md5`/`sha1`/`hash_hmac`/`hash_file`/`hash_final` → `Str`; `hash_init`/`hash_copy` → `Resource("hash")`; `hash_equals` → `Bool`; `hash_algos` → `Array(Str)`; `hash_update` → `Bool`.
- **First-class callable**: update `first_class_callable_builtin_sig()` for the new builtins so callable syntax stays coherent.

### Heavy integration points

1. **`ValueError` on unknown algorithm.** elephc supports `throw` and a builtin exception-class catalog (`src/types/checker/builtin_types/exception.rs`). On an unknown algorithm name, the runtime raises `ValueError` (added to the exception catalog if not already present), matching PHP 8 and replacing today's silent empty string.
2. **`HashContext` auto-cleanup.** A context that goes out of scope without `hash_final` must not leak. Extend the function-scope cleanup (`src/codegen/functions/cleanup.rs`) to call `elephc_crypto_free` on owned `Resource("hash")` locals, mirroring how owned string locals are released at the epilogue. This is the most invasive hook and lives in the incremental phase.

## Component 4 — phar migration & full fork removal

- Replace both `bl_c("CC_SHA1")` call sites in `src/codegen/runtime/io/phar_write.rs` (ARM64 + x86_64) with a SHA1 one-shot through elephc-crypto (a `__rt_sha1_raw` routing the `_elephc_crypto_hash_fn` slot with `"sha1"`). The phar-write path must publish the crypto pointers even when the program uses no other hash builtin, and the checker must register `elephc_crypto` in phar-write mode (currently registers `crypto`).
- Remove CommonCrypto/libcrypto entirely: drop the `.weak MD5/SHA1/SHA256` declarations, remove the crypto entries of the `CC_*→*` platform transform (`src/codegen/platform/linux_transform.rs` / `target.rs`), and remove `require_linux_builtin_library("crypto")` and the `-lcrypto` link path.
- `__rt_crc32` stays (pure asm, no library): `crc32()` and phar entry checksums keep using it. `hash("crc32b")` maps to `crc32fast` in the crate.
- Net result: zero system crypto dependency on every supported target — macOS no longer relies on CommonCrypto, Linux no longer links libcrypto.

## Component 5 — linker bridge

Add one entry to `const BRIDGES` in `src/linker.rs` (the mechanism is fully table-driven):

```rust
BridgeStaticlib {
    lib_name: "elephc_crypto",
    env_var: "ELEPHC_CRYPTO_LIB_DIR",
    crate_name: "elephc-crypto",
    whole_archive: false,        // no link-time side effects (unlike rustls provider registration)
    macos_frameworks: &[],       // pure-Rust, no native transitive deps
    needs_libdl: true,           // Rust runtime symbols, like the other bridges (verify; relax if unused)
}
```

Add `elephc-crypto` to the workspace `members` / `default-members` and as a `[dev-dependencies]` entry in the root `Cargo.toml` so `cargo test` builds `libelephc_crypto.a` automatically (mirrors elephc-tls/elephc-pdo).

## Testing strategy

- **Crate unit tests** (`cargo test -p elephc-crypto`): NIST/RFC known-answer vectors per algorithm; HMAC vectors (RFC 4231); `incremental == one-shot`; `clone` correctness. This is the key payoff of the Rust extraction.
- **Codegen tests** (`tests/codegen/`): `hash()` per supported algorithm vs expected hex; `$binary` raw output; **byte-for-byte parity** of md5/sha1/sha256 with current output (regression); `hash_hmac`; `hash_file`; `hash_equals` (true/false + length mismatch); `incremental == one-shot`; `hash_copy`; `hash_algos` returns the list; unknown algorithm → `ValueError`; case-insensitive + namespaced calls; phar signed-archive still valid (regression).
- **PHP cross-check** (`ELEPHC_PHP_CHECK=1`) over a representative algorithm set.
- **Docker Linux** (`scripts/test-linux-x86_64.sh`, `scripts/test-linux-arm64.sh`): target-sensitive — confirm linking succeeds **without `-lcrypto`** on both Linux targets.
- Full suite + `cargo test -- --include-ignored` + `git diff --check` + assembly-comment alignment before commit.

## Example & docs

- `examples/hashing/main.php` (with its own `.gitignore`): hash() across several algorithms, `hash_hmac`, and incremental hashing.
- Docs: a hashing page under `docs/php/` (supported-algorithm list + documented gaps), remove any "not supported" notes, update internals docs for the new bridge crate and the `HashContext` resource, update `docs/README.md` index, and mark the feature in `ROADMAP.md`.

## Phasing (each phase leaves the tree green and testable)

1. **Crate + bridge.** Scaffold `elephc-crypto`, the algorithm table, the one-shot `elephc_crypto_hash`, the `BRIDGES` entry, and linker discovery. Crate unit tests. No codegen behavior change yet.
2. **Migrate one-shot builtins.** Reroute `hash`/`md5`/`sha1` through elephc-crypto, full algorithm set, `$binary`, `ValueError`. Codegen tests + parity regression. Remove libcrypto/CommonCrypto for these builtins.
3. **hash_hmac, hash_file, hash_equals, hash_algos.** Add functions + tests.
4. **Incremental (HashContext).** `hash_init`/`update`/`final`/`copy`, the `Resource("hash")` type, the scope-cleanup hook. Tests.
5. **phar migration + final fork removal.** Swap `CC_SHA1`, delete the weak symbols + platform transform. Full suite + Docker Linux + `--include-ignored`. Docs/example/roadmap.
