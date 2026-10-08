# Generics

## Task checklist

### Phase 0 — type arguments on `array`

- [x] Parse `array<T>` in every type position.
- [x] Reject a declared element type widened by an element write, as a hard error.
- [x] Reject a declared `array<T>` return whose body has a different element storage.
- [x] Audit the 23 passes that match on `TypeExpr::Array` for user-originated values.
- [x] Reject `array<T>` under `--strict-php` in `audit_type`.
- [x] Add lexer, parser, codegen, and error tests.
- [x] Add `examples/generics/` and `docs/beyond-php/generics.md`.
- [x] Sharpen loop inference. **Done 2026-09-19**, and the characterisation below was WRONG for
      most of the plan's life — twice. See "What actually loses the element type" and then
      "Where it really ends", at the end of this section.

#### What actually loses the element type, measured

Not "appending into `[]`". Six probes, one variable at a time:

| Body | Result |
| --- | --- |
| `$out = []; $out[] = 1; $out[] = 2;` — no loop | **passes** |
| `for (…) { $out[] = 7; }` — a constant | **passes** |
| `$k = 5; for (…) { $out[] = $k; }` — assigned before the loop | **passes** |
| `for (…) { $out[] = $v; }` — a parameter | **passes** |
| `foreach ($src as $v) { $out[] = $v; }` | **passes** |
| `for ($i = …) { $out[] = $i; }` — the loop COUNTER | fails |
| `$out = [0]; for (…) { $out[] = $i; }` — seeded non-empty | fails |

So the empty literal is innocent — a seeded `[0]` degrades too — and so is the loop machinery,
which handles a constant append fine. What fails is appending a value whose own type is still
settling: a local MODIFIED INSIDE the loop.

**The environment really does hold `array<mixed>`.** An intermediate conclusion said otherwise
and was wrong: it rested on

    $probe = takesInts($out);   // takesInts(array<int> $a) — accepted
    return $out;                // "declares array<int> but returns array<mixed>"

which proves nothing, because `type_accepts` is COERCIVE — `array<int>` accepts `array<mixed>`,
since `Int` accepts `Mixed`. Only the return path is strict, because it also runs
`require_matching_array_element_storage`, which compares storage rather than assignability.
**Never probe an element type from an argument position.**

So the loss is in the environment, in the loop, as first thought. What IS refuted, by
implementing it: a two-phase fixed point in `loop_storage` (settle the plain locals before
applying any array write) changes nothing. And neither `merge_array_element_type` nor
`join_array_payload_type` is at fault — both handle `Never` correctly, and both are where the
search naturally starts.

**The chain, as far as it is established.** `stabilize_loop_storage`
(`stmt_check/control_flow.rs:35`) computes contracts and writes them straight into the
environment:

    for (name, storage_type) in contracts {
        recorded.insert(name.clone(), storage_type.clone());
        env.insert(name, storage_type);        // this is what $out ends up as
    }

`representation_contract` (`loop_storage.rs:381`) returns `Some` only when the FIXED type is
`Mixed` — otherwise the name is not written at all. So `$out` being overwritten proves the
fixed point concluded `Array(Mixed)`, which in turn means the appended value was `Mixed` there.

**What is refuted, each by running it:**

- the empty literal (a seeded `[0]` fails too);
- the loop machinery (a constant append passes);
- the increment SYNTAX (`$i = $i + 1` fails exactly like `$i++`);
- a two-phase fixed point settling plain locals before array writes (implemented, no effect);
- `merge_array_element_type` and `join_array_payload_type` (both handle `Never` correctly);
- the "environment is fine, only the return path disagrees" reading — it rested on an argument
  position, and `array<int>` accepts `array<mixed>` coercively.

**Measured, by printing the fixed point instead of reasoning about it:**

    [loop] out entry=Array(Never) fixed=Array(Mixed)
    [loop] i   entry=Int          fixed=Mixed
    [loop] n   entry=Int          fixed=Int          (a parameter, never assigned)

    [asg]  i <- Mixed (source Opaque)

So `$i` enters as `int` and the fixed point makes it `Mixed`, which then poisons `$out`
through the append. There are TWO causes, not one:

**[FIXED] `$i++` was collected as `AssignedValue::Opaque`**, whose whole meaning is "no statically
   available RHS" and which maps to `Mixed` unconditionally. But an increment DOES have a
   statically available type: the variable's own. This is the actionable one — record an
   increment as the variable's arithmetic type (`Int` stays `Int`, `Float` stays `Float`,
   anything else stays opaque, since PHP's `++` on a string is a string increment).
Fixed with an `AssignedValue::Increment` variant resolved where the environment is in hand:
`Int` stays `Int`, `Float` stays `Float`, anything else stays as unknown as before. The fixed
point is now ACCURATE — traced as `out entry=Array(Never) fixed=Array(Int)` where it used to say
`Array(Mixed)`.

**It fixes exactly one shape**, and honesty demands saying which: a counter incremented in the
loop BODY (`while ($i < $n) { $out[] = $i; $i++; }`). Pinned by two codegen tests.

**[STILL OPEN] Everything with the increment in a `for` HEADER still fails**, and now for a
different reason than the one just fixed: the storage fixed point reports `Array(Int)`
correctly, and `representation_contract` therefore writes nothing into the environment — yet
`$out` still arrives at the return as `array<mixed>`. So a THIRD widening exists, in the
ordinary body-checking path rather than in `loop_storage` at all. `$i = $i + 1` fails the same
way.

Next step, and the same method: the fixed point is no longer the suspect, so trace what `$out`
holds in `env` immediately after `check_break_continue_target_body` returns, and compare it
with what the body's `check_array_push` put there.

Method note: three hypotheses about which `unwrap_or(PhpType::Mixed)` fires were wrong in a row.
The answer took one `eprintln` behind an env var, built into a separate target dir so the
running verification was undisturbed, and reverted to a byte-identical tree afterwards.

#### Where it really ends

The trace above was run, and it answered in one line:

    [loop] after stabilize: i=Int n=Int out=Array(Never)
    [loop] after condition: i=Int n=Int out=Array(Never)
    [loop] after update:    i=Mixed ...
    [loop] after body:      i=Mixed n=Int out=Array(Mixed)

The "third widening" is `infer_type_with_assignment_effects`'s own increment arm, which retyped
an incremented `int` local to `Mixed` — nothing to do with `loop_storage`, and firing wherever a
`++` appears, in a loop or not. Its reason was sound: PHP promotes an integer at the overflow
boundary, `PHP_INT_MAX++` is `float(9.223372036854776E+18)` in php-src and in elephc.

But `Mixed` is the TOP type, so every value derived from a counter inherited it. The honest type
is `int|float`, and that is what the arm now writes.

**What that fixes, and what it does not.** It does not make `for ($i = 0; …) { $out[] = $i; }`
satisfy a declared `array<int>`, and nothing could without giving up the overflow promotion: a
union is boxed storage either way, so the element storage genuinely differs from a packed `int`
vector. What it fixes is everything a type is FOR — the diagnostic now reads `returns
array<int|float>` and carries the cast that pins it, and a value derived from a counter is no
longer indistinguishable from a `json_decode` result.

**Two rules had to follow**, both found by the preludes rather than by reasoning. A scalar
parameter and a string offset each accepted `Mixed` while refusing `int|float`, which made the
NARROWER type the stricter one; 32 error tests and one lib test failed at once, all of them
prelude code passing a loop counter to an `int` parameter or indexing a string with it. Only the
arithmetic union is admitted, not unions in general: `string|false` at a `string` parameter must
keep being refused, because that diagnostic is how an unchecked sentinel gets caught.

**The precision cost a sweep of the gates, and the sweep was the work.** Six of them read "this
exact type, or `mixed`", so a type strictly MORE precise than `mixed` was refused where `mixed`
was allowed — the narrower type became the stricter one. Each was found by a different suite:
`type_accepts` at a scalar parameter (32 error tests, all prelude code passing a counter to an
`int` parameter), the string offset rule, the buffer ELEMENT rule, the buffer INDEX rule on three
paths, `merged_assignment_type` (whose `existing: Union(_) => None` arm made a re-walked body
report `cannot reassign $t from int to int|float` on unchanged code — `func_get_arg($i++)`
desugars to exactly that), and `is_integer_operand_type` for `$i & $mask`.

The rule that finds the rest without another 90-minute run: grep the acceptance SHAPE, not the
type — `PhpType::Int | PhpType::Mixed`, `(Int | Float | Bool, Mixed)`, `== PhpType::Mixed ||`. A
gate written against `codegen_repr()` never needed changing, because every union maps to `Mixed`
there; every gate written against the type itself did. And only the one NAMED union is admitted
(`PhpType::is_int_float_union`), never unions in general: `string|false` at a `string` parameter
must keep being refused, because that diagnostic is how an unchecked sentinel gets caught.

**Found while measuring, NOT fixed, and not caused by this** — a `for`-header counter wraps at
the integer boundary instead of promoting:

    for ($i = PHP_INT_MAX - 1, $n = 0; $n < 3; $i++, $n++) { var_dump($i); }
    elephc : … int(9223372036854775807) | int(-9223372036854775808)
    php    : … int(9223372036854775807) | float(9.223372036854776E+18)

The same increment in a `while` body, or outside a loop, is correct. Verified not to be the
`AssignedValue::Increment` change (neutralizing it changes nothing), and not the optimizer
(`--ir-opt=off` keeps the promoting `ichecked_add` and still wraps) — the unoptimized IR stores a
`Heap(Mixed)` into a slot another instruction reads as `I64`, so the loss is at the slot
boundary. It is the same "the type and the storage disagree" shape as the array_map defect, and
it wants a decision — box the counter and lose the fast loop, or keep the slot and lose the
promotion — rather than a quiet fix.

### Pre-existing bugs found while landing phase 0

Neither needs `array<T>` syntax to reproduce, so neither is caused by this work.

- [x] `array $a = [1,2,3]; $a[0] = 9;` SEGFAULTed — a typed-local array declaration bound the
      slot as `array<mixed>` but allocated `array<int>` from the initializer and emitted no
      `array_to_mixed` between them, so the element header tag still said int while the code
      read the slot as boxed. The declaration alone, with no element write, was fine, and
      `array<int> $a` was never affected because declared and allocated element types agree.
      **Fixed** in `coerce_typed_assign_value`: the conversion is driven from the value's type
      rather than the slot's history, through the same `conversion_op` the region fixpoint
      uses. Three codegen regressions pin it.
