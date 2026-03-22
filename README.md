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
| `array` | `[1, 2, 3]` (indexed only) |

### Supported constructs

| Construct | Example |
|---|---|
| Echo | `echo $x;` |
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
| For / Foreach | `for (;;) { }`, `foreach ($arr as $v) { }` |
| Break / Continue | `break;`, `continue;` |
| Functions | `function foo($x) { return $x + 1; }` |
| Include/Require | `include 'file.php';`, `require_once 'lib.php';` |
| Comments | `// ...`, `/* ... */` |

### Built-in functions

**Strings:** `strlen`, `intval`, `number_format`, `substr`, `strpos`, `strrpos`, `strstr`, `str_replace`, `str_ireplace`, `substr_replace`, `strtolower`, `strtoupper`, `ucfirst`, `lcfirst`, `ucwords`, `trim`, `ltrim`, `rtrim`, `str_repeat`, `str_pad`, `strrev`, `str_split`, `strcmp`, `strcasecmp`, `str_contains`, `str_starts_with`, `str_ends_with`, `ord`, `chr`, `explode`, `implode`, `addslashes`, `stripslashes`, `nl2br`, `wordwrap`, `bin2hex`, `hex2bin`
**Arrays:** `count`, `array_push`, `array_pop`, `in_array`, `array_keys`, `array_values`, `sort`, `rsort`, `isset`
**Math:** `abs`, `floor`, `ceil`, `round`, `sqrt`, `pow`, `min`, `max`, `intdiv`, `fmod`, `fdiv`, `floatval`, `rand`, `mt_rand`, `random_int`
**Types:** `gettype`, `settype`, `empty`, `unset`, `is_int`, `is_float`, `is_string`, `is_bool`, `is_null`, `is_numeric`, `is_nan`, `is_finite`, `is_infinite`, `boolval`
**System:** `exit`, `die`

### Constants

`INF`, `NAN`, `PHP_INT_MAX`, `PHP_INT_MIN`, `PHP_FLOAT_MAX`, `M_PI`

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
- **Array** — heap-allocated indexed array (homogeneous)

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
│   │   └── system/      # exit, die
│   │
│   └── runtime/         # ARM64 runtime routines (one file per function)
│       ├── strings/     # itoa, concat, ftoa, strpos, str_replace, ...
│       ├── arrays/      # heap_alloc, array_new, array_push, sort, ...
│       └── system/      # build_argv
│
└── errors/              # Error formatting with line:col
```

## Tests

```bash
cargo test                      # all tests (~500)
cargo test test_my_feature      # run specific tests
ELEPHC_PHP_CHECK=1 cargo test   # cross-check output with PHP interpreter
```

## Documentation

- [`docs/language-reference.md`](docs/language-reference.md) — Complete spec of what elephc supports
- [`docs/architecture.md`](docs/architecture.md) — Compiler internals and ARM64 conventions

## License

MIT
