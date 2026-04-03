---
title: "The Parser"
description: "How tokens become an AST with Pratt parsing."
sidebar:
  order: 4
---

**Source:** `src/parser/` — `expr.rs`, `stmt.rs`, `control.rs`, `ast.rs`, `mod.rs`

The parser takes the token stream from the [lexer](the-lexer.md) and builds an **Abstract Syntax Tree** (AST) — a tree structure that represents the program's meaning, not just its text.

## What is an AST?

An AST strips away syntactic noise (parentheses, semicolons, braces) and captures the **structure** of the program:

```php
echo 1 + 2 * 3;
```

The tokens are flat: `Echo, Int(1), Plus, Int(2), Star, Int(3), Semicolon`. But the AST is a tree:

```
Echo
 └── BinaryOp(Add)
      ├── IntLiteral(1)
      └── BinaryOp(Mul)
           ├── IntLiteral(2)
           └── IntLiteral(3)
```

The tree encodes that `2 * 3` happens before `+ 1` — **operator precedence** is baked into the structure. The parser is responsible for getting this right.

## The AST types

**File:** `src/parser/ast.rs`

The AST has two main node types:

### Expressions (`Expr`)

Things that have a value:

| Variant | Example | Notes |
|---|---|---|
| `IntLiteral(i64)` | `42` | |
| `FloatLiteral(f64)` | `3.14` | |
| `StringLiteral(String)` | `"hello"` | Escapes already resolved by lexer |
| `BoolLiteral(bool)` | `true`, `false` | |
| `Null` | `null` | |
| `Variable(String)` | `$x` | Name without `$` |
| `BinaryOp { left, op, right }` | `$a + $b` | See operator table below |
| `Negate(Expr)` | `-$x` | Unary minus |
| `Not(Expr)` | `!$x` | Logical NOT |
| `BitNot(Expr)` | `~$x` | Bitwise NOT (complement) |
| `Throw(Expr)` | `throw new Exception("boom")` | Throw expression node used both in statements and expression positions such as `??` or ternaries |
| `NullCoalesce { value, default }` | `$x ?? $y` | Returns `$x` if non-null, otherwise `$y` |
| `PreIncrement(String)` | `++$i` | Returns new value |
| `PostIncrement(String)` | `$i++` | Returns old value |
| `PreDecrement(String)` | `--$i` | |
| `PostDecrement(String)` | `$i--` | |
| `FunctionCall { name, args }` | `strlen($s)`, `Tools\fmt($s)`, `\strlen($s)` | Parsed as a structured name so later phases can resolve namespace aliases and fully-qualified names |
| `ArrayLiteral(Vec<Expr>)` | `[1, 2, 3]`, `[...$arr, 4]` | Indexed array; elements may include `Spread` expressions |
| `ArrayLiteralAssoc(Vec<(Expr, Expr)>)` | `["a" => 1]` | Associative array |
| `Match { subject, arms, default }` | `match($x) { 1, 2 => "low", 3 => "high" }` | Match expression (returns a value). `arms` is `Vec<(Vec<Expr>, Expr)>`, so each arm can have multiple comma-separated patterns before `=>`, and `default` is optional (`Option<Box<Expr>>`) |
| `ArrayAccess { array, index }` | `$arr[0]`, `$str[-1]` | Same AST node is used for indexed arrays, associative-array lookups, and string indexing |
| `Ternary { condition, then_expr, else_expr }` | `$a ? $b : $c` | |
| `Cast { target, expr }` | `(int)$x` | |
| `Closure { params, variadic, body, is_arrow, captures }` | `function(int $x = 1) use ($y) { ... }` or `fn(int $x): int => $x * 2` | Anonymous function / arrow function. Params is `Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>` — name, declared type, default, is_ref. `variadic` is an optional parameter name. `captures` is `Vec<String>` — variables captured via an explicit `use (...)` clause. Arrow functions are still represented as `Closure`, parse with `is_arrow = true`, and do not carry explicit `use (...)` captures in the AST. |
| `NamedArg { name, value }` | `foo(name: "Alice")` | Named call argument. Later phases reorder these against the declared parameter list. |
| `ClosureCall { var, args }` | `$fn(1, 2)` | Calling a closure stored in a variable |
| `ExprCall { callee, args }` | `$arr[0](1, 2)` | Calling the result of an expression (e.g., array access returning a callable) |
| `Spread(Expr)` | `...$arr` | Spread/unpack operator — expands an array into individual arguments or elements |
| `ConstRef(Name)` | `MAX_RETRIES`, `Config\PORT`, `\App\Config\PORT` | Reference to a user-defined constant |
| `EnumCase { enum_name, case_name }` | `Color::Red`, `App\Status::Ok` | Reference to a declared enum case before later phases lower it to enum metadata |
| `NewObject { class_name, args }` | `new Point(1, 2)`, `new App\Model\User()` | Object instantiation |
| `PropertyAccess { object, property }` | `$p->x` | Property access via `->` |
| `MethodCall { object, method, args }` | `$p->move(1, 2)` | Instance method call |
| `StaticMethodCall { receiver, method, args }` | `Point::origin()`, `self::boot()`, `parent::boot()`, `static::boot()` | Static-style call via `::`, where `receiver` is a named class, `Self_`, `Static`, or `Parent` |
| `FirstClassCallable(CallableTarget)` | `strlen(...)`, `Tools\fmt(...)`, `Math::twice(...)` | PHP-style first-class callable syntax; the target is preserved structurally instead of being parsed as a call |
| `This` | `$this` | Reference to the current object inside a method |
| `PtrCast { target_type, expr }` | `ptr_cast<Point>($p)` | Pointer-tag cast parsed specially after `ptr_cast<T>` |
| `BufferNew { element_type, len }` | `buffer_new<int>(256)` | Compiler extension for contiguous hot-path buffers |

