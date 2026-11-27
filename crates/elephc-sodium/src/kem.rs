//! Purpose:
//! Pure-Rust X-Wing and ML-KEM-768 bindings for PHP 8.6's `sodium_crypto_kem_*`
//! surface.
//!
//! Called from:
//! - `crate::` C ABI wrappers.
//!
//! Key details:
//! - X-Wing is the hybrid ML-KEM-768 + X25519 KEM; ML-KEM-768 is FIPS 203.
//! - Keypair byte strings use libsodium's `secret || public` layout, and
//!   `enc` publishes libsodium's `ciphertext || shared_secret` pair for the PHP
//!   wrapper to split into its two-element array.
//! - ML-KEM secret keys use the expanded 2400-byte encoding PHP exposes, not
//!   the shorter 64-byte seed.

#[allow(deprecated)]
use ml_kem::{
    ml_kem_768::{Ciphertext as MlKemCiphertext, DecapsulationKey as MlKemSecretKey,
                 EncapsulationKey as MlKemPublicKey},
    Encapsulate as _, ExpandedDecapsulationKey, ExpandedKeyEncoding, FromSeed, Kem as _, Key,
    KeyExport as _, MlKem768, TryKeyInit,
};
use x_wing::{
    kem::{Decapsulate as _, Decapsulator, TryKeyInit as XwingTryKeyInit},
    DecapsulationKey as XwingSecretKey, EncapsulationKey as XwingPublicKey, XWingKem,
};

/// X-Wing public key bytes.
pub(crate) const XWING_PUBLICKEYBYTES: usize = 1216;
/// X-Wing secret key bytes.
pub(crate) const XWING_SECRETKEYBYTES: usize = 32;
/// X-Wing ciphertext bytes.
pub(crate) const XWING_CIPHERTEXTBYTES: usize = 1120;
/// X-Wing shared-secret bytes.
pub(crate) const XWING_SHAREDSECRETBYTES: usize = 32;
/// X-Wing seed bytes.
pub(crate) const XWING_SEEDBYTES: usize = 32;

/// ML-KEM-768 public key bytes.
pub(crate) const MLKEM768_PUBLICKEYBYTES: usize = 1184;
/// ML-KEM-768 expanded secret key bytes.
pub(crate) const MLKEM768_SECRETKEYBYTES: usize = 2400;
/// ML-KEM-768 ciphertext bytes.
pub(crate) const MLKEM768_CIPHERTEXTBYTES: usize = 1088;
/// ML-KEM-768 shared-secret bytes.
pub(crate) const MLKEM768_SHAREDSECRETBYTES: usize = 32;
/// ML-KEM-768 seed bytes.
pub(crate) const MLKEM768_SEEDBYTES: usize = 64;

/// Builds the `secret || public` X-Wing keypair.
fn xwing_keypair(secret: &XwingSecretKey, public: &XwingPublicKey) -> Vec<u8> {
    let mut out = Vec::with_capacity(XWING_SECRETKEYBYTES + XWING_PUBLICKEYBYTES);
    out.extend_from_slice(secret.to_bytes().as_ref());
    out.extend_from_slice(public.to_bytes().as_ref());
    out
}

/// Generates a random X-Wing keypair.
pub(crate) fn xwing_keypair_random() -> Vec<u8> {
    let (secret, public) = XWingKem::generate_keypair();
    xwing_keypair(&secret, &public)
}

/// Derives an X-Wing keypair from a 32-byte seed.
pub(crate) fn xwing_seed_keypair(seed: &[u8]) -> Option<Vec<u8>> {
    let seed: [u8; XWING_SEEDBYTES] = seed.try_into().ok()?;
    let secret = XwingSecretKey::from(seed);
    let public = secret.encapsulation_key().clone();
    Some(xwing_keypair(&secret, &public))
}

