//! Purpose:
//! Emits runtime fallback helpers for PHP `is_callable()` dynamic values.
//! Handles strings, method arrays, invokable objects, Mixed boxes, and erased iterable heap pointers.
//!
//! Called from:
//! - `crate::codegen::runtime::callables::emit_is_callable_runtime()`.
//!
//! Key details:
//! - String lookup uses fixed builtin metadata plus user metadata emitted with the program.
//! - Method and `__invoke` lookup use public-method name tables indexed by runtime class id.

use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits all `is_callable()` runtime helpers for the current target.
/// Dispatches to x86_64 or ARM64 emitters based on `emitter.target.arch`.
pub(crate) fn emit_is_callable_runtime(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
        return;
    }
    emit_aarch64(emitter);
}

/// Emits all ARM64 `is_callable()` runtime helpers for the current program.
fn emit_aarch64(emitter: &mut Emitter) {
    emit_string_aarch64(emitter);
    emit_method_name_aarch64(emitter);
    emit_static_method_name_aarch64(emitter);
    emit_object_aarch64(emitter);
    emit_array_aarch64(emitter);
    emit_assoc_aarch64(emitter);
    emit_mixed_aarch64(emitter);
    emit_heap_aarch64(emitter);
}

/// Emits the ARM64 runtime helper for string callable lookup.
/// Scans builtin names (case-insensitive), user functions (exact match),
/// then checks for `Class::method` format via `__rt_is_callable_static_method_name`.
/// Input: x0=string pointer, x1=string length. Output: x0=1 (true) or 0 (false).
fn emit_string_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: is_callable_string ---");
    emitter.label_global("__rt_is_callable_string");

    emitter.instruction("sub sp, sp, #80");                                     // reserve lookup frame for string inputs and table cursor state
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save caller frame pointer and return address around string comparisons
    emitter.instruction("add x29, sp, #64");                                    // establish this helper's frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save candidate string pointer for repeated table comparisons
    emitter.instruction("str x1, [sp, #8]");                                    // save candidate string length for repeated table comparisons
    abi::emit_symbol_address(emitter, "x9", "_callable_builtin_table");
    emitter.instruction("str x9, [sp, #16]");                                   // save the builtin callable-name table pointer
    abi::emit_symbol_address(emitter, "x9", "_callable_builtin_count");
    emitter.instruction("ldr x9, [x9]");                                        // load builtin callable-name count from fixed runtime data
    emitter.instruction("str x9, [sp, #24]");                                   // save the active table count for the builtin scan
    emitter.instruction("str xzr, [sp, #32]");                                  // start the builtin scan at entry index zero

    emitter.label("__rt_is_callable_string_builtin_loop");
    emitter.instruction("ldr x9, [sp, #32]");                                   // load current builtin table index
    emitter.instruction("ldr x10, [sp, #24]");                                  // load builtin table entry count
    emitter.instruction("cmp x9, x10");                                         // have all builtin names been checked?
    emitter.instruction("b.ge __rt_is_callable_string_user_setup");             // continue with user functions after the builtin table
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload builtin table base pointer
    emitter.instruction("lsl x12, x9, #4");                                     // multiply index by 16-byte builtin table entry size
    emitter.instruction("add x11, x11, x12");                                   // compute address of the current builtin table entry
    emitter.instruction("ldr x1, [sp, #0]");                                    // pass candidate string pointer as comparison left side
    emitter.instruction("ldr x2, [sp, #8]");                                    // pass candidate string length as comparison left side
    emitter.instruction("ldr x3, [x11]");                                       // pass builtin name pointer as comparison right side
    emitter.instruction("ldr x4, [x11, #8]");                                   // pass builtin name length as comparison right side
    abi::emit_call_label(emitter, "__rt_strcasecmp");                           // compare builtin names case-insensitively like PHP internals
    emitter.instruction("cbz x0, __rt_is_callable_string_true");                // a zero comparison result means the string is a builtin callable
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload current builtin table index after comparison
    emitter.instruction("add x9, x9, #1");                                      // advance to the next builtin table entry
    emitter.instruction("str x9, [sp, #32]");                                   // persist the incremented builtin table index
    emitter.instruction("b __rt_is_callable_string_builtin_loop");              // continue scanning builtin callable names

    emitter.label("__rt_is_callable_string_user_setup");
    abi::emit_symbol_address(emitter, "x9", "_callable_user_function_table");
    emitter.instruction("str x9, [sp, #16]");                                   // switch the saved table pointer to user function metadata
    abi::emit_symbol_address(emitter, "x9", "_callable_user_function_count");
    emitter.instruction("ldr x9, [x9]");                                        // load user function table entry count
    emitter.instruction("str x9, [sp, #24]");                                   // save user function count for the exact-name scan
    emitter.instruction("str xzr, [sp, #32]");                                  // restart table scanning at user entry index zero

    emitter.label("__rt_is_callable_string_user_loop");
    emitter.instruction("ldr x9, [sp, #32]");                                   // load current user function table index
    emitter.instruction("ldr x10, [sp, #24]");                                  // load user function table entry count
    emitter.instruction("cmp x9, x10");                                         // have all user function names been checked?
    emitter.instruction("b.ge __rt_is_callable_string_static_setup");           // continue with Class::method strings after user functions
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload user function table base pointer
    emitter.instruction("lsl x12, x9, #4");                                     // compute index * 16 as the first part of index * 24
    emitter.instruction("add x12, x12, x9, lsl #3");                            // complete index * 24 for user table entries
    emitter.instruction("add x11, x11, x12");                                   // compute address of the current user function entry
    emitter.instruction("ldr x1, [sp, #0]");                                    // pass candidate string pointer as comparison left side
    emitter.instruction("ldr x2, [sp, #8]");                                    // pass candidate string length as comparison left side
    emitter.instruction("ldr x3, [x11]");                                       // pass user function name pointer as comparison right side
    emitter.instruction("ldr x4, [x11, #8]");                                   // pass user function name length as comparison right side
    emitter.instruction("str x11, [sp, #40]");                                  // preserve current user table entry across the string comparison call
    abi::emit_call_label(emitter, "__rt_str_eq");                               // compare user function names exactly, matching current AOT lookup behavior
    emitter.instruction("cbz x0, __rt_is_callable_string_user_next");           // skip inactive checks when this user name did not match
    emitter.instruction("ldr x11, [sp, #40]");                                  // reload current user table entry after caller-saved registers were clobbered
    emitter.instruction("ldr x12, [x11, #16]");                                 // load optional active-variant symbol pointer for include-loaded functions
    emitter.instruction("cbz x12, __rt_is_callable_string_true");               // ordinary user functions are callable as soon as their name matches
    emitter.instruction("ldr x12, [x12]");                                      // read the active implementation pointer for this function variant group
    emitter.instruction("cbnz x12, __rt_is_callable_string_true");              // include-loaded variants are callable only after an implementation is active

    emitter.label("__rt_is_callable_string_user_next");
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload current user function table index
    emitter.instruction("add x9, x9, #1");                                      // advance to the next user function entry
    emitter.instruction("str x9, [sp, #32]");                                   // persist the incremented user function table index
    emitter.instruction("b __rt_is_callable_string_user_loop");                 // continue scanning user function names

    emitter.label("__rt_is_callable_string_static_setup");
    emitter.instruction("ldr x9, [sp, #8]");                                    // load candidate string length before scanning for the static-method separator
    emitter.instruction("cmp x9, #3");                                          // Class::method needs at least one byte on both sides of ::
    emitter.instruction("b.lt __rt_is_callable_string_false");                  // too-short strings cannot name a static method callable
    emitter.instruction("str xzr, [sp, #48]");                                  // start separator scan at byte index zero

    emitter.label("__rt_is_callable_string_static_scan");
    emitter.instruction("ldr x9, [sp, #48]");                                   // load current byte index while searching for ::
    emitter.instruction("ldr x10, [sp, #8]");                                   // load candidate string length
    emitter.instruction("sub x10, x10, #1");                                    // stop before the final byte so [index + 1] is in bounds
    emitter.instruction("cmp x9, x10");                                         // have all possible separator positions been checked?
    emitter.instruction("b.ge __rt_is_callable_string_false");                  // no :: separator means this is not a static-method string
    emitter.instruction("ldr x11, [sp, #0]");                                   // reload candidate string pointer
    emitter.instruction("add x11, x11, x9");                                    // point at the current candidate separator byte
    emitter.instruction("ldrb w12, [x11]");                                     // read the current byte
    emitter.instruction("cmp w12, #58");                                        // is the current byte ':'?
    emitter.instruction("b.ne __rt_is_callable_string_static_next");            // keep scanning until the first ':' byte
    emitter.instruction("ldrb w12, [x11, #1]");                                 // read the following byte
    emitter.instruction("cmp w12, #58");                                        // is the following byte ':' too?
    emitter.instruction("b.eq __rt_is_callable_string_static_found");           // split the string at the :: separator

    emitter.label("__rt_is_callable_string_static_next");
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload separator scan index
    emitter.instruction("add x9, x9, #1");                                      // advance to the next candidate separator byte
    emitter.instruction("str x9, [sp, #48]");                                   // persist the incremented separator scan index
    emitter.instruction("b __rt_is_callable_string_static_scan");               // continue scanning for Class::method separator

    emitter.label("__rt_is_callable_string_static_found");
    emitter.instruction("ldr x9, [sp, #48]");                                   // load separator index, which is also the class-name length
    emitter.instruction("cbz x9, __rt_is_callable_string_false");               // empty class name before :: is not callable
    emitter.instruction("add x10, x9, #2");                                     // compute the method-name start index after ::
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload total candidate string length
    emitter.instruction("cmp x10, x11");                                        // is there at least one method byte after ::?
    emitter.instruction("b.ge __rt_is_callable_string_false");                  // empty method name after :: is not callable
    emitter.instruction("ldr x0, [sp, #0]");                                    // pass class-name pointer to static-method lookup
    emitter.instruction("mov x1, x9");                                          // pass class-name length to static-method lookup
    emitter.instruction("add x2, x0, x10");                                     // pass method-name pointer after the :: separator
    emitter.instruction("sub x3, x11, x10");                                    // pass method-name length after the :: separator
    abi::emit_call_label(emitter, "__rt_is_callable_static_method_name");       // test Class::method strings against public static method metadata
    emitter.instruction("b __rt_is_callable_string_done");                      // keep static-method lookup result and restore this helper's frame

    emitter.label("__rt_is_callable_string_true");
    emitter.instruction("mov x0, #1");                                          // return true for recognized callable strings
    emitter.instruction("b __rt_is_callable_string_done");                      // restore this helper's frame before returning

    emitter.label("__rt_is_callable_string_false");
    emitter.instruction("mov x0, #0");                                          // return false when no callable string target exists

    emitter.label("__rt_is_callable_string_done");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore caller frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release lookup frame
    emitter.instruction("ret");                                                 // return boolean result in x0
}

