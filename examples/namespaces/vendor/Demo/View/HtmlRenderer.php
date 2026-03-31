<?php

namespace Demo\View;

class HtmlRenderer {
    public function page($title, $body) {
        return "<h1>" . $title . "</h1> " . $body;
    }
}
