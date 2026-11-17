<?php
function localMemberValue(mixed $value): mixed { return $value; }
function putLocalMember(&$target, mixed $value): void { $target = $value; }

$nullMember = null;
$nullMember = localMemberValue(null);
echo gettype($nullMember), '|';
$textMember = 'before';
$textMember = localMemberValue('after');
$boolMember = true;
$boolMember = localMemberValue(false);
echo $textMember, '|', gettype($boolMember), '|', $boolMember, '|';
try { putLocalMember($textMember, 2); echo 'W'; }
catch (TypeError $error) { echo 'T'; }
echo $textMember, '|';
putLocalMember($nullMember, null);
try { putLocalMember($nullMember, 2); echo 'W'; }
catch (TypeError $error) { echo 'T'; }
echo gettype($nullMember), '|';

class LocalValueBase {}
class LocalValueChild extends LocalValueBase {}
class LocalValueOther {}
$objectMember = new LocalValueBase();
putLocalMember($objectMember, new LocalValueChild());
echo get_class($objectMember), '|';

class RejectedLocalValue {
    public function __destruct() { echo 'D'; }
}
$destructorMember = new LocalValueBase();
try { $destructorMember = localMemberValue(new RejectedLocalValue()); echo 'W'; }
catch (TypeError $error) { echo 'T'; }
echo get_class($destructorMember), '|';
try { putLocalMember($objectMember, new LocalValueOther()); echo 'W'; }
catch (TypeError $error) { echo 'T'; }
echo get_class($objectMember), '|';

$arrayMember = [1];
putLocalMember($arrayMember, ['next']);
echo $arrayMember[0], '|';
try { putLocalMember($arrayMember, 2); echo 'W'; }
catch (TypeError $error) { echo 'T'; }
echo gettype($arrayMember), '|';
$inferredArray = [1];
$inferredArray = ['next'];
echo $inferredArray[0], '|';

function falseLocalMember(?false $owner): void {
    putLocalMember($owner, false);
    echo gettype($owner), '|';
    try { putLocalMember($owner, true); echo 'W'; }
    catch (TypeError $error) { echo 'T'; }
    putLocalMember($owner, null);
    echo gettype($owner), '|';
}
falseLocalMember(false);
int|string $unionMember = 1;
$unionMember = localMemberValue('next');
echo $unionMember, '|';
array<int> $declaredArray = [1];
$declaredArray = [2];
echo $declaredArray[0], '|';
gc_collect_cycles();