/// Emits the ARM64 runtime helper for object method callable lookup.
/// Looks up a named public method on an object via the class's public-method name table.
/// Input: x0=object pointer, x1=method name pointer, x2=method name length. Output: x0=1 (true) or 0 (false).
fn emit_method_name_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: is_callable_method_name ---");
    emitter.label_global("__rt_is_callable_method_name");

    emitter.instruction("sub sp, sp, #64");                                     // reserve frame for receiver, method string, and table cursor
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save caller frame pointer and return address around comparisons
    emitter.instruction("add x29, sp, #48");                                    // establish this helper's frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save receiver object pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save candidate method string pointer
    emitter.instruction("str x2, [sp, #16]");                                   // save candidate method string length
    emitter.instruction("cbz x0, __rt_is_callable_method_false");               // null receivers are not callable method arrays
    emitter.instruction("ldr x9, [x0]");                                        // load receiver runtime class id from object header
    abi::emit_symbol_address(emitter, "x10", "_class_callable_method_ptrs");
    emitter.instruction("ldr x10, [x10, x9, lsl #3]");                          // load class-specific public-method name table pointer
    emitter.instruction("str x10, [sp, #24]");                                  // save method table pointer across string comparisons
    emitter.instruction("ldr x11, [x10]");                                      // load public-method name count from the class table
    emitter.instruction("str x11, [sp, #32]");                                  // save method table count
    emitter.instruction("str xzr, [sp, #40]");                                  // start method table scan at entry index zero

    emitter.label("__rt_is_callable_method_loop");
    emitter.instruction("ldr x9, [sp, #40]");                                   // load current method table index
    emitter.instruction("ldr x10, [sp, #32]");                                  // load method table entry count
    emitter.instruction("cmp x9, x10");                                         // have all public methods been checked?
    emitter.instruction("b.ge __rt_is_callable_method_false");                  // no method name matched
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload method table pointer
    emitter.instruction("add x11, x11, #8");                                    // skip the count word before indexed entries
    emitter.instruction("lsl x12, x9, #4");                                     // multiply method index by 16-byte entry size
    emitter.instruction("add x11, x11, x12");                                   // compute current method entry address
    emitter.instruction("ldr x1, [sp, #8]");                                    // pass candidate method pointer as comparison left side
    emitter.instruction("ldr x2, [sp, #16]");                                   // pass candidate method length as comparison left side
    emitter.instruction("ldr x3, [x11]");                                       // pass table method pointer as comparison right side
    emitter.instruction("ldr x4, [x11, #8]");                                   // pass table method length as comparison right side
    abi::emit_call_label(emitter, "__rt_strcasecmp");                           // compare method names case-insensitively like PHP method lookup
    emitter.instruction("cbz x0, __rt_is_callable_method_true");                // equal method names mean the object/method pair is callable
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload current method table index
    emitter.instruction("add x9, x9, #1");                                      // advance to the next public method
    emitter.instruction("str x9, [sp, #40]");                                   // persist the incremented method table index
    emitter.instruction("b __rt_is_callable_method_loop");                      // continue scanning public methods

    emitter.label("__rt_is_callable_method_true");
    emitter.instruction("mov x0, #1");                                          // return true for public method matches
    emitter.instruction("b __rt_is_callable_method_done");                      // restore frame before returning true

    emitter.label("__rt_is_callable_method_false");
    emitter.instruction("mov x0, #0");                                          // return false when no public method matches

    emitter.label("__rt_is_callable_method_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore caller frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release method lookup frame
    emitter.instruction("ret");                                                 // return boolean result in x0
}

/// Emits the ARM64 runtime helper for static method callable lookup.
/// Compares the given class name and method name against the global static method table
/// (both compared case-insensitively). Handles leading backslash normalization.
/// Input: x0=class name ptr, x1=class name len, x2=method name ptr, x3=method name len. Output: x0=1 or 0.
fn emit_static_method_name_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: is_callable_static_method_name ---");
    emitter.label_global("__rt_is_callable_static_method_name");

    emitter.instruction("sub sp, sp, #80");                                     // reserve frame for class string, method string, and table cursor
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save caller frame pointer and return address around comparisons
    emitter.instruction("add x29, sp, #64");                                    // establish this helper's frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save candidate class string pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save candidate class string length
    emitter.instruction("str x2, [sp, #16]");                                   // save candidate static method string pointer
    emitter.instruction("str x3, [sp, #24]");                                   // save candidate static method string length
    emitter.instruction("cbz x0, __rt_is_callable_static_method_false");        // null class-name pointers cannot name static callables
    emitter.instruction("cbz x1, __rt_is_callable_static_method_false");        // empty class names cannot name static callables
    emitter.instruction("cbz x2, __rt_is_callable_static_method_false");        // null method-name pointers cannot name static callables
    emitter.instruction("cbz x3, __rt_is_callable_static_method_false");        // empty method names cannot name static callables
    emitter.instruction("ldrb w9, [x0]");                                       // read the first class-name byte for optional namespace separator
    emitter.instruction("cmp w9, #92");                                         // does the class name start with a leading backslash?
    emitter.instruction("b.ne __rt_is_callable_static_method_setup");           // class names without leading slash can be compared as-is
    emitter.instruction("cmp x1, #1");                                          // a single leading slash leaves an empty class name
    emitter.instruction("b.le __rt_is_callable_static_method_false");           // reject empty class names after removing the leading slash
    emitter.instruction("add x0, x0, #1");                                      // skip the leading namespace separator for table comparison
    emitter.instruction("sub x1, x1, #1");                                      // shorten the class-name length after skipping the slash
    emitter.instruction("str x0, [sp, #0]");                                    // save normalized class-name pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save normalized class-name length

    emitter.label("__rt_is_callable_static_method_setup");
    abi::emit_symbol_address(emitter, "x9", "_class_callable_static_method_table");
    emitter.instruction("str x9, [sp, #32]");                                   // save static callable table pointer across comparisons
    abi::emit_symbol_address(emitter, "x9", "_class_callable_static_method_count");
    emitter.instruction("ldr x9, [x9]");                                        // load number of public static method callable entries
    emitter.instruction("str x9, [sp, #40]");                                   // save static callable entry count
    emitter.instruction("str xzr, [sp, #48]");                                  // start static callable scan at entry index zero

    emitter.label("__rt_is_callable_static_method_loop");
    emitter.instruction("ldr x9, [sp, #48]");                                   // load current static callable table index
    emitter.instruction("ldr x10, [sp, #40]");                                  // load static callable table entry count
    emitter.instruction("cmp x9, x10");                                         // have all static method entries been checked?
    emitter.instruction("b.ge __rt_is_callable_static_method_false");           // no public static method matched the class/method pair
    emitter.instruction("ldr x11, [sp, #32]");                                  // reload static callable table pointer
    emitter.instruction("lsl x12, x9, #5");                                     // multiply index by 32-byte static callable entry size
    emitter.instruction("add x11, x11, x12");                                   // compute address of the current static callable entry
    emitter.instruction("str x11, [sp, #56]");                                  // preserve current table entry across string comparison calls
    emitter.instruction("ldr x1, [sp, #0]");                                    // pass candidate class pointer as comparison left side
    emitter.instruction("ldr x2, [sp, #8]");                                    // pass candidate class length as comparison left side
    emitter.instruction("ldr x3, [x11]");                                       // pass table class pointer as comparison right side
    emitter.instruction("ldr x4, [x11, #8]");                                   // pass table class length as comparison right side
    abi::emit_call_label(emitter, "__rt_strcasecmp");                           // compare class names case-insensitively like PHP class lookup
    emitter.instruction("cbnz x0, __rt_is_callable_static_method_next");        // move on when the class name did not match
    emitter.instruction("ldr x11, [sp, #56]");                                  // reload current table entry after caller-saved registers were clobbered
    emitter.instruction("ldr x1, [sp, #16]");                                   // pass candidate method pointer as comparison left side
    emitter.instruction("ldr x2, [sp, #24]");                                   // pass candidate method length as comparison left side
    emitter.instruction("ldr x3, [x11, #16]");                                  // pass table method pointer as comparison right side
    emitter.instruction("ldr x4, [x11, #24]");                                  // pass table method length as comparison right side
    abi::emit_call_label(emitter, "__rt_strcasecmp");                           // compare static method names case-insensitively like PHP method lookup
    emitter.instruction("cbz x0, __rt_is_callable_static_method_true");         // matching class and method strings name a public static callable

    emitter.label("__rt_is_callable_static_method_next");
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload current static callable table index
    emitter.instruction("add x9, x9, #1");                                      // advance to the next static callable entry
    emitter.instruction("str x9, [sp, #48]");                                   // persist the incremented static callable index
    emitter.instruction("b __rt_is_callable_static_method_loop");               // continue scanning public static methods

    emitter.label("__rt_is_callable_static_method_true");
    emitter.instruction("mov x0, #1");                                          // return true for public static method matches
    emitter.instruction("b __rt_is_callable_static_method_done");               // restore frame before returning true

    emitter.label("__rt_is_callable_static_method_false");
    emitter.instruction("mov x0, #0");                                          // return false when no public static method matches

    emitter.label("__rt_is_callable_static_method_done");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore caller frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release static callable lookup frame
    emitter.instruction("ret");                                                 // return boolean result in x0
}

