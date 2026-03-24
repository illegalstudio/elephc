# Implementation Plan: Classes for elephc (without polymorphism)

## Overview

Add basic class support to the elephc PHP-to-ARM64 compiler. Classes are compiled with a flat, statically-known memory layout — no vtable, no inheritance, no dynamic dispatch. Every class instance is a heap-allocated struct with fields at fixed offsets. Methods compile to regular functions with an implicit `$this` pointer passed as the first argument.

### In scope

- Classes with `public`/`private` fields
- Constructor (`__construct`)
- Methods (instance and static)
- `new` keyword
- `$this` in methods
- `->` property access and method calls
- `::` static method calls
- Objects as function parameters and return values
- Objects stored in arrays
- `readonly` properties (type checker enforced)

### Not in scope

- Inheritance (`extends`)
- Interfaces / abstract classes
- Traits
- Polymorphism / vtable
- Magic methods (except `__construct`)
- Dynamic properties (`$obj->$propName`)
- `protected` visibility (only `public` + `private`)

---

## Phase 1: Lexer — New Tokens

**Files to modify:**
- `src/lexer/token.rs`
- `src/lexer/scan.rs`
- `src/lexer/literals.rs`

**New tokens to add to the `Token` enum:**

```
Class          // class
New            // new
Public         // public
Private        // private
Readonly       // readonly
Arrow          // ->
DoubleColon    // ::
This           // $this (special variable)
```

**Changes:**

1. In `token.rs`, add the new token variants.

2. In `literals.rs`, function `scan_keyword` (match block): add keyword mappings:
   - `"class"` → `Token::Class`
   - `"new"` → `Token::New`
   - `"public"` → `Token::Public`
   - `"private"` → `Token::Private`
   - `"readonly"` → `Token::Readonly`

3. In `scan.rs`, function `scan_token`: the `-` match arm currently handles `--` and `-=`. Add: if next char is `>`, consume both and return `Token::Arrow`.

4. In `scan.rs`, the `:` match arm currently returns `Token::Colon`. Add: if next char is `:`, consume both and return `Token::DoubleColon`.

5. In `literals.rs`, function `scan_variable`: add special handling for `$this` — when the identifier after `$` is `this`, return `Token::This` instead of `Token::Variable("this")`.

---

## Phase 2: Parser — AST Nodes

**Files to modify:**
- `src/parser/ast.rs`
- `src/parser/stmt.rs`
- `src/parser/expr.rs`

### New AST nodes

Add to `StmtKind`:
```rust
ClassDecl {
    name: String,
    properties: Vec<ClassProperty>,
    methods: Vec<ClassMethod>,
},
PropertyAssign {
    object: Box<Expr>,
    property: String,
    value: Expr,
},
```

Add supporting structs:
```rust
pub enum Visibility {
    Public,
    Private,
}

pub struct ClassProperty {
    pub name: String,
    pub visibility: Visibility,
    pub readonly: bool,
    pub default: Option<Expr>,
    pub span: Span,
}

pub struct ClassMethod {
    pub name: String,
    pub visibility: Visibility,
    pub is_static: bool,
    pub params: Vec<(String, Option<Expr>)>,
    pub body: Vec<Stmt>,
    pub span: Span,
}
```

Add to `ExprKind`:
```rust
NewObject {
    class_name: String,
    args: Vec<Expr>,
},
PropertyAccess {
    object: Box<Expr>,
    property: String,
},
MethodCall {
    object: Box<Expr>,
    method: String,
    args: Vec<Expr>,
},
StaticMethodCall {
    class_name: String,
    method: String,
    args: Vec<Expr>,
},
This,
```

### Parser changes

1. **stmt.rs** — `parse_stmt`: Add match arm for `Token::Class` → `parse_class_decl` function:
   - Consume `class`, read name, consume `{`
   - Loop parsing members until `}`:
     - Read optional visibility (`public`/`private`, default: `public`)
     - Read optional `readonly`
     - If next is `Token::Variable` → property declaration (with optional `= default;`)
     - If next is `function` → method declaration (reuse existing param/body parsing)
     - If `static` before `function` → mark method as static
   - Return `StmtKind::ClassDecl`

2. **expr.rs** — `parse_prefix`: Add match arm for `Token::New`:
   - Consume `new`, read class name, parse `(args...)`
   - Return `ExprKind::NewObject`

3. **expr.rs** — `parse_prefix`: Add match arm for `Token::This`:
   - Return `ExprKind::This`

4. **expr.rs** — Postfix parsing in `parse_expr_bp` loop: handle `Token::Arrow`:
   - If followed by `Identifier` + `(` → `MethodCall`
   - Otherwise → `PropertyAccess`
   - Handle `Token::DoubleColon` → `StaticMethodCall`

5. **stmt.rs** — `parse_variable_stmt`: After `$var`, if `Token::Arrow` follows, parse property chain + `= value;` as `PropertyAssign`. Same for `$this->prop = value;`.

---

## Phase 3: Type Checker

**Files to modify:**
- `src/types/mod.rs`
- `src/types/checker/mod.rs`

**New file:** `src/types/checker/classes.rs`

### PhpType changes

