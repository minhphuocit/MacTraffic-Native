# Clean-install check

- Core portable ZIP: extract and run MacTraffic.exe; no build tools or blur dependency needed for captions.
- Full portable ZIP: includes the optional blur provider. Follow EXPLORER-BLUR.md to register it explicitly before using MacTraffic. Do not delete its directory while registered.
- Browser migration once: run MacTraffic, then use the tray command Open stable browser Spacer folder. Its file is `%LOCALAPPDATA%\MacTraffic\Spacer.html`. Close only stale Spacer tabs. Open the stable file in three tabs in Brave and two in Edge, pin them at the left, select a normal tab. Keep the stable file across upgrades. Existing pinned tabs pointing at deleted project folders cannot migrate automatically.
- Verify Explorer blur while MacTraffic is paused and resumed, active/inactive titles, maximize/restore, browser tabs, Win+D and normal exit. If blur does not work independently of MacTraffic, stop and troubleshoot the provider before changing caption settings.
- Startup is optional; disable the old shortcut and enable it from the new permanent location.
- Logs: `%TEMP%\MacTraffic-v103-hotfix.log`. Use `log_level=debug` for detailed layout diagnostics.
- Publish only after the full package passes Windows CI and a live desktop test; the prior 100/100 Rust tests apply to the RC, not proof of a working blur extension.
