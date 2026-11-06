---
title: "ctype_punct()"
description: "Checks if all characters in the string are punctuation."
sidebar:
  order: 911
---

## ctype_punct()

```php
function ctype_punct(string $text): bool
```

Checks if all characters in the string are punctuation.

**Parameters**:
- `$text` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/ctype_punct.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/ctype_punct.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ctype_punct` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/ctype_punct.md).
