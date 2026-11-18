---
title: "libxml_use_internal_errors()"
description: "Switches libxml error buffering on or off and answers the previous setting."
---

## libxml_use_internal_errors()

```php
function libxml_use_internal_errors(?bool $use_errors = null): bool
```

Switches libxml error buffering on or off and answers the previous setting.

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
