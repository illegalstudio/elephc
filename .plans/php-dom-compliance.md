# PHP DOM 8.5 compliance plan

Last verified: 2026-08-21

Authoritative branch: `feat/php-dom-compliance`

Authoritative worktree:
`/Users/guillaumeloulier/PhpstormProjects/oss/elephc/.claude/worktrees/php-dom-compliance`

Current committed implementation checkpoint: `65727371d`
(`test(dom): checkpoint coverage and safety hardening`), containing rebased
implementation checkpoint `dfacafcf1`.

Current branch head before this plan-only synchronization update: `65727371d`.

Current published synchronization checkpoint: `19ca0cddd`
(`docs(dom): record main synchronization`), containing implementation/test
checkpoint `65727371d`.

Current synchronized upstream baseline: `1c6bb5e34`
(`chore: update repository stats`)

Legend:

- `[x]` complete and verified for the stated scope
- `[~]` partially implemented; the completion gate remains open
- `[ ]` not yet complete

## Executive status

- [x] Create the dedicated branch and isolated worktree from fresh `origin/main`.
- [x] Rebase the complete local DOM campaign onto current `origin/main`
  `127ca4f6b` and resolve the modular compiler/runtime migration. The 182
  historical branch commits plus the uncommitted TDD checkpoint were preserved
  under `backup/php-dom-compliance-pre-rebase-20260810` and
  `backup/php-dom-compliance-wip-pre-rebase-20260810`, then collapsed into the
  single auditable commit `571d19bbd` directly above upstream. Independent
  Terra/Luna audits found and restored lost `ArrayAppend` eval-AOT handling,
  URL-stat success flags, `__PHP_Incomplete_Class` serialization, parser
  regressions, SimpleXML comparisons/casts/isset/unset/numeric coercion,
  DOM `NamedNodeMap` errors, XPath dynamic-spread presence, DOM/SimpleXML
  writes/foreach ownership, and their new modular homes. The branch is now two
  commits ahead and zero behind `origin/main`; the rebase state is clean. The
  remote fork still names the pre-rewrite history and must later be updated
  with `--force-with-lease` after validation, never pulled into this branch.
- [x] Refresh the rebased campaign again on 2026-08-21 onto `origin/main`
  `1c6bb5e34`. Checkpoint `0f0b9a091` was preserved as
  `backup/php-dom-compliance-wip-pre-rebase-20260821`; the three rebased commits
  are now `dfacafcf1`, `20070e4a7`, and `65727371d`. Terra/Luna resolved the 15
  modular conflicts across frontend/EIR, backend/runtime metadata, and
  builtins/IO while retaining the deleted legacy `web_prelude/usage.rs` in its
  new `prelude_prune` home. The resulting topology is zero behind and three
  commits ahead of `origin/main` before this plan-only update. The locked spec
  digest remains exact and the 52 focused Python coverage/PHPT-runner tests pass.
  The fork was then force-updated with an explicit lease from old remote head
  `50347ae2ee` to synchronized checkpoint `19ca0cddd`; the lease and a direct
  `ls-remote` verification both confirmed the intended publication.
- [x] Read `CONTRIBUTING.md` and the repository target/test/ownership rules;
  the referenced `RTK.md` is not present in this checkout.
- [x] Freeze PHP `8.5.8`, php-src commit
  `26b97507444c4fbda072f57dda1820f7b7d5e467`, libxml2 `2.15.3`, and
  bundled Lexbor `2.7.0`.
- [x] Freeze the complete DOM/libxml/SimpleXML reflection and PHPT ledgers.
- [x] Historical revision-2 specification consensus was obtained from GLM 5.2,
  Kimi K2.7, and MiniMax M3 on digest
  `2d58e6fe4787e82938d5f053c0271d534619687d8140a53aeea28c40a9712f4b`.
- [x] Re-lock revision 6 byte-for-byte with the replacement read-only college:
  GLM 5.2, Kimi K2.7, and Kimi K3. MiniMax M3 is no longer a current reviewer;
  implementation writing is paused until all three new LOCK lines name the
  same revision-6 digest
  `fb1b6bac24987ba64ab7330262bc2f534d1273f0556b00daf4463071e8b02690`.
  GLM's revision-3 lock was invalidated after Kimi K2.7 requested stronger
  evidence. Revision 4 adds exact PHP 8.5.8 oracle transcripts for dynamic DOM
  properties and all six writable `LibXMLError` fields, direct pinned-stub
  evidence for the disputed modern types/property, the full
  `ext/dom/html5_parser.c` path, and explicit malformed/ABI-prefix semantics.
  GLM's revision-4 lock was then invalidated after Kimi K3 found four ABI/cache
  ambiguities. Revision 5 defines stable `MALFORMED_REQUEST = 5` semantics,
  result ownership for every status, observable test instrumentation plus
  production no-op semantics for invalid releases, adversarial rows, and a
  native-identity cache key whose registered class is creation metadata.
  Kimi K3 locked revision 5, while GLM 5.2 and Kimi K2.7 raised claims that the
  exact PHP 8.5.8 CLI and pinned stub disproved. Revision 6 adds the missing
  direct Reflection transcripts for `DOMException::$code`, the two one-argument
  SimpleXML import functions, and the non-final/non-abstract modifiers of
  `Dom\CharacterData`, `Dom\HTMLCollection`, and `Dom\CDATASection`; all
  revision-5 verdicts are invalidated by this evidence-only change. GLM 5.2,
  Kimi K2.7, and Kimi K3 then returned only the exact revision-6 LOCK line.
- [x] Preserve the complete review evidence under
  `docs/specs/reviews/php-dom-spec/`.
- [x] Open upstream design issue
  `https://github.com/illegalstudio/elephc/issues/622`.
- [x] Add the statically linked `elephc-dom` bridge, generated operation
  registry, `--with-dom`, automatic usage detection, panic-safe ABI, host
  callbacks, and native source locks.
- [x] Add compiler lowering and runtime materialization for native DOM wrappers,
  structured `DOMException`/`ValueError`/`Error`, and `LibXMLError` values.
- [~] Implement the complete PHP 8.5.8 DOM/libxml/SimpleXML behavior.
- [x] Reach zero unimplemented generated operation routes (603/603 explicit;
  reproducible `comm` inventory empty on 2026-08-01).
- [ ] Reach zero DOM-behavior exclusions across the complete upstream PHPT ledgers.
- [~] Complete the test-first DOM layer before further production work. The
  coverage contract is derived from every locked specification requirement,
  all 603 explicit operations/object handlers, ABI malformed/lifetime cases,
  checker/EIR/runtime/ownership paths, and all 1,056 frozen PHPT entries. The
  upstream PHPT replay is the exact integration test for each upstream case;
  focused Rust tests cover compiler/ABI/root-cause invariants and supported-
  target emission without duplicating every PHPT body. A machine-checkable
  coverage map must leave no spec cell, route, handler family, target-sensitive
  path, or PHPT without a named test/evidence owner before implementation
  resumes.
  The initial read-only audit finds 208 dedicated DOM E2E tests, 77 DOM bridge
  tests, only three dedicated DOM diagnostics and three dedicated DOM EIR
  tests, and no structured DOM suite under `tests/codegen/runtime_gc/`. The 603
  operations partition into 546 DOM, 20 libxml, and 37 SimpleXML routes. First
  test-only waves therefore own: the coverage-gate unit tests and deliberately
  unmapped bootstrap; exhaustive bridge/ABI adversarial cases; DOM surface,
  EIR-family, and runtime-GC matrices; then legacy/modern mutation, selectors,
  XPath, parser/diagnostic/stream/entity, validation/C14N/XInclude, and the
  SimpleXML/libxml handler/loader/interop/override matrices. One existing
  production branch still says legacy XPath namespace-node results are not
  implemented, and SimpleXMLElement serialization denial appears uncovered;
  both require oracle-pinned red tests before any production correction.
  The coverage-gate TDD tranche now has twelve Python contract tests covering
  all of the above inventory/provenance/orphan/target rules. Its first deliberate
  red run completed in 1.042 seconds with 12 tests and 15 failed assertions,
  each caused by the still-absent `generate_coverage_manifest.py` or
  `check_coverage.py`; this is recorded as the pre-implementation baseline, not
  as a product regression. The SimpleXML/libxml tests-only wave is now written:
  it covers dynamic and by-reference handlers, iterator states, forbidden
  serialization (including subclasses and nested values), DOM import/family
  isolation, file/stream-context loading, the shared libxml error queue,
  re-entrant external-entity callbacks, `__debugInfo(): ?array`, and a
  table-driven 47-reference EIR matrix (39 SimpleXML and 8 libxml references)
  for all three supported targets. PHP 8.5.8 oracle transcripts pin the
  serialization exceptions, loader callback state, ordered shared errors, and
  detached/clone/import behavior. No production code, Cargo build, or PHPT
  replay was performed for this wave; the missing native serialization guard,
  `__debugInfo` return validation, stream/callback propagation, and any absent
  reference lowering remain deliberate red causes. The cross-suite
  completeness audit remains in flight before production work may resume. The
  reflection/surface wave is now written as five runtime tests and three
  frontend tests. It pins PHP 8.5.8 hierarchy/interface/finality/
  constructibility/clone metadata, signatures and parameters, readonly and
  virtual properties, `Dom\\AdjacentPosition`, class/interface/trait and
  extension registration, DOM/libxml/SimpleXML function signatures, plus exact
  arity/named/type failures. Reflection/ReflectionEnum completeness, registry
  visibility, and exact runtime exception transport are retained as deliberate
  reds. The parser/diagnostics/stream/entity/validation tranche is also now
  written and registered across six focused modules. Its PHP 8.5.8 oracle
  matrices cover BOM/NUL/UTF-8 recovery, `PARSEHUGE`/`NO_XXE`, ordered libxml
  diagnostics, PHP stream contexts and re-entrant entity callbacks, validation
  post-failure state, C14N filters/files/errors, and XInclude epoch/fallback/
  cycle behavior. This wave likewise performed no Rust build or product edit;
  parser-option routing, diagnostic-queue fidelity, callback propagation,
  validation state preservation, C14N file errors, and XInclude mutation/cycle
  invalidation remain explicit red hooks. The legacy/modern/selectors/XPath
  tests-only wave is now written and registered as four compact matrices with
  14 table-driven tests and four identity/heap probes. Its explicit namespace-
  axis case pins all six PHP 8.5.8 `DOMNameSpaceNode` results and remains ignored
  as a declared pre-implementation red because the current bridge documents
  that route as unsupported. Oracle evidence also shows both `DOMXPath` and
  `Dom\\XPath` are uncloneable; this conflicts with an older test that expects a
  modern XPath clone. The conflicting older expectations have now been removed
  from the tests after a second exact oracle probe (`DOMXPath:false`,
  `Dom\\XPath:false`, `DOMNameSpaceNode:true`); the legitimate namespace-node
  clone case remains. No Rust/product command or edit occurred in these waves.
  The coverage-gate implementation now has 26 strict contract tests and runs
  together with the 25 PHPT-runner tests: all 51 pass. Independent reviews
  rejected earlier permissive revisions, leading to fail-closed checks for the
  exact 603 route identities, the 1,056-entry `868/32/156` PHPT partition, all
  three supported targets, complete locked-source and build provenance, true
  registered Rust test owners, confined non-symlink paths, atomic writes, and
  closed non-skipped ledgers. After four adversarial rejection/correction
  rounds, the independent final re-review returned `GO` for the gate contract;
  no complete coverage manifest or target attestation exists yet. The
  checked-in ledgers correctly remain
  `closed: false` with all 1,056 entries `pending`, so the strict gate must fail
  on the real campaign today. Additional tests now cover DOM/SimpleXML wrapper
  retention, live and weak cache identity, detach/reattach, clone/import,
  cycles and heap-debug finalization, plus the last previously unmentioned
  direct routes (`importLegacyNode`, `relaxNgValidateSource`, `isSupported`) and
  `libxml_disable_entity_loader` state/deprecation behavior. A real bootstrap
  now materializes all 603 requirements/routes and 1,056 pending PHPTs without
  inventing owners or target attestations; its strict run reproducibly exits
  non-zero with 2,872 gap diagnostics. The official PHP 8.5.8 tarball and
  libxml2 2.15.3 archive were restored and verified at the locked SHA-256
  values (libxml tag commit `c94eb0210`). The serialized `-j1` oracle build is
  complete at `/private/tmp/php-dom-oracle-build-8.5.8/sapi/cli/php`; it reports
  `PHP 8.5.8`, `libxml2 2.15.3`, statically linked libxml, and binary SHA-256
  `6253fe2a063a1368d4a821878afebe946a604b73715f4069f687e72502ee9f79`.
  Every expectation added in the newest XPath, GC/cache, SimpleXML, entity-
  loader, and uncovered-route tests has now been replayed byte-for-byte against
  that exact oracle. Elephc Cargo/heap-debug execution remains pending while
  unrelated Cargo work is consuming memory after two earlier OOMs.
- [ ] Add final examples plus public and internals documentation.
- [ ] Validate the complete supported target matrix.
- [ ] Obtain absolute implementation consensus from GLM 5.2, Kimi K2.7, and
  Kimi K3 on the same final commit.
- [x] Push checkpoint `bcae90c96a` to `fork` and open draft pull request
  `https://github.com/illegalstudio/elephc/pull/654`.
- [x] Resolve the draft's conflicts with `main` at merge checkpoint
  `b75bb3d6bd`, preserving the modular `LinkPlan`, typed DOM bridge metadata,
  macOS `iconv`, runtime dispatch, ownership, and the new loop-storage
  contracts.
- [x] Push the synchronized merge checkpoint through `555da45a96`; draft PR
  #654 is `MERGEABLE`.
- [x] Push the CI-remediation checkpoint through `8f26eba93a`; draft PR #654
  remains `MERGEABLE`.
- [~] Complete checkpoint CI run `30542897247`. The macOS archive and managed
  native-package jobs pass. Linux archive jobs expose the missing CMake package
  in the published CI image, the Ubuntu builtins-doc job exposes strict-C11
  `PATH_MAX` visibility, and codegen/eval shards all expose the same missing
  archived `elephc-dom` staticlib under Cargo offline mode. Commits
  `1cd9c71fa` and `4bd4e49e0` add CMake plus a guarded Linux fallback, the
  strict-C11 path fallback, `elephc-dom` to every test archive, and the mixed
  `Route::matches(Request)`
  collision is fixed with exact PHP `TypeError` and Stringable coercion
  semantics. The six option-driven session regressions were bisected to
  `cb1f285d417564fb379dfbb3024742445d0d559e`: a reachable `if` join restored
  the pre-split logical type map wholesale, so conditionally assigned session
  options remained statically `null` and their setters received the null
  sentinel. Commit `63cce73d8` instead merges every reachable local type
  variable-by-variable, with `Mixed` absorption, normalized unions, physical
  slot widening, and unilateral-key removal. Focused EIR/codegen plus all six
  session regressions pass locally. Final readiness and merge remain gated on
  checkpoint CI, full implementation, and same-commit reviewer consensus.
- [~] Complete replacement CI run `30549092295` for `8f26eba93a`. All three
  archive jobs now pass, including Linux x86_64 and Linux AArch64, as do all
  three managed-native-package jobs and both Linux web shards. Root-cause
  triage found four remaining causes: stale generated `fread` docs, indexed
  array branch merges degrading `Array(Mixed)` to representation-level
  `Mixed`, unconditional references to an un-emitted DOM finalizer, and stale
  DOM EIR tests plus builtin parity allowlists. All four are corrected in the
  current working tree and focused regressions pass; a replacement CI run is
  still required before any green-CI claim.
- [~] Validate published checkpoint `d039d1a4c7`. The `CI Image` run
  `30690381777` and `PR labels` run `30690380847` are green. Complete `CI` run
  `30690381815` exposed a cross-suite linker failure on every supported target:
  Mixed property dispatch could emit latent DOM branches while EIR runtime
  feature detection left `dom_bridge` disabled. The current working tree now
  detects virtual DOM property candidates, bodyless DOM method candidates, and
  direct and dynamic native-wrapper descendant allocation. A dedicated
  read-only GLM 5.2 audit found the covered EIR paths exhaustive for the active
  linker failure; its defensive `ProfiledData` finding and two negative
  feature-detection tests are also integrated. The two reproduced non-DOM
  failures, 19 focused lowering tests, the dedicated end-to-end regression,
  `cargo check --tests`, and the exact CLI hydrator reproduction pass locally;
  replacement complete CI run `30693835590` is queued for `ccde8612b1`;
  CI Image run `30693835592` and PR classification run `30693834485` are
  green, but no complete green-CI claim is made yet.
- [~] Complete replacement CI run `30693835590` for `ccde8612b1`. The
  `Web Tests (linux-x86_64 1/2)` shard exposed one narrower link contract:
  any `dom_bridge` runtime contains the native host callback, which references
  `__rt_dom_xpath_resolve_callable`, while wrapper allocation or conservative
  boxed dispatch can activate the bridge without lowering a direct DOM call
  that used to emit that resolver. The current working tree now guarantees
  both XPath callable resolvers at the first emitted function whenever
  `dom_bridge` is active. Its dedicated assembly regression, all 20 focused
  DOM feature/lowering tests, `cargo check --tests`, and the exact repeated
  Web-router test pass locally at `e7315176e`. Linux x86_64 target validation and a
  replacement CI run remain required before this remediation is closed. A
  focused Linux x86_64 callback replay is rebuilding its Docker test image.
  Replacement `CI Image` run `30695058098` is green on amd64 and
  arm64. Complete `CI` run `30695058078` for published head `50347ae2ee`
  finished failed with 57 successful, 57 failed, and one skipped job; within it,
  both Web shards now pass on Linux AArch64, Linux x86_64, and macOS AArch64,
  closing the resolver-link reproduction on every supported target. The run
  exposed multiple deterministic codegen regressions,
  including DOM/XPath callback value materialization, stream-wrapper
  by-reference paths, callable arguments, and unrelated flow-narrowing cases;
  these are being clustered by root cause, so no complete green-CI claim is
  made yet.
- [~] Remediate the deterministic failures from CI run `30695058078` in the
  current uncommitted checkpoint. The 15 flow-narrowing/list-unpack failures
  reproduced as silent exit 139 on macOS. EIR, assembly, and crash tracing found
  the actual cause: a narrowed `?array` parameter remained physically boxed
  `Mixed`, but list-unpack used static `ArrayGet` and interpreted a typed string
  element's length word as a Mixed-cell pointer. Mixed-backed locals now take a
  borrowed physical-storage read through `__rt_mixed_array_get`; statically
  homogeneous array slots retain their specialized `ArrayGet`. The complete
  narrowing filter passes 37/37, both break/continue list-unpack regressions pass,
  all four branch-assignment guards pass, and `cargo check --lib` is green.
  XPath callback node snapshots now retain each
  returned node's actual foreign `DocumentGraph`, and x86_64 host-loader
  cleanup passes boxed/raw arguments in SysV `rdi` instead of scratch `rax`.
  A main-only `cdylib` fallback also publishes both XPath callable resolvers.
  The two shared callable/`Mixed` descriptor failures are now corrected: a
  returned argument transfers exactly one retained invoker owner, duplicate
  aliases are released, and refcounted returns use the owning Mixed boxer. Both
  original regressions, a heap-debug duplicate-owner regression, success/throw
  ownership units, three-target assembly emission, and `cargo check --lib`
  pass. The two all-target DOM Stringable failures are also corrected: native
  preflight snapshots every boxed operand before userland coercion, preserves
  source order and exact diagnostics, accepts boxed/static `Stringable` where
  PHP does, rejects the manual variadic node-or-string surface, and releases
  staged owned strings on throws without duplicating caller-owned `__toString()`
  returns. Six mixed-method regressions, two heap-debug throw paths, property,
  variadic, x86_64 lowering, and `cargo check --lib` pass. The remaining CI
  inventory is explicitly clustered as
  eight residual x86_64 DOM/stream ABI failures, four x86_64 stream conversion
  failures, four object/interface/reflection failures, two nullable-string
  ownership failures, ten evaluator failures, one LFC runtime-feature mismatch,
  and one apparently independent TLS test. No downstream failure is
  marked fixed until its exact focused replay passes.
