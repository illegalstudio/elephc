# elephc

[![GitHub Downloads](https://img.shields.io/github/downloads/illegalstudio/elephc/total?style=flat-square&logo=github&label=downloads)](https://github.com/illegalstudio/elephc/releases)
[![License: MIT](https://img.shields.io/github/license/illegalstudio/elephc?style=flat-square)](LICENSE)

A PHP-to-native compiler. Takes a subset of PHP and compiles it directly to **ARM64 assembly**, producing **standalone macOS binaries**. No interpreter, no VM, no runtime dependencies.

> **If you like the idea or find it useful, please star the repo** — it helps others discover it and keeps the project going.

## Why

My first "serious programming" book was *PHP 4 and MySQL*. After years of experimenting with code, that book turned my passion into a profession. I've worked with many languages over the past 20 years, but PHP is the one that has most consistently put food on the table.

One thing I always missed about PHP was the ability to produce optimized, fast native binaries. With the advent of AI, we can build ambitious things quickly. While everyone else is busy building the next Facebook, chasing ephemeral wealth that will never come, I thought I could try to fill that gap and write a compiler for PHP.

It's not perfect — it's 99% written by Claude — but **it works**. It's a solid starting point, and more importantly, it's a great way to understand **how a compiler works** and how assembly language operates under the hood.

I made the project as modular as possible. Every function has its own codegen file, and each one is **commented line by line**, so you can see exactly how a high-level construct gets translated into its low-level equivalent.

### What you should not expect

Don't expect to take any existing PHP project and magically compile it. There's no Composer and no interfaces yet. We support PHP classes with single inheritance, traits, constructors, instance/static methods, `self::method()`, `parent::method()`, `static::method()` with late static binding, `readonly` properties, and `public` / `protected` / `private` visibility — roughly at the level of that famous *PHP 4* book where my journey began, plus some PHP 8 features.

### What you can expect

You can write a PHP file using only the constructs documented in this project's [language reference](docs/language-reference.md). You can include other files with `include`, `require`, `include_once`, and `require_once`, compose classes with traits, extend concrete classes with `extends`, and watch your code run at the speed of light after running:

```bash
elephc myfile.php
```

But you should also expect the binary to segfault, the compiler to blow up, or worse. So experiment, have fun, but don't expect to use elephc for anything serious — at least not yet. I'd love for that to be possible someday. We'll see how it evolves.

If you want to contribute, you're welcome. Mi casa es tu casa.

## Learn how a compiler works

elephc is designed to be read. **Every line of Rust that emits ARM64 assembly** is annotated with an inline comment explaining what it does and why — from stack frame setup to syscall invocation, from integer-to-string conversion to array memory layout. If you've ever wondered what happens between `echo "hello"` and the CPU executing it, follow the code from `src/codegen/` and read the comments. **No prior assembly knowledge required.**

## Requirements

- Rust toolchain (`cargo`)
- Xcode Command Line Tools (`xcode-select --install`)
- macOS on Apple Silicon (ARM64)

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

# Link extra native libraries or frameworks for FFI
elephc app.php -l sqlite3 -L /opt/homebrew/lib --framework Cocoa
```

Or via cargo:

```bash
cargo run -- hello.php
./hello
```

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
- See `examples/ffi-memory`, `examples/sdl_window`, `examples/sdl_input`, `examples/sdl_framebuffer`, and `examples/sdl_audio` for end-to-end native interop examples.

## What it compiles

elephc supports a growing subset of PHP and aims to match PHP behavior for the language features it implements. Most supported programs are ordinary PHP, but elephc also includes compiler-specific pointer builtins such as `ptr()` and `ptr_cast<T>()` that intentionally extend PHP syntax.

```php
<?php
require_once 'math.php';

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
| `int` | `42`, `-7`, `PHP_INT_MAX` |
| `float` | `3.14`, `.5`, `1e-5`, `INF`, `NAN` |
| `string` | `"hello\n"`, `'raw'` |
| `bool` | `true`, `false` |
| `null` | `null` |
| `array` | `[1, 2, 3]`, `["key" => "value"]`, `[[1,2],[3,4]]` (indexed, associative, multi-dimensional) |
| `object` | `new Foo()`, `$user->name` |
| `pointer` | `ptr($x)`, `ptr_null()`, `ptr_cast<int>($p)` |

### Supported constructs

| Construct | Example |
|---|---|
| Echo / Print | `echo $x;`, `print $x;` |
| Variables | `$name = "hello";` |
| Arithmetic | `+`, `-`, `*`, `/`, `%`, `**` |
| Comparison | `==`, `!=`, `<`, `>`, `<=`, `>=`, `===`, `!==`, `<=>` |
| Logical | `&&`, `\|\|`, `!` |
| Bitwise | `&`, `\|`, `^`, `~`, `<<`, `>>` |
| Null coalescing | `$x ?? $default` |
| Concatenation | `"a" . "b"`, `"val=" . 42` |
| Assignment | `=`, `+=`, `-=`, `*=`, `/=`, `%=`, `.=` |
| Increment/Decrement | `$i++`, `++$i`, `$i--`, `--$i` |
| Type casting | `(int)`, `(float)`, `(string)`, `(bool)`, `(array)` |
| Ternary | `$x > 0 ? "yes" : "no"` |
| If / elseif / else | `if (...) { } elseif (...) { } else { }` |
| While / Do-while | `while (...) { }`, `do { } while (...);` |
| For / Foreach | `for (;;) { }`, `foreach ($arr as $v) { }`, `foreach ($arr as $k => $v) { }` |
| Switch | `switch ($x) { case 1: ...; break; default: ...; }` |
| Match | `$r = match($x) { 1 => "one", default => "other" };` |
| Break / Continue | `break;`, `continue;` |
| Functions | `function foo($x, $y = 10) { return $x + $y; }` |
| Variadic / Spread | `function sum(...$args) { }`, `func(...$arr)`, `[...$a, ...$b]` |
| Pass by reference | `function inc(&$x) { $x++; }` |
| Global / Static | `global $var;`, `static $counter = 0;` |
| Closures / Arrow | `$fn = function($x) use ($y) { return $x * $y; };`, `fn($x) => $x * 2` |
| Constants | `const MAX = 100;`, `define("PI", 3.14)` |
| List unpacking | `[$a, $b] = [1, 2];` |
| Include/Require | `include 'file.php';`, `require_once 'lib.php';` |
| Classes | `class Foo extends Base { public readonly $id; protected $x; private $y; public function get() { return parent::get() + $this->x; } }` |
| Traits | `trait Named { public function name() { return "x"; } }`, `use Named { Named::name as protected; }` |
| New / Property / Method | `$f = new Foo(); $f->x = 1; $f->get();` |
| Static methods | `Foo::create()` |
| String interpolation | `"Hello $name"` |
| Heredoc / Nowdoc | `<<<EOT ... EOT;`, `<<<'EOT' ... EOT;` |
| Comments | `// ...`, `/* ... */` |

### Built-in functions

**Strings:** `strlen`, `intval`, `number_format`, `substr`, `strpos`, `strrpos`, `strstr`, `str_replace`, `str_ireplace`, `substr_replace`, `strtolower`, `strtoupper`, `ucfirst`, `lcfirst`, `ucwords`, `trim`, `ltrim`, `rtrim`, `str_repeat`, `str_pad`, `strrev`, `str_split`, `strcmp`, `strcasecmp`, `str_contains`, `str_starts_with`, `str_ends_with`, `ord`, `chr`, `explode`, `implode`, `addslashes`, `stripslashes`, `nl2br`, `wordwrap`, `bin2hex`, `hex2bin`, `sprintf`, `printf`, `sscanf`, `md5`, `sha1`, `hash`, `htmlspecialchars`, `htmlentities`, `html_entity_decode`, `urlencode`, `urldecode`, `rawurlencode`, `rawurldecode`, `base64_encode`, `base64_decode`, `ctype_alpha`, `ctype_digit`, `ctype_alnum`, `ctype_space`
**Arrays:** `count`, `array_push`, `array_pop`, `in_array`, `array_keys`, `array_values`, `sort`, `rsort`, `isset`, `array_key_exists`, `array_search`, `array_merge`, `array_slice`, `array_splice`, `array_combine`, `array_flip`, `array_reverse`, `array_unique`, `array_sum`, `array_product`, `array_chunk`, `array_pad`, `array_fill`, `array_fill_keys`, `array_diff`, `array_intersect`, `array_diff_key`, `array_intersect_key`, `array_unshift`, `array_shift`, `asort`, `arsort`, `ksort`, `krsort`, `natsort`, `natcasesort`, `shuffle`, `array_rand`, `array_column`, `range`, `array_map`, `array_filter`, `array_reduce`, `array_walk`, `usort`, `uksort`, `uasort`, `call_user_func`, `call_user_func_array`, `function_exists`
**Math:** `abs`, `floor`, `ceil`, `round`, `sqrt`, `pow`, `min`, `max`, `intdiv`, `fmod`, `fdiv`, `rand`, `mt_rand`, `random_int`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`, `sinh`, `cosh`, `tanh`, `log`, `log2`, `log10`, `exp`, `hypot`, `deg2rad`, `rad2deg`, `pi`
**Types:** `gettype`, `settype`, `empty`, `unset`, `is_int`, `is_float`, `is_string`, `is_bool`, `is_null`, `is_numeric`, `is_nan`, `is_finite`, `is_infinite`, `boolval`, `floatval`
**I/O:** `fopen`, `fclose`, `fread`, `fwrite`, `fgets`, `feof`, `readline`, `fseek`, `ftell`, `rewind`, `file_get_contents`, `file_put_contents`, `file`, `fgetcsv`, `fputcsv`, `file_exists`, `is_file`, `is_dir`, `is_readable`, `is_writable`, `filesize`, `filemtime`, `copy`, `rename`, `unlink`, `mkdir`, `rmdir`, `scandir`, `glob`, `getcwd`, `chdir`, `tempnam`, `sys_get_temp_dir`
**Pointers:** `ptr`, `ptr_null`, `ptr_is_null`, `ptr_get`, `ptr_set`, `ptr_read8`, `ptr_read32`, `ptr_write8`, `ptr_write32`, `ptr_offset`, `ptr_cast<T>`, `ptr_sizeof`
**Debugging:** `var_dump`, `print_r`
**System:** `exit`, `die`, `define`, `time`, `microtime`, `date`, `mktime`, `strtotime`, `sleep`, `usleep`, `getenv`, `putenv`, `php_uname`, `phpversion`, `exec`, `shell_exec`, `system`, `passthru`, `json_encode`, `json_decode`, `json_last_error`, `preg_match`, `preg_match_all`, `preg_replace`, `preg_split`

### Constants

`INF`, `NAN`, `PHP_INT_MAX`, `PHP_INT_MIN`, `PHP_FLOAT_MAX`, `PHP_FLOAT_MIN`, `PHP_FLOAT_EPSILON`, `M_PI`, `M_E`, `M_SQRT2`, `M_PI_2`, `M_PI_4`, `M_LOG2E`, `M_LOG10E`, `PHP_EOL`, `PHP_OS`, `DIRECTORY_SEPARATOR`, `STDIN`, `STDOUT`, `STDERR`

User-defined constants are also supported via `const NAME = value;` and `define("NAME", value);`.

## How it works

```
PHP source → Lexer → Parser (AST) → Resolver (include) → Type Checker → Codegen → as + ld → Mach-O binary
```

The compiler emits human-readable ARM64 assembly. You can inspect the `.s` file to see exactly what your PHP becomes:

```bash
elephc hello.php
cat hello.s
```

### Type system

The static type system tracks these runtime shapes at compile time:

- **Int** — 64-bit signed integer
- **Float** — 64-bit double-precision
- **Str** — pointer + length pair
- **Bool** — `true`/`false`, coerces to 0/1
- **Void / null** — null sentinel value, coerces to 0/""
- **Array** — indexed arrays with inferred element type
- **AssocArray** — associative arrays with key/value types
- **Callable** — closures and callable function references
- **Object** — heap-allocated class instances
- **Pointer** — raw 64-bit addresses, optionally tagged via `ptr_cast<T>()`

A variable's type is set at first assignment. Compatible types (int/float/bool/null) can be reassigned between each other.

## Error messages

Errors include line and column numbers:

```
error[3:1]: Undefined variable: $x
error[5:7]: Type error: cannot reassign $x from Int to Str
error[2:1]: Required file not found: 'missing.php'
```

## Project structure

```
src/
├── main.rs              # CLI entry point, assembler + linker invocation
├── lib.rs               # Public module exports
├── span.rs              # Source position tracking (line, col)
├── resolver.rs          # Include/require file resolution
│
├── lexer/               # Source text → token stream
│   ├── token.rs         # Token enum
│   ├── scan.rs          # Main scanning loop, operators
│   ├── literals.rs      # String, number, variable, keyword scanning
│   └── cursor.rs        # Byte-level source reader
│
├── parser/              # Tokens → AST (Pratt parser)
│   ├── ast.rs           # ExprKind, StmtKind, BinOp, CastType
│   ├── expr.rs          # Expression parsing with binding powers
│   ├── stmt.rs          # Statement parsing
│   └── control.rs       # if, while, for, foreach, do-while
│
├── types/               # Static type checking
│   ├── mod.rs           # PhpType, TypeEnv, check(), CheckResult
│   ├── traits.rs        # Trait flattening and conflict resolution
│   └── checker/
│       ├── mod.rs       # check_stmt(), infer_type()
│       ├── builtins.rs  # Built-in function type signatures
│       └── functions.rs # User function type inference
│
├── codegen/             # AST → ARM64 assembly
│   ├── mod.rs           # Pipeline entry, main/global codegen orchestration
│   ├── expr.rs          # Expression codegen
│   ├── stmt.rs          # Statement codegen
│   ├── abi.rs           # Register conventions (load, store, write)
│   ├── functions.rs     # User function emission
│   ├── ffi.rs           # Extern function/global/class codegen
│   ├── context.rs       # Variables, labels, loop stack
│   ├── data_section.rs  # String/float literal .data section
│   ├── emit.rs          # Assembly text buffer
│   │
│   ├── builtins/        # Built-in function codegen (one file per language function)
│   │   ├── strings/     # strlen, substr, strpos, explode, implode, ...
│   │   ├── arrays/      # count, array_push, array_pop, sort, ...
│   │   ├── math/        # abs, floor, pow, rand, fmod, ...
│   │   ├── types/       # is_int, gettype, empty, unset, settype, ...
│   │   ├── io/          # fopen, fclose, fread, fwrite, fgets, file_get_contents, ...
│   │   ├── pointers/    # ptr, ptr_get, ptr_set, ptr_read8, ptr_write8, ptr_offset, ...
│   │   └── system/      # exit, die, time, sleep, getenv, exec, ...
│   │
│   └── runtime/         # ARM64 runtime routines (one file per language/runtime helper)
│       ├── strings/     # itoa, concat, ftoa, strpos, str_replace, ...
│       ├── arrays/      # heap_alloc, array_new, array_push, sort, ...
│       ├── io/          # fopen, fclose, fread, fwrite, file_ops, ...
│       ├── pointers/    # ptoa, ptr_check_nonnull, str_to_cstr, cstr_to_str
│       └── system/      # build_argv, time, getenv, shell_exec
│
└── errors/              # Error formatting with line:col
```

## Tests

```bash
cargo test                      # all tests
cargo test test_my_feature      # run specific tests
ELEPHC_PHP_CHECK=1 cargo test   # cross-check output with PHP interpreter
```

## Documentation

The `docs/` directory is a **complete wiki** covering every aspect of the compiler — from what a compiler is, to how each phase works, to the ARM64 instruction set. If you're new to compilers or assembly, **start from the top and work your way down**.

For runnable language samples, start with `examples/classes`, `examples/inheritance`, `examples/traits`, `examples/arrays`, `examples/closures`, and `examples/ffi-memory`.

| Guide | What you'll learn |
|---|---|
| [What is a compiler?](docs/what-is-a-compiler.md) | The big picture: source code in, binary out |
| [How elephc works](docs/how-elephc-works.md) | The full pipeline walkthrough, step by step |
| [The Lexer](docs/the-lexer.md) | How source text becomes a stream of tokens |
| [The Parser](docs/the-parser.md) | How tokens become an AST (with Pratt parsing) |
| [The Type Checker](docs/the-type-checker.md) | Static types, inference, and error detection |
| [The Codegen](docs/the-codegen.md) | How the AST becomes ARM64 assembly |
| [The Runtime](docs/the-runtime.md) | Runtime routines: itoa, concat, hash tables, I/O |
| [Memory Model](docs/memory-model.md) | Stack, heap, concat buffer, hash tables |
| [ARM64 Assembly](docs/arm64-assembly.md) | ARM64 primer for people who've never seen assembly |
| [ARM64 Instructions](docs/arm64-instructions.md) | Quick reference for every instruction elephc uses |
| [Language Reference](docs/language-reference.md) | Complete spec: types, operators, built-ins, limits |
| [Architecture](docs/architecture.md) | Module map, file counts, conventions |

## License

MIT
