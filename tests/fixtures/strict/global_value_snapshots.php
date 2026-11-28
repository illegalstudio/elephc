<?php
$scalarSnapshotSource = 1;
function makeScalarSnapshot196(): callable {
    global $scalarSnapshotSource;
    return function() use ($scalarSnapshotSource): mixed { return $scalarSnapshotSource; };
}
$scalarSnapshot = makeScalarSnapshot196();
$scalarSnapshotSource = 2;
echo $scalarSnapshot(), ':', $scalarSnapshotSource, '|';

$textSnapshotSource = 'old';
function makeTextSnapshot196(): callable {
    global $textSnapshotSource;
    return function() use ($textSnapshotSource): mixed { return $textSnapshotSource; };
}
$textSnapshot = makeTextSnapshot196();
$textSnapshotSource = 'next';
echo $textSnapshot(), ':', $textSnapshotSource, '|';

$arraySnapshotSource = [1];
function makeArraySnapshot196(): callable {
    global $arraySnapshotSource;
    return function() use ($arraySnapshotSource): mixed { return $arraySnapshotSource; };
}
$arraySnapshot = makeArraySnapshot196();
$arraySnapshotSource = [2];
$arrayCopy196 = $arraySnapshot();
echo $arrayCopy196[0], ':', $arraySnapshotSource[0], '|';

class GlobalSnapshotValue196 {
    public function __construct(public int $number) {}
}
$objectSnapshotSource = new GlobalSnapshotValue196(1);
function makeObjectSnapshot196(): callable {
    global $objectSnapshotSource;
    return function() use ($objectSnapshotSource): mixed { return $objectSnapshotSource; };
}
$objectSnapshot = makeObjectSnapshot196();
$objectSnapshotSource = new GlobalSnapshotValue196(2);
$objectCopy196 = $objectSnapshot();
echo $objectCopy196->number, ':', $objectSnapshotSource->number, '|';
