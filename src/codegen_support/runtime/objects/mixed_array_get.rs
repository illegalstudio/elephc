//! Purpose:
//! Emits the `__rt_mixed_array_get` runtime helper for `$mixed[$key]` access.
//! Routes boxed JSON-style values to indexed-array, hash, stdClass, or `ArrayAccess` paths.
//!
//! Called from:
//! - `crate::codegen_support::runtime::objects::emit_mixed_array_get()`.
//!
//! Key details:
//! - The key tuple matches `emit_normalized_hash_key`: int keys use `key_hi = -1`.
//! - Callers pass an explicit warning flag so normal reads diagnose missing/null
//!   offsets while `isset()`/`??` keep the same helper quiet.
//! - A container payload that is null or the in-band null-container sentinel
//!   (`NULL_SENTINEL`, materialized by a missed read forwarded through a ternary merge)
//!   is treated as an absent container and also returns `Mixed(null)` (issue #585).
//! - Every successful return is an owned `Mixed*`; borrowed array/hash slots are retained first.
//! - Objects resolve in two steps: the runtime's OWN containers by class id, then any PHP class
//!   declaring `ArrayAccess`, through the dense `_class_offsetget_ptrs` table. Without the second
//!   step a synthetic `ArrayObject` read as `$m["a"]` answered null while `$m->offsetGet("a")` on
//!   the SAME object answered correctly.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::sentinels::emit_branch_if_null_container;

/// Dispatches to the target-specific `__rt_mixed_array_get` emitter.
///
/// Checks `emitter.target.arch` and routes to either `emit_mixed_array_get_x86_64`
/// (SysV ABI) or `emit_mixed_array_get_aarch64` (AAPCS64). The helper is emitted
/// once into the runtime object and is called by generated code for `$mixed[$key]`
/// access on a boxed `Mixed` value.
pub fn emit_mixed_array_get(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_array_get_x86_64(emitter);
        return;
    }
    emit_mixed_array_get_aarch64(emitter);
}

