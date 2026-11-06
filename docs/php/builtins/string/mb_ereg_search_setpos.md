---
title: "mb_ereg_search_setpos()"
description: "Changes the progressive regex byte position, accepting negative offsets relative to the retained subject."
---

## mb_ereg_search_setpos()

```php
function mb_ereg_search_setpos(int $offset): bool
```

Changes the progressive regex byte position, accepting negative offsets relative to the retained subject.

**Parameters**:
- `$offset` (`int`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_ereg_search_setpos` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_ereg_search_setpos.md).
