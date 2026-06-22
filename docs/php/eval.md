---
title: "Eval"
description: "Runtime PHP fragment evaluation, dynamic scope synchronization, supported EvalIR subset, and current limitations."
sidebar:
  order: 5
---

`eval($code): mixed` parses and executes a PHP fragment at runtime in the
caller-visible local scope. It is a PHP language construct, not a normal
callable: `function_exists("eval")` and `is_callable("eval")` return `false`,
and first-class callable syntax for `eval` is rejected.

Programs that call `eval()` link the optional `elephc_eval` bridge. Programs
that do not use `eval()` keep the ordinary fully native runtime path and do not
link the bridge.

The evaluated string must be a PHP fragment without an opening `<?php` tag.

## Scope behavior

Variables from the caller's local scope are visible in the fragment.
Assignments and `unset()` are reflected back into that scope, variables created
by the fragment remain visible after `eval()`, and `return expr;` returns from
the `eval()` call itself.

`eval()` is a dynamic barrier for native code. The compiler flushes visible
locals into a materialized eval scope before entering the bridge, then reloads
locals that may have been read, written, created, or unset by the evaluated
fragment. Runtime cells use elephc's boxed `Mixed` representation, so the eval
interpreter does not introduce a second PHP value ABI.

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
| Assignment forms | Simple variable assignment, compound assignment (`+=`, `-=`, `*=`, `**=`, `/=`, `%=`, `&=`, `|=`, `^=`, `<<=`, `>>=`, `.=`), and simple variable increment/decrement (`++$x`, `$x++`, `--$x`, `$x--`) are supported. |
| Control flow | Braced and single-statement `if`/`elseif`/`else`, `else if`, `while`, `do/while`, `for`, `foreach`, `switch`, `break`, and `continue` are supported. |
| Exceptions | `throw`, `try`, `catch`, union catches, class-specific catches, optional catch variables, and `finally` are supported. `finally` runs before a fragment returns or propagates a `Throwable`; a control action from `finally` replaces the pending action from the protected body or catch. |
| Functions | Eval fragments can declare functions. Static locals inside eval-declared functions are initialized once per eval context and persist across later calls through that context. Top-level `static` declarations in separate eval fragments are initialized for each eval execution. |
| Classes | Eval fragments can declare classes with properties, asymmetric property write visibility (`private(set)` / `protected(set)`), constructor property promotion including by-reference promotion for variable, array-element, object-property, and default-value targets, concrete property get/set hooks, methods, `__construct()`, inheritance, visibility, readonly properties/classes, abstract/final modifiers, trait uses with `insteadof` / `as` adaptations, interface implementations, static members, class/interface/trait constants including `final` constants, and class-level attributes. Duplicate eval class-like names are rejected. |
| Enums | Eval fragments can declare pure and `int` / `string` backed enums with cases, constants including `final` constants, methods, interface implementations, `::cases()`, `::from()`, `::tryFrom()`, `->name`, and backed `->value`. |
| Includes | `include`, `include_once`, `require`, and `require_once` execute local filesystem paths from inside fragments. |
| Namespaces | Both `namespace Name;` and `namespace Name { ... }` forms are supported, including simple and grouped `use`, `use function`, and `use const` declarations. |

