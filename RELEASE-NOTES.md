# MacTraffic v1.0.3 — Caption Stability Hotfix

Release candidate until Windows desktop acceptance.

- Foreground-first rendering and immediate refresh of verified sessions.
- Preserve Ribbon theme across resizing; verify edge slots and avoid scanning distant title positions.
- Use elapsed-time cover recovery to avoid premature resets during event bursts.
- Retain verified custom caption surfaces on temporary color sampling failures; update generic title text in place.
- Admit verified owned Settings frames, including supported deep-link launches.
- Classic console native maximize and one deferred client invalidation after resize.
- Preserve the optional Explorer blur provider and stable browser Spacer.
- Include Setup-Blur.ps1 in the Full ZIP.
- Fix E0133 in the Settings admission path.
- Office document launch: native role verification fallback, strict same-process owned Office frame admission, transient-title handling and bounded left-slot revalidation.
- Prevent native maximize fallback for Office when the top work area is reserved and work-area zoom is enabled. Native restore remains available.

## Installation

Existing v1.0.2 Full users may replace only MacTraffic.exe after normal exit and preserve the registered provider, configuration and Spacer. New users can extract Full, run Setup-Blur.ps1 if blur is desired, restart Windows and open MacTraffic.exe. See README for disable/uninstall instructions.

Windows build, unit tests and actual desktop checks must pass before publishing. Safe fallback remains intentional where title-bar geometry cannot be verified.
