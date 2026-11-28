# v1.0.3 — desktop acceptance

Source hotfix candidate, not a prebuilt or desktop-verified release. Existing provider code is unchanged.

## Build
Run package.cmd using the Windows GNU Rust toolchain. This builds and tests Core, with no need for C++ compiler. The full ZIP is built by GitHub Actions.

## Existing Full installations
Exit v1.0.2 normally (Ctrl+Alt+F12). Keep the installed blur-provider folder and registered DLL untouched. Run dist/MacTraffic.exe from this checkout for acceptance; do not enable startup from the test directory. Preserve the existing MacTraffic.ini and stable browser Spacer. After acceptance exit the test instance, back up the installed EXE and replace it with the new one.

## Focused desktop tests (3–5 minutes)
- Paint: narrow, wide, maximize, restore. If the real title row is occupied or covered by the Finder bar, safe right/stock fallback is intentional.
- Alt+Tab repeatedly between Paint, Explorer, Brave, Word and Settings. Observe first-paint delay and side changes.
- Launch Settings from Desktop Display settings and Finder Bluetooth shortcut. Verify three controls, title, and click behavior.
- PowerShell: maximize, restore, resize and scroll output; check for stale lines. Native maximize is now used for classic console hosts; one deferred client invalidation runs after size changes.
- Office: open Word normally, exit and open a DOCX directly from Explorer; repeat for Excel XLSX. Check first paint, title changes, minimize, maximize and restore. If Windows reports a reserved top work area, the green button must not fall back to a native maximize that could enter that area. If Office rejects the requested work-area geometry, retain the normal window and log the reason. If MyDockFinder does not reserve work area, inspect the office_zoom_request log; no unverified hard-coded top margin is introduced.
- Confirm Explorer blur and browser pinned Spacer still work.

The Windows build, 107 unit-test definitions, and desktop behavior have not been executed in the authoring environment. Do not publish before Windows build and focused test pass.

For diagnostics, launch dist/MacTraffic.exe --debug-log after exiting the running instance. Log: %TEMP%\MacTraffic-v103-title-fix.log. Normal logging may filter detailed messages.

## Layout fix follow-up
- Office exact-left mode rejects intermediate positions, including the space before Search. A verified right fallback is intentional if the real Office controls occupy the left side. It is not possible to reflow all live Office title controls from this overlay without a separate architecture.
- Settings host recognition accepts the verified Settings child PID; if full centered-band validation fails, the verified stock button group may receive a right-side Traffic Lights cover after 400 ms. Unverified geometry still remains stock.
- Windows build and desktop acceptance not executed here.

## Title stability follow-up
- Office exact-left now screens a bounded narrow title stripe per row, including offsets beyond 300 DIP, before using the existing blank/drag/ownership and relocation validation. It does not move real Office controls. When no safe prefix or gap can be proven, retain the right cover; no intermediate position.
- Settings keeps a valid cached title during transient empty titles, updates the title without rebuilding the entire surface, avoids ambiguous periodic color samples, and briefly defers an unknown hit-test result (not an occupied hit).
- Windows build and live acceptance are still required. No provider changes.

## Browser profile picker follow-up
- Exact browser product title plus live nonclient client-origin validation distinguishes a native-caption chooser from a normal tab strip. For this path require a verified complete DWM visual cover and prefer the existing centered native band; if not provable keep stock. Ordinary browser Spacer is unchanged.
- Test Brave profile chooser before selecting a profile, then enter a normal browser window with the existing pinned tabs. Check no duplicate stock minimize and no covered profile controls. Edge profile chooser can be checked similarly when its native title matches.
- 109 unit-test definitions; Windows build and desktop test remain required.
