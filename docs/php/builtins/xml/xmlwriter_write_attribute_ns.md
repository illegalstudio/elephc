---
title: "xmlwriter_write_attribute_ns()"
description: "Writes a complete namespaced attribute."
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

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_write_attribute_ns` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_write_attribute_ns.md).
