<?php

$type = $_GET['type'] ?? 'none';

switch ($type) {
    case '301':
        header('Location: /redirected.php', true, 301);
        echo "Moved Permanently";
        break;

    case '302':
        header('Location: /temporary.php');
        http_response_code(302);
        echo "Found";
        break;

    case '303':
        header('Location: /see-other.php', true, 303);
        echo "See Other";
        break;

    case '307':
        header('HTTP/1.1 307 Temporary Redirect');
        header('Location: /temp-redirect.php');
        echo "Temporary Redirect";
        break;

    case '308':
        header('HTTP/1.1 308 Permanent Redirect');
        header('Location: /perm-redirect.php');
        echo "Permanent Redirect";
        break;

    case 'relative':
        header('Location: ../other/path.php');
        echo "Relative redirect";
        break;

    case 'external':
        header('Location: https://example.com/external', true, 302);
        echo "External redirect";
        break;

    default:
        header('Content-Type: application/json');
        echo json_encode(['status' => 'no redirect']);
}
