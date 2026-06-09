# elephc-crypto Phase 1 (crate + bridge) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build and unit-test the standalone pure-Rust `crates/elephc-crypto` staticlib (full algorithm table, one-shot hash, HMAC, incremental HashContext) and wire it into the linker's bridge table — with zero codegen behavior change yet.

**Architecture:** A `staticlib`+`rlib` crate exposing a small C ABI of raw-digest functions keyed by algorithm name. Internally a `HashState` trait unifies RustCrypto `DynDigest` algorithms with the non-crypto checksums (crc32, adler32, fnv, joaat). HMAC is implemented generically over `HashState`. The crate is fully validated through Rust unit tests against published test vectors and `php` golden values before any compiler integration. A single table-driven entry is added to `BRIDGES` in `src/linker.rs`.

**Tech Stack:** Rust, RustCrypto (`digest`, `md-5`, `md2`, `md4`, `sha1`, `sha2`, `sha3`, `ripemd`, `whirlpool`, `blake2`), `crc32fast`, `crc`, `adler`, `fnv`.

**Spec:** `docs/superpowers/specs/2026-06-09-elephc-crypto-design.md` (Components 1 and 5; Phase 1).

**Scope note:** This plan is Phase 1 only. Phases 2–5 (codegen one-shot migration, hash_* builtins, incremental wiring, phar migration + fork removal) involve target-aware assembly and get their own plans once Phase 1 is green.

---

## File Structure

- Create: `crates/elephc-crypto/Cargo.toml` — crate manifest (`staticlib`+`rlib`, RustCrypto deps).
- Create: `crates/elephc-crypto/src/lib.rs` — module preamble, `HashState` trait, algorithm table, C ABI (`elephc_crypto_hash`/`_hmac`/`_init`/`_init_hmac`/`_update`/`_final`/`_clone`/`_free`).
- Create: `crates/elephc-crypto/src/algos.rs` — `HashState` impls + `make(name) -> Option<Box<dyn HashState>>` table (kept out of `lib.rs` so each file owns one concern).
- Create: `crates/elephc-crypto/tests/vectors.rs` — integration tests (the crate is an `rlib`, so tests link it normally).
- Modify: `Cargo.toml` (workspace root) — add `crates/elephc-crypto` to `members`, `default-members`, and `[dev-dependencies]`.
- Modify: `src/linker.rs` — add the `elephc_crypto` entry to `const BRIDGES` and a unit test asserting it resolves.

---

### Task 1: Scaffold the crate and wire it into the workspace

**Files:**
- Create: `crates/elephc-crypto/Cargo.toml`
- Create: `crates/elephc-crypto/src/lib.rs`
- Modify: `Cargo.toml` (root)

- [ ] **Step 1: Create the crate manifest**

Create `crates/elephc-crypto/Cargo.toml`:

```toml
[package]
name = "elephc-crypto"
version = "0.1.0"
edition = "2021"
license = "MIT"
description = "Pure-Rust hashing/HMAC bridge staticlib (RustCrypto) for the elephc PHP-to-native compiler's hash() family"
publish = false

# Built as a C-callable staticlib (linked into compiled PHP programs that use
# the hash family) and an rlib (so the bridge is unit-testable via
# `cargo test -p elephc-crypto`). Mirrors crates/elephc-tls and crates/elephc-pdo.
[lib]
crate-type = ["staticlib", "rlib"]

[dependencies]
# Pure-Rust, musl-friendly (Docker test images). `digest` provides the
# DynDigest trait + box_clone used to hold a heterogeneous boxed hasher.
digest = "0.10"
md-5 = "0.10"
md2 = "0.10"
md4 = "0.10"
sha1 = "0.10"
sha2 = "0.10"
sha3 = "0.10"
ripemd = "0.10"
whirlpool = "0.10"
blake2 = "0.10"
# Non-crypto checksums PHP's hash() exposes. crc32fast = crc32b (ISO-HDLC);
# the `crc` crate covers PHP's non-`b` "crc32" variant.
crc32fast = "1"
crc = "3"
adler = "1"
fnv = "1"
```

- [ ] **Step 2: Create a minimal `lib.rs` so the crate compiles**

Create `crates/elephc-crypto/src/lib.rs`:

```rust
//! Purpose:
//! Pure-Rust hashing/HMAC bridge staticlib for elephc's PHP hash() family.
//! Exposes a C ABI of raw-digest functions keyed by algorithm name, consumed by
//! compiled PHP binaries via function-pointer slots (see src/codegen runtime).
//!
//! Called from:
//! - Compiled PHP program assembly through the `_elephc_crypto_*_fn` slots.
//! - `cargo test -p elephc-crypto` (the rlib) for in-isolation validation.
//!
//! Key details:
//! - All ABI functions are `#[no_mangle] pub extern "C"`; raw digests are written
//!   into a caller-provided 64-byte buffer (max digest size across supported algos).
//! - `ctx` handles are thin pointers to a boxed `HashCtx`; `final`/`free` own them.

