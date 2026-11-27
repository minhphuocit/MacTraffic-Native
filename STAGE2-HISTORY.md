# MacTraffic v0.8.6 — Stage 2: Event-driven Engine & Stability

Baseline: v0.8.5 Stage1 Final (user reported visual PASS). This is the second of the three agreed development stages. No layout/renderer redesign, additional dependencies, elevation, injection or registry changes. The same Spacer.html and pinned tabs are required.

## Stage 2 final caption polish
- Native Mac title geometry now accepts maximized frames with a visible title row. The reserved-work-area guard and explicit safe-zoom fallback remain intact. The visible work area determines the center and left inset. Paint/WordPad prefer the previously proven 8-DIP inset, with a verified alternate if necessary.
- Known generic centered-caption profiles (Settings, SimpMusic, Regedit, MMC, Task Manager) no longer display right-side or post-title custom dots when centered validation fails. Keep stock until verification succeeds; retry after geometry/focus changes and at a bounded cadence. Clip only small invisible caption-envelope edges, retaining verified role centers; compact title rows need no extra top inset.
- Settings: sample tiny, visible, owner-verified flat client-body patches under the title. Require a majority consistent color, not the Windows accent strip. Render the verified title band and stock-button mask contiguously. Light/dark colors are sampled, not hard-coded. No changes to Settings configuration. If the color or drag band cannot be verified, leave stock; do not claim universal coverage of every Settings page.
- Browser Spacer source unchanged. Office Ribbon and browser tab controls retain their existing verified fallbacks; title placement cannot be forced over live controls. Ghost-caption visibility guards and adaptive scheduler preserved.
- Six additional unit tests (89 total expected). Source diff, ZIP integrity and independent geometry checks only in the authoring environment. Rust/Windows execution not available here.
- Acceptance: maximize/restore Settings, SimpMusic, Services, Task Manager, Regedit, Explorer, Notepad/PowerShell; check browser and Office regressions; Settings light/dark if used; minimize and Win+D; graceful exit. Log `settings_body_color`, `generic_centered_verified`, `generic_centered_wait`, `layout_side`, `owner_visibility_suspend`. One 3-minute resource check is enough for short-term acceptance; not a proof of absence of long-term leaks.

## Visibility hotfix
- Explicitly hide and reset all owned surfaces when the owner is minimized, hidden or DWM-cloaked; do not rely on automatic popup hiding. Visibility events bypass the background rate limit. On minimize via Traffic Lights, hide/reset before the asynchronous minimize request and briefly defer redraw. Two additional visibility-policy tests.
- This addresses a possible orphan-caption mechanism observed with PowerShell; the exact user-side transition was not captured. Verify that the PowerShell body and caption recover on restore, and that normal exit restores stock.

## Changes
- Six out-of-context WinEvent hook ranges (foreground, move/resize, minimize, window destroy/show/hide/reorder, state/location/name, cloak/uncloak). Coalesced manager wake, filtered to foreground or known owner windows; no UIA or rendering inside callbacks. The previous two UIA worker threads still signal completed probes.
- Adaptive watchdog: 40 ms while settling, moving, scanning or retrying; 100 ms for steady supported foreground, 250 ms for unrelated foreground with existing sessions, 500 ms with no sessions. A failed hook uses the old 40/160 ms watchdog and 160 ms background audit. Normal background audit 750 ms with event-triggered, rate-limited passes (80 ms floor for ordinary event bursts); focus and topology changes bypass that floor. This reduces timer wakeups, not a guarantee of reduced measured CPU or battery use.
- Mouse-entry/leave wakeups and existing hit-role verification. Keep fast settling ticks; do not change renderer or verification algorithms.
- Monitor identity, monitor bounds, work area and DPI are tracked independently of the visible frame. On change, discard stale surfaces and restore stock before re-verification. The foreground-only verification rule remains. No forced positioning onto a disconnected monitor when restoring work-area zoom.
- Bounded native retries on transient geometry errors; permission/style failures do not gain new retry permissions. Caption session is retained while a close request is outstanding (including Office/native close confirmations). A newer destroy event retires the session and disarms restoration against the old HWND. HWND lifetime identity checks remain as fallback.
- On graceful exit, unhook WinEvents, restore caption styles/theme via existing RAII guards, destroy owned surfaces, restore valid on-screen work-zoom records, and disconnect worker channels. Windows logoff/shutdown notifications request best-effort graceful cleanup; abrupt process termination is not recoverable by this mechanism. A cancelled system shutdown may leave MacTraffic stopped; restart manually.
- `engine_stats` every 60 seconds and on clean exit: cumulative timer ticks, event wakes, background passes and current interval. These are diagnostic counters, not direct CPU or power measures.

