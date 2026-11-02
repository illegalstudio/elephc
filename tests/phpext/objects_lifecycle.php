<?php

// Each request, including overlapping ZTS threads, starts with an empty native owner.
(function () {
    if (phpext_object_kept(null, false) !== null) { throw new Exception('object owner crossed request'); }
    $object = new LifecycleObject();
    $object->value = 7;
    $object->items = ['binary' => "a\0b"];
    if (phpext_object_kept($object, true) !== $object || $object->add(1) !== 8) {
        throw new Exception('object request identity failed');
    }
    $weak = WeakReference::create($object);
    unset($object);
    gc_collect_cycles();
    if ($weak->get() === null || phpext_object_kept(null, false) !== $weak->get()) {
        throw new Exception('native object owner did not keep wrapper');
    }
    $object = $weak->get();
    $fiber = new Fiber(function () use ($object) {
        set_error_handler(function () use ($object) {
            if ($object->value !== 9) { throw new Exception('object not published before suspension'); }
            Fiber::suspend('object parked');
            $object->value = 20;
            return true;
        });
        try { $object->pause('object resumed'); }
        catch (RuntimeException $error) {
            if ($error->getMessage() !== 'object resumed') { throw $error; }
        } finally { restore_error_handler(); }
    });
    if ($fiber->start() !== 'object parked') { throw new Exception('object Fiber did not park'); }
    $blocked = false;
    try { $object->add(1); }
    catch (Error $error) {
        if (!str_contains($error->getMessage(), 'another Fiber')) { throw $error; }
        $blocked = true;
    }
    if (!$blocked) { throw new Exception('overlapping object Fiber entered native image'); }
    $fiber->resume();
    if ($object->value !== 20 || $object->add(1) !== 21) {
        throw new Exception('object state lost at Fiber handoff');
    }
    $peer = new LifecycleObject();
    $object->next = $peer;
    $peer->next = $object;
    $peer->add(1);
    // Leave both a native static owner and a cycle for complete request shutdown.
})();
