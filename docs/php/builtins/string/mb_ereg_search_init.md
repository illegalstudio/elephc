---
title: "mb_ereg_search_init()"
description: "Initializes the retained multibyte regex subject and optionally compiles a search pattern."
---

## mb_ereg_search_init()

```php
function mb_ereg_search_init(string $string, ?string $pattern = null, ?string $options = null): bool
```

Initializes the retained multibyte regex subject and optionally compiles a search pattern.

**Parameters**:
- `$string` (`string`)
- `$pattern` (`?string`), default `null`, optional
- `$options` (`?string`), default `null`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_ereg_search_init` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_ereg_search_init.md).
