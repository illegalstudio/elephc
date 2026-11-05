//! Purpose:
//! Implements every PHP `ext/sodium` operation on top of libsodium.
//!
//! Called from:
//! - `crate::dispatch` for the generic `elephc_sodium_call` C entry point.
//!
//! Key details:
//! - PHP argument validation lives in the compiled prelude, so each handler receives
//!   already length-checked buffers. A violated invariant reports `Internal` rather than
//!   reading out of bounds.
//! - Failure statuses distinguish PHP's observable `false` results from exception-shaped
//!   failures (`invalid base64`, `invalid padding`, conversion and identity errors).
//! - `sodium_memzero` preserves PHP's observable null reset; the bridge zeroizes its own
//!   decoded temporary buffers after each call. The caller PHP string storage is owned by
//!   the generated runtime, not this library.

use std::os::raw::{c_char, c_int, c_ulonglong};

use crate::ffi::*;

/// Result of one bridge operation.
pub(crate) enum Outcome {
    /// Success with the exact output bytes.
    Bytes(Vec<u8>),
    /// Success with one integer or boolean result.
    Integer(i64),
    /// PHP's `false` result (authentication or decryption failure).
    False,
    /// The input was not valid base64.
    InvalidBase64,
    /// The input was not valid hexadecimal.
    InvalidHex,
    /// The input was not valid libsodium padding.
    InvalidPadding,
    /// An Ed25519/Curve25519 conversion failed.
    ConversionFailed,
    /// A Ristretto255 scalar multiplication produced the identity element.
    IdentityElement,
    /// A Ristretto255 base multiplication received a zero scalar.
    ZeroScalar,
    /// libsodium reported an unexpected failure.
    Internal,
}

/// Dispatches one named PHP sodium operation over four byte and three integer slots.
pub(crate) fn dispatch(name: &str, a: [&[u8]; 4], i: [i64; 3]) -> Outcome {
    match name {
        "sodium_add" => add(a[0], a[1]),
        "sodium_base642bin" => base642bin(a[0], i[0], a[1]),
        "sodium_bin2base64" => bin2base64(a[0], i[0]),
        "sodium_bin2hex" => Outcome::Bytes(hex_encode(a[0])),
        "sodium_compare" => compare(a[0], a[1]),
        "sodium_hex2bin" => hex2bin(a[0], a[1]),
        "sodium_increment" => increment(a[0]),
        "sodium_memcmp" => memcmp(a[0], a[1]),
        "sodium_memzero" => Outcome::Bytes(Vec::new()),
        "password_hash_bcrypt" => password_hash_bcrypt(a[0], i[0]),
        "password_verify_bcrypt" => password_verify_bcrypt(a[0], a[1]),
        "sodium_pad" => pad(a[0], i[0]),
        "sodium_unpad" => unpad(a[0], i[0]),
        "sodium_crypto_aead_aes256gcm_is_available" => {
            Outcome::Integer(i64::from(unsafe { crypto_aead_aes256gcm_is_available() } != 0))
        }
        "sodium_crypto_auth" => auth(a[0], a[1]),
        "sodium_crypto_auth_verify" => auth_verify(a[0], a[1], a[2]),
        "sodium_crypto_box_seed_keypair" => box_seed_keypair(a[0]),
        "sodium_crypto_kx_seed_keypair" => kx_seed_keypair(a[0]),
        "sodium_crypto_sign_seed_keypair" => sign_seed_keypair(a[0]),
        "sodium_crypto_core_ristretto255_random" => keygen(name, a),
        "sodium_crypto_core_ristretto255_scalar_random" => keygen(name, a),
        name if name.ends_with("_keygen") || name.ends_with("_keypair") => keygen(name, a),
        name if name.starts_with("sodium_crypto_aead_") => aead(name, a),
        "sodium_crypto_box" => box_encrypt(a[0], a[1], a[2]),
        "sodium_crypto_box_open" => box_open(a[0], a[1], a[2]),
        "sodium_crypto_box_seal" => box_seal(a[0], a[1]),
        "sodium_crypto_box_seal_open" => box_seal_open(a[0], a[1]),
        "sodium_crypto_box_publickey_from_secretkey" => box_publickey_from_secretkey(a[0]),
        "sodium_crypto_box_keypair_from_secretkey_and_publickey" => keypair_concat(a[0], a[1], 32),
        "sodium_crypto_box_secretkey" => keypair_part(a[0], 0, 32),
        "sodium_crypto_box_publickey" => keypair_part(a[0], 32, 32),
        "sodium_crypto_generichash" => generichash(a[0], a[1], i[0]),
        "sodium_crypto_generichash_init" => generichash_init(a[0], i[0]),
        "sodium_crypto_generichash_update" => generichash_update(a[0], a[1]),
        "sodium_crypto_generichash_final" => generichash_final(a[0], i[0]),
        "sodium_crypto_kdf_derive_from_key" => kdf_derive(a[0], a[1], a[2], i[0], i[1]),
        "sodium_crypto_kx_client_session_keys" => kx_client_session_keys(a[0], a[1]),
        "sodium_crypto_kx_server_session_keys" => kx_server_session_keys(a[0], a[1]),
        "sodium_crypto_kx_secretkey" => keypair_part(a[0], 0, 32),
        "sodium_crypto_kx_publickey" => keypair_part(a[0], 32, 32),
        "sodium_crypto_pwhash_str" => pwhash_str(a[0], i[0], i[1]),
        "sodium_crypto_pwhash_str_verify" => pwhash_str_verify(a[0], a[1]),
        "sodium_crypto_pwhash_str_needs_rehash" => pwhash_str_needs_rehash(a[0], i[0], i[1]),
        "sodium_crypto_pwhash_scryptsalsa208sha256" => scrypt(a[0], a[1], i[0], i[1], i[2]),
        "sodium_crypto_pwhash_scryptsalsa208sha256_str" => scrypt_str(a[0], i[0], i[1]),
        "sodium_crypto_pwhash_scryptsalsa208sha256_str_verify" => scrypt_str_verify(a[0], a[1]),
        "sodium_crypto_scalarmult" => scalarmult(a[0], a[1]),
        "sodium_crypto_scalarmult_base" => scalarmult_base(a[0]),
        "sodium_crypto_secretbox" => secretbox(a[0], a[1], a[2]),
        "sodium_crypto_secretbox_open" => secretbox_open(a[0], a[1], a[2]),
        "sodium_crypto_shorthash" => shorthash(a[0], a[1]),
        "sodium_crypto_sign" => sign(a[0], a[1]),
        "sodium_crypto_sign_open" => sign_open(a[0], a[1]),
        "sodium_crypto_sign_detached" => sign_detached(a[0], a[1]),
        "sodium_crypto_sign_verify_detached" => sign_verify_detached(a[0], a[1], a[2]),
        "sodium_crypto_sign_publickey_from_secretkey" => sign_publickey_from_secretkey(a[0]),
        "sodium_crypto_sign_keypair_from_secretkey_and_publickey" => keypair_concat(a[0], a[1], 64),
        "sodium_crypto_sign_secretkey" => keypair_part(a[0], 0, 64),
        "sodium_crypto_sign_publickey" => keypair_part(a[0], 64, 32),
        "sodium_crypto_sign_ed25519_sk_to_curve25519" => ed25519_sk_to_curve25519(a[0]),
        "sodium_crypto_sign_ed25519_pk_to_curve25519" => ed25519_pk_to_curve25519(a[0]),
        "sodium_crypto_stream" => stream(a[0], a[1], i[0]),
        "sodium_crypto_stream_xor" => stream_xor(a[0], a[1], a[2]),
        "sodium_crypto_stream_xchacha20" => stream_xchacha20(a[0], a[1], i[0]),
        "sodium_crypto_stream_xchacha20_xor" => stream_xchacha20_xor(a[0], a[1], a[2]),
        "sodium_crypto_stream_xchacha20_xor_ic" => stream_xchacha20_xor_ic(a[0], a[1], a[2], i[0]),
        "sodium_crypto_secretstream_xchacha20poly1305_init_push" => secretstream_init_push(a[0]),
        "sodium_crypto_secretstream_xchacha20poly1305_init_pull" => secretstream_init_pull(a[0], a[1]),
        "sodium_crypto_secretstream_xchacha20poly1305_push" => secretstream_push(a[0], a[1], a[2], i[0]),
        "sodium_crypto_secretstream_xchacha20poly1305_pull" => secretstream_pull(a[0], a[1], a[2]),
        "sodium_crypto_secretstream_xchacha20poly1305_rekey" => secretstream_rekey(a[0]),
        "sodium_crypto_core_ristretto255_add" => ristretto_add(a[0], a[1]),
        "sodium_crypto_core_ristretto255_sub" => ristretto_sub(a[0], a[1]),
        "sodium_crypto_core_ristretto255_from_hash" => ristretto_from_hash(a[0]),
        "sodium_crypto_core_ristretto255_is_valid_point" => ristretto_is_valid(a[0]),
        "sodium_crypto_core_ristretto255_scalar_invert" => ristretto_scalar_invert(a[0]),
        "sodium_crypto_core_ristretto255_scalar_add" => ristretto_scalar_binary(a[0], a[1], RistrettoScalarOp::Add),
        "sodium_crypto_core_ristretto255_scalar_sub" => ristretto_scalar_binary(a[0], a[1], RistrettoScalarOp::Sub),
        "sodium_crypto_core_ristretto255_scalar_mul" => ristretto_scalar_binary(a[0], a[1], RistrettoScalarOp::Mul),
        "sodium_crypto_core_ristretto255_scalar_complement" => ristretto_scalar_unary(a[0], RistrettoScalarOp::Complement),
        "sodium_crypto_core_ristretto255_scalar_negate" => ristretto_scalar_unary(a[0], RistrettoScalarOp::Negate),
        "sodium_crypto_core_ristretto255_scalar_reduce" => ristretto_scalar_unary(a[0], RistrettoScalarOp::Reduce),
        "sodium_crypto_scalarmult_ristretto255" => scalarmult_ristretto(a[0], a[1]),
        "sodium_crypto_scalarmult_ristretto255_base" => scalarmult_ristretto_base(a[0]),
        _ => Outcome::Internal,
    }
}

