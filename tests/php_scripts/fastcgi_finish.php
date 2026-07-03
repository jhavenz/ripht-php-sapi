<?php

header('X-Ripht-Finalized: yes');

echo json_encode([
    'available' => function_exists('fastcgi_finish_request'),
]);

$first = fastcgi_finish_request();
$second = fastcgi_finish_request();

echo json_encode(['after' => true]);

error_log(json_encode([
    'first' => $first,
    'second' => $second,
]));
