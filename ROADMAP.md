# Roadmap

## Direction to 1.0

The remaining pre-1.0 work is intentionally focused on PHP parity and release
stability. Product expansion tracks such as shared libraries, WebAssembly, and
the PHP extension bridge are useful, but they are no longer part of the 1.0
definition.

Current direction:

- Finish the PHP-visible compatibility gaps that are already well-bounded.
- Keep completed historical items in their original version sections.
- Move optimizer work behind concrete benchmark or release-candidate needs.
- Treat new targets and distribution formats as post-1.0 product tracks.

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

## v0.20.x — PHP filesystem and runtime parity closure

Close the small, well-scoped PHP filesystem/runtime compatibility gaps that are
already adjacent to the current implementation.

- [x] `fnmatch()` non-zero flag parity for `FNM_PATHNAME`, `FNM_PERIOD`, `FNM_CASEFOLD`, and `FNM_NOESCAPE`
- [x] Dynamic `pathinfo($path, $flag)` parity for runtime flags that may evaluate to `PATHINFO_ALL`; runtime exact `PATHINFO_ALL` now returns the associative-array shape while component flags return strings
- [x] PHP resource type compatibility — model file handles and future extension handles separately from integers
- [x] `fopen()` failure parity — return `false` on open failure while keeping successful handles as `resource`, and make stream built-ins reject/handle the `resource|false` path without passing boxed `false` as a native descriptor
- [x] Runtime-dynamic include paths — explicitly reject runtime-evaluated `include` / `require` path expressions beyond the current compile-time string-folder (`$path`, function calls, ternaries, property access)
- [ ] Runtime-order-aware `include_once` / `require_once` — add runtime guards inside functions, methods, loops, and conditional branches so skipped files match PHP execution order rather than only compile-time traversal order

## v0.21.x — PHP expression, call, and callable parity

Finish language-level PHP behavior that is visible in everyday source code and
does not require a new backend or product mode.

- [ ] Mixed nullsafe/member chains — match PHP's full chain semantics for forms that mix `?->` and `->`, such as `$a?->b->c`
- [ ] Dynamic `instanceof` targets — support PHP forms such as `$obj instanceof $className` with runtime validation for class-string/object target expressions
- [ ] Full PHP list destructuring — skipped entries, nested patterns, associative-key destructuring, and non-local destructuring targets where PHP permits them
- [ ] Named-argument parity for built-ins, extern calls, and spread — extend validation/lowering outside user-defined calls and handle spread interactions
- [ ] Full first-class callable targets — support `static::method(...)` and `$object->method(...)` in addition to function, `ClassName::`, `self::`, and `parent::` targets
- [ ] Captured closures as callback values — forward hidden `use (...)` environments through callback-style built-ins such as `array_map`, `array_filter`, and `call_user_func`

## v0.22.x — PHP array and type-model parity

Tighten the PHP value model where the current static representation is still
more restrictive than PHP.

- [ ] Heterogeneous indexed arrays — allow mixed payloads in indexed arrays instead of requiring homogeneous indexed values
- [ ] Mixed indexed/associative array union — model `array + array` across indexed/hash representations while preserving PHP's shared int/string key space and left-key precedence
- [ ] Runtime-value compatibility polishing v2 — continue with PHP's uninitialized typed-property state, integer overflow promotion, broader loose-comparison semantics, and future warning/notice sites as they are added
- [ ] Broader date, regex, and JSON PHP parity — expand `strtotime()` relative formats, PCRE-compatible regex features/captures/backreferences, and `json_decode()` structured array/object decoding

## v0.23.x — PHP OOP parity finish

Close the remaining class/property compatibility gaps before freezing the 1.0
language contract.

- [ ] OOP property parity v2 — cover abstract properties, `readonly static` properties, instance property redeclaration rules, and the remaining by-reference constructor-promotion gaps (`readonly` and default values)

## v0.24.x — Optimization and release performance pass

Optimization work should now be driven by benchmarks, generated assembly size,
and release-candidate validation rather than by speculative pass work.

