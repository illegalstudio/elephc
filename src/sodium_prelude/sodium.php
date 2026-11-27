<?php

const SODIUM_CRYPTO_SECRETBOX_KEYBYTES = 32;
const SODIUM_CRYPTO_SECRETBOX_MACBYTES = 16;
const SODIUM_CRYPTO_SECRETBOX_NONCEBYTES = 24;
const SODIUM_CRYPTO_XOF_SHAKE128_BLOCKBYTES = 168;
const SODIUM_CRYPTO_XOF_SHAKE128_STATEBYTES = 256;
const SODIUM_CRYPTO_XOF_SHAKE256_BLOCKBYTES = 136;
const SODIUM_CRYPTO_XOF_SHAKE256_STATEBYTES = 256;

class SodiumException extends \Exception
{
}

extern "elephc_sodium" {
    function elephc_sodium_secretbox(string $key, int $keyLength, string $nonce, int $nonceLength, string $message, int $messageLength): ptr;
    function elephc_sodium_secretbox_open(string $key, int $keyLength, string $nonce, int $nonceLength, string $ciphertext, int $ciphertextLength): ptr;
    function elephc_sodium_shake128(int $length, string $message, int $messageLength): ptr;
    function elephc_sodium_shake256(int $length, string $message, int $messageLength): ptr;
    function elephc_sodium_bin2ip(string $binary, int $length): ptr;
    function elephc_sodium_ip2bin(string $text, int $length): ptr;
    function elephc_sodium_shake128_init(int $domain): int;
    function elephc_sodium_shake128_update(int $handle, string $message, int $length): int;
    function elephc_sodium_shake128_squeeze(int $handle, int $length): ptr;
    function elephc_sodium_shake128_drop(int $handle): void;
    function elephc_sodium_shake256_init(int $domain): int;
    function elephc_sodium_shake256_update(int $handle, string $message, int $length): int;
    function elephc_sodium_shake256_squeeze(int $handle, int $length): ptr;
    function elephc_sodium_shake256_drop(int $handle): void;
    function elephc_sodium_last_len(): int;
}

final class SodiumCryptoXofShake128State
{
    private int $__handle = 0;

    public function __construct(int $handle = 0)
    {
        if ($handle <= 0) {
            throw new \SodiumException("SodiumCryptoXofShake128State is not initialized");
        }
        $this->__handle = $handle;
    }

    public function __elephc_handle(): int
    {
        return $this->__handle;
    }

    public function __destruct()
    {
        if ($this->__handle !== 0) {
            \elephc_sodium_shake128_drop($this->__handle);
            $this->__handle = 0;
        }
    }
}

final class SodiumCryptoXofShake256State
{
    private int $__handle = 0;

    public function __construct(int $handle = 0)
    {
        if ($handle <= 0) {
            throw new \SodiumException("SodiumCryptoXofShake256State is not initialized");
        }
        $this->__handle = $handle;
    }

    public function __elephc_handle(): int
    {
        return $this->__handle;
    }

    public function __destruct()
    {
        if ($this->__handle !== 0) {
            \elephc_sodium_shake256_drop($this->__handle);
            $this->__handle = 0;
        }
    }
}

