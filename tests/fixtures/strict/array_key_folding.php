<?php
echo [1 => 'int', '1' => 'str', '01' => 'pad'][1], '|';
echo [1 => 'int', '1' => 'str']['1'], '|';
echo ['1' => 'old', '1' => 'new', 1 => 'int']['1'], '|';
echo ['01' => 'pad', 1 => 'int']['01'], '|';
$entries = [1 => 'int', '1' => 'str'];
echo $entries[1], '|', $entries['1'], '|';
echo [1 => ['1' => 'inner-int'], '1' => ['1' => 'inner-str']][1]['1'], '|';
echo ['a\0' => 'text', "a\0" => 'nul']["a\0"], '|';
echo 'abc'['1'];
