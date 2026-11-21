<?php

namespace Elephc\Async {

final class CancelledException extends \RuntimeException
{
    private ?\Throwable $cancellationReason = null;
    private ?__CancellationState $requestState = null;

    public function __construct(
        string $message = "Elephc Async task cancelled",
        ?\Throwable $reason = null,
    ) {
        $this->message = $message;
        $this->code = 0;
        $this->cancellationReason = $reason;
    }

    public function reason(): ?\Throwable
    {
        return $this->cancellationReason;
    }

    public function __markRequestPaired(__CancellationState $state, \stdClass $token): void
    {
        if (!$state->__isRequestToken($token)) {
            return;
        }
        $this->requestState = $state;
    }

    public static function __isRequestPaired(mixed $error, __CancellationState $state): bool
    {
        if (!($error instanceof CancelledException)) {
            return false;
        }
        return $error->requestState === $state;
    }

}

final class __CancellationState
{
    private bool $requested = false;
    private bool $hasReason = false;
    private \Throwable $reason;
    private \stdClass $requestToken;

    public function __construct()
    {
        $this->requestToken = new \stdClass();
    }

    public function __isRequestToken(\stdClass $token): bool
    {
        return $this->requestToken === $token;
    }

    public function request(?\Throwable $reason = null): void
    {
        if ($this->requested) {
            return;
        }
        $this->requested = true;
        if ($reason !== null) {
            $this->hasReason = true;
            $this->storeReason($reason);
        }
    }

    private function storeReason(\Throwable $reason): void
    {
        $this->reason = $reason;
    }

    public function __clearReason(): void
    {
        if ($this->hasReason) {
            unset($this->reason);
            $this->hasReason = false;
        }
    }

    public function isRequested(): bool
    {
        return $this->requested;
    }

    public function throwIfRequested(): void
    {
        if ($this->requested) {
            throw $this->exception();
        }
    }

    public function exception(): CancelledException
    {
        if ($this->hasReason) {
            $exception = new CancelledException("Elephc Async task cancelled", $this->reason);
        } else {
            $exception = new CancelledException();
        }
        $exception->__markRequestPaired($this, $this->requestToken);
        return $exception;
    }

}

final class Cancellation
{
    private __CancellationState $state;

    public function __construct(__CancellationState $state)
    {
        $this->state = $state;
    }

    public function isRequested(): bool
    {
        return $this->state->isRequested();
    }

    public function throwIfRequested(): void
    {
        if ($this->state->isRequested()) {
            throw $this->state->exception();
        }
    }

}

final class __TaskOutcome
{
    private bool $failed = false;
    private bool $observed = false;
    private \Throwable $failure;

    public function recordFailure(\Throwable $failure): void
    {
        $this->failed = true;
        $this->observed = false;
        $this->failure = $failure;
    }

    public function observeAndThrowIfFailed(): void
    {
        if ($this->failed) {
            $this->observed = true;
            throw $this->failure;
        }
    }

    public function throwIfFailed(): void
    {
        if ($this->failed) {
            throw $this->failure;
        }
    }

    public function hasUnobservedFailure(): bool
    {
        return $this->failed && !$this->observed;
    }

}

final class __RunResult
{
    private mixed $value = null;

    public function set(mixed $value): void
    {
        $this->value = $value;
    }

    public function get(): mixed
    {
        return $this->value;
    }
}

final class __TaskStart
{
    private \Fiber $fiber;
    private array $fiberArgs = [];

    private function __construct(\Fiber $fiber)
    {
        $this->fiber = $fiber;
    }

    public static function fromArguments(\Fiber $fiber, array $args): __TaskStart
    {
        $start = new self($fiber);
        $start->fiberArgs = $args;
        return $start;
    }

    public static function fromNoArgs(\Fiber $fiber): __TaskStart
    {
        $start = new self($fiber);
        return $start;
    }

