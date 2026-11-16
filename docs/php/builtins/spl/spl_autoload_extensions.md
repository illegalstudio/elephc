---
title: "spl_autoload_extensions()"
description: "Register and return default file extensions for spl_autoload."
---

## spl_autoload_extensions()

```php
function spl_autoload_extensions(string $file_extensions = null): string
```

Register and return default file extensions for spl_autoload.

**Parameters**:
- `$file_extensions` (`string`), default `null`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `spl_autoload_extensions` is implemented in the compiler, see [the internals page](../../../internals/builtins/spl/spl_autoload_extensions.md).
