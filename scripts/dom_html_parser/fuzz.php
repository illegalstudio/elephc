<?php
mt_srand((int)($argv[1] ?? 1));
$tags = ['p','div','span','b','i','a','ul','li','ol','table','tr','td','th','tbody','head','body','html','title','script','style','br','hr','img','h1','h2','form','input','select','option','pre','center','font','dd','dt','dl','frameset','frame','noscript','meta','link','textarea','em','x:y','foo'];
$bits = ['text',' ','  ',"\n",'&amp;','&lt;','&nbsp;','&#233;','&#x41;','&bogus;','&','<','>','<!-- c -->','<!--x','<?pi data?>','<?pi>','<![CDATA[x]]>','<!DOCTYPE html>','<!doctype html public "-//W3C//DTD HTML 4.01//EN" "x">',"\xe9", '"', "'", '=', '/'];
$cases = [];
for ($k = 0; $k < 3000; $k++) {
    $s = '';
    $n = mt_rand(1, 14);
    for ($j = 0; $j < $n; $j++) {
        $r = mt_rand(0, 9);
        $t = $tags[array_rand($tags)];
        if ($r < 3) { $s .= "<$t>"; }
        elseif ($r < 5) { $s .= "</$t>"; }
        elseif ($r < 6) { $s .= "<$t a=1 b=\"two\" c hidden class='k'>"; }
        elseif ($r < 7) { $s .= "<$t/>"; }
        else { $s .= $bits[array_rand($bits)]; }
    }
    $cases[] = $s;
}
file_put_contents(($argv[2] ?? '/tmp/cases.ser'), serialize($cases));
