//! Purpose:
//! Scalar types and numeric constants of the extension ABI: version numbers,
//! zval type codes, flag words, error levels, type masks, access flags, and the
//! codes of the enumerations extensions switch on.
//!
//! Called from:
//! - Every engine module (`use crate::abi::*`), and `build.rs`, which turns
//!   each item here into a C typedef or `#define`.
//!
//! Key details:
//! - Names and values follow PHP 8.5: extension code is written against them
//!   (it compares a zval's type byte with `IS_STRING`, tests `ZEND_ACC_STATIC`).
//! - C enumerations are plain `c_int` aliases with one constant per member, so
//!   both languages agree on their size without a `#[repr]` contract.
//! - Integer constants are literals or operator expressions over other
//!   constants, never casts: the header's `#define`s must stay usable in `#if`.

#![allow(non_camel_case_types, non_upper_case_globals)]

use std::ffi::c_int;

// ------------------------------------------------------------ versions

pub const PHP_MAJOR_VERSION: c_int = 8;
pub const PHP_MINOR_VERSION: c_int = 5;
pub const PHP_RELEASE_VERSION: c_int = 6;
pub const PHP_EXTRA_VERSION: &str = "";
pub const PHP_VERSION: &str = "8.5.6";
pub const PHP_VERSION_ID: c_int = 80506;
pub const ZEND_VERSION: &str = "4.5.6";
pub const ZEND_MODULE_API_NO: c_int = 20250925;
pub const ZEND_EXTENSION_API_NO: c_int = 420250925;
pub const PHP_API_VERSION: c_int = 20250925;
pub const ZEND_MODULE_BUILD_ID: &str = "API20250925,NTS";
pub const ZEND_EXTENSION_BUILD_ID: &str = "API420250925,NTS";
pub const ZEND_DEBUG: c_int = 0;
pub const USING_ZTS: c_int = 0;
pub const PHP_OS: &str = "Elephc";
pub const PHP_OS_FAMILY: &str = "Elephc";
pub const PHP_EOL: &str = "\n";
pub const PHP_DEBUG: c_int = 0;
pub const PHP_SHLIB_SUFFIX: &str = "so";

// ------------------------------------------------------------ scalars

pub type zend_long = i64;
pub type zend_ulong = u64;
pub type zend_off_t = i64;
pub type zend_intptr_t = isize;
pub type zend_uintptr_t = usize;
pub type zend_uchar = u8;
pub type zend_bool = bool;
/// `SUCCESS` or `FAILURE`.
/// c: enum
pub type zend_result = c_int;
pub const SUCCESS: zend_result = 0;
pub const FAILURE: zend_result = -1;

pub type HashPosition = u32;
pub type zend_stat_t = stat;

/// The platform's `struct stat`.
/// c: system struct stat
#[repr(C)]
pub struct stat {
    _private: [u8; 0],
}

/// The platform's `FILE`.
/// c: system FILE
#[repr(C)]
pub struct FILE {
    _private: [u8; 0],
}

/// The platform's `struct dirent`.
/// c: system struct dirent
#[repr(C)]
pub struct dirent {
    _private: [u8; 0],
}

/// The platform's `sigjmp_buf`, the bailout target.
/// c: system sigjmp_buf
#[repr(C)]
pub struct sigjmp_buf {
    _private: [u8; 0],
}

/// The platform's `va_list`, only ever passed by value to C.
/// c: system va_list
#[repr(C)]
pub struct va_list {
    _private: [u8; 0],
}

pub const SIZEOF_ZEND_LONG: c_int = 8;
pub const SIZEOF_SIZE_T: c_int = 8;
pub const ZEND_LONG_MAX: i64 = 0x7fffffffffffffff;
pub const ZEND_LONG_MIN: i64 = -0x7fffffffffffffff - 1;
pub const ZEND_ULONG_MAX: u64 = 0xffffffffffffffff;
pub const ZEND_LTOA_BUF_LEN: c_int = 65;
pub const ZEND_DOUBLE_MAX_LENGTH: c_int = 1080;
pub const MAX_LENGTH_OF_LONG: c_int = 20;
pub const MAX_LENGTH_OF_DOUBLE: c_int = 32;
pub const ZEND_MM_ALIGNMENT: c_int = 8;
pub const ZEND_MM_ALIGNMENT_LOG2: c_int = 3;
pub const ZEND_MAX_RESERVED_RESOURCES: c_int = 6;
pub const MAXPATHLEN: c_int = 4096;

// ------------------------------------------------------------ zval type codes

pub const IS_UNDEF: u8 = 0;
pub const IS_NULL: u8 = 1;
pub const IS_FALSE: u8 = 2;
pub const IS_TRUE: u8 = 3;
pub const IS_LONG: u8 = 4;
pub const IS_DOUBLE: u8 = 5;
pub const IS_STRING: u8 = 6;
pub const IS_ARRAY: u8 = 7;
pub const IS_OBJECT: u8 = 8;
pub const IS_RESOURCE: u8 = 9;
pub const IS_REFERENCE: u8 = 10;
pub const IS_CONSTANT_AST: u8 = 11;
pub const IS_INDIRECT: u8 = 12;
pub const IS_PTR: u8 = 13;
pub const IS_ALIAS_PTR: u8 = 14;
pub const _IS_ERROR: u8 = 15;
/// Pseudo-types: only in type declarations, sharing numbers with the above.
pub const IS_CALLABLE: u8 = 12;
pub const IS_ITERABLE: u8 = 13;
pub const IS_VOID: u8 = 14;
pub const IS_STATIC: u8 = 15;
pub const IS_MIXED: u8 = 16;
pub const IS_NEVER: u8 = 17;
pub const _IS_BOOL: u8 = 18;
pub const _IS_NUMBER: u8 = 19;

