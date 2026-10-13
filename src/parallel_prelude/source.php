<?php
extern "elephc_parallel" {
    function elephc_parallel_worker_active(): int;
    function elephc_parallel_parent_scope_enter(): int;
    function elephc_parallel_parent_scope_leave(): int;
    function elephc_parallel_parent_scope_active(): int;
    function elephc_parallel_job_create_php_serialized(ptr $source, int $sourceLen): int;
    function elephc_parallel_job_input_php_prepare(int $jobId): int;
    function elephc_parallel_job_complete_php_serialized(int $jobId, ptr $source, int $sourceLen): int;
    function elephc_parallel_job_fail_php_serialized(int $jobId, ptr $source, int $sourceLen): int;
    function elephc_parallel_job_result_php_prepare(int $jobId): int;
    function elephc_parallel_job_failure_php_prepare(int $jobId): int;
    function elephc_parallel_job_observe_failure(int $jobId): int;
    function elephc_parallel_job_failure_observed(int $jobId): int;
    function elephc_parallel_php_blob_ptr(): ptr;
    function elephc_parallel_php_blob_len(): int;
    function elephc_parallel_php_blob_release(): void;
    function elephc_parallel_php_buffer_alloc(int $length): ptr;
    function elephc_parallel_php_buffer_free(ptr $pointer, int $length): void;
    function elephc_parallel_job_phase(int $jobId): int;
    function elephc_parallel_job_wait(int $jobId): int;
    function elephc_parallel_job_cancel(int $jobId): int;
    function elephc_parallel_job_cancellation_requested(int $jobId): int;
    function elephc_parallel_job_release(int $jobId): int;
    function elephc_parallel_completion_generation(): int;
    function elephc_parallel_completion_wait(int $observed): int;
}

namespace Elephc\Parallel {

enum TaskFailureKind: int
{
    case PhpThrowable = 1;
    case PhpFatal = 2;
    case WorkerPanic = 3;
    case ContextUnavailable = 4;
    case ArenaExhausted = 5;
    case TransferEncode = 6;
    case TransferDecode = 7;
    case Infrastructure = 8;
}

final class TaskFailure extends \RuntimeException
{
    private TaskFailureKind $kind;
    private ?string $remoteClass;
    private ?string $remoteFile;
    private int $remoteLine;
    private array $remoteFrames;

    public function __construct(
        TaskFailureKind $kind,
        string $message,
        int $code = 0,
        ?string $remoteClass = null,
        ?string $remoteFile = null,
        int $remoteLine = 0,
        array $remoteFrames = [],
    ) {
        $this->kind = $kind;
        $this->message = $message;
        $this->code = $code;
        $this->remoteClass = $remoteClass;
        $this->remoteFile = $remoteFile;
        $this->remoteLine = $remoteLine;
        $this->remoteFrames = $remoteFrames;
    }

    public function kind(): TaskFailureKind
    {
        return $this->kind;
    }

    public function remoteClass(): ?string
    {
        return $this->remoteClass;
    }

    public function remoteFile(): ?string
    {
        return $this->remoteFile;
    }

    public function remoteLine(): int
    {
        return $this->remoteLine;
    }

    public function remoteFrames(): array
    {
        return $this->remoteFrames;
    }
}

final class TaskGroupFailure extends \RuntimeException
{
    private array $taskFailures;
    private ?\Throwable $rootFailure;

    public function __construct(array $failures, ?\Throwable $rootFailure = null)
    {
        $this->message = "One or more Elephc Parallel tasks failed";
        $this->code = 0;
        $this->taskFailures = $failures;
        $this->rootFailure = $rootFailure;
    }

    public function failures(): array
    {
        return $this->taskFailures;
    }

    public function rootFailure(): ?\Throwable
    {
        return $this->rootFailure;
    }
}

final class Future
{
    private int $jobId;
    private bool $settled = false;
    private bool $failed = false;
    private bool $observed = false;
    private bool $cancelled = false;
    private string $resultBytes;
    private TaskFailure $failure;
    private ?TaskGroup $scope = null;

    private function __construct(int $jobId)
    {
        $this->jobId = $jobId;
    }

    private function __clone(): void
    {
    }

    private static function __takePreparedBytes(): string
    {
        $length = \elephc_parallel_php_blob_len();
        $bytes = \__elephc_ptr_read_string(\elephc_parallel_php_blob_ptr(), $length);
        \elephc_parallel_php_blob_release();
        return $bytes;
    }