- [~] Establish executable PHPT evidence. The new pinned runner validates the
  exact php-src commit/tree/ledger and PHP 8.5.8 + libxml2 2.15.3 oracle,
  stages isolated oracle/Elephc sandboxes, and supports the complete CLI
  section subset plus PHP-PCRE EXPECTF/EXPECTREGEX matching, raw outputs,
  exit codes, and before/after-CLEAN filesystem deltas. Its 24 self-tests,
  Python/PHP syntax checks, exact snapshot check, and fail-closed rejection of
  the local PHP 8.5.6/libxml2 2.9.13 CLI pass. `FILE_EXTERNAL` and external
  expectations are now resolved within the pinned component tree, so all 868
  DOM, 32 libxml, and 156 SimpleXML PHPTs parse without a harness exclusion.
  The frozen oracle is reconstructed from both official archives whose SHA-256
  values match `source-lock.json`; its preflight is exactly
  `8.5.8|2.15.3|21503|dom,libxml,simplexml`. After the temporary evidence
  directory was lost, the parent reconstructed
  the pinned Git source at `/private/tmp/php-dom-spec-php-src-8.5.8`, built the
  vendored static libxml2 archive into
  `/private/tmp/php-dom-libxml2-2.15.3-install`, and rebuilt the exact oracle at
  `/private/tmp/php-dom-oracle-build-8.5.8/sapi/cli/php`; the live probe again
  reports `8.5.8|2.15.3|21503|dom,libxml,simplexml` from the clean pinned
  php-src commit `26b97507444c4fbda072f57dda1820f7b7d5e467`. The
  provisional source-tree CLI links libxml2 2.9.13 and is explicitly excluded
  from final differential evidence.
  The first real SimpleXML replay
  reached Elephc and exposed the terminal closing PHP tag in `000.phpt`. The
  lexer now lowers a close tag to PHP's implicit statement terminator, absorbs
  exactly one adjacent LF/CRLF, and emits remaining terminal inline HTML
  literally; lexer/parser/codegen/LFC regressions pass, while unsupported PHP
  reopening is rejected explicitly. Compilation failures now retain the complete
  oracle FILE output, exit, match, and filesystem-delta evidence rather than
  returning before PHP 8.5.8 is executed. The Elephc sandbox now also stages
  the repository's locked managed-PCRE2 project, while compilation restores
  the host's fingerprinted toolchain environment instead of deriving a fresh
  cache key from every isolated PHPT temporary directory. All 24 harness tests
  pass, and `000.phpt` now compiles and executes through both runtimes: its
  first genuine implementation blocker is the runtime `eval()` failure after
  the first two boolean checks. The independent no-`eval()` replay
  `ext/simplexml/tests/001.phpt` now passes oracle expectation, exit, and
  filesystem-delta parity after the SimpleXML debug projection was corrected
  to use libxml2's raw node names for empty comments and processing
  instructions. The pinned report is
  `/tmp/simplexml-001-empty-nodes-report.json`. The next 31 numeric no-`eval()`
  replays (`002` through `030`, including `009b` and `016a`) now have frozen
  reports at `/tmp/simplexml-002-009-report.json`,
  `/tmp/simplexml-009b-015-report.json`,
  `/tmp/simplexml-016-022-report.json`, and
  `/tmp/simplexml-023-030-report.json`: `003.phpt`, `009b.phpt`, `010.phpt`,
  `024.phpt`, and `025.phpt` pass exact expectation, exit, and file-delta
  parity in the original batch. Compiler corrections now also make
  `004.phpt`, `005.phpt`, `009.phpt`, `011.phpt`, `013.phpt`, `015.phpt`,
  `018.phpt`, `021.phpt`, `026.phpt`, `profile02.phpt`, and
  `iterator_interaction_empty_and_var_dump.phpt` pass exactly. The pinned
  follow-up reports are `/tmp/simplexml-prop-get-after-fix.json`,
  `/tmp/simplexml-string-coercion-after-fix.json`,
  `/tmp/simplexml-phpt-009-final-native-vtables.json`,
  `/tmp/simplexml-phpt-profile02-final-audit.json`,
  `/tmp/simplexml-property-string-iterator-after-fix.json`, and
  `/tmp/simplexml-021-after-fallible-isset.json`. A targeted replay after the
  wrapper/string comparison correction also makes `020.phpt` and the previously
  stale `023.phpt` pass exactly; its report is
  `/tmp/simplexml-020-023-after-compare-fix.json`. Fallible-wrapper clone
  lowering now preserves the concrete class, throws PHP's exact catchable
  `TypeError` on actual false/null arms, and balances owning inline receivers;
  `002.phpt`, `006.phpt`, and `019.phpt` pass 3/3 in
  `/tmp/simplexml-clone-fixed-report.json`. Static object/interface foreach
  states now emit `IterEnd` on exhaustion, `break`, multi-level loop exits,
  `return`, and `throw`, dropping the iterator retain before an owning source
  expression. The nine focused heap-debug/assembly regressions pass, including
  `foreach (clone $root->children() ...)`; object-decref emission passes for
  macOS AArch64, Linux AArch64, and Linux x86_64, and the frozen `006.phpt` plus
  `019.phpt` replay passes 2/2 in
  `/tmp/simplexml-006-019-after-iterend.json`. Dynamic iterator-source cleanup
  and exception unwinding during `IterStart`/`rewind` remain separate audit
  boundaries. Chained SimpleXML dimension unsets
  now bypass the generic language-construct backend and `030.phpt` passes in
  `/tmp/simplexml-030-after-chained-unset.json`. `022.phpt` now passes in
  `/tmp/simplexml-phpt-022-after-fix.json`: native debug projection no longer
  reapplies a selected Element view name after resolving its effective node,
  and foreach keeps the effective `current()` return type for native/inherited
  SimpleXML while preserving user `current(): mixed` overrides.
  Fallible loader property
  chains retain a concrete wrapper result, scalar/string calls use the native
  cast handler, and `isset()`/`empty()` distinguish a boxed `false` loader
  result from a valid but empty XML node. Runtime-callable bodies fill all five
  native `Iterator` slots plus `__toString()` for the base class,
  `SimpleXMLIterator`, and non-overriding descendants; the assembly-only
  regression passes for macOS AArch64, Linux AArch64, and Linux x86_64.
  Property-dimension writes on a fallible loader union now lower through the
  SimpleXML `read_property` and `write_dimension` opcodes instead of a generic
  no-op `RuntimeCall`; the EIR-shape regression and exact `028.phpt` replay pass,
  with the frozen report at
  `/tmp/simplexml-028-after-eir-autovivification.json`. A read-only php-src/oracle
  audit also proves that `012.phpt` requires the complete byte-oriented
  `__HALT_COMPILER()` language feature: context-sensitive lexer termination,
  byte-exact suffix offsets, per-physical-file metadata, namespaces/includes,
  and dynamic constant APIs. That structural path is now implemented across the
  main source, includes, and autoload/classmap reads: opaque binary suffixes,
  exact `;`/`?>` newline offsets, outermost scope, per-file mangled constants,
  and direct/dynamic constant lookup are covered by focused frontend tests.
  Exact `012.phpt` now passes in `/tmp/simplexml-012-after-halt.json`; the
  codegen harness now uses the same idempotent physical-source finalization as
  production. Five lexer, three parser, one source-unit, four offset/namespace/
  include E2E, one terminal-entry, one PSR-4 autoload, and two close-tag
  regressions pass alongside a warning-free binary build. The broader constant
  surface still needs separate parity work for dynamic `constant()`/`defined()`
  names, `get_defined_constants()`, imported `use const` aliases, reserved-name
  `define()` behavior, byte-oriented executable prefixes, exact syntax
  diagnostics, and `eval()` fragment finalization; none blocks the exact frozen
  DOM/SimpleXML `012.phpt` evidence. A subsequent php-src scanner audit also
  closed standalone-CR close-tag/newline handling, narrowed trivia to PHP's four
  scanner whitespace bytes, prevents static `ClassName::__HALT_COMPILER()` from
  defining the terminal payload boundary, and restores PHP's reserved-token
  rejection for static calls and method declarations while preserving instance
  member calls. Focused lexer/parser regressions lock those boundaries.
  SimpleXML arithmetic now follows php-src's `_IS_NUMBER` object cast: the
  bridge preserves dynamic integer/float results (including exponent and
  integer-overflow cases), nullable/failed wrapper reads convert to integer
  zero, and dimension write-back applies PHP scalar-to-string conversion
  before the native handler. Focused bridge and E2E regressions cover integer,
  float, empty, missing, false-loader, compound-write, bool, and null values;
  exact `014.phpt` expectation, exit, and file-delta parity pass in
  `/tmp/simplexml-014-after-numeric-cast.json`.
  SimpleXML XPath parse diagnostics now carry a dedicated ABI flag that asks
  the compiler to append only the PHP call-site path and line, preserving the
  libxml-generated warning prefix and the leading blank line. The focused
  bridge regression, macOS AArch64/Linux AArch64/Linux x86_64 assembly
  regression, and exact `008.phpt` replay pass; the frozen report is
  `/tmp/simplexml-008-after-callsite-location.json`.
  Nested SimpleXML dimension assignments now fetch their numeric parent through
  `read_dimension` with php-src's `BP_VAR_W=1` mode before routing the named leaf
  through `write_dimension`; the EIR regression locks opcodes 4453 and 4457 and
  rejects the former generic `RuntimeCall` fallback. Numeric-gap diagnostics
  cross the native ABI as raw details with an explicit decoration flag, then the
  target-aware backend adds PHP's exact callable, source path, and line while
  preserving `@` suppression. Focused bridge and macOS AArch64, Linux AArch64,
  and Linux x86_64 assembly regressions pass. Exact `027.phpt` expectation,
  exit, and file-delta parity pass on the consolidated binary in
  `/tmp/simplexml-027-after-callsite-warning.json`.
  The refreshed `007.phpt` attribute read/write/unset replay also passes exactly
  in `/tmp/simplexml-007-017-029-current.json`; `017.phpt` and `029.phpt` now
  compile through the corrected shared `count()` checker contract for direct
  and fallible SimpleXML wrappers, but the exact replay
  `/tmp/simplexml-017-029-after-count.json` exposes a separate DynamicMixed
  wrapper-dispatch defect in untyped function parameters and foreach values, so
  neither case was counted at that checkpoint. Dynamic property/dimension/count
  lowering now dispatches boxed SimpleXML receivers by runtime class on all three
  targets. The accompanying interface-return ownership transfer closes the former
  two-block-per-element `IterCurrentValue` leak; focused heap-debug tests are clean
  on exhaustion, `break`, and `return`. Exact `017.phpt` and `029.phpt` now both
  pass in `/private/tmp/simplexml-017-029-after-dynamic-mixed.json`. A refreshed
  ten-case replay passes `001-mb`,
  `014a`, `014b`, `016`, `016a`, `035`, `037`, and `038` exactly; `032` still
  rejects a Mixed native-wrapper argument. `036`'s missing
  runtime-callable `_method_SimpleXMLElement_count` is now materialized through
  the native handler for user overrides calling `parent::count()`; its EIR/E2E
  regressions and exact replay pass, with frozen evidence in
  `/tmp/simplexml-036-after-count-runtime-entry.json`. The ten-case baseline
  report is `/tmp/simplexml-next-probable-green.json`. The `addAttribute()` and
  `addChild()` warnings in `031.phpt` now use the location-only diagnostic ABI
  for opcodes 4428/4429; bridge, three-target assembly, and exact PHPT evidence
  pass in `/tmp/simplexml-031-after-callsite-warning.json`. The `032.phpt`
  comparison path now guards `SimpleXMLElement|false` operands before native
  request marshalling, invokes opcode 4448 only on the object/object arm, and
  reduces failure arms plus direct object/bool pairs through the opcode-4447
  boolean cast in source order. The focused EIR/backend regression passes for
  macOS AArch64, Linux AArch64, and Linux x86_64. Independent php-src review
  additionally rejected the former same-class-only optimization: distinct
  `SimpleXMLElement` subclasses share the handler table and must still use
  opcode 4448. That routing correction is integrated. The bridge now reproduces
  `sxe_object_cast_ex`/`sxe_prop_is_empty` across direct roots, properties,
  dimensions, attributes, and filtered namespace views; its focused bridge test,
  the three-target lowering/backend regression, the native heap-debug comparison
  test, and the frozen PHP 8.5.8 oracle matrix all pass. Exact `032.phpt`
  expectation, exit, and file-delta parity pass in
  `/private/tmp/simplexml-032-after-comparisons.json`. A supplemental exact-output
  regression also proves that two distinct SimpleXML subclasses imported from
  the same DOM node compare equal through their shared handler table. Its
  heap-debug variant exposed a separate pre-existing DOM/SimpleXML interop
  retention of two blocks/603 bytes even after explicit `unset`. The audit
  isolated the cause to a narrowed `DOMElement|null` local: unboxing retained
  the source wrapper while the internal-function call's conservative unknown
  return-alias summary suppressed its matching release. `simplexml_import_dom()`
  now has the narrow php-src-backed non-aliasing contract that its fresh
  cross-family wrapper result requires. The focused EIR release regression and
  complete subclass comparison heap-debug test pass cleanly. A six-case named-method replay on the current consolidated binary
  passes `SimpleXMLElement_addAttribute_basic.phpt`,
  `SimpleXMLElement_addAttribute_required_attribute_name.phpt`, and
  `SimpleXMLElement_asXML_fragment_filename.phpt` exactly in
  `/private/tmp/simplexml-named-six-current.json`. The remaining three failures
  are root-caused: `getDocNamespaces` and `xpath` both require preserving a
  successfully recovered XML document with no root node, while `xpath_4` is
  compiler-blocked by the missing `PHP_INT_SIZE` constant and then undefined
  variable runtime semantics before its already-correct bridge option check.
  The rootless-document bridge tranche now preserves the recovered document
  while representing the missing root explicitly. Focused bridge and E2E
  regressions pass, and both named PHPTs have exact expectation, exit, and
  file-delta parity in `/private/tmp/simplexml-rootless-after-fix.json`;
  `xpath_4` remains explicitly open. The first exact `033.phpt`/`034.phpt` replay is frozen at
  `/private/tmp/simplexml-033-034-before-casts.json`: `(object)` is absent from
  `CastType`, SimpleXML `(array)` reaches an unsupported heap-array cast, and
  `034` additionally exposes the strict local-reassignment checker. A bounded
  parser/checker/EIR/bridge implementation is integrated without changing opcode
  numbering. Both PHPTs now pass exactly in
  `/private/tmp/simplexml-033-034-after-casts.json`; parser, checker, bridge,
  EIR/backend, and focused E2E regressions pass. The supplemental heap-debug
  cast regression initially exposed three retained blocks/208 bytes after
  `get_class($array['person'][0])`. Differential variants proved the recursive
  `(array)` projection and `count()` read were clean; `get_class()` alone retained
  its owned Mixed argument because its result was incorrectly classified as
  possibly aliasing that argument. `GetClass` and `GetParentClass` now carry the
  non-aliasing `Independent` result contract, the generic EIR release regression
  passes, and the complete SimpleXML cast regression is heap-debug clean. A separate 18-case namespace/profile survey passes nine cases
  exactly: `bug24392`, `bug27010`, `bug41867`, `feature55218`, `profile01`,
  `profile03`, `profile04`, `profile12`, and `profile13`; evidence is frozen in
  `/private/tmp/simplexml-views-namespaces-survey.json`. Its nine failures reduce
  to three compiler roots rather than nine bridge defects: strict local
  reassignment (`bug41861`, shared with `034`), `count()` rejecting a fallible
  SimpleXML union (`bug41947`), and the missing PHP `error_reporting()` builtin
  (`profile05` through `profile11`). The `error_reporting()` audit shows this is
  not safely closed by a scalar global alone: PHP 8.5.8 requires the nullable
  getter/setter contract, signed 32-bit masks, version-correct `E_ALL=30719`,
  severity-aware diagnostics, Zend-compatible `@` save/conditional-restore,
  exception snapshots, Fiber-local effective state sourced from the INI base,
  web reset, and eval/Magician parity. That cross-runtime diagnostic-state
  tranche remains explicitly open. A second 12-case historical-bug survey adds
  seven exact passes: `bug26976`, `bug36611`, `bug37076`, `bug37386`,
  `bug38354`, `bug38424`, and `bug39662`, frozen in
  `/private/tmp/simplexml-bugs-26976-39760-survey.json`. Its five failures are
  retained as distinct work: append-dimension parsing (`bug35785`), empty-name
  `addChild` warning/output (`bug37076_1`), foreach-null debug/warning flow
  (`bug38347`), complex dynamic-property assignment (`bug38406`), and
  SimpleXML debug-wrapper materialization/runtime exit 71 (`bug39760`).
  A third 12-case survey adds six exact passes: `bug40451`, `bug41175`,
  `bug43221`, `bug45553`, `bug46003`, and `bug46047`, frozen in
  `/private/tmp/simplexml-bugs-40451-48601-survey.json`. Its remaining clusters
  are append-dimension parsing (`bug41582`), dynamic/fallible `count()`
  (`bug42259`, `bug48601`), object/string conversion flow (`bug42369`), exact
  XML escaping (`bug44478`), and the missing `get_object_vars()` builtin
  (`bug46048`). A seven-case later-bug survey passes `bug51615`, `bug54973`,
  and `bug61335` exactly in
  `/private/tmp/simplexml-bugs-51615-66113-survey.json`; its four open roots are
  debug/XPath wrapper materialization (`bug52751`), remaining SimpleXML numeric
  inference (`bug53033`), append-dimension parsing (`bug55098`), and tentative
  internal return-type override compatibility (`bug62328`). A 17-case late-bug
  survey adds eight exact passes: `bug67116`, `bug67572`, `bug69491`,
  `bug72588`, `bug72971`, `bug72971_2`, `bug75245`, and `bug76712`, frozen in
  `/private/tmp/simplexml-bugs-62639-81325-survey.json`. Its failures cluster in
  debug/JSON projection, missing generic builtins/coercions (`current`,
  `str_replace`), string fallback, SKIPIF constants and undefined-variable
  execution, constructor scalar coercion, plus legacy DOM parse/import warnings.
  The first complete consolidated SimpleXML replay is frozen in
  `/private/tmp/simplexml-full-after-034-rootless-20260801.json`: 91/156 pass
  exact expectation, exit, and file-delta parity, 63 fail, and two are
  unavailable because the frozen oracle lacks `xsl` or `zend_test`. The first
  complete consolidated libxml replay is frozen in
  `/private/tmp/libxml-full-current-20260801.json`: 2/32 pass exactly, 22 fail,
  six have SKIPIF-control mismatches, and two require unavailable oracle
  extensions (`soap` or `xml`). These two complete component reports supersede
  the former 85-entry exploratory aggregate and established 93 distinct passing
  entries before the 868-entry DOM corpus was replayed. The bounded fallible
  `count()` tranche subsequently made `bug41947.phpt` and `bug48601.phpt` pass
  exactly in `/private/tmp/simplexml-count-unions-after-20260801.json` by
  accepting only `array|false|null`-shaped unions, guarding the false/null arms
  with exact catchable `TypeError`, and retaining the generic rejection of
  unrelated scalar unions. Checker positive/negative tests, three-target EIR,
  and heap-debug E2E pass. `bug42259.phpt` now compiles past `xpath()`/`count()`
  but exposes a separate null interface-dispatch call in
  `RecursiveIteratorIterator::__elephcAdvance` during `rewind()` and is not
  counted as passing. The complete DOM replay is frozen in
  `/private/tmp/dom-full-current-20260801.json`: 209/868 pass exact expectation,
  exit, and file-delta parity, 633 fail, 19 have SKIPIF-control mismatches, four
  time out, two require unavailable oracle extensions, and one oracle execution
  itself fails. The consolidated evidence floor is therefore 304/1056 exact
  passes (209 DOM, 2 libxml, 93 SimpleXML), with 752 entries still lacking
  passing evidence. Failure clustering is in progress before the next bounded
  implementation tranche. The runner now executes oracle and Elephc
  `SKIPIF`/`FILE`/`CLEAN` sections from the sandbox root, matching php-src's
  `TEST_PHP_SRCDIR` working-directory contract; 25 runner tests pass and
  `DOMDocument_getElementsByTagName_liveness_xinclude.phpt` is no longer an
  `oracle_failure`, instead exposing its genuine static
  `DOMNameSpaceNode::$textContent` gap in
  `/private/tmp/dom-xinclude-cwd-after-20260801.json`. PHP's long opening tag is
  now ASCII case-insensitive in PHP and LFC boundary detection. This removes
  the `<?PHP` include barrier from 26 DOM PHPTs, although their targeted replay
  in `/private/tmp/dom-uppercase-open-tag-after-20260801.json` reaches further
  independent dynamic-property, missing-method, reassignment, and core-language
  compile errors and therefore adds no exact PASS yet. Modern
  `Dom\\Node::$baseURI` now follows php-src's non-null contract for every node:
  the bridge returns the effective native base, then the document URL, then
  `about:blank`; legacy `DOMNode::$baseURI` remains nullable. The bridge unit
  and exact `modern/spec/Node_baseURI.phpt` replay pass in
  `/private/tmp/dom-node-base-uri-final-20260801.json`. The current evidence
  floor then reached 305/1056 exact passes (210 DOM, 2 libxml, 93 SimpleXML).
  Legacy XPath namespace snapshots now drop their owned fake
  `DOMNameSpaceNode` allocations before both foreign-member and primary
  `DocumentGraph` retainers. This closes the libxml double-free caused by Rust
  field drop order when PHP released the materialized namespace wrapper and
  document before the `DOMNodeList`; the adversarial bridge regression passes.
  Exact replay in
  `/private/tmp/dom-namespace-node-lifetime-after-20260801.json` makes
  `xpath_domnamespacenode_advanced.phpt` pass and leaves the basic case safely
  executing with only the separate generic debug-projection mismatch. The
  current evidence floor is therefore 306/1056 exact passes (211 DOM, 2 libxml,
  93 SimpleXML), with 750 entries still lacking passing evidence. Native DOM
  collection dimension reads are now accepted and lowered for legacy and modern
  node lists, named-node maps, HTML collections, and DTD named-node maps. The
  checker preserves collection-specific nullable unions; EIR selects `item()`,
  `getNamedItem()`, or `namedItem()` with literal numeric-string coercion and
  passes macOS AArch64, Linux AArch64, and Linux x86_64 emission. The exact
  heap-debug E2E is clean, and
  `/private/tmp/dom-collection-dimensions-targeted-final-20260801.json` makes
  `modern/xml/gh17572.phpt` pass. Three other targeted PHPTs no longer fail with
  `Cannot index non-array` but expose separate append-dimension or overly broad
  legacy-result-union roots. Runtime-unknown string classification and exact
  invalid-offset diagnostics remain explicit collection gaps. The evidence
  floor is therefore 307/1056 exact passes (212 DOM, 2 libxml, 93 SimpleXML),
  with 749 entries still lacking passing evidence. Synthetic native-wrapper
  method bodies referenced only through interface tables are now materialized,
  so DOM collection `getIterator()` slots no longer remain null. Dynamic
  iterator lowering also releases the owned `InternalIterator` at `IterEnd`
  while preserving the borrowed collection receiver. The focused checker/EIR
  and heap-debug regressions pass, and the final 15-case exact replay in
  `/private/tmp/dom-interface-dispatch-after-20260801.json` records ten passes,
  five output/diagnostic mismatches, and zero crashes. One of those passes,
  `xpath_domnamespacenode_advanced.phpt`, was already included in the namespace
  lifetime floor, so this tranche contributes nine new exact DOM passes. The
  evidence floor is therefore 316/1056 exact passes (221 DOM, 2 libxml, 93
  SimpleXML), with 740 entries still lacking passing evidence. The same
  interface-only materializer now covers `RecursiveIterator::hasChildren()`
  and `getChildren()` for `SimpleXMLElement`/`SimpleXMLIterator`, in addition
  to all five `Iterator` methods. The seven-slot vtable regression passes and
  the final rebuilt replay in
  `/private/tmp/simplexml-bug42259-interface-final-20260801.json` makes
  `bug42259.phpt` pass exactly without SIGSEGV. The evidence floor is therefore
  317/1056 exact passes (221 DOM, 2 libxml, 94 SimpleXML), with 739 entries
  still lacking passing evidence. The four frozen EntityReference timeouts are
  also eliminated. Native descendant traversal no longer follows an
  `XML_ENTITY_REF_NODE` pseudo-child into its DTD declaration; stale reads
  resynchronize through `xmlGetDocEntity`, while clone/import/adopt neutralize
  declaration links around libxml reconciliation and rebind them afterward.
  Both focused bridge regressions and the bridge build pass. The final replay
  in `/private/tmp/dom-entity-reference-timeouts-final-20260801.json` records
  zero timeout and zero crash for `DOMDocument_adoptNode.phpt`,
  `entity_reference_stale_03.phpt`, `clone_entity_reference.phpt`, and
  `import_entity_reference.phpt`; all four now terminate at independent
  debug-projection, nullable-typing, warning, or serialization mismatches, so
  this safety correction deliberately adds no exact PASS to the evidence floor.
  Native `DOMNameSpaceNode` debug projection now travels through a boxed
  `Mixed` adapter and exposes php-src's virtual node properties instead of the
  compiler's declared-property `uninitialized(...)` placeholders. The rebuilt
  exact replay in `/private/tmp/dom-debug-projection-second-20260802.json`
  makes `xpath_domnamespacenode.phpt` pass and makes the entire namespace-node
  projection of `gh12616_3.phpt` exact; that second PHPT remains failed only on
  the independent `removeAttributeNS()` namespace-removal behavior. The
  evidence floor is therefore 318/1056 exact passes (222 DOM, 2 libxml, 94
  SimpleXML), with 738 entries still lacking passing evidence. Manual calls to
  all nine legacy DOM constructors now rebind the receiver's native resource
  in place while preserving PHP object identity, subclass discriminator,
  canonical-cache integrity, and any old attached graph retained elsewhere.
  Internal constructors are generically typed as returning `null`; their EIR
  and assembly pass on all three supported targets. Empty document fragments
  return `false` before adoption as php-src does. The heap-clean E2E and exact
  replay `/private/tmp/dom-manual-constructors-final-v6.json` pass 9/9, raising
  the evidence floor to 327/1056 exact passes (231 DOM, 2 libxml, 94
  SimpleXML), with 729 entries still lacking passing evidence. The unsuppressed
  warning for an empty fragment remains a separate diagnostic-plus-union ABI
  gap; the nine frozen PHPTs suppress it with `@` and are exact.
  Runtime-named access to nine virtual DOM node properties now coalesces
  candidate classes and dispatches the native property opcode instead of
  reading an uninitialized declared slot. Three-target EIR/codegen and the
  heap-debug regression pass. In
  `/private/tmp/dom-nullable-stale-cluster-20260802.json`, `stale_03` now exits
  normally with its three correct `NULL` values rather than timing out/fataling;
  it and `stale_01` remain blocked only by the independent
  `Dom\\NodeList::$length` debug projection, while `stale_02` was already in
  the baseline floor. This nullable tranche therefore adds no distinct exact
  PASS yet. Hidden native debug projections now cover seven DOM collection
  families, reading live `length` (and token-list `value`) through the bridge
  instead of declared uninitialized slots. The focused exact replay in
  `/private/tmp/dom-collection-debug-priority-20260802.json` makes
  `entity_reference_stale_01.phpt` pass. `stale_03.phpt` now has the exact
  `Dom\\NodeList` value but retains the separate object-identity-number
  mismatch. The evidence floor is therefore 328/1056 exact passes (232 DOM, 2
  libxml, 94 SimpleXML), with 728 entries still lacking passing evidence.
  Legacy `removeAttributeNS()` now recognizes a local namespace declaration in
  `nsDef`, moves it into libxml's document old-namespace storage, and clears
  matching namespace pointers throughout the subtree. The bridge regression
  and explicit bridge build pass; the namespace removal portion of
  `gh12616_3.phpt` is exact in
  `/private/tmp/dom-remove-attribute-ns-after-exact-20260802.json`. That PHPT
  remains failed only on its distinct XPath `DOMNameSpaceNode` wrapper identity
  and reuse ordering. The three-case replay
  `/private/tmp/dom-remove-attribute-ns-cluster-20260802.json` also makes
  `gh12616_1.phpt` pass exactly; `_2` has exact XML/XPath semantics and remains
  failed only on a `DOMNodeList` identity number. The evidence floor is
  therefore 329/1056 exact passes (233 DOM, 2 libxml, 94 SimpleXML), with 727
  entries still lacking passing evidence. Finally,
  bridge diagnostics accept callsite metadata for legacy and modern
  `appendChild()` opcodes even when the result contract is `DOMNode|false`.
  A non-suppressed empty-fragment probe now emits php-src's exact Warning and
  line, returns `false`, preserves `ownerDocument === null`, and the `@` form
  remains silent; the original 9/9 constructor corpus stays exact in
  `/private/tmp/dom-manual-constructors-diagnostic-final.json`.
  After those green replays, the host Xcode GUI state changed externally and
  began rejecting its developer tools with an unaccepted-license diagnostic.
  No privileged license acceptance was attempted. The installed standalone
  Command Line Tools remain usable through
  `DEVELOPER_DIR=/Library/Developer/CommandLineTools`; the final diagnostic
  relink and replay pass through that non-privileged path. Subsequent macOS
  builds and PHPT replays must carry this environment override until the GUI
  Xcode license is accepted outside this campaign.
  Because this campaign widened the checker contract of the registered
  `count()` builtin, the generated-builtin documentation workflow has also
  been rerun through the mandatory exporter. It rendered 485 registry entries
  and 944 pages; `audit_builtins.py` reports 459 catalog/user pages, 448
  internals pages, and zero errors, while `validate_site_compat.py` validates
  all 961 generated pages. The generated ownership metadata and shifted source
  links are retained with the implementation checkpoint.
  Native-wrapper JSON descriptors now omit only handler-backed virtual
  properties, preventing null/sentinel fields from reaching the string encoder
  while retaining real storage declared by PHP subclasses. The descriptor unit
  and heap-clean E2E preserve `{}` for a base `DOMDocument` and
  `{"label":"x"}` for its subclass. `DOMDocument_json_encode.phpt` passes in
  the focused security replay, raising the evidence floor to 330/1056 exact
  passes (234 DOM, 2 libxml, 94 SimpleXML), with 726 entries still lacking
  passing evidence. The remaining serialization, clone, and canonicalization
  crashes are tracked as distinct roots rather than being masked by this JSON
  filter.
  XPath nodesets now materialize their PHP wrappers eagerly in php-src order
  before constructing the `DOMNodeList`: ordinary members are canonicalized
  directly, while namespace entries first materialize and strongly retain their
  parent wrapper. Explicit bridge/compiler builds and the namespace bridge test
  pass. This makes `gh12616_2.phpt` exact, raising the evidence floor to
  331/1056 exact passes (235 DOM, 2 libxml, 94 SimpleXML), with 725 entries still
  lacking passing evidence. `gh12616_3.phpt` is now content- and
  projection-exact and differs only on two object IDs caused by the temporary
  XPath collection/iterator release order; that final lifetime ordering is the
  remaining root in this cluster. The temporary DOM collection ownership is
  now transferred and released immediately after `IterStart`, matching php-src
  rather than retaining it through `IterEnd`; the eager wrapper IDs are exactly
  `[4,5,8,9,5]`. The five-header heap-debug regression passes, the full bridge
  suite passes 150/150, and the final two-case exact replay
  `/private/tmp/dom-gh12616-2-3-final-rerun.json` passes 2/2. This adds
  `gh12616_3.phpt` to the floor, now 332/1056 exact passes (236 DOM, 2 libxml,
  94 SimpleXML), with 724 entries still lacking passing evidence. The auxiliary
  strong-owner finalizer is also emitted correctly on both AArch64 and x86_64;
  the x86_64 path now reloads the owner before clearing its field.
  Runtime dispatch from statically broad `DOMNode`/`Dom\\Node`/`Dom\\ParentNode`
  receivers now validates compatible concrete native descendants and dispatches
  through their class IDs instead of rejecting concrete-only methods at the
  checker. Variadic `before()`/`after()` operations retain their flat arity,
  and already boxed nullable DOM unions are no longer boxed a second time. The
  checker/EIR/assembly regression passes all three targets and
  `bug80600.phpt` passes exactly, raising the evidence floor to 333/1056 exact
  passes (237 DOM, 2 libxml, 94 SimpleXML), with 723 entries still lacking
  passing evidence. `bug80602_3.phpt` now executes both mutations with exact
  XML and remains blocked only on the generic 27-property `DOMElement`
  `var_dump()` projection; modern `setAttributeNS()`/`saveXML()` cases likewise
  advance to narrower parameter-type blockers rather than undefined methods.
  Serialization now carries a dense php-src-compatible DOM deny mode: normal,
  node fallback (which permits a subclass hook), and hard deny before any
  subclass hook. A base DOM wrapper therefore throws the exact non-serializable
  exception instead of walking virtual storage, a `DOMDocument` subclass with
  `__serialize()` succeeds, and `DOMXPath` descendants remain hard-denied.
  Both target helpers, two heap-clean E2E regressions, and the JSON regression
  pass. The exact replay `/private/tmp/dom-not-serializable-final.json` makes
  `not_serializable.phpt` pass, raising the evidence floor to 334/1056 exact
  passes (238 DOM, 2 libxml, 94 SimpleXML), with 722 entries still lacking
  passing evidence.
  The delegated return-compatibility tranche first preserved a
  SimpleXML-specific allowance for a descendant `__toString()` override that
  omits the explicit `string` return type, including the inherited `Stringable`
  contract. The frozen runner
  replay using
  `/private/tmp/php-dom-oracle-build-8.5.8/sapi/cli/php` confirms
  `ext/simplexml/tests/bug62328.phpt` with exact expectation, exit, and
  file-delta parity (`/private/tmp/simplexml-bug62328-exact.json`). This raises
  the evidence floor to 335/1056 exact passes (238 DOM, 2 libxml, 95
  SimpleXML), with 721 entries still lacking passing evidence. The provisional
  PHP 8.5.8 CLI linked to libxml2 2.9.13 remains excluded from ledger credit.
  Parent differential audit then disproved the tranche's initial negative
  guard: PHP 8.5.8 also permits an ordinary userland `__toString()` override to
  omit its explicit return type. A corrective Terra tranche now separates the
  global php-src magic-method rule from the pre-existing SimpleXML
  `current()` plus `#[ReturnTypeWillChange]` exception, including a userland
  intermediate-class non-bypass regression. The PHPT credit remains exact, but
  this implementation tranche is not integration-ready until the focused
  differential and compiler tests finish green. The corrective module now
  passes 6/6, the separate SimpleXML foreach/current regression passes, and
  `cargo check --lib` passes. Its audit also found and corrected the distinct
  PHP rule that a standalone `__toString(): never` declaration is valid while
  an untyped child may not widen that inherited `never` contract. The final
  post-correction exact `bug62328.phpt` replay remains pending on the serialized
  shared build cache, so no additional ledger credit is taken.
  A subsequent read-only audit validates those four contracts and confirms the
  `current()`/`#[ReturnTypeWillChange]` exception remains independently scoped,
  but finds one remaining global parity gap: implicit `Stringable` registration
  still tests only an explicit `string` return, so a valid untyped
  `__toString()` class may fail `instanceof Stringable` or a `Stringable`
  parameter. The statically-object native-extension argument path may likewise
  reject its scalar-coerced return even though the Mixed path already supports
  it. A delegated implementation tranche must close both paths, add
  `instanceof`/parameter/DOM-argument regressions, correct the ambiguous test
  preamble, and then rerun the focused contracts plus exact `bug62328.phpt`.
  A separate Luna append-dimension tranche was rejected before integration:
  it represented empty `[]` with `ExprKind::Null`/`VALUE_NULL`, which conflates
  PHP's append operation with the observably different explicit `[null]`
  dimension. A corrective Terra tranche owns a distinct AST/EIR/bridge append
  marker, both-form regressions, removal of its temporary probe, and exact
  replays of `bug35785.phpt`, `bug41582.phpt`, and `bug55098.phpt`; none of
  those PHPTs is credited yet.
  The remaining `bug80602_3.phpt` mismatch is now isolated to debug projection
  lifetime: without `var_dump()` the second wrapper identity matches PHP,
  while `__debugInfo()` leaves two extra wrapper identities live (oracle
  sequence `#4,#2,#1`, Elephc `#4,#6,#2`). A Terra tranche is adding the
  dedicated red identity-order regression before changing the demonstrated
  refcount/transfer cause. No PHPT credit is recorded for this test yet.
  The first sequential-release patch makes the dedicated identity regression
  pass (`#3` then `#2`) but fails the mandatory heap gate with
  `allocs=321`, `frees=309`, `live_blocks=12`, and `live_bytes=592`.
  `bug80602_3.phpt` therefore has not been rerun or credited; the tranche is
  back in read-only leak diagnosis before any further implementation.
  The completed static audit excludes the top-level `var_dump()` Mixed release,
  the Mixed-to-hash/object deep free, and bridge-union materialization as an
  evidenced cause. `DOMElement::__debugInfo()` creates 13
  `__dom_debug_*` object-property temporaries, and the 12-block/592-byte delta
  is most compatible with their wrapper allocations; however local-slot
  analysis says the epilogue should clean them. No speculative production
  patch is authorized: the next delegated tranche must inspect emitted
  assembly/instrument allocations, prove which of the 13 slots or return paths
  skips cleanup, then preserve the identity order while reaching heap zero.
  A 2026-08-04 read-only evidence audit found that the three checked-in frozen
  ledgers still contain 1,056 `pending` entries (868 DOM, 32 libxml, 156
  SimpleXML), and that every temporary JSON report cited for the historical
  335-pass floor has disappeared from `/tmp` and `/private/tmp`. The arithmetic
  history remains internally consistent (238 DOM, 2 libxml, 95 SimpleXML), but
  it is no longer live-verifiable evidence and must not be used as a final
  completion claim. Each pass will be reconstructed with the pinned oracle,
  exact expectation/exit/filesystem-delta evidence, a persistent report, and a
  reviewed ledger transition before final credit.
  The same audit confirms that `tools/php-dom/phpt_runner.py` already captures
  raw/hash outputs, exit codes, SKIPIF/CLEAN execution, and filesystem deltas,
  but it cannot yet establish the final durable gate. A delegated tooling
  tranche must add source-lock, ledger, oracle, Elephc build, PHPT, and resolved
  expectation digests; persist the nine complete component-by-target reports
  under `tests/php_dom/evidence/php-8.5.8/<build-commit>/`; generate a canonical
  manifest; link every direct ledger entry to its case evidence; and add a
  verifier that accepts only 868/32/156 exact `passed` results on each of the
  three supported targets. `oracle_skip`, filters, limits, translated cases,
  XFAILs, and evidence-free `not-applicable` entries cannot satisfy the final
  gate. The evidence commit may follow the clean tested build commit only when
  their diff is restricted to evidence, ledgers, and documentation.
  The ledger entries themselves remain
  `pending` until this evidence is reviewed and linked. PHP
  tokenization of every frozen FILE section finds seven genuine
  close-tag/inline-HTML programs (five DOM and two SimpleXML), all covered by
  the terminal lowering rather than a harness exclusion.
