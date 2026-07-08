---
title: "strstr()"
description: "Returns the portion of a string starting at the first occurrence of a substring."
sidebar:
  order: 396
---

## strstr()

```php
function strstr(string $haystack, string $needle, bool $before_needle = false): string
```

Returns the portion of a string starting at the first occurrence of a substring.

**Parameters**:
- `$haystack` (`string`)
- `$needle` (`string`)
- `$before_needle` (`bool`), default `false`, optional

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `strstr` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/strstr.md).

