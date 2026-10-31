//! Purpose:
//! Integration tests for the libcrypto-backed OpenSSL cipher catalog and EVP ABI.
//!
//! Called from:
//! - `cargo test -p elephc-openssl` through Rust's test harness.
//!
//! Key details:
//! - Native tests run only when `ELEPHC_OPENSSL_LIB_DIR` points at a pinned
//!   OpenSSL 3.6.4 `lib/` so a plain checkout still builds and tests the ABI.
//! - `php_ciphers.txt` is the exact PHP 8.5/OpenSSL 3.6.4 default-provider list.

#![cfg(elephc_openssl_native)]

use std::ptr;

use elephc_openssl::*;

/// Returns the checked-in PHP cipher list.
fn php_ciphers() -> Vec<&'static str> {
    include_str!("php_ciphers.txt")
        .lines()
        .filter(|line| !line.is_empty())
        .collect()
}

/// Invokes the encrypt ABI with caller-owned buffers.
fn encrypt_raw(
    cipher: &str,
    data: &[u8],
    key: &[u8],
    iv: &[u8],
    options: u32,
    aad: &[u8],
    tag_len: usize,
) -> Result<(Vec<u8>, Vec<u8>), i32> {
    let mut output = vec![0u8; data.len() + 64];
    let mut tag = vec![0u8; 16];
    let mut output_len = 0usize;
    let mut tag_output_len = 0usize;
    let status = unsafe {
        elephc_openssl_encrypt(
            cipher.as_ptr(),
            cipher.len(),
            data.as_ptr(),
            data.len(),
            key.as_ptr(),
            key.len(),
            iv.as_ptr(),
            iv.len(),
            options,
            aad.as_ptr(),
            aad.len(),
            tag_len,
            output.as_mut_ptr(),
            output.len(),
            &mut output_len,
            tag.as_mut_ptr(),
            tag.len(),
            &mut tag_output_len,
        )
    };
    if status != CIPHER_OK {
        return Err(status);
    }
    output.truncate(output_len);
    tag.truncate(tag_output_len);
    Ok((output, tag))
}

/// Invokes the decrypt ABI with a caller-owned plaintext buffer.
fn decrypt_raw(
    cipher: &str,
    data: &[u8],
    key: &[u8],
    iv: &[u8],
    options: u32,
    aad: &[u8],
    tag: &[u8],
) -> Result<Vec<u8>, i32> {
    let mut output = vec![0u8; data.len() + 64];
    let mut output_len = 0usize;
    let status = unsafe {
        elephc_openssl_decrypt(
            cipher.as_ptr(),
            cipher.len(),
            data.as_ptr(),
            data.len(),
            key.as_ptr(),
            key.len(),
            iv.as_ptr(),
            iv.len(),
            options,
            aad.as_ptr(),
            aad.len(),
            tag.as_ptr(),
            tag.len(),
            output.as_mut_ptr(),
            output.len(),
            &mut output_len,
        )
    };
    if status != CIPHER_OK {
        return Err(status);
    }
    output.truncate(output_len);
    Ok(output)
}

/// Checks the stable public status codes without touching libcrypto.
#[test]
fn status_codes_are_stable() {
    assert_eq!(CIPHER_OK, 0);
    assert_eq!(CIPHER_ERR_UNKNOWN, -1);
    assert_eq!(CIPHER_ERR_INVALID_ARGUMENT, -8);
    let status = unsafe { elephc_openssl_cipher_methods(0, ptr::null_mut(), 0, ptr::null_mut()) };
    assert_eq!(status, CIPHER_ERR_INVALID_ARGUMENT as isize);
}

/// Verifies the exact PHP 135-name default-provider catalog, in PHP's sorted order.
#[test]
fn catalog_matches_php_135() {
    let expected = php_ciphers();
    assert_eq!(elephc_openssl_cipher_count(), expected.len() as isize);
    let mut packed = vec![0u8; 4096];
    let mut packed_len = 0usize;
    let count = unsafe {
        elephc_openssl_cipher_methods(0, packed.as_mut_ptr(), packed.len(), &mut packed_len)
    };
    assert_eq!(count, expected.len() as isize);
    packed.truncate(packed_len);
    let actual: Vec<_> = packed
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
        .map(|name| std::str::from_utf8(name).expect("ASCII cipher name"))
        .collect();
    assert_eq!(actual, expected);
}

