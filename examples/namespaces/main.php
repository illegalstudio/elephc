<?php

namespace Demo\App;

require "vendor/autoload.php";

use Demo\Domain\User;
use Demo\Http\Controller\HomeController;
use function Demo\Support\format_user as formatUser;
use const Demo\Support\APP_ENV;
use Demo\Theme\Default\Palette;

$controller = new HomeController();
$user = new User("nahime", "admin");

echo "env=" . APP_ENV . "\n";
echo $controller->index($user) . "\n";
echo formatUser($user) . "\n";
echo function_exists("\\Demo\\Support\\format_user") . "\n";
echo call_user_func("\\Demo\\Support\\format_user", $user) . "\n";
echo "accent=" . Palette::accent() . "\n";

// A namespaced constant may reuse a predefined constant's name.
const NAN = "not-a-number";
echo \constant(__NAMESPACE__ . "\\NAN") . "\n";

// Predefined constants can be written fully qualified, as namespaced code often does.
echo "max int digits: " . strlen((string) \PHP_INT_MAX) . \PHP_EOL;
