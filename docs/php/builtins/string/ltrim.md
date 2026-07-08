---
title: "ltrim()"
description: "Strips whitespace (or other characters) from the beginning of a string."
sidebar:
  order: 369
---

## ltrim()

```php
function ltrim(string $string, string $characters = ' \n\r\t\x0b\x0c\x00'): string
```

Strips whitespace (or other characters) from the beginning of a string.

**Parameters**:
- `$string` (`string`)
- `$characters` (`string`), default `' \n\r\t\x0b\x0c\x00'`, optional

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ltrim` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/ltrim.md).