    public function start(): mixed
    {
        $args = $this->fiberArgs;
        $this->fiberArgs = [];
        $argCount = count($args);
        if ($argCount === 0) {
            $signal = $this->fiber->start();
        } elseif ($argCount === 1) {
            $arg0 = $args[0];
            $signal = $this->fiber->start($arg0);
            unset($arg0);
        } elseif ($argCount === 2) {
            $arg0 = $args[0];
            $arg1 = $args[1];
            $signal = $this->fiber->start($arg0, $arg1);
            unset($arg0);
            unset($arg1);
        } elseif ($argCount === 3) {
            $arg0 = $args[0];
            $arg1 = $args[1];
            $arg2 = $args[2];
            $signal = $this->fiber->start($arg0, $arg1, $arg2);
            unset($arg0);
            unset($arg1);
            unset($arg2);
        } elseif ($argCount === 4) {
            $arg0 = $args[0];
            $arg1 = $args[1];
            $arg2 = $args[2];
            $arg3 = $args[3];
            $signal = $this->fiber->start($arg0, $arg1, $arg2, $arg3);
            unset($arg0);
            unset($arg1);
            unset($arg2);
            unset($arg3);
        } elseif ($argCount === 5) {
            $arg0 = $args[0];
            $arg1 = $args[1];
            $arg2 = $args[2];
            $arg3 = $args[3];
            $arg4 = $args[4];
            $signal = $this->fiber->start($arg0, $arg1, $arg2, $arg3, $arg4);
            unset($arg0);
            unset($arg1);
            unset($arg2);
            unset($arg3);
            unset($arg4);
        } elseif ($argCount === 6) {
            $arg0 = $args[0];
            $arg1 = $args[1];
            $arg2 = $args[2];
            $arg3 = $args[3];
            $arg4 = $args[4];
            $arg5 = $args[5];
            $signal = $this->fiber->start($arg0, $arg1, $arg2, $arg3, $arg4, $arg5);
            unset($arg0);
            unset($arg1);
            unset($arg2);
            unset($arg3);
            unset($arg4);
            unset($arg5);
        } elseif ($argCount === 7) {
            $arg0 = $args[0];
            $arg1 = $args[1];
            $arg2 = $args[2];
            $arg3 = $args[3];
            $arg4 = $args[4];
            $arg5 = $args[5];
            $arg6 = $args[6];
            $signal = $this->fiber->start($arg0, $arg1, $arg2, $arg3, $arg4, $arg5, $arg6);
            unset($arg0);
            unset($arg1);
            unset($arg2);
            unset($arg3);
            unset($arg4);
            unset($arg5);
            unset($arg6);
        } else {
            throw new \Error("Elephc Async internal Fiber launch received an unsupported argument count");
        }
        unset($args);
        return $signal;
    }

}

final class __Scheduler
{
    private static bool $asyncTaskActive = false;
    private __CancellationState $cancellation;
    private array $fibers = [];
    private array $taskStarts = [];
    private array $ready = [];
    private array $queued = [];
    private array $waitingOn = [];
    private array $done = [];
    private array $cancelled = [];
    private array $cancelling = [];
    private array $outcomes = [];
    private array $sleepDeadlines = [];
    private array $ioSources = [];
    private array $ioPollFds = [];
    private array $ioInterests = [];
    private array $ioTaskIds = [];
    private array $ioDeadlines = [];
    private array $ioActive = [];
    private array $ioReady = [];
    private array $taskIoRegistration = [];
    private array $taskGenerations = [];
    private array $taskHandles = [];
    private array $parentTaskIds = [];
    private array $groupTaskIds = [];
    private bool $closed = false;
    private int $nextTaskGeneration = 1;
    private int $head = 0;
    private int $live = 0;
    private int $currentId = -1;
    private int $rootId = -1;
    private array $failureOrder = [];

    public function __construct(__CancellationState $cancellation)
    {
        $this->cancellation = $cancellation;
    }

    public static function __isAsyncTaskActive(): bool
    {
        return self::$asyncTaskActive;
    }

    public static function __assertFiberSuspendAllowed(): void
    {
        if (__Scheduler::__isAsyncTaskActive()) {
            throw new \Error("Fiber::suspend() is not a scheduler wakeup inside an Elephc Async task");
        }
    }

    private static function __assertFiberSuspendCallableAllowed(): void
    {
        __Scheduler::__assertFiberSuspendAllowed();
    }

    private static function __isFiberSuspendCallableArray(array $callback): bool
    {
        return
            is_array($callback)
            && count($callback) === 2
            && is_string($callback[1])
            && (
                (
                    is_string($callback[0])
                    && (
                        strcasecmp($callback[0], "Fiber") === 0
                        || strcasecmp($callback[0], "\\Fiber") === 0
                    )
                )
                || $callback[0] instanceof \Fiber
            )
            && strcasecmp($callback[1], "suspend") === 0;
    }

    private static function __assertFiberSuspendCallableGuard(mixed $callback): void
    {
        unset($callback);
        throw new \Error("Elephc Async callable guard was not lowered");
    }

