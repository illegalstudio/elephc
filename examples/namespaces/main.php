<?php

namespace Demo\App;

require "support.php";

use Demo\Lib\User;
use function Demo\Lib\render as paint;
use const Demo\Lib\APP_NAME;

$user = new User("nahime");

echo APP_NAME . "\n";
echo paint($user->label()) . "\n";
echo function_exists("paint") . "\n";
echo call_user_func("paint", "ready") . "\n";