pub const Z_TYPE_MASK: u32 = 0xff;
pub const Z_TYPE_FLAGS_MASK: u32 = 0xff00;
pub const Z_TYPE_FLAGS_SHIFT: u32 = 8;
pub const IS_TYPE_REFCOUNTED: u32 = 1;
pub const IS_TYPE_COLLECTABLE: u32 = 2;
pub const IS_STRING_EX: u32 = IS_STRING as u32 | (IS_TYPE_REFCOUNTED << Z_TYPE_FLAGS_SHIFT);
pub const IS_INTERNED_STRING_EX: u32 = IS_STRING as u32;
pub const IS_ARRAY_EX: u32 = IS_ARRAY as u32 | ((IS_TYPE_REFCOUNTED | IS_TYPE_COLLECTABLE) << Z_TYPE_FLAGS_SHIFT);
pub const IS_OBJECT_EX: u32 = IS_OBJECT as u32 | ((IS_TYPE_REFCOUNTED | IS_TYPE_COLLECTABLE) << Z_TYPE_FLAGS_SHIFT);
pub const IS_RESOURCE_EX: u32 = IS_RESOURCE as u32 | (IS_TYPE_REFCOUNTED << Z_TYPE_FLAGS_SHIFT);
pub const IS_REFERENCE_EX: u32 = IS_REFERENCE as u32 | (IS_TYPE_REFCOUNTED << Z_TYPE_FLAGS_SHIFT);
pub const IS_CONSTANT_AST_EX: u32 = IS_CONSTANT_AST as u32 | (IS_TYPE_REFCOUNTED << Z_TYPE_FLAGS_SHIFT);

// ------------------------------------------------------------ refcounted headers

pub const GC_TYPE_MASK: u32 = 0x0000000f;
pub const GC_FLAGS_MASK: u32 = 0x000003f0;
pub const GC_INFO_MASK: u32 = 0xfffffc00;
pub const GC_FLAGS_SHIFT: u32 = 0;
pub const GC_INFO_SHIFT: u32 = 10;
pub const GC_NOT_COLLECTABLE: u32 = 1 << 4;
pub const GC_PROTECTED: u32 = 1 << 5;
pub const GC_IMMUTABLE: u32 = 1 << 6;
pub const GC_PERSISTENT: u32 = 1 << 7;
pub const GC_PERSISTENT_LOCAL: u32 = 1 << 8;
pub const GC_NULL: u32 = IS_NULL as u32 | GC_NOT_COLLECTABLE;
pub const GC_STRING: u32 = IS_STRING as u32 | GC_NOT_COLLECTABLE;
pub const GC_ARRAY: u32 = IS_ARRAY as u32;
pub const GC_OBJECT: u32 = IS_OBJECT as u32;
pub const GC_RESOURCE: u32 = IS_RESOURCE as u32 | GC_NOT_COLLECTABLE;
pub const GC_REFERENCE: u32 = IS_REFERENCE as u32 | GC_NOT_COLLECTABLE;
pub const GC_CONSTANT_AST: u32 = IS_CONSTANT_AST as u32 | GC_NOT_COLLECTABLE;
pub const IS_STR_INTERNED: u32 = GC_IMMUTABLE;
pub const IS_STR_PERSISTENT: u32 = GC_PERSISTENT;
pub const IS_STR_PERMANENT: u32 = 1 << 8;
pub const IS_STR_VALID_UTF8: u32 = 1 << 9;
pub const IS_ARRAY_IMMUTABLE: u32 = GC_IMMUTABLE;
pub const IS_ARRAY_PERSISTENT: u32 = GC_PERSISTENT;
pub const IS_OBJ_WEAKLY_REFERENCED: u32 = GC_PERSISTENT;
pub const IS_OBJ_DESTRUCTOR_CALLED: u32 = 1 << 8;
pub const IS_OBJ_FREE_CALLED: u32 = 1 << 9;

// ------------------------------------------------------------ hash tables

pub const HT_INVALID_IDX: u32 = 0xffffffff;
pub const HT_MIN_MASK: u32 = 0xfffffffe;
pub const HT_MIN_SIZE: u32 = 8;
pub const HT_MAX_SIZE: u32 = 0x40000000;
pub const HASH_FLAG_CONSISTENCY: u32 = 3;
pub const HASH_FLAG_PACKED: u32 = 1 << 2;
pub const HASH_FLAG_UNINITIALIZED: u32 = 1 << 3;
pub const HASH_FLAG_ALLOW_COW_VIOLATION: u32 = 1 << 4;
pub const HASH_FLAG_HAS_EMPTY_IND: u32 = 1 << 5;
pub const HASH_FLAG_STATIC_KEYS: u32 = 1 << 6;
pub const HASH_FLAG_MASK: u32 = 0xff;
pub const HASH_KEY_IS_STRING: c_int = 1;
pub const HASH_KEY_IS_LONG: c_int = 2;
pub const HASH_KEY_NON_EXISTENT: c_int = 3;
pub const HASH_UPDATE: u32 = 1 << 0;
pub const HASH_ADD: u32 = 1 << 1;
pub const HASH_UPDATE_INDIRECT: u32 = 1 << 2;
pub const HASH_ADD_NEW: u32 = 1 << 3;
pub const HASH_ADD_NEXT: u32 = 1 << 4;
pub const HASH_LOOKUP: u32 = 1 << 5;
pub const ZEND_HASH_APPLY_KEEP: c_int = 0;
pub const ZEND_HASH_APPLY_REMOVE: c_int = 1 << 0;
pub const ZEND_HASH_APPLY_STOP: c_int = 1 << 1;
pub const ZEND_HASH_APPLY_SHIFT: c_int = 8;

// ------------------------------------------------------------ error levels

pub const E_ERROR: c_int = 1 << 0;
pub const E_WARNING: c_int = 1 << 1;
pub const E_PARSE: c_int = 1 << 2;
pub const E_NOTICE: c_int = 1 << 3;
pub const E_CORE_ERROR: c_int = 1 << 4;
pub const E_CORE_WARNING: c_int = 1 << 5;
pub const E_COMPILE_ERROR: c_int = 1 << 6;
pub const E_COMPILE_WARNING: c_int = 1 << 7;
pub const E_USER_ERROR: c_int = 1 << 8;
pub const E_USER_WARNING: c_int = 1 << 9;
pub const E_USER_NOTICE: c_int = 1 << 10;
pub const E_STRICT: c_int = 1 << 11;
pub const E_RECOVERABLE_ERROR: c_int = 1 << 12;
pub const E_DEPRECATED: c_int = 1 << 13;
pub const E_USER_DEPRECATED: c_int = 1 << 14;
pub const E_DONT_BAIL: c_int = 1 << 15;
pub const E_ALL: c_int = E_ERROR
    | E_WARNING
    | E_PARSE
    | E_NOTICE
    | E_CORE_ERROR
    | E_CORE_WARNING
    | E_COMPILE_ERROR
    | E_COMPILE_WARNING
    | E_USER_ERROR
    | E_USER_WARNING
    | E_USER_NOTICE
    | E_RECOVERABLE_ERROR
    | E_DEPRECATED
    | E_USER_DEPRECATED;