- [~] Implement the shared SimpleXML foundation before its 39 generated
  routes: one `Rc<DocumentGraph>`, document-wide `None | Legacy | Modern`
  claim, unified None/Element/Child/AttrList view state, fresh external
  handles, separately retained iterator identity, wrapper-local XPath
  registrations, and finalizer routing are integrated. Nine foundation tests,
  five existing XML/HTML/context tests, and `cargo check -p elephc-dom` pass.
  All six loader/constructor/import keys are now explicit. Bridge regressions lock
  fresh SimpleXML wrappers, non-destructive iterator element/attribute import,
  documentless/node-type diagnostics, family claims/conflicts, and legacy to
  modern canonical DOM identity. All 21 method keys are now explicit, and the
  strict opcode-4096 compiler materializer alone accepts legacy `DOMElement`/
  `DOMAttr` kinds returned by `Dom\\import_simplexml()`; three-target assembly
  plus exact legacy/modern identity and family-conflict E2E regressions pass.
  Compiler result materialization now also accepts exact `VALUE_MAP` namespace
  maps and `VALUE_ARRAY` XPath node sets, including fresh base/subclass
  `SimpleXMLElement` wrappers. The 21 method implementations now include
  recursive `__debugInfo()` materialization (attributes, duplicate children,
  scalar text, fresh nested subclass wrappers), inline-text-only
  `__toString()` parity, exact empty namespace/XPath selector behavior,
  unsupported XPath filtering, re-entrant stream-wrapper
  `asXML()`/`saveXML()` writes including zero-byte success, php-src subnode
  QName serialization, namespaced `addChild()` views, and nullable-bool
  deprecation order. `cargo test -p elephc-dom` passes 132/132 on 2026-08-01,
  and the focused public method, serialization, and recursive debug bridge
  regressions pass.
  The compiler/runtime now synthesizes a callable native
  `SimpleXMLElement::__debugInfo` EIR entry plus per-concrete-class adapters;
  `var_dump()` uses one owned dynamic projection for both count and recursive
  body, while `print_r()` renders native and user-override projections on both
  supported architectures. The no-`__debugInfo()` path now retains ordinary
  declared-property output, including visibility suffixes, omission of
  uninitialized typed properties, and nested arrays; its exact PHP 8.5 E2E plus
  both architecture-emission tests pass. The recursive flat-ABI materializer
  now follows non-contiguous child ranges by absolute index; exact native
  SimpleXML `var_dump()`/`print_r()` recursive output and heap-debug pass, as
  does the fresh-wrapper recursive `__debugInfo()` regression. The complete
  four-test debug-output module passes, including runtime subclass overrides,
  the covered recursion cases, nullable-return deprecations, and single-shot
  dispatch. A subsequent independent php-src audit found seven runtime defects.
  Six are now closed with exact regressions: `print_r()` invokes `__debugInfo()`
  before installing the recursion guard; physical parent/child private slots
  retain their own visibility metadata; declared rendering appends dynamic
  properties in insertion order; invalid scalar `__debugInfo()` returns are
  process-fatal; supported indirect SPL debug methods are forced into EIR; and
  top-level debug-projection hash keys demangle PHP's protected/private NUL
  form without changing nested associative-array keys. `SplObjectStorage` and
  `MultipleIterator` now expose php-src's private `storage` array of
  `obj`/`inf` pairs, while `SplDoublyLinkedList` exposes exact private keys.
  The recursive flat-ABI result materializer now validates the entire tree
  before allocating PHP values: null pointers, flags, scalar payloads, exact
  byte/range coverage, aliases, overlaps, gaps, orphan/trailing values,
  backedges/cycles, depth, handle/callable identities, and the strict
  `LibXMLError`/`Dom\\NamespaceInfo` schemas are rejected deterministically.
  Its five validator tests, recursive E2E tests, and macOS AArch64, Linux
  AArch64, and Linux x86_64 generation pass; AArch64 stack alignment and SysV
  `r12`/`r13` preservation are locked by assembly assertions. Docker execution
  remains unavailable because the daemon blocks at image inspection, so it is
  not counted as Linux runtime evidence. Exact nullable-return source location
  remains an active gate. The SPL heap projection gate is closed: `SplHeap`
  and `SplPriorityQueue` persist the physical binary-heap layout through
  insert/extract, `__debugInfo()` reads it without invoking user `compare()` or
  mutating the receiver, and seven focused max/min/priority/custom-comparator
  histories pass with exact private NUL-mangled keys.
  The reproducible generated-route residue has fallen from 12 SimpleXML object
  handlers to zero: all twelve strict-C/native bridge routes are explicit.
  `object-handler:simplexml::get_iterator` returns the original receiver handle
  and concrete wrapper discriminator without allocating a fresh view, reusing
  the eager strong iterator-current owner. The generic
  `count(SimpleXMLElement)` linker failure was traced to selecting a nonexistent
  `_method_SimpleXMLElement_count` symbol instead of the SimpleXML handler
  opcode and is included in the current correction. Strong eager iterator identity is now
  integrated with a hidden owned wrapper at native offset `+80`, movement epoch
  at `+88`, clear-before-decref finalization, six-pointer GC metadata, clone
  reset/deep-copy, and destructive destructor re-entry cancellation. Four exact
  E2E identity/re-entry/clone/heap-debug tests plus bridge/runtime checks pass.
  The narrower `$this->getName()` destructor-reentry crash is now traced to a
  null bodyless-method vtable slot: user descendants were omitted from the
  singular native-wrapper bridge selection even though their native handle
  remained valid. Descendant-aware lowering now routes inherited bodyless
  SimpleXML methods to opcode 4437 while preserving concrete PHP overrides;
  exact next/rewind and heap-debug replays now pass, and the Linux x86_64 EIR /
  request-assembly regression proves opcode 4437 while a concrete override is
  not hijacked. The iterator re-entry divergence and the separate dynamic
  `Iterator`/mixed-string null-vtable defect are closed. Read-only php-src and
  oracle audits isolate the next cause families. The clone bridge already
  matches root/non-root deep-copy semantics; checker and EIR lowering now
  accept only one exact fallible SimpleXML wrapper class, emit the exact
  scalar-failure `TypeError`, and balance temporary ownership. The separate
  foreach-source retain leak is also closed: `IterEnd` now releases the static
  Object/Interface retain on normal, break, return, throw, and nested-break
  exits without over-releasing borrowed sources; eight heap-debug/assembly
  regressions and the exact `006.phpt`/`019.phpt` replay pass. Dimension/nested writes
  now pass a strict checker gate that rejects `Mixed`, `Bool`, and multi-object
  unions. The bridge now autovivifies a missing selected child before an
  attribute write, preserves its namespace, reuses the node on repeated writes,
  and passes 11 focused handler tests. `028.phpt` emitted an unchanged XML tree
  in the pre-lowering report `/tmp/simplexml-028-autovivification-report.json`;
  the active EIR path is now corrected and its exact passing report is
  `/tmp/simplexml-028-after-eir-autovivification.json`. Numeric `BP_VAR_W`
  parent reads and gap-warning call-site decoration are now closed by the exact
  `027.phpt` replay; the full byte-oriented `__HALT_COMPILER()` source-unit
  architecture and exact `012.phpt` replay are also closed. A
  same-URI/two-prefix edge also remains: php-src
  preserves the exact selected `mynode->ns` pointer while the bridge currently
  reselects by URI. `023.phpt` was separated from that write cluster and now passes on
  the rebuilt compiler without a dedicated correction. The complete handler merge,
  recursive flat-ABI debug materializer, exact native debug-output replay,
  forced cycle-collection evidence, full frozen PHPT replay, and target matrix
  remain open.

