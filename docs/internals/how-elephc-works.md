---
title: "The Pipeline"
description: "The full journey from PHP source to a running binary."
sidebar:
  order: 2
---

This page walks through the entire compilation process — from PHP source to running binary — using a concrete example.

## The example

```php
<?php
$x = 10;
if ($x > 5) {
    echo "big\n";
}
```

Let's follow this through every phase.

## Phase 1: Lexing

**File:** `src/lexer/` — See [The Lexer](the-lexer.md) for details.

The lexer reads the source character by character and produces a sequence of tokens:

```
OpenTag          <?php
Variable("x")   $x
Assign           =
IntLiteral(10)   10
Semicolon        ;
If               if
LParen           (
Variable("x")   $x
Greater          >
IntLiteral(5)    5
RParen           )
LBrace           {
Echo             echo
StringLiteral("big\n")  "big\n"
Semicolon        ;
RBrace           }
Eof
```

Each token also carries a **Span** — its line and column number — for error reporting.

## Phase 2: Parsing

**File:** `src/parser/` — See [The Parser](the-parser.md) for details.

The parser reads the token stream and builds an **Abstract Syntax Tree** (AST):

```
Program [
    Assign {
        name: "x",
        value: IntLiteral(10)
    },
    If {
        condition: BinaryOp {
            left: Variable("x"),
            op: Gt,
            right: IntLiteral(5)
        },
        then_body: [
            Echo(StringLiteral("big\n"))
        ],
        elseif_clauses: [],
        else_body: None
    }
]
```

The tree captures the **structure** — `IntLiteral(5)` is the right operand of `Gt`, and `Echo` is inside the `then_body` of the `If`. Token details like parentheses and braces are gone — they served their purpose during parsing.

## Phase 3: Conditional compilation

**File:** `src/conditional.rs`

If the program uses elephc-only `ifdef SYMBOL { ... } else { ... }` blocks, the conditional pass evaluates them against the active CLI `--define` symbols and removes the inactive branches from the AST before any include resolution or type checking happens.

In this example, there are no `ifdef` blocks, so the AST passes through unchanged.

## Phase 4: Resolving

**File:** `src/resolver.rs`

If the program had `include` or `require` statements, the resolver would parse those files and inline their ASTs. In this example, there's nothing to resolve — the AST passes through unchanged.

## Phase 5: Name resolution

**File:** `src/name_resolver/`

After includes are flattened, elephc resolves namespace-aware names. This pass applies the current `namespace`, any `use` / `use function` / `use const` imports, and rewrites references to their canonical fully-qualified names before semantic analysis.

In this example there are no namespaces or imports, so the AST still passes through unchanged.

## Phase 6: Early optimization (constant folding)

**File:** `src/optimize.rs`

Before type checking, elephc runs a conservative AST simplification pass. This stage folds expressions whose result is already statically known without needing any type-environment information.

Typical examples include:

- `2 + 3 * 4` → `14`
- `"hello " . "world"` → `"hello world"`
- `(int)"42"` → `42`
- `2 < 3 ? 8 : 9` → `8`
- `null ?? "fallback"` → `"fallback"`

The pass is deliberately local and side-effect aware. It simplifies scalar computations, but it does not speculate across arbitrary calls or other expressions that may have runtime behavior. More precise call-side purity and `may_throw` reasoning happens later, after type checking, when elephc has enough context to build conservative effect summaries for known call targets.

In our running example there is nothing to fold yet: the pass does not currently propagate `$x = 10` into the later `$x > 5` comparison.

## Phase 7: Type checking

**File:** `src/types/` — See [The Type Checker](the-type-checker.md) for details.

The type checker walks the AST and determines the type of every variable and expression:

```
$x = 10           →  $x: Int
$x > 5            →  Int > Int → Bool  ✓
echo "big\n"      →  Str  ✓
```

It builds a **type environment** — a map from variable names to their types:

```
{ "x" → Int, "argc" → Int, "argv" → Array(Str) }
```

If you tried `$x = "hello"` after `$x = 10`, the type checker would reject it — elephc doesn't allow variables to change type (except from `null`). The checker also resolves class/interface metadata for exception handling, so `throw` only accepts objects implementing `Throwable` and each `catch` target can be matched correctly later in codegen.

On successful type checking, elephc also runs a warning pass that reports issues such as unused variables and unreachable code. On failing compilations, the parser and checker both try to recover conservatively so they can often report more than one independent error in a single run.