/// Emits the ARM64 runtime helper for invokable object lookup.
/// Delegates to `__rt_is_callable_method_name` with the `"__invoke"` method name.
/// Input: x0=object pointer. Output: x0=1 (true) or 0 (false).
fn emit_object_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: is_callable_object ---");
    emitter.label_global("__rt_is_callable_object");

    emitter.instruction("sub sp, sp, #32");                                     // reserve frame while delegating to method-name lookup
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save caller frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish this helper's frame pointer
    abi::emit_symbol_address(emitter, "x1", "_callable_invoke_name");
    emitter.instruction("mov x2, #8");                                          // method string length for "__invoke"
    abi::emit_call_label(emitter, "__rt_is_callable_method_name");              // test whether the object exposes public __invoke
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore caller frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release delegation frame
    emitter.instruction("ret");                                                 // return the method lookup result
}

/// Emits the ARM64 runtime helper for indexed-array callable lookup.
/// Validates exactly 2 slots; dispatches Mixed slots (receiver unboxed to object or string)
/// and raw string slots (treated as [class-name, static-method] pair).
/// Input: x0=indexed array pointer. Output: x0=1 (true) or 0 (false).
fn emit_array_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: is_callable_array ---");
    emitter.label_global("__rt_is_callable_array");

    emitter.instruction("sub sp, sp, #64");                                     // reserve frame for array pointer and extracted callable pieces
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save caller frame pointer and return address around mixed unboxing
    emitter.instruction("add x29, sp, #48");                                    // establish this helper's frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save indexed array pointer
    emitter.instruction("cbz x0, __rt_is_callable_array_false");                // null arrays are not callable
    emitter.instruction("ldr x9, [x0]");                                        // load indexed array length
    emitter.instruction("cmp x9, #2");                                          // callable arrays must have receiver and method entries
    emitter.instruction("b.ne __rt_is_callable_array_false");                   // reject arrays without exactly two entries
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load packed heap kind and value-type metadata
    emitter.instruction("lsr x9, x9, #8");                                      // move array value-type tag into low bits
    emitter.instruction("and x9, x9, #0x7f");                                   // isolate array value-type tag without ownership flags
    emitter.instruction("cmp x9, #7");                                          // are indexed slots boxed Mixed pointers?
    emitter.instruction("b.eq __rt_is_callable_array_mixed_slots");             // Mixed arrays may hold object/string method callables
    emitter.instruction("cmp x9, #1");                                          // are indexed slots raw string pointer/length pairs?
    emitter.instruction("b.eq __rt_is_callable_array_string_slots");            // string arrays may hold [class-name, static-method] callables
    emitter.instruction("b __rt_is_callable_array_false");                      // other homogeneous arrays cannot represent callable arrays

    emitter.label("__rt_is_callable_array_string_slots");
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload indexed array pointer for raw string slot reads
    emitter.instruction("ldr x0, [x9, #24]");                                   // pass class-name string pointer from slot 0
    emitter.instruction("ldr x1, [x9, #32]");                                   // pass class-name string length from slot 0
    emitter.instruction("ldr x2, [x9, #40]");                                   // pass static method string pointer from slot 1
    emitter.instruction("ldr x3, [x9, #48]");                                   // pass static method string length from slot 1
    abi::emit_call_label(emitter, "__rt_is_callable_static_method_name");       // check whether raw string slots name a public static method
    emitter.instruction("b __rt_is_callable_array_done");                       // keep static method lookup result and restore frame

    emitter.label("__rt_is_callable_array_mixed_slots");
    emitter.instruction("ldr x0, [x0, #24]");                                   // load boxed Mixed receiver slot
    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // unwrap receiver slot to runtime tag and payload
    emitter.instruction("cmp x0, #6");                                          // is the receiver slot an object?
    emitter.instruction("b.eq __rt_is_callable_array_receiver_object");         // object receivers use public instance method lookup
    emitter.instruction("cmp x0, #1");                                          // is the receiver slot a class-name string?
    emitter.instruction("b.eq __rt_is_callable_array_receiver_string");         // string receivers use public static method lookup
    emitter.instruction("b __rt_is_callable_array_false");                      // reject receiver slots that are neither object nor string

    emitter.label("__rt_is_callable_array_receiver_object");
    emitter.instruction("str x1, [sp, #8]");                                    // save receiver object pointer
    emitter.instruction("mov x9, #6");                                          // mark saved receiver as an object
    emitter.instruction("str x9, [sp, #24]");                                   // save receiver kind for dispatch after method extraction
    emitter.instruction("b __rt_is_callable_array_method");                     // continue by extracting method-name slot

    emitter.label("__rt_is_callable_array_receiver_string");
    emitter.instruction("str x1, [sp, #8]");                                    // save receiver class-name string pointer
    emitter.instruction("str x2, [sp, #16]");                                   // save receiver class-name string length
    emitter.instruction("mov x9, #1");                                          // mark saved receiver as a class-name string
    emitter.instruction("str x9, [sp, #24]");                                   // save receiver kind for dispatch after method extraction

    emitter.label("__rt_is_callable_array_method");
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload indexed array pointer
    emitter.instruction("ldr x0, [x9, #32]");                                   // load boxed Mixed method-name slot
    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // unwrap method-name slot to runtime tag and payload
    emitter.instruction("cmp x0, #1");                                          // method slot must contain a string
    emitter.instruction("b.ne __rt_is_callable_array_false");                   // reject non-string method slots
    emitter.instruction("ldr x9, [sp, #24]");                                   // load saved receiver kind
    emitter.instruction("cmp x9, #6");                                          // should this be dispatched as an object method?
    emitter.instruction("b.eq __rt_is_callable_array_dispatch_object");         // object receiver uses instance method metadata
    emitter.instruction("mov x3, x2");                                          // move method string length into static lookup argument 3
    emitter.instruction("mov x2, x1");                                          // move method string pointer into static lookup argument 2
    emitter.instruction("ldr x0, [sp, #8]");                                    // pass saved class-name string pointer
    emitter.instruction("ldr x1, [sp, #16]");                                   // pass saved class-name string length
    abi::emit_call_label(emitter, "__rt_is_callable_static_method_name");       // check whether [class-string, method] names a public static method
    emitter.instruction("b __rt_is_callable_array_done");                       // keep static method lookup result and restore frame

    emitter.label("__rt_is_callable_array_dispatch_object");
    emitter.instruction("ldr x0, [sp, #8]");                                    // pass saved receiver object to method-name lookup
    abi::emit_call_label(emitter, "__rt_is_callable_method_name");              // x1/x2 already hold method pointer and length from mixed unbox
    emitter.instruction("b __rt_is_callable_array_done");                       // keep method lookup result and restore frame

    emitter.label("__rt_is_callable_array_false");
    emitter.instruction("mov x0, #0");                                          // return false for malformed callable arrays

    emitter.label("__rt_is_callable_array_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore caller frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release array inspection frame
    emitter.instruction("ret");                                                 // return boolean result in x0
}

