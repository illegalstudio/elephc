# Architecture

## Compilation pipeline

```
PHP source (.php)
    │
    ▼
┌─────────┐
│  Lexer   │  src/lexer/
│          │  scan.rs, literals.rs, cursor.rs, token.rs
│          │  Source text → Vec<(Token, Span)>
└────┬─────┘
     │
     ▼
┌─────────┐
│  Parser  │  src/parser/
│          │  expr.rs (Pratt parser), stmt.rs, control.rs, ast.rs
│          │  Tokens → Program (Vec<Stmt>)
└────┬─────┘
     │
     ▼
┌─────────┐
│ Resolver │  src/resolver.rs
│          │  Resolves include/require by inlining referenced files.
│          │  Recursively parses and merges included ASTs.
└────┬─────┘
     │
     ▼
┌─────────┐
│  Type    │  src/types/
│  Checker │  checker/mod.rs, builtins.rs, functions.rs
│          │  Validates types, returns CheckResult (TypeEnv + FunctionSig map)
└────┬─────┘
     │
     ▼
┌─────────┐
│ Codegen  │  src/codegen/
│          │  mod.rs, expr.rs, stmt.rs, builtins.rs, functions.rs
│          │  AST → ARM64 assembly string (.s file)
└────┬─────┘
     │
     ▼
┌─────────┐
│ as + ld  │  System assembler and linker
│          │  .s → .o → Mach-O binary
└─────────┘
```

## Module map

```
src/
├── main.rs                    CLI entry point
├── lib.rs                     Public module exports
├── span.rs                    Source position (line, col)
├── resolver.rs                Include/require file resolution
│
├── lexer/
│   ├── mod.rs                 tokenize() → Vec<(Token, Span)>
│   ├── token.rs               Token enum
│   ├── cursor.rs              Byte-level source reader
│   ├── scan.rs                Main scanning loop, operators
│   └── literals.rs            String, number, variable, keyword scanning
│
├── parser/
│   ├── mod.rs                 parse() → Program
│   ├── ast.rs                 ExprKind, StmtKind, BinOp, CastType
│   ├── expr.rs                Pratt parser for expressions
│   ├── stmt.rs                Statement parsing, assignment, functions
│   └── control.rs             if, while, for, do-while, foreach
│
├── types/
│   ├── mod.rs                 PhpType enum, TypeEnv, FunctionSig, CheckResult
│   └── checker/
│       ├── mod.rs             check_stmt(), infer_type()
│       ├── builtins.rs        Built-in function type signatures
│       └── functions.rs       User function type inference
│
├── codegen/
│   ├── mod.rs                 generate() orchestration
│   ├── expr.rs                Expression codegen
│   ├── stmt.rs                Statement codegen
│   ├── functions.rs           User function emission
│   ├── abi.rs                 ARM64 register conventions
│   ├── context.rs             Variables, labels, loop stack
│   ├── data_section.rs        String/float literal .data section
│   ├── emit.rs                Assembly text buffer
│   │
│   ├── builtins/              Built-in function codegen (one file per function)
│   │   ├── mod.rs             Dispatcher — chains to category modules
│   │   ├── strings/           strlen, substr, strpos, explode, implode, ... (37 files)
│   │   ├── arrays/            count, array_push, array_pop, sort, ... (9 files)
│   │   ├── math/              abs, floor, pow, rand, fmod, fdiv, ... (13 files)
│   │   ├── types/             is_*, gettype, empty, unset, settype, ... (15 files)
│   │   └── system/            exit, die (1 file)
│   │
│   └── runtime/               ARM64 runtime routines (one file per function)
│       ├── mod.rs             Emits all runtime functions into assembly
│       ├── strings/           itoa, concat, ftoa, strpos, str_replace, ... (35 files)
│       ├── arrays/            heap_alloc, array_new, push_int/str, sort (5 files)
│       └── system/            build_argv (1 file)
│
└── errors/
    ├── mod.rs                 CompileError, error trait
    └── report.rs              Error formatting
```

## ARM64 calling conventions

| What | Register | Notes |
|---|---|---|
| Integer result | `x0` | After emit_expr for Int/Bool |
| Float result | `d0` | After emit_expr for Float |
| String result | `x1` (ptr), `x2` (len) | After emit_expr for Str |
| Array result | `x0` (heap ptr) | After emit_expr for Array |
| Function args (int) | `x0`-`x7` | Int/Bool/Array = 1 reg, Str = 2 regs |
| Function args (float) | `d0`-`d7` | Separate index from int regs |
| Frame pointer | `x29` | Saved in prologue |
| Link register | `x30` | Saved in prologue |
| Stack locals | `[x29, #-offset]` | Negative offsets from frame pointer |
| Null sentinel | `0x7FFFFFFFFFFFFFFE` | Distinguished from real integers |

## Runtime memory layout

### Array header (heap-allocated)

```
Offset  Size  Field
  0      8    length    (current number of elements)
  8      8    capacity  (allocated slots)
 16      8    elem_size (8 for Int, 16 for Str)
 24      ...  elements  (contiguous)
```

### Heap allocator

1MB bump allocator in BSS (`_heap_buf`). No free, no GC. Simple offset bump via `_heap_off`.

### String buffer

64KB bump allocator in BSS (`_concat_buf`). Used by `itoa` and `concat` routines. Strings are never freed.
