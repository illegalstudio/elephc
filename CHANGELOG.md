# Changelog

All notable changes to elephc, a PHP-to-native compiler written in Rust.
Releases are listed newest first.

## [Unreleased]
- Migrated every registry-backed PHP builtin to the backend-neutral EIR boundary. Builtin declarations now own one shared semantic descriptor for validation, result typing, effects, ownership, runtime/bridge requirements, callable policy, and lowering strategy; direct calls and generated callable wrappers emit typed EIR primitives or runtime calls that the target-aware backend materializes uniformly on macOS AArch64, Linux AArch64, and Linux x86_64. The old assembly hooks, opaque builtin opcode, duplicated per-name result/effect/requirement tables, legacy checker/signature fallbacks, and separately maintained callable wrappers have been removed.
- Added regression coverage ensuring `strlen()` calls on nullable string return values remain heap-clean while preserving the boxed value across repeated calls, null branches, and parameter boundaries.
- Fixed branch merges of indexed and associative arrays with different element types (issue #549): `match`, ternary, `?:`, `??`, and `??=` now widen payloads elementwise to boxed `mixed` storage and materialize typed arms through copy-on-write-safe conversions instead of reading one arm through the other arm's slot layout.
- Fixed list destructuring after non-fallthrough null guards: `break` and `continue` branches now preserve the complementary non-null type, allowing guarded `?array` values to be unpacked safely. Associative-array right-hand sides with positional integer keys are also accepted with adaptive `mixed` element types, while unguarded nullable and non-array values remain compile-time errors.
- Fixed heap-typed functions whose `try` body and every `catch` terminate being assigned a fabricated implicit return. Dead `try`/`catch` joins, including joins after `finally`, are now explicitly unreachable, so object and array return paths compile consistently with EIR optimization enabled or disabled.
- Added PHP's builtin `UnhandledMatchError` as an `Error` subclass for explicit `new`, `throw`, `catch`, and `instanceof` use, with inherited `Throwable` constructor and method signatures plus autoload recognition. The implicit no-arm/no-default `match` path remains the existing fatal runtime error rather than a catchable object.
- Fixed by-reference `foreach` over a missing array element segfaulting (issue #556): the by-reference path was left out of scope by the #526/#533 read-side hardening, so the null-container sentinel from a missed read was handed to the copy-on-write helpers as a real array pointer and then re-read as a live array header on every iteration. `__rt_array_ensure_unique` and `__rt_hash_ensure_unique` now recognize the sentinel and return it unchanged (hardening every copy-on-write call site), foreach initialization folds a sentinel source to the canonical zero pointer in the iterator's private slot while the user-visible variable stays null, and the by-reference live-length read substitutes zero for a null source. The direct form reports the missing key while the `?? []` form silently iterates its empty default; both skip the loop body and continue instead of crashing. Ordinary by-reference mutation, aliasing, and PHP's visit-appended-elements semantics are unchanged, and heap-debug stays clean with `--ir-opt` enabled or disabled across every supported target.

## [0.26.2]
- Added experimental PHP `eval()` support across macOS ARM64, Linux ARM64, and Linux x86_64. Eligible literal fragments are parsed at compile time and lowered to native EIR, including direct or scope-backed caller-local synchronization; dynamic strings and unsupported literal shapes fall back to the optional statically linked `elephc-magician` EvalIR interpreter. Within the supported eval subset, the fallback preserves caller/global scope updates, dynamic functions/classes/constants, callables, reflection, builtins, exceptions, ownership/COW behavior, and PHP-visible diagnostics without requiring PHP or the Zend Engine. Bridge linking is automatic when required and can be forced with `--with-eval`; generated builtin documentation now reports AOT and eval availability separately.
- Added PHP-compatible sessions to `--web` across PHP 8.2–8.5 and every supported target: `$_SESSION`, the complete `session_*()` API and interfaces, file persistence with `flock`, the `php`/`php_binary`/`php_serialize` wire formats, custom save handlers, strict mode, lazy writes, GC, SID and runtime INI configuration, cookies and cache limiters, auto-start, multipart upload progress, and trans-SID rewriting. Session state, locks, and bridge buffers are reset between requests, while invalid names, IDs, serialized input, and every supported callable-handler form follow PHP-compatible validation and failure paths. The web prelude now retains session helpers by reachability, omits the legacy callable-handler adapter when unused, uses a compact auto-start path, and shares callable descriptors and invokers module-wide, keeping ordinary `--web` compilation and assembly size close to the pre-session baseline.
- Added PHP's output-buffering builtins to both native compilation and the Magician `eval()` runtime, with one shared buffer stack across static and eval'd code: `ob_start`, `ob_get_contents`, `ob_get_clean`, `ob_get_flush`, `ob_get_length`, `ob_get_level`, `ob_clean`, `ob_end_clean`, `ob_end_flush`, `ob_flush`, `ob_get_status`, `ob_implicit_flush`, and `ob_list_handlers`. Buffers nest, grow dynamically, capture every stdout writer (`echo`/`print`, `printf`, `print_r`, `var_dump`, `readfile`, `fpassthru`), and are flushed automatically at script end and on `exit()`/`die()`. User output handlers are fully supported — closures, first-class callables, function-name strings, and eval-registered callables run on flush/clean with PHP's phase bits, `false` pass-through, and string casting of other returns — along with `chunk_size` auto-flushing, PHP's cleanable/flushable/removable `flags` gating, the matching E_NOTICE/E_WARNING diagnostics, and PHP-shaped `ob_get_status()` reporting (handler name/type/flags/chunk/buffer sizes).
- Added the `--strict-php` flag: the compiler accepts only PHP-compatible constructs. Extension syntax (`ifdef`, `packed class`, `extern`, `ptr_cast<T>`, `buffer_new<T>`, typed local declarations, `ptr`/`buffer<T>` annotations) is rejected at compile time with per-violation diagnostics across the main file, includes, and autoloaded files, while extension builtins (`ptr_*`, `zval_*`, `buffer_*`, `class_attribute_*`) behave exactly as under the PHP interpreter — `function_exists()` reports `false`, calling one is an undefined function with a hint naming the disabled extension, and user code may declare its own functions with those names. Strict mode also reaches `eval()` with PHP's execute-time semantics: extension builtins do not exist inside eval'd fragments (runtime fatal on call, coherent `function_exists`/`is_callable`), extension syntax in a fragment is a runtime parse error, and user functions shadowing extension names stay callable. Programs using compiler preludes (PDO, timezone, image, web) keep compiling; `--define` cannot be combined with the flag.
- Added `declare` directive support (PR #459): `declare(strict_types=1);` — the customary first statement of modern PHP files — now parses in both the statement form and the block form (`declare(ticks=1) { ... }`) instead of failing to compile. elephc compiles an always-strict subset, so `strict_types`, `ticks`, and `encoding` are validated syntactically (literal values only; `strict_types` must be the file's first statement, in statement form, with value `0` or `1`) and treated as no-ops.
- Added PHP 8.3 typed class constants for classes, interfaces, traits, and enums. Declared types are enforced for initializers and inherited overrides, including covariant narrowing, and are exposed through `ReflectionClassConstant::hasType()` and `getType()`; untyped constants and enum cases retain PHP-compatible reflection defaults.
- Added PHP-compatible `::class` support on object expressions: `$object::class` now returns the receiver's concrete runtime class name, evaluates the receiver exactly once, and rejects statically known non-object receivers, while existing named and static forms remain unchanged.
- Added PHP-compatible `mb_strlen()` to both native compilation and the Magician `eval()` runtime, including the nullable optional `$encoding` argument, UTF-8 malformed-sequence handling, byte-count aliases, iconv-backed multibyte encodings, callable dispatch, and catchable `ValueError` for unknown encodings on every supported target.
- Added support for `static $x;` function-static declarations without an initializer, in both the native parser and the Magician `eval()` parser: the missing initializer desugars to `= null`, matching PHP, where `static $x;` and `static $x = null;` are identical (including `isset()` behavior).
- Expanded declared call and return boundaries for runtime-backed values (PR #470): boxed `mixed` values and unions with at least one compatible member can flow into narrower declared types through the existing runtime conversion path, while incompatible unions remain rejected. Typed callbacks over unknown-element arrays now keep their declared parameter contracts instead of receiving fabricated integer placeholder types; interface conformance and property assignment remain strict, and concrete object-element `array_map()` lowering remains unsupported.
- Fixed functions returning a Mixed-boxed static local handing the caller a borrowed reference: checked `++` widens an integer static's slot to a boxed `int|float`, and callers release call results after consuming them, so `return $counter` drove the slot's own box to refcount zero — every later call incremented freed memory, and once the block was reused (for example by a checked-arithmetic store into a global) the returned value read back as an empty string. `return` of a static local now acquires the loaded box like other persistent-storage reads, keeping the slot's ownership balanced.
- Fixed bare `array` parameters acquiring the first call site's concrete object element class (PR #519): indexed object arrays now use a runtime-dynamic `Mixed` element contract, while associative arrays retain their key and hash-storage shape with `Mixed` values. Sibling object arrays no longer read properties through a stale class layout or crash on missing properties; runtime property dispatch emits the PHP-style undefined-property warning and returns `null` on every supported target. User-comparator sorts also adapt boxed `Mixed` elements to typed callback parameters, keeping `usort()`/`uasort()` valid after this widening.
- Fixed native method return `static` losing its late-bound meaning (PR #578): checker inference and EIR lowering now bind it to the call-site receiver across inherited instance methods, static factories, interfaces, nullable returns, and compound unions. Override/interface validation and reflection metadata preserve PHP-compatible `static` semantics instead of rewriting the declaration to its owning class or interface.
- Fixed a silent miscompile around indirect callable invocations (issue #487): the generated descriptor-invoker trampoline used callee-saved registers as scratch without preserving them, so any value the register allocator kept live across a closure / callable-string / first-class-callable call — most visibly the accumulator in `$acc += $f()` inside a loop — was clobbered and read back as the trampoline's leftover scratch. The invoker now saves and restores the callee-saved registers it touches, like every compiled function prologue (AArch64 x19–x26, x86_64 r12/rbx/r13–r15).
- Fixed several PHP conformance gaps: `Exception`/`Error` constructors now accept and store `$previous` (`?Throwable`) so `getPrevious()` round-trips instead of always returning null; interface and parent method implementations may narrow self returns covariantly, including static methods; soft-keyword `enum` is accepted throughout class-like declarations, type hints, imports, and scoped expressions; `use const` handles lexer-tokenized globals in single, multiple, and grouped imports; and `foreach` over unknown `array` values preserves both integer and string keys instead of treating them as integers only.
- Fixed keyword-named class members losing their source spelling (PR #572): enum cases, class constants, and other members named with PHP keywords (any except the reserved `class`) now retain their exact case-sensitive declaration spelling, so `case Match` and `case MATCH` are two distinct cases that report `Match` / `MATCH` through `->name`, and constant access resolves against the preserved spelling.
- Fixed heap corruption when arrays grown inside loops are promoted to mixed-element storage (issue #452). Loop analysis now covers `$a[] =`, `$a[$i] =`, and `array_push($a, …)` sites and uses inferred call and assignment value types before lowering, so every write uses consistent boxed storage without widening homogeneous typed rebuilds such as `MultipleIterator::detachIterator`. This also fixes the silent value corruption and per-conversion leak in the string/float form of the same pattern.
- Fixed `match` and ternary expressions with heterogeneous result types (issues #488 and #494): object/array/string/int/float/`null` mixes now merge to a boxed `mixed` result and keep each value-producing branch's runtime type instead of coercing every branch into one scalar-biased unified type (fatal object-to-string casts, silent string/array coercions, or compile errors). Checker inference joins all branches — including assign→inferred-return and abbreviated `?:` shapes — and agrees with EIR lowering; nullable inferred returns preserve `null`, and `gettype()` on nullable-int merge temps no longer segfaults by unboxing an inline tagged scalar as a boxed Mixed cell. Same-representation `int`/`bool` merges remain a documented PHP divergence.
- Fixed a transposed x86_64 heap magic stamped by several throwable emitters (issue #482): objects created through the runtime-raised `ValueError`/`TypeError`/`LogicException`/`Error` paths, JSON throw errors, and some SPL/hash/static-property paths carried a header magic the refcount helpers do not recognize, silently opting them out of reference counting on linux-x86_64. Every x86_64 heap kind-word stamp and magic check now goes through the shared `sentinels` helpers (`x86_64_heap_kind_word` / `X86_64_HEAP_MAGIC_HI32`), and repository lint tests keep both the transposed typo and hand-typed canonical immediates from coming back.
- Fixed PHP type-hint resolution for `object` and `Closure`: namespaced `object`/`Object` hints now keep the generic object contract and reject non-object values instead of being rewritten as namespace-local classes, while global `Closure`, fully qualified `\Closure`, and imported `use Closure` hints resolve to callable storage across parameters, returns, and properties. An unimported `Closure` inside a namespace remains namespace-relative, and PHP's ban on `callable` properties still applies to plain, nullable, and union forms while `Closure`, `?Closure`, and `Closure|null` remain valid.
- Fixed the caught-exception leak (issue #448): every handled `throw` leaked its throwable (~48 bytes per `catch`), because the catch binding moved the in-flight exception into the variable's slot without ever scheduling a release — the slot escaped rebind, function-epilogue, and program-exit cleanup alike. Catch binding now transfers an owned EIR value through the ordinary storage planner, so locals, globals, static locals, and reference cells release and replace their previous values correctly; untaken local catch paths stay safe through zero-initialization, and a variable-less `catch` consumes the reference through a hidden temporary. Rethrowing a caught variable — statement-form `throw $e` or expression-form such as `true ? throw $e : 0` — retains it so inner and outer bindings each own their reference. The ARM64 uniform release dispatcher and linux-x86_64 object/any/deep-free helpers now accept throwable heap kind 6 (previously only kind 4), so those releases run on every supported target. Long-running throw/catch loops now keep a flat heap.
- Fixed symbolic defaults for object-typed parameters: enum-case defaults are now resolved and type-checked semantically after class schemas are complete across functions, methods and constructors, constructor-promoted properties, and closures, including named, `self::`, `static::`, and `parent::` receivers. Missing cases and scalar class constants are rejected instead of slipping through syntactic typing, while `ReflectionParameter::getDefaultValueConstantName()` preserves the source-level constant spelling.
- Fixed untyped properties (instance and static) initializing to their inferred type's zero value instead of PHP's implicit `null`: `public $x;` and `public $x = null;` now read as `NULL` before the first write, `is_null()` / `=== null` observe it, and later scalar assignments keep nullable storage (the same slot layout as a typed `?T` property) instead of failing to compile (`prop_set assigning PHP type Void ...`) or crashing `var_dump()` on null array slots. Heterogeneous assignments widen the slot to `mixed`; assignments inside the class's own constructor keep the historical precise inferred type, and untyped properties with concrete defaults are unchanged. `ReflectionClass::getDefaultProperties()` and `ReflectionProperty::getDefaultValue()` now see the implicit `null` default through the same schema.
- Fixed inferred object static properties rejecting values passed through untyped static-method parameters: patterns such as `protected static $instance; return static::$instance = $container;` now unbox the parameter's EIR `Mixed` value into the inferred object slot, preserve the assignment expression's object identity, and use the same normalized EIR signature at the callee and every call site. Static-call argument boxing now transfers or cleans up temporary ownership while preserving returned aliases, so repeated setters with fresh objects release replaced values instead of leaking one object per call. The target-aware store and cleanup paths are covered on ARM64 and x86_64.
- Fixed by-value `foreach` values corrupting arrays returned from functions (issue #405): concrete `int`, `float`, `bool`, and `string` element layouts now remain consistent across the loop local and function-return boundary, while stored string values retain their borrowed iterator payload before the source array is released. Callers now read the returned values correctly instead of seeing box addresses, empty strings, or a fatal heap-exhaustion error.
- Fixed checked-arithmetic Mixed boxes leaking when consumed directly by a parent operation (issue #500): the boxed `int|float` result of runtime `+`/`-`/`*` (and unary `-`) is now released after it is coerced for `%`, bitwise/shift operators, `**`, comparisons, and int-coerced or mixed-key array indexes, so shapes like `($i * 7 + 1) & 0xFFFF` or `$SIN[($i * 7 + 5) & 1023]` in tight loops no longer exhaust the heap. Assignment, call-argument, chained mixed-op, and string-coercion releases stay balanced (no double frees), overflow-to-float promotion is unchanged, and heap-debug stays clean with the EIR optimizer enabled or disabled.
- Fixed `--web --max-requests` crash-loop accounting (issue #516): workers that exit after a planned request-quota recycle now use a dedicated status that the master excludes from startup-death streaks, so sustained traffic with a low quota keeps respawning workers instead of shutting down the server; genuine setup failures, other exit codes, and signal deaths still feed the guard.
- Fixed nullable chained array reads (issue #525): consuming a value from a `?array` receiver now releases the one-shot hidden owned Mixed temporary created by nullable access, on both null and non-null paths, without invalidating the extracted result or double-releasing repeated reads.
- Fixed chained container reads after a missing outer array offset (issues #526 and #554): indexed, associative, mixed-key, truthiness, `empty()`, `(bool)`, `count()`, spread-length, object-property, and method-call consumers now recognize null-container receivers before dereferencing them, preventing segfaults across every supported target. Direct nested reads emit PHP's missing-key and null-offset diagnostics, associative misses warn consistently, and invalid `count()`, spread, and method operations raise catchable PHP errors without evaluating skipped method arguments; `isset()`, `empty()`, and null coalescing remain silent across the full subscript chain, string-valued misses retain their null marker so `??` selects its default without conflating real empty strings, and `foreach` skips a missing container instead of crashing.
- Fixed nested writes into an `Array(Mixed)` element (issue #529): `$a[$i][$j] = ...` no longer mutates a detached Mixed cell from the read path and silently drops the update. EIR now splits off the innermost key and writes through the parent via `__rt_mixed_array_set` (or `offsetSet` for `ArrayAccess` object parents), so homogeneous string/int inner slots, boxed Mixed inners, associative keys, compound assigns, and object parents all persist, with heap-debug remaining clean.
- Fixed owned boxed Mixed temporaries leaking after string conversion (issue #527): explicit `(string)` casts plus implicit concatenation and interpolation now release detached Mixed sources, including by-value `foreach` element reads and non-null unions represented as Mixed; borrowed, persistent, moved, and non-heap values remain release no-ops.
- Fixed boxed Mixed values leaking when nested loops reinitialize a local whose storage widens after lowering (issue #534): EIR now records a deferred `ReleaseLocalSlot` before the overwrite, prunes it for final scalar or ref-bound slots, and lowers it using the final slot representation. Cleanup is flow-sensitive across conditional ref-cell promotion, aliases, by-reference `foreach` and pointer paths, and widened parameters, preventing both missed releases and double frees with the EIR optimizer enabled or disabled.
- Fixed function- and method-local indexed arrays leaking previous COW hash generations after promotion to string-keyed associative storage (issue #538): lowering now releases the boxed Mixed slot owner before consuming mutations and finalizes provisional concrete-load releases against the slot's final storage type. This preserves real COW aliases while avoiding an artificial clone on every insertion, leaving heap-debug clean with the EIR optimizer enabled or disabled across every supported target.
- Fixed owned constructor argument temporaries leaking after fixed-class object creation (issues #540 and #516): EIR now releases temporary objects, arrays, strings, and boxed Mixed values once `ObjectNew` returns while preserving borrowed and by-reference arguments. Constructors that retain callable state, including `Fiber` and callback-filter iterators, now acquire their own descriptor reference before caller cleanup, preventing dangling captures while keeping ordinary descriptor cleanup balanced across every supported target.
- Fixed `file_get_contents()` results leaking after retaining consumers (issue #540): the builtin now declares fresh caller-owned result storage in the single-source registry, allowing EIR to release both successful boxed string results and boxed `false` results after casts without affecting borrowed builtin results. Heap-debug remains clean with the EIR optimizer enabled or disabled.
- Fixed owned refcounted temporaries leaking when boxed as Mixed (issues #484, #540, and #516): EIR now releases the producer reference after `MixedBox` retains the payload, covering nullable/Mixed returns and other boxing sites for fresh objects, arrays, hashes, callables, and nested cells. Borrowed values remain valid and untouched. This closes root cause 3 from issue #540.
- Fixed owning temporary receivers leaking after object property reads (issues #540 and #516): named, dynamic, and nullsafe EIR property paths now stabilize borrowed string, array, object, and callable results before releasing a Mixed-unboxed or otherwise temporary receiver. Nullsafe short-circuits also release owning nullable receivers on the null branch, while indexed consumers preserve chained reads such as `$object->items[0]`, aliasing, and COW behavior. Heap-debug remains clean with the EIR optimizer enabled or disabled. This closes root cause 4 from issue #540.
- Fixed inline array arguments leaking when a source function or instance method returns an independent array (issues #540 and #516): the checker now derives conservative return-to-parameter alias summaries, so EIR suppresses temporary cleanup only for arguments the result can actually reuse. Real passthrough returns, descendant overrides, dynamic/indirect storage, and copy-on-write aliases remain protected; optimizer inlining preserves borrowed parameter/return slots without double cleanup, and heap-debug stays clean with `--ir-opt` enabled or disabled on the shared target-independent path.
- Fixed the remaining persistent-worker ownership growth from issues #540 and #516: scalar casts now release consumed owning inputs; builtin metadata identifies the independent storage returned by `rawurldecode()`, `implode()`, `htmlspecialchars()`, and `htmlentities()` so wrappers and direct calls can release stabilized read arguments; Mixed-to-string call conversions are now owned explicitly by EIR and follow the same alias-aware cleanup; and constructor materialization releases temporary boxed-Mixed arguments. The original Ivory workload now has zero live-block growth through 500 requests with EIR optimization enabled or disabled, and its first/last 50-request latency stays flat at approximately 3 ms.
- Fixed chained writes through a missing array element dropping the assignment (issue #555): the parent chain of a nested assignment such as `$a[7][1] = 'patched'` is now lowered with fetch-for-write semantics, so missing indexed elements, null gap slots, boxed-null elements, and missing hash keys autovivify as empty arrays installed into the parent storage — with no undefined-key warning, matching PHP. The write-context read returns the STORED cell after slot normalization, which also lands writes through concrete intermediates of 3+ level chains (the residual limitation documented alongside issue #529), and a string key on an indexed payload now promotes it to hash storage instead of silently dropping the write. Covered receivers are concrete `array<mixed>` locals (integer and literal string keys, including the first-string-key promotion of an empty array), Mixed-valued associative locals, and boxed Mixed values; incompatible scalar intermediates are still not converted, COW splits are preserved for shared outer arrays, and heap-debug stays clean with the EIR optimizer enabled or disabled on every supported target.

## [0.26.1]
- Expanded flow-sensitive type narrowing for PHP's common guard patterns: `int|false` and other false-sentinel unions now preserve the literal `false` subtype and narrow to their success type after a divergent `=== false` guard, without incorrectly removing a full `bool` member; `=== null` and `is_null()` guards narrow nullable values; and stable object properties can be narrowed through `instanceof`, ternaries, and throw guards. Property facts are invalidated after writes or receiver rebindings and are not retained across property hooks or `__get`, whose repeated reads may differ.
- Object-subtype declaration defaults are now validated after class and interface schemas are complete: parameters, methods, and constructor-promoted properties may use an implementing class or subclass instance as the default for an interface/base-class type, while unrelated object defaults are still rejected with a type error.
- `htmlspecialchars()` / `htmlentities()` now accept the optional `$flags` and `$encoding` arguments (issue #506), and the `ENT_*` constants (`ENT_QUOTES`, `ENT_COMPAT`, `ENT_NOQUOTES`, `ENT_HTML401`, `ENT_HTML5`, `ENT_XHTML`, `ENT_XML1`, `ENT_SUBSTITUTE`, `ENT_IGNORE`) are defined with PHP's values. The escaper currently always applies `ENT_QUOTES` behavior.
- Static interface methods (PHP 8.3+): an interface may declare `public static function` signatures; a concrete implementing class must provide a compatible public static method (abstract classes may defer to a concrete child), dispatched by class with no vtable slot. `#[\Override]` is accepted on the static implementation, including when the interface is implemented by an abstract parent class.
- Deprecated `${var}` / `${expr}` string interpolation is now accepted by the lexer (issue #340), matching PHP 8.x's deprecated-but-working behavior.
- `var_dump()` is now variadic (issue #389): each argument is dumped independently in source order. `print_r()` gains the `$return` flag — `print_r($v, true)` returns the rendered string instead of echoing, including when the flag is only known at runtime (`string|bool` boxed result); captures are truncated at the 64 KiB buffer cap.
- Reference aliases to indexed-array elements (issue #331): `$b =& $a[0]` binds a local to the element's storage with write-through in both directions, on every supported target. Associative arrays and out-of-range autovivification are documented limits.
- Windows groundwork (issue #379): `windows-x86_64` is parsed as a target and every platform dispatch has an explicit "not yet supported" diagnostic instead of an exhaustiveness gap.
- Source maps v2: `--source-map` now writes a versioned machine-readable schema (`format: "elephc-source-map"`, `version: 2`) with function ranges (PHP name, entry symbol, assembly line range, synthetic flag for compiler-generated bodies), assembly labels attributed to their owning function and EIR basic block, instruction mappings tagged with the originating EIR opcode, expression end positions, and optimization provenance (`const_fold`/`licm`), plus a PHP-line → assembly-range inverse index and a `source_sha256` staleness checksum. The schema contract for external tooling is documented in `docs/compiling/source-maps.md`; the flat v1 `entries` format is superseded.
- New `--debug-info` flag: embeds DWARF debug information in the generated assembly — a `.file`/`.loc` line table plus a compile unit with one `DW_TAG_subprogram` per PHP function, derived from the same source markers as `--source-map` — so lldb/gdb breakpoints (`b file.php:3`) and profiler samples resolve to PHP source lines without custom tooling. On macOS the pipeline runs `dsymutil` to produce a `.dSYM` next to the binary (keeping the object file as a fallback when that fails); on Linux the line tables link directly into the binary.
- Expression spans now carry end positions: the lexer records token extents and the parser widens binary, assignment, and call spans through their last token, keeping the start anchored so diagnostics are unchanged.
- Fixed `self`/`static`/`parent` in variadic parameter types: a relative-class-typed variadic parameter such as `function concat(self ...$items)` on a class, interface, or enum method was rejected with "Cannot use 'self' as a type outside of a class", because the pass rewriting relative class types skipped the variadic parameter's annotation. The rewrite now covers regular parameters, the variadic parameter, and the return type through one shared helper.
- Fixed name resolution inside named-argument values (issue #495): an imported alias or namespace-relative name nested in a named argument's value — such as `Url` in `new self(url: new Url('/'))` — is now rewritten to its canonical fully-qualified form like positional arguments, instead of failing with "Undefined class: Url". The name resolver's expression walk previously had no `NamedArg` arm, so the value expression escaped rewriting entirely.
- Removed the legacy direct AST → ASM backend completely. EIR is now the only
  codegen implementation path.
- `in_array()` now honors its optional third `$strict` argument: omitted/false
  uses PHP loose membership for supported scalar/string paths, including
  numeric-string coercion, string loose equality, and bool/int truthiness, while
  `true` uses strict type-identical membership.
- Added `mb_ereg_match()`: a PCRE2-backed, start-anchored mbregex builtin with
  the optional `$options` argument and support for `i` case-insensitive matching.
- Int-backed enum `from()` / `tryFrom()` now accept a dynamically-typed (`mixed`) argument (issue #449): a `foreach` value over a heterogeneous array, an untyped parameter, etc. are coerced on their runtime type before the enum lookup — integer/numeric-string resolve (or throw `ValueError`), float truncates, bool/null coerce, and array/object/resource/closure throw `TypeError` naming the given type. Previously any `mixed` argument was rejected at compile time. Target-aware on every supported backend.
- Int-backed enum `from()` / `tryFrom()` now accept a numeric string (issue #349): `Level::from("1")` coerces the string to the integer backing value (as a distinct EIR coercion lowered before the enum call) and returns the matching case, instead of being rejected at compile time. A numeric string with no matching case throws `ValueError`; a non-numeric string (e.g. `"x"`) throws `TypeError` with PHP's exact argument-type message — matching PHP's coercive typing on every supported target, including PHP-rejected libc `strtod` extensions such as hexadecimal `"0x1"`, `"INF"`, and `"NAN"`.
- Fixed an enum `from()` / `tryFrom()` refcount bug (surfaced while fixing #349): the returned case singleton was under-retained, so storing the result into a reassigned variable inside a loop drove the persistent singleton's refcount to zero and freed it — producing garbage reads or a heap crash after a few iterations. `from()`/`tryFrom()` now retain the matched singleton, keeping it alive like direct case access. Affected both backed-enum backings.
- Fixed integer-arithmetic overflow parity (issue #369): runtime `int + int`, `int - int`, and `int * int` now promote to `float` on 64-bit overflow instead of wrapping, clamping, or staying statically `int`, while non-overflowing runtime arithmetic remains `integer`. The EIR backend now lowers checked integer arithmetic through target-aware runtime helpers, folds constant checked operations back to scalar `int`/`float` values under `--ir-opt`, and preserves PHP behavior through chained arithmetic plus prefix/postfix increment overflow. The ownership cleanup around widened `Mixed` results, statics, ordinary globals, returned locals, and `--web` request resets was tightened so the extra boxed values introduced by checked arithmetic are released exactly once.
- Fixed a constant-propagation miscompile with by-reference calls in a `match` subject (issue #384): `echo match(bump($i)) { ... } . "|" . $i` kept the pre-call constant for `$i` and printed the stale value, because a call's unknown write set was treated as "no writes" instead of forcing conservative invalidation. A read sequenced after any by-reference-mutating call in the same expression now observes the post-call value, matching PHP; pure calls (`gettype`, `strlen`, …) keep their operands foldable.
- Fixed PHP parser/codegen parity for parenthesis-free object instantiation (issue #371): `new Foo`, `new self`, `new static`, `new parent`, and `new $class` now compile with empty constructor arguments, while immediate postfix forms such as `new Foo->bar`, `new Foo::bar()`, `new Foo?->bar`, and `new Foo[0]` are rejected instead of being misparsed.
- Fixed three PHP parity regressions in the EIR backend: indexed array elements such as `$a[0]` can now be passed to by-reference parameters with copy-on-write storage split before mutation (issue #360); `IteratorAggregate::getIterator()` may declare the marker `Traversable` return type while `foreach` still dispatches through the returned object's concrete `Iterator` methods (issue #385); and catchable private/protected method access plus readonly-property write errors now preserve PHP's receiver/RHS evaluation order before throwing `Error` (issue #383). The merge also removes a duplicate `_spl_error_class_id` data symbol that could make post-merge user assembly fail to assemble.
- Fixed EIR parity for two PHP runtime edge cases: `foreach` by reference now observes elements appended during iteration instead of using the by-value snapshot length, while by-value indexed and mixed-array iteration still stops at the original array length; and reading an uninitialized typed instance or static property now emits PHP's specific fatal message when uncaught while still throwing a catchable `Error` when an exception handler is active.
- Fixed missing-key reads on indexed arrays: null coalescing now suppresses undefined-key warnings for missing integer, string, mixed, and `null` keys while direct reads still emit PHP-compatible `Warning: Undefined array key ...` diagnostics and return the correct null fallback. The EIR/runtime paths now handle mixed-key indexed reads consistently across macOS ARM64, Linux x86_64, and Linux ARM64, including string-key warnings and mixed integer-key misses.
- Fixed static-member and integer-division parity (issues #336, #372, #356): `static::CONST` now late-binds to the runtime class and falls back to the declaring-class value when not overridden; prefix/postfix `++` and `--` now work on static properties through `ClassName::$x`, `self::$x`, `static::$x`, and `parent::$x`; and `intdiv(PHP_INT_MIN, -1)` now throws catchable `ArithmeticError` with PHP's message instead of wrapping or trapping on every supported backend target.
- Fixed mixed float loose equality and `switch` comparisons (issue #397): `Mixed` float operands now compare numerically instead of truncating through integers, and numeric loose equality dispatches through the boxed runtime tag so numeric strings, booleans, non-scalars, and NaN/unordered float comparisons follow PHP semantics on every supported backend target.
- Fixed undefined-variable compound assignments (issue #370): `$x += 1`, `$x -= 1`, `$x *= 5`, and `$y .= "..."` now treat the missing target as PHP `null`/`0`/`""` with a single warning instead of failing during type checking or reading uninitialized stack storage. Null-coalescing assignment (`??=`) remains warning-free and now also works correctly when used as an expression, such as `echo ($z ??= 42)`.
- Fixed enum type resolution in class member positions: enum names can now be used as declared property types and constructor-promoted property types without failing early with "Unknown type" during the class schema pass.
- Fixed the Linux x86_64 `strtotime()` weekday-modifier scanner: `next Mon`, `last Fri`, and similar modifier + weekday forms now pass the remaining input length capped at 16 bytes to the keyword matcher, matching the ARM64 path and avoiding a fragile fixed-width scan into the zero-padded lowercase buffer tail.

## [0.26.0]
- Runtime dead stripping: compiled executables now link only the runtime helpers the program actually reaches and drop the rest, shrinking binaries without changing behavior. Works on every supported target — Linux via per-symbol sections and `--gc-sections`, macOS via `.subsections_via_symbols` atoms and `-dead_strip`. Shared libraries (`--emit cdylib`) keep the full runtime.
- Closures can be rebound to a new receiver: `Closure::bind()`, `bindTo()`, and `Closure::call()` are supported, and a top-level closure that captures `$this` now binds it correctly instead of losing the receiver. A by-reference `Closure::bind` stored in a variable and called later is tracked as a static callable, so the call carries the bound cell directly rather than going through the generic descriptor invoker.
- New magic methods `__callStatic`, `__isset`, and `__unset`: a static call to an undeclared method dispatches to `__callStatic`, `isset()`/`empty()` on an undeclared property route through `__isset` (and only read `__get` when `__isset` is truthy, so an unset virtual property is empty without ever being read), and `unset($obj->prop)` on a virtual property calls `__unset`.
- Reflection over functions: `ReflectionFunction` (name and parameter counts), `getParameters()`, `ReflectionParameter`, `ReflectionParameter::getType()`, and `ReflectionNamedType`. Attribute arguments are now exposed in reflection metadata, including float, positional-array, named-argument and associative-array values, references to global and class constants, and enum-case references.
- References to object properties: `$x = &$obj->prop` aliases the property with write-through in both directions, and a by-reference function/method return can be captured with `$x = &f()`. By-reference returns also work for `string`- and `float`-typed properties. Reassigning an array reference to a non-empty literal of a different type boxes the literal's elements to match the property's element type.
- `unset()` on array elements: `unset($hash[$key])` removes an associative entry and `unset($arr[$key])` removes a packed indexed element with sparse semantics. `array_map()` now works over heterogeneous (mixed-element) arrays.
- Added 15 array builtins on the EIR backend: `array_is_list()`, `array_key_first()`/`array_key_last()`, `array_replace()`/`array_replace_recursive()`, `array_diff_assoc()`/`array_intersect_assoc()`, `array_merge_recursive()`, `array_walk_recursive()`, `array_find()`/`array_any()`/`array_all()` (PHP 8.4), `array_udiff()`/`array_uintersect()`, and `array_multisort()`. The hash-based set operations accept associative arrays and scalar-element indexed arrays (converted to integer-keyed hashes, with result keys/values widening to `mixed` for heterogeneous inputs); the predicate/comparator builtins accept string, function, and non-capturing closure callbacks. All are target-aware (macOS ARM64, Linux x86_64, Linux ARM64) and documented in `docs/php/arrays.md`.
- Added the enum case `->name` property (issue #330): every enum case, pure or backed, now exposes the read-only `name` string holding the case identifier (`E::A->name` is `"A"`), matching PHP's `UnitEnum::$name`. Previously this property access was rejected at compile time with "Undefined property". Backed cases keep `->value`, and `$this->name` is now readable inside enum methods. Access works through direct case access, an aliasing variable, `cases()`, and string interpolation.
- Generators now run on stackful coroutines (issue #329): a generator body is compiled by the normal backend and runs on its own coroutine stack, so `Generator::throw()` raises the exception at the suspended `yield` and a `try`/`catch` *inside* the generator body handles it and resumes — instead of always terminating the generator and propagating to the caller. In-generator method calls, arbitrary control flow, and `try`/`finally` around `yield` work like ordinary functions; `yield from` (over generators and arrays), `send()`, and `getReturn()` are preserved. Generator parameters passed on the caller stack (e.g. a 7th integer parameter under the x86_64 SysV ABI) are now forwarded correctly instead of arriving as zero, and a generator declaring more than 7 parameters (counting closure captures) is rejected with a clear diagnostic rather than corrupting coroutine state.
- Fixed associative-array method parameters (issue #406): an `array`-typed parameter of an instance or static method now preserves the associative shape known at the call site, exactly like a free-function `array` parameter. String-key access (`$d['a']`) type-checks instead of failing with "Array index must be integer", and `json_encode()` of the parameter emits a JSON object instead of a JSON list with garbage values. A declared generic `array` parameter is sharpened from the call-site argument type during method-call inference, and the sharpened shape is used when checking the method body.
- Fixed associative-array property defaults (issue #407): a typed `array` (or untyped) property initialized with an associative literal such as `['a' => 1]` is now stored as associative (hash) storage, so string-key reads and writes (`$this->data['a']`, `$this->data[$key]`) type-check and run instead of failing with "Array index must be integer". The EIR backend also lowers string-keyed associative literal defaults for instance and static properties instead of rejecting them as an unsupported `object_new` feature.
- Fixed heterogeneous associative-array property defaults (issue #413): a typed `array` or untyped property initialized with an associative literal whose values have different types — such as `public array $data = ['n' => 1, 's' => 'hi'];` — now compiles, inferring a boxed `mixed` value slot instead of being rejected with an `unsupported EIR backend feature: prop_set` error. Previously the value type was over-widened to a single scalar (`int` + `string` → `string`), which diverged from the array's actual heterogeneous shape.
- `serialize()` / `unserialize()` builtins covering scalars, nested arrays, and objects — including the `__serialize`/`__unserialize`/`__sleep`/`__wakeup` magic methods and `r:`/`R:` object back-references (repeated objects rebuild as one shared instance) — byte-for-byte compatible with PHP's wire format.
- `Phar` / `PharData` global metadata and stub now persist into the archive (native PHAR, tar, and zip) and round-trip across objects and processes via `setMetadata()`/`getMetadata()`/`hasMetadata()`/`delMetadata()` and `setStub()`/`getStub()`.
- `PharFileInfo` per-file metadata (`setMetadata()`/`getMetadata()`/`hasMetadata()`/`delMetadata()` on `$phar["entry"]`) now persists per-entry into the archive for native PHAR, tar, and zip, round-tripping across objects and the PHP interpreter.
- `PharData::compress(Phar::GZ|Phar::BZ2)` / `decompress()` now perform whole-archive tar compression, writing a sibling `.tar.gz` / `.tar.bz2` (or plain `.tar`); compressed archives are read transparently and are interchangeable with the PHP interpreter.
- `Phar::setSignatureAlgorithm()` / `getSignature()` now sign native PHAR, tar, and zip phars, including `Phar::OPENSSL` RSA-SHA1 signing with a PEM private key (verifiable by the PHP interpreter) alongside the MD5/SHA1/SHA256/SHA512 hash algorithms. Tar/zip signatures use a `.phar/signature.bin` control entry.
- `phar://` zip readers now accept entries written with a streaming data descriptor (general-purpose flag bit 3), reading the authoritative central-directory sizes instead of rejecting the archive.
- ZIP64 phar archives (over 65535 entries, or sizes/offsets over 4 GiB) are now read and written, interchangeable with the PHP interpreter and other ZIP64 tools.
- `Phar`/`PharData::setZipPassword()` (a compiler extension) reads and writes traditional-PKWARE (ZipCrypto) encrypted ZIP entries: with a password set, zip entries (the stub included) are encrypted on write and decrypted on read, while the `.phar/signature.bin` entry stays in the clear. The cipher is cryptographically weak and kept only for compatibility with legacy archives.
- EIR backend correctness: method calls on `mixed` values and on object-iterator `foreach` values (e.g. `DirectoryIterator`, `FilesystemIterator`) now dispatch synthetic SPL methods instead of crashing on an unemitted vtable slot, and `fopen("compress.zlib://…")` / `fopen("compress.bzip2://…")` read wrappers now decompress on the EIR backend.
- Type-checker correctness: a method call on a `mixed` receiver now infers the union of the declared return types across the classes that declare the method (mirroring the runtime class-id dispatch) instead of falling back to `int`. An un-annotated function such as `function f($x) { return $x->name(); }` now returns the method's actual value — previously the inferred `int` return type coerced a returned string to `0`.
- Fixed `foreach`-key array rebuilds: `foreach ($src as $k => $v) $dst[$k] = $v` now preserves every entry instead of collapsing all string keys onto index `0` (which dropped all but the last). Sparse and negative integer keys are kept with PHP semantics — the destination promotes to hash storage instead of zero-filling gaps or silently dropping negative-index writes — while contiguous integer-key rebuilds stay on indexed storage so `implode()` and other indexed consumers are unaffected. A `foreach` key variable no longer leaks its key classification into other functions that reuse the same name, and reassigning the key variable to a string inside the loop routes the write as a string key.
- Resource scope-cleanup: an `fopen()` stream, `popen()` pipe, `opendir()` directory handle, or `hash_init()` hashing context that leaves scope without an explicit `fclose()`/`pclose()`/`closedir()`/`hash_final()` is now released automatically at scope exit (closing the fd, reaping the `popen` child, freeing the context) instead of leaking until the process exits. Aliasing (`$b = $a`) is reference-counted so the handle is freed exactly once; finalizing a hash context and then dropping it is a single safe free; and an explicit close is never doubled even if the descriptor number is later reused.
- `zval` bridge builtins `zval_pack`/`zval_unpack`/`zval_type`/`zval_free`: convert elephc values to and from PHP-shaped `zval` structs (scalars, strings, packed and hash arrays, nested), the foundation for linking against a PHP extension shared library. Works on every supported target.

## [0.25.2] - 2026-06-26
- `--web`: compile a PHP program into a standalone prefork HTTP server binary with per-request top-level execution, `echo`/`print` response bodies, `$_SERVER`/`$_GET`/`$_POST` and `php://input` request input, PHP-compatible `http_response_code()`/`header()` handling, configurable listen address/workers/body limit, clean signal shutdown, worker respawn, bounded keep-alive handling, fixed-heap request cleanup, and full sharded CI coverage across macOS ARM64, Linux x86_64, and Linux ARM64.
- Fixed a heap leak when releasing string-keyed associative arrays (issue #408): promoting an indexed array to hash storage (`array_to_hash`) built the result hash from a copy of the source array but never freed that source array, leaking one allocation per conversion. Reassigning an assoc array in a loop — or, under `--web`, rebuilding the request superglobals each request — slowly exhausted the heap. The conversion now releases the temporary source array, so the heap stays flat.
- EIR small-function inliner: splices small (≤24-instruction), non-recursive user functions into their callers — covering scalar, string, and array/value helpers — with copy-on-write and reference-counting semantics preserved, gated by `--ir-opt`. Recursive (direct or mutual), generator/fiber, exception-handling, object/closure/resource/by-reference, and argument-coercing call sites are left as ordinary calls.
- EIR optimization pipeline now runs to a module-level fixed point: the small-function inliner and the per-function passes are interleaved and repeated until neither changes anything, so optimization and inlining feed each other (e.g. a function inlined once its callees fold below the size threshold). Behavior is unchanged with `--ir-opt` on vs off; only the generated code gets tighter.
- EIR common-subexpression elimination now unifies constant operands by value, so repeated computations built on constants are deduplicated too — `($n + 1) * ($n + 1)` computes `$n + 1` once. Gated by `--ir-opt`; output is unchanged, only the generated code is tighter.
- Fixed an EIR optimizer miscompile: `$x / 1` (PHP's float-producing division) was folded to its integer operand, so the result's bits were misread as a float — `var_dump($x / 1)` and float arithmetic on it produced wrong values with the optimizer on. Identity folding now only folds genuine integer division.

## [0.25.1] - 2026-06-22
- Image support (EIR backend), pure-Rust with no runtime dependency (`elephc-image` crate): GD raster I/O (PNG/JPEG/GIF/BMP/WebP/TGA), drawing primitives, bitmap text, transforms/filters/copy, and color handling; Exif and IPTC metadata; Imagick and Gmagick OOP with the full method surface callable (operations with no pure-Rust equivalent throw `*Exception("... is not supported in elephc")` at runtime, matching PHP); and a Cairo (tiny-skia) procedural + OOP subset.
- Loose equality (`==`/`!=`) and `switch` compare a float against an int numerically (`1.5 == 1` is `false`, `1.0 == 1` is `true`, `switch (1.5)` matches `case 1.5`) instead of failing to compile or truncating the float subject to int.
- EIR dead store elimination over PHP local slots: a CFG-liveness pass that drops `store_local` writes to scalar locals that are never read before being overwritten or the function exits, gated by `--ir-opt`. Refcounted and by-reference-aliased slots are left untouched to preserve ownership and aliasing semantics.
- EIR branch simplification: folds constant-condition `cond_br`/`switch` terminators to unconditional branches (e.g. `while (true)` loops), threads predecessors through empty forwarding blocks, and neutralizes unreachable blocks, gated by `--ir-opt`.
- EIR per-block constant folding: folds pure operations whose operands are all compile-time constants (integer/float arithmetic and bitwise ops, in-range shifts, comparisons, `is_null`/`is_truthy`) into a single constant, gated by `--ir-opt`. Composed with the peephole's scalar load/store forwarding, it propagates constants through EIR value ids and local slots.
- EIR dominator-tree and natural-loop analyses: read-only sidecar analyses (Cooper–Harvey–Kennedy dominators; a back-edge/natural-loop forest with nesting and preheader detection) that underpin the cross-block optimizations below.
- EIR common-subexpression elimination: a dominator-tree value-numbering pass that removes a pure computation when an identical one already dominates it (per-block and cross-block), gated by `--ir-opt`.
- EIR loop-invariant code motion: hoists pure loop-invariant computations out of loop bodies into loop preheaders, gated by `--ir-opt`.
- `--web` flag: compile a PHP program into a standalone prefork HTTP server binary. Each request re-runs the top-level code from fresh state; `echo`/`print` output becomes the response body. Request input is exposed through `$_SERVER`/`$_GET`/`$_POST` and `php://input`; response status and headers are controlled with `http_response_code()` and `header()` (PHP-compatible, including status lines and `Location:`→302). Runtime args: `--listen host:port`, `--workers N`, `--max-body-size N` (413 on overflow). The prefork master shuts down cleanly on `SIGINT`/`SIGTERM`, respawns workers that die, and bounds slow/idle keep-alive connections with a header-read timeout.

## [0.25.0] - 2026-06-19
- EIR dead instruction elimination over CFG liveness, registered after identity and peephole passes and gated by `--ir-opt`.
- PHP date/time, timezone, and calendar parity (EIR backend): `DateTime`, `DateTimeImmutable`, `DateTimeInterface`, `DateTimeZone`, `DateInterval`, and `DatePeriod`, the PHP 8.3 date exception hierarchy, plus the procedural `getdate`/`localtime`/`mktime`/`gmmktime`/`checkdate`/`microtime`/`hrtime`/`strtotime`, `date`/`gmdate` format tokens, solar functions (`date_sun_info`/`date_sunrise`/`date_sunset`), and `ext/calendar` functions — backed by a bundled IANA timezone database (`elephc-tz` crate).
- Date/time correctness fixes: `getdate()`/`localtime()` default to UTC like PHP, `gmdate("T")` reports `GMT` on every target, `createFromTimestamp()` keeps fractional-second microseconds, `DateInterval` requires a leading `P`, `diff()` and `DatePeriod::createFromISO8601String()` match PHP signatures (`$targetObject`, `$specification` + `$options`).
- General fixes surfaced by the date/time work: `var_dump` renders heterogeneous indexed-array bodies, `array_fill` terminates on a negative count on ARM64, and user-declared functions/classes are no longer hijacked when their name collides with a procedural date alias.

## [0.24.3] - 2026-06-17
- EIR peephole optimization pass: box/unbox cancellation, scalar load/store forwarding, paired acquire/release cancellation, string-literal concat folding, and redundant move/borrow cleanup.

## [0.24.2] - 2026-06-17
- Add a fixed-point EIR optimization pass driver with identity arithmetic folding.
- Document the pass driver and add the Compiling docs section.

## [0.24.1] - 2026-06-16
- Register allocator: caller-saved reuse and use-weighted spilling.
- Add benchmark time-series history and dashboard.

## [0.24.0] - 2026-06-16
- Linear-scan register allocator for the EIR backend (liveness, live intervals, int/float pools, spilling) behind `--regalloc`.

## [0.23.14] - 2026-06-16
- Fix empty-array string-grow corruption in the EIR backend.

## [0.23.13] - 2026-06-15
- Property hooks (get/set bodies), anonymous classes, intersection types (`A&B`), asymmetric visibility (`private(set)`).
- Enum methods/constants/implements, dynamic method and static calls, expression-position require/include, typed variadic parameters.

## [0.23.12] - 2026-06-15
- PDO: persistent connection pooling, class fetch modes, binary columns.
- Dynamic property reads/writes, including on Mixed values.

## [0.23.11] - 2026-06-13
- Phar streams: tar/zip/bzip2 read & write, OOP iteration, ArrayAccess, metadata/stubs, compression controls.
- Symlink ownership builtins and user stream filter params.

## [0.23.10] - 2026-06-12
- EIR is now the only active codegen backend. The legacy AST backend is frozen: `--ast-backend` is retained solely as a diagnostic fallback and is not a feature or parity target.
- Broad EIR parity hardening across arrays, mixed values, callables, fibers, SPL, generators, streams, and ownership/GC paths.

## [0.23.9] - 2026-06-11
- `--emit cdylib` produces loadable shared libraries (PHP → C dlopen).
- Tagged null representation as the default scalar/null encoding; PIC global access via GOT.

## [0.23.8] - 2026-06-09
- elephc-crypto crate: full `hash()` family, HMAC, incremental HashContext, checksums, timing-safe `hash_equals`; phar SHA1 signing migrated.
- Lexer/runtime correctness: UTF-8 identifiers, heredoc closers, string interpolation, string-to-int precision, Mixed-arg coercions.

## [0.23.7] - 2026-06-07
- Flow-sensitive type narrowing for `is_*`/`instanceof` guards and `if`/`elseif`/`never` divergence.
- Infer union types for untyped params called with heterogeneous arguments.

## [0.23.6] - 2026-06-05
- PDO: SQLite, PostgreSQL, and MySQL/MariaDB drivers via a driver-agnostic bridge; binding, fetch modes, attributes, `quote()`, Traversable statements.
- `__destruct` object destructor support.

## [0.23.5] - 2026-06-04
- Stream URL reads: http/https/ftp/ftps in `file_get_contents()`.
- `$length`/`$offset` for `stream_get_contents()` and `stream_copy_to_stream()`; real TLS teardown.

## [0.23.4] - 2026-06-03
- Streams subsystem: core resource model, sockets (TCP/UDP/Unix, IPv6, DNS), TLS, wrappers (data/http/ftp + zlib/bzip2), filters, contexts, userspace wrappers, phar read/write.

## [0.23.3] - 2026-06-02
- Generator fixes around yield-from echoes and send values; pipe callable ownership; string return persistence.

## [0.23.2] - 2026-06-01
- New builtins: `clamp`, `grapheme_strrev`, `SortDirection` enum, `json_decode` error locations, `array_filter` mode constants.
- Many PHP-parity fixes: finally semantics, float array keys, enum `from` errors, arrow-fn captures, parser edge cases.

## [0.23.1] - 2026-05-29
- PCRE2-backed SPL regex and filesystem iterators; PCRE2 regex requirements documented.

## [0.23.0] - 2026-05-28
- Phase 6 SPL containers.

## [0.22.5] - 2026-05-28
- Universal callable descriptors: unified runtime dispatch for string/array/closure/method callables across call_user_func, array_map/filter/reduce, sort, iterators, fibers, and externs.
- SPL iterator family expansion (recursive, caching, filter, multi-source) and stateful FFI callback trampolines.

## [0.22.4] - 2026-05-21
- Async HTTP server showcase; raw pointer memory builtins.
- Ownership/lifetime hotfixes across fibers, concat, by-ref foreach, and externs.

## [0.22.3] - 2026-05-19
- Reject by-reference foreach over iterators; isolate by-ref fallback flags.

## [0.22.2] - 2026-05-19
- Nested/mixed array assignment fixes, foreach reference aliasing, dynamic spreads after named args, built-in `Error` type.

## [0.22.1] - 2026-05-18
- Many parity fixes: by-reference foreach values, by-ref closure captures, multi-argument `isset`/`unset`/`echo`, unicode regex escapes, `preg_replace_callback` captures, PHP string escapes.

## [0.22.0] - 2026-05-18
- `ArrayAccess` subscript syntax.

## [0.21.16] - 2026-05-16
- OOP property parity v2; avoid clobbering by-ref scalar slots.

## [0.21.15] - 2026-05-16
- Runtime compatibility v2: loose comparisons, integer overflow promotion, uninitialized typed property reads, mixed value coercions.

## [0.21.14] - 2026-05-16
- Case-insensitive user function string lookup and callable fallbacks.

## [0.21.13] - 2026-05-16
- `is_callable` runtime fallback.

## [0.21.12] - 2026-05-16
- `preg_replace` backreferences and `strtotime` article offsets.

## [0.21.11] - 2026-05-16
- Callable parity follow-up: captured callable forwarding and signature validation.

## [0.21.10] - 2026-05-16
- JSON performance: inline pretty-print, fused decode validation, list-shape encoder optimization.

## [0.21.9] - 2026-05-15
- Abstract properties (redeclaration, readonly-static); expanded `strtotime` relative formats; macOS time/localtime crash fix.

## [0.21.8] - 2026-05-14
- SPL foundation: autoloader and core SPL infrastructure; hardened class introspection builtins.

## [0.21.7] - 2026-05-14
- Runtime attribute reflection: metadata tracking and emission.

## [0.21.6] - 2026-05-13
- PHP 8.5 pipe operator (`|>`) with first-class-callable short-circuit optimizations.

## [0.21.5] - 2026-05-13
- Filesystem builtins: symbolic-link (Phase 5) and stream-extension (Phase 4).

## [0.21.4] - 2026-05-13
- v0.21 JSON parity; `ReflectionAttribute` and class attribute reflection builtins.

## [0.21.3] - 2026-05-12
- Mixed array union support.

## [0.21.2] - 2026-05-12
- PHP generators: `yield`, `send`, `throw`, `yield from`, `getReturn`; complete backend support.

## [0.21.1] - 2026-05-11
- Parse PHP attributes; observe `Override`/`Deprecated`.

## [0.21.0] - 2026-05-11
- Heterogeneous indexed arrays (boxed/widened element types).
- Mandatory Rust module preambles; large-scale module split across compiler, runtime, and tests.

## [0.20.12] - 2026-05-09
- Method first-class callables; forward captured method callables in callbacks.

## [0.20.11] - 2026-05-09
- Forward captured closures through callbacks; string array callbacks.

## [0.20.10] - 2026-05-08
- PHP Fibers: context switch, captures, exception propagation; Linux x86_64 support.

## [0.20.9] - 2026-05-07
- Named arguments for builtins and externs; shared call-argument planning and spread normalization.

## [0.20.8] - 2026-05-06
- Full list destructuring.

## [0.20.7] - 2026-05-05
- Dynamic `instanceof` targets.

## [0.20.6] - 2026-05-05
- Mixed nullsafe chains.

## [0.20.5] - 2026-05-05
- Path-sensitive include graph declaration discovery and conditional include function variants.
- Large module split across resolver, type checker, lexer, and codegen.

## [0.20.4] - 2026-05-04
- Runtime include-once guards.

## [0.20.3] - 2026-05-04
- Reject dynamic include paths.

## [0.20.2] - 2026-05-04
- Model PHP stream resources; `fopen` failure parity.

## [0.20.1] - 2026-05-03
- Dynamic `pathinfo` flags.

## [0.20.0] - 2026-05-03
- `fnmatch` flag support.

## [0.19.14] - 2026-05-03
- Filesystem coverage: path manipulation (Phase 1), extended stat (Phase 2), modification builtins (Phase 3).
- PHP case-insensitive symbol lookup; built-in `Iterator`/`IteratorAggregate` with foreach.

## [0.19.13] - 2026-05-01
- `iterable` type, assignment expressions, multilevel break/continue, closure return types, `print` expression.

## [0.19.12] - 2026-04-29
- `never` and literal types, nullsafe operator, `instanceof`, magic constants in includes, short ternary, word-form logical operators, PHP array unions, error-control parity.

## [0.19.11] - 2026-04-27
- Typed/static/promoted/constructor-promoted properties, null-coalescing assignment, compound assignment operators, `php_uname`/`PHP_OS`.

## [0.19.10] - 2026-04-24
- `final` OOP members; benchmark suite machine-readable output and CI integration.

## [0.19.9] - 2026-04-24
- Constant propagation v3: propagate constants through known loop exits.

## [0.19.8] - 2026-04-23
- Constant folding of array/assoc-array literal access and known match expressions.

## [0.19.7] - 2026-04-23
- DCE v3: CFG-lite path analysis and guard inference to prune impossible switch/if branches.

## [0.19.6] - 2026-04-22
- DCE v2: scalar guard tracking, shadowed-arm dropping, try/switch path unification; optimizer split into modules.

## [0.19.5] - 2026-04-21
- Control-flow normalization pass: canonicalize elseif chains, merge identical branches/catches, normalize switches.

## [0.19.4] - 2026-04-20
- Local constant propagation pass with loop/branch/try-aware invalidation.

## [0.19.3] - 2026-04-20
- Purity and may-throw effect analysis for functions, methods, and closures.

## [0.19.2] - 2026-04-18
- Dead code elimination pass; control-flow simplification.

## [0.19.1] - 2026-04-18
- Constant folding pass and constant control-flow pruning; pure-subexpression pruning.

## [0.19.0] - 2026-04-17
- Compiler tooling: source maps, benchmark harness, timing output, runtime object caching.

## [0.18.5] - 2026-04-17
- Preserve `require_once` error locations.

## [0.18.4] - 2026-04-17
- By-ref array method fix; sync `Cargo.lock` in release workflow.

## [0.18.3] - 2026-04-17
- Specialize generic array hints.

## [0.18.2] - 2026-04-17
- Deep mixed postfix chains; property indexed/array-push assignments.

## [0.18.1] - 2026-04-17
- CLI `check` and `emit-asm` modes.

## [0.18.0] - 2026-04-16
- Linux support: full port to Linux x86_64 and Linux ARM64 with target-aware ABI.
- DOOM showcase; target model plumbing and ABI centralization; large codegen modularization.

## [0.17.9] - 2026-04-03
- Docs restructured into Astro-compatible sections; CI auto-sets `Cargo.toml` version from tag.
- DOOM showcase rendering work (BSP, sectors, fog, doors, HUD).

## [0.17.8] - 2026-04-02
- Error recovery and warnings.

## [0.17.7] - 2026-04-02
- Named arguments.

## [0.17.6] - 2026-04-02
- Type annotations on function params and return types, with enforcement.

## [0.17.5] - 2026-04-01
- Enum support; object-typed locals.

## [0.17.4] - 2026-04-01
- Union and nullable typed locals.

## [0.17.3] - 2026-04-01
- Split generated assembly into user + runtime objects with global runtime labels; cache pre-assembled runtime in test harness.

## [0.17.2] - 2026-04-01
- `readonly` classes, first-class callables, match fatal path, variadic methods/callables.

## [0.17.1] - 2026-04-01
- Packed buffers for hot-path data; manual `buffer_free`.

## [0.17.0] - 2026-03-31
- Namespace resolution across the compiler pipeline.

## [0.16.8] - 2026-03-31
- Build-time `ifdef` conditional compilation.

## [0.16.7] - 2026-03-31
- Magic methods for string conversion and dynamic properties.

## [0.16.6] - 2026-03-31
- String indexing syntax.

## [0.16.5] - 2026-03-31
- Exception handling: throw/catch (incl. multi-catch and variable-less), built-in `Exception`/`Throwable`.

## [0.16.4] - 2026-03-30
- Mixed-type associative arrays via runtime element tags.

## [0.16.3] - 2026-03-30
- Preserve associative array insertion order.

## [0.16.2] - 2026-03-30
- Interfaces and abstract class checks; common-parent object arrays; codegen modularization.

## [0.16.1] - 2026-03-30
- Single inheritance dispatch, late static binding, `self` receiver.

## [0.16.0] - 2026-03-30
- Inheritance groundwork release.

## [0.15.3] - 2026-03-30
- PHP-like trait composition.

## [0.15.2] - 2026-03-30
- Copy-on-write arrays.

## [0.15.1] - 2026-03-30
- Targeted cycle collection; small-bin heap size classes; call-scoped extern string args.

## [0.15.0] - 2026-03-29
- Heap ownership lattice, heap debug verification, free-block coalescing/splitting; cycle collection strategy.

## [0.14.0] - 2026-03-28
- FFI: `extern` keyword, C types, callbacks, extern globals, linker flags; SDL2 examples.

## [0.13.0] - 2026-03-28
- Pointer type, `ptr_cast<T>()`, pointer builtins and runtime.

## [0.12.1] - 2026-03-27
- Class visibility and checker coverage.

## [0.12.0] - 2026-03-27
- Trigonometry, logarithms, math constants; extensive type-inference fixes.

## [0.11.0] - 2026-03-27
- Reference-counting garbage collector with scope-based local cleanup.

## [0.10.0] - 2026-03-27
- Basic PHP classes: properties, constructors, methods, static members.

## [0.9.1] - 2026-03-26
- Bump-reset optimization and string dedup.

## [0.9.0] - 2026-03-26
- Free-list heap allocator with bounds checking, copy-on-store strings, dynamic array/hash growth, `--heap-size` flag.

## [0.8.2] - 2026-03-26
- `use ($var)` closure captures; `round()` precision, char-mask trims, variadic `min`/`max`; many correctness fixes.

## [0.8.1] - 2026-03-26
- Large batch of correctness fixes closing ~30 issues (cross-type `==`, `sprintf` modifiers, IIFE, spread into named params, heredoc interpolation, and more).

## [0.8.0] - 2026-03-25
- Date/time, JSON, and regex functions.

## [0.7.2] - 2026-03-25
- Global/static variables, pass by reference, variadic functions, spread operator; v0.8 system functions.

## [0.7.1] - 2026-03-25
- `define()`/`const` constants, list unpacking, `call_user_func_array`; CI and release workflow.

## [0.7.0] - 2026-03-24
- Default params, null coalescing, bitwise operators, spaceship, heredoc/nowdoc.

## [0.6.0] - 2026-03-23
- Associative arrays, `switch`/`match`, multi-dimensional arrays, ~30 array functions.
- Anonymous and arrow functions; callback functions (`array_map`/`filter`/`reduce`, `usort`, etc.); I/O and filesystem.

## [0.4.0] - 2026-03-23
- String functions: `substr`, `strpos`, `explode`/`implode`, `sprintf`/`printf`, string interpolation, HTML/URL/base64/ctype helpers, `hash`, `sscanf`.

## [0.3.0] - 2026-03-22
- Float type with FP registers and math builtins, proper Bool/null types, type casting, `===`/`!==`, `include`/`require`, `**`, constants.

## [0.2.0] - 2026-03-22
- Indexed arrays with heap allocator, `foreach`, array functions; ternary, do-while, `$argc`/`$argv`, `strlen`/`intval`.

## [0.1.0] - 2026-03-22
- Initial compiler: echo, variables, integers, arithmetic and string concatenation, comparison operators, control flow (`if`/`while`/`for`/`break`/`continue`), functions, logical/assignment/increment operators.

[Unreleased]: https://github.com/illegalstudio/elephc/compare/v0.26.2...HEAD
[0.26.2]: https://github.com/illegalstudio/elephc/compare/v0.26.1...v0.26.2
[0.26.1]: https://github.com/illegalstudio/elephc/compare/v0.26.0...v0.26.1
[0.26.0]: https://github.com/illegalstudio/elephc/compare/v0.25.2...v0.26.0
[0.25.2]: https://github.com/illegalstudio/elephc/compare/v0.25.1...v0.25.2
[0.25.1]: https://github.com/illegalstudio/elephc/compare/v0.25.0...v0.25.1
[0.25.0]: https://github.com/illegalstudio/elephc/compare/v0.24.3...v0.25.0
[0.24.3]: https://github.com/illegalstudio/elephc/compare/v0.24.2...v0.24.3
[0.24.2]: https://github.com/illegalstudio/elephc/compare/v0.24.1...v0.24.2
[0.24.1]: https://github.com/illegalstudio/elephc/compare/v0.24.0...v0.24.1
[0.24.0]: https://github.com/illegalstudio/elephc/compare/v0.23.14...v0.24.0
[0.23.14]: https://github.com/illegalstudio/elephc/compare/v0.23.13...v0.23.14
[0.23.13]: https://github.com/illegalstudio/elephc/compare/v0.23.12...v0.23.13
[0.23.12]: https://github.com/illegalstudio/elephc/compare/v0.23.11...v0.23.12
[0.23.11]: https://github.com/illegalstudio/elephc/compare/v0.23.10...v0.23.11
[0.23.10]: https://github.com/illegalstudio/elephc/compare/v0.23.9...v0.23.10
[0.23.9]: https://github.com/illegalstudio/elephc/compare/v0.23.8...v0.23.9
[0.23.8]: https://github.com/illegalstudio/elephc/compare/v0.23.7...v0.23.8
[0.23.7]: https://github.com/illegalstudio/elephc/compare/v0.23.6...v0.23.7
[0.23.6]: https://github.com/illegalstudio/elephc/compare/v0.23.5...v0.23.6
[0.23.5]: https://github.com/illegalstudio/elephc/compare/v0.23.4...v0.23.5
[0.23.4]: https://github.com/illegalstudio/elephc/compare/v0.23.3...v0.23.4
[0.23.3]: https://github.com/illegalstudio/elephc/compare/v0.23.2...v0.23.3
[0.23.2]: https://github.com/illegalstudio/elephc/compare/v0.23.1...v0.23.2
[0.23.1]: https://github.com/illegalstudio/elephc/compare/v0.23.0...v0.23.1
[0.23.0]: https://github.com/illegalstudio/elephc/compare/v0.22.5...v0.23.0
[0.22.5]: https://github.com/illegalstudio/elephc/compare/v0.22.4...v0.22.5
[0.22.4]: https://github.com/illegalstudio/elephc/compare/v0.22.3...v0.22.4
[0.22.3]: https://github.com/illegalstudio/elephc/compare/v0.22.2...v0.22.3
[0.22.2]: https://github.com/illegalstudio/elephc/compare/v0.22.1...v0.22.2
[0.22.1]: https://github.com/illegalstudio/elephc/compare/v0.22.0...v0.22.1
[0.22.0]: https://github.com/illegalstudio/elephc/compare/v0.21.16...v0.22.0
[0.21.16]: https://github.com/illegalstudio/elephc/compare/v0.21.15...v0.21.16
[0.21.15]: https://github.com/illegalstudio/elephc/compare/v0.21.14...v0.21.15
[0.21.14]: https://github.com/illegalstudio/elephc/compare/v0.21.13...v0.21.14
[0.21.13]: https://github.com/illegalstudio/elephc/compare/v0.21.12...v0.21.13
[0.21.12]: https://github.com/illegalstudio/elephc/compare/v0.21.11...v0.21.12
[0.21.11]: https://github.com/illegalstudio/elephc/compare/v0.21.10...v0.21.11
[0.21.10]: https://github.com/illegalstudio/elephc/compare/v0.21.9...v0.21.10
[0.21.9]: https://github.com/illegalstudio/elephc/compare/v0.21.8...v0.21.9
[0.21.8]: https://github.com/illegalstudio/elephc/compare/v0.21.7...v0.21.8
[0.21.7]: https://github.com/illegalstudio/elephc/compare/v0.21.6...v0.21.7
[0.21.6]: https://github.com/illegalstudio/elephc/compare/v0.21.5...v0.21.6
[0.21.5]: https://github.com/illegalstudio/elephc/compare/v0.21.4...v0.21.5
[0.21.4]: https://github.com/illegalstudio/elephc/compare/v0.21.3...v0.21.4
[0.21.3]: https://github.com/illegalstudio/elephc/compare/v0.21.2...v0.21.3
[0.21.2]: https://github.com/illegalstudio/elephc/compare/v0.21.1...v0.21.2
[0.21.1]: https://github.com/illegalstudio/elephc/compare/v0.21.0...v0.21.1
[0.21.0]: https://github.com/illegalstudio/elephc/compare/v0.20.12...v0.21.0
[0.20.12]: https://github.com/illegalstudio/elephc/compare/v0.20.11...v0.20.12
[0.20.11]: https://github.com/illegalstudio/elephc/compare/v0.20.10...v0.20.11
[0.20.10]: https://github.com/illegalstudio/elephc/compare/v0.20.9...v0.20.10
[0.20.9]: https://github.com/illegalstudio/elephc/compare/v0.20.8...v0.20.9
[0.20.8]: https://github.com/illegalstudio/elephc/compare/v0.20.7...v0.20.8
[0.20.7]: https://github.com/illegalstudio/elephc/compare/v0.20.6...v0.20.7
[0.20.6]: https://github.com/illegalstudio/elephc/compare/v0.20.5...v0.20.6
[0.20.5]: https://github.com/illegalstudio/elephc/compare/v0.20.4...v0.20.5
[0.20.4]: https://github.com/illegalstudio/elephc/compare/v0.20.3...v0.20.4
[0.20.3]: https://github.com/illegalstudio/elephc/compare/v0.20.2...v0.20.3
[0.20.2]: https://github.com/illegalstudio/elephc/compare/v0.20.1...v0.20.2
[0.20.1]: https://github.com/illegalstudio/elephc/compare/v0.20.0...v0.20.1
[0.20.0]: https://github.com/illegalstudio/elephc/compare/v0.19.14...v0.20.0
[0.19.14]: https://github.com/illegalstudio/elephc/compare/v0.19.13...v0.19.14
[0.19.13]: https://github.com/illegalstudio/elephc/compare/v0.19.12...v0.19.13
[0.19.12]: https://github.com/illegalstudio/elephc/compare/v0.19.11...v0.19.12
[0.19.11]: https://github.com/illegalstudio/elephc/compare/v0.19.10...v0.19.11
[0.19.10]: https://github.com/illegalstudio/elephc/compare/v0.19.9...v0.19.10
[0.19.9]: https://github.com/illegalstudio/elephc/compare/v0.19.8...v0.19.9
[0.19.8]: https://github.com/illegalstudio/elephc/compare/v0.19.7...v0.19.8
[0.19.7]: https://github.com/illegalstudio/elephc/compare/v0.19.6...v0.19.7
[0.19.6]: https://github.com/illegalstudio/elephc/compare/v0.19.5...v0.19.6
[0.19.5]: https://github.com/illegalstudio/elephc/compare/v0.19.4...v0.19.5
[0.19.4]: https://github.com/illegalstudio/elephc/compare/v0.19.3...v0.19.4
[0.19.3]: https://github.com/illegalstudio/elephc/compare/v0.19.2...v0.19.3
[0.19.2]: https://github.com/illegalstudio/elephc/compare/v0.19.1...v0.19.2
[0.19.1]: https://github.com/illegalstudio/elephc/compare/v0.19.0...v0.19.1
[0.19.0]: https://github.com/illegalstudio/elephc/compare/v0.18.5...v0.19.0
[0.18.5]: https://github.com/illegalstudio/elephc/compare/v0.18.4...v0.18.5
[0.18.4]: https://github.com/illegalstudio/elephc/compare/v0.18.3...v0.18.4
[0.18.3]: https://github.com/illegalstudio/elephc/compare/v0.18.2...v0.18.3
[0.18.2]: https://github.com/illegalstudio/elephc/compare/v0.18.1...v0.18.2
[0.18.1]: https://github.com/illegalstudio/elephc/compare/v0.18.0...v0.18.1
[0.18.0]: https://github.com/illegalstudio/elephc/compare/v0.17.9...v0.18.0
[0.17.9]: https://github.com/illegalstudio/elephc/compare/v0.17.8...v0.17.9
[0.17.8]: https://github.com/illegalstudio/elephc/compare/v0.17.7...v0.17.8
[0.17.7]: https://github.com/illegalstudio/elephc/compare/v0.17.6...v0.17.7
[0.17.6]: https://github.com/illegalstudio/elephc/compare/v0.17.5...v0.17.6
[0.17.5]: https://github.com/illegalstudio/elephc/compare/v0.17.4...v0.17.5
[0.17.4]: https://github.com/illegalstudio/elephc/compare/v0.17.3...v0.17.4
[0.17.3]: https://github.com/illegalstudio/elephc/compare/v0.17.2...v0.17.3
[0.17.2]: https://github.com/illegalstudio/elephc/compare/v0.17.1...v0.17.2
[0.17.1]: https://github.com/illegalstudio/elephc/compare/v0.17.0...v0.17.1
[0.17.0]: https://github.com/illegalstudio/elephc/compare/v0.16.8...v0.17.0
[0.16.8]: https://github.com/illegalstudio/elephc/compare/v0.16.7...v0.16.8
[0.16.7]: https://github.com/illegalstudio/elephc/compare/v0.16.6...v0.16.7
[0.16.6]: https://github.com/illegalstudio/elephc/compare/v0.16.5...v0.16.6
[0.16.5]: https://github.com/illegalstudio/elephc/compare/v0.16.4...v0.16.5
[0.16.4]: https://github.com/illegalstudio/elephc/compare/v0.16.3...v0.16.4
[0.16.3]: https://github.com/illegalstudio/elephc/compare/v0.16.2...v0.16.3
[0.16.2]: https://github.com/illegalstudio/elephc/compare/v0.16.1...v0.16.2
[0.16.1]: https://github.com/illegalstudio/elephc/compare/v0.16.0...v0.16.1
[0.16.0]: https://github.com/illegalstudio/elephc/compare/v0.15.3...v0.16.0
[0.15.3]: https://github.com/illegalstudio/elephc/compare/v0.15.2...v0.15.3
[0.15.2]: https://github.com/illegalstudio/elephc/compare/v0.15.1...v0.15.2
[0.15.1]: https://github.com/illegalstudio/elephc/compare/v0.15.0...v0.15.1
[0.15.0]: https://github.com/illegalstudio/elephc/compare/v0.14.0...v0.15.0
[0.14.0]: https://github.com/illegalstudio/elephc/compare/v0.13.0...v0.14.0
[0.13.0]: https://github.com/illegalstudio/elephc/compare/v0.12.1...v0.13.0
[0.12.1]: https://github.com/illegalstudio/elephc/compare/v0.12.0...v0.12.1
[0.12.0]: https://github.com/illegalstudio/elephc/compare/v0.11.0...v0.12.0
[0.11.0]: https://github.com/illegalstudio/elephc/compare/v0.10.0...v0.11.0
[0.10.0]: https://github.com/illegalstudio/elephc/compare/v0.9.1...v0.10.0
[0.9.1]: https://github.com/illegalstudio/elephc/compare/v0.9.0...v0.9.1
[0.9.0]: https://github.com/illegalstudio/elephc/compare/v0.8.2...v0.9.0
[0.8.2]: https://github.com/illegalstudio/elephc/compare/v0.8.1...v0.8.2
[0.8.1]: https://github.com/illegalstudio/elephc/compare/v0.8.0...v0.8.1
[0.8.0]: https://github.com/illegalstudio/elephc/compare/v0.7.2...v0.8.0
[0.7.2]: https://github.com/illegalstudio/elephc/compare/v0.7.1...v0.7.2
[0.7.1]: https://github.com/illegalstudio/elephc/compare/v0.7.0...v0.7.1
[0.7.0]: https://github.com/illegalstudio/elephc/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/illegalstudio/elephc/compare/v0.4.0...v0.6.0
[0.4.0]: https://github.com/illegalstudio/elephc/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/illegalstudio/elephc/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/illegalstudio/elephc/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/illegalstudio/elephc/releases/tag/v0.1.0
