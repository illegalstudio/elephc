---
title: "php_uname()"
description: "Returns information about the operating system PHP is running on."
---

## php_uname()

```php
function php_uname(string $mode = 'a'): string
```

Returns information about the operating system PHP is running on.

**Parameters**:
- `$mode` (`string`), default `'a'`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `php_uname` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/php_uname.md).
