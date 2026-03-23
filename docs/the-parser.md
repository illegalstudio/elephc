# The Parser

[← Back to Wiki](README.md) | Previous: [The Lexer](the-lexer.md) | Next: [The Type Checker →](the-type-checker.md)

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
| `PreIncrement(String)` | `++$i` | Returns new value |
| `PostIncrement(String)` | `$i++` | Returns old value |
| `PreDecrement(String)` | `--$i` | |
| `PostDecrement(String)` | `$i--` | |
| `FunctionCall { name, args }` | `strlen($s)` | |
| `ArrayLiteral(Vec<Expr>)` | `[1, 2, 3]` | Indexed array |
| `ArrayLiteralAssoc(Vec<(Expr, Expr)>)` | `["a" => 1]` | Associative array |
| `Match { subject, arms, default }` | `match($x) { 1 => "one" }` | Match expression (returns a value) |
| `ArrayAccess { array, index }` | `$arr[0]` | |
| `Ternary { cond, then, else }` | `$a ? $b : $c` | |
| `Cast { target, expr }` | `(int)$x` | |

### Statements (`Stmt`)

Things that do something:

| Variant | Example |
|---|---|
| `Echo(Expr)` | `echo $x;` |
| `Assign { name, value }` | `$x = 42;` |
| `If { condition, then_body, elseif_clauses, else_body }` | `if (...) { } elseif (...) { } else { }` |
| `While { condition, body }` | `while (...) { }` |
| `DoWhile { body, condition }` | `do { } while (...);` |
| `For { init, condition, update, body }` | `for (...; ...; ...) { }` |
| `Foreach { array, key_var, value_var, body }` | `foreach ($arr as $v) { }` or `foreach ($arr as $k => $v) { }` |
| `Switch { subject, cases, default }` | `switch ($x) { case 1: ...; default: ... }` |
| `ArrayAssign { array, index, value }` | `$arr[0] = 5;` |
| `ArrayPush { array, value }` | `$arr[] = 5;` |
| `FunctionDecl { name, params, body }` | `function foo($a) { }` |
| `Return(Option<Expr>)` | `return $x;` or `return;` |
| `Break` | `break;` |
| `Continue` | `continue;` |
| `Include { path, once, required }` | `include 'file.php';` |
| `ExprStmt(Expr)` | `my_func();` (expression used as statement) |

### Binary operators (`BinOp`)

```
Add  Sub  Mul  Div  Mod  Pow  Concat
Eq  NotEq  StrictEq  StrictNotEq  Lt  Gt  LtEq  GtEq
And  Or
```

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
||                  1          2         left
&&                  3          4         left
.  (concat)         5          6         left
== != === !==       7          8         left
< > <= >=           7          8         left
+ -                11         12         left
* / %              13         14         left
**                 17         16         RIGHT (r < l)
```

**Left-associative** operators have `right_bp > left_bp`. This means `1 + 2 + 3` parses as `(1 + 2) + 3`.

**Right-associative** operators have `right_bp < left_bp`. This means `2 ** 3 ** 4` parses as `2 ** (3 ** 4)`.

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
    next token: +  → (left_bp=11, right_bp=12)
    11 >= 0? yes → consume +
    parse_expr_bp(12):
      prefix → IntLiteral(2)
      loop iteration:
        next token: *  → (left_bp=13, right_bp=14)
        13 >= 12? yes → consume *
        parse_expr_bp(14):
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
| `-` (minus) | Parse inner expr at bp=15, return `Negate` |
| `!` (not) | Parse inner expr at bp=15, return `Not` |
| `++` / `--` | Return `PreIncrement` / `PreDecrement` |
| `(int)` / `(float)` / ... | Parse inner expr, return `Cast` |
| `(` | Parse inner expr, expect `)`, return inner expr |
| `[` | Parse comma-separated exprs, expect `]`, return `ArrayLiteral` |
| `Identifier` + `(` | Parse as function call with arguments |

### Postfix: array access

After parsing a prefix, the parser checks for `[` to handle array access:

```php
$arr[0]          →  ArrayAccess { array: Variable("arr"), index: IntLiteral(0) }
$arr[$i + 1]     →  ArrayAccess { array: Variable("arr"), index: BinaryOp(Add, ...) }
```

## Statement parsing

**Files:** `src/parser/stmt.rs`, `src/parser/control.rs`

Statement parsing is simpler — it looks at the current token to decide what kind of statement to parse:

| Current token | Parse as |
|---|---|
| `Echo` / `Print` | `Echo` statement — parse expression, expect `;` |
| `Variable` | Assignment, compound assignment, array assign/push, or expression statement |
| `If` | `If` with optional `elseif` chain and `else` |
| `While` | `While` loop |
| `Do` | `DoWhile` loop |
| `For` | `For` loop with init/condition/update |
| `Foreach` | `Foreach` loop |
| `Function` | Function declaration with parameters and body |
| `Return` | Return with optional expression |
| `Break` | Break statement |
| `Continue` | Continue statement |
| `Include`/`Require` | Include statement (path must be a string literal) |
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

## How it connects

The parser's output — `Program` (which is `Vec<Stmt>`) — feeds into the [resolver](how-elephc-works.md) and then the [type checker](the-type-checker.md):

```
[(Token, Span), ...] → Parser → Program (Vec<Stmt>) → Resolver → Type Checker
```

---

Next: [The Type Checker →](the-type-checker.md)
