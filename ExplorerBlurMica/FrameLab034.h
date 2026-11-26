#pragma once
// Experimental opt-in extra caption band for the existing Explorer subclass, Win10 18363 only.
// No extra hooks, no desktop capture or separate top-level window.
#include <commctrl.h>
#include <new>
#include <cwchar>
#include <cstdlib>
namespace MacTrafficNative {
    using FrameHit = int (WINAPI*)(int,int,int,int,int,int,unsigned);
    using FrameDraw = int (WINAPI*)(HDC,int,int,int,int,unsigned,const wchar_t*,unsigned,int);
    struct FrameApi { FrameHit hit; FrameDraw draw; };
    inline FrameApi Api() {
        static FrameApi api=[](){
            HMODULE m=BridgeModule();
            return FrameApi{m?reinterpret_cast<FrameHit>(GetProcAddress(m,"mt_frame_hit")):nullptr,
                m?reinterpret_cast<FrameDraw>(GetProcAddress(m,"mt_frame_draw")):nullptr};
        }();
        return api;
    }
    constexpr UINT kToggle=0x1e30;
    constexpr wchar_t kProp[]=L"MacTraffic.FrameLab035.State";
    struct FrameState { LONG_PTR captionBits; BOOL oldNcPaint; bool restoreNcPaint; int pressed; };
    inline FrameState* State(HWND h) {return reinterpret_cast<FrameState*>(GetPropW(h,kProp));}
    inline bool Eligible(HWND h) {
        wchar_t cls[64]{};
        return GetClassNameW(h,cls,64)>0 && wcscmp(cls,L"CabinetWClass")==0 &&
            GetWindowThreadProcessId(h,nullptr)==GetCurrentThreadId();
    }
    inline UINT Dpi(HWND h){UINT d=GetDpiForWindow(h);return d?d:96;}
    inline int CaptionHeight(HWND h){return GetSystemMetricsForDpi(SM_CYCAPTION,Dpi(h));}
    inline bool Band(HWND h,RECT& band) {
        RECT wr{},cr{};POINT origin{};
        if(!GetWindowRect(h,&wr)||!GetClientRect(h,&cr)||!ClientToScreen(h,&origin))return false;
        const int top=origin.y-wr.top;
        const int extra=CaptionHeight(h);
        if(extra<=0||top<extra)return false;
        band={origin.x-wr.left,top-extra,origin.x-wr.left+cr.right,top};
        return band.right-band.left>MulDiv(160,Dpi(h),96);
    }
    inline int RoleAt(HWND h,POINT pt){
        RECT wr{},r{};auto api=Api();
        if(!api.hit||!GetWindowRect(h,&wr)||!Band(h,r))return -1;
        return api.hit(pt.x-wr.left,pt.y-wr.top,r.left,r.top,r.right,r.bottom,Dpi(h));
    }
    inline POINT ScreenPoint(LPARAM l){return POINT{static_cast<short>(LOWORD(l)),static_cast<short>(HIWORD(l))};}
    inline void Refresh(HWND h){
        SetWindowPos(h,nullptr,0,0,0,0,SWP_NOMOVE|SWP_NOSIZE|SWP_NOZORDER|SWP_NOACTIVATE|SWP_FRAMECHANGED|SWP_NOOWNERZORDER);
        RedrawWindow(h,nullptr,nullptr,RDW_INVALIDATE|RDW_FRAME|RDW_NOCHILDREN);
    }
    inline void EnsureMenu(HWND h){
        if(!Eligible(h)||!Api().draw||!Api().hit)return;
        HMENU menu=GetSystemMenu(h,FALSE);
        if(menu&&GetMenuState(menu,kToggle,MF_BYCOMMAND)==static_cast<UINT>(-1)){
            AppendMenuW(menu,MF_SEPARATOR,0,nullptr);
            if(AppendMenuW(menu,MF_STRING,kToggle,L"MacTraffic: Toggle test caption band")) Event(6,0,0,0,0);
        }
    }
    inline void Disable(HWND h,bool destroying=false){
        auto s=reinterpret_cast<FrameState*>(RemovePropW(h,kProp));if(!s)return;
        const LONG_PTR caption=s->captionBits;const BOOL old=s->oldNcPaint;const bool restore=s->restoreNcPaint;
        delete s;
        if(destroying)return;
        if(GetCapture()==h)ReleaseCapture();
        SetWindowLongPtrW(h,GWL_STYLE,(GetWindowLongPtrW(h,GWL_STYLE)&~static_cast<LONG_PTR>(WS_CAPTION))|caption);
        if(restore)DwmSetWindowAttribute(h,DWMWA_ALLOW_NCPAINT,&old,sizeof(old));
        Refresh(h);EnsureMenu(h);Event(6,2,0,0,0);
    }
    inline void Toggle(HWND h){
        if(State(h)){Disable(h);return;}
        if(!Eligible(h)||IsIconic(h)||IsZoomed(h)||!Api().draw||!Api().hit){Event(6,3,1,0,0);return;}
        LONG_PTR style=GetWindowLongPtrW(h,GWL_STYLE);
        if((style&WS_CAPTION)!=WS_CAPTION){Event(6,3,2,0,0);return;}
        RECT wr{},oldClient{};POINT oldOrigin{};
        if(!GetWindowRect(h,&wr)||!GetClientRect(h,&oldClient)||!ClientToScreen(h,&oldOrigin))return;
        auto s=new(std::nothrow) FrameState{style&WS_CAPTION,FALSE,false,-1};
        if(!s)return;
        // Only change the optional NC paint attribute if the previous value can be restored.
        s->restoreNcPaint=SUCCEEDED(DwmGetWindowAttribute(h,DWMWA_ALLOW_NCPAINT,&s->oldNcPaint,sizeof(BOOL)));
        if(!SetPropW(h,kProp,s)){delete s;return;}
        if(s->restoreNcPaint){BOOL yes=TRUE;DwmSetWindowAttribute(h,DWMWA_ALLOW_NCPAINT,&yes,sizeof(yes));}
        SetLastError(0);
        if(!SetWindowLongPtrW(h,GWL_STYLE,style&~static_cast<LONG_PTR>(WS_CAPTION))&&GetLastError()){
            Event(6,3,3,0,0);Disable(h);return;
        }
        Refresh(h);EnsureMenu(h);
        POINT newOrigin{};RECT newWr{},newClient{},band{};
        if(!ClientToScreen(h,&newOrigin)||!GetWindowRect(h,&newWr)||!GetClientRect(h,&newClient)||!Band(h,band)){
            Event(6,3,4,0,0);Disable(h);return;
        }
        // The experiment intentionally reserves one extra caption band. Do not
        // reject that expected inset, but reject unrelated layout changes.
        const int before=oldOrigin.y-wr.top,after=newOrigin.y-newWr.top;
        const int delta=after-before, extra=CaptionHeight(h);
        const int tol=MulDiv(4,Dpi(h),96);
        const int dx=(newOrigin.x-newWr.left)-(oldOrigin.x-wr.left);
        const int dw=newClient.right-oldClient.right;
        const int dh=newClient.bottom-oldClient.bottom;
        const int bottomDelta=delta+dh;
        Event(6,1,before,after,band.bottom-band.top);
        Event(10,delta,dx,dw,dh);
        // On this build, the old Explorer frame already extended to y=0.
        // Allow only the expected caption-height shift, with a small frame tolerance.
        if(extra<=0 || std::abs(delta-extra)>tol || std::abs(dx)>tol ||
           std::abs(dw)>2*tol || std::abs(bottomDelta)>tol ||
           std::abs((newWr.right-newWr.left)-(wr.right-wr.left))>tol ||
           std::abs((newWr.bottom-newWr.top)-(wr.bottom-wr.top))>tol ||
           newClient.bottom<MulDiv(120,Dpi(h),96)){
            Event(6,3,5,before,after);Disable(h);return;
        }
        Event(11,delta,extra,tol,bottomDelta);
        RECT buttons{};HRESULT hr=DwmGetWindowAttribute(h,DWMWA_CAPTION_BUTTON_BOUNDS,&buttons,sizeof(buttons));
        Event(9,static_cast<int>(hr),buttons.right-buttons.left,buttons.bottom-buttons.top,
            (GetWindowLongPtrW(h,GWL_STYLE)&WS_CAPTION)==WS_CAPTION?1:0);
        RedrawWindow(h,nullptr,nullptr,RDW_INVALIDATE|RDW_FRAME|RDW_NOCHILDREN|RDW_UPDATENOW);
    }
    inline void Paint(HWND h){
        RECT r{};auto api=Api();if(!api.draw||!Band(h,r))return;
        HDC dc=GetWindowDC(h);if(!dc)return;
        wchar_t title[256]{};int len=GetWindowTextW(h,title,256);
        const int saved=SaveDC(dc);
        int ok=0;
        if(saved){
            IntersectClipRect(dc,r.left,r.top,r.right,r.bottom);
            ok=api.draw(dc,r.left,r.top,r.right,r.bottom,Dpi(h),title,len>0?static_cast<unsigned>(len):0,GetForegroundWindow()==h?1:0);
            RestoreDC(dc,saved);
        }
        ReleaseDC(h,dc);Event(7,ok,r.right-r.left,r.bottom-r.top,0);
    }
    // True means the existing subclass has already forwarded/handled the message.
    inline bool HandleFrame(HWND h,UINT msg,WPARAM w,LPARAM l,LRESULT& result){
        if(msg==WM_SYSCOMMAND&&(w&0xfff0)==kToggle){Toggle(h);result=0;return true;}
        auto s=State(h);if(!s)return false;
        if(msg==WM_NCDESTROY){Disable(h,true);return false;}
        switch(msg){
        case WM_NCCALCSIZE:{
            result=DefSubclassProc(h,msg,w,l);
            if(l){RECT* r=w?&reinterpret_cast<NCCALCSIZE_PARAMS*>(l)->rgrc[0]:reinterpret_cast<RECT*>(l);
                const int extra=CaptionHeight(h);if(r->bottom-r->top>extra)r->top+=extra;}
            result=0;return true;
        }
        case WM_NCPAINT:
            result=DefSubclassProc(h,msg,w,l);if(State(h))Paint(h);return true;
        case WM_NCACTIVATE:
            result=DefSubclassProc(h,msg,w,l);if(State(h))RedrawWindow(h,nullptr,nullptr,RDW_INVALIDATE|RDW_FRAME|RDW_NOCHILDREN);return true;
        case WM_SETTEXT:
            result=DefSubclassProc(h,msg,w,l);if(State(h))RedrawWindow(h,nullptr,nullptr,RDW_INVALIDATE|RDW_FRAME|RDW_NOCHILDREN);return true;
        case WM_NCHITTEST:{
            result=DefSubclassProc(h,msg,w,l);
            // Keep the OS resize edges and the rest of Explorer's hit testing.
            if(result>=HTLEFT&&result<=HTBOTTOMRIGHT)return true;
            POINT p=ScreenPoint(l);RECT wr{},r{};
            if(GetWindowRect(h,&wr)&&Band(h,r)){
                POINT q{p.x-wr.left,p.y-wr.top};
                if(PtInRect(&r,q))result=RoleAt(h,p)>=0?HTBORDER:HTCAPTION;
            }
            return true;
        }
        case WM_NCLBUTTONDOWN:
            {int role=RoleAt(h,ScreenPoint(l));if(role>=0){
                s->pressed=role;SetCapture(h);result=0;return true;
            }}break;
        case WM_NCRBUTTONUP:{
            POINT p=ScreenPoint(l);RECT wr{},r{};
            if(GetWindowRect(h,&wr)&&Band(h,r)){POINT q{p.x-wr.left,p.y-wr.top};if(PtInRect(&r,q)){Disable(h);result=0;return true;}}
            break;
        }
        case WM_NCLBUTTONDBLCLK:
            if(RoleAt(h,ScreenPoint(l))>=0){result=0;return true;}break;
        case WM_LBUTTONUP:case WM_NCLBUTTONUP:
            if(s->pressed>=0){
                int role=s->pressed;s->pressed=-1;POINT p{};bool clicked=GetCursorPos(&p)&&RoleAt(h,p)==role;
                if(GetCapture()==h)ReleaseCapture();
                if(clicked){UINT cmd=role==0?SC_CLOSE:role==1?SC_MINIMIZE:IsZoomed(h)?SC_RESTORE:SC_MAXIMIZE;
                    Event(8,role,0,0,0);PostMessageW(h,WM_SYSCOMMAND,cmd,0);}
                result=0;return true;
            }break;
        case WM_CAPTURECHANGED:case WM_CANCELMODE:s->pressed=-1;break;
        }
        return false;
    }
}
