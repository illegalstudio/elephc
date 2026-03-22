# Roadmap

## Completed

- [x] Lexer, parser (Pratt), type checker, ARM64 codegen pipeline
- [x] Integers, strings, echo, variables, comments
- [x] Arithmetic (`+`, `-`, `*`, `/`, `%`), comparison (`==`, `!=`, `<`, `>`, `<=`, `>=`)
- [x] String concatenation (`.`) with automatic int coercion
- [x] `if` / `elseif` / `else`, `while`, `for`, `break`, `continue`
- [x] Functions with local scope, return, recursion, nested calls
- [x] Pre/post increment/decrement (`++$i`, `$i++`, `--$i`, `$i--`)
- [x] Error messages with line/column numbers
- [x] 114 tests (lexer, parser, codegen end-to-end, error reporting)

---

## v0.1.0 — Usable CLI compiler

The compiler should be able to produce real CLI tools that accept arguments and do basic I/O.

- [ ] Logical operators: `&&`, `||`, `!` (with short-circuit evaluation)
- [ ] Assignment operators: `+=`, `-=`, `*=`, `/=`, `.=`, `%=`
- [ ] Boolean literals: `true`, `false`
- [ ] `null` keyword and `is_null()`
- [ ] Ternary operator: `$x = $a > $b ? $a : $b;`
- [ ] `do { } while ();` loop
- [ ] `$argc` / `$argv` globals (read command-line arguments)
- [ ] `exit($code);` / `die();`
- [ ] Single-quoted strings (`'no $interpolation'`)
- [ ] Built-in `strlen()` and `intval()`

## v0.2.0 — Arrays

Core data structure support. Unlocks most real-world PHP patterns.

- [ ] Indexed arrays: `$arr = [1, 2, 3];`
- [ ] Array access: `$arr[0]`, `$arr[$i]`
- [ ] Array assignment: `$arr[0] = 42;`, `$arr[] = "new";`
- [ ] `count()`, `array_push()`, `array_pop()`
- [ ] `foreach ($arr as $value) { }` loop
- [ ] `in_array()`, `array_keys()`, `array_values()`
- [ ] `sort()`, `rsort()`
- [ ] Heap allocator for dynamic-size arrays
- [ ] `isset()`, `unset()` for array elements

## v0.3.0 — Strings and I/O

Make string handling practical and add file I/O.

- [ ] String interpolation: `"Hello $name"`, `"val={$expr}"`
- [ ] Single-quoted raw strings
- [ ] `substr()`, `strpos()`, `str_replace()`
- [ ] `strtolower()`, `strtoupper()`, `trim()`
- [ ] `explode()`, `implode()`
- [ ] `fgets(STDIN)` / `readline()` — read from keyboard
- [ ] `fopen()`, `fread()`, `fwrite()`, `fclose()`
- [ ] `file_get_contents()`, `file_put_contents()`

## v0.4.0 — Associative arrays and switch

- [ ] Associative arrays: `$map = ["key" => "value"];`
- [ ] `foreach ($map as $key => $value) { }`
- [ ] Hash table runtime for string keys
- [ ] `array_merge()`, `array_slice()`, `array_map()`, `array_filter()`
- [ ] `switch` / `case` / `default` (with fall-through)
- [ ] `match` expression (PHP 8 style, no fall-through)

## v0.5.0 — Float and math

- [ ] Float type: `3.14`, `1.0e-5`
- [ ] Mixed int/float arithmetic (auto-promotion)
- [ ] Float comparison and formatting
- [ ] `abs()`, `min()`, `max()`, `floor()`, `ceil()`, `round()`
- [ ] `sqrt()`, `pow()`
- [ ] `rand()` / `mt_rand()`
- [ ] `number_format()`
- [ ] `is_int()`, `is_string()`, `is_float()`, `is_array()`

## v0.6.0 — Optimizations

Make the generated code competitive with C -O0.

- [ ] Constant folding (`2 + 3` → `5` at compile time)
- [ ] Dead code elimination (unreachable branches after `return`)
- [ ] Register allocation (reduce stack spills for temporaries)
- [ ] Inline small functions
- [ ] Tail-call optimization for recursive functions
- [ ] Peephole optimization (redundant load/store elimination)
- [ ] Strength reduction (`$x * 2` → `$x << 1`)

