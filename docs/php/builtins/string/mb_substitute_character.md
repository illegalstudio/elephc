---
title: "mb_substitute_character()"
description: "Reads or changes the replacement codepoint or mode used for invalid or unrepresentable characters."
sidebar:
  order: 855
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

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_substitute_character.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_substitute_character.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_substitute_character` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_substitute_character.md).
