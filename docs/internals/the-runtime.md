---
title: "The Runtime"
description: "Hand-written assembly routines for strings, arrays, generators, fibers, system calls, exceptions, and I/O."
sidebar:
  order: 8
---

**Source:** `src/codegen/runtime/` — `mod.rs`, `emitters.rs`, `data/`, `strings/`, `arrays/`, `buffers/`, `callables/`, `exceptions.rs`, `exceptions/`, `io/`, `objects/`, `spl/`, `system/`, `pointers/`, `fibers/`, `generators/`

The runtime is a collection of **hand-written assembly routines** that handle operations too complex for inline code generation. When the [code generator](the-codegen.md) needs to convert an integer to a string or concatenate two strings, it emits a `bl __rt_itoa` or `bl __rt_concat` — a call to a runtime routine.

These routines end up in every compiled binary. In the CLI flow they are usually pre-assembled into the cached runtime object rather than textually appended to each user `.s` file, but they are still part of the final executable rather than an external shared dependency.

## Why a runtime?

Some operations can't be done with a few inline instructions:

- **Integer to string** (`itoa`): Requires a loop that divides by 10, extracts digits, and writes them right-to-left
- **String concatenation**: Needs to copy bytes from two source strings into a buffer
- **Array operations**: Require heap allocation, bounds checking, and element copying

These are 20-50+ instructions each. Inlining them at every call site would bloat the binary. Instead, they're emitted once and called with `bl`.

## Naming convention

All runtime routines start with `__rt_`:

```
__rt_itoa          integer → string
__rt_resource_to_string resource → "Resource id #N"
__rt_ftoa          float → string
__rt_concat        string + string → string
__rt_str_eq        string == string → bool
__rt_array_new     allocate a new array
__rt_throw_current throw the active exception
__rt_build_argv    build $argv from C strings
```

## Diagnostic routines

**Source:** `src/codegen/runtime/diagnostics.rs`

These helpers implement PHP's `@` error-suppression operator and the runtime warning channel. The suppression depth lives in `_rt_diag_suppression`; while it is non-zero, suppressible warnings are silently dropped instead of written to stderr. They are emitted before any PHP-visible helper so the rest of the runtime can report warnings through a single path.

| Routine | What it does | Input | Output |
|---|---|---|---|
| `__rt_diag_push_suppression` | Enter one nested `@` suppression scope (increment `_rt_diag_suppression`) | — | — |
| `__rt_diag_pop_suppression` | Leave one `@` suppression scope, clamped against underflow | — | — |
| `__rt_diag_warning` | Write a runtime warning string to stderr unless suppression is active | `x1`/`x2` = message string | — |

## String routines

**Source:** `src/codegen/runtime/strings/`

### `__rt_itoa` — Integer to string

**File:** `strings/itoa.rs`

Converts a signed 64-bit integer in `x0` to a decimal string.

**Input:** `x0` = integer value
**Output:** `x1` = pointer to string, `x2` = length

**Algorithm:**
1. Check for negative → set flag, negate
2. Check for zero → output "0" directly
3. Loop: divide by 10 (`udiv` + `msub`), convert remainder to ASCII digit (`+ 48`), store right-to-left
4. Prepend '-' if negative
5. Update concat buffer offset

