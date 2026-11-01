---
id: bug_fix-a-method-s-declared-callable-parameter-was-not-r-7927c99a
type: bug_fix
title: "A method's declared callable parameter was not registered, and a method's errors never named their class"
description: "Two method-pass gaps found together: callable_param_names was populated only for plain functions, and method-body diagnostics reached the reporter with no declaration so they printed a bare line:col"
created: 2026-09-18
verified_by: "index_preload_hotpath.php --web went from 1 error to 0 and linked; error_tests stayed at 99 failures with identical names"
sources:
  - path: src/types/checker/method_pass.rs
    blob: aaff5093c15551130572106bc8d30bbb9a248e73
  - path: src/types/checker/functions/resolution/signature.rs
    blob: 5e39731b2aaf72b5e0b4ca0f7b66a92702b0ff5f
---

# A method's declared callable parameter was not registered, and a method's errors never named their class

## Fact

TWO GAPS, both because `method_pass` did less than `resolve_function_signature`:

1. A parameter DECLARED `callable` carries no signature anyone can check against -- its real one
   arrives with the argument. `resolve_function_signature` records those names in
   `callable_param_names`, and `inference/ops` consults it to route the call through
   `check_known_callable_call_allowing_by_ref_spread`, so a spread is not refused for the
   by-reference parameters of whatever signature inference happened to guess. That registration
   NEVER RAN FOR METHODS, so the identical parameter was accepted in a function and refused in a
   method. Symfony's
   `PhpFileLoader::callConfigurator(callable $callback, ...)` -> `$callback(...$arguments)` is
   exactly that shape and was the last error in the hotpath preload.

   NOTE FOR A FUTURE REDUCTION: the obvious small fixtures do NOT reproduce it. A method taking
   `callable $cb` and spreading into it compiles fine even with the registration disabled, and so
   does the `$cb = $cb(...)` rebinding. The refusal needs whatever signature inference attaches in
   the real file; two attempts at a minimal repro both passed either way, so the change is pinned
   by the preload compiling, not by a unit fixture.

2. `method_pass` extended its errors with `error.flatten()` and no `within_declaration`, so a
   method-body diagnostic reached the reporter untagged and printed as a bare `error[213:13]` --
   meaningless after autoload splicing puts every file in one line-number space. Tagging with the
   class name makes `pipeline`'s `resolve_declaration_files` recover the path:

       error[/.../vendor/symfony/dependency-injection/Loader/PhpFileLoader.php:213:13]: ...

   `ELEPHC_BACKEND_INVENTORY=1` already prefixed the MESSAGE with `Class::method`, which is a
   different thing and does not give the file.

WHERE THE PRELOAD STANDS NOW: `index_preload_hotpath.php --web` COMPILES AND LINKS with zero
errors. It does not BOOT. The next blocker is a semantic one, not a type error:

    Fatal error: Uncaught LogicException: You cannot use the "...AsciiSlugger" as the
    "symfony/translation-contracts" package is not installed.

`AsciiSlugger.php` guards itself AT FILE SCOPE (`if (!interface_exists(LocaleAwareInterface::class))
{ throw ... }` above the class). Real PHP only runs that when something autoloads the class, and
this app never does. elephc pulled the file into the closed world and runs its file-scope
statements at startup, so a guard meant for a missing optional package fires in an app that never
touches it. Any file-scope side effect in an autoloaded class file has the same shape.

## Why

It was the last checker blocker on the bigger Symfony preload, which is the measured performance lever
