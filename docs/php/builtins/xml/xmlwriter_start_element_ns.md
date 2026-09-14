---
title: "xmlwriter_start_element_ns()"
description: "Starts a namespaced element."
sidebar:
  order: 1021
---

## xmlwriter_start_element_ns()

```php
function xmlwriter_start_element_ns(mixed $writer, ?string $prefix, string $name, ?string $namespace): bool
```

Starts a namespaced element.

**Parameters**:
- `$writer` (`mixed`)
- `$prefix` (`?string`)
- `$name` (`string`)
- `$namespace` (`?string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_start_element_ns.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_start_element_ns.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_start_element_ns` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_start_element_ns.md).