Checkpoint validation after the `origin/main` integration:

- [x] `cargo build -p elephc-dom`
- [x] `cargo build -p elephc`
- [x] 22 focused binary bridge/linker tests
- [x] 14 focused library bridge tests plus the exact heterogeneous
  `DOMNodeList` runtime-dispatch regression
- [x] all 75 `elephc-dom` bridge/handle/native tests
- [x] all four focused DTD codegen tests with `ELEPHC_PHP_CHECK=1`
- [x] both focused boxed-`Mixed` DOM dispatch regressions
- [x] assembly-comment alignment and `git diff --check`

Active implementation tranche:

- [~] Complete legacy `DOMXPath` and modern `Dom\XPath`.
  - [x] Construction, retained canonical `document`, mutable
    `registerNodeNamespaces`, persistent `registerNamespace()`, scalar and
    node-set `evaluate()`/`query()` results, context-node namespace defaults,
    exact omission across positional/named/static-spread arguments,
    family-specific evaluation failures, wrong-document errors, modern
    namespace-axis rejection, cloning, and `quote()`.
  - [x] Re-entrant custom-namespace PHP callback registration/invocation: both
    `registerPhpFunctionNS()` routes now validate and retain custom namespace
    callables, preserve replacement/clone/release ownership, invoke PHP
    outside the bridge context borrow, preserve scalar and node-set argument
    order and types, return XPath strings/booleans/DOM nodes, reject other
    objects with php-src's exact `TypeError`, defer callback-result release
    until the context borrow is dropped, and rethrow the exact callback
    `Throwable`.
  - [x] Both reserved-namespace `registerPhpFunctions()` routes, including
    none/all/restricted modes, retained allow-list entries, exact aliases,
    scalar weak coercions, function-name/closure/instance/static callable
    forms, node-set conversion, clone/replacement ownership, and exact primary
    php-src errors.
  - [x] Legacy namespace-axis `DOMNameSpaceNode` wrappers and their complete
    property/lifecycle behavior.
  - [x] Modern `Dom\NamespaceInfo` in-scope and descendant namespace snapshots,
    including php-src ordering/shadowing rules and ordinary readonly value
    objects.
  - [x] Dynamic runtime-spread omission parity is integrated at `16472ee281`,
    including multiple dynamic spreads, explicit third-argument presence,
    source-order materialization, and balanced temporary ownership.
  - [x] Runtime duplicate named parameters from spreads now evaluate both
    sources and throw PHP's catchable `Error` with the exact message instead
    of terminating through an uncatchable compiler fatal (`b5a7fa95bb`).
    Uncaught duplicates also take the dedicated `ThrowError` path and preserve
    PHP's non-zero exit plus concrete `Fatal error: Uncaught Error: ...`
    diagnostic (`448f1f3234`).
  - [x] Catchable wrong-context `TypeError` preflight now runs before request
    allocation for both XPath families. Runtime `Mixed` values preserve exact
    nullable native-wrapper contracts, object class names, literal boolean
    names, parameter positions, and Stringable evaluation order. Owning call
    arguments and object-to-string results remain cleanup-visible across later
    coercion or validation throws; focused exact-output and heap-debug
    regressions pass.
  - [x] The same signature-driven validator now covers modern typed DOM
    properties and both reflected and php-src-manual variadic node-or-string
    mutation contracts. Legacy `DOMElement::append()`-family diagnostics use
    `DOMNode|string` despite their intentionally untyped Reflection surface;
    modern routes use `Dom\Node|string`, and Stringable objects are rejected
    exactly like PHP for these unions.
  - [~] The complete frozen XPath PHPT/differential replay stays open and may
    expose further cases beyond the focused exact regressions.

Delegated implementation status:

- [~] GLM 5.2, Codex CLI worktree `dom-simplexml-functions-glm`, branch
  `feat/dom-simplexml-functions-glm`: production routes now exist for the five
  SimpleXML/DOM interoperability functions plus
  `SimpleXMLElement::__construct()`, with family ownership transfer,
  canonical handles, subclass discrimination, parse diagnostics, and nine
  focused bridge tests. All 99 `elephc-dom` library tests passed, but an
  independent php-src 8.5.8 audit rejected the tranche as non-integrable:
  SimpleXML lifecycle was absent, views were incorrectly canonicalized,
  import node types/messages and document-wide family conversion diverged,
  namespace filters were ignored, constructor re-entry was rejected, and
  class/options/file diagnostics were incomplete. The same GLM session is now
  correcting all 11 findings in place against exact oracle tests. No
  integration credit is claimed yet.
- [~] Kimi K2.7, Codex CLI worktree `dom-simplexml-methods-kimi`, branch
  `feat/dom-simplexml-methods-kimi`: the 21 named SimpleXMLElement methods,
  native libxml helpers, and iterator state are in active implementation. The
  first bridge prototype reached a compilable intermediate state, but used a
  pointer-to-handle cache that violates PHP identity (`$s->x === $s->x` must
  be false). The same Kimi session is removing all SimpleXML canonicalization,
  retaining only shared native document/node identity, and completing routes,
  compiler result materialization, warnings, lifecycle, and tests.
- [~] MiniMax M3, Codex CLI worktree `dom-simplexml-handlers-minimax`, branch
  `feat/dom-simplexml-handlers-minimax`: the isolated WIP audit now provides a
  corrected nine-test reference nucleus for all 12 routes, including fresh
  filtered views, attribute wrappers, count/casts/compare, `isset` versus
  `empty`, namespace filters, mutations/unsets, detached-node liveness, and a
  corrected length-safe libxml2 write boundary. `cargo check -p elephc-dom`
  and `git diff --check` pass in the isolated tree. Its obsolete duplicate
  object model, raw handle insertion, fake `get_iterator`, unused compiler
  helpers, and lost subclass discriminator are explicitly rejected. A fresh
  authoritative integration is active and remains gated on BP_VAR/compiler
  lowering, strong iterator-zval identity, property autovivification, exact
  coercions/diagnostics, overridden `__toString()`/`count()`, mutation-list
  invalidation, by-reference foreach, and the frozen PHPTs.

- [~] Kimi K2.7, Codex CLI worktree `padawan-dom-iterators-kimi`, branch
  `padawan/dom-iterators-kimi`: shared live `InternalIterator` behavior for
  legacy/modern node lists and named-node maps, modern HTML collections and
  token lists. Commits `aa4bd7afd9` and `757169ecaa` are integrated, and
  `f0ec2c74ae` reserves all six native routes for compiler-resident lowering.
  The independently corrected implementation at `fef80e8d54` uses
  family-specific typed owners, a stable collection-kind discriminator, and
  PHP's sticky exhausted state. All three focused DOM iterator cases and the
  `SplFixedArray` non-regression now pass. Kimi's attempted corrective rerun
  was rate-limited before it could act, so this tranche has implementation
  credit but not final reviewer-consensus credit.
- [x] Kimi-assisted Codex worktree `padawan-dom-mixed-dispatch-kimi`, branch
  `padawan/dom-mixed-dispatch-kimi`: bodyless native-wrapper methods selected
  from boxed `Mixed` receivers now reuse the generated DOM operation ABI,
  while compiler-resident synthetic methods such as `getIterator()` retain
  their PHP bodies. The audited implementation is integrated at `f1ec2c8292`;
  this is implementation credit only, not final Ollama-consensus credit.
- [x] MiniMax M3, Codex CLI worktree `padawan-dom-routes-minimax`, branch
  `padawan/dom-routes-minimax`: DTD named-node maps plus entity/notation
  wrappers, properties, dimensions, iteration, lifecycle, metadata, and DTD
  mutation behavior. Commits `e9412087d4` and `b4c20bc571` are integrated.
  The implementation preserves nullable map semantics and evaluation order,
  matches php-src's legacy/modern entity/notation quirks, and fixes
  heterogeneous native-property union dispatch so runtime classes select
  their own opcode. All four focused DTD cases, both mixed-wrapper
  regressions, the 75 bridge tests, and the warning-free compiler build pass.
  This is implementation credit only, not final Ollama-consensus credit.
- [~] GLM-assisted `registerNodeClass()` work. A post-merge audit rejected the
  original 2,363-line `padawan-dom-register-node-glm` WIP wholesale because 14
  of 25 files overlap the authoritative branch and properties, nullsafe,
  callable dispatch, lifecycle, and post-P0 validation remain incomplete.
  The bounded bridge-only port from `padawan-dom-register-port-glm` is
  integrated at `a61f13effa`: compiler class-metadata ABI, case-insensitive
  validation, per-document classmaps, replacement/reset, family isolation,
  legacy `true` versus modern `void`, mapped class-id markers, both explicit
  routes, and 15 focused bridge tests. The current uncommitted compiler/runtime
  tranche emits the locked 40-byte class metadata rows, installs them in every
  target ABI, recognizes transitive native-wrapper descendants, emits their
  payload/GC/finalizer/name metadata, consumes mapped high-bit class IDs, and
  materializes legacy grandchildren plus modern subclasses with canonical
  wrapper identity. LLDB exposed one lifecycle defect: the mapped-class
  comparison register overwrote the live DOM context before allocation. The
  dispatch now preserves the context/handle pair across that comparison and
  both end-to-end materialization regressions pass. Follow-up heap-debug
  coverage found and fixed descendant receivers plus direct descendant
  construction being rejected by the native-wrapper allocator. Registered
  wrappers now preserve identity, leave and re-enter the weak cache, call
  inherited DOM properties, and finish with a clean heap summary. Runtime
  callable/nullsafe dispatch, clone/finalization breadth, XPath interaction,
  and full target validation remain open, so this is not yet complete
  compatibility credit.

## Quantified progress

The generated PHP 8.5.8 registry currently contains 603 distinct operation
keys. In the current uncommitted SimpleXML checkpoint:

- 591 keys have an explicit bridge route;
- the 12 SimpleXML object handlers are the exact remaining unimplemented keys;
- 184 DOM-matching end-to-end codegen tests are present; the DTD and mixed
  wrapper filters pass against the PHP oracle;
- all 90 bridge/handle/host/native tests pass, including 15 focused
  `registerNodeClass()` tests;
- the current compiler/runtime register-node-class working tree additionally
  passes both legacy-grandchild and modern-subclass materialization tests; its
  final implementation commit is not yet frozen;
- 171 branch commits exist above the synchronized upstream baseline through the
  implementation commit.
- all 1,056 frozen upstream PHPT entries remain explicitly `pending`
  (868 DOM, 32 libxml, and 156 SimpleXML); no ledger-closure claim is made.

An explicit route is routing progress, not proof of full PHP acceptance. Final
completion still requires the frozen differential oracle and complete PHPT
ledgers.

## Completed implementation tranches

- [x] Reproducible native bridge foundation and source verification.
- [x] Stable ABI requests/results, re-entrant result frames, context-local
  state, callable retain/release callbacks, and structured failure channels.
- [x] Generation-checked document, node, collection, implementation, and
  token-list handles.
- [x] Authoritative libxml document graphs, detached-root retention, wrapper
  cache identity, and family-aware concrete wrapper materialization.
- [x] Legacy `DOMDocument` construction, in-memory `loadXML()`, and `saveXML()`.
- [x] Modern empty XML/HTML construction and XML string construction.
- [x] Core document/node factories for elements, attributes, text, CDATA,
  comments, fragments, processing instructions, and entity references.
- [x] Legacy `DOMDocumentFragment::appendXML()` and modern
  `Dom\DocumentFragment::appendXml()` now parse through php-src's balanced XML
  chunk algorithm, sanitize and restore the same libxml parser globals, append
  the complete node list atomically, preserve embedded-NUL truncation, reject
  unbound legacy fragments with code 7, and publish suppressible warnings plus
  context-local `LibXMLError` state. Legacy, modern XML, and modern HTML
  wrappers share the XML parser exactly like PHP. The tests deliberately
  follow the frozen libxml2 2.15.3 oracle for empty chunks and error count.
- [x] All ten generated document-validation routes are explicit:
  `validate()`, `schemaValidate()`, `schemaValidateSource()`,
  `relaxNGValidate()`, and `relaxNGValidateSource()` across the reflected
  legacy/modern families. The bridge uses pinned libxml2 DTD, W3C XML Schema,
  and Relax NG engines; honors `LIBXML_SCHEMA_CREATE`; temporarily maps modern
  namespace-declaration attributes back into validation namespace definitions
  and restores the observable graph; records context-local `LibXMLError`
  objects; preserves normal/internal/suppressed diagnostic policy; resolves
  local grammars and PHP stream-wrapper grammars plus relative
  imports/external references; runs host callbacks outside the bridge
  `RefCell` borrow; and rethrows the exact loader `Throwable`. Core route and
  validity behavior is complete, including php-src's target-specific
  overlong-local-path preflight and canonical modern `relaxNgValidate*`
  diagnostic spelling. Non-internal validation also follows php-src's generic
  callback buffering, including parser header, source-line, caret, resource,
  and final invalid-grammar warning fan-out, while internal mode retains
  structured `LibXMLError` records.
- [x] Legacy `DOMDocument::xinclude()` and modern
  `Dom\XMLDocument::xinclude()` execute through pinned libxml2 with
  `XML_PARSE_NOXINCNODE`, exact family-specific flag/result/exception
  channels, suppressible generic warnings versus internal structured errors,
  re-entrant PHP stream and external-loader callbacks, and exact pending
  `Throwable` identity. The bridge snapshots every XInclude-owned subtree
  before mutation and converts destroyed node, attribute, text, token-list,
  live-collection, and selector-snapshot references into safe invalid-state or
  null/empty views before any retained wrapper can dereference freed native
  memory. Unrelated wrappers remain live, invalidated wrapper release is
  heap-clean, and the internal-extension exception ABI now carries the exact
  message length independently from appended diagnostics on both supported
  assembly backends.
- [x] Legacy/modern `C14N()` and `C14NFile()` canonicalize through pinned
  libxml2 with family-specific detached-node behavior, modern namespace
  relinking, XPath filtering, namespace-prefix validation, exclusive and
  comment options, exact file and registered-wrapper output semantics, and
  ordered warning/notice publication. Internal-extension requests now carry
  bounded nested runtime array/hash/object values losslessly on both assembly
  backends. A C14N wrapper regression also exposed and fixed borrowed string
  concatenation scratch storage escaping through static-property assignment;
  the EIR lowering now acquires persistent storage before that write.
- [~] Legacy `DOMXPath` and modern `Dom\XPath` now retain authoritative
  document graphs and persistent namespace state; expose their virtual
  properties; evaluate pinned-libxml XPath node sets, booleans, doubles, and
  strings; publish static `NodeList` snapshots; preserve the property-based
  context-namespace default when the reflected third argument is omitted;
  distinguish legacy false from modern `Error`; reject wrong-document and
  modern namespace-axis results exactly; clone state independently; and
  implement php-src's `quote()` algorithm. Compiler lowering now preserves
  native-wrapper alternatives inside mixed scalar unions and materializes
  floating-point internal-extension results on AArch64 and x86_64. Dynamic
  runtime spreads now preserve whether the reflected third argument was
  explicitly supplied, pre-materialize multiple spreads in source order, and
  balance every temporary owner. Custom
  namespace callbacks now cross a target-aware, exception-contained host ABI
  with balanced callable ownership, source-order scalar and node-set
  arguments, re-entrant nested DOM access, XPath string/boolean/DOM-node
  results, php-src's exact unsupported-object `TypeError`, deferred callback
  result release, and exact `Throwable` identity. A focused compiler
  regression also preserves object-valued `Mixed` elements returned directly
  from untyped-array closures instead of applying the syntactic `int`
  fallback. Reserved `php:function()`/`php:functionString()` callbacks now
  preserve none/all/restricted registration modes, exact aliases, retained
  maps across mode changes, clone/replacement ownership, node-set
  string-versus-array conversion, builtins, user functions, closures,
  instance/static callable arrays, PHP weak scalar name coercions, and the
  primary exact php-src errors. The compiler prepares nested callable arrays
  once while their object identity is still live, lets the bridge retain its
  own descriptors, and releases the temporary descriptor plan after the
  native call on both supported assembly backends. Legacy namespace-axis
  results now materialize retained, canonical `DOMNameSpaceNode` wrappers with
  complete property, clone, and serialization-hook behavior. Exact
  undefined-prefix diagnostics and dynamic context-argument `TypeError`
  behavior now pass focused regressions; only the global frozen PHPT replay
  keeps XPath acceptance open.
- [x] Standalone legacy node constructors implemented so far.
- [x] Node navigation, canonical identity, metadata, parent-element views,
  namespace lookup, equality, containment, and document-position comparison.
- [x] Core child mutations plus variadic `append()`, `prepend()`,
  `replaceChildren()`, `before()`, `after()`, `replaceWith()`, and `remove()`.
- [x] Legacy and modern `insertAdjacentElement()`/`insertAdjacentText()`,
  including string-backed enum ABI marshalling, legacy case-insensitive
  positions and loose-mode warnings, cross-document auto-adoption, detached
  receivers, document hierarchy messages, and wrapper identity.
- [x] Modern `Dom\Element::rename()` and `Dom\Attr::rename()`, including
  in-place wrapper identity, QName/namespace validation, duplicate-attribute
  rejection, HTML-class and template restrictions, and transient XML
  namespace-prefix generation (`ns1`) without mutating observable DOM
  declarations.
- [x] Modern `Dom\Element::$innerHTML`, `$outerHTML`, and
  `insertAdjacentHTML()`, including XML context-fragment namespaces and atomic
  syntax failures, HTML5 context parsing and invalid-UTF-8 replacement,
  template content behavior, detached/document-parent errors, retained wrapper
  identity, XML well-formed serialization exceptions, nullable-wrapper
  property-set lowering, and PHP-compatible suppression of conflicting default
  namespace declarations.
- [x] Modern `Dom\Element::$substitutedNodeValue` plus native PHP object and
  `cloneNode()` handling for documents/nodes, including deep-versus-shallow
  document ownership, retained wrapper configuration, PHP 8.5 template
  fragment reset on clone/import/cross-document adoption, template-aware
  detached cleanup, and XML serialization that hides ordinary template
  children behind the private content fragment.
- [x] Import/adopt behavior implemented so far, including subtree wrapper
  re-homing and document ID lookup.
- [x] Attribute reads/writes, namespace-aware attributes, attribute-node
  identity, XML ID marking, and modern namespace declaration materialization.
- [x] Character-data operations, `Text::splitText()`, `wholeText`, and
  document normalization implemented so far.
- [x] Live child/element/attribute collections and indexed/named lookup
  implemented so far.
- [x] Modern `querySelector()`, `querySelectorAll()`, `matches()`, and
  `closest()` through php-src's pinned libxml2-adapted Lexbor selector engine,
  including static node-list snapshots, canonical member identity, HTML
  limited/full quirks mode, exact syntax/unsupported/`:blank` failures, and
  document/fragment/element/parent-node routes.
- [x] Modern document/element `getElementsByClassName()` as live
  `HTMLCollection` descriptors, including ordered-set parsing, duplicate and
  empty tokens, document-versus-element roots, `namedItem()`, mutation
  visibility, exact NUL validation, unnamespaced XML attributes, and PHP's
  distinct standards/limited-quirks/full-quirks casing rules.
- [x] Core modern `Dom\TokenList`/`classList` behavior: cached same-object
  identity; live raw-value synchronization; ordered-set parsing,
  canonicalization, and duplicate handling; `add()`, `remove()`, `toggle()`,
  `replace()`, `contains()`, `item()`, `count()`, `length`, and `value`;
  validation-before-mutation; exact `DOMException`, `ValueError`, and
  `TypeError` channels; detached-element retention; and token-list graph
  re-homing across `adoptNode()`. Internal optional-variadic signatures now
  preserve PHP's zero-argument arity.
- [x] Native-to-PHP host callbacks now install target-aware `setjmp`
  sentinels before entering callable retain/release helpers, restore exception
  and diagnostic chains on both normal and exceptional exits, return a
  pointer-free pending-Throwable result through Rust, release bridge call
  state, and rethrow the exact original PHP object through the caller's
  `try/catch`. A destructor throwing while an external entity loader is
  released matches the frozen PHP 8.5.8 oracle.
- [x] Legacy document configuration flags, URI/version/encoding/standalone,
  formatting, doctype, implementation, and document-element properties
  implemented so far.
- [x] `DOMImplementation::hasFeature()`, legacy/modern
  `createDocumentType()`, legacy/modern `createDocument()`, and modern
  `createHTMLDocument()`, including exact doctype identity/adoption policy.
- [x] Modern document `characterSet`/`charset`/`inputEncoding`, `head`, `body`,
  and `title` properties, including WHATWG encoding canonicalization, body
  auto-adoption, HTML/SVG title algorithms, detached-wrapper preservation, and
  prefixed SVG serialization.
- [x] Namespace-derived modern element wrapper classes and HTML-document
  `nodeName`/`tagName` casing, including the concrete
  `HTMLDocument::createElement()` return type.
- [x] Modern `Dom\Element::getInScopeNamespaces()` and
  `getDescendantNamespaces()`, including php-src's exact self/descendant
  traversal order, namespace shadowing, default-namespace undeclaration
  omission, duplicate-value preservation, readonly `Dom\NamespaceInfo`
  materialization, fresh value-object identity, and precise
  `array<Dom\NamespaceInfo>` typing for XML and HTML element wrappers.