/// Returns libsodium's linked version string.
pub(crate) fn version() -> String {
    unsafe {
        let pointer = sodium_version_string();
        if pointer.is_null() {
            return String::new();
        }
        std::ffi::CStr::from_ptr(pointer).to_string_lossy().into_owned()
    }
}

/// Returns libsodium's linked library major version.
pub(crate) fn version_major() -> i64 {
    unsafe { sodium_library_version_major() as i64 }
}

/// Returns libsodium's linked library minor version.
pub(crate) fn version_minor() -> i64 {
    unsafe { sodium_library_version_minor() as i64 }
}

/// Fills `len` bytes with cryptographically secure random data.
fn random_bytes(len: usize) -> Vec<u8> {
    let mut output = vec![0u8; len];
    unsafe { randombytes_buf(output.as_mut_ptr(), len) };
    output
}

/// Encodes bytes as lowercase hexadecimal.
fn hex_encode(input: &[u8]) -> Vec<u8> {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = Vec::with_capacity(input.len() * 2);
    for byte in input {
        output.push(DIGITS[(byte >> 4) as usize]);
        output.push(DIGITS[(byte & 0x0f) as usize]);
    }
    output
}

/// Adds two equal-length little-endian integers in place.
fn add(first: &[u8], second: &[u8]) -> Outcome {
    if first.len() != second.len() {
        return Outcome::Internal;
    }
    let mut output = first.to_vec();
    unsafe { sodium_add(output.as_mut_ptr(), second.as_ptr(), output.len()) };
    Outcome::Bytes(output)
}

/// Increments one little-endian integer.
fn increment(input: &[u8]) -> Outcome {
    let mut output = input.to_vec();
    unsafe { sodium_increment(output.as_mut_ptr(), output.len()) };
    Outcome::Bytes(output)
}

/// Compares two equal-length strings in constant time.
fn memcmp(first: &[u8], second: &[u8]) -> Outcome {
    if first.len() != second.len() {
        return Outcome::Internal;
    }
    let result = unsafe { sodium_memcmp(first.as_ptr(), second.as_ptr(), first.len()) };
    Outcome::Integer(result as i64)
}

/// Compares two equal-length strings lexicographically.
fn compare(first: &[u8], second: &[u8]) -> Outcome {
    if first.len() != second.len() {
        return Outcome::Internal;
    }
    let result = unsafe { sodium_compare(first.as_ptr(), second.as_ptr(), first.len()) };
    Outcome::Integer(result as i64)
}

/// Decodes base64 with one sodium variant and optional ignored characters.
fn base642bin(input: &[u8], variant: i64, ignore: &[u8]) -> Outcome {
    let mut output = vec![0u8; input.len() / 4 * 3 + 3];
    let mut output_len = 0usize;
    let mut end: *const c_char = std::ptr::null();
    let ignore = (!ignore.is_empty()).then(|| nul_terminated(ignore));
    let ignore_ptr = ignore
        .as_ref()
        .map_or(std::ptr::null(), |ignore| ignore.as_ptr() as *const c_char);
    let input_end = unsafe { input.as_ptr().add(input.len()) } as *const c_char;
    let result = unsafe {
        sodium_base642bin(
            output.as_mut_ptr(),
            output.len(),
            input.as_ptr() as *const c_char,
            input.len(),
            ignore_ptr,
            &mut output_len,
            &mut end,
            variant as c_int,
        )
    };
    if result != 0 || end != input_end {
        return Outcome::InvalidBase64;
    }
    output.truncate(output_len);
    Outcome::Bytes(output)
}

/// Encodes base64 with one sodium variant.
fn bin2base64(input: &[u8], variant: i64) -> Outcome {
    let mut output = vec![0u8; (input.len() + 2) / 3 * 4 + 1];
    let result = unsafe {
        sodium_bin2base64(
            output.as_mut_ptr() as *mut c_char,
            output.len(),
            input.as_ptr(),
            input.len(),
            variant as c_int,
        )
    };
    if result.is_null() {
        return Outcome::Internal;
    }
    truncate_at_nul(output)
}

/// Decodes hexadecimal with optional ignored characters.
fn hex2bin(input: &[u8], ignore: &[u8]) -> Outcome {
    let mut output = vec![0u8; input.len() / 2 + 1];
    let mut output_len = 0usize;
    let mut end: *const c_char = std::ptr::null();
    let ignore = (!ignore.is_empty()).then(|| nul_terminated(ignore));
    let ignore_ptr = ignore
        .as_ref()
        .map_or(std::ptr::null(), |ignore| ignore.as_ptr() as *const c_char);
    let input_end = unsafe { input.as_ptr().add(input.len()) } as *const c_char;
    let result = unsafe {
        sodium_hex2bin(
            output.as_mut_ptr(),
            output.len(),
            input.as_ptr() as *const c_char,
            input.len(),
            ignore_ptr,
            &mut output_len,
            &mut end,
        )
    };
    if result != 0 || end != input_end {
        return Outcome::InvalidHex;
    }
    output.truncate(output_len);
    Outcome::Bytes(output)
}

/// Hashes a password with PHP's bcrypt provider (`$2y$`, cost 4..=31).
fn password_hash_bcrypt(password: &[u8], cost: i64) -> Outcome {
    if !(4..=31).contains(&cost) {
        return Outcome::Internal;
    }
    match bcrypt::hash_with_result(password, cost as u32) {
        Ok(parts) => Outcome::Bytes(
            parts
                .format_for_version(bcrypt::Version::TwoY)
                .into_bytes(),
        ),
        Err(_) => Outcome::Internal,
    }
}

/// Verifies a password against a bcrypt hash.
fn password_verify_bcrypt(password: &[u8], hash: &[u8]) -> Outcome {
    let Ok(hash) = std::str::from_utf8(hash) else {
        return Outcome::Integer(0);
    };
    Outcome::Integer(i64::from(matches!(
        bcrypt::verify(password, hash),
        Ok(true)
    )))
}

/// Pads a string with libsodium's ISO/IEC 7816-4 scheme.
fn pad(input: &[u8], block_size: i64) -> Outcome {
    if block_size <= 0 {
        return Outcome::Internal;
    }
    let block_size = block_size as usize;
    let maximum = input.len().saturating_add(block_size).saturating_add(1);
    let mut output = vec![0u8; maximum];
    output[..input.len()].copy_from_slice(input);
    let mut output_len = 0usize;
    let result = unsafe {
        sodium_pad(
            &mut output_len,
            output.as_mut_ptr(),
            input.len(),
            block_size,
            output.len(),
        )
    };
    if result != 0 {
        return Outcome::Internal;
    }
    output.truncate(output_len);
    Outcome::Bytes(output)
}