mod algos;
```

- [ ] **Step 3: Add the crate to the workspace and dev-dependencies**

In the root `Cargo.toml`, add `"crates/elephc-crypto"` to both `members` and `default-members` (alongside the existing `crates/elephc-tls` and `crates/elephc-pdo`), and add to `[dev-dependencies]`:

```toml
elephc-crypto = { path = "crates/elephc-crypto" }
```

- [ ] **Step 4: Verify the crate builds and produces the staticlib**

Run: `cargo build -p elephc-crypto`
Expected: builds clean (zero warnings). Then run `ls target/debug/libelephc_crypto.a`
Expected: the archive exists.

- [ ] **Step 5: Commit**

```bash
git add crates/elephc-crypto/Cargo.toml crates/elephc-crypto/src/lib.rs Cargo.toml
git commit -m "feat(crypto): scaffold elephc-crypto crate and workspace wiring"
```

---

### Task 2: `HashState` trait + crypto algorithm table + one-shot `elephc_crypto_hash`

**Files:**
- Create: `crates/elephc-crypto/src/algos.rs`
- Modify: `crates/elephc-crypto/src/lib.rs`
- Create: `crates/elephc-crypto/tests/vectors.rs`

- [ ] **Step 1: Write the failing test (crypto one-shot vectors)**

Create `crates/elephc-crypto/tests/vectors.rs`:

```rust
//! Purpose:
//! Integration tests validating elephc-crypto digests against published test
//! vectors and PHP golden values.
//!
//! Called from:
//! - `cargo test -p elephc-crypto` through Rust's test harness.
//!
//! Key details:
//! - Calls the C ABI functions directly (the crate links as an rlib in tests).
//! - Vectors are NIST/RFC published values; non-crypto checksums are cross-checked
//!   against `php -r 'echo hash(...);'`.

use elephc_crypto::*;

/// Convenience: run the one-shot C ABI and return the lowercase hex digest, or
/// `None` when the algorithm is unknown (ABI returns -1).
fn hash_hex(algo: &str, data: &[u8]) -> Option<String> {
    let mut out = [0u8; 64];
    let n = unsafe {
        elephc_crypto_hash(
            algo.as_ptr(),
            algo.len(),
            data.as_ptr(),
            data.len(),
            out.as_mut_ptr(),
        )
    };
    if n < 0 {
        return None;
    }
    Some(out[..n as usize].iter().map(|b| format!("{:02x}", b)).collect())
}

#[test]
fn crypto_one_shot_known_vectors() {
    assert_eq!(hash_hex("md5", b"").unwrap(), "d41d8cd98f00b204e9800998ecf8427e");
    assert_eq!(hash_hex("md5", b"abc").unwrap(), "900150983cd24fb0d6963f7d28e17f72");
    assert_eq!(hash_hex("sha1", b"abc").unwrap(), "a9993e364706816aba3e25717850c26c9cd0d89d");
    assert_eq!(
        hash_hex("sha256", b"abc").unwrap(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        hash_hex("sha512", b"abc").unwrap(),
        "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a\
2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
    );
    assert_eq!(
        hash_hex("sha3-256", b"abc").unwrap(),
        "3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532"
    );
    assert_eq!(hash_hex("ripemd160", b"abc").unwrap(), "8eb208f7e05d987a9b044a8e98c6b087f15a0bfc");
}

#[test]
fn unknown_algorithm_returns_negative() {
    assert!(hash_hex("tiger", b"abc").is_none());
    assert!(hash_hex("not-a-hash", b"abc").is_none());
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p elephc-crypto --test vectors`
Expected: FAIL to compile — `elephc_crypto_hash` / the crate API don't exist yet.

- [ ] **Step 3: Implement the `HashState` trait and crypto table in `algos.rs`**

Create `crates/elephc-crypto/src/algos.rs`:

```rust
//! Purpose:
//! Internal hashing state abstraction and the algorithm-name table for
//! elephc-crypto. Unifies RustCrypto DynDigest hashers with non-crypto checksums.
//!
//! Called from:
//! - `crate` (lib.rs) C ABI functions via `make()`.
//!
//! Key details:
//! - `HashState` is object-safe so a heterogeneous hasher lives behind one
//!   `Box<dyn HashState>`. `block_size` feeds the generic HMAC construction.

use digest::DynDigest;

/// Object-safe streaming hash state covering both RustCrypto `DynDigest`
/// algorithms and the non-`DynDigest` checksums (crc32, adler32, fnv, joaat).
pub trait HashState {
    /// Feeds more input into the running digest.
    fn update(&mut self, data: &[u8]);
    /// Consumes the state and returns the raw digest bytes.
    fn finalize_box(self: Box<Self>) -> Vec<u8>;
    /// Raw digest size in bytes (e.g. 32 for sha256).
    fn output_size(&self) -> usize;
    /// Algorithm block size in bytes, used by the HMAC key schedule.
    fn block_size(&self) -> usize;
    /// Clones the running state (backs hash_copy / `clone $ctx`).
    fn box_clone(&self) -> Box<dyn HashState>;
}

/// Wraps any RustCrypto `DynDigest` hasher as a `HashState`, carrying the
/// algorithm block size (DynDigest does not expose it).
struct DigestState {
    inner: Box<dyn DynDigest>,
    block: usize,
}

impl HashState for DigestState {
    /// Feeds input into the boxed DynDigest.
    fn update(&mut self, data: &[u8]) {
        self.inner.update(data);
    }
    /// Finalizes the boxed DynDigest into its raw digest bytes.
    fn finalize_box(self: Box<Self>) -> Vec<u8> {
        self.inner.finalize().to_vec()
    }
    /// Returns the DynDigest's raw output size.
    fn output_size(&self) -> usize {
        self.inner.output_size()
    }
    /// Returns the stored algorithm block size.
    fn block_size(&self) -> usize {
        self.block
    }
    /// Clones the underlying DynDigest state.
    fn box_clone(&self) -> Box<dyn HashState> {
        Box::new(DigestState { inner: self.inner.box_clone(), block: self.block })
    }
}

/// Builds a boxed `DigestState` for a default-constructed RustCrypto hasher.
fn digest_state<D>(block: usize) -> Box<dyn HashState>
where
    D: digest::Digest + Clone + Send + Sync + 'static,
{
    Box::new(DigestState { inner: Box::new(D::new()), block })
}

/// Resolves a PHP hash() algorithm name to a freshly initialized `HashState`,
/// or `None` if the algorithm is unsupported (caller maps to PHP ValueError).
pub fn make(name: &str) -> Option<Box<dyn HashState>> {
    use blake2::{Blake2b512, Blake2s256};
    use ripemd::{Ripemd128, Ripemd160, Ripemd256, Ripemd320};
    use sha2::{Sha224, Sha256, Sha384, Sha512, Sha512_224, Sha512_256};
    use sha3::{Sha3_224, Sha3_256, Sha3_384, Sha3_512};
    Some(match name {
        "md2" => digest_state::<md2::Md2>(16),
        "md4" => digest_state::<md4::Md4>(64),
        "md5" => digest_state::<md5::Md5>(64),
        "sha1" => digest_state::<sha1::Sha1>(64),
        "sha224" => digest_state::<Sha224>(64),
        "sha256" => digest_state::<Sha256>(64),
        "sha384" => digest_state::<Sha384>(128),
        "sha512" => digest_state::<Sha512>(128),
        "sha512/224" => digest_state::<Sha512_224>(128),
        "sha512/256" => digest_state::<Sha512_256>(128),
        "sha3-224" => digest_state::<Sha3_224>(144),
        "sha3-256" => digest_state::<Sha3_256>(136),
        "sha3-384" => digest_state::<Sha3_384>(104),
        "sha3-512" => digest_state::<Sha3_512>(72),
        "ripemd128" => digest_state::<Ripemd128>(64),
        "ripemd160" => digest_state::<Ripemd160>(64),
        "ripemd256" => digest_state::<Ripemd256>(64),
        "ripemd320" => digest_state::<Ripemd320>(64),
        "whirlpool" => digest_state::<whirlpool::Whirlpool>(64),
        "blake2b512" => digest_state::<Blake2b512>(128),
        "blake2s256" => digest_state::<Blake2s256>(64),
        // Non-crypto checksums are added in Task 3.
        _ => return None,
    })
}
```

Note: `md5` is the `md-5` crate but its crate root module is `md5`. Confirm the import path compiles; if the crate name differs, use `use md5;` already brings it in via the `md-5` package's lib name `md5`.

- [ ] **Step 4: Implement the one-shot C ABI in `lib.rs`**

Replace the body of `crates/elephc-crypto/src/lib.rs` (keep the preamble) with:

```rust
//! ...preamble unchanged...

mod algos;

use algos::{make, HashState};
use std::os::raw::c_void;

/// Builds a byte slice from a possibly-null/zero-length C pointer pair.
unsafe fn slice<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
    if len == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(ptr, len)
    }
}

