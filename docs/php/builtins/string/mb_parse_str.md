---
title: "mb_parse_str()"
description: "Parses URL-encoded input with shared encoding detection and writes decoded variables by reference."
---

## mb_parse_str()

```php
function mb_parse_str(string $string, mixed $result): bool
```

Parses URL-encoded input with shared encoding detection and writes decoded variables by reference.

**Parameters**:
- `$string` (`string`)
- `$result` (`mixed`), passed by reference

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_parse_str` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_parse_str.md).