### Statements (`Stmt`)

Things that do something:

| Variant | Example |
|---|---|
| `Echo(Expr)` | `echo $x;` |
| `Assign { name, value }` | `$x = 42;` |
| `If { condition, then_body, elseif_clauses, else_body }` | `if (...) { } elseif (...) { } else { }` |
| `While { condition, body }` | `while (...) { }` |
| `DoWhile { body, condition }` | `do { } while (...);` |
| `For { init, condition, update, body }` | `for (...; ...; ...) { }` — `init`, `condition`, and `update` are all optional, so `for (;;) { }` is valid |
| `Foreach { array, key_var, value_var, body }` | `foreach ($arr as $v) { }` or `foreach ($arr as $k => $v) { }` |
| `Switch { subject, cases, default }` | `switch ($x) { case 1: ...; default: ... }` |
| `ArrayAssign { array, index, value }` | `$arr[0] = 5;` |
| `ArrayPush { array, value }` | `$arr[] = 5;` |
| `TypedAssign { type_expr, name, value }` | `int $x = 42;`, `buffer<int> $xs = buffer_new<int>(8);` |
| `FunctionDecl { name, params, variadic, return_type, body }` | `function foo(int $a, &$b, string $c = "x"): string { }` — params is `Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>` where the tuple stores name, declared type, default value, and `is_ref` (pass by reference). `variadic` is `Option<String>` for variadic parameters (`...$args`) and `return_type` is an optional declared `TypeExpr` |
| `Return(Option<Expr>)` | `return $x;` or `return;` |
| `Break` | `break;` |
| `Continue` | `continue;` |
| `Include { path, once, required }` | `include 'file.php';` |
| `Throw(Expr)` | `throw new Exception("boom");` |
| `Try { try_body, catches, finally_body }` | `try { ... } catch (Exception $e) { ... } finally { ... }` |
| `ConstDecl { name, value }` | `const MAX = 100;` |
| `IfDef { symbol, then_body, else_body }` | `ifdef DEBUG { ... } else { ... }` |
| `NamespaceDecl { name: Option<Name> }` | `namespace App\Core;`, `namespace;` |
| `NamespaceBlock { name: Option<Name>, body }` | `namespace App\Core { ... }`, `namespace { ... }` |
| `UseDecl { imports }` | `use App\Lib\Tool;`, `use function App\fn as helper;`, `use Vendor\Pkg\{Thing, Other as Alias};` |
| `ListUnpack { vars, value }` | `[$a, $b] = [1, 2];` |
| `Global { vars }` | `global $x, $y;` — declares variables as referencing global storage |
| `StaticVar { name, init }` | `static $count = 0;` — declares a variable that persists across function calls |
| `ClassDecl { name, extends, implements, is_abstract, is_readonly_class, trait_uses, properties, methods }` | `abstract readonly class Point extends Shape implements Named { use NamedTrait; ... }` |
| `EnumDecl { name, backing_type, cases }` | `enum Status: int { case Ok = 1; case Err = 2; }` |
| `PackedClassDecl { name, fields }` | `packed class Vec2 { public float $x; public float $y; }` |
| `InterfaceDecl { name, extends, methods }` | `interface Named extends Jsonable { public function name(); }` |
| `TraitDecl { name, trait_uses, properties, methods }` | `trait Named { ... }` |
| `PropertyAssign { object, property, value }` | `$p->x = 10;` |
| `ExternFunctionDecl { name, params, return_type, library }` | `extern function foo(int $x): int;` or entries inside `extern "lib" { ... }` — `params` is `Vec<ExternParam>`, where each `ExternParam` stores `{ name, c_type }`, and `return_type` is a `CType` |
| `ExternClassDecl { name, fields }` | `extern class Point { public int $x; }` |
| `ExternGlobalDecl { name, c_type }` | `extern global ptr $environ;` — the declared type is a C-facing `CType`, not a `PhpType` |
| `ExprStmt(Expr)` | `my_func();` (expression used as statement) |

