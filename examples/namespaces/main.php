<?php

namespace Demo\App;

require "vendor/autoload.php";

use Demo\Domain\User;
use Demo\Http\Controller\HomeController;
use function Demo\Support\format_user as formatUser;
use const Demo\Support\APP_ENV;

$controller = new HomeController();
$user = new User("nahime", "admin");

echo "env=" . APP_ENV . "\n";
echo $controller->index($user) . "\n";
echo formatUser($user) . "\n";
echo function_exists("formatUser") . "\n";
echo call_user_func("formatUser", $user) . "\n";
