# PR #1559 / issue #1482: PHP constant identity

- [x] Cross-check registration and fetch identity against PHP/php-src.
- [x] Share canonical identities across AOT, eval and public inventories.
- [x] Add and replay the focused constant regression tests.
- [x] Recheck generated builtin documentation after rebasing.
- [x] Reproduce and fix constant-definition guards leaking across pool requests.
- [x] Replay final constant and web regressions; publish on the existing PR.
- [x] Grow shared constant storage to its widest declaration and replay the cross-scope regression.

Reviewed base: `ee8c40cc76d30697c32bc286c485a701613f9078`.

## Oracle and review correction

PHP 8.5.10 was executed locally. Its sources were checked at:

- https://github.com/php/php-src/blob/php-8.5.10/Zend/zend_constants.c#L477-L523
- https://github.com/php/php-src/blob/php-8.5.10/Zend/zend_constants.c#L389-L445
- https://github.com/php/php-src/blob/php-8.5.10/Zend/zend_builtin_functions.c#L521-L568

Registration lowercases only namespace bytes and preserves every literal leading separator. Fetches remove exactly one leading separator before the same namespace folding. Thus `const FOO = 4; define('\FOO', 5)` returns true and creates a separate key. The review's suggested registration-prefix stripping would diverge from PHP. The genuine issues are namespace alias guards and storage collisions caused by stripping definition separators.

Baseline: `const App\X = 1` followed by `define('app\X', 2)` returned true without a warning. A fixture defining FOO, \FOO and \\FOO read `4:4:4:4` instead of PHP's `4:4:5:6` through constant().

## Change

Shared definition/fetch key helpers live in the neutral constant contract. AOT guards, warning spelling, and storage use definition identity. Constant storage adds one compiler anchor; the ordinary global-symbol helper removes just that anchor. Namespace casing of ordinary globals is not folded. Literal constant() fetches apply one lookup normalization.

Magician separates definition checks from fetch checks, including its predefined fallback. Its registries keep canonical keys and a separate original-name map for get_defined_constants(). Root-separator definitions can coexist with both user and predefined unprefixed names.

## Validation

- Warning-free default compiler/Magician build after resetting the docs exporter's curl feature.
- 79 Magician constant-related unit tests; 84 compiler constant-related unit tests; one shared-key contract test.
- 12 constant codegen tests, 140 adjacent callable/system controls, nine constant/inventory controls, and two focused EIR smoke tests.
- Eight PHP-oracle fixtures with EIR on/off on macOS ARM64 and Linux ARM64/x86_64: 48 output/full-warning matches, all heap-clean.
- Both iOS device/Simulator targets, three fixtures and EIR on/off: 24 assembled program/runtime objects with runtime-symbol closure.
- Full builtin docs generation, module/comparison generation and all three docs/boundary audits pass; generated files are unchanged.
- Module preambles, assembly comments and git diff whitespace checks pass.

Replay scripts and results are under `/private/tmp/elephc-1559-review`.

## Separate controls and check disposition

The native eval identity test asserts output and diagnostics. A separate ordinary unprefixed `eval('define("N", 5); echo N;')` control retains one 40-byte Mixed cell under the existing dynamic-constant ownership path; no heap-clean claim is made for that control. A trial owner-aware argument adapter did not affect that retained cell and was removed. Constant-reference expressions inside inventories remain an independent unsupported expression path.

`trail check --base ee8c40cc76d30697c32bc286c485a701613f9078` reports six keyword conflicts against this change's own learning. Each was inspected: definition keys preserve literal separators; fetch keys remove one; the storage helper adds an internal anchor that the symbol helper consumes once. The flagged tests/docs explicitly demonstrate those rules. These are negation/context false positives, justified by the PHP 8.5.10 oracle and regression results, rather than changes against the learning.

No full local suite or iOS execution is claimed. The Python 3.9 host lacks tomllib; the comparison/audit scripts ran with an isolated tomli compatibility runner, without source changes.

## CI maintenance, 2026-10-15

Rebased on main 132b4da6f7, including rustls 0.23.46. Range-diff preserves all
three original patches; the third patch has only generic_templates context
changes in Magician's context initializers.

