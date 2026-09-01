---
title: "setlocale()"
description: "Sets locale information from ordered candidates."
sidebar:
  order: 337
---

## setlocale()

```php
function setlocale(int $category, mixed $locales, ...$rest): mixed
```

Sets locale information from ordered candidates.

**Parameters**:
- `$category` (`int`)
- `$locales` (`mixed`)
- `...$rest` — variadic: collects excess arguments into `$rest`.

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/setlocale.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/setlocale.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `setlocale` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/setlocale.md).
