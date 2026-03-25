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
│          │  mod.rs, expr.rs, stmt.rs, functions.rs, abi.rs
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
│   │   ├── strings/           strlen, substr, strpos, explode, sprintf, md5, ... (56 files)
│   │   ├── arrays/            count, array_push, sort, array_map, usort, ... (50 files)
│   │   ├── math/              abs, floor, pow, rand, fmod, fdiv, ... (12 files)
│   │   ├── types/             is_*, gettype, empty, unset, settype, ... (15 files)
│   │   ├── io/                fopen, fwrite, file_get_contents, scandir, ... (35 files)
│   │   └── system/            exit, define, time, date, mktime, json_encode, preg_match, ... (24 files)
│   │
│   └── runtime/               ARM64 runtime routines (one file per function)
│       ├── mod.rs             Emits all runtime functions into assembly
│       ├── strings/           itoa, concat, ftoa, sprintf, md5, sha1, ... (47 files)
│       ├── arrays/            heap_alloc, array_new, hash_*, sort, usort, ... (45 files)
│       ├── io/                fopen, fgets, fread, stat, scandir, ... (16 files)
│       └── system/            build_argv, time, date, mktime, strtotime, json_encode, json_decode, preg (11 files)
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
| Array result | `x0` (heap ptr) | After emit_expr for Array/AssocArray |
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

### Hash table header (heap-allocated, for associative arrays)

```
Offset  Size  Field
  0      8    count       (number of occupied entries)
  8      8    capacity    (number of slots)
 16      8    value_type  (0=int, 1=str, 2=float, 3=bool)
 24      ...  entries     (each entry is 40 bytes)
```

Each hash table entry:

```
Offset  Size  Field
  0      8    occupied   (0=empty, 1=occupied, 2=tombstone)
  8      8    key_ptr    (pointer to key string)
 16      8    key_len    (key string length)
 24      8    value_lo   (value or pointer)
 32      8    value_hi   (string length, or unused for int)
```

Uses FNV-1a hashing with linear probing for collision resolution.

### String buffer

64KB bump allocator in BSS (`_concat_buf`). Used by `itoa` and `concat` routines. Strings are never freed.

### I/O buffers

Two 4KB C-string conversion buffers (`_cstr_buf`, `_cstr_buf2`) for converting PHP strings (ptr+len) to null-terminated C strings before syscalls. Plus a 256-byte EOF flag array (`_eof_flags`) tracking end-of-file state per file descriptor.
