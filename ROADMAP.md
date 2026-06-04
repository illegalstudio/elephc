# Roadmap

## Direction for upcoming 0.x releases

The roadmap stays in the 0.x series while the compiler, runtime, and product
shape are still moving. Current work is focused on PHP parity, backend
foundations, and concrete product tracks without a major-version release gate.

Current direction:

- Finish the well-bounded PHP-visible compatibility gaps before backend replacement.
- Keep completed historical items in their original version sections.
- Move optimizer work behind EIR, benchmark evidence, and real-world validation.
- Treat shared libraries, the PHP extension bridge, and WebAssembly as later 0.x product tracks.
- Leave the major-version discussion for the final future-perspective section.

## v0.1.x — Usable CLI compiler (done)

- [x] Lexer, parser (Pratt), type checker, ARM64 codegen pipeline
- [x] Integers, strings (double and single quoted), echo, variables, comments
- [x] Arithmetic (`+`, `-`, `*`, `/`, `%`), comparison (`==`, `!=`, `<`, `>`, `<=`, `>=`)
- [x] String concatenation (`.`) with automatic int coercion
- [x] `if` / `elseif` / `else`, `while`, `for`, `do...while`, `break`, `continue`, including multi-level `break N` / `continue N`
- [x] Functions with local scope, return, recursion, nested calls
- [x] Pre/post increment/decrement (`++$i`, `$i++`, `--$i`, `$i--`)
- [x] Logical operators: `&&`, `||`, `and`, `or`, `xor`, `!` (`and`/`or`/symbolic forms use short-circuit evaluation)
- [x] Assignment operators: `+=`, `-=`, `*=`, `/=`, `.=`, `%=`
- [x] Boolean literals: `true`, `false` (as integer 1/0)
- [x] Ternary operator: `$x = $a > $b ? $a : $b;`
- [x] `$argc` / `$argv` superglobals
- [x] `exit($code);` / `die();`
- [x] Built-in `strlen()`, `intval()`
- [x] Error messages with line/column numbers

## v0.2.x — Arrays and null (done)

- [x] Indexed arrays: `$arr = [1, 2, 3];`
- [x] Array access, assignment, push: `$arr[0]`, `$arr[0] = 42`, `$arr[] = "new"`
- [x] `count()`, `array_push()`, `array_pop()`
- [x] `foreach ($arr as $value) { }` loop
- [x] `in_array()`, `array_keys()`, `array_values()`, `sort()`, `rsort()`, `isset()`
- [x] Heap allocator (1MB bump allocator)
- [x] Proper null: `echo null` prints nothing, `is_null()`, null coercion in operations

## v0.3.x — Bool, float, and type system (done)

Proper type system for PHP compatibility.

### Bool type
- [x] `true`/`false` as distinct Bool type
- [x] `echo false` prints nothing, `echo true` prints `1` (like PHP)
- [x] Bool coercion: `false` → `0`/`""` in arithmetic/concat, `true` → `1`/`"1"`
- [x] `is_bool()`, `boolval()`
- [x] `===` and `!==` strict comparison (type-aware)

### Float type
- [x] Float literals: `3.14`, `1.0e-5`, `-0.5`
- [x] Division returns float: `10 / 3` → `3.3333...`
- [x] `intdiv()` for integer division
- [x] Mixed int/float arithmetic (auto-promotion to float)
- [x] Float comparison and formatting
- [x] `floatval()`, `is_float()`, `is_int()`, `is_string()`, `is_numeric()`
- [x] `INF`, `NAN`, `is_nan()`, `is_finite()`, `is_infinite()`

### Type operations
- [x] Type casting: `(int)`, `(string)`, `(float)`, `(bool)`, `(array)`
- [x] `gettype()`, `settype()`
- [x] `empty()` — check if variable is empty/falsy
- [x] `unset()` — destroy variable

### Math functions
- [x] `abs()`, `min()`, `max()`, `floor()`, `ceil()`, `round()`
- [x] `sqrt()`, `pow()`
- [x] `**` exponentiation operator
- [x] `fmod()`, `fdiv()`
- [x] `rand()`, `mt_rand()`, `random_int()`
- [x] `number_format()`
- [x] Constants: `PHP_INT_MAX`, `PHP_INT_MIN`, `PHP_FLOAT_MAX`, `M_PI`

## v0.4.x — Strings (done)

- [x] String interpolation: `"Hello $name"`
- [x] `substr()`, `strpos()`, `strrpos()`, `strstr()`
- [x] `str_replace()`
- [x] `str_ireplace()`, `substr_replace()`
- [x] `strtolower()`, `strtoupper()`, `ucfirst()`, `lcfirst()`
- [x] `ucwords()`
- [x] `trim()`, `ltrim()`, `rtrim()`
- [x] `str_repeat()`, `strrev()`
- [x] `str_pad()`
- [x] `explode()`, `implode()`
- [x] `str_split()`
- [x] `sprintf()`, `printf()`, `sscanf()`
- [x] `strcmp()`, `strcasecmp()`, `str_contains()`, `str_starts_with()`, `str_ends_with()`
- [x] `ord()`, `chr()`
- [x] `nl2br()`, `wordwrap()`
- [x] `addslashes()`, `stripslashes()`
- [x] `htmlspecialchars()`, `htmlentities()`, `html_entity_decode()`
- [x] `urlencode()`, `urldecode()`, `rawurlencode()`, `rawurldecode()`
- [x] `md5()`, `sha1()`, `hash()`
- [x] `base64_encode()`, `base64_decode()`
- [x] `bin2hex()`, `hex2bin()`
- [x] `ctype_alpha()`, `ctype_digit()`, `ctype_alnum()`, `ctype_space()`

## v0.5.x — I/O and file system (done)

- [x] `fgets(STDIN)` / `readline()` — read from keyboard
- [x] `STDIN`, `STDOUT`, `STDERR` constants
- [x] `fopen()`, `fclose()`, `fread()`, `fwrite()`, `fgets()`, `feof()`
- [x] `fgetcsv()`, `fputcsv()`
- [x] `fseek()`, `ftell()`, `rewind()`
- [x] `file_get_contents()`, `file_put_contents()`
- [x] `file()` — read file into array
- [x] `file_exists()`, `is_file()`, `is_dir()`, `is_readable()`, `is_writable()`
- [x] `filesize()`, `filemtime()`
- [x] `copy()`, `rename()`, `unlink()`, `mkdir()`, `rmdir()`
- [x] `scandir()`, `glob()`, `getcwd()`, `chdir()`
- [x] `tempnam()`, `sys_get_temp_dir()`
- [x] Statement-form `print` output
- [x] `var_dump()`, `print_r()` for debugging

## v0.6.x — Associative arrays and switch (done)

- [x] Multi-dimensional arrays: `[[1,2],[3,4]]`, `$a[0][1]`
- [x] Associative arrays: `$map = ["key" => "value"];`
- [x] `foreach ($map as $key => $value) { }`
- [x] Hash table runtime for string keys
- [x] `array_key_exists()`, `array_search()`
- [x] `array_merge()`, `array_slice()`, `array_splice()`
- [x] `array_map()`, `array_filter()`, `array_reduce()`, `array_walk()` (string callbacks)
- [x] `array_combine()`, `array_flip()`, `array_reverse()`, `array_unique()`
- [x] `array_column()`
- [x] `array_sum()`, `array_product()`
- [x] `array_chunk()`, `array_pad()`, `array_fill()`, `array_fill_keys()`
- [x] `array_diff()`, `array_intersect()`, `array_diff_key()`, `array_intersect_key()`
- [x] `array_unshift()`, `array_shift()`
- [x] `usort()`, `uksort()`, `uasort()` (string callbacks)
- [x] `asort()`, `arsort()`, `ksort()`, `krsort()`
- [x] `natsort()`, `natcasesort()`, `shuffle()`, `array_rand()`
- [x] `range()`
- [x] Direct indexed-array growth preserves existing slots for writes such as `$items[2] = 1` after `$items = [10, 20]`
- [x] `switch` / `case` / `default` (with fall-through)
- [x] `match` expression (PHP 8 style, no fall-through)

## v0.7.x — Advanced language features

- [x] `define()` / `const` constants
- [x] `global $var;` keyword
- [x] Static variables: `static $counter = 0;`
- [x] Pass by reference: `function foo(&$x) { }`
- [x] Default parameter values: `function foo($x = 10) { }`
- [x] Variadic functions: `function foo(...$args) { }`
- [x] Anonymous functions / closures: `$fn = function($x) { }` with `use ($var)` captures
- [x] Arrow functions: `$fn = fn($x) => $x * 2`
- [x] Null coalescing: `$x ?? $default`, `$x ??= $default`
- [x] Spread operator: `func(...$args)`, `[...$a, ...$b]`
- [x] List unpacking: `[$a, $b] = $array;`
- [x] Heredoc / nowdoc strings
- [x] Bitwise operators: `&`, `|`, `^`, `~`, `<<`, `>>`
- [x] Full compound assignment family: `**=`, `&=`, `|=`, `^=`, `<<=`, `>>=`
- [x] Assignment expressions — local variables and stabilized non-local targets (`$items[0]`, `$items[idx()]`, `$obj->x`, `makeBox()->x`, `ClassName::$x`, property array slots) support `=`, compound assignment, and `??=` as PHP-compatible expressions, including RHS-mutated target dependencies such as `$items[$i] ??= ($i = 1)`, with assignment precedence below `?:` / `??` and above `and` / `xor` / `or`
- [x] Spaceship operator: `<=>`
- [x] `call_user_func()` (string callbacks)
- [x] `call_user_func_array()`
- [x] `function_exists()`

## v0.8.x — Date/time, JSON, regex

