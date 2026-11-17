---
title: "get_extension_funcs()"
description: "Returns functions exported by a loaded extension or false."
---

## get_extension_funcs()

```php
function get_extension_funcs(string $extension): mixed
```

Returns functions exported by a loaded extension or false.

**Parameters**:
- `$extension` (`string`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `get_extension_funcs` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/get_extension_funcs.md).
