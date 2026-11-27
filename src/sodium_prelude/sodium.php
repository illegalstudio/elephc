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


const SODIUM_CRYPTO_GENERICHASH_BYTES = 32;
const SODIUM_CRYPTO_GENERICHASH_BYTES_MIN = 16;
const SODIUM_CRYPTO_GENERICHASH_BYTES_MAX = 64;
const SODIUM_CRYPTO_GENERICHASH_KEYBYTES = 32;
const SODIUM_CRYPTO_GENERICHASH_KEYBYTES_MIN = 16;
const SODIUM_CRYPTO_GENERICHASH_KEYBYTES_MAX = 64;
const SODIUM_CRYPTO_AUTH_BYTES = 32;
const SODIUM_CRYPTO_AUTH_KEYBYTES = 32;
const SODIUM_CRYPTO_KDF_BYTES_MIN = 16;
const SODIUM_CRYPTO_KDF_BYTES_MAX = 64;
const SODIUM_CRYPTO_KDF_CONTEXTBYTES = 8;
const SODIUM_CRYPTO_KDF_KEYBYTES = 32;
const SODIUM_CRYPTO_SIGN_BYTES = 64;
const SODIUM_CRYPTO_SIGN_KEYPAIRBYTES = 96;
const SODIUM_CRYPTO_SIGN_PUBLICKEYBYTES = 32;
const SODIUM_CRYPTO_SIGN_SECRETKEYBYTES = 64;
const SODIUM_CRYPTO_SIGN_SEEDBYTES = 32;
const SODIUM_CRYPTO_BOX_KEYPAIRBYTES = 64;
const SODIUM_CRYPTO_BOX_MACBYTES = 16;
const SODIUM_CRYPTO_BOX_NONCEBYTES = 24;
const SODIUM_CRYPTO_BOX_PUBLICKEYBYTES = 32;
const SODIUM_CRYPTO_BOX_SEALBYTES = 48;
const SODIUM_CRYPTO_BOX_SECRETKEYBYTES = 32;
const SODIUM_CRYPTO_BOX_SEEDBYTES = 32;
const SODIUM_CRYPTO_KX_KEYPAIRBYTES = 64;
const SODIUM_CRYPTO_KX_PUBLICKEYBYTES = 32;
const SODIUM_CRYPTO_KX_SECRETKEYBYTES = 32;
const SODIUM_CRYPTO_KX_SEEDBYTES = 32;
const SODIUM_CRYPTO_KX_SESSIONKEYBYTES = 32;
const SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_ABYTES = 16;
const SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_KEYBYTES = 32;
const SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_NPUBBYTES = 8;
const SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_NSECBYTES = 0;
const SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_IETF_ABYTES = 16;
const SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_IETF_KEYBYTES = 32;
const SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_IETF_NPUBBYTES = 12;
const SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_IETF_NSECBYTES = 0;
const SODIUM_CRYPTO_AEAD_XCHACHA20POLY1305_IETF_ABYTES = 16;
const SODIUM_CRYPTO_AEAD_XCHACHA20POLY1305_IETF_KEYBYTES = 32;
const SODIUM_CRYPTO_AEAD_XCHACHA20POLY1305_IETF_NPUBBYTES = 24;
const SODIUM_CRYPTO_AEAD_XCHACHA20POLY1305_IETF_NSECBYTES = 0;
const SODIUM_CRYPTO_AEAD_AES256GCM_ABYTES = 16;
const SODIUM_CRYPTO_AEAD_AES256GCM_KEYBYTES = 32;
const SODIUM_CRYPTO_AEAD_AES256GCM_NPUBBYTES = 12;
const SODIUM_CRYPTO_AEAD_AES256GCM_NSECBYTES = 0;
const SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_ABYTES = 17;
const SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_HEADERBYTES = 24;
const SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_KEYBYTES = 32;
const SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_TAG_MESSAGE = 0;
const SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_TAG_PUSH = 1;
const SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_TAG_REKEY = 2;
const SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_TAG_FINAL = 3;
const SODIUM_CRYPTO_STREAM_KEYBYTES = 32;
const SODIUM_CRYPTO_STREAM_NONCEBYTES = 24;
const SODIUM_CRYPTO_STREAM_XCHACHA20_KEYBYTES = 32;
const SODIUM_CRYPTO_STREAM_XCHACHA20_NONCEBYTES = 24;
const SODIUM_CRYPTO_SCALARMULT_BYTES = 32;
const SODIUM_CRYPTO_SCALARMULT_SCALARBYTES = 32;
const SODIUM_CRYPTO_SHORTHASH_BYTES = 8;
const SODIUM_CRYPTO_SHORTHASH_KEYBYTES = 16;
const SODIUM_BASE64_VARIANT_ORIGINAL = 1;
const SODIUM_BASE64_VARIANT_ORIGINAL_NO_PADDING = 3;
const SODIUM_BASE64_VARIANT_URLSAFE = 5;
const SODIUM_BASE64_VARIANT_URLSAFE_NO_PADDING = 7;

const SODIUM_CRYPTO_CORE_RISTRETTO255_BYTES = 32;
const SODIUM_CRYPTO_CORE_RISTRETTO255_HASHBYTES = 64;
const SODIUM_CRYPTO_CORE_RISTRETTO255_NONREDUCEDSCALARBYTES = 64;
const SODIUM_CRYPTO_CORE_RISTRETTO255_SCALARBYTES = 32;
const SODIUM_CRYPTO_SCALARMULT_RISTRETTO255_BYTES = 32;
const SODIUM_CRYPTO_SCALARMULT_RISTRETTO255_SCALARBYTES = 32;

