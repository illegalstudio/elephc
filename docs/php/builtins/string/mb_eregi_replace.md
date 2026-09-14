---
title: "mb_eregi_replace()"
description: "Replaces multibyte regex matches without case sensitivity."
sidebar:
  order: 821
---

## mb_eregi_replace()

```php
function mb_eregi_replace(string $pattern, string $replacement, string $string, ?string $options = null): string|false|null
```

Replaces multibyte regex matches without case sensitivity.

**Parameters**:
- `$pattern` (`string`)
- `$replacement` (`string`)
- `$string` (`string`)
- `$options` (`?string`), default `null`, optional

**Returns**: `string|false|null`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_eregi_replace.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_eregi_replace.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_eregi_replace` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_eregi_replace.md).
