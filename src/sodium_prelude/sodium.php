<?php

const SODIUM_CRYPTO_SECRETBOX_KEYBYTES = 32;
const SODIUM_CRYPTO_SECRETBOX_MACBYTES = 16;
const SODIUM_CRYPTO_SECRETBOX_NONCEBYTES = 24;
const SODIUM_CRYPTO_XOF_SHAKE128_BLOCKBYTES = 168;
const SODIUM_CRYPTO_XOF_SHAKE128_STATEBYTES = 256;
const SODIUM_CRYPTO_XOF_SHAKE256_BLOCKBYTES = 136;
const SODIUM_CRYPTO_XOF_SHAKE256_STATEBYTES = 256;
const SODIUM_CRYPTO_XOF_TURBOSHAKE128_BLOCKBYTES = 168;
const SODIUM_CRYPTO_XOF_TURBOSHAKE128_STATEBYTES = 256;
const SODIUM_CRYPTO_XOF_TURBOSHAKE256_BLOCKBYTES = 136;
const SODIUM_CRYPTO_XOF_TURBOSHAKE256_STATEBYTES = 256;
const SODIUM_CRYPTO_IPCRYPT_BYTES = 16;
const SODIUM_CRYPTO_IPCRYPT_KEYBYTES = 16;
const SODIUM_CRYPTO_IPCRYPT_ND_KEYBYTES = 16;
const SODIUM_CRYPTO_IPCRYPT_ND_TWEAKBYTES = 8;
const SODIUM_CRYPTO_IPCRYPT_ND_INPUTBYTES = 16;
const SODIUM_CRYPTO_IPCRYPT_ND_OUTPUTBYTES = 24;
const SODIUM_CRYPTO_IPCRYPT_NDX_KEYBYTES = 32;
const SODIUM_CRYPTO_IPCRYPT_NDX_TWEAKBYTES = 16;
const SODIUM_CRYPTO_IPCRYPT_NDX_INPUTBYTES = 16;
const SODIUM_CRYPTO_IPCRYPT_NDX_OUTPUTBYTES = 32;
const SODIUM_CRYPTO_IPCRYPT_PFX_KEYBYTES = 32;
const SODIUM_CRYPTO_IPCRYPT_PFX_BYTES = 16;
const SODIUM_CRYPTO_KEM_PUBLICKEYBYTES = 1216;
const SODIUM_CRYPTO_KEM_SECRETKEYBYTES = 32;
const SODIUM_CRYPTO_KEM_CIPHERTEXTBYTES = 1120;
const SODIUM_CRYPTO_KEM_SHAREDSECRETBYTES = 32;
const SODIUM_CRYPTO_KEM_SEEDBYTES = 32;
const SODIUM_CRYPTO_KEM_KEYPAIRBYTES = 1248;
const SODIUM_CRYPTO_KEM_MLKEM768_PUBLICKEYBYTES = 1184;
const SODIUM_CRYPTO_KEM_MLKEM768_SECRETKEYBYTES = 2400;
const SODIUM_CRYPTO_KEM_MLKEM768_CIPHERTEXTBYTES = 1088;
const SODIUM_CRYPTO_KEM_MLKEM768_SHAREDSECRETBYTES = 32;
const SODIUM_CRYPTO_KEM_MLKEM768_SEEDBYTES = 64;
const SODIUM_CRYPTO_KEM_MLKEM768_KEYPAIRBYTES = 3584;
const SODIUM_CRYPTO_PWHASH_ALG_ARGON2I13 = 1;
const SODIUM_CRYPTO_PWHASH_ALG_ARGON2ID13 = 2;
const SODIUM_CRYPTO_PWHASH_ALG_DEFAULT = 2;
const SODIUM_CRYPTO_PWHASH_SALTBYTES = 16;
const SODIUM_CRYPTO_PWHASH_STRBYTES = 128;
const SODIUM_CRYPTO_PWHASH_STRPREFIX = '$argon2id$';
const SODIUM_CRYPTO_PWHASH_OPSLIMIT_MIN = 1;
const SODIUM_CRYPTO_PWHASH_MEMLIMIT_MIN = 8192;
const SODIUM_CRYPTO_PWHASH_OPSLIMIT_INTERACTIVE = 2;
const SODIUM_CRYPTO_PWHASH_MEMLIMIT_INTERACTIVE = 67108864;
const SODIUM_CRYPTO_PWHASH_OPSLIMIT_MODERATE = 3;
const SODIUM_CRYPTO_PWHASH_MEMLIMIT_MODERATE = 268435456;
const SODIUM_CRYPTO_PWHASH_OPSLIMIT_SENSITIVE = 4;
const SODIUM_CRYPTO_PWHASH_MEMLIMIT_SENSITIVE = 1073741824;
const SODIUM_CRYPTO_PWHASH_BYTES_MIN = 16;
const SODIUM_CRYPTO_PWHASH_BYTES_MAX = 4294967295;
const SODIUM_CRYPTO_PWHASH_PASSWD_MIN = 0;
const SODIUM_CRYPTO_PWHASH_PASSWD_MAX = 4294967295;
const SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_SALTBYTES = 32;
const SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_BYTES_MIN = 16;
const SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_BYTES_MAX = 4294967295;
const SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_STRBYTES = 102;
const SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_STRPREFIX = '$7$';
const SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_OPSLIMIT_MIN = 32768;
const SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_MEMLIMIT_MIN = 16777216;
const SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_OPSLIMIT_INTERACTIVE = 524288;
const SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_MEMLIMIT_INTERACTIVE = 16777216;
const SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_OPSLIMIT_SENSITIVE = 33554432;
const SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_MEMLIMIT_SENSITIVE = 1073741824;

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
    function elephc_sodium_turboshake128(int $length, string $message, int $messageLength): ptr;
    function elephc_sodium_turboshake256(int $length, string $message, int $messageLength): ptr;
    function elephc_sodium_turboshake128_init(int $domain): int;
    function elephc_sodium_turboshake128_update(int $handle, string $message, int $length): int;
    function elephc_sodium_turboshake128_squeeze(int $handle, int $length): ptr;
    function elephc_sodium_turboshake128_drop(int $handle): void;
    function elephc_sodium_turboshake256_init(int $domain): int;
    function elephc_sodium_turboshake256_update(int $handle, string $message, int $length): int;
    function elephc_sodium_turboshake256_squeeze(int $handle, int $length): ptr;
    function elephc_sodium_turboshake256_drop(int $handle): void;
    function elephc_sodium_ipcrypt_encrypt(string $input, int $inputLength, string $key, int $keyLength): ptr;
    function elephc_sodium_ipcrypt_decrypt(string $input, int $inputLength, string $key, int $keyLength): ptr;
    function elephc_sodium_ipcrypt_nd_encrypt(string $input, int $inputLength, string $tweak, int $tweakLength, string $key, int $keyLength): ptr;
    function elephc_sodium_ipcrypt_nd_decrypt(string $input, int $inputLength, string $key, int $keyLength): ptr;
    function elephc_sodium_ipcrypt_ndx_encrypt(string $input, int $inputLength, string $tweak, int $tweakLength, string $key, int $keyLength): ptr;
    function elephc_sodium_ipcrypt_ndx_decrypt(string $input, int $inputLength, string $key, int $keyLength): ptr;
    function elephc_sodium_ipcrypt_pfx_encrypt(string $input, int $inputLength, string $key, int $keyLength): ptr;
    function elephc_sodium_ipcrypt_pfx_decrypt(string $input, int $inputLength, string $key, int $keyLength): ptr;
    function elephc_sodium_kem_keypair(): ptr;
    function elephc_sodium_kem_seed_keypair(string $seed, int $seedLength): ptr;
    function elephc_sodium_kem_enc(string $publicKey, int $publicKeyLength): ptr;
    function elephc_sodium_kem_dec(string $ciphertext, int $ciphertextLength, string $secretKey, int $secretKeyLength): ptr;
    function elephc_sodium_kem_mlkem768_keypair(): ptr;
    function elephc_sodium_kem_mlkem768_seed_keypair(string $seed, int $seedLength): ptr;
    function elephc_sodium_kem_mlkem768_enc(string $publicKey, int $publicKeyLength): ptr;
    function elephc_sodium_kem_mlkem768_dec(string $ciphertext, int $ciphertextLength, string $secretKey, int $secretKeyLength): ptr;
    function elephc_sodium_pwhash(int $length, string $password, int $passwordLength, string $salt, int $saltLength, int $opslimit, int $memlimit, int $algorithm): ptr;
    function elephc_sodium_pwhash_str(string $password, int $passwordLength, string $salt, int $saltLength, int $opslimit, int $memlimit, int $algorithm): ptr;
    function elephc_sodium_pwhash_str_verify(string $password, int $passwordLength, string $hash, int $hashLength): int;
    function elephc_sodium_pwhash_scryptsalsa208sha256(int $length, string $password, int $passwordLength, string $salt, int $saltLength, int $opslimit, int $memlimit): ptr;
    function elephc_sodium_pwhash_scryptsalsa208sha256_str(string $password, int $passwordLength, string $salt, int $saltLength, int $opslimit, int $memlimit): ptr;
    function elephc_sodium_pwhash_scryptsalsa208sha256_str_verify(string $password, int $passwordLength, string $hash, int $hashLength): int;
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

