# Roadmap

## v0.1.x — Usable CLI compiler (done)

- [x] Lexer, parser (Pratt), type checker, ARM64 codegen pipeline
- [x] Integers, strings (double and single quoted), echo, variables, comments
- [x] Arithmetic (`+`, `-`, `*`, `/`, `%`), comparison (`==`, `!=`, `<`, `>`, `<=`, `>=`)
- [x] String concatenation (`.`) with automatic int coercion
- [x] `if` / `elseif` / `else`, `while`, `for`, `do...while`, `break`, `continue`
- [x] Functions with local scope, return, recursion, nested calls
- [x] Pre/post increment/decrement (`++$i`, `$i++`, `--$i`, `$i--`)
- [x] Logical operators: `&&`, `||`, `!` (with short-circuit evaluation)
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
- [x] `print` as alias for `echo`
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
- [x] Null coalescing: `$x ?? $default` (`??=` not yet supported)
- [x] Spread operator: `func(...$args)`, `[...$a, ...$b]`
- [x] List unpacking: `[$a, $b] = $array;`
- [x] Heredoc / nowdoc strings
- [x] Bitwise operators: `&`, `|`, `^`, `~`, `<<`, `>>`
- [x] Spaceship operator: `<=>`
- [x] `call_user_func()` (string callbacks)
- [x] `call_user_func_array()`
- [x] `function_exists()`

## v0.8.x — Date/time, JSON, regex

- [x] `time()`, `microtime()`
- [x] `date()`, `mktime()`, `strtotime()`
- [x] `sleep()`, `usleep()`
- [x] `json_encode()`, `json_decode()`, `json_last_error()`
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
- [x] `new` keyword for object instantiation
- [x] `->` property access and method calls
- [x] `readonly` properties (enforced at compile time)
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
- [x] Traits — compile-time method copying / inlining with `use`, `as`, `insteadof`, and trait properties
- [x] Exceptions (`try`/`catch`) — stack unwinding via `setjmp`/`longjmp` with runtime frame cleanup and `finally` support
- [x] Hash table insertion order — preserve PHP associative-array insertion order with a secondary linked list through entries
- [x] Mixed-type associative arrays — per-entry type tags instead of one value type per table
- [x] String indexing (`$str[$i]`) — lower to one-character slice syntax as sugar for string reads
- [x] `protected` visibility — third visibility level between public and private
- [x] Magic methods (`__toString`, `__get`, `__set`) — implicit hooks on property access and string conversion
- [x] ifdef or similar support

## v0.17.x — Language maturity and compiler ergonomics

- [ ] Hot-path data type
- [ ] Full namespace support
- [ ] Comprehensive error recovery (multiple errors per compilation)
- [ ] Warning system (unused variables, unreachable code)
- [ ] Generators / `yield` — compile-time state machine transformation (struct holds locals + state index, `next()` dispatches via switch)
- [ ] `yield from` delegation — forward iteration to an inner generator
- [ ] Enums (`enum Color { Red; Green; Blue; }`) — backed enums with `->value`, `::from()`, `::cases()`
- [ ] Named arguments (`foo(name: "Alice", age: 30)`) — reorder args at compile time based on parameter names
- [ ] First-class callable syntax (`strlen(...)`) — create closures from function names without string indirection
- [ ] Union types (`int|string`) — tagged union with runtime type dispatch
- [ ] Nullable types (`?int`) — sugar for `int|null`
- [ ] `match` with no-match error — runtime fatal when no arm matches and no default
- [ ] Readonly classes (`readonly class Point {}`) — all properties implicitly readonly
- [ ] Constructor promotion (`public function __construct(public int $x)`) — declare + assign properties in constructor signature
- [ ] Fibers — cooperative multitasking via `Fiber::start()`, `Fiber::suspend()`, `Fiber::resume()` (heap-allocated stack frames)

## v0.18.x — Multi-platform and optimizations

- [ ] Linux x86_64 target
- [ ] Linux ARM64 target
- [ ] Cross-compilation support
- [ ] Constant folding (`2 + 3` → `5` at compile time)
- [ ] Dead code elimination
- [ ] Register allocation (reduce stack spills)
- [ ] Inline small functions
- [ ] Tail-call optimization
- [ ] Peephole optimization (redundant load/store elimination)

## v1.0.x — Production-ready

- [ ] Source maps (assembly ↔ PHP line mapping)
- [ ] `--emit-asm`, `--check` flags
- [ ] Benchmark suite (vs C, vs PHP interpreter)
- [x] Full test coverage (>500 focused checks across lexer/parser/codegen/error suites)
- [x] Documentation: language subset spec, architecture guide
- [x] CI/CD with release binaries
- [ ] Apple notarization for direct downloads (codesign + notarytool)
- [ ] Performance within 2x of C -O0 on compute benchmarks
- [ ] Real-world CLI tools compiled as validation

---

## v1.1.x — Shared and static libraries (C ABI)

- [ ] `--lib` flag, export PHP functions as C-callable symbols
- [ ] `.dylib` / `.so` / `.a` output
- [ ] Auto-generated C header file
- [ ] Null-terminated string convention for C interop
- [ ] FFI documentation for C, Rust, Python, Go

## v1.2.x — Library ecosystem

- [ ] `--export` flag for symbol selection
- [ ] Multi-file library compilation
- [ ] Symbol visibility control
- [ ] `pkg-config` generation

## v1.3.x — WebAssembly target

- [ ] WASM codegen backend
- [ ] `.wat` / `.wasm` emission
- [ ] WASI support for I/O
- [ ] NPM package generation

## v1.4.x — PHP extension bridge (experimental)

- [ ] `zval` pack/unpack routines (convert elephc values ↔ PHP `zval` structs)
- [ ] Link against PHP extension `.so`/`.dylib` shared libraries
- [ ] Bridge for string, int, float, bool, array types
- [ ] Proof of concept with one extension (e.g., `mbstring` or `curl`)
- [ ] `--ext` flag to specify extension libraries at compile time
- [ ] Documentation: how to bridge a PHP extension

---

## Will not implement

Features that are fundamentally incompatible with a static ahead-of-time compiler.

| Feature | Reason |
|---|---|
| `compact()` | Resolves variable names from strings at runtime. In elephc, variables are fixed stack slots allocated at compile time — there is no variable name table at runtime. |
| `extract()` | Creates new variables from array keys at runtime. A static compiler must know all variables before execution — it cannot allocate stack slots on the fly. |
| `$$var` (variable variables) | Requires a runtime symbol table to resolve variable names dynamically. Incompatible with static stack-based variable allocation. |
| `eval()` | Requires a full interpreter/compiler at runtime. Fundamentally impossible in an AOT compiler. |
