---
title: "mb_regex_set_options()"
description: "Reads multibyte regex options, or changes them and returns the previous canonical option string."
---

## mb_regex_set_options()

```php
function mb_regex_set_options(?string $options = null): string
```

Reads multibyte regex options, or changes them and returns the previous canonical option string.

**Parameters**:
- `$options` (`?string`), default `null`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_regex_set_options` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_regex_set_options.md).
