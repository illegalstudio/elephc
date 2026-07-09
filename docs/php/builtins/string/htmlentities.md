---
title: "htmlentities()"
description: "Converts all applicable characters in a string into their HTML entities."
sidebar:
  order: 357
---

## htmlentities()

```php
function htmlentities(string $string, int $flags = 11, string $encoding = 'UTF-8'): string
```

Converts all applicable characters in a string into their HTML entities.

**Parameters**:
- `$string` (`string`)
- `$flags` (`int`), default `11`, optional
- `$encoding` (`string`), default `'UTF-8'`, optional

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `htmlentities` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/htmlentities.md).

