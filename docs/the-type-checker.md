# The Type Checker

[← Back to Wiki](README.md) | Previous: [The Parser](the-parser.md) | Next: [The Code Generator →](the-codegen.md)

---

**Source:** `src/types/` — `mod.rs`, `traits.rs`, `checker/mod.rs`, `checker/builtins.rs`, `checker/functions.rs`

PHP is dynamically typed — variables can change type at runtime. But elephc compiles to native code where every value must have a known size and location. The type checker bridges this gap by **inferring types at compile time**.

## Why type checking matters

The [code generator](the-codegen.md) needs to know types to emit correct assembly:

- An `Int` lives in register `x0` (8 bytes)
- A `Float` lives in register `d0` (8 bytes)
- A `String` lives in `x1` (pointer) + `x2` (length) = 16 bytes

If the code generator doesn't know whether `$x` is an integer or a string, it doesn't know which registers to use, how many bytes to allocate on the stack, or which comparison instruction to emit (`cmp` for integers vs `fcmp` for floats).

## The type system

**File:** `src/types/mod.rs`

elephc has a small type system:

```rust
pub enum PhpType {
    Int,
    Float,
    Str,
    Bool,
    Void,                          // null
    Mixed,                         // runtime-boxed heterogeneous assoc-array value
    Array(Box<PhpType>),           // e.g., Array(Int) = int[]
    AssocArray {                    // e.g., AssocArray { key: Str, value: Int }
        key: Box<PhpType>,
        value: Box<PhpType>,
    },
    Callable,                      // closures and function references
    Object(String),                // class instance, e.g., Object("Point") or Object("App\\Point")
    Pointer(Option<String>),       // opaque ptr or typed ptr<Class>
}
```

This is simpler than PHP's surface syntax — there are still no user-written union types or nullable annotations. Each variable gets exactly one static type for its lifetime, but associative-array values can widen to the internal `Mixed` type when later entries do not match the first value type. The distinction between `Array` (indexed) and `AssocArray` (key-value) is determined at compile time from the literal syntax (`[1, 2]` vs `["a" => 1]`).

`Callable` is used for anonymous functions (closures) and arrow functions. A callable value is stored as a function pointer (8 bytes) on the stack, and is invoked via an indirect branch (`blr`).

`Object(String)` represents a class instance. The string carries the canonical class name after name resolution (for example `"Point"` or `"App\\Point"`). Objects are heap-allocated pointers (8 bytes on the stack).

`Pointer(Option<String>)` represents a raw 64-bit address. `Pointer(None)` is an opaque pointer, while `Pointer(Some("Point"))` is a pointer tagged with a checked pointee type. The tag affects static checking, but the runtime value is still just an address in `x0`.

## How inference works

The type checker walks the AST top-down, maintaining a **type environment** — a `HashMap<String, PhpType>` that maps variable names to their types. It also tracks a **constants map** — a `HashMap<String, PhpType>` that records the type of each user-defined constant (declared via `const` or `define()`).

### Assignments create types

```php
$x = 42;          // $x: Int (inferred from the literal)
$name = "Alice";   // $name: Str
$pi = 3.14;       // $pi: Float
$ok = true;       // $ok: Bool
$nothing = null;   // $nothing: Void
```

The first assignment determines a variable's type. After that, reassignment is only allowed to the same type (with some exceptions):

### Type compatibility rules

| From | To | Allowed? |
|---|---|---|
| `Int` | `Int` | Yes |
| `Int` | `Float` | Yes (numeric types are interchangeable) |
| `Int` | `Bool` | Yes (numeric/bool interchangeable) |
| `Int` | `Str` | **No** — compile error |
| `Void` | anything | Yes (null can become any type) |
| anything | `Void` | Yes (any variable can become null) |
| `AssocArray(_, T)` | `AssocArray(_, U)` | Yes, if `T` and `U` merge; heterogeneous values widen to `Mixed` |
| `Pointer(None)` | `Pointer(Some("T"))` | Yes (merged to the more specific pointer tag) |
| `Pointer(Some("A"))` | `Pointer(Some("B"))` | Yes, but merged to opaque `Pointer(None)` if tags differ |
| `Pointer(*)` | `Int` / `Str` / `Array` | **No** — compile error |

This means elephc rejects code that PHP would allow:

```php
$x = 42;
$x = "hello";  // ← Type error: cannot reassign $x from Int to Str
```

This is intentional — it lets the compiler know exactly what `$x` is at every point, without needing runtime type tags.

## Expression type inference

The type checker computes the type of every expression:

### Literals

