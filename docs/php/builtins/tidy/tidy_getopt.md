---
title: "tidy_getopt()"
description: "Returns the current value of one configuration option."
---

## tidy_getopt()

```php
function tidy_getopt(mixed $tidy, string $option): string|int|bool
```

Returns the current value of one configuration option.

**Parameters**:
- `$tidy` (`mixed`)
- `$option` (`string`)

**Returns**: `string|int|bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `tidy_getopt` is implemented in the compiler, see [the internals page](../../../internals/builtins/tidy/tidy_getopt.md).