- [x] `time()`, `microtime()`
- [x] `date()`, `mktime()`, `strtotime()`
- [x] `sleep()`, `usleep()`
- [x] `json_encode()`, `json_decode()`, `json_last_error()` (basic surface)
- [x] Extended JSON surface: `json_encode($value, $flags, $depth)`, `json_decode($json, $associative, $depth, $flags)`, `json_last_error_msg()`, `json_validate()` (signatures, structural decode, depth/flag handling, validation, and JSON error state)
- [x] All PHP `JSON_*` flag and `JSON_ERROR_*` constants exposed
- [x] Object encoding via public properties + `JsonSerializable` dispatch (including nested objects, arrays of objects, and assoc-of-objects)
- [x] `JsonException` + `RuntimeException` class hierarchy (catchable as themselves and any parent class)
- [x] `JSON_PRETTY_PRINT` (4-space indent, newlines between elements, single space after `:`) and `JSON_UNESCAPED_SLASHES`
- [x] `JSON_HEX_TAG`, `JSON_HEX_AMP`, `JSON_HEX_APOS`, `JSON_HEX_QUOT` (hex-escape `<`/`>`, `&`, `'`, `"` for HTML/XML embedding contexts)
- [x] `JSON_FORCE_OBJECT` (indexed arrays encode as `{"0":val,...}`; specialized array_int / array_str fast paths runtime-redirect to array_dynamic when the flag is on)
- [x] `JSON_PRESERVE_ZERO_FRACTION` (integer-valued floats encode as `1.0` instead of collapsing to `1`; tail-appends `.0` when the formatted slice has no `.` / `e` / `E` marker)
- [x] `JSON_UNESCAPED_UNICODE` (multibyte UTF-8 escaped to `\uXXXX` by default, with surrogate-pair encoding for codepoints ≥ U+10000; flag preserves the literal bytes); ARM64 + x86_64 paths both implemented (Linux runtime parity validated via Docker scripts).
- [x] `JSON_NUMERIC_CHECK` (numeric strings encode as raw JSON numbers when the entire input matches the RFC 8259 number grammar; `array_str` redirects to `array_dynamic` so the per-element check fires inside indexed string arrays).
- [x] `$depth` enforcement: each container encoder (assoc, indexed array, object) increments `_json_active_depth` at entry, compares with `_json_depth_limit`, triggers `JSON_ERROR_DEPTH` (and `JsonException` under `JSON_THROW_ON_ERROR`) when the limit is crossed, and decrements on exit so siblings start fresh.
- [x] `JSON_THROW_ON_ERROR` for `json_encode()` and `json_decode()` errors (raises `JsonException` with PHP-compatible messages); `json_validate()` follows PHP's allowed flag set and rejects `JSON_THROW_ON_ERROR`
- [x] `json_validate()` recursive-descent RFC 8259 validator: literals (`null`/`true`/`false`), number grammar (`-?(0|[1-9][0-9]*)(.[0-9]+)?([eE][+-]?[0-9]+)?`), string escapes (`\"`, `\\`, `\/`, `\b`, `\f`, `\n`, `\r`, `\t`, `\uHHHH`), balanced arrays/objects, colon between key and value, no trailing content; depth tracked against `$depth` and routed through `__rt_json_throw_error` (`JSON_ERROR_DEPTH` for overflow, `JSON_ERROR_SYNTAX` for any malformed token).
- [x] Inf/NaN detection in `json_encode()` (sets `JSON_ERROR_INF_OR_NAN`, returns `false` by default, substitutes `0` with `JSON_PARTIAL_OUTPUT_ON_ERROR`, and throws with `JSON_THROW_ON_ERROR`)
- [x] List-shape detection in associative-array encoder: hashes whose keys form `0..count-1` in insertion order emit JSON arrays (`[...]`), matching PHP's runtime detection. `JSON_FORCE_OBJECT` overrides; empty hashes encode as `[]`.
- [x] Malformed UTF-8 detection in `json_encode()` (lead-byte validation, continuation-byte validation, bounds-checked truncated sequences) honoring `JSON_INVALID_UTF8_IGNORE` (silent drop), `JSON_INVALID_UTF8_SUBSTITUTE` (emit `�`), and `JSON_THROW_ON_ERROR` (raises `JsonException` for `JSON_ERROR_UTF8`)
- [x] `JSON_PARTIAL_OUTPUT_ON_ERROR` semantics: encoder errors return `false` by default; when the flag is set, substitutable failures such as Inf/NaN keep partial output (`0`) while malformed UTF-8 is handled by the explicit ignore/substitute flags
- [x] `json_decode()` returning a fully structured `Mixed` value: scalars (null, bool, int, float, string with full escape decoding), empty containers, **non-empty arrays** (recursive-descent with depth-and-string-aware boundary scanner; each element recursively decodes via `__rt_json_decode_mixed`), and **non-empty objects** (recursive: keys parsed as JSON strings, values recursively decoded, pairs inserted into a hash via `__rt_hash_set`).
- [x] `stdClass` builtin class with dynamic property storage. `new stdClass()` allocates a 16-byte object whose hidden hash backs `$obj->name = $val` / `$obj->name`. Property access on `stdClass` (and on `Mixed` receivers — the common `json_decode($json)->name` idiom) routes through `__rt_stdclass_get` / `__rt_stdclass_set` (and `__rt_mixed_property_get` / `__rt_mixed_property_set` for the unbox+dispatch path). `json_decode($json)` returns `stdClass` by default (PHP semantics), `json_decode($json, true)` returns the assoc array; the `_json_decode_assoc` runtime flag threads the choice through nested objects. `json_encode()` of `stdClass` walks the dynamic-property hash through a stdClass-aware wrapper that preserves `{}` for empty instances.
- [x] Structural decode-error detectors. `json_decode()` uses a checked recursive decoder instead of a full-buffer pre-validation pass: invalid input returns `Mixed(null)` and sets `JSON_ERROR_SYNTAX` (and raises `JsonException` under `JSON_THROW_ON_ERROR`); depth overflow sets `JSON_ERROR_DEPTH` and raises `JsonException` likewise. The `_json_active_flags`/`_json_depth_limit`/`_json_active_depth` plumbing is shared with `json_validate()` and `json_encode()`. `json_last_error()` / `json_last_error_msg()` reflect the failure; `json_decode()` resets the slot at entry so a previous failure does not leak.
- [x] `preg_match()`, `preg_match_all()`, `preg_replace()`, `preg_split()`
- [x] `exec()`, `shell_exec()`, `system()`, `passthru()`
- [x] `getenv()`, `putenv()`
- [x] `php_uname()`, `phpversion()`
- [x] Constants: `PHP_EOL`, `PHP_OS`, `DIRECTORY_SEPARATOR`

## v0.9.x — Memory management (done)

- [x] Free-list allocator (replace bump allocator with reusable memory)
- [x] Heap allocation headers (8-byte block size, minimum 8-byte allocation)
- [x] `__rt_heap_free` / `__rt_heap_free_safe` — return blocks to free list
- [x] Copy-on-store (`__rt_str_persist`) — strings persisted to heap, concat buffer is scratch-only
- [x] Concat buffer recycling (reset per statement, no more overflow)
- [x] Free on reassignment (old string/array freed when variable is overwritten)
- [x] `unset()` frees heap memory
- [x] Configurable heap size (`--heap-size=BYTES`, default 8MB)
- [x] Heap bounds checking with fatal error message
- [x] Array push capacity checking with fatal error message
- [x] `include` / `require` / `include_once` / `require_once`
- [x] Dynamic array growth (automatic 2x reallocation on push beyond capacity)
- [x] Dynamic hash table growth (automatic 2x rehash at 75% load factor)
- [x] Persist strings to heap before pushing to arrays and hash values
- [x] String deduplication — `str_persist` skips copy for .data and heap strings (only copies from concat_buf)
- [x] Block coalescing — bump pointer reset when freeing the last allocated block (O(1), zero fragmentation for `.=` loops)
- [x] Deep free for arrays via `unset()` (frees string elements + array struct)
- [x] Zero-init local variables in function prologues (prevents stale pointer frees)

## v0.10.x — Basic classes (done)

- [x] Classes with `public`/`private` properties and optional defaults
- [x] Constructor (`__construct`) with arguments
- [x] Instance methods with `$this` access
- [x] Static methods via `ClassName::method()`
- [x] Static properties via `ClassName::$prop`, `self::$prop`, `parent::$prop`, and `static::$prop`
- [x] `::class` magic constant (`Class::class`, `self::class`, `parent::class`, `static::class`)
- [x] `new self()` / `new static()` / `new parent()` factory pattern
- [x] `new` keyword for object instantiation
- [x] `->` property access and method calls
- [x] Nullsafe property access and method calls with `?->` for nullable object receivers
- [x] `readonly` properties (enforced at compile time)
- [x] Property type declarations (`public int $x`, `readonly ?string $name`) with checked defaults and assignments
- [x] Objects as function parameters and return values
- [x] Objects stored in arrays

## v0.11.x — Reference-counting garbage collector (done)

- [x] Reference counting infrastructure (header: `[size:4][refcount:4]`, zero overhead)
- [x] Runtime: `__rt_incref`, `__rt_decref_array`, `__rt_decref_hash`, `__rt_decref_object`
- [x] `unset()` uses decref (frees when refcount drops to zero)
- [x] GC statistics (`--gc-stats` flag: allocations, frees printed to stderr)
- [x] Strings freed on variable reassignment (value-copied, always owned)

### Known limitations
- Ordinary local/global reassignment now releases previous arrays/objects safely, and indexed array writes / associative-array writes / object property writes / `static` slots now retain borrowed heap values consistently
- Automatic epilogue cleanup has since been re-enabled for locals proven to own heap values; the remaining gaps are conservative control-flow merges and cyclic graphs
- Assoc-derived and broader container-copy paths now retain borrowed heap values consistently; the main remaining memory-model work has moved to targeted cycle collection, richer debug instrumentation, and tighter ownership precision

## v0.12.x — Math coverage (done)

### Trigonometry
- [x] `sin()`, `cos()`, `tan()`
- [x] `asin()`, `acos()`, `atan()`, `atan2()`
- [x] `deg2rad()`, `rad2deg()`
- [x] `sinh()`, `cosh()`, `tanh()`

### Logarithms and exponentials
- [x] `log()` — natural logarithm
- [x] `log2()`, `log10()`
- [x] `exp()` — e^x

### Utility
- [x] `hypot()` — sqrt(x² + y²)
- [x] `pi()` — alias for M_PI

### Constants
- [x] `M_E`, `M_SQRT2`, `M_PI_2`, `M_PI_4`, `M_LOG2E`, `M_LOG10E`
- [x] `PHP_FLOAT_MIN`, `PHP_FLOAT_EPSILON`

## v0.13.x — Pointers

- [x] Opaque pointer type (`ptr`) for handles and `void*`
- [x] Typed pointer tags via `ptr_cast<T>()` for annotating raw addresses with a checked pointee type
- [x] Pointer builtins: `ptr()`, `ptr_null()`, `ptr_is_null()`, `ptr_offset()`, `ptr_cast<T>()`, `ptr_get()`, `ptr_set()`
- [x] Raw buffer pointer builtins: `ptr_read8()`, `ptr_read32()`, `ptr_write8()`, `ptr_write32()`
- [x] `ptr_sizeof()` — returns byte size of a type (`"int"` → 8, `"float"` → 8, class name → computed)
- [x] Pointer echo: `echo $ptr` prints hex address (`0x...`)
- [x] Pointer comparison: `===`, `!==` between pointer values

## v0.14.x — FFI (Foreign Function Interface)

- [x] `extern function` declarations with C type annotations (`int`, `float`, `string`, `bool`, `void`, `ptr`)
- [x] `extern "libname" { }` blocks (auto `-l` linker flag)
- [x] `extern "libname" function name(): type;` single-line syntax
- [x] `--link` / `-l` and `--link-path` / `-L` CLI flags
- [x] `--framework` flag for macOS frameworks
- [x] Owned null-terminated string ↔ length-prefixed string conversion (`__rt_str_to_cstr`, `__rt_cstr_to_str`)
- [x] `extern class` for C struct mapping (flat layout, available to `ptr_sizeof()` and typed pointer field access)
- [x] `extern global` for accessing C global variables
- [x] Callback support: pass elephc functions as C function pointers (`callable` params)
- [x] C memory management via extern libc: `malloc()`, `free()`, `memcpy()`, `memset()`
- [x] Native interop validation examples: raw FFI memory + SDL2 window/input/framebuffer/audio demos

## v0.15.x — Memory model hardening

- [x] Ownership lattice for heap values in codegen (`Owned` / `Borrowed` / `MaybeOwned` / non-heap)
- [x] Re-enable epilogue cleanup for locals that are proven to own their heap values
- [x] Broader container propagation rules for nested array/hash/object transfers
- [x] Focused regressions for aliasing across locals, returns, nested containers, and scope exit
- [x] Heap allocator improvements: adjacent-block coalescing and less fragmentation under mixed allocation sizes
- [x] Runtime heap verification / debug mode (`double free`, bad refcount, free-list corruption checks)
- [x] Uniform runtime heap-kind metadata for arrays / assoc arrays / objects / persisted strings
- [x] Evaluate a cycle-collection strategy for circular container/object graphs
- [x] Introduce targeted cycle collection for circular array/hash/object graphs
- [x] Add uniform `__rt_decref_any` / heap-kind-based release dispatch for mixed heap values
- [x] Emit richer runtime metadata for refcounted object/container payload scanning
- [x] Extend `--heap-debug` with leak summaries, high-watermark stats, and freed-block poisoning
- [x] Introduce segregated free lists / size classes to reduce allocator scan cost and fragmentation
- [x] Tighten ownership propagation in remaining conservative control-flow / merge paths
- [x] Formalize FFI heap ownership boundaries for borrowed vs owned native buffers and strings

## v0.16.x — Language and runtime expansion

- [x] Copy-on-write arrays — PHP-style shared-until-modified semantics with a COW flag in array headers and copy-on-mutation
- [x] Inheritance (`extends`) — vtable-based method dispatch, property layout chaining, and `self::` / `parent::` / `static::` calls
- [x] Interfaces / abstract classes — interface method tables and compile-time conformance checking
- [x] `instanceof` — class/interface runtime metadata checks with inheritance, interface inheritance, `self`, `parent`, and late-bound `static`
- [x] Traits — compile-time method copying / inlining with `use`, `as`, `insteadof`, and trait properties
- [x] Exceptions (`try`/`catch`) — stack unwinding via `setjmp`/`longjmp` with runtime frame cleanup and `finally` support
- [x] Hash table insertion order — preserve PHP associative-array insertion order with a secondary linked list through entries
- [x] Mixed-type associative arrays — per-entry type tags instead of one value type per table
- [x] String indexing (`$str[$i]`) — lower to one-character slice syntax as sugar for string reads
- [x] `protected` visibility — third visibility level between public and private
- [x] Magic methods (`__toString`, `__get`, `__set`) — implicit hooks on property access and string conversion
- [x] ifdef or similar support

