---
title: "xmlwriter_write_element_ns()"
description: "Writes a complete namespaced element."
---

## xmlwriter_write_element_ns()

```php
function xmlwriter_write_element_ns(mixed $writer, ?string $prefix, string $name, ?string $namespace, ?string $content = null): bool
```

Writes a complete namespaced element.

**Parameters**:
- `$writer` (`mixed`)
- `$prefix` (`?string`)
- `$name` (`string`)
- `$namespace` (`?string`)
- `$content` (`?string`), default `null`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_write_element_ns` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_write_element_ns.md).