`foreach` supports value-only and key-value iteration over indexed and
associative arrays. Eval associative arrays preserve PHP insertion order for
iteration.

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
| Variables and properties | Variable reads, `$this->property` reads/writes from native methods, dynamic `stdClass` properties, eval object property access including `__get()` / `__set()` fallback for missing or inaccessible eval properties, `isset()`, `empty()`, and `unset()` magic property dispatch through `__isset()` / `__unset()`, `instanceof` over static and dynamic class/interface targets, static property access, and class constant fetches through the bridge. |
| Arrays | Indexed and associative literals, modern `[...]` and legacy `array(...)`, keyed elements, append writes (`$array[] = value`), numeric-index reads/writes, and string-key reads/writes. |
| Function-like calls | Direct calls, named arguments, argument unpacking (`...`), dynamic string/expression calls, invokable eval objects, `call_user_func()`, and `call_user_func_array()` for supported call targets. |
| Object construction | `new ClassName(...)` for eval-declared classes, including constructor named arguments and unpacking; anonymous `new class [(args)] [extends Parent] [implements Iface, ...] { ... }` expressions; `stdClass` and emitted AOT classes visible through runtime metadata support positional arguments, named arguments, numeric unpacking, string-keyed named unpacking, and registered scalar or null default arguments for supported public scalar/Mixed constructor signatures. |
| Object cloning | `clone $object` shallow-copies eval-declared objects, `stdClass` storage, and ordinary emitted/AOT object storage. Eval-declared `__clone()` hooks run after the copy and obey public/protected/private visibility; public emitted/AOT `__clone()` hooks run through the method bridge. |
| Method calls | Eval-declared object and static method calls support positional arguments, named arguments, numeric unpacking, string-keyed named unpacking, and by-reference parameters for direct variable, array-element, and object-property arguments. Missing or inaccessible eval methods dispatch through `__call()` / `__callStatic()` when those hooks are available. Runtime/AOT object-method and static-method fallback supports the same argument binding plus registered scalar or null default arguments for supported public scalar/Mixed/object method signatures. |
| Includes | `include`, `include_once`, `require`, and `require_once` are expressions. |
| Magic constants | `__LINE__`, call-site `__FILE__` / `__DIR__`, empty eval-scope `__CLASS__` / `__TRAIT__`, namespace-aware `__NAMESPACE__`, and eval-declared-function `__FUNCTION__` / `__METHOD__`. |
| Constants | Predefined eval-visible constants, dynamic constants from `define()`, namespaced constant fallback, and bare constant fetches are supported. |
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
parameters.

String-variable and expression callable calls such as `$fn(...)` and
`$callbacks[0](...)` share the eval callable dispatcher for supported builtins,
eval-declared functions, and registered AOT functions.

Inside eval fragments, two-element object-method callable arrays such as
`[$this, "method"]` can be invoked through `$cb(...)`, `call_user_func($cb,
...)`, `call_user_func_array($cb, [...])`, and `iterator_apply()`. Eval-declared
object methods support string-keyed named arguments through
`call_user_func_array()`. Eval-declared objects with `__invoke()` can be called
through `$object(...)`, `call_user_func($object, ...)`, and
`call_user_func_array($object, [...])`, and `is_callable($object)` reports them
as callable. Static method callables can use `["ClassName", "method"]` or
`"ClassName::method"` through `$cb(...)`, `call_user_func()`, and
`call_user_func_array()`. Eval-declared static methods also support string-keyed
named arguments through `call_user_func_array()`; generated/AOT static method
fallback supports the same named-argument binding for public scalar/Mixed/object
signatures supported by the generated bridge.

Post-barrier native direct calls and string-literal `call_user_func()` callbacks
currently accept simple positional arguments. Post-barrier
`call_user_func_array()` callbacks can pass indexed or string-keyed argument
containers to eval-declared functions.

## Classes and objects