- [ ] Source maps v2 — richer mappings for functions / expressions / labels and a more stable machine-readable schema for external tooling
- [ ] Peephole optimization (redundant load/store elimination)
- [ ] Dead code elimination v3 — fuller fixed-point/basic-block pass beyond the current path-aware AST pruning
- [ ] Constant propagation v4 — full fixed-point / basic-block propagation across arbitrary loops and general path merges once there are measured cases that justify the extra pass complexity
- [ ] Memory-model-aware propagation for heap-backed locals and targeted runtime invalidations beyond `unset($var)` and the currently modeled local writes
- [ ] Purity / may-throw v2 for dynamic instance dispatch, richer property/array reads, and less pessimistic builtin modeling
- [ ] Guard reasoning v2 for dead-code elimination — broader range reasoning and multi-variable facts beyond current strict-scalar, boolean, loose-comparison, and safe relational-complement guards
- [ ] Exception-aware DCE v2 — exact thrown-type / handler reachability, nested try rethrow modeling, and less conservative finally-path invalidation
- [ ] Control-flow normalization v2 — broader canonicalization of nested block/control shells before CFG-aware optimization passes
- [ ] Runtime routine dead stripping — include or link only runtime helpers reachable from the generated program instead of carrying the whole target runtime slice
- [ ] Register allocation (reduce stack spills)
- [ ] Inline small functions
- [ ] Tail-call optimization
- [ ] Performance within 2x of C -O0 on compute benchmarks
- [ ] Real-world CLI tools compiled as validation
- [ ] Apple notarization for direct downloads (codesign + notarytool)
- [ ] Installation / packaging documentation for the supported host platforms

## v1.0.x — Stabilization and release gate

- [x] `--emit-asm`, `--check` flags
- [x] Full test coverage (>500 focused checks across lexer/parser/codegen/error suites)
- [x] Documentation: language subset spec, architecture guide
- [x] CI/CD with release binaries
- [x] PHP case-insensitive symbol parity for keywords, built-in/user function calls, class/interface/trait names, and method lookup while preserving PHP's case-sensitive variables, object properties, string array keys, and user constants
- [ ] Freeze the documented language/runtime contract for the supported target matrix
- [ ] Run a dedicated release-candidate stabilization pass across compiler, runtime, docs, and examples
- [ ] Ship 1.0 once the pre-1.0 roadmap is reduced to true post-1.0 work only

---

## Post-1.0 product tracks

These are valuable directions, but they should build on a frozen 1.0 language
and runtime contract instead of competing with PHP parity work before 1.0.

## v1.1.x — Shared and static libraries (C ABI)

- [ ] `--lib` flag, export PHP functions as C-callable symbols
- [ ] `--export` flag for symbol selection
- [ ] `.dylib` / `.so` / `.a` output
- [ ] Multi-file library compilation
- [ ] Symbol visibility control
- [ ] Auto-generated C header file
- [ ] Null-terminated string convention for C interop
- [ ] `pkg-config` generation
- [ ] FFI documentation for C, Rust, Python, Go

## v1.2.x — WebAssembly target

- [ ] WASM codegen backend
- [ ] `.wat` / `.wasm` emission
- [ ] WASI support for I/O
- [ ] NPM package generation

## v1.3.x — PHP extension bridge (experimental)

- [ ] `zval` pack/unpack routines (convert elephc values ↔ PHP `zval` structs)
- [ ] Link against PHP extension `.so` / `.dylib` shared libraries
- [ ] Bridge for string, int, float, bool, array types
- [ ] Proof of concept with one extension (e.g., `mbstring` or `curl`)
- [ ] `--ext` flag to specify extension libraries at compile time
- [ ] Documentation: how to bridge a PHP extension

## Deferred ideas

Features that are feasible but intentionally not on the pre-1.0 path. They are
either product-specific, very high complexity, or better justified by concrete
post-1.0 use cases.

| Feature | Complexity | Notes |
|---|---|---|
| Buffer ergonomics v2 | Medium | Consider dynamic resize/push/pop, `foreach`, array conversion, and automatic cleanup for `buffer<T>` while keeping the hot-path POD contract explicit. |
| String-capable FFI callbacks | Medium | Allow C callback signatures that pass or return strings once ownership and temporary C-string lifetimes are modeled safely across callback boundaries. |
| Generators / `yield` | High | Requires compile-time state machine transformation: every yield point becomes a switch case, all locals promoted to heap-allocated generator object. Edge cases with yield inside try/catch/finally are significant. |
| `yield from` delegation | High | Depends on generators. Forwards iteration to an inner generator, propagating values and return. |
| Fibers | Very high | Cooperative multitasking needs per-fiber stack allocation, custom context switch in assembly, guard pages, and GC awareness of multiple stack roots. Best suited for async I/O workloads, not the current game/systems target. |

---

## Will not implement

Features that are fundamentally incompatible with a static ahead-of-time compiler.

| Feature | Reason |
|---|---|
| `compact()` | Resolves variable names from strings at runtime. In elephc, variables are fixed stack slots allocated at compile time — there is no variable name table at runtime. |
| `extract()` | Creates new variables from array keys at runtime. A static compiler must know all variables before execution — it cannot allocate stack slots on the fly. |
| `$$var` (variable variables) | Requires a runtime symbol table to resolve variable names dynamically. Incompatible with static stack-based variable allocation. |
| `eval()` | Requires a full interpreter/compiler at runtime. Fundamentally impossible in an AOT compiler. |