### Statement dispatch

At statement level, parsing is split between `parser/mod.rs` and `stmt.rs`:

- `parse()` in `mod.rs` special-cases `extern` so one `extern "lib" { ... }` block can expand into multiple AST statements.
- Everything else flows through `stmt::parse_stmt()`, which selects the parser entry point from the current token.

| Current token | Parse as |
|---|---|
| `Class` / `Abstract Class` / `Readonly Class` / `Abstract Readonly Class` | Class declaration |
| `Enum` | Enum declaration |
| `Packed` | Packed-class declaration |
| `Interface` | Interface declaration |
| `Trait` | Trait declaration |
| `Function` | Function declaration |
| `Namespace` | Namespace declaration |
| `Use` | Namespace import declaration |
| `Return` | Return statement |
| `Throw` | Throw statement |
| `Echo` / `Print` | Echo/print statement |
| `If` / `While` / `Do` / `For` / `Foreach` / `Switch` / `Try` | Control-flow statement |
| `Const` / `Global` / `Static` | Declaration-like statement |
| `Variable` / `This` / `Identifier` / `Backslash` / `Self_` / `Parent` / `Static::...` | Assignment, property write, call, or generic expression statement |

This is intentionally narrower than full PHP statement syntax. In the current subset, expression statements only enter through the token arms handled by `stmt::parse_stmt()` above; starting a statement with tokens such as `match`, `new`, `fn`, a literal, `(`, or a unary operator still produces an "unexpected token at statement position" parser error unless that construct appears inside another statement form.

## Error recovery

The parser does not stop at the first syntax error anymore. It now performs conservative synchronization at statement boundaries and block boundaries so one malformed statement does not necessarily prevent later statements from being parsed and reported.

Current recovery behavior is intentionally simple:
- top-level parsing can skip forward to the next plausible statement boundary after a syntax error
- block parsing (`{ ... }`) can resynchronize on `;`, `}`, and `EOF`
- the parser still prefers correctness over aggressive recovery, so heavily malformed input may still collapse into fewer diagnostics than an IDE-style parser would produce

### Binary operators (`BinOp`)

```
Add  Sub  Mul  Div  Mod  Pow  Concat
Eq  NotEq  StrictEq  StrictNotEq  Lt  Gt  LtEq  GtEq  Spaceship
And  Or
BitAnd  BitOr  BitXor  ShiftLeft  ShiftRight
NullCoalesce
```

### Class-related types

`ClassDecl` uses several supporting types:

