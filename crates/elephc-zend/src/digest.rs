//! Purpose:
//! The digests extensions call directly (`PHP_MD5*`, `PHP_SHA1*`,
//! `PHP_SHA256*`, `make_digest`) and ext/hash's algorithm descriptors.
//!
//! Called from:
//! - Extension code (APCu keys, zstd/brotli dictionary ids).
//!
//! Key details:
//! - The context lives in the extension's own struct (`PHP_MD5_CTX` on its
//!   stack), so the state is plain words in that struct: a running state,
//!   the byte count, and a partial block.

use std::ffi::{c_char, c_uchar, c_void};

use crate::abi::*;

/// Runs the MD5 compression function over one 64-byte block (little-endian words), updating the
/// four-word state in place.
fn md5_compress(state: &mut [u32; 4], block: &[u8; 64]) {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9,
        14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15,
        21, 6, 10, 15, 21,
    ];
    // RFC 1321's table, floor(|sin(i + 1)| * 2^32), spelled out instead of
    // computed per block with the platform's libm.
    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501, 0x698098d8, 0x8b44f7af,
        0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa,
        0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8, 0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8,
        0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
        0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665, 0xf4292244, 0x432aff97,
        0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1, 0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1,
        0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
    ];
    let m: [u32; 16] = std::array::from_fn(|i| u32::from_le_bytes(block[i * 4..i * 4 + 4].try_into().unwrap()));
    let [mut a, mut b, mut c, mut d] = *state;
    for i in 0..64 {
        let (f, g) = match i / 16 {
            0 => ((b & c) | (!b & d), i),
            1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
            2 => (b ^ c ^ d, (3 * i + 5) % 16),
            _ => (c ^ (b | !d), (7 * i) % 16),
        };
        let rotated = a.wrapping_add(f).wrapping_add(K[i]).wrapping_add(m[g]).rotate_left(S[i]);
        a = d;
        d = c;
        c = b;
        b = b.wrapping_add(rotated);
    }
    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
}

/// Runs the SHA-1 compression function over one 64-byte block (big-endian words, 80 rounds),
/// updating the five-word state in place.
fn sha1_compress(state: &mut [u32; 5], block: &[u8; 64]) {
    let mut w = [0u32; 80];
    for i in 0..16 {
        w[i] = u32::from_be_bytes(block[i * 4..i * 4 + 4].try_into().unwrap());
    }
    for i in 16..80 {
        w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
    }
    let [mut a, mut b, mut c, mut d, mut e] = *state;
    for (i, wi) in w.iter().enumerate() {
        let (f, k) = match i / 20 {
            0 => ((b & c) | (!b & d), 0x5a827999),
            1 => (b ^ c ^ d, 0x6ed9eba1),
            2 => ((b & c) | (b & d) | (c & d), 0x8f1bbcdc),
            _ => (b ^ c ^ d, 0xca62c1d6u32),
        };
        let temp = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(*wi);
        e = d;
        d = c;
        c = b.rotate_left(30);
        b = a;
        a = temp;
    }
    for (s, v) in state.iter_mut().zip([a, b, c, d, e]) {
        *s = s.wrapping_add(v);
    }
}

const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98,
    0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
    0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8,
    0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819,
    0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
    0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
    0xc67178f2,
];

/// Runs the SHA-256 compression function over one 64-byte block (64 rounds), updating the
/// eight-word state in place.
fn sha256_compress(state: &mut [u32; 8], block: &[u8; 64]) {
    let mut w = [0u32; 64];
    for i in 0..16 {
        w[i] = u32::from_be_bytes(block[i * 4..i * 4 + 4].try_into().unwrap());
    }
    for i in 16..64 {
        let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
        let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
    }
    let mut v = *state;
    for i in 0..64 {
        let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
        let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
        let t1 = v[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(SHA256_K[i]).wrapping_add(w[i]);
        let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
        let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
        let t2 = s0.wrapping_add(maj);
        v = [t1.wrapping_add(t2), v[0], v[1], v[2], v[3].wrapping_add(t1), v[4], v[5], v[6]];
    }
    for (s, x) in state.iter_mut().zip(v) {
        *s = s.wrapping_add(x);
    }
}

/// Feeds `input` through `compress`, keeping a partial block in `buffer`.
fn update(count: &mut u64, buffer: &mut [u8; 64], input: &[u8], mut compress: impl FnMut(&[u8; 64])) {
    let mut have = (*count % 64) as usize;
    *count += input.len() as u64;
    let mut input = input;
    if have > 0 {
        let take = (64 - have).min(input.len());
        buffer[have..have + take].copy_from_slice(&input[..take]);
        have += take;
        input = &input[take..];
        if have < 64 {
            return;
        }
        let block = *buffer;
        compress(&block);
    }
    while input.len() >= 64 {
        compress(input[..64].try_into().unwrap());
        input = &input[64..];
    }
    buffer[..input.len()].copy_from_slice(input);
}

/// The final padding: 0x80, zeros, then the bit length.
fn padding(count: u64, big_endian: bool) -> Vec<u8> {
    let mut pad = vec![0x80u8];
    while (count as usize + pad.len()) % 64 != 56 {
        pad.push(0);
    }
    let bits = count.wrapping_mul(8);
    pad.extend_from_slice(&if big_endian { bits.to_be_bytes() } else { bits.to_le_bytes() });
    pad
}

/// Builds a byte slice from a C pointer and length, returning an empty slice for length 0 so a null
/// pointer is never dereferenced.
unsafe fn slice<'a>(data: *const c_void, len: usize) -> &'a [u8] {
    if len == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(data.cast(), len)
    }
}

