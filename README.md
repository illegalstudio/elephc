# elephc

[![GitHub Stars](https://img.shields.io/github/stars/illegalstudio/elephc?style=flat-square&logo=github&label=stars)](https://github.com/illegalstudio/elephc/stargazers)
[![GitHub Downloads](https://img.shields.io/github/downloads/illegalstudio/elephc/total?style=flat-square&logo=github&label=downloads)](https://github.com/illegalstudio/elephc/releases)
[![Unique Cloners](.github/traffic/clones-badge.svg)](https://github.com/illegalstudio/elephc)
[![License: MIT](https://img.shields.io/github/license/illegalstudio/elephc?style=flat-square)](LICENSE)
[![Follow @nahime0 on X](https://img.shields.io/badge/Follow%20%40nahime0-000000?logo=x&logoColor=white)](https://x.com/nahime0)

> 🐦 **[Follow me on X (@nahime0)](https://x.com/nahime0) for updates, new features, and behind-the-scenes development.**

---

A PHP-to-native compiler. Takes a subset of PHP and compiles it directly to native assembly, producing standalone binaries for the currently supported targets: **macOS ARM64**, **Linux ARM64**, and **Linux x86_64**. No interpreter, no VM, no Zend Engine, no opcode fallback.

> **If you like the idea or find it useful, please star the repo** — it helps others discover it and keeps the project going.

> **Want to support the project?** elephc is built and maintained independently. If you'd like to help it grow, consider [sponsoring on GitHub](https://github.com/sponsors/nahime0). Every contribution — big or small — makes a real difference.

## DOOM rendered in PHP

The flagship showcase: a real-time 3D renderer that loads original DOOM WAD files and renders E1M1 — BSP traversal, perspective projection, per-column fog, sector lighting, collision detection, step climbing — entirely in PHP compiled to a native binary.

![DOOM E1M1 rendered in PHP](showcases/doom/demo.gif)

See [showcases/doom/](showcases/doom/) for full source and build instructions.

## Why

My first "serious programming" book was *PHP 4 and MySQL*. After years of experimenting with code, that book turned my passion into a profession. I've worked with many languages over the past 20 years, but PHP is the one that has most consistently put food on the table.

PHP has a simple, approachable, and elegant syntax. Millions of developers worldwide already know it well. That makes it an ideal bridge to bring web developers closer to lower-level programming — systems work, native binaries, understanding what happens under the hood — without forcing them to learn an entirely new language first.

One thing I always missed about PHP was the ability to produce optimized, fast native binaries. While everyone else is busy building the next Facebook, I thought I could try to fill that gap and write a compiler for PHP.

Of course, PHP has its limits when it comes to performance-critical or systems-level work. That's why elephc introduces compiler extensions like `packed class` for flat POD records, `buffer<T>` for contiguous typed arrays, `ptr` for raw memory access, and `extern` for FFI — constructs that give PHP developers the tools they need without abandoning the language they already know.

It's not perfect, but **it works**. It's a solid starting point, and more importantly, it's a great way to understand **how a compiler works** and how assembly language operates under the hood.

I made the project as modular as possible. Every function has its own codegen file, and each one is **commented line by line**, so you can see exactly how a high-level construct gets translated into its low-level equivalent.

## What you can expect

You can write PHP using the constructs documented in the [docs](docs/). Classes with single inheritance, interfaces, `instanceof`, nullsafe access (`?->`), abstract classes, final classes, methods and typed/static properties, PHP-style static property redeclarations, constructor property promotion, traits, constructors, instance/static methods, case-insensitive PHP symbol lookup for functions/classes/methods, `self::` / `parent::` / `static::` with late static binding, `readonly` properties and classes, enums, PHP 8 attributes on declarations, named arguments, first-class callables, typed function and method parameters and returns, `try` / `catch` / `finally` / `throw`, visibility modifiers, union and nullable types, copy-on-write arrays, associative arrays with PHP insertion order and integer/numeric-string key normalization, array union with `+`, closures, generator functions and generator closures with `yield` / `yield from`, namespaces, includes, compile-time Composer/SPL autoloading, class/introspection helpers, and PHP 8.1-style `Fiber` coroutines on macOS ARM64, Linux ARM64, and Linux x86_64.

For performance-oriented code, elephc exposes compiler extensions beyond standard PHP — see the Why section above.

Then compile and run:

```bash
elephc myfile.php
./myfile
```

The compiler is experimental and evolving. Not everything PHP supports is implemented, and you will find bugs. But as the DOOM showcase demonstrates, you can build real, non-trivial programs with it today.

If you want to contribute, you're welcome. Mi casa es tu casa.

## Learn how a compiler works

elephc is designed to be read. The code generation and runtime layers are heavily annotated, so you can see what each lowering step and emitted instruction is doing — from stack frame setup to syscall invocation, from integer-to-string conversion to array memory layout. If you've ever wondered what happens between `echo "hello"` and the CPU executing it, follow the code from `src/codegen/` and read the comments. **No prior assembly knowledge required.**

## How elephc is different

There are several ways to make PHP easier to distribute or faster to run: bundling a PHP runtime into one executable, encrypting bytecode, running through the Zend VM with JIT, or compiling selected hot paths while falling back to opcodes for dynamic code.

elephc takes a narrower but cleaner route: it is a from-scratch compiler for a static subset of PHP. It parses PHP source, type-checks it, lowers it to target-specific assembly, assembles and links it into a native executable, and ships only the small runtime routines needed by the generated program. If elephc compiles a construct, that construct is native code rather than interpreted PHP.

That tradeoff is intentional:

- **Less legacy compatibility** than a VM-backed PHP implementation.
- **More mechanical transparency**: readable assembly output, source maps, line-by-line commented codegen, and a documented memory model.
- **No hidden runtime dependency**: the generated binary does not need PHP, the Zend Engine, a loader extension, or an embedded interpreter.
- **Native-oriented extensions**: `extern`, `ptr`, `buffer<T>`, and `packed class` let PHP-shaped code cross into systems, FFI, game, and performance-sensitive workloads.

That does not mean elephc has to live outside the existing PHP ecosystem. The current CLI path produces standalone executables, but the roadmap also includes shared/static library output and an experimental PHP extension bridge. That opens a practical middle path: keep a framework such as WordPress, Laravel, or Symfony running on PHP, then compile static, performance-sensitive modules into native libraries or PHP extensions.

So elephc is not a drop-in replacement for an entire dynamic framework today. The longer-term goal is more useful: make it possible to move the parts of PHP code that are static enough to compile into inspectable native code, while the rest of the application can stay in ordinary PHP.

## Requirements

- Rust toolchain (`cargo`)
- A native assembler and linker for your host/target
- On macOS: Xcode Command Line Tools (`xcode-select --install`)
- On Linux: a standard native toolchain (`as`, `ld`, libc development files)

## Install

### Homebrew (recommended)

```bash
brew install illegalstudio/tap/elephc
```

### From source

```bash
git clone https://github.com/illegalstudio/elephc.git
cd elephc
cargo build --release
```

The binary is at `./target/release/elephc`.

### Manual download

Pre-built binaries are available on the [Releases](https://github.com/illegalstudio/elephc/releases) page. If macOS blocks the binary, run:

```bash
xattr -cr elephc
```

## Usage

```bash
# Compile a PHP file to a native binary
elephc hello.php
./hello

# Custom heap size (default: 8MB)
elephc --heap-size=16777216 heavy.php

# Enable runtime heap verification while debugging ownership issues
elephc --heap-debug heavy.php

# Print allocation/free counters to stderr while debugging GC behavior
elephc --gc-stats heavy.php

# Enable compile-time feature branches
elephc --define DEBUG app.php

# Print per-phase compiler timings
elephc --timings hello.php

# Emit assembly and a simple source-map sidecar
elephc --emit-asm --source-map hello.php

# Run the front-end checks without writing assembly or a binary
elephc --check hello.php

# Link extra native libraries or frameworks for FFI
elephc app.php -l sqlite3 -L /opt/homebrew/lib --framework Cocoa

# Explicit target selection
# Supported targets today: macos-aarch64, linux-aarch64, linux-x86_64
elephc --target linux-aarch64 hello.php
elephc --target linux-x86_64 hello.php
```

Or via cargo:

```bash
cargo run -- hello.php
./hello
```

## Showcases

| Showcase | Description |
|---|---|
| [DOOM E1M1](showcases/doom/) | Real-time 3D WAD renderer with BSP traversal, SDL2 FFI, `packed class` geometry, `buffer<T>` storage, collision detection, HUD |
| [SDL framebuffer](examples/sdl_framebuffer/) | Pixel-level rendering with SDL2 via FFI |
| [SDL audio](examples/sdl_audio/) | Audio playback with SDL2 via FFI |
| [Hot-path buffers](examples/hot-path/) | `packed class` + `buffer<T>` for performance-critical data |
| [FFI memory](examples/ffi-memory/) | Raw C memory patterns with `malloc`, `free`, `memcpy` via FFI |

## FFI

elephc can call native C functions directly through `extern` declarations.

```php
<?php
extern function atoi(string $s): int;
extern function signal(int $sig, callable $handler): ptr;
extern function raise(int $sig): int;
extern global ptr $environ;

function on_signal($sig) {
    echo "signal = " . $sig . "\n";
}

echo atoi("999") . "\n";
echo ptr_is_null($environ) ? "missing\n" : "ok\n";
signal(15, "on_signal");
raise(15);
```

Notes:

- `extern function`, `extern "lib" { ... }`, `extern global`, and `extern class` are supported.
- `string` arguments are copied to temporary null-terminated C strings for the duration of the native call.
- `string` return values are copied back into owned elephc strings.
- `callable` parameters pass a user-defined elephc function by string name, for example `"on_signal"`.
- Callback functions must stay C-compatible: use `int`, `float`, `bool`, `ptr`, or `void`-shaped values. String callbacks are not supported yet.
- Raw C memory patterns are supported through ordinary extern declarations such as `malloc`, `free`, `memcpy`, and `memset`.
- Pointer helpers include byte/word buffer access (`ptr_read8`, `ptr_read32`, `ptr_write8`, `ptr_write32`) in addition to `ptr_get` / `ptr_set`.

## What it compiles

elephc supports a growing subset of PHP and aims to match PHP behavior for the language features it implements.

```php
<?php
$pi = M_PI;
echo "Pi is approximately " . number_format($pi, 5) . "\n";
echo "2 ** 10 = " . (2 ** 10) . "\n";
echo "10 / 3 = " . (10 / 3) . "\n";
echo "Type: " . gettype($pi) . "\n";

$x = (int)$pi;
echo "Truncated: " . $x . "\n";

if ($x === 3) {
    echo "Correct!\n";
}
```

### Supported types

| Type | Example |
|---|---|
| `int` | `42`, `-7`, `0xFF`, `0755`, `0o755`, `0b1010`, `1_000_000`, `PHP_INT_MAX` |
| `float` | `3.14`, `.5`, `1e-5`, `1_000.5`, `1e1_0`, `INF`, `NAN` |
| `string` | `"hello\n"`, `'raw'` |
| `bool` | `true`, `false` |
| `null` | `null` |
| `void` | `function log_it(): void { echo "ok"; }` |
| `never` | `function fail(): never { throw new Exception("boom"); }` |
| `mixed` | `mixed $x = 42;`, `function show(mixed $x): string { ... }` |
| `iterable` | `function walk(iterable $items): iterable { ... }` (PHP `array \| Traversable` pseudo-type; accepts indexed arrays, associative arrays, `Iterator`, and `IteratorAggregate`) |
| `resource` | Successful `$f = fopen("file.txt", "r")`, `STDIN`, `STDOUT`, `STDERR` |
| `callable` | `function apply(callable $fn): int { return $fn(); }` |
| `array` | `[1, 2, 3]`, `["key" => "value"]`, `[[1,2],[3,4]]` (indexed, associative, multi-dimensional, copy-on-write, union with `+`) |
| `object` | `new Foo()`, `$user->name` |
| `pointer` | `ptr($x)`, `ptr_null()`, `ptr_cast<int>($p)` |
| `enum` | `enum Color: int { case Red = 1; }`, `Color::Red->value`, `Color::from(1)` |
| `int\|string` | `int\|string $x = 42;`, `function show(int\|string $x): string { ... }` |
| `?int` | `?int $x = null;`, `function find(): ?int { ... }` |
| `buffer<T>` | `buffer<int> $xs = buffer_new<int>(256)` |
| `packed class` | `packed class Vec2 { public float $x; public float $y; }` |

### Supported constructs

The full list of supported constructs, operators, and control structures is in the [docs](docs/). Highlights:

- **OOP**: classes, abstract/final classes, typed/final/static properties and methods, PHP-style static property redeclarations, direct static array property writes, constructor property promotion, interfaces, `instanceof`, traits, enums, PHP 8 declaration attributes, limited attribute reflection (`ReflectionClass`/`ReflectionMethod`/`ReflectionProperty::getAttributes()`, `ReflectionAttribute::newInstance()`), `readonly`, static/instance methods, case-insensitive class/interface/trait and method lookup, `self::`/`parent::`/`static::`, `::class` reflection, `new self()` / `new static()` / `new parent()`, magic methods (`__toString`, `__get`, `__set`)
- **Functions**: case-insensitive user and built-in function calls, default parameters, variadic/spread, pass by reference, named arguments, global variables, static locals, first-class callables, closures, arrow functions, static closures (`static function () { }`, `static fn () => ...`)
- **Generators**: generator functions and closures, `yield`, key/value yields, `yield from`, `Generator::send()`, `throw()`, `getReturn()`, and `foreach` over `Iterator` / `IteratorAggregate`
- **Fibers**: `Fiber`, `FiberError`, `Fiber::suspend()`, `Fiber::getCurrent()`, `start()`, `resume()`, `throw()`, `getReturn()`, state predicates, closure captures, guarded native stacks, and ARM64 / Linux x86_64 context switching
- **Control flow**: if/elseif/else, while, do-while, for, foreach, switch, match, break/continue including multi-level depths, try/catch/finally/throw
- **Statements and literals**: `const` / `define()` constants, `global` declarations, `static` locals, `print` expressions, list unpacking, PHP numeric literal forms, heredoc / nowdoc strings
- **Operators**: arithmetic, comparison, `instanceof`, logical, bitwise, ternary, null coalescing (`??`), PHP 8.5 pipe (`|>`), assignment expressions for local and stabilized non-local targets, null coalescing assignment (`??=`), error control (`@`), and compound assignments
- **Types**: union types (`int|string`), nullable (`?int`), `never` return type, `iterable` pseudo-type, inferred `resource|false` values for `fopen()` and `resource` values for standard streams, type casting, typed properties, typed function, method, closure, and arrow parameters and returns
- **Modules**: namespaces, use imports, include/require/include_once/require_once, compile-time Composer PSR-4/PSR-0/classmap/files autoloading, `spl_autoload_register()` rule extraction, PHP magic constants
- **FFI**: extern functions, extern blocks, extern globals, extern classes, pointer builtins
- **Extensions**: `ifdef`, `packed class`, `buffer<T>`, `buffer_new<T>()`, `buffer_len()`, `buffer_free()`

### Built-in functions (290+)

**Strings:** `strlen`, `substr`, `strpos`, `strrpos`, `strstr`, `str_replace`, `str_ireplace`, `substr_replace`, `strtolower`, `strtoupper`, `ucfirst`, `lcfirst`, `ucwords`, `trim`, `ltrim`, `rtrim`, `str_repeat`, `str_pad`, `strrev`, `str_split`, `strcmp`, `strcasecmp`, `str_contains`, `str_starts_with`, `str_ends_with`, `ord`, `chr`, `explode`, `implode`, `sprintf`, `printf`, `sscanf`, `md5`, `sha1`, `hash`, `number_format`, `addslashes`, `stripslashes`, `nl2br`, `wordwrap`, `bin2hex`, `hex2bin`, `htmlspecialchars`, `htmlentities`, `html_entity_decode`, `urlencode`, `urldecode`, `rawurlencode`, `rawurldecode`, `base64_encode`, `base64_decode`, `ctype_alpha`, `ctype_digit`, `ctype_alnum`, `ctype_space`

**Arrays:** `count`, `array_push`, `array_pop`, `in_array`, `array_keys`, `array_values`, `sort`, `rsort`, `isset`, `array_key_exists`, `array_search`, `array_merge`, `array_slice`, `array_splice`, `array_combine`, `array_flip`, `array_reverse`, `array_unique`, `array_sum`, `array_product`, `array_chunk`, `array_pad`, `array_fill`, `array_fill_keys`, `array_diff`, `array_intersect`, `array_diff_key`, `array_intersect_key`, `array_unshift`, `array_shift`, `asort`, `arsort`, `ksort`, `krsort`, `natsort`, `natcasesort`, `shuffle`, `array_rand`, `array_column`, `range`, `array_map`, `array_filter`, `array_reduce`, `array_walk`, `usort`, `uksort`, `uasort`, `call_user_func`, `call_user_func_array`, `function_exists`

**Math:** `abs`, `floor`, `ceil`, `round`, `sqrt`, `pow`, `min`, `max`, `intdiv`, `fmod`, `fdiv`, `rand`, `mt_rand`, `random_int`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`, `sinh`, `cosh`, `tanh`, `log`, `log2`, `log10`, `exp`, `hypot`, `deg2rad`, `rad2deg`, `pi`

**Types and class introspection:** `gettype`, `settype`, `empty`, `unset`, `is_int`, `is_float`, `is_string`, `is_bool`, `is_null`, `is_numeric`, `is_nan`, `is_finite`, `is_infinite`, `is_iterable`, `is_callable`, `boolval`, `floatval`, `intval`, `class_exists`, `interface_exists`, `trait_exists`, `enum_exists`, `class_alias`, `get_class`, `get_parent_class`, `get_declared_classes`, `get_declared_interfaces`, `get_declared_traits`, `is_a`, `is_subclass_of`

**I/O:** `fopen`, `fclose`, `fread`, `fwrite`, `fgets`, `fgetc`, `fpassthru`, `flock`, `tmpfile`, `readfile`, `feof`, `readline`, `fseek`, `ftell`, `rewind`, `file_get_contents`, `file_put_contents`, `file`, `fgetcsv`, `fputcsv`, `file_exists`, `is_file`, `is_dir`, `is_readable`, `is_writable`, `is_writeable`, `is_executable`, `is_link`, `symlink`, `link`, `readlink`, `linkinfo`, `filesize`, `filemtime`, `fileatime`, `filectime`, `fileperms`, `fileowner`, `filegroup`, `fileinode`, `filetype`, `stat`, `lstat`, `fstat`, `clearstatcache`, `basename`, `dirname`, `pathinfo`, `realpath`, `fnmatch`, `touch`, `chmod`, `chown`, `chgrp`, `umask`, `ftruncate`, `fflush`, `fsync`, `fdatasync`, `copy`, `rename`, `unlink`, `mkdir`, `rmdir`, `scandir`, `glob`, `getcwd`, `chdir`, `tempnam`, `sys_get_temp_dir`, `var_dump`, `print_r`

**System:** `exit`, `die`, `time`, `microtime`, `date`, `mktime`, `strtotime`, `sleep`, `usleep`, `getenv`, `putenv`, `php_uname`, `phpversion`, `exec`, `shell_exec`, `system`, `passthru`, `json_encode`, `json_decode`, `json_last_error`, `json_last_error_msg`, `json_validate`, `preg_match`, `preg_match_all`, `preg_replace`, `preg_split`, `define`, `class_attribute_names`, `class_attribute_args`, `class_get_attributes`

**SPL/autoload:** `spl_autoload_register`, `spl_autoload_unregister`, `spl_autoload_functions`, `spl_autoload_extensions`, `spl_autoload_call`, `spl_autoload`, `spl_classes`, `spl_object_id`, `spl_object_hash`

**Pointers/Buffers:** `ptr`, `ptr_null`, `ptr_is_null`, `ptr_get`, `ptr_set`, `ptr_read8`, `ptr_read32`, `ptr_write8`, `ptr_write32`, `ptr_offset`, `ptr_cast<T>`, `ptr_sizeof`, `buffer_new<T>`, `buffer_len`, `buffer_free`

### Constants

`INF`, `NAN`, `PHP_INT_MAX`, `PHP_INT_MIN`, `PHP_FLOAT_MAX`, `PHP_FLOAT_MIN`, `PHP_FLOAT_EPSILON`, `M_PI`, `M_E`, `M_SQRT2`, `M_PI_2`, `M_PI_4`, `M_LOG2E`, `M_LOG10E`, `PHP_EOL`, `PHP_OS`, `DIRECTORY_SEPARATOR`, `STDIN`, `STDOUT`, `STDERR`, `PATHINFO_DIRNAME`, `PATHINFO_BASENAME`, `PATHINFO_EXTENSION`, `PATHINFO_FILENAME`, `PATHINFO_ALL`, `FNM_NOESCAPE`, `FNM_PATHNAME`, `FNM_PERIOD`, `FNM_CASEFOLD`, `LOCK_SH`, `LOCK_EX`, `LOCK_UN`, `LOCK_NB`, `JSON_HEX_TAG`, `JSON_HEX_AMP`, `JSON_HEX_APOS`, `JSON_HEX_QUOT`, `JSON_FORCE_OBJECT`, `JSON_NUMERIC_CHECK`, `JSON_UNESCAPED_SLASHES`, `JSON_PRETTY_PRINT`, `JSON_UNESCAPED_UNICODE`, `JSON_PARTIAL_OUTPUT_ON_ERROR`, `JSON_PRESERVE_ZERO_FRACTION`, `JSON_INVALID_UTF8_IGNORE`, `JSON_INVALID_UTF8_SUBSTITUTE`, `JSON_THROW_ON_ERROR`, `JSON_OBJECT_AS_ARRAY`, `JSON_BIGINT_AS_STRING`, `JSON_ERROR_NONE`, `JSON_ERROR_DEPTH`, `JSON_ERROR_STATE_MISMATCH`, `JSON_ERROR_CTRL_CHAR`, `JSON_ERROR_SYNTAX`, `JSON_ERROR_UTF8`, `JSON_ERROR_RECURSION`, `JSON_ERROR_INF_OR_NAN`, `JSON_ERROR_UNSUPPORTED_TYPE`, `JSON_ERROR_INVALID_PROPERTY_NAME`, `JSON_ERROR_UTF16`, `__DIR__`, `__FILE__`, `__LINE__`, `__FUNCTION__`, `__CLASS__`, `__METHOD__`, `__NAMESPACE__`, `__TRAIT__`

User-defined constants are also supported via `const NAME = value;` and `define("NAME", value);`. Constants remain case-sensitive, matching PHP.

## How it works

```
PHP source → Lexer → Parser (AST) → Magic constants (per-file) → Conditional (ifdef/--define) → Autoload registry build (Composer + SPL rules) → Resolver (include declaration discovery, include/require inlining, per-file constants, once guards, function variant marks) → NameResolver (namespaces/use/FQNs) → Autoload run (class-triggered file insertion) → Optimizer (constant folding) → Type Checker → Optimizer (constant propagation) → Optimizer (control-flow pruning) → Optimizer (control-flow normalization) → Optimizer (dead-code elimination) → Codegen → as + ld → native executable
```

The compiler emits human-readable assembly for the selected target. You can inspect the `.s` file to see exactly what your PHP becomes:

```bash
elephc hello.php
cat hello.s
```

If you add `--source-map`, elephc also writes `hello.map`, a compact JSON sidecar that maps emitted assembly lines back to PHP line/column pairs. If you add `--timings`, the compiler prints per-phase durations such as lexing, parsing, early optimization, type checking, constant propagation, post-check pruning, control-flow normalization, dead-code elimination, runtime-cache preparation, code generation, assembling, and linking.

### Current optimization passes

elephc already performs a small but useful AST-level optimization pipeline before emitting assembly:

- **Constant folding before type checking**: folds scalar arithmetic, bitwise ops, comparisons, logical ops, string-literal concatenation, scalar casts, ternaries, null coalescing, known `match` expressions, and scalar indexed/associative array-literal reads when the result is statically known.
- **Constant propagation after type checking**: forwards scalar local values through straight-line code, across agreeing `if` / `switch` / `try` merges, through known-subject `switch` paths, through non-throwing `try` bodies without poisoning the merge with unreachable catches, through uniform local `?:` / `match` assignments, through fixed scalar destructuring like `[$a, $b] = [2, 3]`, and across simple loops when untouched locals or stable `for` init assignments can be proven safe even with conservative nested `switch`, `try/catch/finally`, `foreach`, other simple nested loop writes, local array mutations like `$items[] = $i` / `$items[0] = $i`, local property writes like `$box->last = $i` / `$box->items[] = $i`, or targeted local invalidations like `unset($tmp)`. It also uses local loop path summaries for known `while(false)`, `do...while(false)`, `while(true)` / `for(;;)` break exits, and branch-local loop exits that agree on scalar values, which in turn unlocks more folding in later expressions such as `$x ** $y`.
- **Control-flow pruning after type checking**: removes constant-dead `if` / `elseif` / `while (false)` / `for (...; false; ...)` branches, materializes constant `switch` execution, prunes `match` arms, and trims unreachable statements after terminating constructs such as `return`, `throw`, `break`, and `continue`.
- **Control-flow normalization after pruning**: canonicalizes equivalent residual shapes such as nested `elseif` chains, merged `if` heads/tails, single-case or fallthrough-only `switch` shells, canonical multi-catch handlers, folded outer `finally` wrappers, and identical `if` branches so later passes see fewer structurally different but semantically identical trees.
- **Dead-code elimination after normalization**: removes empty control shells, simplifies single-path conditionals, prunes guard contradictions across boolean, strict-scalar, loose-equality, and safe relational checks, uses CFG-lite reachability for local `if` / `switch` / `try` shapes, hoists safe non-throwing `try` prefixes, and drops unused pure expression statements and dead pure subexpressions when the surrounding expression already determines the result.
- **Local effect summaries for purity / may-throw reasoning**: tracks known pure and non-throwing builtins, user functions, static methods, private `$this` methods, closures, first-class callables, and merged callable aliases through `if` / `switch` / `try` control flow so the optimizer can simplify `try` regions and prune dead handlers more precisely.

The optimizer is intentionally conservative. It does not yet do full function-level CFG fixed-point propagation, aggressive whole-program optimization, or assembly-level peephole rewriting, but it does compute lightweight effect summaries and local CFG-lite reachability for known call targets and structured control flow so AST rewrites can stay more precise without becoming risky.

### Type system

The static type system tracks these runtime shapes at compile time:

- **Int** — 64-bit signed integer
- **Float** — 64-bit double-precision
- **Str** — pointer + length pair
- **Bool** — `true`/`false`, coerces to 0/1
- **Void / null** — null sentinel value, coerces to 0/""
- **Never** — non-returning function/method/closure return type
- **Iterable** — type-erased array / `Traversable` pseudo-type
- **Array** — indexed arrays with inferred element type; heterogeneous payloads widen to boxed `Mixed`
- **AssocArray** — associative arrays with key/value types
- **Buffer** — fixed-size contiguous `buffer<T>` storage for hot-path values
- **Mixed** — boxed runtime-tagged payload used for heterogeneous array values, union storage, and user-facing `mixed` hints
- **Callable** — closures and callable function references
- **Object** — heap-allocated class instances
- **Packed** — nominal packed-record metadata used with pointers and buffers
- **Pointer** — raw 64-bit addresses, optionally tagged via `ptr_cast<T>()`
- **Resource** — stream handles such as successful `fopen()` results and standard streams
- **Union** — declared union types lowered to boxed tagged runtime payloads

A variable's type is set at first assignment. Compatible types (int/float/bool/null) can be reassigned between each other.

## Error messages

Errors include line and column numbers, and the compiler tries to recover far enough to report multiple independent syntax / semantic errors in one pass. Successful compilations may also emit non-fatal warnings such as unused variables / parameters or unreachable code:

```
error[3:1]: Undefined variable: $x
error[5:7]: Type error: cannot reassign $x from Int to Str
error[2:1]: Required file not found: 'missing.php'
warning[9:5]: Unused variable: $tmp
warning[14:9]: Unreachable code
```

## Project structure

High-level map of the source tree. The codebase contains more focused helper submodules than shown here; treat this as an orientation guide rather than a byte-for-byte file listing.

```
src/
├── lib.rs               # Public module exports
├── main.rs              # CLI binary entry point
├── cli.rs               # Command-line argument parsing and options
├── pipeline.rs          # Frontend/backend compilation pipeline
├── linker.rs            # Assembler + linker invocation
├── timings.rs           # Phase timing collection/reporting
├── span.rs              # Source position tracking (line, col)
├── conditional/         # Build-time `ifdef` pass driven by --define
├── magic_constants.rs   # Per-file PHP magic constant lowering
├── magic_constants/     # File/scope/trait magic-constant walkers
├── autoload/            # Composer/SPL AOT autoload indexing and file insertion
├── resolver/            # Include/require resolution, declaration discovery, once guards
├── runtime_cache.rs     # Preassembled runtime object cache
├── source_map.rs        # Assembly/source-map sidecar emission
├── termination.rs       # Structured terminal-effect analysis
├── optimize.rs          # Optimizer public entry points and effect context
├── optimize/            # AST optimizer: folding, propagation, pruning, normalization, dead-code elimination
├── names.rs             # Qualified/FQN name model + symbol mangling helpers
├── name_resolver/       # Namespace/use resolution to canonical names
│
├── lexer/               # Source text → token stream
│   ├── token.rs         # Token enum
│   ├── scan.rs          # Main scanning loop, operators
│   ├── literals.rs      # Literal scanning entry point
│   ├── literals/        # Identifier, number, and string scanners
│   └── cursor.rs        # Byte-level source reader
│
├── parser/              # Tokens → AST (Pratt parser)
│   ├── ast/             # ExprKind, StmtKind, BinOp, CastType
│   ├── expr/            # Expression parsing helpers and Pratt parser passes
│   ├── stmt/            # Statement parsing, OOP, namespaces, FFI
│   └── control.rs       # if, while, for, foreach, do-while, switch, try/catch/finally
│
├── types/               # Static type checking
│   ├── mod.rs           # check() entry point and type exports
│   ├── model.rs         # PhpType and TypeEnv
│   ├── result.rs        # CheckResult and semantic metadata
│   ├── signatures.rs    # Built-in and callable signatures
│   ├── call_args/       # Shared named/spread call planner
│   ├── schema.rs        # Class/interface/enum metadata
│   ├── fibers.rs        # Fiber callback validation
│   ├── traits.rs        # Trait flattening and conflict resolution
│   ├── traits/          # Trait expansion, merge, and validation helpers
│   ├── warnings/        # Non-fatal diagnostics (unused vars, unreachable code)
│   └── checker/
│       ├── mod.rs       # Type-checker orchestration
│       ├── builtin_interfaces.rs # Built-in SPL/core interface injection
│       ├── builtin_iterators.rs # Built-in Iterator / IteratorAggregate metadata
│       ├── builtin_json.rs # JsonException / JsonSerializable metadata
│       ├── builtin_spl_exceptions.rs # SPL exception hierarchy metadata
│       ├── builtin_stdclass.rs # stdClass dynamic-property metadata
│       ├── builtin_types/ # Built-in class/interface/enum metadata
│       ├── builtins/    # Built-in function type signatures
│       ├── callables/   # Callable values, first-class callables, and callback checks
│       ├── driver/      # Checker initialization and orchestration helpers
│       ├── functions/   # User function type inference
│       ├── inference/   # Focused inference helpers
│       ├── schema/      # Class/interface/trait/enum schema validation
│       ├── stmt_check/  # Statement-level checking helpers
│       ├── type_compat/ # Type compatibility and assignment rules
│       └── yield_validation/ # Generator/yield placement validation
│
├── codegen/             # AST → target assembly
│   ├── mod.rs           # Pipeline entry, main/global codegen orchestration
│   ├── driver_support.rs # Pipeline glue and orchestration helpers
│   ├── prescan.rs       # Pre-pass collecting program-wide codegen metadata
│   ├── program_usage.rs # Usage analysis feeding metadata emission
│   ├── expr.rs          # Expression codegen dispatcher
│   ├── expr/            # Focused expression helpers (arrays, calls, objects, binops, ...)
│   ├── stmt.rs          # Statement codegen dispatcher
│   ├── stmt/            # Focused statement helpers (arrays, control_flow, io, storage, ...)
│   ├── abi/             # Target-aware calling-convention, frame, and value helpers
│   ├── functions/       # User function emission, wrappers, and epilogue cleanup
│   ├── main_emission.rs # Top-level program emission
│   ├── class_methods.rs # Class/static method emission orchestration
│   ├── function_variants.rs # Include-loaded function dispatchers
│   ├── interface_wrappers.rs # Interface dispatch return-shape adapters
│   ├── ffi.rs           # Extern function/global/class codegen
│   ├── context.rs       # Variables, labels, loop/finally stacks, ownership lattice
│   ├── data_section.rs  # String/float literal .data section
│   ├── emit.rs          # Assembly text buffer
│   ├── platform/        # Target parsing, syscall remapping, Linux transforms
│   │
│   ├── builtins/        # Built-in function codegen (one file per language function)
│   │   ├── strings/     # strlen, substr, strpos, explode, implode, ...
│   │   ├── arrays/      # count, array_push, array_pop, sort, ...
│   │   ├── math/        # abs, floor, pow, rand, fmod, ...
│   │   ├── types/       # is_int, gettype, empty, unset, settype, ...
│   │   ├── spl/         # spl_autoload_*, spl_classes, spl_object_id/hash
│   │   ├── io/          # fopen, fclose, fread, fwrite, fgets, var_dump, print_r, file_get_contents, ...
│   │   ├── pointers/    # ptr, ptr_get, ptr_set, ptr_read8, ptr_write8, ptr_offset, ...
│   │   └── system/      # exit, die, time, sleep, getenv, exec, ...
│   │
│   └── runtime/         # Runtime routines and target-specific emission helpers
│       ├── strings/     # itoa, concat, ftoa, strpos, str_replace, ...
│       ├── arrays/      # heap_alloc, array_new, array_push, sort, ...
│       ├── buffers/     # buffer_new, buffer_len, bounds and lifetime checks
│       ├── data/        # fixed and user-program runtime data/metadata
│       ├── exceptions.rs # exception runtime orchestration / re-exports
│       ├── exceptions/  # setjmp/longjmp-based exception helpers
│       ├── io/          # fopen, fclose, fread, fwrite, file_ops, ...
│       ├── objects/     # stdClass, dynamic property, mixed object/index helpers
│       ├── pointers/    # ptoa, ptr_check_nonnull, str_to_cstr, cstr_to_str
│       ├── system/      # build_argv, time, getenv, shell_exec
│       ├── fibers/      # Fiber stacks, context switch, entry trampoline, Fiber API
│       └── generators/  # Generator frame layout and __rt_gen_* helpers
│
└── errors/              # Error formatting with line:col
```

## Tests

3000+ tests across lexer, parser, codegen, and error reporting. Each codegen test compiles inline PHP source to a native binary, runs it, and asserts stdout.

```bash
cargo test                      # all tests
cargo test -- --include-ignored # all tests, including ignored integration tests
cargo test test_my_feature      # run specific tests
ELEPHC_PHP_CHECK=1 cargo test   # cross-check output with PHP interpreter
./scripts/test-linux-arm64.sh   # Linux ARM64 suite in Docker
./scripts/test-linux-x86_64.sh  # Linux x86_64 suite in Docker
```

## Documentation

The **[docs/](docs/)** directory is a complete wiki covering every aspect of the compiler. Inside you'll find:

- **PHP syntax reference** — types, operators, control structures, functions, classes, namespaces, and all 290+ built-in functions with signatures and examples
- **Compiler extensions** — pointers, `buffer<T>`, `packed class`, FFI with `extern`, and conditional compilation with `ifdef` — the features that take PHP beyond the web
- **Compiler internals** — a step-by-step walkthrough of the full pipeline, from lexing to Pratt parsing to type checking to code generation and runtime structure
- **ARM64 primer** — an introduction to ARM64 assembly for people who've never seen it, plus a quick reference of the ARM64 instruction set used by elephc's AArch64 backend
- **Memory model** — how the stack, heap, concat buffer, and hash tables work under the hood

If you're new to compilers or assembly, start from the top and work your way down. No prior low-level knowledge required.

For runnable language samples, see `examples/`. For the benchmark harness and CI trend artifacts that compare elephc against PHP and equivalent C fixtures, see `benchmarks/README.md`. For a focused perf comparison, see `benchmarks/hot-path-buffer-vs-arrays`.

## License

MIT