/// Reads a UTF-8 algorithm name from a C pointer pair (lossy on invalid UTF-8,
/// which simply fails to match any known algorithm).
unsafe fn name_str<'a>(ptr: *const u8, len: usize) -> std::borrow::Cow<'a, str> {
    String::from_utf8_lossy(slice(ptr, len))
}

/// Computes a one-shot raw digest of `data` under `name`, writing the bytes to
/// `out` (caller guarantees a 64-byte buffer). Returns the digest length, or -1
/// for an unknown algorithm.
///
/// # Safety
/// All pointers must be valid for their stated lengths; `out` must hold 64 bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_crypto_hash(
    name_ptr: *const u8,
    name_len: usize,
    data_ptr: *const u8,
    data_len: usize,
    out_ptr: *mut u8,
) -> isize {
    let name = name_str(name_ptr, name_len);
    let mut st = match make(&name) {
        Some(s) => s,
        None => return -1,
    };
    st.update(slice(data_ptr, data_len));
    let digest = st.finalize_box();
    std::ptr::copy_nonoverlapping(digest.as_ptr(), out_ptr, digest.len());
    digest.len() as isize
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p elephc-crypto --test vectors`
Expected: PASS (both `crypto_one_shot_known_vectors` and `unknown_algorithm_returns_negative`).

- [ ] **Step 6: Commit**

```bash
git add crates/elephc-crypto/src/algos.rs crates/elephc-crypto/src/lib.rs crates/elephc-crypto/tests/vectors.rs
git commit -m "feat(crypto): HashState trait, crypto algorithm table, one-shot hash ABI"
```

---

### Task 3: Non-crypto checksums (crc32, crc32b, adler32, fnv, joaat)

**Files:**
- Modify: `crates/elephc-crypto/src/algos.rs`
- Modify: `crates/elephc-crypto/tests/vectors.rs`

- [ ] **Step 1: Capture PHP golden values for the checksums**

Run (PHP is the oracle, matching the project's `ELEPHC_PHP_CHECK` philosophy):

```bash
php -r 'foreach (["crc32","crc32b","adler32","fnv132","fnv1a32","fnv164","fnv1a64","joaat"] as $a) printf("%s %s\n", $a, hash($a, "abc"));'
```

Record each printed hex value; they become the `EXPECTED` constants in Step 2. (Known anchor: `adler32` of `"abc"` is `024d0127`.) If `php` is unavailable, install it or run this on any machine with PHP 8 — these values are interpreter-defined and must match exactly.

- [ ] **Step 2: Write the failing test**

Append to `crates/elephc-crypto/tests/vectors.rs`, substituting the hex strings printed in Step 1:

```rust
#[test]
fn non_crypto_checksum_vectors_match_php() {
    // Values produced by `php -r 'echo hash($algo, "abc");'` (PHP 8).
    assert_eq!(hash_hex("crc32", b"abc").unwrap(), "<paste crc32 abc>");
    assert_eq!(hash_hex("crc32b", b"abc").unwrap(), "<paste crc32b abc>");
    assert_eq!(hash_hex("adler32", b"abc").unwrap(), "024d0127");
    assert_eq!(hash_hex("fnv132", b"abc").unwrap(), "<paste fnv132 abc>");
    assert_eq!(hash_hex("fnv1a32", b"abc").unwrap(), "<paste fnv1a32 abc>");
    assert_eq!(hash_hex("fnv164", b"abc").unwrap(), "<paste fnv164 abc>");
    assert_eq!(hash_hex("fnv1a64", b"abc").unwrap(), "<paste fnv1a64 abc>");
    assert_eq!(hash_hex("joaat", b"abc").unwrap(), "<paste joaat abc>");
}
```

- [ ] **Step 2b: Run the test to verify it fails**

Run: `cargo test -p elephc-crypto --test vectors non_crypto`
Expected: FAIL — these algorithms return -1 (unknown) today.

- [ ] **Step 3: Implement the checksum `HashState`s and table entries**

In `crates/elephc-crypto/src/algos.rs`, add these state types (PHP emits all checksums as big-endian hex of the final integer):

```rust
/// crc32b (CRC-32/ISO-HDLC, reflected) — PHP `hash("crc32b", ...)` and the value
/// PHP's `crc32()` function returns. Big-endian hex of the final u32.
struct Crc32bState(crc32fast::Hasher);
impl HashState for Crc32bState {
    fn update(&mut self, data: &[u8]) { self.0.update(data); }
    fn finalize_box(self: Box<Self>) -> Vec<u8> { self.0.finalize().to_be_bytes().to_vec() }
    fn output_size(&self) -> usize { 4 }
    fn block_size(&self) -> usize { 4 }
    fn box_clone(&self) -> Box<dyn HashState> { Box::new(Crc32bState(self.0.clone())) }
}

/// PHP's non-`b` "crc32" variant (CRC-32/BZIP2: same polynomial, non-reflected).
/// Verify the exact `crc::Algorithm` against the PHP golden value in tests.
struct Crc32State { digest_bytes: Vec<u8>, hasher: crc::Digest<'static, u32> }
// NOTE: `crc::Digest` borrows its `Crc`; store a 'static Crc to satisfy the lifetime.
static CRC32_BZIP2: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_BZIP2);
struct Crc32 { hasher: crc::Digest<'static, u32> }
impl HashState for Crc32 {
    fn update(&mut self, data: &[u8]) { self.hasher.update(data); }
    fn finalize_box(self: Box<Self>) -> Vec<u8> { self.hasher.finalize().to_be_bytes().to_vec() }
    fn output_size(&self) -> usize { 4 }
    fn block_size(&self) -> usize { 4 }
    // crc::Digest is not Clone; rebuild from the running value is unavailable, so
    // checksums opt out of incremental clone by panicking — they are never used
    // with hash_copy in PHP-typical code, and Task 5's clone test covers only
    // crypto algorithms. If clone support is later required, switch to a crate
    // that exposes the running state.
    fn box_clone(&self) -> Box<dyn HashState> { unreachable!("crc32 is not clonable") }
}
```

Because `crc::Digest` is awkward to store and clone, prefer a buffering implementation for both crc variants and adler/fnv/joaat (these inputs are small in practice and PHP semantics are one-shot for checksums): accumulate bytes, compute on finalize.

Replace the above crc sketch with this simpler, uniformly-clonable buffering design and add all checksum entries:

```rust
/// Buffering checksum state: accumulates input and computes the digest on
/// finalize. Used for the non-crypto algorithms whose crates do not expose a
/// resumable running state; cloning is trivial (clone the buffer).
struct BufChecksum {
    buf: Vec<u8>,
    finish: fn(&[u8]) -> Vec<u8>,
}
impl HashState for BufChecksum {
    fn update(&mut self, data: &[u8]) { self.buf.extend_from_slice(data); }
    fn finalize_box(self: Box<Self>) -> Vec<u8> { (self.finish)(&self.buf) }
    fn output_size(&self) -> usize { 4 } // 8 for the 64-bit fnv variants; not load-bearing
    fn block_size(&self) -> usize { 0 }
    fn box_clone(&self) -> Box<dyn HashState> {
        Box::new(BufChecksum { buf: self.buf.clone(), finish: self.finish })
    }
}

fn buf(finish: fn(&[u8]) -> Vec<u8>) -> Box<dyn HashState> {
    Box::new(BufChecksum { buf: Vec::new(), finish })
}

/// CRC-32/BZIP2 = PHP "crc32" (non-`b`); big-endian hex.
fn crc32_bzip2(data: &[u8]) -> Vec<u8> {
    static C: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_BZIP2);
    C.checksum(data).to_be_bytes().to_vec()
}
/// CRC-32/ISO-HDLC = PHP "crc32b"; big-endian hex.
fn crc32b(data: &[u8]) -> Vec<u8> {
    let mut h = crc32fast::Hasher::new();
    h.update(data);
    h.finalize().to_be_bytes().to_vec()
}
/// Adler-32; big-endian hex.
fn adler32(data: &[u8]) -> Vec<u8> {
    let mut a = adler::Adler32::new();
    a.write_slice(data);
    a.checksum().to_be_bytes().to_vec()
}
/// FNV variants; PHP emits big-endian hex of the final integer.
fn fnv1_32(data: &[u8]) -> Vec<u8> { fnv1_32_inner(data, false).to_be_bytes().to_vec() }
fn fnv1a_32(data: &[u8]) -> Vec<u8> { fnv1_32_inner(data, true).to_be_bytes().to_vec() }
fn fnv1_64(data: &[u8]) -> Vec<u8> { fnv1_64_inner(data, false).to_be_bytes().to_vec() }
fn fnv1a_64(data: &[u8]) -> Vec<u8> { fnv1_64_inner(data, true).to_be_bytes().to_vec() }

fn fnv1_32_inner(data: &[u8], a: bool) -> u32 {
    let mut h: u32 = 0x811c9dc5;
    for &b in data {
        if a { h ^= b as u32; h = h.wrapping_mul(0x0100_0193); }
        else { h = h.wrapping_mul(0x0100_0193); h ^= b as u32; }
    }
    h
}
fn fnv1_64_inner(data: &[u8], a: bool) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in data {
        if a { h ^= b as u64; h = h.wrapping_mul(0x0000_0100_0000_01b3); }
        else { h = h.wrapping_mul(0x0000_0100_0000_01b3); h ^= b as u64; }
    }
    h
}
/// Jenkins one-at-a-time (PHP "joaat"); big-endian hex.
fn joaat(data: &[u8]) -> Vec<u8> {
    let mut h: u32 = 0;
    for &b in data {
        h = h.wrapping_add(b as u32);
        h = h.wrapping_add(h << 10);
        h ^= h >> 6;
    }
    h = h.wrapping_add(h << 3);
    h ^= h >> 11;
    h = h.wrapping_add(h << 15);
    h.to_be_bytes().to_vec()
}
```

Then add to the `match name` in `make()` (before the `_ => return None` arm):

```rust
        "crc32" => buf(crc32_bzip2),
        "crc32b" => buf(crc32b),
        "adler32" => buf(adler32),
        "fnv132" => buf(fnv1_32),
        "fnv1a32" => buf(fnv1a_32),
        "fnv164" => buf(fnv1_64),
        "fnv1a64" => buf(fnv1a_64),
        "joaat" => buf(joaat),
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p elephc-crypto --test vectors non_crypto`
Expected: PASS. If `crc32` (non-`b`) mismatches, the `crc::Algorithm` is wrong — try `CRC_32_ISO_HDLC` vs `CRC_32_BZIP2` against the PHP golden value until it matches, and leave a comment recording which one PHP uses.

- [ ] **Step 5: Commit**

```bash
git add crates/elephc-crypto/src/algos.rs crates/elephc-crypto/tests/vectors.rs
git commit -m "feat(crypto): non-crypto checksum algorithms (crc32/crc32b/adler32/fnv/joaat)"
```

---

### Task 4: Generic HMAC over `HashState` — `elephc_crypto_hmac`

**Files:**
- Modify: `crates/elephc-crypto/src/lib.rs`
- Create: `crates/elephc-crypto/src/hmac.rs`
- Modify: `crates/elephc-crypto/tests/vectors.rs`

- [ ] **Step 1: Write the failing test (RFC 4231 + PHP parity)**

Append to `crates/elephc-crypto/tests/vectors.rs`:

```rust
fn hmac_hex(algo: &str, key: &[u8], data: &[u8]) -> Option<String> {
    let mut out = [0u8; 64];
    let n = unsafe {
        elephc_crypto_hmac(
            algo.as_ptr(), algo.len(),
            key.as_ptr(), key.len(),
            data.as_ptr(), data.len(),
            out.as_mut_ptr(),
        )
    };
    if n < 0 { return None; }
    Some(out[..n as usize].iter().map(|b| format!("{:02x}", b)).collect())
}