/// Initialises an MD5 context to the standard initial state with a zero byte count.
#[no_mangle]
pub unsafe extern "C" fn PHP_MD5Init(context: *mut PHP_MD5_CTX) {
    (*context).state = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476];
    (*context).count = 0;
}

/// Feeds `size` bytes into an MD5 context, compressing each full 64-byte block and buffering the
/// remainder.
#[no_mangle]
pub unsafe extern "C" fn PHP_MD5Update(context: *mut PHP_MD5_CTX, data: *const c_void, size: usize) {
    let ctx = &mut *context;
    let state = &mut ctx.state;
    update(&mut ctx.count, &mut ctx.buffer, slice(data, size), |block| md5_compress(state, block));
}

/// Pads the message and writes the 16-byte MD5 digest (little-endian words) to `result`.
#[no_mangle]
pub unsafe extern "C" fn PHP_MD5Final(result: *mut c_uchar, context: *mut PHP_MD5_CTX) {
    let pad = padding((*context).count, false);
    PHP_MD5Update(context, pad.as_ptr().cast(), pad.len());
    for (i, word) in (*context).state.iter().enumerate() {
        std::ptr::copy_nonoverlapping(word.to_le_bytes().as_ptr(), result.add(i * 4), 4);
    }
}

/// Initialises a SHA-1 context to the standard initial state with a zero byte count.
#[no_mangle]
pub unsafe extern "C" fn PHP_SHA1Init(context: *mut PHP_SHA1_CTX) {
    (*context).state = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476, 0xc3d2e1f0];
    (*context).count = 0;
}

/// Feeds `len` bytes into a SHA-1 context, compressing each full block and buffering the remainder.
#[no_mangle]
pub unsafe extern "C" fn PHP_SHA1Update(context: *mut PHP_SHA1_CTX, input: *const c_uchar, len: usize) {
    let ctx = &mut *context;
    let state = &mut ctx.state;
    update(&mut ctx.count, &mut ctx.buffer, slice(input.cast(), len), |block| sha1_compress(state, block));
}

/// Pads the message and writes the 20-byte SHA-1 digest (big-endian words) to `digest`.
#[no_mangle]
pub unsafe extern "C" fn PHP_SHA1Final(digest: *mut c_uchar, context: *mut PHP_SHA1_CTX) {
    let pad = padding((*context).count, true);
    PHP_SHA1Update(context, pad.as_ptr(), pad.len());
    for (i, word) in (*context).state.iter().enumerate() {
        std::ptr::copy_nonoverlapping(word.to_be_bytes().as_ptr(), digest.add(i * 4), 4);
    }
}

/// Initialises a SHA-256 context to the standard initial state with a zero byte count; the options
/// table is ignored.
#[no_mangle]
pub unsafe extern "C" fn PHP_SHA256InitArgs(context: *mut PHP_SHA256_CTX, _args: *mut HashTable) {
    (*context).state = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];
    (*context).count = 0;
}

/// Initialises a SHA-256 context (`PHP_SHA256InitArgs` without options).
#[no_mangle]
pub unsafe extern "C" fn PHP_SHA256Init(context: *mut PHP_SHA256_CTX) {
    PHP_SHA256InitArgs(context, std::ptr::null_mut());
}

/// Feeds `len` bytes into a SHA-256 context, compressing each full block and buffering the
/// remainder.
#[no_mangle]
pub unsafe extern "C" fn PHP_SHA256Update(context: *mut PHP_SHA256_CTX, input: *const c_uchar, len: usize) {
    let ctx = &mut *context;
    let state = &mut ctx.state;
    update(&mut ctx.count, &mut ctx.buffer, slice(input.cast(), len), |block| sha256_compress(state, block));
}