pub const E_CORE: c_int = E_CORE_ERROR | E_CORE_WARNING;
pub const E_FATAL_ERRORS: c_int =
    E_ERROR | E_CORE_ERROR | E_COMPILE_ERROR | E_USER_ERROR | E_RECOVERABLE_ERROR | E_PARSE;

// ------------------------------------------------------------ type masks

pub const MAY_BE_UNDEF: u32 = 1 << IS_UNDEF;
pub const MAY_BE_NULL: u32 = 1 << IS_NULL;
pub const MAY_BE_FALSE: u32 = 1 << IS_FALSE;
pub const MAY_BE_TRUE: u32 = 1 << IS_TRUE;
pub const MAY_BE_BOOL: u32 = MAY_BE_FALSE | MAY_BE_TRUE;
pub const MAY_BE_LONG: u32 = 1 << IS_LONG;
pub const MAY_BE_DOUBLE: u32 = 1 << IS_DOUBLE;
pub const MAY_BE_STRING: u32 = 1 << IS_STRING;
pub const MAY_BE_ARRAY: u32 = 1 << IS_ARRAY;
pub const MAY_BE_OBJECT: u32 = 1 << IS_OBJECT;
pub const MAY_BE_RESOURCE: u32 = 1 << IS_RESOURCE;
pub const MAY_BE_ANY: u32 = MAY_BE_NULL
    | MAY_BE_FALSE
    | MAY_BE_TRUE
    | MAY_BE_LONG
    | MAY_BE_DOUBLE
    | MAY_BE_STRING
    | MAY_BE_ARRAY
    | MAY_BE_OBJECT
    | MAY_BE_RESOURCE;
pub const MAY_BE_REF: u32 = 1 << IS_REFERENCE;
pub const MAY_BE_CALLABLE: u32 = 1 << IS_CALLABLE;
pub const MAY_BE_ITERABLE: u32 = 1 << IS_ITERABLE;
pub const MAY_BE_VOID: u32 = 1 << IS_VOID;
pub const MAY_BE_STATIC: u32 = 1 << IS_STATIC;
pub const MAY_BE_NEVER: u32 = 1 << IS_NEVER;
pub const MAY_BE_ARRAY_OF_ANY: u32 = MAY_BE_ANY << 8;
pub const MAY_BE_ARRAY_KEY_ANY: u32 = 0x80000000;

pub const _ZEND_TYPE_EXTRA_FLAGS_SHIFT: u32 = 25;
pub const _ZEND_TYPE_MASK: u32 = (1 << 25) - 1;
pub const _ZEND_TYPE_NAME_BIT: u32 = 1 << 24;
pub const _ZEND_TYPE_LITERAL_NAME_BIT: u32 = 1 << 23;
pub const _ZEND_TYPE_LIST_BIT: u32 = 1 << 22;
pub const _ZEND_TYPE_KIND_MASK: u32 = _ZEND_TYPE_LIST_BIT | _ZEND_TYPE_NAME_BIT | _ZEND_TYPE_LITERAL_NAME_BIT;
pub const _ZEND_TYPE_INTERSECTION_BIT: u32 = 1 << 19;
pub const _ZEND_TYPE_UNION_BIT: u32 = 1 << 18;
pub const _ZEND_TYPE_MAY_BE_MASK: u32 = (1 << 18) - 1;
pub const _ZEND_TYPE_NULLABLE_BIT: u32 = 2;
pub const _ZEND_TYPE_ITERABLE_MASK: u32 = MAY_BE_ARRAY | MAY_BE_OBJECT;
pub const _ZEND_SEND_MODE_SHIFT: u32 = _ZEND_TYPE_EXTRA_FLAGS_SHIFT;
pub const _ZEND_IS_VARIADIC_BIT: u32 = 1 << (_ZEND_TYPE_EXTRA_FLAGS_SHIFT + 2);
pub const _ZEND_IS_PROMOTED_BIT: u32 = 1 << (_ZEND_TYPE_EXTRA_FLAGS_SHIFT + 3);
pub const _ZEND_IS_TENTATIVE_BIT: u32 = 1 << (_ZEND_TYPE_EXTRA_FLAGS_SHIFT + 4);
pub const ZEND_SEND_BY_VAL: u32 = 0;
pub const ZEND_SEND_BY_REF: u32 = 1;
pub const ZEND_SEND_PREFER_REF: u32 = 2;
pub const ZEND_RETURN_VALUE: c_int = 0;
pub const ZEND_RETURN_REFERENCE: c_int = 1;

// ------------------------------------------------------------ functions, classes, access flags

pub const ZEND_INTERNAL_FUNCTION: u8 = 1;
pub const ZEND_USER_FUNCTION: u8 = 2;
pub const ZEND_EVAL_CODE: u8 = 4;
pub const ZEND_INTERNAL_CLASS: u8 = 1;
pub const ZEND_USER_CLASS: u8 = 2;

