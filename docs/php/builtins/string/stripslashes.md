---
title: "stripslashes()"
description: "Removes backslashes from a string previously escaped by addslashes."
---

## stripslashes()

```php
function stripslashes(string $string): string
```

Removes backslashes from a string previously escaped by addslashes.

**Parameters**:
- `$string` (`string`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `stripslashes` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/stripslashes.md).
