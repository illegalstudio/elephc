<?php
// The libcrypto-backed bridge exposes PHP's complete default-provider catalog.
$methods = openssl_get_cipher_methods();
echo count($methods), " cipher methods\n";
foreach (["aes-128-gcm", "aria-128-cbc", "chacha20-poly1305", "sm4-gcm"] as $cipher) {
    echo $cipher, ": ", openssl_cipher_iv_length($cipher), "-byte IV\n";
}

$key = str_repeat("k", 32);
$iv = str_repeat("i", 12);
$tag = null;
$ciphertext = openssl_encrypt("hello, native OpenSSL", "aes-256-gcm", $key, OPENSSL_RAW_DATA, $iv, $tag);
$plaintext = openssl_decrypt($ciphertext, "aes-256-gcm", $key, OPENSSL_RAW_DATA, $iv, $tag, "example-aad");
echo "round trip: ", $plaintext, "\n";
