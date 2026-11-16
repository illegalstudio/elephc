---
title: "mb_scrub()"
description: "Replaces malformed encoded units using the current substitution setting."
---

## mb_scrub()

```php
function mb_scrub(string $string, ?string $encoding = null): string
```

Replaces malformed encoded units using the current substitution setting.

**Parameters**:
- `$string` (`string`)
- `$encoding` (`?string`), default `null`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_scrub` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_scrub.md).
