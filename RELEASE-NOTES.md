## MacTraffic v1.0.2 — Full & Core

Native Rust window controls with an optional bundled Explorer Acrylic blur companion.

### Changes
- Improved Explorer caption background handling and focus recovery.
- Stable per-user browser Spacer file for Brave and Edge.
- Optional bundled ExplorerBlurMica companion in the Full package.
- Portable Core and Full downloads with SHA-256 checksums.

### Downloads
- **Full**: `MacTraffic-v1.0.2-full-win-x64.zip` — includes optional Explorer blur.
- **Core**: `MacTraffic-v1.0.2-win-x64.zip` — caption manager only.

### Quick start (Windows 10 x64)
Extract into a permanent folder. Exit older MacTraffic versions with Ctrl+Alt+F12, then run `MacTraffic.exe`. No compiler or Rust installation is needed.

**For Explorer blur (Full only):** exit MacTraffic, open 64-bit Windows PowerShell as Administrator in the extracted `blur-provider` folder and run:

```powershell
.\Manage-Blur.ps1 -Action Enable
```

Confirm with `YES` and restart Windows. This is a one-time setup requiring explicit COM registration; do not move or delete the registered provider folder. To remove blur, use `-Action Disable` from the same folder, then restart Windows before deleting it. If you already have another ExplorerBlurMica installation, uninstall it using its own uninstaller first.

**Brave / Edge:** open the stable Spacer folder from the tray. Pin three Spacer tabs in Brave or two in Edge at the far left; close any obsolete Spacer tabs first.

**Controls:** Ctrl+Alt+P to pause/resume; Ctrl+Alt+F12 to exit and restore. Startup is optional through the tray.

See the [full installation and removal guide](https://github.com/minhphuocit/MacTraffic-Native#download-and-install). CI passed; desktop behavior can vary by Windows build and application. The optional blur companion is derived from Maplespe/ExplorerBlurMica (LGPLv3 notices retained).