#[test]
fn hmac_rfc4231_case2() {
    // RFC 4231 test case 2: key="Jefe", data="what do ya want for nothing?"
    assert_eq!(
        hmac_hex("sha256", b"Jefe", b"what do ya want for nothing?").unwrap(),
        "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
    );
}

#[test]
fn hmac_unknown_algorithm_returns_negative() {
    assert!(hmac_hex("tiger", b"k", b"d").is_none());
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p elephc-crypto --test vectors hmac`
Expected: FAIL to compile — `elephc_crypto_hmac` does not exist.

- [ ] **Step 3: Implement generic HMAC**

Create `crates/elephc-crypto/src/hmac.rs`:

```rust
//! Purpose:
//! Generic HMAC over any `HashState`, avoiding a per-digest-type match.
//!
//! Called from:
//! - `crate::elephc_crypto_hmac` and the HMAC-mode incremental context.
//!
//! Key details:
//! - Implements RFC 2104 using the algorithm's block size from `HashState`.
//!   A key longer than the block size is hashed down first, then zero-padded.

use crate::algos::{make, HashState};

/// Derives the block-sized key (K'): hash-down if longer than block, then
/// zero-pad to the block size.
fn block_key(algo: &str, key: &[u8]) -> Option<Vec<u8>> {
    let probe = make(algo)?;
    let block = probe.block_size();
    let mut k = if key.len() > block {
        let mut h = make(algo)?;
        h.update(key);
        h.finalize_box()
    } else {
        key.to_vec()
    };
    k.resize(block, 0);
    Some(k)
}

/// Computes HMAC(key, data) under `algo`, returning the raw digest, or `None`
/// for an unknown algorithm.
pub fn hmac(algo: &str, key: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    let k = block_key(algo, key)?;
    let ipad: Vec<u8> = k.iter().map(|b| b ^ 0x36).collect();
    let opad: Vec<u8> = k.iter().map(|b| b ^ 0x5c).collect();

    let mut inner = make(algo)?;
    inner.update(&ipad);
    inner.update(data);
    let inner_digest = inner.finalize_box();

    let mut outer = make(algo)?;
    outer.update(&opad);
    outer.update(&inner_digest);
    Some(outer.finalize_box())
}
```

Add `mod hmac;` to `lib.rs` (after `mod algos;`) and the ABI function:

```rust
/// Computes a one-shot raw HMAC of `data` keyed by `key` under `name`, writing
/// the digest to `out` (64-byte buffer). Returns length, or -1 for unknown algo.
///
/// # Safety
/// All pointers must be valid for their lengths; `out` must hold 64 bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_crypto_hmac(
    name_ptr: *const u8,
    name_len: usize,
    key_ptr: *const u8,
    key_len: usize,
    data_ptr: *const u8,
    data_len: usize,
    out_ptr: *mut u8,
) -> isize {
    let name = name_str(name_ptr, name_len);
    let digest = match hmac::hmac(&name, slice(key_ptr, key_len), slice(data_ptr, data_len)) {
        Some(d) => d,
        None => return -1,
    };
    std::ptr::copy_nonoverlapping(digest.as_ptr(), out_ptr, digest.len());
    digest.len() as isize
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p elephc-crypto --test vectors hmac`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/elephc-crypto/src/hmac.rs crates/elephc-crypto/src/lib.rs crates/elephc-crypto/tests/vectors.rs
git commit -m "feat(crypto): generic HMAC over HashState and elephc_crypto_hmac ABI"
```

---

### Task 5: Incremental HashContext ABI (init / init_hmac / update / final / clone / free)

**Files:**
- Modify: `crates/elephc-crypto/src/lib.rs`
- Modify: `crates/elephc-crypto/tests/vectors.rs`

- [ ] **Step 1: Write the failing test (incremental == one-shot, clone, hmac streaming)**

Append to `crates/elephc-crypto/tests/vectors.rs`:

```rust
use std::os::raw::c_void;

fn finalize_hex(ctx: *mut c_void) -> String {
    let mut out = [0u8; 64];
    let n = unsafe { elephc_crypto_final(ctx, out.as_mut_ptr()) };
    assert!(n >= 0);
    out[..n as usize].iter().map(|b| format!("{:02x}", b)).collect()
}

#[test]
fn incremental_matches_one_shot() {
    let ctx = unsafe { elephc_crypto_init(b"sha256".as_ptr(), 6) };
    assert!(!ctx.is_null());
    unsafe {
        elephc_crypto_update(ctx, b"ab".as_ptr(), 2);
        elephc_crypto_update(ctx, b"c".as_ptr(), 1);
    }
    assert_eq!(finalize_hex(ctx), hash_hex("sha256", b"abc").unwrap());
}

#[test]
fn clone_produces_independent_state() {
    let ctx = unsafe { elephc_crypto_init(b"sha256".as_ptr(), 6) };
    unsafe { elephc_crypto_update(ctx, b"a".as_ptr(), 1); }
    let ctx2 = unsafe { elephc_crypto_clone(ctx) };
    assert!(!ctx2.is_null());
    unsafe {
        elephc_crypto_update(ctx, b"bc".as_ptr(), 2);
        elephc_crypto_update(ctx2, b"bc".as_ptr(), 2);
    }
    let h1 = finalize_hex(ctx);
    let h2 = finalize_hex(ctx2);
    assert_eq!(h1, hash_hex("sha256", b"abc").unwrap());
    assert_eq!(h2, hash_hex("sha256", b"abc").unwrap());
}

#[test]
fn incremental_hmac_matches_one_shot() {
    let key = b"Jefe";
    let ctx = unsafe { elephc_crypto_init_hmac(b"sha256".as_ptr(), 6, key.as_ptr(), key.len()) };
    assert!(!ctx.is_null());
    unsafe {
        elephc_crypto_update(ctx, b"what do ya ".as_ptr(), 11);
        elephc_crypto_update(ctx, b"want for nothing?".as_ptr(), 17);
    }
    assert_eq!(
        finalize_hex(ctx),
        hmac_hex("sha256", key, b"what do ya want for nothing?").unwrap()
    );
}

#[test]
fn init_unknown_algorithm_returns_null() {
    let ctx = unsafe { elephc_crypto_init(b"tiger".as_ptr(), 5) };
    assert!(ctx.is_null());
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p elephc-crypto --test vectors`
Expected: FAIL to compile — the `elephc_crypto_init*`/`update`/`final`/`clone`/`free` functions don't exist.

- [ ] **Step 3: Implement the context type and the incremental ABI in `lib.rs`**

Add to `crates/elephc-crypto/src/lib.rs`:

```rust
/// Heap context behind a `HashContext` handle: either a plain running digest or
/// an HMAC-streaming state (inner already primed with K'⊕ipad).
enum HashCtx {
    Plain(Box<dyn HashState>),
    Hmac { algo: String, opad_key: Vec<u8>, inner: Box<dyn HashState> },
}

impl HashCtx {
    /// Feeds more input into the running context.
    fn update(&mut self, data: &[u8]) {
        match self {
            HashCtx::Plain(s) => s.update(data),
            HashCtx::Hmac { inner, .. } => inner.update(data),
        }
    }
    /// Finalizes the context into its raw digest, consuming it.
    fn finalize(self) -> Vec<u8> {
        match self {
            HashCtx::Plain(s) => s.finalize_box(),
            HashCtx::Hmac { algo, opad_key, inner } => {
                let inner_digest = inner.finalize_box();
                // outer hash recomputed fresh; algo is known-valid (init succeeded).
                let mut outer = make(&algo).expect("hmac algo was validated at init");
                outer.update(&opad_key);
                outer.update(&inner_digest);
                outer.finalize_box()
            }
        }
    }
    /// Deep-clones the running context (backs hash_copy / clone $ctx).
    fn clone_box(&self) -> HashCtx {
        match self {
            HashCtx::Plain(s) => HashCtx::Plain(s.box_clone()),
            HashCtx::Hmac { algo, opad_key, inner } => HashCtx::Hmac {
                algo: algo.clone(),
                opad_key: opad_key.clone(),
                inner: inner.box_clone(),
            },
        }
    }
}

/// Creates a plain hashing context for `name`. Returns a `HashContext` handle,
/// or null for an unknown algorithm.
///
/// # Safety
/// `name_ptr` must be valid for `name_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_crypto_init(name_ptr: *const u8, name_len: usize) -> *mut c_void {
    let name = name_str(name_ptr, name_len);
    match make(&name) {
        Some(s) => Box::into_raw(Box::new(HashCtx::Plain(s))) as *mut c_void,
        None => std::ptr::null_mut(),
    }
}

/// Creates an HMAC-streaming context (PHP `hash_init($algo, HASH_HMAC, $key)`).
/// Returns a handle, or null for an unknown algorithm.
///
/// # Safety
/// `name_ptr`/`key_ptr` must be valid for their lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_crypto_init_hmac(
    name_ptr: *const u8,
    name_len: usize,
    key_ptr: *const u8,
    key_len: usize,
) -> *mut c_void {
    let name = name_str(name_ptr, name_len).into_owned();
    let k = match hmac::block_key(&name, slice(key_ptr, key_len)) {
        Some(k) => k,
        None => return std::ptr::null_mut(),
    };
    let ipad: Vec<u8> = k.iter().map(|b| b ^ 0x36).collect();
    let opad_key: Vec<u8> = k.iter().map(|b| b ^ 0x5c).collect();
    let mut inner = make(&name).expect("algo validated by block_key");
    inner.update(&ipad);
    Box::into_raw(Box::new(HashCtx::Hmac { algo: name, opad_key, inner })) as *mut c_void
}

