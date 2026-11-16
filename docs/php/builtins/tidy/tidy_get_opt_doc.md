---
title: "tidy_get_opt_doc()"
description: "Returns libtidy's documentation for a configuration option, or false when it has none."
---

## tidy_get_opt_doc()

```php
function tidy_get_opt_doc(mixed $tidy, string $option): string|false
```

Returns libtidy's documentation for a configuration option, or false when it has none.

**Parameters**:
- `$tidy` (`mixed`)
- `$option` (`string`)

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `tidy_get_opt_doc` is implemented in the compiler, see [the internals page](../../../internals/builtins/tidy/tidy_get_opt_doc.md).
