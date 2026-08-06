<?php

/*
 +-----------------------------------------------------------------------+
 | Local configuration for the Roundcube Webmail installation.           |
 |                                                                       |
 | This is a sample configuration file only containing the minimum       |
 | setup required for a functional installation. Copy more options       |
 | from defaults.inc.php to this file to override the defaults.          |
 |                                                                       |
 | This file is part of the Roundcube Webmail client                     |
 | Copyright (C) The Roundcube Dev Team                                  |
 |                                                                       |
 | Licensed under the GNU General Public License version 3 or            |
 | any later version with exceptions for skins & plugins.                |
 | See the README file for a full license statement.                     |
 +-----------------------------------------------------------------------+
*/

$config = [];

// Do not set db_dsnw here, use dpkg-reconfigure roundcube-core to configure database!
include("/etc/roundcube/debian-db-roundcube.php");

// IMAP host chosen to perform the log-in.
// Havenmail-Dovecot erzwingt ssl=required (siehe 10-ssl.conf) — daher
// tls:// (STARTTLS auf Port 143), nicht Klartext. Verbindung läuft über
// localhost, das Zertifikat ist aber auf mail.xfabio.de ausgestellt,
// daher verify_peer_name unten deaktiviert (siehe imap_conn_options).
$config['imap_host'] = ["tls://localhost:143"];

// SMTP server host (for sending mails). Ebenfalls STARTTLS, siehe oben.
$config['smtp_host'] = 'tls://localhost:587';

// SMTP username (if required) if you use %u as the username Roundcube
// will use the current username for login
$config['smtp_user'] = '%u';

// SMTP password (if required) if you use %p as the password Roundcube
// will use the current user's password for login
$config['smtp_pass'] = '%p';

// Zertifikat ist für mail.xfabio.de ausgestellt, wir verbinden aber über
// localhost — Hostname-Verifikation würde daher fälschlich fehlschlagen.
// Die Verbindung selbst bleibt verschlüsselt (STARTTLS), nur der
// Hostname-Abgleich wird übersprungen (Server verlässt die Maschine nicht).
$config['imap_conn_options'] = [
    'ssl' => [
        'verify_peer' => false,
        'verify_peer_name' => false,
    ],
];
$config['smtp_conn_options'] = [
    'ssl' => [
        'verify_peer' => false,
        'verify_peer_name' => false,
    ],
];

// provide an URL where a user can get support for this Roundcube installation
// PLEASE DO NOT LINK TO THE ROUNDCUBE.NET WEBSITE HERE!
$config['support_url'] = '';

// Name your service. This is displayed on the login screen and in the window title
$config['product_name'] = 'Havenmail Webmail';

// This key is used to encrypt the users imap password which is stored
// in the session record. For the default cipher method it must be
// exactly 24 characters long.
// YOUR KEY MUST BE DIFFERENT THAN THE SAMPLE VALUE FOR SECURITY REASONS
$config['des_key'] = '7TqwvtJwjMx18cTwRayb1sV6';

// List of active plugins (in plugins/ directory)
// Debian: install roundcube-plugins first to have any
$config['plugins'] = [
    'archive',
    'zipdownload',
    'managesieve',
];

// skin name: folder from skins/
$config['skin'] = 'havenmail';
$config['skin_logo'] = 'skins/havenmail/images/logo.svg';

// Disable spellchecking
// Debian: spellchecking needs additional packages to be installed, or calling external APIs
//         see defaults.inc.php for additional informations
$config['enable_spellcheck'] = false;
