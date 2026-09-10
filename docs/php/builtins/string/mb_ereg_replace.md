---
title: "mb_ereg_replace()"
description: "Replaces multibyte regex matches using numbered and named capture references."
sidebar:
  order: 811
---

## mb_ereg_replace()

```php
function mb_ereg_replace(string $pattern, string $replacement, string $string, ?string $options = null): string|false|null
```

Replaces multibyte regex matches using numbered and named capture references.

**Parameters**:
- `$pattern` (`string`)
- `$replacement` (`string`)
- `$string` (`string`)
- `$options` (`?string`), default `null`, optional

**Returns**: `string|false|null`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_ereg_replace.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_ereg_replace.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_ereg_replace` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_ereg_replace.md).
