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
│  Checker │  traits.rs, checker/mod.rs, checker/builtins.rs, checker/functions.rs
│          │  Validates types, returns CheckResult (TypeEnv + FunctionSig map)
└────┬─────┘
     │
     ▼
┌─────────┐
│ Codegen  │  src/codegen/
│          │  mod.rs, expr.rs + expr/, stmt.rs + stmt/, functions.rs, abi.rs
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
│   ├── traits.rs              Trait flattening and conflict-resolution helpers
│   └── checker/
│       ├── mod.rs             check_stmt(), infer_type()
│       ├── builtins.rs        Built-in function type signatures
│       └── functions.rs       User function type inference
│
├── codegen/
│   ├── mod.rs                 generate() orchestration
│   ├── expr.rs                Expression codegen
│   ├── expr/                  Expression submodules
│   │   ├── arrays.rs          Indexed/assoc arrays, match, array access
│   │   ├── binops.rs          Arithmetic, comparison, bitwise, null-coalesce helpers
│   │   ├── calls.rs           Function / closure / indirect call dispatch
│   │   ├── calls/             Call-specific helpers
│   │   ├── coerce.rs          Truthiness / string / null coercions
│   │   ├── compare.rs         Comparison and widening helpers
│   │   ├── helpers.rs         Shared expression-codegen utilities
│   │   ├── objects.rs         Object-expression dispatch
│   │   ├── objects/           Allocation / property / method dispatch helpers
│   │   └── ownership.rs       Result ownership classification
│   ├── stmt.rs                Statement codegen
│   ├── stmt/                  Statement submodules
│   │   ├── assignments.rs     Variable / property assignment helpers
│   │   ├── arrays.rs          Array statement dispatch
│   │   ├── arrays/            Array assign / push / list-unpack helpers
│   │   ├── control_flow.rs    Loop / branch dispatch
│   │   ├── control_flow/      Branching / foreach / loop helpers
│   │   ├── io.rs              Echo / print helpers
│   │   └── storage.rs         Global / static / extern-global helpers
│   ├── functions.rs           User function emission
│   ├── abi.rs                 ARM64 register conventions
│   ├── ffi.rs                 Extern function/global/class codegen
│   ├── context.rs             Variables, labels, loop stack, ownership lattice
│   ├── data_section.rs        String/float literal .data section
│   ├── emit.rs                Assembly text buffer
│   │
│   ├── builtins/              Built-in function codegen (one file per language function)
│   │   ├── mod.rs             Dispatcher — chains to category modules
│   │   ├── strings/           strlen, substr, strpos, explode, sprintf, md5, ... (57 files)
│   │   ├── arrays/            count, array_push, sort, array_map, usort, ... (56 files)
│   │   ├── math/              abs, floor, pow, rand, fmod, fdiv, round, min, max, sin, cos, ... (32 files)
│   │   ├── types/             is_*, gettype, empty, unset, settype, ... (16 files)
│   │   ├── io/                fopen, fwrite, file_get_contents, scandir, ... (36 files)
│   │   ├── pointers/          ptr, ptr_get, ptr_set, ptr_read8, ptr_write8, ptr_offset, ... (12 files)
│   │   └── system/            exit, define, time, date, mktime, json_encode, preg_match, ... (25 files)
│   │
│   └── runtime/               ARM64 runtime routines (one file per language/runtime helper)
│       ├── mod.rs             Emits all runtime functions into assembly
│       ├── strings/           itoa, concat, ftoa, sprintf, md5, sha1, str_persist, ... (53 files)
│       ├── arrays/            heap_alloc, heap_free, array_free_deep, array_grow, hash_grow, hash_*, sort, usort, refcount, gc/decref dispatch, ... (89 files)
│       ├── io/                fopen, fgets, fread, stat, scandir, ... (17 files)
│       ├── system/            build_argv, time, getenv, shell_exec, date, mktime, strtotime, json_encode_*, json_decode, preg_*, ... (24 files)
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
2. `string` arguments are converted with `__rt_str_to_cstr`, which allocates a null-terminated C buffer that is valid for the duration of the native call and is released immediately after the call returns.
3. `string` return values are converted with `__rt_cstr_to_str`, which treats the returned `char *` as borrowed and copies bytes back into an owned elephc string.
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
| Heap allocator | `_heap_buf`, `_heap_off`, `_heap_free_list`, `_heap_small_bins`, `_heap_debug_enabled`, `_heap_max` | Heap storage plus general/small-bin allocator metadata and heap-debug toggle |
| Runtime diagnostics | `_heap_err_msg`, `_arr_cap_err_msg`, `_ptr_null_err_msg`, `_heap_dbg_*` | Fatal error messages plus heap-debug summary/failure strings |
| GC statistics and cycle state | `_gc_allocs`, `_gc_frees`, `_gc_live`, `_gc_peak`, `_gc_collecting`, `_gc_release_suppressed` | Allocation/free/live-byte counters plus targeted-cycle-collector coordination flags |
| I/O scratch | `_cstr_buf`, `_cstr_buf2`, `_eof_flags` | Syscall-oriented C-string scratch buffers and EOF bookkeeping |
| String/regex tables | `_fmt_g`, `_b64_encode_tbl`, `_b64_decode_tbl`, `_pcre_*` | Formatting and lookup tables for runtime helpers |
| JSON/date tables | `_json_true`, `_json_false`, `_json_null`, `_day_names`, `_month_names` | Static data used by JSON and date routines |
| Class/interface metadata tables | `_interface_count`, `_interface_method_ptrs`, `_interface_methods_<id>`, `_class_interface_ptrs`, `_class_interfaces_<id>`, `_class_interface_impl_<class>_<iface>`, `_class_gc_desc_count`, `_class_gc_desc_ptrs`, `_class_gc_desc_<id>`, `_class_vtable_ptrs`, `_class_vtable_<id>`, `_class_static_vtable_ptrs`, `_class_static_vtable_<id>` | Per-interface method-order metadata plus per-class property traversal metadata and instance/static dispatch tables |

