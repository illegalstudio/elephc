---
title: "The EIR Design"
description: "Specification for elephc's default intermediate representation between AST optimization and assembly emission."
sidebar:
  order: 13
---

**Status:** EIR is the default user-facing backend. The diagnostic `--emit-ir`
path lowers the checked and optimized AST into validated textual EIR, and the
normal executable/cdylib path lowers that same EIR into assembly. The legacy
AST backend remains available only as the temporary `--ast-backend` fallback.

**Implementation phases:** `.plans/eir-*.md`

**Authoritative source audit for this spec:**

- `src/types/model.rs`
- `src/parser/ast/expr.rs`
- `src/parser/ast/stmt.rs`
- `src/parser/ast/operators.rs`
- `src/parser/ast/types.rs`
- `src/parser/ast/oop.rs`
- `src/parser/ast/ffi.rs`
- `src/ir/`
- `src/ir_lower/`
- `src/codegen_ir/`
- `src/codegen/expr.rs` and `src/codegen/expr/`
- `src/codegen/stmt.rs` and `src/codegen/stmt/`
- `src/codegen/functions/locals.rs`
- `src/codegen/context.rs`
- `src/codegen/builtins/`
- `src/codegen/runtime/emitters.rs`
- `src/optimize/effects.rs` and `src/optimize/effects/`

EIR is elephc's intermediate representation. It sits between the AST-level
optimizer and the assembly emitter, giving the compiler a function-wide
control-flow and value model without replacing the hand-written assembly
backend.

EIR is intentionally PHP-shaped. Arrays, hashes, Mixed boxing, callable
descriptors, copy-on-write checks, fatal paths, exception paths, runtime calls,
and exact source evaluation order are first-class compiler concepts. EIR is not
a generic LLVM- or Cranelift-style IR.

## Historical Design Boundary

The first roadmap item originally covered this document only:

```text
EIR design specification (`docs/internals/the-ir.md`) - types, instructions,
terminators, effects, ownership, textual format
```

That first item did **not** include:

- EIR -> assembly backend
- `--ir-backend`
- register allocation
- IR optimization passes

That design-only milestone is complete. The current implementation has since
added `src/ir/`, `src/ir_lower/`, `--emit-ir`, and the default EIR assembly
backend under `src/codegen_ir/`. Register allocation and IR optimization passes
remain follow-up work.

## Pipeline Position

Current default production path:

```text
PHP source
  -> Lexer
  -> Parser
  -> Magic constants
  -> Conditional compilation
  -> Resolver
  -> NameResolver
  -> Autoload insertion
  -> AST constant folding
  -> Type checker / warnings
  -> AST optimizer passes
  -> AST -> EIR lowering
  -> EIR validation
  -> EIR optimization passes (fixed-point driver)
  -> EIR -> assembly backend
  -> runtime cache
  -> assembler / linker
  -> binary
```

Temporary legacy fallback path (`--ast-backend`):

```text
PHP source
  -> Lexer
  -> Parser
  -> Magic constants
  -> Conditional compilation
  -> Resolver
  -> NameResolver
  -> Autoload insertion
  -> AST constant folding
  -> Type checker / warnings
  -> AST optimizer passes
  -> AST -> assembly codegen
  -> runtime cache
  -> assembler / linker
  -> binary
```

The AST optimizer remains in front of EIR. It handles PHP-preserving rewrites
that are naturally expressed over syntax: constant folding, local scalar
propagation, control-flow pruning, control-flow normalization, and DCE. EIR
adds what the AST cannot express well: value identity, basic blocks, block
parameters, liveness, dominance, register placement, CSE, LICM, and inlining.

## Backend Selection

The compiler currently supports two assembly backends:

- EIR backend, selected by default and also by explicit `--ir-backend`
- legacy AST backend, selected only by `--ast-backend`