pub const ZEND_ACC_PUBLIC: u32 = 1 << 0;
pub const ZEND_ACC_PROTECTED: u32 = 1 << 1;
pub const ZEND_ACC_PRIVATE: u32 = 1 << 2;
pub const ZEND_ACC_PPP_MASK: u32 = ZEND_ACC_PUBLIC | ZEND_ACC_PROTECTED | ZEND_ACC_PRIVATE;
pub const ZEND_ACC_CHANGED: u32 = 1 << 3;
pub const ZEND_ACC_STATIC: u32 = 1 << 4;
pub const ZEND_ACC_PROMOTED: u32 = 1 << 5;
pub const ZEND_ACC_FINAL: u32 = 1 << 5;
pub const ZEND_ACC_ABSTRACT: u32 = 1 << 6;
pub const ZEND_ACC_EXPLICIT_ABSTRACT_CLASS: u32 = 1 << 6;
pub const ZEND_ACC_READONLY: u32 = 1 << 7;
pub const ZEND_ACC_VIRTUAL: u32 = 1 << 9;
pub const ZEND_ACC_PUBLIC_SET: u32 = 1 << 10;
pub const ZEND_ACC_PROTECTED_SET: u32 = 1 << 11;
pub const ZEND_ACC_PRIVATE_SET: u32 = 1 << 12;
pub const ZEND_ACC_IMMUTABLE: u32 = 1 << 7;
pub const ZEND_ACC_HAS_TYPE_HINTS: u32 = 1 << 8;
pub const ZEND_ACC_TOP_LEVEL: u32 = 1 << 9;
pub const ZEND_ACC_PRELOADED: u32 = 1 << 10;
pub const ZEND_ACC_INTERFACE: u32 = 1 << 0;
pub const ZEND_ACC_TRAIT: u32 = 1 << 1;
pub const ZEND_ACC_ANON_CLASS: u32 = 1 << 2;
pub const ZEND_ACC_ENUM: u32 = 1 << 28;
pub const ZEND_ACC_LINKED: u32 = 1 << 3;
pub const ZEND_ACC_IMPLICIT_ABSTRACT_CLASS: u32 = 1 << 4;
pub const ZEND_ACC_USE_GUARDS: u32 = 1 << 11;
pub const ZEND_ACC_CONSTANTS_UPDATED: u32 = 1 << 12;
pub const ZEND_ACC_NO_DYNAMIC_PROPERTIES: u32 = 1 << 13;
pub const ZEND_HAS_STATIC_IN_METHODS: u32 = 1 << 14;
pub const ZEND_ACC_ALLOW_DYNAMIC_PROPERTIES: u32 = 1 << 15;
pub const ZEND_ACC_READONLY_CLASS: u32 = 1 << 16;
pub const ZEND_ACC_RESOLVED_PARENT: u32 = 1 << 17;
pub const ZEND_ACC_RESOLVED_INTERFACES: u32 = 1 << 18;
pub const ZEND_ACC_UNRESOLVED_VARIANCE: u32 = 1 << 19;
pub const ZEND_ACC_NEARLY_LINKED: u32 = 1 << 20;
pub const ZEND_ACC_HAS_READONLY_PROPS: u32 = 1 << 21;
pub const ZEND_ACC_NOT_SERIALIZABLE: u32 = 1 << 29;
pub const ZEND_ACC_DEPRECATED: u32 = 1 << 11;
pub const ZEND_ACC_NODISCARD: u32 = 1 << 29;
pub const ZEND_ACC_VARIADIC: u32 = 1 << 14;
pub const ZEND_ACC_HAS_RETURN_TYPE: u32 = 1 << 13;
pub const ZEND_ACC_RETURN_REFERENCE: u32 = 1 << 12;
pub const ZEND_ACC_CTOR: u32 = 1 << 28;
pub const ZEND_ACC_CLOSURE: u32 = 1 << 22;
pub const ZEND_ACC_FAKE_CLOSURE: u32 = 1 << 23;
pub const ZEND_ACC_GENERATOR: u32 = 1 << 24;
pub const ZEND_ACC_CALL_VIA_TRAMPOLINE: u32 = 1 << 18;
pub const ZEND_ACC_CALL_VIA_HANDLER: u32 = ZEND_ACC_CALL_VIA_TRAMPOLINE;
pub const ZEND_ACC_NEVER_CACHE: u32 = 1 << 19;
pub const ZEND_ACC_USES_THIS: u32 = 1 << 25;
pub const ZEND_ACC_HAS_FINALLY_BLOCK: u32 = 1 << 15;
pub const ZEND_ACC_COMPILE_TIME_EVAL: u32 = 1 << 27;
pub const ZEND_ACC_USER_ARG_INFO: u32 = 1 << 26;
pub const ZEND_ACC_ARENA_ALLOCATED: u32 = 1 << 25;
pub const ZEND_ACC_STRICT_TYPES: u32 = 0x80000000;
pub const ZEND_ACC_HAS_AST_CONSTANTS: u32 = 1 << 9;
pub const ZEND_ACC_HAS_AST_PROPERTIES: u32 = 1 << 10;
pub const ZEND_ACC_HAS_AST_STATICS: u32 = 1 << 11;
pub const ZEND_ACC_OVERRIDE: u32 = 1 << 28;
pub const ZEND_ACC_TRAIT_CLONE: u32 = 1 << 27;
pub const ZEND_CLASS_CONST_IS_CASE: u32 = 1 << 6;

pub const ZEND_FETCH_CLASS_DEFAULT: u32 = 0;
pub const ZEND_FETCH_CLASS_SELF: u32 = 1;
pub const ZEND_FETCH_CLASS_PARENT: u32 = 2;
pub const ZEND_FETCH_CLASS_STATIC: u32 = 3;
pub const ZEND_FETCH_CLASS_AUTO: u32 = 4;
pub const ZEND_FETCH_CLASS_INTERFACE: u32 = 5;
pub const ZEND_FETCH_CLASS_TRAIT: u32 = 6;
pub const ZEND_FETCH_CLASS_MASK: u32 = 0x0f;
pub const ZEND_FETCH_CLASS_NO_AUTOLOAD: u32 = 0x80;
pub const ZEND_FETCH_CLASS_SILENT: u32 = 0x0100;
pub const ZEND_FETCH_CLASS_EXCEPTION: u32 = 0x0200;
pub const ZEND_FETCH_CLASS_ALLOW_UNLINKED: u32 = 0x0400;
pub const ZEND_FETCH_CLASS_ALLOW_NEARLY_LINKED: u32 = 0x0800;

pub const ZEND_ATTRIBUTE_TARGET_CLASS: u32 = 1 << 0;
pub const ZEND_ATTRIBUTE_TARGET_FUNCTION: u32 = 1 << 1;
pub const ZEND_ATTRIBUTE_TARGET_METHOD: u32 = 1 << 2;
pub const ZEND_ATTRIBUTE_TARGET_PROPERTY: u32 = 1 << 3;
pub const ZEND_ATTRIBUTE_TARGET_CLASS_CONST: u32 = 1 << 4;
pub const ZEND_ATTRIBUTE_TARGET_PARAMETER: u32 = 1 << 5;
pub const ZEND_ATTRIBUTE_TARGET_CONST: u32 = 1 << 6;
pub const ZEND_ATTRIBUTE_TARGET_ALL: u32 = (1 << 7) - 1;
pub const ZEND_ATTRIBUTE_IS_REPEATABLE: u32 = 1 << 7;
pub const ZEND_ATTRIBUTE_PERSISTENT: u32 = 1;

// ------------------------------------------------------------ objects

pub const BP_VAR_R: c_int = 0;
pub const BP_VAR_W: c_int = 1;
pub const BP_VAR_RW: c_int = 2;
pub const BP_VAR_IS: c_int = 3;
pub const BP_VAR_FUNC_ARG: c_int = 4;
pub const BP_VAR_UNSET: c_int = 5;
pub const ZEND_PROPERTY_ISSET: c_int = 0;
pub const ZEND_ISEMPTY: c_int = 1;
pub const ZEND_PROPERTY_NOT_EMPTY: c_int = ZEND_ISEMPTY;
pub const ZEND_PROPERTY_EXISTS: c_int = 2;
pub const ZEND_PROPERTY_HOOK_COUNT: c_int = 2;

