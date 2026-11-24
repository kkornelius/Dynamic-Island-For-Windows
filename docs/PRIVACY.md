# Privacy and data handling

This describes app code paths, not Windows or provider policies.

## Local storage

Preferences, plaintext Notes, optional lyrics/weather caches and bounded error logs live under `%LOCALAPPDATA%\DynamicIslandForWindows\`. Files use temporary-file replacement. Enabling startup creates a stable `app/` EXE copy and a current-user Run entry.

There is no app account, advertising, telemetry or cloud note sync. Clipboard access occurs only on copy/cut/paste; clipboard history is not monitored. Notes are not encrypted.

## Audio/media

WASAPI loopback reads the combined default Windows output, including other apps and notifications. Levels are processed locally in memory; recordings are not saved or uploaded. Disabling the visualizer closes capture. Windows media-session metadata/artwork drive the player. No Spotify login credentials or cookies are requested.

## Network requests

| Feature | Destination | Data |
|---|---|---|
| Weather | Open-Meteo forecast | Selected city coordinates and forecast fields |
| City search | Open-Meteo geocoding | Entered city-search text |
| Optional lyrics | LRCLIB | Track title, artist, album, duration |

Weather has a default city and refreshes while running. GPS/IP-based location lookup is not implemented. Lyrics start disabled. `--offline` disables online weather/lyrics and retains cached/local functionality.

Providers receive normal connection information, including the requesting IP address. See [Open-Meteo terms/privacy](https://open-meteo.com/en/terms), [geocoding](https://open-meteo.com/en/docs/geocoding-api), and [LRCLIB](https://lrclib.net/docs).

## Desktop/devices

Pointer position is processed locally for hover/eyes. Fullscreen/browser avoidance examines window/control geometry and edit focus. Address-bar avoidance does not read the entered URL/search text, page content or history. Bluetooth notices use paired-device properties. These features do not upload browser history, screen recordings or device names.

## Browser playback correction

For browser YouTube playback, the app can cross-check stale Windows media metadata against the browser window caption and the player's seek/play/pause accessibility controls. It reads numeric playback position/duration and control labels only. It does not retrieve URLs, address/search field values, comments, page body text, cookies or browser history. Checks run on the media worker and do not require a browser extension.

## Removal

Turn startup off, quit and remove the portable EXE/stable `app/` copy. Remaining data is retained for Notes/preferences. Delete it separately only to erase those files. [Removal guide](USAGE.md#remove).