- [x] Returned arrays whose payload representation disagrees with the call site. **Fixed
      2026-09-19** — and it needed no runtime support at all. See the diagnosis below; the
      original claim that this required a new IR op, a runtime helper and codegen on two
      architectures was wrong.

  The cause is one arm of `coerce_to_return_type` (`src/ir_lower/stmt/return_coercions.rs`):
  `IrType::Heap(_)` emits an `Op::RuntimeCall` with no callee immediate that simply CLAIMS the
  value has `ctx.return_php_type`, converting nothing.

  Reduced, with no extension syntax:

  ```php
  function c(array $src) { return array_map(fn(int $x): int => $x * $x, $src); }
  echo c([1, 2, 3])[0];          // prints a pointer; php -n prints 1
  ```

  The same `array_map` call used directly is correct, because the result then stays
  `Heap(Mixed)` and every element read goes through a mixed-aware path.

  This is the same shape as the two previously reduced miscompiles (a returned nested array
  whose call site is `array<array<mixed>>`, and a union method return coerced through
  `mixed_to_hash` then moved into an indexed slot). They are one bug, not three.

  `array_storage_conversion` cannot express the fix: it requires the PREVIOUS type to already
  be an `Array(_)`, so a `Mixed` source returns `None`, and there is no unboxing op to emit in
  the `Mixed -> Array(T)` direction.

  **Re-verified, still live.** The reduced case prints a pointer where php prints 1:

      elephc : 4366994056  1
      php    : 1           1

  The exact arm is `return_coercions.rs:39` — `Op::RuntimeCall` with `None` as the callee
  immediate, which relabels the value with `ctx.return_php_type` and converts nothing. The
  inverse of `Op::ArrayToMixed` genuinely does not exist: `__rt_mixed_unbox` unboxes ONE value,
  not an array's elements.

  It is also the only WRONG-OUTPUT bug left on this plan, in plain PHP with no extension syntax,
  which by the project's own "no silent wrong output" rule makes it the highest-value one.

  **Diagnosis, 2026-09-19 — and a correction.** This entry used to end "so the fix is a new IR op
  plus a runtime helper plus codegen on both architectures, the largest remaining item". That was
  wrong, and the measurements say so. The relabel is the SYMPTOM; the cause is one level up, and
  no new backend work is needed.

  `array_map` deliberately keeps TWO answers for one call (`src/builtins/array/array_map.rs`):

  - `check()` returns the PHP-accurate container — `array<int>` for a callback declared `: int`;
  - `eir_result_type()` returns `Mixed`, on purpose, because a STRING callback binds through a
    runtime descriptor whose element ABI is only known once that descriptor resolves.

  Nothing reconciles them. The value is a boxed Mixed cell wrapping an array whose slots are
  themselves boxed Mixed cells. Every consumer that reads the EIR type is correct; every consumer
  that reads the CHECKER type — a return contract, a parameter contract — reads a cell as if it
  were an array.

  Measured, all against one prebuilt compiler, `[0]` of the mapped array unless stated:

  | shape                                                    | elephc  | php   |
  |----------------------------------------------------------|---------|-------|
  | `function c($s) { return array_map(fn(int $x): int …); }` | pointer | 1     |
  | same, with `: array` declared                             | pointer | 1     |
  | same, through a local                                     | pointer | 1     |
  | same, through two hops                                    | pointer | 1     |
  | `foreach` over the returned array                         | pointers, right count | 1,4,9 |
  | the result used INSIDE the function, never returned       | 1       | 1     |
  | the result passed to `f(array $a)`                        | pointer | 1     |
  | the result stored into `public array $items`              | compile error | 1 |

  So it is not a return-boundary bug: it is every boundary that quotes the checker's type. And it
  is NOT a general "Mixed reaches a container contract" hole — a genuine `mixed` value at a
  container contract is already a loud compile error (`Function 'f' parameter $a expects
  Array(Mixed), got Mixed`, and the same for returns). Only a builtin holding two answers slips
  through, because there the checker's own answer IS the container.

  Two more measurements decide the fix:

  - The whole-program storage fixpoint already handles the honest cases. A declared `array<int>`
    parameter is lowered as `array<mixed>` as soon as one call site passes a mixed-element array
    (verified with `--emit-ir`: one function, two programs, two parameter reprs), and a function
    with two disagreeing returns is lowered as `-> Heap(Mixed)`. Both print PHP's answer.
  - The lowering is ALREADY written for a narrowed result slot.
    `array_map_descriptor_callback_result_element_type` consults the EIR result slot first and
    accepts `Int`/`Bool`/`Str` from it — code unreachable today, because that slot is always
    `Mixed`. Given a container slot, the descriptor path selects `__rt_array_map`, which writes
    typed slots, and `box_array_result_for_mixed_builtin` then boxes nothing.

  **The fix**: make the two answers one answer, then keep the boundary honest.

  1. One shared element decision for `array_map`, used by `check()` AND by the EIR result type:
     the callback's return type when it is `Int`, `Bool` or `Str` and the callback operand is not
     a string; `Mixed` otherwise. That set is the intersection of what every lowering path
     accepts (`Float` stays `Mixed`, as today). Then switch the builtin to
     `BuiltinResultType::Checked`: `checked_result_type_fits_operands` answers `true` for
     `ArrayMap`, so the checker's type is used verbatim and the two channels cannot diverge
     again. Typed closures stop being boxed at all, which is also faster than today.
  2. Make the relabel arm of `coerce_to_return_type` a hard error when the value's repr is
     `Mixed` and the return contract is a typed container, instead of claiming the type. That is
     the durable guard: the next builtin that grows a second answer fails at compile time instead
     of printing a pointer.
  3. The same guard on the argument side, where the property side already refuses.

  No new IR op, no new runtime helper, no new assembly. `__rt_array_from_mixed` — the inverse of
  `__rt_array_to_mixed` — stays unwritten and unneeded: nothing has to turn boxed slots back into
  typed ones once the boxed slots are no longer created.

  **Landed.** Two edits and a test rewrite, all three applied after the previous full run reported
  (codegen 9051/0, lib 1723/0, error 1624/0, check 0 warnings):

  - `src/builtins/array/array_map.rs` — `BuiltinResultType::Checked`, and `mapped_element_type`
    narrows to `Int`/`Bool`/`Str` only, and only when the callback does not reach EIR as a string.
  - `src/codegen/lower_inst/array_access_runtime.rs` — unboxing a Mixed cell into a TYPED element
    contract is now refused (`unboxing cannot narrow boxed element slots to a typed element
    contract`) instead of emitted.
  - `tests/parser_tests/extensions.rs` — the enum/generic-interface test now pins the positive
    shape (`interface_args: [[Int]]`), which is what the parser produces since enums may implement
    a generic interface.

  The IR for the reduced case is now shorter as well as correct: `v4: Heap(Array) php=array<int> =
  runtime_call v2 v3 runtime.array_map` followed by `return v4`. The relabeling `runtime_call` is
  gone, and so is the boxing — `__rt_array_map_mixed` is no longer selected for a typed closure.

  Verified against `php` on every shape measured above, plus the shapes that must NOT narrow:

  | shape                                             | elephc | php  |
  |----------------------------------------------------|--------|------|
  | the reduced case, and `: array`, local, two hops    | 1      | 1    |
  | `foreach` over the returned array                   | 1,4,9  | 1,4,9|
  | passed to `f(array $a)`                             | 1      | 1    |
  | stored into `public array $items` (was refused)     | 1      | 1    |
  | `array_map('twice', $s)` returned (was a pointer)   | 4      | 4    |
  | `: float` callback returned (stays boxed)           | 1.5    | 1.5  |
  | `: array` callback returned (stays boxed)           | 2      | 2    |
  | associative source returned                         | 4      | 4    |
  | `callable` parameter / first-class callable         | 3 / 4  | 3 / 4|

  Twelve codegen regressions in `tests/codegen/arrays/callbacks.rs` pin all of it; the module's 30
  `array_map` tests pass. The guard was also proved to have teeth: with the OLD `array_map` and the
  NEW guard, `array_map('twice', $s)` returned from a function stops at compile time with exactly
  that message instead of printing a pointer.

  **The guard needed one exception, found by the suite.** `Array(Never)` is the EMPTY-array
  contract — it names no element storage — and a borrowed `?array` cell reaching `??` unboxes into
  it (`$r = $a ?? ["x", "y"]`). Three `control_flow::nulls` tests caught it. The trap in the first
  attempt at the exception: `codegen_repr()` does NOT recurse into an array's element, and
  `Never.codegen_repr()` is `Void`, so the arm has to test the element for `Void`, not `Never`.

  **Verdict after the fix**: `check` 0 warnings, `lib` 1723/0, `lexer` 220/0, `parser` 432/0,
  `error_tests` 1624/0, `codegen_tests` **9062 passed / 1 failed / 130 ignored** in 92 min. The one
  failure is `cli::test_cli_monitor_live_compiles_the_source_with_the_probe`, which monitors a
  6-second program for 1 second and needs the child to answer inside a window; its fixture has no
  arrays at all, and it passes in 9.3s on an idle machine. Load flake, not a regression.

  Generated docs regenerated afterwards: `Result type source: shared` -> `checked` on the array_map
  internals page, plus 55 `codegen_line` shifts in `date/` pages that this session's earlier edits
  had already desynced. `docs/php/arrays.md` now states the complement as well — which callbacks
  produce a typed result array rather than boxed elements.

  Pre-existing, NOT caused by this and still open: a string callback naming a BUILTIN is refused
  (`array_map('strtoupper', $a)` → `Undefined function: strtoupper`), on this build before and
  after. Worth its own reduction.

  Next instance of the same shape, recorded but not touched: `preg_split` — `check()` says
  `array<string>` for 2-3 arguments, `eir_result_type()` says `array<mixed>`. The guard does not
  catch it, because there the SOURCE is already a container (`Array(Mixed)`), not a boxed cell, so
  the relabel produces a real array with boxed slots under an `array<string>` contract. It needs
  pcre2 to verify here, which this machine does not have.

### Phase 0b — `array<K, V>`

Split out of phase 0: `TypeExpr` has no associative variant, so unlike `array<T>`
this DOES add an AST node and pulls in the full "Adding or changing an AST node"
audit.

- [x] Add the `TypeExpr` associative variant and resolve it to `PhpType::AssocArray`.
- [x] Walk it through every AST-walking pass per the AGENTS.md checklist.
- [x] Parse `array<K, V>` and extend the declared-contract check to hash storage.
- [x] Reject a key type that is not `int`, `string`, or `mixed`.
- [x] Reject a declared form whose body returns the other storage kind.
- [x] Accept the empty literal for either form.
- [x] Reject `array<K, V>` under `--strict-php`, with its own message.
- [x] Add parser, codegen, and error tests; extend the example and the docs.

The audit surface split cleanly in two. The compiler found 13 exhaustive matches
(name resolution, EIR lowering, the strict audit, type resolution, reachability,
the six `*_prelude/detect.rs` scanners, eval signature metadata). It could NOT
find the `_ =>` arms, which had to be walked by hand from the list of sites that
mention `TypeExpr::Array`; the ones that mattered were `autoload/walk.rs` and
`prelude_prune/usage.rs`, where missing the new variant would have let a class
named only in an `array<K, V>` be dead-stripped or never autoloaded.

Two neighbouring fixes fell out of it:

- `type_accepts` rejected `Array(Never)` — the empty literal — for a declared
  associative type whose key was not `Mixed | Int`, so `function f(): array<string, int>
  { return []; }` did not compile. The indexed arm already had that shortcut; the
  associative one now mirrors it.
- The declared-contract diagnostic took the ELEMENT type, so an associative
  violation reported `array<int>` for a local written `array<string, int>`. It now
  takes the whole container type.

### Diagnostics (independent, do separately)

- [x] Format `PhpType` with `Display` instead of `{:?}` in argument-mismatch errors. Five sites,
      56 expectations. `Display` already existed and was complete; the one gap it had was
      `Object("")` — the bare `object` hint — which rendered as nothing and produced
      `expects , got int`.
- [x] A declared local was judged against a GUARD'S NARROWING rather than its declaration:
      `?Node $c = $head; while ($c !== null) { $c = $c->next; }` was rejected with
      `cannot reassign $c from Node to Node|null`, which blocked the cursor idiom every linked
      structure uses. `Checker::typed_local_names` now keeps the declared TYPE, not just the
      name, and the reassignment consults it once the ordinary merge has failed — so the branch
      can only accept what used to be an error, never change a program that already compiled.
      `array<int>` is now user-written syntax, but `require_compatible_arg_type`
      (`call_validation.rs:332`) and `binding_mismatch_error`
      (`param_binding.rs:170`) still print `Array(Int)`. Touches ~54 error-test
      expectations, so it is its own change.

### Phase 1 — generic functions

- [x] Parse `function f<T>(...)`, with a `type_params` field on `StmtKind::FunctionDecl`.
- [x] Thread `type_params` through every AST-rewriting pass.
- [x] Reject a generic declaration under `--strict-php`.
- [x] Report an honest diagnostic until instantiation lands, instead of `Unknown type: T`.
- [x] Type substitution over `TypeExpr` (`TypeExpr::substitute_type_params`).
- [x] Type argument inference from a call site's actual types (`generics::infer_bindings`).
- [x] Instantiation naming (`generics::instantiated_name`).
- [x] Record requested instantiations during checking.
- [x] Splice instantiated declarations into the program and re-check to a fixpoint.
- [x] Resolve a call to its instantiated symbol at lowering.
- [x] Substitute type parameters inside a body (a typed local `T $tmp = …`).
- [x] Reject polymorphic recursion instead of relying on the round cap.
- [x] Support bounds (`<T : Entity>`) and defaults (`<K = string>`).
- [x] Make bound checking hierarchy-aware.
- [x] Parse `@template` docblocks into the same model.

#### The docblock surface

`src/docblock.rs` reads `@template`, `@param`, `@return`, `@var`, `@extends` and `@implements`
out of `/** … */` comments and fills the same fields native syntax fills, on a function, a class
or an interface, so both surfaces reach one monomorphization path. The class half is written up
under "`@template` on a class, as built".