/// `zend_prop_purpose`.
/// c: enum
pub type zend_prop_purpose = c_int;
pub const ZEND_PROP_PURPOSE_DEBUG: zend_prop_purpose = 0;
pub const ZEND_PROP_PURPOSE_ARRAY_CAST: zend_prop_purpose = 1;
pub const ZEND_PROP_PURPOSE_SERIALIZE: zend_prop_purpose = 2;
pub const ZEND_PROP_PURPOSE_VAR_EXPORT: zend_prop_purpose = 3;
pub const ZEND_PROP_PURPOSE_JSON: zend_prop_purpose = 4;
pub const ZEND_PROP_PURPOSE_GET_OBJECT_VARS: zend_prop_purpose = 5;
pub const _ZEND_PROP_PURPOSE_NON_EXHAUSTIVE_ENUM: c_int = 6;

/// `zend_property_hook_kind`.
/// c: enum
pub type zend_property_hook_kind = c_int;
pub const ZEND_PROPERTY_HOOK_GET: zend_property_hook_kind = 0;
pub const ZEND_PROPERTY_HOOK_SET: zend_property_hook_kind = 1;

// ------------------------------------------------------------ argument parsing

pub const ZEND_PARSE_PARAMS_THROW: c_int = 0;
pub const ZEND_PARSE_PARAMS_QUIET: c_int = 1 << 1;
pub const ZPP_ERROR_OK: c_int = 0;
pub const ZPP_ERROR_FAILURE: c_int = 1;
pub const ZPP_ERROR_WRONG_CALLBACK: c_int = 2;
pub const ZPP_ERROR_WRONG_CLASS: c_int = 3;
pub const ZPP_ERROR_WRONG_CLASS_OR_NULL: c_int = 4;
pub const ZPP_ERROR_WRONG_CLASS_OR_STRING: c_int = 5;
pub const ZPP_ERROR_WRONG_CLASS_OR_STRING_OR_NULL: c_int = 6;
pub const ZPP_ERROR_WRONG_CLASS_OR_LONG: c_int = 7;
pub const ZPP_ERROR_WRONG_CLASS_OR_LONG_OR_NULL: c_int = 8;
pub const ZPP_ERROR_WRONG_ARG: c_int = 9;
pub const ZPP_ERROR_WRONG_COUNT: c_int = 10;
pub const ZPP_ERROR_UNEXPECTED_EXTRA_NAMED: c_int = 11;
pub const ZPP_ERROR_WRONG_CALLBACK_OR_NULL: c_int = 12;

/// `zend_expected_type`: what a fast-ZPP failure expected, in PHP's order
/// (the error message reads its name from this index).
/// c: enum
pub type zend_expected_type = c_int;
pub const Z_EXPECTED_LONG: zend_expected_type = 0;
pub const Z_EXPECTED_LONG_OR_NULL: zend_expected_type = 1;
pub const Z_EXPECTED_BOOL: zend_expected_type = 2;
pub const Z_EXPECTED_BOOL_OR_NULL: zend_expected_type = 3;
pub const Z_EXPECTED_STRING: zend_expected_type = 4;
pub const Z_EXPECTED_STRING_OR_NULL: zend_expected_type = 5;
pub const Z_EXPECTED_ARRAY: zend_expected_type = 6;
pub const Z_EXPECTED_ARRAY_OR_NULL: zend_expected_type = 7;
pub const Z_EXPECTED_ARRAY_OR_LONG: zend_expected_type = 8;
pub const Z_EXPECTED_ARRAY_OR_LONG_OR_NULL: zend_expected_type = 9;
pub const Z_EXPECTED_ITERABLE: zend_expected_type = 10;
pub const Z_EXPECTED_ITERABLE_OR_NULL: zend_expected_type = 11;
pub const Z_EXPECTED_FUNC: zend_expected_type = 12;
pub const Z_EXPECTED_FUNC_OR_NULL: zend_expected_type = 13;
pub const Z_EXPECTED_RESOURCE: zend_expected_type = 14;
pub const Z_EXPECTED_RESOURCE_OR_NULL: zend_expected_type = 15;
pub const Z_EXPECTED_PATH: zend_expected_type = 16;
pub const Z_EXPECTED_PATH_OR_NULL: zend_expected_type = 17;
pub const Z_EXPECTED_OBJECT: zend_expected_type = 18;
pub const Z_EXPECTED_OBJECT_OR_NULL: zend_expected_type = 19;
pub const Z_EXPECTED_DOUBLE: zend_expected_type = 20;
pub const Z_EXPECTED_DOUBLE_OR_NULL: zend_expected_type = 21;
pub const Z_EXPECTED_NUMBER: zend_expected_type = 22;
pub const Z_EXPECTED_NUMBER_OR_NULL: zend_expected_type = 23;
pub const Z_EXPECTED_NUMBER_OR_STRING: zend_expected_type = 24;
pub const Z_EXPECTED_NUMBER_OR_STRING_OR_NULL: zend_expected_type = 25;
pub const Z_EXPECTED_ARRAY_OR_STRING: zend_expected_type = 26;
pub const Z_EXPECTED_ARRAY_OR_STRING_OR_NULL: zend_expected_type = 27;
pub const Z_EXPECTED_STRING_OR_LONG: zend_expected_type = 28;
pub const Z_EXPECTED_STRING_OR_LONG_OR_NULL: zend_expected_type = 29;
pub const Z_EXPECTED_OBJECT_OR_CLASS_NAME: zend_expected_type = 30;
pub const Z_EXPECTED_OBJECT_OR_CLASS_NAME_OR_NULL: zend_expected_type = 31;
pub const Z_EXPECTED_OBJECT_OR_STRING: zend_expected_type = 32;
pub const Z_EXPECTED_OBJECT_OR_STRING_OR_NULL: zend_expected_type = 33;
pub const Z_EXPECTED_LAST: zend_expected_type = 34;

pub const IS_CALLABLE_CHECK_SYNTAX_ONLY: u32 = 1 << 0;
pub const IS_CALLABLE_SUPPRESS_DEPRECATIONS: u32 = 1 << 1;

// ------------------------------------------------------------ modules

