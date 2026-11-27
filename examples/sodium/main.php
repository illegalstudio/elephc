<?php
// ext/sodium: symmetric, public-key, signing, hashing and streaming primitives.

$key = sodium_crypto_secretbox_keygen();
$nonce = random_bytes(SODIUM_CRYPTO_SECRETBOX_NONCEBYTES);
$cipher = sodium_crypto_secretbox("hello", $nonce, $key);
echo sodium_crypto_secretbox_open($cipher, $nonce, $key), "\n";

$alice = sodium_crypto_box_keypair();
$sealed = sodium_crypto_box_seal("sealed", sodium_crypto_box_publickey($alice));
echo sodium_crypto_box_seal_open($sealed, $alice), "\n";

$signing = sodium_crypto_sign_keypair();
$signature = sodium_crypto_sign_detached("hello", sodium_crypto_sign_secretkey($signing));
echo sodium_crypto_sign_verify_detached($signature, "hello", sodium_crypto_sign_publickey($signing)) ? "verified" : "bad", "\n";

echo bin2hex(sodium_crypto_generichash("hello")), "\n";

$stream_key = sodium_crypto_secretstream_xchacha20poly1305_keygen();
$push = sodium_crypto_secretstream_xchacha20poly1305_init_push($stream_key);
$state = $push[0];
$chunk = sodium_crypto_secretstream_xchacha20poly1305_push($state, "chunk");
$pull = sodium_crypto_secretstream_xchacha20poly1305_init_pull($push[1], $stream_key);
$message = sodium_crypto_secretstream_xchacha20poly1305_pull($pull, $chunk);
echo $message[0], ":", $message[1], "\n";