/// Emits the ARM64 runtime helper for associative-array callable lookup.
/// Extracts the receiver (key 0, unboxed if Mixed) and method name (key 1, unboxed if Mixed),
/// then dispatches to instance method or static method lookup based on receiver type.
/// Input: x0=associative array pointer. Output: x0=1 (true) or 0 (false).
fn emit_assoc_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: is_callable_assoc ---");
    emitter.label_global("__rt_is_callable_assoc");

    emitter.instruction("sub sp, sp, #80");                                     // reserve frame for hash pointer and extracted callable pieces
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save caller frame pointer and return address around hash lookups
    emitter.instruction("add x29, sp, #64");                                    // establish this helper's frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save associative array pointer
    emitter.instruction("cbz x0, __rt_is_callable_assoc_false");                // null hashes are not callable
    emitter.instruction("mov x1, #0");                                          // lookup key 0 for callable receiver
    emitter.instruction("mov x2, #-1");                                         // key_hi = -1 marks integer hash key
    abi::emit_call_label(emitter, "__rt_hash_get");                             // fetch entry 0 from the hash table
    emitter.instruction("cbz x0, __rt_is_callable_assoc_false");                // missing receiver entry rejects the callable array
    emitter.instruction("cmp x3, #7");                                          // is receiver entry a boxed Mixed cell?
    emitter.instruction("b.eq __rt_is_callable_assoc_first_mixed");             // unwrap boxed receiver entries before checking type
    emitter.instruction("cmp x3, #6");                                          // is the direct receiver entry an object?
    emitter.instruction("b.eq __rt_is_callable_assoc_first_object");            // object receivers use public instance method lookup
    emitter.instruction("cmp x3, #1");                                          // is the direct receiver entry a class-name string?
    emitter.instruction("b.eq __rt_is_callable_assoc_first_string");            // string receivers use public static method lookup
    emitter.instruction("b __rt_is_callable_assoc_false");                      // reject direct receiver entries that are neither object nor string

    emitter.label("__rt_is_callable_assoc_first_object");
    emitter.instruction("str x1, [sp, #8]");                                    // save direct receiver object pointer
    emitter.instruction("mov x9, #6");                                          // mark saved receiver as an object
    emitter.instruction("str x9, [sp, #24]");                                   // save receiver kind for dispatch after method extraction
    emitter.instruction("b __rt_is_callable_assoc_second");                     // continue with method-name entry lookup

    emitter.label("__rt_is_callable_assoc_first_string");
    emitter.instruction("str x1, [sp, #8]");                                    // save direct receiver class-name string pointer
    emitter.instruction("str x2, [sp, #16]");                                   // save direct receiver class-name string length
    emitter.instruction("mov x9, #1");                                          // mark saved receiver as a class-name string
    emitter.instruction("str x9, [sp, #24]");                                   // save receiver kind for dispatch after method extraction
    emitter.instruction("b __rt_is_callable_assoc_second");                     // continue with method-name entry lookup

    emitter.label("__rt_is_callable_assoc_first_mixed");
    emitter.instruction("mov x0, x1");                                          // pass boxed receiver Mixed pointer to unbox helper
    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // unwrap receiver entry to runtime tag and payload
    emitter.instruction("cmp x0, #6");                                          // is the boxed receiver an object?
    emitter.instruction("b.eq __rt_is_callable_assoc_first_mixed_object");      // object receivers use public instance method lookup
    emitter.instruction("cmp x0, #1");                                          // is the boxed receiver a class-name string?
    emitter.instruction("b.eq __rt_is_callable_assoc_first_mixed_string");      // string receivers use public static method lookup
    emitter.instruction("b __rt_is_callable_assoc_false");                      // reject boxed receiver entries that are neither object nor string

    emitter.label("__rt_is_callable_assoc_first_mixed_object");
    emitter.instruction("str x1, [sp, #8]");                                    // save unboxed receiver object pointer
    emitter.instruction("mov x9, #6");                                          // mark saved receiver as an object
    emitter.instruction("str x9, [sp, #24]");                                   // save receiver kind for dispatch after method extraction
    emitter.instruction("b __rt_is_callable_assoc_second");                     // continue with method-name entry lookup

    emitter.label("__rt_is_callable_assoc_first_mixed_string");
    emitter.instruction("str x1, [sp, #8]");                                    // save unboxed receiver class-name string pointer
    emitter.instruction("str x2, [sp, #16]");                                   // save unboxed receiver class-name string length
    emitter.instruction("mov x9, #1");                                          // mark saved receiver as a class-name string
    emitter.instruction("str x9, [sp, #24]");                                   // save receiver kind for dispatch after method extraction

    emitter.label("__rt_is_callable_assoc_second");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload associative array pointer for the method-name lookup
    emitter.instruction("mov x1, #1");                                          // lookup key 1 for callable method name
    emitter.instruction("mov x2, #-1");                                         // key_hi = -1 marks integer hash key
    abi::emit_call_label(emitter, "__rt_hash_get");                             // fetch entry 1 from the hash table
    emitter.instruction("cbz x0, __rt_is_callable_assoc_false");                // missing method entry rejects the callable array
    emitter.instruction("cmp x3, #7");                                          // is method entry a boxed Mixed cell?
    emitter.instruction("b.eq __rt_is_callable_assoc_second_mixed");            // unwrap boxed method entries before checking type
    emitter.instruction("cmp x3, #1");                                          // direct method entry must contain a string
    emitter.instruction("b.ne __rt_is_callable_assoc_false");                   // reject direct non-string method entries
    emitter.instruction("b __rt_is_callable_assoc_dispatch");                   // dispatch direct method string against the saved receiver kind

    emitter.label("__rt_is_callable_assoc_second_mixed");
    emitter.instruction("mov x0, x1");                                          // pass boxed method Mixed pointer to unbox helper
    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // unwrap method entry to runtime tag and payload
    emitter.instruction("cmp x0, #1");                                          // boxed method entry must contain a string
    emitter.instruction("b.ne __rt_is_callable_assoc_false");                   // reject boxed non-string method entries

    emitter.label("__rt_is_callable_assoc_dispatch");
    emitter.instruction("ldr x9, [sp, #24]");                                   // load saved receiver kind
    emitter.instruction("cmp x9, #6");                                          // should this be dispatched as an object method?
    emitter.instruction("b.eq __rt_is_callable_assoc_dispatch_object");         // object receiver uses instance method metadata
    emitter.instruction("mov x3, x2");                                          // move method string length into static lookup argument 3
    emitter.instruction("mov x2, x1");                                          // move method string pointer into static lookup argument 2
    emitter.instruction("ldr x0, [sp, #8]");                                    // pass saved class-name string pointer
    emitter.instruction("ldr x1, [sp, #16]");                                   // pass saved class-name string length
    abi::emit_call_label(emitter, "__rt_is_callable_static_method_name");       // check whether [class-string, method] names a public static method
    emitter.instruction("b __rt_is_callable_assoc_done");                       // keep static method lookup result and restore frame

    emitter.label("__rt_is_callable_assoc_dispatch_object");
    emitter.instruction("ldr x0, [sp, #8]");                                    // pass saved receiver object to method-name lookup
    abi::emit_call_label(emitter, "__rt_is_callable_method_name");              // x1/x2 already hold method pointer and length from hash or mixed lookup
    emitter.instruction("b __rt_is_callable_assoc_done");                       // keep method lookup result and restore frame

    emitter.label("__rt_is_callable_assoc_false");
    emitter.instruction("mov x0, #0");                                          // return false for malformed associative callable arrays

    emitter.label("__rt_is_callable_assoc_done");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore caller frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release hash inspection frame
    emitter.instruction("ret");                                                 // return boolean result in x0
}

/// Emits the ARM64 runtime helper for boxed Mixed callable dispatch.
/// Unboxes the Mixed payload and dispatches by runtime tag:
/// string → `__rt_is_callable_string`, array → `__rt_is_callable_array`,
/// assoc → `__rt_is_callable_assoc`, object → `__rt_is_callable_object`.
/// Input: x0=Mixed pointer. Output: x0=1 (true) or 0 (false).
fn emit_mixed_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: is_callable_mixed ---");
    emitter.label_global("__rt_is_callable_mixed");

    emitter.instruction("sub sp, sp, #32");                                     // reserve frame while dispatching by mixed runtime tag
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save caller frame pointer and return address around nested helpers
    emitter.instruction("add x29, sp, #16");                                    // establish this helper's frame pointer
    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // unwrap nested Mixed cells into a concrete tag and payload
    emitter.instruction("cmp x0, #1");                                          // is the mixed payload a string?
    emitter.instruction("b.eq __rt_is_callable_mixed_string");                  // strings use runtime function-name lookup
    emitter.instruction("cmp x0, #4");                                          // is the mixed payload an indexed array?
    emitter.instruction("b.eq __rt_is_callable_mixed_array");                   // indexed arrays may be [object, method] callables
    emitter.instruction("cmp x0, #5");                                          // is the mixed payload an associative array?
    emitter.instruction("b.eq __rt_is_callable_mixed_assoc");                   // associative arrays may carry numeric 0/1 callable keys
    emitter.instruction("cmp x0, #6");                                          // is the mixed payload an object?
    emitter.instruction("b.eq __rt_is_callable_mixed_object");                  // objects may be invokable through public __invoke
    emitter.instruction("mov x0, #0");                                          // unsupported mixed payloads are not callable
    emitter.instruction("b __rt_is_callable_mixed_done");                       // restore frame before returning false

    emitter.label("__rt_is_callable_mixed_string");
    emitter.instruction("mov x0, x1");                                          // pass unboxed string pointer to string callable lookup
    emitter.instruction("mov x1, x2");                                          // pass unboxed string length to string callable lookup
    abi::emit_call_label(emitter, "__rt_is_callable_string");                   // resolve dynamic function-name strings at runtime
    emitter.instruction("b __rt_is_callable_mixed_done");                       // restore frame after string lookup

    emitter.label("__rt_is_callable_mixed_array");
    emitter.instruction("mov x0, x1");                                          // pass unboxed indexed array pointer to array callable lookup
    abi::emit_call_label(emitter, "__rt_is_callable_array");                    // inspect indexed array callable shape
    emitter.instruction("b __rt_is_callable_mixed_done");                       // restore frame after array lookup

    emitter.label("__rt_is_callable_mixed_assoc");
    emitter.instruction("mov x0, x1");                                          // pass unboxed associative array pointer to hash callable lookup
    abi::emit_call_label(emitter, "__rt_is_callable_assoc");                    // inspect associative array callable shape
    emitter.instruction("b __rt_is_callable_mixed_done");                       // restore frame after hash lookup

    emitter.label("__rt_is_callable_mixed_object");
    emitter.instruction("mov x0, x1");                                          // pass unboxed object pointer to invokable-object lookup
    abi::emit_call_label(emitter, "__rt_is_callable_object");                   // test for public __invoke

    emitter.label("__rt_is_callable_mixed_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore caller frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release mixed dispatch frame
    emitter.instruction("ret");                                                 // return boolean result in x0
}

/// Emits the ARM64 runtime helper for erased heap-kind callable dispatch.
/// Probes the heap kind via `__rt_heap_kind` and delegates to the appropriate
/// array/object/mixed helper. Used for FFI-safe opaque heap pointers.
/// Input: x0=heap pointer. Output: x0=1 (true) or 0 (false).
fn emit_heap_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: is_callable_heap ---");
    emitter.label_global("__rt_is_callable_heap");

    emitter.instruction("sub sp, sp, #32");                                     // reserve frame while probing heap kind
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save caller frame pointer and return address around nested helpers
    emitter.instruction("add x29, sp, #16");                                    // establish this helper's frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save original heap pointer before heap_kind rewrites x0
    abi::emit_call_label(emitter, "__rt_heap_kind");                            // identify indexed array, hash, object, or mixed heap payloads
    emitter.instruction("cmp x0, #2");                                          // heap kind 2 = indexed array
    emitter.instruction("b.eq __rt_is_callable_heap_array");                    // indexed arrays may be method callables
    emitter.instruction("cmp x0, #3");                                          // heap kind 3 = associative array
    emitter.instruction("b.eq __rt_is_callable_heap_assoc");                    // associative arrays may be method callables
    emitter.instruction("cmp x0, #4");                                          // heap kind 4 = object
    emitter.instruction("b.eq __rt_is_callable_heap_object");                   // objects may be invokable
    emitter.instruction("cmp x0, #5");                                          // heap kind 5 = boxed Mixed cell
    emitter.instruction("b.eq __rt_is_callable_heap_mixed");                    // Mixed cells dispatch by runtime value tag
    emitter.instruction("mov x0, #0");                                          // other heap payloads are not callable
    emitter.instruction("b __rt_is_callable_heap_done");                        // restore frame before returning false

    emitter.label("__rt_is_callable_heap_array");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload original indexed array pointer
    abi::emit_call_label(emitter, "__rt_is_callable_array");                    // inspect indexed array callable shape
    emitter.instruction("b __rt_is_callable_heap_done");                        // restore frame after indexed array lookup

    emitter.label("__rt_is_callable_heap_assoc");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload original associative array pointer
    abi::emit_call_label(emitter, "__rt_is_callable_assoc");                    // inspect associative array callable shape
    emitter.instruction("b __rt_is_callable_heap_done");                        // restore frame after associative lookup

    emitter.label("__rt_is_callable_heap_object");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload original object pointer
    abi::emit_call_label(emitter, "__rt_is_callable_object");                   // test for public __invoke
    emitter.instruction("b __rt_is_callable_heap_done");                        // restore frame after object lookup

    emitter.label("__rt_is_callable_heap_mixed");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload original mixed cell pointer
    abi::emit_call_label(emitter, "__rt_is_callable_mixed");                    // dispatch boxed mixed payload by runtime tag

    emitter.label("__rt_is_callable_heap_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore caller frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release heap dispatch frame
    emitter.instruction("ret");                                                 // return boolean result in x0
}

