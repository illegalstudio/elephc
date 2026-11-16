<?php
function boxedCapturedCallable(mixed $value): mixed { return $value; }
class CapturedCycleHolder {
    public static int $destroyed = 0;
    public static ?CapturedCycleHolder $saved = null;
    public $callback = null;
    public ?Fiber $fiber = null;
    public string $payload;
    public string $kind;
    public function __construct(string $kind) {
        $this->kind = $kind;
        $this->payload = str_repeat('x', 4096);
        if ($kind === '/arrow') {
            $this->callback = fn(): int => strlen($this->payload);
        } elseif ($kind === '/value') {
            $valueOwner = $this;
            $this->callback = static function() use ($valueOwner): int { return strlen($valueOwner->payload); };
        } elseif ($kind === '/reference') {
            $referenceOwner = $this;
            $this->callback = static function() use (&$referenceOwner): int { return strlen($referenceOwner->payload); };
        } elseif ($kind === '/array') {
            $arrayOwner = $this;
            $arrayEntry = static fn(): int => strlen($arrayOwner->payload);
            $arrayItems = [$arrayEntry];
            $this->callback = static function() use ($arrayItems): int { $read = $arrayItems[0]; return $read(); };
        } elseif ($kind === '/hash') {
            $hashOwner = $this;
            $hashEntry = static fn(): int => strlen($hashOwner->payload);
            $mapItems = ['read' => $hashEntry];
            $this->callback = static function() use ($mapItems): int { $read = $mapItems['read']; return $read(); };
        } elseif ($kind === '/boxed') {
            $boxedOwner = $this;
            $boxedEntry = static fn(): int => strlen($boxedOwner->payload);
            $boxed = boxedCapturedCallable($boxedEntry);
            $this->callback = static fn(): int => $boxed();
        } elseif ($kind === '/ref-callable') {
            $callableOwner = $this;
            $callableEntry = static fn(): int => strlen($callableOwner->payload);
            $this->callback = static function() use (&$callableEntry): int { return $callableEntry(); };
        } elseif ($kind === '/nested') {
            $inner = fn(): int => strlen($this->payload);
            $this->callback = static fn(): int => $inner();
        } elseif ($kind === '/bound') {
            $original = function(): int { return strlen($this->payload); };
            $this->callback = Closure::bind($original, $this);
        } elseif ($kind === '/method') {
            $this->callback = $this->size(...);
        } elseif ($kind === '/fiber') {
            $this->fiber = new Fiber(fn(): int => strlen($this->payload));
            $this->callback = static fn(): int => 4096;
        } else {
            $this->callback = function(): int { return strlen($this->payload); };
        }
    }
    public function size(): int { return strlen($this->payload); }
    public function invoke(): int { $read = $this->callback; return $read(); }
    public function __destruct() {
        self::$destroyed++;
        if ($this->kind === '/throw') { throw new RuntimeException('captured-cycle cleanup'); }
        if ($this->kind === '/resurrect') { self::$saved = $this; }
        if ($this->kind === '/mutate') { $this->callback = static fn(): int => 0; }
    }
}