Eval-declared classes support inheritance, public/protected/private properties
and methods, asymmetric property write visibility (`private(set)` /
`protected(set)`), constructor property promotion including by-reference promotion
for variable, array-element, object-property, and default-value targets,
concrete property `get` / `set` hooks, interface property hook contract checks,
abstract property hook contracts, property-level `readonly`, `readonly class`,
`__construct()`, abstract classes and methods, final classes, methods, and
properties, trait composition with `insteadof` conflict resolution and `as`
aliases/visibility adaptations, interface implementation checks, static
properties, static methods, static interface method contracts, class, interface,
trait, and enum constants including `final` constants, class-level attributes,
`ClassName::class` literals, magic method fallback through `__call()` and
`__callStatic()`, and magic property fallback through `__get()` and `__set()`.
Eval validates method override and interface method return types with PHP-style
covariance for supported declared return type metadata, including nullable,
union, `mixed`, `self`, `parent`, `static`, class, and interface return types.
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
Eval validates magic method staticness, visibility, and arity contracts for
`__toString()`, `__get()`, `__set()`, `__isset()`, `__unset()`, `__call()`,
`__callStatic()`, `__invoke()`, `__clone()`, `__destruct()`, and `__construct()`
when dynamic classes or traits are declared.
Member
visibility is checked at runtime for eval-declared objects and
static/class-constant accesses. Class-level attributes declared on eval classes,
interfaces, traits, and enums are visible through `class_attribute_names()`,
`class_attribute_args()`, and `class_get_attributes()` when their arguments are
supported literal positional values (`string`, `int`, `bool`, `null`, or negated
integer literals). `ReflectionAttribute::newInstance()` instantiates
eval-declared attribute classes from those materialized attributes, and
`ReflectionAttribute::getTarget()` / `isRepeated()` report the reflected owner
target and same-owner repetition metadata.
Attribute names remain visible when an attribute uses unsupported argument
syntax, but requesting those arguments is a runtime fatal.
Private parent properties shadowed by same-named child properties use separate
runtime storage, so parent methods keep seeing the private parent value while
child methods and public access see the child property.
`ReflectionClass::getAttributes()`, `ReflectionMethod::getAttributes()`,
`ReflectionProperty::getAttributes()`, and `ReflectionParameter::getAttributes()`
expose eval-retained class, method, property, and method-parameter attributes
for eval-declared class-like symbols when their arguments fit the same literal
subset, and `getName()` returns the reflected class, member, or parameter name
for those owners. `ReflectionClass`, `ReflectionFunction`, `ReflectionMethod`,
`ReflectionProperty`, `ReflectionClassConstant`, `ReflectionEnumUnitCase`, and
`ReflectionEnumBackedCase` expose `getDocComment()` and report `false` because
eval does not retain docblock text. `ReflectionClass`, `ReflectionFunction`,
and `ReflectionMethod` expose `getExtensionName()` and `getExtension()` and
report `false` / `null` for eval-declared user symbols.
`ReflectionClass` construction accepts class-name strings and object arguments;
object arguments reflect the runtime class of eval-created or generated/AOT
objects.
`ReflectionMethod` construction accepts class-name strings and object
arguments; object arguments resolve to the runtime class before method lookup.
`ReflectionClass::getShortName()`,
`ReflectionClass::getNamespaceName()`, and `ReflectionClass::inNamespace()`
derive namespace-aware parts from the resolved eval class-like name.
`ReflectionFunction::getShortName()`, `getNamespaceName()`, and
`inNamespace()` derive namespace-aware parts from the reflected eval function
name. `ReflectionMethod::getShortName()` reports the reflected method name,
while `ReflectionMethod::getNamespaceName()` reports an empty string and
`inNamespace()` reports `false`, matching PHP's method reflection behavior.
`ReflectionFunction` and `ReflectionMethod` report eval user-symbol defaults
through `isInternal()`, `isUserDefined()`, `isClosure()`, `isDeprecated()`,
`returnsReference()`, `isGenerator()`, `isVariadic()`,
`hasTentativeReturnType()`, and `getTentativeReturnType()`. `hasReturnType()`
and `getReturnType()` expose retained eval return type metadata for supported
named, nullable, union, and intersection declarations, including `void` and
`never` as builtin non-nullable named types.
`ReflectionFunction::isDisabled()` reports `false` for eval-visible functions.
`ReflectionClass::isFinal()`, `ReflectionClass::isAbstract()`,
`ReflectionClass::isInterface()`, `ReflectionClass::isTrait()`, and
`ReflectionClass::isEnum()` report eval class-like metadata, including
PHP-compatible enum finality and class-like kind checks for eval interfaces,
traits, and enums. `ReflectionClass::isReadOnly()` reports eval `readonly class`
metadata. `ReflectionClass::isAnonymous()` reports true for eval anonymous
classes and false for eval-declared named class-like symbols.
`ReflectionClass::isInstantiable()` reports whether eval class-like metadata
describes a concrete class with no constructor or a public constructor.
`ReflectionClass::isCloneable()` reports whether eval class metadata describes
a concrete class with no `__clone()` or a public `__clone()`.
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
returns traits used directly by eval classes, `ReflectionClass::getTraits()`
materializes those direct trait names as `ReflectionClass` objects, and
`ReflectionClass::getTraitAliases()` exposes direct eval trait `as` aliases as
PHP's alias-name to `Trait::method` map.
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
`ReflectionClass::hasConstant()`, `getConstant()`, `getConstants()`,
`getDefaultProperties()`, `getStaticProperties()`,
`getStaticPropertyValue()`, `setStaticPropertyValue()`,
`getReflectionConstant()`, and `getReflectionConstants()` expose eval-visible
class constants, interface constants, trait constants, enum constants, enum
cases, supported materialized property defaults, and current eval-declared
static property values. Constant lookup is case-sensitive; single-value
lookups return `false` when no constant or case is visible. `getConstants()`
and `getReflectionConstants()` accept PHP's `ReflectionClassConstant::IS_*`
visibility/finality filter bitmask; `null` means no filter and `0` returns no
constants.
`ReflectionClass::getMethods()` and `ReflectionClass::getProperties()` return
materialized `ReflectionMethod` and `ReflectionProperty` objects for the same
visible member metadata, including supported member attributes and predicate
flags. For generated/AOT classes, `ReflectionClass::getMethod()` /
`getProperty()` and `getMethods()` / `getProperties()` materialize reflection
objects from emitted member-name and predicate metadata, including optional
modifier filters. AOT method reflection also exposes registered parameter
names, declared parameter types, declared return types, required/optional
counts, and registered scalar or null default values for generated
constructor, instance-method, and static-method signatures. AOT property
reflection exposes registered declared property types and supported scalar,
string, or null default values for generated property metadata. AOT method and
property reflection expose generated member attributes when their arguments fit
the materializable literal subset.
`ReflectionMethod::getDeclaringClass()` and
`ReflectionProperty::getDeclaringClass()` return a materialized
`ReflectionClass` for the symbol that declares the reflected
member. `ReflectionMethod::hasPrototype()` and `getPrototype()` expose
eval parent-class overrides and interface implementation prototypes; inherited
methods that are not overridden report no prototype, matching PHP reflection.
`ReflectionClass::getConstructor()` returns a materialized
`ReflectionMethod` for direct, inherited, interface, trait, and generated/AOT
constructors, including registered generated/AOT constructor parameter names,
counts, and scalar/null defaults where available; it returns `null` when no
constructor is visible. `ReflectionClass::getParentClass()`
returns a materialized `ReflectionClass` for eval-declared and generated/AOT
parent classes or `false` when no parent class exists.
`ReflectionClass::newInstance()` constructs
eval-declared reflected classes and forwards constructor arguments through
eval's positional, named, and unpacking-aware call binding.
`ReflectionClass::newInstanceArgs()` constructs eval-declared reflected classes
from an argument array, treating string keys as named constructor arguments.
`ReflectionClass::newInstanceWithoutConstructor()` allocates eval-declared
reflected classes, initializes supported property defaults, and skips
`__construct()`.
`ReflectionMethod::invoke()` and `invokeArgs()` call eval-declared reflected
methods, bypass public/protected/private visibility like PHP reflection,
preserve named arguments for the invoked method, follow PHP's by-value
`invoke()` variadic forwarding, accept `null` or an object for static methods,
and throw catchable `ReflectionException` values when an instance receiver is
not compatible with the reflected declaring class.
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
method metadata, plus registered generated/AOT method parameter names, declared
parameter and return types, required/optional counts, and scalar or null default
values when native method/static-method signatures are registered. Eval-declared
functions and methods expose declared-type presence for parameters and return types, simple
named type metadata through
`ReflectionParameter::getType()` / `ReflectionNamedType::getName()`,
`allowsNull()`, and `isBuiltin()`, and the legacy
`ReflectionParameter::isArray()` / `isCallable()` predicates for named `array`
and `callable` parameter types. Multi-member union metadata is exposed through
`ReflectionUnionType::getTypes()` and `allowsNull()`. Intersection parameter
metadata is exposed through `ReflectionIntersectionType::getTypes()` and
`allowsNull()`. Function, method, and parameter attributes are exposed through
`getAttributes()` using materialized `ReflectionAttribute` objects. Parameter
default values, optionality, nullability, variadic flags, and by-reference
flags are retained for eval-declared functions and methods, including
`ReflectionParameter::allowsNull()`. `ReflectionParameter::getDeclaringClass()`
returns the declaring class-like symbol for eval method parameters, and
`ReflectionParameter::getDeclaringFunction()` returns a `ReflectionFunction`
object for eval free-function parameters or a `ReflectionMethod` object for the
declaring eval method. `ReflectionFunction::invoke()` and `invokeArgs()`
dispatch eval-declared functions with the same named/default/variadic argument
binding used by direct eval function calls. Defaulted eval method parameters are
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
non-spread constructor arguments. Late-bound `static::` defaults and unpacked
constructor arguments in defaults are rejected like PHP constant expressions.
Variadic eval method parameters bind extra positional and unknown named
arguments into a PHP array and are reported through
`ReflectionParameter::isVariadic()` and `ReflectionParameter::isOptional()`.
Constructor-promoted eval parameters are reported through
`ReflectionParameter::isPromoted()`. By-reference eval method parameters accept
direct variable, array-element, and object-property arguments, write back fixed
parameters after method execution, write back mutated `&...$items` elements
when the variadic container itself is not rebound, and are reported through
`ReflectionParameter::isPassedByReference()` and
`ReflectionParameter::canBePassedByValue()`.
`ReflectionProperty::isStatic()`, `isPublic()`, `isProtected()`, `isPrivate()`,
`isFinal()`, `isAbstract()`, `isReadOnly()`, `isPromoted()`, `isVirtual()`,
`isDynamic()`, `isProtectedSet()`, `isPrivateSet()`, `isInitialized()`,
`isDefault()`, and `getModifiers()` report eval property
metadata with PHP-compatible `ReflectionProperty::IS_*` constants for the
bitmask. `isPromoted()` reports generated/AOT and eval-declared
promoted-property metadata. `isProtectedSet()` and `isPrivateSet()` derive from
the retained modifier bitmask, including eval-declared and generated/AOT
asymmetric visibility plus public readonly property metadata. `isDynamic()` reports `false` for supported
declared properties and `true` for public dynamic object properties
materialized with `new ReflectionProperty($object, $property_name)`.
`ReflectionProperty::isDefault()` is the inverse for those supported dynamic
properties. `isInitialized()` tracks eval-backed instance and static property
storage, including typed properties without defaults, unset properties, virtual
property hooks, and public dynamic properties on the inspected object.
`ReflectionProperty::hasType()`, `getType()`, and `getSettableType()` expose
retained property type metadata through `ReflectionNamedType`,
`ReflectionUnionType`, and `ReflectionIntersectionType` where eval has retained
a supported declared type. For the supported property surface,
`getSettableType()` currently returns the same retained type metadata as
`getType()`.
`ReflectionProperty::hasDefaultValue()` and `getDefaultValue()` expose
materialized property default metadata, including PHP's implicit `null` default
for untyped concrete properties without an explicit initializer.
`ReflectionProperty::__toString()` formats retained eval/generated property
metadata as a PHP-style `Property [ ... ]` descriptor for the supported
visibility, static, type, default, and virtual-property surface.
`ReflectionProperty::hasHooks()`, `hasHook()`, `getHooks()`, and `getHook()`
expose eval-declared concrete, abstract, and interface property get/set hook
metadata and return hook `ReflectionMethod` objects using PHP's
`$property::get` / `$property::set` names. Eval also exposes
`PropertyHookType::Get` and `PropertyHookType::Set` inside evaluated fragments
for those APIs, including `PropertyHookType::cases()`, `from()`, and
`tryFrom()`.
`ReflectionProperty::setAccessible()` is accepted as a PHP-compatible no-op.
`ReflectionProperty::getValue()` and `setValue()` read and write eval-declared
instance and static property values, bypass public/protected/private visibility
like PHP reflection, route concrete property hooks through their accessors, and
still reject readonly writes.
`ReflectionProperty::getRawValue()` and `setRawValue()` are supported for
eval-declared backed instance properties, including backed property hooks, and
bypass concrete property hook accessors. Virtual property hooks reject raw
access like PHP. `ReflectionProperty::isLazy()` reports `false` for
eval-declared properties because eval does not implement lazy properties;
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
and `getDeclaringClass()` returns the declaring class or enum as a
`ReflectionClass`. `ReflectionClassConstant::isEnumCase()` reports enum cases.
`ReflectionClassConstant::isPublic()`, `isProtected()`, `isPrivate()`,
`isFinal()`, and `getModifiers()` report visibility/finality metadata with
PHP's `ReflectionClassConstant::IS_*` bitmasks; enum cases report public,
non-final constants.
Concrete property hooks are lowered to eval accessor methods; reads and writes
route through inherited hooks, while access from the accessor itself uses the
raw backing slot. `readonly` eval properties may be assigned from the
constructor of the declaring class and later writes fail as eval runtime fatals.
A `readonly class` makes instance properties readonly implicitly while leaving
static properties mutable. `self::`, `parent::`, and late-bound `static::` work
for supported static members, class constants, and class-name literals.

