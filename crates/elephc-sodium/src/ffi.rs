//! Purpose:
//! Declares the subset of libsodium 1.0.22's C API the elephc sodium bridge calls.
//!
//! Called from:
//! - `crate::ops` for every cryptographic operation.
//!
//! Key details:
//! - The declarations mirror `sodium.h` for the LP64 targets elephc supports.
//! - No symbol is linked here: the bridge is a staticlib whose `crypto_*`/`sodium_*`
//!   references are resolved by the managed `libsodium` package at final link time.
//! - Opaque state structs use the byte sizes libsodium exposes in PHP (`384` for
//!   BLAKE2b, `52` for XChaCha20-Poly1305 secretstream). The BLAKE2b state is 64-byte
//!   aligned exactly like `crypto_generichash_blake2b_state`.

#![allow(non_camel_case_types)]

use std::os::raw::{c_char, c_int, c_uchar, c_ulonglong};

/// `crypto_generichash_state`, an opaque 384-byte, 64-byte-aligned BLAKE2b state.
#[repr(C, align(64))]
pub struct GenerichashState(pub [u8; 384]);

/// `crypto_secretstream_xchacha20poly1305_state`, a packed 52-byte state.
#[repr(C)]
pub struct SecretstreamState(pub [u8; 52]);

extern "C" {
    /// Initializes libsodium once; returns 0 on success.
    pub fn sodium_init() -> c_int;
    /// Returns the linked libsodium version string.
    pub fn sodium_version_string() -> *const c_char;
    /// Returns the linked libsodium library major version.
    pub fn sodium_library_version_major() -> c_int;
    /// Returns the linked libsodium library minor version.
    pub fn sodium_library_version_minor() -> c_int;
    /// Fills `size` bytes with cryptographically secure random data.
    pub fn randombytes_buf(buffer: *mut c_uchar, size: usize);
    /// Constant-time equality comparison over equal-length inputs.
    pub fn sodium_memcmp(first: *const c_uchar, second: *const c_uchar, len: usize) -> c_int;
    /// Lexicographic comparison over equal-length inputs.
    pub fn sodium_compare(first: *const c_uchar, second: *const c_uchar, len: usize) -> c_int;
    /// Adds two little-endian integers in place.
    pub fn sodium_add(first: *mut c_uchar, second: *const c_uchar, len: usize);
    /// Increments a little-endian integer in place.
    pub fn sodium_increment(value: *mut c_uchar, len: usize);
    /// Pads a buffer as libsodium's ISO/IEC 7816-4 scheme requires.
    pub fn sodium_pad(
        padded_len: *mut usize,
        buffer: *mut c_uchar,
        unpadded_len: usize,
        block_size: usize,
        max_buffer_len: usize,
    ) -> c_int;
    /// Removes libsodium's ISO/IEC 7816-4 padding.
    pub fn sodium_unpad(
        unpadded_len: *mut usize,
        buffer: *const c_uchar,
        padded_len: usize,
        block_size: usize,
    ) -> c_int;
    /// Decodes hexadecimal, optionally skipping `ignore` characters.
    pub fn sodium_hex2bin(
        binary: *mut c_uchar,
        binary_max_len: usize,
        hex: *const c_char,
        hex_len: usize,
        ignore: *const c_char,
        binary_len: *mut usize,
        hex_end: *mut *const c_char,
    ) -> c_int;
    /// Encodes binary data as one base64 variant.
    pub fn sodium_bin2base64(
        base64: *mut c_char,
        base64_max_len: usize,
        binary: *const c_uchar,
        binary_len: usize,
        variant: c_int,
    ) -> *mut c_char;
    /// Decodes one base64 variant, optionally skipping `ignore` characters.
    pub fn sodium_base642bin(
        binary: *mut c_uchar,
        binary_max_len: usize,
        base64: *const c_char,
        base64_len: usize,
        ignore: *const c_char,
        binary_len: *mut usize,
        base64_end: *mut *const c_char,
        variant: c_int,
    ) -> c_int;

    /// Computes a SipHash-2-4 short hash.
    pub fn crypto_shorthash(out: *mut c_uchar, input: *const c_uchar, len: c_ulonglong, key: *const c_uchar) -> c_int;
    /// Encrypts and authenticates with XSalsa20-Poly1305.
    pub fn crypto_secretbox_easy(ciphertext: *mut c_uchar, message: *const c_uchar, message_len: c_ulonglong, nonce: *const c_uchar, key: *const c_uchar) -> c_int;
    /// Decrypts and verifies an XSalsa20-Poly1305 ciphertext.
    pub fn crypto_secretbox_open_easy(message: *mut c_uchar, ciphertext: *const c_uchar, ciphertext_len: c_ulonglong, nonce: *const c_uchar, key: *const c_uchar) -> c_int;

    /// Computes a one-shot BLAKE2b hash.
    pub fn crypto_generichash(out: *mut c_uchar, out_len: usize, input: *const c_uchar, input_len: c_ulonglong, key: *const c_uchar, key_len: usize) -> c_int;
    /// Initializes an incremental BLAKE2b state.
    pub fn crypto_generichash_init(state: *mut GenerichashState, key: *const c_uchar, key_len: usize, out_len: usize) -> c_int;
    /// Updates an incremental BLAKE2b state.
    pub fn crypto_generichash_update(state: *mut GenerichashState, input: *const c_uchar, input_len: c_ulonglong) -> c_int;
    /// Finalizes an incremental BLAKE2b state.
    pub fn crypto_generichash_final(state: *mut GenerichashState, out: *mut c_uchar, out_len: usize) -> c_int;

    /// Generates a Curve25519 key pair.
    pub fn crypto_box_keypair(public_key: *mut c_uchar, secret_key: *mut c_uchar) -> c_int;
    /// Derives a Curve25519 key pair from a seed.
    pub fn crypto_box_seed_keypair(public_key: *mut c_uchar, secret_key: *mut c_uchar, seed: *const c_uchar) -> c_int;
    /// Encrypts with a Curve25519-XSalsa20-Poly1305 box.
    pub fn crypto_box_easy(ciphertext: *mut c_uchar, message: *const c_uchar, message_len: c_ulonglong, nonce: *const c_uchar, public_key: *const c_uchar, secret_key: *const c_uchar) -> c_int;
    /// Decrypts a Curve25519-XSalsa20-Poly1305 box.
    pub fn crypto_box_open_easy(message: *mut c_uchar, ciphertext: *const c_uchar, ciphertext_len: c_ulonglong, nonce: *const c_uchar, public_key: *const c_uchar, secret_key: *const c_uchar) -> c_int;
    /// Encrypts an anonymous sealed box.
    pub fn crypto_box_seal(ciphertext: *mut c_uchar, message: *const c_uchar, message_len: c_ulonglong, public_key: *const c_uchar) -> c_int;
    /// Decrypts an anonymous sealed box.
    pub fn crypto_box_seal_open(message: *mut c_uchar, ciphertext: *const c_uchar, ciphertext_len: c_ulonglong, public_key: *const c_uchar, secret_key: *const c_uchar) -> c_int;

    /// Generates an Ed25519 key pair.
    pub fn crypto_sign_keypair(public_key: *mut c_uchar, secret_key: *mut c_uchar) -> c_int;
    /// Derives an Ed25519 key pair from a seed.
    pub fn crypto_sign_seed_keypair(public_key: *mut c_uchar, secret_key: *mut c_uchar, seed: *const c_uchar) -> c_int;
    /// Signs a message and returns the combined signed message.
    pub fn crypto_sign(signed_message: *mut c_uchar, signed_len: *mut c_ulonglong, message: *const c_uchar, message_len: c_ulonglong, secret_key: *const c_uchar) -> c_int;
    /// Opens a combined signed message.
    pub fn crypto_sign_open(message: *mut c_uchar, message_len: *mut c_ulonglong, signed_message: *const c_uchar, signed_len: c_ulonglong, public_key: *const c_uchar) -> c_int;
    /// Creates a detached Ed25519 signature.
    pub fn crypto_sign_detached(signature: *mut c_uchar, signature_len: *mut c_ulonglong, message: *const c_uchar, message_len: c_ulonglong, secret_key: *const c_uchar) -> c_int;
    /// Verifies a detached Ed25519 signature.
    pub fn crypto_sign_verify_detached(signature: *const c_uchar, message: *const c_uchar, message_len: c_ulonglong, public_key: *const c_uchar) -> c_int;
    /// Derives an Ed25519 public key from a secret key.
    pub fn crypto_sign_ed25519_sk_to_pk(public_key: *mut c_uchar, secret_key: *const c_uchar) -> c_int;
    /// Converts an Ed25519 secret key to a Curve25519 secret key.
    pub fn crypto_sign_ed25519_sk_to_curve25519(curve_secret: *mut c_uchar, ed_secret: *const c_uchar) -> c_int;
    /// Converts an Ed25519 public key to a Curve25519 public key.
    pub fn crypto_sign_ed25519_pk_to_curve25519(curve_public: *mut c_uchar, ed_public: *const c_uchar) -> c_int;

    /// Generates an XSalsa20 keystream.
    pub fn crypto_stream(out: *mut c_uchar, out_len: c_ulonglong, nonce: *const c_uchar, key: *const c_uchar) -> c_int;
    /// XORs data with an XSalsa20 keystream.
    pub fn crypto_stream_xor(ciphertext: *mut c_uchar, message: *const c_uchar, message_len: c_ulonglong, nonce: *const c_uchar, key: *const c_uchar) -> c_int;
    /// Generates an XChaCha20 keystream.
    pub fn crypto_stream_xchacha20(out: *mut c_uchar, out_len: c_ulonglong, nonce: *const c_uchar, key: *const c_uchar) -> c_int;
    /// XORs data with an XChaCha20 keystream.
    pub fn crypto_stream_xchacha20_xor(ciphertext: *mut c_uchar, message: *const c_uchar, message_len: c_ulonglong, nonce: *const c_uchar, key: *const c_uchar) -> c_int;
    /// XORs data with an XChaCha20 keystream starting at `counter`.
    pub fn crypto_stream_xchacha20_xor_ic(ciphertext: *mut c_uchar, message: *const c_uchar, message_len: c_ulonglong, nonce: *const c_uchar, counter: c_ulonglong, key: *const c_uchar) -> c_int;

    /// Computes a Poly1305 authentication tag.
    pub fn crypto_auth(out: *mut c_uchar, input: *const c_uchar, input_len: c_ulonglong, key: *const c_uchar) -> c_int;
    /// Verifies a Poly1305 authentication tag.
    pub fn crypto_auth_verify(tag: *const c_uchar, input: *const c_uchar, input_len: c_ulonglong, key: *const c_uchar) -> c_int;

    /// Derives a Key Derivation Function subkey.
    pub fn crypto_kdf_derive_from_key(subkey: *mut c_uchar, subkey_len: usize, subkey_id: c_ulonglong, context: *const c_char, key: *const c_uchar) -> c_int;

    /// Generates a key-exchange key pair.
    pub fn crypto_kx_keypair(public_key: *mut c_uchar, secret_key: *mut c_uchar) -> c_int;
    /// Derives a key-exchange key pair from a seed.
    pub fn crypto_kx_seed_keypair(public_key: *mut c_uchar, secret_key: *mut c_uchar, seed: *const c_uchar) -> c_int;
    /// Derives the client's receive/transmit session keys.
    pub fn crypto_kx_client_session_keys(receive: *mut c_uchar, transmit: *mut c_uchar, client_public: *const c_uchar, client_secret: *const c_uchar, server_public: *const c_uchar) -> c_int;
    /// Derives the server's receive/transmit session keys.
    pub fn crypto_kx_server_session_keys(receive: *mut c_uchar, transmit: *mut c_uchar, server_public: *const c_uchar, server_secret: *const c_uchar, client_public: *const c_uchar) -> c_int;

    /// Multiplies a Curve25519 point by a scalar.
    pub fn crypto_scalarmult(out: *mut c_uchar, scalar: *const c_uchar, point: *const c_uchar) -> c_int;
    /// Computes a Curve25519 public key from a secret key.
    pub fn crypto_scalarmult_base(public_key: *mut c_uchar, secret_key: *const c_uchar) -> c_int;

    /// Derives a key from a password with Argon2.
    pub fn crypto_pwhash(
        out: *mut c_uchar,
        out_len: c_ulonglong,
        password: *const c_char,
        password_len: c_ulonglong,
        salt: *const c_uchar,
        ops_limit: c_ulonglong,
        mem_limit: usize,
        algorithm: c_int,
    ) -> c_int;
    /// Hashes a password with the selected Argon2 algorithm in libsodium's string format.
    pub fn crypto_pwhash_str_alg(out: *mut c_char, password: *const c_char, password_len: c_ulonglong, ops_limit: c_ulonglong, mem_limit: usize, algorithm: c_int) -> c_int;
    /// Hashes a password with Argon2id in libsodium's string format.
    pub fn crypto_pwhash_str(out: *mut c_char, password: *const c_char, password_len: c_ulonglong, ops_limit: c_ulonglong, mem_limit: usize) -> c_int;
    /// Verifies an Argon2 password hash.
    pub fn crypto_pwhash_str_verify(hash: *const c_char, password: *const c_char, password_len: c_ulonglong) -> c_int;
    /// Reports whether an Argon2 password hash needs rehashing.
    pub fn crypto_pwhash_str_needs_rehash(hash: *const c_char, ops_limit: c_ulonglong, mem_limit: usize) -> c_int;
    /// Derives a key from a password with scrypt.
    pub fn crypto_pwhash_scryptsalsa208sha256(
        out: *mut c_uchar,
        out_len: c_ulonglong,
        password: *const c_char,
        password_len: c_ulonglong,
        salt: *const c_uchar,
        ops_limit: c_ulonglong,
        mem_limit: usize,
    ) -> c_int;
    /// Hashes a password with scrypt in libsodium's string format.
    pub fn crypto_pwhash_scryptsalsa208sha256_str(out: *mut c_char, password: *const c_char, password_len: c_ulonglong, ops_limit: c_ulonglong, mem_limit: usize) -> c_int;
    /// Verifies a scrypt password hash.
    pub fn crypto_pwhash_scryptsalsa208sha256_str_verify(hash: *const c_char, password: *const c_char, password_len: c_ulonglong) -> c_int;

    /// Reports whether the CPU supports AES-256-GCM.
    pub fn crypto_aead_aes256gcm_is_available() -> c_int;
    /// Encrypts with AES-256-GCM.
    pub fn crypto_aead_aes256gcm_encrypt(ciphertext: *mut c_uchar, ciphertext_len: *mut c_ulonglong, message: *const c_uchar, message_len: c_ulonglong, additional_data: *const c_uchar, additional_len: c_ulonglong, secret_nonce: *const c_uchar, public_nonce: *const c_uchar, key: *const c_uchar) -> c_int;
    /// Decrypts with AES-256-GCM.
    pub fn crypto_aead_aes256gcm_decrypt(message: *mut c_uchar, message_len: *mut c_ulonglong, secret_nonce: *const c_uchar, ciphertext: *const c_uchar, ciphertext_len: c_ulonglong, additional_data: *const c_uchar, additional_len: c_ulonglong, public_nonce: *const c_uchar, key: *const c_uchar) -> c_int;
    /// Encrypts with AEGIS-128L.
    pub fn crypto_aead_aegis128l_encrypt(ciphertext: *mut c_uchar, ciphertext_len: *mut c_ulonglong, message: *const c_uchar, message_len: c_ulonglong, additional_data: *const c_uchar, additional_len: c_ulonglong, secret_nonce: *const c_uchar, public_nonce: *const c_uchar, key: *const c_uchar) -> c_int;
    /// Decrypts with AEGIS-128L.
    pub fn crypto_aead_aegis128l_decrypt(message: *mut c_uchar, message_len: *mut c_ulonglong, secret_nonce: *const c_uchar, ciphertext: *const c_uchar, ciphertext_len: c_ulonglong, additional_data: *const c_uchar, additional_len: c_ulonglong, public_nonce: *const c_uchar, key: *const c_uchar) -> c_int;
    /// Encrypts with AEGIS-256.
    pub fn crypto_aead_aegis256_encrypt(ciphertext: *mut c_uchar, ciphertext_len: *mut c_ulonglong, message: *const c_uchar, message_len: c_ulonglong, additional_data: *const c_uchar, additional_len: c_ulonglong, secret_nonce: *const c_uchar, public_nonce: *const c_uchar, key: *const c_uchar) -> c_int;
    /// Decrypts with AEGIS-256.
    pub fn crypto_aead_aegis256_decrypt(message: *mut c_uchar, message_len: *mut c_ulonglong, secret_nonce: *const c_uchar, ciphertext: *const c_uchar, ciphertext_len: c_ulonglong, additional_data: *const c_uchar, additional_len: c_ulonglong, public_nonce: *const c_uchar, key: *const c_uchar) -> c_int;
    /// Encrypts with ChaCha20-Poly1305.
    pub fn crypto_aead_chacha20poly1305_encrypt(ciphertext: *mut c_uchar, ciphertext_len: *mut c_ulonglong, message: *const c_uchar, message_len: c_ulonglong, additional_data: *const c_uchar, additional_len: c_ulonglong, secret_nonce: *const c_uchar, public_nonce: *const c_uchar, key: *const c_uchar) -> c_int;
    /// Decrypts with ChaCha20-Poly1305.
    pub fn crypto_aead_chacha20poly1305_decrypt(message: *mut c_uchar, message_len: *mut c_ulonglong, secret_nonce: *const c_uchar, ciphertext: *const c_uchar, ciphertext_len: c_ulonglong, additional_data: *const c_uchar, additional_len: c_ulonglong, public_nonce: *const c_uchar, key: *const c_uchar) -> c_int;
    /// Encrypts with the IETF ChaCha20-Poly1305 construction.
    pub fn crypto_aead_chacha20poly1305_ietf_encrypt(ciphertext: *mut c_uchar, ciphertext_len: *mut c_ulonglong, message: *const c_uchar, message_len: c_ulonglong, additional_data: *const c_uchar, additional_len: c_ulonglong, secret_nonce: *const c_uchar, public_nonce: *const c_uchar, key: *const c_uchar) -> c_int;
    /// Decrypts with the IETF ChaCha20-Poly1305 construction.
    pub fn crypto_aead_chacha20poly1305_ietf_decrypt(message: *mut c_uchar, message_len: *mut c_ulonglong, secret_nonce: *const c_uchar, ciphertext: *const c_uchar, ciphertext_len: c_ulonglong, additional_data: *const c_uchar, additional_len: c_ulonglong, public_nonce: *const c_uchar, key: *const c_uchar) -> c_int;
    /// Encrypts with XChaCha20-Poly1305.
    pub fn crypto_aead_xchacha20poly1305_ietf_encrypt(ciphertext: *mut c_uchar, ciphertext_len: *mut c_ulonglong, message: *const c_uchar, message_len: c_ulonglong, additional_data: *const c_uchar, additional_len: c_ulonglong, secret_nonce: *const c_uchar, public_nonce: *const c_uchar, key: *const c_uchar) -> c_int;
    /// Decrypts with XChaCha20-Poly1305.
    pub fn crypto_aead_xchacha20poly1305_ietf_decrypt(message: *mut c_uchar, message_len: *mut c_ulonglong, secret_nonce: *const c_uchar, ciphertext: *const c_uchar, ciphertext_len: c_ulonglong, additional_data: *const c_uchar, additional_len: c_ulonglong, public_nonce: *const c_uchar, key: *const c_uchar) -> c_int;

    /// Initializes a secretstream push state and writes its header.
    pub fn crypto_secretstream_xchacha20poly1305_init_push(state: *mut SecretstreamState, header: *mut c_uchar, key: *const c_uchar) -> c_int;
    /// Pushes one message into a secretstream.
    pub fn crypto_secretstream_xchacha20poly1305_push(state: *mut SecretstreamState, ciphertext: *mut c_uchar, ciphertext_len: *mut c_ulonglong, message: *const c_uchar, message_len: c_ulonglong, additional_data: *const c_uchar, additional_len: c_ulonglong, tag: c_uchar) -> c_int;
    /// Initializes a secretstream pull state from a header.
    pub fn crypto_secretstream_xchacha20poly1305_init_pull(state: *mut SecretstreamState, header: *const c_uchar, key: *const c_uchar) -> c_int;
    /// Pulls one message from a secretstream.
    pub fn crypto_secretstream_xchacha20poly1305_pull(state: *mut SecretstreamState, message: *mut c_uchar, message_len: *mut c_ulonglong, tag: *mut c_uchar, ciphertext: *const c_uchar, ciphertext_len: c_ulonglong, additional_data: *const c_uchar, additional_len: c_ulonglong) -> c_int;
    /// Rekeys a secretstream state.
    pub fn crypto_secretstream_xchacha20poly1305_rekey(state: *mut SecretstreamState);

    /// Adds two Ristretto255 points.
    pub fn crypto_core_ristretto255_add(out: *mut c_uchar, first: *const c_uchar, second: *const c_uchar) -> c_int;
    /// Subtracts two Ristretto255 points.
    pub fn crypto_core_ristretto255_sub(out: *mut c_uchar, first: *const c_uchar, second: *const c_uchar) -> c_int;
    /// Maps a 64-byte hash to a Ristretto255 point.
    pub fn crypto_core_ristretto255_from_hash(out: *mut c_uchar, hash: *const c_uchar) -> c_int;
    /// Reports whether a byte string is a valid Ristretto255 point.
    pub fn crypto_core_ristretto255_is_valid_point(point: *const c_uchar) -> c_int;
    /// Generates a random Ristretto255 point.
    pub fn crypto_core_ristretto255_random(out: *mut c_uchar);
    /// Adds two Ristretto255 scalars.
    pub fn crypto_core_ristretto255_scalar_add(out: *mut c_uchar, first: *const c_uchar, second: *const c_uchar);
    /// Computes the Ristretto255 scalar complement.
    pub fn crypto_core_ristretto255_scalar_complement(out: *mut c_uchar, scalar: *const c_uchar);
    /// Computes the Ristretto255 scalar inverse.
    pub fn crypto_core_ristretto255_scalar_invert(out: *mut c_uchar, scalar: *const c_uchar) -> c_int;
    /// Multiplies two Ristretto255 scalars.
    pub fn crypto_core_ristretto255_scalar_mul(out: *mut c_uchar, first: *const c_uchar, second: *const c_uchar);
    /// Negates a Ristretto255 scalar.
    pub fn crypto_core_ristretto255_scalar_negate(out: *mut c_uchar, scalar: *const c_uchar);
    /// Generates a random Ristretto255 scalar.
    pub fn crypto_core_ristretto255_scalar_random(out: *mut c_uchar);
    /// Reduces a 64-byte value to a Ristretto255 scalar.
    pub fn crypto_core_ristretto255_scalar_reduce(out: *mut c_uchar, scalar: *const c_uchar);
    /// Subtracts two Ristretto255 scalars.
    pub fn crypto_core_ristretto255_scalar_sub(out: *mut c_uchar, first: *const c_uchar, second: *const c_uchar);
    /// Multiplies a Ristretto255 point by a scalar.
    pub fn crypto_scalarmult_ristretto255(out: *mut c_uchar, scalar: *const c_uchar, point: *const c_uchar) -> c_int;
    /// Multiplies the Ristretto255 base point by a scalar.
    pub fn crypto_scalarmult_ristretto255_base(out: *mut c_uchar, scalar: *const c_uchar) -> c_int;
}
