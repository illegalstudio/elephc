---
title: "var_export()"
description: "Renders a value as parsable PHP code, printed or returned."
---

## var_export()

```php
function var_export(mixed $value, bool $return = false): mixed
```

Renders a value as parsable PHP code, printed or returned.

**Parameters**:
- `$value` (`mixed`)
- `$return` (`bool`), default `false`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `var_export` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/var_export.md).