| Type | Fields | Description |
|---|---|---|
| `Visibility` | `Public`, `Protected`, `Private` | Enum for property/method visibility |
| `ClassProperty` | `name`, `visibility`, `readonly`, `default`, `span` | A property declaration inside a class |
| `ClassMethod` | `name`, `visibility`, `is_static`, `is_abstract`, `has_body`, `params`, `variadic`, `return_type`, `body`, `span` | A method declaration inside a class, trait, or interface |
| `CatchClause` | `exception_types`, `variable`, `body` | A catch arm. `exception_types` supports both single-type and PHP-style multi-catch (`TypeA | TypeB`), and `variable` is optional for PHP 8-style `catch (Exception)` |
| `StaticReceiver` | `Named(Name)`, `Self_`, `Static`, `Parent` | Left-hand side of `ClassName::method()`, `self::method()`, `static::method()`, and `parent::method()` |
| `TraitUse` | `trait_names`, `adaptations`, `span` | A `use TraitA, TraitB { ... }` clause inside a class or trait body |
| `TraitAdaptation` | `Alias { trait_name: Option<Name>, method, alias: Option<String>, visibility: Option<Visibility> }`, `InsteadOf { trait_name: Option<Name>, method, instead_of: Vec<Name> }` | PHP-style trait conflict resolution and aliasing |
| `UseItem` / `UseKind` | `kind`, `name`, `alias` | Namespace import entries for `use`, `use function`, `use const`, and group-use declarations |
| `CallableTarget` | `Function(Name)`, `StaticMethod { receiver, method }`, `Method { object, method }` | Structured target of first-class callable syntax such as `foo(...)` or `Cls::bar(...)` |

Every AST node carries a `Span` (line + column) from the source, so error messages in later phases can point to the right location.

## The Pratt parser

**File:** `src/parser/expr.rs`

Parsing expressions with operators is the hardest part. Consider:

```php
1 + 2 * 3 ** 4
```

This should parse as `1 + (2 * (3 ** 4))` because `**` binds tighter than `*`, which binds tighter than `+`. And `**` is right-associative (`2 ** 3 ** 4` = `2 ** (3 ** 4)`), while `+` and `*` are left-associative.

elephc uses a **Pratt parser** (also called top-down operator precedence parser) to handle this elegantly. The key idea: every operator has a **binding power** — a pair of numbers (left, right) that determine how tightly it grabs its operands.

### Binding power table

```
Operator          Left BP    Right BP    Associativity
─────────────────────────────────────────────────────
??                  2          1         RIGHT (null coalescing)
||                  3          4         left
&&                  5          6         left
|  (bitwise OR)     7          8         left
^  (bitwise XOR)    9         10         left
&  (bitwise AND)   11         12         left
== != === !==      13         14         left
< > <= >= <=>      15         16         left
<< >>              17         18         left
.  (concat)        19         20         left
+ -                21         22         left
* / %              23         24         left
unary (- ! ~)          27                prefix
**                 29         28         RIGHT (r < l)
```

**Left-associative** operators have `right_bp > left_bp`. This means `1 + 2 + 3` parses as `(1 + 2) + 3`.

**Right-associative** operators have `right_bp < left_bp`. This means `2 ** 3 ** 4` parses as `2 ** (3 ** 4)`.

For `??`, the Pratt table still uses `BinOp::NullCoalesce` to assign binding power, but the parser builds a dedicated `ExprKind::NullCoalesce { value, default }` node rather than a generic `BinaryOp`.

### The algorithm

```
parse_expr_bp(min_bp):
    1. Parse prefix (literal, variable, unary op, parenthesized expr, ...)
       → this is the "left" node

    2. Loop:
       a. Look at the next token — is it an infix operator?
       b. Get its (left_bp, right_bp)
       c. If left_bp < min_bp → stop (operator doesn't bind tight enough)
       d. Consume the operator
       e. Parse right side: parse_expr_bp(right_bp)
       f. Build BinaryOp(left, op, right) → this becomes the new "left"
       g. Continue loop

    3. After loop: check for ternary (? :) at min_bp == 0

    Return left
```