**Order matters, and getting it wrong destroyed the point.** The pass runs AFTER
`strict_php::check_file_with_mode`, not before. An annotated file is valid PHP — the annotations
are comments — so `--strict-php` must keep accepting it. Applying the annotations first made the
audit see the injected `<T>` and `array<T>` and reject a file php-src parses happily, which is
exactly the file this surface exists to serve. Only the end-to-end `--strict-php` test caught
it; every unit test still passed.

It runs PER PHYSICAL FILE because the lexer discards comments, so they are recovered from the
source text and matched by line — which only means anything while the file is still its own,
before include resolution splices without rebasing line numbers. That is why
`finalize_physical_program` gained a `source: &str` parameter (four call sites).

Four rules keep an existing codebase safe:

- A doc comment with no `@template` is inert. `@param`/`@return` alone carry no type parameter,
  and acting on them would re-type every annotated PHP file in the world.
- An annotation the grammar rejects (`non-empty-list<T>`) is ignored, never a compile error.
- Written syntax wins over an annotation, so a file migrates one function at a time.
- Annotation types go through `parse_type_expr`, the language's own grammar, so a docblock
  cannot spell a type the language cannot and a new type form is gained in both at once.

#### Bounds and defaults

`type_params` is now `Vec<TypeParam>` — name, optional bound, optional default. The ten
AST-rewriting passes pass it through untouched, so only the type changed there; the real
consumers are the parser, the strict audit, `FnDecl`, and `infer_bindings`.

The `:` is unambiguous inside a type parameter list: a return type's `:` comes after the
parameter list's `)`, and an enum's backing `:` never appears inside `<...>`.

A DEFAULT replaces `InferError::Unconstrained` when nothing determines the parameter;
inference still wins over it. A BOUND is checked at the instantiation, so a violating call is
rejected at the CALL rather than inside the instantiated body.

Bound checking lives in `instantiate_generic_call`, NOT in `infer_bindings`. Satisfying a bound
is a subtyping question and only the checker holds the class table: `type_accepts` already
answers it for subclasses, implemented interfaces and interface inheritance. Inference knows
nothing about the class graph and could only compare spellings, which admits an unrelated class
that merely happens to declare the same members — the case the test
`bound_rejects_an_unrelated_class` pins.

That split is why the bound check reads `self.resolve_type_expr` on both sides: the bound and
the type argument are `TypeExpr`s, and the question is asked in `PhpType`.

#### Five review decisions, and what each cost

**1. Polymorphic recursion FAILS.** It is caught by type-argument DEPTH
(`MAX_TYPE_ARGUMENT_DEPTH`, 8) at the instantiation that creates it, so the diagnostic names
the type that ran away: `<T> reached array<array<array<...<int>>>>`. Depth is the signal
because it is what actually grows — an instantiation COUNT cannot tell a runaway from a
template a program simply uses with many types. The round cap survives as a backstop and is now
an ERROR; it previously broke out of the loop and compiled on, which would have emitted a
program missing instantiations its call sites needed.

**2. The `Span` ambiguity is TREATED, and treating it found a real bug.** The call-site key is
now `(enclosing function, Span)`. One source position inside a template is reached once per
instantiation of that template and legitimately resolves differently each time:
`twice<T>() { return identity($value); }` calls `identity<int>` from `twice<int>` and
`identity<string>` from `twice<string>`, at the same line and column. Keying on `Span` alone
collapsed those into one and made the second call invoke the first's function. The residue —
same position, same-named enclosing function, two included files — is a hard compile error,
the same "no silent wrong output" invariant `binding_decision_ambiguity` enforces.

The checker and lowering must agree on the enclosing name, including `"main"` for top-level
statements, where the checker has no enclosing function and lowering uses that literal.

**3. The thread-local is GONE.** `generic_call_sites` is a `LoweringContext` field threaded
like `builtin_call_types`. Cost: one parameter on `lower_body_into_function` plus the two
`LoweringContext::new` call sites. Much smaller than feared — the fifteen intermediate
functions were mostly helpers that never build a context.

**4. Body substitution is COMPLETE.** It reuses `magic_constants::walker`, whose statement and
expression matches carry no wildcard arm, through a new `Pass::transform_type` hook applied at
every type position: typed locals, `buffer<T>` element types, and the parameter, variadic and
return types of functions, methods and closures. Borrowing that walker rather than writing a
second one is the whole point — the walk that misses a node leaves `T` behind.

`generics::substitute_in_body` is shared by the checker's instantiation and the AST splice, so
the signature the checker resolves describes exactly the function the backend emits.

**5. The `identity<int>` naming stands.**

#### A third pre-existing miscompile found on the way

A typed-local string passed to a CLOSURE comes back zeroed — five NUL bytes where the string
should be, length intact, pointer lost:

```php
function plain(string $value): string {
    string $held = $value;
    $again = function (string $inner): string { return $inner; };
    return $again($held);
}
```

No generics involved. Each half is correct alone: drop the typed local, or call an ordinary
function instead of a closure, and it works. Same family as the typed-local array declaration
crash fixed in phase 0, which suggests typed-local declarations are a thin spot worth auditing
as a group.

#### It works end to end

```php
function identity<T>(T $value): T { return $value; }
function firstOf<T>(array<T> $xs): T { return $xs[0]; }
function pairUp<K, V>(K $k, V $v): string { return $k . "=" . $v; }
function twice<T>(T $value): T { return identity($value); }
```

lowers to eight monomorphic functions, every one of them unboxed:

```
function identity<int>(value: I64 php=int) -> I64
function identity<string>(value: Str php=string) -> Str
function identity<float>(value: F64 php=float) -> F64
function firstOf<int>(xs: Heap(Array) php=array<int>) -> I64
function firstOf<string>(xs: Heap(Array) php=array<string>) -> Str
function pairUp<string, int>(k: Str php=string, v: I64 php=int) -> Str
function twice<int>(value: I64 php=int) -> I64
function twice<string>(value: Str php=string) -> Str
```

#### How the three halves fit together

1. **Checker.** A template is registered in `fn_decls` with its `type_params` and deliberately
   kept OUT of `functions`, so `resolve_unchecked_functions` never checks its body against `T`.
   A call to it goes through `instantiate_generic_call`, which infers the bindings, creates an
   ordinary `FnDecl` under the instantiated name, and records the request. From there every
   existing checker path applies to it with no knowledge that generics exist.
2. **Pipeline.** `generics::monomorphize` runs check-splice-check to a fixpoint, then strips
   the templates. Both the CLI pipeline and the codegen test harness call it — a caller that
   checks by hand would leave templates in the program and hand lowering a call to a function
   that does not exist.
3. **Lowering.** `lower_function_call` swaps the template name for the instantiation before
   anything else looks at it.

#### Two decisions worth challenging

**Lowering does NOT re-derive the instantiation.** That was the original design, and it is
wrong: choosing the instantiation needs the argument TYPES, and `lower_args_with_signature`
needs the callee signature BEFORE it lowers the arguments. So the checker's answer is carried
across, keyed by `Span` exactly like the neighbouring `builtin_call_types` that lowering
already consumes the same way. The residual risk is the one that key already carries
repo-wide: a `Span` has no file identity.

**It travels in a thread-local, not a `LoweringContext` field.** The consumer sits under
fifteen intermediate lowering functions that would all have to thread the map. `strict_php`
sets the precedent for a compile-wide constant fact held this way, and states the reason the
pipeline is single-threaded per invocation.

#### Two traps this hit

- The instantiation request has to be recorded on EVERY call, not only the first. The pipeline
  re-checks after splicing, and on that pass the declaration already exists; skipping the
  request then left the final result with an empty list, and the pruner dropped every
  instantiation.
- A generic instantiation is reachable by construction but NO call site names it in the AST —
  lowering resolves that from the checker's answer. Without adding `instantiation_roots` to the
  pruner's roots, reachability strips them all and lowering finds nothing to call. Both the
  pipeline and the test harness need it.

#### What `src/generics.rs` already does

The inference and naming half is written and unit-tested (11 tests). `infer_bindings` unifies
each declared parameter type against its argument type, deliberately mirroring the shapes
`substitute_type_params` can produce, so anything substitution writes it can read back:

| Declaration | Argument | Binding |
| --- | --- | --- |
| `T $x` | `int` | `T = int` |
| `array<T> $xs` | `array<string>` | `T = string` |
| `array<K, V> $m` | `array<string, int>` | `K = string`, `V = int` |
| `?T $x` | `int` | `T = int` — the nullable wrapper is the declaration's |

Anything else contributes no binding rather than guessing, so an unconstrained type parameter
is REPORTED (`InferError::Unconstrained`) instead of silently becoming `mixed`. Two positions
that disagree are a `Conflict`; an argument type with no source spelling (`ptr`, `buffer`,
`resource`, the codegen-internal shapes) is `Unspellable` and makes the call site decline.

`instantiated_name` produces `identity<int>`. That spelling is deliberate: it reads correctly
in a diagnostic, `mangle_fqn` already escapes `<` and `>` into a valid symbol, and it cannot
collide with a user function because the parser rejects `<` in a name.

Nothing outside the tests calls it yet, so the module carries
`#![cfg_attr(not(test), allow(dead_code))]` with that reason stated in its header.

#### Note on the AST field's test cost

`cargo build` does not compile `#[cfg(test)]` modules, so the 21 errors the field first
produced were only the production ones. `cargo test --lib` then surfaced 218 more constructions
in the optimizer/ir_lower test trees, mechanically rewritten across 36 files. Run
`cargo test --lib --no-run` as well as `cargo build` after changing an AST node.

#### The AST field was cheap

Adding `type_params` to `StmtKind::FunctionDecl` produced only 21 compile errors across 10
files, all of them the AST-rewriting passes AGENTS.md lists — conditional compilation, magic
constants, name resolution, three optimizer passes, the resolver, the strict audit, and the
synthetic-class builder. The preludes construct declarations through builders, so they needed
nothing. The compiler enforced the audit checklist by itself here, unlike phase 0b's `_ =>`
arms.

#### Where instantiation has to live

The existing function-variant machinery is NOT reusable. It dispatches through a RUNTIME flag
(`emit_function_variant_dispatcher`, `_fn_variant_active_<name>`), because which `ifdef`
variant is live is decided at link time. Generics resolve statically, per call site: the call
site itself has to name a different symbol.

So the call site must be rewritten, and that rules out both obvious shortcuts:

- A pre-checker pass cannot infer type arguments, because it does not have argument types.
- A Span-keyed `call site -> instantiated name` map would inherit the `Span` identity problem
  already documented for the three decision maps.

That leaves a monomorphization pass that runs BETWEEN type checking and lowering, rewriting
`identity(5)` into `identity$int(5)` and emitting the instantiated declarations. It needs a
check-instantiate fixpoint, since each instantiated body must itself be checked and may
instantiate more. Precedent for the rewriting half exists in `flatten_classes`, the resolver's
include splicing, and `magic_constants`.

This is the structural work the plan flagged from the start, and it is where the
`functions: HashMap<String, FunctionSig>` rekeying decision actually bites.

### Phase 2 — generic classes and interfaces

- [x] Parse `class Box<T>` / `interface Repository<T: Entity>`, with a `generics` field on
      `StmtKind::ClassDecl` and `StmtKind::InterfaceDecl`.
- [x] Parse type ARGUMENTS on an inheritance clause (`implements Repository<User>`).
- [x] Parse `Box<int>` in every type position (`TypeExpr::GenericClass`).
- [x] Parse `new Box<int>(...)` (`ExprKind::NewGeneric`).
- [x] Split the `>>` token in type position for nested type arguments.
- [x] Instantiate every mention to a fixpoint, before the checker runs.
- [x] Strip templates so the checker never meets an unrepresentable annotation.
- [x] Check bounds in the checker, where the class table is.
- [x] Reject arity mismatches, missing arguments with no default, and type arguments on a
      class that declares none.
- [x] Infer a generic FUNCTION's parameters from a generic class argument.
- [x] Resolve type parameters correctly inside a namespace (this was broken for generic
      FUNCTIONS too, and the fix covers both).
- [x] Support static access on a generic class type (`Box<int>::of(1)`, `Box<int>::LABEL`,
      `Box<int>::class`).
- [x] Support `instanceof Box<int>`, disambiguated from a comparison chain by what follows.
- [x] Reject the whole surface under `--strict-php`: declarations, inheritance arguments,
      and mentions.
