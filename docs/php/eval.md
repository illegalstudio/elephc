---
title: "Eval"
description: "Experimental PHP eval support, including literal AOT lowering, dynamic interpreter fallback, scope synchronization, safety, and limitations."
sidebar:
  order: 5
---

`eval($code): mixed` evaluates a PHP fragment at the call site in the
caller-visible local scope. Dynamic source is parsed at runtime; eligible
string literals may be parsed and lowered ahead of time. It is a PHP language
construct, not a normal callable: `function_exists("eval")` and
`is_callable("eval")` return `false`,
and first-class callable syntax for `eval` is rejected.

> **Experimental:** Eval support is still evolving. The supported fragment
> surface and the boundary between AOT lowering and interpreter fallback may
> change between releases.

> **Security:** `eval()` is not a sandbox. Evaluated code can access the caller's
> state and the host-visible filesystem, environment, process, and network
> facilities exposed by supported builtins. Never evaluate untrusted input.

The evaluated string must be a PHP fragment without an opening `<?php` tag.
The call is statically typed as `mixed`, regardless of the execution path.

## Execution modes

elephc chooses the narrowest execution path it can prove safe:

| Source shape | Execution path | Extra runtime state |
|---|---|---|
| Eligible string literal with no caller-scope access | Parsed at compile time and lowered as an internal EIR function | No eval scope, eval context, or Magician bridge |
| Eligible string literal with read-only caller values | Lowered as an internal EIR function with boxed `Mixed` parameters | No eval scope, eval context, or Magician bridge |
| Eligible string literal with known scope writes | Lowered as an internal scope-aware EIR function using `EvalScopeGet` / `EvalScopeSet` | Core eval scope only; no interpreter bridge |
| Dynamic string, runtime declaration, include, reference, dynamic dispatch, or unsupported literal construct | Parsed into EvalIR and interpreted at runtime | Persistent eval context, synchronized scopes, and `elephc_magician` |

The compiler makes this decision per literal call. A program may therefore use
`eval()` without linking Magician when every call is fully handled by the AOT
paths. A program that needs interpreter fallback links the optional static
bridge into the standalone binary; programs without that requirement do not.
`--with-eval` force-links the bridge for testing or indirect use, but it is not
required to enable the language construct.

The AOT and fallback paths are covered on macOS ARM64, Linux ARM64, and Linux
x86_64. CI runs dedicated eval integration shards for every supported target.

## Performance

A fully native literal fragment has no runtime parsing or interpreter-dispatch
cost and does not increase the binary with Magician. Scope-backed AOT adds only
the materialization required for the statically known reads and writes.

Interpreter fallback parses dynamic source into EvalIR. Exact source strings
up to 64 KiB share a process-wide FIFO cache of 256 immutable parse results;
larger fragments bypass the cache. The cache avoids repeated tokenization and
parsing, but execution remains interpreted and scope/context state is never
cached. Force-linking with `--with-eval` also increases binary size even if no
call ultimately requires fallback.

## Quick start

```php
<?php
$value = 2;
$result = eval('$value = $value + 3; return $value * 2;');
echo $value . "|" . $result . "\n"; // 5|10
```

Compile and run it like any other program:

```bash
elephc example.php
./example
```

