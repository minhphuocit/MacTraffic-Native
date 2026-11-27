# Explorer blur: optional bundled companion

MacTraffic is a standalone Rust caption manager. The full package bundles a separately compiled C++ ExplorerBlurMica companion from the archived project; the core EXE alone cannot blur the full Explorer body. A solid caption is not blur. The old experimental Rust DLL caption renderer and its C++ bridge are excluded to avoid competing with the current engine.

**No automatic registration:** build, launch, and CI never register the blur extension. Registering the companion explicitly requires Administrator and changes COM registry entries; the existing provider uses in-process Explorer hooks. If you require no registry changes or no in-process hooks, use the core-only package (no full Explorer blur).

## Install the full package
1. Extract `MacTraffic-v1.0.2-full-win-x64.zip` into a permanent writable folder, not a temporary Downloads project. Verify SHA256 against the release and review the source. Exit MacTraffic normally. If a different ExplorerBlurMica is registered, keep its folder and use its own official uninstall process first; the helper refuses to overwrite it.
2. Open **64-bit Windows PowerShell as Administrator**, change to the extracted `blur-provider` folder, and run `& .\Manage-Blur.ps1 -Action Enable`. Read the confirmation and type YES only if you accept registration. If script execution is blocked by your policy, do not bypass that policy; use the normal provider registration method or consult the administrator.
3. Save your work and restart Windows. Verify whole-window blur in Explorer before starting `MacTraffic.exe`. The bundled config uses Acrylic (`effect=1`). Keep `explorer_caption=auto` (detect known provider) or set `native` to preserve its caption background even if module inspection fails. Restart MacTraffic after changing INI.
4. Open two Explorer windows, switch focus, maximize/restore and check the blur, title and buttons. Also check normal Exit and Pause.

## Remove the companion
Exit MacTraffic. From the same permanent provider folder in an administrator 64-bit PowerShell run `& .\Manage-Blur.ps1 -Action Disable`. It refuses to unregister a different DLL. Restart Windows before deleting/moving the DLL. If Windows has a different provider installed, use that provider's own uninstall method.

The optional companion is derived from Maplespe/ExplorerBlurMica, LGPLv3 notices retained. The complete modified source is in `blur-provider/` in the source release/tag. No precompiled DLL was supplied or verified in this environment. Windows CI compiles it but does not test the live desktop.