## Build and 3-minute acceptance
Exit previous version via Ctrl+Alt+F12. Extract to a separate folder; run:

```powershell
.\build.cmd
if ($LASTEXITCODE -eq 0) {
    cargo test --locked --target x86_64-pc-windows-gnu
    if ($LASTEXITCODE -eq 0) {
        .\dist\MacTrafficCaptionLab.exe --mac-confirmed --alt-keys --office-exact-left --browser-spacer
    }
}
```

Log: `%TEMP%\MacTrafficCaption-v0806-stage2-final.log`. Check `event_engine_ready`, `engine_stats`, `session_environment_changed`, `native_geometry_retry`, `target_error`, `panic`, `engine_exit_cleanup`.

Test browser close/cancel, native unsaved-document close/cancel, Settings/Services/Task Manager/SimpMusic, Explorer blur and Office; open/close, minimize/restore, maximize/restore, move between attached monitors (if available), verify no stale or floating overlay. Measure working set/private memory/handles/threads for three minutes with normal interaction. For CPU comparison hold the same foreground and window set as the baseline, because mixed interactive workloads cannot establish a fair improvement. Exit with Ctrl+Alt+F12 and verify native buttons return, then relaunch as needed. If only one monitor is available, mark cross-monitor/DPI test unverified.

## Validation limits
Source-level diff and lexical delimiter checks, byte-for-byte preservation of Browser Spacer and Spacer.html, ZIP integrity. Five added unit tests cover scheduler intervals, native retry policy, event timestamp wrap, and environment changes. The environment has no Rust compiler or Windows runtime; build, unit tests and visual/performance acceptance are not executed here. No claim of zero delay, zero leaks, power savings or final release readiness. Stage 3 (configuration, autostart, portable release and GitHub) remains separate.

---

## Inherited notes

# MacTraffic v0.8.5-stage1-final — Settings & Top Edge Polish

Current build: `build.cmd`; `cargo test --locked --target x86_64-pc-windows-gnu` (76 test definitions). Start `dist\MacTrafficCaptionLab.exe --mac-confirmed --alt-keys --office-exact-left --browser-spacer --trace-focus`. Current log: `%TEMP%\MacTrafficCaption-v0805-stage1-final.log`. The Stage1 notes below are historical. See final section for changes and limitations.

## Previous Stage1 — Centered Caption & Startup Fix

Round 1 acceptance candidate. Hotfix 2 selected a blank area at offsets 80–112 DIP; this preserved the original title but did not reproduce Explorer's centered title. The new path uses 8 DIP layout padding (same as Explorer), a centered title, and a click-through title band for the existing allowlisted SunAwtFrame (SimpMusic), RegEdit_RegEdit, MMCMainFrame and TaskManagerWindow classes. Settings retains its prior verified path. The bar stays outside the client area and requires a verified DWM button group and caption hit checks. If unavailable, preserve safe fallback; recheck up to eight times per activation or geometry. No elevation, injection, registry or target title/style changes for these apps.

