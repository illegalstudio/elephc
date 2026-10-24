# SimpleXML

- [x] Implement document ownership, live selections, mutation, namespaces, and XPath in the existing XML bridge.
- [x] Expose the three functions and both classes through EIR-compatible prelude declarations and shared contracts.
- [x] Cover property/attribute access, iteration, conversions, DOM import, errors, and eval with focused tests.
- [x] Update generated coverage, user documentation, and a runnable example.
- [ ] Open a draft PR and resolve CI failures without running local heavy builds or tests.

Reuse the managed libxml2 package and its opaque C shim on all supported targets. Keep documents alive while any selection references them. Select children and attributes lazily so mutations remain visible to existing wrappers. Expose PHP 8.5 signatures and preserve custom element subclasses. Use the existing XML bridge capability for opaque eval source.

Validation follows the user's instruction: local read-only and lightweight source checks only; compilation, linking, executable tests, and target coverage run in the draft PR's CI.

Draft PR: https://github.com/illegalstudio/elephc/pull/2060. CI has built both Linux Rust test archives. Native execution and iOS linking are being unblocked by correcting prelude type errors; generated documentation is imported from the CI artifact and audited locally without invoking Cargo.
