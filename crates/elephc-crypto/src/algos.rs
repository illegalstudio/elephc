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

/// Buffering checksum state: accumulates input and computes the digest on
/// finalize. Buffering is used for uniformity and simplicity: PHP's checksum
/// usage is inherently one-shot, so streaming APIs offer no benefit here.
/// Note: `crc32fast::Hasher` and `adler2::Adler32` do have streaming APIs;
/// FNV and joaat are simply computed in one pass. The buffering approach
/// keeps all checksum paths consistent without sacrificing correctness.
struct BufChecksum {
    buf: Vec<u8>,
    out_len: usize,
    finish: fn(&[u8]) -> Vec<u8>,
}

impl HashState for BufChecksum {
    /// Accumulates input bytes for the final checksum computation.
    fn update(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }
    /// Computes the checksum over all buffered input.
    fn finalize_box(self: Box<Self>) -> Vec<u8> {
        (self.finish)(&self.buf)
    }
    /// Returns the checksum width in bytes (4, or 8 for the 64-bit FNV variants).
    fn output_size(&self) -> usize {
        self.out_len
    }
    /// Returns 0 as a sentinel meaning "not a valid HMAC algorithm".
    /// PHP rejects `hash_hmac()` calls over checksum algorithms with a
    /// ValueError, so any future HMAC path MUST treat `block_size() == 0`
    /// as a signal to reject the algorithm before proceeding.
    fn block_size(&self) -> usize {
        0
    }
    /// Clones the accumulated buffer into an independent checksum state.
    fn box_clone(&self) -> Box<dyn HashState> {
        Box::new(BufChecksum { buf: self.buf.clone(), out_len: self.out_len, finish: self.finish })
    }
}

/// Builds a boxed buffering checksum state of the given output width.
fn buf(out_len: usize, finish: fn(&[u8]) -> Vec<u8>) -> Box<dyn HashState> {
    Box::new(BufChecksum { buf: Vec::new(), out_len, finish })
}

/// CRC-32/BZIP2 = PHP's non-`b` "crc32" algorithm.
/// PHP serializes this checksum in little-endian byte order (LSB first in the hex string),
/// unlike every other hash() algorithm. `CRC_32_BZIP2` is the correct polynomial; only
/// the byte-serialization differs from the usual big-endian convention.
fn crc32_bzip2(data: &[u8]) -> Vec<u8> {
    static C: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_BZIP2);
    C.checksum(data).to_le_bytes().to_vec()
}

/// CRC-32/ISO-HDLC = PHP "crc32b" (and the value PHP's crc32() returns); big-endian hex.
fn crc32b(data: &[u8]) -> Vec<u8> {
    let mut h = crc32fast::Hasher::new();
    h.update(data);
    h.finalize().to_be_bytes().to_vec()
}

/// Adler-32 checksum; big-endian hex of the u32.
fn adler32(data: &[u8]) -> Vec<u8> {
    let mut a = adler2::Adler32::new();
    a.write_slice(data);
    a.checksum().to_be_bytes().to_vec()
}

/// FNV-1 (32-bit) when `alt` is false, FNV-1a (32-bit) when true.
fn fnv32(data: &[u8], alt: bool) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for &b in data {
        if alt {
            h ^= b as u32;
            h = h.wrapping_mul(0x0100_0193);
        } else {
            h = h.wrapping_mul(0x0100_0193);
            h ^= b as u32;
        }
    }
    h
}

/// FNV-1 (64-bit) when `alt` is false, FNV-1a (64-bit) when true.
fn fnv64(data: &[u8], alt: bool) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in data {
        if alt {
            h ^= b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        } else {
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
            h ^= b as u64;
        }
    }
    h
}

/// Jenkins one-at-a-time hash (PHP "joaat"); big-endian hex of the u32.
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

/// FNV-1 32-bit wrapper for use as a `fn` pointer.
fn fnv132(d: &[u8]) -> Vec<u8> {
    fnv32(d, false).to_be_bytes().to_vec()
}

/// FNV-1a 32-bit wrapper for use as a `fn` pointer.
fn fnv1a32(d: &[u8]) -> Vec<u8> {
    fnv32(d, true).to_be_bytes().to_vec()
}

/// FNV-1 64-bit wrapper for use as a `fn` pointer.
fn fnv164(d: &[u8]) -> Vec<u8> {
    fnv64(d, false).to_be_bytes().to_vec()
}

/// FNV-1a 64-bit wrapper for use as a `fn` pointer.
fn fnv1a64(d: &[u8]) -> Vec<u8> {
    fnv64(d, true).to_be_bytes().to_vec()
}

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
    D: digest::Digest + digest::FixedOutputReset + Clone + 'static,
{
    Box::new(DigestState { inner: Box::new(D::new()), block })
}

/// CRC-32C (CRC-32/ISCSI, Castagnoli) = PHP "crc32c"; big-endian hex.
fn crc32c(data: &[u8]) -> Vec<u8> {
    static C: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_ISCSI);
    C.checksum(data).to_be_bytes().to_vec()
}

/// Buffered state backed by an `allthehashes` implementation (HAVAL, Tiger,
/// Snefru, GOST94). The library exposes one-shot digests, so the bridge keeps
/// the streaming buffer and calls it at finalize time.
struct LibraryState {
    buf: Vec<u8>,
    out_len: usize,
    block: usize,
    algorithm: &'static dyn allthehashes::HashAlgorithm,
}

