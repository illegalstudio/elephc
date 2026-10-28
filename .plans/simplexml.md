# SimpleXML

- [x] Implement document ownership, live selections, mutation, namespaces, and XPath in the existing XML bridge.
- [x] Expose the three functions and both classes through EIR-compatible prelude declarations and shared contracts.
- [x] Cover property/attribute access, iteration, conversions, DOM import, errors, and eval with focused tests.
- [x] Update generated coverage, user documentation, and a runnable example.
- [x] Open a draft PR and route heavy validation and regression checks through CI.

Reuse the managed libxml2 package and its opaque C shim on all supported targets. Keep documents alive while any selection references them. Select children and attributes lazily so mutations remain visible to existing wrappers. Expose PHP 8.5 signatures and preserve custom element subclasses. Use the existing XML bridge capability for opaque eval source.

Validation follows the user's instruction: local read-only and lightweight source checks only; compilation, linking, executable tests, and target coverage run in the PR's CI.

Ready PR: https://github.com/illegalstudio/elephc/pull/2060. Earlier CI runs passed both Linux builds, both iOS XML/SimpleXML links, native ownership tests, and the isolated array/JSON heap regressions. The integrated work also covers live DOM identity and relationships, text and namespace writes, XPath contexts, and guarded interface receivers. All 22 public PHP class methods are present, including the debug projection. Completion requires a clean full-work Claude review and green CI on the same current head. Native and boxed debug calls, side-effecting property-name reads, and production CLI prelude type checking have dedicated regressions. Generated documentation comes from CI artifacts and local JSON-based audits, without invoking Cargo.