## v0.17.x — Language maturity and compiler ergonomics

- [x] Hot-path data type
- [x] Full namespace support
- [x] Comprehensive error recovery (multiple errors per compilation)
- [x] Warning system (unused variables, unreachable code)
- [x] Enums (`enum Color { Red; Green; Blue; }`) — backed enums with `->value`, `::from()`, `::cases()`
- [x] Named arguments (`foo(name: "Alice", age: 30)`) — reorder args at compile time based on parameter names
- [x] First-class callable syntax (`strlen(...)`) — create closures from function names without string indirection
- [x] `match` with no-match error — runtime fatal when no arm matches and no default
- [x] Readonly classes (`readonly class Point {}`) — all properties implicitly readonly
- [x] Final classes, methods, and properties (`final class Foo {}`, `final public function run() {}`, `final public $id`) — compile-time inheritance and override enforcement
- [x] Union types (`int|string`) — tagged union with runtime type dispatch
- [x] Nullable types (`?int`) — sugar for `int|null`
- [x] Function / method parameter and return type hints (`function foo(int $x): string`) — compile-time validation for functions, methods, constructor parameters, closures, arrow functions, and non-`void` return-path coverage
- [x] Constructor property promotion (`public function __construct(public int $x)`) — promoted parameters lower to declared properties plus constructor assignments, including visibility, `readonly`, defaults, nullable/union type declarations, and by-reference promoted parameters

## v0.18.x — Multi-platform and optimizations

- [x] Linux x86_64 target
- [x] Linux ARM64 target
- [x] Split `src/codegen/expr.rs` into a slim dispatcher plus smaller focused helpers
- [x] Split `src/codegen/stmt.rs` into a slim dispatcher plus smaller focused helpers
- [x] Target support matrix: `macos-aarch64`, `linux-aarch64`, `linux-x86_64`

## v0.19.x — Tooling, compiler throughput, and optimization

- [x] Runtime object cache — pre-assemble the runtime into `~/.cache/elephc/runtime-<version>-<runtime-hash>.o` and reuse across compilations, invalidating on compiler version, target, heap size, or generated runtime assembly changes. Cuts repeated compile time by ~50%.
- [x] Benchmark suite (vs C, vs PHP interpreter)
- [x] Source maps (assembly ↔ PHP line mapping)
- [x] Compiler timing / profiling output for parse, typecheck, codegen, assemble, and link phases
- [x] Benchmark automation — run the benchmark harness in CI, publish markdown summaries and JSON artifacts, and use it as a correctness/trend gate without noisy hard thresholds
- [x] Constant folding (`2 + 3` → `5` at compile time)
- [x] Dead code elimination
- [x] Add regression benchmarks so optimization work is measured instead of anecdotal
- [x] Constant propagation across locals / statement boundaries
- [x] Path-aware dead code elimination foundations — shared reachability/tail-path analysis for `if` / `ifdef` / `switch` / `try`, shadowed handler/pattern removal, and guard-aware nested region pruning
- [x] Purity / may-throw analysis so AST optimizations can reason more precisely about safe hoisting and branch removal
- [x] Exception-aware dead code elimination beyond conservative `try` / `catch` / `finally` heuristics — catch/finally guard invalidation now tracks pre-handler throw paths, including CFG-pruned switch paths
- [x] Control-flow normalization pass for flattening redundant nested `if` / `switch` / `try` shells after pruning
- [x] Alias-aware constant propagation so local callables and scalar values can stay precise across `if` / `switch` / `try` merges
- [x] Relational and loose-comparison contradiction guards for dead-code elimination
- [x] Advanced static property parity — PHP-style static property redeclaration rules and direct array element writes such as `ClassName::$items[] = $value`
- [x] Short ternary operator `?:` — PHP Elvis form with single evaluation of the left-hand expression
- [x] `print` expression form — writes output and returns `1`, including statement-form `print $x;` as `ExprStmt(Print(...))`
- [x] Constant propagation v2 — known-subject `switch` path merges, non-throwing `try` / unreachable-catch env merges, known `match` folding, and scalar indexed/associative array-literal access folding
- [x] Constant propagation v3 — local loop path summaries for `while(false)`, `do...while(false)`, `while(true)` / `for(;;)` break exits, branch-local loop-exit merges, and safe pruning around `do...while(false)` loop exits
- [x] PHP-compatible magic constants: `__DIR__`, `__FILE__`, `__LINE__`, `__FUNCTION__`, `__CLASS__`, `__METHOD__`, `__NAMESPACE__`, `__TRAIT__` (case-insensitive names, per-file include scope, closure names, trait `__CLASS__` rebinding)
- [x] Compile-time-constant expressions in `include` / `require` paths (string literals, concat, magic constants, namespace-aware `const` / `use const` / `define()` refs)
- [x] Error-control operator `@` backed by a suppressible runtime warning channel and exception-safe suppression-depth restoration
- [x] Runtime-value compatibility pass for `strpos()` / `strrpos()` / `array_search()` / `file_get_contents()` false-return conventions and `define()` boolean duplicate behavior
- [x] Static closures: `static function() { }` and `static fn() => ...` (no `$this` capture)
- [x] PHP array union operator `+` for indexed+indexed and associative+associative arrays, preserving left-side duplicate keys and associative insertion order
- [x] PHP-compatible associative array key normalization for integer keys and numeric-string keys across literals, reads/writes, `foreach`, `array_keys()`, `array_search()`, `array_key_exists()`, `array_flip()`, `json_encode()`, and associative array union
- [x] PHP-compatible octal integer literals: legacy leading-zero octal (`0755`, `0_755`) and PHP 8.1 explicit octal (`0o755` / `0O755`) alongside the existing decimal and hexadecimal forms
- [x] PHP 5.4 binary integer literals (`0b1010` / `0B1010`)
- [x] PHP 7.4 numeric separators (`1_000_000`, `0xFF_FF`, `0b1010_1010`, `0o7_7_7`, `1_000.5`, `1e1_0`) across decimal, hex, octal, binary, and float literals
- [x] Trailing-character validation on numeric literals (rejects `0o78`, `078`, `0xfg`, `0b12`, `1_`, `1__0` at lex time instead of silently splitting tokens)
- [x] Support for `never` return type
- [x] `iterable` pseudo-type runtime parity — `foreach` over indexed-array, hash-backed, `Iterator`, and `IteratorAggregate` iterables; `echo`, `gettype()`, `var_dump()`, `===`, scalar casts (`(int)`, `(float)`, `(string)`, `(bool)`), and the `is_iterable()` builtin all dispatch through heap-kind, value-type, or interface metadata where needed
- [x] Filesystem modification: `touch()` (with optional nullable `$mtime` / `$atime`, explicit numeric timestamps including `-1`, and PHP-style `0666 & umask` creation mode), `chmod()`, `chown()` / `chgrp()` with numeric IDs or string names, `umask()` (with the no-arg probe form), `ftruncate()`, `fflush()` (implemented as `fsync()`), `fsync()`, and `fdatasync()` (with a Darwin `fsync` fallback). Runtime paths use libc where practical, with target-aware file-creation handling for `touch()`.
- [x] Filesystem path manipulation: `basename()`, `dirname()` including multi-level parent lookup, `pathinfo()` (component flags returning strings plus no-flag / `PATHINFO_ALL` associative-array forms), `realpath()`, `fnmatch()` with shell-glob `*` / `?` / `[...]` / `\\` support and zero flags, plus the `PATHINFO_DIRNAME` / `PATHINFO_BASENAME` / `PATHINFO_EXTENSION` / `PATHINFO_FILENAME` / `PATHINFO_ALL` constants
- [x] Filesystem metadata coverage: scalar getters (`fileatime()`, `filectime()`, `fileperms()`, `fileowner()`, `filegroup()`, `fileinode()`, `filetype()`) with PHP-compatible `false` on stat failure, permission predicates (`is_executable()`, `is_link()`, `is_writeable()` alias of `is_writable()`), full PHP-compatible `stat()` / `lstat()` / `fstat()` arrays (numeric `0..=12` + string keys `dev`/`ino`/`mode`/`nlink`/`uid`/`gid`/`rdev`/`size`/`atime`/`mtime`/`ctime`/`blksize`/`blocks`) with PHP-compatible `false` on failure, and `clearstatcache()` as a no-op (elephc has no stat cache)

## v0.20.x — PHP filesystem, expression, call, and callable parity closure

Close the small, well-scoped PHP filesystem/runtime compatibility gaps that are
already adjacent to the current implementation, plus the language-level PHP
behavior visible in everyday source code that does not require a new backend or
product mode. This is also the series that delivered the Fibers MVP.

- [x] `fnmatch()` non-zero flag parity for `FNM_PATHNAME`, `FNM_PERIOD`, `FNM_CASEFOLD`, and `FNM_NOESCAPE`
- [x] Dynamic `pathinfo($path, $flag)` parity for runtime flags that may evaluate to `PATHINFO_ALL`; runtime exact `PATHINFO_ALL` now returns the associative-array shape while component flags return strings
- [x] PHP resource type compatibility — model file handles and future extension handles separately from integers
- [x] `fopen()` failure parity — return `false` on open failure while keeping successful handles as `resource`, and make stream built-ins reject/handle the `resource|false` path without passing boxed `false` as a native descriptor
- [x] Runtime-dynamic include paths — explicitly reject runtime-evaluated `include` / `require` path expressions beyond the current compile-time string-folder (`$path`, function calls, ternaries, property access)
- [x] Runtime-order-aware `include_once` / `require_once` — add runtime guards inside functions, methods, loops, and conditional branches so skipped files match PHP execution order rather than only compile-time traversal order
- [x] Include graph declaration discovery — pre-scan all statically resolvable `include` / `require` targets for function/class/interface/trait declarations before name resolution and type checking, so symbol references are not sensitive to source include order while top-level include execution order remains PHP-compatible
- [x] Path-sensitive include declaration discovery — avoid false duplicate declaration errors when the same statically resolvable regular `include` / `require` target is reachable only through mutually exclusive control-flow paths, while still reporting duplicates for sequential loads and potentially repeated loop loads
- [x] Runtime-loaded include function dispatch — compile include-discovered functions behind public dispatchers activated at each real include point, so direct calls and `function_exists()` follow PHP runtime load order while type checking can still see the include graph
- [x] Conditional include function variants — when mutually exclusive `if` / `elseif` / `else` branches include different files that declare the same function name with identical signatures, compile each declaration as a hidden variant and dispatch the public function name to the variant loaded at runtime
- [x] Mixed nullsafe/member chains — match PHP's full chain semantics for forms that mix `?->` and `->`, such as `$a?->b->c`
- [x] Dynamic `instanceof` targets — support PHP forms such as `$obj instanceof $className` with runtime validation for class-string/object target expressions
- [x] Full PHP list destructuring — skipped entries, nested patterns, associative-key destructuring, and non-local destructuring targets where PHP permits them
- [x] Named-argument parity for built-ins, extern calls, and spread — extend validation/lowering outside user-defined calls and handle spread interactions
- [x] Fibers MVP — `Fiber` and `FiberError` built-in classes, `start()` / `resume()` / `suspend()` / `throw()` / `getReturn()`, state predicates, `Fiber::getCurrent()`, closure captures, uncaught-exception propagation through the caller, guarded per-fiber `mmap` stacks, and context switching on ARM64 plus Linux x86_64
- [x] Full first-class callable targets — support `static::method(...)` and `$object->method(...)` in addition to function, `ClassName::`, `self::`, and `parent::` targets
- [x] Captured closures as callback values — forward hidden `use (...)` environments through callback-style built-ins such as `array_map`, `array_filter`, and `call_user_func`

### JSON parity polish (post v0.8.x base surface)

