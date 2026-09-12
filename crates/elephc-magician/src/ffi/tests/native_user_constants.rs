//! Purpose:
//! Tests the AOT user-constant registration ABI and its separation from the
//! Core global-constant registry.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Array specs reuse the shared native array-default record encoder.
//! - Invalid handles, ABI versions, kinds, and truncated specs must fail closed.
//! - Object-valued elements stay decodable for callable defaults but must be
//!   rejected, at any depth, by user constant registration.

use super::*;
use crate::context::{EvalNativeGlobalConstant, EvalNativeUserConstant};

const USER_CONSTANT_NULL: u64 = 0;
const USER_CONSTANT_BOOL: u64 = 1;
const USER_CONSTANT_INT: u64 = 2;
const USER_CONSTANT_FLOAT: u64 = 3;
const USER_CONSTANT_STRING: u64 = 4;

/// Registers one scalar user constant through the exported ABI.
fn register_scalar(
    ctx: &mut ElephcEvalContext,
    name: &str,
    kind: u64,
    word: u64,
    len: u64,
) -> i32 {
    unsafe {
        __elephc_eval_register_native_user_constant(
            ctx,
            name.as_ptr(),
            name.len() as u64,
            kind,
            word,
            len,
        )
    }
}

/// Registers one array user constant through the exported ABI.
fn register_array(ctx: &mut ElephcEvalContext, name: &str, spec: &[u8]) -> i32 {
    unsafe {
        __elephc_eval_register_native_user_constant_array(
            ctx,
            name.as_ptr(),
            name.len() as u64,
            spec.as_ptr(),
            spec.len() as u64,
        )
    }
}

/// Verifies every scalar shape survives the user-constant ABI with its exact PHP value.
#[test]
fn user_constant_registration_preserves_scalar_metadata() {
    let mut ctx = ElephcEvalContext::new();
    let text = b"seeded";

    assert_eq!(register_scalar(&mut ctx, "USER_NULL", USER_CONSTANT_NULL, 0, 0), 1);
    assert_eq!(register_scalar(&mut ctx, "USER_TRUE", USER_CONSTANT_BOOL, 1, 0), 1);
    assert_eq!(
        register_scalar(&mut ctx, "USER_INT", USER_CONSTANT_INT, (-7_i64) as u64, 0),
        1
    );
    assert_eq!(
        register_scalar(
            &mut ctx,
            "USER_FLOAT",
            USER_CONSTANT_FLOAT,
            (-1.5_f64).to_bits(),
            0
        ),
        1
    );
    assert_eq!(
        register_scalar(
            &mut ctx,
            "USER_STRING",
            USER_CONSTANT_STRING,
            text.as_ptr() as u64,
            text.len() as u64
        ),
        1
    );

    assert_eq!(
        ctx.native_user_constant("USER_NULL"),
        Some(&EvalNativeUserConstant::Scalar(EvalNativeGlobalConstant::Null))
    );
    assert_eq!(
        ctx.native_user_constant("USER_TRUE"),
        Some(&EvalNativeUserConstant::Scalar(EvalNativeGlobalConstant::Bool(
            true
        )))
    );
    assert_eq!(
        ctx.native_user_constant("USER_INT"),
        Some(&EvalNativeUserConstant::Scalar(EvalNativeGlobalConstant::Int(
            -7
        )))
    );
    assert_eq!(
        ctx.native_user_constant("USER_FLOAT"),
        Some(&EvalNativeUserConstant::Scalar(
            EvalNativeGlobalConstant::Float(-1.5)
        ))
    );
    assert_eq!(
        ctx.native_user_constant("USER_STRING"),
        Some(&EvalNativeUserConstant::Scalar(
            EvalNativeGlobalConstant::String("seeded".to_string())
        ))
    );
    // A leading namespace separator resolves to the same global name, like every other
    // eval constant lookup.
    assert!(ctx.has_native_user_constant("\\USER_INT"));
    // User names must not leak into the Core registry.
    assert!(!ctx.has_native_global_constant("USER_INT"));
}