    public function spawn(callable $body, mixed $args): Awaitable
    {
        if ($this->cancellation->isRequested()) {
            unset($body);
            unset($args);
            throw new \Error("Elephc Async TaskGroup cannot spawn after cancellation has been requested");
        }
        $runner = static function () use ($body, $args): mixed {
            return call_user_func_array($body, $args);
        };
        $taskId = $this->createTaskNoArgs($runner);
        unset($runner);
        $awaitable = new Awaitable($this, $taskId);
        unset($body);
        unset($args);
        return $awaitable;
    }

    private function runRoot(callable $body, TaskGroup $group): mixed
    {
        $this->closed = false;
        $rootTaskId = $this->createTask($body, [$group]);
        $rootId = $this->resolveTaskId($rootTaskId);
        $this->rootId = $rootId;
        try {
            $this->drive();
            $group->__detach();
            $this->throwFirstUnobservedFailure();
            $root = $this->fibers[$rootId];
            $result = $root->getReturn();
            $this->fibers[$rootId] = null;
            unset($root);
            $this->teardown();
        } catch (\Throwable $error) {
            $this->teardown();
            unset($body);
            unset($group);
            throw $error;
        }
        unset($body);
        unset($group);
        return $result;
    }

    public function awaitTask(int $taskId): mixed
    {
        $targetId = $this->resolveTaskId($taskId);
        if ($this->done[$targetId]) {
            if ($this->cancelled[$targetId]) {
                throw $this->cancellation->exception();
            }
            $outcome = $this->outcomes[$targetId];
            $outcome->observeAndThrowIfFailed();
            unset($outcome);
            $target = $this->fibers[$targetId];
            return $target->getReturn();
        }
        if ($this->currentId < 0) {
            throw new \Error("Awaitable::await() must be called from an Elephc Async task");
        }
        if ($this->currentId === $targetId) {
            throw new \Error("An Elephc Async task cannot await itself");
        }

        $currentId = $this->currentId;
        $this->waitingOn[$currentId] = $targetId;
        $this->monitorTask($currentId, 4, 4);
        \Fiber::suspend(1);
        if ($currentId !== $this->rootId) {
            $this->throwIfCancellationRequested();
        }

        if ($this->cancelled[$targetId]) {
            throw $this->cancellation->exception();
        }
        $outcome = $this->outcomes[$targetId];
        $outcome->observeAndThrowIfFailed();
        unset($outcome);
        $target = $this->fibers[$targetId];
        return $target->getReturn();
    }

    public function isComplete(int $taskId): bool
    {
        $slot = $this->resolveTaskId($taskId);
        return $this->done[$slot];
    }

    public function rescheduleCurrent(): void
    {
        if ($this->currentId < 0) {
            throw new \Error("TaskGroup::reschedule() must be called from an Elephc Async task");
        }
        if ($this->currentId !== $this->rootId) {
            $this->throwIfCancellationRequested();
        }
        \Fiber::suspend(0);
        if ($this->currentId !== $this->rootId) {
            $this->throwIfCancellationRequested();
        }
    }

    public function sleepCurrent(float $seconds): void
    {
        if ($this->currentId < 0) {
            throw new \Error("TaskGroup::sleep() must be called from an Elephc Async task");
        }
        if (!is_finite($seconds)) {
            throw new \ValueError("TaskGroup::sleep(): Argument #1 (\$seconds) must be finite");
        }
        if ($seconds < 0.0) {
            throw new \ValueError("TaskGroup::sleep(): Argument #1 (\$seconds) must be greater than or equal to 0");
        }
        if ($this->currentId !== $this->rootId) {
            $this->throwIfCancellationRequested();
        }
        if ($seconds === 0.0) {
            \Fiber::suspend(0);
            if ($this->currentId !== $this->rootId) {
                $this->throwIfCancellationRequested();
            }
            return;
        }

        $deadlineBase = hrtime(true);
        $maximumSeconds = (PHP_INT_MAX - $deadlineBase) / 1000000000.0;
        if ($seconds > $maximumSeconds) {
            throw new \ValueError("TaskGroup::sleep(): seconds exceed the monotonic deadline range");
        }
        $nanoseconds = (int) ($seconds * 1000000000.0);
        $this->sleepDeadlines[$this->currentId] = $deadlineBase + $nanoseconds;
        $this->monitorTask($this->currentId, 5, 0);
        \Fiber::suspend(2);
        if ($this->currentId !== $this->rootId) {
            $this->throwIfCancellationRequested();
        }
    }

    public function awaitReadable(mixed $stream, ?float $timeout = null): bool
    {
        return $this->awaitIo($stream, 1, $timeout, "TaskGroup::awaitReadable()");
    }