The digits are written **right-to-left** because division gives us the least significant digit first. The result is written into the [concat buffer](memory-model.md#the-string-buffer-scratch-pad).

### `__rt_resource_to_string` — Resource to string

**File:** `strings/resource_to_string.rs`

Formats the native resource payload used by stream handles as PHP's display string (`Resource id #N`). The helper keeps resources distinct from integers when boxing into `mixed`, while still letting the I/O runtime pass the underlying file descriptor to stream syscalls.

**Input:** `x0` = native resource payload
**Output:** `x1` = pointer to string, `x2` = length

`__rt_resource_write_stdout` uses the same display form for `echo` / `print` without exposing the raw file descriptor as an integer.

### `__rt_ftoa` — Float to string

**File:** `strings/ftoa.rs`

Converts a double-precision float in `d0` to a decimal string. Handles special cases: `INF`, `-INF`, `NAN`. For normal numbers, it separates the integer and fractional parts, converts each to digits, and joins them with a decimal point.

**Input:** `d0` = float value
**Output:** `x1` = pointer to string, `x2` = length

### `__rt_concat` — String concatenation

**File:** `strings/concat.rs`

Concatenates two strings by copying both into the [concat buffer](memory-model.md#the-string-buffer-scratch-pad).

**Input:** `x1`/`x2` = left string (ptr/len), `x3`/`x4` = right string (ptr/len)
**Output:** `x1` = pointer to result, `x2` = total length

**Algorithm:**
1. Get current position in concat buffer
2. Copy left string bytes (byte-by-byte loop)
3. Copy right string bytes
4. Update buffer offset
5. Return pointer to start of result + total length

### `__rt_atoi` — String to integer

**File:** `strings/atoi.rs`

Parses a decimal string into a 64-bit integer. Handles optional leading `-` sign.

**Input:** `x1` = string pointer, `x2` = length
**Output:** `x0` = integer value

### `__rt_str_eq` — String equality

**File:** `strings/str_eq.rs`

Compares two strings byte-by-byte.

**Input:** `x1`/`x2` = first string, `x3`/`x4` = second string
**Output:** `x0` = 1 if equal, 0 if not

**Algorithm:**
1. Compare lengths — if different, return 0 immediately
2. Loop: compare byte by byte
3. If all bytes match, return 1

### Other string routines

Each routine follows the same pattern — inputs in registers, output in standard result registers:

| Routine | What it does | Input | Output |
|---|---|---|---|
| `__rt_strcopy` | Copy string into concat buffer | `x1`/`x2` | `x1`/`x2` |
| `__rt_str_to_number` | Parse a PHP numeric string for loose comparison and numeric-string casts | `x1`/`x2` | numeric payload + success flag |
| `__rt_str_to_int` | Parse a PHP numeric-string prefix (via `__rt_str_to_number`) and truncate toward zero like PHP `(int)` casts | `x1`/`x2` | `x0` (integer) |
| `__rt_str_loose_eq` | Compare two strings using PHP loose-comparison numeric-string rules before falling back to bytes | two strings | `x0` (0 or 1) |
| `__rt_strtolower` | Lowercase conversion | `x1`/`x2` | `x1`/`x2` |
| `__rt_strtoupper` | Uppercase conversion | `x1`/`x2` | `x1`/`x2` |
| `__rt_trim` | Strip whitespace (no args) or chars in mask | `x1`/`x2` | `x1`/`x2` |
| `__rt_ltrim` / `__rt_rtrim` | Strip left/right whitespace or mask | `x1`/`x2` | `x1`/`x2` |
| `__rt_trim_mask` | Strip chars in custom mask from both ends | `x1`/`x2` + mask | `x1`/`x2` |
| `__rt_ltrim_mask` / `__rt_rtrim_mask` | Strip custom mask from left/right | `x1`/`x2` + mask | `x1`/`x2` |
| `__rt_strrev` | Reverse string (byte-wise) | `x1`/`x2` | `x1`/`x2` |
| `__rt_grapheme_strrev` | Reverse a UTF-8 string by grapheme cluster for PHP 8.6 `grapheme_strrev()`; returns false on malformed UTF-8 | `x1`/`x2` | `x1`/`x2` |
| `__rt_strpos` | Find substring | `x1`/`x2` + `x3`/`x4` | `x0` (index or -1) |
| `__rt_strrpos` | Find last occurrence | `x1`/`x2` + `x3`/`x4` | `x0` |
| `__rt_str_repeat` | Repeat N times with heap fallback for large results | `x1`/`x2` + count | `x1`/`x2` |
| `__rt_str_replace` | Replace all occurrences | search + replace + subject | `x1`/`x2` |
| `__rt_explode` | Split by delimiter | delimiter + string | `x0` (array ptr) |
| `__rt_implode` | Join string array with glue | glue + array | `x1`/`x2` |
| `__rt_implode_int` | Join integer array with glue | glue + array | `x1`/`x2` |
| `__rt_strcmp` | Binary comparison | two strings | `x0` (-1, 0, 1) |
| `__rt_strcasecmp` | Case-insensitive compare | two strings | `x0` |
| `__rt_str_starts_with` | Check prefix match | `x1`/`x2` + `x3`/`x4` | `x0` (0 or 1) |
| `__rt_str_ends_with` | Check suffix match | `x1`/`x2` + `x3`/`x4` | `x0` (0 or 1) |
| `__rt_chr` | ASCII code → char | `x0` | `x1`/`x2` |
| `__rt_addslashes` | Escape quotes/backslashes | `x1`/`x2` | `x1`/`x2` |
| `__rt_nl2br` | Insert `<br />` before newlines | `x1`/`x2` | `x1`/`x2` |
| `__rt_bin2hex` | Binary → hex string | `x1`/`x2` | `x1`/`x2` |
| `__rt_hex2bin` | Hex → binary | `x1`/`x2` | `x1`/`x2` |
| `__rt_md5` | MD5 hash | `x1`/`x2` | `x1`/`x2` |
| `__rt_sha1` | SHA1 hash | `x1`/`x2` | `x1`/`x2` |
| `__rt_sprintf` | Format string | format + args on stack | `x1`/`x2` |
| `__rt_base64_encode` | Base64 encode | `x1`/`x2` | `x1`/`x2` |
| `__rt_base64_decode` | Base64 decode | `x1`/`x2` | `x1`/`x2` |
| `__rt_urlencode` | URL encode | `x1`/`x2` | `x1`/`x2` |
| `__rt_urldecode` | URL decode | `x1`/`x2` | `x1`/`x2` |
| `__rt_htmlspecialchars` | HTML escape | `x1`/`x2` | `x1`/`x2` |
| `__rt_html_entity_decode` | Decode HTML entities | `x1`/`x2` | `x1`/`x2` |
| `__rt_rawurlencode` | URL encode (RFC 3986) | `x1`/`x2` | `x1`/`x2` |
| `__rt_stripslashes` | Remove escape backslashes | `x1`/`x2` | `x1`/`x2` |
| `__rt_ucwords` | Uppercase first letter of each word | `x1`/`x2` | `x1`/`x2` |
| `__rt_str_ireplace` | Case-insensitive replace | search + replace + subject | `x1`/`x2` |
| `__rt_substr_replace` | Replace substring at offset | str + replacement + start + len | `x1`/`x2` |
| `__rt_str_pad` | Pad string to length | str + len + pad_str + type | `x1`/`x2` |
| `__rt_str_split` | Split into chunks | str + chunk_len | `x0` (array ptr) |
| `__rt_wordwrap` | Wrap text at word boundaries | str + width + break + cut | `x1`/`x2` |
| `__rt_number_format` | Format number with separators | float + decimals + sep | `x1`/`x2` |
| `__rt_hash` | Hash with algorithm | algo + data | `x1`/`x2` |
| `__rt_hash_init` / `__rt_hash_update` / `__rt_hash_final` | Incremental hash-context API backing `hash_init()` and friends | context + data | context / `x1`/`x2` |
| `__rt_hash_copy` | Clone an incremental hash context | context | context |
| `__rt_hash_hmac` | Keyed HMAC over a message | algo + key + data | `x1`/`x2` |
| `__rt_hash_equals` | Constant-time string comparison | two strings | `x0` (0 or 1) |
| `__rt_hash_algos_list` | Build the `hash_algos()` array of supported algorithm names | — | `x0` (array ptr) |
| `__rt_digest_to_string` | Format a raw digest as lowercase hex | digest | `x1`/`x2` |
| `__rt_crc32` | CRC32 checksum | `x1`/`x2` | `x0` |
| `__rt_inet_ntop` / `__rt_inet_pton` | IPv4/IPv6 address ↔ packed-binary conversion | address | `x1`/`x2` |
| `__rt_long2ip` / `__rt_ip2long` | Dotted-quad string ↔ integer conversion | `x0` or `x1`/`x2` | `x1`/`x2` or `x0` |
| `__rt_vsprintf` | `vsprintf()` formatting with an argument array | format + array | `x1`/`x2` |
| `__rt_sscanf` | Parse string with format | str + format | `x0` (array ptr) |

## Callable routines

**Source:** `src/codegen/runtime/callables/` (3 files including `mod.rs`)

These routines implement the runtime fallback path for `is_callable()` when the argument is not a compile-time literal or statically known callable value. They consult generated metadata for builtins, user functions, public methods, public static methods, and `__invoke` objects.

Dynamic invocation builtins use generated callable descriptors rather than these boolean helpers. A descriptor is an eight-word record: callable kind, native entry pointer, PHP-visible name pointer, name length, signature-record pointer, environment-record pointer, invocation-record pointer, and optional uniform invoker pointer. Indirect calls keep the one-word callable ABI by loading the native entry from the descriptor, while descriptor-invoker paths call the generated `(descriptor, boxed argument container) -> mixed` adapter.

The signature side record stores visible, required, and regular parameter counts; variadic index; return type and return register count; declared-return flags; parameter names and types; defaults; by-reference flags; and declared-parameter flags. The environment record stores capture and hidden wrapper-parameter bindings. The invocation record stores the callable shape (`string`, callable array, closure, first-class callable, object `__invoke`, static method, instance method, builtin, extern, or user function) plus receiver, method, and auxiliary names where applicable.

`call_user_func()`, `call_user_func_array()`, direct string-variable calls, direct callable-array variable and literal calls, direct invokable-object calls, method first-class callable variable calls, and `iterator_apply()` compare runtime string names or descriptor-selected callable entries against generated cases. Runtime string callback names now materialize the matched descriptor and invoke its uniform invoker slot for user functions, extern wrappers, builtins, and public static methods, so signature defaults, named arguments, by-reference flags, variadics, and return boxing live behind the descriptor rather than in each string-dispatch callsite. `iterator_apply()` can also retain a branch-selected captured descriptor or runtime-selected callable-array descriptor and call that descriptor's invoker directly for each loop iteration. `array_map()`, `array_filter()`, `array_reduce()`, `array_walk()`, `usort()`, `uksort()`, `uasort()`, and `preg_replace_callback()` retain descriptor-valued callable variables and `callable` parameters in descriptor callback environments, so by-value closure captures, method receivers, and late-static binding state are read from descriptor storage instead of current source locals. These callback runtimes also match runtime callable-array variables such as `[$object, $method]` or `[$class, $method]` against public method descriptor cases before invoking the shared descriptor callback wrapper. `usort()`, `uksort()`, and `uasort()` use descriptor comparator environments for branch-selected captured descriptors. `CallbackFilterIterator` and `RecursiveCallbackFilterIterator` store branch-selected captured descriptors and runtime-selected callable-array variable or literal descriptors in persistent callback environments, then call the same descriptor invoker from `accept()`. `call_user_func()` and direct callable-variable invocations build boxed indexed argument containers for descriptor-backed calls; named direct calls build associative hashes. Variable arguments in either shape can be encoded as internal reference-cell markers, including boxed markers inside named hash entries, so the generated invoker can either pass the original storage to by-reference parameters or dereference it for by-value parameters after reading the descriptor signature. `call_user_func_array()` clones the provided indexed array or hash, widens the clone to boxed `Mixed`, and passes one boxed container to the descriptor invoker. The invoker inspects the boxed container tag at runtime, dispatches to indexed-array or associative-hash argument materialization, unboxes boxed array/hash payloads for declared array parameters, and is keyed by callable signature rather than by caller array shape. Closure and first-class-callable descriptors with `use (...)`, receiver, or late-static-binding context allocate runtime descriptor copies whose fixed header is followed by capture value slots; descriptor invokers reload those slots as hidden arguments, including by-reference captures. Method first-class callable variables use this descriptor path even when the original receiver variable still has static metadata, so reassignment of that source local cannot change the stored receiver. Callable variables and array elements whose descriptor was selected by an earlier runtime expression use the descriptor invoker when the local callsite no longer has static signature or capture metadata. Compile-time static-method callable arrays now materialize static-method descriptors for direct variable and literal calls, `call_user_func()` calls, and `call_user_func_array()` calls, including associative argument containers whose leftover string keys are forwarded into user-defined variadic method tails. Direct instance-method callable-array variable calls read the receiver from slot zero of the stored callable array, while direct instance-method callable-array literal calls evaluate the receiver before visible call arguments; both prepend it as descriptor argument zero and then let the descriptor signature normalize visible named/default/variadic/by-reference arguments. Direct invokable-object calls prepend the object as descriptor argument zero and use the object-invoke descriptor shape for the same named/default/variadic/by-reference handling, including non-local receiver expressions such as `(new Runner())(...)`. Compile-time instance-method callable arrays and invokable objects use instance/object descriptor shapes for direct `call_user_func()` calls and for `call_user_func_array()` calls whose argument container is literal indexed, literal associative, dynamic indexed, dynamic associative, or runtime-opaque mixed/union; the receiver is prepended as a synthetic descriptor argument before the visible callback arguments. Receiver-bound runtime-opaque containers branch on their boxed payload tag, clone and widen the payload to Mixed entries, then build a receiver-prefixed indexed array or associative hash for the descriptor invoker.

Extern callback trampolines use the same descriptor invoker from a C-facing entry point. Each extern callable callsite has a generated descriptor slot and trampoline symbol; the trampoline reloads the current descriptor, boxes incoming scalar/pointer C callback arguments as a temporary indexed `Mixed` array, invokes the descriptor, casts the boxed result to `int`, `float`, `bool`, `ptr`, or `void`, and returns through the target C ABI.

| Routine | What it does | Input | Output |
|---|---|---|---|
| `__rt_is_callable_string` | Resolve a string as a builtin, active user function, or `Class::method` static-method callable | `x1`/`x2` = string | `x0` = bool |
| `__rt_is_callable_method_name` | Check whether an object exposes a public method with the supplied name | object pointer + method string | `x0` = bool |
| `__rt_is_callable_static_method_name` | Check whether a class string exposes a public static method with the supplied name | class string + method string | `x0` = bool |
| `__rt_is_callable_object` | Check object callability through public `__invoke` metadata | object pointer | `x0` = bool |
| `__rt_is_callable_array` | Validate indexed callable arrays such as `[$obj, "method"]` or `[ClassName::class, "method"]` | array pointer | `x0` = bool |
| `__rt_is_callable_assoc` | Validate associative callable-array payloads produced through boxed or dynamic data paths | hash pointer | `x0` = bool |
| `__rt_is_callable_mixed` | Unbox a Mixed value and dispatch string, array, hash, or object callable checks | mixed pointer | `x0` = bool |
| `__rt_is_callable_heap` | Dispatch callable checks from a raw heap pointer by inspecting its heap-kind tag | heap pointer | `x0` = bool |
| `__rt_callable_descriptor_release` | Free a heap-backed callable descriptor copy plus the by-value capture slots appended after its static header; static `.data` descriptors are ignored | `x0` = descriptor pointer | — |

## Array routines

**Source:** `src/codegen/runtime/arrays/` (131 files)

### Core allocation

| Routine | What it does | Input | Output |
|---|---|---|---|
| `__rt_heap_alloc` | Free-list + bump allocator with a 16-byte `[size:4][refcount:4][kind:8]` header | `x0` = size | `x0` = pointer |
| `__rt_heap_free` | Return block to free list (bump reset if last block) | `x0` = pointer | — |
| `__rt_heap_free_safe` | Free only if pointer is in heap range | `x0` = pointer | — |
| `__rt_heap_debug_fail` | Print a heap-debug fatal error and terminate immediately | `x1` = msg ptr, `x2` = msg len | — |
| `__rt_heap_debug_check_live` | Reject `incref` / `decref` operations on already-freed heap blocks | `x0` = pointer | — |
| `__rt_heap_debug_validate_free_list` | Validate the ordered free list and small-bin chains before allocator mutations | — | — |
| `__rt_heap_debug_report` | Print heap-debug exit summary with leak/high-water stats | — | — |
| `__rt_heap_kind` | Return the uniform heap-kind tag for a heap-backed pointer | `x0` = pointer | `x0` = kind |
| `__rt_array_new` | Create indexed array with header | `x0` = capacity, `x1` = elem_size | `x0` = array ptr |
| `__rt_array_clone_shallow` | Clone indexed array storage for copy-on-write splitting, retaining nested heap children as needed | `x0` = array | `x0` = new array |
| `__rt_array_to_mixed` | Convert an indexed array's live slots to boxed Mixed cells and stamp the array metadata as mixed | `x0` = array | `x0` = same array |
| `__rt_array_ensure_unique` | Split a shared indexed array before mutation | `x0` = array | `x0` = unique array |
| `__rt_array_grow` | Ensure uniqueness, double array capacity, copy elements, free old unique storage | `x0` = array | `x0` = new array |
| `__rt_array_free_deep` | Free array storage and release nested heap-backed elements | `x0` = array | — |
| `__rt_array_union` | Build PHP indexed-array union: left numeric keys win, only missing right suffix keys are appended | `x0` = left array, `x1` = right array | `x0` = result array |
| `__rt_array_hash_union` | Build PHP indexed+associative union by converting left indexes to integer hash keys before appending missing right entries | `x0` = left array, `x1` = right hash | `x0` = result hash |
| `__rt_array_push_int` | Append int to array (grows if needed) | `x0` = array, `x1` = value | `x0` = array |
| `__rt_array_push_refcounted` | `incref` borrowed heap payload, then append it as an 8-byte array element | `x0` = array, `x1` = heap ptr | `x0` = array |
| `__rt_array_push_str` | Persist string + append to array (grows if needed) | `x0` = array, `x1`/`x2` = str | `x0` = array |
| `__rt_sort_int` / `__rt_rsort_int` | In-place sort ascending or descending | `x0` = array | — |
| `__rt_str_persist` | Copy string from concat_buf to heap (skips .data/heap) | `x1`/`x2` = str | `x1`/`x2` = heap str |

Common copy-producing array/hash routines now also have dedicated `_refcounted` siblings for nested heap-backed payloads. These variants retain borrowed values before pushing or inserting them into freshly allocated arrays/hash tables, covering array literals with spreads plus `array_merge`, `array_chunk`, `array_slice`, `array_reverse`, `array_pad`, `array_unique`, `array_splice`, `array_diff`, `array_intersect`, `array_filter`, `array_fill`, `array_combine`, and `array_fill_keys`.

| Refcounted sibling | What it does |
|---|---|
| `__rt_array_reverse_refcounted` | Reverse an indexed array while retaining nested heap-backed elements |
| `__rt_array_merge_refcounted` | Merge indexed arrays that carry nested heap-backed payloads |
| `__rt_array_slice_refcounted` / `__rt_array_splice_refcounted` | Slice or splice while retaining nested heap-backed payloads |
| `__rt_array_unique_refcounted` | Remove duplicates while preserving retained heap-backed elements |
| `__rt_array_fill_refcounted` / `__rt_array_fill_keys_refcounted` | Build filled arrays/hashes from borrowed heap-backed values |
| `__rt_array_pad_refcounted` | Pad an array with retained heap-backed values |
| `__rt_array_diff_refcounted` / `__rt_array_intersect_refcounted` | Set-style comparisons that keep nested heap-backed values alive |
| `__rt_array_combine_refcounted` | Combine key/value arrays into a hash while retaining heap-backed values |
| `__rt_array_chunk_refcounted` | Split an array into retained heap-backed chunks |
| `__rt_array_filter_refcounted` | Filter an array of heap-backed elements without dropping borrowed payloads; an optional third argument carries a captured-closure environment |
| `__rt_array_merge_into_refcounted` | Append one indexed array into another in-place while retaining nested heap-backed elements |

### Hash table (for associative arrays)

| Routine | What it does | Input | Output |
|---|---|---|---|
| `__rt_hash_fnv1a` | FNV-1a hash of string | `x1`/`x2` = string | `x0` = hash |
| `__rt_hash_normalize_key` | Normalize PHP string array keys, converting integer-form numeric strings to integer keys | `x1`/`x2` = string key | `x1`/`x2` = normalized key |
| `__rt_hash_key_hash` | Hash a normalized int/string array key | `x1`/`x2` = normalized key | `x0` = hash |
| `__rt_hash_key_eq` | Compare normalized int/string array keys | `x1`/`x2`, `x3`/`x4` = keys | `x0` = equal flag |
| `__rt_hash_new` | Create hash table | `x0` = capacity, `x1` = coarse value-type summary | `x0` = hash ptr |
| `__rt_hash_clone_shallow` | Clone hash storage for copy-on-write splitting, re-persisting keys and retaining nested heap values as needed | `x0` = hash | `x0` = new hash |
| `__rt_hash_ensure_unique` | Split a shared hash table before mutation | `x0` = hash | `x0` = unique hash |
| `__rt_hash_grow` | Double hash table capacity, rehash all entries | `x0` = hash | `x0` = new hash |
| `__rt_hash_set` | Insert/update (grows at 75% load) | `x0`=hash, `x1`/`x2`=normalized key, `x3`/`x4`=value, `x5`=value_tag | `x0` = hash |
| `__rt_hash_append` | Append with PHP's next automatic integer key (largest existing int key + 1, or 0), then delegate to `__rt_hash_set` | `x0`=hash, `x3`/`x4`=value, `x5`=value_tag | `x0` = hash |
| `__rt_hash_insert_owned` | Reinsert an already-owned key/value pair during hash growth | `x0`=hash, `x1`/`x2`=normalized key, `x3`/`x4`=value, `x5`=value_tag | `x0` = hash |
| `__rt_hash_get` | Look up value by key | `x0`=hash, `x1`/`x2`=normalized key | `x0`=found, `x1`=val_lo, `x2`=val_hi, `x3`=value_tag |
| `__rt_hash_iter_next` | Iterate to next entry in insertion order | `x0`=hash, `x1`=cursor | `x0`=next cursor, `x1`/`x2`=key, `x3`/`x4`=value, `x5`=value_tag |
| `__rt_hash_union` | Build PHP associative-array union: left duplicate keys win, missing right entries append in insertion order | `x0`=left hash, `x1`=right hash | `x0`=result hash |
| `__rt_hash_array_union` | Build PHP associative+indexed union by cloning the left hash and appending right indexes absent from the shared key space | `x0`=left hash, `x1`=right array | `x0`=result hash |
| `__rt_hash_count` | Count occupied entries | `x0`=hash | `x0`=count |
| `__rt_hash_free_deep` | Free a hash table plus owned keys and nested heap-backed values | `x0`=hash | — |
| `__rt_hash_to_mixed` | Copy-on-write a hash, then widen each entry payload into a boxed Mixed cell so by-reference `foreach` can alias a stable pointer slot | `x0`=hash | `x0` = same hash |
| `__rt_mixed_from_value` | Box a tagged payload into a heap-allocated mixed cell | `x0`=value_tag, `x1`=value_lo, `x2`=value_hi | `x0` = mixed cell |
| `__rt_mixed_write_stdout` | Print a boxed mixed value by inspecting its inner tag | `x0` = mixed cell | — |

`__rt_hash_iter_next` uses a small cursor protocol rather than a raw slot index: `0` starts from the hash header's `head`, positive cursors encode `slot_index + 1`, `-2` marks the post-tail state after yielding the final entry, and `-1` means iteration is exhausted.

See [Memory Model](memory-model.md) for the hash table memory layout.

### Array manipulation

| Routine | What it does |
|---|---|
| `__rt_array_key_exists` | Check if integer key is in bounds |
| `__rt_warn_undefined_array_key_int` | Emit PHP's `Undefined array key` warning for a missing integer key (warning-only; caller still supplies the null fallback) |
| `__rt_array_search` | Linear search for value in indexed array |
| `__rt_array_reverse` | Reverse element order |
| `__rt_array_sum` / `__rt_array_product` | Sum/product of all elements |
| `__rt_array_shift` / `__rt_array_unshift` | Remove/add at beginning |
| `__rt_array_merge` | Concatenate two indexed arrays into a new array |
| `__rt_array_merge_into` | Append all elements from source array into dest array (in-place) |
| `__rt_array_slice` / `__rt_array_splice` | Extract slices and remove splice windows from indexed arrays |
| `__rt_array_unique` | Remove duplicate values |
| `__rt_array_diff` / `__rt_array_intersect` | Set difference/intersection by value |
| `__rt_array_diff_key` / `__rt_array_intersect_key` | Set operations by key |
| `__rt_array_flip` | Swap indexed integer values into associative-array keys |
| `__rt_array_flip_string` | Swap indexed string values into associative-array keys, normalizing numeric-string keys |
| `__rt_array_combine` | Combine key array + value array → AssocArray |
| `__rt_array_fill` / `__rt_array_fill_keys` | Create filled arrays |
| `__rt_array_chunk` / `__rt_array_pad` | Chunk/pad arrays |
| `__rt_array_column` | Extract column from array of assoc arrays (int values) |
| `__rt_array_column_ref` | Extract column of retained heap-backed values (arrays / hashes / objects) |
| `__rt_array_column_str` | Extract column from array of assoc arrays (string values) |
| `__rt_array_column_mixed` | Extract column values as boxed Mixed cells for heterogeneous input payloads |
| `__rt_range` | Generate integer range array |
| `__rt_shuffle` / `__rt_array_rand` | Randomize order / pick random |
| `__rt_random_u32` / `__rt_random_uniform` | Target-aware random primitives used by `rand()`, `random_int()`, `shuffle()`, and `array_rand()` |
| `__rt_asort` / `__rt_arsort` | Sort by value while preserving keys, ascending or descending |
| `__rt_ksort` / `__rt_krsort` | Sort by key, ascending or descending |
| `__rt_natsort` / `__rt_natcasesort` | Natural-order sort, case-sensitive or case-insensitive |
| `__rt_array_map` | Apply callback to each scalar element, return new array; an optional third argument carries a captured-closure environment for generated callback wrappers |
| `__rt_array_map_str` | Apply callback to each scalar or string element and return a string array; an optional third argument carries a captured-closure environment |
| `__rt_array_map_str_owned` | Apply a descriptor-wrapper callback that returns owned strings and transfer those strings directly into the result array |
| `__rt_array_map_mixed` | Apply a descriptor-backed callback that returns owned boxed Mixed cells and store them directly into a newly allocated result array |
| `__rt_array_filter` | Filter scalar elements where callback returns truthy; an optional third argument carries a captured-closure environment |
| `__rt_array_reduce` | Reduce array to single value via callback; an optional fourth argument carries a captured-callback environment |
| `__rt_array_walk` | Call callback on each element (side-effects); an optional third argument carries a captured-callback environment |
| `__rt_usort` | Sort array using user comparison callback; an optional third argument carries a captured-callback environment |

### Reference counting

| Routine | What it does | Input | Output |
|---|---|---|---|
| `__rt_incref` | Increment reference count (safe with null/non-heap pointers) | `x0` = user pointer | — |
| `__rt_decref_any` | Release any heap-backed value by inspecting the uniform heap-kind tag | `x0` = pointer | — |
| `__rt_decref_array` | Decrement refcount, deep-free indexed array if zero | `x0` = array pointer | — |
| `__rt_decref_hash` | Decrement refcount, free hash table if zero | `x0` = hash pointer | — |
| `__rt_decref_mixed` | Decrement refcount, deep-free mixed cell if zero | `x0` = mixed pointer | — |
| `__rt_decref_object` | Decrement refcount, free object if zero | `x0` = object pointer | — |
| `__rt_gc_note_child_ref` | Add one transient incoming edge to a heap child during cycle counting | `x0` = child pointer | — |
| `__rt_gc_mark_reachable` | Recursively mark array/hash/object blocks reachable from external roots | `x0` = pointer | — |
| `__rt_gc_collect_cycles` | Run the targeted cycle collector over heap-backed arrays/hashes/objects | — | — |
| `__rt_mixed_free_deep` | Free a mixed cell and release any nested heap-backed payload | `x0` = mixed pointer | — |
| `__rt_object_free_deep` | Free an object and release heap-backed properties using runtime/class metadata | `x0` = object pointer | — |

Refcounts are stored as a 32-bit value in the uniform 16-byte heap header, at `[user_ptr - 12]`. Each heap allocation starts with refcount 1. When a reference is shared (e.g., assigned to another variable or passed to a function), `__rt_incref` bumps it. When the reference goes away, `__rt_decref_any` can dispatch through the uniform heap-kind tag to the concrete string/array/hash/object/mixed release path. Arrays, hashes, objects, and boxed mixed cells still use ordinary reference counting first, but when a decref sees a container/object graph that can contain nested heap-backed values, the runtime can invoke `__rt_gc_collect_cycles` to clear transient metadata, count heap-only incoming edges, mark externally reachable blocks, and deep-free the remaining unreachable array/hash/object/mixed island.

## System routines

**Source:** `src/codegen/runtime/system/` (34 top-level files plus `date/`, `strtotime/`, `json_validate/`, `json_decode_mixed/`, and `json_encode_str/` subdirectories)

### `__rt_build_argv` — Build $argv array

**File:** `system/build_argv.rs`

At program start, the OS passes `argc` (argument count) in `x0` and `argv` (pointer to C string pointers) in `x1`. This routine:

1. Creates a new string array
2. For each C string pointer in argv: measures the string length (scan for null byte), pushes ptr+len into the array
3. Returns the array pointer

### Core system routines

| Routine | What it does | Input | Output |
|---|---|---|---|
| `__rt_time` | Get current Unix timestamp via `gettimeofday` syscall | — | `x0` = seconds since epoch |
| `__rt_microtime` | Get current time as float seconds via `gettimeofday` syscall | — | `d0` = seconds.microseconds |
| `__rt_getenv` | Get environment variable value via libc `getenv()` | `x1`/`x2` = name string | `x1`/`x2` = value string |
| `__rt_php_uname` | Read target runtime system information via libc `uname()`; supports PHP modes `a`, `s`, `n`, `r`, `v`, and `m` | `x1`/`x2` = mode string | `x1`/`x2` = selected uname string |
| `__rt_shell_exec` | Execute shell command and capture output via libc `popen()`/`pclose()` | `x1`/`x2` = command string | `x1`/`x2` = output string |

## Exception routines

**Source:** `src/codegen/runtime/exceptions.rs` plus `src/codegen/runtime/exceptions/` (6 files)

elephc lowers exceptions with a small runtime layer around `_setjmp` / `_longjmp`. Codegen publishes the current exception object into `_exc_value`, pushes a handler record into `_exc_handler_top`, and then uses these helpers to unwind, match catch clauses, and resume control flow through `catch` / `finally`.

| Routine | What it does | Input | Output |
|---|---|---|---|
| `__rt_exception_cleanup_frames` | Walk the activation-record stack, run per-frame cleanup callbacks, and stop at the frame that should survive the catch | `x0` = surviving activation record | — |
| `__rt_exception_matches` | Check whether the active exception matches a catch target by class id or interface id | `x0` = exception object, `x1` = target id, `x2` = 0 for class / 1 for interface | `x0` = 1 if it matches, 0 otherwise |
| `__rt_instanceof_lookup` | Resolve a dynamic class-string `instanceof` target through emitted case-insensitive class/interface metadata | `x1`/`x2` = target string | `x0` = found flag, `x1` = target id, `x2` = 0 class / 1 interface |
| `__rt_instanceof_invalid_target` | Abort when a dynamic `instanceof` target is neither a string nor an object | — | does not return |
| `__rt_class_implements_interface` | Test class metadata against an interface id for dynamic class-string checks without an object instance | `x0` = class id, `x1` = interface id | `x0` = 1 if implemented, 0 otherwise |
| `__rt_throw_current` | Unwind to the nearest active handler or print the fatal uncaught-exception message and exit | reads `_exc_value`, `_exc_handler_top`, `_exc_call_frame_top` | does not return normally |
| `__rt_rethrow_current` | Re-enter the ordinary throw path with the currently active exception | none (uses global exception state) | does not return normally |

The fatal uncaught-exception path writes `Fatal error: uncaught exception` to stderr and exits with status 1. The runtime also resets the concat-buffer cursor before the final `longjmp`, so partially built string state from the throwing frame does not leak into the resumed catch/finally code.

### Date/time routines

**Files:** `system/date/`, `system/date_data.rs`, `system/mktime.rs`, `system/strtotime/`

| Routine | What it does | Input | Output |
|---|---|---|---|
| `__rt_date` | Format a Unix timestamp using PHP date format characters (Y, m, d, H, i, s, l, F, etc.). Uses `localtime_r()` from libc and static lookup tables (`_day_names`, `_month_names`) for day/month names | `x1`/`x2` = format string, `x0` = timestamp | `x1`/`x2` = formatted string |
| `__rt_mktime` | Create a Unix timestamp from date components (hour, minute, second, month, day, year). Populates a `tm` struct on the stack and calls libc `mktime()` | `x0`-`x5` = h, m, s, mon, day, year | `x0` = Unix timestamp |
| `__rt_strtotime` | Parse trimmed date/time strings through strategy emitters: ISO dates/datetimes, time-only forms, bare keywords (`now`, `today`, `tomorrow`, `yesterday`, `midnight`, `noon`), relative offsets (`+1 day`, `3 months ago`, `a/an <unit>` article forms), and named weekdays with `next` / `last` / `this`. Successful paths populate a `tm` struct and call libc `mktime()`; malformed input returns `-1`. | `x1`/`x2` = date string | `x0` = Unix timestamp or `-1` |

### JSON routines

**Files:** `system/json_data.rs`, `system/json_depth.rs`, `system/json_throw_error.rs`, `system/json_last_error_msg.rs`, `system/json_validate/`, `system/json_decode.rs`, `system/json_decode_mixed/`, `system/json_encode_bool.rs`, `system/json_encode_null.rs`, `system/json_encode_float.rs`, `system/json_encode_str/`, `system/json_encode_array_int.rs`, `system/json_encode_array_str.rs`, `system/json_encode_array_dynamic.rs`, `system/json_encode_assoc.rs`, `system/json_encode_mixed.rs`, `system/json_encode_object.rs`, `system/json_pretty.rs`, plus `objects/stdclass.rs` for stdClass-specific JSON object encoding.

The `json_encode` implementation uses **type-aware dispatch** — the codegen calls a different runtime routine depending on the compile-time type of the value being encoded:

| Routine | What it does | Input | Output |
|---|---|---|---|
| `__rt_json_encode_bool` | Encode bool as `"true"` or `"false"` using static data labels | `x0` = 0 or 1 | `x1`/`x2` = JSON string |
| `__rt_json_encode_null` | Encode null as `"null"` using a static data label | — | `x1`/`x2` = JSON string |
| `__rt_json_encode_str` | Encode a string with JSON escaping (quotes, backslashes, control chars) | `x1`/`x2` = input string | `x1`/`x2` = JSON string |
| `__rt_json_encode_array_int` | Encode an integer array as a JSON array (e.g., `[1,2,3]`) | `x0` = array ptr | `x1`/`x2` = JSON string |
| `__rt_json_encode_array_str` | Encode a string array as a JSON array with quoted elements | `x0` = array ptr | `x1`/`x2` = JSON string |
| `__rt_json_encode_array_dynamic` | Encode an indexed array by inspecting its packed runtime `value_type` tag at runtime (int, string, float, bool, nested array/hash, mixed, or null fallback), caching active JSON flags in a callee-saved register during the walk | `x0` = array ptr | `x1`/`x2` = JSON string |
| `__rt_json_encode_assoc` | Encode an associative array as a JSON object, while tracking PHP list-shape keys during the same hash walk and compacting to JSON array form when applicable | `x0` = hash ptr | `x1`/`x2` = JSON string |
| `__rt_json_encode_float` | Encode finite floats and record `JSON_ERROR_INF_OR_NAN` for `INF`/`NAN`, honoring throw and partial-output flags | `d0` = float | `x1`/`x2` = JSON string |
| `__rt_json_encode_mixed` | Encode a boxed mixed payload by unboxing its runtime tag and dispatching to the concrete JSON encoder | `x0` = mixed ptr | `x1`/`x2` = JSON string |
| `__rt_json_encode_object` | Encode class objects by consulting per-class JSON descriptors; dispatches `JsonSerializable::jsonSerialize()` when present, otherwise walks public properties | `x0` = object ptr | `x1`/`x2` = JSON string |
| `__rt_json_encode_stdclass` | Encode the dynamic-property hash backing `stdClass`, preserving `{}` for empty instances | `x0` = stdClass hash ptr | `x1`/`x2` = JSON string |
| `__rt_json_decode` | String-only compatibility helper used by string decode paths; trims outer whitespace and unescapes quoted JSON strings including surrogate-aware `\uXXXX` sequences | `x1`/`x2` = JSON string | `x1`/`x2` = decoded string |
| `__rt_json_decode_mixed` | Checked structural recursive decoder that returns boxed `Mixed` cells for null, bool, int, float, string, indexed arrays, associative arrays, and stdClass objects depending on `_json_decode_assoc`; records syntax/depth/UTF-8/UTF-16 errors plus source offsets and returns 0 on malformed input | `x1`/`x2` = JSON string | `x0` = Mixed* or 0 |
| `__rt_json_decode_mixed_array_real` | Recursive array parser used by `json_decode_mixed` once the outer `[` token is known | parser cursor + JSON bounds | boxed Mixed array |
| `__rt_json_decode_mixed_object_real` | Recursive object parser used by `json_decode_mixed` once the outer `{` token is known; returns assoc hash or stdClass payload based on decode mode | parser cursor + JSON bounds | boxed Mixed object/hash |
| `__rt_json_skip_ws` | Shared RFC 8259 whitespace skipper used by `json_decode_mixed` and its recursive array/object parsers; advances a caller-owned cursor to the next token or caller-supplied limit | JSON slice pointer, exclusive limit, cursor | updated cursor |
| `__rt_json_validate` | Standalone RFC 8259 validator used by `json_validate()`; scalar validator helpers are also reused by `json_decode_mixed` for strings and numbers | `x1`/`x2` = JSON string | `x0` = 1 valid / 0 invalid |
| `__rt_json_depth_enter` / `__rt_json_depth_exit` | Maintain `_json_active_depth` and compare against `_json_depth_limit` for recursive encode/decode/validate walks | global JSON state | status / updated state |
| `__rt_json_set_error_location` | Convert a decoder target pointer into one-based line/column data relative to `_json_error_source_ptr` | target pointer | updates `_json_error_location_active`, `_json_error_line`, `_json_error_column` |
| `__rt_json_error_message` | Build the current JSON error message, appending the PHP 8.6 `" near location line:column"` suffix when decode location state is active | global JSON state | `x1`/`x2` = message |
| `__rt_json_throw_error` | Record a JSON error code and construct/throw `JsonException` when `JSON_THROW_ON_ERROR` is active, using the shared formatted JSON error message | `x0` = JSON_ERROR_* code | may not return |
| `__rt_json_last_error_msg` | Return the message string corresponding to `_json_last_error` through the `_json_err_msg_table` data table, including decode location suffixes when active | global JSON state | `x1`/`x2` = message |
| `__rt_json_pretty_push` / `__rt_json_pretty_pop` / `__rt_json_pretty_line` / `__rt_json_pretty_colon_space` | Maintain `_json_indent_depth` and append PHP-style pretty-print whitespace while each container encoder emits bytes. These helpers are no-ops unless `JSON_PRETTY_PRINT` is active, avoiding a second buffer walk. | current JSON state, `x11` write pointer for line/space helpers | updated formatting state / `x11` write pointer |

### Regex routines

**Files:** `system/preg_strip.rs`, `system/pcre_to_posix.rs`, `system/preg_match.rs`, `system/preg_match_all.rs`, `system/preg_replace.rs`, `system/preg_replace_callback.rs`, `system/preg_split.rs`

All regex routines use PCRE2 through the PCRE2 POSIX-compatible wrapper (`pcre2_regcomp()`, `pcre2_regexec()`, and `pcre2_regfree()`). `__rt_preg_strip` strips PHP-style delimiters and maps supported modifiers (`i`, `m`, `s`, `u`, `U`) to PCRE2 wrapper flags. `__rt_pcre_to_posix` keeps its historic symbol name for compatibility with existing emitters, but now only materializes the stripped PCRE pattern as a null-terminated C string. Regex-enabled programs request `pcre2-posix` and `pcre2-8` during final linking.

| Routine | What it does | Input | Output |
|---|---|---|---|
| `__rt_preg_match` | Test if a regex matches the subject string. Compiles the pattern, executes once, frees | pattern + subject strings | `x0` = 1 (match) or 0 (no match) |
| `__rt_preg_match_capture` | Test once and materialize PHP's optional `$matches` array from the compiled `regex_t.re_nsub` capture count, omitting trailing unmatched captures while keeping interior unmatched captures as empty strings | pattern + subject strings | match flag plus matches array pointer |
| `__rt_preg_match_all` | Count all non-overlapping matches by repeatedly executing the regex with advancing offsets | pattern + subject strings | `x0` = match count |
| `__rt_preg_replace` | Replace all regex matches with a replacement string. Builds the result incrementally in the concat buffer and expands `$0`..`$99` / `\0`..`\99` from the PCRE2 capture vector | pattern + replacement + subject | `x1`/`x2` = result string |
| `__rt_preg_replace_callback` | Replace all regex matches by allocating capture storage from `regex_t.re_nsub`, building an indexed `$matches` string array, invoking the callback, and appending the callback string result while preserving concat-buffer state across callback prologues | pattern + callback + subject | `x1`/`x2` = result string |
| `__rt_preg_split` | Split the subject string at regex match boundaries using `regex_t.re_nsub`-sized capture storage. Applies limit, no-empty, delimiter-capture, and offset-capture flags; dynamic flags return boxed Mixed slots to preserve layout | pattern + subject strings, limit, flags | `x0` = array pointer |

## I/O routines

**Source:** `src/codegen/runtime/io/` (108 files)

These routines handle file and filesystem operations through target-aware libc/syscall helpers. PHP strings (pointer + length) must be converted to null-terminated C strings before passing to C or OS APIs — `__rt_cstr` handles the primary buffer and also emits `__rt_cstr2` for routines that need a second simultaneous C string.

The first table covers the file/filesystem core; the subsections after it cover the stream/networking surface emitted from the same directory: stream contexts and metadata, stream filters and user-defined stream wrappers, TCP/Unix/IPv6 sockets, TLS/SSL helpers, FTP/HTTP transfer helpers, hostname/service resolution, phar archives, and `var_dump`.

| Routine | What it does |
|---|---|
| `__rt_cstr` | Convert PHP string (ptr+len) to null-terminated C string |
| `__rt_fopen` | Open file via target-aware `open()` handling, or return `-1` after emitting a suppressible warning for open failures and invalid modes |
| `__rt_fgets` | Read line from file descriptor |
| `__rt_fgetc` | Read a single byte from a file descriptor (tail-calls `__rt_fread` with length 1) |
| `__rt_feof` | Check end-of-file flag for a file descriptor |
| `__rt_fread` | Read N bytes from file descriptor |
| `__rt_readfile` | Open a path, stream contents to stdout, and return copied byte count, `-1` on read failure, or a false sentinel on open failure |
| `__rt_fpassthru` | Stream the remaining bytes from an existing descriptor to stdout and return copied byte count or `-1` on read failure |
| `__rt_flock` | Call libc `flock()`, translating PHP's `LOCK_UN` constant and exposing would-block state for the optional output parameter |
| `__rt_tmpfile` | Create an anonymous temporary file descriptor through `mkstemp()` plus immediate unlink |
| `__rt_file_get_contents` | Read entire file into string, or return a null pointer after emitting a suppressible warning on failure |
| `__rt_file_put_contents` | Write string to file (create/truncate) |
| `__rt_file` | Read file into array of lines |
| `__rt_file_exists` / `__rt_is_file` / `__rt_is_dir` | Existence and path-type checks backed by `stat()` |
| `__rt_is_readable` / `__rt_is_writable` | Access checks backed by `access()` |
| `__rt_filesize` / `__rt_filemtime` | File size and modification timestamp from stat metadata |
| `__rt_fileatime` / `__rt_filectime` / `__rt_fileperms` / `__rt_fileowner` / `__rt_filegroup` / `__rt_fileinode` | Extended stat scalar metadata. Return a payload plus success flag so codegen can box PHP `false` without confusing legitimate zero values. |
| `__rt_filetype` / `__rt_is_executable` / `__rt_is_link` | File type and permission predicates; `filetype()` uses `lstat()` so symlinks report `"link"` and missing paths box as `false`. |
| `__rt_stat_array` / `__rt_lstat_array` / `__rt_fstat_array` | Build PHP-compatible stat arrays with numeric and string keys, returning a null pointer for codegen to box as `false` on failure |
| `__rt_unlink` / `__rt_mkdir` / `__rt_rmdir` / `__rt_chdir` | Filesystem path operations via libc/syscalls |
| `__rt_rename` / `__rt_copy` | Two-path filesystem helpers using dual C-string scratch buffers |
| `__rt_symlink` / `__rt_link` | Create symbolic or hard links through libc |
| `__rt_readlink` | Read a symbolic-link target into a heap-backed string, with null output for PHP `false` on failure |
| `__rt_linkinfo` | Return `lstat()` device metadata for a link path, or PHP's `-1` failure sentinel |
| `__rt_getcwd` | Get current working directory |
| `__rt_scandir` | List directory contents into array |
| `__rt_glob` | Pattern-match filenames |
| `__rt_tempnam` | Create temporary filename |
| `__rt_fgetcsv` | Parse CSV line from file |
| `__rt_fputcsv` | Write CSV line to file |
| `__rt_basename` / `__rt_dirname` / `__rt_dirname_levels` | Compute path components for `basename()` / `dirname()` including repeated parent traversal |
| `__rt_fnmatch` | Match shell-style path globs with PHP/libc-compatible flag bits for the selected target |
| `__rt_realpath` | Canonicalize an existing path, returning a null pointer on failure so codegen can box PHP `false` |
| `__rt_pathinfo_str` / `__rt_pathinfo_array` | Return one `pathinfo()` component for component flags, or build the associative-array `PATHINFO_ALL` shape |
| `__rt_chmod` / `__rt_chown` / `__rt_lchown` / name-resolving variants | File ownership and mode modification helpers, including symlink-aware ownership updates |
| `__rt_umask` / `__rt_ftruncate` | Process umask and file truncation helpers |
| `__rt_fsync` / `__rt_fflush` / `__rt_fdatasync` | File descriptor flush helpers; `fflush()` maps to `fsync()` because elephc has no userspace stdio buffer |
| `__rt_touch` | Create missing files and update access/modification timestamps |

### Stream and socket routines

| Routine | What it does |
|---|---|
| `__rt_stream_socket_client` / `__rt_stream_socket_client_v6` | Open TCP client connections (IPv4/IPv6) with timeout handling |
| `__rt_stream_socket_server` / `__rt_stream_socket_server_v6` | Bind and listen on TCP server sockets (IPv4/IPv6) |
| `__rt_unix_socket_client` / `__rt_unix_socket_server` | Unix-domain socket client/server endpoints |
| `__rt_stream_socket_accept` | Accept a pending connection with optional timeout |
| `__rt_stream_socket_pair` | Create a connected socket pair |
| `__rt_stream_socket_recvfrom` / `__rt_stream_socket_sendto` | Datagram receive/send with peer-address formatting |
| `__rt_stream_socket_get_name` | Local or remote endpoint name for a socket |
| `__rt_stream_socket_shutdown` | Half/full shutdown of a connected socket |
| `__rt_socket_backlog` / `__rt_apply_socket_bindto` / `__rt_apply_socket_client_opts` / `__rt_apply_socket_server_opts` | Socket-option plumbing for context-driven behavior |
| `__rt_stream_select` | `stream_select()` over descriptor arrays via `poll()`/`select()` |
| `__rt_stream_set_blocking` / `__rt_stream_set_timeout` | Per-descriptor blocking mode and read timeout |
| `__rt_stream_get_contents` / `__rt_stream_get_contents_bounded` / `__rt_stream_get_line` | Bulk and line-delimited stream reads |
| `__rt_stream_copy_to_stream` | Copy bytes between two descriptors |
| `__rt_stream_get_meta_data` | Build the `stream_get_meta_data()` associative array |
| `__rt_stream_context_set_option_4` | Store context options consumed by the open/transfer helpers |
| `__rt_stream_isatty` | TTY detection for descriptors |
| `__rt_data_stream` | `data://` stream payload decoding |
| `__rt_get_ssl_peer_name` | Peer-certificate name lookup for TLS streams |

### Networking and transfer routines

| Routine | What it does |
|---|---|
| `__rt_http_open` / `__rt_https_open` | Open `http://` / `https://` streams (TLS via the elephc-tls bridge) |
| `__rt_http_build_request` | Assemble the HTTP request from context options (method, headers, body, `request_fulluri`) |
| `__rt_http_fire_notification` | Invoke the stream-notification callback during transfers |
| `__rt_ftp_open` / `__rt_ftp_send_recv` / `__rt_ftp_parse_pasv` | `ftp://` stream support (control dialog, passive-mode parsing) |
| `__rt_resolve_host` / `__rt_resolve_host_v6` | Hostname resolution to IPv4/IPv6 addresses |
| `__rt_gethostbyname` / `__rt_gethostbyaddr` / `__rt_gethostname` | Host lookup builtins |
| `__rt_getprotobyname` / `__rt_getprotobynumber` / `__rt_protoent_load` | Protocol-database lookups backed by `/etc/protocols` |
| `__rt_getservbyname` / `__rt_getservbyport` | Service-database lookups backed by `/etc/services` |

### User stream wrappers and filters

Userspace `streamWrapper` classes registered with `stream_wrapper_register()` dispatch through a vtable of `__rt_user_wrapper_*` routines (`fopen`/`fread`/`fwrite`/`fclose`/`feof`/`fseek`/`ftell`/`fflush`/`fstat`/`ftruncate`/`flock`/`set_option`/`stream_cast`, the `dir_*` family, `path_op`, and `rename`), each bridging the synthetic descriptor back to PHP method calls on the wrapper instance. Stream filters use `__rt_stream_filter_register`, `__rt_apply_stream_filter` / `__rt_apply_user_stream_filter`, `__rt_stream_filter_attach_user`, `__rt_resolve_user_filter_id`, `__rt_user_filter_brigade_invoke`, and `__rt_user_filter_release_fd` to run built-in (`zlib.*`, `bzip2.*`, `convert.iconv.*`, `string.*`) and user-defined filter chains over stream reads and writes.

### Phar archive routines

| Routine | What it does |
|---|---|
| `__rt_fopen_maybe_phar` / `__rt_file_get_contents_maybe_phar` | Route dynamic `phar://` read paths to archive entry reads and write-mode `fopen()` paths to PHAR write streams, falling through to plain file I/O otherwise |
| `__rt_phar_read_entry` | Locate and read one entry from a PHAR URL. When the `elephc-phar` bridge is published it handles native PHAR, tar, and ZIP containers; the assembly fallback handles native PHAR plus gzip/bzip2 payloads through published zlib/libbz2 slots |
| `__rt_phar_write_open` / `__rt_phar_write_open_url` / `__rt_phar_write_append` / `__rt_phar_write_finalize` / `__rt_file_put_contents_maybe_phar` | Buffer `phar://` write entries in bridge-owned descriptor slots, then finalize each through the `elephc-phar` bridge so native PHAR, tar, and ZIP archives preserve existing entries; runtime-built `file_put_contents()` and `fopen()` write URLs call a bridge variant that splits the full `phar://` URL; the assembly fallback still emits a single-entry SHA1-signed native archive |

### var_dump output routines

`var_dump()` lowering calls a family of `__rt_var_dump_array_*` routines (`int`, `float`, `str`, `bool`, `mixed`) that walk array payloads and a set of `__rt_var_dump_emit_*` helpers that print one typed line (`int(...)`, `float(...)`, `bool(...)`, string headers, indexed keys) with the PHP-compatible indentation.

## Pointer routines

**Source:** `src/codegen/runtime/pointers/` (7 files including `mod.rs`)

These helpers support the compiler-specific pointer builtins.

| Routine | What it does | Input | Output |
|---|---|---|---|
| `__rt_ptoa` | Format a pointer value as a hexadecimal string with `0x` prefix | `x0` = pointer/address | `x1`/`x2` = formatted string |
| `__rt_ptr_check_nonnull` | Abort with `Fatal error: null pointer dereference` if the pointer is null | `x0` = pointer/address | `x0` unchanged on success |
| `__rt_str_to_cstr` | Copy an elephc string to temporary null-terminated heap storage for a native call | `x1`/`x2` = string | `x0` = C string pointer |
| `__rt_cstr_to_str` | Copy a borrowed null-terminated C string back into an owned elephc string | `x0` = C string pointer | `x1`/`x2` = elephc string |
| `__rt_ptr_read_string` | Copy a fixed-length byte range from a raw pointer into an owned elephc string | `x0` = pointer, `x1` = length | `x1`/`x2` = elephc string |
| `__rt_ptr_write_string` | Copy an elephc string's bytes into the memory addressed by a raw pointer | `x0` = pointer, `x1`/`x2` = string | — |

## Buffer routines

**Source:** `src/codegen/runtime/buffers/` (5 files including `mod.rs`)

These helpers support the compiler-specific `buffer<T>` hot-path data type.

| Routine | What it does | Input | Output |
|---|---|---|---|
| `__rt_buffer_new` | Allocate a contiguous buffer with header `[length:8][stride:8]` followed by zero-initialized payload | `x0` = element count, `x1` = element stride | `x0` = buffer pointer |
| `__rt_buffer_len` | Read the logical element count from a buffer header | `x0` = buffer pointer | `x0` = length |
| `__rt_buffer_bounds_fail` | Abort with `Fatal error: buffer index out of bounds` | — | does not return |
| `__rt_buffer_use_after_free` | Abort with `Fatal error: use of buffer after buffer_free()` | — | does not return |

## Mixed-type helpers

| Routine | What it does | Input | Output |
|---|---|---|---|
| `__rt_mixed_cast_int` | Unbox a mixed cell and cast to integer | `x0` = mixed cell pointer | `x0` = integer |
| `__rt_mixed_cast_bool` | Unbox a mixed cell and cast to boolean | `x0` = mixed cell pointer | `x0` = 0 or 1 |
| `__rt_mixed_cast_float` | Unbox a mixed cell and cast to float | `x0` = mixed cell pointer | `d0` = float |
| `__rt_mixed_cast_string` | Unbox a mixed cell and cast to string | `x0` = mixed cell pointer | `x1`/`x2` = string |
| `__rt_mixed_instanceof` | Unbox a mixed cell and test object payloads against class/interface metadata | `x0` = mixed cell pointer, `x1` = target id, `x2` = 0 class / 1 interface | `x0` = 0 or 1 |
| `__rt_instanceof_lookup` | Resolve a dynamic class-string target against emitted class/interface name metadata | `x1`/`x2` = string | `x0` = found, `x1` = target id, `x2` = 0 class / 1 interface |
| `__rt_mixed_is_empty` | Check emptiness of a mixed cell (PHP semantics) | `x0` = mixed cell pointer | `x0` = 0 or 1 |
| `__rt_mixed_strict_eq` | Compare two mixed cells by tag and value | `x0`, `x1` = mixed pointers | `x0` = 0 or 1 |
| `__rt_mixed_unbox` | Extract the raw payload from a mixed cell | `x0` = mixed cell pointer | `x0`/`x1`/`x2` depending on type |
| `__rt_mixed_count` | Count boxed indexed arrays and hashes, returning zero for non-countable payloads | `x0` = mixed cell pointer | `x0` = count |
| `__rt_iterable_write_stdout` | Print iterable arrays and hashes as PHP's `"Array"` display string | `x0` = iterable heap pointer | — |
| `__rt_iterable_unsupported_kind` | Abort when runtime iterable dispatch sees an unsupported heap kind | — | does not return |
| `__rt_hash_may_have_cyclic_values` | Scan hash entries to check if any contain refcounted children | `x0` = hash pointer | `x0` = 0 (scalar-only) or 1 (has cycles) |
| `__rt_match_unhandled` | Abort with `Fatal error: unhandled match case` | — | does not return |

## Object and stdClass routines

**Source:** `src/codegen/runtime/objects/` (6 files)

These helpers support `stdClass`, `json_decode()` object results, boxed Mixed property/index access, object destructor dispatch, and dynamic `new $name()` instantiation. `stdClass` instances use a compact `[class_id][hash_ptr]` payload, with dynamic properties stored in a hash of boxed `Mixed` values.

| Routine | What it does | Input | Output |
|---|---|---|---|
| `__rt_new_by_name` | Instantiate a class by its textual name through the `_classes_by_name` table (case-insensitive `__rt_strcasecmp` lookup), allocating and zeroing the object payload | class name string | object pointer, or 0 (null) on miss |
| `__rt_call_object_destructor` | Look up the object's `__destruct` in the class_id-indexed `_class_destruct_ptrs` table and invoke it with `$this` borrowed before storage is released; guarded against re-entry | object pointer | — |
| `__rt_stdclass_new` | Allocate an empty stdClass object with hash-backed dynamic property storage | stdClass class id from runtime data | object pointer |
| `__rt_stdclass_from_hash` | Wrap a decoded JSON object hash in a stdClass instance | hash pointer | object pointer |
| `__rt_stdclass_get` | Read a dynamic property and return a boxed Mixed value, or Mixed(null) when missing | object pointer + property string | boxed `mixed` payload |
| `__rt_stdclass_set` | Store a boxed Mixed value into a dynamic property hash | object pointer + property string + boxed value | — |
| `__rt_mixed_property_get` | Unbox a Mixed object payload and dispatch stdClass property reads | boxed `mixed` + property string | boxed `mixed` payload |
| `__rt_mixed_property_set` | Unbox a Mixed object payload and dispatch stdClass property writes | boxed `mixed` + property string + boxed value | — |
| `__rt_mixed_array_get` | Unbox Mixed array/hash/stdClass payloads for `$mixed[$key]` access | boxed `mixed` + normalized key tuple | boxed `mixed` payload |
| `__rt_mixed_array_set` | Unbox a Mixed indexed-array/hash payload and write a boxed Mixed value for `$mixed[$key] = ...`; consumes the boxed value on success | boxed `mixed` + normalized key tuple + boxed value | — |
| `__rt_json_encode_stdclass` | Encode the dynamic-property hash backing stdClass as a JSON object | stdClass hash pointer | `x1`/`x2` = JSON string |

## SPL and iterable routines

**Source:** `src/codegen/runtime/spl/` (3 files including `mod.rs`)

These helpers back SPL container classes whose PHP surface needs custom runtime storage. `SplDoublyLinkedList` (and its `SplStack` / `SplQueue` subclasses) store a class id, an owned indexed array of boxed `Mixed` cells, an iterator index, and iterator-mode bits. `SplFixedArray` stores a class id and a fixed-size storage array of owned boxed `Mixed` cells (or null for unset/null slots). Mutating methods take ownership of the boxed `Mixed` arguments prepared by call lowering, and resize/overwrite paths release any replaced cell first.

### SplDoublyLinkedList / SplStack / SplQueue

| Routine | What it does |
|---|---|
| `__rt_spl_dll_new` | Allocate an empty doubly-linked-list object with initial mixed-cell storage |
| `__rt_spl_dll_push` / `__rt_spl_dll_pop` | Append to / remove from the end |
| `__rt_spl_dll_unshift` / `__rt_spl_dll_shift` | Prepend to / remove from the front |
| `__rt_spl_dll_top` / `__rt_spl_dll_bottom` | Peek the last / first element |
| `__rt_spl_dll_insert` | Insert at an index honoring the iterator mode |
| `__rt_spl_dll_count` / `__rt_spl_dll_is_empty` | Element count / emptiness check |
| `__rt_spl_dll_set_iterator_mode` / `__rt_spl_dll_get_iterator_mode` | Write / read LIFO/FIFO and DELETE iterator-mode bits |
| `__rt_spl_dll_rewind` / `__rt_spl_dll_valid` / `__rt_spl_dll_current` / `__rt_spl_dll_key` / `__rt_spl_dll_next` / `__rt_spl_dll_prev` | Iterator surface honoring the active iterator mode |
| `__rt_spl_dll_offset_exists` / `__rt_spl_dll_offset_get` / `__rt_spl_dll_offset_set` / `__rt_spl_dll_offset_unset` | `ArrayAccess` operations |
| `__rt_spl_dll_serialize` / `__rt_spl_dll_serialize_array` / `__rt_spl_dll_unserialize` | Serialization helpers |

### SplFixedArray

| Routine | What it does |
|---|---|
| `__rt_spl_fixed_new` | Allocate a fixed-size array object with a zero-initialized mixed-cell storage block |
| `__rt_spl_fixed_count` | Return the fixed size |
| `__rt_spl_fixed_set_size` | Resize storage, releasing dropped cells and zero-filling new slots |
| `__rt_spl_fixed_offset_exists` / `__rt_spl_fixed_offset_get` / `__rt_spl_fixed_offset_set` / `__rt_spl_fixed_offset_unset` | `ArrayAccess` operations with bounds checking |
| `__rt_spl_fixed_to_array` / `__rt_spl_fixed_from_array` / `__rt_spl_fixed_copy_from_array` | Convert to / build from a PHP array |
| `__rt_spl_fixed_unserialize` | Serialization helper |

## Generator routines

**Source:** `src/codegen/runtime/generators/` (2 files)

These helpers back the built-in `Generator` class. Generator functions emit a heap-allocated frame and a generated resume function; the runtime helpers read/write that frame for the public Iterator surface and coroutine operations.

| Routine | What it does | Input | Output |
|---|---|---|---|
| `__rt_gen_current` | Return an owned ref to the boxed Mixed value from the most recent yield | `GeneratorFrame*` | boxed `mixed` payload |
| `__rt_gen_key` | Return an owned ref to the boxed Mixed key from the most recent yield | `GeneratorFrame*` | boxed `mixed` key |
| `__rt_gen_valid` | Report whether the generator is not terminated | `GeneratorFrame*` | bool |
| `__rt_gen_next` | Resume the state machine past the current yield unless terminated | `GeneratorFrame*` | — |
| `__rt_gen_next_done` | Shared global return label used after `next()` skips or completes a resume | `GeneratorFrame*` | — |
| `__rt_gen_send` | Store a boxed Mixed sent value, then resume the state machine | `GeneratorFrame*`, boxed `mixed` value | boxed `mixed` payload |
| `__rt_gen_send_done` | Shared global return label used after `send()` skips or completes a resume | `GeneratorFrame*` | boxed `mixed` payload |
| `__rt_gen_send_epilogue` | Shared epilogue that boxes and returns the yield produced by a resumed `send()` | `GeneratorFrame*` | boxed `mixed` payload |
| `__rt_gen_rewind` | Run the generator to its first yield once | `GeneratorFrame*` | — |
| `__rt_gen_rewind_done` | Shared global return label used when `rewind()` has already run or just finished | `GeneratorFrame*` | — |
| `__rt_gen_throw` | Mark the generator terminated and throw through the normal exception runtime | `GeneratorFrame*`, throwable object | does not return |
| `__rt_gen_get_return` | Return an owned ref to the boxed terminal return value | `GeneratorFrame*` | boxed `mixed` payload |

Generator frames are stamped as object heap blocks because `Generator` is a built-in class implementing `Iterator`. `__rt_object_free_deep` detects the built-in Generator class id and releases the frame's custom Mixed slots plus any active `yield from` delegate instead of treating the payload as ordinary class properties.

## Fiber routines

**Source:** `src/codegen/runtime/fibers/` (4 files plus the `api/` subdirectory)

These helpers implement PHP 8.1-style cooperative coroutines. They are emitted by the shared runtime on every supported target.

| Routine | What it does | Input | Output |
|---|---|---|---|
| `__rt_fiber_alloc_stack` | Allocate a per-fiber native stack with a protected guard page | requested usable stack size | stack base, initial top, mapped size |
| `__rt_fiber_free_stack` | Return a mapped fiber stack to the OS | stack base, mapped size | — |
| `__rt_fiber_switch` | Save the current callee-saved context and restore the target fiber/main context | target `Fiber*` or null for main | resumes when this context is switched back to |
| `__rt_fiber_entry` | Trampoline run on first entry to a fiber stack; calls the generated wrapper, records return/escape state, and switches back | active `_fiber_current` | does not return normally inside the fiber |
| `__rt_fiber_construct` | Allocate and initialize the runtime-managed Fiber object and its initial stack frame | callable descriptor pointer, `Fiber` class id, generated wrapper pointer | `Fiber*` |
| `__rt_fiber_throw_state_error` | Allocate a `FiberError` and throw it through the normal exception runtime | message pointer and length | does not return |
| `__rt_fiber_start` | Start a not-yet-started fiber and return its first yielded value or null on immediate termination | `Fiber*` | boxed `mixed` payload |
| `__rt_fiber_resume` | Resume a suspended fiber with a boxed payload | `Fiber*`, boxed `mixed` value | boxed `mixed` payload |
| `__rt_fiber_suspend` | Suspend the current fiber and yield a boxed payload to its caller | boxed `mixed` value | boxed `mixed` value supplied by the next resume |
| `__rt_fiber_throw` | Resume a suspended fiber by throwing into its pending suspend point | `Fiber*`, throwable object | boxed `mixed` payload or rethrown exception |
| `__rt_fiber_get_current` | Return the currently running fiber, or null when running on the main stack | — | boxed `mixed` payload |
| `__rt_fiber_get_return` | Read the terminal return payload from a terminated fiber | `Fiber*` | boxed `mixed` payload |
| `__rt_fiber_state_eq` | Shared predicate helper for `isStarted()`, `isSuspended()`, `isRunning()`, and `isTerminated()` | `Fiber*`, state id | bool |

## How routines are emitted

**File:** `src/codegen/runtime/emitters.rs`

The `emit_runtime()` function calls the target-aware routine emitters in a fixed order. Each runtime module owns the shared helper surface and dispatches internally when AArch64 and Linux `x86_64` need different instruction sequences or ABI setup.

```rust
pub fn emit_runtime(emitter: &mut Emitter) {
    // diagnostics: runtime warning emission and @ suppression state
    // strings: itoa, resource display/stdout, ftoa, concat, atoi, equality, formatting, trim/mask,
    // search/replace, explode/implode, hashing, encoding, sscanf, ...
    // callables: dynamic is_callable() fallback plus callable-descriptor release
    // system: argv, time, getenv, shell, date/mktime/strtotime, JSON, regex
    // exceptions: cleanup walk, catch matching, class-implements, throw/rethrow helpers
    // generators: Generator current/key/valid/next/send/rewind/throw/getReturn helpers
    // arrays: heap alloc/free, array/hash helpers, sort, callbacks, refcount
    // spl: SplDoublyLinkedList/SplStack/SplQueue and SplFixedArray storage helpers
    // objects: stdClass dynamic properties and boxed Mixed property/index dispatch
    // buffers: contiguous buffer allocation, bounds checking, UAF traps
    // io: c-string buffers, file I/O, stat/fs helpers, scandir/glob/tempnam, CSV
    // pointers: ptoa, null check, str_to_cstr, cstr_to_str
    // fibers: guarded stack allocation, context switch, entry trampoline, Fiber API
}
```

Notable runtime-only helpers emitted here include `__rt_diag_push_suppression`, `__rt_diag_pop_suppression`, `__rt_diag_warning`, `__rt_exception_cleanup_frames`, `__rt_exception_matches`, `__rt_instanceof_lookup`, `__rt_instanceof_invalid_target`, `__rt_throw_current`, `__rt_heap_debug_fail`, `__rt_heap_kind`, `__rt_hash_insert_owned`, `__rt_hash_free_deep`, `__rt_array_column_ref`, `__rt_mixed_instanceof`, `__rt_iterable_write_stdout`, `__rt_iterable_unsupported_kind`, `__rt_class_implements_interface`, `__rt_callable_descriptor_release`, `__rt_spl_dll_new`, `__rt_spl_fixed_new`, `__rt_gen_current`, `__rt_gen_send`, `__rt_preg_strip`, `__rt_pcre_to_posix`, `__rt_str_to_cstr`, `__rt_cstr_to_str`, `__rt_fiber_switch`, and `__rt_fiber_entry` in addition to the more user-visible helpers.

Every routine in the selected target runtime slice is linked into the binary, even if unused by the current program. elephc already does AST-side control-flow pruning and dead-code elimination before codegen, but runtime-specific dead stripping is still future work.

The runtime can also be emitted in **position-independent mode** for `--emit cdylib` builds: the emitter's `pic_data_refs` flag makes the `abi::symbols` helpers route every global data reference through the GOT (`@GOTPCREL` on x86_64, `:got:`/`:got_lo12:` on AArch64) instead of direct PC-relative addressing, and on ELF targets every internal global gets a `.hidden` visibility directive. The PIC and non-PIC variants produce different assembly text, so they cache as separate runtime objects. See [The Codegen](the-codegen.md) and [Shared Libraries](../beyond-php/cdylib.md).

## Runtime data

The runtime data layer lives in `src/codegen/runtime/data/`. `fixed.rs` emits shared buffers, error strings, and lookup tables; `user.rs` emits per-program globals, statics, enum-case slots, and metadata tables; `instanceof.rs` formats dynamic `instanceof` lookup names. Together they declare global buffers using `.comm` and static data tables:

```asm
.comm _concat_buf, 65536     ; 64KB string buffer
.comm _concat_off, 8         ; current offset into string buffer
.comm _global_argc, 8        ; saved argc from OS
.comm _global_argv, 8        ; saved argv pointer from OS
.comm _exc_handler_top, 8    ; top of the active exception-handler stack
.comm _exc_call_frame_top, 8 ; top of the activation-record cleanup stack
.comm _exc_value, 8          ; currently propagating exception object
.comm _fiber_current, 8      ; currently running Fiber object, or null on main
.comm _fiber_main_saved_sp, 8 ; saved main-stack pointer while running a fiber
.comm _fiber_main_saved_exc, 8 ; saved main exception-handler chain while running a fiber
.comm _fiber_main_saved_call_frame, 8 ; saved main cleanup-frame chain while running a fiber
.comm _rt_diag_suppression, 8 ; nested runtime warning-suppression depth for @
.comm _heap_buf, 8388608     ; 8MB heap by default (--heap-size overrides)
.comm _heap_off, 8           ; current heap offset
.comm _heap_free_list, 8     ; head of the general address-ordered free list
.comm _heap_small_bins, 32   ; 4 x 8-byte heads for <=8/16/32/64-byte cached blocks
.comm _heap_debug_enabled, 8 ; BSS-backed debug flag, set to 1 in _main when compiled with --heap-debug
.comm _gc_collecting, 8      ; cycle collector re-entry guard
.comm _gc_release_suppressed, 8 ; suppress nested collection during deep frees
.comm _json_last_error, 8    ; last JSON_ERROR_* code
.comm _json_active_flags, 8  ; active JSON flags for encode/decode/validate
.comm _json_active_depth, 8  ; current recursive JSON container depth
.comm _json_indent_depth, 8  ; current JSON_PRETTY_PRINT formatting depth
.comm _json_depth_limit, 8   ; configured JSON depth limit
.comm _json_validate_idx, 8  ; validator cursor index
.comm _json_validate_ptr, 8  ; validator input pointer
.comm _json_validate_len, 8  ; validator input length
.comm _json_decode_assoc, 8  ; json_decode object-shape selector
.comm _json_error_source_ptr, 8 ; json_decode input pointer for error locations
.comm _json_error_location_active, 8 ; whether line/column should be appended
.comm _json_error_line, 8    ; one-based line for the last json_decode error
.comm _json_error_column, 8  ; one-based column for the last json_decode error
_heap_max:
    .quad 8388608            ; configured heap size limit
.comm _gc_allocs, 8          ; allocation counter
.comm _gc_frees, 8           ; free counter
.comm _gc_live, 8            ; current live heap footprint in bytes
.comm _gc_peak, 8            ; high-water mark counter
.comm _cstr_buf, 4096        ; 4KB C-string conversion buffer
.comm _cstr_buf2, 4096       ; 4KB second C-string buffer
.comm _eof_flags, 256        ; EOF flag per file descriptor
; Per-program: global variable storage (one per `global $var` used)
.comm _gvar_x, 16            ; 16 bytes per global variable
; Per-program: static variable storage (one pair per `static $var`)
.comm _static_func_var, 16   ; 16 bytes for persisted value
.comm _static_func_var_init, 8 ; 8-byte initialization flag
; Per-program: static property storage (one slot per effective declaring class property)
.comm _static_prop_Class_prop, 16 ; 16 bytes for the static property value
```

Additionally, the runtime emits static data tables:
- `_fmt_g` — printf format string for float-to-string conversion via `%.14G`
- `_b64_encode_tbl` — 64-byte Base64 encoding lookup table
- `_b64_decode_tbl` — 256-byte Base64 decoding lookup table
- `_spl_autoload_exts_default`, `_spl_autoload_exts_ptr`, `_spl_autoload_exts_len` — mutable SPL autoload extension state
- `_heap_err_msg`, `_arr_cap_err_msg`, `_ptr_null_err_msg` — fatal runtime error strings
- `_buffer_bounds_msg`, `_buffer_uaf_msg`, `_match_unhandled_msg`, `_static_prop_private_access_msg`, `_instanceof_target_type_msg`, `_iterable_unsupported_kind_msg` — fatal runtime error strings for buffers, `match`, late-bound private static-property access, dynamic `instanceof` target validation, and iterable dispatch
- `_heap_dbg_bad_refcount_msg`, `_heap_dbg_double_free_msg`, `_heap_dbg_free_list_msg` — fatal heap-debug error strings enabled by `--heap-debug`
- `_heap_dbg_*` summary labels — fixed strings used by `__rt_heap_debug_report` for alloc/free/live/leak output
- `_resource_id_prefix` — prefix used by resource display helpers
- `_uncaught_exc_msg` — fatal exception string written by `__rt_throw_current` when no handler exists
- `_diag_fopen_failed_msg`, `_diag_file_get_contents_failed_msg`, `_diag_define_already_defined_msg` — suppressible runtime warning text routed through `__rt_diag_warning`
- `_fiber_msg_already_started`, `_fiber_msg_not_suspended`, `_fiber_msg_throw_not_suspended`, `_fiber_msg_not_terminated`, `_fiber_msg_suspend_outside`, `_fiber_msg_unsupported_callable`, `_fiber_msg_stack_alloc_failed` — messages used by `FiberError` runtime paths
- `_fiber_class_id`, `_fiber_error_class_id` — per-program class ids used by Fiber object cleanup and `FiberError` construction
- `_generator_class_id` — per-program class id used to recognize Generator frames during object deep-free
- `_php_uname_mode_len_msg`, `_php_uname_mode_value_msg` — fatal `php_uname()` argument diagnostics for invalid mode strings
- `_filetype_*`, `_stat_key_*`, `_dirname_*`, `_pathinfo_key_*`, `_tmpfile_template` — file metadata, path, stat-array, and temporary-file lookup strings used by I/O helpers
- `_locale_utf8_name`, `_locale_env_name` — locale selectors used by runtime helpers that need host locale fallback
- `_json_true`, `_json_false`, `_json_null` — JSON keyword strings used by `__rt_json_encode_bool` and `__rt_json_encode_null`
- `_json_int_max_str`, `_json_int_min_str` — decimal threshold strings used by `JSON_BIGINT_AS_STRING` overflow detection without wrapping through integer parsing
- `_json_err_msg_0` ... `_json_err_msg_10`, `_json_err_msg_table`, `_json_err_msg_count`, `_json_err_loc_prefix`, `_json_err_loc_colon` — `json_last_error_msg()` lookup data and location-suffix fragments for the supported `JSON_ERROR_*` code range
- `_day_names` — 7 entries (84 bytes), each 12 bytes: day name padded to 10 chars + 1 length byte + 1 padding byte. Used by `__rt_date` for `l` (full name) and `D` (abbreviated) format characters
- `_month_names` — 12 entries (144 bytes), same layout as day names. Used by `__rt_date` for `F` (full name) and `M` (abbreviated) format characters
- `_strtotime_keyword_tab`, `_strtotime_unit_tab` — keyword, weekday, modifier, and unit lookup tables used by `__rt_strtotime`
- `_instanceof_target_count`, `_instanceof_target_entries`, `_instanceof_name_*` — case-insensitive class/interface name metadata used by dynamic `instanceof` string targets, including leading-backslash aliases
- `_class_gc_desc_count`, `_class_gc_desc_ptrs`, `_class_gc_desc_<id>` — per-class property traversal metadata used by object deep-free and cycle collection
- `_class_json_desc_ptrs`, `_class_json_desc_<id>`, `_class_json_pname_<id>_<slot>`, `_json_exception_class_id`, `_stdclass_class_id` — JSON object encoding descriptors, JsonException construction metadata, and stdClass runtime class id
- `_class_attribute_count`, `_class_attribute_ptrs`, `_class_attributes_<id>` — emitted class-level PHP attribute metadata. The current PHP-facing helpers and Reflection owner constructors materialize their results from the same `ClassInfo` metadata during codegen, rather than doing dynamic runtime class/member lookup.
- `_class_vtable_ptrs`, `_class_vtable_<id>` — per-class virtual-method tables used by inheritance dispatch through `class_id`
- `_class_static_vtable_ptrs`, `_class_static_vtable_<id>` — per-class static-method tables used by late static binding
- `_class_destruct_ptrs` — class_id-indexed `__destruct` method pointers (or `0`) consulted by `__rt_call_object_destructor` during object deep-free
- `_classes_by_name`, `_classes_by_name_count` — case-insensitive `name -> (class_id, object size)` lookup table used by `__rt_new_by_name` for `new $variable()`
- `static_property_symbol(...)`-derived `.comm` slots — 16-byte storage slots for effective declaring static properties, shared by inherited static properties until a subclass redeclares the property
- `enum_case_symbol(...)`-derived `.comm` slots — singleton backing storage for enum cases emitted from user program metadata

When `--heap-debug` is enabled, the runtime also activates `__rt_heap_debug_check_live`, `__rt_heap_debug_validate_free_list`, and `__rt_heap_debug_report`. These helpers turn allocator corruption into immediate fatal errors for duplicate frees, zero-refcount `incref`/`decref` paths, and malformed free-list or small-bin state, poison freed payload bytes with `0xA5`, and print an end-of-process summary with alloc/free counts, live block count, live bytes, leak summary, and the peak live-byte watermark.

Every heap allocation now also carries a uniform 8-byte kind tag in its 16-byte allocator header. The current runtime uses `0=raw/untyped`, `1=string`, `2=indexed array`, `3=assoc/hash`, `4=object`, and `5=boxed mixed`, which lets runtime dispatch stay independent from each payload's internal layout. Generator frames use heap kind `4` because `Generator` is a built-in object with a custom payload layout. The low 16 bits keep the persistent container metadata: low byte = heap kind, bits `8..14` = indexed-array runtime `value_type`, and bit `15` = copy-on-write container flag. The collector reuses higher bits for transient reachable/incoming-edge metadata during `__rt_gc_collect_cycles`. Runtime data also now includes `_gc_collecting`, `_gc_release_suppressed`, `_class_gc_desc_count`, `_class_gc_desc_ptrs`, `_class_vtable_ptrs`, `_class_static_vtable_ptrs`, and static-property storage slots so deep-free / cycle-collection paths can coordinate nested releases, discover class property traversal metadata, and support inherited instance dispatch, static-property reads/writes, and late static binding.

See [Memory Model](memory-model.md) for details on how these buffers work.
