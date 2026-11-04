---
title: "Sodium builtins"
description: "Builtins in the Sodium category."
sidebar:
  order: 123
---

## Sodium builtins

| Function | Signature | Returns | AOT | eval() |
|---|---|---|:-:|:-:|
| [`sodium_add()`](./sodium/sodium_add.md) | `(string $string1, string $string2): void` | `void` | ✓ | - |
| [`sodium_base642bin()`](./sodium/sodium_base642bin.md) | `(string $string, int $id, string $ignore = ''): string` | `string` | ✓ | ✓ |
| [`sodium_bin2base64()`](./sodium/sodium_bin2base64.md) | `(string $string, int $id): string` | `string` | ✓ | ✓ |
| [`sodium_bin2hex()`](./sodium/sodium_bin2hex.md) | `(string $string): string` | `string` | ✓ | ✓ |
| [`sodium_compare()`](./sodium/sodium_compare.md) | `(string $string1, string $string2): int` | `int` | ✓ | ✓ |
| [`sodium_crypto_aead_aegis128l_decrypt()`](./sodium/sodium_crypto_aead_aegis128l_decrypt.md) | `(string $ciphertext, string $additional_data, string $nonce, string $key): string|false` | `string|false` | ✓ | ✓ |
| [`sodium_crypto_aead_aegis128l_encrypt()`](./sodium/sodium_crypto_aead_aegis128l_encrypt.md) | `(string $message, string $additional_data, string $nonce, string $key): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_aead_aegis128l_keygen()`](./sodium/sodium_crypto_aead_aegis128l_keygen.md) | `(): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_aead_aegis256_decrypt()`](./sodium/sodium_crypto_aead_aegis256_decrypt.md) | `(string $ciphertext, string $additional_data, string $nonce, string $key): string|false` | `string|false` | ✓ | ✓ |
| [`sodium_crypto_aead_aegis256_encrypt()`](./sodium/sodium_crypto_aead_aegis256_encrypt.md) | `(string $message, string $additional_data, string $nonce, string $key): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_aead_aegis256_keygen()`](./sodium/sodium_crypto_aead_aegis256_keygen.md) | `(): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_aead_aes256gcm_decrypt()`](./sodium/sodium_crypto_aead_aes256gcm_decrypt.md) | `(string $ciphertext, string $additional_data, string $nonce, string $key): string|false` | `string|false` | ✓ | ✓ |
| [`sodium_crypto_aead_aes256gcm_encrypt()`](./sodium/sodium_crypto_aead_aes256gcm_encrypt.md) | `(string $message, string $additional_data, string $nonce, string $key): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_aead_aes256gcm_is_available()`](./sodium/sodium_crypto_aead_aes256gcm_is_available.md) | `(): bool` | `bool` | ✓ | ✓ |
| [`sodium_crypto_aead_aes256gcm_keygen()`](./sodium/sodium_crypto_aead_aes256gcm_keygen.md) | `(): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_aead_chacha20poly1305_decrypt()`](./sodium/sodium_crypto_aead_chacha20poly1305_decrypt.md) | `(string $ciphertext, string $additional_data, string $nonce, string $key): string|false` | `string|false` | ✓ | ✓ |
| [`sodium_crypto_aead_chacha20poly1305_encrypt()`](./sodium/sodium_crypto_aead_chacha20poly1305_encrypt.md) | `(string $message, string $additional_data, string $nonce, string $key): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_aead_chacha20poly1305_ietf_decrypt()`](./sodium/sodium_crypto_aead_chacha20poly1305_ietf_decrypt.md) | `(string $ciphertext, string $additional_data, string $nonce, string $key): string|false` | `string|false` | ✓ | ✓ |
| [`sodium_crypto_aead_chacha20poly1305_ietf_encrypt()`](./sodium/sodium_crypto_aead_chacha20poly1305_ietf_encrypt.md) | `(string $message, string $additional_data, string $nonce, string $key): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_aead_chacha20poly1305_ietf_keygen()`](./sodium/sodium_crypto_aead_chacha20poly1305_ietf_keygen.md) | `(): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_aead_chacha20poly1305_keygen()`](./sodium/sodium_crypto_aead_chacha20poly1305_keygen.md) | `(): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_aead_xchacha20poly1305_ietf_decrypt()`](./sodium/sodium_crypto_aead_xchacha20poly1305_ietf_decrypt.md) | `(string $ciphertext, string $additional_data, string $nonce, string $key): string|false` | `string|false` | ✓ | ✓ |
| [`sodium_crypto_aead_xchacha20poly1305_ietf_encrypt()`](./sodium/sodium_crypto_aead_xchacha20poly1305_ietf_encrypt.md) | `(string $message, string $additional_data, string $nonce, string $key): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_aead_xchacha20poly1305_ietf_keygen()`](./sodium/sodium_crypto_aead_xchacha20poly1305_ietf_keygen.md) | `(): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_auth()`](./sodium/sodium_crypto_auth.md) | `(string $message, string $key): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_auth_keygen()`](./sodium/sodium_crypto_auth_keygen.md) | `(): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_auth_verify()`](./sodium/sodium_crypto_auth_verify.md) | `(string $mac, string $message, string $key): bool` | `bool` | ✓ | ✓ |
| [`sodium_crypto_box()`](./sodium/sodium_crypto_box.md) | `(string $message, string $nonce, string $key_pair): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_box_keypair()`](./sodium/sodium_crypto_box_keypair.md) | `(): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_box_keypair_from_secretkey_and_publickey()`](./sodium/sodium_crypto_box_keypair_from_secretkey_and_publickey.md) | `(string $secret_key, string $public_key): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_box_open()`](./sodium/sodium_crypto_box_open.md) | `(string $ciphertext, string $nonce, string $key_pair): string|false` | `string|false` | ✓ | ✓ |
| [`sodium_crypto_box_publickey()`](./sodium/sodium_crypto_box_publickey.md) | `(string $key_pair): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_box_publickey_from_secretkey()`](./sodium/sodium_crypto_box_publickey_from_secretkey.md) | `(string $secret_key): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_box_seal()`](./sodium/sodium_crypto_box_seal.md) | `(string $message, string $public_key): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_box_seal_open()`](./sodium/sodium_crypto_box_seal_open.md) | `(string $ciphertext, string $key_pair): string|false` | `string|false` | ✓ | ✓ |
| [`sodium_crypto_box_secretkey()`](./sodium/sodium_crypto_box_secretkey.md) | `(string $key_pair): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_box_seed_keypair()`](./sodium/sodium_crypto_box_seed_keypair.md) | `(string $seed): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_core_ristretto255_add()`](./sodium/sodium_crypto_core_ristretto255_add.md) | `(string $p, string $q): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_core_ristretto255_from_hash()`](./sodium/sodium_crypto_core_ristretto255_from_hash.md) | `(string $s): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_core_ristretto255_is_valid_point()`](./sodium/sodium_crypto_core_ristretto255_is_valid_point.md) | `(string $s): bool` | `bool` | ✓ | ✓ |
| [`sodium_crypto_core_ristretto255_random()`](./sodium/sodium_crypto_core_ristretto255_random.md) | `(): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_core_ristretto255_scalar_add()`](./sodium/sodium_crypto_core_ristretto255_scalar_add.md) | `(string $x, string $y): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_core_ristretto255_scalar_complement()`](./sodium/sodium_crypto_core_ristretto255_scalar_complement.md) | `(string $s): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_core_ristretto255_scalar_invert()`](./sodium/sodium_crypto_core_ristretto255_scalar_invert.md) | `(string $s): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_core_ristretto255_scalar_mul()`](./sodium/sodium_crypto_core_ristretto255_scalar_mul.md) | `(string $x, string $y): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_core_ristretto255_scalar_negate()`](./sodium/sodium_crypto_core_ristretto255_scalar_negate.md) | `(string $s): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_core_ristretto255_scalar_random()`](./sodium/sodium_crypto_core_ristretto255_scalar_random.md) | `(): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_core_ristretto255_scalar_reduce()`](./sodium/sodium_crypto_core_ristretto255_scalar_reduce.md) | `(string $s): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_core_ristretto255_scalar_sub()`](./sodium/sodium_crypto_core_ristretto255_scalar_sub.md) | `(string $x, string $y): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_core_ristretto255_sub()`](./sodium/sodium_crypto_core_ristretto255_sub.md) | `(string $p, string $q): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_generichash()`](./sodium/sodium_crypto_generichash.md) | `(string $message, string $key = '', int $length = 32): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_generichash_final()`](./sodium/sodium_crypto_generichash_final.md) | `(string $state, int $length = 32): string` | `string` | ✓ | - |
| [`sodium_crypto_generichash_init()`](./sodium/sodium_crypto_generichash_init.md) | `(string $key = '', int $length = 32): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_generichash_keygen()`](./sodium/sodium_crypto_generichash_keygen.md) | `(): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_generichash_update()`](./sodium/sodium_crypto_generichash_update.md) | `(string $state, string $message): bool` | `bool` | ✓ | - |
| [`sodium_crypto_kdf_derive_from_key()`](./sodium/sodium_crypto_kdf_derive_from_key.md) | `(int $subkey_length, int $subkey_id, string $context, string $key): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_kdf_keygen()`](./sodium/sodium_crypto_kdf_keygen.md) | `(): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_kx_client_session_keys()`](./sodium/sodium_crypto_kx_client_session_keys.md) | `(string $client_key_pair, string $server_key): array` | `array` | ✓ | ✓ |
| [`sodium_crypto_kx_keypair()`](./sodium/sodium_crypto_kx_keypair.md) | `(): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_kx_publickey()`](./sodium/sodium_crypto_kx_publickey.md) | `(string $key_pair): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_kx_secretkey()`](./sodium/sodium_crypto_kx_secretkey.md) | `(string $key_pair): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_kx_seed_keypair()`](./sodium/sodium_crypto_kx_seed_keypair.md) | `(string $seed): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_kx_server_session_keys()`](./sodium/sodium_crypto_kx_server_session_keys.md) | `(string $server_key_pair, string $client_key): array` | `array` | ✓ | ✓ |
| [`sodium_crypto_pwhash()`](./sodium/sodium_crypto_pwhash.md) | `(int $length, string $password, string $salt, int $opslimit, int $memlimit, int $algo = 2): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_pwhash_scryptsalsa208sha256()`](./sodium/sodium_crypto_pwhash_scryptsalsa208sha256.md) | `(int $length, string $password, string $salt, int $opslimit, int $memlimit): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_pwhash_scryptsalsa208sha256_str()`](./sodium/sodium_crypto_pwhash_scryptsalsa208sha256_str.md) | `(string $password, int $opslimit, int $memlimit): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_pwhash_scryptsalsa208sha256_str_verify()`](./sodium/sodium_crypto_pwhash_scryptsalsa208sha256_str_verify.md) | `(string $hash, string $password): bool` | `bool` | ✓ | ✓ |
| [`sodium_crypto_pwhash_str()`](./sodium/sodium_crypto_pwhash_str.md) | `(string $password, int $opslimit, int $memlimit): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_pwhash_str_needs_rehash()`](./sodium/sodium_crypto_pwhash_str_needs_rehash.md) | `(string $password, int $opslimit, int $memlimit): bool` | `bool` | ✓ | ✓ |
| [`sodium_crypto_pwhash_str_verify()`](./sodium/sodium_crypto_pwhash_str_verify.md) | `(string $hash, string $password): bool` | `bool` | ✓ | ✓ |
| [`sodium_crypto_scalarmult()`](./sodium/sodium_crypto_scalarmult.md) | `(string $n, string $p): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_scalarmult_base()`](./sodium/sodium_crypto_scalarmult_base.md) | `(string $secret_key): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_scalarmult_ristretto255()`](./sodium/sodium_crypto_scalarmult_ristretto255.md) | `(string $n, string $p): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_scalarmult_ristretto255_base()`](./sodium/sodium_crypto_scalarmult_ristretto255_base.md) | `(string $n): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_secretbox()`](./sodium/sodium_crypto_secretbox.md) | `(string $message, string $nonce, string $key): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_secretbox_keygen()`](./sodium/sodium_crypto_secretbox_keygen.md) | `(): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_secretbox_open()`](./sodium/sodium_crypto_secretbox_open.md) | `(string $ciphertext, string $nonce, string $key): string|false` | `string|false` | ✓ | ✓ |
| [`sodium_crypto_secretstream_xchacha20poly1305_init_pull()`](./sodium/sodium_crypto_secretstream_xchacha20poly1305_init_pull.md) | `(string $header, string $key): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_secretstream_xchacha20poly1305_init_push()`](./sodium/sodium_crypto_secretstream_xchacha20poly1305_init_push.md) | `(string $key): array` | `array` | ✓ | ✓ |
| [`sodium_crypto_secretstream_xchacha20poly1305_keygen()`](./sodium/sodium_crypto_secretstream_xchacha20poly1305_keygen.md) | `(): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_secretstream_xchacha20poly1305_pull()`](./sodium/sodium_crypto_secretstream_xchacha20poly1305_pull.md) | `(string $state, string $ciphertext, string $additional_data = ''): array|false` | `array|false` | ✓ | - |
| [`sodium_crypto_secretstream_xchacha20poly1305_push()`](./sodium/sodium_crypto_secretstream_xchacha20poly1305_push.md) | `(string $state, string $message, string $additional_data = '', int $tag = 0): string` | `string` | ✓ | - |
| [`sodium_crypto_secretstream_xchacha20poly1305_rekey()`](./sodium/sodium_crypto_secretstream_xchacha20poly1305_rekey.md) | `(string $state): void` | `void` | ✓ | - |
| [`sodium_crypto_shorthash()`](./sodium/sodium_crypto_shorthash.md) | `(string $message, string $key): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_shorthash_keygen()`](./sodium/sodium_crypto_shorthash_keygen.md) | `(): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_sign()`](./sodium/sodium_crypto_sign.md) | `(string $message, string $secret_key): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_sign_detached()`](./sodium/sodium_crypto_sign_detached.md) | `(string $message, string $secret_key): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_sign_ed25519_pk_to_curve25519()`](./sodium/sodium_crypto_sign_ed25519_pk_to_curve25519.md) | `(string $public_key): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_sign_ed25519_sk_to_curve25519()`](./sodium/sodium_crypto_sign_ed25519_sk_to_curve25519.md) | `(string $secret_key): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_sign_keypair()`](./sodium/sodium_crypto_sign_keypair.md) | `(): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_sign_keypair_from_secretkey_and_publickey()`](./sodium/sodium_crypto_sign_keypair_from_secretkey_and_publickey.md) | `(string $secret_key, string $public_key): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_sign_open()`](./sodium/sodium_crypto_sign_open.md) | `(string $signed_message, string $public_key): string|false` | `string|false` | ✓ | ✓ |
| [`sodium_crypto_sign_publickey()`](./sodium/sodium_crypto_sign_publickey.md) | `(string $key_pair): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_sign_publickey_from_secretkey()`](./sodium/sodium_crypto_sign_publickey_from_secretkey.md) | `(string $secret_key): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_sign_secretkey()`](./sodium/sodium_crypto_sign_secretkey.md) | `(string $key_pair): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_sign_seed_keypair()`](./sodium/sodium_crypto_sign_seed_keypair.md) | `(string $seed): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_sign_verify_detached()`](./sodium/sodium_crypto_sign_verify_detached.md) | `(string $signature, string $message, string $public_key): bool` | `bool` | ✓ | ✓ |
| [`sodium_crypto_stream()`](./sodium/sodium_crypto_stream.md) | `(int $length, string $nonce, string $key): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_stream_keygen()`](./sodium/sodium_crypto_stream_keygen.md) | `(): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_stream_xchacha20()`](./sodium/sodium_crypto_stream_xchacha20.md) | `(int $length, string $nonce, string $key): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_stream_xchacha20_keygen()`](./sodium/sodium_crypto_stream_xchacha20_keygen.md) | `(): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_stream_xchacha20_xor()`](./sodium/sodium_crypto_stream_xchacha20_xor.md) | `(string $message, string $nonce, string $key): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_stream_xchacha20_xor_ic()`](./sodium/sodium_crypto_stream_xchacha20_xor_ic.md) | `(string $message, string $nonce, int $counter, string $key): string` | `string` | ✓ | ✓ |
| [`sodium_crypto_stream_xor()`](./sodium/sodium_crypto_stream_xor.md) | `(string $message, string $nonce, string $key): string` | `string` | ✓ | ✓ |
| [`sodium_hex2bin()`](./sodium/sodium_hex2bin.md) | `(string $string, string $ignore = ''): string` | `string` | ✓ | ✓ |
| [`sodium_increment()`](./sodium/sodium_increment.md) | `(string $string): void` | `void` | ✓ | - |
| [`sodium_memcmp()`](./sodium/sodium_memcmp.md) | `(string $string1, string $string2): int` | `int` | ✓ | ✓ |
| [`sodium_memzero()`](./sodium/sodium_memzero.md) | `(string $string): void` | `void` | ✓ | - |
| [`sodium_pad()`](./sodium/sodium_pad.md) | `(string $string, int $block_size): string` | `string` | ✓ | ✓ |
| [`sodium_unpad()`](./sodium/sodium_unpad.md) | `(string $string, int $block_size): string` | `string` | ✓ | ✓ |
