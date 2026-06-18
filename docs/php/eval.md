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
| Classes | Eval fragments can declare classes with properties, methods, `__construct()`, inheritance, visibility, abstract/final modifiers, trait uses with `insteadof` / `as` adaptations, interface implementations, static members, and class constants. Duplicate eval class-like names are rejected. |
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
| Variables and properties | Variable reads, `$this->property` reads/writes from native methods, dynamic `stdClass` properties, eval object property access, static property access, and class constant fetches through the bridge. |
| Arrays | Indexed and associative literals, modern `[...]` and legacy `array(...)`, keyed elements, append writes (`$array[] = value`), numeric-index reads/writes, and string-key reads/writes. |
| Function-like calls | Direct calls, named arguments, argument unpacking (`...`), dynamic string/expression calls, `call_user_func()`, and `call_user_func_array()` for supported call targets. |
| Object construction | `new ClassName(...)` for eval-declared classes, `stdClass`, and emitted AOT classes visible through runtime metadata. |
| Method calls | Public object method calls with positional arguments and numeric array unpacking. |
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
...)`, `call_user_func_array($cb, [...])`, and `iterator_apply()` with
positional arguments.

Post-barrier native direct calls and string-literal `call_user_func()` callbacks
currently accept simple positional arguments. Post-barrier
`call_user_func_array()` callbacks can pass indexed or string-keyed argument
containers to eval-declared functions.

## Classes and objects

Eval-declared classes support inheritance, public/protected/private properties
and methods, `__construct()`, abstract classes and methods, final classes and
methods, trait composition with `insteadof` conflict resolution and `as`
aliases/visibility adaptations, interface implementation checks, static
properties, static methods, class constants, interface constants, trait
constants, and `ClassName::class` literals. Member visibility is checked at
runtime for eval-declared objects and static/class-constant accesses. `self::`,
`parent::`, and late-bound `static::` work for supported static members, class
constants, and class-name literals.

Eval object construction can allocate eval-declared classes, `stdClass`, and
emitted AOT classes visible through runtime class metadata. Missing class names
during eval object construction fail with an eval runtime fatal diagnostic.

AOT and eval-declared class-name probes are visible through `class_exists()`.
Eval object relation probes through `is_a()` and `is_subclass_of()` use
generated AOT class/interface metadata and eval-created object metadata.
`interface_exists()`, `trait_exists()`, and `enum_exists()` can probe generated
AOT metadata. Eval-declared classes, interfaces, traits, and class aliases are
visible through the corresponding eval and post-barrier native metadata probes.

Public declared property reads/writes through `$this->property` from native
methods are bridged to eval. Public zero-, one-, or two-scalar-argument method
calls through `$this->method(...)` are supported by the native method bridge.

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
| Types, metadata, and dynamic calls | `intval()`, `floatval()`, `strval()`, `boolval()`, `settype()`, `gettype()`, `get_class()`, `get_parent_class()`, `get_resource_type()`, `get_resource_id()`, `function_exists()`, `is_callable()`, `class_exists()`, `interface_exists()`, `trait_exists()`, `enum_exists()`, `is_a()`, `is_subclass_of()`, `call_user_func()`, `call_user_func_array()`, `is_int()`, `is_integer()`, `is_long()`, `is_float()`, `is_double()`, `is_real()`, `is_nan()`, `is_finite()`, `is_infinite()`, `is_string()`, `is_bool()`, `is_null()`, `is_array()`, `is_object()`, `is_iterable()`, `is_numeric()`, `is_resource()` |
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
particular, advanced native callable descriptors, closure callback values, and
static-method callable arrays are still outside eval fragments. Object method
calls through eval support positional arguments and numeric array unpacking;
named method arguments remain unsupported.

Eval class support is still smaller than the full static class system. The main
remaining class-system gaps are eval-declared enums, property hooks,
attributes/reflection metadata, readonly semantics, dynamic static callables,
and advanced method-call forms such as named method arguments.

Because `eval()` is a dynamic barrier, the compiler must be conservative after
an eval call. Values that cross the barrier may be widened to boxed `Mixed`
storage internally, and optimizer/type facts from before the call cannot be
blindly reused afterward.