/// Emits all x86_64 `is_callable()` runtime helpers for the current program.
fn emit_x86_64(emitter: &mut Emitter) {
    emit_string_x86_64(emitter);
    emit_method_name_x86_64(emitter);
    emit_static_method_name_x86_64(emitter);
    emit_object_x86_64(emitter);
    emit_array_x86_64(emitter);
    emit_assoc_x86_64(emitter);
    emit_mixed_x86_64(emitter);
    emit_heap_x86_64(emitter);
}

/// Emits the x86_64 runtime helper for string callable lookup.
/// Scans builtin names (case-insensitive), user functions (exact match),
/// then checks for `Class::method` format via `__rt_is_callable_static_method_name`.
/// Input: rdi=string pointer, rsi=string length. Output: rax=1 (true) or 0 (false).
fn emit_string_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: is_callable_string ---");
    emitter.label_global("__rt_is_callable_string");

    emitter.instruction("push rbp");                                            // preserve caller frame pointer before table scans
    emitter.instruction("mov rbp, rsp");                                        // establish stable frame base for saved string and table cursor
    emitter.instruction("sub rsp, 48");                                         // reserve slots for string pointer, length, table pointer, count, index, and entry pointer
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save candidate string pointer for repeated comparisons
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save candidate string length for repeated comparisons
    abi::emit_symbol_address(emitter, "r10", "_callable_builtin_table");
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save builtin callable-name table pointer
    abi::emit_symbol_address(emitter, "r10", "_callable_builtin_count");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load builtin callable-name count from fixed runtime data
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save active table count for builtin scan
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // start builtin scan at entry index zero

    emitter.label("__rt_is_callable_string_builtin_loop_x86_64");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // load current builtin table index
    emitter.instruction("cmp r10, QWORD PTR [rbp - 32]");                       // have all builtin names been checked?
    emitter.instruction("jge __rt_is_callable_string_user_setup_x86_64");       // continue with user functions after builtin scan
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload builtin table pointer
    emitter.instruction("shl r10, 4");                                          // multiply index by 16-byte builtin table entry size
    emitter.instruction("add r11, r10");                                        // compute current builtin table entry address
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass candidate string pointer as comparison left side
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // pass candidate string length as comparison left side
    emitter.instruction("mov rdx, QWORD PTR [r11]");                            // pass builtin name pointer as comparison right side
    emitter.instruction("mov rcx, QWORD PTR [r11 + 8]");                        // pass builtin name length as comparison right side
    abi::emit_call_label(emitter, "__rt_strcasecmp");                           // compare builtin names case-insensitively like PHP internals
    emitter.instruction("test rax, rax");                                       // zero comparison result means equal strings
    emitter.instruction("je __rt_is_callable_string_true_x86_64");              // return true for recognized builtin callable strings
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload current builtin table index
    emitter.instruction("add r10, 1");                                          // advance to the next builtin table entry
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // persist incremented builtin table index
    emitter.instruction("jmp __rt_is_callable_string_builtin_loop_x86_64");     // continue scanning builtin callable names

    emitter.label("__rt_is_callable_string_user_setup_x86_64");
    abi::emit_symbol_address(emitter, "r10", "_callable_user_function_table");
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // switch saved table pointer to user function metadata
    abi::emit_symbol_address(emitter, "r10", "_callable_user_function_count");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load user function table entry count
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save user function count for exact-name scan
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // restart table scanning at user entry index zero

    emitter.label("__rt_is_callable_string_user_loop_x86_64");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // load current user function table index
    emitter.instruction("cmp r10, QWORD PTR [rbp - 32]");                       // have all user function names been checked?
    emitter.instruction("jge __rt_is_callable_string_static_setup_x86_64");     // continue with Class::method strings after user functions
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload user function table pointer
    emitter.instruction("mov r8, r10");                                         // copy index before scaling to 24-byte entries
    emitter.instruction("shl r10, 4");                                          // compute index * 16 as the first part of index * 24
    emitter.instruction("shl r8, 3");                                           // compute index * 8 as the second part of index * 24
    emitter.instruction("add r10, r8");                                         // combine scaled parts into index * 24
    emitter.instruction("add r11, r10");                                        // compute current user function table entry address
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // save current entry pointer across string comparison
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass candidate string pointer as comparison left side
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // pass candidate string length as comparison left side
    emitter.instruction("mov rdx, QWORD PTR [r11]");                            // pass user function name pointer as comparison right side
    emitter.instruction("mov rcx, QWORD PTR [r11 + 8]");                        // pass user function name length as comparison right side
    abi::emit_call_label(emitter, "__rt_str_eq");                               // compare user function names exactly, matching current AOT lookup behavior
    emitter.instruction("test rax, rax");                                       // nonzero means the exact user function name matched
    emitter.instruction("je __rt_is_callable_string_user_next_x86_64");         // continue scanning when the user name did not match
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // reload current entry pointer after comparison
    emitter.instruction("mov r8, QWORD PTR [r11 + 16]");                        // load optional active-variant symbol pointer
    emitter.instruction("test r8, r8");                                         // ordinary functions use a null active-symbol pointer
    emitter.instruction("je __rt_is_callable_string_true_x86_64");              // ordinary user functions are callable as soon as their name matches
    emitter.instruction("mov r8, QWORD PTR [r8]");                              // read active implementation pointer for this variant group
    emitter.instruction("test r8, r8");                                         // include variant is callable only when an implementation is active
    emitter.instruction("jne __rt_is_callable_string_true_x86_64");             // return true for active function variants

    emitter.label("__rt_is_callable_string_user_next_x86_64");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload current user function table index
    emitter.instruction("add r10, 1");                                          // advance to next user function entry
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // persist incremented user function index
    emitter.instruction("jmp __rt_is_callable_string_user_loop_x86_64");        // continue scanning user function names

    emitter.label("__rt_is_callable_string_static_setup_x86_64");
    emitter.instruction("cmp QWORD PTR [rbp - 16], 3");                         // Class::method needs at least one byte on both sides of ::
    emitter.instruction("jl __rt_is_callable_string_false_x86_64");             // too-short strings cannot name a static method callable
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // start separator scan at byte index zero

    emitter.label("__rt_is_callable_string_static_scan_x86_64");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // load current byte index while searching for ::
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // load candidate string length
    emitter.instruction("sub r11, 1");                                          // stop before the final byte so [index + 1] is in bounds
    emitter.instruction("cmp r10, r11");                                        // have all possible separator positions been checked?
    emitter.instruction("jge __rt_is_callable_string_false_x86_64");            // no :: separator means this is not a static-method string
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload candidate string pointer
    emitter.instruction("add r8, r10");                                         // point at the current candidate separator byte
    emitter.instruction("cmp BYTE PTR [r8], 58");                               // is the current byte ':'?
    emitter.instruction("jne __rt_is_callable_string_static_next_x86_64");      // keep scanning until the first ':' byte
    emitter.instruction("cmp BYTE PTR [r8 + 1], 58");                           // is the following byte ':' too?
    emitter.instruction("je __rt_is_callable_string_static_found_x86_64");      // split the string at the :: separator

    emitter.label("__rt_is_callable_string_static_next_x86_64");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload separator scan index
    emitter.instruction("add r10, 1");                                          // advance to the next candidate separator byte
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // persist the incremented separator scan index
    emitter.instruction("jmp __rt_is_callable_string_static_scan_x86_64");      // continue scanning for Class::method separator

    emitter.label("__rt_is_callable_string_static_found_x86_64");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // load separator index, which is also the class-name length
    emitter.instruction("test r10, r10");                                       // empty class name before :: is not callable
    emitter.instruction("je __rt_is_callable_string_false_x86_64");             // reject strings that start with ::
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload total candidate string length
    emitter.instruction("mov r8, r10");                                         // copy class-name length before computing method offset
    emitter.instruction("add r8, 2");                                           // compute the method-name start index after ::
    emitter.instruction("cmp r8, r11");                                         // is there at least one method byte after ::?
    emitter.instruction("jge __rt_is_callable_string_false_x86_64");            // empty method name after :: is not callable
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass class-name pointer to static-method lookup
    emitter.instruction("mov rsi, r10");                                        // pass class-name length to static-method lookup
    emitter.instruction("lea rdx, [rdi + r8]");                                 // pass method-name pointer after the :: separator
    emitter.instruction("mov rcx, r11");                                        // copy total length before deriving method-name length
    emitter.instruction("sub rcx, r8");                                         // pass method-name length after the :: separator
    abi::emit_call_label(emitter, "__rt_is_callable_static_method_name");       // test Class::method strings against public static method metadata
    emitter.instruction("jmp __rt_is_callable_string_done_x86_64");             // keep static-method lookup result and restore this helper's frame

    emitter.label("__rt_is_callable_string_true_x86_64");
    emitter.instruction("mov rax, 1");                                          // return true for recognized callable strings
    emitter.instruction("jmp __rt_is_callable_string_done_x86_64");             // restore frame before returning true

    emitter.label("__rt_is_callable_string_false_x86_64");
    emitter.instruction("xor eax, eax");                                        // return false when no callable string target exists

    emitter.label("__rt_is_callable_string_done_x86_64");
    emitter.instruction("add rsp, 48");                                         // release lookup frame slots
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return boolean result in rax
}