- [x] Add parser, codegen and error tests; extend the example and the docs.
- [x] Accept bare `Box` as `Box<bound>`. **Done 2026-09-19**: each parameter is read at its
      bound, its default, or `mixed`, and every bare mention WARNS, because the reading takes
      exactly one instantiation unless the parameter is declared covariant.
- [x] Infer `new Box(5)` as `Box<int>` from the constructor arguments.
- [x] Warn when that inference lands on `mixed`, which is erasure by another name. Writing
      `new Box<mixed>(…)` says the same thing deliberately and warns about nothing.
- [x] Reject a construction that resolves two ways rather than picking one.
- [x] Resolve a WRITTEN `new Box<T>(…)` inside a generic function, which `generics::classes`
      cannot: `T` means nothing until a call binds it, and the checker instantiates the
      function on the fly and walks a body no pass has rewritten yet.
- [x] Infer at a named static factory (`Box::of(5)`), not only at `new`.
- [x] Substitute `self` in a type argument; refuse `static` and `parent` by name.
- [x] Support `catch (Err<int> $e)`, the fifth place PHP names a class.
- [x] Reject `new Box(null)` at the construction instead of substituting the null type into the
      declaration.
- [x] Read `@template` on a CLASS, with `@var`, `@extends` and `@implements`.
- [x] Let an enum implement a generic interface. `EnumDecl` gained the same `GenericDecl` a class
      carries; the parser already PARSED the arguments and threw them away, so keeping them was
      the small half. An enum variant has no `..base` spread, so the compiler named all 12 sites —
      patterns and constructions alike — which makes this sweep safe in a way the `ClassMethod`
      one was not.

      Two things it needed beyond carrying the field: the walker must route the clause through
      `walk_inherited_list` rather than merely transforming the argument types, or the enum keeps
      implementing the stripped TEMPLATE and codegen panics with missing interface metadata; and
      `audit_generic_decl` must see it, or `--strict-php` would accept an extension.

      Noted while testing, NOT introduced here: elephc does not check an enum's method signatures
      against any interface it implements, generic or not. A `get(): string` satisfies a plain
      `Plain { get(): int }` too. Out of scope, but it is why the new test pins parameter
      acceptance rather than conformance.
- [x] Collect a template declared inside a conditional — **deliberately NOT done, 2026-09-19**,
      and the answer fixed instead. Measured first: an ORDINARY `class Foo` inside
      `if (!class_exists('Foo')) { … }` is already invisible (`Undefined class: Foo`), so
      collecting a TEMPLATE there would make generics the odd one out — its instantiations
      splice at the top level and would exist unconditionally next to the class that does not.
      What was wrong was the message: `'Box' is written with type arguments but declares no type
      parameters` is FALSE, the declaration is right there. The mention now says where its
      template is and that a conditional declaration is not part of the compiled program.

#### An external review, and which half of it survived contact

Three models (Kimi K3, GLM 5.3, DeepSeek v4.1) were given the design above and asked for
soundness holes. Every claim was then RUN through the compiler rather than taken on trust,
which is the only reason the list below splits the way it does.

**Five were real, and are fixed:**

- `Box<self>` emitted a class literally named `Box<self>`, whose property was typed `Box<self>`.
  `self` is lexical, so the instantiating pass can substitute it from the class it is walking;
  `static` is late-bound and `parent` needs an inheritance clause that pass does not carry, so
  both are now refused by name.
- `Box::of(5)` reported `Undefined class: Box`. A named static factory is how a PHP codebase
  offers a second way to build something — there is only one `__construct` — so inference that
  fired only at `new` missed where most construction happens. Static methods now infer too.
- `catch (Err<int> $e)` was a parse error. It is the FIFTH place PHP names a class, and once
  `Err<int>` is a real class, being unable to name it in a catch is a hole in the language, not
  a parse inconvenience. It now discriminates: `Err<int>` does not catch an `Err<string>`.
- `new Box(null)` reported `parameter $v cannot use type void` — a type in a declaration the
  programmer never wrote. `null` carries no information about what a container holds, so it is
  rejected at the construction with the way out.
- **Rejecting a construction that resolved two ways was wrong, and the written rationale was
  wrong with it.** Two models pointed this out independently: after `splice_instantiations`
  clones `wrap`'s body, the two `new Box($v)` ARE different AST nodes — a clone keeps its
  source spans, so what collided was the KEY, not the node. The function-call path had already
  solved this by keying on (enclosing function, span). Doing the same costs one stack in the
  walker and un-bans the most ordinary generic-wrapper pattern there is.

**Four were wrong, and the compiler said so:**

- `$x instanceof Foo < BAR;` would be misparsed — it is not. There is no closing `>`, so the
  argument list never parses and nothing is claimed.
- `instanceof Box<Box<int>>` never finds its closing `>>` — it does. `AngleCloses` splits the
  token, which is what that mechanism is for.
- `Box<int>>$y` diverges silently from php-src — it does not. It is a compile error.
- `Box<User>` and `Box<user>` become two classes — they do not. The name resolver canonicalises
  before this pass runs; both give `Box<User>`.

**Two were questions, now answered in the docs:** the template does not exist at runtime
(`class_exists('Box')` is false, `$b instanceof Box` is false), and a bound reached through a
wrapper (`class Holder<U>` holding a `Box<U>` where `Box` needs `T : Entity`) is rejected at the
USE — `Holder<int>` — rather than by inventing an invisible bound on `U`.

The pattern worth keeping: the models were good at finding where a design's stated REASONING
was weak, and unreliable about what the implementation actually does. The rationale for
rejecting an ambiguous construction was the one thing in this design that did not survive being
read back by someone else, and both models that mentioned it were right.

#### Two bugs the `new Box(5)` work uncovered, both older than it

**A function's return and variadic types were transformed after leaving its scope.** The walker
built its `StmtKind::FunctionDecl` with `return_type: return_type.map(|ty| pass.transform_type(…))`
inside the struct literal, which Rust evaluates AFTER the `pass.leave_function()` above it. Two
of a function's type positions were therefore handed to the pass with the ENCLOSING scope
active. For `generics::classes` that meant `function wrap<T>(T $v): Box<T>` had its return type
instantiated as though it were outside the template, emitting a `Box<T>` class whose property is
typed `T` — reported as `Unknown type: T` in a declaration nobody wrote. The closure arm had the
same shape. Both now compute those two types before leaving.

**A warning only an early round could produce was dropped with that round's result.**
`monomorphize` returned the LAST check's `CheckResult`, and each round re-checks the whole
program, so ordinary warnings survive by being re-produced. The mixed-inference warning cannot
be: by the next round the construction reads `new Box<mixed>(…)` and there is nothing left to
warn about. Warnings are now unioned across rounds, deduplicated by span and message.

#### `@template` on a class, as built

The four things native syntax writes inside the `<>` each have a PHPStan spelling, and each
lands in the same field: `@template` in `type_params`, `@extends Base<int>` in `extends_args`,
`@implements Iface<T>` in `interface_args`, and `@var`/`@param`/`@return` on the members.
Nothing downstream can tell an annotated declaration from a written one, which is the point —
`generics::classes` sees one shape.

Three decisions carried the work.

**Only an annotation that MENTIONS a type parameter is honoured inside the class.** A member's
doc comment carries no `@template` of its own, so the function surface's guard — "no `@template`,
no effect" — cannot be the test. Without a replacement, `@template` on a class would promote
every `@param` in the body into a declaration the compiler enforces, and adding one line to a
class would re-type its whole body. `mentions_type_param` is exactly the set of annotations
generics need and nothing else.

**The alignment invariant is the parser's, so the annotation must reproduce it.** `parse_name_list`
returns one `interface_args` entry per written name, empty where the name was bare, so
`class Box<T> implements Countable` carries `[[]]` and not `[]`. Building `[]` from an
annotation would give the same program two ASTs — the trap `exception_type_args` already
sprang once, caught there by the prelude-body comparison and the printer round-trip.

**A promoted constructor parameter is two AST nodes.** The parser builds the `ClassProperty`
from the parameter's written type, so retyping only the parameter leaves `new Box(5)` inferring
`T = int` from the signature and storing into a `mixed` field.

Two bugs the work surfaced, neither about classes:

- `parse_block` trimmed only the continuation `*`, so it read ` * @template T` and silently
  ignored `/** @var T */`. A member's doc comment is almost always the single-line shape, so
  nothing in the class surface worked until both markers came off.
- The codegen harness did not mirror `finalize_physical_program` and skipped `docblock::apply`
  entirely. A `@template` fixture compiled as ordinary untyped PHP and still printed the right
  values, so the surface would have looked tested while nothing exercised it. Only `get_class`
  showed it: `Box` where `Box<int>` was expected.

#### Design for `new Box(5)`, as built

The last ergonomic gap, and what `@template` on a CLASS waited on: a docblock cannot write type
arguments at the mention, so until a construction could infer them there was nothing to read the
annotation into. Sketched here because the shape is not obvious and one part of it is a known
hazard.

Constructor arguments are only typed by the CHECKER, so this half cannot stay pure syntax the
way written arguments do. The pieces:

1. `monomorphize` already hands the checker the bound obligations; add the class TEMPLATE
   SIGNATURES beside them — name, `type_params`, and the constructor's declared parameter
   `TypeExpr`s. A signature list, not the `Templates` store, which holds whole ASTs.

2. `Checker::infer_new_object_type` recognizes a template name and calls the EXISTING
   `generics::infer_bindings(type_params, ctor_param_types, actual_types)` — the same function
   a generic function call uses. It checks the bounds on the spot (it is the checker, and
   `type_accepts` is right there), records the instantiation on `CheckResult`, and returns
   `PhpType::Object(instantiated)`.

3. The next round's `classes::instantiate` splices those instantiations with the `splice` it
   already has, AND REWRITES the `NewObject` node's class name in the AST.

That third step is the one worth arguing about, and it was built as designed. The alternative
was a span-keyed side map read by lowering, the way `generic_call_sites` works for functions.
Rewriting the AST is better here: lowering needs no new map, reachability records the
instantiated class by itself because the AST names it, and the invariant "the AST names the
class that gets emitted" is preserved. The span key still exists, but only between the checker
and the very next round.

It is keyed by a BARE span, unlike `generic_call_sites`, and the ambiguity that key cannot
express is REJECTED rather than resolved — a span that collected two different classes is a
compile error naming both. The enclosing-function half of the other key exists to resolve that
case; here there is nothing to resolve it to, because the AST node is one node.

Both open questions were decided as the design suggested:

- `new Box($mixed)` warns. It compiles, correctly, at twice the allocation cost of every other
  instantiation — and giving up the storage the feature exists to pin deserves a word.
  `new Box<mixed>(…)` says the same thing deliberately and warns about nothing.
- `new Box()` with nothing to infer from points at `new Box<...>(...)`, not at the declaration.

#### The measurement the plan opened with, now for a generic CLASS

The plan's table compared a hand-written `int` field against a hand-written `mixed` one. With
generic classes landed, the question is whether an INSTANTIATION reaches the same floor. Same
shape, same 3,000,000 iterations, same dev build on `macos-aarch64`:

| Container | Allocations | Time |
| --- | --- | --- |
| `class Box<T>` used as `Box<int>` | 6,000,001 | 0.11 s |
| hand-written `private int $value` | 6,000,001 | 0.11 s |
| erased `private mixed $value` | 12,000,001 | 0.21 s |

The instantiation is indistinguishable from the hand-written monomorphic class — same allocation
count to the unit — and halves what erasure costs. That is the whole thesis, measured on the
feature rather than on a stand-in for it.

Times are the best of five on an idle machine, stable across three repetitions; the allocation
counts are exact.

#### What smoke-testing found that the tests did not ask for

Exercised against the built compiler, all correct:

- A SELF-REFERENTIAL generic class terminates. `class Node<T> { public ?Node<T> $next; }` with
  `new Node<T>` in its own method is the shape that could ask for a new class every round.
- A typed collection works end to end: `class TypedList<T> implements Countable` over
  `array<T>`, with `count()` through the interface and a typed `foreach`.
- Exotic type arguments all instantiate and mangle: `Box<Status>` (an enum), `Box<?int>`,
  `Box<int|string>`, `Box<array<string, int>>`.
- Namespaced generics across three namespace blocks with `use` imports:
  `App\Support\Box<App\Model\User>`, head and argument both canonicalized.