/// Removes libsodium's ISO/IEC 7816-4 padding.
fn unpad(input: &[u8], block_size: i64) -> Outcome {
    if block_size <= 0 {
        return Outcome::Internal;
    }
    let mut output_len = 0usize;
    let result = unsafe {
        sodium_unpad(
            &mut output_len,
            input.as_ptr(),
            input.len(),
            block_size as usize,
        )
    };
    if result != 0 {
        return Outcome::InvalidPadding;
    }
    Outcome::Bytes(input[..output_len].to_vec())
}

/// Appends a terminating NUL for libsodium C-string parameters.
fn nul_terminated(input: &[u8]) -> Vec<u8> {
    let mut output = input.to_vec();
    output.push(0);
    output
}

/// Truncates a libsodium C-string result at its first NUL byte.
fn truncate_at_nul(mut output: Vec<u8>) -> Outcome {
    if let Some(position) = output.iter().position(|byte| *byte == 0) {
        output.truncate(position);
    }
    Outcome::Bytes(output)
}


/// Returns the first `len` bytes at `offset`, or `Internal` when the input is too short.
fn keypair_part(keypair: &[u8], offset: usize, len: usize) -> Outcome {
    match keypair.get(offset..offset + len) {
        Some(part) => Outcome::Bytes(part.to_vec()),
        None => Outcome::Internal,
    }
}

/// Concatenates a secret key and public key into PHP's key-pair layout.
fn keypair_concat(secret: &[u8], public: &[u8], secret_len: usize) -> Outcome {
    if secret.len() != secret_len || public.len() != 32 {
        return Outcome::Internal;
    }
    let mut output = Vec::with_capacity(secret.len() + public.len());
    output.extend_from_slice(secret);
    output.extend_from_slice(public);
    Outcome::Bytes(output)
}

/// Generates every key or key pair whose PHP name ends in `_keygen` or `_keypair`.
fn keygen(name: &str, _a: [&[u8]; 4]) -> Outcome {
    match name {
        "sodium_crypto_aead_aegis128l_keygen" => Outcome::Bytes(random_bytes(16)),
        "sodium_crypto_aead_aegis256_keygen"
        | "sodium_crypto_aead_aes256gcm_keygen"
        | "sodium_crypto_aead_chacha20poly1305_keygen"
        | "sodium_crypto_aead_chacha20poly1305_ietf_keygen"
        | "sodium_crypto_aead_xchacha20poly1305_ietf_keygen" => Outcome::Bytes(random_bytes(32)),
        "sodium_crypto_auth_keygen" => Outcome::Bytes(random_bytes(32)),
        "sodium_crypto_generichash_keygen" => Outcome::Bytes(random_bytes(32)),
        "sodium_crypto_kdf_keygen" => Outcome::Bytes(random_bytes(32)),
        "sodium_crypto_secretbox_keygen" => Outcome::Bytes(random_bytes(32)),
        "sodium_crypto_shorthash_keygen" => Outcome::Bytes(random_bytes(16)),
        "sodium_crypto_stream_keygen" | "sodium_crypto_stream_xchacha20_keygen" => Outcome::Bytes(random_bytes(32)),
        "sodium_crypto_secretstream_xchacha20poly1305_keygen" => Outcome::Bytes(random_bytes(32)),
        "sodium_crypto_core_ristretto255_random" => {
            let mut point = vec![0u8; 32];
            unsafe { crypto_core_ristretto255_random(point.as_mut_ptr()) };
            Outcome::Bytes(point)
        }
        "sodium_crypto_core_ristretto255_scalar_random" => {
            let mut scalar = vec![0u8; 32];
            unsafe { crypto_core_ristretto255_scalar_random(scalar.as_mut_ptr()) };
            Outcome::Bytes(scalar)
        }
        "sodium_crypto_box_keypair" => {
            let mut public = vec![0u8; 32];
            let mut secret = vec![0u8; 32];
            if unsafe { crypto_box_keypair(public.as_mut_ptr(), secret.as_mut_ptr()) } != 0 {
                return Outcome::Internal;
            }
            let mut pair = secret;
            pair.extend_from_slice(&public);
            Outcome::Bytes(pair)
        }
        "sodium_crypto_kx_keypair" => {
            let mut public = vec![0u8; 32];
            let mut secret = vec![0u8; 32];
            if unsafe { crypto_kx_keypair(public.as_mut_ptr(), secret.as_mut_ptr()) } != 0 {
                return Outcome::Internal;
            }
            let mut pair = secret;
            pair.extend_from_slice(&public);
            Outcome::Bytes(pair)
        }
        "sodium_crypto_sign_keypair" => {
            let mut public = vec![0u8; 32];
            let mut secret = vec![0u8; 64];
            if unsafe { crypto_sign_keypair(public.as_mut_ptr(), secret.as_mut_ptr()) } != 0 {
                return Outcome::Internal;
            }
            let mut pair = secret;
            pair.extend_from_slice(&public);
            Outcome::Bytes(pair)
        }
        _ => return Outcome::Internal,
    }
}

/// Derives a box key pair from a seed.
fn box_seed_keypair(seed: &[u8]) -> Outcome {
    if seed.len() != 32 {
        return Outcome::Internal;
    }
    let mut public = vec![0u8; 32];
    let mut secret = vec![0u8; 32];
    if unsafe { crypto_box_seed_keypair(public.as_mut_ptr(), secret.as_mut_ptr(), seed.as_ptr()) } != 0 {
        return Outcome::Internal;
    }
    let mut pair = secret;
    pair.extend_from_slice(&public);
    Outcome::Bytes(pair)
}

/// Derives a kx key pair from a seed.
fn kx_seed_keypair(seed: &[u8]) -> Outcome {
    if seed.len() != 32 {
        return Outcome::Internal;
    }
    let mut public = vec![0u8; 32];
    let mut secret = vec![0u8; 32];
    if unsafe { crypto_kx_seed_keypair(public.as_mut_ptr(), secret.as_mut_ptr(), seed.as_ptr()) } != 0 {
        return Outcome::Internal;
    }
    let mut pair = secret;
    pair.extend_from_slice(&public);
    Outcome::Bytes(pair)
}

/// Derives a signature key pair from a seed.
fn sign_seed_keypair(seed: &[u8]) -> Outcome {
    if seed.len() != 32 {
        return Outcome::Internal;
    }
    let mut public = vec![0u8; 32];
    let mut secret = vec![0u8; 64];
    if unsafe { crypto_sign_seed_keypair(public.as_mut_ptr(), secret.as_mut_ptr(), seed.as_ptr()) } != 0 {
        return Outcome::Internal;
    }
    let mut pair = secret;
    pair.extend_from_slice(&public);
    Outcome::Bytes(pair)
}

/// Derives a box public key from a secret key.
fn box_publickey_from_secretkey(secret: &[u8]) -> Outcome {
    if secret.len() != 32 {
        return Outcome::Internal;
    }
    let mut public = vec![0u8; 32];
    if unsafe { crypto_scalarmult_base(public.as_mut_ptr(), secret.as_ptr()) } != 0 {
        return Outcome::Internal;
    }
    Outcome::Bytes(public)
}

/// Encrypts with a Curve25519 box.
fn box_encrypt(message: &[u8], nonce: &[u8], keypair: &[u8]) -> Outcome {
    if nonce.len() != 24 || keypair.len() != 64 {
        return Outcome::Internal;
    }
    let (secret, public) = keypair.split_at(32);
    let mut output = vec![0u8; message.len() + 16];
    let result = unsafe {
        crypto_box_easy(
            output.as_mut_ptr(),
            message.as_ptr(),
            message.len() as c_ulonglong,
            nonce.as_ptr(),
            public.as_ptr(),
            secret.as_ptr(),
        )
    };
    if result != 0 {
        return Outcome::Internal;
    }
    Outcome::Bytes(output)
}

