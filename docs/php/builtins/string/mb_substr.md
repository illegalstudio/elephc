---
title: "mb_substr()"
description: "Selects a substring using character offsets and an optional character length."
sidebar:
  order: 856
---

## mb_substr()

```php
function mb_substr(string $string, int $start, ?int $length = null, ?string $encoding = null): string
```

Selects a substring using character offsets and an optional character length.

**Parameters**:
- `$string` (`string`)
- `$start` (`int`)
- `$length` (`?int`), default `null`, optional
- `$encoding` (`?string`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_substr.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_substr.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_substr` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_substr.md).