pub const MODULE_PERSISTENT: c_int = 1;
pub const MODULE_TEMPORARY: c_int = 2;
pub const MODULE_DEP_REQUIRED: c_int = 1;
pub const MODULE_DEP_CONFLICTS: c_int = 2;
pub const MODULE_DEP_OPTIONAL: c_int = 3;

/// `zend_error_handling_t`.
/// c: enum
pub type zend_error_handling_t = c_int;
pub const EH_NORMAL: zend_error_handling_t = 0;
pub const EH_THROW: zend_error_handling_t = 1;

// ------------------------------------------------------------ constants and INI

pub const CONST_CS: c_int = 0;
pub const CONST_PERSISTENT: c_int = 1 << 0;
pub const CONST_NO_FILE_CACHE: c_int = 1 << 1;
pub const CONST_DEPRECATED: c_int = 1 << 2;
pub const CONST_OWNED: c_int = 1 << 3;
pub const PHP_USER_CONSTANT: c_int = 0x7fffff;

pub const ZEND_INI_USER: c_int = 1 << 0;
pub const ZEND_INI_PERDIR: c_int = 1 << 1;
pub const ZEND_INI_SYSTEM: c_int = 1 << 2;
pub const ZEND_INI_ALL: c_int = ZEND_INI_USER | ZEND_INI_PERDIR | ZEND_INI_SYSTEM;
pub const ZEND_INI_STAGE_STARTUP: c_int = 1 << 0;
pub const ZEND_INI_STAGE_SHUTDOWN: c_int = 1 << 1;
pub const ZEND_INI_STAGE_ACTIVATE: c_int = 1 << 2;
pub const ZEND_INI_STAGE_DEACTIVATE: c_int = 1 << 3;
pub const ZEND_INI_STAGE_RUNTIME: c_int = 1 << 4;
pub const ZEND_INI_STAGE_HTACCESS: c_int = 1 << 5;
pub const ZEND_INI_STAGE_IN_REQUEST: c_int =
    ZEND_INI_STAGE_ACTIVATE | ZEND_INI_STAGE_DEACTIVATE | ZEND_INI_STAGE_RUNTIME | ZEND_INI_STAGE_HTACCESS;
pub const ZEND_INI_DISPLAY_ORIG: c_int = 1;
pub const ZEND_INI_DISPLAY_ACTIVE: c_int = 2;

// ------------------------------------------------------------ main/ and ext/

pub const TRACK_VARS_POST: c_int = 0;
pub const TRACK_VARS_GET: c_int = 1;
pub const TRACK_VARS_COOKIE: c_int = 2;
pub const TRACK_VARS_SERVER: c_int = 3;
pub const TRACK_VARS_ENV: c_int = 4;
pub const TRACK_VARS_FILES: c_int = 5;
pub const TRACK_VARS_REQUEST: c_int = 6;

pub const PHP_BASE64_NO_PADDING: c_int = 1;
pub const PHP_ROUND_HALF_UP: c_int = 1;
pub const PHP_ROUND_HALF_DOWN: c_int = 2;
pub const PHP_ROUND_HALF_EVEN: c_int = 3;
pub const PHP_ROUND_HALF_ODD: c_int = 4;
pub const PHP_JSON_OBJECT_AS_ARRAY: c_int = 1 << 0;
pub const PHP_JSON_BIGINT_AS_STRING: c_int = 1 << 1;
pub const PHP_JSON_PARSER_DEFAULT_DEPTH: c_int = 512;
pub const PCRE2_NO_UTF_CHECK: u32 = 0x40000000;
pub const php_session_active: c_int = 2;

/// `php_iconv_err_t`.
/// c: enum
pub type php_iconv_err_t = c_int;
pub const PHP_ICONV_ERR_SUCCESS: php_iconv_err_t = 0;
pub const PHP_ICONV_ERR_CONVERTER: php_iconv_err_t = 1;
pub const PHP_ICONV_ERR_WRONG_CHARSET: php_iconv_err_t = 2;
pub const PHP_ICONV_ERR_TOO_BIG: php_iconv_err_t = 3;
pub const PHP_ICONV_ERR_ILLEGAL_SEQ: php_iconv_err_t = 4;
pub const PHP_ICONV_ERR_ILLEGAL_CHAR: php_iconv_err_t = 5;
pub const PHP_ICONV_ERR_UNKNOWN: php_iconv_err_t = 6;
pub const PHP_ICONV_ERR_MALFORMED: php_iconv_err_t = 7;
pub const PHP_ICONV_ERR_ALLOC: php_iconv_err_t = 8;
pub const PHP_ICONV_ERR_OUT_BY_BOUNDS: php_iconv_err_t = 9;

/// `php_stream_filter_status_t`.
/// c: enum
pub type php_stream_filter_status_t = c_int;
pub const PSFS_ERR_FATAL: php_stream_filter_status_t = 0;
pub const PSFS_FEED_ME: php_stream_filter_status_t = 1;
pub const PSFS_PASS_ON: php_stream_filter_status_t = 2;
pub const PSFS_FLAG_NORMAL: c_int = 0;
pub const PSFS_FLAG_FLUSH_INC: c_int = 1;
pub const PSFS_FLAG_FLUSH_CLOSE: c_int = 2;