    public function awaitWritable(mixed $stream, ?float $timeout = null): bool
    {
        return $this->awaitIo($stream, 4, $timeout, "TaskGroup::awaitWritable()");
    }

    private function awaitIo(mixed $stream, int $interest, ?float $timeout, string $operation): bool
    {
        if ($this->currentId < 0) {
            throw new \Error($operation . " must be called from an Elephc Async task");
        }
        if (!is_resource($stream) && !is_int($stream)) {
            throw new \TypeError($operation . " expects a native stream resource or descriptor");
        }
        if (is_int($stream) && ($stream < 0 || $stream > 2147483647)) {
            throw new \ValueError($operation . " expects a descriptor within the non-negative C int range");
        }
        if ($timeout !== null && !is_finite($timeout)) {
            throw new \ValueError($operation . ": Argument #2 (\$timeout) must be finite");
        }
        if ($timeout !== null && $timeout < 0.0) {
            throw new \ValueError($operation . ": Argument #2 (\$timeout) must be greater than or equal to 0");
        }
        if ($timeout === 0.0) {
            return $this->probeIo($stream, $interest);
        }

        $deadline = -1;
        if ($timeout !== null) {
            $deadlineBase = hrtime(true);
            $maximumTimeout = (PHP_INT_MAX - $deadlineBase) / 1000000000.0;
            if ($timeout > $maximumTimeout) {
                throw new \ValueError($operation . ": timeout exceeds the monotonic deadline range");
            }
            $deadline = $deadlineBase + (int) ($timeout * 1000000000.0);
        }

        $pollFd = $this->duplicateIoSource($stream);
        if ($pollFd < 0) {
            throw new \Error($operation . " could not duplicate the native descriptor");
        }
        $registrationId = count($this->ioSources);
        $this->ioSources[$registrationId] = $stream;
        $this->ioPollFds[$registrationId] = $pollFd;
        $this->ioInterests[$registrationId] = $interest;
        $this->ioTaskIds[$registrationId] = $this->currentId;
        $this->ioDeadlines[$registrationId] = $deadline;
        $this->ioActive[$registrationId] = true;
        $this->ioReady[$registrationId] = false;
        $this->taskIoRegistration[$this->currentId] = $registrationId;
        $taskId = $this->currentId;
        $this->monitorTask($taskId, 6, 0);
        \Fiber::suspend(3);
        if ($taskId !== $this->rootId) {
            $this->throwIfCancellationRequested();
        }
        return $this->ioReady[$registrationId];
    }

    private function probeIo(mixed $source, int $interest): bool
    {
        $pollFd = $this->duplicateIoSource($source);
        if ($pollFd < 0) {
            throw new \Error("Elephc Async reactor could not duplicate the native descriptor");
        }
        $entries = [$pollFd, $interest];
        $result = -2;
        while ($result === -2) {
            $result = \__elephc_async_poll($entries, 0);
        }
        $this->releasePollFd($pollFd);
        if ($result === 0) {
            return true;
        }
        if ($result === -1) {
            return false;
        }
        throw new \Error("Elephc Async reactor poll failed");
    }

    private function duplicateIoSource(mixed $source): int
    {
        return \__elephc_async_fd([$source], 0);
    }

    private function throwIfCancellationRequested(): void
    {
        // Stabilize the property read in a cleanup-tracked local. If the cancellation check
        // throws, Fiber unwinding releases this frame slot before reaching the task catch.
        $cancellation = $this->cancellation;
        $cancellation->throwIfRequested();
        unset($cancellation);
    }

    private function releasePollFd(int $pollFd): void
    {
        \__elephc_async_fd([$pollFd], 1);
    }

    private function createTask(callable $body, array $args): int
    {
        $fiber = new \Fiber($body);
        $taskStart = __TaskStart::fromArguments($fiber, $args);
        $taskId = $this->registerTask($fiber, $taskStart);
        unset($fiber);
        unset($taskStart);
        unset($args);
        unset($body);
        return $taskId;
    }

    private function createTaskNoArgs(callable $body): int
    {
        $fiber = new \Fiber($body);
        $taskStart = __TaskStart::fromNoArgs($fiber);
        $taskId = $this->registerTask($fiber, $taskStart);
        unset($fiber);
        unset($taskStart);
        unset($body);
        return $taskId;
    }

