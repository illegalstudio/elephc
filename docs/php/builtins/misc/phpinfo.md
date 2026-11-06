---
title: "phpinfo()"
description: "Outputs information about the PHP runtime and its loaded modules."
---

## phpinfo()

```php
function phpinfo(int $flags = 4294967295): bool
```

Outputs information about the PHP runtime and its loaded modules.

**Parameters**:
- `$flags` (`int`), default `4294967295`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `phpinfo` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/phpinfo.md).
