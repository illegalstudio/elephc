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
│   ├── context.rs             Variables, labels, loop stack, ownership lattice
│   ├── data_section.rs        String/float literal .data section
│   ├── emit.rs                Assembly text buffer
│   │
│   ├── builtins/              Built-in function codegen (one file per language function)
│   │   ├── mod.rs             Dispatcher — chains to category modules
│   │   ├── strings/           strlen, substr, strpos, explode, sprintf, md5, ... (57 files)
│   │   ├── arrays/            count, array_push, sort, array_map, usort, ... (51 files)
│   │   ├── math/              abs, floor, pow, rand, fmod, fdiv, round, min, max, sin, cos, ... (32 files)
│   │   ├── types/             is_*, gettype, empty, unset, settype, ... (16 files)
│   │   ├── io/                fopen, fwrite, file_get_contents, scandir, ... (36 files)
│   │   ├── pointers/          ptr, ptr_get, ptr_set, ptr_read8, ptr_write8, ptr_offset, ... (12 files)
│   │   └── system/            exit, define, time, date, mktime, json_encode, preg_match, ... (25 files)
│   │
│   └── runtime/               ARM64 runtime routines (one file per language/runtime helper)
│       ├── mod.rs             Emits all runtime functions into assembly
│       ├── strings/           itoa, concat, ftoa, sprintf, md5, sha1, str_persist, ... (53 files)
│       ├── arrays/            heap_alloc, heap_free, array_free_deep, array_grow, hash_grow, hash_*, sort, usort, refcount, ... (56 files)
│       ├── io/                fopen, fgets, fread, stat, scandir, ... (17 files)
│       ├── system/            build_argv, time, getenv, shell_exec, date, mktime, strtotime, json_encode, json_decode, preg (14 files)
│       └── pointers/          ptoa, ptr_check_nonnull, str_to_cstr, cstr_to_str, ... (5 files)
│
│
└── errors/
    ├── mod.rs                 CompileError, error trait
    └── report.rs              Error formatting