- [x] Legacy XPath namespace-axis `DOMNameSpaceNode` compatibility, including
  fake-node ownership shared by snapshot slots and materialized wrappers,
  canonical repeated `item()` identity, survival after list/document release,
  independent cloning, exact default/prefixed declaration fields, all ten
  virtual properties, inherited-union property dispatch, and exact
  `__sleep()`/`__wakeup()` rejection.
- [x] Initial modern HTML XML serialization rule for empty HTML non-void
  elements, without persistent synthetic children.
- [x] Modern `HTMLDocument::createFromString()` HTML5 parsing through pinned
  Lexbor, including encoding detection/override, namespace-derived wrappers,
  accepted option masks, and `Dom\HTML_NO_DEFAULT_NS`.
- [x] Modern `HTMLDocument::saveHtml()` document/subtree serialization and
  family-aware `saveXml()` empty-element output, including exact HTML void and
  SVG/XML distinctions covered by the frozen PHP oracle.
- [x] Legacy `DOMDocument::loadHTML()` HTML4 parsing and `saveHTML()`
  document/subtree serialization, including legacy doctype, entity, void,
  foreign-element, and empty-source contracts.
- [x] Plain/local-`file://` parsing and saving for legacy XML/HTML plus modern
  XML/HTML, including canonical file document URIs, byte counts, all nine
  generated file-method routes, and PHP's base `Exception` file-open channel.
- [x] Modern XML string/file option-mask and override-encoding validation plus
  PHP's default UTF-8 document metadata when the source has no declaration.
- [x] Modern HTML memory/file output transcoding through pinned Lexbor,
  including replacement behavior for code points unavailable in the document
  encoding.
- [x] Parsed modern HTML template contents stored as private document
  fragments, hidden from ordinary child/text APIs, and preserved through
  nested HTML/XML document and subtree serialization, including transient
  subtree namespace declarations.
- [x] XML document/subtree/file serialization honors
  `LIBXML_NOXMLDECL` and `LIBXML_NOEMPTYTAG` with legacy/modern family
  differences, formatted output, modern document newline handling, and HTML
  void-element closing rules matched to PHP.
- [x] Modern HTML5 tokenizer/tree diagnostics match php-src for string and
  file inputs, including Unicode columns, 4096-byte chunk ordering, internal
  `LibXMLError` state, `LIBXML_NOERROR`/`LIBXML_HTML_NOIMPLIED`, normal runtime
  warnings, and `@` suppression.
- [x] libxml2 XML and legacy-HTML parse diagnostics flow through the same
  retained ABI for string and file inputs, including canonical file names,
  recovery results, `LIBXML_NOERROR`/`LIBXML_NOWARNING`, internal-error
  collection, and warning-before-exception ordering.
- [x] Compiler pre-scan materialization of all registry-provided DOM/libxml
  constant values, including the high-bit `Dom\HTML_NO_DEFAULT_NS` flag.
- [x] libxml internal-error mode, error collection, last-error, clearing,
  entity-loader state, stream context, and host callable ownership implemented
  so far.
- [x] External-loader replacement releases its previous PHP callable only
  after dropping the bridge context's mutable borrow; nested DOM calls from
  destructors can therefore re-enter the ABI, while a destructor Throwable is
  still transported and rethrown as the original PHP object.
- [x] Signature-driven internal-extension marshalling materializes every frozen
  PHP callable representation used by DOM: function-name strings, indexed and
  numerically keyed associative instance/static callable arrays, invokable
  objects, closures, and existing descriptors. Receiver-bound descriptors have
  balanced temporary ownership, remain callable after bridge round-trips, and
  can be invoked through the returned loader value.
- [x] `DOMDocument::loadXML()` invokes the configured external-entity loader
  without retaining the bridge context borrow, with php-src's exact nullable
  public/system IDs and four-key context array. Null and string results follow
  libxml2's resource-loader contract; callback-returned stream resources remain
  leased across incremental parser reads and close exactly once. Nested DOM
  calls are safe, and a callback Throwable is rethrown as the same PHP object.
- [x] External-loader streams returned by registered PHP stream wrappers now
  use PHP's 8192-byte userspace read buffer while serving libxml2's smaller
  reads from a re-entrant Rust-side lease buffer. Host results larger than the
  requested PHP read are rejected, pending bytes do not trigger duplicate PHP
  callbacks, and the parser's final release invokes `stream_close()` exactly
  once.
- [x] Synthetic wrapper resource destruction now routes the exact
  `0x40000000..0x40000100` descriptor range through the wrapper close
  dispatcher on ARM64 and x86_64. Explicit and ownership-driven close clear the
  handle slot, release its wrapper-object owner, run the final destructor, and
  leave heap-debug clean without misrouting phar or already-closed sentinels.
- [x] Computed userspace-wrapper read values now have balanced ownership:
  `fread()` is classified as returning fresh bytes, and the immutable `$this`
  receiver is no longer treated as a provisional owning local while lowering
  borrowed property reads. Repeated direct-property and `substr()` method
  returns, generic wrapper reads, and DOM external-loader reads are heap-debug
  clean.
- [x] Direct legacy and modern DOM file reads now open registered PHP stream
  wrappers through the no-unwind host ABI, perform PHP's quiet
  `url_stat($path, 2)` probe, use mode `rb`, request 8192-byte chunks, and
  invoke `stream_close()` exactly once. `ClassName::class` registrations now
  participate in dynamic class materialization.
- [x] Stream contexts now have stable per-resource identities and independently
  owned option hashes. `libxml_set_streams_context()` selection reaches both
  temporary `url_stat` and live stream wrapper objects; legacy and modern DOM
  loads observe the exact selected context instead of a process-wide last
  value. Registry teardown and injected untyped-property cells are heap-debug
  clean on ARM64, and equivalent x86_64 runtime emission is present.
- [x] Indexed runtime callable invokers now release by-value string, Mixed,
  container, object, and callable argument owners on both success and
  exceptional escape before exact rethrow. Native `LibXMLError` value objects
  are included in runtime class/GC metadata, so their persisted message/file
  properties are reclaimed with the containing error array.
- [x] Userspace stream-wrapper vtables now target per-class, per-method,
  target-aware ABI adapters. Untyped by-value callback parameters follow
  PHP's boxed `Mixed` method ABI while canonical runtime strings, integers,
  resources, and the by-reference opened-path cell retain their fixed wrapper
  contracts. Caller-owned temporary boxes, return aliases, string-return
  ownership, missing-parameter materialization, register overflow, and stack
  cleanup are balanced; the frozen PHP 8.5.8 oracle and heap-debug output are
  exact for direct DOM reads, with an independent x86_64 assembly-generation
  regression.
- [x] Declared fixed userspace-wrapper callback parameters now apply the
  verified weak scalar conversions for the canonical runtime string/integer
  inputs, including persisted integer-to-string owners, PHP string
  truthiness, numeric-string preflight, and literal defaults beyond the
  runtime callback arity. Statically incompatible fixed parameters throw the
  exact catchable `TypeError`; conversion owners and return aliases remain
  balanced, and independent x86_64 adapter-emission coverage accompanies the
  AArch64 end-to-end regressions.
- [x] Declared union, nullable, and intersection wrapper callback parameters
  now preserve PHP's exact-type-first rule before applying the php-src weak
  scalar preference. Runtime `Mixed` tags are checked against their concrete
  PHP value kind, class/interface intersections use compiled runtime metadata,
  numeric strings select `int` versus `float` at signed 64-bit boundaries,
  bool fallbacks remain ordered after numeric candidates, and exact
  `TypeError` diagnostics name the concrete runtime type. Fresh composite
  boxes, converted strings, rejected native metadata values, and exceptional
  exits have balanced ownership. AArch64 end-to-end coverage and independent
  x86_64 adapter/classifier emission checks are both present. Remaining
  context-sensitive warning parity stays grouped with the explicit warning
  follow-up below.
- [x] Lossy weak `float`/float-string to `int` conversions at userspace-wrapper
  callback boundaries now emit php-src's exact suppressible deprecation before
  the callback. Integer-valued forms, invalid and out-of-range sources, exact
  union arms, and `@` stay silent. Static strings and boxed `Mixed` values
  preserve their source registers across diagnostic formatting; float
  formatting restores the shared concat cursor. AArch64 end-to-end tests cover
  static and dynamic callbacks with clean heap ownership, while independent
  Linux x86_64 adapter generation covers float and float-string paths.
- [x] Userspace-wrapper callback parameters beyond the fixed runtime contract
  now evaluate their defaults through zero-argument synthetic EIR thunks
  instead of a callback-specific literal interpreter. Global constants, class
  constants, `self::class`, arrays, and `new` object expressions therefore use
  ordinary PHP lowering on every supported backend; owned string, `Mixed`,
  array, associative-array, object, iterable, and callable results are
  released on normal and exceptional exits. The thunks remain hidden from PHP
  callable metadata. Global constant defaults are type-checked only after
  top-level constants are registered, preserving both exact mismatched-type
  and undefined-constant diagnostics. AArch64 end-to-end and independent
  x86_64 adapter-emission regressions are present.
- [x] Callback signatures now reject required parameters beyond the internal
  callback contract with the exact catchable `ArgumentCountError`, including
  the standard `ArgumentCountError extends TypeError` hierarchy. By-value
  variadics pack every remaining callback argument in source order, enforce
  declared element types with PHP argument positions, and transfer their
  element owners into a balanced array. The special `stream_open()`
  opened-path cell is forwarded only to by-reference declarations; ordinary
  and variadic by-value parameters observe its initial `null` value.
- [x] Userspace-wrapper method returns now cross the fixed vtable ABI through a
  slot-aware normalizer instead of leaking the method's declared
  representation into runtime dispatch. Owned scalar, string, container,
  resource, union, and `Mixed` results are first transferred to a boxed value;
  php-src truthiness, string/int conversion, exact int/bool/resource checks,
  stat-array validation, directory boolean termination, false write failure,
  and ignored close returns are then normalized with balanced ownership.
  Declared `mixed` direct DOM callbacks now cover open/read/eof/close/url-stat
  end to end and leave heap-debug clean.
- [x] Native userspace-wrapper EOF handling now follows php-src's callback
  order and context split. Every successful `stream_read()` is followed by one
  lenient `stream_eof()` truthiness probe; internal drains no longer pre-call
  `feof()`. Public `feof()` requires an exact boolean, emits PHP's suppressible
  concrete-type warning for invalid results, coerces that failure to `true`,
  and observes sticky EOF state until a successful seek or handle reuse clears
  it. A hidden per-call adapter word keeps strict and lenient modes re-entrant
  on every supported backend. The read result remains owned across the
  post-read callback and is released before an escaping `Throwable` is
  rethrown through a native exception boundary.
- [x] `elephc-magician` userspace-wrapper EOF handling now shares the same
  public-versus-post-read split. Its `feof()` path accepts only an exact bool,
  emits the concrete PHP result type for invalid values, returns and caches
  `true` for invalid or missing callbacks, preserves false as non-sticky, and
  clears sticky EOF after a successful seek. Direct, aggregate, and
  line-oriented reads no longer pre-test EOF: each completed `stream_read()`
  instead performs one lenient truthiness probe, continues drains until an
  empty chunk, invokes reads even after a cached EOF, and propagates a
  post-read `Throwable` through the original eval catch path.
- [x] Fixed userspace-wrapper values requested through declared by-reference
  parameters now emit PHP's ordered suppressible warning, receive isolated
  16-byte heap cells, own cloned/persisted payloads, tolerate same-type
  mutation, and release both payload and cell after return. The genuine
  `stream_open()` opened-path cell is passed through without a warning; its
  initial `null` is validated before entry, so a non-nullable `string
  &$openedPath` throws the exact catchable `TypeError`. Warning, cell, and
  cleanup generation has independent x86_64 coverage, while direct DOM reads
  validate AArch64 behavior and clean heap ownership.
- [x] Untyped fixed userspace-wrapper references now retain the concrete PHP
  type of every runtime value through boxed `Mixed` cells. Cross-type
  assignments transfer their acquired payload owner into the replacement box
  without an extra retain; fixed by-value sources still receive isolated
  borrowed copies. The runtime-owned `stream_open()` opened-path cell releases
  and clears any callback-assigned string, callable, object, or container
  payload on both normal and exceptional exits without freeing the cell
  itself. Direct DOM reads verify exact `string|string|integer|NULL` inputs,
  warnings only for arguments 1-3, cross-type mutation, and clean heap
  ownership; independent x86_64 adapter generation covers dynamic cell boxing
  and borrowed-cell payload cleanup.
- [x] By-reference variadic userspace-wrapper parameters now materialize one
  PHP alias marker per callback value. Ordinary fixed runtime inputs receive
  isolated owning `Mixed` cells, while `stream_open()` forwards its genuine
  opened-path cell without a warning. Array copies retain element aliases;
  writes release the previous string, callable, resource, object, container,
  or boxed payload before transferring the replacement owner. Declared simple
  and composite element types are enforced at callback entry, but subsequent
  writes through the references may change type exactly like PHP. Outer
  arrays, marker boxes, temporary cells, and borrowed opened-path payloads
  unwind cleanly on normal and exceptional exits on AArch64, with independent
  x86_64 adapter generation and target-aware marker-write lowering.
- [x] Userspace stream-wrapper adapters now place callback argument owners
  behind a target-aware exception boundary whenever an outer PHP/DOM handler
  exists. Exceptional exits release partially materialized boxed values,
  reference cells, path-dispatch receiver objects, and definite owning method
  receiver temporaries before restoring and rethrowing the exact original
  `Throwable`. Direct uncaught callback-signature failures deliberately skip
  the local boundary and retain their precise PHP-style fatal diagnostic.
- [x] All five legacy and modern DOM file serializers now open registered PHP
  stream wrappers in `wb` mode without a read-side `url_stat()` probe and with
  the selected libxml stream context. Native serialization is snapshotted
  before re-entrant PHP callbacks begin; partial writes are retried, while
  exact `false`, zero, partial-then-zero, arbitrary negative integers, and
  over-reported byte counts follow php-src's distinct return contracts.
  Over-reported writes emit the exact wrapper-class warning through the
  suppressible diagnostic channel and clamp to the requested length. Flush
  and close ordering, ignored false flush results, exact callback Throwable
  identity, one-close lifetime, re-entry, and heap ownership are covered, with
  independent AArch64 and x86_64 runtime emission.
- [x] Direct legacy and modern DOM wrapper reads now stop before
  `stream_open()` when the quiet `url_stat($path, 2)` probe is missing or
  returns false, exactly like php-src. Missing `url_stat`, missing
  `stream_open`, and false `stream_open` outcomes cross the strict host ABI
  with stable wrapper-class metadata and produce the exact method-scoped,
  credential-scrubbed warning text through the suppressible PHP diagnostic
  channel. Silent false-stat behavior, modern base-`Exception` results,
  malformed-host rejection, and clean temporary wrapper/stat ownership are
  covered on AArch64; independent Linux x86_64 runtime-emission assertions
  cover the same failure channels. All four legacy/modern registered-wrapper
  reads now use a three-phase prepare/execute/publish pipeline: validated
  inputs are copied under the DOM context borrow, user callbacks and native
  parsing run without that borrow, then errors and the parsed graph are
  published after reacquisition. Missing-stat wrapper destructors can
  therefore perform nested DOM reads with exact warning suppression and
  callback order, while an uncommitted native document is freed if context
  reacquisition fails. Modern HTML wrapper reads also retain the unsigned
  high-bit `Dom\HTML_NO_DEFAULT_NS` option.

## Open surface ledger

All 603 generated operation keys are explicit. The remaining routed-but-
incomplete semantic surfaces are grouped as follows; route closure is an
inventory gate and does not by itself prove behavior.

- [~] File and stream parsing/saving: all reflected file routes work for plain
  paths, local `file://` URLs, direct registered-wrapper reads, and registered
  wrapper writes for all five serializers; the
  no-unwind host exception transport is complete for callable ownership,
  release-time re-entry, callable-array marshalling, controlled
  external-loader invocation/re-entry, and incremental built-in and registered
  custom stream-resource reads, including PHP's userspace-wrapper buffering,
  exact close lifetime, and selected libxml stream-context propagation.
  Native direct-wrapper `stream_eof` ordering, strict diagnostics, sticky
  caching, suppression, and exceptional cleanup are complete, and
  `elephc-magician` now matches the same callback order, exact-bool split,
  sticky cache, missing-method result, drain behavior, and exception
  propagation. Partial writes, write-result failures, oversized-write
  warnings, flush/close ordering, Throwable transport, wrapper-backed saving,
  quiet-stat short-circuiting, exact missing-method/call-failed open warnings,
  credential scrubbing, and `@` suppression are complete. Weak-conversion
  deprecations are complete for static and boxed callback sources.
- [x] CSS selectors and live class-name queries.
- [~] `Dom\TokenList` and `classList`: the complete core method/property
  surface is routed and its compiler-resident private constructor is explicitly
  classified at the bridge boundary; indexed dimension handlers, iteration,
  `getIterator()`, serialization, and clone failures remain grouped with the
  shared wrapper lifecycle/iterator tranche.
- [x] XPath legacy/modern focused surface: the constructor, property, namespace, evaluation,
  query, quoting, custom-namespace callback, and both reserved
  `registerPhpFunctions()` routes are explicit and focused behavior is
  verified. Custom and reserved callbacks cover scalar/node-set arguments,
  scalar/DOM-node results, aliases, all/restricted modes, callable arrays,
  unsupported-object errors, re-entry, clone/replacement ownership, and
  callback-result lifetime. Legacy namespace-axis wrappers and dynamic
  runtime-spread omission plus catchable duplicate named-parameter errors are
  complete. Undefined-prefix suffixes and wrong-context `TypeError` behavior
  pass their exact focused regressions. The complete frozen XPath PHPT replay
  remains part of the global ledger gate and may reopen this surface.
- [x] Document validation: all ten generated DTD, XML Schema, and Relax NG
  routes plus the identified php-src validity, namespace, stream/import,
  argument, warning-fan-out, and internal-error semantics are implemented.
  The complete frozen PHPT ledger remains a global final gate.
- [x] XInclude: both reflected legacy/modern routes, native execution,
  destroyed-wrapper and derived-view invalidation, re-entrant loading, exact
  result/error mapping, and focused tests are complete for the frozen surface.
- [x] C14N/C14NFile: all four reflected legacy/modern routes, nested
  XPath/namespace-prefix option marshalling, native canonicalization,
  file/registered-stream output, exact validation/diagnostics, detached-node
  behavior, and focused tests are complete. The frozen PHPT ledger remains a
  global final gate.
- [x] DTD entity/notation maps and their declaration-node properties:
  legacy and modern map dimensions, `item()`/`getNamedItem()` lookup,
  live iteration, identity and ownership, read-only dimension failures,
  entity/notation identifiers, inherited node metadata, text-content quirks,
  namespace lookups, cloning, and the complete append/insert/replace/remove
  result matrix match PHP 8.5.8.
- [x] Namespace compatibility: modern `Dom\NamespaceInfo` snapshots and legacy
  XPath namespace-axis `DOMNameSpaceNode` wrappers, identity, properties,
  lifetime, cloning, and serialization hooks are complete for the focused
  oracle surface.
- [x] Compiler-resident iterator bodies and routes are PHP-oracle verified for
  legacy/modern node lists and named-node maps plus modern HTML collections
  and token lists, including modern DTD named-node maps. Live additions before
  exhaustion, sticky end state, key behavior, repeated `rewind()`, DTD member
  names/identity, and the unchanged `SplFixedArray` path are covered.
- [x] Dynamic calls on native DOM wrappers whose static receiver is `mixed`
  materialize and dispatch bodyless native methods directly through the
  generated bridge opcode. The exact chained
  `$list->getIterator()->current()->getAttribute("id")` regression passes
  without an `instanceof` workaround, while synthetic PHP method bodies
  retain ordinary mixed-method dispatch.
- [~] Native wrapper lifecycle: DOM document/node object-clone routing and the
  inherited legacy/modern node `__sleep()`/`__wakeup()` rejection paths work
  with exact concrete built-in class names and base `Exception` messages;
  compiler-resident private construction for `Dom\Node`, `Dom\NamespaceInfo`,
  and `Dom\TokenList` is explicitly classified with PHP visibility precedence;
  node-class registration and remaining wrapper families stay open.
- [~] Complete SimpleXML surface plus DOM/SimpleXML imports and family rules:
  all 27 generated function/method routes and all 12 object-handler routes are explicit, with loaders/imports,
  document-wide family claims, fresh views, namespace maps, XPath wrapper
  arrays, and focused identity tests integrated in the working tree. The exact
  generated-route inventory now at zero. Cast, comparison, counting, iterator,
  dimension, and property handlers are explicit; their nine direct bridge
  regressions plus both compiler E2E read/write/unset and cast/compare probes
  pass, while dynamic names, deep autovivification, runtime overrides, nullable
  cast/compare, by-reference foreach, and full PHPT mutation semantics remain
  broader completion gates.
- [x] Remaining reflected readonly/property paths for entity and notation,
  including php-src 8.5's uninitialized modern notation identifiers.
  Legacy mutable `DOMNode::$prefix` now performs php-src-compatible namespace
  rebinding, including loose-mode warnings and forced conflicts.
  `DOMException`/`LibXMLError` are explicitly classified as compiler-resident
  ordinary PHP properties; legacy `schemaTypeInfo` and deprecated document
  `config` reads are complete.

The reproducible command for refreshing this count is:

```bash
comm -23 \
  <(rg -o '"(method|property-get|property-set|function|internal|object-handler):[^"]+"' \
      crates/elephc-dom/src/generated/opcodes.rs | sort -u) \
  <(rg -o '"(method|property-get|property-set|function|internal|object-handler):[^"]+"' \
      crates/elephc-dom/src/dispatch/routes.rs \
      crates/elephc-dom/src/dispatch/routes/properties.rs \
      --no-filename | sort -u)
```

## Immediate sequence

