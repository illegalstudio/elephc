---
website: https://github.com/example/elephc
tags:
  - "#rust"
  - "#compiler"
  - "#php"
  - "#assembly"
  - "#x86_64"
  - "#systems-programming"
---

# elephc — A PHP-to-Native Compiler

## 1. Introduction

**elephc** is a minimalist [[PHP]] compiler written in [[Rust]] that translates a static subset of PHP into [[x86_64]] [[assembly]], producing native [[Linux]] [[ELF]] binaries — no interpreter, no VM, no runtime.

The goal is not to replace the PHP interpreter. The goal is to prove that PHP syntax can serve as a valid surface language for systems-level compilation, and to give PHP developers a path to producing standalone CLI binaries from familiar code.

The compiler pipeline is straightforward: PHP source → Lexer → Parser (AST) → Code Generator → [[NASM]] Assembly → Native Binary.


## 2. Why

PHP powers nearly 80% of the web, yet it remains one of the few major languages with no practical path to native binary compilation. Developers who think in PHP are locked into the interpreter ecosystem — they can't ship a single binary, can't write CLI tools that run without a runtime, can't participate in the systems programming world using their primary language.

Existing approaches all add an intermediate layer: [[KPHP]] transpiles to [[C++]], [[PeachPie]] targets [[.NET]], experimental projects route through [[LLVM]]. None of them generate assembly directly. None of them are written in Rust.

elephc fills this gap with a deliberately constrained approach:

- **No dynamic features.** No `eval()`, no variable variables, no magic methods. If it can't be resolved at compile time, it doesn't exist.
- **Direct assembly output.** The compiler emits human-readable [[NASM]] [[Intel syntax]] assembly. You can inspect every instruction your PHP code becomes.
- **Rust implementation.** Memory safety, strong typing, and excellent tooling for building compilers — pattern matching, algebraic types, zero-cost abstractions.
- **Educational value.** Every module is small, focused, and documented. elephc is designed to be read and understood, not just used.


## 3. Specification

### 3.1 Source Language (PHP Subset)

The initial version supports the following constructs:

| Construct | Example | Notes |
|---|---|---|
| Open tag | `<?php` | Required at file start |
| String literals | `"hello\n"` | Double-quoted, with escape sequences |
| Integer literals | `42`, `-7` | 64-bit signed integers |
| Variables | `$name` | Statically typed at first assignment |
| Assignment | `$x = "hello";` | Binds a value to a variable |
| Echo | `echo $x;` | Prints to stdout |
| Arithmetic | `$x + $y` | `+`, `-`, `*`, `/` on integers |
| Concatenation | `"a" . "b"` | Dot operator on strings |
| Comments | `// ...` and `/* ... */` | Ignored by lexer |

**Explicitly unsupported** (by design): `eval`, `include`/`require`, classes, closures, arrays, type juggling, dynamic dispatch, standard library functions.

### 3.2 Target Platform

| Property | Value |
|---|---|
| Architecture | [[x86_64]] |
| OS | [[Linux]] (syscall ABI) |
| Assembly syntax | [[Intel syntax]] ([[NASM]]) |
| Binary format | [[ELF]] 64-bit |
| Syscalls used | `sys_write` (1), `sys_exit` (60) |

### 3.3 Type System

elephc uses a minimal static type system resolved entirely at compile time:

- **Int** — 64-bit signed integer, stored in registers or on the stack.
- **Str** — pointer + length pair, referencing data in the `.data` section or a runtime buffer.

A variable's type is locked at its first assignment. Reassigning to a different type is a compile error.

### 3.4 Compilation Pipeline

```
┌──────────┐     ┌──────────┐     ┌──────────┐     ┌──────────┐     ┌──────────┐
│  Source   │────▶│  Lexer   │────▶│  Parser  │────▶│ Codegen  │────▶│   NASM   │
│  (.php)   │     │ (tokens) │     │  (AST)   │     │  (.asm)  │     │  + ld    │
└──────────┘     └──────────┘     └──────────┘     └──────────┘     └──────────┘
                                                                      │
                                                                      ▼
                                                                ┌──────────┐
                                                                │  Binary  │
                                                                │  (ELF)   │
                                                                └──────────┘
```


## 4. Project Structure

The project follows a strict one-responsibility-per-file philosophy. Every module is small, focused, and independently testable.

