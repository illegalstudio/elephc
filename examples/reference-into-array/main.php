<?php

// Reference assignment into array elements and object properties.
//
// `$arr[$key] =& $source` makes the array element and the source variable share
// one underlying value: writing either is observed through the other. The same
// works for a stdClass dynamic property, `$obj->prop =& $source`.

// --- Array element shares a variable -------------------------------------

$total = 0;
$report = [
    'label'   => 'sales',
    'total'   => 0,
];

// Alias the running total into the report so both stay in sync.
$report['total'] =& $total;

$total += 10;
$total += 32;

echo "Report total: ", $report['total'], "\n";   // 42 — written through $total

$report['total'] += 8;
echo "Counter:      ", $total, "\n";              // 50 — written back through the element

// --- Two independent aliases in one array --------------------------------

$min = 0;
$max = 0;
$bounds = ['min' => 0, 'max' => 0, 'unit' => 'px'];
$bounds['min'] =& $min;
$bounds['max'] =& $max;

$min = 5;
$max = 95;
echo "Range:        ", $bounds['min'], "..", $bounds['max'], "\n";   // 5..95

// --- Object property shares a variable -----------------------------------

$score = 1;
$player = new stdClass();
$player->score =& $score;

$score = 100;
echo "Player score: ", $player->score, "\n";      // 100 — written through $score

// Writing the property is likewise observed through the variable.
$health = 10;
$enemy = new stdClass();
$enemy->hp =& $health;
$enemy->hp = 0;
echo "Enemy hp:     ", $health, "\n";             // 0 — written back through the property
