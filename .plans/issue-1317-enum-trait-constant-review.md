# PR #1827 / issue #1317: enum trait constant review

- [x] Reproduce imported constant scope and case-name collisions against PHP.
- [x] Bind constants to each consuming enum and validate collisions.
- [x] Add focused codegen/diagnostic regressions and target checks.
- [x] Rebase on current main and replay the focused regressions.

Reviewed base: `92c86b3b67de924561afc85f3b85c14c254f14f1`.

## Reproductions

PHP 8.5.10 gives `2:E` for imported B = self::A + 1 and N = self::class. The reviewed compiler rejected global/function reads; from a caller C with its own A = 99 it printed `100:C`. Direct, nested, grouped/backed and own constant/case collisions also compiled despite PHP's fatal diagnostics.

## Implementation

Enum metadata uses the existing recursive class-constant expression rewriter with the consuming enum as lexical class and no parent. Values remain expressions, with self receivers already concrete before inference or lowering from another scope. The cached trait expression is not mutated, so two consuming enums bind independently.

Trait expansion validates case names before merging the enum metadata. Own constant collisions use PHP's class-constant redefinition message. Imported collisions inspect each directly used trait's expanded cache entry, so nested composition names the consuming trait (MidT rather than BaseT). Member names remain case-sensitive.

## Tests and boundaries

Four heap-debug codegen regressions cover global/function/other-class reads, independent namespaced/nested consumers (pure and backed enums), case-sensitive names, and own enum constants. A separate functional control compares direct and Reflection reads from another class. Four diagnostic regressions cover direct, nested, grouped/backed and own collisions.

The Reflection path retains memory on the exact reviewed baseline too. The baseline enum Reflection fixture emits `100:C|2:E` with 1258 live blocks / 288648 bytes; the new lexical-scope probe returns the PHP values with the same retention. An ordinary class computed-constant Reflection control also retains memory. This is independent of binding enum receivers; no heap-clean claim is made for the Reflection control.

The shared target was forced fresh for this checkout, and the default Magician archive was rebuilt before repeating Reflection checks. All source/test changes are scoped to this review; the overlapping enum-declaration-metadata branch was observed, and its types/enums test file is only exercised, not edited here.

## Validation

- Warning-free `cargo build`; 14 trait codegen tests, 55 enum codegen controls, and 71 focused enum/exception diagnostics pass.
- Four PHP-oracle fixtures with EIR on/off execute on macOS ARM64 and Linux ARM64/x86_64: 24 matching outputs, all heap-clean.
- Three fixtures with EIR on/off for iOS device/Simulator: 24 assembled program/runtime objects with runtime-symbol closure.
- The exact baseline enum Reflection control was replayed with the final compiler: output changes from `100:C|2:E` to `2:E|2:E`; the retained counts remain 1258 blocks / 288648 bytes.

Artifacts and replay scripts: `/private/tmp/elephc-1827-review`. No full local suite or iOS execution is claimed.

## Trail check

Four conflicts match explanatory comments/tests against this change's own learning: they describe the old unbound receivers and the old order-only collision handling. The implementation stores rewritten expressions and explicitly rejects the collisions; PHP regressions verify both. These are keyword/context false positives, with the disposition recorded in the commit and PR response. Read-only overlap notices also identify the enum-declaration-metadata and late-static-property-default branches; neither branch/worktree is modified here.

## CI maintenance, 2026-10-15

Rebased on main 132b4da6f7. Both original patches are unchanged under range-diff.
The updated base includes rustls 0.23.46 for the old advisory-audit failure.
The current warning-free dev build passes 14 trait codegen tests, four enum
collision diagnostics and the two Core tests that previously timed out in the
Linux x86_64 shard. The standalone trait harness also passes 17 support checks.

The first native Core hook replay took 77.22 seconds including automatic bridge
builds; with the bridges built, the same case passes in 24.90 seconds.
The inventory case passes in 27.30 seconds. These are macOS focused replays,
not proof that the Linux CI timing gate has passed. No timeout budget was changed.
A fresh full CI run on the pushed SHA remains the final validation.
Evidence: /private/tmp/elephc-pr-maintenance-20261015/1827-*.