    private static function __takePreparedValue(): mixed
    {
        $bytes = self::__takePreparedBytes();
        $value = unserialize($bytes);
        unset($bytes);
        return $value;
    }

    public function __serialize(): array
    {
        throw new \Error("Elephc Parallel Future cannot be serialized");
    }

    public function __unserialize(array $data): void
    {
        unset($data);
        throw new \Error("Elephc Parallel Future cannot be unserialized");
    }

    public function __destruct()
    {
        if (!isset($this->jobId)) {
            return;
        }
        if ($this->settled && !$this->failed && !$this->cancelled) {
            unset($this->resultBytes);
        }
        if ($this->failed) {
            unset($this->failure);
        }
        if ($this->jobId !== 0) {
            \elephc_parallel_job_release($this->jobId);
            $this->jobId = 0;
        }
    }

    public function join(): mixed
    {
        $this->settle();
        if ($this->cancelled) {
            throw new \Elephc\Async\CancelledException("Elephc Parallel task cancelled");
        }
        if ($this->failed) {
            $scope = $this->scope;
            if ($scope !== null) {
                $scope->cancel();
            }
            unset($scope);
            $this->observed = true;
            if ($this->jobId !== 0) {
                \elephc_parallel_job_observe_failure($this->jobId);
            }
            throw $this->failure;
        }
        $result = unserialize($this->resultBytes);
        return $result;
    }

    public function isComplete(): bool
    {
        if ($this->settled || $this->jobId === 0) {
            return true;
        }
        return \elephc_parallel_job_phase($this->jobId) >= 3;
    }

    private function __scopeFailure(): ?TaskFailure
    {
        $this->settle();
        if ($this->failed && ($this->observed || \elephc_parallel_job_failure_observed($this->jobId) === 1)) {
            return null;
        }
        return $this->failed ? $this->failure : null;
    }

    private function __phase(): int
    {
        if ($this->settled) {
            return $this->failed ? 4 : ($this->cancelled ? 5 : 3);
        }
        if ($this->jobId === 0) {
            return 4;
        }
        return \elephc_parallel_job_phase($this->jobId);
    }

    private function __cancel(): void
    {
        if (!$this->settled && $this->jobId !== 0) {
            \elephc_parallel_job_cancel($this->jobId);
        }
    }

    private function __finishScope(): void
    {
        $this->settle();
        $this->scope = null;
        \elephc_parallel_job_release($this->jobId);
        $this->jobId = 0;
    }

    private function __attachScope(TaskGroup $scope): void
    {
        $this->scope = $scope;
    }

    private function settle(): void
    {
        if ($this->settled) {
            return;
        }
        $phase = \elephc_parallel_job_wait($this->jobId);
        if ($phase === 3) {
            $status = \elephc_parallel_job_result_php_prepare($this->jobId);
            if ($status === 0) {
                $length = \elephc_parallel_php_blob_len();
                $this->resultBytes = \__elephc_ptr_read_string(
                    \elephc_parallel_php_blob_ptr(),
                    $length,
                );
                \elephc_parallel_php_blob_release();
            } else {
                $this->failed = true;
                $this->failure = new TaskFailure(
                    TaskFailureKind::TransferDecode,
                    "Elephc Parallel could not decode the worker result",
                );
            }
        } elseif ($phase === 4) {
            $status = \elephc_parallel_job_failure_php_prepare($this->jobId);
            if ($status === 0) {
                $fields = self::__takePreparedValue();
                $remoteFrames = [];
                foreach ($fields[6] as $remoteFrame) {
                    $remoteFrames[] = $remoteFrame;
                }
                unset($remoteFrame);
                $this->failed = true;
                $this->failure = new TaskFailure(
                    TaskFailureKind::from($fields[0]),
                    $fields[2],
                    $fields[3],
                    $fields[1],
                    $fields[4],
                    $fields[5],
                    $remoteFrames,
                );
                unset($fields);
                unset($remoteFrames);
            } else {
                $this->failed = true;
                $this->failure = new TaskFailure(
                    TaskFailureKind::TransferDecode,
                    "Elephc Parallel could not decode the worker failure",
                );
            }
        } elseif ($phase === 5) {
            $this->cancelled = true;
        } else {
            $this->failed = true;
            $this->failure = new TaskFailure(
                TaskFailureKind::Infrastructure,
                "Elephc Parallel worker failed",
            );
        }
        $this->settled = true;
    }
}

final class TaskGroup
{
    private \Elephc\Async\__CancellationState $state;
    private \Elephc\Async\Cancellation $cancellation;
    private array $futures = [];
    private bool $closed = false;
    private bool $ownsParentRoot = false;

