<?php
class NativeCycleOwner {
    public static int $destroyed = 0;
    public static ?NativeCycleOwner $saved = null;
    public mixed $container = null;
    public string $payload;
    public function __construct(public string $kind) {
        $this->payload = str_repeat('x', 4096);
        if ($kind === '/fixed') {
            $fixed = new SplFixedArray(1);
            $fixed[0] = $this;
            $this->container = $fixed;
        } elseif ($kind === '/stack') {
            $stack = new SplStack();
            $stack->push($this);
            $this->container = $stack;
        } elseif ($kind === '/queue') {
            $queue = new SplQueue();
            $queue->enqueue($this);
            $this->container = $queue;
        } elseif ($kind === '/list') {
            $list = new SplDoublyLinkedList();
            $list->push($this);
            $this->container = $list;
        } elseif ($kind === '/closure' || $kind === '/closure-started') {
            $owner = $this;
            $factory = static function() use ($owner) { yield $owner; return $owner; };
            $generated = $factory();
            if ($kind === '/closure-started') { $generated->current(); }
            $this->container = $generated;
        } else {
            $iterator = $this->rows();
            if ($kind !== '/fresh') { $iterator->current(); }
            if ($kind === '/returned') { $iterator->next(); }
            $this->container = $iterator;
        }
    }
    public function rows() { yield $this; return $this; }
    public function __destruct() {
        self::$destroyed++;
        if ($this->kind === '/resurrect') { $this->kind = '/fresh'; self::$saved = $this; }
        if ($this->kind === '/throw') { throw new RuntimeException('native cycle cleanup'); }
    }
}