- Static properties are PER INSTANTIATION (`Cell<int>::$created` is 2 while
  `Cell<string>::$created` is 1), which is the one place monomorphization and erasure give
  different answers. Documented rather than hidden.

Two errors it surfaced were NOT generics bugs, each checked against a non-generic equivalent:

- Building an array by appending inside a generic method hits the known `array<mixed>`
  inference limit. The diagnostic names the instantiation (`TypedList<int>::map`), and it is a
  compile error rather than a silent miscompile.
- `?Node<int> $cursor = $head;` then `$cursor = $cursor->next` is rejected — and so is the same
  code with a plain `?Node`. A nullable typed local narrows to its non-null initializer and
  then refuses the reassignment, which makes the cursor idiom that generic containers need
  awkward. Worth its own fix; see the diagnostics section.

#### Why a class needs no inference, and what that bought

A generic FUNCTION's type arguments come from its call site's actual types, so only the
checker can resolve them — which is why phase 1 is a check⇄splice fixpoint. A generic CLASS
writes its arguments at every mention: `new Box<int>(5)`, `Box<int> $b`,
`implements Repository<User>`. Nothing is inferred, so instantiation is **pure syntax** and
runs before the checker.

That is the whole design. `src/generics/classes.rs` walks the program, rewrites every
`Box<int>` to `Named("Box<int>")`, appends one ordinary `ClassDecl` under that name, and
repeats until a round adds nothing. No pass after it has a notion of a generic class — the
checker, the optimizer and the backend see `Box<int>` as an ordinary class whose name happens
to carry angle brackets, exactly as they see `identity<int>` as an ordinary function.

The three decision maps needed no instantiation discriminant after all. They are keyed by
`(enclosing function, Span)`, and an instantiated class's methods are ordinary methods of a
DIFFERENT class, so their keys already differ.

#### Bounds are the one thing the checker still answers

Deciding that `User` satisfies `T : Entity` is a subtyping question and the class table that
answers it exists only in the checker. The instantiating pass records one obligation per
binding and `Checker::verify_class_type_argument_bounds` answers them all with `type_accepts`
— the same predicate `functions::resolution::instantiate` uses for a generic function's
bounds. Two bound checks that disagreed would accept a class at a type argument the equivalent
function rejects.

#### `>>` is one token, and the parser splits it

`Box<Box<int>>` ends in a single `GreaterGreater`. Splitting it in the lexer is not an option:
`$a >> $b` is the same two characters and the lexer cannot know it is inside a type. So
`AngleCloses` carries a credit — the innermost list that meets a `>>` consumes the whole token
and leaves one, the enclosing list spends it instead of consuming. Three levels lex as `>>`
then `>`, four as `>>` then `>>`, and both fall out of the same rule. `array<array<int>>` was
a parse error before this and now works.

#### Inference over a generic class parameter

`function unwrap<T>(Box<Box<T>> $b): T` has to bind `T` from an argument whose type is, by
then, an ordinary object called `Box<Box<int>>`. The instantiated NAME is a lossless encoding
of its arguments — `instantiated_name` writes it with `describe`, whose output is valid type
syntax — so `generics::instantiated_type` reads it back through `parser::stmt::parse_type_expr`
and unification proceeds structurally. A round trip through the language's own grammar, not
string surgery on angle brackets.

#### Three traps this hit

**The fast path skipped the error.** `classes::instantiate` returned early when the program
declared no template. But a program with no template can still MENTION one — `new Plain<int>()`
on an ordinary class — and skipping the walk let that reach the checker, which panicked with
`internal error: basic expression routed to call/object inference`. The walk now always runs;
the cost is one AST rebuild in a pipeline that already does several.

**The guard covered generic classes but not generic functions.** A mention inside a template
is not concrete, so the walk leaves it alone. `enter_class` tracked that for classes, and
`function unwrap<T>(Box<T> $b)` slipped through: `Box<T>` was instantiated at the literal type
`T`, emitting a class whose property is typed `T`. `enter_function` now counts the same way.

**The name resolver qualified `T` to `App\T`.** Inside a namespace, a type parameter is an
unqualified name like any other, so the resolver canonicalized it against the namespace and the
checker then reported `Unknown type: App\T`. `restore_type_parameter_names` maps the resolved
spelling back after resolution — which is also the correct SHADOWING rule: inside `Box<T>`, `T`
means the parameter, not a class that happens to share its name. This was broken for generic
FUNCTIONS since phase 1; the fix covers both, and `new Box<int>(...)` needed its own resolver
arm besides, because the catch-all left the class type and the constructor arguments entirely
unresolved.

#### It works end to end

```php
interface Entity { public function id(): int; }
interface Repository<T: Entity> { public function find(int $id): T; }
class UserRepository implements Repository<User> {
    public function find(int $id): User { return new User($id); }
}
class Box<T> {
    public function __construct(private T $v) {}
    public function get(): T { return $this->v; }
}
$i = new Box<int>(41);
$s = new Box<string>("ok");
```

emits `_method___Box_x3c_int_x3e____get` and `_method___Box_x3c_string_x3e____get`: two
classes, two storage layouts, no boxing. `mangle_fqn` already escapes `<` and `>`, so the
instantiated name needed no new symbol scheme.

### Phase 3 — variance

- [x] Settle whether vtable slots agree between two instantiations of one template. They do NOT
      after pruning; see the design note. Fixed in `reachability/reconcile.rs`.
- [x] Add `+T` / `-T` markers behind a parser slot that can also accept `in` / `out`.
- [x] Read `@template-covariant` / `@template-contravariant`, PHPStan's spelling.
- [x] Reject a type parameter in a position its marker does not admit, with polarity composing
      through nested instantiations.
- [x] Admit the widening in `type_accepts`, gated on the storage proof.
- [x] Let `in` / `out` be written as words. One token of lookahead, so a parameter may still be
      named `out`.
- [x] Report the widening that was refused, rather than the plain mismatch.

#### The slot invariant, asserted

`assert_instantiation_vtable_slots_aligned` sits beside the inheritance assertion in
`reachability/reconcile.rs` — not in the instantiation pass, which holds no vtable information.
It caught a REAL latent miscompile the same hour it was written: an instantiated generic method
was taking a vtable slot, so `Pair<int>` (called with `withRight<string>` and `withRight<int>`)
numbered `withright<int>` at slot 3 while `Pair<string>` numbered it 2. The design had said
"dispatched statically, never takes a slot" — an intention, never code. Now enforced in
`schema/classes/methods.rs` by skipping any method whose name carries `<`.

#### Design for variance, before any code

Monomorphization changes what variance IS. `Box<Dog>` and `Box<Animal>` are two real classes
with two storage layouts, not one erased class seen at two types, so `+T` cannot be a statement
about a single runtime object — it has to be a statement about two of them.

Three things were established empirically against the current compiler, not assumed.

**Instantiations are invariant today, by construction.** `is_subclass_of` works on class NAMES
(`src/types/checker/type_compat/object_types.rs:49`), and `Box<Dog>` does not extend
`Box<Animal>`, so the checker already refuses:

    Function 'readAnimal' parameter $box expects Box<Animal>, got Box<Dog>

That is the correct default and nothing has to be unlearned to add variance.

**The mechanism variance would ride on already works.** A covariant return override dispatches
to the derived method through a base-typed parameter:

    class DogBox extends AnimalBox { public function get(): Dog { … } }
    function readAnimal(AnimalBox $box): string { return $box->get()->name(); }
    readAnimal(new DogBox(new Dog()));   // prints "dog"

So instance dispatch is virtual and a derived class lays out as a prefix of its base. Both are
prerequisites, and both hold.

**But variance CANNOT be a synthesized inheritance edge.** The obvious implementation — when
`Box<+T>` is declared and both `Box<Dog>` and `Box<Animal>` exist, emit
`class Box<Dog> extends Box<Animal>` — is wrong, and one experiment settles it:

    class Base { public static int $count = 0; }
    class Derived extends Base {}
    Derived::$count = 7;
    echo Base::$count;      // 7 — elephc and php-src agree

A subclass SHARES its parent's static storage unless it redeclares. An inheritance edge between
two instantiations would therefore merge their statics, and
`test_static_property_is_per_instantiation` pins the opposite. Inheritance carries more than
assignability — static storage, `get_parent_class`, `instanceof`, constructor chaining — and
variance wants only the assignability.

##### What it has to be instead

A checker relation consulted by `type_accepts`, plus a STORAGE-COMPATIBILITY proof. The proof is
the part monomorphization forces and erased implementations never need:

`Box<A>` may be handed to a `Box<B>` slot only when every field derived from the type parameter
has IDENTICAL storage under `A` and under `B`. Two object types are both pointers, so
`Box<Dog>` → `Box<Animal>` is free. `int` is register-width and `mixed` is a boxed tagged cell,
so `Box<int>` → `Box<mixed>` is NOT free even though `int` is assignable to `mixed` — it would
need a materializing copy, and a copy is a different object, which is unsound for anything
mutable and a silent cost for anything else.

So the rule falls out of the representation rather than being chosen:

- `+T` is accepted only between two instantiations whose arguments are both object types, one a
  subclass of the other.
- `T` may not appear in an INPUT position — a parameter, or a writable property. That is the
  standard soundness rule, and here it is also what keeps the ancestor-parameter-storage
  constraint (see the override ABI note) from ever arising.
- Every other pairing is rejected at the call, with a diagnostic that says which argument pair
  has incompatible storage rather than "expects Box<Animal>, got Box<Dog>".

##### The slot question, settled

With no inheritance edge there is no shared vtable, so the first worry is how a `Box<Dog>`
reaching a `Box<Animal>`-typed parameter finds `get`. It does NOT fall back to a direct symbol
call: `lower_method_call` (`src/codegen/lower_inst/method_dispatch.rs:13`) takes
`target.dynamic_slot` whenever the method has one, and
`resolve_method_call_target` (`method_resolution.rs:42`) gives one to every method that is not
`final`. So the emitted code is

    _class_vtable_ptrs[class_id(receiver)][slot_of_get_in_Box<Animal>]

— the RECEIVER's vtable, indexed by a slot number computed from the STATIC type. Sound only if
the two instantiations number their slots identically.

They do NOT, and believing they did cost a silent miscompile. Slots are assigned in
`schema/classes/methods.rs:319` — every non-private method takes `vtable_methods.len()` in
source order — and two instantiations spliced from one body do enumerate identically THERE. But
`reachability/reconcile.rs:251` runs afterwards, drops the methods that are not live in each
class, and RECOMPACTS the numbering per class:

    info.vtable_methods.retain(|key| keep_instance.contains(key) || ...);
    info.vtable_slots = compact_slots(&info.vtable_methods);

`Box<Animal>` is named only in a parameter type, so nothing constructs it, so its `__construct`
is not live, so compaction moves `whoAmI` from slot 2 to slot 1 — while `Box<Dog>`, which IS
constructed, keeps it at 2. The widened call takes the number from `Box<Animal>` and indexes
`Box<Dog>`, landing on `get`, whose returned object is then read as a string. The program
printed 17 bytes of the heap and exited 0.

Reordering the template so `whoAmI` is declared first made it work, which is what turned a
guess into a diagnosis: the failure is an index shift, nothing else.

The fix is four lines and sits where the damage was done. An instantiation keeps every method
NAME in its table, so the numbering survives; the body is still pruned, and with no entry in
`method_impl_classes` the emitter already writes a null into that slot. Nothing dispatches
through the hole, because any receiver that reaches it carries the real entry. The file already
had `assert_inherited_vtable_slots_aligned` guarding exactly this property across an inheritance
edge — variance relates two classes with no edge between them, so the assertion could never have
caught it.

So the corrected finding: **variance needs no emitter change, but it does need the pruner to stop
compacting instantiations.** The relation itself is checker-side, as designed. What was wrong was
the claim that everything downstream could be left alone — three of the five end-to-end tests
failed on it, and only because they named an instantiation nothing constructed. Every earlier
hand-written probe happened to construct both, which is why the feature looked finished twice
before it was.

One lesson worth keeping separately: a design established by running experiments is still only as
good as the experiments. Every probe up to that point constructed both instantiations, so every
probe agreed with a claim that was false.

##### Is it worth it

The cost of not having it is ergonomic, not correctness — and smaller than it looks, because the
alternative is not a workaround, it is better. A bounded generic function over the element type
already works today:

    function readAnimal<T : Animal>(Box<T> $box): string { return $box->get()->name(); }
    readAnimal(new Box<Animal>(new Animal()));   // animal
    readAnimal(new Box<Dog>(new Dog()));         // dog

