---
title: "The Parser"
description: "How tokens become an AST with Pratt parsing."
sidebar:
  order: 4
---

**Source:** `src/parser/` — `expr/`, `stmt/`, `control.rs`, `attributes.rs`, `ast/`, `mod.rs`

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

**File:** `src/parser/ast/`

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
| `InstanceOf { value, target }` | `$obj instanceof User`, `$obj instanceof $className` | Class/interface runtime type check. The target is either a named class/interface target or a dynamic target expression. |
| `Negate(Expr)` | `-$x` | Unary minus |
| `Not(Expr)` | `!$x` | Logical NOT |
| `BitNot(Expr)` | `~$x` | Bitwise NOT (complement) |
| `Throw(Expr)` | `throw new Exception("boom")` | Throw expression node used both in statements and expression positions such as `??` or ternaries |
| `Print(Expr)` | `print $x` | PHP print expression. It writes the operand and returns `1`; statement-form `print $x;` is represented as `ExprStmt(Print(...))`. |
| `NullCoalesce { value, default }` | `$x ?? $y` | Returns `$x` if non-null, otherwise `$y` |
| `Pipe { value, callable }` | `$x |> trim(...)`, `$x |> (fn($v) => $v + 1)` | PHP 8.5 pipe operator. Dedicated node, not a `BinOp`, so later passes can preserve left-hand evaluation before the callable target and emit pipe-specific diagnostics. Arrow functions used as targets must be parenthesized. |
| `Assignment { target, value, result_target, prelude, conditional_value_temp }` | `$x = 1`, `$arr[$i] ??= "fallback"` | Assignment expression. Complex targets can carry prelude statements and synthetic temporaries so side effects are evaluated once while the assignment still returns the assigned value. |
| `PreIncrement(String)` | `++$i` | Returns new value |
| `PostIncrement(String)` | `$i++` | Returns old value |
| `PreDecrement(String)` | `--$i` | |
| `PostDecrement(String)` | `$i--` | |
| `FunctionCall { name, args }` | `strlen($s)`, `Tools\fmt($s)`, `\strlen($s)` | Parsed as a structured name so later phases can resolve namespace aliases and fully-qualified names |
| `Yield { key, value }` | `yield`, `yield $v`, `yield $k => $v` | Yield expression inside a generator body. The parser keeps optional key/value expressions; later checker/codegen turns the enclosing function or closure into a `Generator` state machine. |
| `YieldFrom(Expr)` | `yield from inner()` | Contextual `yield from` delegation. The lexer leaves `from` as an identifier and the parser recognizes it only immediately after `yield`. |
| `ArrayLiteral(Vec<Expr>)` | `[1, 2, 3]`, `[...$arr, 4]` | Indexed array; elements may include `Spread` expressions |
| `ArrayLiteralAssoc(Vec<(Expr, Expr)>)` | `["a" => 1]` | Associative array |
| `Match { subject, arms, default }` | `match($x) { 1, 2 => "low", 3 => "high" }` | Match expression (returns a value). `arms` is `Vec<(Vec<Expr>, Expr)>`, so each arm can have multiple comma-separated patterns before `=>`, and `default` is optional (`Option<Box<Expr>>`) |
| `ArrayAccess { array, index }` | `$arr[0]`, `$str[-1]` | Same AST node is used for indexed arrays, associative-array lookups, and string indexing |
| `Ternary { condition, then_expr, else_expr }` | `$a ? $b : $c` | |
| `ShortTernary { value, default }` | `$a ?: $fallback` | PHP short ternary / Elvis form. Codegen evaluates `value` once, returns it if truthy, otherwise returns `default`. |
| `ErrorSuppress(Expr)` | `@file_get_contents("missing.txt")` | PHP error-control prefix expression. Codegen wraps the operand in a runtime warning-suppression scope. |
| `Cast { target, expr }` | `(int)$x` | |
| `Closure { params, variadic, return_type, body, is_arrow, is_static, captures }` | `function(int $x = 1) use ($y): string { ... }`, `fn(int $x): int => $x * 2`, or `static function(): int { ... }` | Anonymous function / arrow function. Params is `Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>` — name, declared type, default, is_ref. `variadic` is an optional parameter name. `return_type` stores the optional declared closure / arrow return `TypeExpr`. `captures` is `Vec<String>` — variables captured via an explicit `use (...)` clause. Arrow functions are still represented as `Closure`, parse with `is_arrow = true`, and do not carry explicit `use (...)` captures in the AST. `is_static` is set when the closure is prefixed with the `static` keyword (PHP `static function () {}` / `static fn () => …`); the type checker rejects any reference to `$this` inside a static closure. |
| `NamedArg { name, value }` | `foo(name: "Alice")` | Named call argument. The parser preserves source order; later phases validate names against the declared parameter list and normalize known-signature calls for ABI lowering. |
| `ClosureCall { var, args }` | `$fn(1, 2)` | Calling a closure stored in a variable |
| `ExprCall { callee, args }` | `$arr[0](1, 2)` | Calling the result of an expression (e.g., array access returning a callable) |
| `Spread(Expr)` | `...$arr` | Spread/unpack operator — expands an array into individual arguments or elements |
| `ConstRef(Name)` | `MAX_RETRIES`, `Config\PORT`, `\App\Config\PORT` | Reference to a user-defined constant |
| `NewObject { class_name, args }` | `new Point(1, 2)`, `new App\Model\User()` | Object instantiation |
| `NewScopedObject { receiver, args }` | `new self()`, `new static()`, `new parent()` | Object instantiation against a static receiver. Distinct from `NewObject` (which carries a fixed `Name`) so codegen can honour late static binding for `static`. |
| `PropertyAccess { object, property }` | `$p->x` | Property access via `->` |
| `NullsafePropertyAccess { object, property }` | `$p?->x` | Nullsafe property access via `?->` |
| `StaticPropertyAccess { receiver, property }` | `Point::$count`, `self::$count`, `parent::$count`, `static::$count` | Class-scoped property access via `::`, where `receiver` is a named class, `Self_`, `Static`, or `Parent` |
| `MethodCall { object, method, args }` | `$p->move(1, 2)` | Instance method call |
| `NullsafeMethodCall { object, method, args }` | `$p?->move(1, 2)` | Nullsafe instance method call; PHP rejects `?->method(...)` closure creation, so elephc reports `Cannot combine nullsafe operator with Closure creation` for that form |
| `StaticMethodCall { receiver, method, args }` | `Point::origin()`, `self::boot()`, `parent::boot()`, `static::boot()` | Static-style call via `::`, where `receiver` is a named class, `Self_`, `Static`, or `Parent` |
| `FirstClassCallable(CallableTarget)` | `strlen(...)`, `Tools\fmt(...)`, `Math::twice(...)` | PHP-style first-class callable syntax; the target is preserved structurally instead of being parsed as a call |
| `This` | `$this` | Reference to the current object inside a method |
| `PtrCast { target_type, expr }` | `ptr_cast<Point>($p)` | Pointer-tag cast parsed specially after `ptr_cast<T>` |
| `BufferNew { element_type, len }` | `buffer_new<int>(256)` | Compiler extension for contiguous hot-path buffers |
| `MagicConstant(MagicConstant)` | `__DIR__`, `__CLASS__` | Parsed from case-insensitive magic-constant tokens. `__LINE__` is lowered immediately to `IntLiteral`; the remaining magic constants are lowered by `src/magic_constants.rs` before type checking. |
| `ClassConstant { receiver }` | `MyClass::class`, `\App\C::class`, `self::class`, `parent::class`, `static::class` | The PHP `::class` reflection literal. Codegen lowers it to a string literal carrying the fully-qualified class name. `static::class` follows late static binding. |
| `ScopedConstantAccess { receiver, name }` | `MyClass::LIMIT`, `self::DEFAULT_SIZE` | User-declared class constant access through `::`; later phases resolve the receiver and constant metadata. |