/// Feeds `data` into the context.
///
/// # Safety
/// `ctx` must be a live handle from init/init_hmac/clone; `data_ptr` valid for `len`.
#[no_mangle]
pub unsafe extern "C" fn elephc_crypto_update(ctx: *mut c_void, data_ptr: *const u8, data_len: usize) {
    if ctx.is_null() { return; }
    let ctx = &mut *(ctx as *mut HashCtx);
    ctx.update(slice(data_ptr, data_len));
}

/// Finalizes the context into `out`, consuming and freeing it. Returns digest
/// length, or -1 for a null handle.
///
/// # Safety
/// `ctx` must be a live handle (becomes invalid after this call); `out` holds 64 bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_crypto_final(ctx: *mut c_void, out_ptr: *mut u8) -> isize {
    if ctx.is_null() { return -1; }
    let ctx = Box::from_raw(ctx as *mut HashCtx);
    let digest = ctx.finalize();
    std::ptr::copy_nonoverlapping(digest.as_ptr(), out_ptr, digest.len());
    digest.len() as isize
}

/// Deep-clones a context, returning a new independent handle (null if input null).
///
/// # Safety
/// `ctx` must be a live handle.
#[no_mangle]
pub unsafe extern "C" fn elephc_crypto_clone(ctx: *mut c_void) -> *mut c_void {
    if ctx.is_null() { return std::ptr::null_mut(); }
    let ctx = &*(ctx as *mut HashCtx);
    Box::into_raw(Box::new(ctx.clone_box())) as *mut c_void
}

