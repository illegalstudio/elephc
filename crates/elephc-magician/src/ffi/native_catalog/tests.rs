//! Purpose:
//! Exercises catalog identity, registration failure and context lifetime through the C ABI.
//!
//! Called from:
//! - The Magician unit test harness.
//!
//! Key details:
//! - Tokens model separate modules and storage reused after unloading an image.

use super::*;
use crate::ffi::{__elephc_eval_context_free, __elephc_eval_register_declared_class_name};

/// Repeated activations hit; distinct or newly loaded module generations miss.
#[test]
fn native_catalog_ffi_reuses_only_the_same_module_generation() {
    let mut token = 0u64;
    let mut other_token = 0u64;
    unsafe {
        let first = __elephc_eval_context_new_with_native_catalog(&mut token);
        assert!(!first.is_null());
        assert_ne!(token, 0);
        let context = first.as_ref().expect("catalog context allocation failed");
        assert_eq!(context.abi_version(), ABI_VERSION);
        assert_eq!(__elephc_eval_context_native_catalog_ready(first), 0);
        assert_eq!(__elephc_eval_context_publish_native_catalog(first), 1);
        __elephc_eval_context_free(first);
        let second = __elephc_eval_context_new_with_native_catalog(&mut token);
        assert_eq!(__elephc_eval_context_native_catalog_ready(second), 1);
        __elephc_eval_context_free(second);
        let other = __elephc_eval_context_new_with_native_catalog(&mut other_token);
        assert_eq!(__elephc_eval_context_native_catalog_ready(other), 0);
        __elephc_eval_context_free(other);
        let original_generation = token;
        token = 0;
        let reloaded = __elephc_eval_context_new_with_native_catalog(&mut token);
        assert_ne!(token, original_generation);
        assert_eq!(__elephc_eval_context_native_catalog_ready(reloaded), 0);
        __elephc_eval_context_free(reloaded);
        assert_eq!(__elephc_eval_context_native_catalog_ready(std::ptr::null_mut()), 0);
        assert_eq!(__elephc_eval_context_publish_native_catalog(std::ptr::null_mut()), 0);
    }
}

/// A failed public registration prevents publication of preceding valid metadata.
#[test]
fn native_catalog_ffi_rejects_partial_registration() {
    let mut token = 0u64;
    unsafe {
        let context = __elephc_eval_context_new_with_native_catalog(&mut token);
        let name = b"Probe";
        assert_eq!(__elephc_eval_register_declared_class_name(
            context, name.as_ptr(), name.len() as u64,
        ), 1);
        assert_eq!(__elephc_eval_register_declared_class_name(
            context, std::ptr::null(), 1,
        ), 0);
        assert_eq!(__elephc_eval_context_publish_native_catalog(context), 0);
        __elephc_eval_context_free(context);
        let next = __elephc_eval_context_new_with_native_catalog(&mut token);
        assert_eq!(__elephc_eval_context_native_catalog_ready(next), 0);
        __elephc_eval_context_free(next);
    }
}

/// Catalogs never move opaque native descriptors between threads.
#[test]
fn native_catalog_ffi_starts_a_separate_catalog_on_each_thread() {
    let token = AtomicU64::new(0);
    unsafe {
        let context = __elephc_eval_context_new_with_native_catalog(token.as_ptr());
        assert_eq!(__elephc_eval_context_publish_native_catalog(context), 1);
        __elephc_eval_context_free(context);
    }
    std::thread::scope(|scope| {
        scope.spawn(|| unsafe {
            let context = __elephc_eval_context_new_with_native_catalog(token.as_ptr());
            assert_eq!(__elephc_eval_context_native_catalog_ready(context), 0);
            assert_eq!(__elephc_eval_context_publish_native_catalog(context), 1);
            __elephc_eval_context_free(context);
        }).join().unwrap();
    });
}

/// Invalid tokens preserve the legacy uncached behavior instead of selecting a catalog.
#[test]
fn native_catalog_ffi_invalid_tokens_remain_uncached() {
    unsafe {
        let context = __elephc_eval_context_new_with_native_catalog(std::ptr::null_mut());
        assert!(!context.is_null());
        assert_eq!(__elephc_eval_context_publish_native_catalog(context), 0);
        __elephc_eval_context_free(context);
        let mut storage = [0u64; 2];
        let misaligned = storage.as_mut_ptr().cast::<u8>().add(1).cast::<u64>();
        let context = __elephc_eval_context_new_with_native_catalog(misaligned);
        assert!(!context.is_null());
        assert_eq!(__elephc_eval_context_publish_native_catalog(context), 0);
        __elephc_eval_context_free(context);
        let mut incompatible = ElephcEvalContext::for_abi_version(ABI_VERSION + 1);
        assert_eq!(__elephc_eval_context_native_catalog_ready(&mut incompatible), 0);
        assert_eq!(__elephc_eval_context_publish_native_catalog(&mut incompatible), 0);
    }
}

/// Valid replacements retain their legacy zero status without blocking a complete catalog.
#[test]
fn native_catalog_ffi_distinguishes_replacements_from_rejected_metadata() {
    use crate::ffi::{__elephc_eval_register_native_method,
        __elephc_eval_register_native_property_type,
        __elephc_eval_register_native_property_default_scalar};
    let mut token = 0u64;
    let key = b"Probe::value";
    let ty = b"int";
    unsafe {
        let context = __elephc_eval_context_new_with_native_catalog(&mut token);
        for expected in [1, 0] {
            assert_eq!(__elephc_eval_register_native_method(
                context, key.as_ptr(), key.len() as u64, 0,
            ), expected);
            assert_eq!(__elephc_eval_register_native_property_type(
                context, key.as_ptr(), key.len() as u64, ty.as_ptr(), ty.len() as u64,
            ), expected);
            assert_eq!(__elephc_eval_register_native_property_default_scalar(
                context, key.as_ptr(), key.len() as u64, 2, 7,
            ), expected);
        }
        assert_eq!(__elephc_eval_context_publish_native_catalog(context), 1);
        __elephc_eval_context_free(context);
        token = 0;
        let rejected = __elephc_eval_context_new_with_native_catalog(&mut token);
        assert_eq!(__elephc_eval_register_native_property_type(
            rejected, key.as_ptr(), key.len() as u64, std::ptr::null(), 1,
        ), 0);
        assert_eq!(__elephc_eval_context_publish_native_catalog(rejected), 0);
        __elephc_eval_context_free(rejected);
    }
}