function sodium_crypto_secretbox(string $message, string $nonce, string $key): string
{
    if (\strlen($nonce) !== SODIUM_CRYPTO_SECRETBOX_NONCEBYTES) {
        throw new \SodiumException("sodium_crypto_secretbox(): Argument #2 (\$nonce) must be SODIUM_CRYPTO_SECRETBOX_NONCEBYTES bytes long");
    }
    if (\strlen($key) !== SODIUM_CRYPTO_SECRETBOX_KEYBYTES) {
        throw new \SodiumException("sodium_crypto_secretbox(): Argument #3 (\$key) must be SODIUM_CRYPTO_SECRETBOX_KEYBYTES bytes long");
    }
    $pointer = \elephc_sodium_secretbox($key, \strlen($key), $nonce, \strlen($nonce), $message, \strlen($message));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_secretbox(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_secretbox_open(string $ciphertext, string $nonce, string $key): string|false
{
    if (\strlen($nonce) !== SODIUM_CRYPTO_SECRETBOX_NONCEBYTES) {
        throw new \SodiumException("sodium_crypto_secretbox_open(): Argument #2 (\$nonce) must be SODIUM_CRYPTO_SECRETBOX_NONCEBYTES bytes long");
    }
    if (\strlen($key) !== SODIUM_CRYPTO_SECRETBOX_KEYBYTES) {
        throw new \SodiumException("sodium_crypto_secretbox_open(): Argument #3 (\$key) must be SODIUM_CRYPTO_SECRETBOX_KEYBYTES bytes long");
    }
    if (\strlen($ciphertext) < SODIUM_CRYPTO_SECRETBOX_MACBYTES) {
        return false;
    }
    $pointer = \elephc_sodium_secretbox_open($key, \strlen($key), $nonce, \strlen($nonce), $ciphertext, \strlen($ciphertext));
    if (\__elephc_ptr_is_null($pointer)) {
        return false;
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_secretbox_keygen(): string
{
    return \random_bytes(SODIUM_CRYPTO_SECRETBOX_KEYBYTES);
}

function sodium_crypto_xof_shake128(int $length, string $message): string
{
    if ($length <= 0) {
        throw new \SodiumException("sodium_crypto_xof_shake128(): Argument #1 (\$length) must be greater than 0");
    }
    $pointer = \elephc_sodium_shake128($length, $message, \strlen($message));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_xof_shake128(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_xof_shake256(int $length, string $message): string
{
    if ($length <= 0) {
        throw new \SodiumException("sodium_crypto_xof_shake256(): Argument #1 (\$length) must be greater than 0");
    }
    $pointer = \elephc_sodium_shake256($length, $message, \strlen($message));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_xof_shake256(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_bin2ip(string $bin): string
{
    if (\strlen($bin) !== 16) {
        throw new \SodiumException("sodium_bin2ip(): Argument #1 (\$bin) must be 16 bytes long");
    }
    $pointer = \elephc_sodium_bin2ip($bin, 16);
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_bin2ip(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_ip2bin(string $ip): string
{
    $pointer = \elephc_sodium_ip2bin($ip, \strlen($ip));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_ip2bin(): Argument #1 (\$ip) must be a valid IP address");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_xof_shake128_init(?int $domain = null): SodiumCryptoXofShake128State
{
    if ($domain !== null && ($domain < 0x01 || $domain > 0x7f)) {
        throw new \SodiumException("sodium_crypto_xof_shake128_init(): Argument #1 (\$domain) must be between 0x01 and 0x7f");
    }
    $handle = \elephc_sodium_shake128_init($domain === null ? -1 : $domain);
    if ($handle < 0) {
        throw new \SodiumException("sodium_crypto_xof_shake128_init(): internal error");
    }
    return new SodiumCryptoXofShake128State($handle);
}

function sodium_crypto_xof_shake128_update(SodiumCryptoXofShake128State $state, string $message): void
{
    $result = \elephc_sodium_shake128_update($state->__elephc_handle(), $message, \strlen($message));
    if ($result !== 0) {
        throw new \SodiumException("sodium_crypto_xof_shake128_update(): internal error");
    }
}

function sodium_crypto_xof_shake128_squeeze(SodiumCryptoXofShake128State $state, int $length): string
{
    if ($length <= 0) {
        throw new \SodiumException("sodium_crypto_xof_shake128_squeeze(): Argument #2 (\$length) must be greater than 0");
    }
    $pointer = \elephc_sodium_shake128_squeeze($state->__elephc_handle(), $length);
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_xof_shake128_squeeze(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_xof_shake256_init(?int $domain = null): SodiumCryptoXofShake256State
{
    if ($domain !== null && ($domain < 0x01 || $domain > 0x7f)) {
        throw new \SodiumException("sodium_crypto_xof_shake256_init(): Argument #1 (\$domain) must be between 0x01 and 0x7f");
    }
    $handle = \elephc_sodium_shake256_init($domain === null ? -1 : $domain);
    if ($handle < 0) {
        throw new \SodiumException("sodium_crypto_xof_shake256_init(): internal error");
    }
    return new SodiumCryptoXofShake256State($handle);
}

function sodium_crypto_xof_shake256_update(SodiumCryptoXofShake256State $state, string $message): void
{
    $result = \elephc_sodium_shake256_update($state->__elephc_handle(), $message, \strlen($message));
    if ($result !== 0) {
        throw new \SodiumException("sodium_crypto_xof_shake256_update(): internal error");
    }
}

function sodium_crypto_xof_shake256_squeeze(SodiumCryptoXofShake256State $state, int $length): string
{
    if ($length <= 0) {
        throw new \SodiumException("sodium_crypto_xof_shake256_squeeze(): Argument #2 (\$length) must be greater than 0");
    }
    $pointer = \elephc_sodium_shake256_squeeze($state->__elephc_handle(), $length);
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_xof_shake256_squeeze(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}
