# SimpleXML

- [x] Implement document ownership, live selections, mutation, namespaces, and XPath in the existing XML bridge.
- [x] Expose the three functions and both classes through EIR-compatible prelude declarations and shared contracts.
- [x] Cover property/attribute access, iteration, conversions, DOM import, errors, and eval with focused tests.
- [x] Update generated coverage, user documentation, and a runnable example.
- [x] Open a draft PR and route heavy validation and regression checks through CI.
- [ ] Complete native typed scalar/union reference publishers, protected declaration validators and per-binding lifecycle.
- [ ] Connect invisible validation roots and the weak type-source registry to promotion, clone, unset, destruction and statics.
- [ ] Preserve the current PHP type through native reference body writes and eval barriers.
- [ ] Finish suspended Fiber owner inventories and verify native Reflection destructor ordering.
- [ ] Obtain a clean full-work installed-default Grok review and green CI on the same final head.

Reuse the managed libxml2 package and its opaque C shim on all supported targets. Keep documents alive while any selection references them. Select children and attributes lazily so mutations remain visible to existing wrappers. Expose PHP 8.5 signatures and preserve custom element subclasses. Use the existing XML bridge capability for opaque eval source.

Validation follows the user's instruction: local read-only and lightweight source checks only; compilation, linking, executable tests, and target coverage run in the PR's CI.

Draft PR: https://github.com/illegalstudio/elephc/pull/2060. Earlier CI runs passed both Linux builds, both iOS XML/SimpleXML links, native ownership tests, and the isolated array/JSON heap regressions. The integrated work also covers live DOM identity and relationships, text and namespace writes, XPath contexts, and guarded interface receivers. All 22 public PHP class methods are present, including the debug projection. Native and boxed debug calls, side-effecting property-name reads, and production CLI prelude type checking have dedicated regressions. Generated documentation comes from CI artifacts and local JSON-based audits, without invoking Cargo.

The user authorized the supporting native reference refactor and unlimited review rounds. Exact-Mixed native properties and constructor bindings share persistent references with eval. Typed scalar/union promotion remains disabled while generated publishers, validators and lifecycle integration are unfinished. Canonical readers, a weak indexed source registry, original-RHS coercion coordination and invisible native validation roots are preparatory components. Each initially matching property source must independently validate the original RHS; removing a source during coercion does not discard its in-flight result. Temporary roots must preserve PHP clone and array-copy sharing semantics. Keep the PR draft throughout implementation. Greptile is waived because its credits are exhausted; final independent review uses installed-default Grok.

Native reads use the canonical child's current PHP tag. Suspended activation inventories cover source leases, pending native pins, binder conversions, argument arrays and synthesized magic arguments. Cleanup keeps its held result and remaining argument owners visible until each lease is transferred or released. The strict cycle, cancellation and destructor-order regressions remain required CI gates.

Post-eval typed native parameter reads use explicit owning Mixed value copies while keeping their entry cell payload ABI. Shared canonical wrapper reloads bypass the legacy projection. Scalar parameter body writes invalidate their stale entry view in value-returning functions. Reference-returning functions retain their existing cell ABI until return coercion can join the same validation boundary. General reference rebinding, object/union body writers, reference-return coercion, expression-result coercion and universal promotion remain open integration gates until their lifecycle and CI coverage are complete.

Reflection construction no longer snapshots live static values. Generic accessors build their current maps lazily through EIR and omit uninitialized properties, preserving the destruction timing of displaced objects.

Statement append and indexed native-reference writes now select concrete raw storage or a live borrowed canonical child, keeping value-copy reads separate from mutation receivers. Their CI regressions cover canonical string and array updates, a headerless raw string cell, raw array writes after opaque eval, ordinary caller aliases and both optimizer modes. Expression, nested-write and iterator consumers still need the same integration audit before this gate closes. Typed literal reassignments and the complete caller/reference-return flow remain open; the gated native body fixture uses an unknown Mixed RHS and still requires the new PHP type, exact mutations and zero live owners.