### Walkthrough: `1 + 2 * 3`

```
parse_expr_bp(0):
  prefix → IntLiteral(1)

  loop iteration 1:
    next token: +  → (left_bp=21, right_bp=22)
    21 >= 0? yes → consume +
    parse_expr_bp(22):
      prefix → IntLiteral(2)
      loop iteration:
        next token: *  → (left_bp=23, right_bp=24)
        23 >= 22? yes → consume *
        parse_expr_bp(24):
          prefix → IntLiteral(3)
          loop: no more operators
          return IntLiteral(3)
        build: Mul(Int(2), Int(3))
      loop: no more operators
      return Mul(Int(2), Int(3))
    build: Add(Int(1), Mul(Int(2), Int(3)))

  loop: no more operators
  return Add(Int(1), Mul(Int(2), Int(3)))
```

Result: `1 + (2 * 3)` — correct!

The beauty of Pratt parsing is that you add a new operator by adding one line to the binding power table. No grammar rules to rewrite, no ambiguity to resolve.

### Prefix parsing

Before looking for infix operators, the parser handles **prefix** constructs — things that start an expression:

| Prefix | What it parses |
|---|---|
| `IntLiteral` | Return `IntLiteral` node |
| `FloatLiteral` | Return `FloatLiteral` node |
| `StringLiteral` | Return `StringLiteral` node |
| `true` / `false` | Return `BoolLiteral` node |
| `null` | Return `Null` node |
| `Variable` | Return `Variable` node (with postfix `++`/`--` check) |
| `throw` | Parse the following expression and wrap it in `ExprKind::Throw` |
| `-` (minus) | Parse inner expr at bp=27, return `Negate` |
| `!` (not) | Parse inner expr at bp=27, return `Not` |
| `~` (bitwise not) | Parse inner expr at bp=27, return `BitNot` |
| `++` / `--` | Return `PreIncrement` / `PreDecrement` |
| `(int)` / `(float)` / ... | Parse inner expr, return `Cast` |
| `(` | Parse inner expr, expect `)`, return inner expr (and allow a later postfix call like `(expr)(args)`) |
| `[` | Parse comma-separated exprs, expect `]`, return `ArrayLiteral` |
| `match` + `(` | Parse `match (...) { ... }` → `Match` |
| `Identifier` / `\Identifier` / qualified name + `(` | Parse as function call with arguments |
| `Identifier` / `\Identifier` / qualified name + `(...)` | Parse as first-class callable → `FirstClassCallable(CallableTarget::Function)` |
| `Identifier` / `\Identifier` / qualified name (no `(`) | Parse as constant reference → `ConstRef` |
| `function` + `(` | Parse anonymous function (closure) → `Closure` |
| `fn` + `(` | Parse arrow function → `Closure` (with `is_arrow = true`) |
| `new` + qualified name | Parse object instantiation → `NewObject` |
| `$this` | Return `This` node |
| `...` + expr | Parse spread/unpack → `Spread` |
| `ptr_cast` + `<Type>` + `(` | Parse pointer cast syntax → `PtrCast` |
| `buffer_new` + `<Type>` + `(` | Parse contiguous-buffer allocation → `BufferNew` |

### Postfix: calls, array access, and member access

After parsing a prefix, the parser checks for postfix operators:

- `(` for calling the result of an expression (`ExprCall`)
- `[` for array access
- `->` for property access or method call
- `::` for enum-case lookup, static method call, or static-method first-class callable (when the prefix is a parsed name)

At statement level, `stmt.rs` also parses `trait` declarations and class/trait-body `use` clauses. That `use` handling is intentionally context-sensitive so it does not interfere with closure capture lists like `function () use ($x) { ... }`.

```php
$arr[0]          →  ArrayAccess { array: Variable("arr"), index: IntLiteral(0) }
$arr[$i + 1]     →  ArrayAccess { array: Variable("arr"), index: BinaryOp(Add, ...) }
$p->x            →  PropertyAccess { object: Variable("p"), property: "x" }
$p->move(1, 2)   →  MethodCall { object: Variable("p"), method: "move", args: [...] }
Point::origin()  →  StaticMethodCall { receiver: Named("Point"), method: "origin", args: [] }
\Lib\Factory::make() → StaticMethodCall { receiver: Named("\\Lib\\Factory"), method: "make", args: [] }
parent::boot()   →  StaticMethodCall { receiver: Parent, method: "boot", args: [] }
```