`--ast-backend` is a deprecated escape hatch while the legacy emitter remains
in-tree. It emits a warning and will be removed after the default EIR path has
completed its validation window. EIR preserves the existing hand-written assembly
style and adds linear-scan register allocation plus a fixed-point IR optimization
pass driver (see [Optimization Passes](#optimization-passes)); further IR passes
are incremental follow-up work.

## Design Invariants

- EIR preserves PHP source evaluation order separately from ABI parameter order.
- EIR values are SSA: each value is defined exactly once.
- Blocks use block parameters instead of phi nodes.
- PHP runtime concepts remain explicit: `Mixed`, COW arrays/hashes, callables,
  exception/fatal behavior, and ownership operations are not hidden inside a
  generic "call".
- EIR is target-aware through metadata and the selected `Target`, but it does
  not hardcode ARM64/x86_64 register names or stack layouts.
- `src/codegen/abi/` remains authoritative for physical ABI lowering.
- Runtime helpers remain outside the EIR `Module`; EIR references them through
  `RuntimeCall` or specialized opcodes.
- Source spans must survive lowering for diagnostics, source maps, and `--emit-ir`
  readability.
- Builder-created effect summaries are conservative. A pass may refine effects
  only when it can prove PHP-visible behavior is preserved.

## Type System

EIR uses a small storage lattice. PHP type details that share the same storage
class are preserved in `Value.php_type` and opcode metadata.

```rust
pub enum IrType {
    I64,
    F64,
    Str,
    Heap(IrHeapKind),
    Void,
}

pub enum IrHeapKind {
    Array,
    Hash,
    Object(String),
    Mixed,
    Iterable,
    Union,
    Buffer(Box<PhpType>),
    Packed(String),
}
```

`IrType` is a storage contract, not a replacement for `PhpType`. Every value
also carries the original or inferred PHP type:

```rust
pub struct Value {
    pub ir_type: IrType,
    pub php_type: PhpType,
    pub def: ValueDef,
    pub ownership: Ownership,
}
```

### PHP Type Mapping

| `PhpType` | `IrType` | Notes |
|---|---|---|
| `Int` | `I64` | Signed 64-bit integer storage. |
| `Float` | `F64` | 64-bit floating-point storage. |
| `Str` | `Str` | `(ptr, len)` pair; ownership may be transient, owned, borrowed, or persistent. |
| `Bool` | `I64` | `0` or `1`, with `php_type = Bool`. |
| `Void` | `Void` when no value is materialized; `I64` null sentinel only when a null value is stored or boxed. |
| `Never` | `Void` | Never materialized; produced paths end in `Return`, `Throw`, `Fatal`, or `Unreachable`. |
| `Iterable` | `Heap(Iterable)` | Type-erased array or traversable object pointer. |
| `Mixed` | `Heap(Mixed)` | Pointer to runtime tagged Mixed cell. |
| `Array(inner)` | `Heap(Array)` | Indexed array heap object; element type stays in `php_type`. |
| `AssocArray { key, value }` | `Heap(Hash)` | Hash table heap object; key/value types stay in `php_type`. |
| `Buffer(inner)` | `Heap(Buffer(inner))` | Fixed-size elephc buffer header; not PHP array COW storage. |
| `Callable` | `I64` | Callable descriptor address; ownership is tracked separately and released via callable descriptor runtime. |
| `Object(name)` | `Heap(Object(name))` | Runtime object pointer and class metadata. |
| `Packed(name)` | `Heap(Packed(name))` | Pointer/layout handle for packed POD data. |
| `Pointer(tag)` | `I64` | Raw native pointer; optional type tag remains in `php_type`. |
| `Resource(kind)` | `I64` | Runtime/native resource handle; optional kind remains in `php_type`. |
| `Union(members)` | `Heap(Union)` | Runtime representation is boxed like `Mixed`; member set stays in `php_type`. |

`Callable`, `Str`, and refcounted heap values can be owned even when their
storage type is not `Heap(...)`. Ownership is a separate value property.

### Parsed Type Expressions

`TypeExpr` maps into `PhpType` during type checking before EIR lowering. EIR
does not store syntactic type expressions except where metadata needs to point
back to a source declaration.

| `TypeExpr` | EIR relevance |
|---|---|
| `Int`, `Float`, `Bool`, `Str`, `Void`, `Never`, `Iterable` | Lower through the resolved `PhpType`. |
| `Ptr(Option<Name>)` | Lowers to `PhpType::Pointer(_)`, then `IrType::I64`. |
| `Buffer(inner)` | Lowers to `PhpType::Buffer(_)`, then `Heap(Buffer(_))`. |
| `Named(name)` | Resolved to object, packed class, enum, interface-compatible object, or builtin type before EIR. |
| `Nullable(inner)` | Lowered as `Union([inner, Void])` or the checker-approved nullable runtime shape. |
| `Union(members)` | Lowered as `PhpType::Union(_)`, represented as `Heap(Union)`. |

### C Type Mapping

Extern declarations use `CType`. EIR preserves the C-facing type in
`ExternDecl` metadata so the IR backend can use the correct C ABI helpers.

| `CType` | EIR storage |
|---|---|
| `Int`, `Bool`, `Ptr`, `TypedPtr(_)`, `Callable` | `I64` |
| `Float` | `F64` |
| `Str` | `I64` native C string pointer; PHP strings are converted at the boundary. |
| `Void` | `Void` |

## Module Structure

```rust
pub struct Module {
    pub target: Target,
    pub functions: Vec<Function>,
    pub class_methods: Vec<Function>,
    pub closures: Vec<Function>,
    pub fiber_wrappers: Vec<Function>,
    pub callback_wrappers: Vec<Function>,
    pub extern_callback_trampolines: Vec<Function>,
    pub runtime_callable_invokers: Vec<Function>,
    pub data: DataPool,
    pub extern_decls: Vec<ExternDecl>,
    pub class_table: ClassTable,
    pub enum_table: EnumTable,
    pub interface_table: InterfaceTable,
    pub packed_layouts: PackedLayoutTable,
    pub required_runtime_features: RuntimeFeatures,
}
```

The separate function vectors mirror current deferred codegen surfaces in
`src/codegen/context.rs`: closures, fiber wrappers, callback wrappers, extern
callback trampolines, and runtime callable invokers. Phase 02 may store them in
one `functions` vector with flags, but the semantic distinction must remain
representable.

```rust
pub struct Function {
    pub id: FunctionId,
    pub name: String,
    pub params: Vec<FunctionParam>,
    pub return_type: IrType,
    pub return_php_type: PhpType,
    pub locals: Vec<LocalSlot>,
    pub blocks: Vec<BasicBlock>,
    pub values: Vec<Value>,
    pub instructions: Vec<Instruction>,
    pub entry: BlockId,
    pub source_signature: Option<FunctionSigRef>,
    pub flags: FunctionFlags,
}

pub struct BasicBlock {
    pub id: BlockId,
    pub name: String,
    pub params: Vec<ValueId>,
    pub instructions: Vec<InstId>,
    pub terminator: Terminator,
}

pub struct LocalSlot {
    pub name: Option<String>,
    pub ir_type: IrType,
    pub php_type: PhpType,
    pub kind: LocalKind,
}

pub enum LocalKind {
    PhpLocal,
    GlobalAlias,
    StaticLocal,
    RefCell,
    HiddenTemp,
    TryHandler,
    ClosureCapture,
    NamedArgTemp,
    IteratorState,
    GeneratorState,
}
```

`LocalSlot` exists even in SSA form because PHP locals, globals, statics,
references, try handlers, and hidden temporaries have addressable storage or
observable lifetime behavior.

## Value Ownership

Ownership mirrors the current `HeapOwnership` model but is attached to every
SSA value, not only context locals.

```rust
pub enum Ownership {
    NonHeap,
    Owned,
    Borrowed,
    MaybeOwned,
    Persistent,
    Moved,
}
```

| State | Meaning |
|---|---|
| `NonHeap` | Plain scalar or native handle with no release operation. |
| `Owned` | The value owns cleanup responsibility. |
| `Borrowed` | The value aliases storage owned elsewhere. |
| `MaybeOwned` | CFG merge or dynamic path where ownership must be resolved before cleanup. |
| `Persistent` | Static data, interned/static string, or runtime-persistent value that must not be released. |
| `Moved` | Value has transferred ownership and cannot be used again except by validator diagnostics. |

Ownership-producing operations:

| Operation | Ownership result |
|---|---|
| Scalar constants | `NonHeap` |
| Static string literal | `Persistent` unless persisted/copied into heap |
| Runtime allocation | `Owned` |
| Local/global/static load | Slot metadata determines `Owned`, `Borrowed`, or `MaybeOwned` |
| `Borrow(value)` | `Borrowed` |
| `Move(value)` | Transfers source to `Moved`, result keeps prior ownership |
| `Acquire(value)` | Produces or records an owned retain |
| `Return(owned)` | Transfers ownership to caller |

Ownership operations:

| Op | Operand | Result | Effects | Lowering |
|---|---|---|---|---|
| `Acquire` | refcounted/string/callable value | `Void` or retained value alias | `REFCOUNT_OP`, maybe `WRITES_HEAP` | `__rt_incref`, string persist/retain, callable descriptor retain if added |
| `Release` | owned value | `Void` | `REFCOUNT_OP`, maybe `WRITES_HEAP`, debug may fatal | `__rt_decref_any`, `__rt_heap_free_safe`, callable descriptor release |
| `Move` | any value | same type | pure validator operation | no machine instruction |
| `Borrow` | value with live owner | same type | pure validator operation | no machine instruction |
| `EnsureOwned` | maybe-owned value | same type | may branch, refcount effect | emits conditional retain/ownership normalization |

Strings are not modeled as generic heap pointers because their ABI is `(ptr,
len)`, but string ownership still participates in validator checks.

## Effects

Each instruction and terminator carries an immutable `Effects` summary assigned
by the builder. Effects are conservative and PHP-observable.

```rust
pub struct Effects {
    pub reads_local: bool,
    pub writes_local: bool,
    pub reads_heap: bool,
    pub writes_heap: bool,
    pub reads_global: bool,
    pub writes_global: bool,
    pub reads_fs: bool,
    pub writes_fs: bool,
    pub reads_process: bool,
    pub writes_process: bool,
    pub output: bool,
    pub alloc_heap: bool,
    pub alloc_concat: bool,
    pub refcount_op: bool,
    pub may_throw: bool,
    pub may_fatal: bool,
    pub may_warn: bool,
    pub may_deopt: bool,
}
```

| Effect | Required when |
|---|---|
| `reads_local` / `writes_local` | Local, hidden temp, ref cell, static local, or global alias slot access. |
| `reads_heap` / `writes_heap` | Array/hash/object/mixed/callable/buffer/generator/fiber state is observed or mutated. |
| `reads_global` / `writes_global` | PHP globals, static properties, constants, runtime global symbols, or JSON error state are observed or mutated. |
| `reads_fs` / `writes_fs` | Filesystem, streams, stat cache, directory, or path APIs are used. |
| `reads_process` / `writes_process` | Environment, time, process execution, argv, sleep, or platform state are used. |
| `output` | `echo`, `print`, `printf`, `var_dump`, `print_r`, passthru/system output, or stdout/stderr writes occur. |
| `alloc_heap` | Runtime heap allocation may occur. |
| `alloc_concat` | Concat scratch buffer may be used. |
| `refcount_op` | Refcount or callable descriptor lifetime may change. |
| `may_throw` | A catchable exception may be raised. |
| `may_fatal` | A PHP fatal/value error/unhandled match/bounds/null pointer path may terminate. |
| `may_warn` | A PHP warning or diagnostic may be emitted. |
| `may_deopt` | Dynamic behavior depends on runtime type, dynamic dispatch, dynamic callback, dynamic class, or object magic behavior. |

Effect sources:

- Scalar arithmetic and comparison: hardcoded by opcode.
- Builtins: from `src/optimize/effects/builtins.rs`, broadened to full bitsets
  for EIR.
- User functions/methods/closures: from analyzed function body effects.
- Callable aliases and first-class callables: from `src/optimize/effects/calls.rs`
  and descriptor metadata.
- Extern calls: conservative unless the extern declaration later gains explicit
  purity metadata.
- Runtime calls: from an EIR runtime effect table keyed by helper name/category.

Pure means no flags set. A pure operation may be CSE'd or removed if its result
is unused. Any operation with `may_throw`, `may_fatal`, `may_warn`, `output`,
`writes_*`, `alloc_*`, or `refcount_op` is observable unless a later pass proves
otherwise.

## Instruction Set

Each instruction has:

- opcode
- operands
- result type (`None` for void)
- PHP type metadata
- ownership result
- effects
- source span

### Constants and Data

| Op | Operands | Result | Effects | Lowering |
|---|---|---|---|---|
| `ConstI64(value)` | none | `I64` | pure | target immediate or data load |
| `ConstF64(data_id)` | none | `F64` | pure | data-section load |
| `ConstStr(data_id)` | none | `Str` | pure | label plus length |
| `ConstNull` | none | `I64` or `Void` context dependent | pure | zero/null sentinel |
| `ConstBool(value)` | none | `I64` | pure | 0/1 |
| `ConstClassName(class_id)` | none | `Str` | pure | static class-string |
| `ConstEnumCase(enum_id, case_id)` | none | object or scalar | may allocate if object case | enum metadata/runtime path |
| `DataAddr(data_id)` | none | `I64` | pure | address materialization |

### Locals, Globals, and Static Storage

| Op | Operands | Result | Effects |
|---|---|---|---|
| `LoadLocal(slot)` | none | slot type | `reads_local` |
| `StoreLocal(slot, value)` | value | `Void` | `writes_local`, maybe `refcount_op` |
| `LoadRefCell(slot)` | none | value or address | `reads_local`, maybe `reads_heap` |
| `StoreRefCell(slot, value)` | value | `Void` | `writes_local`, maybe `writes_heap`, `refcount_op` |
| `LoadGlobal(name)` | none | declared type | `reads_global` |
| `StoreGlobal(name, value)` | value | `Void` | `writes_global`, maybe `refcount_op` |
| `LoadStaticLocal(slot)` | none | slot type | `reads_global` |
| `StoreStaticLocal(slot, value)` | value | `Void` | `writes_global`, maybe `refcount_op` |
| `InitStaticLocal(slot, value)` | value | `Void` | `reads_global`, `writes_global`, maybe allocation/refcount effects |
| `LoadStaticProperty(class, property)` | none | property type | `reads_global`, maybe `may_deopt` for late static |
| `StoreStaticProperty(class, property, value)` | value | `Void` | `writes_global`, maybe `refcount_op`, `may_deopt` |

### Integer, Float, and Bitwise Operations

| Op | Operands | Result | Effects |
|---|---|---|---|
| `IAdd`, `ISub`, `IMul` | `I64`, `I64` | `I64` | pure |
| `IDiv` | `I64`, `I64` | `F64` or `I64` depending on PHP operator context | `may_fatal` on invalid divisor |
| `ISDiv` | `I64`, `I64` | `I64` | `may_fatal` |
| `ISMod` | `I64`, `I64` | `I64` | `may_fatal` |
| `IPow` | `I64`, `I64` | `I64` or `F64` per PHP result | pure or libcall |
| `INeg`, `IBitNot` | `I64` | `I64` | pure |
| `IBitAnd`, `IBitOr`, `IBitXor`, `IShl`, `IShrA` | `I64`, `I64` | `I64` | pure |
| `FAdd`, `FSub`, `FMul`, `FDiv`, `FPow`, `FNeg` | `F64` operands | `F64` | pure or libcall |
| `MixedNumericBinop(add\|sub\|mul)` | mixed/union operands | mixed/scalar | `reads_heap`, maybe `alloc_heap`, `may_warn`, `may_fatal`, `may_deopt` |

`BinOp::Div` follows PHP division behavior and may produce float. `BinOp::Pow`
preserves PHP exponentiation result rules.

### Comparison and Truthiness

| Op | Operands | Result | Effects |
|---|---|---|---|
| `ICmp(predicate)` | `I64`, `I64` | `I64` bool | pure |
| `FCmp(predicate)` | `F64`, `F64` | `I64` bool | pure |
| `StrEq`, `StrCmp`, `StrLooseEq` | `Str`, `Str` | `I64` or compare int | `reads_heap` only if non-static bytes require helper reads |
| `StrictEq`, `StrictNotEq` | typed values | `I64` bool | pure for scalars, `reads_heap` for mixed/refcounted |
| `LooseEq`, `LooseNotEq`, `Spaceship` | typed values | `I64` or compare int | may coerce, `reads_heap`, `may_warn`, `may_deopt` |
| `IsNull`, `IsTruthy`, `IsEmpty` | typed value | `I64` bool | pure for scalars, `reads_heap` for mixed/containers |
| `InstanceOf` | value, target metadata | `I64` bool | `reads_heap`, `reads_global`, maybe `may_deopt` |

### Conversions and Boxing

| Op | From | To | Effects |
|---|---|---|---|
| `IToF` | `I64` | `F64` | pure |
| `FToI` | `F64` | `I64` | pure PHP truncation |
| `IToStr`, `FToStr`, `BoolToStr` | scalar | `Str` | `alloc_concat` or static string |
| `StrToI`, `StrToF`, `StrToNumber` | `Str` | scalar or mixed numeric | reads bytes, maybe `may_warn` |
| `ResourceToStr` | `I64` resource | `Str` | may allocate, may warn |
| `Cast(to_php_type)` | typed value | matching IR type | PHP cast effects |
| `MixedBox` | non-mixed value | `Heap(Mixed)` | `alloc_heap`, maybe `refcount_op` |
| `MixedUnbox(expected)` | `Heap(Mixed)` | expected storage | `reads_heap`, `may_fatal` |
| `MixedTagOf` | `Heap(Mixed)` | `I64` | `reads_heap` |
| `ArrayToMixed`, `HashToMixed` | array/hash | `Heap(Mixed)` | `alloc_heap`, `refcount_op` |
| `MixedCastBool`, `MixedCastInt`, `MixedCastFloat`, `MixedCastString` | `Heap(Mixed)` | scalar/string | `reads_heap`, maybe `alloc_concat`, `may_warn` |

### Strings

| Op | Operands | Result | Effects |
|---|---|---|---|
| `StrConcat` | `Str`, `Str` | `Str` | `alloc_concat` |
| `StrLen` | `Str` | `I64` | pure |
| `StrPersist` | `Str` | `Str` | `alloc_heap`, `refcount_op` when copy needed |
| `StrCharAt` | `Str`, `I64` | `Str` | `may_fatal`, `alloc_concat` |
| `StrInterpolate` | string parts and values | `Str` | combined value effects, `alloc_concat` |
| `ConcatReset` | none | `Void` | `writes_global` |
| `WriteStrStdout` | `Str` | `Void` | `output` |

Concat-buffer operations are statement-boundary-sensitive. Lowering must emit
statement boundaries so EIR passes do not reorder `alloc_concat` operations
across a reset point.

### Arrays and Hashes

| Op | Operands | Result | Effects |
|---|---|---|---|
| `ArrayNew(element_type, capacity)` | none | `Heap(Array)` | `alloc_heap` |
| `HashNew(key_type, value_type, capacity)` | none | `Heap(Hash)` | `alloc_heap` |
| `ArrayLen`, `HashLen` | container | `I64` | `reads_heap` |
| `ArrayGet` | array, index | element type | `reads_heap`, `may_warn`, maybe `may_fatal` |
| `HashGet` | hash, key | value type | `reads_heap`, `may_warn`, maybe `may_fatal` |
| `ArraySet` | array, index, value | `Void` | `writes_heap`, maybe `alloc_heap`, `refcount_op` |
| `HashSet` | hash, key, value | `Void` | `writes_heap`, maybe `alloc_heap`, `refcount_op` |
| `ArrayPush`, `HashAppend` | container, value | `Void` | `writes_heap`, maybe `alloc_heap`, `refcount_op` |
| `ArrayEnsureUnique`, `HashEnsureUnique` | container | same container | `reads_heap`, maybe `alloc_heap`, `refcount_op` |
| `ArrayCloneShallow`, `HashCloneShallow` | container | same kind | `alloc_heap`, `refcount_op` |
| `ArrayUnion`, `HashUnion`, `ArrayHashUnion`, `HashArrayUnion` | containers | container | `reads_heap`, `alloc_heap`, `refcount_op` |
| `ArrayToHash` | array | hash | `reads_heap`, `alloc_heap` |
| `ArrayKeyExists`, `OffsetExists` | container, key | `I64` bool | `reads_heap` |
| `OffsetUnset` | container, key | `Void` | `writes_heap`, `refcount_op` |
| `ListUnpack` | array value, slot list | `Void` | `reads_heap`, `writes_local` |

All mutating operations must preserve copy-on-write. The builder emits
`ArrayEnsureUnique`/`HashEnsureUnique` before mutation unless prior ownership
proofs make it unnecessary.

### Iterables, SPL, and Foreach

| Op | Operands | Result | Effects |
|---|---|---|---|
| `IterStart` | iterable value | iterator state | `reads_heap`, maybe `alloc_heap`, `may_deopt` |
| `IterCurrentKey` | iterator state | key value | `reads_heap`, maybe `may_deopt` |
| `IterCurrentValue` | iterator state | value | `reads_heap`, maybe `may_deopt` |
| `IterNext` | iterator state | `I64` bool | `reads_heap`, `writes_heap`, maybe `may_deopt` |
| `IterEnd` | iterator state | `Void` | maybe `refcount_op` |
| `IteratorMethodCall` | object, method metadata | typed value | method effects, `may_deopt` |
| `SplRuntimeCall` | runtime helper, args | typed value | helper effect summary |

`foreach` over arrays/hashes uses container-specific iterator operations.
`foreach` over objects uses iterator dispatch metadata and may call user code.

### Objects, Classes, Enums, and Attributes

| Op | Operands | Result | Effects |
|---|---|---|---|
| `ObjectNew(class_id, args)` | constructor args | `Heap(Object)` | `alloc_heap`, constructor effects |
| `DynamicObjectNew(class_expr, fallback, required_parent, args)` | class-string and args | `Heap(Object)` | `reads_global`, `alloc_heap`, `may_fatal`, `may_deopt` |
| `PropGet(class_id, property)` | object | property type | `reads_heap`, maybe `may_deopt` |
| `PropSet(class_id, property, value)` | object, value | `Void` | `writes_heap`, `refcount_op`, maybe `may_deopt` |
| `DynamicPropGet`, `DynamicPropSet` | object, property string/value | value or `Void` | `reads_heap`/`writes_heap`, `may_deopt`, maybe `may_warn` |
| `NullsafePropGet`, `NullsafeMethodCall` | nullable object and metadata | nullable result | branch plus underlying op effects |
| `MethodLookup` | object, method id | callable entry | `reads_heap`, `reads_global`, `may_deopt` |
| `MethodCall` | object, method id, args | return type | method effects |
| `StaticMethodCall` | receiver metadata, method id, args | return type | method effects, maybe `may_deopt` for late static |
| `ClassConstant` | receiver | `Str` | pure for fixed receivers, `may_deopt` for `static` |
| `ScopedConstantGet` | receiver, constant name | typed value | `reads_global`, maybe enum/object effects |
| `ClassAttrNames`, `ClassAttrArgs`, `ClassGetAttributes` | class metadata | arrays/objects | `alloc_heap`, `reads_global` |
| `InstanceOfDynamic` | value, runtime class-string | `I64` bool | `reads_global`, `reads_heap`, `may_deopt` |

Class, interface, trait, enum, packed-class, property, method, constant, and
attribute declarations primarily contribute metadata to the module. Method and
closure bodies lower as normal `Function` values.

### Calls and Callables

| Op | Operands | Result | Effects |
|---|---|---|---|
| `Call(function_id, args)` | normalized args | return type | callee effect summary |
| `FunctionVariantCall(group, args)` | normalized args | return type | union of variant effects |
| `BuiltinCall(name, args)` | normalized args | builtin return | builtin effect summary |
| `RuntimeCall(helper, args)` | ABI args | helper return | runtime helper effect summary |
| `ExternCall(name, args)` | C ABI args | extern return | conservative FFI effects |
| `ClosureNew` | captures | `I64` callable descriptor | `alloc_heap`, `refcount_op` |
| `ClosureCall` | descriptor/local callable, args | return type | target effect summary or conservative |
| `ExprCall` | runtime callee expression, args | return type | conservative dynamic callable effects |
| `FirstClassCallableNew` | target metadata, optional object receiver | `I64` callable descriptor | `alloc_heap`, `refcount_op` |
| `CallableArrayNew` | receiver/class/function parts | callable descriptor or array | `alloc_heap`, `refcount_op` |
| `CallableDescriptorInvoke` | descriptor, normalized mixed args | `Heap(Mixed)` or typed value | descriptor effect summary |
| `PipeCall` | value, callable | callable return | evaluates value before callable invocation |

Call argument rules are not reimplemented in each opcode. EIR lowering consumes
the shared semantic planner in `src/types/call_args.rs` and preserves the
observable order:

1. Evaluate arguments in PHP source order.
2. Preserve named/spread checks at the PHP-observable point.
3. Store required hidden temporaries before ABI materialization.
4. Materialize ABI parameters in callee signature order in the backend.
5. Avoid temp preevaluation for ref-like and mutating parameters.

### Externs, Pointers, Buffers, and Packed Data

| Op | Operands | Result | Effects |
|---|---|---|---|
| `PtrCast(target_type)` | pointer value | `I64` | pure type metadata change |
| `PtrRead(width)` | pointer | `I64` | `reads_heap`/native memory, `may_fatal` |
| `PtrWrite(width)` | pointer, value | `Void` | `writes_heap`/native memory, `may_fatal` |
| `PtrReadString`, `PtrWriteString` | pointer and string/len | string or int | native memory effects, maybe `alloc_heap`, `may_fatal` |
| `PtrOffset` | pointer, byte offset | `I64` pointer | pure arithmetic, but php_type tag preserved |
| `PtrCheckNonnull` | pointer | `Void` | `may_fatal` |
| `BufferNew(element_type, len)` | length | `Heap(Buffer)` | `alloc_heap`, `may_fatal` |
| `BufferLen` | buffer | `I64` | `reads_heap`, `may_fatal` on freed buffer |
| `BufferGet`, `BufferSet` | buffer, index, value | typed value or `Void` | `reads_heap`/`writes_heap`, bounds `may_fatal` |
| `BufferFree` | buffer | `Void` | `writes_heap`, `may_fatal` on invalid state |
| `PackedFieldGet`, `PackedFieldSet` | packed pointer, field metadata | typed value or `Void` | raw memory effects |
| `ExternGlobalLoad`, `ExternGlobalStore` | global metadata | typed value or `Void` | external state effects |

Pointer and extern memory effects are conservative because they can alias data
outside PHP's heap.

### Output, Diagnostics, Exceptions, Fibers, and Generators

| Op | Operands | Result | Effects |
|---|---|---|---|
| `EchoValue`, `PrintValue`, `WriteStdout` | value | `Void` or `I64` for print | `output`, value conversion effects |
| `VarDump`, `PrintR` | value | `Void` or string/int per builtin | `output` or `alloc_concat` |
| `ErrorSuppressBegin`, `ErrorSuppressEnd` | none | `Void` | diagnostic state effects |
| `Warn(message_id)` | args | `Void` | `may_warn`, `output`/diagnostic effect |
| `ThrowException` | exception object | terminator path | `may_throw`, `writes_global` exception state |
| `TryPushHandler`, `TryPopHandler` | handler slot metadata | `Void` | `writes_local`, `writes_global` exception stack |
| `CatchBind` | exception object, optional local | `Void` | `writes_local`, ownership transfer |
| `FinallyEnter`, `FinallyExit` | finally state | `Void` | control-flow bookkeeping effects |
| `FiberRuntimeCall` | helper, args | typed value | runtime effects, `may_throw`, `may_fatal` |
| `GeneratorNew` | frame metadata | `Heap(Object)` or generator handle | `alloc_heap` |
| `GeneratorYield` | key, value | suspend/resume value | `writes_heap`, `may_throw`, suspension effect |
| `GeneratorYieldFrom` | iterable | yielded values | iterator/generator effects |
| `GeneratorReturn` | value | `Void` | writes generator return state |

`Yield` and `YieldFrom` are not emitted through the normal expression result
path today. EIR must model them through generator state-machine lowering rather
than ordinary expression instructions.

### Include and Resolver Artifacts

| Op | Operands | Result | Effects |
|---|---|---|---|
| `IncludeOnceMark(label)` | none | `Void` | `writes_global` |
| `IncludeOnceGuard(label)` | none | branch condition | `reads_global`, `writes_global` |
| `FunctionVariantDispatch(group)` | none | `Void` | pure metadata for dispatcher emission |
| `FunctionVariantMark(name, variant)` | none | `Void` | `writes_global` marker state |

Plain `Include` should not reach EIR after resolver in normal operation. If it
does, lowering must produce a compiler error, not silently emit incomplete IR.

## Terminators

Every block ends with exactly one terminator. Terminators own control-flow
effects that cannot be represented by an instruction followed by a fallthrough.

```rust
pub enum Terminator {
    Br { target: BlockId, args: Vec<ValueId> },
    CondBr {
        cond: ValueId,
        then_target: BlockId,
        then_args: Vec<ValueId>,
        else_target: BlockId,
        else_args: Vec<ValueId>,
    },
    Switch {
        scrutinee: ValueId,
        cases: Vec<SwitchCase>,
        default: BlockId,
        default_args: Vec<ValueId>,
    },
    Return { value: Option<ValueId> },
    Throw { value: ValueId },
    Fatal { message: DataId },
    GeneratorSuspend {
        key: Option<ValueId>,
        value: Option<ValueId>,
        resume: BlockId,
        resume_args: Vec<ValueId>,
    },
    Unreachable,
}
```

| Terminator | Rules |
|---|---|
| `Br` | Destination arg count and types must match destination block params. |
| `CondBr` | `cond` must be `I64` bool/truthiness value. Both destinations receive matching args. |
| `Switch` | Cases preserve PHP `switch`/`match` semantics. Fallthrough for `switch` is represented by block structure, not implicit terminator behavior. |
| `Return` | Transfers ownership of returned owned value to caller. `Never` functions cannot return normally. |
| `Throw` | Transfers exception object to runtime exception state and does not fall through. |
| `Fatal` | Ends program path; no cleanup after fatal unless explicitly represented before it. |
| `GeneratorSuspend` | Captures generator frame state and resumes at `resume`. |
| `Unreachable` | Only allowed after proof from `Never`, fatal, throw, or impossible branch. |

`try`/`catch`/`finally` is represented as explicit handler instructions and CFG
edges to landing blocks. The validator treats active exception regions as part
of the control-flow contract.

## Textual Format

The textual format is printer-only. It is for `--emit-ir`, tests, debugging,
and review. There is no parser requirement in the v0.24.x track.

Required properties:

- deterministic function order
- deterministic block order
- stable value numbering per function
- source spans shown when available
- effects printed when non-pure
- ownership printed for values whose ownership is not obvious
- heap subkind and PHP type shown for heap/boxed values

## Using `--emit-ir`

`--emit-ir` is a diagnostic output mode:

```bash
cargo run -- --emit-ir examples/hello/main.php
```

The compiler runs the normal frontend order through type checking and AST
optimization, lowers the optimized AST to EIR, validates the module, prints the
textual format to stdout, and exits before runtime-cache preparation,
assembly generation, assembling, or linking.

`--emit-ir`, `--emit-asm`, and `--check` are mutually exclusive because each is
a terminal output mode for the same source file.

Example:

```eir
module target=macos-aarch64 {
  data str[0] = "hello\n"

  function main() -> Void flags(main) {
    entry:
      v0: Str php=string own=persistent = const_str str[0]
      echo v0                              ; effects: output
      return
  }
}
```

Example with block parameters:

```eir
function sum_to(p0: I64 php=int) -> I64 {
  entry:
    v0: I64 php=int = const_i64 0
    v1: I64 php=int = const_i64 1
    br loop(v1, v0)

  loop(i: I64 php=int, acc: I64 php=int):
    v2: I64 php=bool = icmp sle i, p0
    cond_br v2, body(i, acc), exit(acc)

  body(i: I64 php=int, acc: I64 php=int):
    v3: I64 php=int = iadd acc, i
    v4: I64 php=int = iadd i, 1
    br loop(v4, v3)

  exit(result: I64 php=int):
    return result
}
```

Textual syntax requirements:

- Function header: `function <name>(params) -> <IrType> flags(...)`.
- Block header: `<label>(params):`.
- Instruction: `vN: <IrType> php=<PhpType> own=<Ownership> = <op> ...`.
- Void instruction: `<op> ...`.
- Terminator: `return`, `br`, `cond_br`, `switch`, `throw`, `fatal`,
  `generator_suspend`, or `unreachable`.
- Effects comment: `; effects: reads_heap, writes_heap, may_throw`.
- Source span comment: `; span: line:col`.

## Validator

Validation has two modes:

- **Structural mode:** cheap, runs after builder operations and after every pass.
- **Full mode:** structural + dominance + ownership + effects, runs before
  printing for tests and before EIR backend codegen.

### Structural Rules

1. Every block belongs to exactly one function.
2. Every block has exactly one terminator.
3. Terminators appear only as the block terminator.
4. Every `ValueId` is defined exactly once.
5. Every `InstId` belongs to exactly one block.
6. Every operand references an existing value in the same function.
7. Every use is dominated by its definition, unless the value is a destination
   block parameter supplied by all incoming branches.
8. Destination block argument count matches destination block parameter count.
9. Destination block argument types match destination block parameter types.
10. Entry block has no block parameters.
11. `IrType` and `php_type` are compatible for every value.
12. Void instructions cannot be used as operands.
13. `Never`-typed PHP operations end in non-returning control flow.

### Opcode Rules

1. Integer ops accept `I64` operands only.
2. Float ops accept `F64` operands only.
3. String ops accept `Str` operands only unless the opcode explicitly casts.
4. Array ops require `Heap(Array)`.
5. Hash ops require `Heap(Hash)`.
6. Object property ops require `Heap(Object(_))`, `Heap(Mixed)`, or a dynamic
   receiver opcode that can validate at runtime.
7. Mixed ops require `Heap(Mixed)` or `Heap(Union)`.
8. Callable descriptor ops require `PhpType::Callable` metadata even when the
   storage type is `I64`.
9. Pointer ops require `PhpType::Pointer(_)` metadata.
10. Buffer ops require `Heap(Buffer(_))`.

### Ownership Rules

1. Every owned value is consumed exactly once on each reachable path.
2. `Release`, `Move`, and `Return` consume ownership.
3. Returning an owned value transfers ownership to the caller.
4. Returning a borrowed value requires an `Acquire` before `Return`.
5. `Borrow` must not outlive the source owner.
6. `Moved` values cannot be read.
7. `Persistent` values cannot be released.
8. CFG joins must merge ownership explicitly. If incoming ownership states do
   not match, the joined value is `MaybeOwned`.
9. `MaybeOwned` values must be resolved before unconditional cleanup.
10. Exception, early return, break, continue, fatal, and generator suspend paths
    must have balanced cleanup for values whose cleanup is still required.

### Effect Rules

1. Pure operations may not set hidden memory, output, diagnostic, allocation, or
   control-flow effects.
2. Operations with `output`, `may_warn`, `may_throw`, `may_fatal`, `writes_*`,
   `alloc_*`, or `refcount_op` are observable.
3. Passes cannot move `may_throw` or `may_fatal` operations across other
   observable operations unless they prove equivalent PHP behavior.
4. `alloc_concat` operations cannot cross statement-boundary concat resets.
5. `writes_global` invalidates assumptions about globals, statics, runtime
   global symbols, JSON error state, and include-once/function-variant state.
6. `writes_heap` invalidates reads of possibly aliasing arrays, hashes, objects,
   mixed cells, iterators, buffers, generators, fibers, and callable descriptors.

## Optimization Passes

After lowering and the one-shot module validation, EIR runs a fixed-point pass
driver (`src/ir_passes/driver.rs`) before codegen. The driver is where
IR-level transformations live — the rewrites the AST optimizer could not express
because they need value identity, basic blocks, or dominance.

A pass implements the `IrPass` trait: a stable `name()` and a
`run(&mut Function, &mut DataPool) -> bool` that mutates the function in place and
reports whether it changed anything. The `DataPool` is the module's shared
literal pool, threaded through so passes that materialize new constants (the
peephole string-literal fold interns the joined string) can do so; passes that
need no new literals ignore it. The driver runs the registered passes over each
function-like body (functions, methods, closures, trampolines, invokers)
repeatedly until a full sweep reports no change, capped at a fixed iteration
budget.

In debug and test builds (`debug_assertions`), the driver re-validates the
function with `validate_function` after every pass and panics — naming the
offending pass — if any pass produced malformed IR. The same builds panic if a
pass set fails to converge within the cap, which only happens for a
non-converging pass bug. Both guards compile out of `--release`, where hitting
the cap simply stops and proceeds with the current IR.

Mutating passes share the use-rewriting helper `replace_all_uses`
(`src/ir_passes/rewrite.rs`), which redirects every *use* of a value — across
instruction operands and all terminator slots — to a replacement, leaving
definitions (block parameters and instruction results) untouched.

### Identity Arithmetic Folding

The first registered transform (`src/ir_passes/identity_arith.rs`) folds
algebraic identities on integer and float arithmetic/bitwise operations using
two dominance-safe, validator-clean rewrites:

- Fold-to-operand: when the result equals an existing operand `x` (`x + 0`,
  `x * 1`, `x | 0`, `x << 0`, `x & x`, `x / 1`, `x * 1.0`, …), the instruction is
  neutralized to `nop` and its result uses are redirected to `x`. `x` was already
  an operand, so it dominates every use.
- Fold-to-zero: when the result is the integer `0` (`x ^ x`, `x - x`, `x * 0`,
  `x & 0`, `x % 1`), the instruction is rewritten in place to `const_i64 0`,
  keeping the same result value id so no use-rewrite is needed.

Only PHP-equivalent identities are folded. Integer `x / 0` and `x % 0` are left
to trap at runtime, and float additive-zero and `x * 0.0` are excluded because
signed zero and `NaN` make them observable. Fold-to-operand chains within one
sweep (`a = x + 0; b = a * 1`) are resolved transitively so a neutralized,
dead value is never used as a replacement target.

### Peephole Patterns

The second registered transform (`src/ir_passes/peephole/`) applies local
rewrites to the shape of lowered EIR. Each pattern collects rewrite intents into
a shared accumulator (a fold-to-operand RAUW map, instructions to neutralize, and
`str_concat` instructions to convert to interned `const_str`), and a single apply
phase commits them, sharing `replace_all_uses`, `resolve_chains`, and
`neutralize_to_nop` with the identity pass.

- **Box/unbox cancellation** — `unbox(box(x)) → x`, only for scalar (`NonHeap`)
  payloads with matching ir/php type, so boxing a heap value (where unbox
  extracts a borrowed reference) is never folded.
- **Redundant `move`/`borrow`** — these pure forwarding ops fold to their operand
  only when the result shares the operand's ownership and type, so RAUW cannot
  shift cleanup responsibility. (Current lowering does not emit them; the rewrite
  keeps them correct if it ever does.)
- **Load/store forwarding and dead stores** — a per-block value-numbering tracks
  the value resident in each scalar (`NonHeap`) `PhpLocal`/`HiddenTemp`/
  `NamedArgTemp` slot. A `load_local` of a slot with a known resident value folds
  to it; a `store_local` of the resident value is dropped. Any instruction naming
  the slot (unset, ref-cell promote/alias/release/store) invalidates it, and state
  resets at block boundaries — writes through aliases are never crossed. By-ref
  locals use ref cells, not plain load/store, so plain scalar slots are not
  aliased.
- **Paired acquire/release cancellation** — an `acquire` whose result is used
  exactly once, by its `release`, drops both. The single-use guard makes this
  refcount-neutral on every path regardless of distance between the two ops.
- **String-literal concat folding** — `str_concat(const_str a, const_str b)`
  interns `a ++ b` into the data pool and becomes a single `const_str` marked
  `persistent` so cleanup never frees the literal. Nested concats converge across
  driver sweeps.

### Constant Folding

The third registered transform (`src/ir_passes/const_fold.rs`) folds operations
whose operands are all compile-time constants into a single `const_*`
instruction, rewriting the instruction in place and keeping its result value id
(no use-rewrite needed). A single forward scan over the instruction table tracks
the constant carried by each value — constants in SSA are program-wide, so one
sweep discovers constant operands and collapses chains like `(2 + 3) * 4` at
once. It folds integer `iadd`/`isub`/`imul`, bitwise `and`/`or`/`xor`, in-range
(`0..=63`) `ishl`/`ishr_a`, unary `ineg`/`ibit_not`, float
`fadd`/`fsub`/`fmul`/`fneg`, signed `icmp`, and the `is_null`/`is_truthy`
predicates.

Each fold reproduces exactly what the op's lowering computes at runtime, so the
compiled result is unchanged: integers wrap at 64 bits (matching native
`add`/`sub`/`mul`), shifts fold only for in-range counts, and floats use exact
IEEE-754. The trapping integer division/modulo, float division (PHP's
`DivisionByZeroError` versus IEEE infinity), and `NaN`-sensitive `fcmp` are left
unfolded, the same conservatism as identity arithmetic.

Propagation through local slots is realized by composition with the peephole's
scalar load/store value-numbering, which forwards a constant stored to a local
onto its later `load_local` uses; this pass then folds the resulting
constant-operand operation. Together, under the fixed-point driver, they form
per-block constant propagation over EIR value ids and local slots. Constants
surfaced by identity arithmetic feed it too (`$argc * 0` → `const_i64 0` →
downstream folds), and dead constant producers are cleaned up by dead
instruction elimination.

### Common Subexpression Elimination

The fourth registered transform (`src/ir_passes/cse.rs`) removes a pure
computation whose identical predecessor already dominates it, redirecting the
redundant result to the earlier value (RAUW) and neutralizing it to `nop`. It
covers per-block and cross-block redundancy in one dominator-tree value-numbering
traversal: a scoped hash table maps each pure instruction's key
`(op, result type, immediate, canonicalized operands)` to the value that first
computed it. Because blocks are visited in dominator-tree preorder, the table
holds exactly the definitions that dominate the current block — its own earlier
instructions plus those of dominating blocks — so a match is always a dominating
value, making the redirect dominance-safe. Entries a block inserts are removed
when its whole subtree is done.

Only **pure** (`Effects::PURE`) instructions that have at least one operand and
whose result is `NonHeap` or `Persistent` are eligible: purity makes the value a
function of its operands alone (no memory/state dependence, no fault), and the
ownership restriction keeps the rewrite refcount-neutral — the same value class
dead-instruction elimination is allowed to drop. Since SSA operands are
equal-by-value, identical pure ops on identical operand values compute identical
results. Nullary constant and address materializations (`const_*`, `data_addr`)
are deliberately not deduplicated: they are cheaper to rematerialize at each use
than to keep live across the span, so CSE only targets computations.
Functions with exception handlers are skipped: their handler blocks are reachable
through implicit edges absent from the terminator graph, so a terminator-graph
dominator can be bypassed at runtime by a throw, which would make a cross-block
redirect unsound — the same restriction branch simplification uses. CSE uses the
[Dominance Analysis](#dominance-analysis) and shares
`cfg::has_exception_handlers` with branch simplification.

### Loop-Invariant Code Motion

The fifth registered transform (`src/ir_passes/licm.rs`) moves a pure computation
whose operands do not change across a loop out of the loop body into the loop
preheader, so it runs once instead of per iteration. It builds the
[loop forest](#loop-analysis) on the [dominator tree](#dominance-analysis), then
for each loop grows the invariant set to a fixed point: an instruction is
invariant when each operand is defined by another instruction being hoisted from
the same loop or has a definition that dominates the preheader.

Only **pure** (`Effects::PURE`) instructions with at least one operand and a
`NonHeap`/`Persistent` result are eligible — purity means the value depends only
on the operands and the op neither reads mutable state nor faults, so evaluating
it once in the preheader, unconditionally even when its original block ran only on
some iterations, is safe (no speculation hazard); the ownership bound keeps the
move refcount-neutral. Nullary constant/address materializations are not hoisted
(rematerializing them is cheaper than keeping them live across the loop). Loops
are processed innermost-first with moves applied immediately, so a value invariant
in several nested loops reaches the outermost preheader in one run. Instructions
are relocated between blocks' instruction lists and their result `ValueDef`s
(block + index) are recomputed once at the end so the value table matches the new
layout. Loops without a detected preheader, and functions with exception
handlers, are skipped. (PHP loop variables live in local slots reloaded through
impure `load_local`, so an invariant source expression is not yet hoistable; the
pass's reach grows as more values flow as SSA across loops.)

### Dead Instruction Elimination

The sixth registered transform (`src/ir_passes/dead_inst.rs`) removes
result-producing instructions whose values are not live over the CFG and whose
effect metadata says they are pure. It computes liveness with successor live-in
sets, initializes each block's backward walk with those live-out values plus
terminator uses, and then neutralizes dead instructions to `nop`.

Neutralization preserves instruction/result value table slots, so the validator
does not need value renumbering or block-list surgery. Read-only, allocation,
mutation, refcounting, output, warning, fatal, throw, and deopt-capable
instructions stay intact; dead read elimination is deferred until a later pass
can prove equivalent PHP and ownership behavior. Dead chains in one block
collapse during the backward walk; chains that cross block boundaries converge
through the fixed-point pass driver after liveness is recomputed.

### Dead Store Elimination

The seventh registered transform (`src/ir_passes/dead_store.rs`) removes
`store_local` instructions whose stored value is never read on any path before
the slot is overwritten or the function exits. Unlike dead instruction
elimination, which works at SSA-value granularity, this pass reasons about local
*slots*: it runs a backward dataflow that gens a slot at each `load_local` and
kills it at each `store_local`, iterating to a fixed point so live-out is the
union of successor live-in sets. A store is dead when its slot is not live
immediately after it, so an earlier store with no intervening read also dies.

The pass is restricted to slots that are (1) ordinary `PhpLocal`s, (2) of a
non-refcounted storage type (`!php_type_needs_lifetime_tracking`), (3) named only
by plain `load_local`/`store_local`, and (4) never address-escaped by reference.
The refcounting restriction is the key correctness boundary: assignment lowering
wraps a refcounted slot's store with separate `acquire`/`release` instructions
and releases the prior occupant, so dropping the `store_local` alone would leak
the acquired value. Scalar slots carry no such ownership ops and their scope-exit
cleanup is a no-op, so removing a dead scalar store is refcount-neutral. Any other
slot-naming op (ref-cell promote/alias/release, `unset_local`, static-local or
global access) makes a slot ineligible because it could read or alias the slot in
a way the pass does not model.

Condition (4) is subtle: a by-reference call argument (`new Box($v)` for a
`public int &$value` constructor) or a by-reference closure capture
(`use (&$x)`) is lowered as an ordinary `load_local`, and codegen later resolves
that argument value back to its defining `load_local` and passes the slot's
*address* — so the callee reads or mutates the slot through an alias that forward
`load_local`-only liveness cannot see. Because a single-function pass has no
callee signatures (which parameters are by-reference), the pass uses a
conservative default-deny allowlist: a slot is excluded whenever any of its
`load_local` results is consumed by an instruction that is not a proven
value-only consumer (arithmetic, comparison, cast, output, store, string, or
refcount op). Every call, object construction, closure capture, property/array
access, and any future opcode is treated as a possible by-reference escape.
Feeding a load into a value-only op first is safe because codegen only traces a
*direct* `load_local` back to a slot.

This complements the peephole pass's per-block, value-equality store forwarding
(which drops a store of the value already resident): dead store elimination is
liveness-based and crosses block boundaries, so it removes a store of a
*different* value whose result is never observed. Stores are neutralized to
`nop`; a pure value left feeding a removed store is then cleaned up by dead
instruction elimination on a later driver sweep.

### Branch Simplification

The eighth registered transform (`src/ir_passes/branch_simplify.rs`) prunes the
CFG in three ways:

- **Constant-condition folding** — a `cond_br` whose condition resolves to a
  constant (`const_bool`, `const_i64` via PHP truthiness, or `const_null`) becomes
  an unconditional `br` to the taken edge. A `switch` on a `const_i64`/`const_bool`
  scrutinee folds to a `br` to the matching case (or the default). A `while (true)`
  loop, for example, lowers to a constant `cond_br` that this fold collapses.
- **Empty-block jump threading** — a non-entry block with no parameters and only
  `nop` instructions that ends in an unconditional `br` is a forwarding block.
  Edges targeting it are redirected to the end of the forwarding chain (with cycle
  detection). Because forwarding blocks have no parameters, every edge into them
  carries empty arguments, so retargeting needs no argument rewriting.
- **Unreachable-block neutralization** — blocks no longer reachable from the entry
  have their terminator set to `Unreachable` and their instructions rewritten to
  `nop`.

Like the other passes, unreachable blocks are neutralized **in place** rather than
physically removed. The validator requires `block.id == index` and reports any
*use* in an unreachable block as `UseNotDominated` (an unreachable block's
dominator set collapses to itself). Neutralizing clears every use — terminator
and instruction operands — so the block stays valid, while the block, value, and
instruction table slots keep their indices. This avoids renumbering and, crucially,
keeps `try` handler block-id tokens (encoded in `try_push_handler` immediates)
correct. Functions that use any exception-handling opcode are skipped wholesale,
because their handler blocks are reachable through implicit edges absent from the
terminator graph, so terminator-only reachability could wrongly neutralize a live
handler. Removing edges only enlarges dominator sets and threaded forwarding blocks
carry no definitions, so simplification never invalidates a use that was valid
before; cross-block cascades converge through the fixed-point driver.

## Dominance Analysis

`src/ir_passes/dominance.rs` is a read-only sidecar analysis (like liveness, not
a driver transform) that builds each function's dominator tree, the foundation
for the dominance-aware cross-block passes that follow (common-subexpression
elimination, natural-loop detection, loop-invariant code motion).

`compute_dominance` uses the Cooper–Harvey–Kennedy iterative algorithm: it walks
reachable blocks in reverse postorder and recomputes each block's immediate
dominator as the intersection of its already-processed predecessors' idoms — a
two-finger walk over postorder numbers — until a fixed point. It converges for
arbitrary CFGs and is fast on the small functions EIR produces.

The resulting `DominanceInfo` answers `immediate_dominator`, reflexive
`dominates` / `strictly_dominates`, dominator-tree `children` (top-down
traversal), `nearest_common_dominator`, and `is_reachable`. Only blocks reachable
from the entry participate; unreachable blocks (which branch simplification
neutralizes in place but leaves in the table) are excluded from the tree and
answer `false`/`None`. The internal idom table is self-rooted at the entry so the
intersect and dominance walks terminate without special cases. The analysis uses
the shared `cfg::predecessors` helper.

## Loop Analysis

`src/ir_passes/loops.rs` is a read-only sidecar analysis that builds the
function's natural-loop forest on top of the dominator tree, the foundation for
loop-invariant code motion and other loop optimizations.

`compute_loops(func, &dominance)` first finds **back edges** — CFG edges
`latch -> header` whose target dominates their source — so loop detection is a
thin layer over dominance. Back edges sharing one header form a single
[`NaturalLoop`] with multiple latches. The loop body is the header plus every
block that can reach a latch without passing through the header, found by a
backward walk over reachable predecessors that stops at the header.

Each `NaturalLoop` exposes its `header`, `latches`, sorted `blocks` (with a
binary-search `contains`), `parent` and `depth` in the nesting forest, and its
`preheader`. **Nesting** is by block-set containment: loop `A` is nested in `B`
when `A`'s header lies in `B`'s body, and the immediate parent is the smallest
such `B`; the lowerer emits reducible CFGs, so loops are properly nested or
disjoint. `LoopInfo` additionally answers `innermost_loop`, `loop_depth`,
`is_loop_header`, and `back_edges` per block/function.

A **preheader** is detected as the unique reachable out-of-loop predecessor of
the header whose only successor is the header. PHP loops lower to slot-based CFGs
(the loop variable lives in a local slot, not a block parameter), so the init
block that branches into the header is a natural preheader; when entry into the
loop is shared between blocks or conditional, no preheader exists and an
optimization that needs one inserts it.

## AST Lowering Catalogue

Lowering must cover every variant in `src/parser/ast/expr.rs` and
`src/parser/ast/stmt.rs`. The catalogue below is exhaustive for the current
source tree.

### Supporting AST Metadata

These supporting AST enums and records do not always lower to standalone EIR
instructions, but they must be represented in metadata or consumed by lowering.

| AST surface | EIR contract |
|---|---|
| `MagicConstant::Dir`, `File`, `Function`, `Class`, `Method`, `Namespace`, `Trait` | Must be lowered before EIR to ordinary constants. Raw `MagicConstant` values are rejected by the validator. |
| `CastType::Int`, `Float`, `String`, `Bool`, `Array` | Selects `Cast(to_php_type)` or a specific scalar/string/array conversion opcode. |
| `StaticReceiver::Named`, `Self_`, `Static`, `Parent` | Stored in receiver metadata. `Named`, `Self_`, and `Parent` usually resolve before backend lowering; `Static` may require late-static runtime behavior. |
| `InstanceOfTarget::Name` | Fixed class/interface target metadata. |
| `InstanceOfTarget::Expr` | Dynamic target expression; emits runtime class-string/object target checks. |
| `CallableTarget::Function` | First-class callable or callable-array metadata for a function. |
| `CallableTarget::StaticMethod` | First-class callable or callable-array metadata for a static method and receiver. |
| `CallableTarget::Method` | First-class callable metadata for an object-bound method. |
| `UseKind::Class`, `Function`, `Const` | Name resolver consumes these before EIR; residual `UseDecl` is metadata/no-op. |
| `AttributeGroup` / `Attribute` | Carried on declaration metadata for reflection/attribute builtins; attribute argument expressions lower as metadata values where needed. |
| `EnumCaseDecl` | Enum metadata plus optional scalar/object case value. |
| `TraitUse` / `TraitAdaptation` | Type checking/name resolution flatten traits before EIR method lowering; metadata can be retained for diagnostics. |
| `ClassProperty` / `PropertyHooks` | Class metadata, property layout, readonly/final/static flags, hooks, defaults, and attributes. |
| `ClassConst` | Class/interface/trait constant metadata and value expression. |
| `ClassMethod` | Separate `Function` plus method metadata and vtable/static dispatch metadata. |
| `CType::Int`, `Float`, `Str`, `Bool`, `Void`, `Ptr`, `TypedPtr`, `Callable` | Extern ABI metadata consumed by `ExternCall`, extern globals, and callback trampolines. |
| `ExternParam` | Extern function parameter metadata. |
| `ExternField` | Extern class/struct field offset/type metadata. |
| `PackedField` | Packed class field offset/type metadata. |

### Expression Variants

| `ExprKind` | Lowering contract |
|---|---|
| `StringLiteral` | `ConstStr`; ownership `Persistent`; source span retained. |
| `IntLiteral` | `ConstI64`; `php_type = Int`. |
| `FloatLiteral` | `ConstF64`; data pool entry for exact bits. |
| `Variable` | `LoadLocal`, `LoadGlobal`, `LoadStaticLocal`, or ref-cell load depending on resolved storage. |
| `BinaryOp` | Lower left then right in source order; dispatch by `BinOp` and operand PHP types. |
| `InstanceOf` | Lower value and target; fixed target uses metadata, expression target uses dynamic class-string/runtime lookup. |
| `BoolLiteral` | `ConstBool`; `php_type = Bool`. |
| `Null` | `ConstNull`; materialized as PHP null sentinel or void context. |
| `Negate` | Lower inner; emit `INeg`, `FNeg`, or mixed numeric conversion/op. |
| `Not` | Lower inner; emit truthiness conversion and boolean negation. |
| `BitNot` | Lower inner; emit integer/string/mixed bit-not semantics. |
| `Throw` | Lower exception expression; emit `Throw` terminator or `ThrowException` path. |
| `ErrorSuppress` | Emit suppress begin/end around inner expression; preserve inner result and effects except diagnostics. |
| `Print` | Lower value, write stdout, return `ConstI64(1)`. |
| `NullCoalesce` | Build null-check CFG preserving short-circuiting. |
| `Pipe` | Lower value first, then callable expression, then single-argument call. |
| `Assignment` | Lower prelude, write target exactly once, preserve expression result via `result_target`/temp when present. |
| `PreIncrement` | Load local, increment, store, return new value. |
| `PostIncrement` | Load local, save old value, increment, store, return old value. |
| `PreDecrement` | Load local, decrement, store, return new value. |
| `PostDecrement` | Load local, save old value, decrement, store, return old value. |
| `FunctionCall` | If extern: `ExternCall`; if builtin: `BuiltinCall`; otherwise `Call`/variant dispatch after shared argument planning. |
| `ArrayLiteral` | Allocate indexed array and insert elements in source order, including spread handling. |
| `ArrayLiteralAssoc` | Allocate hash table and insert key/value pairs in source order; empty hash keeps mixed key/value metadata. |
| `Match` | Lower subject once; build strict-comparison arm CFG; default absence may `Fatal` via match-unhandled runtime. |
| `ArrayAccess` | Lower base/index; dispatch to array/hash/mixed/object ArrayAccess/buffer access as type requires. |
| `Ternary` | Lower condition, then branch CFG; result block joins branch values. |
| `ShortTernary` | Lower value once; truthy path reuses value; false path lowers default. |
| `Cast` | Lower expression then emit `Cast`/specific conversion op. |
| `Closure` | Emit callable descriptor, capture metadata, and deferred closure function body. |
| `NamedArg` | Consumed by call argument planning; standalone lowering lowers `value` only for diagnostics/fallback. |
| `Spread` | Consumed by call/array literal lowering; standalone lowering lowers inner expression only for diagnostics/fallback. |
| `ClosureCall` | Load callable local/descriptor and emit closure/descriptor invocation. |
| `ExprCall` | Lower callee expression once; dispatch runtime string, callable array, callable descriptor, closure, invokable object, or fatal path. |
| `ConstRef` | Use resolved constant metadata; non-literal constants lower their stored expression. |
| `NewObject` | Emit object allocation and constructor call for fixed class. |
| `NewDynamicObject` | Lower class-string expression; runtime class lookup, parent constraint check, object allocation, constructor call. |
| `PropertyAccess` | Fixed property read or mixed/stdClass property runtime read. |
| `DynamicPropertyAccess` | Lower object and property expression; dynamic property runtime read. |
| `NullsafePropertyAccess` | Null-check object; null path returns null, object path emits property read. |
| `NullsafeDynamicPropertyAccess` | Null-check object; object path emits dynamic property read. |
| `StaticPropertyAccess` | Fixed or late-static storage read. |
| `MethodCall` | Lower receiver and args; emit method dispatch/vtable/descriptor path. |
| `NullsafeMethodCall` | Null-check receiver; null path returns null, object path emits method call. |
| `StaticMethodCall` | Resolve receiver metadata or late-static dispatch; emit static method call. |
| `FirstClassCallable` | Emit descriptor/wrapper metadata for function, static method, or method target; object-method targets carry the evaluated receiver as the `FirstClassCallableNew` operand. |
| `This` | Load current object receiver. |
| `PtrCast` | Lower expression; preserve pointer bits and change `php_type` metadata. |
| `BufferNew` | Lower length and emit `BufferNew`. |
| `ClassConstant` | Fixed receiver emits class-string constant; `static` uses late-bound class runtime path. |
| `ScopedConstantAccess` | Resolve class constant or enum case metadata; emit scalar/object result. |
| `NewScopedObject` | Allocate `self`, `parent`, or late-static object and call constructor. |
| `MagicConstant` | Must not reach EIR; validator rejects it because magic constants are lowered earlier. |
| `Yield` | Lower through generator state-machine path; emits `GeneratorYield`/`GeneratorSuspend`. |
| `YieldFrom` | Lower iterable forwarding through generator state-machine path. |

### Binary Operators

| `BinOp` | EIR lowering |
|---|---|
| `Add`, `Sub`, `Mul`, `Div`, `Mod`, `Pow` | Numeric scalar or mixed numeric op with PHP coercions. |
| `Concat` | String coercions followed by `StrConcat`. |
| `Eq`, `NotEq` | Loose equality op, negated for `NotEq`. |
| `StrictEq`, `StrictNotEq` | Strict type-aware equality op, negated for `StrictNotEq`. |
| `Lt`, `Gt`, `LtEq`, `GtEq` | Ordered comparison with PHP coercions. |
| `And`, `Or` | Short-circuit CFG. |
| `Xor` | Truthiness of both operands, no short-circuit after both evaluated. |
| `BitAnd`, `BitOr`, `BitXor`, `ShiftLeft`, `ShiftRight` | Integer/string/mixed bitwise lowering per PHP type rules. |
| `Spaceship` | Three-way comparison op. |
| `NullCoalesce` | Same CFG semantics as `ExprKind::NullCoalesce`. |

### Statement Variants

| `StmtKind` | Lowering contract |
|---|---|
| `Echo` | Lower expression and emit stdout write. |
| `Assign` | Lower RHS, store to local/global/ref cell, update ownership. |
| `RefAssign` | Bind target ref cell to source storage; no value copy. |
| `If` | Lower condition and branch CFG, including elseif clauses and optional else. |
| `IfDef` | Should be removed by conditional pass; residual form lowers selected/resolved body or validator reports unresolved conditional. |
| `While` | Header/body/exit CFG with break/continue targets. |
| `DoWhile` | Body executes before condition; loop CFG with break/continue targets. |
| `For` | Lower optional init, condition, update, body with source-order semantics. |
| `ArrayAssign` | Load array local, ensure COW uniqueness, write index, store/update ownership. |
| `NestedArrayAssign` | Lower non-local target chain, preserve intermediate side effects, perform final write. |
| `ArrayPush` | Load array local, ensure uniqueness/growth, append value. |
| `TypedAssign` | Lower RHS, enforce/check declared type if needed, store local. |
| `Foreach` | Lower iterable once; build iterator loop, key/value binding, by-ref/by-value semantics, cleanup. |
| `Switch` | Lower subject once; build case/default CFG preserving fallthrough and `break`. |
| `Include` | Should not reach EIR after resolver; validator/compiler error if unresolved. |
| `IncludeOnceMark` | Emit include-once marker metadata/global state update. |
| `IncludeOnceGuard` | Emit guard branch around body using include-once state. |
| `Throw` | Lower expression and emit throw terminator/path. |
| `Synthetic` | Lower contained statements in order. |
| `Try` | Create try handler slot, protected body, catch landing blocks, optional finally edges. |
| `Break` | Branch to resolved loop/switch target after required cleanup. |
| `Continue` | Branch to resolved loop update/header target after required cleanup. |
| `ExprStmt` | Lower expression, discard result, release owned discarded value. |
| `NamespaceDecl` | Should be flattened by name resolver; residual statement is metadata/no-op. |
| `NamespaceBlock` | Should be flattened; residual body lowers in order. |
| `UseDecl` | Should be resolved by name resolver; residual statement is metadata/no-op. |
| `FunctionDecl` | Emit/register separate `Function`; declaration statement itself is no-op at runtime. |
| `FunctionVariantGroup` | Register include-variant dispatch metadata. |
| `FunctionVariantDispatch` | Declare an include-loaded function variant group for dispatcher emission. |
| `FunctionVariantMark` | Emit variant marker state. |
| `Return` | Lower optional expression, perform cleanup, emit `Return`. |
| `ConstDecl` | Record constant metadata; value expression retained for `ConstRef`. |
| `ListUnpack` | Lower RHS array, load elements, assign locals. |
| `Global` | Bind local names to global storage aliases. |
| `StaticVar` | Emit guarded static initialization and local alias/load. |
| `ClassDecl` | Record class metadata; lower methods separately. |
| `EnumDecl` | Record enum metadata and cases. |
| `PackedClassDecl` | Record packed layout metadata. |
| `InterfaceDecl` | Record interface metadata. |
| `TraitDecl` | Record trait metadata; trait flattening should already be type-checked. |
| `PropertyAssign` | Lower object and value, perform property write. |
| `StaticPropertyAssign` | Lower value, perform static property write. |
| `StaticPropertyArrayPush` | Load static property array/hash, ensure uniqueness, append, store. |
| `StaticPropertyArrayAssign` | Load static property array/hash, ensure uniqueness, indexed/key write, store. |
| `PropertyArrayPush` | Load object property array/hash, ensure uniqueness, append, store property. |
| `PropertyArrayAssign` | Load object property array/hash, ensure uniqueness, indexed/key write, store property. |
| `ExternFunctionDecl` | Record extern function metadata for `ExternCall`; no runtime statement. |
| `ExternClassDecl` | Record extern class/struct layout metadata. |
| `ExternGlobalDecl` | Record extern global metadata for load/store. |

## Runtime Boundary

EIR does not emit runtime helper assembly. It references runtime helpers by
symbol and category through `RuntimeCall` or specialized opcodes.

Runtime effect categories:

| Runtime category | EIR modeling |
|---|---|
| diagnostics | warnings, fatal messages, heap-debug fatals |
| strings | conversions, concat, comparisons, hashing/string builtins |
| callables | descriptor release, callable introspection, runtime invocation |
| system | argv, time/date, JSON, regex, process execution, match-unhandled |
| exceptions | throw/rethrow, cleanup frames, class/interface matching |
| generators | generator frame/state/send/throw/return helpers |
| arrays/hash/mixed/gc | heap allocation, COW, hash operations, Mixed boxing, refcount, GC |
| SPL | runtime-managed containers and iterator helpers |
| objects | stdClass, mixed property/array access, JSON object encode |
| buffers | buffer allocation, length, bounds, freed-buffer checks |
| I/O | streams, filesystem, stat/path helpers |
| pointers | C string conversion and pointer string helpers |
| fibers | stack allocation, switch, start/resume/suspend/throw/getters |

`RuntimeFeatures` remains the mechanism for optional runtime categories such as
regex. EIR lowering must set required runtime features when it emits operations
that need optional helpers.

## Target-Aware Backend Boundary

EIR does not choose physical registers, spill slots, callee-saved preservation,
stack alignment, syscall numbers, object file directives, or symbol decoration.
Those remain in `src/codegen/abi/` and platform helpers.

The EIR backend consumes:

- `Module.target`
- `IrType` and `php_type`
- call signatures and normalized argument plans
- `LocalSlot` metadata
- `DataPool`
- runtime feature requirements

Then it materializes:

- ABI argument registers/stack slots
- frame layout and hidden slots
- target-specific branch/call instructions
- runtime helper calls
- source-map comments
- instruction comments at the repository-required column

## Register Allocation

The `src/ir_passes/` module runs a linear-scan register allocator
(Poletto-Sarkar) over each function before backend lowering. It is the default;
`--regalloc=stack` (or `ELEPHC_REGALLOC=stack`) selects the original
spill-everything path, which keeps every SSA value in a stack slot.

The pass has four stages:

1. **Liveness** (`liveness.rs`): backward dataflow to a fixed point producing
   per-block live-in/live-out value sets. Block parameters are definitions at
   block entry; branch arguments are uses at the predecessor's terminator.
2. **Intervals** (`intervals.rs`): blocks are numbered in reverse postorder and
   each value gets one contiguous `[start, end]` live interval. A value live
   across edges or a loop back-edge spans the intervening positions.
3. **Scan** (`regalloc.rs`): intervals are walked in start order against an
   active set; a free register from the matching pool is assigned, otherwise the
   use-weighted spill heuristic evicts the cheapest interval. Integer and float
   values draw from separate pools.
4. **Frame integration** (`codegen_ir/frame.rs`): the allocation is stored in
   the frame layout, each used callee-saved register gets a save slot, and the
   value-access chokepoints (`load_value_to_result`, `load_value_to_reg`,
   `store_result_value`) read and write registers instead of slots.

### Caller-saved reuse for non-call-crossing intervals

The allocator distinguishes two register classes. A value whose live range
never crosses a **clobber point** — any instruction or terminator whose lowering
emits a call or touches a caller-saved register — is *call-free* and prefers a
**caller-saved** register, which needs no prologue save/restore. A value that
does live across a clobber point uses a **callee-saved** register, which survives
calls. `src/ir_passes/clobber.rs` holds the audited allowlist of volatile-safe
opcodes (constants, integer/float arithmetic, comparisons, scalar conversions);
it is safe-by-default, so an unlisted opcode merely forgoes the caller-saved
optimization rather than risking a clobbered value. The caller-saved pools are
disjoint from every register those volatile-safe lowerings touch:

| Class | aarch64 int | aarch64 float | x86_64 int | x86_64 float |
|---|---|---|---|---|
| Caller-saved | `x12`–`x15` | `d16`–`d23` | `rsi`,`rdi`,`r8`,`r9` | `xmm2`–`xmm7` |
| Callee-saved | `x21`–`x28` | `d8`–`d14` | `rbx` | (none) |

This is especially valuable on x86_64, where the callee-saved integer pool is
just `rbx` and there are no callee-saved XMM registers at all: call-free integer
and float values can now use the caller-saved pools instead of always spilling.
On x86_64 `r14` and `r15` are still never allocated — they are used as scratch by
hand-written runtime routines and shared heap-marker codegen without
ABI-compliant save/restore. A float that lives across a call on x86_64 still
spills, because no XMM register survives the call.

The spill heuristic is use-weighted: under pressure the allocator evicts the
interval with the lowest use count, breaking ties toward the furthest end (the
classic furthest-use rule), so frequently-used "hot" values keep their
registers.

The prologue saves and the epilogue restores exactly the callee-saved registers
the allocator used; caller-saved registers need neither.

The first cut register-allocates only single-word `NonHeap` scalars (`I64`,
`F64`) that are neither block parameters nor branch arguments, keeping the
slot-based block-parameter moves and the ownership/GC cleanup paths unchanged.
Generators and functions containing exception handlers fall back to all-spilled.

## Phase 02 Implementation Contract

The `src/ir/` module should implement at least:

- `types.rs`: `IrType`, `IrHeapKind`, `PhpType` mapping helpers
- `value.rs`: `ValueId`, `Value`, `ValueDef`, `Ownership`
- `instr.rs`: `Instruction`, `Op`, `InstId`, operand/result helpers
- `effects.rs`: `Effects` bitset/struct and common constructors
- `block.rs`: `BlockId`, `BasicBlock`, `Terminator`, switch case records
- `function.rs`: `Function`, `FunctionParam`, `LocalSlot`, flags
- `module.rs`: `Module`, data pool, metadata records, extern declarations
- `builder.rs`: checked construction API that assigns values/instructions
- `validator.rs`: structural, opcode, dominance, ownership, and effect checks
- `print.rs`: deterministic textual printer

Phase 02 does not need AST lowering or assembly codegen. Its unit tests can
construct modules by hand and validate/print them.

## Closure Audit

This specification closes the first EIR roadmap item because it defines:

- the EIR type lattice and exact mapping from every current `PhpType`
- module/function/block/value/local structures
- instruction families and representative opcodes for all current expression,
  statement, builtin, runtime, ownership, exception, generator, fiber, pointer,
  buffer, packed, object, callable, and include/resolver surfaces
- all terminators, including generator suspension
- the effect lattice and effect sources
- ownership states and ownership validation rules
- textual format requirements and examples
- an exhaustive current `ExprKind`, `BinOp`, and `StmtKind` lowering catalogue
- validator requirements sufficient for Phase 02
