<?php

// Hashing — the full PHP hash family, backed by the pure-Rust elephc-crypto
// staticlib (no CommonCrypto/libcrypto system dependency on any target).

echo "--- one-shot digests ---\n";
echo "md5:        " . md5("hello") . "\n";
echo "sha1:       " . sha1("hello") . "\n";
echo "sha256:     " . hash("sha256", "hello") . "\n";
echo "sha3-256:   " . hash("sha3-256", "hello") . "\n";
echo "ripemd160:  " . hash("ripemd160", "hello") . "\n";
echo "crc32b:     " . hash("crc32b", "hello") . "\n";

// $binary = true returns the raw digest bytes; bin2hex renders them readable.
echo "raw sha256: " . bin2hex(hash("sha256", "hello", true)) . "\n";

echo "\n--- HMAC ---\n";
echo "hmac-sha256: " . hash_hmac("sha256", "the message", "the key") . "\n";

echo "\n--- timing-safe comparison ---\n";
$expected = hash("sha256", "secret");
echo "match:    " . (hash_equals($expected, hash("sha256", "secret")) ? "yes" : "no") . "\n";
echo "mismatch: " . (hash_equals($expected, hash("sha256", "guess")) ? "yes" : "no") . "\n";

echo "\n--- hashing a file ---\n";
file_put_contents("greeting.txt", "hello");
echo "hash_file(sha256): " . hash_file("sha256", "greeting.txt") . "\n";

echo "\n--- incremental hashing ---\n";
$ctx = hash_init("sha256");
hash_update($ctx, "hel");
hash_update($ctx, "lo");
echo "incremental sha256: " . hash_final($ctx) . "\n";

// hash_copy clones a context so two digests can share a common prefix.
$base = hash_init("sha256");
hash_update($base, "shared-");
$branch = hash_copy($base);
hash_update($base, "left");
hash_update($branch, "right");
echo "branch A: " . hash_final($base) . "\n";
echo "branch B: " . hash_final($branch) . "\n";

echo "\n--- supported algorithms ---\n";
echo "count: " . count(hash_algos()) . "\n";

// An unknown algorithm throws a catchable \ValueError.
try {
    hash("not-a-real-algo", "x");
} catch (\ValueError $e) {
    echo "caught: " . $e->getMessage() . "\n";
}