- [x] `JSON_BIGINT_AS_STRING` in `json_decode()`: integer-grammar tokens (no `.`, no `e`/`E`) whose magnitude exceeds PHP_INT_MAX (`9223372036854775807`) promote to a `Mixed(string)` preserving the original digits; in-range integers and float-grammar tokens are unaffected. Length-then-lex compare against the threshold strings `9223372036854775807` (positive) / `-9223372036854775808` (negative) detects overflow without invoking `__rt_atoi` (which silently wraps via `imul`).
- [x] Lone UTF-16 surrogate detection in `json_decode()` / `json_validate()`: every `\uXXXX` escape in the high-surrogate range (`0xD800..0xDBFF`) must be immediately followed by a `\uYYYY` escape in the low-surrogate range (`0xDC00..0xDFFF`). Unpaired high surrogates and stand-alone low surrogates set `JSON_ERROR_UTF16` (10) and raise `JsonException` under `JSON_THROW_ON_ERROR`, with the PHP-faithful message `Single unpaired UTF-16 surrogate in unicode escape`. The string parser accumulates each escape into a 16-bit codepoint and walks the surrogate-pair handshake before resuming content scanning.
- [x] PHP-strict depth semantics for `json_decode()` / `json_validate()`: PHP rejects when the active nesting depth equals the `$depth` argument (`active >= limit`), so a flat array fails at depth=1 and a one-level-nested container fails at depth=2. The shared `__rt_json_depth_enter` keeps the lenient `active <= limit` rule (used by `json_encode()` per PHP), and the decode/validate dispatchers pass `$depth - 1` as the runtime limit so both surfaces match PHP exactly without forking the depth helper.
- [x] `Exception::$code` and `Exception::getCode(): int`. The constructor now accepts an optional `$code = 0` second argument and stores it as a `protected int` property. `JsonException` thrown via `JSON_THROW_ON_ERROR` carries the originating `JSON_ERROR_*` code, so `catch (JsonException $e) { $e->getCode(); }` matches PHP exactly (4 = SYNTAX, 1 = DEPTH, 10 = UTF16, 7 = INF_OR_NAN, etc.). User-code `new Exception("msg", $code)` also surfaces the value through `getCode()`.
- [x] `is_callable($value): bool` builtin. Compile-time decision when the value is a string literal (resolves against the catalog + user functions; case-insensitive for builtins) or a `Callable`-typed expression (closures, first-class callables). Non-literal strings, `[$obj, "method"]` arrays, and `__invoke` objects route to a future runtime helper.
- [x] Cache `_json_active_flags` in a callee-saved register (`x19` ARM64 / `r15` x86_64) inside `__rt_json_encode_str`: 8 reload sites collapse to a single-instruction `tst`/`test` against the cached register, eliminating one address-load + memory dereference per HEX_*/UNESCAPED_*/UTF-8 dispatch in the per-byte escape loop.

## v0.21.x — PHP runtime and language parity

Close broad PHP-visible parity gaps across the value model, modern syntax,
runtime helpers, and standard-library surfaces.