1. Close the recursive flat-ABI result materializer: exact native
   `__debugInfo()` output, subclass-aware wrapper ownership, complete
   validation before allocation, heap-debug, and all supported target
   lowerings. The first independent no-`eval()` upstream replay,
   `ext/simplexml/tests/001.phpt`, now passes the frozen PHP 8.5.8 expectation,
   exit, and file-delta gates after preserving the raw libxml names `comment`
   and `test` as empty nested wrappers. Complete malformed-result validation,
   heap-debug, and supported-target evidence remain open.
2. Close the independent debug-renderer audit: move the `print_r()` recursion
   guard to php-src's post-`__debugInfo()` position, make declared descriptors
   physical-slot based, enumerate dynamic properties, reject invalid debug
   return tags exactly, force supported SPL debug bodies, and lock diagnostics
   with focused PHP 8.5.8 oracle tests. The descendant bodyless-method
   destructor re-entry gate is already green on macOS and x86_64.
3. Audit the 12 now-explicit object handlers end to end, including compiler
   BP_VAR modes, autovivification, by-reference foreach, mutation invalidation,
   dynamic property names, runtime `count()`/`__toString()` overrides,
   namespaces, subclasses, nullable cast/compare, and exact diagnostics.
4. Preserve the now-zero generated route count, then drive all 1,056 frozen
   PHPT entries to zero semantic exclusions with recorded oracle evidence.
5. Add final examples/docs, run supported-target evidence, and freeze the exact
   implementation commit.
6. Send that exact commit independently to GLM 5.2, Kimi K2.7, and Kimi K3;
   integrate findings serially and repeat until all three return only the exact
   implementation lock.
7. Update the checkpoint PR to the locked commit and use CI for the complete
   macOS ARM64/Linux ARM64/Linux x86_64 matrix before marking it ready.

## Validation ledger

Current register-node-class/CI follow-up (`a61f13effa` through published
checkpoint `d039d1a4c7`, plus the current working tree):

- The bounded register-node-class bridge port is integrated without the
  unvalidated compiler-dispatch WIP. At published checkpoint `d039d1a4c7`, the
  inventory was 603 total, 564 explicit, and 39 SimpleXML-only gaps; the current
  methods/loaders/handlers checkpoint is now 603 total and 603 explicit; route
  parity is closed while full handler behavior and PHPT parity remain open.
- `CARGO_INCREMENTAL=0 cargo test -p elephc-dom register_node_class`: 15
  passed, 0 failed.
- `CARGO_INCREMENTAL=0 cargo test -p elephc-dom`: all 90 bridge tests and
  doc-tests passed.
- `CARGO_INCREMENTAL=0 cargo build -p elephc`: passed warning-free after
  deleting only reconstructible Cargo incremental caches to recover disk
  space.
- Mixed same-name dispatch now emits the selected internal method's exact
  catchable object-argument `TypeError`. Stringable objects retain PHP weak
  string coercion; each `__toString()` runs once in argument order, the flat
  request is sized from the prepared bytes, and copied temporaries are
  released before the native call.
- `ELEPHC_PHP_CHECK=1 CARGO_INCREMENTAL=0 cargo test --test codegen_tests
  dom_mixed_method_ -- --nocapture`: 3 passed, including three prepared
  Stringable arguments and the exact non-Stringable error.
- `CARGO_INCREMENTAL=0 cargo test --test web_tests
  web_router_interface_handler_survives_repeated_requests -- --nocapture`: 1
  passed across 25 requests.
- The six `session_start()` option regressions were bisected to
  `cb1f285d417564fb379dfbb3024742445d0d559e`, whose reachable-join logic
  discarded conditional assignments by restoring the complete pre-split
  logical type map.
- Reachable `if` arms now merge local types variable-by-variable. `Mixed`
  absorbs narrower members, equal members remain stable, boolean unions drop
  redundant `false`, and only the single reachable arm survives after a
  terminating branch. A follow-up audit required two additional invariants:
  the merged type also widens the physical EIR slot, and a local present on
  only one reachable arm is removed from the logical map rather than treated
  as definitely initialized. Both are implemented.
- `CARGO_INCREMENTAL=0 cargo test --test codegen_tests
  branch_assignment -- --nocapture`: all 4 focused branch-merge cases pass
  against PHP, covering `null|string`, `null|int`, `int|false`, and a recursive
  three-leaf `if`/`elseif`/`else`.
- `CARGO_INCREMENTAL=0 cargo test --lib
  merge_local_type_environments_drops_unilateral_keys -- --nocapture`: 1
  passed.
- `CARGO_INCREMENTAL=0 cargo test --lib
  preserves_dom_narrowing_across_consecutive_terminating_guards --
  --nocapture`: 1 passed.
- Six focused `web_session_tests` now pass: conditional `save_path`, strict
  mode, `referer_check`, trans-SID rewriting, cookie-less GET/POST SID
  transport, and invalid-path/INI handling.
- `CARGO_INCREMENTAL=0 cargo build -p elephc`: passed warning-free with the
  complete CI/session working tree.
- The CI archive audit confirmed that all three archive jobs build
  `elephc-dom`, Nextest includes `libelephc_dom.a`, and Linux archive jobs have
  immediate plus image-level CMake coverage. Its only finding, an inaccurate
  php-src comparison in the `PATH_MAX` comment, is corrected.
- CI run `30549092295` confirms all three archive jobs and all three
  managed-native-package jobs pass at `8f26eba93a`; both Linux web shards also
  pass. The remaining failures were reduced to stale generated `fread` docs,
  `Array(Mixed)` branch storage degrading to `Mixed`, an unconditional DOM
  finalizer symbol in non-DOM programs, two obsolete opcode-based DOM EIR
  assertions, and 13 DOM/libxml/SimpleXML functions whose eval implementation
  has not landed. The first four are fixed; the 13 functions are explicitly
  tracked by the existing static-only parity contract instead of silently
  disappearing from compiler metadata.
- The current register-node-class compiler/runtime tranche emits and installs
  compiler class metadata on macOS AArch64, Linux AArch64, and Linux x86_64;
  treats transitive native-wrapper subclasses as five-slot hidden wrappers in
  payload, GC, destructor, hidden-offset, and class-name tables; and decodes
  mapped high-bit class IDs into registered wrapper allocations.
- LLDB stopped the original legacy-grandchild failure inside
  `elephc_dom_call`: the finalizer received context
  `0x8000000000000051` instead of the live bridge context because the mapped
  class-ID comparison reused `x10/r10`. The registered dispatch now preserves
  and restores the context/handle pair around those comparisons on both
  assembly backends.
- `XDG_CACHE_HOME=/tmp/elephc-dom-register-cache CARGO_INCREMENTAL=0 cargo
  test --test codegen_tests dom_register_node_class_materializes --
  --nocapture`: both legacy-grandchild and modern-subclass end-to-end tests
  pass, including finalization at teardown.
- `XDG_CACHE_HOME=/tmp/elephc-dom-register-cache CARGO_INCREMENTAL=0 cargo
  test --test codegen_tests
  dom_register_node_class_reinserts_released_wrappers_heap_cleanly --
  --nocapture`: passed with canonical identity, weak-cache removal/reinsertion,
  direct descendant construction, inherited `tagName`, and a clean heap-debug
  summary.
- The first large post-register XPath replay exposed AArch64 `cbz/cbnz`'s
  plus-or-minus-one-megabyte range. Truthiness helpers now invert the short
  condition over a local numeric label and use the 128 MiB `b` form for the
  actual target. The focused ABI test passes, and
  `namespace_node_wrapper_lifetime_and_identity` now assembles and passes.
- `CARGO_INCREMENTAL=0 cargo test --test builtin_parity_tests -- --nocapture`:
  all 8 parity tests pass with the 13 not-yet-eval-capable DOM functions
  explicitly documented as static-only.
- `python3 scripts/docs/audit_builtins.py`: 0 errors across 459 catalog
  builtins; `validate_site_compat.py`: all 961 generated pages validated.

Latest verified DTD declaration tranche (`b4c20bc571`):

- All DTD entity/notation collection routes are explicit. Integer dimensions
  use `item()`, string dimensions use `getNamedItem()`, nullable chained map
  expressions retain their concrete map type, and read-only write/append/unset
  diagnostics name the exact legacy or modern map class.
- Legacy and modern entity/notation wrappers preserve node names, identifiers,
  node metadata, namespace lookups, line/path behavior, text-content
  mutability, clone results, and php-src 8.5's uninitialized
  `Dom\Notation::$publicId`/`$systemId` behavior.
- The complete 16-case inherited tree-mutation matrix matches the PHP 8.5.6
  oracle: family/type-specific false results and exact `DOMException`
  codes/messages for append, insert, replace, and remove.
- Heterogeneous native-wrapper property unions no longer select the first
  member's opcode. They defer to target-aware runtime class dispatch, while
  userland shadow properties retain ordinary object storage.
- `ELEPHC_PHP_CHECK=1 CARGO_INCREMENTAL=0 cargo test --test codegen_tests
  dom_dtd_ -- --nocapture`: 4 passed.
- `CARGO_INCREMENTAL=0 cargo test --test codegen_tests dom_mixed_ --
  --nocapture`: 2 passed.
- `CARGO_INCREMENTAL=0 cargo test -p elephc-dom -- --nocapture`: 75 passed.
- `CARGO_INCREMENTAL=0 cargo build -p elephc`, assembly-comment checks, and
  `git diff --check`: passed.

Latest verified mixed native-wrapper dispatch tranche (`f1ec2c8292`):

- Boxed `Mixed` instance-method dispatch detects bodyless internal-extension
  methods only after the runtime class-id branch has selected the concrete
  candidate, then reuses the ordinary generated opcode request encoder with
  the already-unboxed receiver.
- Result flags retain native wrapper and value-object materialization for
  object unions. Compiler-resident synthetic methods remain on their emitted
  PHP bodies and ordinary userland mixed-method dispatch is unchanged.
- `ELEPHC_PHP_CHECK=1 CARGO_INCREMENTAL=0 cargo test --test codegen_tests
  dom_mixed_iterator_current_dispatches_native_wrapper_method -- --nocapture`:
  1 passed.
- `CARGO_INCREMENTAL=0 cargo test --test codegen_tests
  mixed_receiver_method -- --nocapture`: 6 passed.
- `CARGO_INCREMENTAL=0 cargo build`, assembly-comment checks, and
  `git diff --check`: passed.

Latest verified catchable spread-overwrite tranche (`448f1f3234`):

- Every runtime duplicate named-parameter guard used by positional, named, and
  multiple associative spreads constructs PHP's ordinary `Error` object with
  the exact `Named parameter $... overwrites previous argument` message.
- Spread expressions are conservatively marked `may_throw`, preventing the
  AST optimizer from deleting a surrounding `try/catch` before EIR inserts
  runtime unpack validation.
- Source evaluation order remains PHP-compatible: both spread and later named
  expressions run before the duplicate `Error` is thrown.
- Uncaught duplicates use the static `ThrowError` instruction rather than the
  generic throw helper, so they print the exact `Uncaught Error` diagnostic
  and exit non-zero; caught duplicates continue through the ordinary PHP
  handler stack.
- `CARGO_INCREMENTAL=0 cargo test --lib
  test_effect_analysis_treats_spread_as_potentially_throwing`: 1 passed.
- `ELEPHC_PHP_CHECK=1 CARGO_INCREMENTAL=0 cargo test --test codegen_tests
  named_arguments_after_ -- --nocapture`: 13 passed.
- `ELEPHC_PHP_CHECK=1 CARGO_INCREMENTAL=0 cargo test --test codegen_tests
  xpath_dynamic_ -- --nocapture`: 4 passed.
- `CARGO_INCREMENTAL=0 cargo build`, assembly-comment checks, and
  `git diff --check`: passed.

Latest verified InternalIterator tranche (`fef80e8d54`):

- Compiler-resident `InternalIterator` bodies use a stable private collection
  discriminator and typed owner properties for legacy/modern node lists and
  named-node maps plus modern HTML collections and token lists.
- PHP's live-collection boundary is preserved: additions before exhaustion
  remain visible, while an empty or exhausted iterator stays invalid after
  later mutations; end keys and repeated `next()` behavior remain stable.
- Fresh repeated `rewind()` is accepted, while `rewind()` after advancement
  throws PHP's exact `Error`. The unrelated `SplFixedArray` iterator path is
  unchanged.
- `ELEPHC_PHP_CHECK=1 CARGO_INCREMENTAL=0 cargo test --test codegen_tests
  dom_internal_iterator -- --nocapture`: 3 passed.
- `ELEPHC_PHP_CHECK=1 CARGO_INCREMENTAL=0 cargo test --test codegen_tests
  spl_fixed_array_internal_iterator_unchanged -- --nocapture`: 1 passed.
- `CARGO_INCREMENTAL=0 cargo build`: passed warning-free.
- `./scripts/check_asm_comments.py src/ir_lower/program.rs
  src/types/checker/builtin_spl_classes/containers.rs`: passed.
- `git diff --check`: passed before commit.

Latest verified XPath dynamic-spread tranche (`16472ee281`):

- A private fourth bridge argument records whether the reflected third XPath
  argument was explicitly supplied, including positional, named, and dynamic
  spread calls.
- Multiple dynamic spreads are evaluated once in source order before ABI
  reordering; mixed wrappers and temporary owners are balanced.
- `ELEPHC_PHP_CHECK=1 CARGO_INCREMENTAL=0 cargo test --test codegen_tests
  xpath_dynamic_ -- --nocapture`: 3 passed.
- `ELEPHC_PHP_CHECK=1 CARGO_INCREMENTAL=0 cargo test --test codegen_tests
  xpath_multiple_dynamic_spreads -- --nocapture`: 1 passed.
- `CARGO_INCREMENTAL=0 cargo test -p elephc-dom
  xpath_round_trips_through_public_bridge_operations`: 1 passed.
- `CARGO_INCREMENTAL=0 cargo test --test codegen_tests
  spread_variable_supplies -- --nocapture`: 5 passed.
- `CARGO_INCREMENTAL=0 cargo test --test codegen_tests
  named_arguments_after_spread -- --nocapture`: 8 passed.

Latest verified routing tranche (`f0ec2c74ae`):

- The six legacy/modern `NodeList`, `NamedNodeMap`, `HTMLCollection`, and
  `TokenList` `getIterator()` operation keys are explicitly rejected by the
  native bridge as compiler-resident.
- `cargo test -p elephc-dom`: 75 passed.
- `git diff --check`: passed before commit.
- The reproducible explicit-route inventory moved from 58 to 52 missing
  operations.
- The routing commit alone did not establish behavior; the subsequently
  corrected and independently validated behavior is recorded under
  `fef80e8d54` above.

Previous verified tranche (`8c629ec6f8` plus direct-union dispatch
`dbeeb7a6a4`):

- Legacy XPath namespace-axis snapshots now retain every libxml2 fake
  namespace node through a shared allocation owned by both the snapshot slot
  and each materialized `DOMNameSpaceNode` wrapper.
- Repeated `item()` calls are canonical while the slot is live; a retained
  wrapper survives snapshot and document release, and releasing/re-reading a
  slot safely recreates its canonical wrapper.
- All ten `DOMNameSpaceNode` virtual properties match the frozen PHP oracle,
  including default `xmlns`, nullable prefix/local-name rules, parent and
  owner-document identity, and disconnected-state behavior. Reads lowered
  through `DOMNode`-rooted unions are remapped by receiver kind without
  allocation.
- Clones own independent fake nodes. `__sleep()` and `__wakeup()` reject
  serialization and unserialization with the exact concrete-class messages.
- `cargo test -p elephc-dom namespace`: 10 passed, including the
  `Dom\NamespaceInfo` integration regression.
- `cargo test --lib preserves_dom_`: 4 passed.
- `cargo test --test codegen_tests
  namespace_node_wrapper_lifetime_and_identity`: 1 passed.
- `cargo test --test codegen_tests
  namespace_info_matches_php_order_identity_and_value_object_semantics`: 1
  passed after merging both namespace lots.
- `cargo build --bin elephc`, `git diff --check`, and assembly-comment checks:
  passed.
- The explicit route inventory moved from 70 to 58 missing operations.

Previous namespace-info tranche (`951cee6c9e`, validation infrastructure
`10582a1a80`):

- `Dom\Element::getInScopeNamespaces()` returns the exact php-src
  self-to-ancestor namespace environment after prefix shadowing, omits default
  undeclarations, and preserves the reserved `xml` binding in the same order.
- `Dom\Element::getDescendantNamespaces()` returns self plus descendant
  declarations in document order without collapsing equal prefix/value pairs
  declared on distinct elements.
- Each result is a fresh ordinary readonly `Dom\NamespaceInfo` value object;
  `$element` preserves canonical element wrapper identity, while `$prefix` and
  `$namespaceURI` preserve exact nullable/string values.
- `cargo test --test codegen_tests
  namespace_info_matches_php_order_identity_and_value_object_semantics`: 1
  passed.
- `cargo test -p elephc-dom
  modern_namespace_info_round_trips_through_public_bridge_operations`: 1
  passed.
- `cargo build --bin elephc`, the updated `examples/dom-xml/main.php`,
  `git diff --check`, and assembly-comment checks: passed.
- The Linux x86_64 image originally stopped before compilation because CMake
  was absent. Both supported Linux test Dockerfiles install CMake; checkpoint
  CI then showed that the published `.github/docker/ci.Dockerfile` still lacked
  it. The current working tree adds CMake to that multi-arch image and adds a
  guarded fallback install to both Linux archive jobs so the PR can validate
  before the updated image reaches `main`. The target-specific DOM behavior
  test has not yet been rerun, so no Linux behavior-pass claim is made here.
- The explicit route inventory moved from 75 to 70 missing operations.

Previous prefix-write tranche (`2339cead2b`):

- Legacy `DOMNode::$prefix` writes now reproduce php-src's element/attribute
  namespace-host selection, existing-binding reuse, declaration creation,
  empty-prefix fallback, embedded-NUL truncation, and no-op node kinds.
- Reserved `xml`/attribute `xmlns` constraints return exact code-14
  `DOMException` values in strict mode and `Warning: Unknown: Namespace Error`
  with a null result in loose mode; declaration conflicts remain forced
  exceptions exactly like php-src.
- `cargo test --test codegen_tests legacy_node_prefix_writes_match_php`: 1
  passed.
- `cargo test -p elephc-dom
  legacy_node_prefix_writes_round_trip_through_public_bridge_operations`: 1
  passed.
- `cargo build --bin elephc` and `git diff --check`: passed.
- The explicit route inventory moved from 76 to 75 missing operations.

Previous private-constructor tranche (`29200c3885`):

- Generated private constructors for `Dom\\Node`, `Dom\\NamespaceInfo`, and
  `Dom\\TokenList` are explicitly classified as compiler-resident visibility
  guards and fail closed if malformed requests reach the native bridge.
- Exact private-constructor diagnostics are covered, including PHP visibility
  precedence for direct `Dom\\Node` construction.
- `cargo test --test error_tests modern_dom_`: 3 passed.
- `cargo build --bin elephc` and `git diff --check`: passed.
- The explicit route inventory moved from 79 to 76 missing operations.

Previous adjacent-position tranche (`639aeb5699`):

- `Dom\\AdjacentPosition` is injected as an ordinary string-backed PHP enum.
  Its `cases()`, `from()`, `tryFrom()`, `$name`, and `$value` semantics are
  compiler-owned and now explicitly classified as such at the bridge boundary.
- The focused regression covers case order, exact names/backing values,
  successful `from()`, nullable `tryFrom()`, and the exact invalid-backing
  `ValueError`.
- `cargo test --test codegen_tests
  adjacent_position_enum_is_compiler_resident_and_matches_php`: 1 passed.
- `cargo build --bin elephc` and `git diff --check`: passed.
- The explicit route inventory moved from 84 to 79 missing operations.

Previous value-object tranche (`768ac7f467`):

- `DOMException` and `LibXMLError` are ordinary compiler-managed PHP objects,
  not native wrappers. Their fourteen generated property opcodes are now
  explicitly classified at the bridge boundary and fail closed if a malformed
  request attempts to route them natively.
- Ordinary property lowering continues to own the observable behavior:
  mutable inherited `DOMException::$code`, all six mutable `LibXMLError`
  fields, construction, cloning, and parser-materialized values.
- `cargo test --test codegen_tests
  dom_exception_code_property_is_compiler_resident_and_mutable`: 1 passed.
- `cargo test --test codegen_tests
  libxml_error_value_objects_support_construction_mutation_and_clone`: 1
  passed.
- `cargo build --bin elephc` and `git diff --check`: passed.
- The explicit route inventory moved from 98 to 84 missing operations without
  adding native stubs for compiler-resident semantics.

Previous placeholder-property tranche (`7fbd7263f6`):

- php-src `ext/dom/element.c`, the legacy property-handler registry, and the
  PHP 8.5 oracle establish that both legacy `schemaTypeInfo` properties are
  always null while `DOMDocument::$config` is null and emits one suppressible
  deprecation on every unsuppressed read.
- The three generated property routes now validate their legacy element,
  attribute, or document receiver and return the exact placeholder values.
- `cargo test --test codegen_tests
  legacy_schema_type_info_and_document_config_match_php`: 1 passed, including
  `@` suppression of the document-config deprecation.
- `cargo build --bin elephc` and `git diff --check`: passed.
- The explicit route inventory moved from 101 to 98 missing operations.

Previous lifecycle tranche (`5b460e1cbb`):

- php-src `ext/dom/node.c`, `not_serializable.phpt`, and
  `not_unserializable.phpt` establish inherited throwing hooks rather than
  class-wide not-serializable flags, exact base `Exception` messages, and the
  runtime concrete wrapper class embedded in each message.
- All four generated legacy/modern `DOMNode`/`Dom\Node` lifecycle routes now
  validate zero arguments, resolve the live receiver through the authoritative
  document graph, and map stable native wrapper discriminators across legacy,
  modern XML, and modern HTML families.
- `cargo test --test codegen_tests
  dom_node_serialization_hooks_match_php_concrete_class_errors`: 1 passed.