### Statements (`Stmt`)

Things that do something:

Each `Stmt` also carries a source `span` and an `attributes` list. The list is populated only for declaration statements that can legally be decorated with PHP attributes; attributes before non-declaration statements are rejected during parsing.

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
| `FunctionVariantGroup { name, variants }` | Internal resolver metadata for include-loaded hidden function implementations behind one public name |
| `FunctionVariantMark { name, variant }` | Internal include-body marker that activates the hidden function variant loaded at that runtime include point |
| `Return(Option<Expr>)` | `return $x;` or `return;` |
| `Break(usize)` | `break;`, `break 2;` |
| `Continue(usize)` | `continue;`, `continue 2;` |
| `Include { path, once, required }` | `include 'file.php';` |
| `IncludeOnceMark { label }` | Internal resolver lowering for regular `include` / `require`, marking the resolved file as loaded for later `*_once` guards |
| `IncludeOnceGuard { label, body }` | Internal resolver lowering for `include_once` / `require_once`; codegen checks a per-file flag before emitting the guarded body |
| `Throw(Expr)` | `throw new Exception("boom");` |
| `Synthetic(Vec<Stmt>)` | Internal lowering only; a source construct that has already been expanded into one or more ordinary statements before final codegen |
| `Try { try_body, catches, finally_body }` | `try { ... } catch (Exception $e) { ... } finally { ... }` |
| `ConstDecl { name, value }` | `const MAX = 100;` |
| `IfDef { symbol, then_body, else_body }` | `ifdef DEBUG { ... } else { ... }` |
| `NamespaceDecl { name: Option<Name> }` | `namespace App\Core;`, `namespace;` |
| `NamespaceBlock { name: Option<Name>, body }` | `namespace App\Core { ... }`, `namespace { ... }` |
| `UseDecl { imports }` | `use App\Lib\Tool;`, `use function App\fn as helper;`, `use Vendor\Pkg\{Thing, Other as Alias};` |
| `ListUnpack { vars, value }` | `[$a, $b] = [1, 2];` for simple local positional destructuring; skipped, keyed, nested, and non-local destructuring patterns lower to `Synthetic` assignment statements |
| `Global { vars }` | `global $x, $y;` — declares variables as referencing global storage |
| `StaticVar { name, init }` | `static $count = 0;` — declares a variable that persists across function calls |
| `ClassDecl { name, extends, implements, is_abstract, is_final, is_readonly_class, trait_uses, properties, constants, methods }` | `final readonly class Point extends Shape implements Named { use NamedTrait; ... }` |
| `EnumDecl { name, backing_type, cases }` | `enum Status: int { case Ok = 1; case Err = 2; }` |
| `PackedClassDecl { name, fields }` | `packed class Vec2 { public float $x; public float $y; }` |
| `InterfaceDecl { name, extends, methods, constants }` | `interface Named extends Stringable { public function name(): string; }` |
| `TraitDecl { name, trait_uses, properties, constants, methods }` | `trait Named { public const KIND = "name"; ... }` |
| `PropertyAssign { object, property, value }` | `$p->x = 10;` |
| `StaticPropertyAssign { receiver, property, value }` | `Counter::$count = 10;`, `self::$count = 10;` |
| `StaticPropertyArrayPush { receiver, property, value }` | `Counter::$items[] = 10;`, `self::$items[] = 10;` |
| `StaticPropertyArrayAssign { receiver, property, index, value }` | `Counter::$items[0] = 10;`, `self::$items[0] = 10;` |
| `PropertyArrayPush { object, property, value }` | `$p->items[] = 10;` |
| `PropertyArrayAssign { object, property, index, value }` | `$p->items[0] = 10;` |
| `ExternFunctionDecl { name, params, return_type, library }` | `extern function foo(int $x): int;` or entries inside `extern "lib" { ... }` — `params` is `Vec<ExternParam>`, where each `ExternParam` stores `{ name, c_type }`, and `return_type` is a `CType` |
| `ExternClassDecl { name, fields }` | `extern class Point { public int $x; }` |
| `ExternGlobalDecl { name, c_type }` | `extern global ptr $environ;` — the declared type is a C-facing `CType`, not a `PhpType` |
| `ExprStmt(Expr)` | `my_func();` (expression used as statement) |

