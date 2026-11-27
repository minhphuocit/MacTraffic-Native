# MacTraffic

macOS-inspired traffic-light window controls for Windows 10 x64, powered by a lightweight native Rust caption manager. An optional bundled ExplorerBlurMica companion adds Acrylic blur to File Explorer.

[Download MacTraffic v1.0.2](https://github.com/minhphuocit/MacTraffic-Native/releases/tag/v1.0.2)

## Features

- Red, yellow and green window controls; centered titles where supported.
- Left-side controls for supported applications and verified browser layouts.
- Pause/resume, normal exit and restore, and optional startup through the system tray.
- Stable browser Spacer file for Brave and Edge.
- Optional Explorer Acrylic blur in the Full package.
- Stock Windows controls remain available for unsupported or unverifiable windows.

## Download and install

Choose a ZIP from the release assets, not the GitHub-generated source archive.

| Package | Contents |
| --- | --- |
| `MacTraffic-v1.0.2-full-win-x64.zip` | Rust app and optional Explorer blur companion |
| `MacTraffic-v1.0.2-win-x64.zip` | Rust app only; no whole-window Explorer blur |
| `SHA256SUMS.txt` | SHA-256 checksums for the downloads |

Requirements: Windows 10 x64. Extract the ZIP into a permanent, writable folder. No Rust, Cargo or compiler is required for the prebuilt packages. Verify the archive checksum if desired using Windows tools.

Exit any previous MacTraffic instance normally with **Ctrl+Alt+F12**. For the core package, simply double-click `MacTraffic.exe`.

### Enable Explorer blur (Full package only)

Blur is optional. The companion is a separate C++ COM shell extension and is not automatically installed by the EXE. Registering it requires administrator permission and changes COM registration. If another ExplorerBlurMica installation exists, use its own uninstaller first; do not delete its files while it is registered.

1. Make sure MacTraffic is closed.
2. Open the extracted `blur-provider` folder in File Explorer. Click the address bar and copy its full path.
3. Open **Windows PowerShell (64-bit) as Administrator** from the Start menu.
4. Run `Set-Location -LiteralPath "PASTE YOUR BLUR-PROVIDER FOLDER PATH HERE"`, replacing the quoted placeholder with the path copied in step 2.
5. Run:

   ```powershell
   .\Manage-Blur.ps1 -Action Enable
   ```

6. Read the confirmation and type `YES` if you agree. Save your work and restart Windows, then check Explorer blur and launch `MacTraffic.exe`.

This registration is a one-time setup per installation path. Do not delete or move the registered provider folder. If script execution is blocked by your system policy, consult your administrator rather than bypassing that policy. The tray's **Explorer blur setup (optional provider)** entry opens the instructions; it does not install the provider in v1.0.2.

### Brave and Microsoft Edge

Start MacTraffic and open its tray menu (check the hidden-icons area if needed). Select **Open stable browser Spacer folder**. Open the `Spacer.html` file from that folder in three tabs in Brave or two tabs in Edge, pin them, and place them at the far left. Select a regular tab. Close stale Spacer tabs from older installations first. The stable file is located at `%LOCALAPPDATA%\MacTraffic\Spacer.html`; retain it across upgrades. If the layout cannot be verified, MacTraffic intentionally falls back to stock or right-side controls.

## Controls

| Action | Shortcut / tray option |
| --- | --- |
| Pause / Resume | Ctrl+Alt+P |
| Exit and restore | Ctrl+Alt+F12 |
| Change layout | Ctrl+Alt+F10 / F11 |
| Start with Windows | Enable / Disable start with Windows |
| Settings | Edit configuration (restart to apply) |
| Diagnostics | Open log |

Startup is off by default. Enable it only after extracting into a permanent folder. The option creates a current-user Startup shortcut, not a service. Disable it before moving the app.

## Configuration and troubleshooting

Edit `MacTraffic.ini` beside the executable using the tray menu, then exit and reopen the app. The default `explorer_caption=auto` detects the bundled provider; `native` preserves the existing Explorer background, while `solid_dark` and `solid_light` are solid caption fallbacks, not blur. Unsupported or elevated windows may retain stock controls. Use `exclude` to skip specific executable names.

- No blur: confirm that you downloaded the Full package, completed registration and restarted Windows. Check Explorer with MacTraffic paused to isolate provider problems.
- Browser buttons on the right: check that the correct number of stable Spacer tabs is pinned at the far left.
- Unexpected caption behavior: pause and resume, or exit normally. For troubleshooting, open the log in the tray or inspect `%TEMP%\MacTraffic-v102-final.log`.

See [CLEAN-INSTALL.md](CLEAN-INSTALL.md) and [EXPLORER-BLUR.md](EXPLORER-BLUR.md) for additional details.

## Uninstall / move to another folder

Disable **Start with Windows** from the tray and exit normally. If the optional blur provider was registered, open administrator 64-bit Windows PowerShell in the same `blur-provider` folder and run:

```powershell
.\Manage-Blur.ps1 -Action Disable
```

Confirm with `YES` when prompted and restart Windows before deleting or moving the provider folder. Close pinned Spacer tabs before removing the stable Spacer file. For a core-only installation, simply remove the portable app folder after exiting and disabling startup.

## Build from source

Developers can use the Rust GNU x64 toolchain and `package.cmd` for the core. The Full package is built using `package-full.ps1` with MSBuild and the Rust GNU toolchain, as configured in GitHub Actions. The core has 100 unit tests; successful CI does not guarantee every live desktop configuration is compatible.

## Credits

MacTraffic's core is written in Rust. The optional C++ blur provider is derived from [Maplespe/ExplorerBlurMica](https://github.com/Maplespe/ExplorerBlurMica), with its LGPLv3 notices retained in `blur-provider/`. The modified companion source and license notices are included in the repository. This is an independent Windows customization project, not an Apple or Microsoft product.