The final warning-free dev compiler passes all twelve constant codegen tests
(and the harness's seventeen support checks), the neutral constant contract
unit test and four Magician constant identity tests. An initial replay linked
the previous worktree's shared Magician archive and failed the AOT/eval case.
After an explicit default bridge rebuild from this worktree, the complete
constant harness passes without any source correction.

Builtin exporter build with curl, registry/page extraction, module sections,
PHP comparison, docs audit, site validation and target-architecture boundary
audit all pass. Generated docs and registries have no drift. PHP comparison
uses Python 3.12 from the existing ARM64 test container because the local
Python 3.9 venv lacks tomllib; the repository's Python requirement is unchanged.
The docs exporter did not overwrite the top-level default Magician archive.

Native compiler SHA-256: fba8b7cd9574f7e63149ca707d496927ea4176aec87e284460cabdafb783dec0.
Default Magician SHA-256: ee16e83bc58bd69387ee7754341d9250e2701139ee373d313d9346d777e71a53.
Evidence: /private/tmp/elephc-pr-maintenance-20261015/1559-*.
No full local suite was run; the fresh pushed-SHA CI remains authoritative.

## Pooled web request review, 2026-10-16

The published CLI guard was process-persistent while PHP constants are request-local.
The real pool integration regression reuses one worker for five HTTP requests;
the baseline failed on the second request with duplicate warnings and a false
`define()` result. The corrected compiler passes all five requests in the same PID.

DataSection now records zero-initialized request-local words separately from static
locals and arbitrary common symbols. Both literal `define()` and top-level `const`
register their shared guard there. Web reset clears the markers after every cleanup
callback, including resource/handler teardown, and before arena reclamation. No
symbol-prefix convention or heap-owning static-local record is used for these words.
An emitter regression covers ordering and deterministic deduplication on all five
supported targets. The public builtin contract and generated docs are unchanged.

Final focused replay: all twelve existing native constant codegen tests pass,
including AOT/eval identity; the repeated pool-request regression passes all five
requests; the actual Cargo library emitter test passes and checks all five targets.
The compiler build is warning-free; builtin EIR architecture inventory has zero
structural errors; assembly-comment alignment and whitespace checks pass. No public
builtin contract changed, and no generated documentation changed.

Trail check reports twenty lexical overlaps with existing identity/cache learnings
and the new web reset learning. Inspection confirms that definition keys retain
literal separators, lookup keys remove one, caches were rebuilt from this worktree,
and web guards use explicit metadata after callbacks rather than prefixes or static
heap owners. The flagged regression and documentation phrases describe these rules;
none reintroduces the rejected behavior. Full fresh-SHA CI remains pending.

## Common-slot capacity review, 2026-10-16

The current published compiler at c0d347de95 emits `_eir_global_X` with eight bytes
for `function f() { define('X', 2); } const X = 'abc'; f();`. Functions are emitted
before the handler/main body, and common-symbol deduplication kept the first size
although the main string store needs sixteen. The frozen CLI and emitted directives
are recorded in 1559-capacity-baseline.json; no unsafe native baseline was executed.

Common-symbol deduplication now records the entry index and monotonically grows its
size to the maximum request. Labels and first-seen order remain stable, and a later
narrow declaration cannot shrink storage. The actual DataSection source, compiled
as a standalone Rust test module with the real compiler types, reproduces the old
failure and passes all five module tests after the correction. Its new test covers
both size orders and three growth sequences on every supported target.

The native regression checks assembly reservations through the canonical symbol-name
helpers before executing three conflicting definitions across a function and main.
It preserves the first values and exactly one warning per duplicate, on/off. The
first test run used an incorrect raw underscore spelling for the symbol assertion;
the implementation emitted sixteen bytes correctly. The test now uses the same public
symbol identity helper; that corrected case passes on/off. All twelve existing
constant controls passed on the same production build. The real five-request pool
test passes again, and compiler/default Magician builds are warning-free.

The public builtin contract and generated documentation remain unchanged. Full fresh
CI is required after the next push. Evidence: 1559-capacity-*.