/// Decrypts a Curve25519 box.
fn box_open(ciphertext: &[u8], nonce: &[u8], keypair: &[u8]) -> Outcome {
    if nonce.len() != 24 || keypair.len() != 64 || ciphertext.len() < 16 {
        return Outcome::False;
    }
    let (secret, public) = keypair.split_at(32);
    let mut output = vec![0u8; ciphertext.len() - 16];
    let result = unsafe {
        crypto_box_open_easy(
            output.as_mut_ptr(),
            ciphertext.as_ptr(),
            ciphertext.len() as c_ulonglong,
            nonce.as_ptr(),
            public.as_ptr(),
            secret.as_ptr(),
        )
    };
    if result != 0 {
        return Outcome::False;
    }
    Outcome::Bytes(output)
}

/// Encrypts an anonymous sealed box.
fn box_seal(message: &[u8], public_key: &[u8]) -> Outcome {
    if public_key.len() != 32 {
        return Outcome::Internal;
    }
    let mut output = vec![0u8; message.len() + 48];
    if unsafe {
        crypto_box_seal(
            output.as_mut_ptr(),
            message.as_ptr(),
            message.len() as c_ulonglong,
            public_key.as_ptr(),
        )
    } != 0
    {
        return Outcome::Internal;
    }
    Outcome::Bytes(output)
}

/// Decrypts an anonymous sealed box.
fn box_seal_open(ciphertext: &[u8], keypair: &[u8]) -> Outcome {
    if keypair.len() != 64 || ciphertext.len() < 48 {
        return Outcome::False;
    }
    let (secret, public) = keypair.split_at(32);
    let mut output = vec![0u8; ciphertext.len() - 48];
    if unsafe {
        crypto_box_seal_open(
            output.as_mut_ptr(),
            ciphertext.as_ptr(),
            ciphertext.len() as c_ulonglong,
            public.as_ptr(),
            secret.as_ptr(),
        )
    } != 0
    {
        return Outcome::False;
    }
    Outcome::Bytes(output)
}

/// Computes a one-shot BLAKE2b hash with an optional key.
fn generichash(message: &[u8], key: &[u8], out_len: i64) -> Outcome {
    if out_len < 16 || out_len > 64 || !key.is_empty() && !(16..=64).contains(&key.len()) {
        return Outcome::Internal;
    }
    let mut output = vec![0u8; out_len as usize];
    let result = unsafe {
        crypto_generichash(
            output.as_mut_ptr(),
            output.len(),
            message.as_ptr(),
            message.len() as c_ulonglong,
            if key.is_empty() { std::ptr::null() } else { key.as_ptr() },
            key.len(),
        )
    };
    if result != 0 {
        return Outcome::Internal;
    }
    Outcome::Bytes(output)
}

/// Initializes an incremental BLAKE2b state.
fn generichash_init(key: &[u8], out_len: i64) -> Outcome {
    if out_len < 16 || out_len > 64 || !key.is_empty() && !(16..=64).contains(&key.len()) {
        return Outcome::Internal;
    }
    let mut state = GenerichashState([0u8; 384]);
    let result = unsafe {
        crypto_generichash_init(
            &mut state,
            if key.is_empty() { std::ptr::null() } else { key.as_ptr() },
            key.len(),
            out_len as usize,
        )
    };
    if result != 0 {
        return Outcome::Internal;
    }
    Outcome::Bytes(state.0.to_vec())
}

/// Advances an incremental BLAKE2b state.
fn generichash_update(packed_state: &[u8], message: &[u8]) -> Outcome {
    if packed_state.len() != 384 {
        return Outcome::Internal;
    }
    let mut state = GenerichashState([0u8; 384]);
    state.0.copy_from_slice(packed_state);
    let result = unsafe {
        crypto_generichash_update(&mut state, message.as_ptr(), message.len() as c_ulonglong)
    };
    if result != 0 {
        return Outcome::Internal;
    }
    Outcome::Bytes(state.0.to_vec())
}

/// Finalizes an incremental BLAKE2b state.
fn generichash_final(packed_state: &[u8], out_len: i64) -> Outcome {
    if packed_state.len() != 384 || out_len < 16 || out_len > 64 {
        return Outcome::Internal;
    }
    let mut state = GenerichashState([0u8; 384]);
    state.0.copy_from_slice(packed_state);
    let mut output = vec![0u8; out_len as usize];
    let result = unsafe { crypto_generichash_final(&mut state, output.as_mut_ptr(), output.len()) };
    if result != 0 {
        return Outcome::Internal;
    }
    Outcome::Bytes(output)
}

/// Computes a Poly1305 authentication tag.
fn auth(message: &[u8], key: &[u8]) -> Outcome {
    if key.len() != 32 {
        return Outcome::Internal;
    }
    let mut output = vec![0u8; 32];
    if unsafe {
        crypto_auth(
            output.as_mut_ptr(),
            message.as_ptr(),
            message.len() as c_ulonglong,
            key.as_ptr(),
        )
    } != 0
    {
        return Outcome::Internal;
    }
    Outcome::Bytes(output)
}

/// Verifies a Poly1305 authentication tag.
fn auth_verify(tag: &[u8], message: &[u8], key: &[u8]) -> Outcome {
    if tag.len() != 32 || key.len() != 32 {
        return Outcome::Internal;
    }
    let result = unsafe {
        crypto_auth_verify(
            tag.as_ptr(),
            message.as_ptr(),
            message.len() as c_ulonglong,
            key.as_ptr(),
        )
    };
    Outcome::Integer(i64::from(result == 0))
}

/// Derives a Key Derivation Function subkey.
fn kdf_derive(context: &[u8], key: &[u8], _unused: &[u8], subkey_len: i64, subkey_id: i64) -> Outcome {
    if context.len() != 8 || key.len() != 32 || subkey_len < 16 || subkey_len > 64 || subkey_id < 0 {
        return Outcome::Internal;
    }
    let mut output = vec![0u8; subkey_len as usize];
    let result = unsafe {
        crypto_kdf_derive_from_key(
            output.as_mut_ptr(),
            output.len(),
            subkey_id as c_ulonglong,
            context.as_ptr() as *const c_char,
            key.as_ptr(),
        )
    };
    if result != 0 {
        return Outcome::Internal;
    }
    Outcome::Bytes(output)
}

/// Derives client session keys, returned as `[rx, tx]` concatenated.
fn kx_client_session_keys(keypair: &[u8], server_public: &[u8]) -> Outcome {
    if keypair.len() != 64 || server_public.len() != 32 {
        return Outcome::Internal;
    }
    let (secret, public) = keypair.split_at(32);
    let mut receive = vec![0u8; 32];
    let mut transmit = vec![0u8; 32];
    let result = unsafe {
        crypto_kx_client_session_keys(
            receive.as_mut_ptr(),
            transmit.as_mut_ptr(),
            public.as_ptr(),
            secret.as_ptr(),
            server_public.as_ptr(),
        )
    };
    if result != 0 {
        return Outcome::Internal;
    }
    receive.extend_from_slice(&transmit);
    Outcome::Bytes(receive)
}

/// Derives server session keys, returned as `[rx, tx]` concatenated.
fn kx_server_session_keys(keypair: &[u8], client_public: &[u8]) -> Outcome {
    if keypair.len() != 64 || client_public.len() != 32 {
        return Outcome::Internal;
    }
    let (secret, public) = keypair.split_at(32);
    let mut receive = vec![0u8; 32];
    let mut transmit = vec![0u8; 32];
    let result = unsafe {
        crypto_kx_server_session_keys(
            receive.as_mut_ptr(),
            transmit.as_mut_ptr(),
            public.as_ptr(),
            secret.as_ptr(),
            client_public.as_ptr(),
        )
    };
    if result != 0 {
        return Outcome::Internal;
    }
    receive.extend_from_slice(&transmit);
    Outcome::Bytes(receive)
}

/// Derives a key from a password with Argon2.
pub(crate) fn pwhash(password: &[u8], salt: &[u8], out_len: i64, ops_limit: i64, mem_limit: i64, algorithm: i64) -> Outcome {
    if salt.len() != 16 || out_len <= 0 || ops_limit <= 0 || mem_limit <= 0 {
        return Outcome::Internal;
    }
    let mut output = vec![0u8; out_len as usize];
    let result = unsafe {
        crypto_pwhash(
            output.as_mut_ptr(),
            output.len() as c_ulonglong,
            password.as_ptr() as *const c_char,
            password.len() as c_ulonglong,
            salt.as_ptr(),
            ops_limit as c_ulonglong,
            mem_limit as usize,
            algorithm as c_int,
        )
    };
    if result != 0 {
        return Outcome::Internal;
    }
    Outcome::Bytes(output)
}

