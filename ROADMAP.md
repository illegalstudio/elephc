# Roadmap

## v0.1.0 (done)

- [x] **Phase 0 — Scaffolding**: Cargo project, data types (`Token`, `Expr`, `Stmt`), CLI, module structure
- [x] **Phase 1 — Echo Strings**: Full pipeline for `echo "Hello, World!\n";`, ARM64 assembly output, `as` + `ld` linking
- [x] **Phase 2 — Variables and Integers**: Variable assignment, integer literals, `echo` for both types, `itoa` runtime, static type checker
- [x] **Phase 3 — Expressions**: Arithmetic operators (`+`, `-`, `*`, `/`), string concatenation (`.`) with auto int coercion, operator precedence (Pratt parser)
- [x] **Phase 4 — Polish**: 77 tests (lexer, parser, codegen, errors), error messages with line/column, README, v0.1.0 tag
- [x] **Refactoring**: Pratt parser, Span on AST nodes, `codegen/abi.rs` layer, `TypeEnv` from checker, `PhpType::stack_size()`

## v0.2.0 (done)

### Step 1 — Comparison operators and `if`/`else`

- [x] Comparison operators: `==`, `!=`, `<`, `>`, `<=`, `>=`
- [x] Modulo operator (`%`)
- [x] Boolean evaluation in codegen (`cmp` + `cset`)
- [x] `if` / `else` / `elseif` statements
- [x] Parser: `if` (`elseif`)* (`else`)? block structure with `{` `}`
- [x] Codegen: conditional jumps, label generation for branches
- [x] Tests for all comparison operators and branching paths

### Step 2 — Loops

- [x] `while` loops
- [x] `for` loops (init; condition; increment)
- [x] Parser: loop block structure
- [x] Codegen: loop labels, backward jumps, condition re-evaluation
- [x] `break` and `continue` support
- [x] Tests for loops, nested loops, edge cases (zero iterations, break, continue)
- [x] **Goal**: `fizzbuzz.php` compiles and runs correctly

## Future

- [ ] Function declarations and calls (`function foo($x) { ... }`)
- [ ] Local scope and stack frames per function
- [ ] `return` statement
- [ ] Multiple file compilation
- [ ] Linux / x86_64 target
- [ ] Basic optimizations (constant folding, dead code elimination)
- [ ] Logical operators (`&&`, `||`, `!`)
- [ ] String comparison
- [ ] `print` as alias for `echo`