/// Pads the message and writes the 32-byte SHA-256 digest (big-endian words) to `digest`.
#[no_mangle]
pub unsafe extern "C" fn PHP_SHA256Final(digest: *mut c_uchar, context: *mut PHP_SHA256_CTX) {
    let pad = padding((*context).count, true);
    PHP_SHA256Update(context, pad.as_ptr(), pad.len());
    for (i, word) in (*context).state.iter().enumerate() {
        std::ptr::copy_nonoverlapping(word.to_be_bytes().as_ptr(), digest.add(i * 4), 4);
    }
}

/// Writes `len` digest bytes as lowercase hex into `md5str` and NUL-terminates it (the buffer needs
/// `2 * len + 1` bytes).
#[no_mangle]
pub unsafe extern "C" fn make_digest_ex(md5str: *mut c_char, digest: *const c_uchar, len: usize) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for i in 0..len {
        let byte = *digest.add(i);
        *md5str.add(i * 2) = HEX[(byte >> 4) as usize] as c_char;
        *md5str.add(i * 2 + 1) = HEX[(byte & 15) as usize] as c_char;
    }
    *md5str.add(len * 2) = 0;
}

/// Hex-encodes a 16-byte (MD5) digest into `md5str` with a NUL terminator.
#[no_mangle]
pub unsafe extern "C" fn make_digest(md5str: *mut c_char, digest: *const c_uchar) {
    make_digest_ex(md5str, digest, 16);
}

/// Hex-encodes a 20-byte (SHA-1) digest into `sha1str` with a NUL terminator.
#[no_mangle]
pub unsafe extern "C" fn make_sha1_digest(sha1str: *mut c_char, digest: *const c_uchar) {
    make_digest_ex(sha1str, digest, 20);
}

/// Writes `in_len` input bytes as `2 * in_len` lowercase hex characters to `out` (no NUL
/// terminator).
#[no_mangle]
pub unsafe extern "C" fn php_hash_bin2hex(out: *mut c_char, input: *const c_uchar, in_len: usize) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for i in 0..in_len {
        let byte = *input.add(i);
        *out.add(i * 2) = HEX[(byte >> 4) as usize] as c_char;
        *out.add(i * 2 + 1) = HEX[(byte & 15) as usize] as c_char;
    }
}

// ------------------------------------------------------------ ext/hash descriptors

/// ext/hash `hash_init` callback for MD5; forwards to `PHP_MD5Init` (options ignored).
unsafe extern "C" fn md5_init(context: *mut c_void, _args: *mut HashTable) {
    PHP_MD5Init(context.cast());
}
/// ext/hash `hash_update` callback for MD5; forwards to `PHP_MD5Update`.
unsafe extern "C" fn md5_update(context: *mut c_void, buf: *const c_uchar, count: usize) {
    PHP_MD5Update(context.cast(), buf.cast(), count);
}
/// ext/hash `hash_final` callback for MD5; forwards to `PHP_MD5Final`.
unsafe extern "C" fn md5_final(digest: *mut c_uchar, context: *mut c_void) {
    PHP_MD5Final(digest, context.cast());
}
/// ext/hash `hash_init` callback for SHA-1; forwards to `PHP_SHA1Init` (options ignored).
unsafe extern "C" fn sha1_init(context: *mut c_void, _args: *mut HashTable) {
    PHP_SHA1Init(context.cast());
}
/// ext/hash `hash_update` callback for SHA-1; forwards to `PHP_SHA1Update`.
unsafe extern "C" fn sha1_update(context: *mut c_void, buf: *const c_uchar, count: usize) {
    PHP_SHA1Update(context.cast(), buf, count);
}
/// ext/hash `hash_final` callback for SHA-1; forwards to `PHP_SHA1Final`.
unsafe extern "C" fn sha1_final(digest: *mut c_uchar, context: *mut c_void) {
    PHP_SHA1Final(digest, context.cast());
}
/// ext/hash `hash_init` callback for SHA-256; forwards to `PHP_SHA256InitArgs`.
unsafe extern "C" fn sha256_init(context: *mut c_void, args: *mut HashTable) {
    PHP_SHA256InitArgs(context.cast(), args);
}
/// ext/hash `hash_update` callback for SHA-256; forwards to `PHP_SHA256Update`.
unsafe extern "C" fn sha256_update(context: *mut c_void, buf: *const c_uchar, count: usize) {
    PHP_SHA256Update(context.cast(), buf, count);
}
/// ext/hash `hash_final` callback for SHA-256; forwards to `PHP_SHA256Final`.
unsafe extern "C" fn sha256_final(digest: *mut c_uchar, context: *mut c_void) {
    PHP_SHA256Final(digest, context.cast());
}

#[no_mangle]
pub static php_hash_md5_ops: php_hash_ops = php_hash_ops {
    algo: c"md5".as_ptr(),
    hash_init: Some(md5_init),
    hash_update: Some(md5_update),
    hash_final: Some(md5_final),
    hash_copy: Some(php_hash_copy),
    hash_serialize: None,
    hash_unserialize: None,
    serialize_spec: std::ptr::null(),
    digest_size: 16,
    block_size: 64,
    context_size: std::mem::size_of::<PHP_MD5_CTX>(),
    is_crypto: 1,
};

