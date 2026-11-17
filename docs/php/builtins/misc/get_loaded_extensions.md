---
title: "get_loaded_extensions()"
description: "Returns an array with the names of all loaded modules."
---

## get_loaded_extensions()

```php
function get_loaded_extensions(bool $zend_extensions = false): array
```

Returns an array with the names of all loaded modules.

**Parameters**:
- `$zend_extensions` (`bool`), default `false`, optional

**Returns**: `array`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `get_loaded_extensions` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/get_loaded_extensions.md).
