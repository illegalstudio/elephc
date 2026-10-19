# PHP compatibility batch

- [x] Parser: statement forms (attributes on global `const`, destructuring in a `while` condition, literal-led statements, `include`/`require` assigned to a static property, `static $a, $b` lists) and parser recovery that no longer cascades.
- [x] Lexer/parser: reserved keywords as qualified-name segments; a sole `\self`/`\parent`/`\static` stays a keyword.
- [x] Lexer: inline HTML, `?>` as a statement terminator, `<?=`.
- [x] Parser/IR: expression forms and dynamic class access (`$class::X`, `$obj::$prop`, `$obj::m()`, `new` class-name references, `call()[] = $v`, string offsets on `mixed`).
- [x] Parser/IR: assignments as expressions in every position; possibly-undefined locals with a defined-flag.
- [x] Resolver: `include`/`require` as expressions, in PHP evaluation order.
- [x] Parser/IR: dynamic members and calls (`static::{$n}()`, `self::$m()`, `[$a, 'm']()`, `$obj->{$name}[] = v`, named arguments in dynamic calls).
- [x] Variable variables, `compact()` and `extract()` over the frame's compile-time locals; `extract(EXTR_SKIP)` respects `use()` captures.
- [x] Checker: `match (true)` arms narrowed by their `instanceof` condition; widening through chained calls.
- [x] Checker: `throw` accepts `mixed` and Throwable-or-null unions through the runtime check; throwing an Object-typed null raises PHP's `Error`.
- [x] Checker/IR: PHP-compatible override signatures through one call ABI per override family.
- [x] Checker: monotonic fixpoints, lattice join for widened types, quadratic work removed, `file:line:col` diagnostics after autoload.
- [x] Optimizer: `function_exists()` polyfill folding; `use` imports inside `yield`/`clone`/`new $c`; `->name`/`->value` through `UnitEnum`/`BackedEnum`.
- [x] Generics: PHPDoc method templates erased on dispatched methods, `instanceof` on templates, class-template marker interfaces hidden from reflection.
- [x] IR: ternary/merge typing from lowered branch values; catchable `UnhandledMatchError`; runtime coercion of declared scalar returns.
- [x] Callables: first-class callables from runtime values and `ReflectionFunction` over them.
- [x] References: array elements and properties, `&$a[]`, `$a[] = &$x`, runtime-named properties, reference cycles, static-property references and untyped static locals.
- [x] IR: nested array writes with non-literal keys; static-member forms.
- [x] Runtime: file-helper failure paths; `fwrite` returns `int(0)` on `EAGAIN`; virtual hooked properties are not rendered by `var_dump`/`print_r`/`var_export`.
- [x] Builtins: stdlib-prelude functions; native declarations (`UnitEnum`, `BackedEnum`, `WeakMap`, `ErrorException`, `Serializable`, `SessionHandlerInterface`, `ReflectionType`).
- [x] Resolver: dynamic-include manifest (`elephc.includes.toml`, `--dynamic-includes`, `--include-manifest`) and its documentation in `docs/compiling/dynamic-includes.md`.
- [ ] Reflection: hide the builtin Reflection classes' private backing slots and property-hook accessors from `getProperties()`/`getDefaultProperties()`/`getMethods()`/`hasMethod()`, which keeps `new ReflectionClass()` over a user subclass of `ReflectionClass` within the default heap; un-ignore the four `var_dump_object_tests::reflection_subclass_*` tests.
- [ ] Follow-up (also on `main`): build reflection metadata lazily, so reflecting large classes (two `ReflectionClass` subclass reflectors, `getMethods()` on one, `class X extends ArrayObject`) does not exhaust the 8 MiB heap.
- [ ] Follow-up: a `static` local passed by reference to a user function.
- [ ] Follow-up: protected-member visibility checked against the root prototype (php-src `gh19044`).
- [ ] Follow-up: chain the `else` side of `match (true)` conditions into the later arms.

## Scope

Each task is a PHP behavior elephc did not support, or compiled into something that ran
differently from PHP. Every task carries codegen tests whose expected output was produced by
php 8.5 from the same source, plus error tests for the forms that stay refused. The php-src
`.phpt` suite (10 009 tests, run-tests' comparator) is the regression net. The work is tracked
in issue #<issue>; topics land as `Merge branch 'fix/<topic>'` commits so they can be reviewed
one at a time.

## Implementation notes

### Override family ABI
All overrides of a dispatched method share one parameter/return representation, boxed as
`Mixed` where the members disagree; a body whose declared scalar was boxed re-applies PHP's
coercion on entry (parameters) and before boxing its result. Shapes the family cannot bridge
(union parameters with differing scalar representations, an interface declaring a raw return
under a boxed family, optional parameters added by an override, changed defaults) are refused
with "(not yet supported ...)" diagnostics. Key files: `src/types/override_families.rs`,
`src/ir_lower/program/virtual_abi.rs`, `src/types/checker/schema/validation.rs`.

### References
One managed reference cell per reference set. Property slots holding a reference carry
`REFERENCE_PROPERTY_TAG_FLAG` in the property descriptor tag, and the object walkers
(`var_dump`, `print_r`, `serialize`, `json_encode`, `(array)`, `var_export`, `unserialize`)
follow it. Untyped properties that join a reference set widen to boxed storage. Recursion
guards for `var_dump`/`print_r`/`json_encode`/`serialize`/`===` use their own guard stacks.

### Checker convergence
`src/types/checker/lattice.rs` documents the join (distinct scalars to `mixed`, classes to an
ordered union of at most 8 members). `src/types/checker/fixpoint_trace.rs` holds the
oscillation detector (`ELEPHC_CHECKER_OSCILLATION_LIMIT`) and the opt-in tracer
(`ELEPHC_CHECKER_TRACE`, `ELEPHC_CHECKER_TRACE_DETAIL`). A method call on a union built by the
join accepts that only some member declares the method and throws "Call to undefined method"
at run time otherwise; declared unions keep the strict rule.

### `throw` operands
`Checker::check_thrown_type` accepts `mixed` and unions of Throwable objects and null, and
relies on the existing runtime check to raise `Error("Can only throw objects")` or the
non-Throwable `Error`, as PHP does. A flagged maybe-undefined read is checked against the
local's own type. The direct Object throw path guards a null pointer or in-band null sentinel.

### Dynamic-include manifest
`src/resolver/dynamic_includes/` holds the manifest loader (`manifest.rs`), site marking
(`mark.rs`), dispatch lowering (`mod.rs`) and the `PHP_VERSION` folding (`version_fold.rs`).
Sites are identified as `<file>@<scope>#<ordinal>`; each entry pins the md5 of the including
file and lists either `candidates` globs or a `deny` reason. User documentation:
`docs/compiling/dynamic-includes.md`.

### Builtins
PHP-written functions in `src/stdlib_prelude/`, injected only when the program uses them, with
`BuiltinKind::PreludeProvided` contracts in the shared catalog.

### Reflection metadata (pending)
`new ReflectionClass(<name>)` materializes the whole metadata graph eagerly, and every nested
`ReflectionMethod` embeds a `ReflectionClass` of its declaring class. When that class is a
builtin Reflection class, the nested reflectors listed its ~37 private `__*` backing slots and
the synthetic `__propget_*` hook accessors, which PHP never shows; with one more slot and
accessor the graph for a user subclass of `ReflectionClass` crossed the 8 MiB default heap.
Hiding those members is a fidelity fix on its own. The underlying cost of eager metadata is
tracked separately: it also affects `main`.
