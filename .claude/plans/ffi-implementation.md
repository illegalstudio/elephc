# Implementation Plan: FFI — Calling C Libraries from elephc

## Overview

Add Foreign Function Interface (FFI) support so elephc programs can call functions from external C libraries (SDL2, libcurl, SQLite, etc.). Since elephc already emits ARM64 assembly using the standard C ABI and links with `ld`, the core mechanism is straightforward: declare the function signature, emit `bl _symbol`, pass `-l` flags to the linker.

The real complexity is in **type mapping** — bridging PHP types to C types, especially pointers, null-terminated strings, and C structs (mapped to elephc classes).

### Prerequisites

This plan **requires classes** (see `classes-implementation.md`). C structs are mapped to elephc class instances with matching memory layout. Implement classes first.

### In scope

- `extern` function declarations with explicit type annotations
- `extern` global variable declarations
- C type annotations: `int`, `float`, `string`, `bool`, `void`, `ptr`
- Opaque pointer type (`ptr`) for `void*`, handles, etc.
- Typed pointers (`ptr<ClassName>`) for C struct pointers
- Null-terminated string ↔ length-prefixed string conversion
- C struct ↔ elephc class mapping (flat layout, no padding magic)
- CLI flag `--link` / `-l` for specifying libraries
- `extern` block syntax for grouping declarations from the same library
- Pointer operations: `ptr_get()`, `ptr_set()`, `ptr_offset()`, `ptr_null()`, `ptr_is_null()`
- Callback function pointers (passing elephc functions to C)

### Not in scope

- Variadic C functions (e.g., `printf` — would require va_list)
- Union types
- Bitfields
- Inline C code
- C preprocessor / header parsing
- Complex struct nesting (struct containing struct by value — pointers to structs are fine)

---

## Syntax Design

### Single extern declaration

```php
<?php
extern function abs(int $n): int;           // libc
extern function sqrt(float $x): float;      // libm
extern function SDL_Init(int $flags): int;   // libSDL2
```

### Extern block (groups declarations from same library)

```php
<?php
extern "SDL2" {
    function SDL_Init(int $flags): int;
    function SDL_Quit(): void;
    function SDL_CreateWindow(string $title, int $x, int $y, int $w, int $h, int $flags): ptr;
    function SDL_DestroyWindow(ptr $window): void;
    function SDL_Delay(int $ms): void;
}
```

The `extern "SDL2"` block automatically adds `-lSDL2` to the linker. Single `extern function` declarations require manual `--link` flags.

### Type annotations

```
int       → C int/long (64-bit, passed in x register)
float     → C double (64-bit, passed in d register)
string    → C char* (auto null-terminated on call, auto length-computed on return)
bool      → C int (0/1)
void      → no return value
ptr       → opaque pointer (void*, any handle — 64-bit in x register)
ptr<Name> → typed pointer to a C struct mapped by class Name
```

### C struct mapping via classes

```php
<?php
extern class SDL_Rect {
    public int $x;
    public int $y;
    public int $w;
    public int $h;
}
```

An `extern class` has:
- No methods, no constructor — just fields with C-compatible layout
- Fields laid out sequentially in memory (like a C struct)
- Instances are stack-allocated or heap-allocated depending on usage
- Used with `ptr<SDL_Rect>` in extern function signatures

### Pointer operations

```php
<?php
$rect = new SDL_Rect();   // allocate on heap
$rect->x = 10;
$rect->y = 20;

// Pass pointer to C function
SDL_RenderFillRect($renderer, ptr($rect));  // ptr() takes object, returns ptr

// Opaque pointers
$window = SDL_CreateWindow("Hello", 0, 0, 800, 600, 0);  // returns ptr
SDL_DestroyWindow($window);

// Null check
if (ptr_is_null($window)) { ... }

// Pointer from C returning struct pointer
$event = ptr_cast<SDL_Event>(SDL_WaitEvent());
echo $event->type;
```

### Extern global variables

```php
<?php
extern global int $errno;     // C global: errno
extern global ptr $stdin;     // C global: stdin (FILE*)
extern global ptr $stdout;    // C global: stdout
```

These emit `adrp` + `add` to load the address of the external symbol.

---

## Phase 1: Lexer — New Tokens

**Files to modify:**
- `src/lexer/token.rs`
- `src/lexer/scan.rs`
- `src/lexer/literals.rs`