    private function registerTask(\Fiber $fiber, __TaskStart $taskStart): int
    {
        $slot = count($this->fibers);
        $generation = $this->nextTaskGeneration;
        if ($generation >= 4294967296) {
            throw new \Error("Elephc Async task generation space exhausted");
        }
        $this->nextTaskGeneration = $generation + 1;
        $this->taskGenerations[$slot] = $generation;
        $taskId = $slot * 4294967296 + $generation;
        $parentTaskId = -1;
        if ($this->currentId >= 0) {
            $parentTaskId = $this->taskHandles[$this->currentId];
        }
        $groupTaskId = $taskId;
        if ($this->rootId >= 0) {
            $groupTaskId = $this->taskHandles[$this->rootId];
        }
        $this->taskHandles[$slot] = $taskId;
        $this->parentTaskIds[$slot] = $parentTaskId;
        $this->groupTaskIds[$slot] = $groupTaskId;
        $this->fibers[$slot] = $fiber;
        $this->taskStarts[$slot] = $taskStart;
        $this->queued[$slot] = false;
        $this->waitingOn[$slot] = -1;
        $this->done[$slot] = false;
        $this->cancelled[$slot] = false;
        $this->cancelling[$slot] = false;
        $this->outcomes[$slot] = new __TaskOutcome();
        $this->sleepDeadlines[$slot] = -1;
        $this->taskIoRegistration[$slot] = -1;
        $this->live = $this->live + 1;
        $this->monitorTask($slot, 1, 0);
        $this->enqueue($slot, 1);
        $resultTaskId = $taskId;
        unset($fiber);
        unset($taskStart);
        return $resultTaskId;
    }

    private function resolveTaskId(int $taskId): int
    {
        if ($this->closed) {
            throw new \Error("Elephc Async Awaitable scope has ended");
        }
        if ($taskId < 1) {
            throw new \Error("Stale Elephc Async task handle");
        }
        $slot = intdiv($taskId, 4294967296);
        $generation = $taskId % 4294967296;
        if ($slot < 0 || $slot >= count($this->taskGenerations)) {
            throw new \Error("Stale Elephc Async task handle");
        }
        if ($this->taskGenerations[$slot] !== $generation) {
            throw new \Error("Stale Elephc Async task handle");
        }
        return $slot;
    }

    private function monitorTask(int $slot, int $state, int $reason): void
    {
        \__elephc_async_monitor_event(
            1,
            $this->taskHandles[$slot],
            $this->parentTaskIds[$slot],
            $this->groupTaskIds[$slot],
            $state * 256 + $reason,
        );
    }

    private function enqueue(int $taskId, int $wakeReason): void
    {
        if ($this->done[$taskId] || $this->queued[$taskId]) {
            return;
        }
        $this->queued[$taskId] = true;
        $this->ready[] = $taskId;
        $this->monitorTask($taskId, 2, $wakeReason);
    }

    private function dequeue(): int
    {
        if ($this->head >= count($this->ready)) {
            $this->ready = [];
            $this->head = 0;
            return -1;
        }

        $taskId = $this->ready[$this->head];
        $this->head = $this->head + 1;
        $this->queued[$taskId] = false;
        return $taskId;
    }

    private function wakeWaiters(int $completedId): void
    {
        $taskId = 0;
        $count = count($this->fibers);
        while ($taskId < $count) {
            if ($this->waitingOn[$taskId] === $completedId) {
                $this->waitingOn[$taskId] = -1;
                $this->enqueue($taskId, 5);
            }
            $taskId = $taskId + 1;
        }
    }

    private function wakeExpiredTimers(): void
    {
        $now = hrtime(true);
        $taskId = 0;
        $count = count($this->fibers);
        while ($taskId < $count) {
            $deadline = $this->sleepDeadlines[$taskId];
            if ($deadline >= 0 && $deadline <= $now) {
                $this->sleepDeadlines[$taskId] = -1;
                $this->enqueue($taskId, 6);
            }
            $taskId = $taskId + 1;
        }
    }

    private function deregisterIoForTask(int $taskId): void
    {
        $registrationId = $this->taskIoRegistration[$taskId];
        if ($registrationId >= 0) {
            $this->releaseIoRegistration($registrationId);
            $this->taskIoRegistration[$taskId] = -1;
        }
    }

    private function releaseIoRegistration(int $registrationId): void
    {
        if (!$this->ioActive[$registrationId]) {
            return;
        }
        $this->ioActive[$registrationId] = false;
        $this->ioSources[$registrationId] = null;
        $pollFd = $this->ioPollFds[$registrationId];
        if ($pollFd >= 0) {
            $this->releasePollFd($pollFd);
            $this->ioPollFds[$registrationId] = -1;
        }
    }

