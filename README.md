# elephc

A PHP-to-native compiler. Takes a subset of PHP and compiles it directly to **ARM64 assembly**, producing **standalone macOS binaries**. No interpreter, no VM, no runtime dependencies.

## Why

My first "serious programming" book was *PHP 4 and MySQL*. After years of experimenting with code, that book turned my passion into a profession. I've worked with many languages over the past 20 years, but PHP is the one that has most consistently put food on the table.

One thing I always missed about PHP was the ability to produce optimized, fast native binaries. With the advent of AI, we can build ambitious things quickly. While everyone else is busy building the next Facebook, chasing ephemeral wealth that will never come, I thought I could try to fill that gap and write a compiler for PHP.

It's not perfect — it's 99% written by Claude — but **it works**. It's a solid starting point, and more importantly, it's a great way to understand **how a compiler works** and how assembly language operates under the hood.

I made the project as modular as possible. Every function has its own codegen file, and each one is **commented line by line**, so you can see exactly how a high-level construct gets translated into its low-level equivalent.

### What you should not expect

Don't expect to take any existing PHP project and magically compile it. There are no classes here, no Composer. We're roughly at the level of that famous *PHP 4* book where my journey began.

### What you can expect

You can write a PHP file using only the constructs documented in this project's [language reference](docs/language-reference.md). You can include other files with `include`, `require`, `include_once`, and `require_once`, and watch your code run at the speed of light after running:

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

```bash
git clone https://github.com/example/elephc.git
cd elephc
cargo build --release
```

The binary is at `./target/release/elephc`.

## Usage

```bash
# Compile a PHP file to a native binary
elephc hello.php
./hello
```

Or via cargo:

```bash
cargo run -- hello.php
./hello
```

## What it compiles

elephc supports a growing subset of PHP. **Every program it compiles is also valid PHP** and produces the same output when run with `php`.

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

### Supported constructs

| Construct | Example |
|---|---|
| Echo / Print | `echo $x;`, `print $x;` |
| Variables | `$name = "hello";` |
| Arithmetic | `+`, `-`, `*`, `/`, `%`, `**` |
| Comparison | `==`, `!=`, `<`, `>`, `<=`, `>=`, `===`, `!==` |
| Logical | `&&`, `\|\|`, `!` |
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
| Functions | `function foo($x) { return $x + 1; }` |
| Closures / Arrow | `$fn = function($x) { return $x * 2; };`, `fn($x) => $x * 2` |
| Include/Require | `include 'file.php';`, `require_once 'lib.php';` |
| String interpolation | `"Hello $name"` |
| Comments | `// ...`, `/* ... */` |

### Built-in functions