Startup changes: bounded generic admission retries, show event wake, repeated fast DWM verification every 100 ms while waiting, shorter negative UIA cache and generic render retry. The centered path bypasses the incremental blank-slot scan when verified. `generic_session_first_present` includes time since session allocation, whereas `custom_first_present` includes only the current layout attempt. Neither measures time before Windows first exposes an eligible foreground HWND. No guarantee of zero latency or identical results for privileged/custom frames. Existing browser spacer module unchanged. Explorer/native renderer, close-confirmation and UIA lifetime policies preserved. No extra dependencies.

Build: `build.cmd`; `cargo test --locked --target x86_64-pc-windows-gnu` (72 test definitions; actual Windows test result pending). Start `dist\MacTrafficCaptionLab.exe --mac-confirmed --alt-keys --office-exact-left --browser-spacer --trace-focus`. Log: `%TEMP%\MacTrafficCaption-v0805-stage1.log`. Check `generic_centered_verified`, `generic_centered_fallback`, `generic_session_first_present`, `custom_first_present`, `target_error`, and `panic`. Reopen Services, Task Manager, Regedit and SimpMusic; check left margin, title alignment, controls and no stock glyphs. Check browser close/cancel, Settings, Explorer blur. A short 3-minute resource run is sufficient for initial acceptance. Windows compiler/runtime unavailable here; geometry checked separately and ZIP validated, actual Windows acceptance pending.

---

# MacTraffic v0.8.5-hotfix.2 — Verified Row & First Paint (round 1)

Hotfix 1 Windows logs show the generic hit row at 103 x 22 DIP while DWM's full visual envelope is 146 x 30 DIP, e.g. role row (1463,200)-(1566,222) vs visual cover (1421,192)-(1567,222). Hotfix 1 used the expanded envelope for the left search, shifting the search 4 DIP upward. This is a plausible cause of the reported right fallback; it is not yet confirmed by the new build.

- Keep the actual hit row separate from the full stock mask. Preserve the full visual cover and verify left slots against the correct row; small alternate vertical offsets must stay inside that cover's title band.
- For the allowlisted generic apps, try the bounded compact/full DWM geometry check on the first eligible manager tick (three live role checks, 5 ms per call, 18 ms scheduling budget). No unconditional DWM-only cover. Fall back to the existing asynchronous verifier when proof is unavailable.
- Do not publish a provisional right layout while the bounded generic left search is unfinished. Wake the manager to continue it promptly. Preserve verified right fallback only when the search actually fails.
- Remove the fixed 120 ms custom geometry/title reattach delays; retry transient generic color-sampling failures sooner. Scan generic title slots even when the title is initially empty and revalidate when it arrives.
- Preserve Browser Spacer source and marker, Explorer/renderer, COM lifetime, modal close policy, existing supported app list and hotkeys. No new dependencies, DLL injection, registry or startup changes.
- Three new geometry tests cover the exact observed row, cover exclusion boundary and DPI scaling. Expected 69 tests total. Windows compile/runtime not verified in this environment; no guarantee of literal zero-latency or left placement on an occupied title bar.

## Quick Windows test
Exit prior MacTraffic with Ctrl+Alt+F12. Extract this archive into its own folder. Run build.cmd and cargo test --locked --target x86_64-pc-windows-gnu. Launch dist\MacTrafficCaptionLab.exe --mac-confirmed --alt-keys --office-exact-left --browser-spacer --trace-focus. Open/reopen SimpMusic, Services, Task Manager and Settings. Check left placement, no remaining stock controls, close-confirmation Cancel in Brave/Edge, Explorer blur and Office. Capture %TEMP%\MacTrafficCaption-v0805-hotfix2.log (generic_fast_hint, generic_left_fast, custom_first_present, custom_left_probe, custom_left_revert, target_error, panic). One 3-minute resource check is sufficient for short-term round 1 acceptance. If a genuine left slot is not verifiable, keep safe fallback instead of hiding app controls. Round 2 and round 3 are unchanged.

---

# MacTraffic v0.8.5-hotfix.1 — First Paint & Full Caption Cover

Correction within development round 1. Baseline v0.8.5: 63/63 user tests passed; delayed and partially covered captions reported on Windows.

