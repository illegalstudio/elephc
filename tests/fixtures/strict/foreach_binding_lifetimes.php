<?php
function foreach_open($value): mixed {return $value;}
class ForeachKeyObserver {
    public function __destruct() {global $foreachValue;echo 'K',$foreachValue,'|';}
}
class ForeachValueObserver {
    public function __destruct() {global $foreachKey;echo 'V',$foreachKey,'|';}
}
class ForeachValueThrow {
    public function __destruct() {global $foreachKey;echo 'T',$foreachKey,'|';throw new RuntimeException('value');}
}
class ForeachKeyThrow {
    public function __destruct() {global $foreachValue;echo 'U',$foreachValue,'|';throw new RuntimeException('key');}
}
class ForeachPreparedValue {public function __destruct() {echo 'P|';}}
class ForeachKeyFailure implements Iterator {
    public function rewind(): void {}
    public function valid(): bool {return true;}
    public function current(): mixed {echo 'C|';return new ForeachPreparedValue();}
    public function key(): mixed {global $foreachValue;echo 'Q',$foreachValue,'|';throw new RuntimeException('key callback');}
    public function next(): void {}
}
class ForeachReturnSource implements Iterator {
    public bool $fail;
    public function __construct(bool $fail) {$this->fail=$fail;}
    public function rewind(): void {}
    public function valid(): bool {return true;}
    public function current(): mixed {return 1;}
    public function key(): mixed {return 0;}
    public function next(): void {}
    public function __destruct() {
        gc_collect_cycles();
        if($this->fail) {echo 'I|';throw new RuntimeException('source');}
        echo 'J|';
    }
}
function foreach_return(bool $fail): string {
    foreach(new ForeachReturnSource($fail) as $value) {return str_repeat('r',64);}
    return '';
}
$foreachValue=foreach_open('before');
$foreachKey=foreach_open(new ForeachKeyObserver());
foreach([9=>'after'] as $foreachKey=>$foreachValue) {}
echo 'A',$foreachKey,':',$foreachValue,'|';
$foreachKey=foreach_open('old');
$foreachValue=foreach_open(new ForeachValueObserver());
foreach([9=>'after'] as $foreachKey=>$foreachValue) {}
echo 'B',$foreachKey,':',$foreachValue,'|';
$foreachKey=foreach_open('old');
$foreachValue=foreach_open(new ForeachValueThrow());
try {foreach([9=>'after'] as $foreachKey=>$foreachValue) {}}
catch(RuntimeException $error) {echo 'X',$foreachKey,':',$foreachValue,'|';}
$foreachValue=foreach_open('before');
$foreachKey=foreach_open(new ForeachKeyThrow());
try {foreach([9=>'after'] as $foreachKey=>$foreachValue) {}}
catch(RuntimeException $error) {echo 'Y',$foreachKey,':',$foreachValue,'|';}
$foreachValue=foreach_open('before');
$foreachKey=foreach_open('old');
try {foreach(new ForeachKeyFailure() as $foreachKey=>$foreachValue) {}}
catch(RuntimeException $error) {echo 'Z',$foreachKey,':',$foreachValue,'|';}
foreach(new ForeachKeyFailure() as $foreachValue) {echo 'N|';break;}
$foreachValue=null;
$same=foreach_open('old');
foreach([5=>'value'] as $same=>$same) {}
echo 'S',$same,'|';
echo 'L',strlen(foreach_return(false)),'|';
try {foreach_return(true);} catch(RuntimeException $error) {echo 'R|';}
