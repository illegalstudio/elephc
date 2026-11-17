---
title: "extension_loaded()"
description: "Checks whether a named PHP extension is loaded."
---

## extension_loaded()

```php
function extension_loaded(string $extension): bool
```

Checks whether a named PHP extension is loaded.

**Parameters**:
- `$extension` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `extension_loaded` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/extension_loaded.md).
