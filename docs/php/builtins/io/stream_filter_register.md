---
title: "stream_filter_register()"
description: "Registers a user-defined stream filter."
---

## stream_filter_register()

```php
function stream_filter_register(string $filter_name, string $class): bool
```

Registers a user-defined stream filter.

**Parameters**:
- `$filter_name` (`string`)
- `$class` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `stream_filter_register` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_filter_register.md).