## Phase 8: Post-typecheck constant propagation

**File:** `src/optimize.rs`

After the checker succeeds, elephc runs a local constant-propagation pass.

This pass is still conservative, but it can already:

- forward scalar locals through straight-line code
- merge identical scalar values across simple `if` fallthrough paths
- merge identical scalar values across conservative `switch` and `try` / `catch` fallthrough paths
- infer uniform scalar outcomes from assignments using local `?:` and `match` expressions
- infer scalar locals from fixed destructuring assignments such as `[$a, $b] = [2, 3]`
- preserve unrelated scalar locals across simple loops when the loop's local writes are conservatively known, including simple nested `switch`, `try/catch/finally`, `foreach`, and other simple nested loop shapes, and keep stable scalar values introduced by `for` init clauses
- re-run folding after substitutions so expressions like `$x ** $y` can collapse to a literal

In our running example, this still does not change the program, because `$x = 10` at the statement level is not yet propagated into the later comparison shape that would let the whole `if` collapse.

## Phase 9: Post-typecheck control-flow pruning

**File:** `src/optimize.rs`

After the checker succeeds, elephc runs a second optimization pass that is allowed to prune dead control flow without hiding diagnostics from the type checker.

This pass currently handles cases such as:

- `if`, `elseif`, and ternaries with constant conditions
- `while (false)` and `for (...; false; ...)`
- constant `match` expressions and prunable `switch` prefixes
- unreachable statements after `return`, `throw`, `break`, or `continue`
- dead code after exhaustive `if` / `else` and `switch` + `default` structures
- pure expression statements and pure dead subexpressions that can be dropped safely

This pass also consults the optimizer's local effect summaries. Those summaries track known pure / non-throwing builtins, user functions, static methods, private `$this` methods, closures, and callable aliases that survive merges through `if`, `switch`, and `try` paths. That extra precision is what lets elephc prove that some `try` regions cannot actually throw and trim dead handlers safely.

This split is intentional: elephc folds obvious scalar expressions early, but waits until after type checking to remove whole blocks, so diagnostics still see the original checked structure.

In our running example there is still nothing to prune, because `$x > 5` is not yet a compile-time constant at the AST level.

## Phase 10: Dead-code elimination and structural cleanup

**File:** `src/optimize.rs`

After control-flow pruning, elephc runs a final AST cleanup pass. This pass does not try to prove new constants; instead, it simplifies the shapes left behind by earlier rewrites.

This pass currently handles cases such as:

- removing empty `if`, `switch`, `ifdef`, and degenerate `try` shells
- collapsing single-path conditionals such as `if ($a) { if ($b) { ... } }`
- materializing constant `switch` execution into the exact statement tail that would run
- hoisting safe non-throwing prefixes out of `try` blocks
- simplifying non-throwing `try` / `catch` and some non-throwing `try` / `finally` fallthrough cases

This is also where the optimizer does its final local dead-code cleanup before codegen sees the AST.

## Phase 11: Code generation

**File:** `src/codegen/` — See [The Code Generator](the-codegen.md) for details.

The code generator walks the typed AST and emits assembly for the selected target. For ordinary control flow this is mostly straight-line branches and labels; for `try` / `catch` / `finally`, the compiler additionally emits handler records and resume labels around `_setjmp` / `_longjmp`-based exception unwinding. The walkthrough below shows the AArch64 form of our simple example (simplified, with comments):

```asm
.global _main
.align 2

_main:
    ; -- prologue: set up stack frame --
    sub sp, sp, #32
    stp x29, x30, [sp, #16]
    add x29, sp, #16

    ; -- $x = 10 --
    mov x0, #10
    stur x0, [x29, #-8]

    ; -- if ($x > 5) --
    ldur x0, [x29, #-8]         ; load $x
    cmp x0, #5                   ; compare with 5
    b.le _end_if_1               ; skip body if $x <= 5

    ; -- echo "big\n" --
    adrp x1, _str_0@PAGE
    add x1, x1, _str_0@PAGEOFF
    mov x2, #4                   ; length = 4 ("big" + newline)
    mov x0, #1                   ; fd = stdout
    mov x16, #4                  ; syscall = write
    svc #0x80                    ; call kernel

_end_if_1:
    ; -- epilogue: exit(0) --
    mov x0, #0
    mov x16, #1
    svc #0x80

.data
_str_0: .ascii "big\n"
```

