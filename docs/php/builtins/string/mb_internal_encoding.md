---
title: "mb_internal_encoding()"
description: "Reads or changes the default encoding used by mbstring text operations."
sidebar:
  order: 825
---

## mb_internal_encoding()

```php
function mb_internal_encoding(?string $encoding = null): string|bool
```

Reads or changes the default encoding used by mbstring text operations.

**Parameters**:
- `$encoding` (`?string`), default `null`, optional

**Returns**: `string|bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_internal_encoding.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_internal_encoding.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_internal_encoding` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_internal_encoding.md).
