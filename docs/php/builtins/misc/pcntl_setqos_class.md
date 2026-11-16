---
title: "pcntl_setqos_class()"
description: "Changes the current macOS thread quality-of-service class."
---

## pcntl_setqos_class()

```php
function pcntl_setqos_class(mixed $qos_class): void
```

Changes the current macOS thread quality-of-service class.

**Parameters**:
- `$qos_class` (`mixed`)

**Returns**: `void`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_setqos_class` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_setqos_class.md).
