<?php

// A parked native activation retains stack-bound handlers until its Fiber resumes.
(function () {
    $task = function (string $message): Fiber {
        return new Fiber(function () use ($message) {
            set_error_handler(function ($severity, $warning) {
                if ($severity !== E_WARNING || !str_contains($warning, 'missing')) {
                    throw new Exception('unexpected native warning');
                }
                if (phpext_mirror([42]) !== [42]) { throw new Exception('same Fiber reentry failed'); }
                Fiber::suspend('parked');
                if (phpext_mirror([43]) !== [43]) { throw new Exception('resumed Fiber reentry failed'); }
                return true;
            });
            try {
                phpext_pause($message);
                throw new Exception('missing native exception');
            } catch (RuntimeException $e) {
                if ($e->getMessage() !== $message || $e->getCode() !== 42) { throw $e; }
                return $message;
            } catch (Error $e) {
                if (!str_contains($e->getMessage(), 'another Fiber')) { throw $e; }
                return 'blocked';
            } finally {
                restore_error_handler();
            }
        });
    };

    for ($i = 0, $limit = min(8, (int)(getenv('ELEPHC_EMBED_ITERATIONS') ?: 8)); $i < $limit; $i++) {
        $first = $task("first:$i");
        if ($first->start() !== 'parked') { throw new Exception('first Fiber did not suspend'); }
        $second = $task("second:$i");
        $secondStart = $second->start();
        // Unwind a broken implementation in stack order so the regression fails without corrupting stacks.
        if ($secondStart === 'parked') { $second->resume(); }
        $blocked = $second->isTerminated() && $second->getReturn() === 'blocked';
        $mainBlocked = false;
        try { phpext_mirror([]); }
        catch (Error $e) {
            if (!str_contains($e->getMessage(), 'another Fiber')) { throw $e; }
            $mainBlocked = true;
        }
        $first->resume();
        if ($first->getReturn() !== "first:$i") { throw new Exception('wrong Fiber exception'); }
        if (!$blocked || !$mainBlocked) { throw new Exception('overlapping Fiber entered native code'); }

        // Finishing the owner releases the guard for a different Fiber and the main context.
        $later = $task("later:$i");
        if ($later->start() !== 'parked') { throw new Exception('Fiber ownership was not released'); }
        $later->resume();
        if ($later->getReturn() !== "later:$i" || phpext_mirror([44]) !== [44]) {
            throw new Exception('native state failed after Fiber handoff');
        }
    }

    // Throwing into a parked host callback must also retire its native activation and owner.
    $cancelled = $task('cancelled');
    if ($cancelled->start() !== 'parked') { throw new Exception('cancelled Fiber did not suspend'); }
    $hostException = false;
    try { $cancelled->throw(new LogicException('host cancellation')); }
    catch (LogicException $e) {
        if ($e->getMessage() !== 'host cancellation') { throw $e; }
        $hostException = true;
    }
    if (!$hostException || !$cancelled->isTerminated() || phpext_mirror([45]) !== [45]) {
        throw new Exception('native state failed after Fiber cancellation');
    }

    $abandoned = $task('abandoned');
    if ($abandoned->start() !== 'parked') { throw new Exception('abandoned Fiber did not suspend'); }
    unset($abandoned);
    if (phpext_mirror([46]) !== [46]) { throw new Exception('Fiber destruction retained ownership'); }

    // The main host context can also own a parked native stack while a callback starts a Fiber.
    set_error_handler(function () {
        $nested = new Fiber(function () { return phpext_mirror([]); });
        $blocked = false;
        try { $nested->start(); }
        catch (Error $e) {
            if (!str_contains($e->getMessage(), 'another Fiber')) { throw $e; }
            $blocked = true;
        }
        if (!$blocked || phpext_mirror([47]) !== [47]) { throw new Exception('main context ownership failed'); }
        return true;
    });
    try { phpext_pause('main'); }
    catch (RuntimeException $e) {
        if ($e->getMessage() !== 'main' || $e->getCode() !== 42) { throw $e; }
    } finally { restore_error_handler(); }
    if (phpext_mirror([48]) !== [48]) { throw new Exception('main context did not release ownership'); }
})();
