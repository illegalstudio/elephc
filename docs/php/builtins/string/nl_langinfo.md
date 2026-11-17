---
title: "nl_langinfo()"
description: "Returns locale information for one item; elephc exposes the portable CODESET query."
---

## nl_langinfo()

```php
function nl_langinfo(int $item): string|false
```

Returns locale information for one item; elephc exposes the portable CODESET query.

**Parameters**:
- `$item` (`int`)

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `nl_langinfo` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/nl_langinfo.md).