Under monomorphization that is STRICTLY more precise than `+T`. `readAnimal<Dog>` keeps `Dog`
throughout its body and calls `Dog::name` directly; a covariant `Box<Animal>` parameter would
widen the element to `Animal` and dispatch virtually. Covariance exists in erased languages to
recover what erasure took away. Here nothing was taken away, so `+T` buys the calling
convenience and gives up the specialization — which is the whole point of the compiler.

That is the honest comparison to make before spending the emitter change.

### Phase 4 — generic methods

Reported by Vincenzo: *"I can't see generic methods. they seem to rely on generic parameters from
the class."* Exactly right, and it is the limitation the docs already record: `ClassMethod` has no
type parameters of its own, so a method is generic only in its class's. What cannot be written:

```php
class Box<T> {
    public function map<U>(callable $f): Box<U> { return new Box<U>($f($this->value)); }
}
```

Today the parser stops at the method name: `Expected '(' after method name`.

**Landed**, for every shape whose parameters mention the type parameter. The `map` shape still
waits on typed callables — see below, where two experiments settle why.

- [x] `ClassMethod::type_params`, and the field sweep. 240 fixtures, 4 rebuilds by hand.
- [x] Parse `function map<U>(…)` in a class body.
- [x] Skip the template when resolving signatures and when checking bodies — TWO passes, not
      one: `schema/classes/methods.rs` and `method_pass.rs`.
- [x] Instantiate a called generic method into its class, inferring from the arguments.
- [x] Rename the call site to the instantiation it selected, through a new walker hook.
- [x] Dispatch it statically, never through a vtable slot.
- [x] Typed callable parameters (`callable(T): U`). Vincenzo's `map` shape now compiles.
- [x] Written type arguments at a call site. **Done 2026-09-19**, for functions AND methods:
      `identity<int>(41)`, `$box->map<string>($f)`. The parser puts the arguments in the NAME —
      the same spelling `instantiated_name` produces — and the checker decodes them back with
      the language's own type grammar, so one spelling serves both paths. Defaults fill an
      unwritten tail; bounds are checked exactly as for an inferred argument.
- [x] **Invoking a `callable(T): U` inside the body yielded `mixed`, not `U`.** Verified from a
      STRICT position — an argument position would not show it, since `array<string>` accepts
      `array<mixed>` coercively:

          public function mapAll<U>(callable(T): U $f): array<U> { return [$f($this->value)]; }
          // error: declares array<string> but returns array<mixed>; the element storage differs

      The declared signature is used for INFERENCE at the call site and then dropped:
      `resolve_type_expr` maps `CallableSig` to a bare `PhpType::Callable`. So `new Box<U>($f(…))`
      boxes and unboxes on every call — the class is still `Box<string>` and the values are
      right, but the storage the feature exists to pin is paid for twice. And the `array<U>`
      shape above is rejected outright, which is a natural thing to write.

      The channel already exists: `callable_param_sigs[(function_key, param)]` feeds
      `closure_return_types` and `callable_sigs` in `functions/resolution/signature.rs:78`,
      which is exactly what makes `$f(…)` type correctly for a contextually-inferred callable.
      The fix is to build a `FunctionSig` from a DECLARED `CallableSig` and register it there,
      at parameter resolution, for both functions (`resolution/signature.rs`) and methods
      (`method_pass.rs:66`).

      Found by Kimi during the multi-model review — it stopped mid-investigation on "$f(0)
      inferring mixed", and the lead was right. **Fixed**: `declared_callable_signature` builds a
      `FunctionSig` from the declared `CallableSig`, the function path falls back to it where it
      used to ERASE the entry, and the method path — which never wrote to that channel at all —
      now registers it when the parameter is bound.

#### What the multi-model review found

Three models, four real defects, each different, each verified by running it. The harness gave
them `run_php` so they could check a claim before reporting it, and GLM used it.

| Model | Defect | Why it was silent |
| --- | --- | --- |
| GLM | Variance certified against the marked class's OWN members only | `Box<+T> extends Holder<T>` inherits `set(T)`; a widened reference wrote a `Cat` into a `Box<Dog>` and `get(1)` — statically `Dog` — answered `Cat`, exit 0 |
| Kimi | `collect_occurrence` stopped at the FIRST occurrence, not the first VIOLATING one | `T\|Sink<T>` accepted, `Sink<T>\|T` rejected — the same type, order-dependent |
| DeepSeek | No `CallableSig` arm in the polarity walk | the `_ => {}` fallback swallowed the type, so every violation through a callback was admitted |
| DeepSeek | The vtable-slot guard was on the INSTANCE path only | `apply_static_method` still gave an instantiated generic static method a slot |

Two harness lessons: 55 tool steps is not enough for this code, and a model that runs out mid-
investigation answers with its opening narration unless the write-up is explicitly REQUESTED.

#### What typed callables cost, and the two bugs they surfaced

A new `TypeExpr` variant, and the sweep was 17 exhaustive matches — far fewer than the ~100
`GenericClass` caused, because most matches on `TypeExpr` carry a `_` arm. Those `_` arms are
exactly what the compiler cannot report, so the dangerous sites were audited by hand: a
`callable(User): Order` NAMES classes, so `collect_named_classes`, the reachability scan and the
six `*_prelude` detectors all had to recurse or a class named only there would be dead-stripped.

Inference cannot work from types alone. A closure's type is `PhpType::Callable` and carries no
signature, so `infer_bindings` had to gain an expression-aware entry point that reads the
closure's own declared types. Every other parameter is still decided by its type.

Two bugs fell out, both pre-existing in shape:

1. **`walk_class_method` computed the return and variadic types AFTER `leave_method()`**, inside
   the struct literal — the very ordering bug fixed for functions earlier. It could not show
   until a method could be a template: with the guard already lifted, `Box<U>` in a return type
   was instantiated as a class literally called `Box<U>`.
2. **`leave_method` decremented a counter `enter_method` had not incremented.** Every method of a
   generic class passes through those hooks, so the first ordinary method cancelled the enclosing
   CLASS's increment and the pass started instantiating `Box<T>` inside a template. Whether a
   scope incremented is now remembered rather than re-derived.

#### The sweep is smaller than it looks

`ClassMethod {` has 291 construction sites, which is the largest sweep in this whole effort and
the exact shape of the trap recorded for `exception_type_args`. But the distribution is benign:

| Where | Sites | Kind |
| --- | --- | --- |
| `types/checker/builtin_types/**` | 227 | FIXTURE — descriptors for Reflection, DateTime, Exception, Fiber. `Vec::new()` is correct and carries no risk. |
| `types/checker/builtin_*` (iterators, SPL, interfaces, json) | ~25 | FIXTURE, same. |
| `optimize/{control/prune,control/dce,propagate,fold}`, `magic_constants/walker/members.rs`, `name_resolver/declarations.rs`, `func_args/walk.rs`, `resolver/engine.rs` | ~17 | **REBUILD — must carry the field through.** These are the ones that go wrong silently. |
| `parser/stmt/oop/body.rs` | 3 | The real producer. |

Classified by hand before writing anything, the actionable list is FOUR sites:

| Site | Why it needs a hand |
| --- | --- |
| `optimize/fold/expr.rs:67` | Explicit rebuild, spells every field. Drops `type_params` silently. |
| `optimize/propagate/stmt/declarations.rs:66` | Same. |
| `name_resolver/declarations.rs:462` | Carries it via `..method.clone()`, but a bound `<U : Entity>` is a NAME and has to be resolved, exactly as the class-level `type_params` already are. |
| `magic_constants/walker/members.rs:82` | Carries it via `..method`, but a bound and a default are TYPES and must go through `walk_type_params`, or a pass that rewrites types will miss them. |

`optimize/control/prune/statements.rs`, `optimize/control/dce/methods.rs` and
`resolver/engine.rs` already spread the source method and need nothing. `func_args/walk.rs`
destructures, so the compiler reports it.

Order matters: patch those four FIRST, then add the field. Every site the compiler then reports
is a fixture by construction, so a blanket `Vec::new()` is correct for all of them — the sweep
stops being a judgement call.

One trap for any script: `) -> ClassMethod {` is a function SIGNATURE, not a literal. A blind
insert after `ClassMethod {` corrupts `prune/statements.rs:444` and `dce/methods.rs:21`.

#### What the motivating example actually needs, which is not what it looks like

The obvious plan — written type arguments at the call first, inference second, the way generic
classes landed — is wrong, and two experiments against the current compiler settle it.

**There are no written type arguments at any call site, anywhere.** Not for methods, and not for
functions either:

    echo identity<int>(7);      // error: Undefined constant: identity

`identity` parses as a constant and the rest as `< int > (7)`. Generic functions have always been
inference-only at the call, so there is no precedent to copy, and adding one means solving `<` in
expression position — which the project deliberately avoided for `instanceof`, where the rule had
to be restricted to the cases where the following token cannot begin an expression. At a call
there IS a decisive signal (a type argument list is always followed by `(`), so it is solvable;
it is simply not free, and it is not the first step.

**And the `map` shape cannot be inferred, because its parameter type mentions no type
parameter.** `map<U>(callable $f): Box<U>` gives inference nothing: PHP's bare `callable` carries
no types. The form that would carry them does not parse either:

    function applyTyped<T, U>(callable(T): U $f, T $v): U    // error: Expected parameter variable

So Vincenzo's example needs a prerequisite, and it is not the obvious one:

- **(a) typed callable parameters**, `callable(T): U`. A new type form, after which inference
  works with no call-site syntax at all and the method reads like ordinary PHP. This is the one
  that makes `map` work as written.
- **(b) written type arguments at the call**, `$b->map<string>(…)`. Solves the general case
  including parameters that mention nothing, at the cost of the `<` ambiguity.

(a) is the better first prerequisite: it needs no new expression syntax, it composes with
everything already built, and a typed callable is useful on its own — `array_map` with a declared
element type is the same gap.

#### Generic methods are still worth landing before either

Not every generic method takes a callback. A parameter that mentions the type parameter directly
infers exactly the way a generic function does, and that machinery already exists:

```php
class Registry<T> {
    public function pickOr<U>(U $key, T $fallback): U { … }
}
```

So phase 4 is: the field, the declaration syntax, instantiation by inference. `map` waits for a
typed callable, and the plan should say so rather than promising it.

When written type arguments do land, that is a SIXTH place a class name can appear, with the same
alignment care: the type arguments must be empty when nothing was written, so an ordinary method
call keeps the AST it has today. Four `MethodCall` variants exist; only the two with a STATIC
method name can carry them, because a dynamic name has nothing to attach them to.

#### Instantiated methods must not take vtable slots

This is where phase 4 meets phase 3, and it would be a silent miscompile if missed.

A called `map<string>` becomes a real method on the class, named like the instantiated class it
lives in. But `Box<int>` might be called with `map<string>` while `Box<string>` is called with
`map<int>` — so two instantiations of ONE template would carry different method sets. Vtable
slots are numbered from the method list, so the numbering would diverge, and a variance widening
takes the slot from one instantiation and indexes another's table. That is precisely the bug
`reachability/reconcile.rs` was just fixed to prevent, arriving by a different road.

The fix is to not create the problem: an instantiated generic method is dispatched STATICALLY.
The call site knows the exact instantiated name — that is what instantiation MEANS — so there is
nothing to dispatch on, and it never needs a slot. Overriding a generic method polymorphically is
not expressible anyway without variance on the method's own parameters.

The invariant to assert: an instantiated generic method never appears in `vtable_methods`.

#### Where it hooks

`infer_method_call_type` (`src/types/checker/inference/objects/methods.rs:29`) is the analogue of
the function path: infer the bindings from the argument types, check the bounds against the class
table, name the instantiation, record the call site under the same `(enclosing function, span)`
key, and return the substituted return type. Lowering then resolves that site to the instantiated
symbol the way `generic_call_sites` already does for functions.

`parse_type_param_list` is ALREADY imported by `parser/stmt/oop/body.rs`, the file that parses a
method, and it answers with an empty vector when there is no `<`. The declaration half is a
single call placed between the method name and its `(`.

#### Order

