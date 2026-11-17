---
title: "ini_get()"
description: "Returns the value of a configuration directive."
---

## ini_get()

```php
function ini_get(string $option): mixed
```

Returns the value of a configuration directive.

**Parameters**:
- `$option` (`string`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ini_get` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/ini_get.md).