- [x] Heterogeneous indexed arrays — allow mixed payloads in indexed arrays instead of requiring homogeneous indexed values
- [x] PHP 8.0 attribute syntax — `#[Name]`, `#[Name(args)]`, stacked groups (`#[A] #[B]`), comma-separated within a group (`#[A, B(1)]`), qualified names (`#[\Ns\Name]`). Lexer adds `Token::AttrOpen` for `#[` ; bare `#` becomes a PHP-style line comment (no longer ambiguous). Parser invokes `parse_attribute_lists` at every site PHP allows: top-level statements, class/trait/interface members, enum cases, function/method parameters, closures, arrow functions. Attributes preserved in the AST via `attributes: Vec<AttributeGroup>` on `Stmt`, `ClassProperty`, `ClassMethod`, `EnumCaseDecl`, plus per-`ClassConst` storage on the new `ClassConst` AST type. Class-like declarations gain `constants: Vec<ClassConst>`.
- [x] `#[\Override]` enforcement — methods marked `#[\Override]` must override a parent-chain method; otherwise the type checker emits the PHP-faithful error `"<Class>::<method>() has #[\Override] attribute, but no matching parent method was found"`.
- [x] `#[\Deprecated]` warning — calls to functions/methods marked deprecated emit `"Call to deprecated function: <name>()"` warnings, optionally appending the user-supplied reason. Reason extraction lives in `types::checker::schema::validation::extract_deprecation` and threads through `FunctionSig::deprecation`.
- [x] User-defined `#[Attribute]` declarations — classes marked `#[Attribute]` parse without error and accept argument lists at usage sites (`#[MyAttribute("test")] class C {}`).
- [x] Generators / `yield` MVP — `Generator` built-in class with `yield`, `yield $k => $v`, generator functions and captured generator closures, `$x = yield` resume assignment, boxed `Generator::send()` payload delivery, `Generator::throw()`, `Generator::getReturn()`, terminal `return <expr>`, state-machine codegen backed by heap-allocated `GeneratorFrame` objects on ARM64 and Linux x86_64, and yield-context validation that rejects `yield` outside functions or inside `try`/`catch`/`finally`
- [x] `yield from` delegation — forward iteration through compile-time array literals, direct generator calls, and local generator variables, including case-insensitive `from` parsing and cleanup of owned direct-call delegates after completion
- [x] Filesystem stream extensions: `fgetc()` (thin wrapper over `fread`, length 1, returning `false` at EOF/read failure), `readfile()` (open + chunked read+write to stdout + close, returns bytes copied, `-1` on read failure, or `false` on open failure), `fpassthru()` (same loop on an already-open fd, returning `-1` on read failure), `flock()` (libc `flock` with PHP→POSIX `LOCK_UN` translation, preserves the `LOCK_NB` flag, and supports the optional `$would_block` output), and `tmpfile()` (`mkstemp("/tmp/elephc-XXXXXX")` + immediate `unlink` so the file auto-deletes on close, returns a PHP `resource|false`). Also predefines `LOCK_SH=1`, `LOCK_EX=2`, `LOCK_UN=3`, `LOCK_NB=4` constants matching PHP's numbering.
- [x] Filesystem symbolic links: `symlink($target, $link)`, `link($target, $link)`, `readlink($path)` (returns owned heap string boxed as Mixed for the `string|false` convention), and `linkinfo($path)` (returns `st_dev` or PHP's `-1` failure sentinel). All routed through libc to avoid per-syscall remapping work.
- [x] PHP 8.5 pipe operator (`|>`) — left-associative, lower precedence than additive operators, supporting first-class callables, static and instance methods, closures, and variable callables; rejects by-reference callable parameters
- [x] PHP attributes runtime introspection — implement `ReflectionClass::getAttributes()`, `ReflectionMethod::getAttributes()`, `ReflectionProperty::getAttributes()`, plus `ReflectionAttribute::newInstance()`. Class/member declarations expose attribute names and supported literal args through helper builtins and Reflection objects; `ReflectionAttribute::newInstance()` constructs the attribute class on demand from the captured literal args.
- [x] Mixed indexed/associative array union — model `array + array` across indexed/hash representations while preserving PHP's shared int/string key space and left-key precedence
- [x] Callable parity follow-up — support captured method/static first-class callables in the remaining callback runtimes (`array_reduce()`, `array_walk()`, `usort()`, `uksort()`, `uasort()`), direct callable expression calls such as `($obj->method(...))()`, non-local method receivers such as `(new Foo())->method(...)`, nullsafe first-class callables, broader builtin first-class callable wrappers, and the remaining `call_user_func_array()` by-reference callback gaps
- [x] Runtime-value compatibility polishing v2 — uninitialized typed instance/static property reads fail with PHP-style fatal diagnostics; constant-folded and non-folded runtime integer `+`/`-`/`*` overflow promotes to double; scalar loose comparisons cover PHP bool truthiness, null-vs-empty-string, numeric-string, and non-numeric string byte-comparison rules at constant-fold and runtime helper sites. Warning/notice sites added so far route through the suppressible runtime diagnostics channel.
- [x] Broader date and regex PHP parity — expand `strtotime()` relative formats with `a/an <unit>` article offsets, add `preg_replace()` capture backreference expansion (`$0`..`$9`, `\0`..`\9`), and move preg runtimes to PCRE2-backed matching (JSON parity now closed: see v0.8.x base + v0.20.x polish)
- [x] JSON encoder optimization — folded `__rt_json_assoc_is_list_shape` into the main associative-array encoding walk. `__rt_json_encode_assoc` now emits a provisional object form, tracks whether keys remain `0..count-1` while iterating the hash once, and compacts the finished buffer in-place to `[...]` only for real list-shape payloads. Object-shape inputs still stay object form, and `JSON_FORCE_OBJECT` disables compaction.
- [x] JSON decoder optimization — fused the `__rt_json_validate` pre-pass into `__rt_json_decode_mixed` for `json_decode()`. The wrapper now calls the checked structural decoder directly; the decoder trims the input once, validates scalar strings/numbers at the point where they are decoded, enforces depth around containers, records syntax/depth/UTF-16 errors internally, and returns null-on-error for the PHP-facing wrapper. `json_validate()` keeps the standalone RFC 8259 validator surface.
- [x] JSON encoder optimization — extended the `_json_active_flags` callee-saved-register cache to `__rt_json_encode_assoc` and `__rt_json_encode_array_dynamic` (`x19` ARM64 / `r15` x86_64). The recursive encoder chain now preserves that cache: `__rt_json_encode_object` no longer clobbers ARM64 `x19`, and the x86_64 string encoder keeps `r15` dedicated to cached flags during UTF-8 decoding.
- [x] JSON pretty-print optimization — inline indent emission inside each container encoder (assoc, array_int/str/dynamic, object) and retire the `__rt_json_pretty_apply` post-processor. Eliminates the second buffer walk for JSON_PRETTY_PRINT workloads. Multi-day refactor completed with a `_json_indent_depth` BSS slot, balanced normal-path formatting depth maintenance, reset-at-entry protection across throws, and bytewise PHP cross-check coverage on representative payloads.
- [x] `is_callable()` runtime fallback — handle non-literal strings, `[$obj, "method"]` arrays, and objects implementing `__invoke`. The string-literal + Callable-typed compile-time path is already in place.
- [x] Case-insensitive user-function lookup — `function_exists("USER_FN")` and `is_callable("USER_FN")` accept any case for user functions through a shared lookup path, matching PHP's function-name rules.
- [x] OOP property parity v2 — PHP 8.4 property-hook contracts now cover interface properties and abstract properties in traits/classes; `readonly static` remains rejected like PHP, instance property redeclaration validates hook get/set contracts, and by-reference constructor promotion now rejects readonly aliases at compile time while supporting default-value reference cells.

### Standard PHP Library (SPL)

PHP-compatible Standard PHP Library coverage, rolled out in phases.

- [x] Phase 1 — built-in interfaces: `Traversable`, `Iterator` (extends `Traversable`), `IteratorAggregate` (extends `Traversable`), `OuterIterator`, `RecursiveIterator`, `SeekableIterator`, `Countable`, `ArrayAccess`, `SplObserver`, `SplSubject`, `Stringable`, `JsonSerializable`
- [x] Phase 2 — `count($obj)` redirects to `Countable::count()`
- [x] Phase 3 — SPL exception hierarchy: `LogicException`, `BadFunctionCallException`, `BadMethodCallException`, `DomainException`, `InvalidArgumentException`, `LengthException`, `OutOfRangeException`, `RuntimeException`, `OutOfBoundsException`, `OverflowException`, `RangeException`, `UnderflowException`, `UnexpectedValueException`
- [x] Static autoload — composer.json `autoload.psr-4` driven, includes `vendor/<vendor>/<package>/composer.json`. Replaces runtime autoload by inlining every reachable PSR-4 class at compile time
- [x] `spl_autoload_*` stubs — `register`/`unregister` return `true`, `functions` returns `[]`, `extensions` returns `".inc,.php"`, `call` and `spl_autoload` are no-ops. Defensive code that calls these at boot compiles unchanged
- [x] Closure-aware `spl_autoload_register` — the closure body is evaluated symbolically at compile time. Supports `__DIR__ . '/' . str_replace('\\', '/', $name) . '.php'` style autoloaders, intermediate variable assignments, and `if (file_exists(...))` guards. `spl_autoload_unregister` removes matching rules; `spl_autoload_call("App\\Foo")` with a literal name forces compile-time autoload of that class
- [x] Runtime r/w for `spl_autoload_extensions` — backed by mutable globals (`_spl_autoload_exts_ptr` / `_spl_autoload_exts_len`) initialized to `".inc,.php"`. Read returns the current value; write swaps in the new and returns the previous, matching PHP semantics
- [x] `spl_autoload_functions()` returns an indexed array sized to the number of registered closure rules — `count()` and `foreach` see one entry per rule
- [x] Read `autoload.classmap`, `autoload.files`, `autoload.psr-0`, and `autoload-dev.*` sections from composer.json. Longest-prefix wins for PSR-4. PSR-0 supports both namespaced and underscore-class conventions
- [x] `class_exists` / `interface_exists` / `trait_exists` / `enum_exists` with literal class name and `autoload = true` (default) trigger compile-time autoload of the literal
- [x] Variable-stored closures and function-name string callables are accepted by `spl_autoload_register`. The closure assignment / function declaration is stripped from the program after the rule is extracted
- [x] Top-level `if (...)` whose condition folds to a literal bool flattens before rule collection — guard your register call with `if (true)`, `if (false)/else`, or chained `elseif` and the chosen branch is inlined at compile time
- [x] `sprintf`, `dirname`, `basename` are supported by the symbolic interpreter — common autoloader patterns like `require_once sprintf("%s/%s.php", __DIR__, $name)` and `dirname(__DIR__) . '/lib/' . $name . '.php'` fold at compile time
- [x] `get_declared_classes` / `get_declared_interfaces` / `get_declared_traits` return AOT introspection snapshots of the compiled symbol set
- [x] `class_alias($orig, $alias)` synthesises a subclass at compile time so `new $alias()` and `instanceof $alias` work as the user expects
- [x] `autoload.exclude-from-classmap` skips matching paths during classmap scanning. Supports glob patterns (`*`, `**`, `?`) plus the trailing-slash directory shorthand
- [x] PSR-4 empty namespace prefix `""` (root namespace) verified working
- [x] `realpath` and `pathinfo` (with `PATHINFO_*` flags) added to the symbolic interpreter
- [x] PSR-0 underscore-class convention (`Twig_Loader_Filesystem` → `lib/Twig/Loader/Filesystem.php`) verified
- [x] `spl_object_id`, `spl_object_hash`, `spl_classes` runtime helpers; pointer-based identity, stable per process
- [x] `get_class` / `get_parent_class` resolve via the argument's static type at compile time
- [x] `is_a` / `is_subclass_of` with literal class arg fold at compile time, walking parent chain and implemented interfaces
- [x] Compile-time warning when a `spl_autoload_register` closure is rejected (use captures, multi-param, variadic) — explains why the autoloader silently became a no-op

`Serializable` is intentionally not implemented — it has been deprecated since PHP 8.1.

## v0.22.x — Core parity cleanup before EIR

Close the small dispatch, lvalue, and runtime correctness gaps that should not
be carried through a backend migration.

- [x] `Throwable`-via-interface `getMessage()` dispatch fix (pre-existing): catching by `Throwable` and calling `getMessage()` on the typed binding returns garbage instead of the message string
- [x] `$obj[$k]` subscript syntax for `ArrayAccess` implementers (read, write, `isset`, `unset` paths), including `Mixed`-boxing for offsets and values
- [x] Raw pointer memory helpers for FFI/socket showcases: `ptr_read16()`, `ptr_write16()`, `ptr_read_string()`, `ptr_write_string()`
- [x] `IntrinsicCall` foundation for runtime-managed SPL/core objects when direct method interception is still the cleanest implementation path

## v0.23.x — SPL containers and iterator runway

Continue SPL coverage on the 0.x path, but do not let broad library coverage
block the EIR migration unless it exposes core dispatch, ownership, or lvalue
gaps that must be fixed first.

- [x] Phase 4 — `SplDoublyLinkedList`, `SplStack`, `SplQueue`, `SplFixedArray`
- [x] Runtime callable dispatch metadata foundation — shared AOT callable cases for entry-selected callbacks and runtime string-name user callbacks, reused by `call_user_func()`, `call_user_func_array()`, and `iterator_apply()`
- [x] Phase 5 storage foundation — `EmptyIterator`, `ArrayIterator`, `ArrayObject`
- [x] Phase 5 simple iterator decorators — `IteratorIterator`, `LimitIterator`, `NoRewindIterator`, `InfiniteIterator`
- [x] Phase 5 multi-source iterator decorators — `AppendIterator`, `MultipleIterator`
- [x] Phase 5 filter/cache decorators — `FilterIterator`, `CallbackFilterIterator`, `CachingIterator`
- [x] Phase 5 recursive iterator family — `RecursiveArrayIterator`, `RecursiveFilterIterator`, `RecursiveCallbackFilterIterator`, `RecursiveIteratorIterator`, `ParentIterator`
- [x] Phase 5 — iterator decorators (`ArrayIterator`, `ArrayObject`, `IteratorIterator`, `LimitIterator`, `NoRewindIterator`, `InfiniteIterator`, `EmptyIterator`, `AppendIterator`, `MultipleIterator`, `CallbackFilterIterator`, `FilterIterator`, `CachingIterator`, `RecursiveArrayIterator`, `RecursiveCallbackFilterIterator`, `RecursiveFilterIterator`, `RecursiveIteratorIterator`, `ParentIterator`); functions `iterator_to_array`, `iterator_count`, `iterator_apply`, `class_implements`, `class_parents`, `class_uses`
- [x] Runtime callable dispatch expansion — generated descriptor cases for dynamic string builtin callbacks and public `Class::method` strings, plus `call_user_func()` / `call_user_func_array()` support for invokable objects and callable arrays stored directly or in local variables
- [x] Runtime callable descriptor ABI/storage foundation — closure, first-class callable, SPL callback-adapter, object-property, array, local, and Fiber storage now carries descriptor pointers; indirect call sites and callback runtimes load the entry ABI slot before invocation
- [x] Universal runtime callable descriptors — complete runtime descriptor metadata for signature/default/by-ref/variadic handling, receiver/capture environments, and invocation support for string, array, closure, first-class callable, object `__invoke`, static/instance method, builtin, and extern callable shapes
- [x] Phase 5 follow-up — iterator-dependent Phase 4 parity: `SplFixedArray::getIterator()` plus `IteratorAggregate`/`InternalIterator` runtime wiring once iterator classes are available
- [x] Phase 6 — `SplHeap`, `SplMaxHeap`, `SplMinHeap`, `SplPriorityQueue`, `SplObjectStorage`, and per-instance handle finalization
- [x] Phase 7 — `RegexIterator`, `RecursiveRegexIterator`
- [x] Phase 8 — file/directory iterators: `SplFileInfo`, `SplFileObject`, `SplTempFileObject`, `DirectoryIterator`, `FilesystemIterator`, `GlobIterator`, `RecursiveDirectoryIterator`, `RecursiveCachingIterator`

### Streams and sockets

End-to-end PHP streams/sockets/network subsystem, rolled out in
phases on `feat/streams-sockets`. Each phase landed as an autonomous
increment with its own tests; the descriptions below summarize the
PHP-visible surface, not the internal commits.

- [x] Phase 1 — `is_resource`/`get_resource_type`/`get_resource_id` introspection, `STREAM_*`/`PSFS_*`/`FILE_*`/`GLOB_*` constants, `stream_isatty`/`stream_is_local`/`stream_supports_lock`/`stream_get_transports`/`stream_get_wrappers`/`stream_get_filters` stubs
- [x] Phase 2 — `stream_context_*` resources, `php://memory`/`temp`/`stdin`/`stdout`/`stderr` and `data://` pseudo-wrappers, `stream_get_contents`/`stream_copy_to_stream`
- [x] Phase 3 — filter chain (`stream_filter_append`/`prepend`/`remove`) with `string.toupper`/`tolower`/`rot13`/`zlib.deflate`/`zlib.inflate` built-ins
- [x] Phase 4a — TCP socket family (`stream_socket_client`/`server`/`accept`) and the `http://` wrapper
- [x] Phase 4b — `elephc-tls` staticlib bridge (rustls) and the `https://` wrapper, linked indirectly so non-https programs stay libc-only
- [x] Phase 5 — UDP and Unix-domain sockets (`udp://`, `unix://`, `udg://`), `stream_socket_sendto`/`recvfrom`, `stream_socket_pair`, `stream_socket_get_name`
- [x] Phase 6 — `ftp://` wrapper, `opendir`/`readdir`/`closedir`/`rewinddir` with the `glob://` directory wrapper
- [x] Phase 7 — memory streams (`php://memory`, `php://temp`), `stream_get_line`, `disk_free_space`/`disk_total_space`
- [x] Phase 8 — stream options (`stream_set_blocking`/`set_timeout`/`get_meta_data`, plus the chunk/read/write buffer stubs)
- [x] Phase 9 — network utilities (`gethostby*`, `ip2long`/`long2ip`, `inet_pton`/`ntop`, `getservby*`, `gethostname`), `popen`/`pclose`, `fsockopen` with by-reference error outputs, `stream_select`
- [x] Phase 10 — user-defined wrappers: `stream_wrapper_register("scheme", "Class")` plus `new $variable()` parser/runtime so `fopen("scheme://...")` instantiates `Class` and dispatches `stream_open`/`stream_read`/`stream_write`/`stream_close`/`stream_eof`/`stream_seek` through the regular method ABI. fopen returns synthetic descriptors in the `0x40000000+` range; `fread`/`fwrite`/`fclose`/`feof`/`fseek` detect those descriptors and tail-call dedicated wrapper helpers instead of touching the libc-fd-indexed runtime tables
- [x] Phase 11 — maximal-parity push (rebased onto current `origin/main`): feof-first whole-stream drains so `stream_get_contents`/`fpassthru`/`fgets`/`stream_copy_to_stream` work on userspace-wrapper descriptors without corrupting the caller's resource cell; `fgetc`/`rewind` wrapper dispatch; `php://filter/[read=|write=]F/resource=…` wrapper; `fprintf`/`fscanf`; `convert.iconv.<from>/<to>` charset filter via libc `iconv` (macOS auto-links `-liconv`); `ssl.cafile` custom CA bundle (plus the existing `ssl.verify_peer=0`) for `https://`; socket context options including `socket.so_broadcast`; `pfsockopen`. Also previously: `compress.bzip2://` and `ftps://` (RFC 4217) wrappers, `tcp_nodelay`/`so_reuseport`/`bindto`/`ipv6_v6only` socket options
- [x] Phase 11 follow-up — pre-existing-bug fixes surfaced by the streams work: bzip2 x86_64 frame alignment (qemu SIGSEGV), `sscanf`/`fscanf` `%f`, type-aware out-of-bounds array-read null fallback, and string-search builtins (`strpos`/`str_contains`/`str_starts_with`/`str_ends_with`/`strstr`/`strrpos`) coercing Mixed/`string\|false` operands on x86_64
- [x] Phase 11 follow-up — `bzip2.compress` / `bzip2.decompress` stream filters (libbz2, indirect-fn-pointer pattern; interoperable with PHP's `bzcompress`/`bzdecompress`)
- [x] Phase 11 follow-up — write-direction `convert.iconv.*` filter (`STREAM_FILTER_WRITE` transcodes each `fwrite` via libc `iconv` through the same indirect-fn-pointer write-filter mechanism; id 12)
- [x] Phase 11 follow-up — `socket.backlog` context option (`__rt_socket_backlog` reads `['socket']['backlog']` and feeds the TCP/IPv6/Unix `listen()` backlog; default 128) and `ftp.resume_pos` (`REST <N>` before `RETR`, shipped earlier)
- [x] Phase 12 — userspace-wrapper coverage completion: property-default init on dynamic `new $var()` / registered wrappers + filters (per-class `_class_propinit_<id>` thunk run by `__rt_new_by_name`); `fstat()` via `stream_stat`; path-based `file_exists()`/`filesize()`/`is_file()` via `url_stat(string $path, int $flags)` (vtable 8→10, a `__rt_path_is_wrapper` scheme matcher, and a shared `__rt_box_wrapper_stat_result`); `readfile()` on wrapper URLs (fopen + feof-gated drain + close); `fgetcsv()` and `stream_get_line()` on wrapper handles (runtime `__rt_fgets`/`__rt_stream_get_line` gained a feof-gated `__rt_fread` loop accumulating into `_user_wrapper_drain_buf`). Stat methods must be declared without a return type so the assoc stat array round-trips as a boxed Mixed
- [x] Phase 13 — TLS stream-context ssl options: `ssl.capath` (a directory of PEM CA certs → trust anchors, via `elephc_tls_connect_capath`), `ssl.peer_name` (verify the cert / send SNI for a name other than the connection host, via `elephc_tls_connect_peer_name`), and `ssl.allow_self_signed` / `ssl.verify_peer_name = "0"` (relaxed peer verification — encrypted but unauthenticated, routed through the existing insecure verifier; elephc does not distinguish self-signed acceptance from a full identity skip). https dispatch priority: cafile → capath → relaxed → peer_name → default trust store
- [x] Phase 14 — `vsprintf`/`vprintf`/`vfprintf`: the array→variadic bridge `__rt_vsprintf` reads the arguments array (unboxing Mixed-cell slots; reading int/float/bool/string typed slots directly), pushes one 16-byte tagged record per element in reverse order, and tail-calls `__rt_sprintf` (which formats and pops the records). `vprintf` writes the result to stdout; `vfprintf` writes it to a stream via `__rt_fwrite`. All three return PHP-faithful results
- [x] Phase 15 — `phar://` Milestone-1 (read-only, uncompressed): `fopen("phar://archive.phar/entry", "r")` parses the native PHAR manifest at compile time (locating `__HALT_COMPILER();`, reading the little-endian manifest header and per-entry records, slicing the entry's bytes from the data section) and embeds the uncompressed entry, served through the shared `__rt_data_stream` helper — mirroring `data://`. `stream_get_wrappers()` advertises `phar` honestly again. A missing archive/entry or a compressed entry returns `false`
- [x] Phase 16 — `phar://` Milestone-2 (gzip-compressed entries): PHP stores gzip phar entries as raw DEFLATE; the compiler inflates them at compile time via `flate2` (pure-Rust `miniz_oxide` backend, decompress-only) and embeds the result, so reading a gzip entry through `phar://` is transparent
- [x] Phase 17 — `phar://` Milestone-2b (bzip2-compressed entries): bzip2 phar entries (standard `BZh` stream) are decompressed at compile time via `bzip2-rs` (pure-Rust decoder, decompress-only — no system libbz2 or C toolchain, keeping the compiler build portable). `phar://` now reads uncompressed, gzip, and bzip2 entries
- [x] Phase 18 — `crc32()` builtin: pure table-free `__rt_crc32` (reflected polynomial `0xEDB88320`, init/final XOR `0xFFFFFFFF`, no system lib), dual-arch, returning the non-negative 32-bit checksum as a 64-bit int. A genuine missing PHP builtin and the prerequisite for `phar://` writing (PHP verifies per-entry CRC32 on read). Verified against PHP reference vectors
- [x] Phase 19 — `stream_socket_enable_crypto` confirmed real TLS: already a working rustls implementation (`elephc_tls_attach_fd` over the connected fd, fread/fwrite routed through the session). Added an `#[ignore]`'d end-to-end test (real HTTPS host, SNI from `ssl.peer_name`) and corrected the stale docblock. Refinement still open: auto-default SNI to the connection host when no context peer-name is set
- [x] Phase 20 (G1a) — userspace-wrapper vtable widened 10→23 reserving the full PHP `StreamWrapper` surface (stream_cast/lock/truncate/set_option/metadata/unlink/rename/mkdir/rmdir/dir_*); `flock()` dispatches to `stream_lock(int $operation)` (slot 11, `__rt_user_wrapper_flock`) and `ftruncate()` to `stream_truncate(int $new_size)` (slot 12, `__rt_user_wrapper_ftruncate`), threading the operation/size through and returning the wrapper's bool (false when the method is absent); normal fds keep the libc path. ARM64 + x86_64
- [x] Phase 21 (G1b) — userspace-wrapper path-op dispatch: `unlink()` (slot 15), `rename()` (slot 16), `mkdir()` (slot 17), `rmdir()` (slot 18) on a registered `scheme://` path route to the wrapper's same-named method via the new `__rt_user_wrapper_path_op` (single-path) / `__rt_user_wrapper_rename` (two-path) runtime helpers; each builtin gates on `__rt_path_is_wrapper` (the `readfile()` split) and otherwise keeps the libc path. A wrapper missing the method, or a non-wrapper path, returns false / uses the filesystem. ARM64 + x86_64 (both Docker-verified)
- [x] Phase 22 (G1c) — userspace-wrapper `stream_metadata` dispatch: `chmod()` on a registered `scheme://` path routes to the wrapper's `stream_metadata($path, STREAM_META_ACCESS, $mode)` (vtable slot 14) via the shared `__rt_user_wrapper_path_op` helper (option/value threaded as the a3/a4 args); a non-wrapper path keeps libc `__rt_chmod`, a wrapper without `stream_metadata` returns false. ARM64 + x86_64 (both Docker-verified)
- [x] Phase 23 (G1d) — userspace-wrapper `stream_set_option` dispatch: `stream_set_blocking()` (option `STREAM_OPTION_BLOCKING`) and `stream_set_timeout()` (option `STREAM_OPTION_READ_TIMEOUT`) on a synthetic wrapper fd route to the wrapper's `stream_set_option($option, $arg1, $arg2)` (vtable slot 13) via the new fd-based `__rt_user_wrapper_set_option` runtime helper; a normal fd keeps the libc `fcntl`/`setsockopt` path, and a wrapper without `stream_set_option` returns false. ARM64 + x86_64 (both Docker-verified)
- [x] Phase 24 (G1e) — userspace-wrapper directory iteration: `opendir("scheme://")` instantiates the wrapper and calls `dir_opendir` (vtable slot 19), allocating a handle in the shared `_user_wrapper_handles` table and returning the same `0x40000000|slot` synthetic fd as a stream handle; `readdir`/`closedir`/`rewinddir` branch on that fd to `dir_readdir` (20), `dir_closedir` (21, frees the slot like fclose), and `dir_rewinddir` (22). `__rt_opendir` falls through to `glob://`+libc when no registered scheme matches. ARM64 + x86_64 (both Docker-verified)
- [x] Phase 25 (G1f) — userspace-wrapper `stream_metadata` ownership dispatch: `chown()` with an integer uid routes to the wrapper's `stream_metadata($path, STREAM_META_OWNER, $uid)` (vtable slot 14) and `chgrp()` with an integer gid to `stream_metadata($path, STREAM_META_GROUP, $gid)`, via the shared `emit_owner_group_wrapper_dispatch` helper over `__rt_user_wrapper_path_op`; a non-wrapper path keeps libc `__rt_chown`, a wrapper without `stream_metadata` returns false. String owner/group names and `touch()` arrays remain libc (deferred, need boxed-Mixed value passing); `stream_cast` deferred (stream_select cannot select on synthetic fds). ARM64 + x86_64 (both Docker-verified)
- [x] Phase 26 (G1g) — userspace-wrapper `stream_metadata` value as boxed `mixed`: PHP passes `stream_metadata`'s `$value` as `mixed`, so the dispatch now always boxes the value into an owned `Mixed` cell (`__rt_mixed_from_value`), passes the pointer as the method's 4th arg, and releases it with `__rt_decref_mixed` after the call (the callee borrows; wrappers declare `mixed $value`). Completes the metadata surface: `chmod()`/`chown()`/`chgrp()` by integer reuse this; `chown()`/`chgrp()` by string name dispatch via `STREAM_META_OWNER_NAME` (2) / `STREAM_META_GROUP_NAME` (4) through the new `emit_owner_group_name_wrapper_dispatch`; `touch()` builds the `[mtime, atime]` int array (`STREAM_META_TOUCH` = 1) via the new `__rt_touch_meta_array` runtime helper (resolving "now" timestamps through `__rt_time`, with a refcount-balanced array→Mixed boxing). Non-wrapper paths keep libc. ARM64 + x86_64 (both Docker-verified)
- [x] Phase 27 (Tier 1) — `stream_socket_enable_crypto` SNI auto-default to the connection host: `stream_socket_client` now records each connected fd's transport host (scheme/port stripped, `__rt_str_persist`'d) in a per-fd `_stream_connect_host` table via the new `__rt_stash_connect_host` helper; when `stream_socket_enable_crypto` finds no `ssl.peer_name` context option it defaults the SNI / cert-name to that recorded host (matching PHP) before falling back to `"localhost"`. ARM64 + x86_64 (both Docker-verified)
- [x] Phase 28 (Tier 1) — stream-filter `$params` (4th arg): `stream_filter_append`/`stream_filter_prepend` now accept the optional `$params` argument and thread a bare-integer-literal value into the filter at codegen — `zlib.deflate` compression level (`-1`..`9`, clamped) and `bzip2.compress` blockSize (`1`..`9`, clamped). A non-constant / array `$params` keeps the default; other filters ignore it. Shared `const_int_param` extractor in `stream_filter.rs`; the arity/signature widened to 4 args. ARM64 + x86_64 (both Docker-verified)
- [x] Phase 29 (Tier 2) — user stream-filter bucket-brigade dispatch works for the PHP-canonical 4-arg `filter($in, $out, &$consumed, $closing): int` form (the brigade plumbing was already wired; two pre-existing general Mixed bugs blocked the idiom): (1) `__rt_mixed_cast_bool` treated a `Mixed`-boxed object (tag 6) as falsy, so `while ($b = stream_bucket_make_writeable($in))` never entered — objects are now truthy like PHP; (2) `strtoupper`/`strtolower` read a `Mixed` operand via a bare `emit_expr` (stale string registers → empty result) and now coerce through `emit_string_arg` (`__rt_mixed_cast_string`). Both are broad correctness fixes beyond filters. Tests: `test_user_filter_4arg_brigade_transforms_via_while_loop`, `test_mixed_object_is_truthy`. ARM64 + x86_64 (both Docker-verified); macOS full suite 5644/0
- [x] Phase 30 (Tier 2) — stream-context `notification` callback + `STREAM_NOTIFY_*`: a literal `['notification' => <closure|first-class callable>]` entry of the `stream_context_create` / `stream_context_set_params` `$params` array is captured at codegen time into a global slot (retained via the rodata-safe `emit_retain_current_descriptor`). `fopen("http://...")` fires it at three milestones through the new `__rt_http_fire_notification` runtime shim, which builds the 6-element PHP argument array (`int $code, int $severity, ?string $message, int $message_code, int $bytes_transferred, int $bytes_max`), boxes it as a `Mixed(indexed-array)` cell, and invokes the callback through its descriptor invoker (the offset-56 `call_user_func_array` invoker contract): `STREAM_NOTIFY_CONNECT` (2, after each connect — fd restored into the carried register afterward so the request still sends), `STREAM_NOTIFY_COMPLETED` (8, body length in `$bytes_transferred`), and `STREAM_NOTIFY_FAILURE` (9, severity ERR). Bonus fix: `stream_context_create` used a fixed `scc_store_zero` label that was defined twice when a program created more than one context — now uniquified, so multiple `stream_context_create` calls assemble. v1 limits: literal closure / first-class-callable only (string/array/variable callbacks are not fired, slot cleared); single global slot; `http://` only; `$message`/`$message_code` are null/0; HTTPS/FTP and `PROGRESS`/`FILE_SIZE_IS`/redirect/auth milestones deferred. Tests: `test_stream_notification_callback_fires_failure_on_refused_connection`, `_string_callback_not_fired_in_v1`, `_cleared_by_later_context`, `_via_set_params`, `test_stream_context_create_twice_assembles` (CONNECT/COMPLETED validated against a live server during development). ARM64 + x86_64 (both Docker-verified)
- [x] Phase 31 (Tier 2) — userspace-wrapper `stream_cast` (vtable slot 10) + `stream_select` wrapper-awareness: synthetic wrapper fds (`0x40000000 | slot`) cannot be passed to `select(2)`, so the new `__rt_user_wrapper_stream_cast(fd, cast_as)` runtime helper invokes the wrapper object's `stream_cast(int $cast_as)` method (slot 10) to resolve a real underlying fd — passthrough for ordinary fds, `-1` when the handle/method is absent, and it unboxes a boxed-Mixed `int`/`resource` return (raw `: int` returns pass through). `stream_select` now calls it (with `STREAM_CAST_FOR_SELECT` = 3) for every descriptor at both the fd_set build and post-select compaction sites, spilling the caller-saved loop registers around the call (ARM64 grows the select frame to 160 bytes; x86_64 to 160 bytes, relying on callee-saved r12/r13 surviving). A wrapper that exposes a real socket fd via `stream_cast` becomes select()-able; one without `stream_cast` is excluded (matching PHP). The common real-fd path is byte-identical (an extra `tst`/`test` + branch, no call). Tests: `test_stream_select_wrapper_stream_cast_detects_ready`, `test_stream_select_wrapper_without_stream_cast_excluded`. ARM64 + x86_64 (both Docker-verified)
- [x] Phase 32 (Tier 2) — TLS client certificates (mutual TLS) + honest `ciphers`/`security_level`: the `elephc-tls` crate gains `client_cert_config` (loads a PEM cert chain + unencrypted private key into a rustls `with_client_auth_cert` `ClientConfig`) plus the `elephc_tls_attach_fd_client_cert` / `elephc_tls_connect_client_cert` C entry points (crate unit-tested: config builds from a real self-signed cert+key, and rejects missing/cert-less/keyless inputs). `stream_socket_enable_crypto` reads `ssl.local_cert` + `ssl.local_pk` from the active context (via `__rt_get_string_context_option`) and, when both are present, dispatches the client-cert attach variant (new `_elephc_tls_attach_fd_client_cert_fn` / `_elephc_tls_connect_client_cert_fn` fn-pointer slots, published alongside the others); the enable_crypto spill frame grows 32→64 bytes to hold the cert/key ptr/len pairs, and the x86_64 variant passes the 7th argument (`key_len`) on the stack. A bad cert/key path fails the config load before any network I/O → `false`. `ssl.passphrase` is not honored (rustls reads only unencrypted keys). `ssl.ciphers` (no rustls equivalent for OpenSSL cipher strings) and `ssl.security_level` (rustls picks TLS 1.2/1.3 automatically) are accepted without error but documented as not honored — the rustls-feasible subset. Tests: `elephc-tls` crate `client_cert_config_builds_from_valid_cert_and_key` / `_rejects_missing_and_certless` / `connect_client_cert_bad_path_returns_minus_one` / `attach_client_cert_null_paths_returns_minus_one`; codegen `test_stream_socket_enable_crypto_client_cert_bad_path_fails`. Build the staticlib with `cargo build -p elephc-tls`. ARM64 + x86_64 (both Docker-verified)
- [x] Phase 33 (Tier 2) — `stream_set_chunk_size` / `stream_set_read_buffer` / `stream_set_write_buffer` made real-ish: `stream_set_chunk_size($stream, $size)` now tracks a per-fd chunk size in the new `_stream_chunk_size` table (indexed by raw fd up to 256, default 8192) and returns the **previous** value — PHP's observable save/restore contract — instead of always reporting 8192 (out-of-range / synthetic fds report the default without storing; the size does not yet change read granularity, so only the returned value is observable). `stream_set_read_buffer` / `stream_set_write_buffer` keep returning `0` ("success") — the correct result for elephc's unbuffered (direct-syscall) stream model, where the buffer size has no effect. The chunk-size path uses a uniquified `scs_have_old` label so multiple `stream_set_chunk_size` calls in one program assemble. Tests: `test_stream_set_chunk_size_returns_previous`, `test_stream_set_buffer_stubs`. ARM64 + x86_64 (both Docker-verified)
- [x] Phase 34 (Tier 3) — `phar://` write Milestone 1 now produces a **PHP-readable signed** archive: the write path (`fopen("phar://a.phar/e","w")` + `fwrite` + `fclose`) already assembled a single uncompressed entry, but signatureless (real PHP rejects unsigned phars when `phar.require_hash` is on). The manifest now sets `PHAR_HDR_SIGNATURE` (0x10000), and `__rt_phar_write_finalize` computes a SHA1 over the whole assembled archive (via `CC_SHA1`/`SHA1`, the raw 20 bytes — not the hex `__rt_sha1`) and appends the `raw-sha1 ++ LE32(0x0002 = Phar::SHA1) ++ "GBMB"` trailer before writing to disk (both arches). The type checker declares `require_linux_builtin_library("crypto")` for `phar://` write modes so Linux links `-lcrypto` (crc32 is pure asm and needs no lib; macOS uses CommonCrypto in libSystem). Verified: real PHP `new Phar(...)` reads an elephc-written entry back. elephc's own phar reader is compile-time, so a runtime-written archive can't be read back in the same program — the test verifies the on-disk signature bytes directly. Test: `test_fopen_phar_write_signs_single_entry`. Limits: SHA1 only (no OpenSSL key signing), one stream at a time, uncompressed, buffer-bounded. ARM64 + x86_64 (both Docker-verified)
- [x] Phase 35 (Tier 3) — `file_put_contents("phar://archive/entry", $data)` completes the phar-write M1 surface: it lowers to the same `__rt_phar_write_open` → `__rt_phar_write_append` → `__rt_phar_write_finalize` runtime as `fopen`+`fwrite`+`fclose`, producing the identical signed single-entry archive and returning the byte count. The type checker declares `require_linux_builtin_library("crypto")` for `file_put_contents` on a `phar://` literal (same SHA1 → libcrypto need). Verified real PHP reads the entry back. Test: `test_file_put_contents_phar_writes_signed_entry`. ARM64 + x86_64 (both Docker-verified)
- [x] Phase 36 (Tier 3) — runtime `phar://` read (non-literal archive path): `fopen("phar://".$path."/entry","r")` with a non-literal URL now reads+parses the archive at run time instead of only at compile time. New `__rt_phar_read_entry` runtime helper reads the whole archive via `__rt_file_get_contents`, replicates the compile-time `parse_phar_entry` manifest walk in assembly (scan `__HALT_COMPILER();`, skip the stub tail, read manifest header, walk entries summing `compressed` sizes to locate the matched uncompressed entry, with per-load bounds checks), and tail-calls `__rt_data_stream` to materialize the bytes as a tmpfile-backed readable fd. A new `__rt_fopen_maybe_phar` gate (the `fopen` generic non-literal path calls it instead of `__rt_fopen`) routes a `phar://` read URL to the reader and everything else straight to `__rt_fopen`; the literal-URL compile-time fast path (embedded bytes) is unchanged. So a program can read a phar it wrote earlier in the same run, or one whose path is only known at run time. M2 scope: one named **uncompressed** entry, `.phar/`-boundary split; gzip/bzip2 runtime entries and multi-entry write deferred. Test: `test_fopen_phar_runtime_path_reads_entry` (reads the 2nd entry via a runtime path, validating the offset-summing walk). ARM64 + x86_64 (both Docker-verified)
- [x] Phase 37 (Tier 3) — `file_get_contents("phar://literal/entry")`: a literal `phar://` URL passed to `file_get_contents` now decodes the entry at compile time (reusing `phar_stream::extract_phar_entry` — uncompressed, gzip, or bzip2) and embeds the bytes as the string result, the same compile-time model as `fopen("phar://...","r")`; a missing archive/entry yields PHP `false`. Pure codegen (no new runtime/assembly): the embedded bytes go through `data.add_string` and the existing `box_file_get_contents_result` (null ptr → false). A non-literal `phar://` `file_get_contents` path is read at run time via `fopen` + `stream_get_contents` (the runtime reader from Phase 36). Test: `test_file_get_contents_phar_literal_entry`. ARM64 + x86_64 (both Docker-verified)
- [x] Phase 38 (Tier 3) — `file_get_contents("phar://".$path."/entry")` (non-literal runtime read): a non-literal `phar://` URL passed to `file_get_contents` is now read+parsed at run time, completing the symmetry with the Phase-36 non-literal `fopen` path. New `__rt_file_get_contents_maybe_phar` gate (the `file_get_contents` generic fall-through path calls it instead of `__rt_file_get_contents`) checks the `"phar://"` prefix: a match runs `__rt_phar_read_entry` → fd, slurps it with `__rt_stream_get_contents` into an owned string, closes the fd, and returns the string (a phar read error / missing entry → null ptr → boxed PHP `false`); everything else tail-calls `__rt_file_get_contents` unchanged. Returning the `stream_get_contents` copy (rather than a mid-heap slice of the archive buffer) keeps the boxed string safe to own/decref. So a program can `file_get_contents()` a phar it wrote earlier in the same run, or one whose path is only known at run time. Both arches hand-written and verified; the x86 gate caught nothing new (mirrored cleanly). M2 scope inherited from Phase 36: one named **uncompressed** entry, `.phar/`-boundary split; gzip/bzip2 runtime entries deferred. Test: `test_file_get_contents_phar_runtime_path` (write a phar, read it back through a runtime URL; missing entry → false). ARM64 + x86_64 (both Docker-verified)
- [x] Phase 39 (G2) — stream-filter `$params` array form: the compression filters now honor PHP's canonical associative-array `$params` (the 4th `stream_filter_append`/`prepend` arg), not just the bare-int shorthand that already worked. `const_int_param` (in `builtins/io/stream_filter.rs`) was generalized to take a `key` + `primary` flag and to read a static int from an `ArrayLiteralAssoc` entry, so `zlib.deflate` reads `['level' => N]` (-1..9) and `bzip2.compress` reads `['blocks' => N]` (blockSize100k 1..9) **and** `['work' => N]` (workFactor 0..250, previously hardcoded 0 → now threaded into `BZ2_bzCompressInit` on both arches). The value must be a compile-time literal (bare int or literal array with static int entries); a non-constant `$params` keeps the defaults. zlib `window` stays fixed at -15 (required for the raw-deflate round-trip with `compress.zlib://`) and `memory` is not exposed — documented. Test: `test_stream_filter_params_array_form_round_trips` (array form for both filters round-trips through the matching decompressor); the bare-int `test_stream_filter_params_compression_level_round_trips` still passes. Also fixed the stale Docker images (the cached `bzip2-dev` apk layer predated the Dockerfile line) by `--rebuild` of both Linux images. ARM64 + x86_64 (both Docker-verified)
- [x] Phase 40 — `stream_socket_enable_crypto($stream, false)` real mid-stream TLS teardown: the disable path reloads the fd and calls the shared `fclose::emit_tls_session_teardown` (sends `close_notify` via `_elephc_tls_close_fn`, clears `_tls_sessions[fd]`, a no-op when no session is attached), leaving the fd a plain TCP socket, then reports `true` — replacing the v1 stub. Also fixed a latent assembler bug: the enable path's hardcoded `__rt_ssec_peer_ok_*` labels collided when `stream_socket_enable_crypto` was emitted more than once (e.g. enable then disable); uniquified via `ctx.next_label`. ARM64 + x86_64 (both Docker-verified)
- [x] Phase 41 — `stream_get_contents($stream, ?$length, $offset = -1)`: the optional `$length` (max bytes) and `$offset` (seek-before-read) arguments are honored. A finite `$length` routes through `__rt_stream_get_contents_bounded`, which loops via `__rt_fread` until `$length` bytes are accumulated, EOF is reached, or an empty read is produced; `$offset >= 0` seeks first (lseek for a normal fd, the wrapper's `stream_seek` for a synthetic fd) and returns PHP `false` if the seek fails; a `null`/negative `$length` reads to EOF. The read-all/wrapper-drain path remains factored into `emit_read_all_from_fd`, and the builtin is typed/codegenerated as `string|false`. ARM64 + x86_64 (both Docker-verified)
- [x] Phase 42 — `stream_copy_to_stream($from, $to, ?$length, $offset = -1)`: the optional `$length`/`$offset` are honored via a single capped, feof-gated `__rt_fread`/`__rt_fwrite` loop that works for any real/wrapper fd combination (seeks `$from` by `$offset >= 0` and returns PHP `false` if that seek fails, stops at `$length` bytes copied or source EOF; `null`/negative `$length` copies to EOF). The no-extra-args fast path (real fds → `__rt_stream_copy_to_stream`, wrapper fds → compiled loop) is unchanged, and the builtin is typed/codegenerated as `int|false`. ARM64 + x86_64 (both Docker-verified)
- [x] Phase 43 — `file_get_contents()` over `http://`/`https://`/`ftp://`/`ftps://` URLs: a literal URL opens the matching wrapper (the fd-producing core of each wrapper factored into a shared `emit_open_fd`), slurps the whole body via the TLS-aware `__rt_stream_get_contents`, persists it to owned heap with `__rt_str_persist`, and returns it (`false` on a failed open) — the same wrappers as `fopen()`, so `file_get_contents` now covers every URL scheme `fopen` does. Non-literal URL strings now route through `__rt_file_get_contents_maybe_url`, which recognizes runtime `http://`, `https://`, `ftp://`, and `ftps://` before falling back to the phar/filesystem helper. Literal `https://`/`ftps://` URLs pull in `-lelephc_tls` via the checker; non-literal `file_get_contents()` links it conservatively because the runtime scheme is unknown. ARM64 + x86_64 (both Docker-verified)

### Streams — remaining work (subsystem considered *partially complete*; merged as-is)

The streams/sockets subsystem covers the everyday PHP surface end-to-end (file
I/O, all network transports incl. IPv6+DNS, TLS, the wrapper/filter/context
families, and userspace `stream_wrapper_register`/`stream_filter_register`). The
items below are intentionally deferred to a later milestone. They are either
niche, genuinely large (multi-week), or blocked by an upstream-library limit —
none are needed for typical stream usage.

- [ ] **User stream-filter `$params`** — the 4th `stream_filter_append`/`prepend`
  argument is honored for the built-in compression filters (Phase 39) but not
  yet exposed to *userspace* filters as `$this->params`. elephc does not model
  the PHP `php_user_filter` base class; supporting it needs an object-model
  decision (model the base class, or inject a dynamic `params` property on the
  instantiated filter object) plus passing the boxed value through
  `__rt_stream_filter_attach_user`.
- [ ] **`phar://` compressed runtime reads** — gzip/bzip2 entries decompress at
  *compile time* (literal URLs) but the *runtime* reader (`__rt_phar_read_entry`,
  non-literal paths) handles uncompressed entries only. Needs in-asm inflate or
  a runtime decompress shim.
- [ ] **`phar://` advanced writes** — write is Milestone-1: one uncompressed,
  SHA1-signed entry per archive, via a single open stream. Deferred: multi-entry
  archives, modifying an existing archive (read-modify-write), compressed
  entries, and OpenSSL key signing (only SHA1 today).
- [ ] **`phar://` tar/zip variants** — only the native PHAR (phar) format is
  supported; the tar- and zip-based PHAR containers are not.
- [ ] **`Phar` / `PharData` OOP API** — the object-oriented archive API is not
  implemented (multi-week; depends on the advanced read/write items above).
- [ ] **TLS `ciphers` / `security_level`** — accepted without error but *not
  honored*: rustls has no OpenSSL-cipher-string equivalent and selects TLS
  1.2/1.3 automatically. Honest no-op by design (upstream limitation), not a
  planned change. (`ssl.passphrase` is likewise unsupported — rustls reads only
  unencrypted keys.)
- [ ] **Misc lower-level gaps** — true non-blocking semantics (beyond the
  `O_NONBLOCK` fcntl), `realpath_cache_*`, and `lchown`/`lchgrp`.

## v0.24.x — EIR introduction and register allocation

Introduce a domain-specific intermediate representation (EIR) between the
AST-level optimizer and the assembly emitter, then add a real register
allocator.

EIR is a custom, PHP-shaped IR — not Cranelift or LLVM. It preserves the
hand-written-and-commented assembly philosophy while removing the
structural ceiling on optimization that the direct AST → ASM emitter
imposed. See `docs/internals/the-ir.md`.

- [ ] EIR design specification (`docs/internals/the-ir.md`) — types, instructions, terminators, effects, ownership, textual format
- [ ] `src/ir/` module — types, instructions, builder, validator, printer
- [ ] AST → EIR lowering pass — every `ExprKind`/`StmtKind` variant
- [ ] `--emit-ir` CLI flag for diagnostics and snapshot testing
- [ ] EIR → ASM backend producing semantically equivalent output to the legacy backend (no optimizations yet)
- [ ] `--ir-backend` CLI flag (opt-in stable)
- [ ] Two-week soak period to collect external feedback
- [ ] Default backend switch from AST to EIR
- [ ] Deprecation warning on `--ast-backend`
- [ ] Linear-scan register allocator (Poletto-Sarkar) with separate int / float pools and callee-saved preservation across calls
- [ ] Register-pressure mitigations: caller-saved reuse for non-call-crossing intervals; better spill heuristic

Expected outcome: feature parity at end of v0.24.0; ≥15% performance
improvement on compute benchmarks at end of v0.24.x.

## v0.25.x — EIR optimization passes

Build the IR-level passes that the AST optimizer could not reach.

- [ ] Identity arithmetic folding (`x + 0`, `x * 1`, `x ^ x`, etc.)
- [ ] Peephole patterns: redundant load/store, box/unbox cancellation, string-literal concat folding, paired acquire/release cancellation
- [ ] Dead instruction elimination over the IR CFG (absorbs former v0.23 "Dead code elimination v3")
- [ ] Dead store elimination over PHP local slots
- [ ] Branch simplification (constant-condition `CondBr`, empty-block jump threading, unreachable block removal)
- [ ] Common subexpression elimination — per-block, then dominance-aware cross-block (absorbs former v0.23 "Constant propagation v4")
- [ ] Loop detection and natural-loop construction (back edges, headers, preheaders)
- [ ] Loop-invariant code motion for pure operations
- [ ] Small-function inliner (size threshold 24 instructions, non-recursive, no try/catch, no generators/fibers) (absorbs former v0.23 "Inline small functions")
- [ ] Pipeline integration in fixed-point order

Expected outcome: additional 10–20% performance gain on loop-heavy and
call-heavy benchmarks; cumulative ≥30% improvement vs end-of-v0.23
baseline.

## v0.26.x — Performance closure, legacy cleanup, and 0.x stabilization

Optimization work should now be driven by benchmarks, generated assembly size,
and 0.x validation rather than by speculative pass work.

- [ ] Source maps v2 — richer mappings for functions / expressions / labels and a more stable machine-readable schema for external tooling
- [ ] Memory-model-aware propagation for heap-backed locals and targeted runtime invalidations beyond `unset($var)` and the currently modeled local writes
- [ ] Purity / may-throw v2 for dynamic instance dispatch, richer property/array reads, and less pessimistic builtin modeling (feeds the EIR effects table)
- [ ] Guard reasoning v2 for dead-code elimination — broader range reasoning and multi-variable facts beyond current strict-scalar, boolean, loose-comparison, and safe relational-complement guards
- [ ] Exception-aware DCE v2 — exact thrown-type / handler reachability, nested try rethrow modeling, and less conservative finally-path invalidation
- [ ] Control-flow normalization v2 — broader canonicalization of nested block/control shells before CFG-aware optimization passes
- [ ] Composite conditional include function variants — extend include-graph exclusivity from one direct `if` / `elseif` / `else` chain to nested/composed conditional paths where declarations are pairwise exclusive only after combining multiple branch decisions
- [ ] Switch-aware conditional include function variants — extend include-graph exclusivity beyond `if` / `elseif` / `else` to `switch` cases once fall-through, `break`, and terminating case bodies are modeled precisely; revisit `match` only if include-like statement lowering ever appears inside match arms
- [ ] Runtime routine dead stripping — include or link only runtime helpers reachable from the generated program instead of carrying the whole target runtime slice
- [ ] Tail-call optimization — direct tail self- and mutual-recursion lowering on top of EIR (`Br` to function entry with parameter rebinding)
- [ ] Performance within 2x of C -O0 on compute benchmarks
- [ ] Real-world CLI tools compiled as validation
- [ ] Keep the legacy AST → ASM backend available as a fallback through validation; remove it only after the IR backend passes real-world validation
- [ ] Rename `src/codegen_ir/` to `src/codegen/`
- [ ] Move historical codegen doc to `docs/internals/legacy-codegen.md`; refresh `docs/internals/the-codegen.md` to describe the IR pipeline
- [ ] Apple notarization for direct downloads (codesign + notarytool)
- [ ] Installation / packaging documentation for the supported host platforms

## Later 0.x product tracks

These are valuable product directions that build on the stabilized 0.x compiler
and runtime foundation.

## v0.27.x — Shared and static libraries (C ABI)

- [ ] `--lib` flag, export PHP functions as C-callable symbols
- [ ] `--export` flag for symbol selection
- [ ] `.dylib` / `.so` / `.a` output
- [ ] Multi-file library compilation
- [ ] Symbol visibility control
- [ ] Auto-generated C header file
- [ ] Null-terminated string convention for C interop
- [x] Stateful FFI callback trampolines — generate C-ABI-compatible trampoline symbols for descriptor-backed callables passed to extern `callable` parameters, retaining descriptor/capture/receiver environments for supported scalar/ptr signatures and documenting constraints for C APIs without userdata/context slots
- [ ] `pkg-config` generation
- [ ] FFI documentation for C, Rust, Python, Go

## v0.28.x — PHP extension bridge (experimental)

- [ ] `zval` pack/unpack routines (convert elephc values ↔ PHP `zval` structs)
- [ ] Link against PHP extension `.so` / `.dylib` shared libraries
- [ ] Bridge for string, int, float, bool, array types
- [ ] Proof of concept with one extension (e.g., `mbstring` or `curl`)
- [ ] `--ext` flag to specify extension libraries at compile time
- [ ] Documentation: how to bridge a PHP extension

## v0.29.x — WebAssembly target

- [ ] WASM codegen backend
- [ ] `.wat` / `.wasm` emission
- [ ] WASI support for I/O
- [ ] NPM package generation

## Deferred ideas

Features that are feasible but intentionally not on the active 0.x path. They are
either product-specific, very high complexity, or better justified by concrete
future use cases.

| Feature | Complexity | Notes |
|---|---|---|
| Buffer ergonomics v2 | Medium | Consider dynamic resize/push/pop, `foreach`, array conversion, and automatic cleanup for `buffer<T>` while keeping the hot-path POD contract explicit. |
| String-capable FFI callbacks | Medium | Allow C callback signatures that pass or return strings once ownership and temporary C-string lifetimes are modeled safely across callback boundaries. |
| Generator parity v2 | Medium | MVP delivered in v0.21.x for ARM64 and Linux x86_64. Remaining parity work: `yield` inside `try`/`catch`/`finally`, dynamic `yield from` arrays beyond the compile-time literal form, broader dynamic `yield from` Iterator targets, exception propagation through `Generator::throw` to caller-visible finally paths, and PHP-exact `Generator` interface inheritance with `Iterator`. See `docs/php/generators.md`. |
| Fiber parity v2 | Medium | MVP delivered in v0.20.x for ARM64 and Linux x86_64. Remaining parity work: arithmetic auto-unboxing on `mixed` payloads received from `suspend()`, true variadic `start(...$args)` beyond seven args, dynamic callback targets, by-reference callback start parameters, configurable stack sizing, and PHP-exact `FiberError` hierarchy. See `docs/php/fibers.md`. |
| Conditional include class-like variants | High | Keep class/interface/trait/enum duplicate detection strict for now. Supporting branch-selected class-like declarations would require runtime class metadata/layout dispatch, while modern PHP can avoid the ambiguity with namespaces. |

---

## Will not implement

Features that are fundamentally incompatible with a static ahead-of-time compiler.

| Feature | Reason |
|---|---|
| `compact()` | Resolves variable names from strings at runtime. In elephc, variables are fixed stack slots allocated at compile time — there is no variable name table at runtime. |
| `extract()` | Creates new variables from array keys at runtime. A static compiler must know all variables before execution — it cannot allocate stack slots on the fly. |
| `$$var` (variable variables) | Requires a runtime symbol table to resolve variable names dynamically. Incompatible with static stack-based variable allocation. |
| `eval()` | Requires a full interpreter/compiler at runtime. Fundamentally impossible in an AOT compiler. |

## Future 1.0 perspective

1.0 is not an active planning gate for the current roadmap. Revisit it only
after the 0.x compiler/runtime contracts have settled through real-world use.

- [ ] Freeze the documented language/runtime contract for the supported target matrix
- [ ] Decide which 0.x product tracks belong inside the first stable contract and which remain experimental
- [ ] Run a dedicated stabilization pass across compiler, runtime, docs, examples, and packaging
- [ ] Ship 1.0 from a proven 0.x baseline
