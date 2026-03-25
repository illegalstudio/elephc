# The Runtime

[← Back to Wiki](README.md) | Previous: [The Code Generator](the-codegen.md) | Next: [Memory Model →](memory-model.md)

---

**Source:** `src/codegen/runtime/` — `mod.rs`, `strings/`, `arrays/`, `io/`, `system/`

The runtime is a collection of **hand-written assembly routines** that handle operations too complex for inline code generation. When the [code generator](the-codegen.md) needs to convert an integer to a string or concatenate two strings, it emits a `bl __rt_itoa` or `bl __rt_concat` — a call to a runtime routine.

These routines are emitted as assembly functions at the end of every compiled program. They're not external libraries — they're part of the binary.

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

**Input:** `x1`/`x2` = right string (ptr/len), `x3`/`x4` = left string (ptr/len)
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
| `__rt_trim` | Strip whitespace | `x1`/`x2` | `x1`/`x2` |
| `__rt_ltrim` / `__rt_rtrim` | Strip left/right | `x1`/`x2` | `x1`/`x2` |
| `__rt_strrev` | Reverse string | `x1`/`x2` | `x1`/`x2` |
| `__rt_strpos` | Find substring | `x1`/`x2` + `x3`/`x4` | `x0` (index or -1) |
| `__rt_strrpos` | Find last occurrence | `x1`/`x2` + `x3`/`x4` | `x0` |
| `__rt_str_repeat` | Repeat N times | `x1`/`x2` + `x0` (count) | `x1`/`x2` |
| `__rt_str_replace` | Replace all occurrences | search + replace + subject | `x1`/`x2` |
| `__rt_explode` | Split by delimiter | delimiter + string | `x0` (array ptr) |
| `__rt_implode` | Join with glue | glue + array | `x1`/`x2` |
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

**Source:** `src/codegen/runtime/arrays/` (45 files)

### Core allocation

| Routine | What it does | Input | Output |
|---|---|---|---|
| `__rt_heap_alloc` | Bump-allocate N bytes from heap | `x0` = size | `x0` = pointer |
| `__rt_array_new` | Create indexed array with header | `x0` = capacity, `x1` = elem_size | `x0` = array ptr |
| `__rt_array_push_int` | Append int to indexed array | `x0` = array, `x1` = value | — |
| `__rt_array_push_str` | Append string to indexed array | `x0` = array, `x1`/`x2` = str | — |
| `__rt_sort_int` | In-place sort (ascending/descending) | `x0` = array | — |

### Hash table (for associative arrays)

| Routine | What it does | Input | Output |
|---|---|---|---|
| `__rt_hash_fnv1a` | FNV-1a hash of string | `x1`/`x2` = string | `x0` = hash |
| `__rt_hash_new` | Create hash table | `x0` = capacity, `x1` = value type | `x0` = hash ptr |
| `__rt_hash_set` | Insert/update key-value pair | `x0`=hash, `x1`/`x2`=key, `x3`/`x4`=value | — |
| `__rt_hash_get` | Look up value by key | `x0`=hash, `x1`/`x2`=key | `x0`=found, `x1`=val_lo, `x2`=val_hi |
| `__rt_hash_iter_next` | Iterate to next entry | `x0`=hash, `x1`=index | `x0`=next_idx, `x1`/`x2`=key, `x3`/`x4`=value |
| `__rt_hash_count` | Count occupied entries | `x0`=hash | `x0`=count |

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
| `__rt_array_column` | Extract column from array of assoc arrays |
| `__rt_range` | Generate integer range array |
| `__rt_shuffle` / `__rt_array_rand` | Randomize order / pick random |
| `__rt_asort` / `__rt_ksort` / `__rt_natsort` | Sort preserving keys |
| `__rt_array_map` | Apply callback to each element, return new array |
| `__rt_array_filter` | Filter elements where callback returns truthy |
| `__rt_array_reduce` | Reduce array to single value via callback |
| `__rt_array_walk` | Call callback on each element (side-effects) |
| `__rt_usort` | Sort array using user comparison callback |

## System routines

**Source:** `src/codegen/runtime/system/` (10 files)

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

### Date/time routines

**Files:** `system/date.rs`, `system/mktime.rs`, `system/strtotime.rs`

| Routine | What it does | Input | Output |
|---|---|---|---|
| `__rt_date` | Format a Unix timestamp using PHP date format characters (Y, m, d, H, i, s, l, F, etc.). Uses `localtime_r()` from libc and static lookup tables (`_day_names`, `_month_names`) for day/month names | `x1`/`x2` = format string, `x0` = timestamp | `x1`/`x2` = formatted string |
| `__rt_mktime` | Create a Unix timestamp from date components (hour, minute, second, month, day, year). Populates a `tm` struct on the stack and calls libc `mktime()` | `x0`-`x5` = h, m, s, mon, day, year | `x0` = Unix timestamp |
| `__rt_strtotime` | Parse a date string in "YYYY-MM-DD" or "YYYY-MM-DD HH:MM:SS" format to a Unix timestamp. Manually parses the digits, populates a `tm` struct, and calls libc `mktime()` | `x1`/`x2` = date string | `x0` = Unix timestamp |

### JSON routines

**Files:** `system/json_encode.rs`, `system/json_decode.rs`

The `json_encode` implementation uses **type-aware dispatch** — the codegen calls a different runtime routine depending on the compile-time type of the value being encoded:

