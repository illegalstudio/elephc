//! Purpose:
//! Behaviour tests of the engine against PHP's semantics: hash tables,
//! argument parsing and its messages, exceptions, objects, INI.
//!
//! Called from:
//! - `cargo test -p elephc-zend`.
//!
//! Key details:
//! - The engine is single-threaded by contract, so every test holds `lock()`,
//!   which also starts the engine.
//! - Expected messages are PHP 8.5's, as `php -r` prints them.

use std::ffi::{c_char, c_int, CStr};
use std::ptr;
use std::sync::{Mutex, MutexGuard};

use crate::abi::*;
use crate::hash::*;
use crate::string::{self, bytes};
use crate::zval::*;

static LOCK: Mutex<()> = Mutex::new(());

/// Serializes every test that touches the engine, and starts it.
pub(crate) fn lock() -> MutexGuard<'static, ()> {
    let guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    crate::startup::startup();
    guard
}

/// Test helper: builds a string zval holding a copy of `text`.
unsafe fn str_zval(text: &str) -> zval {
    let mut zv = zval::UNDEF;
    set_bytes(&mut zv, text.as_bytes());
    zv
}

/// Test helper: lists a hash table's keys in order as strings, integer keys bare and string keys
/// single-quoted.
unsafe fn keys(ht: *const HashTable) -> Vec<String> {
    let mut out = Vec::new();
    for idx in 0..(*ht).nNumUsed {
        let p = bucket(ht, idx);
        if (*p).val.u1.v.r#type == IS_UNDEF {
            continue;
        }
        out.push(if (*p).key.is_null() { (*p).h.to_string() } else { format!("'{}'", string::text((*p).key)) });
    }
    out
}

// ------------------------------------------------------------ hash tables

/// Verifies a hash table keeps insertion order across growth to 100 mixed integer and string keys
/// and after deleting every fourth, finds entries by string and integer key, and keeps PHP's next
/// free index.
#[test]
fn tables_keep_insertion_order_through_deletes_and_growth() {
    let _guard = lock();
    unsafe {
        let ht = _zend_new_array(0);
        for i in 0..100 {
            let mut value = long(i);
            if i % 2 == 0 {
                zend_hash_index_update(ht, i as zend_ulong * 3, &mut value);
            } else {
                let key = format!("k{i}");
                zend_hash_str_update(ht, key.as_ptr().cast(), key.len(), &mut value);
            }
        }
        assert_eq!((*ht).nNumOfElements, 100);
        assert!((*ht).nTableSize >= 100);
        for i in (0..100).step_by(4) {
            assert_eq!(zend_hash_index_del(ht, i as zend_ulong * 3), SUCCESS);
        }
        assert_eq!((*ht).nNumOfElements, 75);
        let found = zend_hash_str_find(ht, c"k51".as_ptr(), 3);
        assert_eq!((*found).value.lval, 51);
        assert!(zend_hash_index_find(ht, 0).is_null());
        assert_eq!((*zend_hash_index_find(ht, 6)).value.lval, 2);
        let order = keys(ht);
        assert_eq!(&order[..4], &["'k1'", "6", "'k3'", "'k5'"]);
        // PHP's next index is one past the largest integer key ever used.
        assert_eq!((*ht).nNextFreeElement, 98 * 3 + 1);
        zend_array_destroy(ht);
    }
}

/// Verifies the hash storage block follows PHP's layout: an uninitialized table finds nothing, the
/// first insert allocates 8 buckets with two hash slots each, and the hash slot of a key holds its
/// bucket index.
#[test]
fn the_storage_block_is_laid_out_like_php() {
    let _guard = lock();
    unsafe {
        let ht = _zend_new_array(0);
        assert_eq!((*ht).u.flags & HASH_FLAG_UNINITIALIZED, HASH_FLAG_UNINITIALIZED);
        assert!(zend_hash_str_find(ht, c"x".as_ptr(), 1).is_null(), "an uninitialized table finds nothing");
        let mut value = long(1);
        zend_hash_str_update(ht, c"x".as_ptr(), 1, &mut value);
        assert_eq!((*ht).nTableSize, 8);
        assert_eq!((*ht).nTableMask, (16u32).wrapping_neg(), "two hash slots per bucket");
        // HT_HASH(ht, h | mask) holds the index of the bucket the key lives in.
        let p = bucket(ht, 0);
        let slot = *(*ht).data.arHash.offset(((*p).h as u32 | (*ht).nTableMask) as i32 as isize);
        assert_eq!(slot, 0);
        zend_array_destroy(ht);
    }
}

/// Verifies `zend_hash_set_bucket_key` leaves a bucket that already holds the key alone, as
/// php-src does, instead of releasing the key it is about to keep, and refuses a key another
/// bucket holds.
#[test]
fn setting_a_bucket_to_its_own_key_keeps_the_key() {
    let _guard = lock();
    unsafe {
        let ht = _zend_new_array(0);
        let key = string::new(b"own");
        let mut value = long(7);
        zend_hash_update(ht, key, &mut value);
        let other = string::new(b"other");
        let mut second = long(8);
        zend_hash_update(ht, other, &mut second);
        let first = bucket(ht, 0);
        let before = (*key).gc.refcount;
        let kept = zend_hash_set_bucket_key(ht, first, key);
        assert_eq!((*kept).value.lval, 7);
        assert_eq!((*key).gc.refcount, before, "no release and re-retain of the same key");
        assert_eq!(string::text((*first).key), "own");
        assert!(zend_hash_set_bucket_key(ht, first, other).is_null(), "a key another bucket holds");
        string::zend_string_release(key);
        string::zend_string_release(other);
        zend_array_destroy(ht);
    }
}

/// Verifies numeric string keys follow PHP's integer key rules: plain and negative decimals
/// convert, while leading zeros, `-0`, floats, spaces and out-of-range values stay strings.
#[test]
fn numeric_string_keys_become_integers_as_in_php() {
    assert_eq!(numeric_key(b"12"), Some(12));
    assert_eq!(numeric_key(b"-5"), Some((-5i64) as zend_ulong));
    assert_eq!(numeric_key(b"0"), Some(0));
    assert_eq!(numeric_key(b"012"), None);
    assert_eq!(numeric_key(b"-0"), None);
    assert_eq!(numeric_key(b"1.5"), None);
    assert_eq!(numeric_key(b" 1"), None);
    assert_eq!(numeric_key(b"9223372036854775808"), None);
    assert_eq!(numeric_key(b"9223372036854775807"), Some(i64::MAX as zend_ulong));
}

/// Verifies list detection, duplicating a table (values shared by reference count), sorting with a
/// comparator while renumbering keys, and a hole breaking list status.
#[test]
fn lists_duplicates_and_sorting_behave_like_php() {
    let _guard = lock();
    unsafe {
        let ht = _zend_new_array(0);
        for text in ["b", "a", "c"] {
            let mut value = str_zval(text);
            zend_hash_next_index_insert(ht, &mut value);
        }
        assert!(zend_array_is_list(ht));
        let copy = zend_array_dup(ht);
        let first = zend_hash_index_find(copy, 0);
        assert_eq!((*(*first).value.str).gc.refcount, 2, "a duplicate shares its values");
        /// Test comparator ordering two buckets by their string values.
        unsafe extern "C" fn by_value(a: *mut Bucket, b: *mut Bucket) -> c_int {
            bytes((*a).val.value.str).cmp(bytes((*b).val.value.str)) as c_int
        }
        zend_hash_sort(copy, Some(by_value), true);
        assert_eq!(keys(copy), ["0", "1", "2"]);
        let sorted: Vec<String> = (0..3).map(|i| string::text((*zend_hash_index_find(copy, i)).value.str)).collect();
        assert_eq!(sorted, ["a", "b", "c"]);
        zend_hash_index_del(ht, 1);
        assert!(!zend_array_is_list(ht), "a hole breaks the list");
        zend_array_destroy(copy);
        zend_array_destroy(ht);
    }
}

/// Verifies `zend_symtable_update` turns a numeric string key into an integer key instead of a
/// string key.
#[test]
fn symtable_updates_route_numeric_strings_to_integer_keys() {
    let _guard = lock();
    unsafe {
        let ht = _zend_new_array(0);
        let key = string::new(b"42");
        let mut value = long(1);
        zend_symtable_update(ht, key, &mut value);
        assert!(!zend_hash_index_find(ht, 42).is_null());
        assert!(zend_hash_str_find(ht, c"42".as_ptr(), 2).is_null());
        string::zend_string_release(key);
        zend_array_destroy(ht);
    }
}

// ------------------------------------------------------------ argument parsing

