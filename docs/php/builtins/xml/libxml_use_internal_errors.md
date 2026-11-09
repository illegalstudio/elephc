---
title: "libxml_use_internal_errors()"
description: "Collects libxml errors in a list instead of raising warnings; returns the previous mode."
---

## libxml_use_internal_errors()

```php
function libxml_use_internal_errors(?bool $use_errors = null): bool
```

Collects libxml errors in a list instead of raising warnings; returns the previous mode.

**Parameters**:
- `$use_errors` (`?bool`), default `null`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `libxml_use_internal_errors` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/libxml_use_internal_errors.md).
