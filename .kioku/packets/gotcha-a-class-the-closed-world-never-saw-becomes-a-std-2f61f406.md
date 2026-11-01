---
id: gotcha-a-class-the-closed-world-never-saw-becomes-a-std-2f61f406
type: gotcha
title: "A class the closed world never saw becomes a stdClass the moment a compiled frame looks at it"
description: "The Symfony controller service read as stdClass to every compiled builtin while the interpreter still named it correctly; the cause is the class being absent from the compiled world, not a broken object"
created: 2026-09-18
verified_by: "A --web probe built from the preload list: container-get and through-object both report get_class=stdClass, method_exists=false, is_callable=false, while the preloaded error_controller reports its real class; grep of the 1.5 GB .s shows App\\Controller\\HelloController present only as two .ascii strings, with no compiled class"
sources:
  - path: examples/symfony-app/public/index_preload_hotpath.php
    blob: e8b652d89f5a0ff3c110d05122a91cc8517e574b
  - path: crates/elephc-magician/src/interpreter/builtins/types/get_debug_type.rs
    blob: 9dad6273f9dafe25c48d06db53860aeed2b45023
  - path: crates/elephc-magician/src/context/classlike_objects.rs
    blob: 13c61d5fef0c046b60415ea88989dcde49835ec6
---

# A class the closed world never saw becomes a stdClass the moment a compiled frame looks at it

## Fact

SYMPTOM, on the preload build only:

    The controller for URI "/" is not callable: Expected method "home" on class "stdClass",
    did you mean "home"?

Symfony builds that sentence from three probes of ONE value, and they disagreed:
`get_debug_type($controller)` said `stdClass`, `method_exists($controller, 'home')` said false, and
`get_class_methods($controller)` listed `home`. The first two are COMPILED builtins reading the
object header; the third has no compiled lowering here and answers from the interpreter, which
tracks the object's eval-side identity (`context.dynamic_object_class_name`). Same object, two
truths.

CAUSE. `App\Controller\HelloController` was NOT in the closed world. The generated container names
it only as an ARRAY KEY -- `'App\\Controller\\HelloController' => 'getHelloControllerService'` in
`$this->fileMap` -- and that shape is not a dynamic-contract candidate, so the autoload pass never
spliced `src/Controller/HelloController.php`. At runtime `Container::load()` `require`s the factory
by a computed path, the interpreter declares it, and its `new \App\Controller\HelloController()`
has no compiled class to construct: it makes a DYNAMIC object with a stdClass header and an
eval-side identity.

That is invisible while everything is interpreted -- the non-preload `public/index` renders the page
fine -- and fatal the moment a COMPILED frame inspects the value, which is exactly what preloading
`ControllerResolver` arranges.

HOW TO TELL, next time, in one step: `grep -c 'TheClassName' <binary>.s`. Two hits means two string
literals and no compiled class; a compiled class brings labels, method bodies and a class-name row.

WORKAROUND used: add the class, its base and its container factory to the preload list
(`src/Controller/HelloController.php`, `framework-bundle/Controller/AbstractController.php`,
`service-contracts/ServiceSubscriberInterface.php`,
`var/cache/prod/ContainerXvDiqOo/getHelloControllerService.php`). After that all six real routes
answer byte-identically to `php -S`.

THE REAL FIX IS ONE OF TWO, neither done: make a class-string ARRAY KEY that maps to a factory name
a dynamic-contract candidate, or make the compiled builtins ask the eval side when the object has a
dynamic identity, the way `get_class_methods` already does.

## Why

Every route 500'd with 'Expected method "home" on class "stdClass", did you mean "home"?' and the message is self-contradictory enough to send you hunting the wrong bug