    private function __construct(\Elephc\Async\__CancellationState $state)
    {
        $this->state = $state;
        $this->cancellation = new \Elephc\Async\Cancellation($state);
        if (\elephc_parallel_parent_scope_enter() !== 1) {
            throw new \Error("Elephc\\Parallel\\run(): nested Parallel scopes are not supported in a parent root in v1");
        }
        $this->ownsParentRoot = true;
    }

    private function __clone(): void
    {
    }

    private static function __completeJobValue(int $jobId, mixed $value): int
    {
        $serialized = serialize($value);
        $length = strlen($serialized);
        $buffer = \elephc_parallel_php_buffer_alloc($length);
        if ($buffer === null) {
            throw new \Error("Elephc Parallel worker transfer buffer allocation failed");
        }
        \__elephc_ptr_write_string($buffer, $serialized);
        $status = \elephc_parallel_job_complete_php_serialized($jobId, $buffer, $length);
        \elephc_parallel_php_buffer_free($buffer, $length);
        return $status;
    }

    private static function __failJobValue(int $jobId, mixed $failure): int
    {
        $remoteFrames = [];
        foreach ($failure->getTrace() as $remoteFrame) {
            $remoteFunction = isset($remoteFrame["function"]) ? $remoteFrame["function"] : "";
            $remoteFile = isset($remoteFrame["file"]) ? $remoteFrame["file"] : null;
            $remoteLine = isset($remoteFrame["line"]) ? $remoteFrame["line"] : 0;
            $remoteFrames[] = [$remoteFunction, $remoteFile, $remoteLine];
        }
        $fields = [
            1,
            get_class($failure),
            $failure->getMessage(),
            $failure->getCode(),
            $failure->getFile(),
            $failure->getLine(),
            $remoteFrames,
        ];
        $serialized = serialize($fields);
        $length = strlen($serialized);
        $buffer = \elephc_parallel_php_buffer_alloc($length);
        if ($buffer === null) {
            throw new \Error("Elephc Parallel worker transfer buffer allocation failed");
        }
        \__elephc_ptr_write_string($buffer, $serialized);
        $status = \elephc_parallel_job_fail_php_serialized($jobId, $buffer, $length);
        \elephc_parallel_php_buffer_free($buffer, $length);
        unset($serialized);
        unset($fields);
        unset($remoteLine);
        unset($remoteFile);
        unset($remoteFunction);
        unset($remoteFrame);
        unset($remoteFrames);
        return $status;
    }

    public function __serialize(): array
    {
        throw new \Error("Elephc Parallel TaskGroup cannot be serialized");
    }

    public function __unserialize(array $data): void
    {
        unset($data);
        throw new \Error("Elephc Parallel TaskGroup cannot be unserialized");
    }

    public function __destruct()
    {
        $this->__cleanupOnExit();
    }

    private function __cleanupOnExit(): void
    {
        if ($this->closed) {
            $this->__releaseParentRoot();
            return;
        }
        if (!isset($this->state)) {
            return;
        }
        try {
            $this->__cancelAll();
            $failures = $this->__drain();
            unset($failures);
        } catch (\Throwable $cleanupFailure) {
            unset($cleanupFailure);
        } finally {
            try {
                $this->__close();
            } finally {
                $this->__releaseParentRoot();
            }
        }
    }

    public function spawn(\Closure $task, mixed ...$args): Future
    {
        $this->__assertSpawnable();
        unset($task);
        unset($args);
        throw new \Error("Elephc Parallel TaskGroup::spawn() was not lowered by the compiler");
    }

    public function cancellation(): \Elephc\Async\Cancellation
    {
        $this->__assertOpen();
        return $this->cancellation;
    }

    public function cancel(?\Throwable $reason = null): void
    {
        $this->__assertOpen();
        $this->__cancelAll($reason);
    }

    private function __assertOpen(): void
    {
        if (\elephc_parallel_worker_active() === 1) {
            throw new \Error("Elephc Parallel TaskGroup cannot be used from a Parallel worker");
        }
        if ($this->closed) {
            throw new \Error("Elephc Parallel TaskGroup cannot be used after its Parallel\\run() scope has closed");
        }
    }

