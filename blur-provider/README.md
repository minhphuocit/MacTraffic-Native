# Optional Explorer blur companion

Derived from the user-supplied Git archive at commit `0d20bc9560f46a76a3aa22ce3f8b0209ea8cd01c`, itself a modified ExplorerBlurMica 1.0.7-era project by Maplespe. Upstream: https://github.com/Maplespe/ExplorerBlurMica. Original license notices and GPL/LGPL texts are retained; see UPSTREAM-README.txt.

Changes: removed experimental MacTrafficBridge/FrameLab034 includes, logging callbacks, menu, and experimental caption painting. Preserved the existing blur implementation and COM extension registration. The companion is not rewritten in Rust. It is a separate native C++ shell extension loaded by Explorer after explicit registration; its use modifies COM registry entries and uses the original in-process hooks. It is not automatically registered by MacTraffic, the build, or CI. Do not register it alongside an already registered ExplorerBlurMica DLL.

Build x64 Release with the VS 2022 v143 toolset: `msbuild ExplorerBlurMica.sln /m /p:Configuration=Release /p:Platform=x64`. Windows CI builds it; the core GNU build does not require MSVC. The restored provider has not been built or desktop-tested here. Source is included so the exact modified provider can be inspected and rebuilt.
