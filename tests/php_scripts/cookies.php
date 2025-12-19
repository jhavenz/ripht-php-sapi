<?php
/**
 * Cookie handling test script.
 * 
 * Tests $_COOKIE superglobal and Set-Cookie header generation.
 */

header('Content-Type: application/json');

if (isset($_GET['set_cookie'])) {
    setcookie('test_cookie', 'test_value', time() + 3600, '/');
}

$response = [
    'method' => $_SERVER['REQUEST_METHOD'] ?? 'UNKNOWN',
    'cookies' => $_COOKIE,
    'cookie_count' => count($_COOKIE),
];

echo json_encode($response, JSON_PRETTY_PRINT);

