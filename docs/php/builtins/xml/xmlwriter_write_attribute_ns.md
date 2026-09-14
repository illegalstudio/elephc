---
title: "xmlwriter_write_attribute_ns()"
description: "Writes a complete namespaced attribute."
sidebar:
  order: 1025
---

## xmlwriter_write_attribute_ns()

```php
function xmlwriter_write_attribute_ns(mixed $writer, ?string $prefix, string $name, ?string $namespace, string $value): bool
```

Writes a complete namespaced attribute.

**Parameters**:
- `$writer` (`mixed`)
- `$prefix` (`?string`)
- `$name` (`string`)
- `$namespace` (`?string`)
- `$value` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_write_attribute_ns.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_write_attribute_ns.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_write_attribute_ns` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_write_attribute_ns.md).