**New tokens:**

```
Extern         // extern
Ptr            // ptr (type annotation context)
TypeInt        // int (type annotation context)
TypeFloat      // float (type annotation context)
TypeString     // string (type annotation context)
TypeBool       // bool (type annotation context)
TypeVoid       // void (type annotation context)
ColonReturn    // : (return type separator — already Token::Colon)
```

**Changes:**

1. In `token.rs`, add `Extern` to the Token enum.

2. In `literals.rs`, `scan_keyword`: add `"extern"` → `Token::Extern`.

3. Type keywords (`int`, `float`, `string`, `bool`, `void`, `ptr`) — these are context-sensitive. They're only types when used in extern declarations (after parameter `$name` or after `:`). Two options:
   - **Option A**: Lex them as `Token::Identifier("int")` always, let the parser disambiguate.
   - **Option B**: Add dedicated tokens.
   - **Decision: Option A.** Avoids conflicts with existing uses (e.g., `(int)` cast already works via Identifier). The parser recognizes type names in extern context.

4. No new scanning logic needed for `:` (already `Token::Colon`) or `<`/`>` (already `Token::Lt`/`Token::Gt`).

**Minimal token additions: just `Token::Extern`.**

---

## Phase 2: Parser — AST Nodes

**Files to modify:**
- `src/parser/ast.rs`
- `src/parser/stmt.rs`

### New AST nodes

Add to `StmtKind`:
```rust
ExternFunctionDecl {
    name: String,
    params: Vec<ExternParam>,
    return_type: CType,
    library: Option<String>,  // from extern "lib" block
},
ExternClassDecl {
    name: String,
    fields: Vec<ExternField>,
},
ExternGlobalDecl {
    name: String,
    c_type: CType,
},
```

