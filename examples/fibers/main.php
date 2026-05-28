<?php

// Fiber: cooperative coroutines (PHP 8.1+).
// A fiber owns its own call stack and can suspend execution at any depth.
// Control alternates explicitly between the caller and the fiber via
// $f->start() / $f->resume($v) and Fiber::suspend($v).

// 1) Run a fiber to completion without suspending.
$hello = new Fiber(function(): void {
    echo "hello from inside the fiber\n";
});
echo "before start\n";
$hello->start();
echo "after start, terminated=";
if ($hello->isTerminated()) {
    echo "true\n";
} else {
    echo "false\n";
}

// 2) Round-trip values across suspend/resume.
//    Suspend/resume pass values as Mixed payloads, so any scalar or string
//    flows through transparently. Echoing each Mixed cell back unboxes it
//    via __rt_mixed_write_stdout.
$counter = new Fiber(function(): void {
    $a = Fiber::suspend("first");
    echo " [got " . $a . "]";
    $b = Fiber::suspend("second");
    echo " [got " . $b . "]";
    Fiber::suspend("third");
});
echo "round trip: ";
echo $counter->start();              // prints "first"
echo "/";
echo $counter->resume("alpha");      // prints " [got alpha]second"
echo "/";
echo $counter->resume("beta");       // prints " [got beta]third"
echo "\n";

// 3) Cooperative scheduler over two fibers.
//    Each fiber yields after each step; the main loop interleaves them
//    until both terminate.
$ping = new Fiber(function(): void {
    Fiber::suspend("p1");
    Fiber::suspend("p3");
    Fiber::suspend("p5");
});
$pong = new Fiber(function(): void {
    Fiber::suspend("o2");
    Fiber::suspend("o4");
    Fiber::suspend("o6");
});

echo "interleaved:";
$a = $ping->start();
$b = $pong->start();
echo " " . $a . " " . $b;
while (!$ping->isTerminated() || !$pong->isTerminated()) {
    if (!$ping->isTerminated()) {
        $a = $ping->resume("");
        echo " " . $a;
    }
    if (!$pong->isTerminated()) {
        $b = $pong->resume("");
        echo " " . $b;
    }
}
echo "\n";

// 4) Fiber->throw() re-raises an exception at the pending suspend point.
//    The fiber's own try/catch sees the exception and can recover from it.
$err = new Fiber(function(): void {
    try {
        Fiber::suspend(0);
        echo "not reached\n";
    } catch (FiberError $e) {
        echo "fiber caught the throw: " . $e->getMessage() . "\n";
    }
});
$err->start();
$err->throw(new FiberError("delivered"));

// 5) Closures with use(...) captures bind values from the surrounding scope at
//    construction time. Captures ride in the callable descriptor, so
//    $f->start() never overwrites them and they survive the original variable
//    being reassigned.
$base = 100;
$step = 7;
$adder = new Fiber(function() use ($base, $step): void {
    echo "base=" . $base . " step=" . $step . " sum=" . ($base + $step) . "\n";
});
$adder->start();

// 6) Mixed-type captures: int, string, and float keep their descriptor values
//    separate from the visible start() arguments.
$tag = "rate";
$count = 4;
$rate = 0.5;
$mixer = new Fiber(function() use ($tag, $count, $rate): void {
    echo $tag . ": " . $count . " * " . $rate . " = " . ($count * $rate) . "\n";
});
$mixer->start();

// 7) Reference semantics: object captures share the same heap allocation, so
//    multiple fibers can mutate state visible to each other and to main.
class Counter { public int $value = 0; }
$shared = new Counter();
$f_inc = new Fiber(function() use ($shared): void { $shared->value = $shared->value + 1; });
$f_dbl = new Fiber(function() use ($shared): void { $shared->value = $shared->value * 2; });
$shared->value = 5;
$f_inc->start();        // 5 + 1 = 6
$f_dbl->start();        // 6 * 2 = 12
echo "shared counter ended at " . $shared->value . "\n";

// 8) First-class method callables keep their receiver in the descriptor too.
class FiberLabeler {
    public function __construct(private string $prefix) {}
    public function label(string $value): string {
        echo $this->prefix . $value . "\n";
        return $this->prefix . "done";
    }
}
$labeler = new FiberLabeler("method:");
$method_fiber = new Fiber($labeler->label(...));
$method_fiber->start("run");
echo $method_fiber->getReturn() . "\n";

// 9) Runtime descriptors also cover string callbacks, callable arrays, and __invoke objects.
class FiberCallableJob {
    public function __construct(private string $prefix) {}
    public function run(string $value): string {
        echo $this->prefix . $value . "\n";
        return $this->prefix . "done";
    }
    public function __invoke(string $value): string {
        echo "invoke:" . $value . "\n";
        return "invoke:done";
    }
}

$string_fiber = new Fiber("strlen");
$string_fiber->start("string-run");
echo "string length=" . $string_fiber->getReturn() . "\n";

$array_job = new FiberCallableJob("array:");
$array_fiber = new Fiber([$array_job, "run"]);
$array_fiber->start("run");
echo $array_fiber->getReturn() . "\n";

$stored_cb = [$array_job, "run"];
$stored_fiber = new Fiber($stored_cb);
$stored_cb = [new FiberCallableJob("later:"), "run"];
$stored_fiber->start("stored");
echo $stored_fiber->getReturn() . "\n";

$invoke_fiber = new Fiber($array_job);
$invoke_fiber->start("run");
echo $invoke_fiber->getReturn() . "\n";

// 10) Variadic callbacks collect extra start() values in the usual ...$items array.
$batch = new Fiber(function(string $label, ...$items): int {
    echo $label . " count=" . count($items) . "\n";
    return count($items);
});
$batch->start("batch", 10, 20, 30);
echo "variadic return=" . $batch->getReturn() . "\n";

// 11) FiberError is an Error subclass — catch it directly or through Error.
try {
    throw new FiberError("manual");
} catch (Error $e) {
    echo "caught FiberError\n";
}