pub const REPORT_ERRORS: c_int = 8;
pub const USE_PATH: c_int = 1;
pub const IGNORE_URL: c_int = 2;
pub const STREAM_MUST_SEEK: c_int = 16;
pub const STREAM_WILL_CAST: c_int = 32;
pub const STREAM_LOCATE_WRAPPERS_ONLY: c_int = 64;
pub const STREAM_OPEN_FOR_INCLUDE: c_int = 128;
pub const STREAM_USE_URL: c_int = 256;
pub const STREAM_ONLY_GET_HEADERS: c_int = 512;
pub const STREAM_DISABLE_OPEN_BASEDIR: c_int = 1024;
pub const STREAM_ASSUME_REALPATH: c_int = 0x00004000;
pub const STREAM_OPEN_FOR_ZEND_STREAM: c_int = 0x00010000;
pub const PHP_STREAM_FLAG_NO_SEEK: c_int = 0x1;
pub const PHP_STREAM_FLAG_NO_BUFFER: c_int = 0x2;
pub const PHP_STREAM_FREE_CALL_DTOR: c_int = 1;
pub const PHP_STREAM_FREE_RELEASE_STREAM: c_int = 2;
pub const PHP_STREAM_FREE_PRESERVE_HANDLE: c_int = 4;
pub const PHP_STREAM_FREE_RSRC_DTOR: c_int = 8;
pub const PHP_STREAM_FREE_PERSISTENT: c_int = 16;
pub const PHP_STREAM_FREE_IGNORE_ENCLOSING: c_int = 32;
pub const PHP_STREAM_FREE_KEEP_RSRC: c_int = 64;
pub const PHP_STREAM_FREE_CLOSE: c_int = PHP_STREAM_FREE_CALL_DTOR | PHP_STREAM_FREE_RELEASE_STREAM;
pub const PHP_STREAM_FREE_CLOSE_CASTED: c_int = PHP_STREAM_FREE_CLOSE | PHP_STREAM_FREE_PRESERVE_HANDLE;
pub const PHP_STREAM_FREE_CLOSE_PERSISTENT: c_int = PHP_STREAM_FREE_CLOSE | PHP_STREAM_FREE_PERSISTENT;
pub const PHP_STREAM_OPTION_BLOCKING: c_int = 1;
pub const PHP_STREAM_OPTION_READ_BUFFER: c_int = 2;
pub const PHP_STREAM_OPTION_WRITE_BUFFER: c_int = 3;
pub const PHP_STREAM_OPTION_READ_TIMEOUT: c_int = 4;
pub const PHP_STREAM_OPTION_SET_CHUNK_SIZE: c_int = 5;
pub const PHP_STREAM_OPTION_LOCKING: c_int = 6;
pub const PHP_STREAM_OPTION_MMAP_API: c_int = 9;
pub const PHP_STREAM_OPTION_TRUNCATE_API: c_int = 10;
pub const PHP_STREAM_OPTION_META_DATA_API: c_int = 11;
pub const PHP_STREAM_OPTION_CHECK_LIVENESS: c_int = 12;
pub const PHP_STREAM_OPTION_PIPE_BLOCKING: c_int = 13;
pub const PHP_STREAM_OPTION_RETURN_OK: c_int = 0;
pub const PHP_STREAM_OPTION_RETURN_ERR: c_int = -1;
pub const PHP_STREAM_OPTION_RETURN_NOTIMPL: c_int = -2;
pub const PHP_STREAM_BUFFER_NONE: c_int = 0;
pub const PHP_STREAM_BUFFER_LINE: c_int = 1;
pub const PHP_STREAM_BUFFER_FULL: c_int = 2;
pub const PHP_STREAM_AS_STDIO: c_int = 0;
pub const PHP_STREAM_AS_FD: c_int = 1;
pub const PHP_STREAM_AS_SOCKETD: c_int = 2;
pub const PHP_STREAM_AS_FD_FOR_SELECT: c_int = 3;
pub const PHP_STREAM_CAST_TRY_HARD: u32 = 0x80000000;
pub const PHP_STREAM_CAST_RELEASE: c_int = 0x40000000;
pub const PHP_STREAM_CAST_INTERNAL: c_int = 0x20000000;
pub const PHP_STREAM_URL_STAT_LINK: c_int = 1;
pub const PHP_STREAM_URL_STAT_QUIET: c_int = 2;
pub const PHP_STREAM_MKDIR_RECURSIVE: c_int = 1;
pub const PHP_STREAM_META_TOUCH: c_int = 1;
pub const PHP_STREAM_META_OWNER_NAME: c_int = 2;
pub const PHP_STREAM_META_OWNER: c_int = 3;
pub const PHP_STREAM_META_GROUP_NAME: c_int = 4;
pub const PHP_STREAM_META_GROUP: c_int = 5;
pub const PHP_STREAM_META_ACCESS: c_int = 6;
pub const TEMP_STREAM_DEFAULT: c_int = 0;
pub const TEMP_STREAM_READONLY: c_int = 1;
pub const TEMP_STREAM_TAKE_BUFFER: c_int = 2;
pub const TEMP_STREAM_APPEND: c_int = 4;

/// `php_output_handler_hook_t`.
/// c: enum
pub type php_output_handler_hook_t = c_int;
pub const PHP_OUTPUT_HANDLER_HOOK_GET_OPAQ: php_output_handler_hook_t = 0;
pub const PHP_OUTPUT_HANDLER_HOOK_GET_FLAGS: php_output_handler_hook_t = 1;
pub const PHP_OUTPUT_HANDLER_HOOK_GET_LEVEL: php_output_handler_hook_t = 2;
pub const PHP_OUTPUT_HANDLER_HOOK_IMMUTABLE: php_output_handler_hook_t = 3;
pub const PHP_OUTPUT_HANDLER_HOOK_DISABLE: php_output_handler_hook_t = 4;
pub const PHP_OUTPUT_HANDLER_HOOK_LAST: php_output_handler_hook_t = 5;
pub const PHP_OUTPUT_HANDLER_WRITE: c_int = 0x00;
pub const PHP_OUTPUT_HANDLER_START: c_int = 0x01;
pub const PHP_OUTPUT_HANDLER_CLEAN: c_int = 0x02;
pub const PHP_OUTPUT_HANDLER_FLUSH: c_int = 0x04;
pub const PHP_OUTPUT_HANDLER_FINAL: c_int = 0x08;
pub const PHP_OUTPUT_HANDLER_CONT: c_int = PHP_OUTPUT_HANDLER_WRITE;
pub const PHP_OUTPUT_HANDLER_END: c_int = PHP_OUTPUT_HANDLER_FINAL;
pub const PHP_OUTPUT_HANDLER_INTERNAL: c_int = 0x0000;
pub const PHP_OUTPUT_HANDLER_USER: c_int = 0x0001;
pub const PHP_OUTPUT_HANDLER_CLEANABLE: c_int = 0x0010;
pub const PHP_OUTPUT_HANDLER_FLUSHABLE: c_int = 0x0020;
pub const PHP_OUTPUT_HANDLER_REMOVABLE: c_int = 0x0040;
pub const PHP_OUTPUT_HANDLER_STDFLAGS: c_int = 0x0070;
pub const PHP_OUTPUT_HANDLER_STARTED: c_int = 0x1000;
pub const PHP_OUTPUT_HANDLER_DISABLED: c_int = 0x2000;
pub const PHP_OUTPUT_HANDLER_PROCESSED: c_int = 0x4000;
pub const PHP_OUTPUT_HANDLER_DEFAULT_SIZE: c_int = 0x4000;
pub const PHP_OUTPUT_IMPLICITFLUSH: c_int = 0x01;
pub const PHP_OUTPUT_DISABLED: c_int = 0x02;
pub const PHP_OUTPUT_WRITTEN: c_int = 0x04;
pub const PHP_OUTPUT_SENT: c_int = 0x08;
pub const PHP_OUTPUT_ACTIVE: c_int = 0x10;
pub const PHP_OUTPUT_LOCKED: c_int = 0x20;
pub const PHP_OUTPUT_ACTIVATED: c_int = 0x100000;