Key observations:

- `$x = 10` → `mov x0, #10` then `stur` to the stack at offset -8 from the frame pointer
- `if ($x > 5)` → `cmp` + `b.le` (branch if **not** greater — the condition is inverted)
- `echo "big\n"` → load string address + length, then `svc` to write to stdout
- The string literal lives in the `.data` section, referenced by label `_str_0`

## Phase 12: Runtime preparation, assembly, and linking

**Tools:** native `as` and `ld` (or the equivalent system toolchain)

elephc first prepares the shared runtime object, then writes the user assembly to a `.s` file, and finally invokes the system tools.

The runtime is not reassembled on every compile. elephc caches a pre-assembled runtime object under the user's cache directory (typically `~/.cache/elephc/`) using the compiler version, target, and heap size in the cache key. If a matching object already exists, the compile reuses it directly.

The user program still gets its own assembly file. If `--source-map` is enabled, elephc also writes a sidecar `.map` JSON file that records assembly-line to PHP-line/column mappings extracted from source markers inserted during statement emission.

In normal compile mode, the toolchain flow is:

1. Prepare or reuse the cached runtime object
2. Write the program assembly to `file.s`
3. Optionally write `file.map`
4. Assemble `file.s` into `file.o`
5. Link `file.o` together with the cached runtime object into the final executable

If `--timings` is enabled, elephc prints the duration of each major phase to stderr so you can see where time is being spent.

elephc then invokes the system tools:

On macOS, elephc drives the Apple toolchain directly:

```bash
as -arch arm64 -o file.o file.s
ld -arch arm64 -e _main -o file file.o -lSystem -syslibroot /path/to/sdk
```

On Linux, elephc invokes the native assembler/linker for the requested target.

- **`as`** (assembler) converts the user assembly text mnemonics into binary machine code, producing an object file (`.o`)
- **`ld`** (linker) resolves label addresses, links the user object together with the cached runtime object and any requested system libraries, and produces the final native executable (Mach-O on macOS, ELF on Linux)

The `.o` file is deleted after linking. The result is a standalone executable.

## Phase 12: Execution

```bash
./file
big
```

The binary runs directly on the CPU. There is no PHP interpreter or VM at runtime. The kernel loads the executable for the target platform into memory, jumps to the entry point, and the CPU executes the instructions we generated. The binary still contains elephc's emitted helper routines and links the platform's system libraries for OS/libc services.

## The complete flow

```
"<?php $x = 10; if ($x > 5) { echo \"big\\n\"; }"
                    │
                    ▼ Lexer
    [OpenTag, Variable("x"), Assign, IntLiteral(10), ...]
                    │
                    ▼ Parser
    [Assign{x, 10}, If{Gt(Var(x), 5), [Echo("big\n")]}]
                    │
                    ▼ Conditional (ifdef no-op here)
                    │
                    ▼ Resolver (no-op here)
                    │
                    ▼ NameResolver (no-op here)
                    │
                    ▼ Optimizer (fold constants, no-op here)
    [Assign{x, 10}, If{Gt(Var(x), 5), [Echo("big\n")]}]
                    │
                    ▼ Type Checker
    { x: Int } — all types consistent ✓
                    │
                    ▼ Optimizer (constant propagation, no-op here)
    [Assign{x, 10}, If{Gt(Var(x), 5), [Echo("big\n")]}]
                    │
                    ▼ Optimizer (prune dead control flow, no-op here)
    [Assign{x, 10}, If{Gt(Var(x), 5), [Echo("big\n")]}]
                    │
                    ▼ Optimizer (dead-code elimination, no-op here)
    [Assign{x, 10}, If{Gt(Var(x), 5), [Echo("big\n")]}]
                    │
                    ▼ Code Generator
    "sub sp, sp, #32 / stp x29, x30, ... / mov x0, #10 / ..."
                    │
                    ▼ Runtime Cache
    ~/.cache/elephc/runtime-v<version>-<target>-heap<size>.o
                    │
                    ▼ optional Source Map
    file.map (asm line → PHP line/col)
                    │
                    ▼ as (assembler)
    file.o (machine code bytes for user program)
                    │
                    ▼ ld (linker)
    file (user object + cached runtime object)
                    │
                    ▼ CPU
    "big\n"
```

---

Next: [The Lexer →](the-lexer.md)