/// Hashes a password with the selected Argon2 algorithm in libsodium's string format.
pub(crate) fn pwhash_str_alg(password: &[u8], ops_limit: i64, mem_limit: i64, algorithm: i64) -> Outcome {
    if ops_limit <= 0 || mem_limit <= 0 || !(1..=2).contains(&algorithm) {
        return Outcome::Internal;
    }
    let mut output = vec![0u8; 128];
    let result = unsafe {
        crypto_pwhash_str_alg(
            output.as_mut_ptr() as *mut c_char,
            password.as_ptr() as *const c_char,
            password.len() as c_ulonglong,
            ops_limit as c_ulonglong,
            mem_limit as usize,
            algorithm as c_int,
        )
    };
    if result != 0 {
        return Outcome::Internal;
    }
    truncate_at_nul(output)
}

/// Hashes a password with Argon2id in libsodium's string format.
fn pwhash_str(password: &[u8], ops_limit: i64, mem_limit: i64) -> Outcome {
    if ops_limit <= 0 || mem_limit <= 0 {
        return Outcome::Internal;
    }
    let mut output = vec![0u8; 128];
    let result = unsafe {
        crypto_pwhash_str(
            output.as_mut_ptr() as *mut c_char,
            password.as_ptr() as *const c_char,
            password.len() as c_ulonglong,
            ops_limit as c_ulonglong,
            mem_limit as usize,
        )
    };
    if result != 0 {
        return Outcome::Internal;
    }
    truncate_at_nul(output)
}

/// Verifies an Argon2 password hash.
fn pwhash_str_verify(hash: &[u8], password: &[u8]) -> Outcome {
    let hash = nul_terminated(hash);
    let result = unsafe {
        crypto_pwhash_str_verify(
            hash.as_ptr() as *const c_char,
            password.as_ptr() as *const c_char,
            password.len() as c_ulonglong,
        )
    };
    if result == 0 {
        Outcome::Integer(1)
    } else {
        Outcome::Integer(0)
    }
}

/// Reports whether an Argon2 password hash needs rehashing.
fn pwhash_str_needs_rehash(hash: &[u8], ops_limit: i64, mem_limit: i64) -> Outcome {
    let hash = nul_terminated(hash);
    let result = unsafe {
        crypto_pwhash_str_needs_rehash(
            hash.as_ptr() as *const c_char,
            ops_limit as c_ulonglong,
            mem_limit as usize,
        )
    };
    Outcome::Integer(i64::from(result != 0))
}

/// Derives a key from a password with scrypt.
fn scrypt(password: &[u8], salt: &[u8], out_len: i64, ops_limit: i64, mem_limit: i64) -> Outcome {
    if salt.len() != 32 || out_len <= 0 || ops_limit <= 0 || mem_limit <= 0 {
        return Outcome::Internal;
    }
    let mut output = vec![0u8; out_len as usize];
    let result = unsafe {
        crypto_pwhash_scryptsalsa208sha256(
            output.as_mut_ptr(),
            output.len() as c_ulonglong,
            password.as_ptr() as *const c_char,
            password.len() as c_ulonglong,
            salt.as_ptr(),
            ops_limit as c_ulonglong,
            mem_limit as usize,
        )
    };
    if result != 0 {
        return Outcome::Internal;
    }
    Outcome::Bytes(output)
}

/// Hashes a password with scrypt in libsodium's string format.
fn scrypt_str(password: &[u8], ops_limit: i64, mem_limit: i64) -> Outcome {
    if ops_limit <= 0 || mem_limit <= 0 {
        return Outcome::Internal;
    }
    let mut output = vec![0u8; 102];
    let result = unsafe {
        crypto_pwhash_scryptsalsa208sha256_str(
            output.as_mut_ptr() as *mut c_char,
            password.as_ptr() as *const c_char,
            password.len() as c_ulonglong,
            ops_limit as c_ulonglong,
            mem_limit as usize,
        )
    };
    if result != 0 {
        return Outcome::Internal;
    }
    truncate_at_nul(output)
}

/// Verifies a scrypt password hash.
fn scrypt_str_verify(hash: &[u8], password: &[u8]) -> Outcome {
    let hash = nul_terminated(hash);
    let result = unsafe {
        crypto_pwhash_scryptsalsa208sha256_str_verify(
            hash.as_ptr() as *const c_char,
            password.as_ptr() as *const c_char,
            password.len() as c_ulonglong,
        )
    };
    if result == 0 {
        Outcome::Integer(1)
    } else {
        Outcome::Integer(0)
    }
}

/// Multiplies a Curve25519 point by a scalar.
fn scalarmult(scalar: &[u8], point: &[u8]) -> Outcome {
    if scalar.len() != 32 || point.len() != 32 {
        return Outcome::Internal;
    }
    let mut output = vec![0u8; 32];
    if unsafe { crypto_scalarmult(output.as_mut_ptr(), scalar.as_ptr(), point.as_ptr()) } != 0 {
        return Outcome::Internal;
    }
    Outcome::Bytes(output)
}

/// Computes a Curve25519 public key from a secret key.
fn scalarmult_base(scalar: &[u8]) -> Outcome {
    if scalar.len() != 32 {
        return Outcome::Internal;
    }
    let mut output = vec![0u8; 32];
    if unsafe { crypto_scalarmult_base(output.as_mut_ptr(), scalar.as_ptr()) } != 0 {
        return Outcome::Internal;
    }
    Outcome::Bytes(output)
}

/// Encrypts with XSalsa20-Poly1305.
fn secretbox(message: &[u8], nonce: &[u8], key: &[u8]) -> Outcome {
    if nonce.len() != 24 || key.len() != 32 {
        return Outcome::Internal;
    }
    let mut output = vec![0u8; message.len() + 16];
    if unsafe {
        crypto_secretbox_easy(
            output.as_mut_ptr(),
            message.as_ptr(),
            message.len() as c_ulonglong,
            nonce.as_ptr(),
            key.as_ptr(),
        )
    } != 0
    {
        return Outcome::Internal;
    }
    Outcome::Bytes(output)
}

/// Decrypts an XSalsa20-Poly1305 ciphertext.
fn secretbox_open(ciphertext: &[u8], nonce: &[u8], key: &[u8]) -> Outcome {
    if nonce.len() != 24 || key.len() != 32 || ciphertext.len() < 16 {
        return Outcome::False;
    }
    let mut output = vec![0u8; ciphertext.len() - 16];
    if unsafe {
        crypto_secretbox_open_easy(
            output.as_mut_ptr(),
            ciphertext.as_ptr(),
            ciphertext.len() as c_ulonglong,
            nonce.as_ptr(),
            key.as_ptr(),
        )
    } != 0
    {
        return Outcome::False;
    }
    Outcome::Bytes(output)
}

/// Computes a SipHash-2-4 short hash.
fn shorthash(message: &[u8], key: &[u8]) -> Outcome {
    if key.len() != 16 {
        return Outcome::Internal;
    }
    let mut output = vec![0u8; 8];
    if unsafe {
        crypto_shorthash(
            output.as_mut_ptr(),
            message.as_ptr(),
            message.len() as c_ulonglong,
            key.as_ptr(),
        )
    } != 0
    {
        return Outcome::Internal;
    }
    Outcome::Bytes(output)
}

/// Signs a message and returns the combined signed message.
fn sign(message: &[u8], secret: &[u8]) -> Outcome {
    if secret.len() != 64 {
        return Outcome::Internal;
    }
    let mut output = vec![0u8; message.len() + 64];
    let mut output_len = 0u64;
    if unsafe {
        crypto_sign(
            output.as_mut_ptr(),
            &mut output_len,
            message.as_ptr(),
            message.len() as c_ulonglong,
            secret.as_ptr(),
        )
    } != 0
    {
        return Outcome::Internal;
    }
    output.truncate(output_len as usize);
    Outcome::Bytes(output)
}

/// Opens a combined signed message.
fn sign_open(signed: &[u8], public: &[u8]) -> Outcome {
    if public.len() != 32 || signed.len() < 64 {
        return Outcome::False;
    }
    let mut output = vec![0u8; signed.len() - 64];
    let mut output_len = 0u64;
    if unsafe {
        crypto_sign_open(
            output.as_mut_ptr(),
            &mut output_len,
            signed.as_ptr(),
            signed.len() as c_ulonglong,
            public.as_ptr(),
        )
    } != 0
    {
        return Outcome::False;
    }
    output.truncate(output_len as usize);
    Outcome::Bytes(output)
}