const SODIUM_CRYPTO_AEAD_AEGIS128L_ABYTES = 32;
const SODIUM_CRYPTO_AEAD_AEGIS128L_KEYBYTES = 16;
const SODIUM_CRYPTO_AEAD_AEGIS128L_NPUBBYTES = 16;
const SODIUM_CRYPTO_AEAD_AEGIS128L_NSECBYTES = 0;
const SODIUM_CRYPTO_AEAD_AEGIS256_ABYTES = 32;
const SODIUM_CRYPTO_AEAD_AEGIS256_KEYBYTES = 32;
const SODIUM_CRYPTO_AEAD_AEGIS256_NPUBBYTES = 32;
const SODIUM_CRYPTO_AEAD_AEGIS256_NSECBYTES = 0;

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
    function elephc_sodium_generichash(string $message, int $messageLength, string $key, int $keyLength, int $outputLength): ptr;
    function elephc_sodium_generichash_init(string $key, int $keyLength, int $outputLength): ptr;
    function elephc_sodium_generichash_update(string $state, int $stateLength, string $message, int $messageLength): ptr;
    function elephc_sodium_generichash_final(string $state, int $stateLength, int $outputLength): ptr;
    function elephc_sodium_auth(string $message, int $messageLength, string $key, int $keyLength): ptr;
    function elephc_sodium_auth_verify(string $mac, int $macLength, string $message, int $messageLength, string $key, int $keyLength): int;
    function elephc_sodium_kdf(int $subkeyLength, int $subkeyId, string $context, int $contextLength, string $key, int $keyLength): ptr;
    function elephc_sodium_shorthash(string $message, int $messageLength, string $key, int $keyLength): ptr;
    function elephc_sodium_sign_seed_keypair(string $seed, int $seedLength): ptr;
    function elephc_sodium_sign_publickey_from_secretkey(string $secretKey, int $secretKeyLength): ptr;
    function elephc_sodium_sign_detached(string $message, int $messageLength, string $secretKey, int $secretKeyLength): ptr;
    function elephc_sodium_sign(string $message, int $messageLength, string $secretKey, int $secretKeyLength): ptr;
    function elephc_sodium_sign_open(string $signedMessage, int $signedMessageLength, string $publicKey, int $publicKeyLength): ptr;
    function elephc_sodium_sign_verify_detached(string $signature, int $signatureLength, string $message, int $messageLength, string $publicKey, int $publicKeyLength): int;
    function elephc_sodium_sign_pk_to_curve25519(string $publicKey, int $publicKeyLength): ptr;
    function elephc_sodium_sign_sk_to_curve25519(string $secretKey, int $secretKeyLength): ptr;
    function elephc_sodium_box_seed_keypair(string $seed, int $seedLength): ptr;
    function elephc_sodium_box_publickey_from_secretkey(string $secretKey, int $secretKeyLength): ptr;
    function elephc_sodium_box_encrypt(string $message, int $messageLength, string $nonce, int $nonceLength, string $keyPair, int $keyPairLength): ptr;
    function elephc_sodium_box_decrypt(string $ciphertext, int $ciphertextLength, string $nonce, int $nonceLength, string $keyPair, int $keyPairLength): ptr;
    function elephc_sodium_scalarmult(string $secretKey, int $secretKeyLength, string $publicKey, int $publicKeyLength): ptr;
    function elephc_sodium_scalarmult_base(string $secretKey, int $secretKeyLength): ptr;
    function elephc_sodium_kx_seed_keypair(string $seed, int $seedLength): ptr;
    function elephc_sodium_kx_client_session_keys(string $keyPair, int $keyPairLength, string $serverKey, int $serverKeyLength): ptr;
    function elephc_sodium_kx_server_session_keys(string $keyPair, int $keyPairLength, string $clientKey, int $clientKeyLength): ptr;
    function elephc_sodium_aead_chacha20poly1305_encrypt(string $message, int $messageLength, string $aad, int $aadLength, string $nonce, int $nonceLength, string $key, int $keyLength): ptr;
    function elephc_sodium_aead_chacha20poly1305_decrypt(string $ciphertext, int $ciphertextLength, string $aad, int $aadLength, string $nonce, int $nonceLength, string $key, int $keyLength): ptr;
    function elephc_sodium_aead_chacha20poly1305_ietf_encrypt(string $message, int $messageLength, string $aad, int $aadLength, string $nonce, int $nonceLength, string $key, int $keyLength): ptr;
    function elephc_sodium_aead_chacha20poly1305_ietf_decrypt(string $ciphertext, int $ciphertextLength, string $aad, int $aadLength, string $nonce, int $nonceLength, string $key, int $keyLength): ptr;
    function elephc_sodium_aead_xchacha20poly1305_ietf_encrypt(string $message, int $messageLength, string $aad, int $aadLength, string $nonce, int $nonceLength, string $key, int $keyLength): ptr;
    function elephc_sodium_aead_xchacha20poly1305_ietf_decrypt(string $ciphertext, int $ciphertextLength, string $aad, int $aadLength, string $nonce, int $nonceLength, string $key, int $keyLength): ptr;
    function elephc_sodium_aead_aes256gcm_encrypt(string $message, int $messageLength, string $aad, int $aadLength, string $nonce, int $nonceLength, string $key, int $keyLength): ptr;
    function elephc_sodium_aead_aes256gcm_decrypt(string $ciphertext, int $ciphertextLength, string $aad, int $aadLength, string $nonce, int $nonceLength, string $key, int $keyLength): ptr;
    function elephc_sodium_secretstream_init_push(string $key, int $keyLength, string $header, int $headerLength): ptr;
    function elephc_sodium_secretstream_init_pull(string $header, int $headerLength, string $key, int $keyLength): ptr;
    function elephc_sodium_secretstream_push(string $state, int $stateLength, string $message, int $messageLength, string $aad, int $aadLength, int $tag): ptr;
    function elephc_sodium_secretstream_pull(string $state, int $stateLength, string $ciphertext, int $ciphertextLength, string $aad, int $aadLength): ptr;
    function elephc_sodium_secretstream_rekey(string $state, int $stateLength): ptr;
    function elephc_sodium_stream(int $length, string $nonce, int $nonceLength, string $key, int $keyLength): ptr;
    function elephc_sodium_stream_xor(string $message, int $messageLength, string $nonce, int $nonceLength, string $key, int $keyLength): ptr;
    function elephc_sodium_stream_xchacha20(int $length, string $nonce, int $nonceLength, string $key, int $keyLength): ptr;
    function elephc_sodium_stream_xchacha20_xor(string $message, int $messageLength, string $nonce, int $nonceLength, string $key, int $keyLength): ptr;
    function elephc_sodium_stream_xchacha20_xor_ic(string $message, int $messageLength, string $nonce, int $nonceLength, int $counter, string $key, int $keyLength): ptr;
    function elephc_sodium_ristretto255_from_hash(string $input, int $inputLength): ptr;
    function elephc_sodium_ristretto255_is_valid_point(string $input, int $inputLength): int;
    function elephc_sodium_ristretto255_add(string $left, int $leftLength, string $right, int $rightLength): ptr;
    function elephc_sodium_ristretto255_sub(string $left, int $leftLength, string $right, int $rightLength): ptr;
    function elephc_sodium_ristretto255_scalar_reduce(string $input, int $inputLength): ptr;
    function elephc_sodium_ristretto255_scalar_add(string $left, int $leftLength, string $right, int $rightLength): ptr;
    function elephc_sodium_ristretto255_scalar_sub(string $left, int $leftLength, string $right, int $rightLength): ptr;
    function elephc_sodium_ristretto255_scalar_mul(string $left, int $leftLength, string $right, int $rightLength): ptr;
    function elephc_sodium_ristretto255_scalar_negate(string $input, int $inputLength): ptr;
    function elephc_sodium_ristretto255_scalar_complement(string $input, int $inputLength): ptr;
    function elephc_sodium_ristretto255_scalar_invert(string $input, int $inputLength): ptr;
    function elephc_sodium_ristretto255_scalarmult(string $scalar, int $scalarLength, string $point, int $pointLength): ptr;
    function elephc_sodium_ristretto255_scalarmult_base(string $scalar, int $scalarLength): ptr;
    function elephc_sodium_aead_aegis128l_encrypt(string $message, int $messageLength, string $aad, int $aadLength, string $nonce, int $nonceLength, string $key, int $keyLength): ptr;
    function elephc_sodium_aead_aegis128l_decrypt(string $ciphertext, int $ciphertextLength, string $aad, int $aadLength, string $nonce, int $nonceLength, string $key, int $keyLength): ptr;
    function elephc_sodium_aead_aegis256_encrypt(string $message, int $messageLength, string $aad, int $aadLength, string $nonce, int $nonceLength, string $key, int $keyLength): ptr;
    function elephc_sodium_aead_aegis256_decrypt(string $ciphertext, int $ciphertextLength, string $aad, int $aadLength, string $nonce, int $nonceLength, string $key, int $keyLength): ptr;
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