/// Verifies nested array constants keep their keys, duplicates, and element types.
#[test]
fn user_constant_registration_preserves_nested_array_metadata() {
    let mut ctx = ElephcEvalContext::new();
    let nested = vec![
        NativeCallableArrayDefaultElement::keyed(
            NativeCallableArrayDefaultKey::Int(0),
            NativeCallableDefault::String("first".to_string()),
        ),
        NativeCallableArrayDefaultElement::keyed(
            NativeCallableArrayDefaultKey::Int(1),
            NativeCallableDefault::Float(-0.25),
        ),
    ];
    let elements = vec![
        NativeCallableArrayDefaultElement::keyed(
            NativeCallableArrayDefaultKey::String("items".to_string()),
            NativeCallableDefault::Array(nested),
        ),
        NativeCallableArrayDefaultElement::keyed(
            NativeCallableArrayDefaultKey::Int(-3),
            NativeCallableDefault::Null,
        ),
        // A repeated key is retained verbatim; PHP's last-wins collapsing happens when the
        // value is materialized, exactly as the native inventory collapses it.
        NativeCallableArrayDefaultElement::keyed(
            NativeCallableArrayDefaultKey::Int(-3),
            NativeCallableDefault::Bool(true),
        ),
    ];
    let spec = native_array_default_record(&elements);

    assert_eq!(register_array(&mut ctx, "USER_TABLE", &spec), 1);
    assert_eq!(
        ctx.native_user_constant("USER_TABLE"),
        Some(&EvalNativeUserConstant::Array(elements))
    );
}

/// Verifies an empty array constant registers as an empty element list.
#[test]
fn user_constant_registration_accepts_an_empty_array() {
    let mut ctx = ElephcEvalContext::new();
    let spec = native_array_default_record(&[]);

    assert_eq!(register_array(&mut ctx, "USER_EMPTY", &spec), 1);
    assert_eq!(
        ctx.native_user_constant("USER_EMPTY"),
        Some(&EvalNativeUserConstant::Array(Vec::new()))
    );
}

/// Verifies a seeded user name blocks a later eval `define()` of the same constant.
#[test]
fn user_constant_registration_blocks_a_duplicate_dynamic_define() {
    let mut ctx = ElephcEvalContext::new();
    let mut payload = 1_u64;
    let cell = RuntimeCellHandle::from_raw((&mut payload as *mut u64).cast());

    assert_eq!(register_scalar(&mut ctx, "USER_INT", USER_CONSTANT_INT, 1, 0), 1);

    assert!(!ctx.define_constant("USER_INT", cell));
    // The dynamic inventory that flows back into native `get_defined_constants()` must
    // stay eval-only.
    assert!(ctx.constant_inventory_entry(0).is_none());
    // A second registration of the same name is rejected, so seeding twice cannot
    // duplicate an entry.
    assert_eq!(register_scalar(&mut ctx, "USER_INT", USER_CONSTANT_INT, 2, 0), 0);
}

/// Verifies an object element is rejected at every depth while the shared codec keeps it.
///
/// The binary spec is the one native callable array defaults use, and that surface does
/// materialize object values, so the decoder must keep accepting the same bytes. Only the
/// user constant registration path, which has no eval context to construct an instance
/// with, refuses them.
#[test]
fn user_constant_registration_rejects_object_valued_elements() {
    let mut ctx = ElephcEvalContext::new();
    let object = NativeCallableDefault::Object {
        class_name: "Seeded".to_string(),
        args: vec![NativeCallableObjectDefaultArg::positional(
            NativeCallableDefault::Int(1),
        )],
    };
    let top_level = vec![NativeCallableArrayDefaultElement::keyed(
        NativeCallableArrayDefaultKey::Int(0),
        object.clone(),
    )];
    let nested = vec![NativeCallableArrayDefaultElement::keyed(
        NativeCallableArrayDefaultKey::String("outer".to_string()),
        NativeCallableDefault::Array(vec![NativeCallableArrayDefaultElement::keyed(
            NativeCallableArrayDefaultKey::String("inner".to_string()),
            NativeCallableDefault::Array(vec![NativeCallableArrayDefaultElement::keyed(
                NativeCallableArrayDefaultKey::Int(0),
                object,
            )]),
        )]),
    )];
    let top_level_spec = native_array_default_record(&top_level);
    let nested_spec = native_array_default_record(&nested);

    assert_eq!(register_array(&mut ctx, "USER_OBJECT", &top_level_spec), 0);
    assert_eq!(register_array(&mut ctx, "USER_NESTED", &nested_spec), 0);
    assert!(!ctx.has_native_user_constant("USER_OBJECT"));
    assert!(!ctx.has_native_user_constant("USER_NESTED"));

    // The shared decoder is untouched: both specs still decode for callable defaults.
    let decoded_top_level = unsafe {
        native_callable_array_default(top_level_spec.as_ptr(), top_level_spec.len() as u64)
    };
    let decoded_nested =
        unsafe { native_callable_array_default(nested_spec.as_ptr(), nested_spec.len() as u64) };
    assert_eq!(
        decoded_top_level,
        Some(NativeCallableDefault::Array(top_level))
    );
    assert_eq!(decoded_nested, Some(NativeCallableDefault::Array(nested)));
}

