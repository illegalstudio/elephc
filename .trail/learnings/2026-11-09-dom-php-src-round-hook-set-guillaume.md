---
title: "DOM php-src round: hook set types, mixed property warnings, merge temps, DOM debug hook"
author: Guillaume
date: 2026-11-09
branch: feat/dom-php-src-compliance
touches:
  - crates/elephc-builtin-contract/src/catalog_classes.rs
  - crates/elephc-xml/src/dom/abi.rs
  - crates/elephc-xml/src/dom/ffi.rs
  - crates/elephc-xml/src/dom/mod.rs
  - src/codegen/lower_inst/objects/known_property_reads.rs
  - src/codegen/lower_inst/objects/mixed_property_reads.rs
  - src/codegen_support/runtime/data/user.rs
  - src/dom_html_prelude.rs
  - src/dom_html_prelude/detect.rs
  - src/dom_html_prelude/legacy.rs
---

## Decided
- Oracle is php-src 8.5.8 built against libxml2 2.15.3 (../php-dom-oracle/php-src-8.5.8/sapi/cli/php -n). The system php links libxml2 2.9.13 and differs on name validation and warnings.
- A `set(T $value)` hook parameter type is kept by the parser; writes are checked against it, not the property type (php semantics). The AST printer prints it when it differs from the property type.
- DOM nodes dump through the hidden `__elephcDebugInfo()` (php_ext DEBUG_INFO_METHOD); internal prelude classes declaring it get a `_class_debuginfo_ptrs` entry, so `__debugInfo()` stays undefined like php.
- Unbound DOM nodes: hooks read `__elephc_state()` (DOMException 11), methods `__elephc_handle()` (Error "Couldn't fetch X"), simplexml uses `__elephc_raw()`.
- Name checks are libxml2 `xmlValidateName(name, 0)` in native create/setAttribute (DOMException 5).
- String loads set doc URL to getcwd()/ with xmlNodeSetBase (php-src dom_document_parser).
## Tried and ruled out
- `array_merge` with 3 args or on AssocArray<string,mixed> in a prelude: refused (2-arg contract) or unsupported in the backend; use `+` for disjoint keys.
- WeakReference for the DOM identity cache: correct lifetimes but each wrapper costs a php object id (php DOM allocates none).
## Watch out
- IR merge temps typed a property read on a `mixed` receiver as the syntactic `int`; now `mixed` (merge_temps.rs).
- `_php_diagnostic_line` is only published by some instructions; a cold warning path must call `publish_diagnostic_site`.
- A shim change under the same recipe revision needs the r12 artifact cache deleted and `elephc native install --locked` rerun.