- `cargo test -p elephc-dom
  wrapper_names_follow_the_stable_dom_discriminators`: 1 passed.
- `cargo build --bin elephc` and `git diff --check`: passed.
- The explicit route inventory moved from 105 to 101 missing operations.
- The nonrecoverable cleanup performed during this tranche removed only
  5.8 GiB of reproducible Rust incremental build cache after three isolated
  reviewer worktrees exhausted the available volume; no source or Git object
  was removed.

Previous reserved-callback tranche (`dacf4def5f`):

- php-src `ext/dom/xpath_callbacks.c`,
  `DOMXPath_callables.phpt`, `DOMXPath_callables_errors.phpt`, and
  `php_function_edge_cases.phpt` establish the always-present reserved
  namespace functions, none/all/set registry modes, alias rules, node-set
  conversion split, weak scalar name coercions, callable-array acceptance,
  and exact primary Error/TypeError/ValueError messages.
- The bridge now registers both legacy and modern routes transactionally,
  retains callback maps across mode changes, clones them with balanced
  ownership, invokes builtins/user functions/closures/instance methods/static
  methods outside the context borrow, and transports callback errors through
  the re-entrant native XPath boundary.
- Nested `[$object, "method"]` and `[ClassName::class, "method"]` values are
  prepared before flat serialization, while PHP object identity is still
  available. The program-level resolver reuses the ordinary callable
  descriptor machinery; the bridge retains the published descriptors and the
  compiler releases every temporary plan descriptor after the native call.
- `cargo test --test codegen_tests codegen::dom_xpath`: 12 passed.
- `cargo test -p elephc-dom`: 65 passed.
- `cargo test test_dom_runtime_`: 8 passed.
- `cargo build --bin elephc` and `cargo check --tests`: passed warning-free.
- `XDG_CACHE_HOME=/tmp/elephc-dom-runtime-cache cargo run --quiet --
  examples/dom_xpath_callbacks/main.php` and the compiled example: passed
  with custom and reserved callback output.
- `git diff --check`, the temporary-diagnostic search, and added
  assembly-comment alignment checks passed.
- macOS AArch64 executed the reserved callable-name, closure, instance-method,
  static-method, scalar-coercion, alias, clone, node-set, and exact-error
  paths. Both assembly backends have focused runtime-emission coverage; no new
  local Linux execution pass is claimed and the complete supported-target
  matrix remains a CI gate.

Previous node-callback tranche (`339674eecc`):

- php-src `ext/dom/xpath_callbacks.c`, its callback PHPTs, and the frozen
  PHP 8.5.8 signatures establish node-set argument conversion, DOM-node
  result conversion, the exact unsupported-object `TypeError`, and callback
  result lifetime requirements.
- The native callback adapter now flattens XPath node sets into canonical
  PHP wrapper arrays, preserves wrapper identity and document ownership,
  accepts DOM-node callback results as XPath node sets, and returns a
  generation-checked release lease. The bridge defers that release through a
  pending host action until after the DOM context `RefCell` borrow is dropped,
  which permits wrapper finalizers and subsequent callbacks to re-enter DOM
  safely.
- The AArch64 and x86_64 host runtimes validate nested flat ABI values,
  materialize the correct legacy/modern wrapper class from generated tables,
  preserve callback result ownership, and publish php-src's exact
  unsupported-object diagnostic. A compiler-side regression fixes direct
  closure returns such as `function (array $values) { return $values[0]; }`
  so their `Mixed` object value is not coerced to `int`.
- `cargo test --test codegen_tests
  test_mixed_array_element_closure_return_preserves_object`: 1 passed.
- `cargo test --test codegen_tests codegen::dom_xpath`: 8 passed.
- `cargo test -p elephc-dom`: 62 passed.
- `cargo test --lib
  codegen_support::runtime::internal_extensions::tests::`: 3 passed.
- `cargo test --lib test_fixed_runtime_data_gates_dom_bridge_state`: 1
  passed; `cargo test --lib test_emit_runtime_data_user_`: 4 passed.
- `cargo build --bin elephc` and `cargo check --tests`: passed warning-free.
- `XDG_CACHE_HOME=/tmp/elephc-dom-cache ./target/debug/elephc
  examples/dom_xpath_callbacks/main.php` and the compiled example: passed
  with scalar and direct DOM-node callback results.
- `git diff --check`, the temporary-diagnostic search, the changed-function
  Rustdoc audit, and added assembly-comment alignment checks passed.
- macOS AArch64 executed the real node-set/result/lifetime path. Both assembly
  backends have target-independent runtime-emission coverage; no new local
  Linux execution pass is claimed and the complete supported-target matrix
  remains a CI gate.

Previous custom-namespace callback tranche (`497f07d2da`):

- php-src `ext/dom/xpath_callbacks.c`, its callback PHPTs, and the generated
  PHP 8.5.8 method signatures establish custom namespace/name validation,
  scalar argument order and conversion, boolean-versus-string results,
  callable replacement ownership, and exact callback `Throwable` identity.
- The bridge retains callable descriptors in XPath handles, releases them on
  replacement/reset/drop, retains them transactionally during clone, and
  splits evaluation into prepare/execute/publish phases so nested DOM work
  runs outside the context `RefCell` borrow. The host ABI validates and boxes
  null, boolean, double, and string arguments on both AArch64 and x86_64 and
  transports boolean and leased-string results without unwinding across C.
- `cargo test --test codegen_tests codegen::dom_xpath`: 7 passed.
- `cargo test -p elephc-dom`: 61 passed, including exact host marshalling and
  callback replace/clone/release ownership.
- `cargo test codegen_support::runtime::internal_extensions::tests`: 6 passed
  on the target-independent emitter assertions.
- `cargo check --tests`: passed warning-free.
- `XDG_CACHE_HOME=/tmp/elephc-dom-cache cargo run --quiet --
  examples/dom_xpath_callbacks/main.php` and the compiled example: passed.
- The generated-route inventory reports 484 explicit routes, 107 missing
  routes, and zero dispatcher keys outside the generated registry.
- `git diff --check` and the changed-function Rustdoc audit passed.
- macOS AArch64 executed the real callback path. Linux x86_64 has independent
  runtime-emission coverage; no new local Linux execution pass is claimed and
  the complete supported-target matrix remains a CI gate.

Previous core XPath tranche (`f918ddee77`):

- php-src `ext/dom/xpath.c`, `ext/dom/xpath_callbacks.c`, the frozen stubs and
  XPath PHPTs, and pinned libxml2 establish the constructor/property state,
  context and persistent namespace rules, scalar/node-set mapping, failure
  channels, namespace-axis split, callback boundary, and quoting algorithm.
- The native bridge creates one isolated libxml XPath context per evaluation,
  copies result pointers/bytes and diagnostics before freeing temporary native
  storage, and retains the authoritative document graph in generation-checked
  XPath handles. Compiler lowering preserves legacy mixed return alternatives,
  omitted XPath defaults, static associative-spread omission, and exact
  floating-point result bits on both assembly backends.
- `CARGO_INCREMENTAL=0 cargo test --test codegen_tests codegen::dom_xpath
  -- --nocapture`: 5 passed.
- `CARGO_INCREMENTAL=0 cargo test --test codegen_tests 'codegen::dom::'
  -- --nocapture`: 117 passed; the specialized fragment, validation,
  XInclude, C14N, and XPath modules add 31 passes, for 148 focused DOM tests.
- `CARGO_INCREMENTAL=0 cargo test -p elephc-dom`: 59 passed, including public
  XPath ABI round-tripping and per-call context-namespace isolation.
- `CARGO_INCREMENTAL=0 cargo test --lib
  legacy_xpath_result_overrides_preserve_runtime_variants`: passed.
- `CARGO_INCREMENTAL=0 cargo check --bin elephc` and
  `CARGO_INCREMENTAL=0 cargo build --bin elephc`: passed warning-free.
- `./target/debug/elephc examples/dom_xpath/main.php` and the compiled example:
  passed with node-list access, scalar evaluation, and persistent namespaces.
- The generated-route inventory reports 482 explicit routes, 109 missing
  routes, and zero dispatcher keys outside the generated registry.
- `python3 scripts/check_asm_comments.py
  src/codegen/lower_inst/internal_extensions.rs`: passed.
- `git diff --check`: passed.
- The implementation emits the new mixed-float and wrapper-union paths for
  both AArch64 and x86_64. No new local Linux execution pass is claimed; the
  complete supported-target matrix remains a CI gate.

Previous C14N tranche (`01a30190b`):

- php-src `ext/dom/node.c`, its XPath/C14N option parsing, and pinned libxml2
  establish canonicalization validation, detached-node differences, namespace
  relinking, callback ordering, and file/stream output behavior.
- The internal-extension ABI now recursively validates and marshals bounded
  nested arrays, maps, and objects without cycles, aliasing, overlapping
  roots, or orphaned records. The native bridge canonicalizes through
  `xmlC14NExecute()`/`xmlC14NDocSaveTo()` and publishes exact legacy/modern
  results, diagnostics, and pending exceptions.
- `CARGO_INCREMENTAL=0 cargo test --test codegen_tests codegen::dom_c14n
  -- --nocapture`: 8 passed.
- `CARGO_INCREMENTAL=0 cargo test --test codegen_tests codegen::dom
  -- --nocapture`: 143 passed.
- `CARGO_INCREMENTAL=0 cargo test -p elephc-dom -- --nocapture`: 57 passed.
- The EIR static-property string-concatenation ownership unit, its direct
  codegen regression, and the existing static compound/string-property
  regressions all passed.
- `CARGO_INCREMENTAL=0 cargo build --bin elephc`: passed warning-free.
- `./target/debug/elephc examples/dom-xml/main.php` and the compiled example:
  passed with C14N memory/file output plus the previous XML/validation output.
- The generated-route inventory reports 466 explicit routes, 125 missing
  routes, and zero dispatcher keys outside the generated registry.
- `./scripts/check_asm_comments.py
  src/codegen/lower_inst/internal_extensions.rs`: passed.
- `git diff --check`: passed.
- A Linux x86_64 emission attempt reached the macOS host assembler after using
  an isolated runtime cache; the host assembler cannot consume the generated
  GNU ELF syntax. No local Linux execution pass is claimed, and the complete
  supported-target matrix remains a CI gate.

Previous XInclude tranche (`bc9c73e30`):

- php-src `ext/dom/node.c` and pinned libxml2 establish the shared XInclude
  algorithm, forced `XML_PARSE_NOXINCNODE`, legacy false/integer behavior,
  modern invalid-modification exception, and pre-mutation removal of retained
  XInclude wrapper references.
- The native bridge records every XInclude-owned element, attribute, and
  attribute/text child before processing; invalidated node/token handles and
  live/static derived views cannot access freed libxml memory. External
  loaders and PHP streams execute outside the bridge context borrow and
  preserve exact callback `Throwable` identity.
- `CARGO_INCREMENTAL=0 cargo test --test codegen_tests codegen::dom`: 135
  passed, including seven XInclude equivalence, invalidation, re-entry,
  ownership, diagnostic, and exception tests.
- `CARGO_INCREMENTAL=0 cargo test -p elephc-dom`: 54 passed, including the
  native substitution/destroyed-pointer result and exact structured-exception
  message-length ABI.
- `CARGO_INCREMENTAL=0 cargo build --bin elephc`: passed warning-free.
- `./target/debug/elephc examples/dom-xml/main.php` and the compiled example:
  passed with fallback substitution plus the previous XML/validation output.
- The generated-route inventory reports 462 explicit routes, 129 missing
  routes, and zero dispatcher keys outside the generated registry.
- `./scripts/check_asm_comments.py
  src/codegen/lower_inst/internal_extensions.rs`: passed.
- `git diff --check`: passed.
- The exception result-length correction is emitted for both AArch64 and
  x86_64. Docker Desktop remained unresponsive at the server handshake, so no
  new local Linux x86_64 execution is recorded; the complete supported-target
  matrix remains a CI gate.

Previous validation-warning tranche (`b26c1e678`):

- php-src's schema and Relax NG code installs legacy generic callbacks when
  internal libxml errors are disabled, while libxml's structured channel owns
  `LibXMLError` values in internal mode. The bridge now snapshots that mode
  before callback-capable execution and selects the same native channel.
- The generic adapter buffers varargs fragments until libxml emits a complete
  line, temporarily installs itself as the thread-local generic handler for
  nested XML-parser formatting, restores the previous handler on every exit,
  and preserves ordered parser header, source line, caret, schema-resource,
  validation, and final invalid-grammar messages.
- `CARGO_INCREMENTAL=0 cargo test --test codegen_tests
  codegen::dom_validation -- --nocapture`: 8 passed.
- `CARGO_INCREMENTAL=0 cargo test -p elephc-dom`: 53 passed, including
  independent structured and five-message generic schema paths.
- `CARGO_INCREMENTAL=0 cargo build --bin elephc`: passed warning-free.
- `git diff --check`: passed.
- No target-specific assembly changed; the final supported-target matrix
  remains a CI gate.

Previous validation-path tranche (`af104c3cd`):

- php-src's `dom_get_valid_file_path()` and
  `DOMDocument_schemaValidate_error6.phpt` require overlong local grammar
  paths to fail before libxml parsing. The pinned native bridge now applies
  the target `PATH_MAX` limit to plain and local `file://` paths while leaving
  URI schemes to their stream handlers.
- Legacy and modern schema/Relax NG file validation return `false` with the
  exact `Invalid ... file source` warning. Modern diagnostics use php-src's
  reflected `relaxNgValidate*` capitalization while legacy retains
  `relaxNGValidate*`.
- `CARGO_INCREMENTAL=0 cargo test --test codegen_tests
  codegen::dom_validation -- --nocapture`: 8 passed.
- `CARGO_INCREMENTAL=0 cargo test -p elephc-dom`: 53 passed.
- `CARGO_INCREMENTAL=0 cargo build --bin elephc`: passed warning-free.
- `git diff --check`: passed.
- This follow-up is target-neutral apart from reading the target C library's
  compile-time `PATH_MAX`; checkpoint CI showed that glibc hides it from
  `<limits.h>` under strict C11. The current native source therefore uses
  PHP's 4096-byte Unix fallback only when the platform headers omit
  `PATH_MAX`; no target-specific assembly changed.

Previous document-validation tranche (`1d215eed02`):

- php-src `ext/dom/document.c`, the frozen signatures/PHPTs, and the PHP manual
  confirm the five shared document-validation operations, their legacy/modern
  exposure, `LIBXML_SCHEMA_CREATE`, namespace guard, boolean/error channels,
  empty/NUL argument failures, and invalid-grammar warnings.
- Pinned libxml2 validates DTDs, W3C XML Schemas, and Relax NG grammars through
  the native bridge. File and source grammars both resolve PHP
  stream-wrapper-backed imports without retaining the DOM context borrow;
  nested DOM calls succeed, resource ownership is balanced, and external
  loader exceptions preserve exact object identity.
- `CARGO_INCREMENTAL=0 cargo test -p elephc-dom`: 53 passed, including pinned
  DTD/XSD/Relax NG validation and modern QName-valued schema data.
- `CARGO_INCREMENTAL=0 cargo test --test codegen_tests
  codegen::dom_validation -- --nocapture`: 7 passed, covering DTD validity,
  schema-created default attributes, modern namespace relinking, Relax NG
  validity, local files, registered wrappers, relative grammar dependencies,
  diagnostics, argument failures, exact loader `Throwable` identity, and clean
  heap ownership.
- `CARGO_INCREMENTAL=0 cargo test --test codegen_tests
  codegen::dom -- --nocapture`: 127 passed, with no regression across the
  complete focused DOM surface.
- `CARGO_INCREMENTAL=0 cargo build --bin elephc`: passed warning-free.
- `XDG_CACHE_HOME=/tmp/elephc-dom-compliance-cache
  ./target/debug/elephc examples/dom-xml/main.php` and the compiled example:
  passed with legacy/modern mutation output plus successful XSD and Relax NG
  validation.
- The generated-route inventory reports 460 explicit routes, 131 missing
  routes, and zero dispatcher keys outside the generated registry.
- `git diff --check`: passed.
- At this commit, the structured-error adapter still did not split libxml's
  echoed source line and caret into separate PHP warnings; `b26c1e678`
  subsequently closed that gap.
- This tranche changes target-neutral Rust dispatch, host ABI plumbing, and
  the pinned native C bridge. No new target-specific assembly path is
  introduced; the complete macOS ARM64/Linux ARM64/Linux x86_64 execution
  matrix remains a CI gate.

Previous document-fragment tranche (`0f1e8f55c5`):

- php-src `ext/dom/documentfragment.c`, the frozen signatures/PHPTs, and the
  PHP manual confirm the shared legacy/modern balanced-chunk algorithm,
  unbound-fragment code 7 exception, boolean failure, and XML parsing for
  modern HTML documents.
- The local PHP 8.5.6 CLI linked to libxml2 2.9.13 was used only as an
  exploratory comparison. Its empty-chunk result and three-error malformed
  sequence differ from the normative PHP 8.5.8 + pinned libxml2 2.15.3
  differential oracle; the pinned bridge produces the locked 2.15.3 behavior
  and its tests assert that behavior.
- `CARGO_INCREMENTAL=0 cargo test -p elephc-dom`: 52 passed, including public
  route validation, wrong-receiver rejection, unbound legacy failure,
  structured-error retention, empty input, embedded NUL, and both API-family
  routes.
- `CARGO_INCREMENTAL=0 cargo test --test codegen_tests
  codegen::dom_fragment`: 3 passed, covering legacy/modern XML/modern HTML,
  namespace isolation, malformed input, exact warning text, internal errors,
  `@` suppression, serialization, and clean heap ownership.
- `CARGO_INCREMENTAL=0 cargo build --bin elephc`: passed warning-free.
- `XDG_CACHE_HOME=/tmp/elephc-dom-compliance-cache
  ./target/debug/elephc examples/dom-xml/main.php` and the compiled example:
  passed with both legacy and modern fragment insertion rendered.
- The generated-route inventory reports 450 explicit routes, 141 missing
  routes, and zero dispatcher keys outside the generated registry.
- `git diff --check`: passed.
- This tranche changes only target-neutral Rust dispatch and the pinned native
  C bridge. No new target-specific assembly path is introduced; the complete
  macOS ARM64/Linux ARM64/Linux x86_64 execution matrix remains a CI gate.

Previous weak-conversion deprecation tranche (`836b99ef8e`):

- php-src and the local PHP 8.5 oracle confirm the exact
  `Implicit conversion from float[-string] ... to int loses precision`
  conditions, text, union preference, callback ordering, and `@` suppression.
- `CARGO_INCREMENTAL=0 cargo build --bin elephc`: passed warning-free.
- `CARGO_INCREMENTAL=0 cargo test --lib wrapper_adapter -- --nocapture`: 14
  passed, including the exact-first preference table and independent x86_64
  static-string, boxed-float, and boxed-float-string deprecation generation.
- `CARGO_INCREMENTAL=0 cargo test --test codegen_tests
  test_user_wrapper_weak_int_conversion_deprecations_match_php
  -- --nocapture`: passed with exact static and boxed-source diagnostics,
  integer-valued and suppressed silence, restored callback inputs, and clean
  heap-debug ownership.
- `CARGO_INCREMENTAL=0 cargo test --test codegen_tests
  test_user_wrapper_dynamic_mixed -- --nocapture`: 4 passed.
- `CARGO_INCREMENTAL=0 cargo test --test codegen_tests
  test_user_wrapper_static_union_parameters_match_php -- --nocapture`: passed.
- `./scripts/check_asm_comments.py
  src/codegen/user_wrapper_adapters/deprecations.rs`: passed.
- `git diff --check`: passed.
- Focused Linux x86_64 execution was attempted, but Docker Desktop remained
  unresponsive before container creation; the command was interrupted
  cleanly. Independent x86_64 deprecation-path emission tests are green and CI
  remains the execution gate.

Previous re-entrant wrapper-read tranche (`e5483ecc13`):

- `CARGO_INCREMENTAL=0 cargo check -p elephc-dom`: passed.
- `CARGO_INCREMENTAL=0 cargo check --bin elephc`: passed.
- `CARGO_INCREMENTAL=0 cargo build --bin elephc`: passed warning-free.
- `CARGO_INCREMENTAL=0 cargo test -p elephc-dom`: 51 passed, including strict
  host marshalling for all four null failure reasons, wrapper class bytes,
  silent failures, and malformed reason/class combinations.
- `CARGO_INCREMENTAL=0 cargo test --lib test_dom_runtime_ -- --nocapture`: 4
  passed, including independent AArch64 and Linux x86_64 host-call validation,
  stat short-circuiting, open-failure discriminators, wrapper class-name
  lookup, write/flush dispatch, and warning emission.
- `CARGO_INCREMENTAL=0 cargo test --test codegen_tests 'codegen::dom::'
  -- --nocapture`: 117 passed. This includes all legacy/modern XML and HTML
  file reads, exact open-failure diagnostics, output-wrapper behavior,
  libxml stream-context propagation, nested missing-stat destructor re-entry,
  warning suppression, native graph publication, high-bit modern HTML options,
  and clean heap ownership.
- `CARGO_INCREMENTAL=0 cargo test --test codegen_tests dom_file_reads_
  -- --nocapture`: 18 passed after making each `stream_open()` ABI fixture
  provide the required successful quiet `url_stat()` probe.
- `./scripts/check_asm_comments.py
  src/codegen_support/runtime/io/user_wrapper_url_stat.rs
  src/codegen_support/runtime/io/fopen.rs
  src/codegen_support/runtime/internal_extensions.rs`: passed.
- `git diff --check`: passed.
- The immediately preceding Docker check remained unresponsive, so no local
  Linux execution is newly recorded as a pass; the x86_64 runtime-emission
  test is green and CI remains the execution gate.

Previous exact wrapper-open tranche (`ed6ce22c6d`):

- Direct registered-wrapper reads stop after missing/false `url_stat()` and
  distinguish missing/false `stream_open()` with php-src-compatible callback
  order, method-scoped warnings, credential scrubbing, modern base
  `Exception`, suppression, strict host ABI classification, and heap-clean
  temporary ownership.