/// Verifies a name an eval fragment already defined cannot be re-seeded, and stays intact.
///
/// AOT seeds a newly created context once. The exported registration API must also reject
/// a late caller attempting to replace a name a fragment has already defined.
/// The existing dynamic entry survives unchanged.
#[test]
fn user_constant_registration_rejects_an_existing_dynamic_constant() {
    let mut ctx = ElephcEvalContext::new();
    let mut payload = 1_u64;
    let cell = RuntimeCellHandle::from_raw((&mut payload as *mut u64).cast());

    assert!(ctx.define_constant("EVAL_ONE", cell));
    assert_eq!(register_scalar(&mut ctx, "EVAL_ONE", USER_CONSTANT_INT, 2, 0), 0);
    let spec = native_array_default_record(&[NativeCallableArrayDefaultElement::keyed(
        NativeCallableArrayDefaultKey::Int(0),
        NativeCallableDefault::Int(2),
    )]);
    assert_eq!(register_array(&mut ctx, "EVAL_ONE", &spec), 0);

    assert!(!ctx.has_native_user_constant("EVAL_ONE"));
    // The original dynamic entry is preserved untouched, cell identity included.
    assert_eq!(ctx.constant("EVAL_ONE"), Some(cell));
    assert_eq!(
        ctx.constant_inventory_entry(0).map(|(name, _)| name),
        Some("EVAL_ONE")
    );
}

/// Verifies invalid handles, ABI versions, kinds, names, and specs fail closed.
#[test]
fn user_constant_registration_rejects_invalid_abi_input() {
    let mut ctx = ElephcEvalContext::new();
    let mut stale = ElephcEvalContext::for_abi_version(ABI_VERSION + 1);
    let spec = native_array_default_record(&[NativeCallableArrayDefaultElement::keyed(
        NativeCallableArrayDefaultKey::Int(0),
        NativeCallableDefault::Int(1),
    )]);

    let null_handle = unsafe {
        __elephc_eval_register_native_user_constant(
            std::ptr::null_mut(),
            b"USER_X".as_ptr(),
            6,
            USER_CONSTANT_INT,
            1,
            0,
        )
    };
    let null_array_handle = unsafe {
        __elephc_eval_register_native_user_constant_array(
            std::ptr::null_mut(),
            b"USER_X".as_ptr(),
            6,
            spec.as_ptr(),
            spec.len() as u64,
        )
    };

    assert_eq!(null_handle, 0);
    assert_eq!(null_array_handle, 0);
    assert_eq!(register_scalar(&mut stale, "USER_X", USER_CONSTANT_INT, 1, 0), 0);
    assert_eq!(register_array(&mut stale, "USER_X", &spec), 0);
    // Unknown scalar kind.
    assert_eq!(register_scalar(&mut ctx, "USER_X", 9, 1, 0), 0);
    // Empty name.
    assert_eq!(register_scalar(&mut ctx, "", USER_CONSTANT_INT, 1, 0), 0);
    // Truncated spec.
    assert_eq!(register_array(&mut ctx, "USER_X", &spec[..spec.len() - 1]), 0);
    // Trailing bytes after a complete spec.
    let mut trailing = spec.clone();
    trailing.push(0);
    assert_eq!(register_array(&mut ctx, "USER_X", &trailing), 0);
    assert!(!ctx.has_native_user_constant("USER_X"));
}

/// Verifies null pointers carrying a nonzero length fail closed instead of being read.
///
/// Both exported entry points validate the pointer before building any slice, so a null
/// name or payload pointer with a nonzero declared length is a rejection, not a read. This
/// is the only invalid-pointer shape the unsafe ABI contract lets a test exercise; a
/// dangling non-null pointer would be undefined behavior to pass.
#[test]
fn user_constant_registration_rejects_null_pointers_with_a_nonzero_length() {
    let mut ctx = ElephcEvalContext::new();

    let null_string_payload = unsafe {
        __elephc_eval_register_native_user_constant(
            &mut ctx,
            b"USER_X".as_ptr(),
            6,
            USER_CONSTANT_STRING,
            0,
            6,
        )
    };
    let null_name = unsafe {
        __elephc_eval_register_native_user_constant(
            &mut ctx,
            std::ptr::null(),
            6,
            USER_CONSTANT_INT,
            1,
            0,
        )
    };
    let empty_spec = native_array_default_record(&[]);
    let null_array_name = unsafe {
        __elephc_eval_register_native_user_constant_array(
            &mut ctx,
            std::ptr::null(),
            6,
            empty_spec.as_ptr(),
            empty_spec.len() as u64,
        )
    };
    let null_spec = unsafe {
        __elephc_eval_register_native_user_constant_array(
            &mut ctx,
            b"USER_X".as_ptr(),
            6,
            std::ptr::null(),
            8,
        )
    };

    assert_eq!(null_string_payload, 0);
    assert_eq!(null_name, 0);
    assert_eq!(null_array_name, 0);
    assert_eq!(null_spec, 0);
    assert!(!ctx.has_native_user_constant("USER_X"));
    // A null name pointer with a zero length decodes to the empty name, which the registry
    // itself rejects even though the array spec is well formed.
    assert_eq!(
        unsafe {
            __elephc_eval_register_native_user_constant_array(
                &mut ctx,
                std::ptr::null(),
                0,
                empty_spec.as_ptr(),
                empty_spec.len() as u64,
            )
        },
        0
    );
}

