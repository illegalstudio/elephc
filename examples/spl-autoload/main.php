<?php

// Demonstrates the full compile-time autoload + introspection surface.
//
// 1. composer.json PSR-4 — App\* classes are resolved via the
//    autoload.psr-4 mapping in composer.json. Acme\Widgets\Banner
//    comes from vendor/acme/widgets/composer.json the same way.
//
// 2. spl_autoload_register with a closure — the compiler reads the
//    closure body at compile time and runs it symbolically against
//    each unknown class name to derive the file path. The closure
//    below is a hand-rolled PSR-0-style autoloader that loads
//    lib/App_Helper.php under the App_ prefix.
//
// 3. class_alias — synthesises `class WelcomeBanner extends Banner {}`
//    at compile time so the alias works as a drop-in name. Both class
//    names only need to be compile-time constants: a string literal,
//    `Banner::class` (resolved through the `use` import below, as PHP
//    does), or a `.` concatenation of those, like the versioned
//    `Acme\Widgets\BannerV1` alias that keeps an older name working.
//
// 4. class_exists / interface_exists / is_a — compile-time-folded
//    introspection: literal class names trigger autoload, and
//    instanceof checks decide statically.
//
// 5. spl_object_id / get_class / get_declared_classes — runtime
//    introspection helpers backed by the AOT class registry.

use Acme\Widgets\Banner;

spl_autoload_register(function ($name) {
    require_once __DIR__ . '/lib/' . str_replace('\\', '_', $name) . '.php';
});

class_alias(Banner::class, "WelcomeBanner");
class_alias(Banner::class, Banner::class . "V1");

if (class_exists("App\\Models\\User")) {
    $user = new App\Models\User("Ada");
    $service = new App\Service();
    $banner = new WelcomeBanner("welcome");
    $legacyBanner = new Acme\Widgets\BannerV1("legacy");
    $helper = new App_Helper();

    echo $service->welcome($user) . PHP_EOL;
    echo $banner->render() . PHP_EOL;
    echo $legacyBanner->render() . PHP_EOL;
    echo $helper->greet() . PHP_EOL;

    echo "User class: " . get_class($user) . PHP_EOL;
    echo "User id: " . spl_object_id($user) . PHP_EOL;
    echo "User instanceof App\\Models\\User: "
        . (is_a($user, "App\\Models\\User") ? "yes" : "no")
        . PHP_EOL;

    echo "Total compiled classes: " . count(get_declared_classes()) . PHP_EOL;
    echo "SPL classes shipped: " . count(spl_classes()) . PHP_EOL;
}