## Statement parsing

**Files:** `src/parser/stmt.rs`, `src/parser/control.rs`

Statement parsing is simpler — after `parse()` has peeled off top-level `extern` blocks, `stmt.rs` looks at the current token to decide what kind of statement to parse:

| Current token | Parse as |
|---|---|
| `Echo` / `Print` | `Echo` statement — parse expression, expect `;` |
| `Throw` | `Throw` statement — parse one expression, expect `;` |
| `IfDef` | Build-time conditional statement |
| `Variable` | Assignment, compound assignment, array assign/push, or expression statement |
| `If` | `If` with optional `elseif` chain and `else` |
| `Try` | `Try` with one or more `catch` clauses and optional `finally` |
| `While` | `While` loop |
| `Do` | `DoWhile` loop |
| `For` | `For` loop with init/condition/update |
| `Foreach` | `Foreach` loop |
| `Switch` | `Switch` statement with cases and optional default |
| `Function` | Function declaration with parameters and body |
| `Class` / `Abstract Class` / `Readonly Class` / `Abstract Readonly Class` | Class declaration with properties and methods |
| `Enum` | Enum declaration |
| `Packed` | Packed class declaration |
| `Interface` | Interface declaration |
| `Trait` | Trait declaration with trait uses, properties, and methods |
| `Extern` | Handled one level up in `parser/mod.rs` via `parse_extern_stmts()` |
| `Return` | Return with optional expression |
| `Break` | Break statement |
| `Continue` | Continue statement |
| `Include`/`Require` | Include statement (path must be a string literal) |
| `Const` | Constant declaration (`const NAME = value;`) |
| `Namespace` | Namespace declaration (`namespace App\Core;` or `namespace App\Core { ... }`) |
| `Use` | Namespace import declaration (`use Foo\Bar;`, `use function Foo\bar as baz;`) |
| `Global` | Global variable declaration (`global $x, $y;`) |
| `Static` | Static variable declaration (`static $count = 0;`) |
| `[` | List unpacking (`[$a, $b] = expr;`) |
| `Identifier` + `(` | Expression statement (function call) |

### Assignment parsing

When the parser sees a `Variable`, it looks ahead to decide:

```php
$x = 42;         →  Assign { name: "x", value: IntLiteral(42) }
$x += 5;         →  Assign { name: "x", value: BinaryOp(Add, Variable("x"), IntLiteral(5)) }
$arr[0] = 5;     →  ArrayAssign { array: "arr", index: IntLiteral(0), value: IntLiteral(5) }
$arr[] = 5;      →  ArrayPush { array: "arr", value: IntLiteral(5) }
$x++;            →  ExprStmt(PostIncrement("x"))
```

Compound assignments (`+=`, `-=`, `*=`, `/=`, `.=`, `%=`) are desugared into regular assignments with binary operations.

### `try` / `catch` / `finally`

`control.rs` parses exception handling statements with this general shape:

```php
try {
    // body
} catch (TypeA | TypeB $e) {
    // handler
} catch (Exception) {
    // optional variable binding omitted
} finally {
    // cleanup
}
```

Each `catch` becomes a `CatchClause { exception_types, variable, body }`. `exception_types` always stores a vector, so single-type catches are just a one-element list.

## How it connects

The parser's output — `Program` (which is `Vec<Stmt>`) — first feeds into elephc's build-time conditional pass for `ifdef`, then into the [resolver](how-elephc-works.md), then into the dedicated name-resolution pass that canonicalizes namespace-aware names, and finally into the [type checker](the-type-checker.md):

```
[(Token, Span), ...] → Parser → Program (Vec<Stmt>) → Conditional → Resolver → NameResolver → Type Checker
```

---

Next: [The Type Checker →](the-type-checker.md)