Previous wrapper-output tranche (`32ce47e6f7`):

- `CARGO_INCREMENTAL=0 cargo check -p elephc-dom`: passed.
- `CARGO_INCREMENTAL=0 cargo check --bin elephc`: passed.
- `CARGO_INCREMENTAL=0 cargo test -p elephc-dom -- --nocapture`: 50 passed,
  including host marshalling for partial, false, zero, negative, oversized,
  warning, flush, and release results.
- `CARGO_INCREMENTAL=0 cargo test --lib test_dom_runtime_ -- --nocapture`: 4
  passed, including independent AArch64 and Linux x86_64 host-call validation,
  wrapper class-name lookup, write/flush dispatch, and warning emission.
- `CARGO_INCREMENTAL=0 cargo test --test codegen_tests dom_file_save
  -- --nocapture`: 5 passed. All five serializers use `wb`, options zero, the
  selected context, partial-write retries, one flush/close, and re-entry;
  false, zero, partial-then-zero, arbitrary negative values, oversized results,
  `@` suppression, exact warning text, callback Throwable identity, and clean
  heap ownership match the frozen PHP oracle.
- `./scripts/check_asm_comments.py
  src/codegen_support/runtime/internal_extensions.rs`: passed.
- `git diff --check`: passed.
- `./scripts/test-linux-x86_64.sh dom_file_save` was attempted after reopening
  Docker Desktop, but the daemon remained unresponsive even to `docker info`.
  The command was interrupted cleanly and is not recorded as a target pass;
  the x86_64 runtime-emission test is green and CI remains the execution gate.

Previous eval EOF tranche (`b946a3e3c4`):

- `CARGO_INCREMENTAL=0 cargo build`: passed after the eval EOF alignment.
- `CARGO_INCREMENTAL=0 cargo check -p elephc-magician`: passed.
- `CARGO_INCREMENTAL=0 cargo test -p elephc-magician user_stream_wrapper`:
  13 passed, including direct, aggregate, line, metadata, option, path, cast,
  directory, and file-I/O wrapper coverage.
- `CARGO_INCREMENTAL=0 cargo test -p elephc-magician
  execute_program_matches_php_user_stream_wrapper_eof_semantics`: passed with
  exact strict warning text for string and concrete object results, sticky
  cache reuse, successful-seek reset, repeated false callbacks, missing-method
  EOF, direct reads after cached EOF, `RERE` aggregate drain ordering, and
  post-read `Exception` propagation.
- The locally available PHP 8.5.6 CLI independently confirms that a cached
  userspace-wrapper EOF does not suppress a subsequent explicit `fread()`,
  that aggregate drains continue until `stream_read()` returns an empty
  string, and that every such read is followed by `stream_eof()`. This
  exploratory oracle does not replace the frozen PHP 8.5.8 final gate.
- `git diff --check`: passed.

Previous native EOF tranche (`ca8a23a170`):

- `CARGO_INCREMENTAL=0 cargo test -p elephc-dom`: 49 passed.
- `CARGO_INCREMENTAL=0 cargo test --test codegen_tests 'codegen::dom::'
  -- --nocapture`: 107 passed, including exact untyped, typed, and composite
  by-reference variadic callback entry values plus the new EOF callback order,
  strictness, cache, warning, suppression, and exceptional cleanup paths.
- `CARGO_INCREMENTAL=0 cargo test --test codegen_tests user_wrapper
  -- --nocapture`: 37 passed, including post-read EOF ordering, exact strict
  warnings, cache reset, suppression, Throwable identity, and clean heap-debug
  ownership.
- `CARGO_INCREMENTAL=0 cargo test --lib
  codegen::user_wrapper_adapters::tests -- --nocapture`: 11 passed, including
  Linux x86_64 strict/lenient EOF adapter dispatch, warning type-name
  materialization, dynamic and variadic reference boxing, and cleanup emission.
- `CARGO_INCREMENTAL=0 cargo test --lib test_user_wrapper_fread_
  -- --nocapture`: 2 passed, independently checking AArch64 and Linux x86_64
  exception-boundary ordering, hidden strictness modes, EOF cache emission, and
  exceptional chunk release.
- `CARGO_INCREMENTAL=0 cargo test --test error_tests parameter_default
  -- --nocapture`: 4 passed, including mismatched and undefined global
  constant method defaults.
- `CARGO_INCREMENTAL=0 cargo test --test codegen_tests stream_context`:
  11 passed.
- `CARGO_INCREMENTAL=0 cargo check --lib`: passed.
- `CARGO_INCREMENTAL=0 cargo build`: passed.
- `CARGO_INCREMENTAL=0 cargo build -p elephc-dom`: passed.
- `git diff --check` and the touched-emitter inline-comment alignment audit:
  passed.
- `test_user_wrapper_stream_eof_context_and_cached_state_match_php`: passed
  with exact order `feof -> stream_eof`, sticky reuse, successful-`fseek`
  reset, post-read `stream_eof`, `@` suppression, concrete object type names,
  exactly one unsuppressed string warning, and a clean heap-debug summary.
- `test_user_wrapper_post_read_stream_eof_throw_unwinds_chunk_owner`: passed
  with exact output `R|E|Exception:eof|C|`; the pending string owner is
  released before the identical `Throwable` is rethrown and heap-debug is
  clean.
- The locally available PHP 8.5.6 CLI confirms php-src's stable callback order,
  strict `feof()` exact-bool warning, invalid-result `true`, sticky EOF, seek
  reset, suppression, and concrete object type naming. This exploratory oracle
  does not replace the frozen PHP 8.5.8 final gate.
- A focused Linux x86_64 Docker execution was attempted after the assembly and
  host-runtime gates passed, but the local Docker daemon remained unresponsive
  even to `docker info` after Docker Desktop was reopened. The command was
  interrupted cleanly and is not recorded as a target pass; CI remains the
  authoritative Linux execution gate.
- A fixed-path `stream_open($path, &...$arguments)` receives three element
  references, warns only for runtime arguments 2 and 3, retains the genuine
  fourth opened-path reference without warning, and preserves aliasing through
  an array copy. Mutations produce exact output
  `3:copy-mode:changed-options|bool(false)\n`; heap-debug reports `clean`.
- A typed `stream_read(string &...$arguments)` weakly converts the runtime
  integer count on entry, then permits a cross-type reference write. Exact
  output is `string:8192|integer:1|bool(true)\n`, with the PHP warning at
  argument 1.
- A composite `stream_open(string|int|null &...$arguments)` accepts the exact
  string/string/integer/null runtime sequence, warns for arguments 1-3 but not
  the genuine fourth reference, and permits post-entry writes to
  bool/float/string/array. Exact output is
  `string:string:integer:NULL|boolean:double:string:array|bool(false)\n`.
- The exceptional variadic regression assigns an owned string through the
  real opened-path marker before throwing; the adapter restores
  `Exception:variadic` and heap-debug reports `clean`.
- `CARGO_INCREMENTAL=0 cargo test --test codegen_tests
  test_by_ref_variadic_function_and_method_element_writeback`: 1 passed,
  preserving the pre-existing direct function/method writeback contract.
- Four eval-specific by-reference variadic filters remain blocked before
  exercising this tranche by the unrelated existing EIR diagnostic
  `unsupported EIR backend feature: local load from PHP type TaggedScalar as
  Int`; they are not counted as passes.
- The currently available PHP 8.5.6 CLI confirms the same element aliasing,
  warning positions, entry-only typed-reference constraint, and cross-type
  mutation behavior. This exploratory oracle does not replace the frozen PHP
  8.5.8 final gate.
- Untyped by-reference DOM callbacks observe exact runtime types
  `string|string|integer|NULL`, warn for by-value runtime inputs 1-3 but not
  the genuine opened-path reference, and permit mutation to array, integer,
  string, and string respectively. The result is `bool(false)` and heap-debug
  reports `clean`.
- The exceptional wrapper regression assigns an owned string to
  `$openedPath` before throwing; the adapter clears that runtime-owned scratch
  cell, restores the original `Exception:wrapper`, and leaves heap-debug
  `clean`.
- The currently available PHP 8.5.6 CLI confirms these stable input types,
  warning positions, and cross-type reference mutations. This exploratory
  oracle does not replace the frozen PHP 8.5.8 final gate.
- The wrapper-default regression evaluates a global constant, two class
  constants, `self::class`, an array, and a constructed object through the
  synthetic thunks. It produces
  `1,2|global|NonLiteralDefaultWrapper|class|WrapperDefaultObject:objectbool(false)\n`
  and reports `HEAP DEBUG: leak summary: clean`.
- The currently available PHP 8.5.6 CLI confirms the same stable default
  expressions and values. Its CLI `var_dump()` source-location prefix differs
  from Elephc's existing output contract; this exploratory oracle does not
  replace the frozen PHP 8.5.8 final gate.
- The currently available PHP 8.5.6 CLI confirms the stable php-src
  exact-first union behavior used by this tranche: metadata arrays remain
  arrays, integer metadata values prefer an allowed string, numeric strings
  choose `int`/`float` at `PHP_INT_MIN`/`PHP_INT_MAX`, and incompatible arrays
  fail an interface intersection with the same concrete-type diagnostic. This
  exploratory oracle does not replace the frozen PHP 8.5.8 final gate.
- Declared-`mixed` wrapper return regression produces exact output
  `C|bool(true)\nok` for a direct `DOMDocument::load()` and reports
  `HEAP DEBUG: leak summary: clean`; this covers truthy open, converted string
  reads, dynamic EOF, ignored container close results, and exact stat arrays.
- `CARGO_INCREMENTAL=0 cargo test --test codegen_tests
  dom_file_reads_normalize_mixed_stream_wrapper_returns`: 1 passed.
- Focused PHP 8.5.8 differential for untyped userspace-wrapper callbacks:
  exact output
  `untypeddom://source|2|untypeddom://source|rb|0|8192|8192|8192|8192|C|bool(true)\n`;
  callback arguments are PHP values and heap-debug reports `clean`.
- `CARGO_INCREMENTAL=0 cargo test --test codegen_tests
  dom_file_reads_adapt_untyped_stream_wrapper_parameters`: 1 passed.
- `CARGO_INCREMENTAL=0 cargo test --test codegen_tests user_wrapper`: 34
  passed.
- `CARGO_INCREMENTAL=0 cargo test --lib
  user_wrapper_adapters::tests`: 8 passed, including typed coercion,
  composite runtime contracts, variadic packing, catchable `TypeError`, and
  catchable `ArgumentCountError`, plus dynamic exact-int/stat return
  normalization in target-aware x86_64 adapter generation without invoking
  the unavailable local GNU toolchain.
- `CARGO_INCREMENTAL=0 cargo test --test codegen_tests
  dom_file_reads_unwind_wrapper_callback_temporaries_on_throw`: 1 passed;
  an anonymous direct `DOMDocument` receiver, callback reference cells,
  converted arguments, and the throwaway wrapper object all unwind with the
  exact original `Exception:wrapper` and a clean heap-debug summary.
- `CARGO_INCREMENTAL=0 cargo test --test codegen_tests
  test_regression_temporary_method_receiver_unwinds_on_throw`: 1 passed;
  definite owning method receivers now remain visible to activation-frame
  cleanup without falsely claiming borrowed concrete locals.
- `CARGO_INCREMENTAL=0 cargo test --test codegen_tests
  test_fopen_user_wrapper_uncaught_typed_callback_error_is_precise`: 1 passed;
  without an outer handler the adapter retains the exact
  `Fatal error: Uncaught TypeError: ... array, int given` diagnostic.
- `CARGO_INCREMENTAL=0 cargo test --test codegen_tests dom_file_reads_`: 11
  passed after reference-cell materialization.
- Fixed by-reference DOM callbacks observe exact runtime types
  `string|string|integer|NULL`, emit warnings for arguments 1-3 but not the
  genuine fourth reference, allow same-type mutation, return `bool(false)`,
  and leave heap-debug clean.
- A non-nullable opened-path reference throws the exact
  `TypeError:TypedOpenedPathDomWrapper::stream_open(): Argument #4
  ($openedPath) must be of type string, null given`.
- Required-extra callback invocation returns the exact
  `ArgumentCountError|type|Too few arguments to function
  ArityDomWrapper::stream_open(), 4 passed and exactly 5 expected`.
- Untyped by-value variadic DOM reads return exact argument shapes
  `1:2|3:rb:0:null|1:8192|1:8192|1:8192|1:8192|C|bool(true)\n` and heap-debug
  reports `clean`.
- Typed variadic rejection returns the exact PHP argument-position diagnostic
  `TypedVariadicDomWrapper::stream_open(): Argument #2 must be of type int,
  string given`.
- Focused typed-callback differential against the currently available PHP
  8.5.6 CLI matches weak bool/string/float conversion, the defaulted fifth
  parameter, four `8192` string reads, and exact catchable `TypeError`
  messages. This stable-semantic exploratory result is not a substitute for
  the frozen PHP 8.5.8 acceptance oracle, which must be reconstructed before
  final acceptance.
- Typed wrapper conversion ownership reports
  `HEAP DEBUG: leak summary: clean`; the statically incompatible array
  callback case is also clean after explicit document/Throwable release.
- A direct anonymous `DOMDocument` temporary escaping through a callback
  `Throwable` now leaves heap-debug clean; the exceptional-unwind owner
  imbalance is closed.
- `CARGO_INCREMENTAL=0 cargo test --lib
  codegen_support::runtime::data::user::tests`: 5 passed.
- Focused PHP 8.5.8 direct-wrapper differential for legacy
  `DOMDocument::load()` and modern `Dom\XMLDocument::createFromFile()`:
  exact `url_stat` flags `2`, mode `rb`, four 8192-byte reads, one close, and
  independently selected context names `two` then `one`; heap-debug reports
  `clean`.
- Focused independent-context regression returns `a|b` for two resources and
  leaves heap-debug clean.
- `ClassConstantWrapper::class` registration regression returns a valid stream
  resource, guarding dynamic class metadata collection for `ConstClassName`.
- Runtime-data unit regression proves untyped properties carry GC tag `7`, so
  host-injected boxed context values are reclaimed.
- Focused borrowed-`$this` property return regression: direct property and 100
  computed `substr()` method returns produce exact output `12|700` and leave
  heap-debug clean.
- Focused generic userspace-wrapper regression: two computed `substr()` chunks
  returned through `fread()` produce exact output `abcdefghijkl` and leave
  heap-debug clean.
- Focused PHP 8.5.8 differential for a registered external-loader stream:
  exact output
  `C:O|R8192|R8192|R8192|R8192|R8192|R8192|R8192|R8192|X|T:42:0`,
  with heap-debug clean in Elephc.
- Explicit and ownership-driven registered-wrapper close regression: exact
  output `B|X|D|A|C|X|D|E`, with one `stream_close()` and one final destructor
  per object and heap-debug clean.
- Bridge host buffering regression proves one 8192-byte PHP read can serve
  multiple smaller native reads without a duplicate host call and that both
  chunk and resource leases release exactly once.
- AArch64/x86_64 runtime-emission tests for indexed invoker owner cleanup,
  exception rethrow, external-loader argument construction, and leased stream
  reads: passed.
- Focused PHP 8.5.8 differential for parser invocation/re-entry: exact output
  `B:C:PUB:4:root:PUB:nested:bool(true)\n:A:1\n`, with heap-debug clean.
- Focused PHP 8.5.8 differential for callback Throwable identity: exact output
  `B:C:n:S:loader`.
- Focused PHP 8.5.8 differential for a callback-returned `php://temp` stream:
  exact output `C:N:root|T:42:0`, with heap-debug clean.
- Focused PHP 8.5.8 differential for six external-loader callable shapes
  (function string, indexed and associative instance/static arrays, and an
  invokable object): exact output `TCfNTCiNTCsNTCiNTCsNTCoN`. Each returned
  value also passed `is_callable()` and direct invocation.
- Receiver-bound callable-array heap-debug validation reports
  `HEAP DEBUG: leak summary: clean`.
- Release/rethrow regression with an instance callable array remains exact at
  `BDrC:release:N`, proving release-time ABI re-entry, original-Throwable
  rethrow, and post-destruction loader state.
- AArch64 and x86_64 boxed-descriptor runtime-emission tests, the locked
  operation-registry test, the complete libxml E2E filter, explicit bridge
  debug/release builds, and assembly-comment alignment checks: passed.
- Complete CI run `30690381815` for `d039d1a4c7` is not green. Its broad linker
  failures were reproduced locally with
  `test_dynamic_property_set_on_mixed_receiver_with_concat_built_string` and
  `test_stream_bucket_append_then_pop_in_order`: both emitted latent DOM
  branches without linking the DOM runtime. The current feature-detection fix
  makes both pass, alongside focused EIR property/method candidate tests and
  `mixed_dispatch_with_dom_candidates_links_required_runtime`.
- Read-only GLM 5.2 audit: the Mixed property/method plus direct/dynamic
  descendant detection covers every active DOM-symbol-emitting EIR path. Its
  defensive `Immediate::ProfiledData` parity finding is fixed, while nullsafe
  property/method calls remain intentionally excluded because their current
  backend paths do not emit DOM bridge symbols.
- `cargo test --lib ir_lower::tests::internal_extensions`: 19 passed.
- `cargo test --test codegen_tests mixed_dispatch_with_dom_candidates_links_required_runtime`:
  passed.
- `cargo check --tests`: passed.
- Rebuilt `target/debug/elephc`, then compiled and ran the exact
  `/tmp/elephc-dom-ci-hydrator.php` reproduction: `Row:1:Ada`.
- Assembly-comment alignment and `git diff --check`: passed before commit.

`CARGO_INCREMENTAL=0 cargo clippy -p elephc-dom -- -D warnings` remains blocked
by the pre-existing `needless_borrows_for_generic_args` finding in the untouched
`crates/elephc-dom/build.rs:212`.

`CARGO_INCREMENTAL=0 cargo clippy -p elephc --lib --no-deps -- -D warnings`
reports no diagnostics in this tranche's five touched files, but the crate-wide
gate remains blocked by 234 pre-existing lints. The dependency-inclusive form
also stops first on two unrelated `elephc-phar` lints.

Local Linux Docker validation is currently unavailable because the Docker
daemon is not running. GNU cross-compilers are also unavailable locally.
An attempted Linux x86_64 CLI emission reached the host assembler, which cannot
assemble the generated GNU target syntax on macOS; Linux target claims must
therefore come from CI, and no local absence is recorded as a target pass.

## Working rules

Only Codex 5.6 Terra and 5.6 Luna agents may write implementation code. The
parent agent owns specification, dispatch, evidence, integration, and
publication, but does not implement code. GLM 5.2, Kimi K2.7, and Kimi K3 are
read-only reviewers through Ollama and never receive implementation-writing
credit. Each implementation agent owns only its assigned files/tranche, must
produce focused evidence, and must not edit this plan, rebase, push, or modify
unrelated work. Final consensus remains an independent read-only audit of the
exact same frozen implementation commit.

The original checkout is dirty and must not be reset, cleaned, stashed, or
otherwise modified. Inherited work in other worktrees must not be interrupted.

`cargo fmt` and `cargo fmt --all` are forbidden. Local test runs stay focused
unless a broad/high-risk gate genuinely requires more.

From 2026-08-04 onward, each remaining cause family follows an explicit TDD
gate: the delegated Terra/Luna writer first adds the complete focused test
matrix for that tranche without production edits, records the expected red
result (or the already-pinned failing PHPT), and only then implements until the
same matrix is green. Two observed OOMs also make resource serialization an
acceptance constraint: one DOM Cargo process at a time, `CARGO_INCREMENTAL=0`,
`-j 1`, no full local suite during implementation, and no DOM build while an
unrelated high-memory Rust build leaves the host under pressure. Tests are run
individually or with verified non-zero filters; a zero-test result is never
evidence. The 2026-08-04 pre-red resource check found unrelated nextest/rustc
jobs active in other preserved worktrees, only 1.3 GiB free on the data volume,
and substantial compressed-memory pressure; therefore DOM Rust execution stays
suspended until those processes finish and capacity is rechecked. Only the
reconstructible DOM incremental cache has been removed so far; no source,
oracle, foreign worktree, or other target directory was touched.

The normative specification is `docs/specs/php-dom-compliance.md`. Its locked
bytes must not change unless all three reviewers repeat a complete
specification review and lock the new identical digest.

At the user's explicit request, a draft checkpoint may be pushed and opened as
a pull request before the final three-reviewer `LOCK`. The PR must remain
clearly marked incomplete, and final readiness/merge still requires all three
reviewers to return `LOCK` for the exact same completed implementation commit.

## Reference freeze

- PHP manual: complete DOM book, legacy DOM and modern `Dom\`.
- PHP source tag: `php-8.5.8`.
- Peeled source commit: `26b97507444c4fbda072f57dda1820f7b7d5e467`.
- Primary API contract: `ext/dom/php_dom.stub.php`.
- Primary behavioral contract: all `ext/dom/tests/**/*.phpt`.
- Companion contracts: complete `ext/libxml/libxml.stub.php` and
  `ext/simplexml/simplexml.stub.php` surfaces plus their PHPT suites.
- Differential oracle: PHP 8.5.8 built from the official source archive
  against pinned static libxml2 2.15.3 and bundled Lexbor 2.7.0.

## Gate order

1. Surface manifest and parity generator.
2. Bridge registration, panic-safe ABI, re-entrant result frames, and host callbacks.
3. Authoritative libxml graph, safe handles, document ownership, wrapper cache, and finalizers.
4. Exact declarations, virtual properties, constants, errors, and reflection.
5. Legacy XML/HTML tree operations and serialization.
6. Modern XML tree operations and serialization.
7. Modern HTML5, CSS selectors, HTML collections, and token lists.
8. XPath callbacks and namespaces; validation, C14N, and XInclude.
9. Complete SimpleXML surface/interoperability and legacy/modern family isolation.
10. DOM, libxml, and SimpleXML PHPT manifests at zero semantic exclusions.
11. Supported-target checks and final three-model audit.

Each gate adds a regression test for every fixed defect. A later gate may not
silently weaken an earlier gate's semantics or acceptance criteria.