/// Creates a detached Ed25519 signature.
fn sign_detached(message: &[u8], secret: &[u8]) -> Outcome {
    if secret.len() != 64 {
        return Outcome::Internal;
    }
    let mut output = vec![0u8; 64];
    let mut output_len = 0u64;
    if unsafe {
        crypto_sign_detached(
            output.as_mut_ptr(),
            &mut output_len,
            message.as_ptr(),
            message.len() as c_ulonglong,
            secret.as_ptr(),
        )
    } != 0
        || output_len != 64
    {
        return Outcome::Internal;
    }
    Outcome::Bytes(output)
}

/// Verifies a detached Ed25519 signature.
fn sign_verify_detached(signature: &[u8], message: &[u8], public: &[u8]) -> Outcome {
    if signature.len() != 64 || public.len() != 32 {
        return Outcome::Internal;
    }
    let result = unsafe {
        crypto_sign_verify_detached(
            signature.as_ptr(),
            message.as_ptr(),
            message.len() as c_ulonglong,
            public.as_ptr(),
        )
    };
    Outcome::Integer(i64::from(result == 0))
}

/// Derives an Ed25519 public key from a secret key.
fn sign_publickey_from_secretkey(secret: &[u8]) -> Outcome {
    if secret.len() != 64 {
        return Outcome::Internal;
    }
    let mut public = vec![0u8; 32];
    if unsafe { crypto_sign_ed25519_sk_to_pk(public.as_mut_ptr(), secret.as_ptr()) } != 0 {
        return Outcome::Internal;
    }
    Outcome::Bytes(public)
}

/// Converts an Ed25519 secret key to Curve25519.
fn ed25519_sk_to_curve25519(secret: &[u8]) -> Outcome {
    if secret.len() != 64 {
        return Outcome::Internal;
    }
    let mut output = vec![0u8; 32];
    if unsafe { crypto_sign_ed25519_sk_to_curve25519(output.as_mut_ptr(), secret.as_ptr()) } != 0 {
        return Outcome::ConversionFailed;
    }
    Outcome::Bytes(output)
}

/// Converts an Ed25519 public key to Curve25519.
fn ed25519_pk_to_curve25519(public: &[u8]) -> Outcome {
    if public.len() != 32 {
        return Outcome::Internal;
    }
    let mut output = vec![0u8; 32];
    if unsafe { crypto_sign_ed25519_pk_to_curve25519(output.as_mut_ptr(), public.as_ptr()) } != 0 {
        return Outcome::ConversionFailed;
    }
    Outcome::Bytes(output)
}

/// Generates an XSalsa20 keystream.
fn stream(nonce: &[u8], key: &[u8], length: i64) -> Outcome {
    if nonce.len() != 24 || key.len() != 32 || length <= 0 {
        return Outcome::Internal;
    }
    let mut output = vec![0u8; length as usize];
    if unsafe {
        crypto_stream(
            output.as_mut_ptr(),
            output.len() as c_ulonglong,
            nonce.as_ptr(),
            key.as_ptr(),
        )
    } != 0
    {
        return Outcome::Internal;
    }
    Outcome::Bytes(output)
}

/// XORs data with an XSalsa20 keystream.
fn stream_xor(message: &[u8], nonce: &[u8], key: &[u8]) -> Outcome {
    if nonce.len() != 24 || key.len() != 32 {
        return Outcome::Internal;
    }
    let mut output = vec![0u8; message.len()];
    if unsafe {
        crypto_stream_xor(
            output.as_mut_ptr(),
            message.as_ptr(),
            message.len() as c_ulonglong,
            nonce.as_ptr(),
            key.as_ptr(),
        )
    } != 0
    {
        return Outcome::Internal;
    }
    Outcome::Bytes(output)
}

/// Generates an XChaCha20 keystream.
fn stream_xchacha20(nonce: &[u8], key: &[u8], length: i64) -> Outcome {
    if nonce.len() != 24 || key.len() != 32 || length <= 0 {
        return Outcome::Internal;
    }
    let mut output = vec![0u8; length as usize];
    if unsafe {
        crypto_stream_xchacha20(
            output.as_mut_ptr(),
            output.len() as c_ulonglong,
            nonce.as_ptr(),
            key.as_ptr(),
        )
    } != 0
    {
        return Outcome::Internal;
    }
    Outcome::Bytes(output)
}

/// XORs data with an XChaCha20 keystream.
fn stream_xchacha20_xor(message: &[u8], nonce: &[u8], key: &[u8]) -> Outcome {
    if nonce.len() != 24 || key.len() != 32 {
        return Outcome::Internal;
    }
    let mut output = vec![0u8; message.len()];
    if unsafe {
        crypto_stream_xchacha20_xor(
            output.as_mut_ptr(),
            message.as_ptr(),
            message.len() as c_ulonglong,
            nonce.as_ptr(),
            key.as_ptr(),
        )
    } != 0
    {
        return Outcome::Internal;
    }
    Outcome::Bytes(output)
}

/// XORs data with an XChaCha20 keystream starting at `counter`.
fn stream_xchacha20_xor_ic(message: &[u8], nonce: &[u8], key: &[u8], counter: i64) -> Outcome {
    if nonce.len() != 24 || key.len() != 32 {
        return Outcome::Internal;
    }
    let mut output = vec![0u8; message.len()];
    if unsafe {
        crypto_stream_xchacha20_xor_ic(
            output.as_mut_ptr(),
            message.as_ptr(),
            message.len() as c_ulonglong,
            nonce.as_ptr(),
            counter as c_ulonglong,
            key.as_ptr(),
        )
    } != 0
    {
        return Outcome::Internal;
    }
    Outcome::Bytes(output)
}

/// Initializes a secretstream push state and returns `state || header`.
fn secretstream_init_push(key: &[u8]) -> Outcome {
    if key.len() != 32 {
        return Outcome::Internal;
    }
    let mut state = SecretstreamState([0u8; 52]);
    let mut header = vec![0u8; 24];
    if unsafe { crypto_secretstream_xchacha20poly1305_init_push(&mut state, header.as_mut_ptr(), key.as_ptr()) } != 0 {
        return Outcome::Internal;
    }
    let mut output = state.0.to_vec();
    output.extend_from_slice(&header);
    Outcome::Bytes(output)
}

/// Initializes a secretstream pull state from a header.
fn secretstream_init_pull(header: &[u8], key: &[u8]) -> Outcome {
    if header.len() != 24 || key.len() != 32 {
        return Outcome::Internal;
    }
    let mut state = SecretstreamState([0u8; 52]);
    if unsafe { crypto_secretstream_xchacha20poly1305_init_pull(&mut state, header.as_ptr(), key.as_ptr()) } != 0 {
        return Outcome::Internal;
    }
    Outcome::Bytes(state.0.to_vec())
}

/// Pushes one message and returns `new_state || ciphertext`.
fn secretstream_push(packed_state: &[u8], message: &[u8], additional: &[u8], tag: i64) -> Outcome {
    if packed_state.len() != 52 || !(0..=255).contains(&tag) {
        return Outcome::Internal;
    }
    let mut state = SecretstreamState([0u8; 52]);
    state.0.copy_from_slice(packed_state);
    let mut ciphertext = vec![0u8; message.len() + 17];
    let mut ciphertext_len = 0u64;
    if unsafe {
        crypto_secretstream_xchacha20poly1305_push(
            &mut state,
            ciphertext.as_mut_ptr(),
            &mut ciphertext_len,
            message.as_ptr(),
            message.len() as c_ulonglong,
            if additional.is_empty() { std::ptr::null() } else { additional.as_ptr() },
            additional.len() as c_ulonglong,
            tag as u8,
        )
    } != 0
    {
        return Outcome::Internal;
    }
    ciphertext.truncate(ciphertext_len as usize);
    let mut output = state.0.to_vec();
    output.extend_from_slice(&ciphertext);
    Outcome::Bytes(output)
}