/// Verifies array specs carrying an unknown key or value tag are rejected outright.
#[test]
fn user_constant_registration_rejects_unknown_spec_tags() {
    let mut ctx = ElephcEvalContext::new();

    // One element whose key tag is not auto/int/string.
    let mut unknown_key = Vec::new();
    unknown_key.extend_from_slice(&1_u32.to_le_bytes());
    unknown_key.push(9);
    unknown_key.push(TEST_NATIVE_OBJECT_DEFAULT_ARG_SCALAR);
    unknown_key.extend_from_slice(&TEST_NATIVE_DEFAULT_INT.to_le_bytes());
    unknown_key.extend_from_slice(&1_u64.to_le_bytes());

    // One auto-keyed element whose value tag is not scalar/string/object/array.
    let mut unknown_value = Vec::new();
    unknown_value.extend_from_slice(&1_u32.to_le_bytes());
    unknown_value.push(TEST_NATIVE_ARRAY_DEFAULT_KEY_AUTO);
    unknown_value.push(9);

    // One auto-keyed scalar element whose scalar kind is not a known default kind.
    let mut unknown_scalar_kind = Vec::new();
    unknown_scalar_kind.extend_from_slice(&1_u32.to_le_bytes());
    unknown_scalar_kind.push(TEST_NATIVE_ARRAY_DEFAULT_KEY_AUTO);
    unknown_scalar_kind.push(TEST_NATIVE_OBJECT_DEFAULT_ARG_SCALAR);
    unknown_scalar_kind.extend_from_slice(&9_u64.to_le_bytes());
    unknown_scalar_kind.extend_from_slice(&0_u64.to_le_bytes());

    // A declared element count the spec body cannot satisfy.
    let mut overlong_count = Vec::new();
    overlong_count.extend_from_slice(&4_u32.to_le_bytes());
    overlong_count.push(TEST_NATIVE_ARRAY_DEFAULT_KEY_AUTO);
    overlong_count.push(TEST_NATIVE_OBJECT_DEFAULT_ARG_SCALAR);
    overlong_count.extend_from_slice(&TEST_NATIVE_DEFAULT_INT.to_le_bytes());
    overlong_count.extend_from_slice(&1_u64.to_le_bytes());

    assert_eq!(register_array(&mut ctx, "USER_KEY", &unknown_key), 0);
    assert_eq!(register_array(&mut ctx, "USER_VALUE", &unknown_value), 0);
    assert_eq!(
        register_array(&mut ctx, "USER_KIND", &unknown_scalar_kind),
        0
    );
    assert_eq!(register_array(&mut ctx, "USER_COUNT", &overlong_count), 0);
    assert!(!ctx.has_native_user_constant("USER_KEY"));
    assert!(!ctx.has_native_user_constant("USER_VALUE"));
    assert!(!ctx.has_native_user_constant("USER_KIND"));
    assert!(!ctx.has_native_user_constant("USER_COUNT"));
}

/// Verifies a name already seeded as a Core constant cannot be re-seeded as a user one.
#[test]
fn user_constant_registration_does_not_shadow_a_core_constant() {
    let mut ctx = ElephcEvalContext::new();

    assert_eq!(
        unsafe {
            __elephc_eval_register_native_global_constant(
                &mut ctx,
                b"PHP_INT_SIZE".as_ptr(),
                12,
                USER_CONSTANT_INT,
                8,
                0,
            )
        },
        1
    );

    assert_eq!(
        register_scalar(&mut ctx, "PHP_INT_SIZE", USER_CONSTANT_INT, 4, 0),
        0
    );
    assert_eq!(
        ctx.native_global_constant("PHP_INT_SIZE"),
        Some(&EvalNativeGlobalConstant::Int(8))
    );
    assert!(!ctx.has_native_user_constant("PHP_INT_SIZE"));
}