**Strings:** `strlen`, `intval`, `number_format`, `substr`, `strpos`, `strrpos`, `strstr`, `str_replace`, `str_ireplace`, `substr_replace`, `strtolower`, `strtoupper`, `ucfirst`, `lcfirst`, `ucwords`, `trim`, `ltrim`, `rtrim`, `str_repeat`, `str_pad`, `strrev`, `str_split`, `strcmp`, `strcasecmp`, `str_contains`, `str_starts_with`, `str_ends_with`, `ord`, `chr`, `explode`, `implode`, `addslashes`, `stripslashes`, `nl2br`, `wordwrap`, `bin2hex`, `hex2bin`, `sprintf`, `printf`, `sscanf`, `md5`, `sha1`, `hash`, `htmlspecialchars`, `htmlentities`, `html_entity_decode`, `urlencode`, `urldecode`, `rawurlencode`, `rawurldecode`, `base64_encode`, `base64_decode`, `ctype_alpha`, `ctype_digit`, `ctype_alnum`, `ctype_space`
**Arrays:** `count`, `array_push`, `array_pop`, `in_array`, `array_keys`, `array_values`, `sort`, `rsort`, `isset`, `array_key_exists`, `array_search`, `array_merge`, `array_slice`, `array_splice`, `array_combine`, `array_flip`, `array_reverse`, `array_unique`, `array_sum`, `array_product`, `array_chunk`, `array_pad`, `array_fill`, `array_fill_keys`, `array_diff`, `array_intersect`, `array_diff_key`, `array_intersect_key`, `array_unshift`, `array_shift`, `asort`, `arsort`, `ksort`, `krsort`, `natsort`, `natcasesort`, `shuffle`, `array_rand`, `array_column`, `range`, `array_map`, `array_filter`, `array_reduce`, `array_walk`, `usort`, `uksort`, `uasort`, `call_user_func`, `function_exists`
**Math:** `abs`, `floor`, `ceil`, `round`, `sqrt`, `pow`, `min`, `max`, `intdiv`, `fmod`, `fdiv`, `rand`, `mt_rand`, `random_int`
**Types:** `gettype`, `settype`, `empty`, `unset`, `is_int`, `is_float`, `is_string`, `is_bool`, `is_null`, `is_numeric`, `is_nan`, `is_finite`, `is_infinite`, `boolval`, `floatval`
**I/O:** `fopen`, `fclose`, `fread`, `fwrite`, `fgets`, `feof`, `readline`, `fseek`, `ftell`, `rewind`, `file_get_contents`, `file_put_contents`, `file`, `fgetcsv`, `fputcsv`, `file_exists`, `is_file`, `is_dir`, `is_readable`, `is_writable`, `filesize`, `filemtime`, `copy`, `rename`, `unlink`, `mkdir`, `rmdir`, `scandir`, `glob`, `getcwd`, `chdir`, `tempnam`, `sys_get_temp_dir`
**Debugging:** `var_dump`, `print_r`
**System:** `exit`, `die`

### Constants

`INF`, `NAN`, `PHP_INT_MAX`, `PHP_INT_MIN`, `PHP_FLOAT_MAX`, `M_PI`, `STDIN`, `STDOUT`, `STDERR`

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

Six types, resolved at compile time:

- **Int** — 64-bit signed integer
- **Float** — 64-bit double-precision
- **Str** — pointer + length pair
- **Bool** — `true`/`false`, coerces to 0/1
- **Null** — sentinel value, coerces to 0/""
- **Array** — heap-allocated indexed or associative array (hash table for string keys)

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
│   └── checker/
│       ├── mod.rs       # check_stmt(), infer_type()
│       ├── builtins.rs  # Built-in function type signatures
│       └── functions.rs # User function type inference
│
├── codegen/             # AST → ARM64 assembly
│   ├── expr.rs          # Expression codegen
│   ├── stmt.rs          # Statement codegen
│   ├── abi.rs           # Register conventions (load, store, write)
│   ├── functions.rs     # User function emission
│   ├── context.rs       # Variables, labels, loop stack
│   ├── data_section.rs  # String/float literal .data section
│   ├── emit.rs          # Assembly text buffer
│   │
│   ├── builtins/        # Built-in function codegen (one file per function)
│   │   ├── strings/     # strlen, substr, strpos, explode, implode, ...
│   │   ├── arrays/      # count, array_push, array_pop, sort, ...
│   │   ├── math/        # abs, floor, pow, rand, fmod, ...
│   │   ├── types/       # is_int, gettype, empty, unset, settype, ...
│   │   ├── io/          # fopen, fclose, fread, fwrite, fgets, file_get_contents, ...
│   │   └── system/      # exit, die
│   │
│   └── runtime/         # ARM64 runtime routines (one file per function)
│       ├── strings/     # itoa, concat, ftoa, strpos, str_replace, ...
│       ├── arrays/      # heap_alloc, array_new, array_push, sort, ...
│       ├── io/          # fopen, fclose, fread, fwrite, file_ops, ...
│       └── system/      # build_argv
│
└── errors/              # Error formatting with line:col
```

## Tests

```bash
cargo test                      # all tests (~797)
cargo test test_my_feature      # run specific tests
ELEPHC_PHP_CHECK=1 cargo test   # cross-check output with PHP interpreter
```

## Documentation

The `docs/` directory is a **complete wiki** covering every aspect of the compiler — from what a compiler is, to how each phase works, to the ARM64 instruction set. If you're new to compilers or assembly, **start from the top and work your way down**.

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