## v0.7.0 — Advanced language features

- [ ] `global $var;` keyword
- [ ] Static variables: `static $counter = 0;`
- [ ] Null coalescing operator: `$x ?? $default`
- [ ] Spread operator: `func(...$args)`, `[...$a, ...$b]`
- [ ] List unpacking: `[$a, $b] = $array;`
- [ ] Heredoc / nowdoc strings
- [ ] `var_dump()`, `print_r()` for debugging
- [ ] Type casting: `(int)`, `(string)`, `(float)`, `(array)`

## v0.8.0 — Multi-platform and multi-file

- [ ] Linux x86_64 target
- [ ] Linux ARM64 target
- [ ] `include` / `require` (static, resolved at compile time)
- [ ] `include_once` / `require_once`
- [ ] Cross-compilation support
- [ ] Environment variables: `getenv()`, `putenv()`

## v0.9.0 — Hardening and tooling

- [ ] Comprehensive error recovery (multiple errors per compilation)
- [ ] Warning system (unused variables, unreachable code)
- [ ] Source maps (assembly ↔ PHP line mapping)
- [ ] `--emit-asm` flag to keep assembly without linking
- [ ] `--check` flag for type checking without compilation
- [ ] Benchmark suite (vs C, vs PHP interpreter)
- [ ] Regular expressions via `preg_match()` / `preg_replace()` (link libpcre or minimal engine)

## v1.0.0 — Production-ready

- [ ] Stable compilation pipeline with documented semantics
- [ ] All built-in functions covered for CLI use cases
- [ ] Full test coverage (>500 tests)
- [ ] Documentation: language subset spec, architecture guide
- [ ] CI/CD with release binaries for macOS ARM64 and Linux x86_64
- [ ] Performance within 2x of C -O0 on compute benchmarks
- [ ] Real-world CLI tools compiled as validation (JSON parser, file processor, etc.)

---

## v1.1.0 — Shared and static libraries (C ABI)

PHP functions become callable from any language via FFI.

- [ ] `--lib` flag: emit only function symbols (no `_main`, no exit syscall)
- [ ] Export PHP functions as C-callable symbols (`function add()` → `_php_add`)
- [ ] `.global` directives for all exported functions
- [ ] Shared library output: `.dylib` (macOS) / `.so` (Linux) via `ld -dylib` / `ld -shared`
- [ ] Static library output: `.a` via `ar rcs`
- [ ] Auto-generated C header file (`.h`) with function signatures
- [ ] Null-terminated string convention for C interop (append `\0` to string returns)
- [ ] Wrapper functions for C-string input (`const char*` → internal ptr+len)
- [ ] String return convention: caller-provided buffer or malloc'd pointer
- [ ] Integration tests: call compiled PHP from C, verify results
- [ ] Documentation: FFI usage examples for C, Rust, Python (ctypes), Go (cgo)

## v1.2.0 — Library ecosystem

- [ ] `#[export]` annotation or `--export` flag to select which functions to expose
- [ ] Multiple PHP files compiled into a single library
- [ ] Symbol visibility control (public vs internal functions)
- [ ] `pkg-config` / `.pc` file generation for library discovery
- [ ] Versioned symbol names for ABI stability
- [ ] Benchmarks: PHP-compiled library vs equivalent C library

## v1.3.0 — WebAssembly target

PHP compiled to WASM — usable from any language with a WASM runtime, fully cross-platform.

- [ ] WASM codegen backend (parallel to ARM64)
- [ ] WASM text format (`.wat`) emission for debugging
- [ ] WASM binary format (`.wasm`) emission
- [ ] Linear memory management for strings and arrays
- [ ] WASI support for I/O (`fd_write`, `fd_read`, `args_get`)
- [ ] Integration tests: run compiled WASM via `wasmtime`
- [ ] NPM package generation for JS/Node consumption
- [ ] Documentation: usage from JS, Rust (wasmtime), Python (wasmer), Go
