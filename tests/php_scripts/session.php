<?php
/**
 * Session handling test script.
 * 
 * Tests PHP session functionality and cookie generation.
 */

header('Content-Type: application/json');

session_start();

if (!isset($_SESSION['visit_count'])) {
    $_SESSION['visit_count'] = 0;
}

$_SESSION['visit_count']++;

$response = [
    'session_id' => session_id(),
    'visit_count' => $_SESSION['visit_count'],
    'session_data' => $_SESSION,
];

echo json_encode($response, JSON_PRETTY_PRINT);