| Expression | Type |
|---|---|
| `42` | `Int` |
| `3.14` | `Float` |
| `"hello"` | `Str` |
| `true` / `false` | `Bool` |
| `null` | `Void` |
| `[1, 2, 3]` | `Array(Int)` |
| `["a" => 1]` | `AssocArray { key: Str, value: Int }` |
| `["a" => 1, "b" => "two"]` | `AssocArray { key: Str, value: Mixed }` |

### Binary operations

| Operation | Types | Result |
|---|---|---|
| `Int + Int` | arithmetic | `Int` |
| `Float + Float` | arithmetic | `Float` |
| `Int + Float` | mixed arithmetic | `Float` |
| `Int / Int` | division | `Float` (always — matches PHP) |
| `Int % Int` | modulo | `Int` |
| `Str . Str` | concatenation | `Str` |
| `Int . Str` | concat with coercion | `Str` |
| `Int > Int` | comparison | `Bool` |
| `Bool && Bool` | logical | `Bool` |
| `Int & Int` | bitwise | `Int` |
| `Int <=> Int` | spaceship | `Int` (-1, 0, or 1) |
| `expr ?? expr` | null coalescing | Type of the non-null operand |

### Function calls

Built-in functions have hardcoded type signatures (see below). User-defined functions have their return type inferred from the `return` statements in their body.

## Built-in function signatures

**Files:** `src/types/checker/builtins.rs`, plus `src/types/checker/mod.rs` for special expression forms such as `ExprKind::PtrCast`

Every built-in function has a registered type signature:

```
strlen($str: Str) → Int
substr($str: Str, $start: Int, $len?: Int) → Str
strpos($hay: Str, $needle: Str) → Int
count($arr: Array|AssocArray) → Int
abs($val: Int|Float) → Int|Float
floor($val: Int|Float) → Float
rand($min?: Int, $max?: Int) → Int
ptr($var: lvalue) → Pointer(None)
ptr_get($ptr: Pointer) → Int
ptr_set($ptr: Pointer, $value: Int|Bool|Void|Pointer) → Void
ptr_cast<T>($ptr: Pointer) → Pointer(Some(T))
```

Most entries in the table above come from the builtin signature registry, while pointer-tag casts like `ptr_cast<T>()` are checked directly when the type checker visits `ExprKind::PtrCast`. For some built-ins the checker also enforces container shape, not just raw argument count:

- `array_push($arr, $val)` requires the first argument to be an indexed `Array`, not an `AssocArray`
- `array_column($rows, $column_key)` requires the first argument to be an indexed array whose element type is `AssocArray`
- `wordwrap()` accepts 1 to 4 arguments, matching the builtin checker

The type checker validates:
1. **Argument count** — too few or too many arguments → error
2. **Argument types** — wrong types → error (in some cases; many builtins accept multiple types)
3. **Return type** — used to infer the type of the call expression

## User-defined function checking

**File:** `src/types/checker/functions.rs`

When the type checker encounters a function declaration, it:

