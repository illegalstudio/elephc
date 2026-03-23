# elephc

A PHP-to-native compiler. Takes a subset of PHP and compiles it directly to ARM64 assembly, producing standalone macOS binaries. No interpreter, no VM, no runtime dependencies.

## Learn how a compiler works

elephc is designed to be read. Every line of ARM64 assembly emitted by the compiler is annotated with an inline comment explaining what it does and why — from stack frame setup to syscall invocation, from integer-to-string conversion to array memory layout. If you've ever wondered what happens between `echo "hello"` and the CPU executing it, follow the code from `src/codegen/` and read the comments. No prior assembly knowledge required.

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

elephc supports a growing subset of PHP. Every program it compiles is also valid PHP and produces the same output when run with `php`.

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
| `array` | `[1, 2, 3]`, `["key" => "value"]` (indexed and associative) |

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
| Include/Require | `include 'file.php';`, `require_once 'lib.php';` |
| String interpolation | `"Hello $name"` |
| Comments | `// ...`, `/* ... */` |

### Built-in functions

**Strings:** `strlen`, `intval`, `number_format`, `substr`, `strpos`, `strrpos`, `strstr`, `str_replace`, `str_ireplace`, `substr_replace`, `strtolower`, `strtoupper`, `ucfirst`, `lcfirst`, `ucwords`, `trim`, `ltrim`, `rtrim`, `str_repeat`, `str_pad`, `strrev`, `str_split`, `strcmp`, `strcasecmp`, `str_contains`, `str_starts_with`, `str_ends_with`, `ord`, `chr`, `explode`, `implode`, `addslashes`, `stripslashes`, `nl2br`, `wordwrap`, `bin2hex`, `hex2bin`, `sprintf`, `printf`, `sscanf`, `md5`, `sha1`, `hash`, `htmlspecialchars`, `htmlentities`, `html_entity_decode`, `urlencode`, `urldecode`, `rawurlencode`, `rawurldecode`, `base64_encode`, `base64_decode`, `ctype_alpha`, `ctype_digit`, `ctype_alnum`, `ctype_space`
**Arrays:** `count`, `array_push`, `array_pop`, `in_array`, `array_keys`, `array_values`, `sort`, `rsort`, `isset`, `array_key_exists`, `array_search`, `array_merge`, `array_slice`, `array_splice`, `array_combine`, `array_flip`, `array_reverse`, `array_unique`, `array_sum`, `array_product`, `array_chunk`, `array_pad`, `array_fill`, `array_fill_keys`, `array_diff`, `array_intersect`, `array_diff_key`, `array_intersect_key`, `array_unshift`, `array_shift`, `asort`, `arsort`, `ksort`, `krsort`, `natsort`, `natcasesort`, `shuffle`, `array_rand`, `range`
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
cargo test                      # all tests (~691)
cargo test test_my_feature      # run specific tests
ELEPHC_PHP_CHECK=1 cargo test   # cross-check output with PHP interpreter
```

## Documentation

- [`docs/language-reference.md`](docs/language-reference.md) — Complete spec of what elephc supports
- [`docs/architecture.md`](docs/architecture.md) — Compiler internals and ARM64 conventions

## License

MIT