#[no_mangle]
pub static php_hash_sha1_ops: php_hash_ops = php_hash_ops {
    algo: c"sha1".as_ptr(),
    hash_init: Some(sha1_init),
    hash_update: Some(sha1_update),
    hash_final: Some(sha1_final),
    hash_copy: Some(php_hash_copy),
    hash_serialize: None,
    hash_unserialize: None,
    serialize_spec: std::ptr::null(),
    digest_size: 20,
    block_size: 64,
    context_size: std::mem::size_of::<PHP_SHA1_CTX>(),
    is_crypto: 1,
};

#[no_mangle]
pub static php_hash_sha256_ops: php_hash_ops = php_hash_ops {
    algo: c"sha256".as_ptr(),
    hash_init: Some(sha256_init),
    hash_update: Some(sha256_update),
    hash_final: Some(sha256_final),
    hash_copy: Some(php_hash_copy),
    hash_serialize: None,
    hash_unserialize: None,
    serialize_spec: std::ptr::null(),
    digest_size: 32,
    block_size: 64,
    context_size: std::mem::size_of::<PHP_SHA256_CTX>(),
    is_crypto: 1,
};

unsafe impl Sync for php_hash_ops {}

/// Returns the algorithm descriptor for `md5`, `sha1` or `sha256` (name matched ASCII
/// case-insensitively), or null for any other algorithm.
#[no_mangle]
pub unsafe extern "C" fn php_hash_fetch_ops(algo: *mut zend_string) -> *const php_hash_ops {
    match crate::string::bytes(algo).to_ascii_lowercase().as_slice() {
        b"md5" => &php_hash_md5_ops,
        b"sha1" => &php_hash_sha1_ops,
        b"sha256" => &php_hash_sha256_ops,
        _ => std::ptr::null(),
    }
}

/// No-op: registering additional ext/hash algorithms is accepted and ignored.
#[no_mangle]
pub unsafe extern "C" fn php_hash_register_algo(_algo: *const c_char, _ops: *const php_hash_ops) {}

/// ext/hash `hash_copy` callback: copies a whole context (`context_size` bytes from the descriptor)
/// from `orig_context` to `dest_context`; always `SUCCESS`.
#[no_mangle]
pub unsafe extern "C" fn php_hash_copy(ops: *const c_void, orig_context: *const c_void, dest_context: *mut c_void) -> zend_result {
    let ops = ops.cast::<php_hash_ops>();
    std::ptr::copy_nonoverlapping(orig_context.cast::<u8>(), dest_context.cast::<u8>(), (*ops).context_size);
    SUCCESS
}

/// Allocates a zeroed context of the algorithm's `context_size` with the engine allocator.
#[no_mangle]
pub unsafe extern "C" fn php_hash_alloc_context(ops: *const php_hash_ops) -> *mut c_void {
    crate::alloc::ecalloc(1, (*ops).context_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test helper: renders bytes as a lowercase hex string.
    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Test: MD5 and SHA-1 of "abc" and SHA-256 of 1000 'a' bytes fed in two updates produce the
    /// known expected digests.
    #[test]
    fn digests_match_known_vectors() {
        unsafe {
            let mut ctx: PHP_MD5_CTX = std::mem::zeroed();
            let mut out = [0u8; 16];
            PHP_MD5Init(&mut ctx);
            PHP_MD5Update(&mut ctx, b"abc".as_ptr().cast(), 3);
            PHP_MD5Final(out.as_mut_ptr(), &mut ctx);
            assert_eq!(hex(&out), "900150983cd24fb0d6963f7d28e17f72");

            let mut ctx: PHP_SHA1_CTX = std::mem::zeroed();
            let mut out = [0u8; 20];
            PHP_SHA1Init(&mut ctx);
            PHP_SHA1Update(&mut ctx, b"abc".as_ptr(), 3);
            PHP_SHA1Final(out.as_mut_ptr(), &mut ctx);
            assert_eq!(hex(&out), "a9993e364706816aba3e25717850c26c9cd0d89d");

            let mut ctx: PHP_SHA256_CTX = std::mem::zeroed();
            let mut out = [0u8; 32];
            PHP_SHA256Init(&mut ctx);
            let long = vec![b'a'; 1000];
            PHP_SHA256Update(&mut ctx, long.as_ptr(), 200);
            PHP_SHA256Update(&mut ctx, long[200..].as_ptr(), 800);
            PHP_SHA256Final(out.as_mut_ptr(), &mut ctx);
            assert_eq!(hex(&out), "41edece42d63e8d9bf515a9ba6932e1c20cbc9f5a5d134645adb5db1b9737ea3");
        }
    }
}
