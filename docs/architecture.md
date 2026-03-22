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
│
├── lexer/
│   ├── mod.rs                 tokenize() → Vec<(Token, Span)>
│   ├── token.rs               Token enum (78 lines)
│   ├── cursor.rs              Byte-level source reader (71 lines)
│   ├── scan.rs                Main scanning loop, operators (166 lines)
│   └── literals.rs            String, integer, variable, keyword scanning (127 lines)
│
├── parser/
│   ├── mod.rs                 parse() → Program
│   ├── ast.rs                 ExprKind, StmtKind, BinOp, Span (190 lines)
│   ├── expr.rs                Pratt parser for expressions (249 lines)
│   ├── stmt.rs                Statement parsing, assignment, functions (292 lines)
│   └── control.rs             if, while, for, do-while, foreach (248 lines)
│
├── types/
│   ├── mod.rs                 PhpType enum, TypeEnv, FunctionSig, CheckResult
│   └── checker/
│       ├── mod.rs             check_stmt(), infer_type() (269 lines)
│       ├── builtins.rs        Built-in function type signatures (151 lines)
│       └── functions.rs       User function type inference (125 lines)
│
├── codegen/
│   ├── mod.rs                 generate() orchestration (108 lines)
│   ├── expr.rs                Expression codegen (373 lines)
│   ├── stmt.rs                Statement codegen (344 lines)
│   ├── builtins.rs            Built-in function codegen (191 lines)
│   ├── functions.rs           User function emission (155 lines)
│   ├── abi.rs                 ARM64 register conventions (60 lines)
│   ├── context.rs             Variables, labels, loop stack (54 lines)
│   ├── data_section.rs        String literal .data section (54 lines)
│   ├── emit.rs                Assembly text buffer (38 lines)
│   └── runtime/
│       ├── mod.rs             Runtime orchestration (29 lines)
│       ├── strings.rs         itoa, concat, atoi (160 lines)
│       ├── arrays.rs          heap_alloc, array_new, push, sort (122 lines)
│       └── system.rs          build_argv (67 lines)
│
└── errors/
    ├── mod.rs                 CompileError, Span-based errors (33 lines)
    └── report.rs              Error formatting (12 lines)
```

## ARM64 calling conventions

| What | Register | Notes |
|---|---|---|
| Integer result | `x0` | After emit_expr for Int |
| String result | `x1` (ptr), `x2` (len) | After emit_expr for Str |
| Array result | `x0` (heap ptr) | After emit_expr for Array |
| Function args | `x0`-`x7` | Int/Array = 1 reg, Str = 2 regs |
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