function sodium_crypto_generichash(string $message, string $key = "", int $length = SODIUM_CRYPTO_GENERICHASH_BYTES): string
{
    if ($length < SODIUM_CRYPTO_GENERICHASH_BYTES_MIN || $length > SODIUM_CRYPTO_GENERICHASH_BYTES_MAX) {
        throw new \SodiumException("unsupported output length");
    }
    $keyLength = \strlen($key);
    if ($keyLength !== 0 && ($keyLength < SODIUM_CRYPTO_GENERICHASH_KEYBYTES_MIN || $keyLength > SODIUM_CRYPTO_GENERICHASH_KEYBYTES_MAX)) {
        throw new \SodiumException("unsupported key length");
    }
    $pointer = \elephc_sodium_generichash($message, \strlen($message), $key, $keyLength, $length);
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_generichash(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_generichash_init(string $key = "", int $length = SODIUM_CRYPTO_GENERICHASH_BYTES): string
{
    if ($length < SODIUM_CRYPTO_GENERICHASH_BYTES_MIN || $length > SODIUM_CRYPTO_GENERICHASH_BYTES_MAX) {
        throw new \SodiumException("unsupported output length");
    }
    $keyLength = \strlen($key);
    if ($keyLength !== 0 && ($keyLength < SODIUM_CRYPTO_GENERICHASH_KEYBYTES_MIN || $keyLength > SODIUM_CRYPTO_GENERICHASH_KEYBYTES_MAX)) {
        throw new \SodiumException("unsupported key length");
    }
    $pointer = \elephc_sodium_generichash_init($key, $keyLength, $length);
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_generichash_init(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_generichash_update(string &$state, string $message): bool
{
    $pointer = \elephc_sodium_generichash_update($state, \strlen($state), $message, \strlen($message));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_generichash_update(): internal error");
    }
    $state = \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
    return true;
}

function sodium_crypto_generichash_final(string &$state, int $length = SODIUM_CRYPTO_GENERICHASH_BYTES): string
{
    if ($length < SODIUM_CRYPTO_GENERICHASH_BYTES_MIN || $length > SODIUM_CRYPTO_GENERICHASH_BYTES_MAX) {
        throw new \SodiumException("unsupported output length");
    }
    $pointer = \elephc_sodium_generichash_final($state, \strlen($state), $length);
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_generichash_final(): internal error");
    }
    $output = \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
    $state = "";
    return $output;
}

function sodium_crypto_generichash_keygen(): string
{
    return \random_bytes(SODIUM_CRYPTO_GENERICHASH_KEYBYTES);
}

function sodium_crypto_auth(string $message, string $key): string
{
    if (\strlen($key) !== SODIUM_CRYPTO_AUTH_KEYBYTES) {
        throw new \SodiumException("sodium_crypto_auth(): Argument #2 (\$key) must be SODIUM_CRYPTO_AUTH_KEYBYTES bytes long");
    }
    $pointer = \elephc_sodium_auth($message, \strlen($message), $key, \strlen($key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_auth(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_auth_verify(string $mac, string $message, string $key): bool
{
    if (\strlen($key) !== SODIUM_CRYPTO_AUTH_KEYBYTES) {
        throw new \SodiumException("sodium_crypto_auth_verify(): Argument #3 (\$key) must be SODIUM_CRYPTO_AUTH_KEYBYTES bytes long");
    }
    if (\strlen($mac) !== SODIUM_CRYPTO_AUTH_BYTES) {
        return false;
    }
    return \elephc_sodium_auth_verify($mac, \strlen($mac), $message, \strlen($message), $key, \strlen($key)) === 1;
}

function sodium_crypto_auth_keygen(): string
{
    return \random_bytes(SODIUM_CRYPTO_AUTH_KEYBYTES);
}

function sodium_crypto_kdf_derive_from_key(int $subkey_length, int $subkey_id, string $context, string $key): string
{
    if ($subkey_length < SODIUM_CRYPTO_KDF_BYTES_MIN) {
        throw new \SodiumException("sodium_crypto_kdf_derive_from_key(): Argument #1 (\$subkey_length) must be greater than or equal to SODIUM_CRYPTO_KDF_BYTES_MIN");
    }
    if ($subkey_length > SODIUM_CRYPTO_KDF_BYTES_MAX) {
        throw new \SodiumException("sodium_crypto_kdf_derive_from_key(): Argument #1 (\$subkey_length) must be less than or equal to SODIUM_CRYPTO_KDF_BYTES_MAX");
    }
    if (\strlen($context) !== SODIUM_CRYPTO_KDF_CONTEXTBYTES) {
        throw new \SodiumException("sodium_crypto_kdf_derive_from_key(): Argument #3 (\$context) must be SODIUM_CRYPTO_KDF_CONTEXTBYTES bytes long");
    }
    if (\strlen($key) !== SODIUM_CRYPTO_KDF_KEYBYTES) {
        throw new \SodiumException("sodium_crypto_kdf_derive_from_key(): Argument #4 (\$key) must be SODIUM_CRYPTO_KDF_KEYBYTES bytes long");
    }
    $pointer = \elephc_sodium_kdf($subkey_length, $subkey_id, $context, \strlen($context), $key, \strlen($key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_kdf_derive_from_key(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_kdf_keygen(): string
{
    return \random_bytes(SODIUM_CRYPTO_KDF_KEYBYTES);
}

function sodium_crypto_shorthash(string $message, string $key): string
{
    if (\strlen($key) !== SODIUM_CRYPTO_SHORTHASH_KEYBYTES) {
        throw new \SodiumException("sodium_crypto_shorthash(): Argument #2 (\$key) must be SODIUM_CRYPTO_SHORTHASH_KEYBYTES bytes long");
    }
    $pointer = \elephc_sodium_shorthash($message, \strlen($message), $key, \strlen($key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_shorthash(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_shorthash_keygen(): string
{
    return \random_bytes(SODIUM_CRYPTO_SHORTHASH_KEYBYTES);
}

function sodium_crypto_sign_seed_keypair(string $seed): string
{
    if (\strlen($seed) !== SODIUM_CRYPTO_SIGN_SEEDBYTES) {
        throw new \SodiumException("sodium_crypto_sign_seed_keypair(): Argument #1 (\$seed) must be SODIUM_CRYPTO_SIGN_SEEDBYTES bytes long");
    }
    $pointer = \elephc_sodium_sign_seed_keypair($seed, \strlen($seed));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_sign_seed_keypair(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_sign_keypair(): string
{
    return \sodium_crypto_sign_seed_keypair(\random_bytes(SODIUM_CRYPTO_SIGN_SEEDBYTES));
}

function sodium_crypto_sign_keypair_from_secretkey_and_publickey(string $secret_key, string $public_key): string
{
    if (\strlen($secret_key) !== SODIUM_CRYPTO_SIGN_SECRETKEYBYTES) {
        throw new \SodiumException("sodium_crypto_sign_keypair_from_secretkey_and_publickey(): Argument #1 (\$secret_key) must be SODIUM_CRYPTO_SIGN_SECRETKEYBYTES bytes long");
    }
    if (\strlen($public_key) !== SODIUM_CRYPTO_SIGN_PUBLICKEYBYTES) {
        throw new \SodiumException("sodium_crypto_sign_keypair_from_secretkey_and_publickey(): Argument #2 (\$public_key) must be SODIUM_CRYPTO_SIGN_PUBLICKEYBYTES bytes long");
    }
    return $secret_key . $public_key;
}

function sodium_crypto_sign_secretkey(string $key_pair): string
{
    if (\strlen($key_pair) !== SODIUM_CRYPTO_SIGN_KEYPAIRBYTES) {
        throw new \SodiumException("sodium_crypto_sign_secretkey(): Argument #1 (\$key_pair) must be SODIUM_CRYPTO_SIGN_KEYPAIRBYTES bytes long");
    }
    return \substr($key_pair, 0, SODIUM_CRYPTO_SIGN_SECRETKEYBYTES);
}

function sodium_crypto_sign_publickey(string $key_pair): string
{
    if (\strlen($key_pair) !== SODIUM_CRYPTO_SIGN_KEYPAIRBYTES) {
        throw new \SodiumException("sodium_crypto_sign_publickey(): Argument #1 (\$key_pair) must be SODIUM_CRYPTO_SIGN_KEYPAIRBYTES bytes long");
    }
    return \substr($key_pair, SODIUM_CRYPTO_SIGN_SECRETKEYBYTES, SODIUM_CRYPTO_SIGN_PUBLICKEYBYTES);
}

function sodium_crypto_sign_publickey_from_secretkey(string $secret_key): string
{
    if (\strlen($secret_key) !== SODIUM_CRYPTO_SIGN_SECRETKEYBYTES) {
        throw new \SodiumException("sodium_crypto_sign_publickey_from_secretkey(): Argument #1 (\$secret_key) must be SODIUM_CRYPTO_SIGN_SECRETKEYBYTES bytes long");
    }
    $pointer = \elephc_sodium_sign_publickey_from_secretkey($secret_key, \strlen($secret_key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_sign_publickey_from_secretkey(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_sign_detached(string $message, string $secret_key): string
{
    if (\strlen($secret_key) !== SODIUM_CRYPTO_SIGN_SECRETKEYBYTES) {
        throw new \SodiumException("sodium_crypto_sign_detached(): Argument #2 (\$secret_key) must be SODIUM_CRYPTO_SIGN_SECRETKEYBYTES bytes long");
    }
    $pointer = \elephc_sodium_sign_detached($message, \strlen($message), $secret_key, \strlen($secret_key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_sign_detached(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_sign(string $message, string $secret_key): string
{
    if (\strlen($secret_key) !== SODIUM_CRYPTO_SIGN_SECRETKEYBYTES) {
        throw new \SodiumException("sodium_crypto_sign(): Argument #2 (\$secret_key) must be SODIUM_CRYPTO_SIGN_SECRETKEYBYTES bytes long");
    }
    $pointer = \elephc_sodium_sign($message, \strlen($message), $secret_key, \strlen($secret_key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_sign(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_sign_open(string $signed_message, string $public_key): string|false
{
    if (\strlen($public_key) !== SODIUM_CRYPTO_SIGN_PUBLICKEYBYTES) {
        throw new \SodiumException("sodium_crypto_sign_open(): Argument #2 (\$public_key) must be SODIUM_CRYPTO_SIGN_PUBLICKEYBYTES bytes long");
    }
    if (\strlen($signed_message) < SODIUM_CRYPTO_SIGN_BYTES) {
        return false;
    }
    $pointer = \elephc_sodium_sign_open($signed_message, \strlen($signed_message), $public_key, \strlen($public_key));
    if (\__elephc_ptr_is_null($pointer)) {
        return false;
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_sign_verify_detached(string $signature, string $message, string $public_key): bool
{
    if (\strlen($signature) !== SODIUM_CRYPTO_SIGN_BYTES) {
        throw new \SodiumException("sodium_crypto_sign_verify_detached(): Argument #1 (\$signature) must be SODIUM_CRYPTO_SIGN_BYTES bytes long");
    }
    if (\strlen($public_key) !== SODIUM_CRYPTO_SIGN_PUBLICKEYBYTES) {
        throw new \SodiumException("sodium_crypto_sign_verify_detached(): Argument #3 (\$public_key) must be SODIUM_CRYPTO_SIGN_PUBLICKEYBYTES bytes long");
    }
    return \elephc_sodium_sign_verify_detached($signature, \strlen($signature), $message, \strlen($message), $public_key, \strlen($public_key)) === 1;
}

function sodium_crypto_sign_ed25519_pk_to_curve25519(string $public_key): string
{
    if (\strlen($public_key) !== SODIUM_CRYPTO_SIGN_PUBLICKEYBYTES) {
        throw new \SodiumException("sodium_crypto_sign_ed25519_pk_to_curve25519(): Argument #1 (\$public_key) must be SODIUM_CRYPTO_SIGN_PUBLICKEYBYTES bytes long");
    }
    $pointer = \elephc_sodium_sign_pk_to_curve25519($public_key, \strlen($public_key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("conversion failed");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_sign_ed25519_sk_to_curve25519(string $secret_key): string
{
    if (\strlen($secret_key) !== SODIUM_CRYPTO_SIGN_SECRETKEYBYTES) {
        throw new \SodiumException("sodium_crypto_sign_ed25519_sk_to_curve25519(): Argument #1 (\$secret_key) must be SODIUM_CRYPTO_SIGN_SECRETKEYBYTES bytes long");
    }
    $pointer = \elephc_sodium_sign_sk_to_curve25519($secret_key, \strlen($secret_key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_sign_ed25519_sk_to_curve25519(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_box_seed_keypair(string $seed): string
{
    if (\strlen($seed) !== SODIUM_CRYPTO_BOX_SEEDBYTES) {
        throw new \SodiumException("sodium_crypto_box_seed_keypair(): Argument #1 (\$seed) must be SODIUM_CRYPTO_BOX_SEEDBYTES bytes long");
    }
    $pointer = \elephc_sodium_box_seed_keypair($seed, \strlen($seed));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_box_seed_keypair(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_box_keypair(): string
{
    $secret = \random_bytes(SODIUM_CRYPTO_BOX_SECRETKEYBYTES);
    $pointer = \elephc_sodium_box_publickey_from_secretkey($secret, \strlen($secret));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_box_keypair(): internal error");
    }
    return $secret . \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_box_keypair_from_secretkey_and_publickey(string $secret_key, string $public_key): string
{
    if (\strlen($secret_key) !== SODIUM_CRYPTO_BOX_SECRETKEYBYTES) {
        throw new \SodiumException("sodium_crypto_box_keypair_from_secretkey_and_publickey(): Argument #1 (\$secret_key) must be SODIUM_CRYPTO_BOX_SECRETKEYBYTES bytes long");
    }
    if (\strlen($public_key) !== SODIUM_CRYPTO_BOX_PUBLICKEYBYTES) {
        throw new \SodiumException("sodium_crypto_box_keypair_from_secretkey_and_publickey(): Argument #2 (\$public_key) must be SODIUM_CRYPTO_BOX_PUBLICKEYBYTES bytes long");
    }
    return $secret_key . $public_key;
}

function sodium_crypto_box_secretkey(string $key_pair): string
{
    if (\strlen($key_pair) !== SODIUM_CRYPTO_BOX_KEYPAIRBYTES) {
        throw new \SodiumException("sodium_crypto_box_secretkey(): Argument #1 (\$key_pair) must be SODIUM_CRYPTO_BOX_KEYPAIRBYTES bytes long");
    }
    return \substr($key_pair, 0, SODIUM_CRYPTO_BOX_SECRETKEYBYTES);
}

function sodium_crypto_box_publickey(string $key_pair): string
{
    if (\strlen($key_pair) !== SODIUM_CRYPTO_BOX_KEYPAIRBYTES) {
        throw new \SodiumException("sodium_crypto_box_publickey(): Argument #1 (\$key_pair) must be SODIUM_CRYPTO_BOX_KEYPAIRBYTES bytes long");
    }
    return \substr($key_pair, SODIUM_CRYPTO_BOX_SECRETKEYBYTES, SODIUM_CRYPTO_BOX_PUBLICKEYBYTES);
}

function sodium_crypto_box_publickey_from_secretkey(string $secret_key): string
{
    if (\strlen($secret_key) !== SODIUM_CRYPTO_BOX_SECRETKEYBYTES) {
        throw new \SodiumException("sodium_crypto_box_publickey_from_secretkey(): Argument #1 (\$secret_key) must be SODIUM_CRYPTO_BOX_SECRETKEYBYTES bytes long");
    }
    $pointer = \elephc_sodium_box_publickey_from_secretkey($secret_key, \strlen($secret_key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_box_publickey_from_secretkey(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_box(string $message, string $nonce, string $key_pair): string
{
    if (\strlen($nonce) !== SODIUM_CRYPTO_BOX_NONCEBYTES) {
        throw new \SodiumException("sodium_crypto_box(): Argument #2 (\$nonce) must be SODIUM_CRYPTO_BOX_NONCEBYTES bytes long");
    }
    if (\strlen($key_pair) !== SODIUM_CRYPTO_BOX_KEYPAIRBYTES) {
        throw new \SodiumException("sodium_crypto_box(): Argument #3 (\$key_pair) must be SODIUM_CRYPTO_BOX_KEYPAIRBYTES bytes long");
    }
    $pointer = \elephc_sodium_box_encrypt($message, \strlen($message), $nonce, \strlen($nonce), $key_pair, \strlen($key_pair));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_box(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_box_open(string $ciphertext, string $nonce, string $key_pair): string|false
{
    if (\strlen($nonce) !== SODIUM_CRYPTO_BOX_NONCEBYTES) {
        throw new \SodiumException("sodium_crypto_box_open(): Argument #2 (\$nonce) must be SODIUM_CRYPTO_BOX_NONCEBYTES bytes long");
    }
    if (\strlen($key_pair) !== SODIUM_CRYPTO_BOX_KEYPAIRBYTES) {
        throw new \SodiumException("sodium_crypto_box_open(): Argument #3 (\$key_pair) must be SODIUM_CRYPTO_BOX_KEYPAIRBYTES bytes long");
    }
    if (\strlen($ciphertext) < SODIUM_CRYPTO_BOX_MACBYTES) {
        return false;
    }
    $pointer = \elephc_sodium_box_decrypt($ciphertext, \strlen($ciphertext), $nonce, \strlen($nonce), $key_pair, \strlen($key_pair));
    if (\__elephc_ptr_is_null($pointer)) {
        return false;
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_box_seal(string $message, string $public_key): string
{
    if (\strlen($public_key) !== SODIUM_CRYPTO_BOX_PUBLICKEYBYTES) {
        throw new \SodiumException("sodium_crypto_box_seal(): Argument #2 (\$public_key) must be SODIUM_CRYPTO_BOX_PUBLICKEYBYTES bytes long");
    }
    $ephemeral = \sodium_crypto_box_keypair();
    $ephemeral_public = \sodium_crypto_box_publickey($ephemeral);
    $ephemeral_secret = \sodium_crypto_box_secretkey($ephemeral);
    $nonce = \sodium_crypto_generichash($ephemeral_public . $public_key, "", SODIUM_CRYPTO_BOX_NONCEBYTES);
    $pair = \sodium_crypto_box_keypair_from_secretkey_and_publickey($ephemeral_secret, $public_key);
    return $ephemeral_public . \sodium_crypto_box($message, $nonce, $pair);
}

function sodium_crypto_box_seal_open(string $ciphertext, string $key_pair): string|false
{
    if (\strlen($key_pair) !== SODIUM_CRYPTO_BOX_KEYPAIRBYTES) {
        throw new \SodiumException("sodium_crypto_box_seal_open(): Argument #2 (\$key_pair) must be SODIUM_CRYPTO_BOX_KEYPAIRBYTES bytes long");
    }
    if (\strlen($ciphertext) < SODIUM_CRYPTO_BOX_SEALBYTES) {
        return false;
    }
    $ephemeral_public = \substr($ciphertext, 0, SODIUM_CRYPTO_BOX_PUBLICKEYBYTES);
    $body = \substr($ciphertext, SODIUM_CRYPTO_BOX_PUBLICKEYBYTES);
    $public_key = \sodium_crypto_box_publickey($key_pair);
    $nonce = \sodium_crypto_generichash($ephemeral_public . $public_key, "", SODIUM_CRYPTO_BOX_NONCEBYTES);
    $pair = \sodium_crypto_box_keypair_from_secretkey_and_publickey(\sodium_crypto_box_secretkey($key_pair), $ephemeral_public);
    return \sodium_crypto_box_open($body, $nonce, $pair);
}

function sodium_crypto_kx_seed_keypair(string $seed): string
{
    if (\strlen($seed) !== SODIUM_CRYPTO_KX_SEEDBYTES) {
        throw new \SodiumException("sodium_crypto_kx_seed_keypair(): Argument #1 (\$seed) must be SODIUM_CRYPTO_KX_SEEDBYTES bytes long");
    }
    $pointer = \elephc_sodium_kx_seed_keypair($seed, \strlen($seed));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_kx_seed_keypair(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_kx_keypair(): string
{
    $secret = \random_bytes(SODIUM_CRYPTO_KX_SECRETKEYBYTES);
    $pointer = \elephc_sodium_scalarmult_base($secret, \strlen($secret));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_kx_keypair(): internal error");
    }
    return $secret . \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_kx_secretkey(string $key_pair): string
{
    if (\strlen($key_pair) !== SODIUM_CRYPTO_KX_KEYPAIRBYTES) {
        throw new \SodiumException("sodium_crypto_kx_secretkey(): Argument #1 (\$key_pair) must be SODIUM_CRYPTO_KX_KEYPAIRBYTES bytes long");
    }
    return \substr($key_pair, 0, SODIUM_CRYPTO_KX_SECRETKEYBYTES);
}

function sodium_crypto_kx_publickey(string $key_pair): string
{
    if (\strlen($key_pair) !== SODIUM_CRYPTO_KX_KEYPAIRBYTES) {
        throw new \SodiumException("sodium_crypto_kx_publickey(): Argument #1 (\$key_pair) must be SODIUM_CRYPTO_KX_KEYPAIRBYTES bytes long");
    }
    return \substr($key_pair, SODIUM_CRYPTO_KX_SECRETKEYBYTES, SODIUM_CRYPTO_KX_PUBLICKEYBYTES);
}

function sodium_crypto_kx_client_session_keys(string $client_key_pair, string $server_key): array
{
    if (\strlen($client_key_pair) !== SODIUM_CRYPTO_KX_KEYPAIRBYTES) {
        throw new \SodiumException("sodium_crypto_kx_client_session_keys(): Argument #1 (\$client_key_pair) must be SODIUM_CRYPTO_KX_KEYPAIRBYTES bytes long");
    }
    if (\strlen($server_key) !== SODIUM_CRYPTO_KX_PUBLICKEYBYTES) {
        throw new \SodiumException("sodium_crypto_kx_client_session_keys(): Argument #2 (\$server_key) must be SODIUM_CRYPTO_KX_PUBLICKEYBYTES bytes long");
    }
    $pointer = \elephc_sodium_kx_client_session_keys($client_key_pair, \strlen($client_key_pair), $server_key, \strlen($server_key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_kx_client_session_keys(): internal error");
    }
    $keys = \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
    return [\substr($keys, 0, SODIUM_CRYPTO_KX_SESSIONKEYBYTES), \substr($keys, SODIUM_CRYPTO_KX_SESSIONKEYBYTES)];
}

function sodium_crypto_kx_server_session_keys(string $server_key_pair, string $client_key): array
{
    if (\strlen($server_key_pair) !== SODIUM_CRYPTO_KX_KEYPAIRBYTES) {
        throw new \SodiumException("sodium_crypto_kx_server_session_keys(): Argument #1 (\$server_key_pair) must be SODIUM_CRYPTO_KX_KEYPAIRBYTES bytes long");
    }
    if (\strlen($client_key) !== SODIUM_CRYPTO_KX_PUBLICKEYBYTES) {
        throw new \SodiumException("sodium_crypto_kx_server_session_keys(): Argument #2 (\$client_key) must be SODIUM_CRYPTO_KX_PUBLICKEYBYTES bytes long");
    }
    $pointer = \elephc_sodium_kx_server_session_keys($server_key_pair, \strlen($server_key_pair), $client_key, \strlen($client_key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_kx_server_session_keys(): internal error");
    }
    $keys = \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
    return [\substr($keys, 0, SODIUM_CRYPTO_KX_SESSIONKEYBYTES), \substr($keys, SODIUM_CRYPTO_KX_SESSIONKEYBYTES)];
}

function sodium_crypto_scalarmult(string $secret_key, string $public_key): string
{
    if (\strlen($secret_key) !== SODIUM_CRYPTO_SCALARMULT_SCALARBYTES) {
        throw new \SodiumException("sodium_crypto_scalarmult(): Argument #1 (\$secret_key) must be SODIUM_CRYPTO_SCALARMULT_SCALARBYTES bytes long");
    }
    if (\strlen($public_key) !== SODIUM_CRYPTO_SCALARMULT_BYTES) {
        throw new \SodiumException("sodium_crypto_scalarmult(): Argument #2 (\$public_key) must be SODIUM_CRYPTO_SCALARMULT_BYTES bytes long");
    }
    $pointer = \elephc_sodium_scalarmult($secret_key, \strlen($secret_key), $public_key, \strlen($public_key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_scalarmult(): invalid public key");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_scalarmult_base(string $secret_key): string
{
    if (\strlen($secret_key) !== SODIUM_CRYPTO_SCALARMULT_SCALARBYTES) {
        throw new \SodiumException("sodium_crypto_scalarmult_base(): Argument #1 (\$secret_key) must be SODIUM_CRYPTO_SCALARMULT_SCALARBYTES bytes long");
    }
    $pointer = \elephc_sodium_scalarmult_base($secret_key, \strlen($secret_key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_scalarmult_base(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_aead_chacha20poly1305_encrypt(string $message, string $additional_data, string $nonce, string $key): string
{
    if (\strlen($nonce) !== SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_NPUBBYTES) {
        throw new \SodiumException("sodium_crypto_aead_chacha20poly1305_encrypt(): Argument #3 (\$nonce) must be SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_NPUBBYTES bytes long");
    }
    if (\strlen($key) !== SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_KEYBYTES) {
        throw new \SodiumException("sodium_crypto_aead_chacha20poly1305_encrypt(): Argument #4 (\$key) must be SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_KEYBYTES bytes long");
    }
    $pointer = \elephc_sodium_aead_chacha20poly1305_encrypt($message, \strlen($message), $additional_data, \strlen($additional_data), $nonce, \strlen($nonce), $key, \strlen($key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_aead_chacha20poly1305_encrypt(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_aead_chacha20poly1305_decrypt(string $ciphertext, string $additional_data, string $nonce, string $key): string|false
{
    if (\strlen($nonce) !== SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_NPUBBYTES) {
        throw new \SodiumException("sodium_crypto_aead_chacha20poly1305_decrypt(): Argument #3 (\$nonce) must be SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_NPUBBYTES bytes long");
    }
    if (\strlen($key) !== SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_KEYBYTES) {
        throw new \SodiumException("sodium_crypto_aead_chacha20poly1305_decrypt(): Argument #4 (\$key) must be SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_KEYBYTES bytes long");
    }
    if (\strlen($ciphertext) < SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_ABYTES) {
        return false;
    }
    $pointer = \elephc_sodium_aead_chacha20poly1305_decrypt($ciphertext, \strlen($ciphertext), $additional_data, \strlen($additional_data), $nonce, \strlen($nonce), $key, \strlen($key));
    if (\__elephc_ptr_is_null($pointer)) {
        return false;
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_aead_chacha20poly1305_keygen(): string
{
    return \random_bytes(SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_KEYBYTES);
}

function sodium_crypto_aead_chacha20poly1305_ietf_encrypt(string $message, string $additional_data, string $nonce, string $key): string
{
    if (\strlen($nonce) !== SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_IETF_NPUBBYTES) {
        throw new \SodiumException("sodium_crypto_aead_chacha20poly1305_ietf_encrypt(): Argument #3 (\$nonce) must be SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_IETF_NPUBBYTES bytes long");
    }
    if (\strlen($key) !== SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_IETF_KEYBYTES) {
        throw new \SodiumException("sodium_crypto_aead_chacha20poly1305_ietf_encrypt(): Argument #4 (\$key) must be SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_IETF_KEYBYTES bytes long");
    }
    $pointer = \elephc_sodium_aead_chacha20poly1305_ietf_encrypt($message, \strlen($message), $additional_data, \strlen($additional_data), $nonce, \strlen($nonce), $key, \strlen($key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_aead_chacha20poly1305_ietf_encrypt(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_aead_chacha20poly1305_ietf_decrypt(string $ciphertext, string $additional_data, string $nonce, string $key): string|false
{
    if (\strlen($nonce) !== SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_IETF_NPUBBYTES) {
        throw new \SodiumException("sodium_crypto_aead_chacha20poly1305_ietf_decrypt(): Argument #3 (\$nonce) must be SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_IETF_NPUBBYTES bytes long");
    }
    if (\strlen($key) !== SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_IETF_KEYBYTES) {
        throw new \SodiumException("sodium_crypto_aead_chacha20poly1305_ietf_decrypt(): Argument #4 (\$key) must be SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_IETF_KEYBYTES bytes long");
    }
    if (\strlen($ciphertext) < SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_IETF_ABYTES) {
        return false;
    }
    $pointer = \elephc_sodium_aead_chacha20poly1305_ietf_decrypt($ciphertext, \strlen($ciphertext), $additional_data, \strlen($additional_data), $nonce, \strlen($nonce), $key, \strlen($key));
    if (\__elephc_ptr_is_null($pointer)) {
        return false;
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_aead_chacha20poly1305_ietf_keygen(): string
{
    return \random_bytes(SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_IETF_KEYBYTES);
}

function sodium_crypto_aead_xchacha20poly1305_ietf_encrypt(string $message, string $additional_data, string $nonce, string $key): string
{
    if (\strlen($nonce) !== SODIUM_CRYPTO_AEAD_XCHACHA20POLY1305_IETF_NPUBBYTES) {
        throw new \SodiumException("sodium_crypto_aead_xchacha20poly1305_ietf_encrypt(): Argument #3 (\$nonce) must be SODIUM_CRYPTO_AEAD_XCHACHA20POLY1305_IETF_NPUBBYTES bytes long");
    }
    if (\strlen($key) !== SODIUM_CRYPTO_AEAD_XCHACHA20POLY1305_IETF_KEYBYTES) {
        throw new \SodiumException("sodium_crypto_aead_xchacha20poly1305_ietf_encrypt(): Argument #4 (\$key) must be SODIUM_CRYPTO_AEAD_XCHACHA20POLY1305_IETF_KEYBYTES bytes long");
    }
    $pointer = \elephc_sodium_aead_xchacha20poly1305_ietf_encrypt($message, \strlen($message), $additional_data, \strlen($additional_data), $nonce, \strlen($nonce), $key, \strlen($key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_aead_xchacha20poly1305_ietf_encrypt(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_aead_xchacha20poly1305_ietf_decrypt(string $ciphertext, string $additional_data, string $nonce, string $key): string|false
{
    if (\strlen($nonce) !== SODIUM_CRYPTO_AEAD_XCHACHA20POLY1305_IETF_NPUBBYTES) {
        throw new \SodiumException("sodium_crypto_aead_xchacha20poly1305_ietf_decrypt(): Argument #3 (\$nonce) must be SODIUM_CRYPTO_AEAD_XCHACHA20POLY1305_IETF_NPUBBYTES bytes long");
    }
    if (\strlen($key) !== SODIUM_CRYPTO_AEAD_XCHACHA20POLY1305_IETF_KEYBYTES) {
        throw new \SodiumException("sodium_crypto_aead_xchacha20poly1305_ietf_decrypt(): Argument #4 (\$key) must be SODIUM_CRYPTO_AEAD_XCHACHA20POLY1305_IETF_KEYBYTES bytes long");
    }
    if (\strlen($ciphertext) < SODIUM_CRYPTO_AEAD_XCHACHA20POLY1305_IETF_ABYTES) {
        return false;
    }
    $pointer = \elephc_sodium_aead_xchacha20poly1305_ietf_decrypt($ciphertext, \strlen($ciphertext), $additional_data, \strlen($additional_data), $nonce, \strlen($nonce), $key, \strlen($key));
    if (\__elephc_ptr_is_null($pointer)) {
        return false;
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_aead_xchacha20poly1305_ietf_keygen(): string
{
    return \random_bytes(SODIUM_CRYPTO_AEAD_XCHACHA20POLY1305_IETF_KEYBYTES);
}

function sodium_crypto_aead_aes256gcm_is_available(): bool
{
    return true;
}

function sodium_crypto_aead_aes256gcm_encrypt(string $message, string $additional_data, string $nonce, string $key): string
{
    if (\strlen($nonce) !== SODIUM_CRYPTO_AEAD_AES256GCM_NPUBBYTES) {
        throw new \SodiumException("sodium_crypto_aead_aes256gcm_encrypt(): Argument #3 (\$nonce) must be SODIUM_CRYPTO_AEAD_AES256GCM_NPUBBYTES bytes long");
    }
    if (\strlen($key) !== SODIUM_CRYPTO_AEAD_AES256GCM_KEYBYTES) {
        throw new \SodiumException("sodium_crypto_aead_aes256gcm_encrypt(): Argument #4 (\$key) must be SODIUM_CRYPTO_AEAD_AES256GCM_KEYBYTES bytes long");
    }
    $pointer = \elephc_sodium_aead_aes256gcm_encrypt($message, \strlen($message), $additional_data, \strlen($additional_data), $nonce, \strlen($nonce), $key, \strlen($key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_aead_aes256gcm_encrypt(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_aead_aes256gcm_decrypt(string $ciphertext, string $additional_data, string $nonce, string $key): string|false
{
    if (\strlen($nonce) !== SODIUM_CRYPTO_AEAD_AES256GCM_NPUBBYTES) {
        throw new \SodiumException("sodium_crypto_aead_aes256gcm_decrypt(): Argument #3 (\$nonce) must be SODIUM_CRYPTO_AEAD_AES256GCM_NPUBBYTES bytes long");
    }
    if (\strlen($key) !== SODIUM_CRYPTO_AEAD_AES256GCM_KEYBYTES) {
        throw new \SodiumException("sodium_crypto_aead_aes256gcm_decrypt(): Argument #4 (\$key) must be SODIUM_CRYPTO_AEAD_AES256GCM_KEYBYTES bytes long");
    }
    if (\strlen($ciphertext) < SODIUM_CRYPTO_AEAD_AES256GCM_ABYTES) {
        return false;
    }
    $pointer = \elephc_sodium_aead_aes256gcm_decrypt($ciphertext, \strlen($ciphertext), $additional_data, \strlen($additional_data), $nonce, \strlen($nonce), $key, \strlen($key));
    if (\__elephc_ptr_is_null($pointer)) {
        return false;
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_aead_aes256gcm_keygen(): string
{
    return \random_bytes(SODIUM_CRYPTO_AEAD_AES256GCM_KEYBYTES);
}

function sodium_crypto_secretstream_xchacha20poly1305_keygen(): string
{
    return \random_bytes(SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_KEYBYTES);
}

function sodium_crypto_secretstream_xchacha20poly1305_init_push(string $key): array
{
    if (\strlen($key) !== SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_KEYBYTES) {
        throw new \SodiumException("sodium_crypto_secretstream_xchacha20poly1305_init_push(): Argument #1 (\$key) must be SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_KEYBYTES bytes long");
    }
    $header = \random_bytes(SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_HEADERBYTES);
    $pointer = \elephc_sodium_secretstream_init_push($key, \strlen($key), $header, \strlen($header));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_secretstream_xchacha20poly1305_init_push(): internal error");
    }
    return [\__elephc_ptr_read_string($pointer, \elephc_sodium_last_len()), $header];
}

function sodium_crypto_secretstream_xchacha20poly1305_init_pull(string $header, string $key): string
{
    if (\strlen($header) !== SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_HEADERBYTES) {
        throw new \SodiumException("sodium_crypto_secretstream_xchacha20poly1305_init_pull(): Argument #1 (\$header) must be SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_HEADERBYTES bytes long");
    }
    if (\strlen($key) !== SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_KEYBYTES) {
        throw new \SodiumException("sodium_crypto_secretstream_xchacha20poly1305_init_pull(): Argument #2 (\$key) must be SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_KEYBYTES bytes long");
    }
    $pointer = \elephc_sodium_secretstream_init_pull($header, \strlen($header), $key, \strlen($key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_secretstream_xchacha20poly1305_init_pull(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_secretstream_xchacha20poly1305_push(mixed &$state, string $message, string $additional_data = "", int $tag = SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_TAG_MESSAGE): string
{
    if (!\is_string($state) || \strlen($state) !== 52) {
        throw new \SodiumException("sodium_crypto_secretstream_xchacha20poly1305_push(): Argument #1 (\$state) must be a reference to a state");
    }
    $pointer = \elephc_sodium_secretstream_push($state, \strlen($state), $message, \strlen($message), $additional_data, \strlen($additional_data), $tag);
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_secretstream_xchacha20poly1305_push(): internal error");
    }
    $output = \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
    $state = \substr($output, 0, 52);
    return \substr($output, 52);
}

function sodium_crypto_secretstream_xchacha20poly1305_pull(mixed &$state, string $ciphertext, string $additional_data = ""): array|false
{
    if (!\is_string($state) || \strlen($state) !== 52) {
        throw new \SodiumException("sodium_crypto_secretstream_xchacha20poly1305_pull(): Argument #1 (\$state) must be a reference to a state");
    }
    if (\strlen($ciphertext) < SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_ABYTES) {
        return false;
    }
    $pointer = \elephc_sodium_secretstream_pull($state, \strlen($state), $ciphertext, \strlen($ciphertext), $additional_data, \strlen($additional_data));
    if (\__elephc_ptr_is_null($pointer)) {
        return false;
    }
    $output = \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
    $state = \substr($output, 0, 52);
    $tag = \ord(\substr($output, 52, 1));
    return [\substr($output, 53), $tag];
}

function sodium_crypto_secretstream_xchacha20poly1305_rekey(mixed &$state): void
{
    if (!\is_string($state) || \strlen($state) !== 52) {
        throw new \SodiumException("sodium_crypto_secretstream_xchacha20poly1305_rekey(): Argument #1 (\$state) must be a reference to a state");
    }
    $pointer = \elephc_sodium_secretstream_rekey($state, \strlen($state));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_secretstream_xchacha20poly1305_rekey(): internal error");
    }
    $state = \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_stream(int $length, string $nonce, string $key): string
{
    if (\strlen($nonce) !== SODIUM_CRYPTO_STREAM_NONCEBYTES) {
        throw new \SodiumException("sodium_crypto_stream(): Argument #2 (\$nonce) must be SODIUM_CRYPTO_STREAM_NONCEBYTES bytes long");
    }
    if (\strlen($key) !== SODIUM_CRYPTO_STREAM_KEYBYTES) {
        throw new \SodiumException("sodium_crypto_stream(): Argument #3 (\$key) must be SODIUM_CRYPTO_STREAM_KEYBYTES bytes long");
    }
    $pointer = \elephc_sodium_stream($length, $nonce, \strlen($nonce), $key, \strlen($key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_stream(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_stream_xor(string $message, string $nonce, string $key): string
{
    if (\strlen($nonce) !== SODIUM_CRYPTO_STREAM_NONCEBYTES) {
        throw new \SodiumException("sodium_crypto_stream_xor(): Argument #2 (\$nonce) must be SODIUM_CRYPTO_STREAM_NONCEBYTES bytes long");
    }
    if (\strlen($key) !== SODIUM_CRYPTO_STREAM_KEYBYTES) {
        throw new \SodiumException("sodium_crypto_stream_xor(): Argument #3 (\$key) must be SODIUM_CRYPTO_STREAM_KEYBYTES bytes long");
    }
    $pointer = \elephc_sodium_stream_xor($message, \strlen($message), $nonce, \strlen($nonce), $key, \strlen($key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_stream_xor(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_stream_keygen(): string
{
    return \random_bytes(SODIUM_CRYPTO_STREAM_KEYBYTES);
}

function sodium_crypto_stream_xchacha20(int $length, string $nonce, string $key): string
{
    if (\strlen($nonce) !== SODIUM_CRYPTO_STREAM_XCHACHA20_NONCEBYTES) {
        throw new \SodiumException("sodium_crypto_stream_xchacha20(): Argument #2 (\$nonce) must be SODIUM_CRYPTO_STREAM_XCHACHA20_NONCEBYTES bytes long");
    }
    if (\strlen($key) !== SODIUM_CRYPTO_STREAM_XCHACHA20_KEYBYTES) {
        throw new \SodiumException("sodium_crypto_stream_xchacha20(): Argument #3 (\$key) must be SODIUM_CRYPTO_STREAM_XCHACHA20_KEYBYTES bytes long");
    }
    $pointer = \elephc_sodium_stream_xchacha20($length, $nonce, \strlen($nonce), $key, \strlen($key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_stream_xchacha20(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_stream_xchacha20_xor(string $message, string $nonce, string $key): string
{
    if (\strlen($nonce) !== SODIUM_CRYPTO_STREAM_XCHACHA20_NONCEBYTES) {
        throw new \SodiumException("sodium_crypto_stream_xchacha20_xor(): Argument #2 (\$nonce) must be SODIUM_CRYPTO_STREAM_XCHACHA20_NONCEBYTES bytes long");
    }
    if (\strlen($key) !== SODIUM_CRYPTO_STREAM_XCHACHA20_KEYBYTES) {
        throw new \SodiumException("sodium_crypto_stream_xchacha20_xor(): Argument #3 (\$key) must be SODIUM_CRYPTO_STREAM_XCHACHA20_KEYBYTES bytes long");
    }
    $pointer = \elephc_sodium_stream_xchacha20_xor($message, \strlen($message), $nonce, \strlen($nonce), $key, \strlen($key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_stream_xchacha20_xor(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_stream_xchacha20_xor_ic(string $message, string $nonce, int $counter, string $key): string
{
    if (\strlen($nonce) !== SODIUM_CRYPTO_STREAM_XCHACHA20_NONCEBYTES) {
        throw new \SodiumException("sodium_crypto_stream_xchacha20_xor_ic(): Argument #2 (\$nonce) must be SODIUM_CRYPTO_STREAM_XCHACHA20_NONCEBYTES bytes long");
    }
    if (\strlen($key) !== SODIUM_CRYPTO_STREAM_XCHACHA20_KEYBYTES) {
        throw new \SodiumException("sodium_crypto_stream_xchacha20_xor_ic(): Argument #4 (\$key) must be SODIUM_CRYPTO_STREAM_XCHACHA20_KEYBYTES bytes long");
    }
    if ($counter < 0) {
        throw new \SodiumException("sodium_crypto_stream_xchacha20_xor_ic(): Argument #3 (\$counter) must be greater than or equal to 0");
    }
    $pointer = \elephc_sodium_stream_xchacha20_xor_ic($message, \strlen($message), $nonce, \strlen($nonce), $counter, $key, \strlen($key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_stream_xchacha20_xor_ic(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_stream_xchacha20_keygen(): string
{
    return \random_bytes(SODIUM_CRYPTO_STREAM_XCHACHA20_KEYBYTES);
}

function sodium_compare(string $string1, string $string2): int
{
    $left = \strlen($string1);
    $right = \strlen($string2);
    $length = $left < $right ? $left : $right;
    for ($i = 0; $i < $length; $i++) {
        $a = \ord(\substr($string1, $i, 1));
        $b = \ord(\substr($string2, $i, 1));
        if ($a < $b) {
            return -1;
        }
        if ($a > $b) {
            return 1;
        }
    }
    if ($left === $right) {
        return 0;
    }
    return $left < $right ? -1 : 1;
}

function sodium_memcmp(string $string1, string $string2): int
{
    if (\strlen($string1) !== \strlen($string2)) {
        throw new \SodiumException("sodium_memcmp(): Argument #1 (\$string1) and argument #2 (\$string_2) must have the same length");
    }
    $left = \strlen($string1);
    $difference = 0;
    for ($i = 0; $i < $left; $i++) {
        $difference |= \ord(\substr($string1, $i, 1)) ^ \ord(\substr($string2, $i, 1));
    }
    return $difference === 0 ? 0 : -1;
}

function sodium_add(string &$string1, string $string2): void
{
    if (\strlen($string1) !== \strlen($string2)) {
        throw new \SodiumException("sodium_add(): Argument #1 (\$string1) and argument #2 (\$string_2) must have the same length");
    }
    $length = \strlen($string1);
    $result = "";
    $carry = 0;
    for ($i = 0; $i < $length; $i++) {
        $sum = \ord(\substr($string1, $i, 1)) + \ord(\substr($string2, $i, 1)) + $carry;
        $result .= \chr($sum & 0xff);
        $carry = $sum >> 8;
    }
    $string1 = $result;
}

function sodium_increment(string &$string): void
{
    $length = \strlen($string);
    $result = "";
    $carry = 1;
    for ($i = 0; $i < $length; $i++) {
        $sum = \ord(\substr($string, $i, 1)) + $carry;
        $result .= \chr($sum & 0xff);
        $carry = $sum >> 8;
    }
    $string = $result;
}

function sodium_memzero(string &$string): void
{
    $string = \str_repeat("\0", \strlen($string));
}

function sodium_pad(string $string, int $block_size): string
{
    if ($block_size <= 0) {
        throw new \SodiumException("sodium_pad(): Argument #2 (\$block_size) must be greater than 0");
    }
    $length = \strlen($string);
    $padding = $block_size - ($length % $block_size);
    $string .= "\x80";
    $string .= \str_repeat("\0", $padding - 1);
    return $string;
}

function sodium_unpad(string $string, int $block_size): string
{
    if ($block_size <= 0) {
        throw new \SodiumException("sodium_unpad(): Argument #2 (\$block_size) must be greater than 0");
    }
    $length = \strlen($string);
    if ($length === 0 || $length % $block_size !== 0) {
        throw new \SodiumException("invalid padding");
    }
    $position = $length - 1;
    while ($position >= 0 && \substr($string, $position, 1) === "\0") {
        $position--;
    }
    if ($position < 0 || \substr($string, $position, 1) !== "\x80") {
        throw new \SodiumException("invalid padding");
    }
    return \substr($string, 0, $position);
}

function sodium_bin2hex(string $string): string
{
    return \bin2hex($string);
}

function sodium_hex2bin(string $string, ?string $ignore = null): string
{
    $clean = $string;
    if ($ignore !== null) {
        $clean = "";
        for ($i = 0; $i < \strlen($string); $i++) {
            $character = \substr($string, $i, 1);
            if (\strpos($ignore, $character) === false) {
                $clean .= $character;
            }
        }
    }
    if (\strlen($clean) % 2 !== 0) {
        throw new \SodiumException("sodium_hex2bin(): Argument #1 (\$string) must be a valid hexadecimal string");
    }
    $valid = "0123456789abcdefABCDEF";
    for ($i = 0; $i < \strlen($clean); $i++) {
        if (\strpos($valid, \substr($clean, $i, 1)) === false) {
            throw new \SodiumException("sodium_hex2bin(): Argument #1 (\$string) must be a valid hexadecimal string");
        }
    }
    $decoded = @\hex2bin($clean);
    if (!\is_string($decoded)) {
        throw new \SodiumException("sodium_hex2bin(): Argument #1 (\$string) must be a valid hexadecimal string");
    }
    return $decoded;
}

function sodium_bin2base64(string $string, int $id): string
{
    if ($id !== SODIUM_BASE64_VARIANT_ORIGINAL && $id !== SODIUM_BASE64_VARIANT_ORIGINAL_NO_PADDING && $id !== SODIUM_BASE64_VARIANT_URLSAFE && $id !== SODIUM_BASE64_VARIANT_URLSAFE_NO_PADDING) {
        throw new \SodiumException("sodium_bin2base64(): Argument #2 (\$id) must be a valid base64 variant identifier");
    }
    $encoded = \base64_encode($string);
    if ($id === SODIUM_BASE64_VARIANT_URLSAFE || $id === SODIUM_BASE64_VARIANT_URLSAFE_NO_PADDING) {
        $encoded = \str_replace("+", "-", $encoded);
        $encoded = \str_replace("/", "_", $encoded);
    }
    if ($id === SODIUM_BASE64_VARIANT_ORIGINAL_NO_PADDING || $id === SODIUM_BASE64_VARIANT_URLSAFE_NO_PADDING) {
        while (\substr($encoded, -1) === "=") {
            $encoded = \substr($encoded, 0, -1);
        }
    }
    return $encoded;
}

function sodium_base642bin(string $string, int $id, ?string $ignore = null): string
{
    if ($id !== SODIUM_BASE64_VARIANT_ORIGINAL && $id !== SODIUM_BASE64_VARIANT_ORIGINAL_NO_PADDING && $id !== SODIUM_BASE64_VARIANT_URLSAFE && $id !== SODIUM_BASE64_VARIANT_URLSAFE_NO_PADDING) {
        throw new \SodiumException("sodium_base642bin(): Argument #2 (\$id) must be a valid base64 variant identifier");
    }
    $clean = $string;
    if ($ignore !== null) {
        $clean = "";
        for ($i = 0; $i < \strlen($string); $i++) {
            $character = \substr($string, $i, 1);
            if (\strpos($ignore, $character) === false) {
                $clean .= $character;
            }
        }
    }
    if ($id === SODIUM_BASE64_VARIANT_URLSAFE || $id === SODIUM_BASE64_VARIANT_URLSAFE_NO_PADDING) {
        $clean = \str_replace("-", "+", $clean);
        $clean = \str_replace("_", "/", $clean);
    }
    while (\strlen($clean) % 4 !== 0) {
        $clean .= "=";
    }
    $decoded = \base64_decode($clean, true);
    if (!\is_string($decoded)) {
        throw new \SodiumException("invalid base64 string");
    }
    return $decoded;
}

function sodium_crypto_pwhash_str_needs_rehash(string $password, int $opslimit, int $memlimit): bool
{
    if (\strpos($password, SODIUM_CRYPTO_PWHASH_STRPREFIX) === 0) {
        $parts = \explode("$", $password);
        $parameters = \explode(",", (string) ($parts[3] ?? ""));
        $memory = 0;
        $time = 0;
        $parallelism = 0;
        foreach ($parameters as $parameter) {
            if (\strpos($parameter, "m=") === 0) {
                $memory = (int) \substr($parameter, 2);
            }
            if (\strpos($parameter, "t=") === 0) {
                $time = (int) \substr($parameter, 2);
            }
            if (\strpos($parameter, "p=") === 0) {
                $parallelism = (int) \substr($parameter, 2);
            }
        }
        return $memory !== (int) ($memlimit / 1024) || $time !== $opslimit || $parallelism !== 1;
    }
    return true;
}

function sodium_crypto_core_ristretto255_from_hash(string $s): string
{
    if (\strlen($s) !== SODIUM_CRYPTO_CORE_RISTRETTO255_HASHBYTES) {
        throw new \SodiumException("sodium_crypto_core_ristretto255_from_hash(): Argument #1 (\$s) must be SODIUM_CRYPTO_CORE_RISTRETTO255_HASHBYTES bytes long");
    }
    $pointer = \elephc_sodium_ristretto255_from_hash($s, \strlen($s));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_core_ristretto255_from_hash(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_core_ristretto255_is_valid_point(string $s): bool
{
    if (\strlen($s) !== SODIUM_CRYPTO_CORE_RISTRETTO255_BYTES) {
        throw new \SodiumException("sodium_crypto_core_ristretto255_is_valid_point(): Argument #1 (\$s) must be SODIUM_CRYPTO_CORE_RISTRETTO255_BYTES bytes long");
    }
    return \elephc_sodium_ristretto255_is_valid_point($s, \strlen($s)) === 1;
}

function sodium_crypto_core_ristretto255_add(string $p, string $q): string
{
    if (\strlen($p) !== SODIUM_CRYPTO_CORE_RISTRETTO255_BYTES) {
        throw new \SodiumException("sodium_crypto_core_ristretto255_add(): Argument #1 (\$p) must be SODIUM_CRYPTO_CORE_RISTRETTO255_BYTES bytes long");
    }
    if (\strlen($q) !== SODIUM_CRYPTO_CORE_RISTRETTO255_BYTES) {
        throw new \SodiumException("sodium_crypto_core_ristretto255_add(): Argument #2 (\$q) must be SODIUM_CRYPTO_CORE_RISTRETTO255_BYTES bytes long");
    }
    $pointer = \elephc_sodium_ristretto255_add($p, \strlen($p), $q, \strlen($q));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_core_ristretto255_sub(string $p, string $q): string
{
    if (\strlen($p) !== SODIUM_CRYPTO_CORE_RISTRETTO255_BYTES) {
        throw new \SodiumException("sodium_crypto_core_ristretto255_sub(): Argument #1 (\$p) must be SODIUM_CRYPTO_CORE_RISTRETTO255_BYTES bytes long");
    }
    if (\strlen($q) !== SODIUM_CRYPTO_CORE_RISTRETTO255_BYTES) {
        throw new \SodiumException("sodium_crypto_core_ristretto255_sub(): Argument #2 (\$q) must be SODIUM_CRYPTO_CORE_RISTRETTO255_BYTES bytes long");
    }
    $pointer = \elephc_sodium_ristretto255_sub($p, \strlen($p), $q, \strlen($q));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_core_ristretto255_scalar_reduce(string $s): string
{
    if (\strlen($s) !== SODIUM_CRYPTO_CORE_RISTRETTO255_NONREDUCEDSCALARBYTES) {
        throw new \SodiumException("sodium_crypto_core_ristretto255_scalar_reduce(): Argument #1 (\$s) must be SODIUM_CRYPTO_CORE_RISTRETTO255_NONREDUCEDSCALARBYTES bytes long");
    }
    $pointer = \elephc_sodium_ristretto255_scalar_reduce($s, \strlen($s));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_core_ristretto255_scalar_reduce(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_core_ristretto255_scalar_random(): string
{
    return \sodium_crypto_core_ristretto255_scalar_reduce(\random_bytes(SODIUM_CRYPTO_CORE_RISTRETTO255_NONREDUCEDSCALARBYTES));
}

function sodium_crypto_core_ristretto255_random(): string
{
    return \sodium_crypto_scalarmult_ristretto255_base(\sodium_crypto_core_ristretto255_scalar_random());
}

function sodium_crypto_core_ristretto255_scalar_add(string $x, string $y): string
{
    $pointer = \elephc_sodium_ristretto255_scalar_add($x, \strlen($x), $y, \strlen($y));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_core_ristretto255_scalar_sub(string $x, string $y): string
{
    $pointer = \elephc_sodium_ristretto255_scalar_sub($x, \strlen($x), $y, \strlen($y));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_core_ristretto255_scalar_mul(string $x, string $y): string
{
    $pointer = \elephc_sodium_ristretto255_scalar_mul($x, \strlen($x), $y, \strlen($y));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_core_ristretto255_scalar_negate(string $s): string
{
    $pointer = \elephc_sodium_ristretto255_scalar_negate($s, \strlen($s));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_core_ristretto255_scalar_complement(string $s): string
{
    $pointer = \elephc_sodium_ristretto255_scalar_complement($s, \strlen($s));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_core_ristretto255_scalar_invert(string $s): string
{
    $pointer = \elephc_sodium_ristretto255_scalar_invert($s, \strlen($s));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_scalarmult_ristretto255(string $n, string $p): string
{
    if (\strlen($n) !== SODIUM_CRYPTO_SCALARMULT_RISTRETTO255_SCALARBYTES) {
        throw new \SodiumException("sodium_crypto_scalarmult_ristretto255(): Argument #1 (\$n) must be SODIUM_CRYPTO_SCALARMULT_RISTRETTO255_SCALARBYTES bytes long");
    }
    if (\strlen($p) !== SODIUM_CRYPTO_SCALARMULT_RISTRETTO255_BYTES) {
        throw new \SodiumException("sodium_crypto_scalarmult_ristretto255(): Argument #2 (\$p) must be SODIUM_CRYPTO_SCALARMULT_RISTRETTO255_BYTES bytes long");
    }
    $pointer = \elephc_sodium_ristretto255_scalarmult($n, \strlen($n), $p, \strlen($p));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("Result is identity element");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_scalarmult_ristretto255_base(string $n): string
{
    if (\strlen($n) !== SODIUM_CRYPTO_SCALARMULT_RISTRETTO255_SCALARBYTES) {
        throw new \SodiumException("sodium_crypto_scalarmult_ristretto255_base(): Argument #1 (\$n) must be SODIUM_CRYPTO_SCALARMULT_RISTRETTO255_SCALARBYTES bytes long");
    }
    if ($n === \str_repeat("\0", SODIUM_CRYPTO_SCALARMULT_RISTRETTO255_SCALARBYTES)) {
        throw new \SodiumException("sodium_crypto_scalarmult_ristretto255_base(): Argument #1 (\$n) must not be zero");
    }
    $pointer = \elephc_sodium_ristretto255_scalarmult_base($n, \strlen($n));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("Result is identity element");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_aead_aegis128l_encrypt(string $message, string $additional_data, string $nonce, string $key): string
{
    if (\strlen($nonce) !== SODIUM_CRYPTO_AEAD_AEGIS128L_NPUBBYTES) {
        throw new \SodiumException("sodium_crypto_aead_aegis128l_encrypt(): Argument #3 (\$nonce) must be SODIUM_CRYPTO_AEAD_AEGIS128L_NPUBBYTES bytes long");
    }
    if (\strlen($key) !== SODIUM_CRYPTO_AEAD_AEGIS128L_KEYBYTES) {
        throw new \SodiumException("sodium_crypto_aead_aegis128l_encrypt(): Argument #4 (\$key) must be SODIUM_CRYPTO_AEAD_AEGIS128L_KEYBYTES bytes long");
    }
    $pointer = \elephc_sodium_aead_aegis128l_encrypt($message, \strlen($message), $additional_data, \strlen($additional_data), $nonce, \strlen($nonce), $key, \strlen($key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_aead_aegis128l_encrypt(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_aead_aegis128l_decrypt(string $ciphertext, string $additional_data, string $nonce, string $key): string|false
{
    if (\strlen($nonce) !== SODIUM_CRYPTO_AEAD_AEGIS128L_NPUBBYTES) {
        throw new \SodiumException("sodium_crypto_aead_aegis128l_decrypt(): Argument #3 (\$nonce) must be SODIUM_CRYPTO_AEAD_AEGIS128L_NPUBBYTES bytes long");
    }
    if (\strlen($key) !== SODIUM_CRYPTO_AEAD_AEGIS128L_KEYBYTES) {
        throw new \SodiumException("sodium_crypto_aead_aegis128l_decrypt(): Argument #4 (\$key) must be SODIUM_CRYPTO_AEAD_AEGIS128L_KEYBYTES bytes long");
    }
    if (\strlen($ciphertext) < SODIUM_CRYPTO_AEAD_AEGIS128L_ABYTES) {
        return false;
    }
    $pointer = \elephc_sodium_aead_aegis128l_decrypt($ciphertext, \strlen($ciphertext), $additional_data, \strlen($additional_data), $nonce, \strlen($nonce), $key, \strlen($key));
    if (\__elephc_ptr_is_null($pointer)) {
        return false;
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_aead_aegis128l_keygen(): string
{
    return \random_bytes(SODIUM_CRYPTO_AEAD_AEGIS128L_KEYBYTES);
}

function sodium_crypto_aead_aegis256_encrypt(string $message, string $additional_data, string $nonce, string $key): string
{
    if (\strlen($nonce) !== SODIUM_CRYPTO_AEAD_AEGIS256_NPUBBYTES) {
        throw new \SodiumException("sodium_crypto_aead_aegis256_encrypt(): Argument #3 (\$nonce) must be SODIUM_CRYPTO_AEAD_AEGIS256_NPUBBYTES bytes long");
    }
    if (\strlen($key) !== SODIUM_CRYPTO_AEAD_AEGIS256_KEYBYTES) {
        throw new \SodiumException("sodium_crypto_aead_aegis256_encrypt(): Argument #4 (\$key) must be SODIUM_CRYPTO_AEAD_AEGIS256_KEYBYTES bytes long");
    }
    $pointer = \elephc_sodium_aead_aegis256_encrypt($message, \strlen($message), $additional_data, \strlen($additional_data), $nonce, \strlen($nonce), $key, \strlen($key));
    if (\__elephc_ptr_is_null($pointer)) {
        throw new \SodiumException("sodium_crypto_aead_aegis256_encrypt(): internal error");
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_aead_aegis256_decrypt(string $ciphertext, string $additional_data, string $nonce, string $key): string|false
{
    if (\strlen($nonce) !== SODIUM_CRYPTO_AEAD_AEGIS256_NPUBBYTES) {
        throw new \SodiumException("sodium_crypto_aead_aegis256_decrypt(): Argument #3 (\$nonce) must be SODIUM_CRYPTO_AEAD_AEGIS256_NPUBBYTES bytes long");
    }
    if (\strlen($key) !== SODIUM_CRYPTO_AEAD_AEGIS256_KEYBYTES) {
        throw new \SodiumException("sodium_crypto_aead_aegis256_decrypt(): Argument #4 (\$key) must be SODIUM_CRYPTO_AEAD_AEGIS256_KEYBYTES bytes long");
    }
    if (\strlen($ciphertext) < SODIUM_CRYPTO_AEAD_AEGIS256_ABYTES) {
        return false;
    }
    $pointer = \elephc_sodium_aead_aegis256_decrypt($ciphertext, \strlen($ciphertext), $additional_data, \strlen($additional_data), $nonce, \strlen($nonce), $key, \strlen($key));
    if (\__elephc_ptr_is_null($pointer)) {
        return false;
    }
    return \__elephc_ptr_read_string($pointer, \elephc_sodium_last_len());
}

function sodium_crypto_aead_aegis256_keygen(): string
{
    return \random_bytes(SODIUM_CRYPTO_AEAD_AEGIS256_KEYBYTES);
}
