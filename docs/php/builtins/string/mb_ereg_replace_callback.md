---
title: "mb_ereg_replace_callback()"
description: "Replaces multibyte regex matches with the string result of a callback receiving all captures."
sidebar:
  order: 812
---

## mb_ereg_replace_callback()

```php
function mb_ereg_replace_callback(string $pattern, callable $callback, string $string, ?string $options = null): string|false|null
```

Replaces multibyte regex matches with the string result of a callback receiving all captures.

**Parameters**:
- `$pattern` (`string`)
- `$callback` (`callable`)
- `$string` (`string`)
- `$options` (`?string`), default `null`, optional

**Returns**: `string|false|null`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_ereg_replace_callback.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_ereg_replace_callback.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_ereg_replace_callback` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_ereg_replace_callback.md).
