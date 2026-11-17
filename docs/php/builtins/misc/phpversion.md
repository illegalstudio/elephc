---
title: "phpversion()"
description: "Returns the targeted PHP language version, or one extension's version."
---

## phpversion()

```php
function phpversion(?string $extension = null): string|false
```

Returns the targeted PHP language version, or one extension's version.

**Parameters**:
- `$extension` (`?string`), default `null`, optional

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `phpversion` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/phpversion.md).
