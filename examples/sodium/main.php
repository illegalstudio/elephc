<?php
// Sodium is PHP's modern cryptography extension. The same program runs under
// `php main.php` and as an elephc-compiled native binary.

$key = sodium_crypto_secretbox_keygen();
$nonce = random_bytes(SODIUM_CRYPTO_SECRETBOX_NONCEBYTES);
$message = 'hello from sodium';

$ciphertext = sodium_crypto_secretbox($message, $nonce, $key);
$plaintext = sodium_crypto_secretbox_open($ciphertext, $nonce, $key);

echo strlen($ciphertext), "\n";
echo $plaintext, "\n";

$hash = sodium_crypto_pwhash_str(
    'correct horse battery staple',
    SODIUM_CRYPTO_PWHASH_OPSLIMIT_INTERACTIVE,
    SODIUM_CRYPTO_PWHASH_MEMLIMIT_INTERACTIVE,
);
var_dump(sodium_crypto_pwhash_str_verify($hash, 'correct horse battery staple'));
var_dump(sodium_crypto_pwhash_str_verify($hash, 'wrong password'));
