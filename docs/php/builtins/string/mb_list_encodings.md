---
title: "mb_list_encodings()"
description: "Lists every supported canonical encoding in PHP order."
sidebar:
  order: 858
---

## mb_list_encodings()

```php
function mb_list_encodings(): array
```

Lists every supported canonical encoding in PHP order.

**Parameters**: none.

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_list_encodings.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_list_encodings.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_list_encodings` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_list_encodings.md).