Eval object construction can allocate eval-declared classes, `stdClass`, and
emitted AOT classes visible through runtime class metadata. Missing class names
during eval object construction fail with an eval runtime fatal diagnostic.
`clone $object` creates a shallow copy for eval-declared objects, `stdClass`
objects, and ordinary emitted/AOT objects. Eval `__clone()` hooks are invoked on
the cloned object after storage copying and use the same runtime visibility
checks as method calls. Public emitted/AOT `__clone()` hooks are invoked through
the generated method bridge after the clone storage has been copied.

AOT and eval-declared class-name probes are visible through `class_exists()`.
Eval object relation probes through `instanceof`, `is_a()`, and `is_subclass_of()` use
generated AOT class/interface metadata and eval-created object metadata.
`interface_exists()`, `trait_exists()`, and `enum_exists()` can probe generated
AOT metadata. `class_alias()` can alias eval-declared and generated/AOT
classes, interfaces, traits, and enums, preserving the target class-like kind
for the corresponding metadata probes. Aliases are usable for class-like
lookups but are not added to `get_declared_classes()`,
`get_declared_interfaces()`, or `get_declared_traits()`. Eval-declared enums are
visible inside eval through `enum_exists()` and through class-like probes such
as `class_exists()`.
`method_exists()` and `property_exists()` inspect eval-declared class/interface/
trait/enum metadata and generated runtime metadata. Object targets also see
dynamic public properties. `get_class_methods()` and `get_object_vars()` follow
PHP visibility from the current eval class scope for eval-declared objects;
generated/AOT objects expose the public bridge-visible metadata and object
properties available through the runtime hook slice.

