<?php

$markerPath = getenv('RIPHT_SHUTDOWN_EXIT_PATH');
$code = (int) ($_GET['code'] ?? 77);

register_shutdown_function(function () use ($markerPath, $code) {
    if ($markerPath !== false && $markerPath !== '') {
        file_put_contents($markerPath, 'shutdown');
    }

    exit($code);
});

echo 'before-shutdown';
