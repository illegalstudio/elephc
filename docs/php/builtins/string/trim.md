---
title: "trim()"
description: "Strips whitespace (or other characters) from the beginning and end of a string."
sidebar:
  order: 401
---

## trim()

```php
function trim(string $string, string $characters = ' \n\r\t\x0b\x0c\x00'): string
```

Strips whitespace (or other characters) from the beginning and end of a string.

**Parameters**:
- `$string` (`string`)
- `$characters` (`string`), default `' \n\r\t\x0b\x0c\x00'`, optional

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `trim` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/trim.md).

