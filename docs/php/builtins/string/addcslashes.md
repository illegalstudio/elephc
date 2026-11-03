---
title: "addcslashes()"
description: "Quotes the listed characters with C-style backslash escapes."
sidebar:
  order: 895
---

## addcslashes()

```php
function addcslashes(string $string, string $characters): string
```

Quotes the listed characters with C-style backslash escapes.

**Parameters**:
- `$string` (`string`)
- `$characters` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected addcslashes prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/addcslashes.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/addcslashes.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `addcslashes` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/addcslashes.md).