impl HashState for LibraryState {
    /// Accumulates input bytes for the one-shot library digest.
    fn update(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }
    /// Computes the library digest over all buffered input.
    fn finalize_box(self: Box<Self>) -> Vec<u8> {
        self.algorithm.hash(&self.buf).expect("supported library hash")
    }
    /// Returns the raw digest width.
    fn output_size(&self) -> usize {
        self.out_len
    }
    /// Returns the HMAC block size for the legacy cryptographic algorithms.
    fn block_size(&self) -> usize {
        self.block
    }
    /// Clones the buffered input and the immutable algorithm descriptor.
    fn box_clone(&self) -> Box<dyn HashState> {
        Box::new(LibraryState {
            buf: self.buf.clone(),
            out_len: self.out_len,
            block: self.block,
            algorithm: self.algorithm,
        })
    }
}

/// Builds a buffered state for one `allthehashes` algorithm and HMAC block size.
fn library(name: &str, out_len: usize, block: usize) -> Box<dyn HashState> {
    let algorithm = allthehashes::get_algorithm(name).expect("known library hash algorithm");
    Box::new(LibraryState { buf: Vec::new(), out_len, block, algorithm })
}

/// MurmurHash3 x86 32-bit with PHP's zero seed, big-endian bytes.
fn murmur3a(data: &[u8]) -> Vec<u8> {
    murmur3::murmur3_32(&mut &data[..], 0)
        .expect("murmur3_32 reads a slice")
        .to_be_bytes()
        .to_vec()
}

/// MurmurHash3 x86 128-bit with PHP's zero seed, PHP's h1..h4 word order and
/// big-endian words.
fn murmur3c(data: &[u8]) -> Vec<u8> {
    let value = murmur3::murmur3_x86_128(&mut &data[..], 0)
        .expect("murmur3_x86_128 reads a slice");
    let mut out = Vec::with_capacity(16);
    for word in 0..4 {
        out.extend(((value >> (32 * word)) as u32).to_be_bytes());
    }
    out
}

/// MurmurHash3 x64 128-bit with PHP's zero seed, PHP's low-then-high big-endian
/// 64-bit word order.
fn murmur3f(data: &[u8]) -> Vec<u8> {
    let value = murmur3::murmur3_x64_128(&mut &data[..], 0)
        .expect("murmur3_x64_128 reads a slice");
    let mut out = Vec::with_capacity(16);
    out.extend((value as u64).to_be_bytes());
    out.extend(((value >> 64) as u64).to_be_bytes());
    out
}

/// xxHash32 with PHP's zero seed, big-endian bytes.
fn xxh32(data: &[u8]) -> Vec<u8> {
    xxhash_rust::xxh32::xxh32(data, 0).to_be_bytes().to_vec()
}

/// xxHash64 with PHP's zero seed, big-endian bytes.
fn xxh64(data: &[u8]) -> Vec<u8> {
    xxhash_rust::xxh64::xxh64(data, 0).to_be_bytes().to_vec()
}

/// xxHash3 64-bit, big-endian bytes.
fn xxh3(data: &[u8]) -> Vec<u8> {
    xxhash_rust::xxh3::xxh3_64(data).to_be_bytes().to_vec()
}

/// xxHash3 128-bit, big-endian bytes.
fn xxh128(data: &[u8]) -> Vec<u8> {
    xxhash_rust::xxh3::xxh3_128(data).to_be_bytes().to_vec()
}

/// Resolves a PHP hash() algorithm name to a freshly initialized `HashState`,
/// or `None` if the algorithm is unsupported (caller maps to PHP ValueError).
pub fn make(name: &str) -> Option<Box<dyn HashState>> {
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
        "tiger128,3" => library("tiger128,3", 16, 64),
        "tiger160,3" => library("tiger160,3", 20, 64),
        "tiger192,3" => library("tiger192,3", 24, 64),
        "tiger128,4" => library("tiger128,4", 16, 64),
        "tiger160,4" => library("tiger160,4", 20, 64),
        "tiger192,4" => library("tiger192,4", 24, 64),
        "snefru" => library("snefru", 32, 32),
        "snefru256" => library("snefru256", 32, 32),
        "gost" => library("gost", 32, 32),
        "gost-crypto" => library("gost-crypto", 32, 32),
        "haval128,3" => library("haval128,3", 16, 128),
        "haval160,3" => library("haval160,3", 20, 128),
        "haval192,3" => library("haval192,3", 24, 128),
        "haval224,3" => library("haval224,3", 28, 128),
        "haval256,3" => library("haval256,3", 32, 128),
        "haval128,4" => library("haval128,4", 16, 128),
        "haval160,4" => library("haval160,4", 20, 128),
        "haval192,4" => library("haval192,4", 24, 128),
        "haval224,4" => library("haval224,4", 28, 128),
        "haval256,4" => library("haval256,4", 32, 128),
        "haval128,5" => library("haval128,5", 16, 128),
        "haval160,5" => library("haval160,5", 20, 128),
        "haval192,5" => library("haval192,5", 24, 128),
        "haval224,5" => library("haval224,5", 28, 128),
        "haval256,5" => library("haval256,5", 32, 128),
        "murmur3a" => buf(4, murmur3a),
        "murmur3c" => buf(16, murmur3c),
        "murmur3f" => buf(16, murmur3f),
        "xxh32" => buf(4, xxh32),
        "xxh64" => buf(8, xxh64),
        "xxh3" => buf(8, xxh3),
        "xxh128" => buf(16, xxh128),
        "crc32" => buf(4, crc32_bzip2),
        "crc32b" => buf(4, crc32b),
        "crc32c" => buf(4, crc32c),
        "adler32" => buf(4, adler32),
        "fnv132" => buf(4, fnv132),
        "fnv1a32" => buf(4, fnv1a32),
        "fnv164" => buf(8, fnv164),
        "fnv1a64" => buf(8, fnv1a64),
        "joaat" => buf(4, joaat),
        _ => return None,
    })
}
