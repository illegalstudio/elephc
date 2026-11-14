---
title: "tidy_parse_string()"
description: "Parses a string of HTML, applies the optional configuration and returns a tidy document object."
---

## tidy_parse_string()

```php
function tidy_parse_string(string $string, array|string|null $config = null, ?string $encoding = null): mixed
```

Parses a string of HTML, applies the optional configuration and returns a tidy document object.

**Parameters**:
- `$string` (`string`)
- `$config` (`array|string|null`), default `null`, optional
- `$encoding` (`?string`), default `null`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `tidy_parse_string` is implemented in the compiler, see [the internals page](../../../internals/builtins/tidy/tidy_parse_string.md).