Constructor property promotion is normalized during class-body parsing. A parameter such as `public int $id` in `__construct` becomes a `ClassProperty` plus a synthetic leading `PropertyAssign` statement equivalent to `$this->id = $id;`. By-reference promoted parameters preserve a `by_ref` flag on the generated property so codegen can bind the property slot to the referenced argument. Later passes otherwise see ordinary properties and ordinary constructor assignments.

### Statement dispatch

At statement level, parsing is split between `parser/mod.rs` and the `stmt/` submodules:

- `parse()` in `mod.rs` special-cases `extern` so one `extern "lib" { ... }` block can expand into multiple AST statements.
- Everything else flows through `stmt::parse_stmt()`, which selects the parser entry point from the current token.

| Current token | Parse as |
|---|---|
| `Class` / `Abstract Class` / `Final Class` / `Readonly Class` / combined class modifiers | Class declaration |
| `Enum` | Enum declaration |
| `Packed` | Packed-class declaration |
| `Interface` | Interface declaration |
| `Trait` | Trait declaration |
| `Function` | Function declaration |
| `Namespace` | Namespace declaration |
| `Use` | Namespace import declaration |
| `Return` | Return statement |
| `Throw` | Throw statement |
| `Echo` | Echo statement |
| `Print` | Generic expression statement containing `Print(...)` |
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
And  Or  Xor
BitAnd  BitOr  BitXor  ShiftLeft  ShiftRight
NullCoalesce
```

`instanceof` is represented as `ExprKind::InstanceOf` rather than `BinOp` because PHP has special RHS grammar: named targets (`User`, `self`, `parent`, `static`) are resolved like class names, while variable/property/array targets and parenthesized expressions are evaluated dynamically.

### Type expressions (`TypeExpr`)

Parsed type annotations use `TypeExpr` before the checker resolves them into
`PhpType` values:

```
Int  Float  Bool  Str  Void  Never  Iterable
Ptr(Option<Name>)  Buffer(Box<TypeExpr>)  Named(Name)
Nullable(Box<TypeExpr>)  Union(Vec<TypeExpr>)
```

`Iterable` represents PHP's `iterable` pseudo-type in parameter, return,
property, and typed-local annotations. Nullable shorthand (`?T`) and explicit
unions (`T|U`) are represented separately so the checker can reject invalid
forms such as `?T|U` and normalize accepted declarations.

### Class-related types

`ClassDecl` uses several supporting types:

| Type | Fields | Description |
|---|---|---|
| `Visibility` | `Public`, `Protected`, `Private` | Enum for property/method visibility |
| `Attribute` | `name`, `args`, `span` | A PHP 8 attribute entry from a `#[...]` group. The parser validates names and optional argument expressions. Class, method, and property names plus supported literal args feed `class_attribute_names()`, `class_attribute_args()`, `class_get_attributes()`, and the supported Reflection `getAttributes()` APIs; parameter reflection is not implemented yet. |
| `AttributeGroup` | `attributes`, `span` | One bracketed attribute group. Declaration sites can carry one or more groups. |
| `EnumCaseDecl` | `name`, `value`, `span`, `attributes` | A backed or unit enum case declaration, with declaration-level attributes preserved in the AST. |
| `ClassConst` | `name`, `visibility`, `is_final`, `value`, `span`, `attributes` | A class, interface, or trait constant declaration. |
| `ClassProperty` | `name`, `visibility`, `type_expr`, `readonly`, `is_final`, `is_static`, `by_ref`, `default`, `span`, `attributes` | A property declaration inside a class or trait, optionally carrying a parsed property type declaration, static-property marker, by-reference promotion marker, or declaration-level attributes |
| `ClassMethod` | `name`, `visibility`, `is_static`, `is_abstract`, `is_final`, `has_body`, `params`, `variadic`, `return_type`, `body`, `span`, `attributes` | A method declaration inside a class, trait, or interface |
| `CatchClause` | `exception_types`, `variable`, `body` | A catch arm. `exception_types` supports both single-type and PHP-style multi-catch (`TypeA | TypeB`), and `variable` is optional for PHP 8-style `catch (Exception)` |
| `StaticReceiver` | `Named(Name)`, `Self_`, `Static`, `Parent` | Left-hand side of `ClassName::method()`, `self::method()`, `static::method()`, and `parent::method()` |
| `TraitUse` | `trait_names`, `adaptations`, `span` | A `use TraitA, TraitB { ... }` clause inside a class or trait body |
| `TraitAdaptation` | `Alias { trait_name: Option<Name>, method, alias: Option<String>, visibility: Option<Visibility> }`, `InsteadOf { trait_name: Option<Name>, method, instead_of: Vec<Name> }` | PHP-style trait conflict resolution and aliasing |
| `UseItem` / `UseKind` | `kind`, `name`, `alias` | Namespace import entries for `use`, `use function`, `use const`, and group-use declarations |
| `CallableTarget` | `Function(Name)`, `StaticMethod { receiver, method }`, `Method { object, method }` | Structured target of first-class callable syntax such as `foo(...)` or `Cls::bar(...)` |