    private function wakeExpiredIo(): void
    {
        $now = hrtime(true);
        $registrationId = 0;
        $count = count($this->ioSources);
        while ($registrationId < $count) {
            $deadline = $this->ioDeadlines[$registrationId];
            if ($this->ioActive[$registrationId] && $deadline >= 0 && $deadline <= $now) {
                $taskId = $this->ioTaskIds[$registrationId];
                $this->releaseIoRegistration($registrationId);
                $this->ioReady[$registrationId] = false;
                $this->taskIoRegistration[$taskId] = -1;
                $this->enqueue($taskId, 8);
            }
            $registrationId = $registrationId + 1;
        }
    }

    private function hasIoRegistrations(): bool
    {
        $registrationId = 0;
        $count = count($this->ioSources);
        while ($registrationId < $count) {
            if ($this->ioActive[$registrationId]) {
                return true;
            }
            $registrationId = $registrationId + 1;
        }
        return false;
    }

    private function pollIo(int $timeoutMilliseconds): void
    {
        $wait = $timeoutMilliseconds;
        while (true) {
            $entries = [];
            $registrationIds = [];
            $registrationId = 0;
            $count = count($this->ioSources);
            while ($registrationId < $count) {
                if ($this->ioActive[$registrationId]) {
                    $entries[] = $this->ioPollFds[$registrationId];
                    $entries[] = $this->ioInterests[$registrationId];
                    $registrationIds[] = $registrationId;
                }
                $registrationId = $registrationId + 1;
            }
            if (count($registrationIds) === 0) {
                return;
            }

            $readyIndex = \__elephc_async_poll($entries, $wait);
            if ($readyIndex === -1 || $readyIndex === -2) {
                return;
            }
            if ($readyIndex < 0) {
                throw new \Error("Elephc Async reactor poll failed");
            }

            $readyRegistrationId = $registrationIds[$readyIndex];
            $taskId = $this->ioTaskIds[$readyRegistrationId];
            $this->releaseIoRegistration($readyRegistrationId);
            $this->ioReady[$readyRegistrationId] = true;
            $this->taskIoRegistration[$taskId] = -1;
            $this->enqueue($taskId, 7);
            $wait = 0;
        }
    }

    private function wakeCancelledTasks(): void
    {
        if (!$this->cancellation->isRequested()) {
            return;
        }

        $taskId = 0;
        $count = count($this->fibers);
        while ($taskId < $count) {
            if ($taskId !== $this->rootId && !$this->done[$taskId]) {
                if (!$this->cancelling[$taskId]) {
                    $this->cancelling[$taskId] = true;
                    $this->monitorTask($taskId, 7, 9);
                }
                $this->waitingOn[$taskId] = -1;
                $this->sleepDeadlines[$taskId] = -1;
                $this->deregisterIoForTask($taskId);
                $this->enqueue($taskId, 9);
            }
            $taskId = $taskId + 1;
        }
    }

    private function nextTimerDeadline(): int
    {
        $next = -1;
        $taskId = 0;
        $count = count($this->fibers);
        while ($taskId < $count) {
            $deadline = $this->sleepDeadlines[$taskId];
            if ($deadline >= 0 && ($next < 0 || $deadline < $next)) {
                $next = $deadline;
            }
            $taskId = $taskId + 1;
        }
        $registrationId = 0;
        $registrationCount = count($this->ioSources);
        while ($registrationId < $registrationCount) {
            $deadline = $this->ioDeadlines[$registrationId];
            if ($this->ioActive[$registrationId] && $deadline >= 0 && ($next < 0 || $deadline < $next)) {
                $next = $deadline;
            }
            $registrationId = $registrationId + 1;
        }
        return $next;
    }

    private function throwFirstUnobservedFailure(): void
    {
        $index = 0;
        $count = count($this->failureOrder);
        while ($index < $count) {
            $taskId = $this->failureOrder[$index];
            $outcome = $this->outcomes[$taskId];
            if ($outcome->hasUnobservedFailure()) {
                $this->teardown();
                $outcome->throwIfFailed();
            }
            $index = $index + 1;
        }
    }