// ------------------------------------------------------------ known strings

/// `zend_known_string_id`: indexes of `zend_known_strings`, whose texts are
/// `crate::string::KNOWN_STRINGS` in the same order.
/// c: enum
pub type zend_known_string_id = c_int;
pub const ZEND_STR_FILE: zend_known_string_id = 0;
pub const ZEND_STR_LINE: zend_known_string_id = 1;
pub const ZEND_STR_FUNCTION: zend_known_string_id = 2;
pub const ZEND_STR_CLASS: zend_known_string_id = 3;
pub const ZEND_STR_OBJECT: zend_known_string_id = 4;
pub const ZEND_STR_TYPE: zend_known_string_id = 5;
pub const ZEND_STR_OBJECT_OPERATOR: zend_known_string_id = 6;
pub const ZEND_STR_PAAMAYIM_NEKUDOTAYIM: zend_known_string_id = 7;
pub const ZEND_STR_ARGS: zend_known_string_id = 8;
pub const ZEND_STR_UNKNOWN: zend_known_string_id = 9;
pub const ZEND_STR_UNKNOWN_CAPITALIZED: zend_known_string_id = 10;
pub const ZEND_STR_EVAL: zend_known_string_id = 11;
pub const ZEND_STR_INCLUDE: zend_known_string_id = 12;
pub const ZEND_STR_REQUIRE: zend_known_string_id = 13;
pub const ZEND_STR_INCLUDE_ONCE: zend_known_string_id = 14;
pub const ZEND_STR_REQUIRE_ONCE: zend_known_string_id = 15;
pub const ZEND_STR_SCALAR: zend_known_string_id = 16;
pub const ZEND_STR_ERROR_REPORTING: zend_known_string_id = 17;
pub const ZEND_STR_STATIC: zend_known_string_id = 18;
pub const ZEND_STR_THIS: zend_known_string_id = 19;
pub const ZEND_STR_VALUE: zend_known_string_id = 20;
pub const ZEND_STR_KEY: zend_known_string_id = 21;
pub const ZEND_STR_MAGIC_INVOKE: zend_known_string_id = 22;
pub const ZEND_STR_PREVIOUS: zend_known_string_id = 23;
pub const ZEND_STR_CODE: zend_known_string_id = 24;
pub const ZEND_STR_MESSAGE: zend_known_string_id = 25;
pub const ZEND_STR_SEVERITY: zend_known_string_id = 26;
pub const ZEND_STR_STRING: zend_known_string_id = 27;
pub const ZEND_STR_TRACE: zend_known_string_id = 28;
pub const ZEND_STR_NAME: zend_known_string_id = 29;
pub const ZEND_STR_ARGV: zend_known_string_id = 30;
pub const ZEND_STR_ARGC: zend_known_string_id = 31;
pub const ZEND_STR_ARRAY_CAPITALIZED: zend_known_string_id = 32;
pub const ZEND_STR_BOOL: zend_known_string_id = 33;
pub const ZEND_STR_INT: zend_known_string_id = 34;
pub const ZEND_STR_FLOAT: zend_known_string_id = 35;
pub const ZEND_STR_CALLABLE: zend_known_string_id = 36;
pub const ZEND_STR_ITERABLE: zend_known_string_id = 37;
pub const ZEND_STR_VOID: zend_known_string_id = 38;
pub const ZEND_STR_NEVER: zend_known_string_id = 39;
pub const ZEND_STR_FALSE: zend_known_string_id = 40;
pub const ZEND_STR_TRUE: zend_known_string_id = 41;
pub const ZEND_STR_NULL_LOWERCASE: zend_known_string_id = 42;
pub const ZEND_STR_MIXED: zend_known_string_id = 43;
pub const ZEND_STR_TRAVERSABLE: zend_known_string_id = 44;
pub const ZEND_STR_SELF: zend_known_string_id = 45;
pub const ZEND_STR_PARENT: zend_known_string_id = 46;
pub const ZEND_STR_ARRAY: zend_known_string_id = 47;
pub const ZEND_STR_NULL: zend_known_string_id = 48;
pub const ZEND_STR_DOUBLE: zend_known_string_id = 49;
pub const ZEND_STR_LONG: zend_known_string_id = 50;
pub const ZEND_STR_SLEEP: zend_known_string_id = 51;
pub const ZEND_STR_WAKEUP: zend_known_string_id = 52;
pub const ZEND_STR_CASES: zend_known_string_id = 53;
pub const ZEND_STR_FROM: zend_known_string_id = 54;
pub const ZEND_STR_TRYFROM: zend_known_string_id = 55;
pub const ZEND_STR_TRYFROM_LOWERCASE: zend_known_string_id = 56;
pub const ZEND_STR_AUTOGLOBAL_SERVER: zend_known_string_id = 57;
pub const ZEND_STR_AUTOGLOBAL_ENV: zend_known_string_id = 58;
pub const ZEND_STR_AUTOGLOBAL_REQUEST: zend_known_string_id = 59;
pub const ZEND_STR_COUNT: zend_known_string_id = 60;
pub const ZEND_STR_SENSITIVEPARAMETER: zend_known_string_id = 61;
pub const ZEND_STR_CONST_EXPR_PLACEHOLDER: zend_known_string_id = 62;
pub const ZEND_STR_DEPRECATED_CAPITALIZED: zend_known_string_id = 63;
pub const ZEND_STR_SINCE: zend_known_string_id = 64;
pub const ZEND_STR_GET: zend_known_string_id = 65;
pub const ZEND_STR_SET: zend_known_string_id = 66;
pub const ZEND_STR_8_DOT_0: zend_known_string_id = 67;
pub const ZEND_STR_8_DOT_1: zend_known_string_id = 68;
pub const ZEND_STR_8_DOT_2: zend_known_string_id = 69;
pub const ZEND_STR_8_DOT_3: zend_known_string_id = 70;
pub const ZEND_STR_8_DOT_4: zend_known_string_id = 71;
pub const ZEND_STR_8_DOT_5: zend_known_string_id = 72;
pub const ZEND_STR_LAST_KNOWN: zend_known_string_id = 73;
