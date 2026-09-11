---
title: "mb_ereg_search()"
description: "Searches the retained multibyte subject and advances the shared byte position."
sidebar:
  order: 813
---

## mb_ereg_search()

```php
function mb_ereg_search(?string $pattern = null, ?string $options = null): bool
```

Searches the retained multibyte subject and advances the shared byte position.

**Parameters**:
- `$pattern` (`?string`), default `null`, optional
- `$options` (`?string`), default `null`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_ereg_search.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_ereg_search.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_ereg_search` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_ereg_search.md).