/// Extracts the X-Wing secret key from a keypair.
pub(crate) fn xwing_secretkey(keypair: &[u8]) -> Option<Vec<u8>> {
    if keypair.len() != XWING_SECRETKEYBYTES + XWING_PUBLICKEYBYTES {
        return None;
    }
    Some(keypair[..XWING_SECRETKEYBYTES].to_vec())
}

/// Extracts the X-Wing public key from a keypair.
pub(crate) fn xwing_publickey(keypair: &[u8]) -> Option<Vec<u8>> {
    if keypair.len() != XWING_SECRETKEYBYTES + XWING_PUBLICKEYBYTES {
        return None;
    }
    Some(keypair[XWING_SECRETKEYBYTES..].to_vec())
}

/// Encapsulates to an X-Wing public key, returning `ciphertext || shared_secret`.
pub(crate) fn xwing_enc(public_key: &[u8]) -> Option<Vec<u8>> {
    let key = Key::<XwingPublicKey>::try_from(public_key).ok()?;
    let public = <XwingPublicKey as XwingTryKeyInit>::new(&key).ok()?;
    let (ciphertext, shared) = public.encapsulate();
    let mut out = Vec::with_capacity(XWING_CIPHERTEXTBYTES + XWING_SHAREDSECRETBYTES);
    out.extend_from_slice(ciphertext.as_ref());
    out.extend_from_slice(shared.as_ref());
    Some(out)
}

/// Decapsulates an X-Wing ciphertext.
pub(crate) fn xwing_dec(ciphertext: &[u8], secret_key: &[u8]) -> Option<Vec<u8>> {
    let ciphertext: [u8; XWING_CIPHERTEXTBYTES] = ciphertext.try_into().ok()?;
    let secret: [u8; XWING_SECRETKEYBYTES] = secret_key.try_into().ok()?;
    let secret = XwingSecretKey::from(secret);
    let shared = secret.decapsulate(&ciphertext.into());
    let shared: &[u8] = shared.as_ref();
    Some(shared.to_vec())
}

/// Builds the `secret || public` ML-KEM-768 keypair.
fn mlkem_keypair(secret: &MlKemSecretKey, public: &MlKemPublicKey) -> Vec<u8> {
    #[allow(deprecated)]
    let expanded = secret.to_expanded_bytes();
    let mut out = Vec::with_capacity(MLKEM768_SECRETKEYBYTES + MLKEM768_PUBLICKEYBYTES);
    out.extend_from_slice(expanded.as_ref());
    out.extend_from_slice(public.to_bytes().as_ref());
    out
}

/// Generates a random ML-KEM-768 keypair.
pub(crate) fn mlkem_keypair_random() -> Vec<u8> {
    let (secret, public) = MlKem768::generate_keypair();
    mlkem_keypair(&secret, &public)
}

/// Derives an ML-KEM-768 keypair from a 64-byte seed.
pub(crate) fn mlkem_seed_keypair(seed: &[u8]) -> Option<Vec<u8>> {
    if seed.len() != MLKEM768_SEEDBYTES {
        return None;
    }
    let seed = ml_kem::array::Array::try_from(seed).ok()?;
    let (secret, public) = MlKem768::from_seed(&seed);
    Some(mlkem_keypair(&secret, &public))
}

/// Extracts the ML-KEM-768 expanded secret key from a keypair.
pub(crate) fn mlkem_secretkey(keypair: &[u8]) -> Option<Vec<u8>> {
    if keypair.len() != MLKEM768_SECRETKEYBYTES + MLKEM768_PUBLICKEYBYTES {
        return None;
    }
    Some(keypair[..MLKEM768_SECRETKEYBYTES].to_vec())
}

/// Extracts the ML-KEM-768 public key from a keypair.
pub(crate) fn mlkem_publickey(keypair: &[u8]) -> Option<Vec<u8>> {
    if keypair.len() != MLKEM768_SECRETKEYBYTES + MLKEM768_PUBLICKEYBYTES {
        return None;
    }
    Some(keypair[MLKEM768_SECRETKEYBYTES..].to_vec())
}