/// Frees a context without finalizing (scope-exit / error cleanup).
///
/// # Safety
/// `ctx` must be a live handle and must not be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn elephc_crypto_free(ctx: *mut c_void) {
    if ctx.is_null() { return; }
    drop(Box::from_raw(ctx as *mut HashCtx));
}
```

Change `block_key` visibility in `hmac.rs` to `pub(crate)` so `init_hmac` can call it.

- [ ] **Step 4: Run the full crate test suite**

Run: `cargo test -p elephc-crypto`
Expected: PASS (all vector + incremental tests).

- [ ] **Step 5: Verify no leaks under the address sanitizer-style check (miri optional)**

Run: `cargo test -p elephc-crypto` once more and confirm `free` is exercised by adding a quick test:

```rust
#[test]
fn free_releases_unfinalized_context() {
    let ctx = unsafe { elephc_crypto_init(b"sha256".as_ptr(), 6) };
    unsafe { elephc_crypto_update(ctx, b"abc".as_ptr(), 3); }
    unsafe { elephc_crypto_free(ctx) }; // must not double-free or leak; run under `cargo test` (and `cargo +nightly miri test` if available)
}
```

Run: `cargo test -p elephc-crypto --test vectors free_releases`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/elephc-crypto/src/lib.rs crates/elephc-crypto/src/hmac.rs crates/elephc-crypto/tests/vectors.rs
git commit -m "feat(crypto): incremental HashContext ABI (init/update/final/clone/free + hmac)"
```

