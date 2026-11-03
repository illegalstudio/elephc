---
title: "preg_filter()"
description: "Performs a regular expression search and replace, keeping only the subjects that matched."
sidebar:
  order: 776
---

## preg_filter()

```php
function preg_filter(mixed $pattern, mixed $replacement, mixed $subject, int $limit = -1, mixed $count = null): mixed
```

Performs a regular expression search and replace, keeping only the subjects that matched.

**Parameters**:
- `$pattern` (`mixed`)
- `$replacement` (`mixed`)
- `$subject` (`mixed`)
- `$limit` (`int`), default `-1`, optional
- `$count` (`mixed`), passed by reference, default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected preg_array prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/regex/preg_filter.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/regex/preg_filter.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `preg_filter` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/preg_filter.md).