1. **Collects all function declarations** in a first pass (so functions can be called before they're defined)
2. **Creates a local type environment** for the function body (separate from global scope)
3. **Infers parameter types** from how they're used in the body
4. **Infers return type** from `return` expressions
5. **Stores the `FunctionSig`** — parameter count, parameter types, return type, reference parameters, and variadic parameter

The `FunctionSig` struct:

```rust
pub struct FunctionSig {
    pub params: Vec<(String, PhpType)>,
    pub defaults: Vec<Option<Expr>>,
    pub return_type: PhpType,
    pub ref_params: Vec<bool>,         // which parameters are pass-by-reference (&$param)
    pub variadic: Option<String>,      // variadic parameter name (...$args), if any
}
```

- `ref_params` tracks which parameters use `&` (pass by reference). The codegen passes the stack address of the argument instead of its value.
- `variadic` holds the name of the variadic parameter (e.g., `$args` in `function foo(...$args)`). Extra arguments beyond the regular parameters are collected into an array.

This information is then used when checking calls to that function.

## The global environment

Before checking user code, the type checker pre-populates the environment with built-in globals:

```rust
global_env.insert("argc", PhpType::Int);
global_env.insert("argv", PhpType::Array(Box::new(PhpType::Str)));
```

These correspond to PHP's `$argc` and `$argv` superglobals.

## Interface type checking

Before `ClassInfo` is built, the checker flattens trait composition through `src/types/traits.rs`, builds `InterfaceInfo` entries for every interface, and only then builds class metadata recursively.

```rust
pub struct InterfaceInfo {
    pub interface_id: u64,
    pub parents: Vec<String>,
    pub methods: HashMap<String, FunctionSig>,
    pub method_declaring_interfaces: HashMap<String, String>,
    pub method_order: Vec<String>,
    pub method_slots: HashMap<String, usize>,
}
```

For each interface, the checker resolves `interface extends interface` transitively, rejects inheritance cycles, flattens required methods into a single signature map, and assigns a stable method ordering used by runtime metadata emission.

## Class type checking

After interfaces are known, the checker builds each class so it sees parent-first property layout, inherited method signatures, abstract obligations, implemented interface contracts, and vtable slot assignments.

When the type checker encounters a `ClassDecl`, it:

1. **Registers the class** in a `classes: HashMap<String, ClassInfo>` map
2. **Resolves the parent chain** (`extends`) and merges inherited metadata
3. **Records each property** with its type (inferred from default values or constructor assignments) and a fixed offset in the inherited object layout
4. **Type-checks each method body** with `$this` bound to `Object(ClassName)`
5. **Builds `ClassInfo`** containing property types, defaults, signatures, declaring/implementation class maps, instance/static vtable slots, implemented interface lists, and constructor-to-property mappings

The `ClassInfo` struct:

```rust
pub struct ClassInfo {
    pub class_id: u64,
    pub parent: Option<String>,
    pub is_abstract: bool,
    pub properties: Vec<(String, PhpType)>,
    pub property_offsets: HashMap<String, usize>,
    pub property_declaring_classes: HashMap<String, String>,
    pub defaults: Vec<Option<Expr>>,
    pub property_visibilities: HashMap<String, Visibility>,
    pub readonly_properties: HashSet<String>,
    pub method_decls: Vec<ClassMethod>,
    pub methods: HashMap<String, FunctionSig>,
    pub static_methods: HashMap<String, FunctionSig>,
    pub method_visibilities: HashMap<String, Visibility>,
    pub method_declaring_classes: HashMap<String, String>,
    pub method_impl_classes: HashMap<String, String>,
    pub vtable_methods: Vec<String>,
    pub vtable_slots: HashMap<String, usize>,
    pub static_method_visibilities: HashMap<String, Visibility>,
    pub static_method_declaring_classes: HashMap<String, String>,
    pub static_method_impl_classes: HashMap<String, String>,
    pub static_vtable_methods: Vec<String>,
    pub static_vtable_slots: HashMap<String, usize>,
    pub interfaces: Vec<String>,
    pub constructor_param_to_prop: Vec<Option<String>>,
}
```

`vtable_methods` / `vtable_slots` drive ordinary inherited instance dispatch, while `static_vtable_methods` / `static_vtable_slots` carry the parallel metadata used by `static::method()` late static binding.

For abstract methods, the checker keeps the inherited signature but intentionally leaves the implementation-class entry unset until a concrete subclass provides a body. Concrete classes are rejected if any abstract or interface requirement remains unresolved after inheritance + trait flattening + interface conformance checks.

When checking property access (`$obj->prop`), the type checker validates that:
- The variable is an `Object` type
- The class has a property with that name
- The property is accessible (`public`, `protected` from the declaring class or a subclass, or `private` only from the declaring class)

When checking method calls, it verifies the method exists, enforces method visibility (`public`, subclass-visible `protected`, declaring-class-only `private`), validates argument count and types against the method's `FunctionSig`, resolves `parent::method()` against the immediate parent class, resolves `self::method()` against the current lexical class, and accepts `static::method()` as a late-static-bound static call against the current class hierarchy.

When checking `new ClassName(...)`, it also rejects interfaces and abstract classes before codegen.

## Output: CheckResult

The type checker produces a `CheckResult`:

```rust
pub struct CheckResult {
    pub global_env: TypeEnv,                    // variable name → type
    pub functions: HashMap<String, FunctionSig>, // function name → signature
    pub interfaces: HashMap<String, InterfaceInfo>, // interface name → interface info
    pub classes: HashMap<String, ClassInfo>,     // class name → class info
    pub extern_functions: HashMap<String, ExternFunctionSig>,
    pub extern_classes: HashMap<String, ExternClassInfo>,
    pub extern_globals: HashMap<String, PhpType>,
    pub required_libraries: Vec<String>,
}
```

This is passed to the [code generator](the-codegen.md), which uses it to:
- Allocate the right amount of stack space per variable
- Choose the correct registers and instructions
- Emit proper type coercions
- Carry FFI declarations and linker requirements into codegen

## Error examples

```php
$x = 42;
$x = "hello";
// Error: Type error: cannot reassign $x from Int to Str

strlen(42);
// Error: strlen() expects string, got Int

unknown_func();
// Error: Undefined function: unknown_func

substr("hello");
// Error: substr() takes 2 or 3 arguments
```

Each error includes the exact line and column, thanks to the `Span` carried through from the [lexer](the-lexer.md).

---

Next: [The Code Generator →](the-codegen.md)
