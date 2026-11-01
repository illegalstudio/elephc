---
id: gotcha-class-alias-creates-a-separate-class-identity-in-00e40c1b
type: gotcha
title: "class_alias() creates a separate class identity instead of an alias"
description: "get_class() on an instance built through a class_alias name answers the ALIAS, where PHP answers the original class"
tags: [class-alias, identity]
created: 2026-09-16
verified_by: "scratchpad/classalias.php compared against php -n, same result with autoload true and false"
sources:
  - path: src/builtins/callables/class_alias.rs
    blob: 94a302d78c3f3acbc14d671ec56f69fa882063bf
  - path: src/autoload/alias.rs
    blob: 4fc6ac94f3cf9c91afe2129ce8cc382046866618
---

# class_alias() creates a separate class identity instead of an alias

## Fact

    namespace Inner { class Original {} }
    namespace {
        class_alias(\Inner\Original::class, \Aliased::class, false);
        echo get_class(new \Aliased());
    }

php prints `Inner\Original`; elephc prints `Aliased`. The alias gets its own class identity in the
AOT class table rather than resolving to the original's. `instanceof` and method dispatch are
correct, so only the REPORTED name diverges.

PRE-EXISTING and independent of the `$autoload` flag: the same divergence appears with `true`,
which was always the accepted form. Noted while fixing the separate bug where a literal `false`
third argument made the whole call unresolvable (Symfony's generated container emits exactly
that).

Not on Symfony's hot path: the container object is constructed as
`new \ContainerXXXX\App_KernelProdContainer(...)`, never through the alias, and the alias is only
read by `class_exists`.