/// Pulls one message and returns `new_state || plaintext || tag`.
fn secretstream_pull(packed_state: &[u8], ciphertext: &[u8], additional: &[u8]) -> Outcome {
    if packed_state.len() != 52 || ciphertext.len() < 17 {
        return Outcome::False;
    }
    let mut state = SecretstreamState([0u8; 52]);
    state.0.copy_from_slice(packed_state);
    let mut message = vec![0u8; ciphertext.len() - 17];
    let mut message_len = 0u64;
    let mut tag = 0u8;
    if unsafe {
        crypto_secretstream_xchacha20poly1305_pull(
            &mut state,
            message.as_mut_ptr(),
            &mut message_len,
            &mut tag,
            ciphertext.as_ptr(),
            ciphertext.len() as c_ulonglong,
            if additional.is_empty() { std::ptr::null() } else { additional.as_ptr() },
            additional.len() as c_ulonglong,
        )
    } != 0
    {
        return Outcome::False;
    }
    message.truncate(message_len as usize);
    let mut output = state.0.to_vec();
    output.extend_from_slice(&message);
    output.push(tag);
    Outcome::Bytes(output)
}

/// Rekeys a secretstream state.
fn secretstream_rekey(packed_state: &[u8]) -> Outcome {
    if packed_state.len() != 52 {
        return Outcome::Internal;
    }
    let mut state = SecretstreamState([0u8; 52]);
    state.0.copy_from_slice(packed_state);
    unsafe { crypto_secretstream_xchacha20poly1305_rekey(&mut state) };
    Outcome::Bytes(state.0.to_vec())
}

/// Adds two Ristretto255 points.
fn ristretto_add(first: &[u8], second: &[u8]) -> Outcome {
    if first.len() != 32 || second.len() != 32 {
        return Outcome::Internal;
    }
    let mut output = vec![0u8; 32];
    if unsafe { crypto_core_ristretto255_add(output.as_mut_ptr(), first.as_ptr(), second.as_ptr()) } != 0 {
        return Outcome::Internal;
    }
    Outcome::Bytes(output)
}

/// Subtracts two Ristretto255 points.
fn ristretto_sub(first: &[u8], second: &[u8]) -> Outcome {
    if first.len() != 32 || second.len() != 32 {
        return Outcome::Internal;
    }
    let mut output = vec![0u8; 32];
    if unsafe { crypto_core_ristretto255_sub(output.as_mut_ptr(), first.as_ptr(), second.as_ptr()) } != 0 {
        return Outcome::Internal;
    }
    Outcome::Bytes(output)
}

/// Maps a 64-byte hash to a Ristretto255 point.
fn ristretto_from_hash(input: &[u8]) -> Outcome {
    if input.len() != 64 {
        return Outcome::Internal;
    }
    let mut output = vec![0u8; 32];
    if unsafe { crypto_core_ristretto255_from_hash(output.as_mut_ptr(), input.as_ptr()) } != 0 {
        return Outcome::Internal;
    }
    Outcome::Bytes(output)
}

/// Reports whether a byte string is a valid Ristretto255 point.
fn ristretto_is_valid(input: &[u8]) -> Outcome {
    if input.len() != 32 {
        return Outcome::Internal;
    }
    Outcome::Integer(i64::from(unsafe { crypto_core_ristretto255_is_valid_point(input.as_ptr()) } != 0))
}

/// Ristretto255 scalar operations.
enum RistrettoScalarOp {
    Add,
    Sub,
    Mul,
    Complement,
    Negate,
    Reduce,
}

/// Applies one binary Ristretto255 scalar operation.
fn ristretto_scalar_binary(first: &[u8], second: &[u8], operation: RistrettoScalarOp) -> Outcome {
    if first.len() != 32 || second.len() != 32 {
        return Outcome::Internal;
    }
    let mut output = vec![0u8; 32];
    unsafe {
        match operation {
            RistrettoScalarOp::Add => crypto_core_ristretto255_scalar_add(output.as_mut_ptr(), first.as_ptr(), second.as_ptr()),
            RistrettoScalarOp::Sub => crypto_core_ristretto255_scalar_sub(output.as_mut_ptr(), first.as_ptr(), second.as_ptr()),
            RistrettoScalarOp::Mul => crypto_core_ristretto255_scalar_mul(output.as_mut_ptr(), first.as_ptr(), second.as_ptr()),
            RistrettoScalarOp::Complement | RistrettoScalarOp::Negate | RistrettoScalarOp::Reduce => {
                return Outcome::Internal;
            }
        }
    }
    Outcome::Bytes(output)
}

/// Applies one unary Ristretto255 scalar operation.
fn ristretto_scalar_unary(input: &[u8], operation: RistrettoScalarOp) -> Outcome {
    let expected = matches!(operation, RistrettoScalarOp::Reduce).then_some(64).unwrap_or(32);
    if input.len() != expected {
        return Outcome::Internal;
    }
    let mut output = vec![0u8; 32];
    unsafe {
        match operation {
            RistrettoScalarOp::Complement => crypto_core_ristretto255_scalar_complement(output.as_mut_ptr(), input.as_ptr()),
            RistrettoScalarOp::Negate => crypto_core_ristretto255_scalar_negate(output.as_mut_ptr(), input.as_ptr()),
            RistrettoScalarOp::Reduce => crypto_core_ristretto255_scalar_reduce(output.as_mut_ptr(), input.as_ptr()),
            RistrettoScalarOp::Add | RistrettoScalarOp::Sub | RistrettoScalarOp::Mul => {
                return Outcome::Internal;
            }
        }
    }
    Outcome::Bytes(output)
}

/// Inverts a Ristretto255 scalar.
fn ristretto_scalar_invert(input: &[u8]) -> Outcome {
    if input.len() != 32 {
        return Outcome::Internal;
    }
    let mut output = vec![0u8; 32];
    if unsafe { crypto_core_ristretto255_scalar_invert(output.as_mut_ptr(), input.as_ptr()) } != 0 {
        return Outcome::Internal;
    }
    Outcome::Bytes(output)
}

/// Multiplies a Ristretto255 point by a scalar.
fn scalarmult_ristretto(scalar: &[u8], point: &[u8]) -> Outcome {
    if scalar.len() != 32 || point.len() != 32 {
        return Outcome::Internal;
    }
    let mut output = vec![0u8; 32];
    if unsafe { crypto_scalarmult_ristretto255(output.as_mut_ptr(), scalar.as_ptr(), point.as_ptr()) } != 0 {
        return Outcome::IdentityElement;
    }
    Outcome::Bytes(output)
}

/// Multiplies the Ristretto255 base point by a scalar.
fn scalarmult_ristretto_base(scalar: &[u8]) -> Outcome {
    if scalar.len() != 32 {
        return Outcome::Internal;
    }
    let mut output = vec![0u8; 32];
    if unsafe { crypto_scalarmult_ristretto255_base(output.as_mut_ptr(), scalar.as_ptr()) } != 0 {
        return Outcome::ZeroScalar;
    }
    Outcome::Bytes(output)
}

/// Encrypts or decrypts one AEAD family.
fn aead(name: &str, a: [&[u8]; 4]) -> Outcome {
    let (message, additional, nonce, key) = (a[0], a[1], a[2], a[3]);
    let decrypt = name.ends_with("_decrypt");
    let encrypt = name.ends_with("_encrypt");
    if !decrypt && !encrypt {
        return Outcome::Internal;
    }
    let (abytes, nonce_len, key_len) = match name {
        n if n.contains("aegis128l") => (32, 16, 16),
        n if n.contains("aegis256") => (32, 32, 32),
        n if n.contains("aes256gcm") => (16, 12, 32),
        n if n.contains("xchacha20poly1305") => (16, 24, 32),
        n if n.contains("chacha20poly1305_ietf") => (16, 12, 32),
        n if n.contains("chacha20poly1305") => (16, 8, 32),
        _ => return Outcome::Internal,
    };
    if nonce.len() != nonce_len || key.len() != key_len {
        return Outcome::Internal;
    }
    let mut output = vec![0u8; if decrypt {
        match message.len().checked_sub(abytes) {
            Some(length) => length,
            None => return Outcome::False,
        }
    } else {
        message.len() + abytes
    }];
    let mut output_len = 0u64;
    let result = unsafe {
        if decrypt {
            aead_decrypt(name, output.as_mut_ptr(), &mut output_len, message, additional, nonce, key)
        } else {
            aead_encrypt(name, output.as_mut_ptr(), &mut output_len, message, additional, nonce, key)
        }
    };
    if result != 0 {
        return if decrypt { Outcome::False } else { Outcome::Internal };
    }
    output.truncate(output_len as usize);
    Outcome::Bytes(output)
}

