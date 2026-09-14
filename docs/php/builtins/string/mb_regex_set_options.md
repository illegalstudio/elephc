---
title: "mb_regex_set_options()"
description: "Reads multibyte regex options, or changes them and returns the previous canonical option string."
sidebar:
  order: 835
---

## mb_regex_set_options()

```php
function mb_regex_set_options(?string $options = null): string
```

Reads multibyte regex options, or changes them and returns the previous canonical option string.

**Parameters**:
- `$options` (`?string`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_regex_set_options.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_regex_set_options.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_regex_set_options` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_regex_set_options.md).
