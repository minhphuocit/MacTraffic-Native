#pragma once
// Experimental opt-in existing caption slot for the existing Explorer subclass, Win10 18363 only.
// No extra hooks, no desktop capture or separate top-level window.
#include <commctrl.h>
#include <new>
#include <cwchar>
#include <cstdlib>
#include <algorithm>
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
    constexpr UINT_PTR kPaintWatchdog=0x4d543038;
    constexpr wchar_t kProp[]=L"MacTraffic.FrameLab038.State";
    struct FrameState { LONG_PTR captionBits; BOOL oldNcPaint; bool restoreNcPaint; int pressed; int rightInset; int slotTop; int slotWidth; int slotHeight; bool accepted; bool painted; };
    inline FrameState* State(HWND h) {return reinterpret_cast<FrameState*>(GetPropW(h,kProp));}
    inline bool Eligible(HWND h) {
        wchar_t cls[64]{};
        return GetClassNameW(h,cls,64)>0 && wcscmp(cls,L"CabinetWClass")==0 &&
            GetWindowThreadProcessId(h,nullptr)==GetCurrentThreadId();
    }
    inline UINT Dpi(HWND h){UINT d=GetDpiForWindow(h);return d?d:96;}
    inline int CaptionHeight(HWND h){return GetSystemMetricsForDpi(SM_CYCAPTION,Dpi(h));}
    // Map the original DWM button slot into the current client coordinates.
    inline bool Band(HWND h,RECT& band) {
        auto s=State(h);RECT wr{},cr{};POINT origin{};
        if(!s||!GetWindowRect(h,&wr)||!GetClientRect(h,&cr)||!ClientToScreen(h,&origin))return false;
        int right=wr.right-s->rightInset-origin.x;
        int top=wr.top+s->slotTop-origin.y;
        band={right-s->slotWidth,(std::max)(0,top),right,top+s->slotHeight};
        band.left=(std::max)(0L,band.left);band.right=(std::min)(cr.right,band.right);
        band.bottom=(std::min)(cr.bottom,band.bottom);
        return band.right-band.left>=MulDiv(94,Dpi(h),96)&&band.bottom-band.top>=MulDiv(12,Dpi(h),96);
    }
    inline int RoleAt(HWND h,POINT pt){
        RECT r{};auto api=Api();auto s=State(h);
        if(!s||!s->accepted||!api.hit||!ScreenToClient(h,&pt)||!Band(h,r))return -1;
        return api.hit(pt.x,pt.y,r.left,r.top,r.right,r.bottom,Dpi(h));
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
            if(AppendMenuW(menu,MF_STRING,kToggle,L"MacTraffic: Toggle test caption")) Event(6,0,0,0,0);
        }
    }
    inline void Disable(HWND h,bool destroying=false){
        auto s=reinterpret_cast<FrameState*>(RemovePropW(h,kProp));if(!s)return;
        const LONG_PTR caption=s->captionBits;const BOOL old=s->oldNcPaint;const bool restore=s->restoreNcPaint;
        delete s;
        KillTimer(h,kPaintWatchdog);
        if(destroying)return;
        if(GetCapture()==h)ReleaseCapture();
        SetWindowLongPtrW(h,GWL_STYLE,(GetWindowLongPtrW(h,GWL_STYLE)&~static_cast<LONG_PTR>(WS_CAPTION))|caption);
        if(restore)DwmSetWindowAttribute(h,DWMWA_ALLOW_NCPAINT,&old,sizeof(old));
        Refresh(h);EnsureMenu(h);RedrawWindow(h,nullptr,nullptr,RDW_INVALIDATE|RDW_FRAME|RDW_ALLCHILDREN|RDW_UPDATENOW);Event(6,2,0,0,0);
    }
    // Invalidate the existing title region, including its Ribbon children.
    inline void QueueRepaint(HWND h,int reason){
        auto s=State(h); RECT band{},cr{};
        if(!s||!s->accepted||IsIconic(h)||!Band(h,band)||!GetClientRect(h,&cr))return;
        RECT header{0,0,cr.right,(std::min)(cr.bottom,band.bottom+MulDiv(6,Dpi(h),96))};
        std::wstring name=L"ribbon_buffer";
        Event(13,reason,band.left,band.right,IsZoomed(h)?1:0,&name);
        RedrawWindow(h,&header,nullptr,RDW_INVALIDATE|RDW_ALLCHILDREN);
    }
    // Adapt upstream Ribbon transparency only for a broad fill matching the title strip.
    inline bool RibbonTitleFill(HWND ribbon,const RECT* fill){
        if(!ribbon||!fill)return false;
        HWND h=GetAncestor(ribbon,GA_ROOT);auto s=State(h);RECT band{},cr{};
        if(!s||!s->accepted||!Band(h,band)||!GetClientRect(ribbon,&cr))return false;
        POINT p[2]={{0,band.top},{band.right,band.bottom}};MapWindowPoints(h,ribbon,p,2);
        const int tolerance=MulDiv(8,Dpi(h),96);
        if(p[0].y < -tolerance || p[0].y > tolerance || p[1].y<=0)return false;
        if(fill->left>tolerance || fill->top>tolerance || fill->top< -tolerance ||
           fill->right<cr.right-tolerance || fill->bottom < p[1].y-tolerance ||
           fill->bottom > p[1].y+tolerance)return false;
        Event(14,fill->left,fill->top,fill->right,fill->bottom);return true;
    }
    // Use only the actual Ribbon memory DC already identified by the upstream code.
    // Coordinates remain in the existing title slot. Never fill the surrounding backdrop.
    inline void PaintRibbonBuffer(HWND ribbon,HDC dc){
        if(!ribbon||!dc)return;
        HWND h=GetAncestor(ribbon,GA_ROOT);auto s=State(h);RECT band{};
        if(!s||!s->accepted||IsIconic(h)||!Band(h,band))return;
        if(GetWindowThreadProcessId(ribbon,nullptr)!=GetCurrentThreadId()||
           GetObjectType(dc)!=OBJ_MEMDC)return;
        POINT p[2]={{band.left,band.top},{band.right,band.bottom}};
        MapWindowPoints(h,ribbon,p,2);RECT r{p[0].x,p[0].y,p[1].x,p[1].y};
        RECT cr{},clip{};
        if(!GetClientRect(ribbon,&cr)||r.left<0||r.top<0||r.right>cr.right||r.bottom>cr.bottom){
            Event(15,1,r.left,r.top,r.right);return;
        }
        const int radius=MulDiv(6,Dpi(h),96),cy=r.top+(r.bottom-r.top)/2;
        const int left=r.right-MulDiv(70,Dpi(h),96)-radius;
        const int right=r.right-MulDiv(26,Dpi(h),96)+radius+1;
        const int clipType=GetClipBox(dc,&clip);
        if(clipType==ERROR||clipType==NULLREGION||clip.left>left||clip.right<right||
           clip.top>cy-radius||clip.bottom<cy+radius+1){
            Event(15,2,clip.left,clip.top,clip.bottom);return;
        }
        auto api=Api();if(!api.draw)return;
        // Prevent reentrancy from GDI calls used by the Rust renderer.
        static thread_local bool drawing=false;if(drawing)return;
        int saved=SaveDC(dc);if(!saved)return;
        drawing=true;
        IntersectClipRect(dc,r.left,r.top,r.right,r.bottom);
        int ok=api.draw(dc,r.left,r.top,r.right,r.bottom,Dpi(h),nullptr,0,GetForegroundWindow()==h?1:0);
        RestoreDC(dc,saved);drawing=false;
        if(ok){if(auto live=State(h))live->painted=true;}
        Event(7,ok,r.right-r.left,r.bottom-r.top,0);
        Event(15,ok?0:3,r.left,r.top,r.right);
    }
    inline void Toggle(HWND h){
        if(State(h)){Disable(h);return;}
        if(!Eligible(h)||IsIconic(h)||IsZoomed(h)||!Api().draw||!Api().hit){Event(6,3,1,0,0);return;}
        LONG_PTR style=GetWindowLongPtrW(h,GWL_STYLE);
        if((style&WS_CAPTION)!=WS_CAPTION){Event(6,3,2,0,0);return;}
        RECT wr{},oldClient{};POINT oldOrigin{};
        if(!GetWindowRect(h,&wr)||!GetClientRect(h,&oldClient)||!ClientToScreen(h,&oldOrigin))return;
        RECT buttonsBefore{};
        if(FAILED(DwmGetWindowAttribute(h,DWMWA_CAPTION_BUTTON_BOUNDS,&buttonsBefore,sizeof(buttonsBefore)))||
           buttonsBefore.right-buttonsBefore.left<MulDiv(94,Dpi(h),96)||
           buttonsBefore.bottom-buttonsBefore.top<MulDiv(12,Dpi(h),96)||
           buttonsBefore.left<0||buttonsBefore.top<0||buttonsBefore.right>wr.right-wr.left||
           buttonsBefore.bottom>MulDiv(80,Dpi(h),96)) {Event(6,3,6,0,0);return;}
        auto s=new(std::nothrow) FrameState{style&WS_CAPTION,FALSE,false,-1,
            static_cast<int>(wr.right-wr.left-buttonsBefore.right),static_cast<int>(buttonsBefore.top),
            static_cast<int>(buttonsBefore.right-buttonsBefore.left),static_cast<int>(buttonsBefore.bottom-buttonsBefore.top),false,false};
        if(!s)return;
        // Only change the optional NC paint attribute if the previous value can be restored.
        s->restoreNcPaint=false;
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
        // No additional caption band: preserve existing client geometry.
        const int before=oldOrigin.y-wr.top,after=newOrigin.y-newWr.top;
        const int delta=after-before, extra=0;
        const int tol=MulDiv(4,Dpi(h),96);
        const int dx=(newOrigin.x-newWr.left)-(oldOrigin.x-wr.left);
        const int dw=newClient.right-oldClient.right;
        const int dh=newClient.bottom-oldClient.bottom;
        const int bottomDelta=delta+dh;
        Event(6,1,before,after,band.bottom-band.top);
        Event(10,delta,dx,dw,dh);
        // Reject client displacement beyond the existing frame tolerance.
        if(std::abs(delta)>tol || std::abs(dx)>tol ||
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
        if(FAILED(hr)||buttons.right>buttons.left){Event(6,3,7,buttons.right-buttons.left,0);Disable(h);return;}
        auto live=State(h);if(!live)return;live->accepted=true;
        Event(12,band.left,band.top,band.right,band.bottom);
        // One-shot safety check: restore native controls if the Ribbon paint path is absent.
        if(!SetTimer(h,kPaintWatchdog,1200,nullptr)){Event(6,3,8,0,0);Disable(h);return;}
        RedrawWindow(h,nullptr,nullptr,RDW_INVALIDATE|RDW_FRAME|RDW_ALLCHILDREN);
        QueueRepaint(h,1);
    }
    // True means the existing subclass has already forwarded/handled the message.
    inline bool HandleFrame(HWND h,UINT msg,WPARAM w,LPARAM l,LRESULT& result){
        if(msg==WM_SYSCOMMAND&&(w&0xfff0)==kToggle){Toggle(h);result=0;return true;}
        auto s=State(h);if(!s)return false;
        if(msg==WM_NCDESTROY){Disable(h,true);return false;}
        switch(msg){
        case WM_TIMER:
            if(w==kPaintWatchdog){KillTimer(h,kPaintWatchdog);if(!s->painted){Event(6,3,9,0,0);Disable(h);}result=0;return true;}break;
        case WM_NCCALCSIZE:{
            result=DefSubclassProc(h,msg,w,l);
            // Preserve geometry, but disallow WVR_VALIDRECTS pixel-copy on resize.
            if(w && State(h) && State(h)->accepted)result=WVR_REDRAW;
            return true;
        }
        case WM_WINDOWPOSCHANGING:
            if(s->accepted&&l){auto pos=reinterpret_cast<WINDOWPOS*>(l);if(!(pos->flags&SWP_NOSIZE))pos->flags|=SWP_NOCOPYBITS;}
            break;
        case WM_SIZE:
            result=DefSubclassProc(h,msg,w,l);if(w!=SIZE_MINIMIZED)QueueRepaint(h,2);return true;
        case WM_NCACTIVATE:
            result=DefSubclassProc(h,msg,w,l);QueueRepaint(h,3);return true;
        case WM_SETTEXT:
            result=DefSubclassProc(h,msg,w,l);QueueRepaint(h,4);return true;
        case WM_NCHITTEST:{
            result=DefSubclassProc(h,msg,w,l);
            // Keep the OS resize edges and the rest of Explorer's hit testing.
            if(result>=HTLEFT&&result<=HTBOTTOMRIGHT)return true;
            if(RoleAt(h,ScreenPoint(l))>=0)result=HTBORDER;
            return true;
        }
        case WM_LBUTTONDOWN:case WM_NCLBUTTONDOWN:
            {POINT p=ScreenPoint(l);if(msg==WM_LBUTTONDOWN&&!ClientToScreen(h,&p))break;int role=RoleAt(h,p);if(role>=0){
                s->pressed=role;SetCapture(h);result=0;return true;
            }}break;
        case WM_NCRBUTTONUP:{
            if(RoleAt(h,ScreenPoint(l))>=0){Disable(h);result=0;return true;}
            break;
        }
        case WM_LBUTTONDBLCLK:case WM_NCLBUTTONDBLCLK:{
            POINT p=ScreenPoint(l);if(msg==WM_LBUTTONDBLCLK&&!ClientToScreen(h,&p))break;
            if(RoleAt(h,p)>=0){result=0;return true;}break;}
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