Eval-declared enums share the dynamic class-like metadata path used by
eval-declared classes. Pure and backed enum cases are singleton objects,
`EnumName::cases()` returns those singletons in declaration order, and backed
`EnumName::from()` / `EnumName::tryFrom()` compare against the declared scalar
values. `EnumName::from()` misses throw a catchable `ValueError`, while
`EnumName::tryFrom()` misses return `null`. Enums can implement eval-declared
or generated interfaces and can use their own instance/static methods and class
constants. Direct `new EnumName()` and property writes to enum cases are
rejected.

Public declared property reads/writes through `$this->property` from native
methods are bridged to eval. Public fixed scalar/Mixed/object method calls
through `$this->method(...)` are supported by the native method bridge,
including registered named arguments and string-keyed unpacking.

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

| Area | Builtins |
|---|---|
| System, time, and environment | `time()`, `microtime()`, `date()`, `mktime()`, `strtotime()`, `phpversion()`, `php_uname()`, `sleep()`, `usleep()`, `getcwd()`, `sys_get_temp_dir()`, `getenv()`, `putenv()` |
| Filesystem and paths | `file()`, `file_get_contents()`, `file_put_contents()`, `readfile()`, `file_exists()`, `is_file()`, `is_dir()`, `is_readable()`, `is_writable()`, `is_writeable()`, `filesize()`, `filemtime()`, `fileatime()`, `filectime()`, `fileperms()`, `fileowner()`, `filegroup()`, `fileinode()`, `filetype()`, `disk_free_space()`, `disk_total_space()`, `stat()`, `lstat()`, `is_executable()`, `is_link()`, `unlink()`, `copy()`, `rename()`, `mkdir()`, `rmdir()`, `chdir()`, `chmod()`, `touch()`, `symlink()`, `link()`, `readlink()`, `linkinfo()`, `clearstatcache()`, `scandir()`, `glob()`, `tempnam()`, `umask()`, `basename()`, `dirname()`, `pathinfo()`, `fnmatch()`, `realpath()`, `realpath_cache_get()`, `realpath_cache_size()` |
| Stream introspection | `stream_get_filters()`, `stream_get_transports()`, `stream_get_wrappers()` |
| Network and protocol databases | `gethostname()`, `gethostbyname()`, `gethostbyaddr()`, `getprotobyname()`, `getprotobynumber()`, `getservbyname()`, `getservbyport()`, `long2ip()`, `ip2long()`, `inet_pton()`, `inet_ntop()` |
| Strings, bytes, and formatting | `strlen()`, `ord()`, `chr()`, `strtolower()`, `strtoupper()`, `ucfirst()`, `lcfirst()`, `ucwords()`, `str_contains()`, `str_starts_with()`, `str_ends_with()`, `strpos()`, `strrpos()`, `strcmp()`, `strcasecmp()`, `trim()`, `ltrim()`, `rtrim()`, `chop()`, `strrev()`, `str_repeat()`, `substr()`, `substr_replace()`, `str_pad()`, `strstr()`, `str_split()`, `wordwrap()`, `nl2br()`, `explode()`, `implode()`, `str_replace()`, `str_ireplace()`, `htmlspecialchars()`, `htmlentities()`, `html_entity_decode()`, `urlencode()`, `urldecode()`, `rawurlencode()`, `rawurldecode()`, `ctype_alpha()`, `ctype_digit()`, `ctype_alnum()`, `ctype_space()`, `addslashes()`, `stripslashes()`, `bin2hex()`, `hex2bin()`, `base64_encode()`, `base64_decode()`, `number_format()`, `sprintf()`, `printf()`, `vsprintf()`, `vprintf()`, `sscanf()` |
| Hashing | `crc32()`, `hash()`, `hash_file()`, `hash_hmac()`, `md5()`, `sha1()`, `hash_equals()`, `hash_algos()` |
| JSON | `json_encode()`, `json_decode()`, `json_validate()`, `json_last_error()`, `json_last_error_msg()` |
| Regex | `preg_match()`, `preg_match_all()`, `preg_replace()`, `preg_replace_callback()`, `preg_split()` |
| Arrays and sorting | `array_sum()`, `array_product()`, `array_chunk()`, `array_column()`, `array_combine()`, `array_fill()`, `array_fill_keys()`, `array_map()`, `array_filter()`, `array_reduce()`, `array_walk()`, `array_flip()`, `array_keys()`, `array_values()`, `array_diff()`, `array_intersect()`, `array_diff_key()`, `array_intersect_key()`, `range()`, `array_merge()`, `array_pad()`, `array_reverse()`, `array_slice()`, `array_splice()`, `array_unique()`, `array_key_exists()`, `array_rand()`, `in_array()`, `array_search()`, `array_pop()`, `array_shift()`, `array_push()`, `array_unshift()`, `arsort()`, `asort()`, `krsort()`, `ksort()`, `natcasesort()`, `natsort()`, `rsort()`, `shuffle()`, `sort()`, `uasort()`, `uksort()`, `usort()`, `count()` |
| Iterators and SPL | `iterator_count()`, `iterator_to_array()`, `iterator_apply()`, `spl_classes()`, `spl_object_id()`, `spl_object_hash()` |
| Math and random | `abs()`, `sqrt()`, `floor()`, `ceil()`, `round()`, `pow()`, `clamp()`, `min()`, `max()`, `pi()`, `sin()`, `cos()`, `tan()`, `asin()`, `acos()`, `atan()`, `atan2()`, `sinh()`, `cosh()`, `tanh()`, `log()`, `log2()`, `log10()`, `exp()`, `deg2rad()`, `rad2deg()`, `hypot()`, `intdiv()`, `fdiv()`, `fmod()`, `rand()`, `mt_rand()`, `random_int()` |
| Types, metadata, and dynamic calls | `intval()`, `floatval()`, `strval()`, `boolval()`, `settype()`, `gettype()`, `get_class()`, `get_parent_class()`, `get_class_methods()`, `get_object_vars()`, `get_resource_type()`, `get_resource_id()`, `function_exists()`, `is_callable()`, `class_exists()`, `interface_exists()`, `trait_exists()`, `enum_exists()`, `class_alias()`, `class_implements()`, `class_parents()`, `class_uses()`, `get_declared_classes()`, `get_declared_interfaces()`, `get_declared_traits()`, `method_exists()`, `property_exists()`, `is_a()`, `is_subclass_of()`, `class_attribute_names()`, `class_attribute_args()`, `class_get_attributes()`, `call_user_func()`, `call_user_func_array()`, `is_int()`, `is_integer()`, `is_long()`, `is_float()`, `is_double()`, `is_real()`, `is_nan()`, `is_finite()`, `is_infinite()`, `is_string()`, `is_bool()`, `is_null()`, `is_array()`, `is_object()`, `is_iterable()`, `is_numeric()`, `is_resource()` |
| Debug output | `print_r()`, `var_dump()` |
| Constants | `define()`, `defined()` |