/// Calls the matching libsodium AEAD encryption entry point.
unsafe fn aead_encrypt(
    name: &str,
    output: *mut u8,
    output_len: &mut u64,
    message: &[u8],
    additional: &[u8],
    nonce: &[u8],
    key: &[u8],
) -> c_int {
    let additional_ptr = if additional.is_empty() { std::ptr::null() } else { additional.as_ptr() };
    let additional_len = additional.len() as c_ulonglong;
    let message_len = message.len() as c_ulonglong;
    let length = output_len as *mut c_ulonglong;
    match name {
        "sodium_crypto_aead_aegis128l_encrypt" => crypto_aead_aegis128l_encrypt(output, length, message.as_ptr(), message_len, additional_ptr, additional_len, std::ptr::null(), nonce.as_ptr(), key.as_ptr()),
        "sodium_crypto_aead_aegis256_encrypt" => crypto_aead_aegis256_encrypt(output, length, message.as_ptr(), message_len, additional_ptr, additional_len, std::ptr::null(), nonce.as_ptr(), key.as_ptr()),
        "sodium_crypto_aead_aes256gcm_encrypt" => crypto_aead_aes256gcm_encrypt(output, length, message.as_ptr(), message_len, additional_ptr, additional_len, std::ptr::null(), nonce.as_ptr(), key.as_ptr()),
        "sodium_crypto_aead_chacha20poly1305_encrypt" => crypto_aead_chacha20poly1305_encrypt(output, length, message.as_ptr(), message_len, additional_ptr, additional_len, std::ptr::null(), nonce.as_ptr(), key.as_ptr()),
        "sodium_crypto_aead_chacha20poly1305_ietf_encrypt" => crypto_aead_chacha20poly1305_ietf_encrypt(output, length, message.as_ptr(), message_len, additional_ptr, additional_len, std::ptr::null(), nonce.as_ptr(), key.as_ptr()),
        "sodium_crypto_aead_xchacha20poly1305_ietf_encrypt" => crypto_aead_xchacha20poly1305_ietf_encrypt(output, length, message.as_ptr(), message_len, additional_ptr, additional_len, std::ptr::null(), nonce.as_ptr(), key.as_ptr()),
        _ => -1,
    }
}

/// Calls the matching libsodium AEAD decryption entry point.
unsafe fn aead_decrypt(
    name: &str,
    output: *mut u8,
    output_len: &mut u64,
    ciphertext: &[u8],
    additional: &[u8],
    nonce: &[u8],
    key: &[u8],
) -> c_int {
    let additional_ptr = if additional.is_empty() { std::ptr::null() } else { additional.as_ptr() };
    let additional_len = additional.len() as c_ulonglong;
    let ciphertext_len = ciphertext.len() as c_ulonglong;
    let length = output_len as *mut c_ulonglong;
    match name {
        "sodium_crypto_aead_aegis128l_decrypt" => crypto_aead_aegis128l_decrypt(output, length, std::ptr::null(), ciphertext.as_ptr(), ciphertext_len, additional_ptr, additional_len, nonce.as_ptr(), key.as_ptr()),
        "sodium_crypto_aead_aegis256_decrypt" => crypto_aead_aegis256_decrypt(output, length, std::ptr::null(), ciphertext.as_ptr(), ciphertext_len, additional_ptr, additional_len, nonce.as_ptr(), key.as_ptr()),
        "sodium_crypto_aead_aes256gcm_decrypt" => crypto_aead_aes256gcm_decrypt(output, length, std::ptr::null(), ciphertext.as_ptr(), ciphertext_len, additional_ptr, additional_len, nonce.as_ptr(), key.as_ptr()),
        "sodium_crypto_aead_chacha20poly1305_decrypt" => crypto_aead_chacha20poly1305_decrypt(output, length, std::ptr::null(), ciphertext.as_ptr(), ciphertext_len, additional_ptr, additional_len, nonce.as_ptr(), key.as_ptr()),
        "sodium_crypto_aead_chacha20poly1305_ietf_decrypt" => crypto_aead_chacha20poly1305_ietf_decrypt(output, length, std::ptr::null(), ciphertext.as_ptr(), ciphertext_len, additional_ptr, additional_len, nonce.as_ptr(), key.as_ptr()),
        "sodium_crypto_aead_xchacha20poly1305_ietf_decrypt" => crypto_aead_xchacha20poly1305_ietf_decrypt(output, length, std::ptr::null(), ciphertext.as_ptr(), ciphertext_len, additional_ptr, additional_len, nonce.as_ptr(), key.as_ptr()),
        _ => -1,
    }
}

#[cfg(all(test, elephc_sodium_native))]
mod native_tests {
    use super::*;

    /// Verifies the incremental BLAKE2b state survives update/final through the same
    /// bridge representation the compiled prelude uses.
    /// Verifies a two-message secretstream round trip through the raw state bytes.
    #[test]
    fn secretstream_pull_twice_uses_the_advanced_state() {
        let key = [14u8; 32];
        let Outcome::Bytes(init) = secretstream_init_push(&key) else { panic!("init") };
        let (state, header) = init.split_at(52);
        let Outcome::Bytes(first) = secretstream_push(state, b"first", b"", 0) else { panic!("push1") };
        let (state1, cipher1) = first.split_at(52);
        let Outcome::Bytes(second) = secretstream_push(state1, b"second", b"", 3) else { panic!("push2") };
        let (_, cipher2) = second.split_at(52);
        let Outcome::Bytes(pull_state) = secretstream_init_pull(header, &key) else { panic!("pull init") };
        let Outcome::Bytes(pulled1) = secretstream_pull(&pull_state, cipher1, b"") else { panic!("pull1") };
        let (pull_state1, payload1) = pulled1.split_at(52);
        assert_eq!(&payload1[..payload1.len() - 1], b"first");
        let Outcome::Bytes(pulled2) = secretstream_pull(pull_state1, cipher2, b"") else { panic!("pull2") };
        let (_, payload2) = pulled2.split_at(52);
        assert_eq!(&payload2[..payload2.len() - 1], b"second");
    }

    /// Verifies the Argon2id entry point accepts PHP's interactive limits.
    #[test]
    fn pwhash_argon2id_produces_a_key() {
        let salt = [8u8; 16];
        match pwhash(b"password", &salt, 32, 2, 67_108_864, 2) {
            Outcome::Bytes(bytes) => assert_eq!(bytes.len(), 32),
            _ => panic!("pwhash failed"),
        }
    }

    /// Verifies PHP's bcrypt provider formats `$2y$` hashes and verifies them.
    #[test]
    fn password_bcrypt_round_trip() {
        let Outcome::Bytes(hash) = password_hash_bcrypt(b"secret", 4) else {
            panic!("bcrypt hash failed")
        };
        let hash = std::str::from_utf8(&hash).expect("bcrypt ASCII hash");
        assert!(hash.starts_with("$2y$04$"), "{hash}");
        assert_eq!(
            matches!(password_verify_bcrypt(b"secret", hash.as_bytes()), Outcome::Integer(1)),
            true
        );
        assert_eq!(
            matches!(password_verify_bcrypt(b"wrong", hash.as_bytes()), Outcome::Integer(0)),
            true
        );
    }

    #[test]
    fn generichash_incremental_matches_one_shot() {
        let key = [3u8; 32];
        let one_shot = match generichash(b"attack at dawn", &key, 32) {
            Outcome::Bytes(bytes) => bytes,
            _ => panic!("one-shot failed"),
        };
        let state = match generichash_init(&key, 32) {
            Outcome::Bytes(bytes) => bytes,
            _ => panic!("init failed"),
        };
        let state = match generichash_update(&state, b"attack ") {
            Outcome::Bytes(bytes) => bytes,
            _ => panic!("update failed"),
        };
        let state = match generichash_update(&state, b"at dawn") {
            Outcome::Bytes(bytes) => bytes,
            _ => panic!("update failed"),
        };
        let final_hash = match generichash_final(&state, 32) {
            Outcome::Bytes(bytes) => bytes,
            _ => panic!("final failed"),
        };
        assert_eq!(final_hash, one_shot);
    }
}