static ARG_INFO: [zend_internal_arg_info; 3] = [
    zend_internal_arg_info { name: 1 as *const c_char, r#type: zend_type { ptr: ptr::null_mut(), type_mask: 0 }, default_value: ptr::null() },
    zend_internal_arg_info { name: c"times".as_ptr(), r#type: zend_type { ptr: ptr::null_mut(), type_mask: MAY_BE_LONG }, default_value: ptr::null() },
    zend_internal_arg_info { name: c"label".as_ptr(), r#type: zend_type { ptr: ptr::null_mut(), type_mask: MAY_BE_STRING }, default_value: ptr::null() },
];

// SAFETY: read-only arginfo, as an extension declares it.
unsafe impl Sync for zend_internal_arg_info {}

/// Runs `body` as if inside the internal function `repeat(int $times, string
/// $label)` called with `args`.
unsafe fn in_frame(args: &[zval], body: impl FnOnce(*mut zend_execute_data)) {
    let function = crate::alloc::alloc_struct::<zend_internal_function>();
    (*function).r#type = ZEND_INTERNAL_FUNCTION;
    (*function).function_name = string::intern(b"repeat");
    (*function).num_args = 2;
    (*function).required_num_args = 1;
    (*function).arg_info = ARG_INFO.as_ptr().add(1).cast_mut();
    let frame = crate::api::new_frame(function.cast(), args.len() as u32);
    for (i, arg) in args.iter().enumerate() {
        copy(crate::api::frame_arg(frame, i as u32 + 1), arg);
    }
    let outer = crate::startup::eg().current_execute_data;
    crate::startup::eg().current_execute_data = frame;
    body(frame);
    crate::startup::eg().current_execute_data = outer;
}

/// Test helper: takes the pending exception and returns its class chain and message, or None when
/// none is pending.
unsafe fn pending_message() -> Option<(String, String)> {
    crate::exception::take_pending().map(|(message, _, chain)| (chain, String::from_utf8_lossy(&message).into_owned()))
}

/// Verifies weak-mode argument coercions follow PHP: surrounding whitespace is tolerated, trailing
/// garbage and non-numeric strings are rejected, numeric strings become floats, and string booleans
/// convert.
#[test]
fn weak_mode_coercions_follow_php() {
    let _guard = lock();
    unsafe {
        let mut out = 0;
        assert!(crate::zpp::zend_parse_arg_long_slow(&str_zval(" 12 "), &mut out, 1));
        assert_eq!(out, 12);
        assert!(!crate::zpp::zend_parse_arg_long_slow(&str_zval("12abc"), &mut out, 1));
        assert!(!crate::zpp::zend_parse_arg_long_slow(&str_zval("abc"), &mut out, 1));
        let mut d = 0.0;
        assert!(crate::zpp::zend_parse_arg_double_slow(&str_zval("1e3"), &mut d, 1));
        assert_eq!(d, 1000.0);
        let mut flag = false;
        assert!(crate::zpp::zend_parse_arg_bool_slow(&str_zval("0"), &mut flag, 1));
        assert!(!flag);
        let mut number = long(5);
        let mut text: *mut zend_string = ptr::null_mut();
        assert!(crate::zpp::zend_parse_arg_str_slow(&mut number, &mut text, 1));
        assert_eq!(bytes(text), b"5");
        zval_ptr_dtor(&mut number);
    }
}

/// Verifies the messages of argument errors match PHP's wording and class chains: a wrong type
/// TypeError, ArgumentCountError for too few or too many arguments, and a ValueError naming the
/// argument.
#[test]
fn argument_errors_read_like_php() {
    let _guard = lock();
    unsafe {
        let array = {
            let mut zv = zval::UNDEF;
            set_arr(&mut zv, _zend_new_array(0));
            zv
        };
        in_frame(&[array], |_| {
            crate::zpp::zend_wrong_parameter_type_error(1, Z_EXPECTED_LONG, &array as *const zval as *mut zval);
        });
        assert_eq!(
            pending_message().unwrap(),
            ("TypeError,Error".to_string(), "repeat(): Argument #1 ($times) must be of type int, array given".to_string())
        );
        in_frame(&[], |_| crate::zpp::zend_wrong_parameters_count_error(1, 2));
        assert_eq!(
            pending_message().unwrap(),
            (
                "ArgumentCountError,TypeError,Error".to_string(),
                "repeat() expects at least 1 argument, 0 given".to_string()
            )
        );
        let extra = [long(1), long(2), long(3)];
        in_frame(&extra, |_| crate::zpp::zend_wrong_parameters_count_error(2, 2));
        assert_eq!(pending_message().unwrap().1, "repeat() expects exactly 2 arguments, 3 given");
        in_frame(&[], |_| {
            let message = c"must not be empty";
            crate::exception::elephc_zend_throw_argument_message(
                crate::exception::zend_ce_value_error,
                2,
                message.as_ptr(),
                message.to_bytes().len(),
            );
        });
        assert_eq!(pending_message().unwrap().1, "repeat(): Argument #2 ($label) must not be empty");
        let mut array = array;
        zval_ptr_dtor(&mut array);
    }
}

/// Verifies a null passed to a scalar parameter is accepted as the zero value without throwing an
/// exception.
#[test]
fn null_to_a_scalar_parameter_is_deprecated_but_accepted() {
    let _guard = lock();
    unsafe {
        let null_arg = null();
        in_frame(&[null_arg], |_| {
            let mut out = 7;
            assert!(crate::zpp::zend_parse_arg_long_slow(&null_arg, &mut out, 1));
            assert_eq!(out, 0);
        });
        assert!(crate::startup::eg().exception.is_null());
    }
}

// ------------------------------------------------------------ exceptions and objects

/// Verifies thrown exceptions carry their class chain, message and code, and that a second
/// exception raised while one is pending takes the first as its previous exception.
#[test]
fn exceptions_carry_their_class_chain_code_and_previous() {
    let _guard = lock();
    unsafe {
        crate::exception::zend_throw_exception(crate::exception::spl_ce_RuntimeException, c"first".as_ptr(), 1);
        let first = crate::startup::eg().exception;
        crate::exception::zend_throw_exception(crate::exception::spl_ce_InvalidArgumentException, c"second".as_ptr(), 2);
        let second = crate::startup::eg().exception;
        let previous = crate::object::find_property(second, b"previous");
        assert_eq!((*previous).value.obj, first, "the earlier exception becomes the previous one");
        let (message, code, chain) = crate::exception::take_pending().unwrap();
        assert_eq!(message, b"second");
        assert_eq!(code, 2);
        assert_eq!(chain, "InvalidArgumentException,LogicException,Exception");
        assert!(crate::startup::eg().exception.is_null());
    }
}

/// Verifies `EH_THROW` turns a warning into an exception of the installed class, even under `@`
/// and a zero `error_reporting`, sets `severity` only on an `ErrorException`, and that restoring
/// the saved mode reports warnings again.
#[test]
fn eh_throw_turns_warnings_into_exceptions() {
    let _guard = lock();
    unsafe {
        let mut saved = std::mem::zeroed::<zend_error_handling>();
        crate::error::zend_replace_error_handling(EH_THROW, crate::exception::spl_ce_RuntimeException, &mut saved);
        let reporting = crate::startup::eg().error_reporting;
        crate::startup::eg().error_reporting = 0;
        crate::error::warning("boom");
        crate::startup::eg().error_reporting = reporting;
        let thrown = crate::startup::eg().exception;
        assert!(crate::object::find_property(thrown, b"severity").is_null(), "only ErrorException has a severity");
        assert_eq!(pending_message(), Some(("RuntimeException,Exception".to_string(), "boom".to_string())));
        crate::error::zend_replace_error_handling(EH_THROW, crate::exception::zend_ce_error_exception, ptr::null_mut());
        crate::error::warning("again");
        let thrown = crate::startup::eg().exception;
        assert_eq!((*crate::object::find_property(thrown, b"severity")).value.lval, E_WARNING as zend_long);
        crate::exception::take_pending();
        crate::error::zend_restore_error_handling(&mut saved);
        assert_eq!(crate::startup::eg().error_handling, EH_NORMAL);
    }
}

/// Verifies `ZEND_PARSE_PARAMS_QUIET` silences the callback and path refusals too, as php-src's
/// classic parser does: the parse fails with nothing thrown, while the loud parse throws.
#[test]
fn quiet_parsing_throws_nothing() {
    let _guard = lock();
    unsafe {
        let mut callable = long(42);
        let mut fci = std::mem::zeroed::<zend_fcall_info>();
        let mut fcc = std::mem::zeroed::<zend_fcall_info_cache>();
        let mut slots = [(&mut fci as *mut zend_fcall_info).cast::<std::ffi::c_void>(), (&mut fcc as *mut zend_fcall_info_cache).cast()];
        let parse = |flags: c_int, arg: *mut zval, spec: &CStr, slots: &mut [*mut std::ffi::c_void]| {
            crate::zpp::elephc_zend_parse_one_arg(flags, 1, arg, spec.as_ptr(), slots.as_mut_ptr(), slots.len())
        };
        in_frame(&[callable], |_| {
            assert_eq!(parse(ZEND_PARSE_PARAMS_QUIET, &mut callable, c"f", &mut slots), FAILURE);
        });
        assert!(crate::startup::eg().exception.is_null(), "a quiet callback refusal throws nothing");
        in_frame(&[callable], |_| {
            assert_eq!(parse(0, &mut callable, c"f", &mut slots), FAILURE);
        });
        assert!(pending_message().is_some_and(|(_, message)| message.contains("must be a valid callback")));
        let mut path = str_zval("a\0b");
        let mut text: *mut c_char = ptr::null_mut();
        let mut len = 0usize;
        let mut path_slots = [(&mut text as *mut *mut c_char).cast::<std::ffi::c_void>(), (&mut len as *mut usize).cast()];
        in_frame(&[path], |_| {
            assert_eq!(parse(ZEND_PARSE_PARAMS_QUIET, &mut path, c"p", &mut path_slots), FAILURE);
        });
        assert!(crate::startup::eg().exception.is_null(), "a quiet path refusal throws nothing");
        zval_ptr_dtor(&mut path);
    }
}

/// Verifies an object's truthiness is its `cast_object` handler's `_IS_BOOL` answer, as GMP's and
/// SimpleXML's are in PHP, and that `is_numeric_str_function` refuses trailing data.
#[test]
fn object_truthiness_asks_the_cast_handler() {
    unsafe extern "C" fn always_false(_object: *mut zend_object, out: *mut zval, r#type: c_int) -> zend_result {
        if r#type == _IS_BOOL as c_int {
            (*out).u1.type_info = IS_FALSE as u32;
            return SUCCESS;
        }
        if r#type == IS_LONG as c_int {
            set_long(out, 7);
            return SUCCESS;
        }
        FAILURE
    }
    let _guard = lock();
    unsafe {
        let mut object = zval::UNDEF;
        crate::object::object_init(&mut object);
        assert!(crate::convert::zend_is_true(&object), "a standard object is true");
        let handlers = Box::into_raw(Box::new(ptr::read((*object.value.obj).handlers)));
        (*handlers).cast_object = Some(always_false);
        (*object.value.obj).handlers = handlers;
        assert!(!crate::convert::zend_is_true(&object), "the handler answers false");
        assert_eq!(crate::convert::zval_get_long(&object), 7, "the handler answers the integer");
        assert_eq!(crate::convert::zval_get_double(&object), 1.0, "a refused float cast falls back to one");
        (*object.value.obj).handlers = &crate::object::std_object_handlers;
        zval_ptr_dtor(&mut object);
        drop(Box::from_raw(handlers));
        let numeric = string::new(b"12abc");
        let (mut lval, mut dval) = (0, 0.0);
        assert_eq!(crate::convert::is_numeric_str_function(numeric, &mut lval, &mut dval), 0);
        string::zend_string_release(numeric);
    }
}

/// Verifies objects store and read properties, builtin exception classes inherit as in PHP, class
/// lookup ignores case and a leading backslash, and `instanceof` checks answer correctly.
#[test]
fn objects_hold_properties_and_classes_inherit() {
    let _guard = lock();
    unsafe {
        let mut object = zval::UNDEF;
        crate::object::object_init(&mut object);
        crate::object::zend_update_property_long(ptr::null_mut(), object.value.obj, c"n".as_ptr(), 1, 5);
        let mut rv = zval::UNDEF;
        let n = crate::object::zend_read_property(ptr::null_mut(), object.value.obj, c"n".as_ptr(), 1, false, &mut rv);
        assert_eq!((*n).value.lval, 5);
        assert!(crate::class::instanceof_function(
            crate::exception::spl_ce_OutOfBoundsException,
            crate::exception::spl_ce_RuntimeException
        ));
        assert!(crate::class::instanceof_function(crate::exception::zend_ce_type_error, crate::exception::zend_ce_throwable));
        assert!(!crate::class::instanceof_function(crate::exception::zend_ce_exception, crate::exception::zend_ce_error));
        let found = crate::class::zend_lookup_class(string::intern(b"\\runtimeexception"));
        assert_eq!(found, ptr::addr_of!(crate::exception::spl_ce_RuntimeException).read());
        zval_ptr_dtor(&mut object);
    }
}

/// The engine reports the program's SAPI: `cli`, or PHP's built-in server for a `--web` program.
#[test]
fn the_engine_reports_the_program_sapi() {
    let _guard = lock();
    unsafe {
        crate::host::elephc_php_ext_sapi(c"cli-server".as_ptr());
        assert_eq!(crate::startup::sapi_name(), c"cli-server");
        assert!(!crate::unsupported::sapi_cli_is_cli());
        crate::host::elephc_php_ext_sapi(c"cli".as_ptr());
        assert_eq!(crate::startup::sapi_name(), c"cli");
        assert!(crate::unsupported::sapi_cli_is_cli());
    }
}

/// An enum's cases exist, but `object_init_ex` refuses the enum itself as php-src does.
#[test]
fn enums_have_cases_but_cannot_be_instantiated() {
    let _guard = lock();
    unsafe {
        let ce = crate::class::zend_register_internal_enum(c"ReviewSuit".as_ptr(), IS_UNDEF as u8, ptr::null());
        crate::class::zend_enum_add_case_cstr(ce, c"Hearts".as_ptr(), ptr::null_mut());
        let case = crate::class::zend_enum_get_case(ce, string::intern(b"Hearts"));
        assert!(!case.is_null() && (*case).ce == ce);
        let mut object = zval::UNDEF;
        assert_eq!(crate::object::object_init_ex(&mut object, ce), FAILURE);
        assert_eq!(type_of(&object), IS_NULL);
        assert_eq!(
            pending_message(),
            Some(("Error".into(), "Cannot instantiate enum ReviewSuit".into()))
        );
    }
}

// ------------------------------------------------------------ INI

static mut LIMIT: zend_long = 0;

/// Verifies a configured php.ini value reaches the registered directive's update handler, is
/// readable as integer and string, can be altered at runtime and read back in original form, and
/// unregisters cleanly.
#[test]
fn configured_directives_reach_update_handlers() {
    let _guard = lock();
    unsafe {
        crate::ini::set_configuration(b"test.limit", b"64M");
        let defs = [
            zend_ini_entry_def {
                name: c"test.limit".as_ptr(),
                on_modify: Some(crate::ini::OnUpdateLong),
                mh_arg1: ptr::null_mut(),
                mh_arg2: ptr::addr_of_mut!(LIMIT).cast(),
                mh_arg3: ptr::null_mut(),
                value: c"1".as_ptr(),
                displayer: None,
                value_length: 1,
                name_length: 10,
                modifiable: ZEND_INI_ALL as u8,
            },
            std::mem::zeroed(),
        ];
        crate::ini::zend_register_ini_entries(defs.as_ptr(), 900);
        assert_eq!(ptr::addr_of!(LIMIT).read(), 64 << 20);
        assert_eq!(crate::ini::zend_ini_long(c"test.limit".as_ptr(), 10, 0), 64);
        let name = string::intern(b"test.limit");
        assert_eq!(crate::ini::zend_alter_ini_entry_chars(name, c"2k".as_ptr(), 2, ZEND_INI_USER, ZEND_INI_STAGE_RUNTIME), SUCCESS);
        assert_eq!(ptr::addr_of!(LIMIT).read(), 2048);
        assert_eq!(CStr::from_ptr(crate::ini::zend_ini_string(c"test.limit".as_ptr(), 10, 1)).to_bytes(), b"64M");
        crate::ini::zend_unregister_ini_entries(900);
    }
}

// ------------------------------------------------------------ iconv

/// Verifies iconv converts Latin-1 to UTF-8, reports an illegal sequence while keeping the
/// converted prefix, and reports a wrong charset without producing output.
#[test]
fn iconv_converts_and_reports_illegal_sequences() {
    let _guard = lock();
    unsafe {
        let mut out: *mut zend_string = ptr::null_mut();
        let latin1 = b"caf\xe9";
        let status = crate::stdlib::php_iconv_string(latin1.as_ptr().cast(), latin1.len(), &mut out, c"UTF-8".as_ptr(), c"ISO-8859-1".as_ptr());
        assert_eq!(status, PHP_ICONV_ERR_SUCCESS);
        assert_eq!(bytes(out), "café".as_bytes());
        string::zend_string_release(out);

        let broken = b"ok\xff";
        let status = crate::stdlib::php_iconv_string(broken.as_ptr().cast(), broken.len(), &mut out, c"UTF-16LE".as_ptr(), c"UTF-8".as_ptr());
        assert_eq!(status, PHP_ICONV_ERR_ILLEGAL_SEQ);
        assert_eq!(bytes(out), b"o\0k\0", "what converted before the bad byte is kept");
        string::zend_string_release(out);

        let status = crate::stdlib::php_iconv_string(latin1.as_ptr().cast(), latin1.len(), &mut out, c"NO-SUCH-CHARSET".as_ptr(), c"UTF-8".as_ptr());
        assert_eq!(status, PHP_ICONV_ERR_WRONG_CHARSET);
        assert!(out.is_null());
    }
}

// ------------------------------------------------------------ unserialize

/// Decodes `text` with `php_var_unserialize`, as an extension does.
unsafe fn unserialize(text: &[u8]) -> Option<zval> {
    let mut value = zval::UNDEF;
    let mut cursor = text.as_ptr();
    let mut var_hash: php_unserialize_data_t = ptr::null_mut();
    let ok = crate::serialize::php_var_unserialize(&mut value, &mut cursor, text.as_ptr().add(text.len()), &mut var_hash);
    crate::serialize::php_var_unserialize_destroy(var_hash);
    if ok == 1 {
        Some(value)
    } else {
        zval_ptr_dtor(&mut value);
        None
    }
}

/// A key repeated after a back-reference was recorded into its first value
/// keeps that value alive: `R:3` still names the inner string.
#[test]
fn back_references_survive_a_repeated_key() {
    let _guard = lock();
    unsafe {
        let mut value = unserialize(br#"a:3:{i:0;a:1:{i:0;s:3:"foo";}i:0;N;i:1;R:3;}"#).expect("PHP accepts it");
        let table = value.value.arr;
        assert_eq!((*table).nNumOfElements, 2);
        assert_eq!(type_of(zend_hash_index_find(table, 0)), IS_NULL);
        let second = deref(zend_hash_index_find(table, 1));
        assert_eq!(bytes((*second).value.str), b"foo");
        zval_ptr_dtor(&mut value);
    }
}

/// Malformed lengths fail the decode instead of aborting the program.
#[test]
fn malformed_string_lengths_fail_cleanly() {
    let _guard = lock();
    unsafe {
        assert!(unserialize(br#"s:-1:"";"#).is_none());
        assert!(unserialize(br#"s:18446744073709551615:"";"#).is_none());
        assert!(unserialize(br#"s:9:"short";"#).is_none());
        // Counts that do not fit, or that the input cannot hold, never size a
        // table that decoding would then have to grow under saved slots.
        assert!(unserialize(br#"a:4294967296:{i:0;i:1;i:1;R:2;}"#).is_none());
        assert!(unserialize(br#"a:-1:{}"#).is_none());
        assert!(unserialize(br#"a:100:{i:0;N;}"#).is_none());
        assert!(unserialize(br#"O:8:"stdClass":4294967296:{}"#).is_none());
        // PHP's grammar is `b:[01];`.
        assert!(unserialize(b"b:2;").is_none());
        assert!(unserialize(b"b:-1;").is_none());
        let truth = unserialize(b"b:1;").expect("a well-formed boolean");
        assert_eq!(type_of(&truth), IS_TRUE);
        // An integer past the range saturates, as php-src's `parse_iv2` (after a warning).
        let big = unserialize(b"i:9223372036854775808;").expect("saturates");
        assert_eq!(big.value.lval, i64::MAX);
        let small = unserialize(b"i:-9223372036854775809;").expect("saturates");
        assert_eq!(small.value.lval, i64::MIN);
        assert!(unserialize(b"i:12x;").is_none());
        let mut ok = unserialize(br#"s:2:"ok";"#).expect("a well-formed string");
        assert_eq!(bytes(ok.value.str), b"ok");
        zval_ptr_dtor(&mut ok);
    }
}

// ------------------------------------------------------------ operators

/// Test helper: the class chain and message of the pending exception, which is then discarded.
unsafe fn take_error() -> Option<(String, String)> {
    pending_message()
}

/// `div_function` divides two ints in integer arithmetic as php-src does: an exact quotient
/// stays an int (even past 2^53, where a double would round), LONG_MIN / -1 becomes a float, an
/// `zend_strtod` reads only a decimal number, as php-src's: no hexadecimal, `inf`, `nan` or
/// leading space, and an exponent without digits is left unread.
#[test]
fn strtod_reads_only_decimal_numbers() {
    unsafe {
        let parse = |text: &CStr| {
            let mut end = ptr::null();
            let value = crate::convert::zend_strtod(text.as_ptr(), &mut end);
            (value, end.offset_from(text.as_ptr()))
        };
        assert_eq!(parse(c"1.5e3x"), (1500.0, 5));
        assert_eq!(parse(c"-.25"), (-0.25, 4));
        assert_eq!(parse(c"7."), (7.0, 2));
        assert_eq!(parse(c"2e"), (2.0, 1));
        assert_eq!(parse(c"2e+"), (2.0, 1));
        assert_eq!(parse(c"0x1A"), (0.0, 1));
        assert_eq!(parse(c"inf"), (0.0, 0));
        assert_eq!(parse(c"nan"), (0.0, 0));
        assert_eq!(parse(c" 1"), (0.0, 0));
        assert_eq!(parse(c"-"), (0.0, 0));
        assert_eq!(parse(c"."), (0.0, 0));
        assert_eq!(parse(c"0.1"), (0.1, 3));
    }
}

/// inexact quotient is a float, and a zero divisor throws DivisionByZeroError.
#[test]
fn integer_division_follows_php() {
    let _guard = lock();
    unsafe {
        let divide = |a: zend_long, b: zend_long| {
            let (mut x, mut y, mut r) = (long(a), long(b), zval::UNDEF);
            let status = crate::convert::div_function(&mut r, &mut x, &mut y);
            (status, r)
        };
        let (status, r) = divide(9007199254740993, 1);
        assert_eq!(status, SUCCESS);
        assert_eq!((type_of(&r), r.value.lval), (IS_LONG, 9007199254740993));
        let (_, r) = divide(7, 2);
        assert_eq!((type_of(&r), r.value.dval), (IS_DOUBLE, 3.5));
        let (_, r) = divide(zend_long::MIN, -1);
        assert_eq!((type_of(&r), r.value.dval), (IS_DOUBLE, 9.223372036854775808e18));
        let (_, r) = divide(-6, 3);
        assert_eq!((type_of(&r), r.value.lval), (IS_LONG, -2));
        let (status, _) = divide(1, 0);
        assert_eq!(status, FAILURE);
        assert_eq!(take_error(), Some(("DivisionByZeroError,ArithmeticError,Error".into(), "Division by zero".into())));
    }
}

/// Arithmetic operands follow PHP 8.5 (measured with `php -n`): `"abc" + 1` throws
/// `TypeError: Unsupported operand types: string + int`, `"12abc" + 1` warns and is 13, an
/// array minus an int throws, and `(int)` of 2^63 wraps to `PHP_INT_MIN`.
#[test]
fn arithmetic_operands_follow_php() {
    let _guard = lock();
    unsafe {
        let (mut text, mut one, mut r) = (str_zval("abc"), long(1), zval::UNDEF);
        assert_eq!(crate::convert::add_function(&mut r, &mut text, &mut one), FAILURE);
        assert_eq!(
            take_error(),
            Some(("TypeError,Error".into(), "Unsupported operand types: string + int".into()))
        );
        zval_ptr_dtor(&mut text);
        let mut prefixed = str_zval("12abc");
        assert_eq!(crate::convert::add_function(&mut r, &mut prefixed, &mut one), SUCCESS);
        assert_eq!((type_of(&r), r.value.lval), (IS_LONG, 13));
        zval_ptr_dtor(&mut prefixed);
        let mut array = zval::UNDEF;
        set_arr(&mut array, _zend_new_array(0));
        assert_eq!(crate::convert::sub_function(&mut r, &mut array, &mut one), FAILURE);
        assert_eq!(
            take_error(),
            Some(("TypeError,Error".into(), "Unsupported operand types: array - int".into()))
        );
        zval_ptr_dtor(&mut array);
        assert_eq!(crate::convert::zend_dval_to_lval(9223372036854775808.0), zend_long::MIN);
    }
}

/// Test helper: a GMP-style `do_operation` that answers the opcode times 100 for any operands.
unsafe extern "C" fn overloaded_operation(opcode: u8, result: *mut zval, _op1: *mut zval, _op2: *mut zval) -> zend_result {
    set_long(result, opcode as zend_long * 100);
    SUCCESS
}

/// An object whose handlers overload arithmetic computes it, on either side, as php-src asks the
/// object's `do_operation` before refusing the operand types.
#[test]
fn objects_with_do_operation_overload_arithmetic() {
    let _guard = lock();
    unsafe {
        // As an extension does: a copy of the standard handlers with one hook replaced.
        let handlers: &'static mut zend_object_handlers =
            Box::leak(Box::new(ptr::read(&crate::object::std_object_handlers)));
        handlers.do_operation = Some(overloaded_operation);
        let mut object = zval::UNDEF;
        crate::object::object_init(&mut object);
        (*object.value.obj).handlers = handlers;
        let (mut one, mut r) = (long(1), zval::UNDEF);
        assert_eq!(crate::convert::add_function(&mut r, &mut object, &mut one), SUCCESS);
        assert_eq!(r.value.lval, 100);
        assert_eq!(crate::convert::div_function(&mut r, &mut one, &mut object), SUCCESS);
        assert_eq!(r.value.lval, 400);
        assert!(take_error().is_none());
        (*object.value.obj).handlers = &crate::object::std_object_handlers;
        zval_ptr_dtor(&mut object);
    }
}

/// An object compared with a non-object follows php-src (`php -r` with a stdClass): above null,
/// above an array (and an array below it), cast to bool against a bool, a noticed 1 against an
/// int, and above a string it cannot be cast to.
#[test]
fn objects_compare_with_other_types_like_php() {
    let _guard = lock();
    unsafe {
        let mut object = zval::UNDEF;
        crate::object::object_init(&mut object);
        let mut array = zval::UNDEF;
        set_arr(&mut array, _zend_new_array(0));
        let cases: Vec<(zval, c_int, c_int)> = vec![
            (null(), 1, -1),
            (array, 1, -1),
            (long(1), 0, 0),
            (long(2), -1, 1),
            (str_zval("abc"), 1, -1),
        ];
        for (mut other, object_first, other_first) in cases {
            assert_eq!(crate::convert::zend_compare(&mut object, &mut other), object_first);
            assert_eq!(crate::convert::zend_compare(&mut other, &mut object), other_first);
            zval_ptr_dtor(&mut other);
        }
        let (mut yes, mut no) = (zval::UNDEF, zval::UNDEF);
        set_bool(&mut yes, true);
        set_bool(&mut no, false);
        assert_eq!(crate::convert::zend_compare(&mut object, &mut yes), 0);
        assert_eq!(crate::convert::zend_compare(&mut object, &mut no), 1);
        assert!(crate::startup::eg().exception.is_null());
        zval_ptr_dtor(&mut object);
    }
}

// ------------------------------------------------------------ hash tables

static mut GRACEFUL_TABLE: *mut HashTable = ptr::null_mut();
static mut GRACEFUL_SEEN: Vec<u32> = Vec::new();
static mut GRACEFUL_VICTIM: zend_ulong = 0;

/// Element destructor for the graceful-destroy test: records how many entries the table still
/// counts, and on its first call deletes another entry from the same table.
unsafe extern "C" fn graceful_dtor(_value: *mut zval) {
    let seen = &mut *ptr::addr_of_mut!(GRACEFUL_SEEN);
    seen.push((*GRACEFUL_TABLE).nNumOfElements);
    if seen.len() == 1 {
        zend_hash_index_del(GRACEFUL_TABLE, GRACEFUL_VICTIM);
    }
}

/// Runs a graceful destroy of a three-entry table whose first destructor call deletes key
/// `victim`; returns the entry counts each destructor call saw.
unsafe fn graceful_run(destroy: unsafe extern "C" fn(*mut HashTable), victim: zend_ulong) -> Vec<u32> {
    let mut table: HashTable = std::mem::zeroed();
    init(&mut table, 8, Some(graceful_dtor), false);
    for i in 0..3 {
        let mut value = long(i);
        zend_hash_index_add(&mut table, i as zend_ulong, &mut value);
    }
    GRACEFUL_TABLE = &mut table;
    GRACEFUL_VICTIM = victim;
    (*ptr::addr_of_mut!(GRACEFUL_SEEN)).clear();
    destroy(&mut table);
    GRACEFUL_TABLE = ptr::null_mut();
    std::mem::take(&mut *ptr::addr_of_mut!(GRACEFUL_SEEN))
}

/// The graceful destroys delete entry by entry as php-src does: each bucket leaves the count
/// before its destructor runs, so a destructor that deletes from the same table neither sees a
/// dead entry nor makes the destroy run a destructor twice.
#[test]
fn graceful_destroys_are_reentrant() {
    let _guard = lock();
    unsafe {
        assert_eq!(graceful_run(zend_hash_graceful_destroy, 2), vec![2, 1, 0]);
        assert_eq!(graceful_run(zend_hash_graceful_reverse_destroy, 0), vec![2, 1, 0]);
    }
}

/// The C-string `_ind` deletions treat an `IS_INDIRECT` slot as `zend_hash_del_ind` does: the
/// pointed-at zval is undefined (and the bucket kept), and a second delete fails.
#[test]
fn string_ind_deletes_undefine_indirect_slots() {
    let _guard = lock();
    unsafe {
        for symtable in [false, true] {
            let ht = _zend_new_array(0);
            let mut target = str_zval("held");
            let mut slot = zval::UNDEF;
            slot.value.zv = &mut target;
            slot.u1.type_info = IS_INDIRECT as u32;
            zend_hash_str_add(ht, c"prop".as_ptr(), 4, &mut slot);
            let delete = |ht| if symtable { zend_symtable_str_del_ind(ht, c"prop".as_ptr(), 4) } else { zend_hash_str_del_ind(ht, c"prop".as_ptr(), 4) };
            assert_eq!(delete(ht), SUCCESS);
            assert_eq!(type_of(&target), IS_UNDEF);
            assert_eq!((*ht).nNumOfElements, 1, "the bucket stays");
            assert_ne!((*ht).u.flags & HASH_FLAG_HAS_EMPTY_IND, 0);
            assert_eq!(delete(ht), FAILURE);
            zend_array_destroy(ht);
        }
    }
}

/// An add over an `IS_INDIRECT` slot whose target is undefined fills that target only when the
/// caller passes `HASH_UPDATE_INDIRECT`, for both key forms, as php-src; a plain add is refused.
#[test]
fn adds_fill_an_undefined_indirect_slot_only_when_asked() {
    let _guard = lock();
    unsafe {
        for by_string in [false, true] {
            let ht = _zend_new_array(0);
            let mut target = zval::UNDEF;
            let mut slot = zval::UNDEF;
            slot.value.zv = &mut target;
            slot.u1.type_info = IS_INDIRECT as u32;
            zend_hash_str_add(ht, c"var".as_ptr(), 3, &mut slot);
            let key = crate::string::new(b"var");
            let add = |flag: u32, value: &mut zval| {
                if by_string {
                    zend_hash_str_add_or_update(ht, c"var".as_ptr(), 3, value, flag)
                } else {
                    zend_hash_add_or_update(ht, key, value, flag)
                }
            };
            let mut one = zval::UNDEF;
            set_long(&mut one, 1);
            assert!(add(HASH_ADD, &mut one).is_null(), "a plain add is refused");
            assert_eq!(type_of(&target), IS_UNDEF);
            assert_eq!(add(HASH_ADD | HASH_UPDATE_INDIRECT, &mut one), &mut target as *mut zval);
            assert_eq!(type_of(&target), IS_LONG);
            let mut two = zval::UNDEF;
            set_long(&mut two, 2);
            assert!(add(HASH_ADD | HASH_UPDATE_INDIRECT, &mut two).is_null(), "a defined target is kept");
            assert_eq!(target.value.lval, 1);
            crate::string::zend_string_release(key);
            zend_array_destroy(ht);
        }
    }
}

/// A `*_add_mem` insert refused because the key exists allocates nothing.
#[test]
fn refused_mem_adds_do_not_allocate() {
    let _guard = lock();
    unsafe {
        let ht = _zend_new_array(0);
        let mut data = [7u8; 16];
        let key = string::new(b"k");
        assert!(!zend_hash_add_mem(ht, key, data.as_mut_ptr().cast(), 16).is_null());
        let mut zero = long(0);
        assert!(!zend_hash_index_add(ht, 0, &mut zero).is_null());
        let before = crate::alloc::elephc_zend_live_blocks();
        assert!(zend_hash_add_mem(ht, key, data.as_mut_ptr().cast(), 16).is_null());
        assert!(zend_hash_str_add_mem(ht, c"k".as_ptr(), 1, data.as_mut_ptr().cast(), 16).is_null());
        // The next free key is taken, so the append is refused.
        (*ht).nNextFreeElement = 0;
        assert!(zend_hash_next_index_insert_mem(ht, data.as_mut_ptr().cast(), 16).is_null());
        assert_eq!(crate::alloc::elephc_zend_live_blocks(), before);
        crate::alloc::free(zend_hash_find_ptr(ht, key));
        string::zend_string_release(key);
        zend_array_destroy(ht);
    }
}

// ------------------------------------------------------------ calls, classes and constants

/// Handler of the global function `review_twice(int $n)`: returns `$n * 2`.
unsafe extern "C" fn review_twice(execute_data: *mut zend_execute_data, return_value: *mut zval) {
    let arg = crate::api::frame_arg(execute_data, 1);
    set_long(return_value, (*arg).value.lval * 2);
}

/// `zend_call_method` with neither an object nor a class calls the global function of that name,
/// as php-src does, instead of reading through a NULL object.
#[test]
fn call_method_without_object_calls_the_global_function() {
    let _guard = lock();
    unsafe {
        let entries = [
            zend_function_entry {
                fname: c"review_twice".as_ptr(),
                handler: Some(review_twice),
                arg_info: ARG_INFO.as_ptr(),
                num_args: 1,
                flags: 0,
                frameless_function_infos: ptr::null(),
                doc_comment: ptr::null(),
            },
            std::mem::zeroed(),
        ];
        assert_eq!(crate::module::zend_register_functions(ptr::null_mut(), entries.as_ptr(), ptr::null_mut(), 0), SUCCESS);
        let mut arg = long(21);
        let mut result = zval::UNDEF;
        let returned = crate::api::zend_call_method(
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            c"Review_Twice".as_ptr(),
            12,
            &mut result,
            1,
            &mut arg,
            ptr::null_mut(),
        );
        assert_eq!(returned, &mut result as *mut zval);
        assert_eq!((type_of(&result), result.value.lval), (IS_LONG, 42));
    }
}

/// Registers an internal class named `name` extending `parent` (NULL for none).
unsafe fn test_class(name: &[u8], parent: *mut zend_class_entry) -> *mut zend_class_entry {
    let mut entry: zend_class_entry = std::mem::zeroed();
    entry.name = string::intern(name);
    crate::class::zend_register_internal_class_ex(&mut entry, parent)
}

/// `zend_fetch_class` gives the called scope for `static`, the executing scope for `self` and its
/// parent for `parent`; `zend_get_constant_ex` resolves `self::`/`parent::` against the scope it is
/// given, throws for a missing one or a missing constant, and stays silent when asked to.
#[test]
fn scoped_class_and_constant_fetches_follow_php() {
    let _guard = lock();
    unsafe {
        let base = test_class(b"ReviewBase", ptr::null_mut());
        crate::class::zend_declare_class_constant_long(base, c"CODE".as_ptr(), 4, 7);
        let child = test_class(b"ReviewChild", base);

        let function = crate::alloc::alloc_struct::<zend_internal_function>();
        (*function).r#type = ZEND_INTERNAL_FUNCTION;
        (*function).function_name = string::intern(b"probe");
        (*function).scope = base;
        let frame = crate::api::new_frame(function.cast(), 0);
        (*frame).This.value.ce = child;
        let outer = crate::startup::eg().current_execute_data;
        crate::startup::eg().current_execute_data = frame;
        assert_eq!(crate::class::zend_fetch_class(ptr::null_mut(), ZEND_FETCH_CLASS_STATIC), child);
        assert_eq!(crate::class::zend_fetch_class(ptr::null_mut(), ZEND_FETCH_CLASS_SELF), base);
        // AUTO decides by the name: self, parent and static resolve against the frame, any other
        // name is an ordinary lookup.
        let auto = |name: &[u8]| crate::class::zend_fetch_class(string::intern(name), ZEND_FETCH_CLASS_AUTO);
        assert_eq!(auto(b"SeLf"), base);
        assert_eq!(auto(b"static"), child);
        assert_eq!(auto(b"ReviewBase"), base);
        assert!(crate::class::zend_fetch_class(string::intern(b"parent"), ZEND_FETCH_CLASS_AUTO | ZEND_FETCH_CLASS_EXCEPTION).is_null());
        assert_eq!(
            take_error(),
            Some(("Error".into(), "Cannot access \"parent\" when current class scope has no parent".into()))
        );
        assert!(crate::class::zend_fetch_class(ptr::null_mut(), ZEND_FETCH_CLASS_PARENT | ZEND_FETCH_CLASS_EXCEPTION).is_null());
        assert_eq!(
            take_error(),
            Some(("Error".into(), "Cannot access \"parent\" when current class scope has no parent".into()))
        );
        crate::startup::eg().current_execute_data = outer;
        crate::alloc::free(frame.cast());

        let get = |name: &[u8], scope, flags| crate::constants::zend_get_constant_ex(string::intern(name), scope, flags);
        let code = get(b"self::CODE", base, 0);
        assert_eq!((*code).value.lval, 7);
        assert_eq!((*get(b"parent::CODE", child, 0)).value.lval, 7);
        assert_eq!((*get(b"ReviewChild::CODE", ptr::null_mut(), 0)).value.lval, 7);
        assert!(get(b"self::CODE", ptr::null_mut(), 0).is_null());
        assert_eq!(take_error(), Some(("Error".into(), "Cannot access \"self\" when no class scope is active".into())));
        assert!(get(b"self::NOPE", base, 0).is_null());
        assert_eq!(take_error(), Some(("Error".into(), "Undefined constant self::NOPE".into())));
        assert!(get(b"self::NOPE", base, ZEND_FETCH_CLASS_SILENT).is_null());
        assert!(take_error().is_none());
    }
}

/// Verifies class constants honour visibility as php-src's `zend_verify_const_access` does: a
/// private constant is readable only from its declaring class, a protected one from the family,
/// a public one from anywhere; a refused read throws `Cannot access private constant C::X` unless
/// `ZEND_FETCH_CLASS_SILENT` is set, through both `zend_get_constant_ex` and
/// `zend_get_class_constant_ex`.
#[test]
fn class_constants_honour_visibility() {
    let _guard = lock();
    unsafe {
        let base = test_class(b"VisibilityBase", ptr::null_mut());
        let unrelated = test_class(b"VisibilityOther", ptr::null_mut());
        let child = test_class(b"VisibilityChild", base);
        for (name, access, value) in [(&b"SECRET"[..], ZEND_ACC_PRIVATE, 1), (b"GUARDED", ZEND_ACC_PROTECTED, 2), (b"OPEN", ZEND_ACC_PUBLIC, 3)] {
            let mut zv = zval::UNDEF;
            set_long(&mut zv, value);
            crate::class::zend_declare_class_constant_ex(base, string::intern(name), &mut zv, access as c_int, ptr::null_mut());
        }
        let by_name = |name: &[u8], scope, flags| crate::constants::zend_get_constant_ex(string::intern(name), scope, flags);
        let by_class = |class: &[u8], name: &[u8], scope, flags| {
            crate::class::zend_get_class_constant_ex(string::intern(class), string::intern(name), scope, flags)
        };
        let value = |zv: *mut zval| {
            assert!(!zv.is_null());
            (*zv).value.lval
        };

        // Public: readable from anywhere.
        assert_eq!(value(by_name(b"VisibilityBase::OPEN", ptr::null_mut(), 0)), 3);
        assert_eq!(value(by_class(b"VisibilityBase", b"OPEN", unrelated, 0)), 3);
        // Private: only the declaring class.
        assert_eq!(value(by_name(b"VisibilityBase::SECRET", base, 0)), 1);
        for scope in [ptr::null_mut(), child, unrelated] {
            assert!(by_name(b"VisibilityBase::SECRET", scope, 0).is_null());
            assert_eq!(
                take_error(),
                Some(("Error".into(), "Cannot access private constant VisibilityBase::SECRET".into()))
            );
            assert!(by_class(b"VisibilityBase", b"SECRET", scope, 0).is_null());
            assert_eq!(
                take_error(),
                Some(("Error".into(), "Cannot access private constant VisibilityBase::SECRET".into()))
            );
        }
        // Protected: the declaring class and its descendants, not strangers.
        assert_eq!(value(by_name(b"VisibilityBase::GUARDED", base, 0)), 2);
        assert_eq!(value(by_name(b"VisibilityBase::GUARDED", child, 0)), 2);
        assert_eq!(value(by_class(b"VisibilityBase", b"GUARDED", child, 0)), 2);
        for scope in [ptr::null_mut(), unrelated] {
            assert!(by_name(b"VisibilityBase::GUARDED", scope, 0).is_null());
            assert_eq!(
                take_error(),
                Some(("Error".into(), "Cannot access protected constant VisibilityBase::GUARDED".into()))
            );
        }
        // Silent: refused without an error.
        assert!(by_name(b"VisibilityBase::SECRET", unrelated, ZEND_FETCH_CLASS_SILENT).is_null());
        assert!(by_class(b"VisibilityBase", b"GUARDED", unrelated, ZEND_FETCH_CLASS_SILENT).is_null());
        assert!(take_error().is_none());
    }
}

/// A namespaced constant is found by its exact name; only `zend_get_constant_ex` also matches its
/// namespace part in another case, as php-src's lookups do.
#[test]
fn namespaced_constant_lookups_follow_php() {
    let _guard = lock();
    unsafe {
        crate::constants::zend_register_long_constant(c"review\\NS_CONST".as_ptr(), 15, 9, CONST_PERSISTENT, 0);
        assert_eq!((*crate::constants::zend_get_constant_str(c"review\\NS_CONST".as_ptr(), 15)).value.lval, 9);
        assert!(crate::constants::zend_get_constant_str(c"Review\\NS_CONST".as_ptr(), 15).is_null());
        let ex = crate::constants::zend_get_constant_ex(string::intern(b"Review\\NS_CONST"), ptr::null_mut(), 0);
        assert_eq!((*ex).value.lval, 9);
    }
}

/// A subclass inherits its parent's public and protected constants but not the private ones, so
/// it may declare a constant of a private one's name, as in PHP.
#[test]
fn private_constants_are_not_inherited() {
    let _guard = lock();
    unsafe {
        let base = test_class(b"InheritBase", ptr::null_mut());
        for (name, access, value) in [(&b"MINE"[..], ZEND_ACC_PRIVATE, 1), (b"SHARED", ZEND_ACC_PUBLIC, 2)] {
            let mut zv = zval::UNDEF;
            set_long(&mut zv, value);
            crate::class::zend_declare_class_constant_ex(base, string::intern(name), &mut zv, access as c_int, ptr::null_mut());
        }
        let child = test_class(b"InheritChild", base);
        let mut own = zval::UNDEF;
        set_long(&mut own, 7);
        let declared =
            crate::class::zend_declare_class_constant_ex(child, string::intern(b"MINE"), &mut own, ZEND_ACC_PUBLIC as c_int, ptr::null_mut());
        assert!(!declared.is_null(), "the child declares its own MINE");
        let read = |name: &[u8]| {
            crate::class::zend_get_class_constant_ex(string::intern(b"InheritChild"), string::intern(name), child, 0)
        };
        assert_eq!((*read(b"MINE")).value.lval, 7);
        assert_eq!((*read(b"SHARED")).value.lval, 2);
        assert!(take_error().is_none());
    }
}

// ------------------------------------------------------------ leaks

/// The callback errors free the detail string even when an exception is already pending, a freed
/// resource frees its `zend_resource`, and a refused constant or INI entry frees its copy.
#[test]
fn error_and_refusal_paths_free_what_they_own() {
    let _guard = lock();
    unsafe {
        let live = crate::alloc::elephc_zend_live_blocks;

        crate::exception::throw_error(crate::exception::zend_ce_exception, "already pending");
        let before = live();
        let detail = crate::alloc::estrdup(c"no array or string given".as_ptr());
        crate::zpp::zend_wrong_callback_error(1, detail);
        let detail = crate::alloc::estrdup(c"no array or string given".as_ptr());
        crate::zpp::zend_wrong_callback_or_null_error(1, detail);
        assert_eq!(live(), before);
        assert_eq!(take_error().map(|(_, message)| message), Some("already pending".into()));

        let kind = crate::api::zend_register_list_destructors_ex(None, None, c"review".as_ptr(), 0);
        // The first resource allocates the regular list's storage, which is not the resource's.
        let warm = crate::api::zend_register_resource(ptr::null_mut(), kind);
        crate::api::zend_list_delete(warm);
        let before = live();
        let resource = crate::api::zend_register_resource(ptr::null_mut(), kind);
        (*resource).gc.refcount = 0;
        crate::api::zend_list_free(resource);
        assert_eq!(live(), before);

        crate::constants::zend_register_long_constant(c"REVIEW_DUP".as_ptr(), 10, 1, CONST_PERSISTENT, 0);
        let before = live();
        assert!(crate::constants::zend_register_long_constant(c"REVIEW_DUP".as_ptr(), 10, 2, CONST_PERSISTENT, 0).is_null());
        assert_eq!(live(), before);

        let defs = [
            zend_ini_entry_def {
                name: c"review.dup".as_ptr(),
                on_modify: None,
                mh_arg1: ptr::null_mut(),
                mh_arg2: ptr::null_mut(),
                mh_arg3: ptr::null_mut(),
                value: c"1".as_ptr(),
                displayer: None,
                value_length: 1,
                name_length: 10,
                modifiable: ZEND_INI_ALL as u8,
            },
            std::mem::zeroed(),
        ];
        crate::ini::zend_register_ini_entries(defs.as_ptr(), 901);
        let before = live();
        crate::ini::zend_register_ini_entries(defs.as_ptr(), 902);
        assert_eq!(live(), before);
        crate::ini::zend_unregister_ini_entries(901);
    }
}

/// Verifies a module registering an INI directive another module owns fails as in php-src:
/// `zend_register_ini_entries` returns FAILURE and unregisters every directive that module had
/// registered (its earlier ones in the same table too), leaving the first owner's untouched.
#[test]
fn a_duplicate_ini_entry_fails_the_registering_module() {
    let _guard = lock();
    unsafe {
        let def = |name: &'static CStr| zend_ini_entry_def {
            name: name.as_ptr(),
            on_modify: None,
            mh_arg1: ptr::null_mut(),
            mh_arg2: ptr::null_mut(),
            mh_arg3: ptr::null_mut(),
            value: c"1".as_ptr(),
            displayer: None,
            value_length: 1,
            name_length: name.to_bytes().len() as u16,
            modifiable: ZEND_INI_ALL as u8,
        };
        let first = [def(c"owner.shared"), std::mem::zeroed()];
        let second = [def(c"loser.before"), def(c"owner.shared"), def(c"loser.after"), std::mem::zeroed()];
        assert_eq!(crate::ini::zend_register_ini_entries(first.as_ptr(), 911), SUCCESS);
        assert_eq!(crate::ini::zend_register_ini_entries(second.as_ptr(), 912), FAILURE);
        let directives = crate::startup::eg().ini_directives;
        let has = |name: &[u8]| !crate::hash::zend_hash_str_find_ptr(directives, name.as_ptr().cast(), name.len()).is_null();
        assert!(has(b"owner.shared"), "the first owner keeps its directive");
        assert!(!has(b"loser.before"), "the failing module's earlier directive is unregistered");
        assert!(!has(b"loser.after"), "and the ones after the clash were never registered");
        crate::ini::zend_unregister_ini_entries(911);
    }
}

// ------------------------------------------------------------ module lifecycle

static LIFECYCLE: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

/// Test helper: records a lifecycle event and reports success.
fn event(name: &'static str) -> zend_result {
    LIFECYCLE.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).push(name);
    SUCCESS
}

unsafe extern "C" fn minit(_type: c_int, _number: c_int) -> zend_result {
    event("MINIT")
}
unsafe extern "C" fn rinit(_type: c_int, _number: c_int) -> zend_result {
    event("RINIT")
}
unsafe extern "C" fn rshutdown(_type: c_int, _number: c_int) -> zend_result {
    event("RSHUTDOWN")
}
unsafe extern "C" fn mshutdown(_type: c_int, _number: c_int) -> zend_result {
    event("MSHUTDOWN")
}
unsafe extern "C" fn close_regular(_res: *mut zend_resource) {
    event("ld");
}
unsafe extern "C" fn close_persistent(_res: *mut zend_resource) {
    event("pld");
}

/// Verifies a hosted module sees PHP's lifecycle: MINIT then RINIT when first used, a request
/// boundary (the `--web` worker's next request) runs RSHUTDOWN then RINIT again, and the program's
/// exit runs RSHUTDOWN then MSHUTDOWN once, in that order, however often the hook is called.
/// A resource type's two destructors keep their roles, as in php-src: closing a resource runs the
/// regular one, and a persistent resource, found again by its key, gets the persistent one when
/// the persistent list is destroyed, before MSHUTDOWN.
#[test]
fn hosted_modules_follow_the_request_lifecycle() {
    let _guard = lock();
    unsafe {
        let module: &'static mut zend_module_entry = Box::leak(Box::new(std::mem::zeroed()));
        module.size = std::mem::size_of::<zend_module_entry>() as u16;
        module.zend_api = ZEND_MODULE_API_NO as u32;
        module.name = c"lifecycle_probe".as_ptr();
        module.version = c"1".as_ptr();
        module.build_id = c"API20250925,NTS".as_ptr();
        module.module_startup_func = Some(minit);
        module.request_startup_func = Some(rinit);
        module.request_shutdown_func = Some(rshutdown);
        module.module_shutdown_func = Some(mshutdown);
        let seen = || LIFECYCLE.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();

        assert!(crate::host::boot_module(module));
        assert!(crate::host::boot_module(module), "a second use starts nothing");
        assert_eq!(seen(), ["MINIT", "RINIT"]);

        crate::host::elephc_php_ext_request_shutdown();
        crate::host::elephc_php_ext_request_shutdown();
        crate::host::elephc_php_ext_request_startup();
        crate::host::elephc_php_ext_request_startup();
        assert_eq!(seen(), ["MINIT", "RINIT", "RSHUTDOWN", "RINIT"], "each half runs once per request");

        let kind = crate::api::zend_register_list_destructors_ex(
            Some(close_regular),
            Some(close_persistent),
            c"link".as_ptr(),
            0,
        );
        let regular = crate::api::zend_register_resource(ptr::null_mut(), kind);
        crate::api::zend_list_close(regular);
        crate::api::zend_list_delete(regular);
        // One left open: the request's end closes it.
        let left_open = crate::api::zend_register_resource(ptr::null_mut(), kind);
        let persistent = crate::api::zend_register_persistent_resource(c"link:a".as_ptr(), 6, ptr::null_mut(), kind);
        let found = crate::hash::zend_hash_str_find(&crate::startup::eg().persistent_list, c"link:a".as_ptr(), 6);
        assert!(!found.is_null() && (*found).value.res == persistent, "found again by its key");
        assert_eq!((*persistent).handle, -1);

        crate::host::elephc_php_ext_exit_hook();
        crate::host::elephc_php_ext_exit_hook();
        assert_eq!(
            seen(),
            ["MINIT", "RINIT", "RSHUTDOWN", "RINIT", "ld", "RSHUTDOWN", "ld", "pld", "MSHUTDOWN"]
        );
        // `left_open` was closed (the second "ld") and freed with its request, as php-src's
        // `zend_destroy_rsrc_list` does: nothing is left to release.
        let _ = left_open;
    }
}

/// The resource type `teardown_rinit` opens a resource of, for `teardown_lifecycle_*`.
static TEARDOWN_KIND: Mutex<c_int> = Mutex::new(0);
/// What the module's directive read in each RINIT and RSHUTDOWN, for the INI assertions.
static TEARDOWN_SEEN: Mutex<Vec<i64>> = Mutex::new(Vec::new());

/// Test helper: the current value of the `teardown.level` directive.
unsafe fn teardown_level() -> i64 {
    crate::ini::zend_ini_long(c"teardown.level".as_ptr(), 14, 0)
}

unsafe extern "C" fn teardown_minit(_type: c_int, number: c_int) -> zend_result {
    let defs = [
        zend_ini_entry_def {
            name: c"teardown.level".as_ptr(),
            on_modify: Some(teardown_on_modify),
            mh_arg1: ptr::null_mut(),
            mh_arg2: ptr::null_mut(),
            mh_arg3: ptr::null_mut(),
            value: c"1".as_ptr(),
            displayer: None,
            value_length: 1,
            name_length: 14,
            modifiable: ZEND_INI_ALL as u8,
        },
        std::mem::zeroed(),
    ];
    crate::ini::zend_register_ini_entries(defs.as_ptr(), number);
    *TEARDOWN_KIND.lock().unwrap() =
        crate::api::zend_register_list_destructors_ex(Some(teardown_close), None, c"teardown".as_ptr(), number);
    event("MINIT")
}
unsafe extern "C" fn teardown_rinit(_type: c_int, _number: c_int) -> zend_result {
    TEARDOWN_SEEN.lock().unwrap().push(teardown_level());
    // A request allocation the module never frees, and a resource it leaves open: both are the
    // request's end to reclaim.
    crate::alloc::emalloc(4096);
    crate::api::zend_register_resource(ptr::null_mut(), *TEARDOWN_KIND.lock().unwrap());
    event("RINIT")
}
unsafe extern "C" fn teardown_rshutdown(_type: c_int, _number: c_int) -> zend_result {
    TEARDOWN_SEEN.lock().unwrap().push(teardown_level());
    event("RSHUTDOWN")
}
unsafe extern "C" fn teardown_post_deactivate() -> zend_result {
    event("POST")
}
unsafe extern "C" fn teardown_close(_res: *mut zend_resource) {
    event("RSRC");
}
unsafe extern "C" fn teardown_on_modify(
    _entry: *mut zend_ini_entry,
    _value: *mut zend_string,
    _arg1: *mut std::ffi::c_void,
    _arg2: *mut std::ffi::c_void,
    _arg3: *mut std::ffi::c_void,
    stage: c_int,
) -> zend_result {
    if stage == ZEND_INI_STAGE_DEACTIVATE {
        event("INI");
    }
    SUCCESS
}

/// Verifies a request is torn down in php-src's `php_request_shutdown` order and leaves nothing
/// behind for the next one (issues #2102, #2104, #2105):
/// - RSHUTDOWN runs first and still sees the request's INI override; the open resource is closed,
///   then the directive is restored through its handler at the deactivation stage, and only then
///   does the post-deactivate hook run;
/// - the next request's RINIT sees the configured value again;
/// - an `emalloc` block RINIT leaves behind is freed with its request, so 100 requests end at the
///   same live-block count, while a persistent block survives and a block the module freed or
///   moved itself is not freed a second time.
#[test]
fn a_request_is_torn_down_in_php_order_and_leaves_nothing_behind() {
    let _guard = lock();
    unsafe {
        let module: &'static mut zend_module_entry = Box::leak(Box::new(std::mem::zeroed()));
        module.size = std::mem::size_of::<zend_module_entry>() as u16;
        module.zend_api = ZEND_MODULE_API_NO as u32;
        module.name = c"teardown_probe".as_ptr();
        module.version = c"1".as_ptr();
        module.build_id = c"API20250925,NTS".as_ptr();
        module.module_startup_func = Some(teardown_minit);
        module.request_startup_func = Some(teardown_rinit);
        module.request_shutdown_func = Some(teardown_rshutdown);
        module.post_deactivate_func = Some(teardown_post_deactivate);
        LIFECYCLE.lock().unwrap().clear();
        TEARDOWN_SEEN.lock().unwrap().clear();
        let seen = || std::mem::take(&mut *LIFECYCLE.lock().unwrap());
        // Between the two calls the program releases the request's objects (its hosting tables):
        // `OBJ` marks where, so the order pins resources closing before any object is freed.
        let next_request = || {
            crate::host::elephc_php_ext_request_shutdown();
            event("OBJ");
            crate::host::elephc_php_ext_request_deactivate();
            crate::host::elephc_php_ext_request_startup();
        };

        assert!(crate::host::boot_module(module));
        assert_eq!(seen(), ["MINIT", "RINIT"]);

        // A runtime override lasts the request: RSHUTDOWN still sees 9, the next RINIT sees 1.
        // Persistent: these outlive the request they are made in, which sweeps request strings.
        let name = crate::string::zend_string_init(c"teardown.level".as_ptr(), 14, true);
        let nine = crate::string::zend_string_init(c"9".as_ptr(), 1, true);
        assert_eq!(crate::ini::zend_alter_ini_entry(name, nine, ZEND_INI_USER, ZEND_INI_STAGE_RUNTIME), SUCCESS);
        let persistent = crate::alloc::__zend_malloc(64);
        let freed = crate::alloc::emalloc(32);
        crate::alloc::efree(freed);
        let moved = crate::alloc::erealloc(crate::alloc::emalloc(16), 4096);
        crate::alloc::efree(moved);
        next_request();
        assert_eq!(seen(), ["RSHUTDOWN", "RSRC", "OBJ", "INI", "POST", "RINIT"], "php_request_shutdown order");
        assert_eq!(*TEARDOWN_SEEN.lock().unwrap(), [1, 9, 1], "RINIT 1, RSHUTDOWN 9, next RINIT 1");
        assert_eq!(teardown_level(), 1);

        let baseline = crate::alloc::elephc_zend_live_blocks();
        for _ in 0..100 {
            // A request block grown through the persistent `perealloc` backend moves: the sweep
            // must free it at its new address, never the old one.
            crate::alloc::__zend_realloc(crate::alloc::emalloc(16), 8192);
            // A request string and a request array (header and bucket storage) nobody releases
            // go with the request too, as php-src's memory manager frees them.
            crate::string::zend_string_alloc(32, false);
            let leaked = crate::hash::_zend_new_array(0);
            crate::hash::zend_hash_next_index_insert(leaked, &mut std::mem::zeroed());
            next_request();
        }
        assert_eq!(crate::alloc::elephc_zend_live_blocks(), baseline, "each request's leftovers go with it");
        *persistent.cast::<u8>() = 1;
        crate::alloc::free(persistent);

        // Each further request: RSHUTDOWN, its resource, its objects, post-deactivate, the next
        // RINIT (no INI phase: nothing was changed). The program's exit then ends the last request
        // the same way.
        crate::host::elephc_php_ext_exit_hook();
        let events = seen();
        assert_eq!(events.len(), 100 * 5 + 3, "{events:?}");
        assert_eq!(&events[..5], ["RSHUTDOWN", "RSRC", "OBJ", "POST", "RINIT"]);
        assert_eq!(&events[events.len() - 3..], ["RSHUTDOWN", "RSRC", "POST"]);
        crate::string::zend_string_release(name);
        crate::string::zend_string_release(nine);
    }
}
