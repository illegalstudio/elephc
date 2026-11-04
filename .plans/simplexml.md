# SimpleXML

- [x] Implement document ownership, live selections, mutation, namespaces, and XPath in the existing XML bridge.
- [x] Expose the three functions and both classes through EIR-compatible prelude declarations and shared contracts.
- [x] Cover property/attribute access, iteration, conversions, DOM import, errors, and eval with focused tests.
- [x] Update generated coverage, user documentation, and a runnable example.
- [x] Open a draft PR and route heavy validation and regression checks through CI.
- [ ] Resolve confirmed SimpleXML correctness defects and regressions in this PR, preserving every test and check.
- [ ] Obtain a clean full-work installed-default Grok review and green CI on the same final head.

The current priority is closing SimpleXML. Do not extend the generalized native-reference refactor. Finish the running Grok review and let CI terminate on the stable published candidate before grouping necessary corrections into another push. Keep the PR draft.

Deferred generalized native-reference follow-up:

- [ ] Complete native typed scalar/union reference publishers, protected declaration validators and per-binding lifecycle.
- [ ] Connect invisible validation roots and the weak type-source registry to promotion, clone, unset, destruction and statics.
- [ ] Complete the general native reference body, caller, eval and reference-return type flow.
- [ ] Extend suspended Fiber owner inventories beyond fixes required for regressions in this PR.

These deferred features remain separate from the SimpleXML readiness criteria. Existing executable regressions, heap assertions and CI gates still have to pass; deferral never justifies removing a test, hiding a failure or enabling incomplete promotion.

Reuse the managed libxml2 package and its opaque C shim on all supported targets. Keep documents alive while any selection references them. Select children and attributes lazily so mutations remain visible to existing wrappers. Expose PHP 8.5 signatures and preserve custom element subclasses. Use the existing XML bridge capability for opaque eval source.

Validation follows the user's instruction: local read-only and lightweight source checks only; compilation, linking, executable tests, and target coverage run in the PR's CI.

Keep each published candidate stable until its complete CI run terminates. The next grouped
CI repair uses owned canonical snapshots for eval access-mask probes, publishes OpenSSL tags
through the existing native output writer, stops nested-write cleanup after an emitted throw,
and preserves per-coroutine deep-release suppression across Fiber switches. The global collector
reentry guard stays global. These repairs retain the executable heap assertions and all-target
checks; they do not enable generalized typed property promotion or nested iterator extensions.

Draft PR: https://github.com/illegalstudio/elephc/pull/2060. Earlier CI runs passed both Linux builds, both iOS XML/SimpleXML links, native ownership tests, and the isolated array/JSON heap regressions. The integrated work also covers live DOM identity and relationships, text and namespace writes, XPath contexts, and guarded interface receivers. All 22 public PHP class methods are present, including the debug projection. Native and boxed debug calls, side-effecting property-name reads, and production CLI prelude type checking have dedicated regressions. Generated documentation comes from CI artifacts and local JSON-based audits, without invoking Cargo.

The user authorized the supporting native reference refactor and unlimited review rounds. Exact-Mixed native properties and constructor bindings share persistent references with eval. Typed scalar/union promotion remains disabled while generated publishers, validators and lifecycle integration are unfinished. Canonical readers, a weak indexed source registry, original-RHS coercion coordination and invisible native validation roots are preparatory components. Each initially matching property source must independently validate the original RHS; removing a source during coercion does not discard its in-flight result. Temporary roots must preserve PHP clone and array-copy sharing semantics. Keep the PR draft throughout implementation. Greptile is waived because its credits are exhausted; final independent review uses installed-default Grok.

Native reads use the canonical child's current PHP tag. Suspended activation inventories cover source leases, pending native pins, binder conversions, argument arrays and synthesized magic arguments. Cleanup keeps its held result and remaining argument owners visible until each lease is transferred or released. The strict cycle, cancellation and destructor-order regressions remain required CI gates.

Post-eval typed native parameter reads use explicit owning Mixed value copies while keeping their entry cell payload ABI. Shared canonical wrapper reloads bypass the legacy projection. Scalar parameter body writes invalidate their stale entry view in value-returning functions. Reference-returning functions retain their existing cell ABI. General reference rebinding, object/union body writers, reference-return coercion, expression-result coercion and universal promotion remain deferred follow-up. Their incomplete paths must remain disabled.

Reflection construction no longer snapshots live static values. Generic accessors build their current maps lazily through EIR and omit uninitialized properties, preserving the destruction timing of displaced objects.

Local append, indexed statements, simple offset expressions, deep local stores/appends/updates and direct reference foreach sources select concrete raw storage or a live borrowed canonical child, keeping value-copy reads separate from mutation receivers. Their CI regressions cover canonical string and array updates, exact written-byte expression results, a headerless raw string cell, raw array writes after opaque eval, nested compound updates, foreach reference writeback, ordinary caller aliases and both optimizer modes. Nested iterator sources, mutating builtin consumers, typed literal reassignments and the complete caller/reference-return flow belong to the deferred generalized integration. The existing gated native body fixture uses an unknown Mixed RHS and still requires the new PHP type, exact mutations and zero live owners.