/// Emits the x86_64 runtime helper for object method callable lookup.
/// Looks up a named public method on an object via the class's public-method name table.
/// Input: rdi=object pointer, rsi=method name pointer, rdx=method name length. Output: rax=1 (true) or 0 (false).
fn emit_method_name_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: is_callable_method_name ---");
    emitter.label_global("__rt_is_callable_method_name");

    emitter.instruction("push rbp");                                            // preserve caller frame pointer before method table scan
    emitter.instruction("mov rbp, rsp");                                        // establish stable frame base for receiver and method string
    emitter.instruction("sub rsp, 48");                                         // reserve slots for receiver, method string, table pointer, count, index, and entry
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save receiver object pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save candidate method string pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save candidate method string length
    emitter.instruction("test rdi, rdi");                                       // null receivers cannot expose callable methods
    emitter.instruction("je __rt_is_callable_method_false_x86_64");             // return false for null receiver objects
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load receiver runtime class id from object header
    abi::emit_symbol_address(emitter, "r11", "_class_callable_method_ptrs");
    emitter.instruction("mov r11, QWORD PTR [r11 + r10 * 8]");                  // load class-specific public-method name table pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // save method table pointer across string comparisons
    emitter.instruction("mov r10, QWORD PTR [r11]");                            // load public-method name count from class table
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save method table count
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // start method table scan at entry index zero

    emitter.label("__rt_is_callable_method_loop_x86_64");
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // load current method table index
    emitter.instruction("cmp r10, QWORD PTR [rbp - 40]");                       // have all public methods been checked?
    emitter.instruction("jge __rt_is_callable_method_false_x86_64");            // no method name matched
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload method table pointer
    emitter.instruction("add r11, 8");                                          // skip count word before indexed method entries
    emitter.instruction("shl r10, 4");                                          // multiply method index by 16-byte entry size
    emitter.instruction("add r11, r10");                                        // compute current method entry address
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // pass candidate method pointer as comparison left side
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // pass candidate method length as comparison left side
    emitter.instruction("mov rdx, QWORD PTR [r11]");                            // pass table method pointer as comparison right side
    emitter.instruction("mov rcx, QWORD PTR [r11 + 8]");                        // pass table method length as comparison right side
    abi::emit_call_label(emitter, "__rt_strcasecmp");                           // compare method names case-insensitively like PHP method lookup
    emitter.instruction("test rax, rax");                                       // zero comparison result means equal method names
    emitter.instruction("je __rt_is_callable_method_true_x86_64");              // return true for public method matches
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload current method table index
    emitter.instruction("add r10, 1");                                          // advance to next public method
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // persist incremented method table index
    emitter.instruction("jmp __rt_is_callable_method_loop_x86_64");             // continue scanning public methods

    emitter.label("__rt_is_callable_method_true_x86_64");
    emitter.instruction("mov rax, 1");                                          // return true for public method matches
    emitter.instruction("jmp __rt_is_callable_method_done_x86_64");             // restore frame before returning true

    emitter.label("__rt_is_callable_method_false_x86_64");
    emitter.instruction("xor eax, eax");                                        // return false when no public method matches

    emitter.label("__rt_is_callable_method_done_x86_64");
    emitter.instruction("add rsp, 48");                                         // release method lookup frame slots
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return boolean result in rax
}

/// Emits the x86_64 runtime helper for static method callable lookup.
/// Compares the given class name and method name against the global static method table
/// (both compared case-insensitively). Handles leading backslash normalization.
/// Input: rdi=class name ptr, rsi=class name len, rdx=method name ptr, rcx=method name len. Output: rax=1 or 0.
fn emit_static_method_name_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: is_callable_static_method_name ---");
    emitter.label_global("__rt_is_callable_static_method_name");

    emitter.instruction("push rbp");                                            // preserve caller frame pointer before static method table scan
    emitter.instruction("mov rbp, rsp");                                        // establish stable frame base for class and method strings
    emitter.instruction("sub rsp, 64");                                         // reserve slots for class string, method string, table cursor, and entry pointer
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save candidate class string pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save candidate class string length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save candidate static method string pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save candidate static method string length
    emitter.instruction("test rdi, rdi");                                       // null class-name pointers cannot name static callables
    emitter.instruction("je __rt_is_callable_static_method_false_x86_64");      // reject null class-name pointers
    emitter.instruction("test rsi, rsi");                                       // empty class names cannot name static callables
    emitter.instruction("je __rt_is_callable_static_method_false_x86_64");      // reject empty class names
    emitter.instruction("test rdx, rdx");                                       // null method-name pointers cannot name static callables
    emitter.instruction("je __rt_is_callable_static_method_false_x86_64");      // reject null method-name pointers
    emitter.instruction("test rcx, rcx");                                       // empty method names cannot name static callables
    emitter.instruction("je __rt_is_callable_static_method_false_x86_64");      // reject empty method names
    emitter.instruction("cmp BYTE PTR [rdi], 92");                              // does the class name start with a leading backslash?
    emitter.instruction("jne __rt_is_callable_static_method_setup_x86_64");     // class names without leading slash can be compared as-is
    emitter.instruction("cmp rsi, 1");                                          // a single leading slash leaves an empty class name
    emitter.instruction("jle __rt_is_callable_static_method_false_x86_64");     // reject empty class names after removing the leading slash
    emitter.instruction("add QWORD PTR [rbp - 8], 1");                          // skip the leading namespace separator for table comparison
    emitter.instruction("sub QWORD PTR [rbp - 16], 1");                         // shorten the class-name length after skipping the slash

    emitter.label("__rt_is_callable_static_method_setup_x86_64");
    abi::emit_symbol_address(emitter, "r10", "_class_callable_static_method_table");
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save static callable table pointer across comparisons
    abi::emit_symbol_address(emitter, "r10", "_class_callable_static_method_count");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load number of public static method callable entries
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // save static callable entry count
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // start static callable scan at entry index zero

    emitter.label("__rt_is_callable_static_method_loop_x86_64");
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // load current static callable table index
    emitter.instruction("cmp r10, QWORD PTR [rbp - 48]");                       // have all static method entries been checked?
    emitter.instruction("jge __rt_is_callable_static_method_false_x86_64");     // no public static method matched the class/method pair
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload static callable table pointer
    emitter.instruction("shl r10, 5");                                          // multiply index by 32-byte static callable entry size
    emitter.instruction("add r11, r10");                                        // compute address of the current static callable entry
    emitter.instruction("mov QWORD PTR [rbp - 64], r11");                       // preserve current table entry across string comparison calls
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass candidate class pointer as comparison left side
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // pass candidate class length as comparison left side
    emitter.instruction("mov rdx, QWORD PTR [r11]");                            // pass table class pointer as comparison right side
    emitter.instruction("mov rcx, QWORD PTR [r11 + 8]");                        // pass table class length as comparison right side
    abi::emit_call_label(emitter, "__rt_strcasecmp");                           // compare class names case-insensitively like PHP class lookup
    emitter.instruction("test rax, rax");                                       // zero comparison result means equal class names
    emitter.instruction("jne __rt_is_callable_static_method_next_x86_64");      // move on when the class name did not match
    emitter.instruction("mov r11, QWORD PTR [rbp - 64]");                       // reload current table entry after caller-saved registers were clobbered
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // pass candidate method pointer as comparison left side
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // pass candidate method length as comparison left side
    emitter.instruction("mov rdx, QWORD PTR [r11 + 16]");                       // pass table method pointer as comparison right side
    emitter.instruction("mov rcx, QWORD PTR [r11 + 24]");                       // pass table method length as comparison right side
    abi::emit_call_label(emitter, "__rt_strcasecmp");                           // compare static method names case-insensitively like PHP method lookup
    emitter.instruction("test rax, rax");                                       // zero comparison result means equal method names
    emitter.instruction("je __rt_is_callable_static_method_true_x86_64");       // matching class and method strings name a public static callable

    emitter.label("__rt_is_callable_static_method_next_x86_64");
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // reload current static callable table index
    emitter.instruction("add r10, 1");                                          // advance to the next static callable entry
    emitter.instruction("mov QWORD PTR [rbp - 56], r10");                       // persist the incremented static callable index
    emitter.instruction("jmp __rt_is_callable_static_method_loop_x86_64");      // continue scanning public static methods

    emitter.label("__rt_is_callable_static_method_true_x86_64");
    emitter.instruction("mov rax, 1");                                          // return true for public static method matches
    emitter.instruction("jmp __rt_is_callable_static_method_done_x86_64");      // restore frame before returning true

    emitter.label("__rt_is_callable_static_method_false_x86_64");
    emitter.instruction("xor eax, eax");                                        // return false when no public static method matches

    emitter.label("__rt_is_callable_static_method_done_x86_64");
    emitter.instruction("add rsp, 64");                                         // release static callable lookup frame slots
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return boolean result in rax
}

/// Emits the x86_64 runtime helper for invokable object lookup.
/// Delegates to `__rt_is_callable_method_name` with the `"__invoke"` method name.
/// Input: rdi=object pointer. Output: rax=1 (true) or 0 (false).
fn emit_object_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: is_callable_object ---");
    emitter.label_global("__rt_is_callable_object");

    emitter.instruction("push rbp");                                            // preserve caller frame pointer while delegating lookup
    emitter.instruction("mov rbp, rsp");                                        // establish stable frame base for delegation
    abi::emit_symbol_address(emitter, "rsi", "_callable_invoke_name");
    emitter.instruction("mov rdx, 8");                                          // method string length for "__invoke"
    abi::emit_call_label(emitter, "__rt_is_callable_method_name");              // test whether object exposes public __invoke
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return delegated method lookup result
}