- Foreground WinEvent (out-of-context) and completed worker replies wake the manager, with coalescing and existing timer fallback.
- Remove initial 120 ms settle; retain geometry settle on established browser/Office sessions. Retry delayed Settings host discovery with bounded backoff.
- Generic caption: fast DWM envelope path with three independently verified hit roles, reconcile narrower verified bounds with matching full visual envelope, reject contradictory geometry.
- Batch left-slot search, prioritize short-title gaps, avoid provisional right overlay during the initial bounded verification phase (180 ms for ribbon scan, 250 ms for browser spacer). These thresholds are not hard latency guarantees.
- One bounded generic left-slot rescan; log custom_first_present, generic_fast_hint, generic_cover_expanded, generic_cover_conflict, focus_wake.
- Preserve Explorer blur, native renderer, persistent UIA, browser spacer geometry and browser close handling. No new dependency, registry or startup changes, or foreign code injection.

Build using build.cmd and cargo test --locked --target x86_64-pc-windows-gnu. Expected 66 tests if compiled. Run dist\MacTrafficCaptionLab.exe --mac-confirmed --alt-keys --office-exact-left --browser-spacer --trace-focus. Test first open/reopen of SimpMusic, Settings, Task Manager, Services, browser close confirmation and existing native/Office apps. One three-minute resource test; log %TEMP%\MacTrafficCaption-v0805-hotfix.log.

No Windows runtime or Rust compiler available here. Build and runtime acceptance remain unverified. Zero latency is not guaranteed before Windows exposes valid geometry. Rounds 2 and 3 remain unchanged.

---

# MacTraffic v0.8.5 — Compatibility & Close Confirmation (development round 1 of 3)

Baseline: v0.8.4 short resource test passed. This candidate is deliberately limited to close handling and a verified supplemental app group. No event-loop rewrite, blur changes, startup registration, or new dependencies in this round.

## Changes
- A browser/custom supplemental close click posts WM_CLOSE without clearing the verified overlay or permanently blocking the HWND. A 750 ms close handoff keeps unchanged surfaces; disabled owners and active owned popups reject caption input. The ordinary geometry, spacer freshness, and ownership safeguards resume thereafter. Destroyed windows are removed by existing session maintenance. A modal that overlaps or changes the verified caption can still cause stock fallback; no forced overlay over a dialog.
- Add a read-only GenericProbe route for SimpMusic (explicit executable names or exact Java/AWT title), Windows Settings (SystemSettings.exe, or an ApplicationFrameHost window with a verified Settings core child), Task Manager, Registry Editor, MMC (Computer Management / Services). Control Panel using CabinetWClass retains the original Explorer route. Unknown apps and owned dialogs stay excluded; no blanket ApplicationFrameHost or Java matching. Elevated windows may be inaccessible. Generic apps try existing native/DWM/hit-test or named UIA verification, sample only verified caption background, and attempt the existing safe near-left slot search without Office title shifting. If left is occupied, keep verified right placement; if a group cannot be verified, leave stock. Actual class/executable and geometry must be checked on the target machine.
- Two policy unit tests added. No changes to Explorer blur/title engine, native layout, Browser Spacer marker/geometry or COM worker lifetime, Office exact-left mode, timer periods, or hotkeys. The original shared mutex remains for backwards compatibility. Log: `%TEMP%\MacTrafficCaption-v0805.log`.

## Windows acceptance (one 3-minute test)
Exit v0.8.4 using Ctrl+Alt+F12. From the extracted folder run `build.cmd`, then `cargo test --locked --target x86_64-pc-windows-gnu`, then `dist\MacTrafficCaptionLab.exe --mac-confirmed --alt-keys --office-exact-left --browser-spacer --trace-focus`. Keep original pinned spacer files and browser tabs. In Brave/Edge press red, wait for the close-all-tabs dialog, cancel, verify caption stays or recovers, then repeat. Test SimpMusic, Settings and Task Manager; check Explorer/Office for regressions, collect 3 minutes of resource samples while using the apps, and close via Ctrl+Alt+F12 to verify stock restoration. `--trace-focus` is for this diagnostic run, not final defaults. No registry, system startup or browser profile changes are performed.