    private function teardown(): void
    {
        $this->closed = true;
        $registrationId = 0;
        $registrationCount = count($this->ioSources);
        while ($registrationId < $registrationCount) {
            $this->releaseIoRegistration($registrationId);
            $registrationId = $registrationId + 1;
        }
        $taskId = 0;
        $taskCount = count($this->fibers);
        while ($taskId < $taskCount) {
            $this->taskStarts[$taskId] = null;
            $this->outcomes[$taskId] = null;
            $this->fibers[$taskId] = null;
            $taskId = $taskId + 1;
        }
        // Teardown closes the current root scope. Reset every record table, rather than leaving
        // scalar queues and completed-task metadata to object destruction: this breaks any
        // remaining scheduler-owned graph before a captured TaskGroup can keep it alive. An
        // internal later runRoot() may reuse this scheduler object; its generation counter remains
        // monotonic so old Awaitables still fail as stale instead of binding to the new task slot.
        $this->fibers = [];
        $this->taskStarts = [];
        $this->ready = [];
        $this->queued = [];
        $this->waitingOn = [];
        $this->done = [];
        $this->cancelled = [];
        $this->cancelling = [];
        $this->outcomes = [];
        $this->sleepDeadlines = [];
        $this->ioSources = [];
        $this->ioPollFds = [];
        $this->ioInterests = [];
        $this->ioTaskIds = [];
        $this->ioDeadlines = [];
        $this->ioActive = [];
        $this->ioReady = [];
        $this->taskIoRegistration = [];
        $this->taskGenerations = [];
        $this->taskHandles = [];
        $this->parentTaskIds = [];
        $this->groupTaskIds = [];
        $this->failureOrder = [];
        $this->head = 0;
        $this->live = 0;
        $this->currentId = -1;
        $this->rootId = -1;
    }

    private function drive(): void
    {
        while ($this->live > 0) {
            $this->wakeCancelledTasks();
            $this->wakeExpiredTimers();
            $this->wakeExpiredIo();
            if ($this->hasIoRegistrations()) {
                $this->pollIo(0);
            }
            $taskId = $this->dequeue();
            if ($taskId < 0) {
                $deadline = $this->nextTimerDeadline();
                $hasIo = $this->hasIoRegistrations();
                if ($hasIo) {
                    $timeoutMilliseconds = -1;
                    if ($deadline >= 0) {
                        $remaining = $deadline - hrtime(true);
                        if ($remaining <= 0) {
                            $timeoutMilliseconds = 0;
                        } else {
                            $timeoutMilliseconds = (int) (($remaining + 999999) / 1000000);
                        }
                    }
                    $this->pollIo($timeoutMilliseconds);
                } elseif ($deadline < 0) {
                    throw new \Error("Elephc Async deadlock: tasks are live but none are runnable");
                } else {
                    $remaining = $deadline - hrtime(true);
                    if ($remaining > 0) {
                        $microseconds = (int) ($remaining / 1000);
                        if ($microseconds < 1) {
                            $microseconds = 1;
                        }
                        usleep($microseconds);
                    }
                }
                $this->wakeExpiredTimers();
                $this->wakeExpiredIo();
                $taskId = $this->dequeue();
                if ($taskId < 0) {
                    continue;
                }
            }

            $this->currentId = $taskId;
            $this->monitorTask($taskId, 3, 2);
            $fiber = $this->fibers[$taskId];
            $signal = null;
            $cancelled = false;
            $failed = false;
            self::$asyncTaskActive = true;
            try {
                if ($this->cancellation->isRequested() && $taskId !== $this->rootId) {
                    if (!$fiber->isStarted()) {
                        $cancelled = true;
                    } else {
                        $signal = $fiber->resume();
                    }
                } elseif (!$fiber->isStarted()) {
                    $start = $this->taskStarts[$taskId];
                    $this->taskStarts[$taskId] = null;
                    $signal = $start->start();
                    unset($start);
                } else {
                    $signal = $fiber->resume();
                }
            } catch (\Throwable $error) {
                if ($error instanceof CancelledException
                    && $taskId !== $this->rootId
                    && CancelledException::__isRequestPaired($error, $this->cancellation)
                ) {
                    $cancelled = true;
                } else {
                    $failed = true;
                    $this->outcomes[$taskId]->recordFailure($error);
                    $this->failureOrder[] = $taskId;
                    $this->cancellation->request($error);
                }
            }
            self::$asyncTaskActive = false;
            // The scheduler either retained the failure in its task outcome or consumed it as
            // cooperative cancellation. Do not keep the catch binding alive in the scheduler
            // activation: its trace may still reference task captures and the scope graph.
            unset($error);

            if ($cancelled || $fiber->isTerminated()) {
                $this->done[$taskId] = true;
                $this->cancelled[$taskId] = $cancelled;
                $this->sleepDeadlines[$taskId] = -1;
                $this->deregisterIoForTask($taskId);
                $this->live = $this->live - 1;
                if ($cancelled) {
                    $this->monitorTask($taskId, 10, 9);
                } elseif ($failed) {
                    $this->monitorTask($taskId, 9, 11);
                } else {
                    $this->monitorTask($taskId, 8, 10);
                }
                $this->wakeWaiters($taskId);
            } elseif ($signal === 0) {
                $this->enqueue($taskId, 3);
            }
            unset($fiber);
            unset($signal);
            $this->currentId = -1;
        }
    }
}

final class __ScopeState
{
    private ?__Scheduler $scheduler;