## Builtin notes

Eval `array_map()` supports one or more source arrays with a string callback or
`null` callback. One-array results preserve source keys, multi-array results
are reindexed, missing source values are padded with `null`, and
`array_map(null, ...)` returns zipped row arrays.

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

Eval regex dispatch uses Rust's `regex` engine for common PCRE-style delimited
patterns. It strips PHP delimiters, supports the `i`, `m`, `s`, `u`, and `U`
modifiers, supports common capture array shapes and replacement references, and
supports `PREG_SPLIT_NO_EMPTY`, `PREG_SPLIT_DELIM_CAPTURE`, and
`PREG_SPLIT_OFFSET_CAPTURE`. PCRE constructs unsupported by Rust `regex` fail
as eval runtime fatals. Native non-eval regex codegen remains PCRE2-backed as
documented in [Regex](regex.md).

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

Eval local filesystem calls operate on host filesystem paths. Stream wrappers,
PHAR URLs, network URLs, ownership/group modification, and `fstat()` array
results remain outside the eval filesystem subset. Stream wrapper functionality
for native code is documented in [Streams](streams.md).

Eval `print_r()` supports the one-argument form. Scalars print through the same
output path as `echo`, boolean false and null print nothing, and arrays print
the same `Array\n` header shape as elephc's native `print_r()` subset.

