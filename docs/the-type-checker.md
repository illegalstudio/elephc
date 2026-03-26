# The Type Checker

[← Back to Wiki](README.md) | Previous: [The Parser](the-parser.md) | Next: [The Code Generator →](the-codegen.md)

---

**Source:** `src/types/` — `mod.rs`, `checker/mod.rs`, `checker/builtins.rs`, `checker/functions.rs`

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
    Array(Box<PhpType>),           // e.g., Array(Int) = int[]
    AssocArray {                    // e.g., AssocArray { key: Str, value: Int }
        key: Box<PhpType>,
        value: Box<PhpType>,
    },
    Callable,                      // closures and function references
    Object(String),                // class instance, e.g., Object("Point")
}
```

This is simpler than PHP's runtime types — no union types, no mixed, no nullable syntax. Each variable gets exactly one type for its lifetime. The distinction between `Array` (indexed) and `AssocArray` (key-value) is determined at compile time from the literal syntax (`[1, 2]` vs `["a" => 1]`).

`Callable` is used for anonymous functions (closures) and arrow functions. A callable value is stored as a function pointer (8 bytes) on the stack, and is invoked via an indirect branch (`blr`).

`Object(String)` represents a class instance. The string carries the class name (e.g., `"Point"`). Objects are heap-allocated pointers (8 bytes on the stack).

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

**File:** `src/types/checker/builtins.rs`

Every built-in function has a registered type signature:

```
strlen($str: Str) → Int
substr($str: Str, $start: Int, $len?: Int) → Str
strpos($hay: Str, $needle: Str) → Int
count($arr: Array) → Int
abs($val: Int|Float) → Int|Float
floor($val: Float) → Float
rand($min?: Int, $max?: Int) → Int
```

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

## Class type checking

When the type checker encounters a `ClassDecl`, it:

1. **Registers the class** in a `classes: HashMap<String, ClassInfo>` map
2. **Records each property** with its type (inferred from default values or constructor assignments)
3. **Type-checks each method body** with `$this` bound to `Object(ClassName)`
4. **Builds `ClassInfo`** containing property types, defaults, method signatures, static method signatures, and constructor-to-property mappings

The `ClassInfo` struct:

```rust
pub struct ClassInfo {
    pub properties: Vec<(String, PhpType)>,
    pub defaults: Vec<Option<Expr>>,
    pub methods: HashMap<String, FunctionSig>,
    pub static_methods: HashMap<String, FunctionSig>,
    pub constructor_param_to_prop: Vec<Option<String>>,
}
```

When checking property access (`$obj->prop`), the type checker validates that:
- The variable is an `Object` type
- The class has a property with that name
- The property is accessible (public, or private and accessed via `$this`)

When checking method calls, it verifies the method exists and validates argument count and types against the method's `FunctionSig`.

## Output: CheckResult

The type checker produces a `CheckResult`:

```rust
pub struct CheckResult {
    pub global_env: TypeEnv,                    // variable name → type
    pub functions: HashMap<String, FunctionSig>, // function name → signature
    pub classes: HashMap<String, ClassInfo>,     // class name → class info
}
```

This is passed to the [code generator](the-codegen.md), which uses it to:
- Allocate the right amount of stack space per variable
- Choose the correct registers and instructions
- Emit proper type coercions

## Error examples

```php
$x = 42;
$x = "hello";
// Error: Type error: cannot reassign $x from Int to Str

strlen(42);
// Error: strlen() expects string, got Int

unknown_func();
// Error: Call to undefined function unknown_func()

substr("hello");
// Error: substr() expects at least 2 arguments, got 1
```

Each error includes the exact line and column, thanks to the `Span` carried through from the [lexer](the-lexer.md).

---

Next: [The Code Generator →](the-codegen.md)