Source changes and archive checked on a non-Windows environment. No Rust compiler or Windows runtime available here. Unit tests and runtime verification are pending; this is a candidate, not a release. If the exact SimpMusic executable/class differs, send the app_detect line from trace focus. Do not bypass integrity-level restrictions for elevated stock applications.

## Remaining rounds (agreed scope)
2. v0.8.6 — event-driven engine and stability/DPI/window lifecycle, with regression fixes as needed.
3. v1.0.0 — configuration, optional user-controlled auto startup, portable release, final short acceptance and GitHub publication (requires repository access/authorization).

---

# MacTraffic v0.8.4 — Low-allocation Memory & CPU candidate

Based on the supplied v0.8.3 source. The short Windows idle sample showed 12.44–12.77 MiB working set, 2.34–2.54 MiB private memory, 219–236 handles, 4–10 threads; both UIA workers were seen initialized after reboot. This is a short-test baseline, not a proven long-term leak fix.

## Changes
- Custom caption ticks and click validation use borrowed UTF-16 window titles from a stack buffer, retaining the original title length guards. The persistent title cache is updated only when text changes and reuses capacity.
- Spacer marker checks work directly on UTF-16 without allocating a UTF-8 string. Exact marker and Unicode alphanumeric boundary behavior are preserved; new equivalence tests include supplementary characters and malformed UTF-16.
- The bounded tab collection is preallocated from the UIA result count and sorted/verified in place rather than cloning the vector for each probe. Rejection diagnostics show sorted tabs; the geometry and validation rules are unchanged.
- Windows unit test builds use the console subsystem so cargo test results can be displayed. Release stays a GUI application. Added two regression tests. The original v0.8.3 mutex name is deliberately retained so these two versions cannot run simultaneously.
- No changes to layout, rendering, worker lifetimes, timings, per-window sessions, Spacer.html, title guards, hotkeys, fallback policies or Win32 style restoration. No new dependencies, injection, registry, memory trimming or forced process termination.

## Build and short acceptance (3 minutes)
Exit the previous version with Ctrl+Alt+F12; keep the original pinned Spacer tabs.

```powershell
.\build.cmd
cargo test --locked --target x86_64-pc-windows-gnu
.\dist\MacTrafficCaptionLab.exe --mac-confirmed --alt-keys --office-exact-left --browser-spacer
```

Open Brave (3 spacers), Edge (2 spacers), Explorer, Word. Switch focus, test the buttons and keep MyDockFinder/ExplorerBlurMica active. Collect 19 samples with 10-second spacing, compare working set, private memory, handles and threads with baseline and check both UIA workers and verified spacer geometry. Exit normally and verify stock controls. Log: `%TEMP%\MacTrafficCaption-v0804.log`.

The target working set below 10 MiB is aspirational, not a measured result. OS/provider and DWM allocations can dominate; compare identical workloads. The three-minute acceptance cannot establish long-term leak freedom. Source and archive reviewed in a non-Windows environment; Rust compiler and Windows runtime unavailable here, so build and unit tests remain unexecuted.

---

# MacTraffic v0.8.3 — UIA lifetime fix

Based on v0.8.2. Runtime sample shows monotonically increasing threads, handles and private memory. The observed resource trend is real; the exact OS/provider allocation source is not yet proven. This release addresses repeated COM apartment and IUIAutomation client creation in both worker loops. It does not claim a confirmed runtime leak fix until a long-duration Windows test passes.