Eval `var_dump()` supports the one-argument form. Scalars print typed
diagnostic lines, and indexed or associative arrays print foreach-visible keys
and nested values through eval value hooks.

## Current limitations

Eval executes through the `elephc_eval` interpreter bridge, not through the full
static AST -> EIR -> native codegen pipeline used for ordinary elephc source.
Unsupported constructs and missing class names during eval object construction
fail at runtime with an eval fatal diagnostic.

The fragment subset is broad but not the full elephc language surface. In
particular, advanced native callable descriptors and closure callback values are
still outside eval fragments. Runtime/AOT object-method and static-method
fallback from eval remain limited to the generated public non-by-reference fixed
scalar/Mixed/object bridge slice, while runtime/AOT constructor fallback remains
limited to public non-by-reference fixed scalar/Mixed signatures. Variadic,
by-reference, and broader parameter/return ABI shapes are still outside those
bridge paths.

Eval class support is still smaller than the full static class system. The main
remaining class-system gaps are broader reflection APIs beyond the supported
ReflectionClass/Function/Method/Parameter/Property/NamedType/UnionType/IntersectionType
and attribute slice, Reflection type APIs beyond retained parameter, generated
property, and function/method return metadata, broader
parameter and generated property default-value materialization beyond the
eval-supported constant-expression subset, object-valued defaults in generated
metadata, and broader generated/AOT method bridge signatures beyond the current public
non-by-reference fixed scalar/Mixed/object slice. Generated/AOT method type
metadata and generated/AOT method/property attributes are exposed for registered
metadata slices, while broader non-public or unsupported bridge shapes remain
outside that slice. Eval object cloning covers ordinary
emitted/AOT storage and public AOT `__clone()` hooks, but non-public AOT
`__clone()` scope checks and broader bridge signatures remain outside that
bridge slice.

Because `eval()` is a dynamic barrier, the compiler must be conservative after
an eval call. Values that cross the barrier may be widened to boxed `Mixed`
storage internally, and optimizer/type facts from before the call cannot be
blindly reused afterward.
