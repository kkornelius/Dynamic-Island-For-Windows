# User guide

Hover anywhere on the capsule to expand. Moving away closes a hover preview. Click a feature to pin it; **−** collapses and unpins it. Drag the expanded header to reposition. Floating/Anchored positions are separate. Alt temporarily gives access to covered controls. The tray menu can show, hide, reset or quit.

## Media and lyrics

Start playback in a Windows media-session-compatible app. Available controls depend on that session. The lyrics button widens the panel; enable lookup in **Settings → Lyrics**. Timing uses the supplied timeline. Some tracks provide untimed lyrics or no result, even if Spotify has lyrics.

The bars display combined system-output amplitude history. Sensitivity changes the visual response; smoothing changes decay. They do not modify sound. Turning the visualizer off closes its loopback capture stream.

## Notes

The paper-note icon opens the editor. Enter inserts a line, Tab four spaces. Ctrl+A/C/X/V, Ctrl+Z/Y and Ctrl+Shift+Z work while editing. Ctrl+Backspace deletes the previous word and Ctrl+Delete the next word. Arrows, Shift+arrows, Home/End, Page Up/Down and Ctrl+Home/End navigate/select. Ctrl+S queues a save; Ctrl+Enter/Escape ends editing without discarding text. **Copy** copies the whole note.

The limit is 20,000 Unicode characters with 32 undo steps. Autosave starts 600 ms after typing pauses. Switching panels, minimizing or exiting queues the newest text; shutdown drains the writer. Saved status follows acknowledgement. A failed save retains the text and retries.

## Timer and water

Timer presets and +/− controls can be changed before starting; click the minute or second value to type directly. Enter or clicking away saves, Tab switches fields, and Escape cancels. Seconds above 59 carry into minutes; duration is limited to 1 second–24 hours. Pause/resume/reset are available. Deadlines include Windows sleep while the process survives. Exiting stops the timer. Its sound follows Windows volume and the Timer sound setting.

Water reminders start disabled. Configure intervals, active/quiet hours and snooze. Equal start/end hours mean all day. **I had some water** records a check-in, reschedules and plays three short gulps. Repeated presses restart the clip. Check-ins do not measure intake.

## Weather

Use **Change city**, type a name and choose a result. Forecasts refresh periodically and support manual refresh with a cooldown. Cached results work offline. Provider credits are available in the tray menu under About / credits.

## Startup and updates

**Settings → Appearance → Run at startup** registers only the current Windows user and copies the EXE to `%LOCALAPPDATA%\DynamicIslandForWindows\app\`. Turning it off removes this app's logon entry while leaving the current process running. Windows-disabled entries are managed in Windows Startup Apps. Windows may delay startup.

To update, quit the old instance first, then run the new build. A manually opened upgrade can refresh a stable startup copy. Automatic downloading is not implemented.

## Local files

`%LOCALAPPDATA%\DynamicIslandForWindows\` contains preferences (`settings.json`), the plaintext note (`quick-note.txt`), optional lyrics/forecast caches, a bounded error log and the optional startup `app/` copy. Data is separate from the executable.

## Remove

1. Turn off **Run at startup**.
2. Quit through the tray and wait for exit.
3. Delete the downloaded executable/extracted folder.
4. If startup was enabled, remove the data directory's `app` subfolder.
5. Keep the other files to retain Notes/settings. Delete the data directory only if you also want to erase them.

This portable beta does not add an entry to Windows Installed Apps.