    public function __construct(__Scheduler $scheduler)
    {
        $this->scheduler = $scheduler;
    }

    public function scheduler(): __Scheduler
    {
        $scheduler = $this->scheduler;
        if ($scheduler === null) {
            throw new \Error("Elephc Async TaskGroup scope has ended");
        }
        return $scheduler;
    }

    public function detach(): bool
    {
        $this->scheduler = null;
        return true;
    }

}

final class TaskGroup
{
    private ?__ScopeState $scope;
    private __CancellationState $state;
    private Cancellation $cancellation;

    public function __construct(__ScopeState $scope, __CancellationState $state)
    {
        $this->scope = $scope;
        $this->state = $state;
        $this->cancellation = new Cancellation($state);
    }

    public function spawn(callable $task, mixed ...$args): Awaitable
    {
        $scheduler = $this->scope()->scheduler();
        $awaitable = $scheduler->spawn($task, $args);
        unset($task);
        unset($args);
        unset($scheduler);
        return $awaitable;
    }

    public function reschedule(): void
    {
        $this->scope()->scheduler()->rescheduleCurrent();
    }

    public function sleep(float $seconds): void
    {
        $scheduler = $this->scope()->scheduler();
        $scheduler->sleepCurrent($seconds);
        unset($scheduler);
    }

    public function awaitReadable(mixed $stream, ?float $timeout = null): bool
    {
        return $this->scope()->scheduler()->awaitReadable($stream, $timeout);
    }

    public function awaitWritable(mixed $stream, ?float $timeout = null): bool
    {
        return $this->scope()->scheduler()->awaitWritable($stream, $timeout);
    }

    public function cancellation(): Cancellation
    {
        $scope = $this->scope;
        if ($scope === null) {
            throw new \Error("Elephc Async TaskGroup scope has ended");
        }
        $scheduler = $scope->scheduler();
        unset($scheduler);
        unset($scope);
        return $this->cancellation;
    }

    public function cancel(?\Throwable $reason = null): void
    {
        $scope = $this->scope;
        if ($scope === null) {
            throw new \Error("Elephc Async TaskGroup scope has ended");
        }
        $scheduler = $scope->scheduler();
        unset($scheduler);
        unset($scope);
        $this->state->request($reason);
    }

    public function __detach(): bool
    {
        $scope = $this->scope;
        if ($scope === null) {
            return true;
        }
        $scope->detach();
        $this->scope = null;
        unset($scope);
        return true;
    }

    private function scope(): __ScopeState
    {
        $scope = $this->scope;
        if ($scope === null) {
            throw new \Error("Elephc Async TaskGroup scope has ended");
        }
        return $scope;
    }

}

final class Awaitable
{
    private __Scheduler $scheduler;
    private int $taskId;

    public function __construct(__Scheduler $scheduler, int $taskId)
    {
        $this->scheduler = $scheduler;
        $this->taskId = $taskId;
    }

    public function await(): mixed
    {
        $scheduler = $this->scheduler;
        $result = $scheduler->awaitTask($this->taskId);
        unset($scheduler);
        return $result;
    }

    public function isComplete(): bool
    {
        return $this->scheduler->isComplete($this->taskId);
    }
}

function run(callable $body): mixed
{
    if (\Fiber::getCurrent() !== null) {
        throw new \Error("Elephc\\Async\\run() cannot be nested or called from a Fiber");
    }
    $state = new __CancellationState();
    $scheduler = new __Scheduler($state);
    $scope = new __ScopeState($scheduler);
    $group = new TaskGroup($scope, $state);
    $runResult = new __RunResult();
    $failed = false;
    $failure = null;
    try {
        $runResult->set($scheduler->runRoot($body, $group));
    } catch (\Throwable $caught) {
        $failed = true;
        $failure = $caught;
        unset($caught);
    }
    $scope->detach();
    unset($group);
    $scope = null;
    unset($scheduler);
    unset($state);
    unset($body);
    \__elephc_async_gc_collect();
    if ($failed) {
        unset($runResult);
        throw $failure;
    }
    $result = $runResult->get();
    unset($runResult);
    return $result;
}

}