### Heap allocator

8MB free-list + bump hybrid allocator in BSS (`_heap_buf`). Each allocation has a uniform 16-byte header: `[size:4][refcount:4][kind:8]` — a 32-bit block size, a 32-bit reference count, and an 8-byte heap-kind tag shared by arrays, hashes, objects, persisted strings, and raw helper buffers. The allocator now keeps four segregated small-block bins (`<=8`, `<=16`, `<=32`, `<=64` bytes) in `_heap_small_bins` ahead of the general address-ordered free list, so tiny short-lived blocks can often be reused without walking the full first-fit chain. When memory is freed (via `__rt_heap_free`), tail blocks still fold directly back into `_heap_off`, small non-tail blocks are cached in their size class, and larger blocks remain in the ordered free list where adjacent neighbors are coalesced and any free chain that reaches the current bump tail is trimmed back into the bump pointer. New allocations consult the matching small-bin class first, then the general free list (splitting oversized free blocks when needed), and only bump allocate when neither path can satisfy the request. Reference counting (`__rt_incref`, `__rt_decref_array`, `__rt_decref_hash`, `__rt_decref_object`) still handles the common acyclic case, while arrays and hashes now add copy-on-write splitting through `__rt_array_ensure_unique` / `__rt_hash_ensure_unique` plus shallow clone helpers before mutating shared containers. The low 16 bits of the kind word are now persistent container metadata: low byte = heap kind, bits 8-14 = indexed-array `value_type`, bit 15 = copy-on-write container flag, and higher bits remain reserved for transient cycle-collector state. Heap kind tags still use `0=raw/untyped`, `1=string`, `2=indexed array`, `3=assoc/hash`, `4=object`, giving the runtime a uniform discriminator regardless of payload layout. With `--heap-debug`, the runtime validates both the ordered free list and the segregated small-bin chains on allocator/free mutations, traps on double free or zero-refcount `incref`/`decref` paths, poisons freed payload bytes, and prints an end-of-process summary with alloc/free counts, live blocks, live bytes, and the peak live-byte watermark. With `--gc-stats`, generated programs also print allocation/free counters to stderr at exit without enabling the heavier heap-debug checks. Codegen now records a local ownership lattice (`Owned`, `Borrowed`, `MaybeOwned`, `NonHeap`) plus an `epilogue_cleanup_safe` bit in `Context::variables` so it can distinguish between stack slots that truly own a heap value and slots that merely alias global/static/container-backed storage. Ownership transfer points currently include ordinary reassignments, by-value call arguments, borrowed heap returns, indexed array writes, associative-array/hash writes, object property writes, `static` slot writes, `global` loads, `foreach` targets, and `list(...)` targets, while container-copy builtins now dispatch to dedicated `_refcounted` runtime helpers for nested array/hash/object/string payloads (`array` literals with spreads, `array_merge`, `array_chunk`, `array_slice`, `array_reverse`, `array_pad`, `array_unique`, `array_splice`, `array_diff`, `array_intersect`, `array_filter`, `array_fill`, `array_combine`, `array_fill_keys`). Mixed heap releases now funnel through `__rt_decref_any`, and object/container deep-free paths use richer runtime metadata plus per-class GC descriptor tables to discover nested heap-backed children. Function epilogues now clean up only locals that are both `Owned` and still marked safe; borrowed aliases such as `$this`, ref params, globals, and statics are explicitly excluded, and exhaustive `if` / `elseif` / `else` branches can now restore epilogue cleanup when every fallthrough branch directly stores the same heap-backed type into the same local. Loop-driven, switch-driven, and more dynamic alias-heavy joins remain conservative until more control-flow cases are proven. Configurable via `--heap-size=BYTES` (minimum 64KB), `--gc-stats`, and `--heap-debug` for runtime verification. Bounds-checked with fatal error on overflow.

