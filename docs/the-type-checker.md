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
}
```

This is simpler than PHP's runtime types — no union types, no mixed, no nullable syntax. Each variable gets exactly one type for its lifetime. The distinction between `Array` (indexed) and `AssocArray` (key-value) is determined at compile time from the literal syntax (`[1, 2]` vs `["a" => 1]`).

## How inference works

The type checker walks the AST top-down, maintaining a **type environment** — a `HashMap<String, PhpType>` that maps variable names to their types.

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
5. **Stores the `FunctionSig`** — parameter count, parameter types, return type

This information is then used when checking calls to that function.

## The global environment

Before checking user code, the type checker pre-populates the environment with built-in globals:

```rust
global_env.insert("argc", PhpType::Int);
global_env.insert("argv", PhpType::Array(Box::new(PhpType::Str)));
```

These correspond to PHP's `$argc` and `$argv` superglobals.

## Output: CheckResult

The type checker produces a `CheckResult`:

```rust
pub struct CheckResult {
    pub global_env: TypeEnv,                    // variable name → type
    pub functions: HashMap<String, FunctionSig>, // function name → signature
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