- Each of the two existing worker threads lazily creates and reuses its own MTA/UIA context. A single shared DLL lookup is used. Keeping workers separate prevents a slow spacer provider from blocking Office caption discovery.
- COM interface wrappers release the client before CoUninitialize, and per-query interfaces remain scoped to the query. Failed context initialization is throttled to 15 seconds. Worker channels are disconnected after restoring windows; no unbounded shutdown wait on UIA calls.
- Logs `uia_context_created` once per successful worker context and `uia_context_reused` every 20 requests. Normal run should show at most one creation per worker. These logs diagnose client lifetime, not provider-internal allocations.
- No changes to polling intervals, verification rules, overlay geometry, persistent sessions, Z-order, Spacer.html, native work-area zoom, Office, or Explorer/Paint fixes. No new dependencies, registry, injection, memory trimming or process termination.

Exit v0.8.2 using Ctrl+Alt+F12 before running. Keep original v0.7.2 Spacer.html where it is.

```powershell
.\build.cmd
if ($LASTEXITCODE -eq 0) {
    .\dist\MacTrafficCaptionLab.exe --mac-confirmed --alt-keys --office-exact-left --browser-spacer
}
```

Warm up the same four windows (Brave, Edge, Explorer, Word) and keep their count fixed. Observe 5 minutes with 30-second samples. Compare private memory, handle and thread trends rather than demanding a specific first reading. Switch tabs, focus, zoom and test restoration of original captions before calling the fix stable. Log: `%TEMP%\MacTrafficCaption-v0803.log`. If counts still grow, investigate provider/UIA calls with a separate isolated probe rather than disabling verification or hiding working set. Windows build and tests are not available here. Source-level diff and archive checks only.

## Inherited v0.8.2 notes
# MacTraffic v0.8.2 — Explorer & Paint regression fix

Based on v0.8.1 with its performance settings intact. Explorer is supported but deliberately excluded from App::native(); use App::supported(false) to allocate its required centered-title surface. Allocation failure is logged. Paint/WordPad try the requested 13-DIP margin, and can use the old 8-DIP geometry if the new slot fails verification; do not bypass title or hit-test checks. During a full ribbon recheck, hide the previous own popups so they do not contaminate the local ownership/color test. The verified margin remains selected until geometry changes. Existing near-left and stock/right fallbacks remain if neither candidate can be validated. Detach hides the button popup even when there is no Target, e.g. a failed attach.

All browser spacer and Office source is unchanged from v0.8.1. Native 8-DIP margin, persistent per-window sessions, 160-ms background pass, UIA caches, and Z-order audit retained. Explorer needs one additional popup compared with broken v0.8.1; memory impact is unmeasured. No new dependencies, registry changes or DLL injection.

Exit previous version via Ctrl+Alt+F12. Keep the original v0.7.2 Spacer.html at its current location.

```powershell
.\build.cmd
if ($LASTEXITCODE -eq 0) {
    .\dist\MacTrafficCaptionLab.exe --mac-confirmed --alt-keys --office-exact-left --browser-spacer
}
```

Test Explorer and Paint normal/maximize/restore, focus away and back, and overlap with another window. Test other apps for regressions. Exit via Ctrl+Alt+F12. Full log `%TEMP%\MacTrafficCaption-v0802.log`.

Source-level inspection and archive verification only. Unit tests added but not executed; Windows build and runtime tests not available in authoring environment. Optional local tests: `cargo test --locked --target x86_64-pc-windows-gnu`.


## v0.8.5 Stage1 Final (targeted polish)
- Settings is now eligible for the centered GenericProbe title. Extended client layouts must pass bounded (40 DIP) drag-role and ownership checks; unknown/client controls retain fallback.
- Centered generic title and stock-button cover use the same flat interior color, with a transparent strip preserving the native top edge. No change to Browser Spacer or Explorer blur.
- Log: `%TEMP%\MacTrafficCaption-v0805-stage1-final.log`. Inspect `generic_settings_title_verified`, `generic_centered_verified`, `generic_centered_fallback`, `custom_first_present`, `target_error`.
- Windows build and visual acceptance are required. Expected 76 unit tests if baseline 72 is correct. The Settings extended title may still fall back if Windows does not return verified drag roles.