/// Emits `__rt_mixed_array_get` for ARM64 (AAPCS64 ABI).
///
/// Inputs arrive in `x0` = mixed_ptr, `x1` = key_lo, `x2` = key_hi,
/// `x3` = nonzero when missing/null-offset warnings are enabled.
/// Returns an owned pointer to a boxed `Mixed` cell in `x0`.
///
/// The function dispatches on the mixed value's tag:
/// - Tag 4 → indexed array path
/// - Tag 5 → associative array path
/// - Tag 6 → stdClass object path
/// - All others → null (boxed `Mixed(null)`)
///
/// For indexed arrays the key must be integer (`key_hi == -1` sentinel); string keys
/// return null. For objects, `stdClass` supports string keys only, the runtime's own SPL
/// containers dispatch by class id, and any other class reaches its `ArrayAccess::offsetGet`
/// through `_class_offsetget_ptrs`; a class declaring no such interface returns null. Missing keys return null. All paths that
/// produce a value box it through `__rt_mixed_from_value` except when storage already
/// holds a boxed `Mixed` pointer (tag 7), which is retained before returning.
fn emit_mixed_array_get_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_array_get ---");
    emitter.label_global("__rt_mixed_array_get");

    // Stack:
    //   [sp, #0]  = mixed_ptr
    //   [sp, #8]  = key_lo
    //   [sp, #16] = key_hi
    //   [sp, #24] = saved x29
    //   [sp, #32] = saved x30
    //   [sp, #40] = warn_on_missing
    emitter.instruction("sub sp, sp, #48");                                     // reserve frame: 3 inputs + saved fp/lr (16-byte aligned)
    emitter.instruction("stp x29, x30, [sp, #24]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #24");                                    // set new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save mixed_ptr
    emitter.instruction("str x1, [sp, #8]");                                    // save key_lo
    emitter.instruction("str x2, [sp, #16]");                                   // save key_hi
    emitter.instruction("str x3, [sp, #40]");                                   // save whether this read should emit PHP offset warnings

    emitter.instruction("cbz x0, __rt_mixed_array_get_null_container");         // null Mixed pointers behave as PHP null receivers
    emitter.instruction("ldr x9, [x0]");                                        // load tag from mixed[0]
    emitter.instruction("cmp x9, #4");                                          // tag = 4 (indexed array)?
    emitter.instruction("b.eq __rt_mixed_array_get_indexed");                   // branch on the current JSON decoder condition
    emitter.instruction("cmp x9, #5");                                          // tag = 5 (associative array)?
    emitter.instruction("b.eq __rt_mixed_array_get_assoc");                     // branch on the current JSON decoder condition
    emitter.instruction("cmp x9, #6");                                          // tag = 6 (object)?
    emitter.instruction("b.eq __rt_mixed_array_get_object");                    // branch on the current JSON decoder condition
    emitter.instruction("cmp x9, #8");                                          // tag = 8 (canonical PHP null)?
    emitter.instruction("b.eq __rt_mixed_array_get_null_container");            // null receivers warn only for ordinary reads
    emitter.instruction("cmp x9, #1");                                          // tag = 1 (string)?
    emitter.instruction("b.eq __rt_mixed_array_get_string");                    // a string offset read, not a container lookup
    emitter.instruction("b __rt_mixed_array_get_null");                         // any other payload → null

    // -- string receiver: `$s[$i]` reads one byte and answers a 1-character string --
    // A boxed string reaching here is ordinary PHP: `$s = fgets($h); $s[0]`. Without this
    // case it fell through to null, and because `ord(null)` is 0 the mistake was silent.
    emitter.label("__rt_mixed_array_get_string");
    emitter.instruction("ldr x10, [x0, #8]");                                   // the string payload pointer
    emitter.instruction("ldr x11, [x0, #16]");                                  // and its length
    emitter.instruction("ldr x12, [sp, #8]");                                   // the offset (key_lo holds the integer key)
    emitter.instruction("tbz x12, #63, __rt_mixed_array_get_string_abs");       // already an absolute offset
    emitter.instruction("add x12, x12, x11");                                   // php counts a negative offset back from the end
    emitter.label("__rt_mixed_array_get_string_abs");
    emitter.instruction("tbnz x12, #63, __rt_mixed_array_get_string_oob");      // still negative: before the start
    emitter.instruction("cmp x12, x11");                                        // past the last byte?
    emitter.instruction("b.hs __rt_mixed_array_get_string_oob");                // php answers "" for either direction
    emitter.instruction("ldrb w13, [x10, x12]");                                // the selected byte
    emitter.instruction("str x13, [sp, #16]");                                  // park it across the reservation (key_hi is dead here)
    emitter.instruction("mov x0, #1");                                          // one byte of storage for the result
    emitter.instruction("bl __rt_concat_reserve");                              // scratch or heap, decided by size
    emitter.instruction("ldr x13, [sp, #16]");                                  // reload the byte
    emitter.instruction("strb w13, [x0]");                                      // write the single character
    emitter.instruction("mov x1, x0");                                          // publish expects the result pointer
    emitter.instruction("mov x2, #1");                                          // and its length
    emitter.instruction("bl __rt_concat_publish");                              // advance the scratch cursor for scratch-backed results
    emitter.instruction("mov x0, #1");                                          // tag 1 = string; publish left x1/x2 untouched
    emitter.instruction("bl __rt_mixed_from_value");                            // box the 1-character result
    emitter.instruction("ldp x29, x30, [sp, #24]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame
    emitter.instruction("ret");                                                 // return Mixed* in x0

    // An out-of-range offset is "" in php, not null. php also emits
    // `Warning: Uninitialized string offset N`, which elephc does not yet compose — that
    // needs an integer rendered into the message, and the runtime has no such helper.
    emitter.label("__rt_mixed_array_get_string_oob");
    emitter.instruction("ldr x9, [sp, #40]");                                   // an ordinary read, or isset()/`??`?
    emitter.instruction("cbz x9, __rt_mixed_array_get_null");                   // isset() must see the offset as ABSENT, not as ""
    emitter.instruction("ldr x0, [sp, #8]");                                    // php names the offset AS WRITTEN, not the resolved one
    emitter.instruction("bl __rt_warn_uninitialized_string_offset");            // `@` is suppressed inside the diagnostic itself
    emitter.instruction("mov x0, #0");                                          // a zero-length reservation still yields a real pointer
    emitter.instruction("bl __rt_concat_reserve");                              // so the empty result is a valid string, not a null one
    emitter.instruction("mov x1, x0");                                          // the empty payload pointer
    emitter.instruction("mov x2, #0");                                          // with no bytes
    emitter.instruction("mov x0, #1");                                          // tag 1 = string
    emitter.instruction("bl __rt_mixed_from_value");                            // box the empty result
    emitter.instruction("ldp x29, x30, [sp, #24]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame
    emitter.instruction("ret");                                                 // return Mixed* in x0

    // Indexed array: integer key only. key_hi == -1 marks int keys.
    emitter.label("__rt_mixed_array_get_indexed");
    emitter.instruction("ldr x10, [x0, #8]");                                   // x10 = array pointer
    // treat a null or in-band null-container sentinel payload as an absent container (issue #585)
    emit_branch_if_null_container(
        emitter,
        "x10",
        "x9",
        "__rt_mixed_array_get_null_container",
    );
    emitter.instruction("ldr x11, [sp, #16]");                                  // load key_hi
    emitter.instruction("cmn x11, #1");                                         // compare with -1 (int-key sentinel)
    emitter.instruction("b.ne __rt_mixed_array_get_indexed_missing_string");    // missing string keys use PHP's string-key warning
    emitter.instruction("ldr x12, [sp, #8]");                                   // x12 = key_lo (int index)
    emitter.instruction("ldr x9, [x10]");                                       // x9 = array length (header offset 0)
    emitter.instruction("cmp x12, #0");                                         // negative index → null
    emitter.instruction("b.lt __rt_mixed_array_get_indexed_missing");           // warn and return null for a negative indexed-array key
    emitter.instruction("cmp x12, x9");                                         // index >= length → null
    emitter.instruction("b.ge __rt_mixed_array_get_indexed_missing");           // warn and return null for an out-of-bounds indexed-array key
    emitter.instruction("ldr x13, [x10, #-8]");                                 // load packed indexed-array kind metadata
    emitter.instruction("ubfx x13, x13, #8, #7");                               // extract the runtime element value_type tag
    emitter.instruction("add x10, x10, #24");                                   // skip the 24-byte array header to reach the contiguous payload
    emitter.instruction("cmp x13, #7");                                         // are indexed slots already boxed Mixed pointers?
    emitter.instruction("b.eq __rt_mixed_array_get_indexed_boxed");             // boxed slots must be retained before returning
    emitter.instruction("cmp x13, #1");                                         // do indexed slots contain string pointer/length pairs?
    emitter.instruction("b.eq __rt_mixed_array_get_indexed_string");            // string slots need a 16-byte load before boxing
    emitter.instruction("cmp x13, #8");                                         // do indexed slots represent null payloads?
    emitter.instruction("b.eq __rt_mixed_array_get_indexed_null");              // null slots have no payload to read
    emitter.instruction("ldr x1, [x10, x12, lsl #3]");                          // load scalar or pointer payload from the typed indexed slot
    emitter.instruction("mov x2, #0");                                          // typed indexed slots use one payload word except strings
    emitter.instruction("mov x0, x13");                                         // x0 = runtime value_type tag for the boxed result
    emitter.instruction("bl __rt_mixed_from_value");                            // box the typed indexed-array element into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #24]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame
    emitter.instruction("ret");                                                 // return Mixed* in x0
    emitter.label("__rt_mixed_array_get_indexed_boxed");
    emitter.instruction("ldr x0, [x10, x12, lsl #3]");                          // load the boxed Mixed pointer from the indexed slot
    emitter.instruction("cbz x0, __rt_mixed_array_get_indexed_missing");        // zero-filled gaps are undefined keys, not present null values
    emitter.instruction("bl __rt_incref");                                      // retain the stored Mixed cell so the caller owns the returned result
    emitter.instruction("ldp x29, x30, [sp, #24]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame
    emitter.instruction("ret");                                                 // return Mixed* in x0
    emitter.label("__rt_mixed_array_get_indexed_string");
    emitter.instruction("lsl x12, x12, #4");                                    // convert the element index to a 16-byte string slot offset
    emitter.instruction("add x10, x10, x12");                                   // x10 = address of the selected string slot
    emitter.instruction("ldr x1, [x10]");                                       // load string pointer from the selected slot
    emitter.instruction("ldr x2, [x10, #8]");                                   // load string length from the selected slot
    emitter.instruction("mov x0, #1");                                          // x0 = string runtime value_type tag
    emitter.instruction("bl __rt_mixed_from_value");                            // box the string indexed-array element into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #24]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame
    emitter.instruction("ret");                                                 // return Mixed* in x0
    emitter.label("__rt_mixed_array_get_indexed_null");
    emitter.instruction("mov x0, #8");                                          // x0 = null runtime value_type tag
    emitter.instruction("mov x1, #0");                                          // value_lo = 0 for null
    emitter.instruction("mov x2, #0");                                          // value_hi = 0 for null
    emitter.instruction("bl __rt_mixed_from_value");                            // box the null indexed-array element into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #24]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame
    emitter.instruction("ret");                                                 // return Mixed* in x0
    emitter.label("__rt_mixed_array_get_indexed_missing");
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload whether ordinary read warnings are enabled
    emitter.instruction("cbz x9, __rt_mixed_array_get_null");                   // `isset()`/`??` suppress undefined-key warnings
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the missing integer key for the PHP warning
    emitter.instruction("bl __rt_warn_undefined_array_key_int");                // emit or suppress the undefined-array-key warning
    emitter.instruction("b __rt_mixed_array_get_null");                         // return boxed Mixed(null) after the warning
    emitter.label("__rt_mixed_array_get_indexed_missing_string");
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload whether ordinary read warnings are enabled
    emitter.instruction("cbz x9, __rt_mixed_array_get_null");                   // `isset()`/`??` suppress undefined-key warnings
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the missing string key pointer
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the missing string key length
    emitter.instruction("bl __rt_warn_undefined_array_key_str");                // emit the PHP warning for a missing string key
    emitter.instruction("b __rt_mixed_array_get_null");                         // return boxed Mixed(null) after the warning

    // Associative array: hash_get with normalized key.
    emitter.label("__rt_mixed_array_get_assoc");
    emitter.instruction("ldr x10, [x0, #8]");                                   // x10 = hash pointer
    // treat a null or in-band null-container sentinel payload as an absent container (issue #585)
    emit_branch_if_null_container(
        emitter,
        "x10",
        "x9",
        "__rt_mixed_array_get_null_container",
    );
    emitter.instruction("mov x0, x10");                                         // x0 = hash pointer for hash_get
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = key_lo
    emitter.instruction("ldr x2, [sp, #16]");                                   // x2 = key_hi
    emitter.instruction("bl __rt_hash_get");                                    // x0=found, x1=value_lo, x2=value_hi, x3=value_tag
    emitter.instruction("cbz x0, __rt_mixed_array_get_assoc_missing");          // diagnose an absent hash key for ordinary reads
    // For value_tag == 7 the entry already holds a boxed Mixed pointer
    // (json_decode and stdClass populate hashes this way). Anything else
    // (typed string/int/array entries from non-Mixed assoc arrays passing
    // through a Mixed receiver) needs to be re-boxed via mixed_from_value
    // so callers always see a uniform Mixed cell.
    emitter.instruction("cmp x3, #7");                                          // is the hash entry already a boxed Mixed?
    emitter.instruction("b.ne __rt_mixed_array_get_assoc_box");                 // no → box (lo, hi, tag) into a fresh Mixed cell
    emitter.instruction("mov x0, x1");                                          // yes → move the stored Mixed cell into the return register
    emitter.instruction("bl __rt_incref");                                      // retain the stored Mixed cell so the caller owns the returned result
    emitter.instruction("ldp x29, x30, [sp, #24]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame
    emitter.instruction("ret");                                                 // return Mixed* in x0

    emitter.label("__rt_mixed_array_get_assoc_missing");
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload whether ordinary read warnings are enabled
    emitter.instruction("cbz x9, __rt_mixed_array_get_null");                   // `isset()`/`??` suppress undefined-key warnings
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the normalized key high word
    emitter.instruction("cmn x9, #1");                                          // does the missing key use the integer-key sentinel?
    emitter.instruction("b.eq __rt_mixed_array_get_assoc_missing_int");         // integer keys use the decimal warning formatter
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the missing string key pointer
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the missing string key length
    emitter.instruction("bl __rt_warn_undefined_array_key_str");                // emit the PHP warning for a missing string key
    emitter.instruction("b __rt_mixed_array_get_null");                         // return boxed Mixed(null) after the warning
    emitter.label("__rt_mixed_array_get_assoc_missing_int");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the missing integer key
    emitter.instruction("bl __rt_warn_undefined_array_key_int");                // emit the PHP warning for a missing integer key
    emitter.instruction("b __rt_mixed_array_get_null");                         // return boxed Mixed(null) after the warning
    emitter.label("__rt_mixed_array_get_assoc_box");
    // mixed_from_value(tag, lo, hi). Move (x1, x2, x3) into (x1, x2, x0).
    emitter.instruction("mov x0, x3");                                          // x0 = value_tag (mixed_from_value first arg)
    // x1 already holds value_lo; x2 already holds value_hi.
    emitter.instruction("bl __rt_mixed_from_value");                            // box the typed entry into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #24]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame
    emitter.instruction("ret");                                                 // return Mixed* in x0

    // Object: SPL ArrayAccess containers or stdClass with string key.
    emitter.label("__rt_mixed_array_get_object");
    emitter.instruction("ldr x10, [x0, #8]");                                   // x10 = obj pointer
    // treat a null or in-band null-container sentinel payload as an absent container (issue #585)
    emit_branch_if_null_container(
        emitter,
        "x10",
        "x9",
        "__rt_mixed_array_get_null_container",
    );
    emitter.instruction("ldr x11, [x10]");                                      // x11 = class_id
    abi::emit_symbol_address(emitter, "x12", "_spl_fixed_array_class_id");
    emitter.instruction("ldr x12, [x12]");                                      // x12 = compile-time SplFixedArray class_id
    emitter.instruction("cmp x11, x12");                                        // is the receiver a SplFixedArray instance?
    emitter.instruction("b.eq __rt_mixed_array_get_spl_fixed");                 // dispatch mixed object indexing to SplFixedArray::offsetGet
    abi::emit_symbol_address(emitter, "x12", "_spl_dll_class_id");
    emitter.instruction("ldr x12, [x12]");                                      // x12 = compile-time SplDoublyLinkedList class_id
    emitter.instruction("cmp x11, x12");                                        // is the receiver a SplDoublyLinkedList instance?
    emitter.instruction("b.eq __rt_mixed_array_get_spl_dll");                   // dispatch mixed object indexing to the list ArrayAccess helper
    abi::emit_symbol_address(emitter, "x12", "_spl_stack_class_id");
    emitter.instruction("ldr x12, [x12]");                                      // x12 = compile-time SplStack class_id
    emitter.instruction("cmp x11, x12");                                        // is the receiver a SplStack instance?
    emitter.instruction("b.eq __rt_mixed_array_get_spl_dll");                   // SplStack shares the list ArrayAccess helper
    abi::emit_symbol_address(emitter, "x12", "_spl_queue_class_id");
    emitter.instruction("ldr x12, [x12]");                                      // x12 = compile-time SplQueue class id
    emitter.instruction("cmp x11, x12");                                        // is the receiver a SplQueue instance?
    emitter.instruction("b.eq __rt_mixed_array_get_spl_dll");                   // SplQueue shares the list ArrayAccess helper
    emitter.instruction("b __rt_mixed_array_get_php_offset_get");               // every other class goes through PHP ArrayAccess or raises
    emitter.label("__rt_mixed_array_get_spl_fixed");
    emitter.instruction("str x10, [sp, #0]");                                   // save unboxed SplFixedArray receiver while boxing the key
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload normalized key high word
    emitter.instruction("cmn x11, #1");                                         // does key_hi carry the integer-key sentinel?
    emitter.instruction("b.eq __rt_mixed_array_get_spl_fixed_int_key");         // integer keys box as Mixed int
    emitter.instruction("mov x0, #1");                                          // tag = string for mixed_from_value
    emitter.instruction("ldr x1, [sp, #8]");                                    // key string pointer
    emitter.instruction("ldr x2, [sp, #16]");                                   // key string length
    emitter.instruction("b __rt_mixed_array_get_spl_fixed_box_key");            // share the offsetGet call after key boxing
    emitter.label("__rt_mixed_array_get_spl_fixed_int_key");
    emitter.instruction("mov x0, #0");                                          // tag = int for mixed_from_value
    emitter.instruction("ldr x1, [sp, #8]");                                    // key integer payload
    emitter.instruction("mov x2, #0");                                          // integer keys have no high payload
    emitter.label("__rt_mixed_array_get_spl_fixed_box_key");
    emitter.instruction("bl __rt_mixed_from_value");                            // allocate the boxed ArrayAccess offset
    emitter.instruction("mov x1, x0");                                          // pass boxed offset as argument 1
    emitter.instruction("ldr x0, [sp, #0]");                                    // pass unboxed SplFixedArray receiver as argument 0
    emitter.instruction("bl __rt_spl_fixed_offset_get");                        // read through SplFixedArray::offsetGet
    emitter.instruction("ldp x29, x30, [sp, #24]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame
    emitter.instruction("ret");                                                 // return Mixed* in x0
    emitter.label("__rt_mixed_array_get_spl_dll");
    emitter.instruction("str x10, [sp, #0]");                                   // save unboxed SPL list receiver while boxing the key
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload normalized key high word
    emitter.instruction("cmn x11, #1");                                         // does key_hi carry the integer-key sentinel?
    emitter.instruction("b.eq __rt_mixed_array_get_spl_dll_int_key");           // integer keys box as Mixed int
    emitter.instruction("mov x0, #1");                                          // tag = string for mixed_from_value
    emitter.instruction("ldr x1, [sp, #8]");                                    // key string pointer
    emitter.instruction("ldr x2, [sp, #16]");                                   // key string length
    emitter.instruction("b __rt_mixed_array_get_spl_dll_box_key");              // share the offsetGet call after key boxing
    emitter.label("__rt_mixed_array_get_spl_dll_int_key");
    emitter.instruction("mov x0, #0");                                          // tag = int for mixed_from_value
    emitter.instruction("ldr x1, [sp, #8]");                                    // key integer payload
    emitter.instruction("mov x2, #0");                                          // integer keys have no high payload
    emitter.label("__rt_mixed_array_get_spl_dll_box_key");
    emitter.instruction("bl __rt_mixed_from_value");                            // allocate the boxed ArrayAccess offset
    emitter.instruction("mov x1, x0");                                          // pass boxed offset as argument 1
    emitter.instruction("ldr x0, [sp, #0]");                                    // pass unboxed SPL list receiver as argument 0
    emitter.instruction("bl __rt_spl_dll_offset_get");                          // read through the shared SPL list offsetGet helper
    emitter.instruction("ldp x29, x30, [sp, #24]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame
    emitter.instruction("ret");                                                 // return Mixed* in x0

    // Classes above are the runtime's OWN containers, recognised by class id. Everything else is a
    // PHP class, and its `ArrayAccess::offsetGet` is reachable only through the dense method table
    // — without this a synthetic `ArrayObject` read as `$m["a"]` answered null while
    // `$m->offsetGet("a")` on the SAME object answered correctly.
    //
    // The table is consulted TWICE on purpose: once here to decide whether to commit, and again
    // after the key is boxed. Boxing allocates, so bailing out afterwards would strand a cell, and
    // no register survives the call to hold the resolved pointer across it.
    emitter.label("__rt_mixed_array_get_php_offset_get");
    emitter.instruction("tbnz x11, #63, __rt_mixed_array_get_not_indexable");   // synthetic negative ids cannot index metadata
    abi::emit_load_symbol_to_reg(emitter, "x12", "_class_iface_method_count", 0);
    emitter.instruction("cmp x11, x12");                                        // is the id within the dense table?
    emitter.instruction("b.hs __rt_mixed_array_get_not_indexable");             // out-of-range ids have no entry
    abi::emit_symbol_address(emitter, "x12", "_class_offsetget_ptrs");
    emitter.instruction("ldr x12, [x12, x11, lsl #3]");                         // resolve the concrete or inherited offsetGet
    emitter.instruction("cbz x12, __rt_mixed_array_get_not_indexable");        // 0 means the class is not ArrayAccess at all
    emitter.instruction("str x10, [sp, #0]");                                   // save unboxed receiver while boxing the key
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload normalized key high word
    emitter.instruction("cmn x11, #1");                                         // does key_hi carry the integer-key sentinel?
    emitter.instruction("b.eq __rt_mixed_array_get_php_int_key");               // integer keys box as Mixed int
    emitter.instruction("mov x0, #1");                                          // tag = string for mixed_from_value
    emitter.instruction("ldr x1, [sp, #8]");                                    // key string pointer
    emitter.instruction("ldr x2, [sp, #16]");                                   // key string length
    emitter.instruction("b __rt_mixed_array_get_php_box_key");                  // share the offsetGet call after key boxing
    emitter.label("__rt_mixed_array_get_php_int_key");
    emitter.instruction("mov x0, #0");                                          // tag = int for mixed_from_value
    emitter.instruction("ldr x1, [sp, #8]");                                    // key integer payload
    emitter.instruction("mov x2, #0");                                          // integer keys have no high payload
    emitter.label("__rt_mixed_array_get_php_box_key");
    emitter.instruction("bl __rt_mixed_from_value");                            // allocate the boxed ArrayAccess offset
    emitter.instruction("str x0, [sp, #8]");                                    // stash the box; key_lo has served its purpose
    emitter.instruction("mov x1, x0");                                          // pass boxed offset as argument 1
    emitter.instruction("ldr x0, [sp, #0]");                                    // pass unboxed receiver as argument 0
    emitter.instruction("ldr x11, [x0]");                                       // reload class id, clobbered across the boxing call
    abi::emit_symbol_address(emitter, "x12", "_class_offsetget_ptrs");
    emitter.instruction("ldr x12, [x12, x11, lsl #3]");                         // re-resolve offsetGet for the same class
    emitter.instruction("blr x12");                                             // read through PHP's ArrayAccess::offsetGet
    emitter.instruction("str x0, [sp, #0]");                                    // stash the owned result; the receiver is done
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the boxed offset
    emitter.instruction("bl __rt_decref_mixed");                                // a PHP method BORROWS its argument, so this frame frees the box
    emitter.instruction("ldr x0, [sp, #0]");                                    // recover the owned result
    emitter.instruction("ldp x29, x30, [sp, #24]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame
    emitter.instruction("ret");                                                 // return Mixed* in x0

    // PHP does not answer for an object it cannot index — not even quietly. `$o["k"]` on anything
    // that is not ArrayAccess raises, in every context including `isset`, `??` and `empty`, so
    // this leaves through the Error rather than through the shared null tail.
    emitter.label("__rt_mixed_array_get_not_indexable");
    emitter.instruction("mov x0, x10");                                         // pass the unboxed receiver to name its class
    emitter.instruction("ldp x29, x30, [sp, #24]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame before the tail-call
    emitter.instruction("b __rt_throw_object_not_array");                       // never returns

    emitter.label("__rt_mixed_array_get_null_container");
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload whether ordinary read warnings are enabled
    emitter.instruction("cbz x9, __rt_mixed_array_get_null");                   // quiet read contexts suppress the null-offset warning
    emitter.instruction("bl __rt_warn_array_offset_on_null");                   // emit PHP's warning for indexing a null receiver
    emitter.label("__rt_mixed_array_get_null");
    emitter.instruction("mov x0, #8");                                          // tag = 8 (null)
    emitter.instruction("mov x1, #0");                                          // value_lo = 0
    emitter.instruction("mov x2, #0");                                          // value_hi = 0
    emitter.instruction("bl __rt_mixed_from_value");                            // box null into a fresh Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #24]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame
    emitter.instruction("ret");                                                 // return Mixed* in x0
}

/// Emits `__rt_mixed_array_get` for x86_64 (SysV ABI).
///
/// Inputs arrive in `rdi` = mixed_ptr, `rsi` = key_lo, `rdx` = key_hi,
/// `rcx` = nonzero when missing/null-offset warnings are enabled.
/// Returns an owned pointer to a boxed `Mixed` cell in `rax`.
///
/// Same dispatch and return semantics as `emit_mixed_array_get_aarch64`:
/// - Tag 4 → indexed array, tag 5 → associative array, tag 6 → object (stdClass, an SPL
///   container, or any PHP `ArrayAccess`)
/// - Integer keys on indexed arrays required (`key_hi == -1`); string keys return null
/// - Objects: only `stdClass` with string key supported; int keys return null
/// - Missing keys and unsupported payloads return boxed `Mixed(null)`
/// - Slots already holding a boxed `Mixed` (tag 7) are retained before return;
///   all other values are boxed through `__rt_mixed_from_value`
fn emit_mixed_array_get_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_array_get ---");
    emitter.label_global("__rt_mixed_array_get");

    // Inputs (SysV): rdi = mixed_ptr, rsi = key_lo, rdx = key_hi, rcx = warn_on_missing.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 32");                                         // reserve slots for the 3 saved inputs (16-byte aligned)
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save mixed_ptr
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save key_lo
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save key_hi
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save whether this read should emit PHP offset warnings

    emitter.instruction("test rdi, rdi");                                       // null Mixed → null
    emitter.instruction("je __rt_mixed_array_get_null_container");              // null Mixed pointers behave as PHP null receivers
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load tag from mixed[0]
    emitter.instruction("cmp r10, 4");                                          // tag = 4 (indexed array)?
    emitter.instruction("je __rt_mixed_array_get_indexed");                     // branch on the current JSON decoder condition
    emitter.instruction("cmp r10, 5");                                          // tag = 5 (associative array)?
    emitter.instruction("je __rt_mixed_array_get_assoc");                       // branch on the current JSON decoder condition
    emitter.instruction("cmp r10, 6");                                          // tag = 6 (object)?
    emitter.instruction("je __rt_mixed_array_get_object");                      // branch on the current JSON decoder condition
    emitter.instruction("cmp r10, 8");                                          // tag = 8 (canonical PHP null)?
    emitter.instruction("je __rt_mixed_array_get_null_container");              // null receivers warn only for ordinary reads
    emitter.instruction("cmp r10, 1");                                          // tag = 1 (string)?
    emitter.instruction("je __rt_mixed_array_get_string");                      // a string offset read, not a container lookup
    emitter.instruction("jmp __rt_mixed_array_get_null");                       // any other payload → null

    // -- string receiver: `$s[$i]` reads one byte and answers a 1-character string --
    // See the AArch64 half. Silent before this case existed, because `ord(null)` is 0.
    emitter.label("__rt_mixed_array_get_string");
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // the string payload pointer
    emitter.instruction("mov r11, QWORD PTR [rdi + 16]");                       // and its length
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // the offset (key_lo holds the integer key)
    emitter.instruction("test rsi, rsi");                                       // is it already absolute?
    emitter.instruction("jns __rt_mixed_array_get_string_abs");                 // yes: use it as it stands
    emitter.instruction("add rsi, r11");                                        // php counts a negative offset back from the end
    emitter.label("__rt_mixed_array_get_string_abs");
    emitter.instruction("test rsi, rsi");                                       // still negative: before the start
    emitter.instruction("js __rt_mixed_array_get_string_oob");                  // php answers "" for either direction
    emitter.instruction("cmp rsi, r11");                                        // past the last byte?
    emitter.instruction("jae __rt_mixed_array_get_string_oob");
    emitter.instruction("movzx eax, BYTE PTR [r10 + rsi]");                     // the selected byte
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // park it across the reservation (key_hi is dead here)
    emitter.instruction("mov rax, 1");                                          // one byte of storage for the result
    emitter.instruction("call __rt_concat_reserve");                            // scratch or heap, decided by size
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the byte
    emitter.instruction("mov BYTE PTR [rax], r10b");                            // write the single character
    emitter.instruction("mov rdx, 1");                                          // publish expects the length in rdx
    emitter.instruction("call __rt_concat_publish");                            // advance the scratch cursor for scratch-backed results
    emitter.instruction("mov rdi, rax");                                        // value_lo = the result pointer
    emitter.instruction("mov rsi, rdx");                                        // value_hi = its length
    emitter.instruction("mov rax, 1");                                          // tag 1 = string
    emitter.instruction("call __rt_mixed_from_value");                          // box the 1-character result
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return Mixed* in rax

    // An out-of-range offset is "" in php, not null. The accompanying
    // `Warning: Uninitialized string offset N` is not composed yet — it needs an integer
    // rendered into the message and the runtime has no helper for that.
    emitter.label("__rt_mixed_array_get_string_oob");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // an ordinary read, or isset()/`??`?
    emitter.instruction("test r10, r10");
    emitter.instruction("jz __rt_mixed_array_get_null");                        // isset() must see the offset as ABSENT, not as ""
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // php names the offset AS WRITTEN, not the resolved one
    emitter.instruction("call __rt_warn_uninitialized_string_offset");          // `@` is suppressed inside the diagnostic itself
    emitter.instruction("mov rax, 0");                                          // a zero-length reservation still yields a real pointer
    emitter.instruction("call __rt_concat_reserve");                            // so the empty result is a valid string, not a null one
    emitter.instruction("mov rdi, rax");                                        // value_lo = the empty payload pointer
    emitter.instruction("mov rsi, 0");                                          // value_hi = no bytes
    emitter.instruction("mov rax, 1");                                          // tag 1 = string
    emitter.instruction("call __rt_mixed_from_value");                          // box the empty result
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return Mixed* in rax

    emitter.label("__rt_mixed_array_get_indexed");
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // r10 = array pointer
    emit_branch_if_null_container(
        emitter,
        "r10",
        "r11",
        "__rt_mixed_array_get_null_container",
    );
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // load key_hi
    emitter.instruction("cmp r11, -1");                                         // int-key sentinel?
    emitter.instruction("jne __rt_mixed_array_get_indexed_missing_string");     // missing string keys use PHP's string-key warning
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // r8 = key_lo (int index)
    emitter.instruction("mov r9, QWORD PTR [r10]");                             // r9 = array length
    emitter.instruction("cmp r8, 0");                                           // negative index → null
    emitter.instruction("jl __rt_mixed_array_get_indexed_missing");             // warn and return null for a negative indexed-array key
    emitter.instruction("cmp r8, r9");                                          // index >= length → null
    emitter.instruction("jge __rt_mixed_array_get_indexed_missing");            // warn and return null for an out-of-bounds indexed-array key
    emitter.instruction("mov r9, QWORD PTR [r10 - 8]");                         // load packed indexed-array kind metadata
    emitter.instruction("shr r9, 8");                                           // shift the runtime element value_type tag into the low bits
    emitter.instruction("and r9, 0x7f");                                        // remove the persistent COW flag from the extracted tag
    emitter.instruction("lea r10, [r10 + 24]");                                 // skip the 24-byte array header to reach the contiguous payload
    emitter.instruction("cmp r9, 7");                                           // are indexed slots already boxed Mixed pointers?
    emitter.instruction("je __rt_mixed_array_get_indexed_boxed");               // boxed slots must be retained before returning
    emitter.instruction("cmp r9, 1");                                           // do indexed slots contain string pointer/length pairs?
    emitter.instruction("je __rt_mixed_array_get_indexed_string");              // string slots need a 16-byte load before boxing
    emitter.instruction("cmp r9, 8");                                           // do indexed slots represent null payloads?
    emitter.instruction("je __rt_mixed_array_get_indexed_null");                // null slots have no payload to read
    emitter.instruction("mov rax, r9");                                         // rax = runtime value_type tag for mixed_from_value
    emitter.instruction("mov rdi, QWORD PTR [r10 + r8 * 8]");                   // rdi = scalar or pointer payload from the typed indexed slot
    emitter.instruction("xor esi, esi");                                        // typed indexed slots use one payload word except strings
    emitter.instruction("call __rt_mixed_from_value");                          // box the typed indexed-array element into a Mixed cell
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return Mixed* in rax
    emitter.label("__rt_mixed_array_get_indexed_boxed");
    emitter.instruction("mov rax, QWORD PTR [r10 + r8 * 8]");                   // load the boxed Mixed pointer from the indexed slot
    emitter.instruction("test rax, rax");                                       // empty slot → null
    emitter.instruction("je __rt_mixed_array_get_indexed_missing");             // zero-filled gaps are undefined keys, not present null values
    abi::emit_push_reg(emitter, "rax");
    emitter.instruction("call __rt_incref");                                    // retain the stored Mixed cell so the caller owns the returned result
    abi::emit_pop_reg(emitter, "rax");
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return Mixed* in rax
    emitter.label("__rt_mixed_array_get_indexed_string");
    emitter.instruction("shl r8, 4");                                           // convert the element index to a 16-byte string slot offset
    emitter.instruction("add r10, r8");                                         // r10 = address of the selected string slot
    emitter.instruction("mov rax, 1");                                          // rax = string runtime value_type tag
    emitter.instruction("mov rdi, QWORD PTR [r10]");                            // rdi = selected string pointer
    emitter.instruction("mov rsi, QWORD PTR [r10 + 8]");                        // rsi = selected string length
    emitter.instruction("call __rt_mixed_from_value");                          // box the string indexed-array element into a Mixed cell
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return Mixed* in rax
    emitter.label("__rt_mixed_array_get_indexed_null");
    emitter.instruction("mov rax, 8");                                          // rax = null runtime value_type tag
    emitter.instruction("mov rdi, 0");                                          // value_lo = 0 for null
    emitter.instruction("mov rsi, 0");                                          // value_hi = 0 for null
    emitter.instruction("call __rt_mixed_from_value");                          // box the null indexed-array element into a Mixed cell
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return Mixed* in rax
    emitter.label("__rt_mixed_array_get_indexed_missing");
    emitter.instruction("cmp QWORD PTR [rbp - 32], 0");                         // are ordinary read warnings enabled?
    emitter.instruction("je __rt_mixed_array_get_null");                        // `isset()`/`??` suppress undefined-key warnings
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the missing integer key for the PHP warning
    emitter.instruction("call __rt_warn_undefined_array_key_int");              // emit or suppress the undefined-array-key warning
    emitter.instruction("jmp __rt_mixed_array_get_null");                       // return boxed Mixed(null) after the warning
    emitter.label("__rt_mixed_array_get_indexed_missing_string");
    emitter.instruction("cmp QWORD PTR [rbp - 32], 0");                         // are ordinary read warnings enabled?
    emitter.instruction("je __rt_mixed_array_get_null");                        // `isset()`/`??` suppress undefined-key warnings
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the missing string key pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // reload the missing string key length
    emitter.instruction("call __rt_warn_undefined_array_key_str");              // emit the PHP warning for a missing string key
    emitter.instruction("jmp __rt_mixed_array_get_null");                       // return boxed Mixed(null) after the warning

    emitter.label("__rt_mixed_array_get_assoc");
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // r10 = hash pointer
    emit_branch_if_null_container(
        emitter,
        "r10",
        "r11",
        "__rt_mixed_array_get_null_container",
    );
    emitter.instruction("mov rdi, r10");                                        // rdi = hash pointer for hash_get
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // rsi = key_lo
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // rdx = key_hi
    emitter.instruction("call __rt_hash_get");                                  // rax=found, rdi=value_lo, rsi=value_hi, rcx=value_tag
    emitter.instruction("test rax, rax");                                       // miss → null
    emitter.instruction("je __rt_mixed_array_get_assoc_missing");               // diagnose an absent hash key for ordinary reads
    // For value_tag == 7 the entry is already a boxed Mixed pointer; for
    // any other tag (typed string/int/array entries from non-Mixed assoc
    // arrays passing through a Mixed receiver) re-box (lo, hi, tag) so
    // callers always see a uniform Mixed cell.
    emitter.instruction("cmp rcx, 7");                                          // is the hash entry already a boxed Mixed?
    emitter.instruction("jne __rt_mixed_array_get_assoc_box");                  // no → box (lo, hi, tag) into a fresh Mixed cell
    emitter.instruction("mov rax, rdi");                                        // yes → move the stored Mixed cell into the return register
    abi::emit_push_reg(emitter, "rax");
    emitter.instruction("call __rt_incref");                                    // retain the stored Mixed cell so the caller owns the returned result
    abi::emit_pop_reg(emitter, "rax");
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return Mixed* in rax

    emitter.label("__rt_mixed_array_get_assoc_missing");
    emitter.instruction("cmp QWORD PTR [rbp - 32], 0");                         // are ordinary read warnings enabled?
    emitter.instruction("je __rt_mixed_array_get_null");                        // `isset()`/`??` suppress undefined-key warnings
    emitter.instruction("cmp QWORD PTR [rbp - 24], -1");                        // does the missing key use the integer-key sentinel?
    emitter.instruction("je __rt_mixed_array_get_assoc_missing_int");           // integer keys use the decimal warning formatter
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the missing string key pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // reload the missing string key length
    emitter.instruction("call __rt_warn_undefined_array_key_str");              // emit the PHP warning for a missing string key
    emitter.instruction("jmp __rt_mixed_array_get_null");                       // return boxed Mixed(null) after the warning
    emitter.label("__rt_mixed_array_get_assoc_missing_int");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the missing integer key
    emitter.instruction("call __rt_warn_undefined_array_key_int");              // emit the PHP warning for a missing integer key
    emitter.instruction("jmp __rt_mixed_array_get_null");                       // return boxed Mixed(null) after the warning
    emitter.label("__rt_mixed_array_get_assoc_box");
    // mixed_from_value(tag, lo, hi). Helper expects rax=tag, rdi=lo, rsi=hi.
    emitter.instruction("mov rax, rcx");                                        // rax = value_tag
    // rdi and rsi already hold value_lo and value_hi.
    emitter.instruction("call __rt_mixed_from_value");                          // box the typed entry into a Mixed cell
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return Mixed* in rax

    emitter.label("__rt_mixed_array_get_object");
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // r10 = obj pointer
    emit_branch_if_null_container(
        emitter,
        "r10",
        "r11",
        "__rt_mixed_array_get_null_container",
    );
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // r11 = class_id
    abi::emit_load_symbol_to_reg(emitter, "r12", "_spl_fixed_array_class_id", 0);
    emitter.instruction("cmp r11, r12");                                        // is the receiver a SplFixedArray instance?
    emitter.instruction("je __rt_mixed_array_get_spl_fixed");                   // dispatch mixed object indexing to SplFixedArray::offsetGet
    abi::emit_load_symbol_to_reg(emitter, "r12", "_spl_dll_class_id", 0);
    emitter.instruction("cmp r11, r12");                                        // is the receiver a SplDoublyLinkedList instance?
    emitter.instruction("je __rt_mixed_array_get_spl_dll");                     // dispatch mixed object indexing to the list ArrayAccess helper
    abi::emit_load_symbol_to_reg(emitter, "r12", "_spl_stack_class_id", 0);
    emitter.instruction("cmp r11, r12");                                        // is the receiver a SplStack instance?
    emitter.instruction("je __rt_mixed_array_get_spl_dll");                     // SplStack shares the list ArrayAccess helper
    abi::emit_load_symbol_to_reg(emitter, "r12", "_spl_queue_class_id", 0);
    emitter.instruction("cmp r11, r12");                                        // is the receiver a SplQueue instance?
    emitter.instruction("je __rt_mixed_array_get_spl_dll");                     // SplQueue shares the list ArrayAccess helper
    emitter.instruction("jmp __rt_mixed_array_get_php_offset_get");             // every other class goes through PHP ArrayAccess or raises
    emitter.label("__rt_mixed_array_get_spl_fixed");
    emitter.instruction("mov QWORD PTR [rbp - 8], r10");                        // save unboxed SplFixedArray receiver while boxing the key
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload normalized key high word
    emitter.instruction("cmp r11, -1");                                         // does key_hi carry the integer-key sentinel?
    emitter.instruction("je __rt_mixed_array_get_spl_fixed_int_key");           // integer keys box as Mixed int
    emitter.instruction("mov rax, 1");                                          // tag = string for mixed_from_value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // key string pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // key string length
    emitter.instruction("jmp __rt_mixed_array_get_spl_fixed_box_key");          // share the offsetGet call after key boxing
    emitter.label("__rt_mixed_array_get_spl_fixed_int_key");
    emitter.instruction("mov rax, 0");                                          // tag = int for mixed_from_value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // key integer payload
    emitter.instruction("xor esi, esi");                                        // integer keys have no high payload
    emitter.label("__rt_mixed_array_get_spl_fixed_box_key");
    emitter.instruction("call __rt_mixed_from_value");                          // allocate the boxed ArrayAccess offset
    emitter.instruction("mov rsi, rax");                                        // pass boxed offset as argument 1
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass unboxed SplFixedArray receiver as argument 0
    emitter.instruction("call __rt_spl_fixed_offset_get");                      // read through SplFixedArray::offsetGet
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return Mixed* in rax
    emitter.label("__rt_mixed_array_get_spl_dll");
    emitter.instruction("mov QWORD PTR [rbp - 8], r10");                        // save unboxed SPL list receiver while boxing the key
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload normalized key high word
    emitter.instruction("cmp r11, -1");                                         // does key_hi carry the integer-key sentinel?
    emitter.instruction("je __rt_mixed_array_get_spl_dll_int_key");             // integer keys box as Mixed int
    emitter.instruction("mov rax, 1");                                          // tag = string for mixed_from_value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // key string pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // key string length
    emitter.instruction("jmp __rt_mixed_array_get_spl_dll_box_key");            // share the offsetGet call after key boxing
    emitter.label("__rt_mixed_array_get_spl_dll_int_key");
    emitter.instruction("mov rax, 0");                                          // tag = int for mixed_from_value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // key integer payload
    emitter.instruction("xor esi, esi");                                        // integer keys have no high payload
    emitter.label("__rt_mixed_array_get_spl_dll_box_key");
    emitter.instruction("call __rt_mixed_from_value");                          // allocate the boxed ArrayAccess offset
    emitter.instruction("mov rsi, rax");                                        // pass boxed offset as argument 1
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass unboxed SPL list receiver as argument 0
    emitter.instruction("call __rt_spl_dll_offset_get");                        // read through the shared SPL list offsetGet helper
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return Mixed* in rax

    // See the ARM64 path: the ids above are the runtime's own containers, everything else is a PHP
    // class whose `ArrayAccess::offsetGet` is only reachable through the dense method table, and
    // the table is read twice so that no allocation happens before the decision to commit.
    emitter.label("__rt_mixed_array_get_php_offset_get");
    emitter.instruction("test r11, r11");                                       // reject negative synthetic class ids
    emitter.instruction("js __rt_mixed_array_get_not_indexable");               // synthetic ids cannot index metadata
    emitter.instruction("cmp r11, QWORD PTR [rip + _class_iface_method_count]"); // is the id within the dense table?
    emitter.instruction("jae __rt_mixed_array_get_not_indexable");              // out-of-range ids have no entry
    emitter.instruction("lea r12, [rip + _class_offsetget_ptrs]");              // dense ArrayAccess::offsetGet table
    emitter.instruction("mov r12, QWORD PTR [r12 + r11 * 8]");                  // resolve the concrete or inherited offsetGet
    emitter.instruction("test r12, r12");                                       // 0 means the class is not ArrayAccess
    emitter.instruction("jz __rt_mixed_array_get_not_indexable");               // 0 means the class is not ArrayAccess at all
    emitter.instruction("mov QWORD PTR [rbp - 8], r10");                        // save unboxed receiver while boxing the key
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload normalized key high word
    emitter.instruction("cmp r11, -1");                                         // does key_hi carry the integer-key sentinel?
    emitter.instruction("je __rt_mixed_array_get_php_int_key");                 // integer keys box as Mixed int
    emitter.instruction("mov rax, 1");                                          // tag = string for mixed_from_value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // key string pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // key string length
    emitter.instruction("jmp __rt_mixed_array_get_php_box_key");                // share the offsetGet call after key boxing
    emitter.label("__rt_mixed_array_get_php_int_key");
    emitter.instruction("mov rax, 0");                                          // tag = int for mixed_from_value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // key integer payload
    emitter.instruction("xor esi, esi");                                        // integer keys have no high payload
    emitter.label("__rt_mixed_array_get_php_box_key");
    emitter.instruction("call __rt_mixed_from_value");                          // allocate the boxed ArrayAccess offset
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // stash the box; key_lo has served its purpose
    emitter.instruction("mov rsi, rax");                                        // pass boxed offset as argument 1
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass unboxed receiver as argument 0
    emitter.instruction("mov r11, QWORD PTR [rdi]");                            // reload class id, clobbered across the boxing call
    emitter.instruction("lea r12, [rip + _class_offsetget_ptrs]");              // dense ArrayAccess::offsetGet table
    emitter.instruction("mov r12, QWORD PTR [r12 + r11 * 8]");                  // re-resolve offsetGet for the same class
    emitter.instruction("call r12");                                            // read through PHP's ArrayAccess::offsetGet
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // stash the owned result; the receiver is done
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the boxed offset
    emitter.instruction("call __rt_decref_mixed");                              // a PHP method BORROWS its argument, so this frame frees the box
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // recover the owned result
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return Mixed* in rax

    // See the ARM64 path: an object PHP cannot index raises, in every context, so this leaves
    // through the Error rather than through the shared null tail.
    emitter.label("__rt_mixed_array_get_not_indexable");
    emitter.instruction("mov rdi, r10");                                        // pass the unboxed receiver to name its class
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer before the tail-call
    emitter.instruction("jmp __rt_throw_object_not_array");                     // never returns

    emitter.label("__rt_mixed_array_get_null_container");
    emitter.instruction("cmp QWORD PTR [rbp - 32], 0");                         // are ordinary read warnings enabled?
    emitter.instruction("je __rt_mixed_array_get_null");                        // quiet read contexts suppress the null-offset warning
    emitter.instruction("call __rt_warn_array_offset_on_null");                 // emit PHP's warning for indexing a null receiver
    emitter.label("__rt_mixed_array_get_null");
    emitter.instruction("mov rax, 8");                                          // tag = 8 (null) for mixed_from_value
    emitter.instruction("mov rdi, 0");                                          // value_lo = 0
    emitter.instruction("mov rsi, 0");                                          // value_hi = 0
    emitter.instruction("call __rt_mixed_from_value");                          // box null into a fresh Mixed cell
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return Mixed* in rax
}
