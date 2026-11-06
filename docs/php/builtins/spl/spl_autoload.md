---
title: "spl_autoload()"
description: "Default implementation for __autoload()."
---

## spl_autoload()

```php
function spl_autoload(string $class, string $file_extensions = null): void
```

Default implementation for __autoload().

**Parameters**:
- `$class` (`string`)
- `$file_extensions` (`string`), default `null`, optional

**Returns**: `void`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `spl_autoload` is implemented in the compiler, see [the internals page](../../../internals/builtins/spl/spl_autoload.md).
