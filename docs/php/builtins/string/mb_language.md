---
title: "mb_language()"
description: "Reads or changes the current mbstring language."
sidebar:
  order: 826
---

## mb_language()

```php
function mb_language(?string $language = null): string|bool
```

Reads or changes the current mbstring language.

**Parameters**:
- `$language` (`?string`), default `null`, optional

**Returns**: `string|bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_language.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_language.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_language` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_language.md).
