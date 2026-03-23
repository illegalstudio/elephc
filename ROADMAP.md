# Roadmap

## v0.1.0 — Usable CLI compiler (done)

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

## v0.2.0 — Arrays and null (done)

- [x] Indexed arrays: `$arr = [1, 2, 3];`
- [x] Array access, assignment, push: `$arr[0]`, `$arr[0] = 42`, `$arr[] = "new"`
- [x] `count()`, `array_push()`, `array_pop()`
- [x] `foreach ($arr as $value) { }` loop
- [x] `in_array()`, `array_keys()`, `array_values()`, `sort()`, `rsort()`, `isset()`
- [x] Heap allocator (1MB bump allocator)
- [x] Proper null: `echo null` prints nothing, `is_null()`, null coercion in operations

## v0.3.0 — Bool, float, and type system

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

## v0.4.0 — Strings

Make string handling practical.

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

## v0.5.0 — I/O and file system (done)

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

## v0.6.0 — Associative arrays and switch

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

## v0.7.0 — Advanced language features

- [ ] `define()` / `const` constants
- [ ] `global $var;` keyword
- [ ] Static variables: `static $counter = 0;`
- [ ] Pass by reference: `function foo(&$x) { }`
- [ ] Default parameter values: `function foo($x = 10) { }`
- [ ] Variadic functions: `function foo(...$args) { }`
- [x] Anonymous functions / closures: `$fn = function($x) { }` (`use` captures not yet supported)
- [x] Arrow functions: `$fn = fn($x) => $x * 2`
- [ ] Null coalescing: `$x ?? $default`, `$x ??= $default`
- [ ] Spread operator: `func(...$args)`, `[...$a, ...$b]`
- [ ] List unpacking: `[$a, $b] = $array;`
- [ ] Heredoc / nowdoc strings
- [ ] Bitwise operators: `&`, `|`, `^`, `~`, `<<`, `>>`
- [ ] Spaceship operator: `<=>`
- [x] `call_user_func()` (string callbacks)
- [ ] `call_user_func_array()`
- [x] `function_exists()`

## v0.8.0 — Date/time, JSON, regex

- [ ] `time()`, `microtime()`
- [ ] `date()`, `mktime()`, `strtotime()`
- [ ] `sleep()`, `usleep()`
- [ ] `json_encode()`, `json_decode()`, `json_last_error()`
- [ ] `preg_match()`, `preg_match_all()`, `preg_replace()`, `preg_split()`
- [ ] `exec()`, `shell_exec()`, `system()`, `passthru()`
- [ ] `getenv()`, `putenv()`, `$_ENV`
- [ ] `php_uname()`, `phpversion()`
- [ ] Constants: `PHP_EOL`, `PHP_OS`, `DIRECTORY_SEPARATOR`

## v0.9.0 — Multi-platform, multi-file, optimizations

- [ ] Linux x86_64 target
- [ ] Linux ARM64 target
- [x] `include` / `require` / `include_once` / `require_once`
- [ ] Cross-compilation support
- [ ] Constant folding (`2 + 3` → `5` at compile time)
- [ ] Dead code elimination
- [ ] Register allocation (reduce stack spills)
- [ ] Inline small functions
- [ ] Tail-call optimization
- [ ] Peephole optimization (redundant load/store elimination)

## v1.0.0 — Production-ready

- [ ] Comprehensive error recovery (multiple errors per compilation)
- [ ] Warning system (unused variables, unreachable code)
- [ ] Source maps (assembly ↔ PHP line mapping)
- [ ] `--emit-asm`, `--check` flags
- [ ] Benchmark suite (vs C, vs PHP interpreter)
- [ ] Full test coverage (>500 tests)
- [ ] Documentation: language subset spec, architecture guide
- [ ] CI/CD with release binaries
- [ ] Performance within 2x of C -O0 on compute benchmarks
- [ ] Real-world CLI tools compiled as validation

---

## v1.1.0 — Shared and static libraries (C ABI)

- [ ] `--lib` flag, export PHP functions as C-callable symbols
- [ ] `.dylib` / `.so` / `.a` output
- [ ] Auto-generated C header file
- [ ] Null-terminated string convention for C interop
- [ ] FFI documentation for C, Rust, Python, Go

## v1.2.0 — Library ecosystem

- [ ] `--export` flag for symbol selection
- [ ] Multi-file library compilation
- [ ] Symbol visibility control
- [ ] `pkg-config` generation

## v1.3.0 — WebAssembly target

- [ ] WASM codegen backend
- [ ] `.wat` / `.wasm` emission
- [ ] WASI support for I/O
- [ ] NPM package generation

---

## Will not implement

Features that are fundamentally incompatible with a static ahead-of-time compiler.

| Feature | Reason |
|---|---|
| `compact()` | Resolves variable names from strings at runtime. In elephc, variables are fixed stack slots allocated at compile time — there is no variable name table at runtime. |
| `extract()` | Creates new variables from array keys at runtime. A static compiler must know all variables before execution — it cannot allocate stack slots on the fly. |
| `$$var` (variable variables) | Requires a runtime symbol table to resolve variable names dynamically. Incompatible with static stack-based variable allocation. |
| `eval()` | Requires a full interpreter/compiler at runtime. Fundamentally impossible in an AOT compiler. |
| Classes / OOP | Out of scope. elephc targets procedural PHP — the subset where the language began. |
| Generators / `yield` | Requires coroutine suspension and stack switching, which is far beyond the current single-stack execution model. |
| Exceptions / `try`-`catch` | Requires stack unwinding infrastructure (setjmp/longjmp or DWARF unwinding). May be reconsidered in the future. |
