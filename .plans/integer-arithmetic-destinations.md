# Integer arithmetic destinations

- [x] Identify the implicit integer conversions that hide arithmetic sinks from EIR.
- [x] Expose conversions at calls and declared integer local writes before owner publication.
- [x] Validate nested expressions, overflow, evaluation order and all supported target emitters.
- [x] Document the allocation behavior and finish task 2b890a89.

Keep checked add/sub/mul trees intact until the integer destination converts their final
result. Concrete integer leaves and arithmetic-only internal nodes prove that the
conversion cannot invoke user code. Unknown Mixed values retain the existing handling.

Existing checked-int sinking and numeric-chain fusion remove eligible intermediate
cells. Scalar add/mul operands around right-hand chains can commute into those chains.
The backend's numeric frame cells handle other right-associated and balanced trees
without heap allocation and preserve each subtree's exact integer-to-float promotion.
Explicit integer local declarations keep this destination contract across subsequent
statement and expression assignments. Inferred PHP locals keep their dynamic tags.

Validation passed: eight new codegen tests, five new EIR unit tests, six existing
numeric-chain tests and three existing overflow regressions. Runtime fixtures cover
both EIR modes with heap validation; PHP-compatible cases match PHP. Assembly emission
passes for all five supported targets. The new scalar-local regression fails on the
base compiler at `777b1024e2`.

Redeclaring a local as `mixed` or `int|float` replaces its prior integer contract.
The redeclaration regressions fail before that contract is cleared and pass in both
EIR modes after the correction, preserving subsequent overflow-to-float promotion.

A single Kimi review found that declaration facts could escape an untaken branch
or a zero-trip loop. Names with declarations inside conditional control flow now
keep the existing dynamic reassignment path throughout their body. The explicit
initializer still receives its declared conversion, and linear declarations retain
scalar reassignments. The regression failed before the correction for an untaken
branch, a sibling arm and a zero-trip loop. Coverage also includes for-loop exits,
switch cases, catch handlers and assignment expressions in both EIR modes.

A local one-million-iteration `intdiv(($i * $stride) + 7, 5)` loop produced the same
checksum with the base and candidate compilers. Across three executions per binary,
median wall time fell from 33.64 ms to 4.01 ms and allocations from 1,000,000 to zero.
This is a focused compiler microbenchmark, not an Ivory application measurement.

Compile this fixture with `--gc-stats` and execute each resulting binary three times:

```php
<?php
function arithmeticBenchmark(int $n, int $stride): int {
    $hash = 0;
    for ($i = 0; $i < $n; $i++) {
        $hash = ($hash + intdiv(($i * $stride) + 7, 5)) & 0x3fffffff;
    }
    return $hash;
}
echo arithmeticBenchmark((int) ($argc * 1000000), 3);
```

Both variants printed `426731104`.
