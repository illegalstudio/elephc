---
id: gotcha-a-reference-in-an-array-element-reads-right-by-e-d6fa131e
type: gotcha
title: "A reference in an array element reads right by element and NULL by traversal"
description: "assign a reference into an array element and the direct read, count, isset, foreach and array_values are all correct, while implode, json_encode, array_sum, in_array and print_r see NULL. Accessors going through the element-read path resolve the reference; those walking the raw storage do not. An element-read probe says the feature works and would report the opposite of the truth"
created: 2026-09-21
verified_by: "Three probes beside php 8.5.10 on a frozen compiler: element reads, whole-array dumps, and ten different accessors over one reference element"
sources:
  - path: src/codegen/lower_inst/arrays.rs
    blob: 142cab4db1b7333168bb53d37e7ebb575fc0324b
---

# A reference in an array element reads right by element and NULL by traversal

## Fact

An array element holding a REFERENCE is read correctly by some accessors and as NULL by others,
in the same array, in the same program. `$arr[0] = &$a;` is accepted, the alias even works when
the element is read directly — and `json_encode` then emits `null` for it.

MEASURED against a frozen compiler, php 8.5.10 beside it
(`scratchpad/arefcheck/{p,q,r}.php`, `run.sh` / `runq.sh` / `runr.sh`):

    $a = 1; $arr = []; $arr[0] = &$a; $arr[1] = 7; $a = 99;

    direct read $arr[0]        php int(99)      elephc int(99)     ok
    count                      php 2            elephc 2           ok
    isset($arr[0])             php true         elephc true        ok
    foreach by value           php 0=>99 1=>7   elephc 0=>99 1=>7  ok
    array_values($arr)[0]      php int(99)      elephc int(99)     ok
    implode(',', $arr)         php "99,7"       elephc ",7"        WRONG
    json_encode($arr)          php [99,7]       elephc [null,7]    WRONG
    array_sum($arr)            php 106          elephc 7           WRONG
    in_array(99, $arr, true)   php true         elephc false       WRONG
    print_r / var_dump         php 99           elephc empty/NULL  WRONG

THE SPLIT IS THE DIAGNOSIS: accessors that go through the element-read path resolve the
reference; accessors that WALK THE RAW STORAGE see the unresolved cell as NULL. The element and
the array disagree about one slot.

BEWARE THE PROBE THAT CLEARS IT. Reading `$arr[0]` and asserting it equals 99 passes. So does
`foreach`, `count` and `isset`. A probe built from those says the feature works. The whole-array
spellings are the ones that fail, and only some of them — `var_dump($arr)` fails where
`var_dump($arr[0])` succeeds. I wrote the element-read probe first and it would have let me
report the opposite of the truth.

APPEND BEHAVES THE SAME: `$brr[] = &$b;` then a whole-array dump shows NULL.

THE OTHER DIRECTION IS DIFFERENT AND MILDER: `$x = &$src[0]; $x = 99;` gives the right VALUE
everywhere, but a whole-array dump prints `int(99)` where php prints `&int(99)` — the alias
marker is lost, the value is not.

THE COMPILE-TIME REFUSALS ARE A SEPARATE, SMALLER PROBLEM than the silence. Three spellings are
rejected outright — `[&$a, $b, &$c]` in an array literal ("Reference elements in array literals
currently require a web superglobal variable"), `$value[$k] = &$refs[$rid]` ("Complex reference
targets currently require a variable or declared-property source"), and a `goto` to a
non-terminal-tail label. Relaxing the literal check alone was tried and makes `[&$a]` compile and
print NULL, so the refusals are hiding the representation gap rather than causing it. **Do not
lift a refusal without fixing the representation first** — a loud refusal is better than a silent
`null` in a JSON payload.

WHY IT IS ON THE CRITICAL PATH: `vendor/symfony/polyfill-deepclone/DeepClone.php:631` is
`$refsPool[] = [&$refs[$k], $value, &$value];`, and that file enters the closed world as soon as
conditionally-declared functions in included files are bound. The binding fix is therefore gated
OFF behind `ELEPHC_BIND_CONDITIONAL_INCLUDE_DECLARATIONS=1` until this is done.

## Why

json_encode emitting [null,7] and array_sum returning 7 instead of 106 is silently wrong business logic, not a cosmetic dump issue. And the compile-time refusals on the other spellings are hiding this rather than causing it -- relaxing the array-literal check alone makes the form compile and print NULL, so a loud refusal is better than a silent null until the representation exists.
