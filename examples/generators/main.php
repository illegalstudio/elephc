<?php
function base() {
    yield 1;
    yield 2;
    return 99;
}

function pipeline() {
    yield 0;
    return yield from base();
}

$g = pipeline();
foreach ($g as $k => $v) {
    echo $k;
    echo ":";
    echo $v;
    echo " ";
}
echo "ret=";
echo $g->getReturn();
echo "\n";

$start = 5;
$make = function() use ($start) {
    yield $start;
    yield $start + 1;
};

foreach ($make() as $v) {
    echo $v;
    echo " ";
}
echo "\n";

function responder() {
    $value = yield 10;
    yield $value;
    return 77;
}

$r = responder();
$r->rewind();
echo $r->current();
echo " ";
$r->send(42);
echo $r->current();
echo " ";
$r->next();
echo $r->getReturn();
echo "\n";

// Generator::throw() raises the exception at the suspended `yield`, so a
// try/catch *inside* the generator body handles it and execution resumes.
function worker() {
    while (true) {
        try {
            $job = yield "ready";
            echo "did:" . $job . " ";
        } catch (Exception $e) {
            echo "recovered:" . $e->getMessage() . " ";
        }
    }
}

$w = worker();
echo $w->current();                          // ready
echo " ";
echo $w->send("a");                          // did:a ready
echo $w->throw(new Exception("oops"));       // recovered:oops ready
echo "\n";