/// Verifies IV lengths, case-insensitive fetch, and unknown-cipher rejection.
#[test]
fn cipher_iv_lengths_match_php() {
    for (cipher, expected) in [
        ("aes-128-cbc", 16),
        ("aes-256-gcm", 12),
        ("aes-192-ecb", 0),
        ("aes-128-wrap", 8),
        ("aes-128-xts", 16),
        ("AES-256-GCM", 12),
    ] {
        let actual = unsafe { elephc_openssl_cipher_iv_length(cipher.as_ptr(), cipher.len()) };
        assert_eq!(actual, expected, "{cipher}");
    }
    let unknown = b"not-a-cipher";
    assert_eq!(
        unsafe { elephc_openssl_cipher_iv_length(unknown.as_ptr(), unknown.len()) },
        CIPHER_ERR_UNKNOWN as isize
    );
}

/// Verifies CBC/CTR/ECB/GCM raw output against PHP and round-trips each result.
#[test]
fn aes_modes_match_php() {
    let key = b"key";
    let iv = b"1234567890123456";
    let vectors = [
        ("aes-128-cbc", "898ada0264cbd40a8ff13a567b23a7f8", ""),
        ("aes-128-ctr", "1a8803e6be", ""),
        ("aes-128-ecb", "5e27050b59c889fee9f953137259bf6b", ""),
        ("aes-128-gcm", "c1e164a8e8", "5baa2ea61146ca73b36739c4ba10cbe3"),
    ];
    for (cipher, ciphertext, tag) in vectors {
        let (actual, actual_tag) =
            encrypt_raw(cipher, b"hello", key, iv, 1, &[], 16).expect("encrypt");
        assert_eq!(hex(&actual), ciphertext, "{cipher}");
        assert_eq!(hex(&actual_tag), tag, "{cipher} tag");
        let plaintext = decrypt_raw(cipher, &actual, key, iv, 1, &[], &actual_tag)
            .unwrap_or_else(|status| panic!("{cipher} decrypt status {status}"));
        assert_eq!(plaintext, b"hello", "{cipher}");
    }
}

/// Encodes bytes as lowercase hexadecimal.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Parses one hexadecimal fixture field.
fn decode_hex(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = char::from(pair[0]).to_digit(16).expect("validated hex");
            let low = char::from(pair[1]).to_digit(16).expect("validated hex");
            ((high << 4) | low) as u8
        })
        .collect()
}

/// Verifies representative non-AES, AEAD, wrap, CTS, XTS, and composite ciphers
/// against PHP ciphertext/tag bytes, then round-trips every successful vector.
#[test]
fn php_mode_matrix_matches() {
    for line in include_str!("php_mode_vectors.tsv").lines().filter(|line| !line.is_empty()) {
        let fields: Vec<&str> = line.split('\t').collect();
        assert_eq!(fields.len(), 9, "{line}");
        let cipher = fields[0];
        let key = decode_hex(fields[1]);
        let iv = decode_hex(fields[2]);
        let plaintext = decode_hex(fields[3]);
        let tag_len: usize = fields[4].parse().expect("tag length");
        let expected_ciphertext = fields[5];
        let expected_tag = fields[6];
        let expected_ok = fields[7] == "ok";
        let expected_plaintext = fields[8];
        let result = encrypt_raw(cipher, &plaintext, &key, &iv, 1, b"aad", tag_len);
        if !expected_ok {
            assert!(result.is_err(), "{cipher} unexpectedly succeeded");
            continue;
        }
        let (ciphertext, tag) = result.unwrap_or_else(|status| panic!("{cipher} encrypt status {status}"));
        // des3-wrap generates a fresh IV internally when none is supplied, so PHP's
        // ciphertext is likewise non-deterministic; every other vector is exact.
        if cipher != "des3-wrap" {
            assert_eq!(hex(&ciphertext), expected_ciphertext, "{cipher} ciphertext");
            assert_eq!(hex(&tag), expected_tag, "{cipher} tag");
        }
        let decrypted = decrypt_raw(cipher, &ciphertext, &key, &iv, 1, b"aad", &tag);
        if expected_plaintext.is_empty() {
            assert!(decrypted.is_err(), "{cipher} unexpectedly decrypted");
        } else {
            let decrypted = decrypted.unwrap_or_else(|status| panic!("{cipher} decrypt status {status}"));
            assert_eq!(hex(&decrypted), expected_plaintext, "{cipher} plaintext");
        }
    }
}
