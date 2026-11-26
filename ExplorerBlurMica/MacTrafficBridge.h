#pragma once
// Reuse upstream callbacks. Native frame is opt-in per Explorer window.
#include <windows.h>
#include <dwmapi.h>
#include <string>
#include <atomic>
extern HMODULE g_hModule;
namespace MacTrafficNative {
    using Logger = void (WINAPI*)(int,int,int,int,int,const wchar_t*,unsigned int);
    inline HMODULE BridgeModule() {
        static HMODULE module = []() -> HMODULE {
            wchar_t path[32768]{};
            DWORD n=GetModuleFileNameW(g_hModule,path,32768);
            if (!n || n>=32768) return nullptr;
            std::wstring file(path,n);
            auto pos=file.find_last_of(L"\\/");
            if(pos==std::wstring::npos) return nullptr;
            file.resize(pos+1); file+=L"mactraffic_caption.dll";
            return LoadLibraryExW(file.c_str(),nullptr,LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR|LOAD_LIBRARY_SEARCH_SYSTEM32);
        }();
        return module;
    }
    inline Logger GetLogger() {
        static Logger logger=[]() -> Logger {
            HMODULE module=BridgeModule();
            if(!module) return nullptr;
            auto fn=reinterpret_cast<Logger>(GetProcAddress(module,"mt_probe_event"));
            if(fn) fn(0,0,0,0,0,nullptr,0);
            return fn;
        }();
        return logger;
    }
    inline void Event(int kind,int a,int b,int c,int d,const std::wstring* name=nullptr) {
        if(auto fn=GetLogger()) fn(kind,a,b,c,d,name?name->c_str():nullptr,name?static_cast<unsigned int>(name->size()):0);
    }
    inline bool Candidate(int part) {return part==15||part==17||part==18||part==21;}
    inline void ObserveTheme(const std::wstring& theme,int part,int state,bool tracked,int path) {
        if(!Candidate(part)) return;
        // Theme part identifiers are only meaningful together with their class.
        if (!tracked && theme.find(L"Window")==std::wstring::npos && theme.find(L"WINDOW")==std::wstring::npos && theme.find(L"window")==std::wstring::npos && !theme.empty()) return;
        Event(3,part,state,tracked?1:0,path,&theme);
    }
    inline void ObserveNC(HWND hwnd,UINT msg) {
        unsigned bit=0;
        switch(msg) {
            case WM_NCCALCSIZE: bit=1; break;
            case WM_NCPAINT: bit=2; break;
            case WM_NCACTIVATE: bit=4; break;
            case WM_NCHITTEST: bit=8; break;
            default:return;
        }
        static std::atomic<unsigned> seen{0};
        if(seen.fetch_or(bit,std::memory_order_relaxed)&bit) return;
        BOOL enabled=FALSE; RECT bounds{};
        HRESULT e=DwmGetWindowAttribute(hwnd,DWMWA_NCRENDERING_ENABLED,&enabled,sizeof(enabled));
        HRESULT b=DwmGetWindowAttribute(hwnd,DWMWA_CAPTION_BUTTON_BOUNDS,&bounds,sizeof(bounds));
        Event(4,static_cast<int>(msg),SUCCEEDED(e)?static_cast<int>(enabled):-1,static_cast<int>(e),static_cast<int>(b));
        if(SUCCEEDED(b)) Event(5,static_cast<int>(msg),bounds.right-bounds.left,bounds.bottom-bounds.top,0);
    }
}
