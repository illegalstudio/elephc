<?php
class InitializedRecursiveGetter {
    public $value;
    public int $calls = 0;
    public bool $recurse = true;
    public function __get(string $name): mixed {
        $this->calls += 1;
        if ($this->calls > 3) { throw new Error('bounded recursion'); }
        if ($this->recurse) { return $this->$name; }
        return 'ready';
    }
}
$name = $argc > 0 ? 'value' : 'other';
$recursive = new InitializedRecursiveGetter();
unset($recursive->value);
try { echo $recursive->$name; } catch (Error $error) { echo 'E'; }
echo ':', $recursive->calls, '|';
$recursive->recurse = false;
echo $recursive->$name, ':', $recursive->calls, '|';
class InitializedOtherGetter {
    public $left;
    public $right;
    public int $calls = 0;
    public function __get(string $name): mixed {
        $this->calls += 1;
        if ($name === 'left') { return $this->right; }
        return 'right';
    }
}
$other = new InitializedOtherGetter();
unset($other->left, $other->right);
echo $other->left, ':', $other->calls, '|';

class InitializedStringGetter {
    public $value;
    public function __get(string $name): string { return str_repeat('text', 1); }
}
class InitializedFloatGetter {
    public $value;
    public function __get(string $name): float { return 2.5; }
}
$stringGetter = new InitializedStringGetter();
$floatGetter = new InitializedFloatGetter();
unset($stringGetter->value, $floatGetter->value);
echo $stringGetter->value, ':', $floatGetter->value, '|';
class InitializedPeerGetter {
    public $value;
    public int $calls = 0;
    public ?InitializedPeerGetter $peer = null;
    public function __get(string $name): mixed {
        $this->calls += 1;
        if ($this->peer !== null) { return $this->peer->value; }
        return 'other';
    }
}
$peerOuter = new InitializedPeerGetter();
$peerInner = new InitializedPeerGetter();
$peerOuter->peer = $peerInner;
unset($peerOuter->value, $peerInner->value);
echo $peerOuter->value, ':', $peerOuter->calls, ':', $peerInner->calls, '|';

class InitializedUntypedGetter {
    public $value;
    public function __get($name): mixed { return $name; }
}
$untypedGetter = new InitializedUntypedGetter();
unset($untypedGetter->value);
echo $untypedGetter->value, '|';
