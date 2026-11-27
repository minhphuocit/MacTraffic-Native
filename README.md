# MacTraffic v1.0.1 — Final release source

Native Rust, Windows 10 x64, GNU toolchain. No third-party Rust crates, DLL injection, registry writes, installer, service or automatic elevation. Tested baseline: v0.8.6 Stage2 Final, 89/89 tests and a 3-minute resource check by the user. **v1.0.0 passed the user’s short resource check, but intermittent stock captions were reported. v1.0.1 is a recovery hotfix candidate, not yet compiled or tested on Windows.**

## Build and run

Exit any previous MacTraffic with Ctrl+Alt+F12. Extract the source into its own folder. From that folder, run `package.cmd` (requires the same Rust GNU toolchain as Stage2). It builds, runs the tests, creates `dist/MacTraffic.exe`, and packages the portable ZIP plus SHA256 in `release/`. The five new tests cover defaults, exclusions, invalid INI, logging policy and notification-icon ABI (97 total expected with the three recovery tests). Unit tests do not replace live desktop testing.

Double-click `dist/MacTraffic.exe`; no arguments or PowerShell needed. Default settings are equivalent to the accepted Stage2 startup flags. The program does not install itself. The pinned Brave/Edge Spacer tabs are retained; keep their existing HTML file path. Do not move/delete the old Spacer file while tabs refer to it.

## Controls

- Tray icon (standard Windows application icon): click/right-click to see menu. Windows may place it under hidden icons.
- Pause / Resume: Ctrl+Alt+P or tray menu. Pause requests normal caption/style cleanup, restores eligible work-zoom geometry, stops session inspection and uses a slow tray retry timer. Resume re-verifies the foreground owner. Focus the desired app after resuming.
- Exit and restore: Ctrl+Alt+F12 or tray menu. Normal cleanup only; abrupt termination cannot be guaranteed to restore stock styles.
- Existing layout shortcuts: Ctrl+Alt+F10 / F11, optionally L / R, remain available. If a shortcut is taken, use the tray menu; F12 remains required for safe startup.
- Enable/disable start with Windows: explicit tray action creates/removes only the current user's `MacTraffic-v1.lnk` in the Startup folder. Default is off; no registry changes or elevated task. If moving the portable folder, disable startup first and re-enable after moving. No autostart is created at build time. Windows can still disable Startup entries via system policy.
- Edit configuration opens `MacTraffic.ini` beside the running EXE. Changes apply on exit/reopen. Invalid keys/values stop startup before caption modifications. The source INI is the default template; build does not overwrite existing dist INI.
- Open log opens the current log in Notepad. Normal logging filters repetitive visual probes and rotates at 1 MiB, retaining one previous log. `log_level=debug` or `--debug-log` for detailed diagnostics; `off` disables normal file logging. `--trace-focus` also enables detailed output. No log upload. Log location `%TEMP%\MacTraffic-v101.log`.

## Configuration example

```ini
[MacTraffic]
start_paused=false
browser_spacer=true
office_exact_left=true
alt_keys=true
native_maximize=false
browser_enabled=true
office_enabled=true
exclude=mmc.exe,taskmgr.exe
log_level=normal
```

`exclude` matches exact executable basenames, case insensitive. `mmc.exe` excludes Services and Computer Management together. `systemsettings.exe` excludes the verified Settings host; `simpmusic.exe` also covers the verified Java SimpMusic window. Console exclusions use the detected owner executable (often conhost.exe). Unknown/unsupported apps remain excluded by the existing allowlist. A disabled browser or Office setting leaves stock captions. `native_maximize=true` chooses native maximize rather than work-area zoom. Old diagnostic CLI flags are retained for rollback/troubleshooting; they can override configuration when explicitly passed.

## Acceptance (3 minutes, plus quick visual check)

1. Build and confirm 97/97 tests, then run EXE directly. Check the tray icon and default left caption layout, Settings background, Explorer blur and MyDockFinder. Verify Brave/Edge pinned tabs, Office and supported stock apps. Maximize/restore/minimize and Win+D, no ghost captions. Verify multi-monitor if available.
2. Ctrl+Alt+P: all managed captions should return to stock. Ctrl+Alt+P again, focus an app and verify the caption. Check tray exit and log `engine_exit_cleanup`.
3. Set `exclude=mmc.exe`, restart, check Services stock and Explorer still managed; remove exclusion and restart. Enable startup from tray and confirm shortcut exists (`shell:startup` in Explorer); disable to remove, or enable if desired and reboot once to verify. Use a permanent writable portable folder first. Do not kill the process to test exit.
4. Monitor RAM, Private Memory, handles, threads and cumulative CPU for 3 minutes with normal use. Inspect log for errors. Baseline roughly 13 MB working set, 2.3 MB private, 230 handles/5 threads at rest; tray integration may add small overhead. No claim of zero long-term leak from a short test.

## GitHub / distribution

The source ZIP is the backup. `.github/workflows/windows.yml` builds and tests with Rust GNU on Windows, and creates a draft GitHub Release only on tag `v1.0.1` after CI succeeds. Review the draft and publish manually only after live acceptance. Do not upload local config, log, target, dist or private credentials. Review the existing project's license and ownership before choosing a public repository; this package does not invent a license.

For a new empty repository, after successful desktop test, initialize Git, commit the source, add the actual repository URL as `origin`, push the branch and then tag `v1.0.1`. For an existing repository, integrate the changes on a branch and review the diff; never force-push. No repository has been created or pushed from this environment.

## Engine scope and history

Browser Spacer geometry and visual rendering policy are unchanged. The v1.0.1 hotfix changes foreground recovery, transient native geometry retry scheduling, negative probe cache TTL and auxiliary overlay visibility checks. The final integration changes manager-level startup, configuration filtering, tray commands, pause/exit cleanup and logging only. Verification and safe stock fallback remain; retries for transient native geometry failures continue at a capped cadence only for the foreground window; app-dependent UIA delays, Windows privilege boundaries, custom title bar differences and abrupt shutdown limitations remain. In the source package, see `STAGE2-HISTORY.md` for earlier engine development. Experimental CLI flags are retained deliberately to avoid removing proven recovery/diagnostic paths; release defaults require none.

## v1.0.1 recovery check

After closing v1.0.0, build with `package.cmd` and launch the new portable EXE. Test repeated minimize/restore, Win+D, and opening the affected app without Alt+Tab. Check `caption_visibility_recover`, `caption_retry_recovered`, `caption_auxiliary_recover`, `native_geometry_retry` and errors in `%TEMP%\MacTraffic-v101.log`. Stock fallback remains intentional for unsupported, obscured, elevated or unverifiable windows. The engine does not sample occluded background windows. If Startup was enabled in an earlier folder, disable it there or disable/re-enable it from the new tray menu after moving to a permanent folder. Keep previous Spacer HTML files as long as pinned tabs refer to them.