### Hash table header (heap-allocated, for associative arrays)

```
Offset  Size  Field
  0      8    count       (number of occupied entries)
  8      8    capacity    (number of slots)
 16      8    value_type  (0=int, 1=str, 2=float, 3=bool, 4=array, 5=assoc, 6=object)
 24      8    head        (slot index of first inserted entry, or -1)
 32      8    tail        (slot index of last inserted entry, or -1)
 40      ...  entries     (each entry is 56 bytes)
```

Each hash table entry:

```
Offset  Size  Field
  0      8    occupied   (0=empty, 1=occupied, 2=tombstone)
  8      8    key_ptr    (pointer to key string)
 16      8    key_len    (key string length)
 24      8    value_lo   (value or pointer)
 32      8    value_hi   (string length, or unused for int)
 40      8    prev       (previous inserted slot, or -1)
 48      8    next       (next inserted slot, or -1)
```

Lookups still use FNV-1a hashing with linear probing for collision resolution, but language-visible iteration follows the `head -> next -> ... -> tail` insertion-order chain. For the full runtime layout and iteration contract, see [Memory Model](memory-model.md).

### Object layout (heap-allocated)

```
Offset  Size  Field
  0      8    class_id  (identifies which class this object belongs to)
  8     16    prop[0]   (first property — 16 bytes regardless of type)
 24     16    prop[1]   (second property)
 ...    ...   ...
```

Total size: `8 + (num_properties × 16)`. Properties are stored at fixed offsets determined at compile time in parent-first order across the inheritance chain. Property access is `base + resolved_property_offset`.

### Method dispatch

- Instance methods: codegen resolves a stable slot number from the static class metadata, then uses the object's `class_id` to load the concrete class vtable entry and `blr` to the implementation. The object pointer is still passed as the first argument in `x0` (as `$this`).
- Private instance methods are excluded from the vtable and emitted as direct calls within the declaring class, preserving PHP's lexical binding for parent-private helpers.
- Interfaces and abstract classes are enforced at compile time. Runtime method calls still use the existing class vtables, while dedicated interface metadata tables are emitted alongside the class metadata for roadmap-aligned interface bookkeeping and future dispatch work.
- Static methods: `bl _static_ClassName_methodName`. No object pointer is passed.
- `self::method()` / `parent::method()`: emitted as direct lexical calls, but static targets still forward the current "called class" id for later `static::` lookups.
- `static::method()`: uses a per-class static-method table keyed by the forwarded called-class id, so late static binding works across inherited static overrides.

Traits are flattened into the owning class before inheritance metadata is built. That means trait members participate in the same inherited property layout and vtable construction as ordinary class members after `use` / `as` / `insteadof` resolution.

### String buffer

64KB scratch buffer in BSS (`_concat_buf`). Used by `itoa`, `concat`, `strtolower`, and all string-producing runtime routines. Reset to offset 0 at the start of each statement. Strings that need to persist beyond the current statement are copied to the heap via `__rt_str_persist`.

### I/O buffers

Two 4KB C-string conversion buffers (`_cstr_buf`, `_cstr_buf2`) are still used by low-level I/O helpers and syscalls. FFI string calls do not use these scratch buffers anymore; they allocate per-call C strings through `__rt_str_to_cstr` so multiple string arguments remain valid across the same native call and are then released as soon as control returns from C. A 256-byte EOF flag array (`_eof_flags`) tracks end-of-file state per file descriptor.
