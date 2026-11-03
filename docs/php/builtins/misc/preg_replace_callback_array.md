---
title: "preg_replace_callback_array()"
description: "Performs a regular expression search and replace using one callback per pattern."
sidebar:
  order: 780
---

## preg_replace_callback_array()

```php
function preg_replace_callback_array(array $pattern, mixed $subject, int $limit = -1, mixed $count = null, int $flags = 0): mixed
```

Performs a regular expression search and replace using one callback per pattern.

**Parameters**:
- `$pattern` (`array`)
- `$subject` (`mixed`)
- `$limit` (`int`), default `-1`, optional
- `$count` (`mixed`), passed by reference, default `null`, optional
- `$flags` (`int`), default `0`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected preg_array prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/regex/preg_replace_callback_array.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/regex/preg_replace_callback_array.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `preg_replace_callback_array` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/preg_replace_callback_array.md).