Every AST node carries a `Span` (line + column) from the source, so error messages in later phases can point to the right location.

## The Pratt parser

**File:** `src/parser/expr/`

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
or                  1          2         left
xor                 3          4         left
and                 5          6         left
assignment          7          6         RIGHT (variable targets)
? : / ?:            7          7         right-ish ternary parse
??                  9          8         RIGHT (null coalescing)
||                 11         12         left
&&                 13         14         left
|  (bitwise OR)    15         16         left
^  (bitwise XOR)   17         18         left
&  (bitwise AND)   19         20         left
== != === !==      21         22         left
< > <= >= <=>      23         24         left
|>                 24         25         left (dedicated Pipe node)
<< >>              25         26         left
.  (concat)        27         28         left
+ -                29         30         left
* / %              31         32         left
instanceof         35         special    left, named-or-dynamic RHS
unary (- ! ~)          35                prefix
**                 37         36         RIGHT (r < l)
```

**Left-associative** operators have `right_bp > left_bp`. This means `1 + 2 + 3` parses as `(1 + 2) + 3`.

**Right-associative** operators have `right_bp < left_bp`. This means `2 ** 3 ** 4` parses as `2 ** (3 ** 4)`.

For `??`, the Pratt table still uses `BinOp::NullCoalesce` to assign binding power, but the parser builds a dedicated `ExprKind::NullCoalesce { value, default }` node rather than a generic `BinaryOp`.

For `instanceof`, the Pratt loop handles the keyword at expression level and then parses either a class/interface target name or a dynamic target expression. Its binding power matches PHP's behavior where `!$obj instanceof User` parses as `!($obj instanceof User)`.

For `|>`, the Pratt loop handles `Token::PipeArrow` before the generic `BinOp` table and builds `ExprKind::Pipe { value, callable }`. The binding power `(24, 25)` places it below concatenation, shifts, and arithmetic, but above comparisons, `??`, ternary, logical operators, and assignment. This matches PHP 8.5 and keeps pipe-specific validation, such as requiring parenthesized arrow-function targets, out of generic binary-operator lowering.

The word-form logical operators (`and`, `xor`, `or`) have PHP's lower precedence. The symbolic `&&` and `||` continue to bind more tightly.

The full ternary form builds `ExprKind::Ternary`. The omitted-middle form `expr ?: fallback` builds `ExprKind::ShortTernary` so later phases can preserve PHP's single-evaluation rule for the left-hand expression.

Assignment expressions build `ExprKind::Assignment { target, value, result_target, prelude, conditional_value_temp }`. Their binding power matches PHP's low-precedence assignment slot, so `$x = true and false` parses as `($x = true) and false`, while `$x = $y = 1` remains right-associative. Standalone variable assignment statements still lower to `StmtKind::Assign` unless a lower-precedence word logical operator requires the whole statement to be represented as an expression statement.

For non-local expression targets, the parser emits hidden assignment prelude statements when a receiver, index, or RHS must be evaluated exactly once before the final write. The lowered `target` is the write target, `result_target` is the expression read after the write, and `prelude` contains temporary assignments such as the captured result of `idx()` in `$items[idx()] = value()` or the RHS value in `$items[$i] = ($i = 1)`. This keeps codegen on the normal assignment paths while preserving PHP's evaluation order for plain and compound assignment expressions. For `??=`, `conditional_value_temp` reserves a hidden temporary that codegen fills only in the null branch, preserving PHP's conditional RHS evaluation for targets such as `$items[$i] ??= ($i = 1)`.

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

    3. The `?` arm handles both full ternary (`? :`) and short ternary (`?:`)

    Return left
```

