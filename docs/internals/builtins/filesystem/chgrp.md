---
title: "chgrp() — internals"
description: "Compiler internals for chgrp(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 100
---

## `chgrp()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/chgrp.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/chgrp.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:4478](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L4478) (`lower_chgrp`)
- **Function symbol**: `lower_chgrp()`


### Lowering notes

- Lowers `chgrp(path, group)` for integer GIDs and string group names.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_umask`

## Signature summary

```php
function chgrp(string $filename, string $group): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `chgrp()`](../../../php/builtins/filesystem/chgrp.md)