    private function __assertSpawnable(): void
    {
        $this->__assertOpen();
        if ($this->state->isRequested()) {
            throw new \Error("Elephc Parallel TaskGroup cannot spawn after cancellation has been requested");
        }
    }

    private function __recordFuture(Future $future): void
    {
        $future->__attachScope($this);
        $this->futures[] = $future;
    }

    private function __drain(): array
    {
        $generation = \elephc_parallel_completion_generation();
        while (true) {
            $allComplete = true;
            $hasFailure = false;
            foreach ($this->futures as $future) {
                $phase = $future->__phase();
                // Native phases: 1 queued, 2 running, 3 completed, 4 failed, 5 cancelled.
                // Only queued/running are incomplete, and cancellation is terminal but not failure.
                if ($phase < 3) {
                    $allComplete = false;
                } elseif ($phase === 4) {
                    $hasFailure = true;
                }
            }
            unset($future);
            if ($hasFailure) {
                $this->__cancelAll();
            }
            if ($allComplete) {
                break;
            }
            $generation = \elephc_parallel_completion_wait($generation);
        }
        $failures = [];
        foreach ($this->futures as $future) {
            $failure = $future->__scopeFailure();
            if ($failure !== null) {
                $failures[] = $failure;
            }
            unset($failure);
            $future->__finishScope();
            unset($future);
        }
        unset($future);
        unset($this->futures);
        return $failures;
    }

    private function __cancelAll(?\Throwable $reason = null): void
    {
        $this->state->request($reason);
        foreach ($this->futures as $future) {
            $future->__cancel();
            unset($future);
        }
        unset($future);
    }

    private function __close(): void
    {
        if ($this->closed) {
            return;
        }
        $this->closed = true;
        unset($this->futures);
        unset($this->cancellation);
        $this->state->__clearReason();
        unset($this->state);
    }

    private function __releaseParentRoot(): void
    {
        if ($this->ownsParentRoot) {
            \elephc_parallel_parent_scope_leave();
            TaskGroup::__assertFiberSuspendAllowed();
            $this->ownsParentRoot = false;
        }
    }

    private static function __assertFiberSuspendAllowed(): void
    {
        if (
            \elephc_parallel_parent_scope_active() === 1
            || \elephc_parallel_worker_active() === 1
        ) {
            throw new \Error("Fiber::suspend() is not supported while an Elephc\\Parallel scope or worker is active in v1");
        }
    }

    private static function __assertFiberSuspendCallableAllowed(): void
    {
        if (
            \elephc_parallel_parent_scope_active() === 1
            || \elephc_parallel_worker_active() === 1
        ) {
            TaskGroup::__assertFiberSuspendAllowed();
        }
        \Elephc\Async\__Scheduler::__assertFiberSuspendAllowed();
    }

    private static function __isFiberSuspendCallableArray(array $callback): bool
    {
        return
            is_array($callback)
            &&
            count($callback) === 2
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
        throw new \Error("Elephc Parallel callable guard was not lowered");
    }
}

function run(\Closure $body): mixed
{
    if (\elephc_parallel_worker_active() === 1) {
        throw new \Error("Elephc\\Parallel\\run(): nested Parallel scopes are not supported inside a Parallel worker in v1");
    }
    $state = new \Elephc\Async\__CancellationState();
    $tasks = new TaskGroup($state);
    unset($state);
    try {
    $resultBox = [];
    $rootFailed = false;
    $rootFailure = null;
    try {
        $resultBox[] = $body($tasks);
    } catch (\Throwable $caught) {
        $rootFailed = true;
        $rootFailure = $caught;
        $tasks->__cancelAll($caught);
        unset($caught);
    }
    $failures = $tasks->__drain();
    $tasks->__close();
    unset($body);
    \__elephc_async_gc_collect();
    if ($rootFailed && count($failures) !== 0) {
        unset($resultBox);
        $groupFailure = new TaskGroupFailure($failures, $rootFailure);
        unset($failures);
        throw $groupFailure;
    }
    if ($rootFailed) {
        unset($resultBox);
        unset($failures);
        throw $rootFailure;
    }
    if (count($failures) !== 0) {
        unset($resultBox);
        $groupFailure = new TaskGroupFailure($failures);
        unset($failures);
        throw $groupFailure;
    }
    $result = $resultBox[0];
    unset($resultBox);
    unset($failures);
    return $result;
    } finally {
        $tasks->__cleanupOnExit();
    }
}

}