### Walkthrough: `1 + 2 * 3`

```
parse_expr_bp(0):
  prefix → IntLiteral(1)

  loop iteration 1:
    next token: +  → (left_bp=29, right_bp=30)
    29 >= 0? yes → consume +
    parse_expr_bp(30):
      prefix → IntLiteral(2)
      loop iteration:
        next token: *  → (left_bp=31, right_bp=32)
        31 >= 30? yes → consume *
        parse_expr_bp(32):
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
| `throw` | Parse the following expression at the lowest precedence and wrap it in `ExprKind::Throw` |
| `print` | Parse the operand at ternary-level precedence (bp=7, above word logical operators) and wrap it in `ExprKind::Print` |
| `yield` | Parse `yield`, `yield expr`, `yield key => value`, or contextual `yield from expr` |
| `-` (minus) | Parse inner expr at unary precedence (bp=35), return `Negate` |
| `!` (not) | Parse inner expr at unary precedence (bp=35), return `Not` |
| `~` (bitwise not) | Parse inner expr at unary precedence (bp=35), return `BitNot` |
| `@` (error control) | Parse inner expr at unary precedence (bp=35), return `ErrorSuppress` |
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
| `static` + `function` / `fn` + `(` | Parse static closure → `Closure` (with `is_static = true`); the type checker rejects `$this` inside the body |
| `new` + qualified name | Parse object instantiation → `NewObject` |
| `new` + `self` / `static` / `parent` + `(` | Parse scoped object instantiation → `NewScopedObject` |
| `<receiver>::class` | Parse `MyClass::class`, `\App\C::class`, `self::class`, `parent::class`, `static::class` → `ClassConstant` |
| `$this` | Return `This` node |
| `...` + expr | Parse spread/unpack → `Spread` |
| `ptr_cast` + `<Type>` + `(` | Parse pointer cast syntax → `PtrCast` |
| `buffer_new` + `<Type>` + `(` | Parse contiguous-buffer allocation → `BufferNew` |
| `__DIR__` / `__FILE__` / other magic constants | Parse magic constants → `MagicConstant` (`__LINE__` becomes `IntLiteral`) |

### Postfix: calls, array access, and member access

After parsing a prefix, the parser checks for postfix operators:

- `(` for calling the result of an expression (`ExprCall`)
- `[` for array access
- `->` for property access or method call
- `?->` for nullsafe property access or method call
- `::` for enum-case lookup, static method call, or static-method first-class callable (when the prefix is a parsed name)

At statement level, `stmt.rs` also parses `trait` declarations and class/trait-body `use` clauses. That `use` handling is intentionally context-sensitive so it does not interfere with closure capture lists like `function () use ($x) { ... }`.

```php
$arr[0]          →  ArrayAccess { array: Variable("arr"), index: IntLiteral(0) }
$arr[$i + 1]     →  ArrayAccess { array: Variable("arr"), index: BinaryOp(Add, ...) }
$p->x            →  PropertyAccess { object: Variable("p"), property: "x" }
$p->move(1, 2)   →  MethodCall { object: Variable("p"), method: "move", args: [...] }
$p?->x           →  NullsafePropertyAccess { object: Variable("p"), property: "x" }
$p?->move(1, 2)  →  NullsafeMethodCall { object: Variable("p"), method: "move", args: [...] }
Point::origin()  →  StaticMethodCall { receiver: Named("Point"), method: "origin", args: [] }
\Lib\Factory::make() → StaticMethodCall { receiver: Named("\\Lib\\Factory"), method: "make", args: [] }
parent::boot()   →  StaticMethodCall { receiver: Parent, method: "boot", args: [] }
```

## Statement parsing

**Files:** `src/parser/stmt/`, `src/parser/control.rs`

Statement parsing is simpler — after `parse()` has peeled off top-level `extern` blocks, `stmt.rs` looks at the current token to decide what kind of statement to parse:

| Current token | Parse as |
|---|---|
| `Echo` | `Echo` statement — parse expression, expect `;` |
| `Print` | Expression statement — parse `Print(...)`, expect `;` |
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
| `Class` / `Abstract Class` / `Final Class` / `Readonly Class` / combined class modifiers | Class declaration with properties and methods |
| `Enum` | Enum declaration |
| `Packed` | Packed class declaration |
| `Interface` | Interface declaration |
| `Trait` | Trait declaration with trait uses, properties, and methods |
| `Extern` | Handled one level up in `parser/mod.rs` via `parse_extern_stmts()` |
| `Return` | Return with optional expression |
| `Break` | Break statement with optional positive integer level |
| `Continue` | Continue statement with optional positive integer level |
| `Include` / `Require` / `IncludeOnce` / `RequireOnce` | Include statement (path is parsed as an expression and later folded by the resolver when it is a compile-time string) |
| `Const` | Constant declaration (`const NAME = value;`) |
| `Namespace` | Namespace declaration (`namespace App\Core;` or `namespace App\Core { ... }`) |
| `Use` | Namespace import declaration (`use Foo\Bar;`, `use function Foo\bar as baz;`) |
| `Global` | Global variable declaration (`global $x, $y;`) |
| `Static` | Static variable declaration (`static $count = 0;`) |
| `[` | List destructuring (`[$a, , $c] = expr;`, `["id" => $id] = expr;`) |
| `Identifier` + `(` | Expression statement (function call) |
| Internal lowering, no source token | `Synthetic` statement sequence used for temporary-backed lowering of effectful compound assignment targets |

### Assignment parsing

When the parser sees a `Variable`, it looks ahead to decide:

```php
$x = 42;         →  Assign { name: "x", value: IntLiteral(42) }
$x += 5;         →  Assign { name: "x", value: BinaryOp(Add, Variable("x"), IntLiteral(5)) }
$x <<= 1;        →  Assign { name: "x", value: BinaryOp(ShiftLeft, Variable("x"), IntLiteral(1)) }
$x ??= 5;        →  Assign { name: "x", value: NullCoalesce(Variable("x"), IntLiteral(5)) }
$x = true and false; → ExprStmt(BinaryOp(Assignment(Variable("x"), BoolLiteral(true)), And, BoolLiteral(false)))
echo ($x += 1);  →  Echo(Assignment(Variable("x"), BinaryOp(Add, Variable("x"), IntLiteral(1))))
$arr[0] = 5;     →  ArrayAssign { array: "arr", index: IntLiteral(0), value: IntLiteral(5) }
$arr[0] += 5;    →  ArrayAssign { array: "arr", index: IntLiteral(0), value: BinaryOp(Add, ArrayGet(...), IntLiteral(5)) }
$arr[] = 5;      →  ArrayPush { array: "arr", value: IntLiteral(5) }
$x++;            →  ExprStmt(PostIncrement("x"))
```

Compound assignments (`+=`, `-=`, `*=`, `**=`, `/=`, `.=`, `%=`, `&=`, `|=`, `^=`, `<<=`, `>>=`) are desugared into regular assignments with binary operations. Null coalescing assignment (`??=`) is represented as a regular assignment with a `NullCoalesce` value; codegen recognizes this shape and emits a conditional store so the right-hand side is only evaluated when the current target value is `null`. In expression form, the type checker and optimizer treat `ExprKind::Assignment`, its hidden prelude, and its conditional value temp as observable assignment machinery so the assignment cannot be folded away or hidden by stale constant-propagation facts.

Compound assignments can target variables, object properties, static properties, and non-append array elements. For simple targets, the parser lowers directly to the final assignment node. For effectful non-local targets such as `$obj->items[next_key()] += 1`, the parser emits a `StmtKind::Synthetic` sequence that stores the receiver or index expressions in hidden temporaries, then performs the read-modify-write using those temporaries. This preserves PHP's single-evaluation behavior without making codegen duplicate the target expressions.

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

The parser's output — `Program` (which is `Vec<Stmt>`) — first feeds into per-file magic-constant lowering, then elephc's build-time conditional pass for `ifdef`, then into the [resolver](how-elephc-works.md), then into the dedicated name-resolution pass that canonicalizes namespace-aware names, and finally into the [type checker](the-type-checker.md):

```
[(Token, Span), ...] → Parser → Program (Vec<Stmt>) → MagicConstants → Conditional → Resolver → NameResolver → Type Checker
```