Inference first, because it is the only thing that works: the declaration syntax plus the
existing `instantiate_generic_call` path covers every generic method whose parameters mention its
type parameter. Then typed callables, which unlock the `map` shape. Written call-site type
arguments last, if at all — by then most of what they would buy is already inferred.


## Two multi-model review rounds, and what they cost

Kimi K3, GLM 5.3 and DeepSeek v4.1-flash, each with its own instance, real read access to the
tree and `run_php` to execute a claim before making it. Eleven defects reported across two
rounds, **eleven reproduced**, eight fixed. Not one was a hallucination; two were misattributed,
and both models said so themselves.

**Round 1 — the array_map change and the unbox guard.** All three spent their budget on
`array_map` and reached nothing else.

- DeepSeek: the guard I had just added REFUSED `function f(array $a): array { return
  array_pop($a); }`, which php accepts. It was right about the cause: `array_pop` boxes a
  container whose slots are already raw, so unboxing is exact, and the IR is indistinguishable
  from the `array_map` shape that carried boxed slots. My premise — "a Mixed cell at a typed
  contract is always a lie" — was false. The static refusal became a RUNTIME check: unbox, read
  the payload's own value_type tag, fatal only when the slots really are boxed. Proved by
  mutation: with the two-answer `array_map` restored, the program that printed
  `4366994056` now exits 1 with that fatal.
- Kimi: `?array<int>` through `??` printed `0` and four zeros where php prints `1` and
  `[1,2,3]` — a silent miscompile in the `array<T>` surface itself, reduced to "typed element
  AND `??`" (a ternary was correct, a bare `?array` was correct). `coerce_container_to_mixed_payload`
  returned the value untouched for a typed target, which before `array<T>` could not happen.
- GLM: two pre-existing refusals in the same builtin — a `void`/`never` first-class callable
  (fixed: the inline path declines and the runtime path answers `[null, null]`), and an
  associative source with float values (recorded, not fixed: it needs `__rt_hash_map` ABI work).

**Round 2 — the three changes round 1 never reached**, with `array_map` explicitly out of scope
and the step budget raised from 55 to 70. Every finding was about ONE thing: the precision of
`int|float`.

- GLM: a by-reference `int &$n` took a boxed counter and left the caller's local reading NULL.
  Measured further: `mixed` does the same, so the hole is older and wider than the union. Fixed
  in `require_by_ref_argument_storage` with a rule 3 symmetric to its rule 1.
- Kimi: unary negation, and every UNION expectation (`?int`, `int|string`, as parameter, return,
  property and typed local) — the per-member recursion asks whether `int` accepts `float`, so the
  whole-union answer had to come first.
- GLM and DeepSeek together: the packed-field guard, whose own comment describes exactly this
  value.
- DeepSeek: a gapped variable index fills the gap instead of going sparse — pre-existing, no
  counter needed, and it flagged its own uncertainty about attribution. Recorded; the precision
  moves counters onto that broken path, which is a real behaviour change from an older defect.

### The gapped variable index: hash storage, with one exception

Settled. The write now chooses storage instead of zero-filling:

```php
$rows = [];
foreach ([101, 102, 205] as $id) { $rows[$id] = "row$id"; }
echo count($rows);   // was 206, now 3, which is php's answer
```

The rule, in `check_array_assign`: an INTEGER key into a still-EMPTY array takes hash storage
unless this pass can bound it against the array's length. A literal is decided exactly as before
(`static_array_key_forces_hash_storage`); anything else is unbounded and goes to the hash.

The exception matters more than the rule. A blanket "non-literal index means hash" was measured
first, and it moves every `for ($i = 0; …; $i++) { $a[$i] = … }` onto hash storage — where 25 of
the 72 array builtins refuse the argument outright, `array_filter`, `array_reduce`, `usort`,
`array_push` and the internal-pointer family among them, and where `array_filter`'s lowering has
no hash path at all to give them. That trades one wrong ANSWER for a large loss of compilable
programs, so it was not shipped. `packed_counter.rs` now proves the one index that cannot gap:
the counter of the `for` loop the write runs DIRECTLY in, initialized to `0`, advanced by `$i++`,
with no `continue` past the write, no second assignment to the counter, and no rebinding of the
array itself inside the body. Both directions of an imperfect proof are safe — accepting too much
keeps the storage elephc already chose, rejecting too much costs a hash where php would have used
one anyway.

Residual, deliberately: a `while`-loop counter is not proven and takes hash storage, and so does a
write under an `if`. A non-empty array's gapped write is unchanged, because the rule only fires
while the element type is still `Never`.

One related fix fell out: `normalized_array_key_type` now folds `int|float` to `Int`. A `$i++`
counter used as a key reached the key merge as a foreign type and widened the whole array's key to
`Mixed`, which no `array_keys` lowering accepts.

### Found while verifying it, NOT caused by it, and fixed

Four string writes into an empty array used to destroy slot 0's value, on committed `main`:

```php
$a = [];
$a[0] = "aa"; $a[1] = "bb"; $a[2] = "cc"; $a[3] = "dd";
echo $a[0];       // php: aa     elephc: two garbage bytes
```

Confirmed against a compiler built from `git archive HEAD` before touching anything, so it
predated every working-tree change.

**It was a buffer overrun, not a use-after-free.** The emitted assembly names the cause in two
instructions: `$a = []` calls `__rt_array_new` with `capacity = 4, elem_size = 8`, because an
`array<never>` has no element type yet, so 32 data bytes are reserved. The first string write
stamps `elem_size = 16` into the header and LEFT `capacity` at 4 — the array then claims 64 data
bytes it never owned, and the grow check, which fires at `index >= capacity`, did not fire until
index 4. Index 3 therefore wrote 16 bytes past the block.

The arithmetic matches the symptom exactly. The array's block rounds to 64 bytes, so the string
allocated right after it carries its payload at `A+72`; index 3 writes `A+72..87`. With a
20-character value that is the first SIXTEEN bytes destroyed and the tail intact, which is what
was observed — and why `strlen` still answered correctly while the bytes were junk.

**The fix already existed one file away.** `__rt_array_push_str` restates the capacity in the new
unit when it widens (`old_capacity * old_elem_size / 16`), and its comment names this very
failure: *"sized for 8-byte slots before the capacity check ever triggers a grow"*. The setter
never got those three instructions. Added now to `__rt_array_set_str` on both targets, which is
why `$a[] = …` was always correct and `$a[0] = …` was not. `4 * 8 / 16 = 2`, so index 2 now grows
properly instead of running off the end. No other setter widens — `int`, `refcounted` and
`push_int` all stamp 8, the same width `__rt_array_new` allocated with.

`--heap-debug` reports `allocs=6 frees=4` and exits 0 on the repro: an overrun inside a live block
is invisible to a free-list validator, so it will not find the next one either. No test covered it
because the suite builds arrays from literals, not from successive index writes into `[]`.

### Two more pre-existing hash defects, found by surveying the storage the switch now picks

Both reproduce on committed `main` with no new rule in sight, and both were found by asking what
an int-keyed hash actually supports rather than by reading code. Of the 24 builtins surveyed, 16
refuse an `AssocArray` at compile time (a clear message, a safe failure); these two answered
WRONGLY instead, which is the reason to go looking.

**A hash sort freed its receiver mid-sort.** A local that starts as `[]` and becomes a hash INSIDE
a loop has no single array storage shape, so its frame slot widens to `Mixed`. Loading it unboxes
it with its own reference; storing it back boxed it and RELEASED that reference, and the pending
EIR `release` dropped the same one again. The block went back to the allocator while
`__rt_hash_ksort` was still relinking it:

