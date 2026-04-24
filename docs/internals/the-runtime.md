---
title: "The Runtime"
description: "Hand-written assembly routines for strings, arrays, and I/O."
sidebar:
  order: 8
---

**Source:** `src/codegen/runtime/` — `mod.rs`, `data.rs`, `strings/`, `arrays/`, `buffers/`, `exceptions.rs`, `exceptions/`, `io/`, `system/`, `pointers/`

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
__rt_ftoa          float → string
__rt_concat        string + string → string
__rt_str_eq        string == string → bool
__rt_array_new     allocate a new array
__rt_throw_current throw the active exception
__rt_build_argv    build $argv from C strings
```

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

The digits are written **right-to-left** because division gives us the least significant digit first. The result is written into the [concat buffer](memory-model.md#the-string-buffer).

### `__rt_ftoa` — Float to string

**File:** `strings/ftoa.rs`

Converts a double-precision float in `d0` to a decimal string. Handles special cases: `INF`, `-INF`, `NAN`. For normal numbers, it separates the integer and fractional parts, converts each to digits, and joins them with a decimal point.

**Input:** `d0` = float value
**Output:** `x1` = pointer to string, `x2` = length

### `__rt_concat` — String concatenation

**File:** `strings/concat.rs`

Concatenates two strings by copying both into the [concat buffer](memory-model.md#the-string-buffer).

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
| `__rt_strtolower` | Lowercase conversion | `x1`/`x2` | `x1`/`x2` |
| `__rt_strtoupper` | Uppercase conversion | `x1`/`x2` | `x1`/`x2` |
| `__rt_trim` | Strip whitespace (no args) or chars in mask | `x1`/`x2` | `x1`/`x2` |
| `__rt_ltrim` / `__rt_rtrim` | Strip left/right whitespace or mask | `x1`/`x2` | `x1`/`x2` |
| `__rt_trim_mask` | Strip chars in custom mask from both ends | `x1`/`x2` + mask | `x1`/`x2` |
| `__rt_ltrim_mask` / `__rt_rtrim_mask` | Strip custom mask from left/right | `x1`/`x2` + mask | `x1`/`x2` |
| `__rt_strrev` | Reverse string | `x1`/`x2` | `x1`/`x2` |
| `__rt_strpos` | Find substring | `x1`/`x2` + `x3`/`x4` | `x0` (index or -1) |
| `__rt_strrpos` | Find last occurrence | `x1`/`x2` + `x3`/`x4` | `x0` |
| `__rt_str_repeat` | Repeat N times | `x1`/`x2` + `x0` (count) | `x1`/`x2` |
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
| `__rt_wordwrap` | Wrap text at width | str + width + break | `x1`/`x2` |
| `__rt_number_format` | Format number with separators | float + decimals + sep | `x1`/`x2` |
| `__rt_hash` | Hash with algorithm | algo + data | `x1`/`x2` |
| `__rt_sscanf` | Parse string with format | str + format | `x0` (array ptr) |

## Array routines

**Source:** `src/codegen/runtime/arrays/` (103 files)

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
| `__rt_array_ensure_unique` | Split a shared indexed array before mutation | `x0` = array | `x0` = unique array |
| `__rt_array_grow` | Ensure uniqueness, double array capacity, copy elements, free old unique storage | `x0` = array | `x0` = new array |
| `__rt_array_free_deep` | Free array storage and release nested heap-backed elements | `x0` = array | — |
| `__rt_array_push_int` | Append int to array (grows if needed) | `x0` = array, `x1` = value | `x0` = array |
| `__rt_array_push_refcounted` | `incref` borrowed heap payload, then append it as an 8-byte array element | `x0` = array, `x1` = heap ptr | `x0` = array |
| `__rt_array_push_str` | Persist string + append to array (grows if needed) | `x0` = array, `x1`/`x2` = str | `x0` = array |
| `__rt_sort_int` | In-place sort (ascending/descending) | `x0` = array | — |
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
| `__rt_array_filter_refcounted` | Filter an array of heap-backed elements without dropping borrowed payloads |
| `__rt_array_merge_into_refcounted` | Append one indexed array into another in-place while retaining nested heap-backed elements |

### Hash table (for associative arrays)

| Routine | What it does | Input | Output |
|---|---|---|---|
| `__rt_hash_fnv1a` | FNV-1a hash of string | `x1`/`x2` = string | `x0` = hash |
| `__rt_hash_new` | Create hash table | `x0` = capacity, `x1` = coarse value-type summary | `x0` = hash ptr |
| `__rt_hash_clone_shallow` | Clone hash storage for copy-on-write splitting, re-persisting keys and retaining nested heap values as needed | `x0` = hash | `x0` = new hash |
| `__rt_hash_ensure_unique` | Split a shared hash table before mutation | `x0` = hash | `x0` = unique hash |
| `__rt_hash_grow` | Double hash table capacity, rehash all entries | `x0` = hash | `x0` = new hash |
| `__rt_hash_set` | Insert/update (grows at 75% load) | `x0`=hash, `x1`/`x2`=key, `x3`/`x4`=value, `x5`=value_tag | `x0` = hash |
| `__rt_hash_insert_owned` | Reinsert an already-owned key/value pair during hash growth | `x0`=hash, `x1`/`x2`=key, `x3`/`x4`=value, `x5`=value_tag | `x0` = hash |
| `__rt_hash_get` | Look up value by key | `x0`=hash, `x1`/`x2`=key | `x0`=found, `x1`=val_lo, `x2`=val_hi, `x3`=value_tag |
| `__rt_hash_iter_next` | Iterate to next entry in insertion order | `x0`=hash, `x1`=cursor | `x0`=next cursor, `x1`/`x2`=key, `x3`/`x4`=value, `x5`=value_tag |
| `__rt_hash_count` | Count occupied entries | `x0`=hash | `x0`=count |
| `__rt_hash_free_deep` | Free a hash table plus owned keys and nested heap-backed values | `x0`=hash | — |
| `__rt_mixed_from_value` | Box a tagged payload into a heap-allocated mixed cell | `x0`=value_tag, `x1`=value_lo, `x2`=value_hi | `x0` = mixed cell |
| `__rt_mixed_write_stdout` | Print a boxed mixed value by inspecting its inner tag | `x0` = mixed cell | — |

`__rt_hash_iter_next` uses a small cursor protocol rather than a raw slot index: `0` starts from the hash header's `head`, positive cursors encode `slot_index + 1`, `-2` marks the post-tail state after yielding the final entry, and `-1` means iteration is exhausted.

See [Memory Model](memory-model.md) for the hash table memory layout.

### Array manipulation

| Routine | What it does |
|---|---|
| `__rt_array_key_exists` | Check if integer key is in bounds |
| `__rt_array_search` | Linear search for value in indexed array |
| `__rt_array_reverse` | Reverse element order |
| `__rt_array_sum` / `__rt_array_product` | Sum/product of all elements |
| `__rt_array_shift` / `__rt_array_unshift` | Remove/add at beginning |
| `__rt_array_merge` | Concatenate two indexed arrays into a new array |
| `__rt_array_merge_into` | Append all elements from source array into dest array (in-place) |
| `__rt_array_slice` / `__rt_array_splice` | Extract/replace subarray |
| `__rt_array_unique` | Remove duplicate values |
| `__rt_array_diff` / `__rt_array_intersect` | Set difference/intersection by value |
| `__rt_array_diff_key` / `__rt_array_intersect_key` | Set operations by key |
| `__rt_array_flip` | Swap keys and values → AssocArray |
| `__rt_array_combine` | Combine key array + value array → AssocArray |
| `__rt_array_fill` / `__rt_array_fill_keys` | Create filled arrays |
| `__rt_array_chunk` / `__rt_array_pad` | Chunk/pad arrays |
| `__rt_array_column` | Extract column from array of assoc arrays (int values) |
| `__rt_array_column_ref` | Extract column of retained heap-backed values (arrays / hashes / objects) |
| `__rt_array_column_str` | Extract column from array of assoc arrays (string values) |
| `__rt_range` | Generate integer range array |
| `__rt_shuffle` / `__rt_array_rand` | Randomize order / pick random |
| `__rt_random_u32` / `__rt_random_uniform` | Target-aware random primitives used by `rand()`, `random_int()`, `shuffle()`, and `array_rand()` |
| `__rt_asort` / `__rt_ksort` / `__rt_natsort` | Sort preserving keys |
| `__rt_array_map` | Apply callback to each int element, return new array |
| `__rt_array_map_str` | Apply callback to each string element, return new array |
| `__rt_array_filter` | Filter elements where callback returns truthy |
| `__rt_array_reduce` | Reduce array to single value via callback |
| `__rt_array_walk` | Call callback on each element (side-effects) |
| `__rt_usort` | Sort array using user comparison callback |

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

**Source:** `src/codegen/runtime/system/` (28 files)

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
| `__rt_shell_exec` | Execute shell command and capture output via libc `popen()`/`pclose()` | `x1`/`x2` = command string | `x1`/`x2` = output string |

## Exception routines

**Source:** `src/codegen/runtime/exceptions.rs` plus `src/codegen/runtime/exceptions/` (4 files)

elephc lowers exceptions with a small runtime layer around `_setjmp` / `_longjmp`. Codegen publishes the current exception object into `_exc_value`, pushes a handler record into `_exc_handler_top`, and then uses these helpers to unwind, match catch clauses, and resume control flow through `catch` / `finally`.

| Routine | What it does | Input | Output |
|---|---|---|---|
| `__rt_exception_cleanup_frames` | Walk the activation-record stack, run per-frame cleanup callbacks, and stop at the frame that should survive the catch | `x0` = surviving activation record | — |
| `__rt_exception_matches` | Check whether the active exception matches a catch target by class id or interface id | `x0` = exception object, `x1` = target id, `x2` = 0 for class / 1 for interface | `x0` = 1 if it matches, 0 otherwise |
| `__rt_throw_current` | Unwind to the nearest active handler or print the fatal uncaught-exception message and exit | reads `_exc_value`, `_exc_handler_top`, `_exc_call_frame_top` | does not return normally |
| `__rt_rethrow_current` | Re-enter the ordinary throw path with the currently active exception | none (uses global exception state) | does not return normally |

The fatal uncaught-exception path writes `Fatal error: uncaught exception` to stderr and exits with status 1. The runtime also resets the concat-buffer cursor before the final `longjmp`, so partially built string state from the throwing frame does not leak into the resumed catch/finally code.

### Date/time routines

**Files:** `system/date.rs`, `system/date_data.rs`, `system/mktime.rs`, `system/strtotime.rs`

| Routine | What it does | Input | Output |
|---|---|---|---|
| `__rt_date` | Format a Unix timestamp using PHP date format characters (Y, m, d, H, i, s, l, F, etc.). Uses `localtime_r()` from libc and static lookup tables (`_day_names`, `_month_names`) for day/month names | `x1`/`x2` = format string, `x0` = timestamp | `x1`/`x2` = formatted string |
| `__rt_mktime` | Create a Unix timestamp from date components (hour, minute, second, month, day, year). Populates a `tm` struct on the stack and calls libc `mktime()` | `x0`-`x5` = h, m, s, mon, day, year | `x0` = Unix timestamp |
| `__rt_strtotime` | Parse a date string in "YYYY-MM-DD" or "YYYY-MM-DD HH:MM:SS" format to a Unix timestamp. Manually parses the digits, populates a `tm` struct, and calls libc `mktime()` | `x1`/`x2` = date string | `x0` = Unix timestamp |

### JSON routines

**Files:** `system/json_data.rs`, `system/json_encode_bool.rs`, `system/json_encode_null.rs`, `system/json_encode_str.rs`, `system/json_encode_array_int.rs`, `system/json_encode_array_str.rs`, `system/json_encode_array_dynamic.rs`, `system/json_encode_assoc.rs`, `system/json_encode_mixed.rs`, `system/json_decode.rs`

The `json_encode` implementation uses **type-aware dispatch** — the codegen calls a different runtime routine depending on the compile-time type of the value being encoded:

| Routine | What it does | Input | Output |
|---|---|---|---|
| `__rt_json_encode_bool` | Encode bool as `"true"` or `"false"` using static data labels | `x0` = 0 or 1 | `x1`/`x2` = JSON string |
| `__rt_json_encode_null` | Encode null as `"null"` using a static data label | — | `x1`/`x2` = JSON string |
| `__rt_json_encode_str` | Encode a string with JSON escaping (quotes, backslashes, control chars) | `x1`/`x2` = input string | `x1`/`x2` = JSON string |
| `__rt_json_encode_array_int` | Encode an integer array as a JSON array (e.g., `[1,2,3]`) | `x0` = array ptr | `x1`/`x2` = JSON string |
| `__rt_json_encode_array_str` | Encode a string array as a JSON array with quoted elements | `x0` = array ptr | `x1`/`x2` = JSON string |
| `__rt_json_encode_array_dynamic` | Encode an indexed array by inspecting its packed runtime `value_type` tag at runtime (int, string, float, bool, nested array/hash, mixed, or null fallback) | `x0` = array ptr | `x1`/`x2` = JSON string |
| `__rt_json_encode_assoc` | Encode an associative array as a JSON object (e.g., `{"key":"val"}`) | `x0` = hash ptr | `x1`/`x2` = JSON string |
| `__rt_json_encode_mixed` | Encode a boxed mixed payload by unboxing its runtime tag and dispatching to the concrete JSON encoder | `x0` = mixed ptr | `x1`/`x2` = JSON string |
| `__rt_json_decode` | Decode the current string-only JSON contract — trims outer whitespace, unescapes quoted JSON strings including `\uXXXX` surrogate-aware UTF-8 decoding, and otherwise returns a trimmed borrowed JSON slice | `x1`/`x2` = JSON string | `x1`/`x2` = decoded string |

### Regex routines

**Files:** `system/preg_strip.rs`, `system/pcre_to_posix.rs`, `system/preg_match.rs`, `system/preg_match_all.rs`, `system/preg_replace.rs`, `system/preg_split.rs`

All regex routines use **POSIX extended regular expressions** via libc's `regcomp()`, `regexec()`, and `regfree()`. Shared helpers (`__rt_preg_strip` and `__rt_pcre_to_posix`) strip PHP-style delimiters and translate common PCRE shorthands before passing the pattern to the POSIX API.

| Routine | What it does | Input | Output |
|---|---|---|---|
| `__rt_preg_match` | Test if a regex matches the subject string. Compiles the pattern, executes once, frees | pattern + subject strings | `x0` = 1 (match) or 0 (no match) |
| `__rt_preg_match_all` | Count all non-overlapping matches by repeatedly executing the regex with advancing offsets | pattern + subject strings | `x0` = match count |
| `__rt_preg_replace` | Replace all regex matches with a replacement string. Builds the result incrementally in the concat buffer | pattern + replacement + subject | `x1`/`x2` = result string |
| `__rt_preg_split` | Split the subject string at regex match boundaries. Returns a string array of the non-matching segments | pattern + subject strings | `x0` = array pointer |

## I/O routines

**Source:** `src/codegen/runtime/io/` (17 files)

These routines handle file and filesystem operations via macOS system calls. PHP strings (pointer + length) must be converted to null-terminated C strings before passing to syscalls — `__rt_cstr` handles the primary buffer and also emits `__rt_cstr2` for routines that need a second simultaneous C string.

| Routine | What it does |
|---|---|
| `__rt_cstr` | Convert PHP string (ptr+len) to null-terminated C string |
| `__rt_fopen` | Open file via `open()` syscall |
| `__rt_fgets` | Read line from file descriptor |
| `__rt_feof` | Check end-of-file flag for a file descriptor |
| `__rt_fread` | Read N bytes from file descriptor |
| `__rt_file_get_contents` | Read entire file into string |
| `__rt_file_put_contents` | Write string to file (create/truncate) |
| `__rt_file` | Read file into array of lines |
| `__rt_stat` | Get file metadata (size, timestamps) |
| `__rt_unlink` / `__rt_mkdir` / `__rt_rmdir` / `__rt_chdir` | Filesystem path operations via libc/syscalls |
| `__rt_rename` / `__rt_copy` | Two-path filesystem helpers using dual C-string scratch buffers |
| `__rt_getcwd` | Get current working directory |
| `__rt_scandir` | List directory contents into array |
| `__rt_glob` | Pattern-match filenames |
| `__rt_tempnam` | Create temporary filename |
| `__rt_fgetcsv` | Parse CSV line from file |
| `__rt_fputcsv` | Write CSV line to file |

## Pointer routines

**Source:** `src/codegen/runtime/pointers/` (5 files)

These helpers support the compiler-specific pointer builtins.

| Routine | What it does | Input | Output |
|---|---|---|---|
| `__rt_ptoa` | Format a pointer value as a hexadecimal string with `0x` prefix | `x0` = pointer/address | `x1`/`x2` = formatted string |
| `__rt_ptr_check_nonnull` | Abort with `Fatal error: null pointer dereference` if the pointer is null | `x0` = pointer/address | `x0` unchanged on success |
| `__rt_str_to_cstr` | Copy an elephc string to temporary null-terminated heap storage for a native call | `x1`/`x2` = string | `x0` = C string pointer |
| `__rt_cstr_to_str` | Copy a borrowed null-terminated C string back into an owned elephc string | `x0` = C string pointer | `x1`/`x2` = elephc string |

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
| `__rt_mixed_is_empty` | Check emptiness of a mixed cell (PHP semantics) | `x0` = mixed cell pointer | `x0` = 0 or 1 |
| `__rt_mixed_strict_eq` | Compare two mixed cells by tag and value | `x0`, `x1` = mixed pointers | `x0` = 0 or 1 |
| `__rt_mixed_unbox` | Extract the raw payload from a mixed cell | `x0` = mixed cell pointer | `x0`/`x1`/`x2` depending on type |
| `__rt_hash_may_have_cyclic_values` | Scan hash entries to check if any contain refcounted children | `x0` = hash pointer | `x0` = 0 (scalar-only) or 1 (has cycles) |
| `__rt_match_unhandled` | Abort with `Fatal error: unhandled match case` | — | does not return |
| `__rt_enum_from_fail` | Abort with `Fatal error: enum case not found` when `Enum::from()` has no match | — | does not return |

## How routines are emitted

**File:** `src/codegen/runtime/emitters.rs`

The `emit_runtime()` function calls every routine emitter in a fixed order:

```rust
pub fn emit_runtime(emitter: &mut Emitter) {
    // strings: itoa, ftoa, concat, atoi, equality, formatting, trim/mask,
    // search/replace, explode/implode, hashing, encoding, sscanf, ...
    // system: argv, time, getenv, shell, date/mktime/strtotime, JSON, regex
    // exceptions: cleanup walk, catch matching, throw/rethrow helpers
    // arrays: heap alloc/free, array/hash helpers, sort, callbacks, refcount
    // buffers: contiguous buffer allocation, bounds checking, UAF traps
    // io: c-string buffers, file I/O, stat/fs helpers, scandir/glob/tempnam, CSV
    // pointers: ptoa, null check, str_to_cstr, cstr_to_str
}
```

Notable runtime-only helpers emitted here include `__rt_exception_cleanup_frames`, `__rt_exception_matches`, `__rt_throw_current`, `__rt_heap_debug_fail`, `__rt_heap_kind`, `__rt_hash_insert_owned`, `__rt_hash_free_deep`, `__rt_array_column_ref`, `__rt_preg_strip`, `__rt_pcre_to_posix`, `__rt_str_to_cstr`, and `__rt_cstr_to_str` in addition to the more user-visible helpers.

All routines are included in every binary, even if unused. elephc already does AST-side control-flow pruning and dead-code elimination before codegen, but runtime-specific dead stripping is still future work.

## Runtime data

The runtime data layer is split between `emit_runtime_data_fixed()` (shared buffers, error strings, lookup tables) and `emit_runtime_data_user()` (per-program globals, statics, enum-case slots, and metadata tables). Together they declare global buffers using `.comm` and static data tables:

```asm
.comm _concat_buf, 65536     ; 64KB string buffer
.comm _concat_off, 8         ; current offset into string buffer
.comm _global_argc, 8        ; saved argc from OS
.comm _global_argv, 8        ; saved argv pointer from OS
.comm _exc_handler_top, 8    ; top of the active exception-handler stack
.comm _exc_call_frame_top, 8 ; top of the activation-record cleanup stack
.comm _exc_value, 8          ; currently propagating exception object
.comm _heap_buf, 8388608     ; 8MB heap by default (--heap-size overrides)
.comm _heap_off, 8           ; current heap offset
.comm _heap_free_list, 8     ; head of the general address-ordered free list
.comm _heap_small_bins, 32   ; 4 x 8-byte heads for <=8/16/32/64-byte cached blocks
.comm _heap_debug_enabled, 8 ; BSS-backed debug flag, set to 1 in _main when compiled with --heap-debug
.comm _gc_collecting, 8      ; cycle collector re-entry guard
.comm _gc_release_suppressed, 8 ; suppress nested collection during deep frees
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
```

Additionally, the runtime emits static data tables:
- `_fmt_g` — printf format string for float-to-string conversion via `%.14G`
- `_b64_encode_tbl` — 64-byte Base64 encoding lookup table
- `_b64_decode_tbl` — 256-byte Base64 decoding lookup table
- `_heap_err_msg`, `_arr_cap_err_msg`, `_ptr_null_err_msg` — fatal runtime error strings
- `_buffer_bounds_msg`, `_buffer_uaf_msg`, `_match_unhandled_msg`, `_enum_from_msg` — fatal runtime error strings for buffers, `match`, and enums
- `_heap_dbg_bad_refcount_msg`, `_heap_dbg_double_free_msg`, `_heap_dbg_free_list_msg` — fatal heap-debug error strings enabled by `--heap-debug`
- `_heap_dbg_*` summary labels — fixed strings used by `__rt_heap_debug_report` for alloc/free/live/leak output
- `_uncaught_exc_msg` — fatal exception string written by `__rt_throw_current` when no handler exists
- `_pcre_space`, `_pcre_digit`, `_pcre_word`, `_pcre_nspace`, `_pcre_ndigit`, `_pcre_nword` — PCRE shorthand replacement strings for regex translation
- `_json_true`, `_json_false`, `_json_null` — JSON keyword strings used by `__rt_json_encode_bool` and `__rt_json_encode_null`
- `_day_names` — 7 entries (84 bytes), each 12 bytes: day name padded to 10 chars + 1 length byte + 1 padding byte. Used by `__rt_date` for `l` (full name) and `D` (abbreviated) format characters
- `_month_names` — 12 entries (144 bytes), same layout as day names. Used by `__rt_date` for `F` (full name) and `M` (abbreviated) format characters
- `_class_gc_desc_count`, `_class_gc_desc_ptrs`, `_class_gc_desc_<id>` — per-class property traversal metadata used by object deep-free and cycle collection
- `_class_vtable_ptrs`, `_class_vtable_<id>` — per-class virtual-method tables used by inheritance dispatch through `class_id`
- `_class_static_vtable_ptrs`, `_class_static_vtable_<id>` — per-class static-method tables used by late static binding
- `enum_case_symbol(...)`-derived `.comm` slots — singleton backing storage for enum cases emitted from user program metadata

When `--heap-debug` is enabled, the runtime also activates `__rt_heap_debug_check_live`, `__rt_heap_debug_validate_free_list`, and `__rt_heap_debug_report`. These helpers turn allocator corruption into immediate fatal errors for duplicate frees, zero-refcount `incref`/`decref` paths, and malformed free-list or small-bin state, poison freed payload bytes with `0xA5`, and print an end-of-process summary with alloc/free counts, live block count, live bytes, leak summary, and the peak live-byte watermark.

Every heap allocation now also carries a uniform 8-byte kind tag in its 16-byte allocator header. The current runtime uses `0=raw/untyped`, `1=string`, `2=indexed array`, `3=assoc/hash`, `4=object`, and `5=boxed mixed`, which lets runtime dispatch stay independent from each payload's internal layout. The low 16 bits keep the persistent container metadata: low byte = heap kind, bits `8..14` = indexed-array runtime `value_type`, and bit `15` = copy-on-write container flag. The collector reuses higher bits for transient reachable/incoming-edge metadata during `__rt_gc_collect_cycles`. Runtime data also now includes `_gc_collecting`, `_gc_release_suppressed`, `_class_gc_desc_count`, `_class_gc_desc_ptrs`, `_class_vtable_ptrs`, and `_class_static_vtable_ptrs` so deep-free / cycle-collection paths can coordinate nested releases, discover class property traversal metadata, and support both inherited instance dispatch and late static binding.

See [Memory Model](memory-model.md) for details on how these buffers work.
