---
title: "addcslashes()"
description: "Returns a string with backslashes before characters listed in a character set, including ranges."
---

## addcslashes()

```php
function addcslashes(string $string, string $characters): string
```

Returns a string with backslashes before characters listed in a character set, including ranges.

**Parameters**:
- `$string` (`string`)
- `$characters` (`string`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `addcslashes` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/addcslashes.md).
