# elephc

A PHP-to-native compiler. Takes a subset of PHP and compiles it directly to ARM64 assembly, producing standalone macOS binaries. No interpreter, no VM, no runtime.

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

elephc supports a static subset of PHP:

```php
<?php
$i = 1;
while ($i <= 15) {
    if ($i % 15 == 0) {
        echo "FizzBuzz\n";
    } elseif ($i % 3 == 0) {
        echo "Fizz\n";
    } elseif ($i % 5 == 0) {
        echo "Buzz\n";
    } else {
        echo $i . "\n";
    }
    $i = $i + 1;
}
```

### Supported constructs

| Construct | Example |
|---|---|
| String literals | `"hello\n"` |
| Integer literals | `42`, `-7` |
| Variables | `$name` |
| Assignment | `$x = "hello";` |
| Echo | `echo $x;` |
| Arithmetic | `+`, `-`, `*`, `/`, `%` |
| Comparison | `==`, `!=`, `<`, `>`, `<=`, `>=` |
| Concatenation | `"a" . "b"`, `"val=" . 42` |
| If / else | `if (...) { } elseif (...) { } else { }` |
| While | `while (...) { }` |
| For | `for ($i = 0; $i < 10; $i = $i + 1) { }` |
| Break / Continue | `break;`, `continue;` |
| Comments | `// ...`, `/* ... */` |

### Not supported (by design)

`eval`, `include`/`require`, classes, closures, arrays, type juggling, dynamic dispatch, standard library functions.

## How it works

```
PHP source → Lexer → Parser (AST) → Type Checker → Codegen → as + ld → Mach-O binary
```

The compiler emits human-readable ARM64 assembly. You can inspect the `.s` file to see exactly what your PHP becomes:

```bash
elephc hello.php
cat hello.s
```

### Type system

Two types, resolved at compile time:

- **Int** — 64-bit signed integer
- **Str** — pointer + length pair

A variable's type is locked at first assignment. Reassigning to a different type is a compile error.

## Error messages

Errors include line and column numbers:

```
error[3:1]: Undefined variable: $x
error[5:7]: Type error: cannot reassign $x from Int to Str
error[1:1]: Expected '<?php' at start of file
```

## Project structure

```
src/
├── main.rs              # CLI, assembler + linker invocation
├── lib.rs               # Public module exports
├── span.rs              # Source position tracking
├── lexer/               # Source text → token stream
├── parser/              # Tokens → AST (Pratt parser)
├── types/               # Static type checking
├── codegen/             # AST → ARM64 assembly
│   ├── abi.rs           # Register conventions (load, store, write)
│   ├── runtime.rs       # Built-in routines (itoa, concat)
│   └── ...
└── errors/              # Diagnostics
```

## Tests

```bash
cargo test
```

98 tests covering lexer, parser, codegen (end-to-end), and error reporting.

## License

MIT