```
elephc/
├── Cargo.toml                  # Project manifest
├── README.md                   # Usage and examples
│
├── src/
│   ├── main.rs                 # CLI entry point, argument parsing
│   │
│   ├── lexer/
│   │   ├── mod.rs              # Lexer public API
│   │   ├── token.rs            # Token enum definition
│   │   ├── cursor.rs           # Character-by-character source reader
│   │   └── scan.rs             # Scanning logic (string, number, keyword)
│   │
│   ├── parser/
│   │   ├── mod.rs              # Parser public API
│   │   ├── ast.rs              # AST node definitions (Expr, Stmt)
│   │   ├── expr.rs             # Expression parsing (literals, binops)
│   │   └── stmt.rs             # Statement parsing (echo, assign)
│   │
│   ├── codegen/
│   │   ├── mod.rs              # Codegen public API
│   │   ├── context.rs          # Compilation state (variables, labels)
│   │   ├── emit.rs             # Assembly instruction emitter
│   │   ├── expr.rs             # Expression code generation
│   │   ├── stmt.rs             # Statement code generation
│   │   ├── data_section.rs     # .data section builder (string literals)
│   │   └── runtime.rs          # Built-in routines (itoa, string concat)
│   │
│   ├── types/
│   │   ├── mod.rs              # Type system public API
│   │   └── checker.rs          # Static type resolution and checking
│   │
│   └── errors/
│       ├── mod.rs              # Error types public API
│       └── report.rs           # Human-readable error formatting
│
├── tests/
│   ├── lexer_tests.rs          # Tokenization tests
│   ├── parser_tests.rs         # AST construction tests
│   ├── codegen_tests.rs        # Assembly output tests
│   └── integration/
│       ├── hello.php           # echo "Hello, World!\n";
│       ├── variables.php       # Variable assignment and echo
│       ├── arithmetic.php      # Integer arithmetic
│       └── concat.php          # String concatenation
│
└── examples/
    ├── hello.php               # Minimal example
    └── fizzbuzz.php            # Goal program for v0.2
```

### Module Responsibilities

| Module | Files | Responsibility |
|---|---|---|
| `lexer/` | 4 | Transforms source text into a flat token stream |
| `parser/` | 4 | Transforms tokens into an abstract syntax tree |
| `codegen/` | 7 | Transforms AST into NASM x86_64 assembly |
| `types/` | 2 | Resolves and validates variable types |
| `errors/` | 2 | Collects and formats all compiler diagnostics |

Each module exposes a single public function through its `mod.rs` — the rest is internal. This keeps the dependency graph clean and forces each stage to communicate through well-defined data structures (tokens, AST nodes, typed AST nodes).


## 5. Timeline

### Phase 0 — Scaffolding (Week 1)

Set up the Cargo project, define all data types (`Token`, `Expr`, `Stmt`), wire up the CLI. No logic yet — just the skeleton that compiles and runs.

**Deliverable:** `cargo run -- hello.php` exits without crashing.

### Phase 1 — Echo Strings (Week 2)

Implement the full pipeline for the simplest possible case: `echo "Hello, World!\n";`. The lexer tokenizes, the parser builds an AST, the codegen emits NASM assembly, and a shell script assembles + links the binary.

**Deliverable:** `./hello` prints "Hello, World!" to stdout.

### Phase 2 — Variables and Integers (Week 3)

Add variable assignment, integer literals, and echo for both types. Implement the type checker to lock variable types at first assignment. Add the `itoa` runtime routine for printing integers.

**Deliverable:** `$x = 42; echo $x;` compiles and prints "42".

### Phase 3 — Expressions (Week 4)

Add arithmetic operators (`+`, `-`, `*`, `/`) for integers and the concatenation operator (`.`) for strings. Implement operator precedence in the parser. Add a runtime buffer for concatenation results.

**Deliverable:** `echo "Result: " . ($a + $b);` compiles and runs correctly.

### Phase 4 — Polish and Test (Week 5)

Write comprehensive tests for all modules. Add meaningful error messages with line numbers. Write the README with usage examples. Tag v0.1.0.

**Deliverable:** `elephc v0.1.0` released on GitHub with passing test suite.

### Future (v0.2+)

- `if` / `else` / `elseif`
- `while` and `for` loops
- Function declarations and calls
- Multiple file compilation
- [[macOS]] / [[ARM64]] targets
- Basic optimizations (constant folding, dead code elimination)
