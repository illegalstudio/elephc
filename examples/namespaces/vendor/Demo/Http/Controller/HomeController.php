<?php

namespace Demo\Http\Controller;

use Demo\View\HtmlRenderer;

class HomeController {
    public function index($user) {
        $renderer = new HtmlRenderer();
        return $renderer->page("Dashboard", $user->badge());
    }
}
