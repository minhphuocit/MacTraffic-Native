#pragma once
// Source-level integration with ExplorerBlurMica 1.0.7 only.
// The upstream project retains ownership of windows and of all original hooks.
#include <windows.h>
#include <string>
extern HMODULE g_hModule;
namespace MacTrafficNative {
    using Painter = int (WINAPI*)(HDC,int,int,RECT,UINT,int,int);
    inline Painter GetPainter() {
        // Same directory as the explicitly installed upstream test extension.
        // Do not search PATH or the current working directory.
        static Painter proc = []() -> Painter {
            wchar_t path[32768]{};
            DWORD n = GetModuleFileNameW(g_hModule,path,32768);
            if (!n || n >= 32768) return nullptr;
            std::wstring file(path,n);
            auto pos=file.find_last_of(L"\\/");
            if (pos==std::wstring::npos) return nullptr;
            file.resize(pos+1); file+=L"mactraffic_caption.dll";
            HMODULE module=LoadLibraryExW(file.c_str(),nullptr,LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR|LOAD_LIBRARY_SEARCH_SYSTEM32);
            if (!module) return nullptr;
            auto proc=reinterpret_cast<Painter>(GetProcAddress(module,"mt_caption_draw"));
            if (proc) {
                using Start=void (WINAPI*)();
                auto start=reinterpret_cast<Start>(GetProcAddress(module,"mt_caption_probe_start"));
                if(start) start();
            }
            return proc;
        }();
        return proc;
    }
    inline bool TryPaint(const std::wstring& theme,HDC dc,int part,int state,LPCRECT r,HWND mainWnd,bool clipOk) {
        if (!r || !mainWnd || !dc) return false;
        wchar_t cls[64]{};
        if (!GetClassNameW(mainWnd,cls,64) || wcscmp(cls,L"CabinetWClass")!=0) return false;
        auto paint=GetPainter(); if (!paint) return false;
        if (_wcsicmp(theme.c_str(),L"Window") != 0 || (part!=15 && part!=17 && part!=18 && part!=21)) return false;
        return paint(dc,part,state,*r,GetDpiForWindow(mainWnd),WindowFromDC(dc)==mainWnd,clipOk?1:0)==1;
    }
    inline bool Contains(LPCRECT outer,LPCRECT inner) {
        return !outer || (inner && outer->left<=inner->left && outer->top<=inner->top && outer->right>=inner->right && outer->bottom>=inner->bottom);
    }
}