| Routine | What it does | Input | Output |
|---|---|---|---|
| `__rt_json_encode_bool` | Encode bool as `"true"` or `"false"` using static data labels | `x0` = 0 or 1 | `x1`/`x2` = JSON string |
| `__rt_json_encode_null` | Encode null as `"null"` using a static data label | — | `x1`/`x2` = JSON string |
| `__rt_json_encode_str` | Encode a string with JSON escaping (quotes, backslashes, control chars) | `x1`/`x2` = input string | `x1`/`x2` = JSON string |
| `__rt_json_encode_array_int` | Encode an integer array as a JSON array (e.g., `[1,2,3]`) | `x0` = array ptr | `x1`/`x2` = JSON string |
| `__rt_json_encode_array_str` | Encode a string array as a JSON array with quoted elements | `x0` = array ptr | `x1`/`x2` = JSON string |
| `__rt_json_encode_assoc` | Encode an associative array as a JSON object (e.g., `{"key":"val"}`) | `x0` = hash ptr | `x1`/`x2` = JSON string |
| `__rt_json_decode` | Decode a JSON string value — strips surrounding quotes and unescapes JSON escape sequences | `x1`/`x2` = JSON string | `x1`/`x2` = decoded string |

### Regex routines

**File:** `system/preg.rs`

All regex routines use **POSIX extended regular expressions** via libc's `regcomp()`, `regexec()`, and `regfree()`. A shared helper (`__rt_preg_strip_delimiters`) strips PHP-style delimiters (e.g., `/pattern/`) before passing the pattern to the POSIX API.

| Routine | What it does | Input | Output |
|---|---|---|---|
| `__rt_preg_match` | Test if a regex matches the subject string. Compiles the pattern, executes once, frees | pattern + subject strings | `x0` = 1 (match) or 0 (no match) |
| `__rt_preg_match_all` | Count all non-overlapping matches by repeatedly executing the regex with advancing offsets | pattern + subject strings | `x0` = match count |
| `__rt_preg_replace` | Replace all regex matches with a replacement string. Builds the result incrementally in the concat buffer | pattern + replacement + subject | `x1`/`x2` = result string |
| `__rt_preg_split` | Split the subject string at regex match boundaries. Returns a string array of the non-matching segments | pattern + subject strings | `x0` = array pointer |

## I/O routines

**Source:** `src/codegen/runtime/io/` (16 files)

These routines handle file and filesystem operations via macOS system calls. PHP strings (pointer + length) must be converted to null-terminated C strings before passing to syscalls — the `__rt_cstr` helper handles this using a dedicated 4KB buffer.

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
| `__rt_fs` | Filesystem operations (mkdir, rmdir, unlink, rename, copy, chmod) |
| `__rt_getcwd` | Get current working directory |
| `__rt_scandir` | List directory contents into array |
| `__rt_glob` | Pattern-match filenames |
| `__rt_tempnam` | Create temporary filename |
| `__rt_fgetcsv` | Parse CSV line from file |
| `__rt_fputcsv` | Write CSV line to file |

## How routines are emitted

**File:** `src/codegen/runtime/mod.rs`

The `emit_runtime()` function calls every routine's emitter in sequence:

```rust
pub fn emit_runtime(emitter: &mut Emitter) {
    strings::emit_itoa(emitter);
    strings::emit_ftoa(emitter);
    strings::emit_concat(emitter);
    // ... 44 more string routines ...
    system::emit_build_argv(emitter);
    system::emit_time(emitter);
    system::emit_microtime(emitter);
    system::emit_getenv(emitter);
    system::emit_shell_exec(emitter);
    system::emit_date(emitter);
    system::emit_mktime(emitter);
    system::emit_strtotime(emitter);
    system::emit_json_encode_bool(emitter);
    system::emit_json_encode_null(emitter);
    system::emit_json_encode_str(emitter);
    system::emit_json_encode_array_int(emitter);
    system::emit_json_encode_array_str(emitter);
    system::emit_json_encode_assoc(emitter);
    system::emit_json_decode(emitter);
    system::emit_preg_match(emitter);
    system::emit_preg_match_all(emitter);
    system::emit_preg_replace(emitter);
    system::emit_preg_split(emitter);
    arrays::emit_heap_alloc(emitter);
    arrays::emit_array_new(emitter);
    // ... 40+ more array routines ...
    io::emit_cstr(emitter);
    io::emit_fopen(emitter);
    // ... 14 more I/O routines ...
}
```

All routines are included in every binary, even if unused. This is simpler than dead-code elimination (a potential future optimization).

## Runtime data

The runtime also declares global buffers using `.comm` and static data tables:

```asm
.comm _concat_buf, 65536     ; 64KB string buffer
.comm _concat_off, 8         ; current offset into string buffer
.comm _global_argc, 8        ; saved argc from OS
.comm _global_argv, 8        ; saved argv pointer from OS
.comm _heap_buf, 1048576     ; 1MB heap
.comm _heap_off, 8           ; current heap offset
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
- `_json_true`, `_json_false`, `_json_null` — JSON keyword strings used by `__rt_json_encode_bool` and `__rt_json_encode_null`
- `_day_names` — 7 entries (84 bytes), each 12 bytes: day name padded to 10 chars + 1 length byte + 1 padding byte. Used by `__rt_date` for `l` (full name) and `D` (abbreviated) format characters
- `_month_names` — 12 entries (144 bytes), same layout as day names. Used by `__rt_date` for `F` (full name) and `M` (abbreviated) format characters

See [Memory Model](memory-model.md) for details on how these buffers work.

---

Next: [Memory Model →](memory-model.md)
