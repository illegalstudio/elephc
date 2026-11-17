---
title: "mb_substitute_character()"
description: "Reads or changes the replacement codepoint or mode used for invalid or unrepresentable characters."
---

## mb_substitute_character()

```php
function mb_substitute_character(string|int|null $substitute_character = null): string|int|bool
```

Reads or changes the replacement codepoint or mode used for invalid or unrepresentable characters.

**Parameters**:
- `$substitute_character` (`string|int|null`), default `null`, optional

**Returns**: `string|int|bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_substitute_character` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_substitute_character.md).