final class SodiumCryptoXofTurboshake128State
{
    private int $__handle = 0;

    public function __construct(int $handle = 0)
    {
        if ($handle <= 0) {
            throw new \SodiumException("SodiumCryptoXofTurboshake128State is not initialized");
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
            \elephc_sodium_turboshake128_drop($this->__handle);
            $this->__handle = 0;
        }
    }
}

final class SodiumCryptoXofTurboshake256State
{
    private int $__handle = 0;

    public function __construct(int $handle = 0)
    {
        if ($handle <= 0) {
            throw new \SodiumException("SodiumCryptoXofTurboshake256State is not initialized");
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
            \elephc_sodium_turboshake256_drop($this->__handle);
            $this->__handle = 0;
        }
    }
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

function sodium_crypto_ipcrypt_keygen(): string
{
    return \random_bytes(SODIUM_CRYPTO_IPCRYPT_KEYBYTES);
}

function sodium_crypto_ipcrypt_encrypt(string $ip, string $key): string
{
    if (\strlen($key) !== SODIUM_CRYPTO_IPCRYPT_KEYBYTES) {
        throw new \SodiumException("sodium_crypto_ipcrypt_encrypt(): Argument #2 (\$key) must be SODIUM_CRYPTO_IPCRYPT_KEYBYTES bytes long");
    }
    $binary = __elephc_sodium_ip_bin($ip, "sodium_crypto_ipcrypt_encrypt(): Argument #1 (\$ip) must be a valid IP address");
    $pointer = \elephc_sodium_ipcrypt_encrypt($binary, \strlen($binary), $key, \strlen($key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_ipcrypt_encrypt(): internal error");
    }
    return \sodium_bin2ip(\__elephc_ptr_read_string($pointer, \elephc_sodium_last_len()));
}

function sodium_crypto_ipcrypt_decrypt(string $encrypted_ip, string $key): string
{
    if (\strlen($key) !== SODIUM_CRYPTO_IPCRYPT_KEYBYTES) {
        throw new \SodiumException("sodium_crypto_ipcrypt_decrypt(): Argument #2 (\$key) must be SODIUM_CRYPTO_IPCRYPT_KEYBYTES bytes long");
    }
    $binary = __elephc_sodium_ip_bin($encrypted_ip, "sodium_crypto_ipcrypt_decrypt(): Argument #1 (\$encrypted_ip) must be a valid IP address");
    $pointer = \elephc_sodium_ipcrypt_decrypt($binary, \strlen($binary), $key, \strlen($key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_ipcrypt_decrypt(): internal error");
    }
    return \sodium_bin2ip(\__elephc_ptr_read_string($pointer, \elephc_sodium_last_len()));
}

function sodium_crypto_ipcrypt_nd_keygen(): string
{
    return \random_bytes(SODIUM_CRYPTO_IPCRYPT_ND_KEYBYTES);
}

function sodium_crypto_ipcrypt_nd_encrypt(string $ip, string $key): string
{
    if (\strlen($key) !== SODIUM_CRYPTO_IPCRYPT_ND_KEYBYTES) {
        throw new \SodiumException("sodium_crypto_ipcrypt_nd_encrypt(): Argument #2 (\$key) must be SODIUM_CRYPTO_IPCRYPT_ND_KEYBYTES bytes long");
    }
    $binary = __elephc_sodium_ip_bin($ip, "sodium_crypto_ipcrypt_nd_encrypt(): Argument #1 (\$ip) must be a valid IP address");
    $tweak = \random_bytes(SODIUM_CRYPTO_IPCRYPT_ND_TWEAKBYTES);
    $pointer = \elephc_sodium_ipcrypt_nd_encrypt($binary, \strlen($binary), $tweak, \strlen($tweak), $key, \strlen($key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_ipcrypt_nd_encrypt(): internal error");
    }
    return \bin2hex(\__elephc_ptr_read_string($pointer, \elephc_sodium_last_len()));
}

function sodium_crypto_ipcrypt_nd_decrypt(string $ciphertext_hex, string $key): string
{
    if (\strlen($key) !== SODIUM_CRYPTO_IPCRYPT_ND_KEYBYTES) {
        throw new \SodiumException("sodium_crypto_ipcrypt_nd_decrypt(): Argument #2 (\$key) must be SODIUM_CRYPTO_IPCRYPT_ND_KEYBYTES bytes long");
    }
    $binary = __elephc_sodium_ciphertext($ciphertext_hex, SODIUM_CRYPTO_IPCRYPT_ND_OUTPUTBYTES, "sodium_crypto_ipcrypt_nd_decrypt(): Argument #1 (\$ciphertext_hex) must be a valid hex-encoded ciphertext");
    $pointer = \elephc_sodium_ipcrypt_nd_decrypt($binary, \strlen($binary), $key, \strlen($key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_ipcrypt_nd_decrypt(): internal error");
    }
    return \sodium_bin2ip(\__elephc_ptr_read_string($pointer, \elephc_sodium_last_len()));
}

function sodium_crypto_ipcrypt_ndx_keygen(): string
{
    return \random_bytes(SODIUM_CRYPTO_IPCRYPT_NDX_KEYBYTES);
}

function sodium_crypto_ipcrypt_ndx_encrypt(string $ip, string $key): string
{
    if (\strlen($key) !== SODIUM_CRYPTO_IPCRYPT_NDX_KEYBYTES) {
        throw new \SodiumException("sodium_crypto_ipcrypt_ndx_encrypt(): Argument #2 (\$key) must be SODIUM_CRYPTO_IPCRYPT_NDX_KEYBYTES bytes long");
    }
    $binary = __elephc_sodium_ip_bin($ip, "sodium_crypto_ipcrypt_ndx_encrypt(): Argument #1 (\$ip) must be a valid IP address");
    $tweak = \random_bytes(SODIUM_CRYPTO_IPCRYPT_NDX_TWEAKBYTES);
    $pointer = \elephc_sodium_ipcrypt_ndx_encrypt($binary, \strlen($binary), $tweak, \strlen($tweak), $key, \strlen($key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_ipcrypt_ndx_encrypt(): internal error");
    }
    return \bin2hex(\__elephc_ptr_read_string($pointer, \elephc_sodium_last_len()));
}

function sodium_crypto_ipcrypt_ndx_decrypt(string $ciphertext_hex, string $key): string
{
    if (\strlen($key) !== SODIUM_CRYPTO_IPCRYPT_NDX_KEYBYTES) {
        throw new \SodiumException("sodium_crypto_ipcrypt_ndx_decrypt(): Argument #2 (\$key) must be SODIUM_CRYPTO_IPCRYPT_NDX_KEYBYTES bytes long");
    }
    $binary = __elephc_sodium_ciphertext($ciphertext_hex, SODIUM_CRYPTO_IPCRYPT_NDX_OUTPUTBYTES, "sodium_crypto_ipcrypt_ndx_decrypt(): Argument #1 (\$ciphertext_hex) must be a valid hex-encoded ciphertext");
    $pointer = \elephc_sodium_ipcrypt_ndx_decrypt($binary, \strlen($binary), $key, \strlen($key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_ipcrypt_ndx_decrypt(): internal error");
    }
    return \sodium_bin2ip(\__elephc_ptr_read_string($pointer, \elephc_sodium_last_len()));
}

function sodium_crypto_ipcrypt_pfx_keygen(): string
{
    return \random_bytes(SODIUM_CRYPTO_IPCRYPT_PFX_KEYBYTES);
}

function sodium_crypto_ipcrypt_pfx_encrypt(string $ip, string $key): string
{
    if (\strlen($key) !== SODIUM_CRYPTO_IPCRYPT_PFX_KEYBYTES) {
        throw new \SodiumException("sodium_crypto_ipcrypt_pfx_encrypt(): Argument #2 (\$key) must be SODIUM_CRYPTO_IPCRYPT_PFX_KEYBYTES bytes long");
    }
    $binary = __elephc_sodium_ip_bin($ip, "sodium_crypto_ipcrypt_pfx_encrypt(): Argument #1 (\$ip) must be a valid IP address");
    $pointer = \elephc_sodium_ipcrypt_pfx_encrypt($binary, \strlen($binary), $key, \strlen($key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_ipcrypt_pfx_encrypt(): internal error");
    }
    return \sodium_bin2ip(\__elephc_ptr_read_string($pointer, \elephc_sodium_last_len()));
}

function sodium_crypto_ipcrypt_pfx_decrypt(string $encrypted_ip, string $key): string
{
    if (\strlen($key) !== SODIUM_CRYPTO_IPCRYPT_PFX_KEYBYTES) {
        throw new \SodiumException("sodium_crypto_ipcrypt_pfx_decrypt(): Argument #2 (\$key) must be SODIUM_CRYPTO_IPCRYPT_PFX_KEYBYTES bytes long");
    }
    $binary = __elephc_sodium_ip_bin($encrypted_ip, "sodium_crypto_ipcrypt_pfx_decrypt(): Argument #1 (\$encrypted_ip) must be a valid IP address");
    $pointer = \elephc_sodium_ipcrypt_pfx_decrypt($binary, \strlen($binary), $key, \strlen($key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_ipcrypt_pfx_decrypt(): internal error");
    }
    return \sodium_bin2ip(\__elephc_ptr_read_string($pointer, \elephc_sodium_last_len()));
}

function __elephc_sodium_ip_bin(string $ip, string $message): string
{
    $pointer = \elephc_sodium_ip2bin($ip, \strlen($ip));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException($message);
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function __elephc_sodium_ciphertext(string $hex, int $bytes, string $message): string
{
    if (\strlen($hex) !== $bytes * 2) {
        throw new \SodiumException($message);
    }
    $binary = @\hex2bin($hex);
    if (!\is_string($binary)) {
        throw new \SodiumException($message);
    }
    return $binary;
}

function sodium_crypto_xof_turboshake128(int $length, string $message): string
{
    if ($length <= 0) {
        throw new \SodiumException("sodium_crypto_xof_turboshake128(): Argument #1 (\$length) must be greater than 0");
    }
    $pointer = \elephc_sodium_turboshake128($length, $message, \strlen($message));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_xof_turboshake128(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_xof_turboshake256(int $length, string $message): string
{
    if ($length <= 0) {
        throw new \SodiumException("sodium_crypto_xof_turboshake256(): Argument #1 (\$length) must be greater than 0");
    }
    $pointer = \elephc_sodium_turboshake256($length, $message, \strlen($message));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_xof_turboshake256(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_xof_turboshake128_init(?int $domain = null): SodiumCryptoXofTurboshake128State
{
    if ($domain !== null && ($domain < 0x01 || $domain > 0x7f)) {
        throw new \SodiumException("sodium_crypto_xof_turboshake128_init(): Argument #1 (\$domain) must be between 0x01 and 0x7f");
    }
    $handle = \elephc_sodium_turboshake128_init($domain === null ? -1 : $domain);
    if ($handle < 0) {
        throw new \SodiumException("sodium_crypto_xof_turboshake128_init(): internal error");
    }
    return new SodiumCryptoXofTurboshake128State($handle);
}

function sodium_crypto_xof_turboshake128_update(SodiumCryptoXofTurboshake128State $state, string $message): void
{
    $result = \elephc_sodium_turboshake128_update($state->__elephc_handle(), $message, \strlen($message));
    if ($result !== 0) {
        throw new \SodiumException("sodium_crypto_xof_turboshake128_update(): internal error");
    }
}

function sodium_crypto_xof_turboshake128_squeeze(SodiumCryptoXofTurboshake128State $state, int $length): string
{
    if ($length <= 0) {
        throw new \SodiumException("sodium_crypto_xof_turboshake128_squeeze(): Argument #2 (\$length) must be greater than 0");
    }
    $pointer = \elephc_sodium_turboshake128_squeeze($state->__elephc_handle(), $length);
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_xof_turboshake128_squeeze(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_xof_turboshake256_init(?int $domain = null): SodiumCryptoXofTurboshake256State
{
    if ($domain !== null && ($domain < 0x01 || $domain > 0x7f)) {
        throw new \SodiumException("sodium_crypto_xof_turboshake256_init(): Argument #1 (\$domain) must be between 0x01 and 0x7f");
    }
    $handle = \elephc_sodium_turboshake256_init($domain === null ? -1 : $domain);
    if ($handle < 0) {
        throw new \SodiumException("sodium_crypto_xof_turboshake256_init(): internal error");
    }
    return new SodiumCryptoXofTurboshake256State($handle);
}

function sodium_crypto_xof_turboshake256_update(SodiumCryptoXofTurboshake256State $state, string $message): void
{
    $result = \elephc_sodium_turboshake256_update($state->__elephc_handle(), $message, \strlen($message));
    if ($result !== 0) {
        throw new \SodiumException("sodium_crypto_xof_turboshake256_update(): internal error");
    }
}

function sodium_crypto_xof_turboshake256_squeeze(SodiumCryptoXofTurboshake256State $state, int $length): string
{
    if ($length <= 0) {
        throw new \SodiumException("sodium_crypto_xof_turboshake256_squeeze(): Argument #2 (\$length) must be greater than 0");
    }
    $pointer = \elephc_sodium_turboshake256_squeeze($state->__elephc_handle(), $length);
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_xof_turboshake256_squeeze(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_kem_keypair(): string
{
    $pointer = \elephc_sodium_kem_keypair();
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_kem_keypair(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_kem_seed_keypair(string $seed): string
{
    if (\strlen($seed) !== SODIUM_CRYPTO_KEM_SEEDBYTES) {
        throw new \SodiumException("sodium_crypto_kem_seed_keypair(): Argument #1 (\$seed) must be SODIUM_CRYPTO_KEM_SEEDBYTES bytes long");
    }
    $pointer = \elephc_sodium_kem_seed_keypair($seed, \strlen($seed));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_kem_seed_keypair(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_kem_secretkey(string $key_pair): string
{
    if (\strlen($key_pair) !== SODIUM_CRYPTO_KEM_KEYPAIRBYTES) {
        throw new \SodiumException("sodium_crypto_kem_secretkey(): Argument #1 (\$key_pair) must be SODIUM_CRYPTO_KEM_KEYPAIRBYTES bytes long");
    }
    return \substr($key_pair, 0, SODIUM_CRYPTO_KEM_SECRETKEYBYTES);
}

function sodium_crypto_kem_publickey(string $key_pair): string
{
    if (\strlen($key_pair) !== SODIUM_CRYPTO_KEM_KEYPAIRBYTES) {
        throw new \SodiumException("sodium_crypto_kem_publickey(): Argument #1 (\$key_pair) must be SODIUM_CRYPTO_KEM_KEYPAIRBYTES bytes long");
    }
    return \substr($key_pair, SODIUM_CRYPTO_KEM_SECRETKEYBYTES);
}

function sodium_crypto_kem_enc(string $public_key): array
{
    if (\strlen($public_key) !== SODIUM_CRYPTO_KEM_PUBLICKEYBYTES) {
        throw new \SodiumException("sodium_crypto_kem_enc(): Argument #1 (\$public_key) must be SODIUM_CRYPTO_KEM_PUBLICKEYBYTES bytes long");
    }
    $pointer = \elephc_sodium_kem_enc($public_key, \strlen($public_key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_kem_enc(): internal error");
    }
    $pair = \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
    return [
        \substr($pair, 0, SODIUM_CRYPTO_KEM_CIPHERTEXTBYTES),
        \substr($pair, SODIUM_CRYPTO_KEM_CIPHERTEXTBYTES),
    ];
}

function sodium_crypto_kem_dec(string $ciphertext, string $secret_key): string
{
    if (\strlen($ciphertext) !== SODIUM_CRYPTO_KEM_CIPHERTEXTBYTES) {
        throw new \SodiumException("sodium_crypto_kem_dec(): Argument #1 (\$ciphertext) must be SODIUM_CRYPTO_KEM_CIPHERTEXTBYTES bytes long");
    }
    if (\strlen($secret_key) !== SODIUM_CRYPTO_KEM_SECRETKEYBYTES) {
        throw new \SodiumException("sodium_crypto_kem_dec(): Argument #2 (\$secret_key) must be SODIUM_CRYPTO_KEM_SECRETKEYBYTES bytes long");
    }
    $pointer = \elephc_sodium_kem_dec($ciphertext, \strlen($ciphertext), $secret_key, \strlen($secret_key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_kem_dec(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_kem_mlkem768_keypair(): string
{
    $pointer = \elephc_sodium_kem_mlkem768_keypair();
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_kem_mlkem768_keypair(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_kem_mlkem768_seed_keypair(string $seed): string
{
    if (\strlen($seed) !== SODIUM_CRYPTO_KEM_MLKEM768_SEEDBYTES) {
        throw new \SodiumException("sodium_crypto_kem_mlkem768_seed_keypair(): Argument #1 (\$seed) must be SODIUM_CRYPTO_KEM_MLKEM768_SEEDBYTES bytes long");
    }
    $pointer = \elephc_sodium_kem_mlkem768_seed_keypair($seed, \strlen($seed));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_kem_mlkem768_seed_keypair(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_kem_mlkem768_secretkey(string $key_pair): string
{
    if (\strlen($key_pair) !== SODIUM_CRYPTO_KEM_MLKEM768_KEYPAIRBYTES) {
        throw new \SodiumException("sodium_crypto_kem_mlkem768_secretkey(): Argument #1 (\$key_pair) must be SODIUM_CRYPTO_KEM_MLKEM768_KEYPAIRBYTES bytes long");
    }
    return \substr($key_pair, 0, SODIUM_CRYPTO_KEM_MLKEM768_SECRETKEYBYTES);
}

function sodium_crypto_kem_mlkem768_publickey(string $key_pair): string
{
    if (\strlen($key_pair) !== SODIUM_CRYPTO_KEM_MLKEM768_KEYPAIRBYTES) {
        throw new \SodiumException("sodium_crypto_kem_mlkem768_publickey(): Argument #1 (\$key_pair) must be SODIUM_CRYPTO_KEM_MLKEM768_KEYPAIRBYTES bytes long");
    }
    return \substr($key_pair, SODIUM_CRYPTO_KEM_MLKEM768_SECRETKEYBYTES);
}

function sodium_crypto_kem_mlkem768_enc(string $public_key): array
{
    if (\strlen($public_key) !== SODIUM_CRYPTO_KEM_MLKEM768_PUBLICKEYBYTES) {
        throw new \SodiumException("sodium_crypto_kem_mlkem768_enc(): Argument #1 (\$public_key) must be SODIUM_CRYPTO_KEM_MLKEM768_PUBLICKEYBYTES bytes long");
    }
    $pointer = \elephc_sodium_kem_mlkem768_enc($public_key, \strlen($public_key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_kem_mlkem768_enc(): internal error");
    }
    $pair = \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
    return [
        \substr($pair, 0, SODIUM_CRYPTO_KEM_MLKEM768_CIPHERTEXTBYTES),
        \substr($pair, SODIUM_CRYPTO_KEM_MLKEM768_CIPHERTEXTBYTES),
    ];
}

function sodium_crypto_kem_mlkem768_dec(string $ciphertext, string $secret_key): string
{
    if (\strlen($ciphertext) !== SODIUM_CRYPTO_KEM_MLKEM768_CIPHERTEXTBYTES) {
        throw new \SodiumException("sodium_crypto_kem_mlkem768_dec(): Argument #1 (\$ciphertext) must be SODIUM_CRYPTO_KEM_MLKEM768_CIPHERTEXTBYTES bytes long");
    }
    if (\strlen($secret_key) !== SODIUM_CRYPTO_KEM_MLKEM768_SECRETKEYBYTES) {
        throw new \SodiumException("sodium_crypto_kem_mlkem768_dec(): Argument #2 (\$secret_key) must be SODIUM_CRYPTO_KEM_MLKEM768_SECRETKEYBYTES bytes long");
    }
    $pointer = \elephc_sodium_kem_mlkem768_dec($ciphertext, \strlen($ciphertext), $secret_key, \strlen($secret_key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_kem_mlkem768_dec(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_pwhash(int $length, string $password, string $salt, int $opslimit, int $memlimit, int $algorithm = SODIUM_CRYPTO_PWHASH_ALG_DEFAULT): string
{
    return __elephc_sodium_pwhash("sodium_crypto_pwhash", $length, $password, $salt, $opslimit, $memlimit, $algorithm);
}

function sodium_crypto_pwhash_argon2id(int $length, string $password, string $salt, int $opslimit, int $memlimit): string
{
    return __elephc_sodium_pwhash("sodium_crypto_pwhash_argon2id", $length, $password, $salt, $opslimit, $memlimit, SODIUM_CRYPTO_PWHASH_ALG_ARGON2ID13);
}

function sodium_crypto_pwhash_argon2i(int $length, string $password, string $salt, int $opslimit, int $memlimit): string
{
    return __elephc_sodium_pwhash("sodium_crypto_pwhash_argon2i", $length, $password, $salt, $opslimit, $memlimit, SODIUM_CRYPTO_PWHASH_ALG_ARGON2I13);
}

function sodium_crypto_pwhash_str(string $password, int $opslimit, int $memlimit): string
{
    return __elephc_sodium_pwhash_str("sodium_crypto_pwhash_str", $password, $opslimit, $memlimit, SODIUM_CRYPTO_PWHASH_ALG_ARGON2ID13);
}

function sodium_crypto_pwhash_argon2id_str(string $password, int $opslimit, int $memlimit): string
{
    return __elephc_sodium_pwhash_str("sodium_crypto_pwhash_argon2id_str", $password, $opslimit, $memlimit, SODIUM_CRYPTO_PWHASH_ALG_ARGON2ID13);
}

function sodium_crypto_pwhash_argon2i_str(string $password, int $opslimit, int $memlimit): string
{
    return __elephc_sodium_pwhash_str("sodium_crypto_pwhash_argon2i_str", $password, $opslimit, $memlimit, SODIUM_CRYPTO_PWHASH_ALG_ARGON2I13);
}

function sodium_crypto_pwhash_str_verify(string $hash, string $password): bool
{
    if (\strlen($password) >= 4294967295) {
        throw new \SodiumException("sodium_crypto_pwhash_str_verify(): Argument #2 (\$password) is too long");
    }
    $result = \elephc_sodium_pwhash_str_verify($password, \strlen($password), $hash, \strlen($hash));
    return $result === 1;
}

function sodium_crypto_pwhash_argon2id_str_verify(string $hash, string $password): bool
{
    if (\substr($hash, 0, 10) !== '$argon2id$') {
        return false;
    }
    return \sodium_crypto_pwhash_str_verify($hash, $password);
}

function sodium_crypto_pwhash_argon2i_str_verify(string $hash, string $password): bool
{
    if (\substr($hash, 0, 9) !== '$argon2i$') {
        return false;
    }
    return \sodium_crypto_pwhash_str_verify($hash, $password);
}

function __elephc_sodium_pwhash(string $name, int $length, string $password, string $salt, int $opslimit, int $memlimit, int $algorithm): string
{
    if ($length <= 0) {
        throw new \ValueError($name . "(): Argument #1 (\$length) must be greater than 0");
    }
    if ($length >= 4294967295) {
        throw new \ValueError($name . "(): Argument #1 (\$length) must be less than 4294967295 bytes");
    }
    if (\strlen($password) >= 4294967295) {
        throw new \ValueError($name . "(): Argument #2 (\$password) must be less than 4294967295 bytes");
    }
    if ($opslimit <= 0) {
        throw new \ValueError($name . "(): Argument #4 (\$opslimit) must be greater than 0");
    }
    if ($memlimit <= 0) {
        throw new \ValueError($name . "(): Argument #5 (\$memlimit) must be greater than 0");
    }
    if ($algorithm !== SODIUM_CRYPTO_PWHASH_ALG_ARGON2I13 && $algorithm !== SODIUM_CRYPTO_PWHASH_ALG_ARGON2ID13) {
        throw new \SodiumException("unsupported password hashing algorithm");
    }
    if (\strlen($salt) !== SODIUM_CRYPTO_PWHASH_SALTBYTES) {
        throw new \ValueError($name . "(): Argument #3 (\$salt) must be SODIUM_CRYPTO_PWHASH_SALTBYTES bytes long");
    }
    if ($opslimit < SODIUM_CRYPTO_PWHASH_OPSLIMIT_MIN) {
        throw new \ValueError($name . "(): Argument #4 (\$opslimit) must be greater than or equal to " . SODIUM_CRYPTO_PWHASH_OPSLIMIT_MIN);
    }
    if ($memlimit < SODIUM_CRYPTO_PWHASH_MEMLIMIT_MIN) {
        throw new \ValueError($name . "(): Argument #5 (\$memlimit) must be greater than or equal to " . SODIUM_CRYPTO_PWHASH_MEMLIMIT_MIN);
    }
    $pointer = \elephc_sodium_pwhash($length, $password, \strlen($password), $salt, \strlen($salt), $opslimit, $memlimit, $algorithm);
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException($name . "(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function __elephc_sodium_pwhash_str(string $name, string $password, int $opslimit, int $memlimit, int $algorithm): string
{
    if ($opslimit <= 0) {
        throw new \ValueError($name . "(): Argument #2 (\$opslimit) must be greater than 0");
    }
    if ($memlimit <= 0) {
        throw new \ValueError($name . "(): Argument #3 (\$memlimit) must be greater than 0");
    }
    if (\strlen($password) >= 4294967295) {
        throw new \ValueError($name . "(): Argument #1 (\$password) must be less than 4294967295 bytes");
    }
    if ($opslimit < SODIUM_CRYPTO_PWHASH_OPSLIMIT_MIN) {
        throw new \ValueError($name . "(): Argument #2 (\$opslimit) must be greater than or equal to " . SODIUM_CRYPTO_PWHASH_OPSLIMIT_MIN);
    }
    if ($memlimit < SODIUM_CRYPTO_PWHASH_MEMLIMIT_MIN) {
        throw new \ValueError($name . "(): Argument #3 (\$memlimit) must be greater than or equal to " . SODIUM_CRYPTO_PWHASH_MEMLIMIT_MIN);
    }
    $salt = \random_bytes(SODIUM_CRYPTO_PWHASH_SALTBYTES);
    $pointer = \elephc_sodium_pwhash_str($password, \strlen($password), $salt, \strlen($salt), $opslimit, $memlimit, $algorithm);
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException($name . "(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_pwhash_scryptsalsa208sha256(int $length, string $password, string $salt, int $opslimit, int $memlimit): string
{
    if ($length <= 0 || $length > 0x1fffffffe0) {
        throw new \ValueError("sodium_crypto_pwhash_scryptsalsa208sha256(): Argument #1 (\$length) must be greater than 0");
    }
    if ($opslimit <= 0) {
        throw new \ValueError("sodium_crypto_pwhash_scryptsalsa208sha256(): Argument #4 (\$opslimit) must be greater than 0");
    }
    if ($memlimit <= 0) {
        throw new \ValueError("sodium_crypto_pwhash_scryptsalsa208sha256(): Argument #5 (\$memlimit) must be greater than 0");
    }
    if (\strlen($salt) !== SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_SALTBYTES) {
        throw new \ValueError("sodium_crypto_pwhash_scryptsalsa208sha256(): Argument #3 (\$salt) must be SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_SALTBYTES bytes long");
    }
    if ($opslimit < SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_OPSLIMIT_INTERACTIVE) {
        throw new \ValueError("sodium_crypto_pwhash_scryptsalsa208sha256(): Argument #4 (\$opslimit) must be greater than or equal to " . SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_OPSLIMIT_INTERACTIVE);
    }
    if ($memlimit < SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_MEMLIMIT_INTERACTIVE) {
        throw new \ValueError("sodium_crypto_pwhash_scryptsalsa208sha256(): Argument #5 (\$memlimit) must be greater than or equal to " . SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_MEMLIMIT_INTERACTIVE);
    }
    $pointer = \elephc_sodium_pwhash_scryptsalsa208sha256($length, $password, \strlen($password), $salt, \strlen($salt), $opslimit, $memlimit);
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_pwhash_scryptsalsa208sha256(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_pwhash_scryptsalsa208sha256_str(string $password, int $opslimit, int $memlimit): string
{
    if ($opslimit <= 0) {
        throw new \ValueError("sodium_crypto_pwhash_scryptsalsa208sha256_str(): Argument #2 (\$opslimit) must be greater than 0");
    }
    if ($memlimit <= 0) {
        throw new \ValueError("sodium_crypto_pwhash_scryptsalsa208sha256_str(): Argument #3 (\$memlimit) must be greater than 0");
    }
    if ($opslimit < SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_OPSLIMIT_INTERACTIVE) {
        throw new \ValueError("sodium_crypto_pwhash_scryptsalsa208sha256_str(): Argument #2 (\$opslimit) must be greater than or equal to " . SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_OPSLIMIT_INTERACTIVE);
    }
    if ($memlimit < SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_MEMLIMIT_INTERACTIVE) {
        throw new \ValueError("sodium_crypto_pwhash_scryptsalsa208sha256_str(): Argument #3 (\$memlimit) must be greater than or equal to " . SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_MEMLIMIT_INTERACTIVE);
    }
    $salt = \random_bytes(SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_SALTBYTES);
    $pointer = \elephc_sodium_pwhash_scryptsalsa208sha256_str($password, \strlen($password), $salt, \strlen($salt), $opslimit, $memlimit);
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_pwhash_scryptsalsa208sha256_str(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_pwhash_scryptsalsa208sha256_str_verify(string $hash, string $password): bool
{
    if (\strlen($hash) !== SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_STRBYTES - 1) {
        return false;
    }
    $result = \elephc_sodium_pwhash_scryptsalsa208sha256_str_verify($password, \strlen($password), $hash, \strlen($hash));
    return $result === 1;
}