Add to `PhpType`:
```rust
Object(String),  // class name
```

- `stack_size()` → 8 (pointer to heap)
- `register_count()` → 1
- `is_float_reg()` → false

### New data structures

```rust
pub struct ClassInfo {
    pub name: String,
    pub properties: Vec<PropertyInfo>,
    pub methods: HashMap<String, MethodInfo>,
}

pub struct PropertyInfo {
    pub name: String,
    pub ty: PhpType,
    pub visibility: Visibility,
    pub readonly: bool,
    pub index: usize,  // positional index in object layout
}

pub struct MethodInfo {
    pub sig: FunctionSig,
    pub visibility: Visibility,
    pub is_static: bool,
}
```

Add to `CheckResult`:
```rust
pub classes: HashMap<String, ClassInfo>,
```

Add to `Checker`:
```rust
pub classes: HashMap<String, ClassInfo>,
pub current_class: Option<String>,
```

### Type checking logic

1. **First pass**: Scan `StmtKind::ClassDecl`, register each class's structure in `checker.classes`. Infer property types from defaults.

2. **Second pass**: For each method:
   - Create `TypeEnv` with `$this: PhpType::Object(class_name)`
   - Set `checker.current_class = Some(class_name)`
   - Type-check body, determine return type
   - Store `FunctionSig` (with implicit `$this` as first param for instance methods)

3. **Expression inference:**
   - `NewObject` → verify constructor args, return `PhpType::Object(class_name)`
   - `PropertyAccess` → infer object type, look up property, check visibility, return property type
   - `MethodCall` → infer object type, look up method, check visibility + args, return return type
   - `StaticMethodCall` → look up method, verify `is_static`, check args, return return type
   - `This` → return `PhpType::Object(current_class)`

4. **Readonly enforcement**: In `PropertyAssign`, if property is `readonly` and we're not inside `__construct`, emit compile error.

---

## Phase 4: Codegen

**Files to modify:**
- `src/codegen/mod.rs`
- `src/codegen/context.rs`
- `src/codegen/abi.rs`
- `src/codegen/expr.rs`
- `src/codegen/stmt.rs`
- `src/codegen/functions.rs`

**New file:** `src/codegen/classes.rs`

### Object Memory Layout

```
+0:  class_id    (8 bytes, u64 — identifies which class)
+8:  property_0  (8 or 16 bytes depending on type)
+N:  property_1
...
```

- `class_id` is a compile-time constant assigned per class (0, 1, 2, ...)
- Int, float, bool, null, callable, object pointer → 8 bytes
- String → 16 bytes (ptr + len)
- Offsets computed at compile time and stored in `ClassInfo`

### Method Dispatch Convention

- Instance methods → label `_cls_{ClassName}_{methodName}`
- `$this` pointer passed in `x0` (first argument)
- All other args shift by one register position
- Static methods → label `_cls_{ClassName}_static_{methodName}`, no `$this`

### Codegen details

**Context (`context.rs`):**
- Add `classes: HashMap<String, ClassInfo>` to `Context`
- Add helper `object_field_offset(class_name, prop_name) -> usize`

**ABI (`abi.rs`):**
- `PhpType::Object(_)` in `emit_store`/`emit_load` → same as Array (single x register pointer)

**Module entry (`mod.rs`):**
- After function emission, emit class methods:
  - Instance methods → `emit_method` (like `emit_function` but with `$this` as implicit first param)
  - Static methods → `emit_function` with static label convention

**New file `classes.rs`:**

`emit_new_object(class_name, args)`:
- Compute total size: 8 (class_id) + sum of property sizes
- `mov x0, #size` → `bl __rt_heap_alloc`
- Store `class_id` at `[x0, #0]`
- Initialize properties to zero/defaults
- If `__construct` exists: evaluate args, pass object in `x0`, `bl _cls_{name}___construct`
- Return `PhpType::Object(class_name)`

`emit_property_access(object_expr, property)`:
- Evaluate `object_expr` → pointer in `x0`
- Load value at `[x0, #offset]` into result register(s)

`emit_method_call(object_expr, method, args)`:
- Evaluate args (push to stack)
- Evaluate `object_expr` (push to stack)
- Pop all into ABI registers (object → `x0`, args → `x1`+)
- `bl _cls_{ClassName}_{method}`

`emit_static_method_call(class_name, method, args)`:
- Same as regular `emit_function_call` with label `_cls_{name}_static_{method}`

**Expression codegen (`expr.rs`):**
- `NewObject` → `classes::emit_new_object`
- `PropertyAccess` → `classes::emit_property_access`
- `MethodCall` → `classes::emit_method_call`
- `StaticMethodCall` → `classes::emit_static_method_call`
- `This` → load `$this` from stack frame

**Statement codegen (`stmt.rs`):**
- `PropertyAssign` → evaluate value (push), evaluate object, store value at property offset

**Functions (`functions.rs`):**
- `collect_local_vars`: handle `PropertyAssign`, skip `ClassDecl`
- `infer_local_type`: handle `NewObject`, `PropertyAccess`, `MethodCall`, `StaticMethodCall`, `This`
- New `emit_method`: like `emit_function` but `$this` as implicit first param