See [`examples/eval/`](https://github.com/illegalstudio/elephc/tree/main/examples/eval)
for the broad feature showcase and
[`examples/eval-globals/`](https://github.com/illegalstudio/elephc/tree/main/examples/eval-globals)
for global-scope synchronization. The implementation boundary is documented in
[Eval Runtime Architecture](../internals/eval-runtime.md).

## Scope behavior

Variables from the caller's local scope are visible in the fragment.
Assignments and `unset()` are reflected back into that scope, variables created
by the fragment remain visible after `eval()`, and `return expr;` returns from
the `eval()` call itself.

When a call needs the dynamic fallback, `eval()` is a runtime barrier. The
compiler flushes visible locals into a materialized eval scope before entering
the bridge, then reloads locals that may have been read, written, created, or
unset by the evaluated fragment. Runtime cells use elephc's boxed `Mixed`
representation, so the eval interpreter does not introduce a second PHP value
ABI. AOT literals without writes skip scope materialization when they need no
caller values or can receive read-only values as direct EIR parameters. Known
writes use the same core scope cells from an internal EIR function.

Inside closures, `use ($x)` captures synchronize only the closure's captured
copy. `use (&$x)` captures write through the shared source variable, so eval
writes are visible to the outer scope after the closure runs.

Top-level eval fragments can see CLI `$argc` and `$argv`. `global $name` can
alias compiler-known program-global storage, and `global $argc` / `global
$argv` inside function eval fragments alias the CLI argument globals. Unsetting
such a local alias removes the alias without unsetting the global value.

## Supported statements

| Construct | Support |
|---|---|
| Comments | PHP comments are accepted inside fragments. |
| Output | `echo` supports comma-separated arguments. `print` is an expression. |
| Variables | Reads, writes, by-name assignment, by-reference assignment, `unset()`, `isset()`, and `empty()` are supported. |
| Assignment forms | Simple variable assignment, by-reference variable, eval object-property, eval static-property, and bridge-supported generated/AOT static-property binding, compound assignment for variables, object properties, and static properties (`+=`, `-=`, `*=`, `**=`, `/=`, `%=`, `&=`, `|=`, `^=`, `<<=`, `>>=`, `.=`), simple variable increment/decrement (`++$x`, `$x++`, `--$x`, `$x--`), object property increment/decrement, and static property increment/decrement are supported. |
| Control flow | Braced and single-statement `if`/`elseif`/`else`, `else if`, `while`, `do/while`, `for`, `foreach`, `switch`, `break`, and `continue` are supported. |
| Exceptions | `throw`, `try`, `catch`, union catches, class-specific catches, optional catch variables, and `finally` are supported. `finally` runs before a fragment returns or propagates a `Throwable`; a control action from `finally` replaces the pending action from the protected body or catch. |
| Functions | Eval fragments can declare functions. Static locals inside eval-declared functions are initialized once per eval context and persist across later calls through that context. Top-level `static` declarations in separate eval fragments are initialized for each eval execution. |
| Classes | Eval fragments can declare classes and traits with properties including comma-separated simple property declarations, PHP's legacy `var` public-property marker, asymmetric property write visibility (`private(set)` / `protected(set)`), constructor property promotion including by-reference promotion for variable, array-element, object-property, static-property, property-array-element, static-property-array-element, and default-value targets, concrete property get/set hooks including by-reference get-hook syntax and PHP-compatible explicit set-hook parameter typing, methods, `__construct()`, inheritance, visibility, readonly properties/classes, abstract/final modifiers, trait uses with `insteadof` / `as` adaptations and PHP-compatible property/constant conflict checks, interface implementations, static members, class/interface/trait comma-separated constants including `final` constants, and class-level attributes. Duplicate eval class-like names and PHP-reserved bare class-like declaration/reference names are rejected. |
| Enums | Eval fragments can declare pure and `int` / `string` backed enums with cases, comma-separated constants including `final` constants, methods, interface implementations, `::cases()`, `::from()`, `::tryFrom()`, `->name`, and backed `->value`. |
| Includes | `include`, `include_once`, `require`, and `require_once` execute local filesystem paths from inside fragments. |
| Namespaces | Both `namespace Name;` and `namespace Name { ... }` forms are supported, including simple and grouped `use`, `use function`, and `use const` declarations. |

`foreach` supports value-only and key-value iteration over indexed and
associative arrays, plus eval-declared and generated/AOT `Iterator` and
`IteratorAggregate` objects. Eval associative arrays preserve PHP insertion
order for iteration.

Includes follow PHP's cwd-first lookup and then fall back to the eval call-site
directory. Included PHP files may contain normal `<?php ... ?>` blocks, raw text
outside PHP tags is echoed, a `return` inside the included file becomes the
include expression value, successful includes without `return` evaluate to `1`,
repeated `*_once` includes evaluate to `true`, missing `include` returns
`false` with warnings, and missing `require` aborts the eval fragment.

## Supported expressions

| Expression area | Support |
|---|---|
| Scalars | `null`, booleans, integers, floats, and strings. |
| Variables and properties | Variable reads, `$this->property` reads/writes from native methods including public/protected/private scalar including string, nullable scalar, Mixed, array, and object AOT property slots when eval is executing in a PHP-visible native class scope, dynamic `stdClass` properties, eval object property access including `__get()` / `__set()` fallback for missing or inaccessible eval properties, `isset()`, `empty()`, and `unset()` magic property dispatch through `__isset()` / `__unset()`, `instanceof` over static and dynamic class/interface targets, eval-declared static property access, runtime-valued static receivers for `$class::$property` reads/writes, increment/decrement, and `isset()` / `empty()` probes, expression-valued static receivers for `($expr)::$property` reads/writes, array writes/appends, and increment/decrement, dynamic static property names (`ClassName::${expr}` / `$class::${expr}` / `($expr)::${expr}`) for reads/writes, `isset()` / `empty()` probes, array writes/appends, array-element unsets, and increment/decrement, static-property unset attempts including dynamic names as PHP-compatible catchable errors, public/protected/private scalar including string, nullable scalar, Mixed, array, and object AOT static property access from PHP-visible native class scopes, and public/protected/private generated/AOT class-like constant fetches through the bridge. |
| Arrays | Indexed and associative literals, modern `[...]` and legacy `array(...)`, keyed elements, append writes (`$array[] = value`), numeric-index reads/writes, string-key reads/writes, object-property and static-property array writes/appends/unsets, and eval-declared or generated/AOT `ArrayAccess` object reads, writes, appends, `isset()`, `empty()`, and `unset()` through `offsetGet()`, `offsetSet()`, `offsetExists()`, and `offsetUnset()`. |
| Function-like calls | Direct calls, named arguments, argument unpacking (`...`), dynamic string/expression calls, first-class callable syntax for supported function, method, and invokable-object targets, invokable eval and generated/AOT objects, `call_user_func()`, and `call_user_func_array()` for supported call targets. |
| Object construction | `new ClassName`, `new ClassName(...)`, `new $className`, `new $className(...)`, and parenthesized class-name expressions (`new ($expr)` / `new ($expr)(...)`) for eval-declared classes, including constructor named arguments and unpacking; runtime class-name expressions may hold strings or objects whose runtime class is used as the construction target. `new self()`, `new static()`, and `new parent()` work inside eval-declared methods; anonymous `new [readonly] class [(args)] [extends Parent] [implements Iface, ...] { ... }` expressions are supported. `stdClass` and emitted AOT classes visible through runtime metadata support positional arguments, named arguments, numeric unpacking, string-keyed named unpacking, positional variadic tails, array-typed arguments, iterable-typed arguments, object-typed arguments, union/nullable registered type validation, intersection object type validation, registered by-reference lvalue validation, and representable scalar/string constant expressions, resolved global/named class-like constants, null, empty-array, supported array-valued, or supported object-valued default arguments when the current eval class scope satisfies PHP visibility. Generated AOT constructor bridge dispatch can invoke visibility-checked non-by-reference signatures plus `mixed`/untyped, scalar including string, nullable scalar, array, iterable, object, and supported union/intersection object shapes, with by-reference constructor parameters limited to the staged scalar/nullable scalar/Mixed/array/iterable/object slice. |
| Object cloning | `clone $object` shallow-copies eval-declared objects, `stdClass` storage, and ordinary emitted/AOT object storage. Eval-declared and emitted/AOT `__clone()` hooks run after the copy and obey public/protected/private visibility. |
| Method calls | Eval-declared object and static method calls support positional arguments, named arguments, numeric unpacking, string-keyed named unpacking, variadic tails, dynamic static receivers (`$class::method()`, `$class::$method()`, and `($expr)::method()`), variable static method names on named receivers (`ClassName::$method()`), braced dynamic static method names (`ClassName::{$method}()` / `$class::{$method}()` / `($expr)::{$method}()`), and by-reference parameters for direct variable, array-element, object-property including dynamic property names, object-property array-element, static-property, and static-property array-element arguments including dynamic receivers and dynamic property names. Missing or inaccessible eval methods dispatch through `__call()` / `__callStatic()` when those hooks are available. Runtime/AOT object-method and static-method fallback supports the same fixed-parameter binding plus positional variadic tails, registered by-reference lvalue validation and writeback target preservation, plus representable scalar/string constant expressions, resolved global/named class-like constants, null, empty-array, supported array-valued, or supported object-valued default arguments for public/protected/private parameters when the current eval class scope satisfies PHP visibility; scalar, nullable, callable, Mixed, array, iterable, object, union, and intersection-object return values are checked and boxed back to eval. Generated AOT method bridge dispatch can invoke visibility-checked non-by-reference signatures plus `mixed`/untyped, scalar including string, nullable scalar, array, iterable, object, and supported union/intersection object shapes, with by-reference parameters limited to the staged scalar/nullable scalar/Mixed/array/iterable/object slice. |
| Includes | `include`, `include_once`, `require`, and `require_once` are expressions. |
| Magic constants | `__LINE__`, call-site `__FILE__` / `__DIR__`, empty top-level eval-scope `__CLASS__` / `__TRAIT__`, namespace-aware `__NAMESPACE__`, eval-declared-function `__FUNCTION__` / `__METHOD__`, eval-declared-method `__FUNCTION__` / `__METHOD__` / `__CLASS__` / `__TRAIT__`, eval class-like constant/property initializers for `__CLASS__` / `__TRAIT__`, and reflected parameter defaults using the declaring callable scope, including trait-origin names for imported trait members. |
| Constants | Predefined eval-visible constants, dynamic constants from `define()`, namespaced constant fallback, bare constant fetches, `$class::CONST`, expression-valued static receivers for `($expr)::CONST`, braced dynamic class constant names (`ClassName::{$constant}` / `$class::{$constant}` / `($expr)::{$constant}`), and `$object::class` are supported. |
| Ternaries | Full ternary and short ternary (`?:`). |
| Match | Strict pattern comparison, comma-separated patterns, lazy result-arm evaluation, and `default`. A miss without `default` is reported as an eval runtime fatal. |

Supported unary operators are `+`, `-`, `!`, and integer bitwise `~`.

Supported binary operators are:

| Category | Operators |
|---|---|
| Arithmetic | `+`, `-`, `*`, `**`, `/`, `%` |
| String | `.` |
| Integer bitwise and shifts | `&`, `|`, `^`, `<<`, `>>` |
| Logical | `&&`, `||`, `and`, `or`, `xor` |
| Null coalescing | `??` |
| Equality | `==`, `!=`, `===`, `!==` |
| Comparison | `<`, `<=`, `>`, `>=`, `<=>` |

Array literals and append writes use PHP's next automatic integer key rule,
including integer-string keys such as `"2"`, boolean and float keys normalized
to integers, and `null` keys normalized to the empty string. Eval array writes
preserve native PHP copy-on-write behavior for by-value aliases while still
mutating reference aliases.

## Functions and callable dispatch

Eval-declared functions are callable from later eval fragments, from native code
after the eval barrier, and from string-literal `call_user_func()` /
`call_user_func_array()` paths. Eval-declared functions and registered AOT
global user functions support positional, named, and spread arguments inside
eval fragments. String keys in unpacked argument arrays bind as named
parameters. Direct calls and variable-function calls can also invoke registered
AOT global user functions when their by-reference parameters use generated
`mixed`/union-style boxed storage or one-word scalar raw storage (`int`, `bool`,
or `float`), string raw storage, or one-word heap raw storage (`array`,
`iterable`, and object/class parameters). Nullable scalar, native-only ABI
layouts, and other unsupported raw-storage by-reference free-function
parameters remain metadata-only until the function bridge has typed
staging/writeback for those ABI layouts.
Generated/AOT positional `&...$variadic` by-reference parameter tails are
supported for eval direct, variable-function, first-class callable,
`Closure::fromCallable()`, and `call_user_func_array()` dispatch when the
passed tail arguments have lvalue/ref-cell storage; element-level writes
propagate back to the caller variables, while rebinding the variadic container
itself remains local to the callee.
`call_user_func()` remains by-value for registered AOT free-function
by-reference parameters.

String-variable and expression callable calls such as `$fn(...)` and
`$callbacks[0](...)` share the eval callable dispatcher for supported builtins,
eval-declared functions, and registered AOT functions.
Inside eval, `is_callable()` also supports PHP's optional `$syntax_only` probe
and by-reference `$callable_name` output for string, array, object, and
`Closure` callable forms.

First-class callable syntax such as `strlen(...)`, `$object->method(...)`,
`ClassName::method(...)`, and `$invokable(...)` materializes eval callback
values as PHP-visible `Closure` objects that can be invoked through
`$callback(...)`, `call_user_func()`, and `call_user_func_array()`. Method
targets are validated when the first-class callable is created, including
missing, inaccessible, non-static static-syntax, and magic-call fallback cases;
static-syntax instance methods capture `$this` when PHP permits that form.
Namespaced function callables follow PHP's global fallback rule when the
namespaced function is not visible.
`Closure::fromCallable()` accepts the same supported string, callable-array,
object, and existing `Closure` callback values and materializes a PHP-visible
`Closure` object backed by the normalized eval callable target. `Closure::call()`
on those closure objects supports same-class method and invokable-object
rebinding, passes the call arguments after the receiver by value like PHP, and
reports PHP-compatible warning/null results for function and static-method
callables. `Closure::bind()` and `Closure::bindTo()` persistently bind eval
closure literals, function callable targets, same-class method targets, and
invokable-object closure targets to a new receiver. Function-target closures
accept omitted, `null`, or `"static"` scope, but reject explicit object/class
scope rebinding like PHP. The optional scope argument is accepted for method
closures, but eval's current binding model derives method scope from the bound
receiver rather than exposing the full PHP scope-mutation surface.

Closure literals created inside eval are PHP-visible `Closure` objects: they
report true for `is_object()`, `get_class($fn)` returns `Closure`,
`$fn instanceof Closure` works, and they remain callable through direct calls,
`call_user_func()`, `call_user_func_array()`, `Closure::call()` for transiently
binding `$this` to an object when invoking the eval-created closure,
`Closure::bind()` / `Closure::bindTo()` for persistent receiver binding, and
`ReflectionFunction`.

Inside eval fragments, two-element object-method callable arrays such as
`[$this, "method"]` can be invoked through `$cb(...)`, `call_user_func($cb,
...)`, `call_user_func_array($cb, [...])`, and `iterator_apply()`. Eval-declared
object methods support string-keyed named arguments through
`call_user_func_array()`. Eval-declared objects with `__invoke()` can be called
through `$object(...)`, `call_user_func($object, ...)`, and
`call_user_func_array($object, [...])`, and `is_callable($object)` reports them
as callable. Generated/AOT objects with bridge-supported `__invoke()` metadata
use the same direct and callback call paths, including named/defaulted argument
binding, and non-invokable generated/AOT objects report PHP-compatible
direct-call or callback errors. Eval-declared classes that extend generated/AOT
parents inherit bridge-supported parent instance-method callable probes and
`__invoke()` object-call dispatch. Static method callables can use
`["ClassName", "method"]` or
`"ClassName::method"` through `$cb(...)`, `call_user_func()`, and
`call_user_func_array()`. Eval-declared static methods also support string-keyed
named arguments through `call_user_func_array()`; generated/AOT static method
fallback supports the same named-argument and positional variadic-tail binding
for public/protected/private scalar, nullable, callable, Mixed, array, iterable,
object, union, and intersection-object parameter signatures, including
registered by-reference lvalue validation, when the current eval class scope
satisfies PHP visibility. Generated AOT bridge dispatch can invoke
`mixed`/untyped, scalar including string, nullable scalar, array, iterable,
object, and supported union/intersection object shapes, with by-reference method
and constructor parameters limited to the staged scalar/nullable
scalar/Mixed/array/iterable/object slice.

Post-barrier native direct calls and string-literal `call_user_func()` callbacks
currently accept simple positional arguments. Post-barrier
`call_user_func_array()` callbacks can pass indexed or string-keyed argument
containers to eval-declared functions.

## Classes and objects

Eval-declared classes support inheritance, public/protected/private properties,
comma-separated simple property declarations, PHP's legacy `var` marker for
public properties, and methods, asymmetric property write visibility (`private(set)` /
`protected(set)`), constructor property promotion including by-reference promotion
for variable, array-element, object-property, static-property,
property-array-element, static-property-array-element, and default-value targets,
concrete property `get` / `set` hooks, interface property hook contract checks
including asymmetric write visibility, abstract property hook contracts
including asymmetric write visibility, property-level `readonly`, `readonly class`,
`__construct()`, abstract classes and methods, final classes, methods, and
properties, trait composition with `insteadof` conflict resolution and `as`
aliases/visibility adaptations, interface implementation checks, static
properties, static methods, static interface method contracts, generated/AOT
interface method contract checks, single and
comma-separated class, interface, trait, and enum constants including `final`
constants, class-level attributes,
`ClassName::class` literals, magic method fallback through `__call()` and
`__callStatic()`, and magic property fallback through `__get()` and `__set()`.
Reserved PHP bare class-like declaration and reference names are rejected, while
semi-reserved names that PHP allows, such as `enum`, `from`, and `resource`,
remain valid.
Explicit concrete `set` hook parameter types are retained as settable-property
metadata and must be PHP-compatible supertypes of the property type.
Eval traits also accept the legacy `var` marker for public properties.
PHP's global `#[Override]` marker is validated on eval-declared methods against
non-private parent methods and eval interface method contracts, and rejected on
non-method OOP declaration targets. On eval-declared interface methods, the
marker requires a matching method inherited from an eval, builtin, or
generated/AOT parent interface.
Eval validates method override and interface method parameter and return types
with PHP-style parameter contravariance and return covariance for supported
declared type metadata, including nullable, union, `mixed`, `self`, `parent`,
`static`, class, and interface types, including generated/AOT parent method
metadata retained by the eval bridge.
Concrete eval classes also enforce inherited generated/AOT abstract parent
method requirements when reflection/signature metadata is available, including
requirements carried through intermediate eval abstract classes.
The same applies to generated/AOT abstract parent property-hook contracts when
the bridge retains the class property contract metadata.
Abstract classes may defer missing interface methods and property contracts, but
declared or inherited members that cover an interface contract are validated at
declaration time. Generated/AOT interface method and property-hook contracts are
validated when their bridge metadata is available.
Eval validates inherited property redeclarations with PHP-style invariant
property types, matching static storage, compatible read/write visibility, and
readonly compatibility: child redeclarations may add `readonly`, but may not
remove inherited `readonly`. The same checks apply to generated/AOT parent
property metadata when the eval bridge exposes reflection flags and declared
property types.
Typed eval-declared instance and static properties track PHP initialization
state: reads before initialization or after `unset()` throw the same catchable
`Error`, while untyped properties and explicit `= null` defaults are initialized
to `null`.
Eval validates inherited class/interface constant redeclarations with PHP-style
visibility compatibility, including generated/AOT parent and interface constants
when their reflection metadata is available, and rejects non-public interface
constants.
Eval-declared method calls also enforce declared return values at runtime, with
weak scalar coercions and PHP-style handling for `void`, `never`, `self`,
`parent`, and late-bound `static`.
`isset()`, `empty()`, and `unset()` on missing or inaccessible eval properties
dispatch through `__isset()` and `__unset()` using PHP's `empty()` gate
ordering. `instanceof` works with eval-declared classes and interfaces,
generated/AOT runtime objects, dynamic string targets, dynamic object targets,
and parenthesized target expressions.
Eval-declared objects in string contexts dispatch through public
parameterless `__toString()` for `echo`, `print`, concatenation, `strval()`,
callable `strval()` dispatch, and weak `string` parameter coercion. Classes
with a compatible `__toString()` satisfy `Stringable` implicitly.
Eval validates magic method staticness, visibility, arity, by-reference
parameter bans, and relevant declared parameter/return-type contracts for
`__toString()`, `__get()`, `__set()`, `__isset()`, `__unset()`, `__call()`,
`__callStatic()`, `__sleep()`, `__wakeup()`, `__serialize()`,
`__unserialize()`, `__debugInfo()`, `__set_state()`, `__invoke()`,
`__clone()`, `__destruct()`, and `__construct()` when dynamic classes or traits
are declared.
Eval-declared class-like metadata and aliases are synchronized across generated
eval contexts, so later eval fragments inside AOT methods can introspect
classes, interfaces, traits, enums, and aliases declared by an earlier eval call
in the same process.
Member
visibility is checked at runtime for eval-declared objects and
static/class-constant accesses. Class-level attributes declared on eval classes,
interfaces, traits, and enums, plus bridge-registered generated/AOT class-level
attributes, are visible through `class_attribute_names()`,
`class_attribute_args()`, and `class_get_attributes()` when their arguments fit
the supported literal positional/named subset (`string`, `int`, `float`, `bool`,
`null`, negated numeric literals, `ClassName::class` strings, or positional
or scalar-keyed array literals containing the same supported values). Positional
arguments keep integer keys in `class_attribute_args()` /
`ReflectionAttribute::getArguments()`, named arguments keep their PHP names as
string keys, and array literal keys support string keys plus PHP-normalized
integer, boolean, null, and float keys.
`ReflectionAttribute::newInstance()` instantiates eval-declared or
bridge-supported generated/AOT attribute classes from those materialized
attributes, and `ReflectionAttribute::getTarget()` /
`isRepeated()` report the reflected owner target and same-owner repetition
metadata.
Attribute names remain visible when an attribute uses unsupported argument
syntax, but requesting those arguments is a runtime fatal.
Private parent properties shadowed by same-named child properties use separate
runtime storage, so parent methods keep seeing the private parent value while
child methods and public access see the child property.
`ReflectionClass::getAttributes()`, `ReflectionEnum::getAttributes()`,
`ReflectionMethod::getAttributes()`, `ReflectionProperty::getAttributes()`,
`ReflectionClassConstant::getAttributes()`, and
`ReflectionParameter::getAttributes()` expose eval-retained class, enum, method,
property, class-constant, and method-parameter attributes for eval-declared
class-like symbols and bridge-registered generated/AOT class-level, method,
property, and class-constant attributes when their arguments fit the same
literal positional/named subset. Attribute array literal keys support string
keys plus PHP-normalized integer, boolean, null, and float keys; dynamic or
otherwise unsupported attribute array keys are still unsupported metadata.
`getName()` returns the reflected class, member, or parameter name
for those owners. `ReflectionClass`, `ReflectionObject`, `ReflectionFunction`, `ReflectionMethod`,
`ReflectionProperty`, `ReflectionClassConstant`, `ReflectionEnumUnitCase`, and
`ReflectionEnumBackedCase` expose `getDocComment()` and report `false` because
eval does not retain docblock text. `ReflectionClass`, `ReflectionFunction`,
and `ReflectionMethod` expose `getExtensionName()` and `getExtension()` and
report `false` / `null` for eval-declared user symbols.
`ReflectionClass`, `ReflectionFunction`, and `ReflectionMethod` expose
`getFileName()`, `getStartLine()`, and `getEndLine()` for parser-backed eval
declarations. File names use PHP's synthetic eval file format from the current
eval call site, and line numbers are one-based inside the evaluated fragment.
Generated/AOT metadata for `ReflectionClass` over classes, interfaces, traits,
and enums, plus `ReflectionMethod`, exposes the original source file and
declaration line when EIR source metadata is available. AOT `getEndLine()`
currently reports the declaration line because the bridge keeps declaration
spans, not full body spans.
`ReflectionClass` construction accepts class-name strings and object arguments;
object arguments reflect the runtime class of eval-created or generated/AOT
objects. `ReflectionObject` construction accepts object arguments and exposes the
same class metadata through a `ReflectionObject` instance. Its inherited
`newInstance()`, `newInstanceArgs()`, and `newInstanceWithoutConstructor()`
helpers use the object's runtime class id. Straight-line `ReflectionObject`
receivers built from statically typed objects also normalize named constructor
arguments through the reflected constructor signature. Its inherited
`hasProperty()`, `getProperty()`, and `getProperties()` include public dynamic
properties from the reflected instance and mark those `ReflectionProperty`
objects as dynamic.
`ReflectionEnum` construction accepts enum-name strings for eval-declared
enums. It exposes `hasCase()`, `getCase()`, `getCases()`, `isBacked()`, and
`getBackingType()` for eval enum metadata, returning `ReflectionEnumUnitCase`,
`ReflectionEnumBackedCase`, and `ReflectionNamedType` objects where PHP does.
`ReflectionMethod` construction accepts class-name strings and object
arguments; object arguments resolve to the runtime class before method lookup.
It also accepts PHP's deprecated one-argument `ClassName::method` string form
for eval-visible and generated/AOT methods.
`ReflectionMethod::createFromMethodName()` accepts `ClassName::method` strings
for eval-visible and generated/AOT methods and returns retained method
metadata equivalent to direct `ReflectionMethod` construction.
`ReflectionClass::getShortName()`,
`ReflectionClass::getNamespaceName()`, and `ReflectionClass::inNamespace()`
derive namespace-aware parts from the resolved eval class-like name.
`ReflectionFunction::getShortName()`, `getNamespaceName()`, and
`inNamespace()` derive namespace-aware parts from the reflected eval function
name. `ReflectionMethod::getShortName()` reports the reflected method name,
while `ReflectionMethod::getNamespaceName()` reports an empty string and
`inNamespace()` reports `false`, matching PHP's method reflection behavior.
`ReflectionFunction` and `ReflectionMethod` report eval user-symbol defaults
through `isInternal()`, `isUserDefined()`, `isClosure()`, `returnsReference()`,
`isGenerator()`, `isVariadic()`, `isStatic()`,
`hasTentativeReturnType()`, and `getTentativeReturnType()`. `hasReturnType()`
and `getReturnType()` expose retained eval return type metadata for supported
named, nullable, union, and intersection declarations, including `void` and
`never` as builtin non-nullable named types.
`isDeprecated()` reflects PHP's global `#[Deprecated]` attribute on
eval-declared functions and methods, and reports `false` otherwise.
`ReflectionFunction::isAnonymous()` reports `false` for eval-declared named
functions, and `getClosureThis()`, `getClosureScopeClass()`, and
`getClosureCalledClass()` report `null` for eval-visible non-closure function
and method reflectors.
`ReflectionFunction::isDisabled()` reports `false` for eval-visible functions.
`ReflectionFunction::getStaticVariables()` and
`ReflectionMethod::getStaticVariables()` expose eval-declared static local
variables, materializing initializer values before the first invocation and
returning updated values after reflected or direct calls.
`ReflectionFunction` over an eval closure literal reports `isClosure()` and
`isAnonymous()` as `true`, reports `isStatic()` from the literal's
`static function` marker, exposes captured `use` variables through
`getClosureUsedVariables()`, and can invoke the closure through `invoke()` or
`invokeArgs()`.
`ReflectionFunction::getClosureUsedVariables()` and
`ReflectionMethod::getClosureUsedVariables()` report empty arrays for supported
non-closure function and method reflectors.
`ReflectionClass::isFinal()`, `ReflectionClass::isAbstract()`,
`ReflectionClass::isInterface()`, `ReflectionClass::isTrait()`, and
`ReflectionClass::isEnum()` report eval and generated/AOT class-like metadata,
including PHP-compatible enum finality and class-like kind checks for eval
interfaces, traits, and enums. `ReflectionClass::isReadOnly()` reports eval and
generated/AOT `readonly class` metadata. `ReflectionClass::isAnonymous()`
reports true for eval anonymous classes and false for eval-declared named
class-like symbols.
`ReflectionClass::isInstantiable()` reports whether eval or generated/AOT
class-like metadata describes a concrete class with no constructor or a public
constructor. `ReflectionClass::isCloneable()` reports whether eval or
generated/AOT class metadata describes a concrete class with no `__clone()` or
a public `__clone()`.
`ReflectionClass::isIterable()` and `isIterateable()` report whether eval or
generated class metadata describes a concrete `Iterator` or `IteratorAggregate`
class.
`ReflectionClass::isInternal()` and `isUserDefined()` distinguish
compiler-injected class-like metadata from eval-declared or generated
user-defined class-like symbols.
`ReflectionClass::getModifiers()` returns PHP's `ReflectionClass::IS_*`
modifier bitmask for eval class-like metadata.
`ReflectionClass::getInterfaceNames()` returns implemented interfaces for eval
and generated/AOT classes, plus parent interfaces for eval and generated/AOT
interfaces. `ReflectionClass::getInterfaces()` materializes those names as a
name-keyed array of `ReflectionClass` objects. `ReflectionClass::getTraitNames()`
returns traits used directly by eval class-like symbols and generated/AOT
classes, traits, and enums. `ReflectionClass::getTraits()` materializes those
direct trait names as `ReflectionClass` objects, and
`ReflectionClass::getTraitAliases()` exposes direct eval class-like and
generated/AOT class/enum trait `as` aliases as PHP's alias-name to
`Trait::method` map.
`ReflectionClass::implementsInterface()` checks those eval relations
case-insensitively, returns true when reflecting the requested interface itself,
and checks generated/AOT class-interface relations through runtime metadata. It
throws catchable `ReflectionException` values when the argument names a class,
trait, enum, or missing interface.
`ReflectionClass::isSubclassOf()` checks eval parent-class chains and
implemented or extended interfaces case-insensitively. It excludes the reflected
symbol itself, returns `false` for trait and enum targets, and throws a
catchable `ReflectionException` when the target name is missing.
`ReflectionClass::isInstance()` checks eval-created or generated/AOT objects
against the reflected class-like metadata, including parent, interface, and enum
relations; trait targets return `false`.
`ReflectionClass::hasMethod()` and `ReflectionClass::hasProperty()` report
method and property membership for eval classes, interfaces, traits, and enums;
method lookup is case-insensitive, while property lookup is case-sensitive.
For generated/AOT classes, `ReflectionClass::hasMethod()` and `hasProperty()`
can also probe emitted method/property metadata without requiring the full
member lists to be materialized on the `ReflectionClass` object.
Eval-declared classes that extend generated/AOT parents expose inherited
bridge-supported public/protected parent members to `method_exists()`,
`property_exists()`, and `get_class_methods()` with PHP-compatible scope
filtering.
`ReflectionClass::hasConstant()`, `getConstant()`, `getConstants()`,
`getDefaultProperties()`, `getStaticProperties()`,
`getStaticPropertyValue()`, `setStaticPropertyValue()`,
`getReflectionConstant()`, and `getReflectionConstants()` expose eval-visible
class constants, interface constants, trait constants, enum constants, enum
cases, supported materialized property defaults, and current eval-declared
static property values. For generated/AOT class-like symbols, the constant APIs
also expose materializable scalar, string, null, `::class`, simple arithmetic or
concatenation, and enum-case constant metadata through runtime hooks, and the
static-property APIs expose bridge-supported public/protected/private generated/AOT static
property values. Constant
lookup is case-sensitive; single-value
lookups return `false` when no constant or case is visible. `getConstants()`
and `getReflectionConstants()` accept PHP's `ReflectionClassConstant::IS_*`
visibility/finality filter bitmask; `null` means no filter and `0` returns no
constants.
`ReflectionClass::getMethods()` and `ReflectionClass::getProperties()` return
materialized `ReflectionMethod` and `ReflectionProperty` objects for the same
visible member metadata, including supported member attributes and predicate
flags. For generated/AOT classes, `ReflectionClass::getMethod()` /
`getProperty()` and `getMethods()` / `getProperties()` materialize reflection
objects from emitted member-name and predicate metadata. Optional modifier
filters are supported for materialized `ReflectionClass` member lists; inline
or tracked receivers with known integer or `ReflectionMethod::IS_*` /
`ReflectionProperty::IS_*` constants can also be statically narrowed before
materialization. AOT method reflection also exposes registered parameter
names, declared parameter types, declared return types, required/optional
counts, and registered scalar, null, empty-array, supported array-valued, or supported object-valued default values for generated
constructor, instance-method, and static-method signatures. AOT property
reflection exposes registered declared property types and supported scalar,
string, null, empty-array, or supported array-valued default values for generated property metadata, including
`ReflectionClass::getDefaultProperties()`. AOT method and
property/class-constant reflection expose generated member attributes when
their arguments fit the materializable literal subset.
`ReflectionClass`, `ReflectionObject`, and `ReflectionEnum` expose a compact
`__toString()` dump for eval-visible class-like metadata, including supported
constants, properties, and methods.
`ReflectionMethod::getDeclaringClass()` and
`ReflectionProperty::getDeclaringClass()` return a materialized
`ReflectionClass` for the symbol that declares the reflected
member. `ReflectionMethod::hasPrototype()` and `getPrototype()` expose
eval and generated/AOT parent-class overrides and interface implementation
prototypes, including static method prototypes; inherited methods that are not
overridden report no prototype, matching PHP reflection.
`ReflectionClass::getConstructor()` returns a materialized
`ReflectionMethod` for direct, inherited, interface, trait, and generated/AOT
constructors, including registered generated/AOT constructor parameter names,
counts, and supported defaults where available; it returns `null` when no
constructor is visible. `ReflectionClass::getParentClass()`
returns a materialized `ReflectionClass` for eval-declared and generated/AOT
parent classes or `false` when no parent class exists.
`ReflectionClass::newInstance()` constructs eval-declared and bridge-supported
generated/AOT reflected classes with public constructors and forwards
constructor arguments through eval's positional, named, and unpacking-aware call
binding. Non-public constructors fail like PHP reflection construction.
`ReflectionClass::newInstanceArgs()` constructs those reflected classes from an
indexed or string-keyed argument array, including arrays built at eval runtime,
and treats string keys as named constructor arguments.
`ReflectionClass::newInstanceWithoutConstructor()` allocates eval-declared and
generated/AOT reflected classes, initializes supported property defaults, skips
`__construct()`, and rejects reflected abstract classes, interfaces, traits,
and enums.
`ReflectionMethod::invoke()` and `invokeArgs()` call eval-declared reflected
methods, bypass public/protected/private visibility like PHP reflection,
preserve named arguments for the invoked method, follow PHP's by-value
`invoke()` variadic forwarding, accept `null` or an object for static methods,
and throw catchable `ReflectionException` values when an instance receiver is
not compatible with the reflected declaring class. For generated/AOT classes,
`ReflectionMethod::invoke()` and `invokeArgs()` are also lowered for inline or straight-line
tracked reflectors with declared or inferred/untyped parameter contracts; untyped
parameters are forwarded through the boxed `Mixed` ABI used by EIR. The lowered
call supports instance and static methods, constructors returned by
`ReflectionClass::getConstructor()`, method-name case-insensitivity, defaults,
and named arguments. Generated/AOT
bridge-supported invoke targets also bypass public/protected/private visibility
like PHP reflection. Inside eval fragments, `invokeArgs()` accepts literal or
runtime-built indexed/string-keyed argument arrays for those bridge-supported
generated/AOT targets; unsupported runtime-only reflector shapes outside that
signature slice still fail at runtime.
Eval-declared method parameter type hints are checked when the method is
entered. Supported checks include scalar hints with PHP-style weak scalar
coercion, `array`, `object`, `iterable`, `mixed`, nullable/union forms, and
eval/runtime class or interface names.
`ReflectionMethod::isStatic()`, `isPublic()`, `isProtected()`, `isPrivate()`,
`isFinal()`, `isAbstract()`, and `getModifiers()` report eval method metadata,
with PHP-compatible `ReflectionMethod::IS_*` constants for the bitmask.
`ReflectionMethod::isConstructor()` and `isDestructor()` report whether the
reflected method is `__construct` or `__destruct`.
`ReflectionMethod::setAccessible()` is accepted as a PHP-compatible no-op.
`ReflectionFunction::getName()`, `ReflectionFunction::getParameters()`,
`ReflectionMethod::getParameters()`, `getNumberOfParameters()`, and
`getNumberOfRequiredParameters()` report retained eval-declared function and
method metadata, plus registered generated/AOT free-function, method,
static-method, and constructor parameter names, declared parameter and return
types, required/optional counts, by-reference and variadic flags, and scalar,
null, empty-array, supported array-valued, or supported object-valued default
values when native signatures are registered. Eval code can also reflect
supported callable-builtin signatures, including internal origin, parameter
names, parameter types, and return type metadata.
Eval-declared
functions and methods expose declared-type presence for parameters and return types, simple
named type metadata through
`ReflectionParameter::getType()` / `ReflectionNamedType::getName()`,
`allowsNull()`, and `isBuiltin()`, and the legacy
`ReflectionParameter::isArray()` / `isCallable()` predicates for named `array`
and `callable` parameter types. `ReflectionParameter::getClass()` returns a
`ReflectionClass` object for retained nullable or non-nullable named object
parameter types and `null` for builtin, union, intersection, or untyped
parameters. Multi-member union metadata is exposed through
`ReflectionUnionType::getTypes()`, `allowsNull()`, and `__toString()`. Intersection parameter
metadata is exposed through `ReflectionIntersectionType::getTypes()` and
`allowsNull()` / `__toString()`. Named, nullable named, union, and intersection
`ReflectionType` objects stringify using the retained eval type metadata.
Function, method, and parameter attributes are exposed through
`getAttributes()` using materialized `ReflectionAttribute` objects. Parameter
default values, optionality, nullability, variadic flags, and by-reference
flags are retained for eval-declared functions and methods, including
`ReflectionParameter::allowsNull()` and `ReflectionParameter::__toString()`.
`ReflectionParameter::getDeclaringClass()`
returns the declaring class-like symbol for eval method parameters, and
`ReflectionParameter::getDeclaringFunction()` returns a `ReflectionFunction`
object for eval free-function parameters or a `ReflectionMethod` object for the
declaring eval method. Direct `new ReflectionParameter(...)` construction
accepts eval and registered generated/AOT free-function names, eval-visible and
generated/AOT class/interface/trait method arrays, and object-method arrays
resolved from the evaluated runtime object, including inline `new` expressions.
`ReflectionFunction::invoke()` and `invokeArgs()`
dispatch eval-declared functions with the same named/default/variadic argument
binding used by direct eval function calls. Runtime-held generated/AOT
`ReflectionFunction` objects can invoke registered generated functions through
the native bridge with parameter names, supported defaults, named arguments, and
indexed or string-keyed runtime argument arrays. Direct eval calls, variable
function calls, and `call_user_func()` paths can also invoke registered
generated/AOT free functions with positional variadic tails when the generated
signature has no by-reference parameters. Direct, variable-function,
first-class callable, `Closure::fromCallable()`, and `call_user_func_array()`
paths can additionally invoke generated/AOT free functions whose by-reference
parameters use boxed Mixed/union storage or one-word scalar raw storage
(`int`, `bool`, or `float`), string raw storage, or one-word heap raw storage
(`array`, `iterable`, and object/class parameters), including positional
`&...` variadic tails when the tail arguments have lvalue/ref-cell storage.
Registered generated/AOT free-function parameter
names, declared types, return types, by-reference and variadic flags,
required/optional counts, and supported defaults are also exposed through
`ReflectionFunction` / `ReflectionParameter` metadata. Unsupported
generated/AOT free-function bridge shapes, such as nullable tagged-scalar,
native-only ABI layouts, and other unsupported raw-storage by-reference
parameters, remain reflectable as metadata but are not invocable through eval.
Supported callable-builtin invocation is
covered by the general Reflection support documented in
`docs/php/classes.md`.
Defaulted eval method parameters are
bound when omitted and reported through `ReflectionParameter::isOptional()`,
`isDefaultValueAvailable()`, `isDefaultValueConstant()`,
`getDefaultValueConstantName()`, and `getDefaultValue()`. Constant-name metadata
is retained for predefined or eval-defined constant fetches, namespaced constant
fallback, and class/interface/trait/enum constant fetches; `::class` literals,
magic constants, and literal defaults are materialized as values but are not
reported as default-value constants. Supported default expressions include
scalar literals, arrays whose keys and values are supported default
expressions, magic constants, unary and binary operators supported by eval,
ternary and null-coalescing expressions, predefined or eval-defined constant
fetches, namespaced constant fallback, class/interface/trait/enum constant
fetches, `self::class` / `parent::class` / named class-like `::class` literals,
and `new ClassName(...)` / `new self(...)` / `new parent(...)` with supported
non-spread constructor arguments. Magic constants in reflected parameter
defaults are evaluated in the declaring function or method scope; methods
imported from traits retain PHP's trait-origin `__TRAIT__`, `__METHOD__`, and
`__FUNCTION__` values while `__CLASS__` follows the using class. Late-bound
`static::` defaults and unpacked
constructor arguments in defaults are rejected like PHP constant expressions.
Variadic eval method parameters bind extra positional and unknown named
arguments into a PHP array and are reported through
`ReflectionParameter::isVariadic()` and `ReflectionParameter::isOptional()`.
Constructor-promoted eval parameters are reported through
`ReflectionParameter::isPromoted()`. By-reference eval method parameters accept
direct variable, array-element, object-property including dynamic property
names, object-property array-element, static-property, and static-property
array-element arguments including dynamic receivers and dynamic property names,
write back fixed parameters after method execution, write back mutated
`&...$items` elements when the variadic container itself is not rebound, and are
reported through
`ReflectionParameter::isPassedByReference()` and
`ReflectionParameter::canBePassedByValue()`.
`ReflectionProperty::isStatic()`, `isPublic()`, `isProtected()`, `isPrivate()`,
`isFinal()`, `isAbstract()`, `isReadOnly()`, `isPromoted()`, `isVirtual()`,
`isDynamic()`, `isProtectedSet()`, `isPrivateSet()`, `isInitialized()`,
`isDefault()`, and `getModifiers()` report eval property
metadata with PHP-compatible `ReflectionProperty::IS_*` constants for the
bitmask. `isPromoted()` reports generated/AOT and eval-declared
promoted-property metadata. `isProtectedSet()` and `isPrivateSet()` derive from
the retained modifier bitmask, including eval-declared class, abstract-property,
interface-property, and generated/AOT asymmetric visibility plus public readonly
property metadata. `isDynamic()` reports `false` for supported
declared properties and `true` for public dynamic object properties
materialized with `new ReflectionProperty($object, $property_name)`.
`ReflectionProperty::isDefault()` is the inverse for those supported dynamic
properties. `isInitialized()` tracks eval-backed and bridge-supported
generated/AOT instance and static property storage, including typed properties
without defaults, unset eval properties, virtual property hooks, and public
dynamic properties on the inspected object.
`ReflectionProperty::hasType()`, `getType()`, and `getSettableType()` expose
retained property type metadata through `ReflectionNamedType`,
`ReflectionUnionType`, and `ReflectionIntersectionType` where eval has retained
a supported declared type. `getSettableType()` follows an explicit property
`set(Type $value)` hook parameter when one is retained; otherwise it falls back
to the declared property type.
`ReflectionProperty::hasDefaultValue()` and `getDefaultValue()` expose
materialized property default metadata, including PHP's implicit `null` default
for untyped concrete properties without an explicit initializer.
`ReflectionProperty::__toString()` formats retained eval/generated property
metadata as a PHP-style `Property [ ... ]` descriptor for the supported
visibility, static, type, default, and virtual-property surface.
`ReflectionProperty::hasHooks()`, `hasHook()`, `getHooks()`, and `getHook()`
expose eval-declared concrete, abstract, and interface property get/set hook
metadata plus registered generated/AOT interface property-hook metadata, and
return hook `ReflectionMethod` objects using PHP's `$property::get` /
`$property::set` names. Eval also exposes
`PropertyHookType::Get` and `PropertyHookType::Set` inside evaluated fragments
for those APIs, including `PropertyHookType::cases()`, `from()`, and
`tryFrom()`.
`ReflectionProperty::setAccessible()` is accepted as a PHP-compatible no-op.
`ReflectionProperty::getValue()` and `setValue()` read and write eval-declared
and bridge-supported generated/AOT instance and static property values, bypass
public/protected/private visibility like PHP reflection, route concrete eval
property hooks through their accessors, and still reject readonly writes.
`ReflectionProperty::getRawValue()` and `setRawValue()` are supported for
eval-declared backed instance properties, including backed property hooks, and
for bridge-supported generated/AOT instance properties. Raw access bypasses
concrete eval property hook accessors. Virtual property hooks reject raw
access like PHP. `ReflectionProperty::isLazy()` reports `false` for
eval-declared and bridge-supported generated/AOT properties because eval does
not implement lazy properties;
`skipLazyInitialization()` is a no-op for supported non-static backed
properties, and `setRawValueWithoutLazyInitialization()` follows the same raw
storage write path as `setRawValue()`.
`ReflectionClassConstant::getAttributes()`,
`ReflectionEnumUnitCase::getAttributes()`, and
`ReflectionEnumBackedCase::getAttributes()` expose eval-retained class-constant
and enum-case attributes through the same materialized `ReflectionAttribute`
shape; their `getName()` methods return the reflected constant or case name,
`ReflectionClassConstant::getValue()` returns the class-constant value or enum
case object, while `ReflectionEnumUnitCase::getValue()` and
`ReflectionEnumBackedCase::getValue()` return the reflected enum-case object.
`ReflectionEnumBackedCase::getBackingValue()` returns the scalar backing value,
`getDeclaringClass()` returns the declaring class or enum as a
`ReflectionClass`, and `getEnum()` returns the declaring enum as a
`ReflectionEnum`. `ReflectionClassConstant::isEnumCase()` reports enum cases.
`ReflectionClassConstant`, `ReflectionEnumUnitCase`, and
`ReflectionEnumBackedCase` expose `isDeprecated()`, `hasType()`, and
`getType()` with PHP's current untyped defaults: `false`, `false`, and `null`.
Their `__toString()` methods format retained constant and enum-case metadata in
PHP's `Constant [ ... ] { ... }` shape.
`ReflectionClassConstant`, `ReflectionEnumUnitCase`, and
`ReflectionEnumBackedCase` expose `isEnumCase()`, `isPublic()`,
`isProtected()`, `isPrivate()`, `isFinal()`, and `getModifiers()` with PHP's
`ReflectionClassConstant::IS_*` bitmasks. Enum cases report public,
non-final constant metadata, and the enum-case reflection classes expose the
same inherited `IS_*` constants as PHP.
Concrete property hooks are lowered to eval accessor methods; reads and writes
route through inherited hooks, while access from the accessor itself uses the
raw backing slot. Both `get => expr;` and `set => expr;` short forms are
supported; short set hooks store the expression result in the raw backing slot.
`readonly` eval properties require a declared type, may be
assigned from the constructor of the declaring class, and later writes fail as
eval runtime fatals. A `readonly class` makes declared instance properties
readonly implicitly, while declared static properties remain mutable and are not
converted to readonly slots. Untyped instance properties in readonly classes are
still rejected.
Missing-property writes can still dispatch through `__set()`, but readonly
classes reject actual dynamic property creation.
PHP's global `#[AllowDynamicProperties]` marker is accepted only on
non-readonly eval-declared classes; eval rejects it on readonly classes,
interfaces, traits, enums, members, and enum cases.
`self::`, `parent::`, and late-bound `static::` work for supported static
members, class constants, and class-name literals.
Attempts to `unset()` static properties parse normally and throw PHP's
catchable `Error`, including runtime-valued static receivers and dynamic
static property names.

Eval object construction can allocate eval-declared classes, `stdClass`, and
emitted AOT classes visible through runtime class metadata. Missing class names
during eval object construction fail with an eval runtime fatal diagnostic.
`clone $object` creates a shallow copy for eval-declared objects, `stdClass`
objects, and ordinary emitted/AOT objects. Eval `__clone()` hooks are invoked on
the cloned object after storage copying and use the same runtime visibility
checks as method calls. Emitted/AOT `__clone()` hooks, including non-public hooks
visible from the current eval class scope, are invoked through the generated
method bridge after the clone storage has been copied.

AOT and eval-declared class-name probes are visible through `class_exists()`.
Eval object relation probes through `instanceof`, `is_a()`, and `is_subclass_of()` use
generated AOT class/interface metadata and eval-created object metadata.
`interface_exists()`, `trait_exists()`, and `enum_exists()` can probe generated
AOT metadata. `class_implements()` and `class_parents()` materialize
generated/AOT interface and parent metadata when the bridge exposes it.
`class_uses()` reports direct trait uses for eval-declared and generated/AOT
classes, traits, and enums. `class_alias()` can
alias eval-declared and generated/AOT
classes, interfaces, traits, and enums, preserving the target class-like kind
for the corresponding metadata probes. Top-level compiled `class_alias()`
calls still use elephc's generated subclass model, so eval sees the same AOT
parent metadata as the compiled program. `get_declared_classes()`,
`get_declared_interfaces()`, and `get_declared_traits()` expose eval-declared
names plus generated/AOT declaration names; enum names are included in the class
list like PHP. Aliases are usable for class-like lookups but are not added to
the declared-name lists. Eval-declared enums are visible inside eval through
`enum_exists()` and through class-like probes such as `class_exists()`.
`method_exists()` and `property_exists()` inspect eval-declared class/interface/
trait/enum metadata and generated runtime metadata. Object targets also see
dynamic public properties, and `property_exists()` follows PHP's current-scope
visibility for inherited private instance properties on object targets while
leaving class-string targets hidden. `get_class_methods()`, `get_class_vars()`,
and `get_object_vars()` follow PHP visibility from the current eval class scope
for eval-declared metadata and objects, and use generated/AOT runtime metadata
when available through the bridge-visible hook slice.

Eval-declared enums share the dynamic class-like metadata path used by
eval-declared classes. Pure and backed enum cases are singleton objects,
`EnumName::cases()` returns those singletons in declaration order, and backed
`EnumName::from()` / `EnumName::tryFrom()` compare against the declared scalar
values. `EnumName::from()` misses throw a catchable `ValueError`, while
`EnumName::tryFrom()` misses return `null`. Enums can implement eval-declared
or generated interfaces and can use their own instance/static methods and class
constants. Eval rejects enum declarations that redeclare reserved enum methods
or PHP-forbidden enum magic methods. Direct `new EnumName()` and property writes
to enum cases are rejected.

Public declared property reads/writes through `$this->property` from native
methods are bridged to eval. Protected and private declared property
reads/writes are bridged when the eval fragment runs inside a native class scope
that satisfies PHP visibility for the declaring class.
Public declared static property reads/writes are bridged to eval. Protected and
private declared static property reads/writes are bridged from native class
scopes that satisfy PHP visibility for the declaring class.
Public/protected/private fixed scalar/nullable scalar/Mixed/array/iterable/object
method parameters through `$this->method(...)` and `Class::method(...)` are
supported by the native method bridge when the eval fragment runs inside a
native class scope that satisfies PHP visibility for the declaring class,
including registered named arguments and string-keyed unpacking; method returns
may be scalar, nullable, callable, Mixed, array, iterable, object, union, and
intersection-object values.

## Namespaces and constants

Eval namespace declarations qualify function declarations, class declarations,
object construction names, and qualified references against the active
namespace. Unqualified function and constant references fall back to the global
builtin/constant namespace when the namespaced symbol is absent.

Simple and grouped `use`, `use function`, and `use const` declarations are
resolved while the bridge parser builds EvalIR: class imports rewrite `new`
targets, function imports rewrite unqualified calls, and constant imports
rewrite unqualified constant fetches in the active namespace declaration
region.

Inside eval, `define()` stores dynamic constants that persist across later eval
fragments, `defined()` probes them, and bare constant expressions fetch their
retained boxed values. Native `defined("Name")`, bare constant fetches, and
string-literal `class_exists("Name")` calls after an eval barrier also probe
eval-created dynamic symbols. Duplicate eval `define()` calls keep the first
value, return `false`, and emit the same suppressible duplicate-constant warning
as AOT `define()`.

Eval predefined constants include `PHP_EOL`, `PHP_OS`, `DIRECTORY_SEPARATOR`,
`PHP_INT_MAX`, `INF`, `NAN`, `PATHINFO_*`, `FNM_*`, `ARRAY_FILTER_USE_*`,
`COUNT_*`, and the supported `PREG_*` / `JSON_*` constants. `defined()` sees
these names, including an optional leading `\`, and `define()` cannot replace
them.

## Builtins available through eval

Eval builtin dispatch supports direct calls, named arguments, callable
dispatch, `call_user_func()`, `call_user_func_array()`, and `function_exists()`
where listed below unless a note says otherwise.

> **Strict mode:** in binaries compiled with
> [`--strict-php`](../compiling/cli-reference.md#strict-php-mode), the
> elephc-extension builtins below — the whole "Raw memory and buffers" row plus
> `class_attribute_names()`, `class_attribute_args()`, and
> `class_get_attributes()` — do not exist inside eval'd code either: calling one
> is a runtime fatal like any unknown function, and
> `function_exists()`/`is_callable()` report them as missing, matching the PHP
> interpreter.

| Area | Builtins |
|---|---|
| System, time, and environment | `time()`, `microtime()`, `hrtime()`, `date()`, `gmdate()`, `mktime()`, `gmmktime()`, `checkdate()`, `getdate()`, `localtime()`, `strtotime()`, `date_default_timezone_get()`, `date_default_timezone_set()`, `http_response_code()`, `header()`, `phpversion()`, `php_uname()`, `sleep()`, `usleep()`, `getcwd()`, `sys_get_temp_dir()`, `getenv()`, `putenv()` |
| Process execution | `exec()`, `shell_exec()`, `system()`, `passthru()`, `popen()`, `pclose()`, `readline()`, `die()`, `exit()` |
| Filesystem and paths | `file()`, `file_get_contents()`, `file_put_contents()`, `readfile()`, `file_exists()`, `is_file()`, `is_dir()`, `is_readable()`, `is_writable()`, `is_writeable()`, `filesize()`, `filemtime()`, `fileatime()`, `filectime()`, `fileperms()`, `fileowner()`, `filegroup()`, `fileinode()`, `filetype()`, `disk_free_space()`, `disk_total_space()`, `stat()`, `lstat()`, `is_executable()`, `is_link()`, `unlink()`, `copy()`, `rename()`, `mkdir()`, `rmdir()`, `chdir()`, `chmod()`, `chgrp()`, `chown()`, `lchgrp()`, `lchown()`, `touch()`, `symlink()`, `link()`, `readlink()`, `linkinfo()`, `clearstatcache()`, `scandir()`, `glob()`, `tempnam()`, `tmpfile()`, `umask()`, `basename()`, `dirname()`, `pathinfo()`, `fnmatch()`, `realpath()`, `realpath_cache_get()`, `realpath_cache_size()` |
| File and directory streams | `fopen()`, `fclose()`, `feof()`, `fflush()`, `fgetc()`, `fgets()`, `fgetcsv()`, `fpassthru()`, `fprintf()`, `fputcsv()`, `fread()`, `fscanf()`, `flock()`, `fseek()`, `fstat()`, `fsync()`, `fdatasync()`, `ftell()`, `ftruncate()`, `fwrite()`, `rewind()`, `vfprintf()`, `opendir()`, `readdir()`, `closedir()`, `rewinddir()` |
| Streams and stream contexts | `stream_get_filters()`, `stream_get_transports()`, `stream_get_wrappers()`, `stream_isatty()`, `stream_is_local()`, `stream_supports_lock()`, `stream_get_contents()`, `stream_get_line()`, `stream_get_meta_data()`, `stream_copy_to_stream()`, `stream_resolve_include_path()`, `stream_select()`, `stream_set_blocking()`, `stream_set_chunk_size()`, `stream_set_read_buffer()`, `stream_set_timeout()`, `stream_set_write_buffer()`, `stream_context_create()`, `stream_context_get_default()`, `stream_context_get_options()`, `stream_context_get_params()`, `stream_context_set_default()`, `stream_context_set_option()`, `stream_context_set_params()`, `stream_wrapper_register()`, `stream_wrapper_unregister()`, `stream_wrapper_restore()`, `stream_filter_register()`, `stream_filter_append()`, `stream_filter_prepend()`, `stream_filter_remove()`, `stream_bucket_new()`, `stream_bucket_make_writeable()`, `stream_bucket_append()`, `stream_bucket_prepend()` |
| Stream sockets and network databases | `stream_socket_server()`, `stream_socket_client()`, `stream_socket_accept()`, `stream_socket_enable_crypto()`, `stream_socket_get_name()`, `stream_socket_pair()`, `stream_socket_recvfrom()`, `stream_socket_sendto()`, `stream_socket_shutdown()`, `fsockopen()`, `pfsockopen()`, `gethostname()`, `gethostbyname()`, `gethostbyaddr()`, `getprotobyname()`, `getprotobynumber()`, `getservbyname()`, `getservbyport()`, `long2ip()`, `ip2long()`, `inet_pton()`, `inet_ntop()` |
| Strings, bytes, and formatting | `strlen()`, `ord()`, `chr()`, `strtolower()`, `strtoupper()`, `ucfirst()`, `lcfirst()`, `ucwords()`, `str_contains()`, `str_starts_with()`, `str_ends_with()`, `strpos()`, `strrpos()`, `strcmp()`, `strcasecmp()`, `trim()`, `ltrim()`, `rtrim()`, `chop()`, `strrev()`, `grapheme_strrev()`, `str_repeat()`, `substr()`, `substr_replace()`, `str_pad()`, `strstr()`, `str_split()`, `wordwrap()`, `nl2br()`, `explode()`, `implode()`, `str_replace()`, `str_ireplace()`, `htmlspecialchars()`, `htmlentities()`, `html_entity_decode()`, `urlencode()`, `urldecode()`, `rawurlencode()`, `rawurldecode()`, `ctype_alpha()`, `ctype_digit()`, `ctype_alnum()`, `ctype_space()`, `addslashes()`, `stripslashes()`, `bin2hex()`, `hex2bin()`, `base64_encode()`, `base64_decode()`, `gzcompress()`, `gzdeflate()`, `gzinflate()`, `gzuncompress()`, `number_format()`, `sprintf()`, `printf()`, `vsprintf()`, `vprintf()`, `sscanf()` |
| Hashing | `crc32()`, `hash()`, `hash_file()`, `hash_hmac()`, `md5()`, `sha1()`, `hash_equals()`, `hash_algos()`, `hash_init()`, `hash_update()`, `hash_final()`, `hash_copy()` |
| JSON | `json_encode()`, `json_decode()`, `json_validate()`, `json_last_error()`, `json_last_error_msg()` |
| Regex | `preg_match()`, `preg_match_all()`, `preg_replace()`, `preg_replace_callback()`, `preg_split()`, `mb_ereg_match()` |
| Arrays and sorting | `array_sum()`, `array_product()`, `array_chunk()`, `array_column()`, `array_combine()`, `array_fill()`, `array_fill_keys()`, `array_map()`, `array_filter()`, `array_reduce()`, `array_walk()`, `array_flip()`, `array_keys()`, `array_values()`, `array_diff()`, `array_intersect()`, `array_diff_key()`, `array_intersect_key()`, `range()`, `array_merge()`, `array_pad()`, `array_reverse()`, `array_slice()`, `array_splice()`, `array_unique()`, `array_key_exists()`, `array_rand()`, `in_array()`, `array_search()`, `array_pop()`, `array_shift()`, `array_push()`, `array_unshift()`, `arsort()`, `asort()`, `krsort()`, `ksort()`, `natcasesort()`, `natsort()`, `rsort()`, `shuffle()`, `sort()`, `uasort()`, `uksort()`, `usort()`, `count()` |
| Iterators and SPL | `iterator_count()`, `iterator_to_array()`, `iterator_apply()`, `spl_classes()`, `spl_object_id()`, `spl_object_hash()`, `spl_autoload()`, `spl_autoload_call()`, `spl_autoload_extensions()`, `spl_autoload_functions()`, `spl_autoload_register()`, `spl_autoload_unregister()` |
| Math and random | `abs()`, `sqrt()`, `floor()`, `ceil()`, `round()`, `pow()`, `clamp()`, `min()`, `max()`, `pi()`, `sin()`, `cos()`, `tan()`, `asin()`, `acos()`, `atan()`, `atan2()`, `sinh()`, `cosh()`, `tanh()`, `log()`, `log2()`, `log10()`, `exp()`, `deg2rad()`, `rad2deg()`, `hypot()`, `intdiv()`, `fdiv()`, `fmod()`, `rand()`, `mt_rand()`, `random_int()` |
| Raw memory and buffers | `buffer_new()`, `buffer_len()`, `buffer_free()`, `ptr()`, `ptr_null()`, `ptr_is_null()`, `ptr_offset()`, `ptr_get()`, `ptr_set()`, `ptr_read8()`, `ptr_read16()`, `ptr_read32()`, `ptr_read_string()`, `ptr_write8()`, `ptr_write16()`, `ptr_write32()`, `ptr_write_string()`, `ptr_sizeof()` |
| Types, metadata, and dynamic calls | `intval()`, `floatval()`, `strval()`, `boolval()`, `settype()`, `gettype()`, `get_called_class()`, `get_class()`, `get_parent_class()`, `get_class_methods()`, `get_class_vars()`, `get_object_vars()`, `get_resource_type()`, `get_resource_id()`, `function_exists()`, `is_callable()`, `class_exists()`, `interface_exists()`, `trait_exists()`, `enum_exists()`, `class_alias()`, `class_implements()`, `class_parents()`, `class_uses()`, `get_declared_classes()`, `get_declared_interfaces()`, `get_declared_traits()`, `method_exists()`, `property_exists()`, `is_a()`, `is_subclass_of()`, `class_attribute_names()`, `class_attribute_args()`, `class_get_attributes()`, `call_user_func()`, `call_user_func_array()`, `empty()`, `isset()`, `unset()`, `is_int()`, `is_integer()`, `is_long()`, `is_float()`, `is_double()`, `is_real()`, `is_nan()`, `is_finite()`, `is_infinite()`, `is_string()`, `is_bool()`, `is_null()`, `is_array()`, `is_object()`, `is_iterable()` for arrays and `Traversable` objects, `is_numeric()`, `is_scalar()`, `is_resource()` |
| Debug output | `print_r()`, `var_dump()` |
| Constants | `define()`, `defined()` |

## Builtin notes

Eval `settype()` mutates direct variables, array elements, object properties
including dynamic property names, and static properties including dynamic
receivers or dynamic property names. Callable dispatch of `settype()` remains
by-value and emits PHP's by-reference warning.

Eval supports the deprecated no-argument `get_class()` and `get_parent_class()`
forms inside class methods. They read the declaring method's class scope rather
than the late-static called class. Outside a class scope, no-argument
`get_class()` throws `Error`; no-argument `get_parent_class()` returns an empty
string, matching elephc's AOT lowering for parentless or scope-less lookups.

Eval `array_map()` supports one or more source arrays with supported callable
values or a `null` callback. `array_filter()`, `array_reduce()`,
`array_walk()`, `usort()`, `uasort()`, and `uksort()` share the same callback
dispatcher, including first-class function, method, and invokable-object callback values. One-array
`array_map()` results preserve source keys, multi-array results are reindexed,
missing source values are padded with `null`, and `array_map(null, ...)`
returns zipped row arrays.

Eval `count()` supports normal and recursive array counting and dispatches
top-level eval-declared or generated/AOT objects implementing `Countable`
through their `count()` method. `COUNT_RECURSIVE` still validates as a mode for
`Countable` objects, but the method result is used directly like PHP.

Eval `array_filter()` supports the PHP default omitted/null callback form,
filters falsey values, preserves source keys, and supports
`ARRAY_FILTER_USE_VALUE`, `ARRAY_FILTER_USE_BOTH`, and
`ARRAY_FILTER_USE_KEY`.

Eval mutating array builtins such as `array_pop()`, `array_shift()`,
`array_push()`, `array_unshift()`, `array_splice()`, `sort()`, `rsort()`,
`asort()`, `arsort()`, `ksort()`, `krsort()`, `natsort()`, `natcasesort()`,
`shuffle()`, `usort()`, `uksort()`, and `uasort()` write back through direct
variable calls. When reached through dynamic callable dispatch, they follow
PHP's by-value callback behavior: the return value is computed from the
supplied array, a by-reference warning is emitted where PHP would emit one, and
the caller's original array is not mutated.

Eval regex dispatch uses PCRE2 through the POSIX wrapper for common PCRE-style
delimited patterns. It strips PHP delimiters, supports the `i`, `m`, `s`, `u`,
and `U` modifiers, supports common capture array shapes and replacement
references, and supports `PREG_SPLIT_NO_EMPTY`, `PREG_SPLIT_DELIM_CAPTURE`, and
`PREG_SPLIT_OFFSET_CAPTURE`. Patterns, delimiters, modifiers, or subject bytes
that the eval bridge cannot pass through this wrapper fail as eval runtime
fatals. Native non-eval regex codegen remains PCRE2-backed as documented in
[Regex](regex.md).

Eval JSON support covers null, booleans, integers, floats, strings, indexed
arrays, associative arrays, and `stdClass` dynamic properties. `json_encode()`
supports zero flags plus the documented `JSON_HEX_*`,
`JSON_UNESCAPED_SLASHES`, `JSON_UNESCAPED_UNICODE`, `JSON_FORCE_OBJECT`,
`JSON_NUMERIC_CHECK`, `JSON_PARTIAL_OUTPUT_ON_ERROR`, `JSON_PRETTY_PRINT`,
`JSON_PRESERVE_ZERO_FRACTION`, `JSON_INVALID_UTF8_IGNORE`,
`JSON_INVALID_UTF8_SUBSTITUTE`, and `JSON_THROW_ON_ERROR` flags. `json_decode()`
and `json_validate()` support PHP-compatible depth handling, malformed UTF-8
ignore/substitute modes where applicable, `JSON_BIGINT_AS_STRING` for
overflowing integer tokens in `json_decode()`, and `JsonException` through
`JSON_THROW_ON_ERROR`.

Eval local filesystem calls operate on host filesystem paths and support the
implemented stream-wrapper paths, including `file://`, `php://memory`,
`data://`, `phar://`, plain `http://`, and eval-registered userspace wrappers.
Eval also supports complete `fstat()` arrays and portable ownership/group
metadata calls where the host platform exposes them. TLS-backed `https://`
URLs remain outside magician's implemented wrapper paths.

Eval `print_r()` supports the normal echoing form and `print_r($value, true)`.
Scalars print through the same output path as `echo`, boolean false and null
print nothing, arrays use PHP's recursive `Array\n(\n    [key] => value\n)\n`
shape, and objects render class names plus bridge-visible properties.

Eval `var_dump()` supports one or more arguments. Scalars print typed
diagnostic lines, indexed or associative arrays print foreach-visible keys and
nested values through eval value hooks, and objects print class names, object
ids, bridge-visible properties, eval-declared private/protected/public property
labels, and eval property references when alias metadata is available.

## Current limitations

Dynamic fragments and literal fragments outside the current AOT eligibility
rules execute through the `elephc_magician` interpreter bridge. Eligible
literal fragments instead use the normal AST -> EIR -> native codegen pipeline,
either with direct read parameters or with core eval-scope helpers. Unsupported
constructs that reach the interpreter, and missing class names during eval
object construction, fail at runtime with an eval fatal diagnostic.

The fragment subset is broad but not the full elephc language surface. Eval
retains `ReflectionFunction` closure metadata for common
`Closure::fromCallable()` targets, including bound free functions, object
methods, static methods, and invokable objects. Advanced native callable
descriptors and full PHP `Closure` APIs, such as arbitrary scope mutation and
reflection over every first-class callable shape, are still outside eval
fragments. Runtime/AOT free-function, object-method, static-method, and
constructor fallback from eval
can bind registered names, defaults, and positional variadic tails; method,
static-method, and constructor fallback can also bind by-reference lvalue
metadata. Generated AOT bridge dispatch supports visibility-checked method,
static-method, and constructor signatures whose generated ABI storage is
scalar/string, callable descriptor, boxed Mixed/union, array/hash, iterable, or
object, plus free-function signatures using the descriptor invoker ABI when
by-reference parameters, if any, use boxed Mixed/union storage or raw one-word
scalar storage (`int`, `bool`, or `float`), string raw storage, or one-word heap
storage (`array`, `iterable`, and object/class parameters), including
positional `&...` variadic tail element writeback.
Registered type specs validate nullable and union members, intersection object
parameters, and intersection object returns before or after the generated AOT
call. By-reference method and constructor parameters remain limited to the
staged scalar/nullable scalar/Mixed/array/iterable/object slice, including the
generated positional variadic array slot when present. These supported method
and constructor shapes use normal target ABI materialization for arguments
beyond the register set instead of a fixed eval arity cap. Layouts such as
pointer, buffer, packed, resource, and other specialized native-only ABI shapes
remain outside those bridge paths.

Eval class support is still smaller than the full static class system.
Eval-declared `__destruct()` hooks run when an eval-owned dynamic object reaches
final release through ordinary eval statement execution, such as `unset($obj)` or
a discarded temporary expression, and through the native runtime final-release
path after an eval-owned object escapes back into compiled code, including
runtime cycle collection of eval-owned object graphs. The main remaining
class-system gaps are
broader reflection APIs beyond the supported
ReflectionClass/Object/Function/Method/Parameter/Property/NamedType/UnionType/IntersectionType
and Enum/attribute slice, Reflection type APIs beyond retained parameter, generated
property, and function/method return metadata, generated property default-value
materialization beyond scalar, null, empty-array, and supported array-valued
defaults, parameter defaults beyond representable scalar/string constant
expressions, resolved global/named class-like constants, supported array-valued
defaults, and supported object-valued defaults during generated/AOT invocation,
and generated/AOT method and constructor
bridge signatures beyond the current visibility-checked by-reference parameter
slice, specialized native-only ABI shapes, and `__clone()` hooks; the remaining
limit there is the supported type slice rather than a fixed parameter count.
Generated/AOT free-function and method type metadata, return metadata,
by-reference and variadic parameter flags, and generated/AOT
class/method/property/class-constant attributes are exposed for registered
metadata slices, while unsupported native-only bridge shapes and raw-storage
by-reference free-function bridge shapes beyond the current
Mixed/scalar/string/one-word heap slice remain metadata-only rather than
invocable through eval.

The type checker and AST optimizer conservatively treat every `eval()` call as
a dynamic barrier: local facts are widened, constant propagation is invalidated,
and pre-call facts cannot be blindly reused afterward. Later EIR planning may
prove that a literal fragment can run natively and omit the runtime barrier;
values that actually cross a dynamic barrier use boxed `Mixed` storage.