```

## ARM64 calling conventions

| What | Register | Notes |
|---|---|---|
| Integer result | `x0` | After emit_expr for Int/Bool/Void |
| Float result | `d0` | After emit_expr for Float |
| String result | `x1` (ptr), `x2` (len) | After emit_expr for Str |
| Array result | `x0` (heap ptr) | After emit_expr for Array/AssocArray |
| Object result | `x0` (heap ptr) | After emit_expr for Object |
| Pointer / Callable result | `x0` | Raw address or function pointer |
| Function args (int) | `x0`-`x7` | Int/Bool/Array/Object/Pointer/Callable = 1 reg, Str = 2 regs |
| Function args (float) | `d0`-`d7` | Separate index from int regs |
| Frame pointer | `x29` | Saved in prologue |
| Link register | `x30` | Saved in prologue |
| Stack locals | `[x29, #-offset]` | Negative offsets from frame pointer |
| Null sentinel | `0x7FFFFFFFFFFFFFFE` | Distinguished from real integers |

## FFI pipeline

FFI declarations are parsed into dedicated AST nodes:

- `StmtKind::ExternFunctionDecl`
- `StmtKind::ExternClassDecl`
- `StmtKind::ExternGlobalDecl`

During type checking, extern declarations are registered in dedicated maps that are carried into codegen:

- `extern_functions`: extern signatures exposed through the C ABI
- `extern_classes`: flat C struct layout metadata
- `extern_globals`: native global symbols loaded through the linker

Extern calls differ from ordinary elephc function calls in four important ways:

1. Codegen dispatches extern functions before built-ins, so an `extern function strlen(...)` declaration really calls C `strlen`, not the elephc builtin.
2. `string` arguments are converted with `__rt_str_to_cstr`, which allocates an owned null-terminated copy on the elephc heap before calling C.
3. `string` return values are converted with `__rt_cstr_to_str`, which copies bytes back into an owned elephc string.
4. `extern class` layouts are available to pointer-oriented codegen too, so `ptr_sizeof("StructName")` and `ptr_cast<StructName>($p)->field` use the same checked layout metadata recorded by the type checker.

`callable` FFI parameters pass a user-defined elephc function by address. The function name is provided as a string literal at the call site, and codegen loads the address of the compiled `_fn_<name>` symbol before branching into C.

## Runtime memory layout

### Array header (heap-allocated)

```
Offset  Size  Field
  0      8    length    (current number of elements)
  8      8    capacity  (allocated slots)
 16      8    elem_size (8 for Int, 16 for Str)
 24      ...  elements  (contiguous)
```

### Runtime BSS and data symbols

The runtime reserves a fixed set of global symbols in `emit_runtime_data()`:

| Symbol group | Symbols | Purpose |
|---|---|---|
| String scratch | `_concat_buf`, `_concat_off` | Temporary string results for expression evaluation |
| CLI globals | `_global_argc`, `_global_argv` | Saved OS argument state used to build `$argv` |
| Heap allocator | `_heap_buf`, `_heap_off`, `_heap_free_list`, `_heap_max` | Heap storage plus allocator metadata |
| Runtime diagnostics | `_heap_err_msg`, `_arr_cap_err_msg`, `_ptr_null_err_msg` | Fatal error messages for heap, array, and pointer failures |
| GC statistics | `_gc_allocs`, `_gc_frees`, `_gc_peak` | Allocation/free counters emitted for runtime tracking |
| I/O scratch | `_cstr_buf`, `_cstr_buf2`, `_eof_flags` | Syscall-oriented C-string scratch buffers and EOF bookkeeping |
| String/regex tables | `_fmt_g`, `_b64_encode_tbl`, `_b64_decode_tbl`, `_pcre_*` | Formatting and lookup tables for runtime helpers |
| JSON/date tables | `_json_true`, `_json_false`, `_json_null`, `_day_names`, `_month_names` | Static data used by JSON and date routines |

### Heap allocator

8MB free-list + bump hybrid allocator in BSS (`_heap_buf`). Each allocation has an 8-byte header: `[size:4][refcount:4]` — a 32-bit block size followed by a 32-bit reference count. When memory is freed (via `__rt_heap_free`), blocks are inserted into an address-ordered free list, adjacent blocks are coalesced, and any free chain that reaches the current bump tail is folded back into `_heap_off`. New allocations check the free list first (first-fit); oversized free blocks are split so the remainder stays reusable, and bump allocation is used only when no suitable free block exists. Reference counting (`__rt_incref`, `__rt_decref_array`, `__rt_decref_hash`, `__rt_decref_object`) automatically frees heap objects when their reference count reaches zero. With `--heap-debug`, the runtime also validates the ordered free list on allocator/free mutations and traps on double free or zero-refcount `incref`/`decref` paths. Codegen now records a local ownership lattice (`Owned`, `Borrowed`, `MaybeOwned`, `NonHeap`) plus an `epilogue_cleanup_safe` bit in `Context::variables` so it can distinguish between stack slots that truly own a heap value and slots that merely alias global/static/container-backed storage. Ownership transfer points currently include ordinary reassignments, by-value call arguments, borrowed heap returns, indexed array writes, associative-array/hash writes, object property writes, `static` slot writes, `global` loads, `foreach` targets, and `list(...)` targets, while container-copy builtins now dispatch to dedicated `_refcounted` runtime helpers for nested array/hash/object/string payloads (`array` literals with spreads, `array_merge`, `array_chunk`, `array_slice`, `array_reverse`, `array_pad`, `array_unique`, `array_splice`, `array_diff`, `array_intersect`, `array_filter`, `array_fill`, `array_combine`, `array_fill_keys`). Function epilogues now clean up only locals that are both `Owned` and still marked safe; borrowed aliases such as `$this`, ref params, globals, and statics are explicitly excluded, and broader alias-driven cleanup remains conservative until more control-flow cases are proven. Configurable via `--heap-size=BYTES` (minimum 64KB) and `--heap-debug` for runtime verification. Bounds-checked with fatal error on overflow.

### Hash table header (heap-allocated, for associative arrays)

```
Offset  Size  Field
  0      8    count       (number of occupied entries)
  8      8    capacity    (number of slots)
 16      8    value_type  (0=int, 1=str, 2=float, 3=bool, 4=array, 5=assoc, 6=object)
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

### Object layout (heap-allocated)

```
Offset  Size  Field
  0      8    class_id  (identifies which class this object belongs to)
  8     16    prop[0]   (first property — 16 bytes regardless of type)
 24     16    prop[1]   (second property)
 ...    ...   ...
```

Total size: `8 + (num_properties × 16)`. Properties are stored at fixed offsets determined at compile time. Property access is `base + 8 + (property_index × 16)`.

### Method dispatch

- Instance methods: `bl _method_ClassName_methodName`. The object pointer is passed as the first argument in `x0` (as `$this`).
- Static methods: `bl _static_ClassName_methodName`. No object pointer is passed.

### String buffer

64KB scratch buffer in BSS (`_concat_buf`). Used by `itoa`, `concat`, `strtolower`, and all string-producing runtime routines. Reset to offset 0 at the start of each statement. Strings that need to persist beyond the current statement are copied to the heap via `__rt_str_persist`.

### I/O buffers

Two 4KB C-string conversion buffers (`_cstr_buf`, `_cstr_buf2`) are still used by low-level I/O helpers and syscalls. FFI string calls do not use these scratch buffers anymore; they allocate owned C strings through `__rt_str_to_cstr` so multiple string arguments remain valid across the same native call. A 256-byte EOF flag array (`_eof_flags`) tracks end-of-file state per file descriptor.
