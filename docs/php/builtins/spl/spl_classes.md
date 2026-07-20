---
title: "spl_classes()"
description: "Return available SPL classes."
sidebar:
  order: 347
---

## spl_classes()

```php
function spl_classes(): array
```

Return available SPL classes.

**Parameters**: none.

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/spl_classes.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/spl_classes.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `spl_classes` is implemented in the compiler, see [the internals page](../../../internals/builtins/spl/spl_classes.md).