New supporting types:
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum CType {
    Int,
    Float,
    Str,
    Bool,
    Void,
    Ptr,                    // opaque void*
    TypedPtr(String),       // ptr<ClassName>
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExternParam {
    pub name: String,
    pub c_type: CType,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExternField {
    pub name: String,
    pub c_type: CType,
    pub span: Span,
}
```

### Parser changes

1. **stmt.rs** — `parse_stmt`: Add match arm for `Token::Extern`:

   ```
   extern function name(type $param, ...): return_type;
   extern "libname" { ... }
   extern class Name { ... }
   extern global type $name;
   ```

   `parse_extern`:
   - If next is `Token::StringLiteral` → extern block:
     - Read library name, consume `{`
     - Loop: parse `function ...;` declarations, each tagged with library name
     - Consume `}`
   - If next is `Token::Function` → single extern function:
     - Parse name, `(`, params with type annotations, `)`, `:`, return type, `;`
   - If next is `Token::Identifier("class")` or `Token::Class` → extern class:
     - Parse name, `{`, fields with types, `}`
   - If next is `Token::Identifier("global")` → extern global:
     - Parse type, `$name`, `;`

2. **Helper** `parse_c_type`:
   - `"int"` → `CType::Int`
   - `"float"` → `CType::Float`
   - `"string"` → `CType::Str`
   - `"bool"` → `CType::Bool`
   - `"void"` → `CType::Void`
   - `"ptr"` → if followed by `<` → `CType::TypedPtr(name)`, else `CType::Ptr`

3. **Helper** `parse_extern_params`:
   - Parse `type $name` pairs separated by `,`
   - Each param: parse CType, then `Token::Variable(name)`

### New expression AST nodes

Add to `ExprKind`:
```rust
PtrCast {
    class_name: String,
    expr: Box<Expr>,
},
```

Add `ptr()`, `ptr_is_null()`, `ptr_null()`, `ptr_offset()`, `ptr_get()`, `ptr_set()` as built-in functions (not new AST nodes — handled in builtins).

---

## Phase 3: Type Checker

**Files to modify:**
- `src/types/mod.rs`
- `src/types/checker/mod.rs`

**New file:** `src/types/checker/extern_decls.rs`

### PhpType changes

Add to `PhpType`:
```rust
Ptr,                    // opaque void* — 8 bytes, 1 register, int reg
TypedPtr(String),       // ptr<ClassName> — 8 bytes, 1 register, int reg
```

- `stack_size()` → 8
- `register_count()` → 1
- `is_float_reg()` → false

### New data structures

```rust
pub struct ExternFunctionInfo {
    pub name: String,               // C symbol name
    pub params: Vec<(String, CType)>,
    pub return_type: CType,
    pub library: Option<String>,    // for linker flag
}

pub struct ExternClassInfo {
    pub name: String,
    pub fields: Vec<ExternFieldInfo>,
    pub total_size: usize,          // computed from fields
}

pub struct ExternFieldInfo {
    pub name: String,
    pub c_type: CType,
    pub offset: usize,              // byte offset in struct
    pub size: usize,                // field size in bytes
}
```

Add to `CheckResult`:
```rust
pub extern_functions: HashMap<String, ExternFunctionInfo>,
pub extern_classes: HashMap<String, ExternClassInfo>,
pub required_libraries: Vec<String>,  // collected from extern blocks
```

### Type checking logic

1. **ExternFunctionDecl**: Register in `checker.extern_functions`. Convert `CType` → `PhpType` for signature. Collect library name into `required_libraries`.

2. **ExternClassDecl**: Compute field offsets and total size. C struct layout:
   - `int` → 8 bytes (we use 64-bit)
   - `float` → 8 bytes
   - `bool` → 8 bytes (C int)
   - `ptr` / `ptr<T>` → 8 bytes
   - `string` → 8 bytes (char*, single pointer — NOT length-prefixed)
   - Fields at sequential offsets, 8-byte aligned (simplified — no padding games)

3. **Function calls**: When resolving `FunctionCall`, check `extern_functions` first (before user functions and builtins). Verify argument count and types match. Type-coerce where safe (int → float, etc.).

4. **Ptr builtins**: Type-check `ptr()`, `ptr_is_null()`, etc.:
   - `ptr($obj)` → takes Object, returns Ptr
   - `ptr_is_null($p)` → takes Ptr, returns Bool
   - `ptr_null()` → returns Ptr
   - `ptr_offset($p, $n)` → takes Ptr + Int, returns Ptr
   - `ptr_get($p, $offset)` → takes TypedPtr + Int, returns field type
   - `ptr_set($p, $offset, $val)` → takes TypedPtr + Int + value, returns Void
   - `ptr_cast<ClassName>($p)` → takes Ptr, returns TypedPtr(ClassName)

5. **String conversion check**: When passing `string` to an extern function expecting `CType::Str`, emit a conversion note (length-prefixed → null-terminated). When receiving a `CType::Str` return, note reverse conversion needed.

---

## Phase 4: Codegen

**Files to modify:**
- `src/codegen/mod.rs`
- `src/codegen/context.rs`
- `src/codegen/expr.rs`
- `src/codegen/abi.rs`

**New files:**
- `src/codegen/ffi.rs` — extern call emission
- `src/codegen/builtins/ffi/mod.rs` — ptr_*() builtins
- `src/codegen/builtins/ffi/ptr.rs`
- `src/codegen/builtins/ffi/ptr_is_null.rs`
- `src/codegen/builtins/ffi/ptr_null.rs`
- `src/codegen/builtins/ffi/ptr_offset.rs`
- `src/codegen/builtins/ffi/ptr_cast.rs`
- `src/codegen/runtime/ffi/str_to_cstr.rs` — length-prefixed → null-terminated
- `src/codegen/runtime/ffi/cstr_to_str.rs` — null-terminated → length-prefixed

### Context changes (`context.rs`)

```rust
pub extern_functions: HashMap<String, ExternFunctionInfo>,
pub extern_classes: HashMap<String, ExternClassInfo>,
```

### Extern function calls (`ffi.rs`)

`emit_extern_call(name, args, emitter, ctx, data) -> PhpType`:

1. Look up `ExternFunctionInfo` for param types.
2. Evaluate each argument:
   - **Int/Bool/Ptr**: evaluate → result in `x0` → push to stack
   - **Float**: evaluate → result in `d0` → push to stack
   - **String**: evaluate → result in `x1` (ptr) + `x2` (len) → call `__rt_str_to_cstr` → null-terminated `char*` in `x0` → push to stack
   - **TypedPtr(class)**: evaluate object → pointer already in `x0` → push
3. Pop arguments into C ABI registers:
   - `x0`-`x7` for int/ptr/bool/string(char*)
   - `d0`-`d7` for float/double
   - (elephc already does this — same ARM64 calling convention)
4. Emit `bl _symbolname` (NOT `_fn_symbolname` — use the raw C symbol)
5. Handle return value:
   - `CType::Int/Bool/Ptr` → result in `x0`
   - `CType::Float` → result in `d0`
   - `CType::Str` → `char*` in `x0` → call `__rt_cstr_to_str` → `x1`=ptr, `x2`=len
   - `CType::Void` → nothing

**Key difference from user function calls:** extern functions use the raw symbol name (`bl _SDL_Init`), not the elephc convention (`bl _fn_SDL_Init`). String arguments need null-termination conversion.

### String conversion routines

**`__rt_str_to_cstr`** (runtime/ffi/str_to_cstr.rs):
- Input: `x1` = pointer to elephc string, `x2` = length
- Allocates `length + 1` bytes on heap
- Copies string, appends `\0`
- Output: `x0` = pointer to null-terminated C string

**`__rt_cstr_to_str`** (runtime/ffi/cstr_to_str.rs):
- Input: `x0` = pointer to null-terminated C string
- Scans for `\0` to compute length
- Output: `x1` = pointer (same as input), `x2` = length
- Note: does NOT copy — the returned string points into C memory. This is safe as long as the C library doesn't free it while elephc holds the reference.

### ABI changes (`abi.rs`)

Add handling for `PhpType::Ptr` and `PhpType::TypedPtr(_)`:
- `emit_store` → same as Int (single `str x0`)
- `emit_load` → same as Int (single `ldr x0`)
- `emit_write_stdout` → print as hex address (for debugging): `"ptr(0x...)"` or custom format

### Pointer builtins

**`ptr($obj)`** — `builtins/ffi/ptr.rs`:
- Object is already a heap pointer in `x0`
- Just return `x0` as `PhpType::Ptr` (no-op, type change only)

**`ptr_null()`** — `builtins/ffi/ptr_null.rs`:
- `mov x0, #0`
- Return `PhpType::Ptr`

**`ptr_is_null($p)`** — `builtins/ffi/ptr_is_null.rs`:
- `cmp x0, #0` → `cset x0, eq`
- Return `PhpType::Bool`

**`ptr_offset($p, $n)`** — `builtins/ffi/ptr_offset.rs`:
- `add x0, x0, x1` (or `add x0, x0, x1, lsl #3` for element-sized offset)
- Return `PhpType::Ptr`

**`ptr_cast<T>($p)`** — handled via `ExprKind::PtrCast` since it has a type parameter:
- No-op at runtime (pointer is pointer)
- Type system changes from `Ptr` to `TypedPtr(T)`

### Expression codegen (`expr.rs`)

In the `ExprKind::FunctionCall` handler, before checking builtins and user functions:
```rust
if ctx.extern_functions.contains_key(name) {
    return ffi::emit_extern_call(name, args, emitter, ctx, data);
}
```

### Module-level changes (`mod.rs`)

1. Skip `ExternFunctionDecl` and `ExternClassDecl` during statement codegen (they're declarations only, no code emitted).

2. Populate `ctx.extern_functions` and `ctx.extern_classes` from `CheckResult` before codegen starts.

3. For each extern class, no code is emitted — the layout info lives in `ctx.extern_classes` and is used at call sites.

---

## Phase 5: Linker Integration

**File to modify:** `src/main.rs` (or wherever `as`/`ld` are invoked)

### CLI flag

```
elephc file.php --link SDL2 --link m
# or shorthand:
elephc file.php -l SDL2 -l m
```

### Automatic library collection

Libraries from `extern "libname" { }` blocks are collected in `CheckResult::required_libraries`. These are merged with CLI `--link` flags.

### Linker command

Current:
```
ld -o output -lSystem -syslibroot ... -e _main file.o
```

With FFI:
```
ld -o output -lSystem -lSDL2 -lm -syslibroot ... -e _main file.o
```

Each library adds a `-l{name}` flag. The linker searches standard paths (`/usr/lib`, `/usr/local/lib`, Homebrew paths).

### Library search paths

Add `--link-path` / `-L` flag for non-standard library locations:
```
elephc file.php -l SDL2 -L /opt/homebrew/lib
```

Passed to `ld` as `-L /opt/homebrew/lib`.

### Framework support (macOS)

macOS uses frameworks for system libraries. Add `--framework` flag:
```
elephc file.php --framework Cocoa --framework OpenGL
```

Passed to `ld` as `-framework Cocoa -framework OpenGL`.

---

## Phase 6: Resolver

**File:** `src/resolver.rs`

Add `StmtKind::ExternFunctionDecl`, `StmtKind::ExternClassDecl`, and `StmtKind::ExternGlobalDecl` to the match in the resolver. These don't contain bodies to resolve — just pass them through.

---

## Phase 7: Runtime

**New files:**
- `src/codegen/runtime/ffi/mod.rs`
- `src/codegen/runtime/ffi/str_to_cstr.rs`
- `src/codegen/runtime/ffi/cstr_to_str.rs`

### `__rt_str_to_cstr`

```asm
__rt_str_to_cstr:
    // Input: x1 = string ptr, x2 = string len
    // Output: x0 = null-terminated C string ptr
    // -- allocate len+1 bytes --
    add x0, x2, #1
    bl __rt_heap_alloc
    // -- copy bytes --
    // ... byte copy loop from x1 to x0, length x2 ...
    // -- null terminate --
    strb wzr, [x0, x2]
    ret
```

### `__rt_cstr_to_str`

```asm
__rt_cstr_to_str:
    // Input: x0 = null-terminated C string ptr
    // Output: x1 = string ptr, x2 = string len
    mov x1, x0
    mov x2, #0
    // -- scan for null --
.loop:
    ldrb w3, [x1, x2]
    cbz w3, .done
    add x2, x2, #1
    b .loop
.done:
    ret
```

---

## Phase 8: Callback Support

Passing elephc functions as C callback function pointers (e.g., for `qsort`, `SDL_AddTimer`, signal handlers).

### Syntax

```php
<?php
extern function qsort(ptr $base, int $nmemb, int $size, callable $compar): void;

function my_compare(ptr $a, ptr $b): int {
    return ptr_get($a, 0) - ptr_get($b, 0);
}

qsort($array_ptr, $count, 8, 'my_compare');
```

### Implementation

When the type checker sees a `callable` param in an extern declaration:
1. The argument must be a string literal naming a user function
2. The codegen emits `adrp x#, _fn_funcname@PAGE` + `add x#, x#, _fn_funcname@PAGEOFF` to load the function's address
3. The function pointer is passed in the corresponding `x` register

**Limitation:** The callback function must use C-compatible types (no length-prefixed strings in callback params). Parameters and return value are passed via standard ARM64 ABI.

---

## Phase 9: Tests

### Lexer tests
- `test_lex_extern_keyword` — `extern` produces `Token::Extern`

### Parser tests
- `test_parse_extern_function` — `extern function abs(int $n): int;`
- `test_parse_extern_block` — `extern "SDL2" { function SDL_Init(int $flags): int; }`
- `test_parse_extern_class` — `extern class Point { public int $x; public int $y; }`
- `test_parse_extern_global` — `extern global int $errno;`
- `test_parse_extern_ptr_type` — `ptr` and `ptr<ClassName>` types
- `test_parse_extern_void_return` — `extern function free(ptr $p): void;`

### Codegen tests (end-to-end with libc)
- `test_ffi_libc_abs` — `extern function abs(int $n): int; echo abs(-42);` → `42`
- `test_ffi_libc_sqrt` — `extern function sqrt(float $x): float; echo sqrt(4.0);` → `2` (libm, already linked)
- `test_ffi_libc_strlen` — `extern function strlen(string $s): int; echo strlen("hello");` → `5` (tests string→cstr conversion)
- `test_ffi_libc_getpid` — `extern function getpid(): int;` → outputs a number
- `test_ffi_ptr_null` — `ptr_null()` returns null, `ptr_is_null()` confirms
- `test_ffi_ptr_operations` — `ptr_offset`, basic pointer math
- `test_ffi_multiple_libs` — functions from multiple extern blocks
- `test_ffi_void_return` — function returning void
- `test_ffi_float_args` — `extern function pow(float $x, float $y): float;`
- `test_ffi_mixed_args` — function with both int and float params
- `test_ffi_string_return` — C function returning `char*`, converted to elephc string
- `test_ffi_extern_class` — extern class with fields, pass to C function
- `test_ffi_callback` — pass elephc function as callback to C (via qsort or similar)

### Error tests
- `test_error_extern_wrong_arg_count` — wrong number of args to extern function
- `test_error_extern_wrong_arg_type` — type mismatch in extern call
- `test_error_extern_undefined` — calling undeclared extern
- `test_error_extern_class_method` — extern class can't have methods
- `test_error_extern_missing_return_type` — extern function without return type
- `test_error_extern_missing_param_type` — extern param without type annotation

### Example program (`examples/ffi/main.php`)

```php
<?php

// Call C standard library functions directly
extern function abs(int $n): int;
extern function getpid(): int;

$pid = getpid();
echo "PID: $pid\n";
echo "abs(-42) = " . abs(-42) . "\n";
echo "abs(17) = " . abs(17) . "\n";
```

---

## Phase 10: Documentation

- **`ROADMAP.md`**: Add FFI section (v1.1 or new version). Update "Will not implement" if needed.
- **`docs/language-reference.md`**: Add FFI section: `extern` syntax, type annotations, pointer operations, extern classes, callback support.
- **`docs/architecture.md`**: Add FFI section: how extern calls are emitted, string conversion, linker integration, C struct mapping.

---

## Implementation Order

| Step | Phase | What | Depends on |
|------|-------|------|------------|
| 1 | Lexer | `Token::Extern` | — |
| 2 | Parser | `CType`, `ExternParam`, `ExternField` types | Step 1 |
| 3 | Parser | `parse_extern` → `ExternFunctionDecl` | Step 2 |
| 4 | Parser | `parse_extern` → `ExternClassDecl` | Step 2 |
| 5 | Parser | `parse_extern` → `ExternGlobalDecl` | Step 2 |
| 6 | Type checker | `PhpType::Ptr`, `PhpType::TypedPtr` | Step 3 |
| 7 | Type checker | Extern function signature validation | Step 6 |
| 8 | Type checker | Extern class layout computation | Step 4, 6 |
| 9 | Runtime | `__rt_str_to_cstr`, `__rt_cstr_to_str` | — |
| 10 | Codegen | `ffi.rs` — `emit_extern_call` | Step 7, 9 |
| 11 | Codegen | Ptr builtins (`ptr()`, `ptr_is_null()`, etc.) | Step 6 |
| 12 | Codegen | Extern class field access | Step 8, classes plan |
| 13 | Linker | `-l`, `-L`, `--framework` CLI flags | Step 10 |
| 14 | Linker | Auto-collect libraries from extern blocks | Step 13 |
| 15 | Codegen | Callback function pointers | Step 10 |
| 16 | Resolver | Pass-through for extern nodes | Step 3, 4, 5 |
| 17 | Tests | Full test suite + example | All above |
| 18 | Docs | Language reference, architecture | Step 17 |

**Total estimated effort: 20-30 hours** (after classes are implemented)

---

## Risks and Considerations

1. **String ownership**: When C returns a `char*`, elephc wraps it without copying. If C frees that memory, elephc has a dangling pointer. Document this as a known limitation — user must ensure C strings outlive their elephc references.

2. **Struct alignment**: Real C structs have platform-specific alignment and padding. This plan uses simplified 8-byte-aligned fields. For most 64-bit types this works, but smaller types (`char`, `short`, `int32_t`) would be wrong. Future work: support explicit field sizes (`int8`, `int16`, `int32`, `int64`).

3. **No GC**: Heap-allocated extern class instances are never freed (same as regular classes). Memory from C (`malloc`) is not tracked at all. Document as known limitation.

4. **macOS library paths**: Homebrew installs to `/opt/homebrew/lib` on ARM64 Macs. Users may need `-L` flags. Consider auto-detecting Homebrew prefix.

5. **libc is always linked**: elephc already links `-lSystem` (macOS libc). Functions like `abs`, `strlen`, `getpid` are available without extra `-l` flags. This makes initial testing easy.

6. **Symbol naming**: macOS prefixes C symbols with `_` (e.g., `abs` → `_abs`). The assembler handles this automatically when you write `bl _abs`. elephc already uses this convention for `_main`.

7. **Variadic functions**: `printf`, `sprintf`, `snprintf` use `va_list` which requires special ABI handling on ARM64. Explicitly out of scope — use elephc's built-in `sprintf` instead.

8. **Thread safety**: Not relevant — elephc is single-threaded. But if linking libraries that create threads (SDL2 does internally), ensure no shared mutable state between threads.

9. **Dependency on classes plan**: `extern class` requires the regular class system to be implemented first (object allocation, field access at offsets). Implement `classes-implementation.md` before this plan.
