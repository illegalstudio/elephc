---
title: "parse_str()"
description: "Parses a URL-encoded query string into variables."
---

## parse_str()

```php
function parse_str(string $string, mixed $result): void
```

Parses a URL-encoded query string into variables.

**Parameters**:
- `$string` (`string`)
- `$result` (`mixed`), passed by reference

**Returns**: `void`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `parse_str` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/parse_str.md).