/// Emits the x86_64 runtime helper for indexed-array callable lookup.
/// Validates exactly 2 slots; dispatches Mixed slots (receiver unboxed to object or string)
/// and raw string slots (treated as [class-name, static-method] pair).
/// Input: rdi=indexed array pointer. Output: rax=1 (true) or 0 (false).
fn emit_array_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: is_callable_array ---");
    emitter.label_global("__rt_is_callable_array");

    emitter.instruction("push rbp");                                            // preserve caller frame pointer before array inspection
    emitter.instruction("mov rbp, rsp");                                        // establish stable frame base for saved array pieces
    emitter.instruction("sub rsp, 32");                                         // reserve slots for array pointer, receiver object, and method payload
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save indexed array pointer
    emitter.instruction("test rdi, rdi");                                       // null arrays are not callable
    emitter.instruction("je __rt_is_callable_array_false_x86_64");              // reject null indexed array pointers
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load indexed array length
    emitter.instruction("cmp r10, 2");                                          // callable arrays must have receiver and method entries
    emitter.instruction("jne __rt_is_callable_array_false_x86_64");             // reject arrays without exactly two entries
    emitter.instruction("mov r10, QWORD PTR [rdi - 8]");                        // load packed heap kind and value-type metadata
    emitter.instruction("shr r10, 8");                                          // move array value-type tag into low bits
    emitter.instruction("and r10, 0x7f");                                       // isolate array value-type tag without ownership flags
    emitter.instruction("cmp r10, 7");                                          // are indexed slots boxed Mixed pointers?
    emitter.instruction("je __rt_is_callable_array_mixed_slots_x86_64");        // Mixed arrays may hold object/string method callables
    emitter.instruction("cmp r10, 1");                                          // are indexed slots raw string pointer/length pairs?
    emitter.instruction("je __rt_is_callable_array_string_slots_x86_64");       // string arrays may hold [class-name, static-method] callables
    emitter.instruction("jmp __rt_is_callable_array_false_x86_64");             // other homogeneous arrays cannot represent callable arrays

    emitter.label("__rt_is_callable_array_string_slots_x86_64");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload indexed array pointer for raw string slot reads
    emitter.instruction("mov rdi, QWORD PTR [r10 + 24]");                       // pass class-name string pointer from slot 0
    emitter.instruction("mov rsi, QWORD PTR [r10 + 32]");                       // pass class-name string length from slot 0
    emitter.instruction("mov rdx, QWORD PTR [r10 + 40]");                       // pass static method string pointer from slot 1
    emitter.instruction("mov rcx, QWORD PTR [r10 + 48]");                       // pass static method string length from slot 1
    abi::emit_call_label(emitter, "__rt_is_callable_static_method_name");       // check whether raw string slots name a public static method
    emitter.instruction("jmp __rt_is_callable_array_done_x86_64");              // keep static method lookup result and restore frame

    emitter.label("__rt_is_callable_array_mixed_slots_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rdi + 24]");                       // load boxed Mixed receiver slot
    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // unwrap receiver slot to runtime tag and payload
    emitter.instruction("cmp rax, 6");                                          // is the receiver slot an object?
    emitter.instruction("je __rt_is_callable_array_receiver_object_x86_64");    // object receivers use public instance method lookup
    emitter.instruction("cmp rax, 1");                                          // is the receiver slot a class-name string?
    emitter.instruction("je __rt_is_callable_array_receiver_string_x86_64");    // string receivers use public static method lookup
    emitter.instruction("jmp __rt_is_callable_array_false_x86_64");             // reject receiver slots that are neither object nor string

    emitter.label("__rt_is_callable_array_receiver_object_x86_64");
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // save receiver object pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], 6");                         // mark saved receiver as an object
    emitter.instruction("jmp __rt_is_callable_array_method_x86_64");            // continue by extracting method-name slot

    emitter.label("__rt_is_callable_array_receiver_string_x86_64");
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // save receiver class-name string pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save receiver class-name string length
    emitter.instruction("mov QWORD PTR [rbp - 32], 1");                         // mark saved receiver as a class-name string

    emitter.label("__rt_is_callable_array_method_x86_64");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload indexed array pointer
    emitter.instruction("mov rax, QWORD PTR [r10 + 32]");                       // load boxed Mixed method-name slot
    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // unwrap method-name slot to runtime tag and payload
    emitter.instruction("cmp rax, 1");                                          // method slot must contain a string
    emitter.instruction("jne __rt_is_callable_array_false_x86_64");             // reject non-string method slots
    emitter.instruction("cmp QWORD PTR [rbp - 32], 6");                         // should this be dispatched as an object method?
    emitter.instruction("je __rt_is_callable_array_dispatch_object_x86_64");    // object receiver uses instance method metadata
    emitter.instruction("mov rcx, rdx");                                        // move method string length into static lookup argument 3
    emitter.instruction("mov rdx, rdi");                                        // move method string pointer into static lookup argument 2
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // pass saved class-name string pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // pass saved class-name string length
    abi::emit_call_label(emitter, "__rt_is_callable_static_method_name");       // check whether [class-string, method] names a public static method
    emitter.instruction("jmp __rt_is_callable_array_done_x86_64");              // keep static method lookup result and restore frame

    emitter.label("__rt_is_callable_array_dispatch_object_x86_64");
    emitter.instruction("mov rsi, rdi");                                        // move method string pointer to second argument register
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload receiver object pointer into first argument register
    abi::emit_call_label(emitter, "__rt_is_callable_method_name");              // rdx already holds method string length from mixed unbox
    emitter.instruction("jmp __rt_is_callable_array_done_x86_64");              // keep method lookup result and restore frame

    emitter.label("__rt_is_callable_array_false_x86_64");
    emitter.instruction("xor eax, eax");                                        // return false for malformed callable arrays

    emitter.label("__rt_is_callable_array_done_x86_64");
    emitter.instruction("add rsp, 32");                                         // release array inspection slots
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return boolean result in rax
}

/// Emits the x86_64 runtime helper for associative-array callable lookup.
/// Extracts the receiver (key 0, unboxed if Mixed) and method name (key 1, unboxed if Mixed),
/// then dispatches to instance method or static method lookup based on receiver type.
/// Input: rdi=associative array pointer. Output: rax=1 (true) or 0 (false).
fn emit_assoc_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: is_callable_assoc ---");
    emitter.label_global("__rt_is_callable_assoc");

    emitter.instruction("push rbp");                                            // preserve caller frame pointer before hash inspection
    emitter.instruction("mov rbp, rsp");                                        // establish stable frame base for saved hash pieces
    emitter.instruction("sub rsp, 32");                                         // reserve slots for hash pointer and extracted receiver object
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save associative array pointer
    emitter.instruction("test rdi, rdi");                                       // null hashes are not callable
    emitter.instruction("je __rt_is_callable_assoc_false_x86_64");              // reject null associative array pointers
    emitter.instruction("xor esi, esi");                                        // lookup key 0 for callable receiver
    emitter.instruction("mov rdx, -1");                                         // key_hi = -1 marks integer hash key
    abi::emit_call_label(emitter, "__rt_hash_get");                             // fetch entry 0 from hash table
    emitter.instruction("test rax, rax");                                       // found flag reports whether receiver key exists
    emitter.instruction("je __rt_is_callable_assoc_false_x86_64");              // missing receiver entry rejects callable array
    emitter.instruction("cmp rcx, 7");                                          // is receiver entry a boxed Mixed cell?
    emitter.instruction("je __rt_is_callable_assoc_first_mixed_x86_64");        // unwrap boxed receiver entries before checking type
    emitter.instruction("cmp rcx, 6");                                          // is the direct receiver entry an object?
    emitter.instruction("je __rt_is_callable_assoc_first_object_x86_64");       // object receivers use public instance method lookup
    emitter.instruction("cmp rcx, 1");                                          // is the direct receiver entry a class-name string?
    emitter.instruction("je __rt_is_callable_assoc_first_string_x86_64");       // string receivers use public static method lookup
    emitter.instruction("jmp __rt_is_callable_assoc_false_x86_64");             // reject direct receiver entries that are neither object nor string

    emitter.label("__rt_is_callable_assoc_first_object_x86_64");
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // save direct receiver object pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], 6");                         // mark saved receiver as an object
    emitter.instruction("jmp __rt_is_callable_assoc_second_x86_64");            // continue with method-name entry lookup

    emitter.label("__rt_is_callable_assoc_first_string_x86_64");
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // save direct receiver class-name string pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // save direct receiver class-name string length
    emitter.instruction("mov QWORD PTR [rbp - 32], 1");                         // mark saved receiver as a class-name string
    emitter.instruction("jmp __rt_is_callable_assoc_second_x86_64");            // continue with method-name entry lookup

    emitter.label("__rt_is_callable_assoc_first_mixed_x86_64");
    emitter.instruction("mov rax, rdi");                                        // pass boxed receiver Mixed pointer to unbox helper
    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // unwrap receiver entry to runtime tag and payload
    emitter.instruction("cmp rax, 6");                                          // is the boxed receiver an object?
    emitter.instruction("je __rt_is_callable_assoc_first_mixed_object_x86_64"); // object receivers use public instance method lookup
    emitter.instruction("cmp rax, 1");                                          // is the boxed receiver a class-name string?
    emitter.instruction("je __rt_is_callable_assoc_first_mixed_string_x86_64"); // string receivers use public static method lookup
    emitter.instruction("jmp __rt_is_callable_assoc_false_x86_64");             // reject boxed receiver entries that are neither object nor string

    emitter.label("__rt_is_callable_assoc_first_mixed_object_x86_64");
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // save unboxed receiver object pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], 6");                         // mark saved receiver as an object
    emitter.instruction("jmp __rt_is_callable_assoc_second_x86_64");            // continue with method-name entry lookup

    emitter.label("__rt_is_callable_assoc_first_mixed_string_x86_64");
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // save unboxed receiver class-name string pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save unboxed receiver class-name string length
    emitter.instruction("mov QWORD PTR [rbp - 32], 1");                         // mark saved receiver as a class-name string

    emitter.label("__rt_is_callable_assoc_second_x86_64");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload associative array pointer for method-name lookup
    emitter.instruction("mov rsi, 1");                                          // lookup key 1 for callable method name
    emitter.instruction("mov rdx, -1");                                         // key_hi = -1 marks integer hash key
    abi::emit_call_label(emitter, "__rt_hash_get");                             // fetch entry 1 from hash table
    emitter.instruction("test rax, rax");                                       // found flag reports whether method key exists
    emitter.instruction("je __rt_is_callable_assoc_false_x86_64");              // missing method entry rejects callable array
    emitter.instruction("cmp rcx, 7");                                          // is method entry a boxed Mixed cell?
    emitter.instruction("je __rt_is_callable_assoc_second_mixed_x86_64");       // unwrap boxed method entries before checking type
    emitter.instruction("cmp rcx, 1");                                          // direct method entry must contain a string
    emitter.instruction("jne __rt_is_callable_assoc_false_x86_64");             // reject direct non-string method entries
    emitter.instruction("jmp __rt_is_callable_assoc_dispatch_x86_64");          // dispatch direct method string against the saved receiver kind

    emitter.label("__rt_is_callable_assoc_second_mixed_x86_64");
    emitter.instruction("mov rax, rdi");                                        // pass boxed method Mixed pointer to unbox helper
    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // unwrap method entry to runtime tag and payload
    emitter.instruction("cmp rax, 1");                                          // boxed method entry must contain a string
    emitter.instruction("jne __rt_is_callable_assoc_false_x86_64");             // reject boxed non-string method entries
    emitter.instruction("mov rsi, rdx");                                        // move unboxed method string length beside its pointer for shared dispatch

    emitter.label("__rt_is_callable_assoc_dispatch_x86_64");
    emitter.instruction("cmp QWORD PTR [rbp - 32], 6");                         // should this be dispatched as an object method?
    emitter.instruction("je __rt_is_callable_assoc_dispatch_object_x86_64");    // object receiver uses instance method metadata
    emitter.instruction("mov rcx, rsi");                                        // move method string length into static lookup argument 3
    emitter.instruction("mov rdx, rdi");                                        // move method string pointer into static lookup argument 2
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // pass saved class-name string pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // pass saved class-name string length
    abi::emit_call_label(emitter, "__rt_is_callable_static_method_name");       // check whether [class-string, method] names a public static method
    emitter.instruction("jmp __rt_is_callable_assoc_done_x86_64");              // keep static method lookup result and restore frame

    emitter.label("__rt_is_callable_assoc_dispatch_object_x86_64");
    emitter.instruction("mov rdx, rsi");                                        // move method string length to third argument register
    emitter.instruction("mov rsi, rdi");                                        // move unboxed method string pointer to second argument register
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload receiver object pointer into first argument register
    abi::emit_call_label(emitter, "__rt_is_callable_method_name");              // check whether the method string names a public receiver method
    emitter.instruction("jmp __rt_is_callable_assoc_done_x86_64");              // keep method lookup result and restore frame

    emitter.label("__rt_is_callable_assoc_false_x86_64");
    emitter.instruction("xor eax, eax");                                        // return false for malformed associative callable arrays

    emitter.label("__rt_is_callable_assoc_done_x86_64");
    emitter.instruction("add rsp, 32");                                         // release hash inspection slots
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return boolean result in rax
}