```php
$a = [];
foreach ([1] as $v) { $a["q"] = "y"; }
ksort($a);
echo json_encode($a);   // php: {"q":"y"}   elephc: {"\u0000":"`"}
```

One entry was enough, so it was never a mis-ordering. The entry COUNT survived and every key and
value read back as garbage, and only an operation that WRITES exposed it — allocation pressure
alone did not, which is what ruled out a recycled block and pointed at the sorter itself.

The fix is a condition that was already written down one function away.
`value_can_transfer_ownership_to_consumer` refuses to hand a consumer the reference an explicit
`release` will consume, and says so in its comment; `value_can_own_mixed_box_source` has a SECOND
path — a value loaded out of a `Mixed` slot as a concrete container — that never asked. Both paths
share the test now.

**`current()` boxed a payload that was already boxed.** A hash entry carries its own `value_tag`,
and `__rt_array_ptr_value` handed it straight to `__rt_mixed_from_value`. Tag 7 means the payload
IS a Mixed cell, and that helper's tag-7 arm retains it and allocates ANOTHER cell around it. No
display path unwraps two levels:

```php
$n = 2;
$h = ["b" => 2 * $n, "a" => 1 * $n];
echo current($h), "|", key($h);   // php: 4|b   elephc: |b
```

Three asymmetries pinned it: `key()` was right because keys are never boxed; `json_encode`,
`foreach` and `implode` were right because they read through the array's STATIC element type;
and an INDEXED array of boxed cells was right because it delegates to `__rt_array_get_mixed_key`,
which understands every `value_type`. Only the hash branch boxed blind. A tag-7 payload is now
returned as-is with a retain — which allocates FEWER blocks than the raw-int path, since the
wrapper is gone.

### The hash builtin surface: `array_filter` first

Measuring the refusal before building anything moved the priority twice.

**It was never about the new storage rule.** Eleven builtins refuse an associative array that was
written as a STRING-KEYED LITERAL — the oldest shape in php, owing nothing to the gapped-index
switch: `array_filter`, `array_reduce`, `array_walk`, `usort`, `uasort`, `array_push`,
`array_find`, `array_any`, `array_all`, `array_udiff`. Only `array_map` passes. The eval
interpreter supports all of them, key preservation included, so the COMPILED path is what is
behind.

**And `array_filter` is wrong on the indexed path too.** php never renumbers a filtered array:

```php
echo json_encode(array_filter([1, 0, 2], fn($x) => $x > 0));
// php: {"0":1,"2":2}     elephc: [1,2]
```

That reproduces on committed `main`, and `implode`/`count` fixtures hide it because both ignore
keys. It is NOT fixed here, and the order is the reason: making the indexed result key-preserving
turns it into a hash, and a hash result feeding any of the eleven refusers above would trade
today's wrong answer for tomorrow's compile error — the same bargain the gapped-index switch
already refused. The hash surface has to come first.

**Delivered: `array_filter` over an associative source.** Checker (accept `AssocArray`, return one
with the SAME key type), lowering (route to the new helper, reusing every callback arm untouched),
and `__rt_hash_filter` on both targets — `__rt_hash_map`'s walk and callback ABI, plus
`__rt_hash_clone_shallow`'s per-tag retain, which is the one thing a filter needs and a map does
not: a map inserts the callback's result, which the wrapper already transferred, while a filter
inserts the SOURCE's value, which the source still owns.

Two refusals rather than two guesses. `USE_KEY`/`USE_BOTH` pass a KEY whose register shape is
independent of the value's, so their ABI is the product of both — a different helper, not a flag.
And `Mixed` values are gated to `hash_map_source_value_type`'s `Int|Bool|Str`: accepting them made
`fn($x) => $x > 2` work and `fn($x) => count($x)` fail as a runtime TypeError, and a clear refusal
beats a silent failure. Both limits are shared with `array_map` and lift together.

### Two leaks, told apart by measuring instead of assuming

`ArrayFilter` was missing from the `Fresh` result-ownership bucket that `ArrayMap` has always been
in, even though both allocate their destination before writing an entry. The hypothesis was that
this explained the three blocks `array_filter` leaks per call. It did not — the number did not
move. What it DID fix is the leak the file's own `ArrayFlip` comment describes: a TEMPORARY source.
`array_filter(make(), $cb)` went from 1200 live blocks to 900 across 300 calls, one recovered per
call. The classification is kept for that measured reason, not the announced one.

The residual is NOT an `array_filter` defect: roughly three blocks per `array_filter` or
`array_map` call and eight per `usort`, identical on committed `main`, and identical whether the
callback is an arrow function, a hoisted closure or a named function — so it is not the descriptor
either. `count(array_keys(…))` is clean, so it is not "any owned temporary" but the callback
builtin family. Open, and its own investigation.

No regression test asserts heap cleanliness for the filter: the residual leak makes any such
number unstable, and pinning one would break at the next fix.

### The leak under the callback builtins was php semantics, in the closure call path

Chasing `array_filter`'s per-call leak led somewhere else entirely. A temporary handed to a
CLOSURE was never released, so its destructor never ran:

```php
class D { function __destruct() { echo "d"; } }
$f = fn($a) => 7;
$f(new D());
echo "|end";        // php: d|end     elephc: |end
```

Named functions, methods and static methods were always correct, and so was the same closure when
it RETURNED its parameter — returning hands the value out as the result for the caller to release,
which is exactly what masked this. With an int argument it showed only as a leak: one boxed cell
per untyped parameter, which is where `array_filter`/`array_map`'s three blocks per call and
`usort`'s eight came from. It was never an array-builtin defect.

**Two call paths, so two fixes.** `StaticCallableBinding::Closure` never called
`release_owned_call_arg_temporaries`, which every other call lowering does — releasing only the
ARGUMENT operands, since the capture operands appended after belong to the closure. And the generic
descriptor invoker, used by a call in ANY loop and by every builtin callback, increfs each by-value
argument for the callee and released nothing.

**Three directions for the second, and the measurements chose.** Removing the retain takes every
leak to zero and fixes the looped destructors — then `fn($a) => $a` dies with
`heap debug detected bad refcount`, because that reference is what the returned result becomes.
Retaining the result instead balances the returned case and leaks the result in every other one.
What balances both is releasing each retained argument UNLESS the result is that same pointer.

The comparison needs the retained pointers live across the call, and re-reading them from the
argument array does not work: a coercion may have unboxed the element, so the slot no longer holds
what was retained. They are spilled into frame slots at marshal time. Growing the frame is safe
because the prologue puts `x29` at `sp_on_entry - 16` whatever the size, so no existing slot
moves — the exception boundary's included.

The regression test for the returned-parameter case stays GREEN under the mutation that breaks the
other two, which is the whole point of it.

**Three gates were deliberately NOT relaxed.** `$obj->$i`, `$p->arr[$i]` / `C::$arr[$i]` and
`exit($i)` all pass the checker once relaxed and then hit `… PHP type Mixed` in the backend,
which has no arm for a boxed value there — for `mixed` either. Relaxing them trades a clear
message for an obscure one, so each gate keeps its refusal with the reason written in.

**What the rounds are worth.** Six of the eight fixes are shapes no test in this repository
covered, and two were silent wrong output. The cost is in verification, not in reading: every
claim had to be re-run here, two needed a mutation or a rebuild to attribute, and the two
misattributions would have sent a trusting reader into the wrong file.

## Goal

Let elephc compile generic types instead of erasing them, so a container shared
between several element types keeps register-width storage instead of collapsing
to boxed `mixed`.

Measured on `main` (`macos-aarch64`, dev build), a one-field container over
3,000,000 iterations:

| Version | Allocations | Time |
| --- | --- | --- |
| `private int $value` | 6,000,001 | 0.10 s |
| `private mixed $value` | 12,000,004 | 0.20 s |

Erasure costs one heap allocation per generic value crossing a boundary. That is
the cost this plan removes.

### Why monomorphization

php-src cannot monomorphize because it compiles file by file and classes arrive
lazily through the autoloader, so the set of instantiations is not knowable. The
PHP RFC "Bound-Erased Generic Types" (v0.22) was declined in June 2026 for the
opposite reason: erasure enforces nothing.

Elephc is a whole-program AOT compiler. `resolve()` flattens every
`include`/`require` into one `Program` before type checking, so the set of
instantiations IS knowable by construction. The option php-src had to reject is
the one available here, and it is also the faster one.

The mechanism is already present, at arity 1. `src/types/array_storage.rs`:

> The checker's parameter specialization compiles a callee for the element type
> it sees at the call site.

A second, disagreeing call site widens the parameter to `Mixed` instead of
emitting a second body. Generics are that mechanism with a key.

## User-visible contract

### Two surfaces

The criterion is `--strict-php`, not the file extension. The flag is opt-in and
enforced per physical file (`SourceMode::strict_php_is_effective`), so a `.php`
file is extension-enabled by default.

| Must pass the strict audit | Surface |
| --- | --- |
| No (default) | Native `array<int>`, `class Box<T : Animal>`, in `.php` |
| Yes | PHPStan docblocks (`@template`, `@param array<T>`) |
| Not applicable | `.lfc`: tagless, always the full surface |

Both surfaces lower to the same model. Each follows its own world's convention:
`:` for bounds in native syntax (the RFC's form), `of` in docblocks (PHPStan's).
Forcing either into the other only costs compatibility.

Generics join the existing family of elephc extensions — `buffer<T>`, `extern`,
`packed class`, `ifdef` — and carry the same documentation banner.

### Syntax

| Element | Native | Docblock |
| --- | --- | --- |
| Type argument | `array<int>`, `array<string, Foo>` | `@param array<int>` |
| Declaration | `class Box<T>` | `@template T` |
| Bound | `class Box<T : Entity>` | `@template T of Entity` |
| Default | `<K = string>` | `@template K = string` |
| Call site | turbofish `::<>`, optional | not applicable |

The turbofish is a parsing necessity, not decoration: in expression position
`foo<int>($x)` is indistinguishable from `foo < int > ($x)`. Declaration
position is unambiguous. Because elephc monomorphizes from argument types, the
turbofish stays optional in nearly every case.

### Graceful degradation

A statically known type argument is specialized. Anything else falls back to the
erased bound, which is exactly today's behavior. Bare `Box` means `Box<bound>`
and lowers to the erased body. `eval()`, `new $className`, string callables and
reflection therefore never become a hard wall: generics are an optimization and
safety layer, never a new compilation constraint.

## Design decisions

1. **Cohabit in phases 0–1, replace in phase 2.** Replacing the existing
   specialization forces the `Span` identity work described below. That work is
   already recorded as the real fix in the codebase, so phase 2 is the forcing
   function for it rather than an extra cost.
2. **Bare `Box` is accepted**, meaning `Box<bound>`.
3. **A contradicted annotation is a hard error**, not a warning. Once elephc
   compiles the annotation it stops being a comment.
4. **Naive monomorphization first.** No ABI-class body sharing in v1, but the
   instantiation key is shaped from the start to become an ABI key: a tuple of
   types today, a tuple of representation classes later, same insertion point.
5. **Variance deferred to phase 3**, as `+T` / `-T`, behind a parser slot that
   can accept `in` / `out` if a future RFC settles it differently.

## Phase 0 — detail

### It adds no AST node

`TypeExpr::Array(Box<TypeExpr>)` already exists and is already constructed
internally by the preludes and builtin schemas. Phase 0 only makes it reachable
from user syntax, so the full "Adding or changing an AST node" checklist does
not apply.

What does apply: 23 files match on `TypeExpr::Array` and every one of them was
written assuming the value is compiler-internal. Each must be audited for
user-originated values — notably `name_resolver/names.rs`,
`type_compat/declarations.rs`, `optimize/reachability/usage.rs`,
`ir_lower/context.rs`, `autoload/walk.rs`, and the seven `*_prelude/detect.rs`.

### The third state

This is the real work of phase 0, and the riskiest part of the whole plan.

Two behaviors exist today, both verified on `main`:

```php
int $x = 1; $x = "s";      // error: cannot reassign $x from int to string
$a = [1,2,3]; $a[0] = "s"; // v4: php=array<mixed> = array_to_mixed v0  (silent)
```

A declared scalar is already a hard contract. An inferred container converts
silently. A declared container is a third state, and decision 3 makes it an
error.

**Resolved, and more cheaply than planned.** `array_storage_conversion` needs no
new parameter and lowering needs no new `CheckResult` map.

The widening happens one level up, in `check_array_assign`
(`stmt_check/assignments/arrays.rs`), which rebinds the local to
`Array(merged_ty)`. Rejecting there means the checker never produces a widened
declared array at all, so lowering cannot disagree with it: for every accepted
program the behavior is byte-for-byte what it was. The shared predicate and its
four call sites are untouched.

The guard must key on the MERGE RESULT, not on `elem_ty != val_ty`. That
inequality is also true when the merge changes nothing: a bare `array`
declaration resolves to `Array(Mixed)`, and `Mixed != Str` holds while
`merge(Mixed, Str)` is still `Mixed`. Keying on the inequality made
`array $a = [1,2,3]; $a[0] = "s";` a spurious error — caught by the neighbouring
cases, and the reason the check compares `merged_ty` to the current element type
instead.

Declaredness itself comes from the existing `Checker::typed_local_names`, which
already gives `int $x = …` its contract. No new state.

### Strict mode

`audit_type` in `src/strict_php/audit.rs:715` is the insertion point; it already
rejects `buffer<T>` a few lines away. Message shape follows the family:

> `array<T>` type arguments are an elephc extension and are not valid PHP

## Phases 1–3 — sketch

### Phase 1

Generalize arity-1 specialization to arity N. `functions: HashMap<String,
FunctionSig>` and `param_specialization_seen: HashSet<(String, usize)>` are both
keyed by name alone and must gain the instantiation. Synthetic instantiation
names mangle for free: `mangle_fqn` is total and injective, so `firstOf<int>`
needs no new symbol scheme.

### Phase 2 — Span identity

Monomorphizing classes inverts an invariant stated in
`binding_decision_ambiguity.rs`:

> The scan counts MATCHING NODES, not decisions. One decision matching two nodes
> is the hazard; one node visited by several checker walks is not (the walks
> re-decide one AST node).

Today several walks over one AST node re-decide it and the last wins, because
there is one compiled body. With N instantiations the walks decide differently
and all N decisions must survive.

Affected surface:

| Assumes one signature per name | Size |
| --- | --- |
| `CheckResult` decision maps | `local_bind_kill_sites`, `local_retype_sites`, `mixed_storage_store_sites` |
| Lowering consumption | 22 references in `src/ir_lower/function.rs` |
| Ambiguity arbitration | 567 lines (`binding_decision_ambiguity.rs`) |
| Per-walk re-decision | 1,287 lines (`mixed_storage_scan.rs`) |
| Superseded-warning retraction | `binding_decision_warnings` |
| Other specialization points | 13 `specialize_*` functions |

The root cause is already named in that file:

> File identity in `Span` is the real fix and is out of scope here.

A `Span` carries line and column and nothing about the file, and include
resolution splices without rebasing line numbers. Decisions need a discriminant,
and it is the same shape of fix for the file and for the instantiation.

### Phase 3

Variance markers only, once phases 0–2 are settled. Invariant until then. The design note under
"Phase 3 — variance" records what monomorphization does to the feature: it cannot be an
inheritance edge, it needs a storage-compatibility proof erased implementations never need, and
a bounded generic function already covers the read-only case more precisely than `+T` would.

## Testing

> **Fixed.** A full `cargo test --test codegen_tests` used to abort:
> `test_suspended_fiber_recursion_budget_does_not_leak_to_main` overflowed its stack on a
> 128-term addition and SIGABRTed the whole harness process, so no summary was printed and
> every other result was lost.
>
> It was the THREAD stack, not a compiler limit: Rust gives a spawned thread 2 MiB against
> 8 MiB for a process's main thread, and `cargo test` runs every test on a spawned thread.
> Generating the same shape into a file and running `elephc --check` on it succeeds, which is
> what separates the two. `.cargo/config.toml` now sets `RUST_MIN_STACK = "16777216"`, so a
> plain `cargo test` works. CI runs nextest, which gives each test its own process and
> therefore the main thread's stack, so this only ever mattered locally.

Per the test policy, a new language construct needs lexer, parser, codegen, AND
error tests, plus an example.

- lexer: `array<int>`, `array<string, Foo>`, the `<>` (`LessGreater`) trap, and
  in phase 2 the `>>` split
- parser: every type position — params, returns, properties, class constants,
  typed locals, closure and arrow signatures
- codegen: the element type survives to EIR (`php=array<int>`, not
  `array<mixed>`) with two disagreeing call sites present
- error: a contradicted declared element type, and `--strict-php` rejection
- example: `examples/generics/main.php` with its own `.gitignore`
  (`*.s`, `*.o`, `main`)

Run focused filters during implementation and leave the full supported-target
matrix to CI, per the test policy.

## Documentation

- `docs/beyond-php/generics.md`, alongside `buffers.md`, `extern.md`,
  `packed-classes.md` and `ifdef.md`, carrying the same strict-mode banner
- a row in the `docs/php/types.md` type table
- a line in `docs/README.md`'s beyond-php index