---

### Task 6: Register the linker bridge entry

**Files:**
- Modify: `src/linker.rs` (the `const BRIDGES` array, around line 47-72)

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` block in `src/linker.rs` (create the block if absent, following the file's preamble/doc conventions):

```rust
/// Verifies the elephc-crypto bridge is registered and produces the expected
/// archive filename, so compiled programs that use hashing can link it.
#[test]
fn bridges_includes_elephc_crypto() {
    let entry = BRIDGES
        .iter()
        .find(|b| b.lib_name == "elephc_crypto")
        .expect("elephc_crypto must be a registered bridge");
    assert_eq!(entry.crate_name, "elephc-crypto");
    assert_eq!(entry.env_var, "ELEPHC_CRYPTO_LIB_DIR");
    assert_eq!(entry.archive_filename(), "libelephc_crypto.a");
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --lib linker::tests::bridges_includes_elephc_crypto`
Expected: FAIL — `find` returns `None`, panics with "must be a registered bridge".

- [ ] **Step 3: Add the bridge entry**

In `src/linker.rs`, append to `const BRIDGES` (after the `elephc_pdo` entry):

```rust
    BridgeStaticlib {
        lib_name: "elephc_crypto",
        env_var: "ELEPHC_CRYPTO_LIB_DIR",
        crate_name: "elephc-crypto",
        // Pure-Rust hashing: no link-time side effects, so a plain `-l`/force_load
        // by the existing path is enough.
        whole_archive: false,
        // No native transitive deps.
        macos_frameworks: &[],
        // Rust runtime/unwinder symbols, like the other bridges.
        needs_libdl: true,
    },
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test --lib linker::tests::bridges_includes_elephc_crypto`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/linker.rs
git commit -m "feat(crypto): register elephc-crypto in the linker bridge table"
```

---

### Task 7: Phase-1 gate — full build, warnings, and crate suite

- [ ] **Step 1: Build the whole workspace clean (zero warnings)**

Run: `cargo build`
Expected: builds with no warnings (project policy).

- [ ] **Step 2: Run the crate's full test suite**

Run: `cargo test -p elephc-crypto`
Expected: all tests PASS.

- [ ] **Step 3: Run the linker unit test and a quick compiler test slice**

Run: `cargo test --lib linker`
Expected: PASS (including the new bridge test). No codegen change yet, so the broader suite is unaffected; the full `cargo test` gate runs at the end of Phase 2 when behavior actually changes.

- [ ] **Step 4: Check formatting/whitespace policy**

Run: `git diff --check`
Expected: no whitespace errors. (Do not run `cargo fmt` — project policy.)

- [ ] **Step 5: Final Phase-1 commit (if anything pending)**

```bash
git status
# commit any stragglers with an appropriate `chore(crypto):` message
```

---

## Self-Review

**Spec coverage (Phase 1 scope = spec Components 1 and 5):**
- Crate `staticlib`+`rlib`, RustCrypto deps → Task 1. ✓
- Algorithm table (crypto + non-crypto + documented gaps via `None`) → Tasks 2, 3. ✓
- HMAC generic over `DynDigest`/`HashState` → Task 4. ✓
- C ABI: `hash`, `hmac`, `init`, `init_hmac`, `update`, `final`, `clone`, `free` → Tasks 2, 4, 5. ✓
- 64-byte output buffer contract → enforced by test helpers and documented in `lib.rs` preamble. ✓
- `Resource("hash")` handle = thin pointer to boxed `HashCtx` → Task 5. ✓
- HASH_HMAC streaming (`init_hmac`) → Task 5. ✓
- crc32 vs crc32b distinction + PHP cross-check → Task 3. ✓
- Linker `BRIDGES` entry + workspace/dev-dependency wiring → Tasks 1, 6. ✓
- Deferred to later phases (not in this plan): codegen slots/publication/runtime helpers, checker/catalog/signatures, `$binary`, `ValueError`, `hash_equals`/`hash_file`/`hash_algos` builtins, scope-cleanup hook, phar migration, fork removal, docs/examples/roadmap. These are spec Components 2-4 and Phases 2-5.

**Placeholder scan:** The only intentional fill-ins are the PHP golden hex values in Task 3 Step 2, which are interpreter-defined and captured by the `php -r` command in Task 3 Step 1 — this is the project's `ELEPHC_PHP_CHECK` oracle pattern, not a vague placeholder. All code steps contain complete code.

**Type consistency:** `HashState` (methods `update`/`finalize_box`/`output_size`/`block_size`/`box_clone`) is used consistently across `algos.rs`, `hmac.rs`, and `lib.rs`. `HashCtx` (Plain/Hmac) and the eight `elephc_crypto_*` ABI names match between the implementation tasks and the test helpers. `block_key` is defined in `hmac.rs` and made `pub(crate)` for `init_hmac` (Task 5 Step 3).

**Open risk flagged for the implementer:** the exact `crc::Algorithm` for PHP's non-`b` `crc32` (CRC_32_BZIP2 vs alternatives) and the `md-5` crate's import path are the two spots most likely to need a one-line adjustment against the compiler/PHP; both have explicit verification steps.