---

## Phase 5: Resolver

**File:** `src/resolver.rs`

Add `StmtKind::ClassDecl` to recursive resolution. Recurse into method bodies to resolve any includes.

---

## Phase 6: Runtime

**No new runtime routines needed.** Objects use `__rt_heap_alloc` (existing bump allocator). Field access is direct `ldr`/`str` at known offsets. Method dispatch is static `bl`.

---

## Phase 7: Tests

### Lexer tests
- `test_lex_class_keyword` — class, new, public, private, readonly
- `test_lex_arrow_operator` — `->` not confused with `-` + `>`
- `test_lex_double_colon` — `::` not confused with `:` + `:`
- `test_lex_this` — `$this` produces `Token::This`

### Parser tests
- `test_parse_class_decl` — class with properties and methods
- `test_parse_new_object` — `new ClassName(args)`
- `test_parse_property_access` — `$obj->prop`
- `test_parse_method_call` — `$obj->method(args)`
- `test_parse_static_method_call` — `ClassName::method(args)`
- `test_parse_property_assign` — `$obj->prop = value;`
- `test_parse_chained_access` — `$obj->method()->prop`

### Codegen tests (end-to-end)
- `test_class_basic` — public int property, construct, echo property
- `test_class_constructor` — `__construct` with arguments
- `test_class_method` — instance method accessing `$this->prop`
- `test_class_static_method` — static method with no `$this`
- `test_class_private_property` — private property accessed within methods
- `test_class_string_property` — class with string properties
- `test_class_float_property` — class with float properties
- `test_class_multiple_objects` — multiple instances, verify independent state
- `test_class_object_as_argument` — pass object to a function
- `test_class_object_return` — return object from a function
- `test_class_object_in_array` — store objects in arrays
- `test_class_method_calling_method` — `$this->otherMethod()` inside a method
- `test_class_readonly_property` — readonly property set in constructor
- `test_class_default_property_values` — properties with defaults
- `test_class_multiple_classes` — two classes in same program
- `test_class_static_and_instance` — both static and instance methods

### Error tests
- `test_error_undefined_class` — `new Nonexistent()`
- `test_error_undefined_property` — `$obj->nonexistent`
- `test_error_undefined_method` — `$obj->nonexistent()`
- `test_error_private_access` — accessing private property outside class
- `test_error_readonly_assign` — assigning to readonly outside `__construct`
- `test_error_static_this` — `$this` in a static method
- `test_error_wrong_constructor_args` — wrong args to constructor

### Example program (`examples/classes/main.php`)

```php
<?php

class Point {
    public $x;
    public $y;

    public function __construct($x, $y) {
        $this->x = $x;
        $this->y = $y;
    }

    public function distanceTo($other) {
        $dx = $this->x - $other->x;
        $dy = $this->y - $other->y;
        return sqrt($dx * $dx + $dy * $dy);
    }

    public static function origin() {
        return new Point(0, 0);
    }
}

$a = new Point(3, 4);
$b = Point::origin();
echo $a->distanceTo($b);
echo "\n";
```

---

## Phase 8: Documentation

- `ROADMAP.md` — Move "Classes / OOP" from "Will not implement", add new version section
- `docs/language-reference.md` — Add classes section
- `docs/architecture.md` — Add object memory layout, method dispatch
- `docs/the-codegen.md` — Add class codegen section
- `docs/the-parser.md` — Add class AST nodes
- `docs/the-type-checker.md` — Add `PhpType::Object`

---

## Implementation Order

Maximizes incremental testability:

| Step | Phase | What | Est. |
|------|-------|------|------|
| 1 | Lexer | New tokens | ~30min |
| 2 | Parser | AST nodes + parsing | ~2-3h |
| 3 | Type checker | `PhpType::Object` + basic inference | ~2h |
| 4 | Codegen | Object allocation + field access | ~3h |
| 5 | Codegen | Method calls (instance) | ~2h |
| 6 | Codegen | Static methods | ~1h |
| 7 | Codegen | `$this` in methods | ~1h |
| 8 | Type checker | Readonly + visibility enforcement | ~1h |
| 9 | Codegen | Objects in arrays / as function args | ~1h |
| 10 | Resolver | ClassDecl handling | ~15min |
| 11 | Tests | Full test suite + example | ~2h |
| 12 | Docs | Documentation updates | ~1h |

**Total estimated effort: 15-20 hours**

---

## Risks and Considerations

1. **Heap pressure**: Objects share the 1MB bump allocator with arrays/strings. No GC — objects are never freed. Pre-existing limitation (ROADMAP v0.9).

2. **String properties**: Strings take 16 bytes (ptr+len) vs 8 bytes for other types. Offset calculation must account for mixed sizing. Same pattern as array element storage.

3. **Method ABI shift**: Instance methods reserve `x0` for `$this`, shifting all explicit args by one register. `emit_method_call` must build a different register assignment.

4. **Forward references**: Class A using class B and vice versa. The two-pass approach (register structure first, then check methods) handles this — same as existing function forward reference pattern.

5. **No GC**: Consistent with existing array/string behavior. Documented limitation.