/// Emits the x86_64 runtime helper for boxed Mixed callable dispatch.
/// Unboxes the Mixed payload and dispatches by runtime tag:
/// string → `__rt_is_callable_string`, array → `__rt_is_callable_array`,
/// assoc → `__rt_is_callable_assoc`, object → `__rt_is_callable_object`.
/// Input: rdi=Mixed pointer. Output: rax=1 (true) or 0 (false).
fn emit_mixed_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: is_callable_mixed ---");
    emitter.label_global("__rt_is_callable_mixed");

    emitter.instruction("push rbp");                                            // preserve caller frame pointer before mixed dispatch
    emitter.instruction("mov rbp, rsp");                                        // establish stable frame base for nested calls
    emitter.instruction("mov rax, rdi");                                        // move boxed Mixed pointer into mixed_unbox input register
    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // unwrap nested Mixed cells into concrete tag and payload
    emitter.instruction("cmp rax, 1");                                          // is the mixed payload a string?
    emitter.instruction("je __rt_is_callable_mixed_string_x86_64");             // strings use runtime function-name lookup
    emitter.instruction("cmp rax, 4");                                          // is the mixed payload an indexed array?
    emitter.instruction("je __rt_is_callable_mixed_array_x86_64");              // indexed arrays may be method callables
    emitter.instruction("cmp rax, 5");                                          // is the mixed payload an associative array?
    emitter.instruction("je __rt_is_callable_mixed_assoc_x86_64");              // associative arrays may be method callables
    emitter.instruction("cmp rax, 6");                                          // is the mixed payload an object?
    emitter.instruction("je __rt_is_callable_mixed_object_x86_64");             // objects may be invokable through public __invoke
    emitter.instruction("xor eax, eax");                                        // unsupported mixed payloads are not callable
    emitter.instruction("jmp __rt_is_callable_mixed_done_x86_64");              // restore frame before returning false

    emitter.label("__rt_is_callable_mixed_string_x86_64");
    emitter.instruction("mov rsi, rdx");                                        // pass unboxed string length to string callable lookup
    abi::emit_call_label(emitter, "__rt_is_callable_string");                   // rdi already holds unboxed string pointer
    emitter.instruction("jmp __rt_is_callable_mixed_done_x86_64");              // restore frame after string lookup

    emitter.label("__rt_is_callable_mixed_array_x86_64");
    abi::emit_call_label(emitter, "__rt_is_callable_array");                    // rdi already holds unboxed indexed array pointer
    emitter.instruction("jmp __rt_is_callable_mixed_done_x86_64");              // restore frame after indexed array lookup

    emitter.label("__rt_is_callable_mixed_assoc_x86_64");
    abi::emit_call_label(emitter, "__rt_is_callable_assoc");                    // rdi already holds unboxed associative array pointer
    emitter.instruction("jmp __rt_is_callable_mixed_done_x86_64");              // restore frame after associative lookup

    emitter.label("__rt_is_callable_mixed_object_x86_64");
    abi::emit_call_label(emitter, "__rt_is_callable_object");                   // rdi already holds unboxed object pointer

    emitter.label("__rt_is_callable_mixed_done_x86_64");
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return boolean result in rax
}

/// Emits the x86_64 runtime helper for erased heap-kind callable dispatch.
/// Probes the heap kind via `__rt_heap_kind` and delegates to the appropriate
/// array/object/mixed helper. Used for FFI-safe opaque heap pointers.
/// Input: rdi=heap pointer. Output: rax=1 (true) or 0 (false).
fn emit_heap_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: is_callable_heap ---");
    emitter.label_global("__rt_is_callable_heap");

    emitter.instruction("push rbp");                                            // preserve caller frame pointer before heap-kind dispatch
    emitter.instruction("mov rbp, rsp");                                        // establish stable frame base for saved heap pointer
    emitter.instruction("sub rsp, 16");                                         // reserve slot for original heap pointer
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save original heap pointer before heap_kind rewrites rax
    emitter.instruction("mov rax, rdi");                                        // move candidate pointer into heap_kind input register
    abi::emit_call_label(emitter, "__rt_heap_kind");                            // identify indexed array, hash, object, or mixed heap payloads
    emitter.instruction("cmp rax, 2");                                          // heap kind 2 = indexed array
    emitter.instruction("je __rt_is_callable_heap_array_x86_64");               // indexed arrays may be method callables
    emitter.instruction("cmp rax, 3");                                          // heap kind 3 = associative array
    emitter.instruction("je __rt_is_callable_heap_assoc_x86_64");               // associative arrays may be method callables
    emitter.instruction("cmp rax, 4");                                          // heap kind 4 = object
    emitter.instruction("je __rt_is_callable_heap_object_x86_64");              // objects may be invokable
    emitter.instruction("cmp rax, 5");                                          // heap kind 5 = boxed Mixed cell
    emitter.instruction("je __rt_is_callable_heap_mixed_x86_64");               // Mixed cells dispatch by runtime value tag
    emitter.instruction("xor eax, eax");                                        // other heap payloads are not callable
    emitter.instruction("jmp __rt_is_callable_heap_done_x86_64");               // restore frame before returning false

    emitter.label("__rt_is_callable_heap_array_x86_64");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload original indexed array pointer
    abi::emit_call_label(emitter, "__rt_is_callable_array");                    // inspect indexed array callable shape
    emitter.instruction("jmp __rt_is_callable_heap_done_x86_64");               // restore frame after indexed array lookup

    emitter.label("__rt_is_callable_heap_assoc_x86_64");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload original associative array pointer
    abi::emit_call_label(emitter, "__rt_is_callable_assoc");                    // inspect associative array callable shape
    emitter.instruction("jmp __rt_is_callable_heap_done_x86_64");               // restore frame after associative lookup

    emitter.label("__rt_is_callable_heap_object_x86_64");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload original object pointer
    abi::emit_call_label(emitter, "__rt_is_callable_object");                   // test for public __invoke
    emitter.instruction("jmp __rt_is_callable_heap_done_x86_64");               // restore frame after object lookup

    emitter.label("__rt_is_callable_heap_mixed_x86_64");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload original mixed cell pointer
    abi::emit_call_label(emitter, "__rt_is_callable_mixed");                    // dispatch boxed mixed payload by runtime tag

    emitter.label("__rt_is_callable_heap_done_x86_64");
    emitter.instruction("add rsp, 16");                                         // release heap dispatch slot
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return boolean result in rax
}
