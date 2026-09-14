---
title: "mb_check_encoding()"
description: "Checks encoded strings or array keys and values recursively for invalid byte sequences."
sidebar:
  order: 798
---

## mb_check_encoding()

```php
function mb_check_encoding(array|string|null $value = null, ?string $encoding = null): bool
```

Checks encoded strings or array keys and values recursively for invalid byte sequences.

**Parameters**:
- `$value` (`array|string|null`), default `null`, optional
- `$encoding` (`?string`), default `null`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_check_encoding.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_check_encoding.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_check_encoding` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_check_encoding.md).