/// Encapsulates to an ML-KEM-768 public key, returning `ciphertext || shared_secret`.
pub(crate) fn mlkem_enc(public_key: &[u8]) -> Option<Vec<u8>> {
    let key = Key::<MlKemPublicKey>::try_from(public_key).ok()?;
    let public = <MlKemPublicKey as TryKeyInit>::new(&key).ok()?;
    let (ciphertext, shared) = public.encapsulate();
    let mut out = Vec::with_capacity(MLKEM768_CIPHERTEXTBYTES + MLKEM768_SHAREDSECRETBYTES);
    out.extend_from_slice(ciphertext.as_ref());
    out.extend_from_slice(shared.as_ref());
    Some(out)
}

/// Decapsulates an ML-KEM-768 ciphertext.
pub(crate) fn mlkem_dec(ciphertext: &[u8], secret_key: &[u8]) -> Option<Vec<u8>> {
    #[allow(deprecated)]
    let expanded = ExpandedDecapsulationKey::<MlKem768>::try_from(secret_key).ok()?;
    #[allow(deprecated)]
    let secret = MlKemSecretKey::from_expanded_bytes(&expanded).ok()?;
    let ciphertext = MlKemCiphertext::try_from(ciphertext).ok()?;
    let shared = secret.decapsulate(&ciphertext);
    let shared: &[u8] = shared.as_ref();
    Some(shared.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// X-Wing and ML-KEM-768 keypairs have libsodium's lengths and round-trip.
    #[test]
    fn xwing_and_mlkem_round_trip() {
        let keypair = xwing_keypair_random();
        assert_eq!(keypair.len(), XWING_SECRETKEYBYTES + XWING_PUBLICKEYBYTES);
        let secret = xwing_secretkey(&keypair).unwrap();
        let public = xwing_publickey(&keypair).unwrap();
        assert_eq!(secret.len(), XWING_SECRETKEYBYTES);
        assert_eq!(public.len(), XWING_PUBLICKEYBYTES);
        let encapsulated = xwing_enc(&public).unwrap();
        assert_eq!(
            encapsulated.len(),
            XWING_CIPHERTEXTBYTES + XWING_SHAREDSECRETBYTES
        );
        let shared = xwing_dec(
            &encapsulated[..XWING_CIPHERTEXTBYTES],
            &secret,
        )
        .unwrap();
        assert_eq!(
            shared.as_slice(),
            &encapsulated[XWING_CIPHERTEXTBYTES..]
        );

        let seed = [7u8; XWING_SEEDBYTES];
        let first = xwing_seed_keypair(&seed).unwrap();
        let second = xwing_seed_keypair(&seed).unwrap();
        assert_eq!(first, second);

        let keypair = mlkem_keypair_random();
        assert_eq!(
            keypair.len(),
            MLKEM768_SECRETKEYBYTES + MLKEM768_PUBLICKEYBYTES
        );
        let secret = mlkem_secretkey(&keypair).unwrap();
        let public = mlkem_publickey(&keypair).unwrap();
        assert_eq!(secret.len(), MLKEM768_SECRETKEYBYTES);
        assert_eq!(public.len(), MLKEM768_PUBLICKEYBYTES);
        let encapsulated = mlkem_enc(&public).unwrap();
        assert_eq!(
            encapsulated.len(),
            MLKEM768_CIPHERTEXTBYTES + MLKEM768_SHAREDSECRETBYTES
        );
        let shared = mlkem_dec(
            &encapsulated[..MLKEM768_CIPHERTEXTBYTES],
            &secret,
        )
        .unwrap();
        assert_eq!(
            shared.as_slice(),
            &encapsulated[MLKEM768_CIPHERTEXTBYTES..]
        );

        let seed = [9u8; MLKEM768_SEEDBYTES];
        let first = mlkem_seed_keypair(&seed).unwrap();
        let second = mlkem_seed_keypair(&seed).unwrap();
        assert_eq!(first, second);
    }
}
