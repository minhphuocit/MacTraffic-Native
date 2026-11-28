#![cfg_attr(not(test), windows_subsystem = "windows")]
// Persistent owner-window manager. Preserves native caption; independent alpha-only sprites
// supplies the three dots. Native path stays alpha-only; custom captions sample local colors.
// Only a bounded local ribbon color probe; no full-screen capture, injection or registry changes.
use std::{collections::HashMap, ffi::c_void, mem::{size_of,zeroed}, ptr::{null,null_mut}, time::{Duration,Instant}};
mod custom_caption;
mod release;
mod explorer_caption;
type H=isize; type W=usize; type L=isize;
const BUTTON_BITS:u32=0x000b0000;
const WS_CAPTION:u32=0x00c00000;
const THEME_HIDE:u32=0x00000001|0x00000002; // native caption text + system icon; not the Ribbon QAT
const FRAME_SETTLE:Duration=Duration::from_millis(160);
const COVERED_GRACE:Duration=Duration::from_millis(650);
fn coverage_expired(since:&mut Option<Instant>,now:Instant)->bool{now.saturating_duration_since(*since.get_or_insert(now))>=COVERED_GRACE}
fn settings_owner_allowed(is_settings:bool,is_root:bool,has_owner:bool)->bool{is_root&&(!has_owner||is_settings)}
const LEFT_MARGIN_DIP:i32=8; // was 3; browser spacer keeps its measured center
// Paint/WordPad originally passed at scale(3)+scale(5); try the new margin first.
fn ribbon_left_x(frame_left:i32,dpi:u32,preferred:bool)->i32{frame_left+scale(if preferred{LEFT_MARGIN_DIP}else{3},dpi)+scale(5,dpi)}
const BACKGROUND_PERIOD:Duration=Duration::from_millis(750);
const BACKGROUND_FALLBACK:Duration=Duration::from_millis(160);
const SWP_NOZORDER:u32=0x0004;
const SWP_NOACTIVATE:u32=0x0010;
const SWP_NOCOPYBITS:u32=0x0100;
const SWP_NOOWNERZORDER:u32=0x0200;
const RDW_INVALIDATE:u32=0x0001;
const RDW_ERASE:u32=0x0004;
const RDW_ALLCHILDREN:u32=0x0080;
const RDW_FRAME:u32=0x0400;
fn zoom_flags(legacy_copy:bool)->u32{
 SWP_NOZORDER|SWP_NOACTIVATE|SWP_NOOWNERZORDER|(if legacy_copy{0}else{SWP_NOCOPYBITS})
}
const WM_LBUTTONUP:u32=0x0202;
const WM_MANAGER_WAKE:u32=0x8001;
static MANAGER_WAKE:std::sync::atomic::AtomicIsize=std::sync::atomic::AtomicIsize::new(0);
static WAKE_QUEUED:std::sync::atomic::AtomicBool=std::sync::atomic::AtomicBool::new(false);
fn wake_manager(){
 use std::sync::atomic::Ordering;
 let h=MANAGER_WAKE.load(Ordering::Acquire);
 if h!=0&&!WAKE_QUEUED.swap(true,Ordering::AcqRel){if unsafe{PostMessageW(h,WM_MANAGER_WAKE,0,0)}==0{WAKE_QUEUED.store(false,Ordering::Release);}}
}
// Out-of-context WinEvents are delivered on the installing message-loop thread.
// Only keep bounded metadata here; never call UIA or paint inside a callback.
const DIRTY_BACKGROUND:u32=1;
const DIRTY_INSPECT:u32=2;
const DIRTY_ENVIRONMENT:u32=4;
const DIRTY_SHUTDOWN:u32=8;
const DIRTY_MOTION:u32=16;
const DIRTY_VISIBILITY:u32=32;
static EVENT_FLAGS:std::sync::atomic::AtomicU32=std::sync::atomic::AtomicU32::new(0);
static HOVER_WINDOW:std::sync::atomic::AtomicIsize=std::sync::atomic::AtomicIsize::new(0);
thread_local!{
 static TRACKED:std::cell::RefCell<Vec<(H,u32)>>=std::cell::RefCell::new(Vec::with_capacity(MAX_SESSIONS));
 static DESTROYED:std::cell::RefCell<Vec<H>>=std::cell::RefCell::new(Vec::with_capacity(MAX_SESSIONS));
}
fn signal(flags:u32){if release::PAUSED.load(std::sync::atomic::Ordering::Relaxed)&&flags&DIRTY_SHUTDOWN==0{return;}EVENT_FLAGS.fetch_or(flags,std::sync::atomic::Ordering::Release);wake_manager();}
fn event_is_current(event_time:u32,born:u32)->bool{event_time.wrapping_sub(born)<0x80000000}
fn track_window(h:H){TRACKED.with(|t|{let mut t=t.borrow_mut();t.retain(|(w,_)|*w!=h);t.push((h,unsafe{GetTickCount()}));});}
fn reconcile_tracked(sessions:&HashMap<H,Session>){TRACKED.with(|t|t.borrow_mut().retain(|(h,_)|sessions.contains_key(h)));}
fn take_destroyed()->Vec<H>{DESTROYED.with(|d|std::mem::take(&mut *d.borrow_mut()))}
unsafe extern "system" fn window_event(_:H,event:u32,h:H,object:i32,child:i32,_:u32,event_time:u32){
 if release::PAUSED.load(std::sync::atomic::Ordering::Relaxed){return;}
 if event==3{signal(DIRTY_BACKGROUND|DIRTY_INSPECT|DIRTY_MOTION);return;}
 if h==0{return;}
 if event>=0x8000&&(object!=0||child!=0){return;}
 let tracked=TRACKED.with(|t|t.try_borrow().ok().is_some_and(|t|t.iter().any(|(w,born)|*w==h&&event_is_current(event_time,*born))));
 if event==0x8001{ // EVENT_OBJECT_DESTROY; never touch a destroyed owner's styles.
  if tracked{DESTROYED.with(|d|{if let Ok(mut d)=d.try_borrow_mut(){if d.len()<MAX_SESSIONS&&!d.contains(&h){d.push(h);}}});signal(DIRTY_BACKGROUND);}
  return;
 }
 let foreground=GetForegroundWindow()==h;
 if !foreground&&!tracked{return;}
 let flags=match event{
  0x0016|0x0017|0x8002|0x8003|0x8017|0x8018=>DIRTY_BACKGROUND|DIRTY_INSPECT|DIRTY_MOTION|DIRTY_VISIBILITY,
  0x000a|0x000b|0x800a=>DIRTY_BACKGROUND|DIRTY_INSPECT|DIRTY_MOTION,
  0x800b=>DIRTY_BACKGROUND|DIRTY_MOTION,
  0x8004=>DIRTY_BACKGROUND,
  0x800c=>0, // title change: refresh the foreground or audit on fallback
  _=>return,
 };
 if foreground||flags!=0{signal(flags);}
}
struct EventHooks(Vec<H>);
impl EventHooks{
 unsafe fn install()->Self{
  let mut hooks=Vec::with_capacity(6);
  for (lo,hi) in [(3,3),(0x000a,0x000b),(0x0016,0x0017),(0x8001,0x8004),(0x800a,0x800c),(0x8017,0x8018)]{
   // WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS.
   let hook=SetWinEventHook(lo,hi,0,Some(window_event),0,0,2);
   if hook!=0{hooks.push(hook);}else{log(&format!("event_hook_failed range={lo:#x}..{hi:#x}"));}
  }
  Self(hooks)
 }
 fn complete(&self)->bool{self.0.len()==6}
}
impl Drop for EventHooks{fn drop(&mut self){use std::sync::atomic::Ordering;MANAGER_WAKE.store(0,Ordering::Release);WAKE_QUEUED.store(false,Ordering::Release);EVENT_FLAGS.store(0,Ordering::Release);for hook in self.0.drain(..){unsafe{UnhookWinEvent(hook);}}}}
// Fast ticks while moving/initializing/verifying; slower audit while stable.
// Worker completions and mouse enter/leave independently wake the manager.
fn interval_ms(hooks_complete:bool,admitted:bool,settling:bool,has_sessions:bool)->u32{
 if !hooks_complete{return if admitted||settling{40}else{160};}
 if settling{return 40;}
 if admitted{return 100;}
 if has_sessions{250}else{500}
}
#[repr(C)] #[derive(Clone,Copy,Default,PartialEq,Eq,Debug)] struct Point{x:i32,y:i32}
#[repr(C)] #[derive(Clone,Copy,Default,PartialEq,Eq,Debug)] struct Rect{left:i32,top:i32,right:i32,bottom:i32}
#[repr(C)] struct MonitorInfo{size:u32,monitor:Rect,work:Rect,flags:u32}
#[repr(C)] struct TrackMouse{size:u32,flags:u32,hwnd:H,hover_time:u32}
#[repr(C)] struct Msg{hwnd:H,message:u32,wparam:W,lparam:L,time:u32,pt:Point,private:u32}
#[repr(C)] struct Class{style:u32,proc:Option<unsafe extern "system" fn(H,u32,W,L)->L>,cls_extra:i32,wnd_extra:i32,instance:H,icon:H,cursor:H,background:H,menu:*const u16,name:*const u16}
#[repr(C)] struct BitmapInfo{size:u32,width:i32,height:i32,planes:u16,bits:u16,compression:u32,image_size:u32,xppm:i32,yppm:i32,used:u32,important:u32,colors:u32}
#[repr(C)] struct Blend{operation:u8,flags:u8,alpha:u8,format:u8}
#[link(name="user32")] extern "system"{
 fn SetWinEventHook(event_min:u32,event_max:u32,module:H,cb:Option<unsafe extern "system" fn(H,u32,H,i32,i32,u32,u32)>,pid:u32,thread:u32,flags:u32)->H;
 fn UnhookWinEvent(h:H)->i32;
 fn GetAncestor(h:H,flags:u32)->H;
 #[link_name="GetWindow"] fn window_neighbor(h:H,cmd:u32)->H;
 fn GetWindowTextW(h:H,buf:*mut u16,max:i32)->i32;
 fn RedrawWindow(h:H,rect:*const Rect,region:H,flags:u32)->i32;
 fn DrawTextW(dc:H,text:*const u16,count:i32,rect:*mut Rect,flags:u32)->i32;
 fn TrackMouseEvent(event:*mut TrackMouse)->i32;
 fn GetCursorPos(p:*mut Point)->i32; fn GetWindowTextLengthW(h:H)->i32;
 fn GetClassNameW(h:H,name:*mut u16,max:i32)->i32;
 fn GetWindowThreadProcessId(h:H,pid:*mut u32)->u32;
 fn IsWindow(h:H)->i32; fn IsWindowEnabled(h:H)->i32; fn EnumChildWindows(h:H,cb:Option<unsafe extern "system" fn(H,L)->i32>,data:L)->i32; fn IsWindowVisible(h:H)->i32; fn IsIconic(h:H)->i32; fn IsZoomed(h:H)->i32;
 fn GetWindowLongW(h:H,index:i32)->i32; fn SetWindowLongW(h:H,index:i32,value:i32)->i32;
 fn MonitorFromRect(r:*const Rect,flags:u32)->H;
 fn MonitorFromWindow(h:H,flags:u32)->H; fn GetMonitorInfoW(h:H,info:*mut MonitorInfo)->i32;
 fn GetWindowRect(h:H,r:*mut Rect)->i32; fn GetClientRect(h:H,r:*mut Rect)->i32; fn ClientToScreen(h:H,p:*mut Point)->i32; fn GetDpiForWindow(h:H)->u32; fn SetProcessDpiAwarenessContext(c:H)->i32;
 fn SetWindowPos(h:H,after:H,x:i32,y:i32,w:i32,hgt:i32,flags:u32)->i32;
 fn ShowWindow(h:H,cmd:i32)->i32; fn ShowWindowAsync(h:H,cmd:i32)->i32; fn GetForegroundWindow()->H; fn SetForegroundWindow(h:H)->i32; fn WindowFromPoint(p:Point)->H;
 fn RegisterClassW(c:*const Class)->u16;
 fn CreateWindowExW(ex:u32,class:*const u16,name:*const u16,style:u32,x:i32,y:i32,w:i32,h:i32,parent:H,menu:H,instance:H,param:*const c_void)->H;
 fn DefWindowProcW(h:H,m:u32,w:W,l:L)->L; fn DestroyWindow(h:H)->i32; 
 fn GetMessageW(m:*mut Msg,h:H,min:u32,max:u32)->i32; fn TranslateMessage(m:*const Msg)->i32; fn DispatchMessageW(m:*const Msg)->L;
 fn PostMessageW(h:H,m:u32,w:W,l:L)->i32; fn LoadCursorW(h:H,name:*const u16)->H;
 fn SetTimer(h:H,id:W,ms:u32,cb:usize)->W; fn KillTimer(h:H,id:W)->i32;
 fn RegisterHotKey(h:H,id:i32,mods:u32,key:u32)->i32; fn UnregisterHotKey(h:H,id:i32)->i32;
 fn GetDC(h:H)->H; fn ReleaseDC(h:H,dc:H)->i32;
 fn UpdateLayeredWindow(h:H,dst:H,position:*const Point,size:*const Point,src:H,origin:*const Point,key:u32,blend:*const Blend,flags:u32)->i32;
 fn MessageBoxW(h:H,text:*const u16,title:*const u16,flags:u32)->i32;
}
// PROCESSENTRY32W, native pointer alignment on x64.
#[repr(C)] struct ProcessEntry{size:u32,usage:u32,pid:u32,heap:usize,module:u32,threads:u32,parent:u32,priority:i32,flags:u32,exe:[u16;260]}
#[link(name="kernel32")] extern "system"{
 fn GetTickCount()->u32;
 fn CreateToolhelp32Snapshot(flags:u32,pid:u32)->H;
 fn Process32FirstW(snapshot:H,entry:*mut ProcessEntry)->i32;
 fn Process32NextW(snapshot:H,entry:*mut ProcessEntry)->i32;
 fn OpenProcess(access:u32,inherit:i32,pid:u32)->H;
 fn QueryFullProcessImageNameW(process:H,flags:u32,name:*mut u16,size:*mut u32)->i32;
 fn CreateMutexW(attributes:*const c_void,owner:i32,name:*const u16)->H; fn CloseHandle(h:H)->i32; fn SetLastError(e:u32); fn GetLastError()->u32; fn GetModuleHandleW(name:*const u16)->H;
 fn LoadLibraryExW(name:*const u16,file:H,flags:u32)->H; fn GetProcAddress(module:H,name:*const u8)->*const c_void; fn FreeLibrary(module:H)->i32;
}
#[link(name="gdi32")] extern "system"{
 fn GetPixel(dc:H,x:i32,y:i32)->u32;
 fn BitBlt(dst:H,x:i32,y:i32,w:i32,h:i32,src:H,sx:i32,sy:i32,rop:u32)->i32;
 fn CreateFontW(height:i32,width:i32,escapement:i32,orientation:i32,weight:i32,italic:u32,underline:u32,strikeout:u32,charset:u32,out_precision:u32,clip_precision:u32,quality:u32,pitch_and_family:u32,face:*const u16)->H;
 fn SetTextColor(dc:H,color:u32)->u32; fn SetBkMode(dc:H,mode:i32)->i32; fn GdiFlush()->i32;
 fn CreateCompatibleDC(dc:H)->H; fn CreateDIBSection(dc:H,info:*const BitmapInfo,usage:u32,bits:*mut *mut c_void,section:H,offset:u32)->H;
 fn SelectObject(dc:H,obj:H)->H; fn DeleteObject(obj:H)->i32; fn DeleteDC(dc:H)->i32;
}
impl Rect{fn width(self)->i32{self.right-self.left}fn height(self)->i32{self.bottom-self.top}}
fn inside(r:Rect,p:Point)->bool{p.x>=r.left&&p.x<r.right&&p.y>=r.top&&p.y<r.bottom}
fn union(a:Rect,b:Rect)->Rect{Rect{left:a.left.min(b.left),top:a.top.min(b.top),right:a.right.max(b.right),bottom:a.bottom.max(b.bottom)}}
fn wide(s:&str)->Vec<u16>{s.encode_utf16().chain(Some(0)).collect()}
fn log(s:&str){release::write_log(s);}
fn scale(v:i32,dpi:u32)->i32{(v*dpi as i32+48)/96}
fn style(h:H)->u32{unsafe{GetWindowLongW(h,-16) as u32}}
fn identity(h:H)->(u32,u32){let mut p=0;let t=unsafe{GetWindowThreadProcessId(h,&mut p)};(t,p)}
fn change(h:H,new:u32)->bool{unsafe{
 SetLastError(0);let prev=SetWindowLongW(h,-16,new as i32);
 if prev==0&&GetLastError()!=0{log(&format!("style_error={}",GetLastError()));return false;}
 if SetWindowPos(h,0,0,0,0,0,0x0237)==0{log(&format!("frame_error={}",GetLastError()));return false;}
 true
}}
struct Restore{h:H,id:(u32,u32),old:u32}
impl Restore{fn alive(&self)->bool{(unsafe{IsWindow(self.h)!=0})&&identity(self.h)==self.id}}
impl Drop for Restore{fn drop(&mut self){if self.alive(){let s=(style(self.h)&!BUTTON_BITS)|(self.old&BUTTON_BITS);let ok=change(self.h,s);log(&format!("restored={} style=0x{:08x}",ok,style(self.h)));}}}
struct Dwm{module:H,attr:unsafe extern "system" fn(H,u32,*mut c_void,u32)->i32}
impl Dwm{fn load()->Option<Self>{unsafe{let m=LoadLibraryExW(wide("dwmapi.dll").as_ptr(),0,0x800);if m==0{return None;}let p=GetProcAddress(m,b"DwmGetWindowAttribute\0".as_ptr());if p.is_null(){FreeLibrary(m);return None;}Some(Self{module:m,attr:std::mem::transmute(p)})}}
 fn rect(&self,h:H,a:u32)->Option<Rect>{let mut r=Rect::default();let hr=unsafe{(self.attr)(h,a,(&mut r as *mut Rect).cast(),size_of::<Rect>() as u32)};if hr<0{None}else{Some(r)}}
 fn cloaked(&self,h:H)->bool{let mut value=0u32;let hr=unsafe{(self.attr)(h,14,(&mut value as *mut u32).cast(),size_of::<u32>() as u32)};hr>=0&&value!=0}
}
impl Drop for Dwm{fn drop(&mut self){unsafe{FreeLibrary(self.module);}}}
fn window_rect(h:H)->Option<Rect>{let mut r=Rect::default();if unsafe{GetWindowRect(h,&mut r)}==0{None}else{Some(r)}}
fn visible_rect(d:&Dwm,h:H)->Option<Rect>{d.rect(h,9).filter(|r|r.right>r.left&&r.bottom>r.top).or_else(||window_rect(h))}
// Work-area zoom deliberately keeps a normal (non-WS_MAXIMIZE) frame.
// It uses the existing visible frame margins, never changes the monitor work area.
fn zoom_rect(outer:Rect,visible:Rect,work:Rect)->Option<Rect>{
 let l=visible.left-outer.left; let t=visible.top-outer.top;
 let r=outer.right-visible.right; let b=outer.bottom-visible.bottom;
 if [l,t,r,b].iter().any(|&n|!(0..=64).contains(&n))||work.right-work.left<320||work.bottom-work.top<240{return None;}
 Some(Rect{left:work.left-l,top:work.top-t,right:work.right+r,bottom:work.bottom+b})
}
fn near_rect(a:Rect,b:Rect,tol:i32)->bool{
 (a.left-b.left).abs()<=tol&&(a.top-b.top).abs()<=tol&&(a.right-b.right).abs()<=tol&&(a.bottom-b.bottom).abs()<=tol
}
// Invalidate the target frame and children without drawing any pixels ourselves.
// No synchronous UPDATENOW and no repeated background erasing in the manager tick.
fn zoom_repaint(h:H,erase:bool)->bool{unsafe{
 let flags=RDW_INVALIDATE|RDW_ALLCHILDREN|RDW_FRAME|(if erase{RDW_ERASE}else{0});
 let ok=RedrawWindow(h,null(),0,flags)!=0;
 log(&format!("zoom_repaint hwnd={h:#x} erase={erase} ok={ok} flags={flags:#06x}"));ok
}}
fn position_normal(h:H,r:Rect,legacy_copy:bool)->bool{unsafe{
 if IsWindow(h)==0||IsIconic(h)!=0||IsZoomed(h)!=0||r.right<=r.left||r.bottom<=r.top{return false;}
 let flags=zoom_flags(legacy_copy);
 let ok=SetWindowPos(h,0,r.left,r.top,r.right-r.left,r.bottom-r.top,flags)!=0;
 log(&format!("zoom_resize hwnd={h:#x} nocopy={} ok={ok} flags={flags:#06x}",!legacy_copy));
 if ok&&!legacy_copy{zoom_repaint(h,true);}
 ok
}}
#[derive(Clone,Copy)] struct WorkZoom{id:(u32,u32),before:Rect,after:Rect}
fn zoom_alive(h:H,z:WorkZoom)->bool{unsafe{IsWindow(h)!=0&&identity(h)==z.id}}
fn zoom_restore_visible(r:Rect)->bool{unsafe{MonitorFromRect(&r,0)!=0}}
fn zoom_matches(h:H,z:WorkZoom)->bool{zoom_alive(h,z)&&unsafe{IsIconic(h)==0&&IsZoomed(h)==0}&&window_rect(h).map_or(false,|r|near_rect(r,z.after,3))}
// Classic consoles can round the requested size down to whole character cells.
// Accept a small shortfall, never allow the visible frame to enter the reserved top bar.
fn zoom_fits(actual:Rect,work:Rect,console:bool,dpi:u32)->bool{
 if !console{return near_rect(actual,work,3);}
 let slack=scale(48,dpi);
 actual.right>actual.left&&actual.bottom>actual.top&&
 actual.top>=work.top&&actual.top-work.top<=3&&
 actual.left>=work.left-3&&actual.left-work.left<=3&&
 actual.right<=work.right+3&&work.right-actual.right<=slack&&
 actual.bottom<=work.bottom+3&&work.bottom-actual.bottom<=slack
}
fn custom_zoom_native_fallback(office:bool,workzoom:bool,reserved_top:bool,already_maximized:bool)->bool{
 !office||!workzoom||!reserved_top||already_maximized
}
fn toggle_work_zoom(d:&Dwm,h:H,zooms:&mut HashMap<H,WorkZoom>,legacy_copy:bool,console:bool)->bool{
 if let Some(z)=zooms.remove(&h){
  if zoom_matches(h,z)&&zoom_restore_visible(z.before){
   let ok=position_normal(h,z.before,legacy_copy);log(&format!("workzoom_restore hwnd={h:#x} ok={ok}"));
   if !ok{zooms.insert(h,z);}
   return true;
  }
  log(&format!("workzoom_discard hwnd={h:#x} reason=geometry_changed"));
 }
 if unsafe{IsZoomed(h)!=0||IsIconic(h)!=0}{return false;}
 let Some(info)=monitor_info(h) else{return false;};
 if info.work.top<=info.monitor.top{return false;}
 let (Some(before),Some(visible))=(window_rect(h),visible_rect(d,h)) else{return false;};
 let Some(request)=zoom_rect(before,visible,info.work) else{return false;};
 if near_rect(before,request,3){return false;}
 let id=identity(h);
 if !position_normal(h,request,legacy_copy){log("workzoom_position_failed");return true;}
 // Check the actual visible frame instead of assuming SetWindowPos succeeded geometrically.
 if let (Some(after),Some(actual))=(window_rect(h),visible_rect(d,h)){
  if zoom_fits(actual,info.work,console,unsafe{GetDpiForWindow(h)}.max(96))&&unsafe{IsZoomed(h)==0&&IsIconic(h)==0}{
   zooms.insert(h,WorkZoom{id,before,after});
   log(&format!("workzoom_enter hwnd={h:#x} before={before:?} after={after:?} visible={actual:?} work={:?}",info.work));
   return true;
  }
 }
 let ok=position_normal(h,before,legacy_copy);log(&format!("workzoom_rejected hwnd={h:#x} rollback={ok} console={console}"));
 // If a console refuses the requested size, permit native maximize after a successful rollback.
 // The normal native-caption guard will leave stock buttons in place if the caption is obscured.
 if console&&ok{log("console_zoom_native_fallback");return false;}true
}
#[cfg(test)] mod workzoom_tests{use super::*;
 #[test] fn resize_discards_old_pixels(){assert_eq!(zoom_flags(false),0x0314);assert_eq!(zoom_flags(true),0x0214);}
 #[test] fn finder_workarea(){let outer=Rect{left:93,top:100,right:1107,bottom:807};let visible=Rect{left:100,top:100,right:1100,bottom:800};let work=Rect{left:0,top:24,right:1920,bottom:1080};assert_eq!(zoom_rect(outer,visible,work),Some(Rect{left:-7,top:24,right:1927,bottom:1087}));}
 #[test] fn invalid_margins(){let r=Rect{left:0,top:0,right:800,bottom:600};assert!(zoom_rect(r,Rect{left:-1,..r},r).is_none());}
 #[test] fn console_cell_rounding(){let w=Rect{left:0,top:24,right:1920,bottom:1080};let r=Rect{right:1912,bottom:1063,..w};assert!(zoom_fits(r,w,true,96));assert!(!zoom_fits(r,w,false,96));assert!(!zoom_fits(Rect{top:23,..r},w,true,96));assert!(!zoom_fits(Rect{bottom:1000,..r},w,true,96));}
 #[test] fn office_reserved_top_blocks_native_fallback(){assert!(!custom_zoom_native_fallback(true,true,true,false));assert!(custom_zoom_native_fallback(true,true,true,true));assert!(custom_zoom_native_fallback(true,true,false,false));assert!(custom_zoom_native_fallback(true,false,true,false));assert!(custom_zoom_native_fallback(false,true,true,false));}
 #[test] fn changed_geometry(){let r=Rect{left:0,top:24,right:1920,bottom:1080};assert!(near_rect(r,Rect{top:26,..r},3));assert!(!near_rect(r,Rect{top:28,..r},3));}
}
#[derive(Clone,Copy,PartialEq,Eq,Debug)] struct Layout{pos:Point,w:i32,h:i32,dpi:u32,work_top:Option<i32>,fallback_row:bool,left_slot:bool}
fn monitor_info(h:H)->Option<MonitorInfo>{unsafe{
 let m=MonitorFromWindow(h,2); if m==0{return None;}
 let mut info=MonitorInfo{size:size_of::<MonitorInfo>() as u32,monitor:Rect::default(),work:Rect::default(),flags:0};
 if GetMonitorInfoW(m,&mut info)==0||info.work.right<=info.work.left||info.work.bottom<=info.work.top{return None;}
 Some(info)
}}
#[derive(Clone,Copy,Debug,PartialEq,Eq)] struct WindowEnvironment{monitor:H,bounds:Rect,work:Rect,dpi:u32}
fn window_environment(h:H)->Option<WindowEnvironment>{unsafe{
 let monitor=MonitorFromWindow(h,2);if monitor==0{return None;}
 let info=monitor_info(h)?;
 Some(WindowEnvironment{monitor,bounds:info.monitor,work:info.work,dpi:GetDpiForWindow(h).max(96)})
}}
fn environment_changed(old:Option<WindowEnvironment>,new:Option<WindowEnvironment>)->bool{old.is_some()&&old!=new}
// If the actual caption is under a reserved top bar, use a compact fallback
// slot in the visible upper row, leaving space for Ribbon help/collapse controls.
// Never change the target window's bounds or the system work area.
fn fit_maximized(r:Rect,monitor:Rect,work:Rect,desired:Point,w:i32,height:i32,dpi:u32)->Option<(Point,bool)>{
 let left=r.left.max(work.left); let right=r.right.min(work.right);
 let top=r.top.max(work.top); let bottom=r.bottom.min(work.bottom);
 let fallback=work.top>monitor.top&&desired.y<work.top;
 let reserve=if fallback{scale(48,dpi)}else{0};
 let min_x=left+scale(120,dpi); let max_x=right-w-reserve;
 let min_y=top+if fallback{scale(2,dpi)}else{0}; let max_y=bottom-height;
 if min_x>max_x||min_y>max_y{return None;}
 Some((Point{x:desired.x.clamp(min_x,max_x),y:desired.y.clamp(min_y,max_y)},fallback))
}
fn layout(d:&Dwm,h:H,right_gap:i32,top_gap:i32,base_dpi:u32)->Option<Layout>{
 let r=visible_rect(d,h)?;let dpi=unsafe{GetDpiForWindow(h)}.max(96);let w=scale(66,dpi);let height=scale(22,dpi);
 let desired=Point{x:r.right-right_gap*dpi as i32/base_dpi as i32-w,y:r.top+top_gap*dpi as i32/base_dpi as i32};
 let mut pos=desired; let mut work_top=None; let mut fallback_row=false;
 if unsafe{IsZoomed(h)}!=0{
  let info=monitor_info(h)?; work_top=Some(info.work.top);
  (pos,fallback_row)=fit_maximized(r,info.monitor,info.work,desired,w,height,dpi)?;
 }
 if pos.x<r.left+scale(120,dpi)||pos.y<r.top-2||pos.y+height>r.bottom||pos.x+w>r.right{return None;}
 Some(Layout{pos,w,h:height,dpi,work_top,fallback_row,left_slot:false})
}
#[cfg(test)] mod geometry_tests{
 use super::*;
 #[test] fn finder_reserved_top(){let monitor=Rect{left:0,top:0,right:1920,bottom:1080};let work=Rect{top:24,..monitor};let r=Rect{top:-8,..monitor};let (p,f)=fit_maximized(r,monitor,work,Point{x:1846,y:-4},66,22,96).unwrap();assert_eq!(p,Point{x:1806,y:26});assert!(f);}
 #[test] fn ordinary_maximized(){let monitor=Rect{left:0,top:0,right:1920,bottom:1080};let (p,f)=fit_maximized(monitor,monitor,monitor,Point{x:1846,y:4},66,22,96).unwrap();assert_eq!(p,Point{x:1846,y:4});assert!(!f);}
 #[test] fn negative_monitor_coordinates(){let monitor=Rect{left:-1920,top:-100,right:0,bottom:980};let work=Rect{top:-76,..monitor};let (p,f)=fit_maximized(monitor,monitor,work,Point{x:-74,y:-96},66,22,96).unwrap();assert_eq!(p,Point{x:-114,y:-74});assert!(f);}
 #[test] fn mac_center_and_left(){let r=Rect{left:100,top:50,right:1100,bottom:700};let l=Layout{pos:Point{x:1026,y:54},w:66,h:22,dpi:96,work_top:None,fallback_row:false,left_slot:false};let (buttons,title)=mac_slots(r,l).unwrap();assert_eq!(buttons.pos.x,108);assert_eq!(title.pos.x+title.w/2,600);}
 #[test] fn mac_narrow_rejected(){let r=Rect{left:0,top:0,right:220,bottom:300};let l=Layout{pos:Point{x:150,y:4},w:66,h:22,dpi:96,work_top:None,fallback_row:false,left_slot:false};assert!(mac_slots(r,l).is_none());}
 #[test] fn mac_maximized_keeps_left_and_center(){for dpi in [96,120,144,192]{let r=Rect{left:-1920,top:24,right:0,bottom:1080};let l=Layout{pos:Point{x:-scale(74,dpi),y:24+scale(4,dpi)},w:scale(66,dpi),h:scale(22,dpi),dpi,work_top:Some(24),fallback_row:false,left_slot:false};let (b,t)=mac_slots(r,l).unwrap();assert_eq!(b.pos.x,r.left+scale(8,dpi));assert_eq!(t.pos.x+t.w/2,-960);assert!(mac_slots(r,Layout{pos:Point{y:23,..l.pos},..l}).is_none());}}
 #[test] fn mac_finder_fallback_rejected(){let r=Rect{left:0,top:0,right:1920,bottom:1080};let l=Layout{pos:Point{x:1806,y:26},w:66,h:22,dpi:96,work_top:Some(24),fallback_row:true,left_slot:false};assert!(mac_slots(r,l).is_none());}
 #[test] fn no_space(){let r=Rect{left:0,top:0,right:100,bottom:30};assert!(fit_maximized(r,r,r,Point{x:0,y:0},66,22,96).is_none());}
}
// Use the same Mac layout for normal and maximized frames, unless the actual
// title row is obscured by a reserved work-area bar.
fn mac_slots(r:Rect,right:Layout)->Option<(Layout,Layout)>{
 if right.fallback_row||right.work_top.is_some_and(|top|right.pos.y<top){return None;}
 let margin=scale(LEFT_MARGIN_DIP,right.dpi);let reserve=scale(90,right.dpi);
 let width=(r.right-r.left-2*reserve).min(scale(640,right.dpi));
 if width<scale(100,right.dpi)||right.pos.y<r.top||right.pos.y+right.h>r.bottom{return None;}
 if right.pos.x+right.w>r.right{return None;}
 let buttons=Layout{pos:Point{x:r.left+margin,y:right.pos.y},left_slot:true,..right};
 let title=Layout{pos:Point{x:r.left+(r.right-r.left-width)/2,y:right.pos.y},w:width,left_slot:true,..right};
 if buttons.pos.x+buttons.w+scale(12,right.dpi)>title.pos.x{return None;}
 Some((buttons,title))
}
#[repr(C)] struct ThemeOptions{flags:u32,mask:u32}
// The documented theme flags request suppression of native caption text and system icon, not the HWND name or QAT.
// Windows has no public getter for these flags. Opt-in assumes the original caption is drawn.
struct ThemeGuard{h:H,id:(u32,u32),module:H,set:unsafe extern "system" fn(H,u32,*mut ThemeOptions,u32)->i32}
impl ThemeGuard{
 fn enable(h:H)->Result<Self,String>{unsafe{
  let module=LoadLibraryExW(wide("uxtheme.dll").as_ptr(),0,0x800);
  if module==0{return Err("uxtheme unavailable".into());}
  let p=GetProcAddress(module,b"SetWindowThemeAttribute\0".as_ptr());
  if p.is_null(){FreeLibrary(module);return Err("SetWindowThemeAttribute unavailable".into());}
  let set:unsafe extern "system" fn(H,u32,*mut ThemeOptions,u32)->i32=std::mem::transmute(p);
  let mut opt=ThemeOptions{flags:THEME_HIDE,mask:THEME_HIDE};
  let hr=set(h,1,&mut opt,size_of::<ThemeOptions>() as u32);
  log(&format!("title_hide_request hwnd={h:#x} mask=0x03 hr=0x{:08x}",hr as u32));
  if hr!=0{FreeLibrary(module);return Err(format!("theme flag failed 0x{:08x}",hr as u32));}
  RedrawWindow(h,null(),0,0x0481);
  Ok(Self{h,id:identity(h),module,set})
 }}
 fn reapply(&self)->bool{unsafe{
  if IsWindow(self.h)==0||identity(self.h)!=self.id{return false;}
  let mut opt=ThemeOptions{flags:THEME_HIDE,mask:THEME_HIDE};
  let hr=(self.set)(self.h,1,&mut opt,size_of::<ThemeOptions>() as u32);
  log(&format!("theme_reapply hwnd={:#x} mask=0x03 hr=0x{:08x}",self.h,hr as u32));
  if hr!=0{return false;}
  RedrawWindow(self.h,null(),0,0x0481);
  true
 }}
}
impl Drop for ThemeGuard{fn drop(&mut self){unsafe{
 if IsWindow(self.h)!=0&&identity(self.h)==self.id{
  let mut opt=ThemeOptions{flags:0,mask:THEME_HIDE};
  let hr=(self.set)(self.h,1,&mut opt,size_of::<ThemeOptions>() as u32);
  RedrawWindow(self.h,null(),0,0x0481);
  log(&format!("title_restore hwnd={:#x} hr=0x{:08x}",self.h,hr as u32));
 }
 FreeLibrary(self.module);
}}}
// Borrow the short-lived title from a caller-owned stack buffer. Keep the
// original length and truncation guards; only persist text that is needed.
fn read_title_buf(h:H,buf:&mut [u16;1024])->Option<&[u16]>{unsafe{
 let len=GetWindowTextLengthW(h);if len<=0||len>=1023{return None;}
 let n=GetWindowTextW(h,buf.as_mut_ptr(),1024);
 if n<=0||n>=1023{None}else{Some(&buf[..n as usize])}
}}
fn read_title(h:H)->Option<Vec<u16>>{
 let mut buf=[0u16;1024];read_title_buf(h,&mut buf).map(<[u16]>::to_vec)
}
// Read only three pixels in the unused title-band gap, never console contents.
// If the gap is too small or another window covers it, honor the existing color preference.
fn console_title_dark(h:H,left:Layout,center:Layout,fallback:bool)->bool{unsafe{
 let start=left.pos.x+left.w+scale(4,left.dpi);
 let end=center.pos.x-scale(4,left.dpi);
 if end-start<scale(6,left.dpi){return fallback;}
 let y=left.pos.y+left.h/2;
 let dc=GetDC(0);if dc==0{return fallback;}
 let mut sum=0u32;let mut n=0u32;
 for i in 1..=3{let p=Point{x:start+(end-start)*i/4,y};
  if GetAncestor(WindowFromPoint(p),2)!=h{continue;}
  let v=GetPixel(dc,p.x,p.y);if v==0xffffffff{continue;}
  sum+=((v&255)*299+((v>>8)&255)*587+((v>>16)&255)*114)/1000;n+=1;
 }
 ReleaseDC(0,dc);if n==0{fallback}else{sum/n>=145}
}}
// Draw ONLY the title glyphs into an alpha surface; no desktop pixels are read.
fn present_title(h:H,l:Layout,title:&[u16],dark_text:bool,fill:Option<u32>,text_layout:Layout)->bool{unsafe{
 let dc=CreateCompatibleDC(0);if dc==0{return false;}
 let info=BitmapInfo{size:40,width:l.w,height:-l.h,planes:1,bits:32,compression:0,image_size:0,xppm:0,yppm:0,used:0,important:0,colors:0};
 let mut bits=null_mut();let bmp=CreateDIBSection(dc,&info,0,&mut bits,0,0);
 let mut ok=false;
 if bmp!=0&&!bits.is_null(){
  let old=SelectObject(dc,bmp);
  if old!=0&&old != -1{
   let face=wide("Segoe UI");let font=CreateFontW(-scale(12,l.dpi),0,0,0,500,0,0,0,1,0,0,4,0,face.as_ptr());
   if font!=0{
    let old_font=SelectObject(dc,font);
    if old_font!=0&&old_font != -1{
     let n=l.w as usize*l.h as usize;std::ptr::write_bytes(bits.cast::<u32>(),0,n);
     let pixels=std::slice::from_raw_parts_mut(bits.cast::<u32>(),n);
     if let Some(color)=fill{pixels.fill(color);}
     SetTextColor(dc,if fill.is_some()&&dark_text{0x181818}else if fill.is_some(){0xf0f0f0}else{0xffffff});SetBkMode(dc,1);
     let x=text_layout.pos.x-l.pos.x;let y=text_layout.pos.y-l.pos.y;
     let mut r=Rect{left:x,top:y,right:x+text_layout.w,bottom:y+text_layout.h};
     if DrawTextW(dc,title.as_ptr(),title.len() as i32,&mut r,0x8825)>0&&GdiFlush()!=0{
      let pixels=std::slice::from_raw_parts_mut(bits.cast::<u32>(),n);
      if fill.is_some(){for v in pixels.iter_mut(){*v|=0xff000000;}}
      else{for v in pixels.iter_mut(){let a=((*v>>16)&255).max((*v>>8)&255).max(*v&255);let c=if dark_text{a*24/255}else{a*240/255};*v=(a<<24)|(c<<16)|(c<<8)|c;}}
      let size=Point{x:l.w,y:l.h};let origin=Point{x:0,y:0};let blend=Blend{operation:0,flags:0,alpha:255,format:1};
      ok=UpdateLayeredWindow(h,0,&l.pos,&size,dc,&origin,0,&blend,2)!=0;
     }
     SelectObject(dc,old_font);
    }
    DeleteObject(font);
   }
   SelectObject(dc,old);
  }
 }
 if bmp!=0{DeleteObject(bmp);}DeleteDC(dc);ok
}}
fn glyph(i:usize,dx:f64,dy:f64)->bool{
 // Local coordinates in 96-DPI logical pixels. Draw only inside the circle.
 match i{
  0=>dx.abs()<=2.7&&dy.abs()<=2.7&&((dx-dy).abs()<0.65||(dx+dy).abs()<0.65),
  1=>dx.abs()<=3.0&&dy.abs()<0.65,
  2=>(dx.abs()<=3.0&&dy.abs()<0.65)||(dy.abs()<=3.0&&dx.abs()<0.65),
  _=>false,
 }
}
fn dot_origin(l:Layout)->(f64,f64){let f=l.dpi as f64/96.0;if l.w==scale(66,l.dpi)&&l.h==scale(22,l.dpi){(0.0,11.0*f)}else{((l.w as f64-66.0*f)/2.0,l.h as f64/2.0)}}
fn pixel_buffer(l:Layout,hover:bool)->Vec<u32>{
 let colors=[(255u32,95u32,87u32),(254,188,46),(40,200,64)];let mut buf=vec![0;l.w as usize*l.h as usize];
 let factor=l.dpi as f64/96.0;let (ox,cy)=dot_origin(l);let radius=6.0*factor;
 // Only visit the small bounding box of each circle; all other pixels stay zero.
 for (i,&(r,g,b)) in colors.iter().enumerate(){
  let cx=ox+(11.0+i as f64*22.0)*factor;
  let x0=((cx-radius).floor() as i32).max(0);let x1=((cx+radius).ceil() as i32).min(l.w);
  let y0=((cy-radius).floor() as i32).max(0);let y1=((cy+radius).ceil() as i32).min(l.h);
  for y in y0..y1{for x in x0..x1{let mut count=0u32;let mut ink=0u32;
   for sy in 0..4{for sx in 0..4{let dx=x as f64+(sx as f64+0.5)/4.0-cx;let dy=y as f64+(sy as f64+0.5)/4.0-cy;if dx*dx+dy*dy<=radius*radius{count+=1;if hover&&glyph(i,dx/factor,dy/factor){ink+=1;}}}}
   if count>0{let a=(count*255+8)/16;let channel=|c:u32|(c*(count-ink)+40*ink+8)/16;buf[(y*l.w+x) as usize]=(a<<24)|(channel(r)<<16)|(channel(g)<<8)|channel(b);}
  }}
 }
 buf
}
#[cfg(test)] mod pixel_regression{use super::*;
fn reference_pixels(l:Layout,hover:bool)->Vec<u32>{
 let colors=[(255u32,95u32,87u32),(254,188,46),(40,200,64)];let mut buf=vec![0;l.w as usize*l.h as usize];
 // Supersampled circles with premultiplied alpha; every background pixel is zero.
 let factor=l.dpi as f64/96.0;
 for y in 0..l.h{for x in 0..l.w{for (i,&(r,g,b)) in colors.iter().enumerate(){
  let (ox,cy)=dot_origin(l);let cx=ox+(11.0+i as f64*22.0)*factor;let radius=6.0*factor;let mut count=0u32;let mut ink=0u32;
  for sy in 0..4{for sx in 0..4{let dx=x as f64+(sx as f64+0.5)/4.0-cx;let dy=y as f64+(sy as f64+0.5)/4.0-cy;if dx*dx+dy*dy<=radius*radius{count+=1;if hover&&glyph(i,dx/factor,dy/factor){ink+=1;}}}}
  if count>0{let a=(count*255+8)/16;
   let channel=|c:u32| (c*(count-ink)+40*ink+8)/16;
   buf[(y*l.w+x) as usize]=(a<<24)|(channel(r)<<16)|(channel(g)<<8)|channel(b);break;}
 }}}
 buf
}
 #[test]fn bounding_boxes_match_original(){for dpi in [96,120,144,192,288,480]{for (w,h) in [(66,22),(137,40),(144,48)]{let l=Layout{pos:Point::default(),w:scale(w,dpi),h:scale(h,dpi),dpi,work_top:None,fallback_row:false,left_slot:false};for hover in [false,true]{assert_eq!(pixel_buffer(l,hover),reference_pixels(l,hover));}}}}
}
fn cursor_over(l:Layout)->bool{unsafe{
 let mut p=Point::default();if GetCursorPos(&mut p)==0{return false;}
 let f=l.dpi as f64/96.0;
 let x=(p.x-l.pos.x) as f64+0.5;let y=(p.y-l.pos.y) as f64+0.5;
 let (ox,cy)=dot_origin(l);
 (0..3).any(|i|{let dx=x-ox-(11.0+i as f64*22.0)*f;let dy=y-cy;dx*dx+dy*dy<=(7.0*f).powi(2)})
}}
fn present(h:H,l:Layout,hover:bool)->bool{present_pixels(h,l,&pixel_buffer(l,hover))}
fn present_pixels(h:H,l:Layout,pixels:&[u32])->bool{unsafe{
 if pixels.len()!=l.w as usize*l.h as usize{return false;}
 let screen=GetDC(0);if screen==0{return false;}
 let mem=CreateCompatibleDC(screen);if mem==0{ReleaseDC(0,screen);return false;}
 let info=BitmapInfo{size:40,width:l.w,height:-l.h,planes:1,bits:32,compression:0,image_size:0,xppm:0,yppm:0,used:0,important:0,colors:0};
 let mut bits=null_mut();let bmp=CreateDIBSection(screen,&info,0,&mut bits,0,0);let mut ok=false;
 if bmp!=0&&!bits.is_null(){let old=SelectObject(mem,bmp);if old!=0&&old != -1{std::ptr::copy_nonoverlapping(pixels.as_ptr(),bits.cast::<u32>(),pixels.len());let size=Point{x:l.w,y:l.h};let origin=Point{x:0,y:0};let blend=Blend{operation:0,flags:0,alpha:255,format:1};ok=UpdateLayeredWindow(h,screen,&l.pos,&size,mem,&origin,0,&blend,2)!=0;SelectObject(mem,old);}}
 if bmp!=0{DeleteObject(bmp);}DeleteDC(mem);ReleaseDC(0,screen);ok
}}
unsafe extern "system" fn wndproc(h:H,m:u32,w:W,l:L)->L{if release::taskbar_notification(h,m){return 0;}match m{
 0x0200=>{ // WM_MOUSEMOVE: wake once on entry, not for every pixel.
  use std::sync::atomic::Ordering;
  if HOVER_WINDOW.swap(h,Ordering::AcqRel)!=h{
   let mut track=TrackMouse{size:size_of::<TrackMouse>() as u32,flags:2,hwnd:h,hover_time:0};
   if TrackMouseEvent(&mut track)==0{HOVER_WINDOW.store(0,Ordering::Release);}
   wake_manager();
  }0
 },
 0x02a3=>{let _=HOVER_WINDOW.compare_exchange(h,0,std::sync::atomic::Ordering::AcqRel,std::sync::atomic::Ordering::Relaxed);wake_manager();0},
 0x007e|0x001a|0x031a|0x031e=>{ // display, settings/work-area, theme, composition
  if MANAGER_WAKE.load(std::sync::atomic::Ordering::Acquire)==h{signal(DIRTY_ENVIRONMENT|DIRTY_BACKGROUND);}
  DefWindowProcW(h,m,w,l)
 },
 0x02e0=>{signal(DIRTY_ENVIRONMENT|DIRTY_BACKGROUND|DIRTY_MOTION);0}, // owner geometry is authoritative, not suggested overlay rect
 0x0011=>{ // WM_QUERYENDSESSION: best effort graceful shutdown before logoff.
  if MANAGER_WAKE.load(std::sync::atomic::Ordering::Acquire)==h{signal(DIRTY_SHUTDOWN);}1
 },
 0x0016=>{if w!=0&&MANAGER_WAKE.load(std::sync::atomic::Ordering::Acquire)==h{signal(DIRTY_SHUTDOWN);}0},
 0x0021=>3, // MA_NOACTIVATE: keep Explorer focused
 0x0202=>0,
 0x0010=>0,
 0x0002=>{let _=HOVER_WINDOW.compare_exchange(h,0,std::sync::atomic::Ordering::AcqRel,std::sync::atomic::Ordering::Relaxed);0},
 _=>DefWindowProcW(h,m,w,l)
}}
struct Sprite(H);
impl Drop for Sprite{fn drop(&mut self){unsafe{if IsWindow(self.0)==0{return;}KillTimer(self.0,1);UnregisterHotKey(self.0,1);UnregisterHotKey(self.0,2);UnregisterHotKey(self.0,3);UnregisterHotKey(self.0,4);UnregisterHotKey(self.0,5);UnregisterHotKey(self.0,release::TOGGLE_ID);ShowWindow(self.0,0);DestroyWindow(self.0);}}}
struct Instance(H);
impl Drop for Instance{fn drop(&mut self){unsafe{CloseHandle(self.0);}}}
// Per-owner surfaces and restore guards; no shared global topmost layer.
// Strict allowlist: no modifications to unknown, custom-caption or excluded windows.
#[derive(Clone,Copy,Debug,PartialEq,Eq)] enum App{Explorer,Console,Notepad,WordPad,Paint,BrowserProbe,OfficeProbe,GenericProbe,Other}
impl App{
 fn native(self)->bool{matches!(self,Self::Console|Self::Notepad|Self::WordPad|Self::Paint)}
 fn supported(self,explorer_only:bool)->bool{self==Self::Explorer||(!explorer_only&&self.native())}
 fn probe(self)->bool{matches!(self,Self::BrowserProbe|Self::OfficeProbe|Self::GenericProbe)}
 fn retain_on_close(self)->bool{self!=Self::Other}
 // Explorer is deliberately separate from native() for layout rules, but
 // it still needs the centered-title surface, just like native apps.
 fn needs_title_surface(self,office_exact:bool)->bool{self.supported(false)||self==Self::GenericProbe||(self==Self::OfficeProbe&&office_exact)}
}
fn classify(class:&str,exe:&str)->App{
 match class{
  "CabinetWClass" if exe=="explorer.exe"=>App::Explorer,
  "ConsoleWindowClass" if matches!(exe,"conhost.exe"|"openconsole.exe"|"cmd.exe"|"powershell.exe"|"pwsh.exe")=>App::Console,
  "Notepad" if exe=="notepad.exe"=>App::Notepad,
  "WordPadClass" if exe=="wordpad.exe"=>App::WordPad,
  "MSPaintApp" if exe=="mspaint.exe"=>App::Paint,
  "Chrome_WidgetWin_1" if matches!(exe,"brave.exe"|"chrome.exe"|"msedge.exe")=>App::BrowserProbe,
  "OpusApp" if exe=="winword.exe"=>App::OfficeProbe,
  "XLMAIN" if exe=="excel.exe"=>App::OfficeProbe,
  "PPTFrameClass" if exe=="powerpnt.exe"=>App::OfficeProbe,
  "TaskManagerWindow" if exe=="taskmgr.exe"=>App::GenericProbe,
  "RegEdit_RegEdit" if exe=="regedit.exe"=>App::GenericProbe,
  "MMCMainFrame" if exe=="mmc.exe"=>App::GenericProbe,
  "Windows.UI.Core.CoreWindow"|"ApplicationFrameWindow" if exe=="systemsettings.exe"=>App::GenericProbe,
  _ if class!="#32770"&&matches!(exe,"simpmusic.exe"|"simp-music.exe")=>App::GenericProbe,
  _=>App::Other,
 }
}
fn supplemental_app(class:&str,exe:&str,settings_child:bool,title:&str)->App{
 if class=="ApplicationFrameWindow"&&exe=="applicationframehost.exe"&&settings_child{return App::GenericProbe;}
 if class=="SunAwtFrame"&&matches!(exe,"java.exe"|"javaw.exe")&&title.trim().eq_ignore_ascii_case("SimpMusic"){return App::GenericProbe;}
 App::Other
}
unsafe extern "system" fn settings_child_cb(h:H,data:L)->i32{
 if settings_exe(identity(h).1).is_some_and(|s|s=="systemsettings.exe"){
  *(data as *mut bool)=true;return 0;
 }
 1
}
fn has_settings_child(h:H)->bool{let mut found=false;unsafe{EnumChildWindows(h,Some(settings_child_cb),(&mut found as *mut bool) as L);}found}
fn settings_exe(pid:u32)->Option<String>{query_name(pid).ok().or_else(||snapshot_name(pid))}
// UIA may report the hosted Settings process rather than ApplicationFrameHost.
unsafe extern "system" fn settings_pid_cb(h:H,data:L)->i32{
 let state=&mut *(data as *mut (u32,bool));
 if identity(h).1==state.0{state.1=true;return 0;}1
}
fn verified_settings_uia_pid(h:H,pid:u32)->bool{
 if pid==0||!verified_settings_frame(h)||!settings_exe(pid).is_some_and(|s|s=="systemsettings.exe"){return false;}
 let mut state=(pid,false);unsafe{EnumChildWindows(h,Some(settings_pid_cb),(&mut state as *mut (u32,bool)) as L);};state.1
}
fn verified_settings_frame(h:H)->bool{
 let class=class_name(h);if !matches!(class.as_str(),"ApplicationFrameWindow"|"Windows.UI.Core.CoreWindow"){return false;}
 let exe=settings_exe(identity(h).1).unwrap_or_default();
 exe=="systemsettings.exe"||(class=="ApplicationFrameWindow"&&exe=="applicationframehost.exe"&&has_settings_child(h))
}
fn verified_owned_office_frame(h:H)->bool{
 let owner=unsafe{window_neighbor(h,4)};
 if owner==0{return false;}
 let pid=identity(h).1;
 pid!=0&&identity(owner).1==pid&&query_name(pid).is_ok_and(|exe|classify(&class_name(h),&exe)==App::OfficeProbe)
}
fn usable_window(h:H)->bool{unsafe{h!=0&&IsWindow(h)!=0&&IsWindowVisible(h)!=0&&IsIconic(h)==0}}
fn visibility_allowed(usable:bool,cloaked:bool)->bool{usable&&!cloaked}
fn owner_presentable(d:&Dwm,h:H)->bool{let usable=usable_window(h);visibility_allowed(usable,usable&&d.cloaked(h))}
fn class_name(h:H)->String{unsafe{let mut buf=[0u16;256];let n=GetClassNameW(h,buf.as_mut_ptr(),256);if n>0{String::from_utf16_lossy(&buf[..n as usize])}else{String::new()}}}
// A snapshot provides a basename if opening the console host for query fails.
// It does not grant access to the target or bypass Windows style permissions.
fn snapshot_name(pid:u32)->Option<String>{unsafe{
 let snapshot=CreateToolhelp32Snapshot(0x00000002,0);
 if snapshot== -1{return None;}
 let mut e:ProcessEntry=zeroed();e.size=size_of::<ProcessEntry>() as u32;
 let mut ok=Process32FirstW(snapshot,&mut e);let mut result=None;
 while ok!=0{
  if e.pid==pid{let n=e.exe.iter().position(|&c|c==0).unwrap_or(e.exe.len());result=Some(String::from_utf16_lossy(&e.exe[..n]).to_ascii_lowercase());break;}
  ok=Process32NextW(snapshot,&mut e);
 }
 CloseHandle(snapshot);result
}}
fn query_name(pid:u32)->Result<String,u32>{unsafe{
 if pid==0{return Err(87);}
 let process=OpenProcess(0x1000,0,pid);if process==0{return Err(GetLastError());}
 let mut buf=[0u16;1024];let mut n=buf.len() as u32;
 let ok=QueryFullProcessImageNameW(process,0,buf.as_mut_ptr(),&mut n)!=0;
 let err=if ok{0}else{GetLastError()};CloseHandle(process);
 if !ok{return Err(err);}if n as usize>buf.len(){return Err(122);}
 let path=String::from_utf16_lossy(&buf[..n as usize]);
 Ok(path.rsplit(['\\','/']).next().unwrap_or("").to_ascii_lowercase())
}}
fn inspect_app(d:&Dwm,h:H,explorer_only:bool,trace_focus:bool)->App{
 if !usable_window(h){return App::Other;}
 let class=class_name(h);
 let console=class.eq_ignore_ascii_case("ConsoleWindowClass");
 // Unknown toolkit classes may belong to the explicitly named SimpMusic executable.
 // Inspect once per foreground change, never admit unknown processes by class alone.
 let (_,pid)=identity(h);
 let (exe,source,error)=match query_name(pid){
  Ok(name)=>(name,"query",0),
  Err(e) if console||matches!(class.as_str(),"OpusApp"|"XLMAIN"|"PPTFrameClass"|"ApplicationFrameWindow"|"Windows.UI.Core.CoreWindow")=>match snapshot_name(pid){Some(name)=>(name,"snapshot",e),None=>(String::new(),"unavailable",e)},
  Err(e)=>(String::new(),"unavailable",e),
 };
 let mut app=classify(if console{"ConsoleWindowClass"}else{class.as_str()},&exe);
 if app==App::Other{
  let settings=class=="ApplicationFrameWindow"&&exe=="applicationframehost.exe"&&has_settings_child(h);
  let title=if class=="SunAwtFrame"&&matches!(exe.as_str(),"java.exe"|"javaw.exe"){read_title(h).map(|t|String::from_utf16_lossy(&t)).unwrap_or_default()}else{String::new()};
  app=supplemental_app(&class,&exe,settings,&title);
 }
 if app==App::GenericProbe&&!settings_owner_allowed(matches!(class.as_str(),"ApplicationFrameWindow"|"Windows.UI.Core.CoreWindow")&&verified_settings_frame(h),unsafe{GetAncestor(h,2)==h},unsafe{window_neighbor(h,4)!=0}){app=App::Other;}
 if release::is_excluded(&exe,app,&class){log(&format!("config_excluded hwnd={h:#x} exe={exe}"));return App::Other;}
 if console{log(&format!("console_detect hwnd={h:#x} pid={pid} class={class} exe={exe:?} source={source} query_error={error} app={app:?}"));}
 if app!=App::Other||console||trace_focus{
  log(&format!("app_detect hwnd={h:#x} class={class} exe={exe} app={app:?} style={:#010x} bounds={:?} action={}",style(h),d.rect(h,5),if app.supported(explorer_only){"native_try"}else if app.probe(){"custom_candidate"}else{"excluded"}));
 }
 app
}
// Non-Explorer native apps never use Explorer's Ribbon fallback row or left-title experiment.
fn native_slot_ok(l:Layout)->bool{!l.fallback_row}
#[cfg(test)] mod app_tests{
 use super::*;
 #[test] fn native_allowlist(){assert_eq!(classify("CabinetWClass","explorer.exe"),App::Explorer);assert_eq!(classify("ConsoleWindowClass","conhost.exe"),App::Console);assert_eq!(classify("Notepad","notepad.exe"),App::Notepad);assert_eq!(classify("WordPadClass","wordpad.exe"),App::WordPad);assert_eq!(classify("MSPaintApp","mspaint.exe"),App::Paint);}
 #[test] fn console_hosts(){for exe in ["conhost.exe","openconsole.exe","cmd.exe","powershell.exe","pwsh.exe"]{assert_eq!(classify("ConsoleWindowClass",exe),App::Console);}assert_eq!(classify("CASCADIA_HOSTING_WINDOW_CLASS","windowsterminal.exe"),App::Other);assert_eq!(classify("ConsoleWindowClass","other.exe"),App::Other);}
 #[test] fn process_entry_size_x64(){if size_of::<usize>()==8{assert_eq!(size_of::<ProcessEntry>(),568);}}
 #[test] fn custom_read_only(){assert_eq!(classify("Chrome_WidgetWin_1","brave.exe"),App::BrowserProbe);assert_eq!(classify("XLMAIN","excel.exe"),App::OfficeProbe);assert!(!App::BrowserProbe.supported(false));assert!(!App::OfficeProbe.supported(false));}
 #[test] fn unknown_excluded(){assert_eq!(classify("SunAwtFrame","studio64.exe"),App::Other);assert_eq!(classify("Notepad","other.exe"),App::Other);assert_eq!(classify("ConsoleWindowClass",""),App::Other);assert!(!App::Console.supported(true));assert!(App::Explorer.supported(true));}
 #[test] fn browser_close_retains_session(){assert!(App::BrowserProbe.retain_on_close());assert!(App::GenericProbe.retain_on_close());assert!(App::OfficeProbe.retain_on_close());assert!(App::Explorer.retain_on_close());assert!(App::Notepad.retain_on_close());assert!(!App::Other.retain_on_close());}
 #[test] fn supplemental_allowlist(){
  assert_eq!(classify("TaskManagerWindow","taskmgr.exe"),App::GenericProbe);
  assert_eq!(classify("RegEdit_RegEdit","regedit.exe"),App::GenericProbe);
  assert_eq!(classify("MMCMainFrame","mmc.exe"),App::GenericProbe);
  assert_eq!(classify("SunAwtFrame","simpmusic.exe"),App::GenericProbe);
  assert_eq!(classify("#32770","simpmusic.exe"),App::Other);
  assert_eq!(supplemental_app("ApplicationFrameWindow","applicationframehost.exe",true,""),App::GenericProbe);
  assert_eq!(supplemental_app("ApplicationFrameWindow","applicationframehost.exe",false,"Settings"),App::Other);
  assert_eq!(supplemental_app("SunAwtFrame","javaw.exe",false,"SimpMusic"),App::GenericProbe);
  assert_eq!(supplemental_app("SunAwtFrame","javaw.exe",false,"Android Studio"),App::Other);
  assert!(!App::GenericProbe.supported(false));assert!(App::GenericProbe.probe());assert!(App::GenericProbe.needs_title_surface(true));
 }
 #[test] fn reject_ribbon_fallback(){let l=Layout{pos:Point{x:0,y:0},w:66,h:22,dpi:96,work_top:Some(24),fallback_row:true,left_slot:false};assert!(!native_slot_ok(l));assert!(native_slot_ok(Layout{fallback_row:false,..l}));}
}
struct Target{
 app:App,
 theme:Option<ThemeGuard>,title_cache:Option<(Layout,Vec<u16>)>,theme_failed:bool,
 title_dark:bool,explorer_fill:Option<u32>,band_attempt:Option<(Layout,bool)>,
 guard:Restore,gap:i32,top:i32,dpi:u32,last:Layout,shown:bool,
 last_zoom:bool,refresh:bool,misses:u32,miss_since:Option<Instant>,hover:bool,
 ribbon_margin_preferred:bool,frame_extent:Option<(i32,i32)>,theme_due:Option<Instant>,zoom_repaint_due:Option<Instant>,ribbon_due:Option<Instant>,ribbon_check:Instant,ribbon_alt_key:Option<(Rect,Vec<u16>)>,ribbon_alt:Option<Layout>,ribbon_alt_due:Option<Instant>,ribbon_scan:custom_caption::RibbonScan,
}
impl Target{
 fn attach(d:&Dwm,h:H,sprite:H,app:App)->Result<Self,String>{unsafe{
  if !owner_presentable(d,h)||GetForegroundWindow()!=h||!(app==App::Explorer||app.native()){return Err("Not an active supported window".into());}
  let old=style(h);
  if old&WS_CAPTION!=WS_CAPTION||old&BUTTON_BITS!=BUTTON_BITS{return Err("Unsupported caption style or another caption mod active".into());}
  let wr=window_rect(h).ok_or("No window rectangle")?;
  let vr=visible_rect(d,h).ok_or("No visible frame")?;
  let b=d.rect(h,5).ok_or("No original caption bounds")?;
  let dpi=GetDpiForWindow(h).max(96);
  if b.right-b.left<scale(66,dpi)||b.bottom-b.top<scale(18,dpi){return Err("Original caption bounds not usable".into());}
  let gap=(vr.right-(wr.left+b.right)).max(0)+scale(8,dpi);
  let top=(wr.top+b.top-vr.top).max(0)+(b.bottom-b.top-scale(22,dpi)).max(0)/2;
  let initial=layout(d,h,gap,top,dpi).ok_or("Invalid initial slot")?;
  if app.native()&&!native_slot_ok(initial){return Err("Native caption obscured by work area; leave stock".into());}
  // Prepare the sprite before altering the target; it remains hidden.
  if !present(sprite,initial,false){return Err("Initial alpha sprite failed".into());}
  if GetForegroundWindow()!=h||!owner_presentable(d,h){return Err("Focus changed during attach".into());}
  let guard=Restore{h,id:identity(h),old};
  if !change(h,old&!BUTTON_BITS){return Err("Style change failed".into());}
  let hidden=d.rect(h,5).ok_or("Cannot verify stock button bounds")?;
  if style(h)&WS_CAPTION!=WS_CAPTION||hidden.right!=hidden.left{return Err("Native caption preservation or stock removal failed".into());}
  log(&format!("attach hwnd={h:#x} app={app:?} caption=true stock_width=0 original_bounds={b:?} gap={gap} top={top} maximized={}",IsZoomed(h)!=0));
  Ok(Self{app,theme:None,title_cache:None,theme_failed:false,title_dark:false,explorer_fill:if app==App::Explorer{explorer_caption::choose_fill(h)}else{None},band_attempt:None,guard,gap,top,dpi,last:initial,shown:false,last_zoom:IsZoomed(h)!=0,refresh:true,misses:0,miss_since:None,hover:false,ribbon_margin_preferred:false,frame_extent:None,theme_due:None,zoom_repaint_due:None,ribbon_due:None,ribbon_check:Instant::now(),ribbon_alt_key:None,ribbon_alt:None,ribbon_alt_due:None,ribbon_scan:custom_caption::RibbonScan::default()})
 }}
 fn needs_fast_tick(&self)->bool{!self.shown||self.refresh||self.misses>0||self.theme_due.is_some()||self.zoom_repaint_due.is_some()||self.ribbon_due.is_some()||self.ribbon_alt_due.is_some()}
 fn tick(&mut self,d:&Dwm,sprite:H,title_sprite:H,mode:u8,dark_text:bool, safe_zoom:bool,console_left:bool,native_left:bool)->Result<(),String>{unsafe{
  let h=self.guard.h;
  if !self.guard.alive()||!owner_presentable(d,h)||GetForegroundWindow()!=h{return Err("Target no longer active".into());}
  let caption_style=style(h)&(WS_CAPTION|BUTTON_BITS);
  if caption_style!=WS_CAPTION{return Err((if caption_style==(WS_CAPTION|BUTTON_BITS){"Stock buttons returned after state change"}else{"Application changed caption style"}).into());}
  let zoom=IsZoomed(h)!=0;let was_zoom=self.last_zoom;
  if zoom!=self.last_zoom{log(&format!("state hwnd={h:#x} maximized={zoom}"));self.last_zoom=zoom;self.refresh=true;}
  let mut next=layout(d,h,self.gap,self.top,self.dpi).ok_or("Caption slot became invalid")?;
  if self.app.native(){
   if !native_slot_ok(next){return Err("Native caption obscured; restore stock".into());}
   if zoom!=was_zoom {let b=d.rect(h,5).ok_or("Native bounds unavailable after state change")?;if b.right!=b.left{return Err("Stock buttons returned after state change".into());}}
  }
  let new_native=native_left&&matches!(self.app,App::Notepad|App::Paint|App::WordPad);
  let ribbon=matches!(self.app,App::Paint|App::WordPad);
  let mode=if self.app==App::Explorer||(self.app==App::Console&&console_left)||new_native{mode}else{0};
  let frame=visible_rect(d,h);
  let extent=frame.map(|r|(r.right-r.left,r.bottom-r.top));
  if extent!=self.frame_extent{
   if self.frame_extent.is_some(){self.theme_failed=false;}
   if ribbon&&self.frame_extent.is_some(){self.ribbon_due=None;self.theme_failed=false;self.ribbon_alt_key=None;self.ribbon_alt=None;self.ribbon_alt_due=None;}
   if self.frame_extent.is_some()&&self.theme.is_some(){
    if ribbon{self.theme_failed=false;self.title_cache=None;self.theme_due=None;self.ribbon_due=None;log("native_left_recheck reason=ribbon_geometry_changed_keep_theme");}
    else if self.app!=App::Console{self.theme_due=Some(Instant::now()+FRAME_SETTLE);}
   }
   if self.app==App::Console&&self.frame_extent.is_some(){self.zoom_repaint_due=Some(Instant::now()+FRAME_SETTLE);}
   self.frame_extent=extent;
  }
  let slot_frame=if zoom{frame.zip(monitor_info(h)).map(|(r,m)|Rect{left:r.left.max(m.work.left),top:r.top.max(m.work.top),right:r.right.min(m.work.right),bottom:r.bottom.min(m.work.bottom)})}else{frame};
  let slots=if safe_zoom{None}else{slot_frame.and_then(|r|mac_slots(r,next).and_then(|(mut left,mut center)|{
   if ribbon{left.pos.x=ribbon_left_x(r.left,next.dpi,self.ribbon_margin_preferred);left.pos.y+=scale(4,next.dpi);center.pos.y+=scale(4,next.dpi);
    // The local DIB verifier accepts at most 330 physical pixels.
    // Do not feed it a generic 640-DIP title box, which could never pass.
    // Bound the centered title to its estimated display width; DrawText
    // already ellipsizes longer titles inside this rectangle.
    let chars=read_title(h)?.len().min(1023) as i32;
    center.w=center.w.min(scale((chars*12+24).clamp(100,320),next.dpi)).min(330);
    center.pos.x=r.left+(r.width()-center.w)/2;
    if left.pos.x+left.w+scale(12,next.dpi)>center.pos.x||center.pos.y+center.h>r.bottom{return None;}
   }
   Some((left,center))
  }))};
  // Mode 1 = visual title-only probe. Mode 2 = explicit confirmation by the user.
  if mode!=0&&slots.is_some()&&!self.theme_failed&&self.ribbon_due.map_or(true,|due|Instant::now()>=due){
   self.ribbon_due=None;
   if self.theme.is_none(){match ThemeGuard::enable(h){Ok(g)=>{self.theme=Some(g);self.refresh=true;log("title_probe_ready visually_verify_original_text_hidden");},Err(e)=>{self.theme_failed=true;log(&format!("title_probe_fallback {e}"));}}}
   if self.theme_due.map_or(false,|due|Instant::now()>=due){
    self.theme_due=None;
    if let Some(g)=self.theme.as_ref(){
     if g.reapply(){self.refresh=true;self.title_cache=None;}
     else{self.theme.take();self.theme_failed=true;}
    }
   }
   if mode==2&&self.theme.is_some(){
    if let (Some((mut left,center)),Some(text))=(slots,read_title(h)){
     let ribbon_clear=if !ribbon{true}else if self.title_cache.is_none(){
      self.ribbon_check=Instant::now();
      // During a geometry recheck our previous dots must not contaminate
      // the ownership/color probe. Re-present the same session after verification.
      let same_left=self.shown&&self.last==left&&IsWindowVisible(sprite)!=0;
      if !same_left&&IsWindowVisible(sprite)!=0{ShowWindow(sprite,0);self.shown=false;self.refresh=true;}
      if IsWindowVisible(title_sprite)!=0{ShowWindow(title_sprite,0);}
      let mut left_ok=if same_left{custom_caption::caption_hits(h,left)}else{custom_caption::blank_caption(h,left)};
      if !left_ok&&!self.ribbon_margin_preferred&&GetForegroundWindow()==h{
       if let Some(r)=slot_frame{
        if IsWindowVisible(sprite)!=0{ShowWindow(sprite,0);self.shown=false;self.refresh=true;}
        let alternate=Layout{pos:Point{x:ribbon_left_x(r.left,next.dpi,true),y:left.pos.y},..left};
        if custom_caption::blank_caption(h,alternate){left=alternate;left_ok=true;self.ribbon_margin_preferred=true;log(&format!("native_left_margin_fallback hwnd={h:#x} preferred_dip=8 alternate_dip=13"));}
       }
      }
      let center_ok=left_ok&&custom_caption::blank_caption(h,center);
      log(&format!("native_left_exact_probe hwnd={h:#x} left_ok={left_ok} center_ok={center_ok} preferred_margin={} left={left:?} center={center:?}",self.ribbon_margin_preferred));
      left_ok&&center_ok
     }else if self.ribbon_check.elapsed()>=Duration::from_millis(400){self.ribbon_check=Instant::now();custom_caption::caption_hits(h,left)&&custom_caption::caption_hits(h,center)}else{true};
     if !ribbon_clear{
      if GetForegroundWindow()!=h{return Err("Focus changed before present".into());}
      self.theme.take();self.theme_failed=true;ShowWindow(title_sprite,0);self.title_cache=None;
      log(&format!("native_left_skip hwnd={h:#x} app={:?} reason=ribbon_slot_not_verified",self.app));
     }else{
     let mut title_layout=center;let mut fill=None;
     if let (Some(color),Some(frame))=(self.explorer_fill,slot_frame){
      if let Some(band)=explorer_caption::candidate(h,frame,center,left){
       let same=self.band_attempt.is_some_and(|(old,_)|old==band);
       if !same{
        // Verify only on changed geometry, never in background or every timer tick.
        let ok=custom_caption::caption_hits(h,Layout{pos:Point{x:band.pos.x,y:center.pos.y},h:center.h,..band});
        self.band_attempt=Some((band,ok));
        log(&format!("{} hwnd={h:#x}",if ok{"explorer_solid_band_verified"}else{"explorer_solid_band_rejected"}));
       }
       if self.band_attempt==Some((band,true)){title_layout=band;fill=Some(color);}
      }
     }
     let needs_title=self.title_cache.as_ref().map_or(true,|(l,t)|*l!=title_layout||t!=&text)||IsWindowVisible(title_sprite)==0;
     if needs_title{
      let move_only=fill.is_none()&&IsWindowVisible(title_sprite)!=0&&self.title_cache.as_ref().map_or(false,|(old,t)|old.w==title_layout.w&&old.h==title_layout.h&&old.dpi==title_layout.dpi&&t==&text);
      if move_only{
       if SetWindowPos(title_sprite,0,title_layout.pos.x,title_layout.pos.y,0,0,0x0215)==0{return Err("Centered title move failed".into());}
      }else{
       self.title_dark=if let Some(c)=fill{c&255>=145}else if self.app==App::Console||new_native{console_title_dark(h,left,center,dark_text)}else{dark_text};
       if GetForegroundWindow()!=h{return Err("Focus changed before title present".into());}
       if !present_title(title_sprite,title_layout,&text,self.title_dark,fill,center)||SetWindowPos(title_sprite,0,title_layout.pos.x,title_layout.pos.y,0,0,0x0251)==0{return Err("Centered title rendering failed".into());}
       if fill.is_some(){self.refresh=true;} // keep the button sprite above the solid band
       log(&format!("center_title_present x={} width={} chars={}",center.pos.x,center.w,text.len()));
      }
      self.title_cache=Some((title_layout,text));
     }
     next=left;
     }
    }else{ShowWindow(title_sprite,0);self.title_cache=None;self.theme.take();self.theme_failed=true;}
   }else{ShowWindow(title_sprite,0);self.title_cache=None;}
  }else{ShowWindow(title_sprite,0);self.title_cache=None;self.theme.take();self.theme_due=None;}

  // Ribbon fallback: preserve the original title and icon if the exact-left
  // plus centered-title path could not verify its two slots. Wait until the
  // theme restore and window geometry settle, then scan once for a free slot.
  if ribbon&&mode==2&&self.theme_failed&&self.theme.is_none()&&!safe_zoom{
   if let (Some(r),Some(text),Some(info))=(frame,read_title(h),monitor_info(h)){
    let key=(r,text);
    if self.ribbon_alt_key.as_ref()!=Some(&key){
     self.ribbon_alt_key=Some(key);self.ribbon_alt=None;
     self.ribbon_alt_due=Some(Instant::now()+FRAME_SETTLE);self.ribbon_scan=custom_caption::RibbonScan::default();
    }
    if self.ribbon_alt_due.map_or(false,|due|Instant::now()>=due){
     let (slot,done)=self.ribbon_scan.step_native_edge(h,next,r,info.work);
     self.ribbon_alt_due=if done{None}else{Some(Instant::now()+Duration::from_millis(40))};
     self.ribbon_alt=slot;
     self.ribbon_check=Instant::now();
     if done{log(&format!("native_left_ribbon_fallback hwnd={h:#x} verified={} title_preserved=true",self.ribbon_alt.is_some()));}
    }
    if let Some(slot)=self.ribbon_alt{
     if self.ribbon_check.elapsed()>=Duration::from_millis(400){
      self.ribbon_check=Instant::now();
      if !custom_caption::caption_hits(h,slot){self.ribbon_alt=None;log("native_left_ribbon_revert reason=slot_changed");}
     }
     if let Some(slot)=self.ribbon_alt{next=slot;}
    }
   }else{self.ribbon_alt_key=None;self.ribbon_alt=None;self.ribbon_alt_due=None;}
  }else{self.ribbon_alt_key=None;self.ribbon_alt=None;self.ribbon_alt_due=None;}

  // One follow-up invalidation only, after the geometry/theme settle.
  if self.zoom_repaint_due.map_or(false,|due|Instant::now()>=due){
   self.zoom_repaint_due=None;
   if self.app==App::Console{let ok=RedrawWindow(h,null(),0,RDW_INVALIDATE|RDW_ERASE|RDW_ALLCHILDREN)!=0;log(&format!("console_client_repaint hwnd={h:#x} ok={ok}"));}else{zoom_repaint(h,false);}
  }
  let resized=next.w!=self.last.w||next.h!=self.last.h||next.dpi!=self.last.dpi;
  let moved=next!=self.last;
  if moved&&(next.work_top!=self.last.work_top||next.fallback_row!=self.last.fallback_row){log(&format!("workarea_layout top={:?} fallback_row={} position={:?}",next.work_top,next.fallback_row,next.pos));}
  let hover=cursor_over(next);
  if next.left_slot!=self.last.left_slot{log(&format!("layout_side left_slot={} mode={mode}",next.left_slot));}
  if self.refresh||resized||hover!=self.hover||!self.shown||IsWindowVisible(sprite)==0{
   if !present(sprite,next,hover){return Err(format!("Sprite present failed: {}",GetLastError()));}
   if GetForegroundWindow()!=h||!owner_presentable(d,h){return Err("Focus changed before present".into());}
   if SetWindowPos(sprite,0,next.pos.x,next.pos.y,0,0,0x0251)==0{return Err(format!("Sprite raise failed: {}",GetLastError()));}
   log(&format!("sprite_present hwnd={h:#x} owned=1 maximized={zoom} layout={next:?}"));
   self.shown=true;self.refresh=false;self.hover=hover;
  }else if moved{
   if SetWindowPos(sprite,0,next.pos.x,next.pos.y,0,0,0x0215)==0{return Err("Sprite move failed".into());}
  }
  self.last=next;
  let center=Point{x:next.pos.x+scale(11,next.dpi),y:next.pos.y+scale(11,next.dpi)};
  let hit=WindowFromPoint(center);
  if hit==sprite{if self.misses>0{log(&format!("sprite_hit_recovered retries={}",self.misses));}self.misses=0;self.miss_since=None;}
  else{self.misses=self.misses.saturating_add(1);if self.misses==1{log(&format!("sprite_hit_miss hwnd={hit:#x} target={h:#x}"));}
   if coverage_expired(&mut self.miss_since,Instant::now()){return Err("Caption popup remained covered; restoring stock".into());}
   if self.misses==3||self.misses==8{self.refresh=true;}
  }
  Ok(())
 }}
}
fn button_role(l:Layout,x:i32,y:i32)->Option<usize>{
 let f=l.dpi as f64/96.0;
 let (ox,cy)=dot_origin(l);
 (0..3).find(|&i|{let dx=x as f64+0.5-ox-(11.0+i as f64*22.0)*f;let dy=y as f64+0.5-cy;dx*dx+dy*dy<=(7.0*f).powi(2)})
}
fn detach(active:&mut Option<Target>,sprite:H,title_sprite:H,reason:&str){
 unsafe{
  if title_sprite!=0{ShowWindow(title_sprite,0);}
  // Hide our own popup even when an attach failed before creating Target.
  if sprite!=0{ShowWindow(sprite,0);SetWindowPos(sprite,0,0,0,0,0,0x0217);}
 }
 if let Some(t)=active.take(){
  log(&format!("detach hwnd={:#x} reason={reason}",t.guard.h));drop(t);
 }
}
// F11 always selects right; F10 reuses a previously confirmed session layout.
// First-time confirmation still requires a successful visual probe.
fn layout_hotkey(mode:u8,confirmed:bool,key:usize,probe_ready:bool)->(u8,bool){
 match key{
  2=>(0,confirmed),
  3 if confirmed=>(2,true),
  3 if mode==0=>(1,false),
  3 if mode==1&&probe_ready=>(2,true),
  _=>(mode,confirmed),
 }
}
#[cfg(test)] mod hotkey_tests{
 use super::layout_hotkey;
 #[test] fn first_confirmation(){assert_eq!(layout_hotkey(0,false,3,false),(1,false));assert_eq!(layout_hotkey(1,false,3,false),(1,false));assert_eq!(layout_hotkey(1,false,3,true),(2,true));}
 #[test] fn repeat_left_right(){let (mode,confirmed)=layout_hotkey(2,true,2,false);assert_eq!((mode,confirmed),(0,true));assert_eq!(layout_hotkey(mode,confirmed,3,false),(2,true));assert_eq!(layout_hotkey(0,true,2,false),(0,true));assert_eq!(layout_hotkey(2,true,3,false),(2,true));}
}
// Explicit opt-in for a previously visually verified left/title layout.
fn startup_mode(mac:bool,confirmed:bool)->(u8,bool){
 if confirmed{(2,true)}else if mac{(1,false)}else{(0,false)}
}
fn layout_key(id:usize)->Option<(usize,&'static str)>{
 match id{2=>Some((2,"F11")),3=>Some((3,"F10")),4=>Some((3,"L")),5=>Some((2,"R")),_=>None}
}
fn register_key(h:H,id:i32,vk:u32,name:&str)->bool{unsafe{
 SetLastError(0);let ok=RegisterHotKey(h,id,0x4003,vk)!=0;
 let err=if ok{0}else{GetLastError()};
 log(&format!("hotkey_register key={name} ok={ok} error={err}"));ok
}}
#[cfg(test)] mod startup_tests{
 use super::{startup_mode,layout_key};
 #[test] fn explicit_confirmation(){assert_eq!(startup_mode(false,false),(0,false));assert_eq!(startup_mode(true,false),(1,false));assert_eq!(startup_mode(false,true),(2,true));assert_eq!(startup_mode(true,true),(2,true));}
 #[test] fn aliases(){assert_eq!(layout_key(4),Some((3,"L")));assert_eq!(layout_key(5),Some((2,"R")));assert_eq!(layout_key(1),None);}
}
// A newly opened Explorer can become foreground before its frame is ready.
// Bound startup reinspection. Foreground native geometry recovery has its own
// capped retry cadence; deliberate close and nongeometry failures remain guarded.
const EXPLORER_RETRY_MS:[u64;5]=[80,160,320,640,1280];
fn explorer_retry_delay(attempt:&mut usize)->Option<Duration>{
 let ms=*EXPLORER_RETRY_MS.get(*attempt)?;
 *attempt+=1;Some(Duration::from_millis(ms))
}
fn maybe_new_explorer(h:H)->bool{
 if h==0{return false;}
 let class=class_name(h);
 class=="CabinetWClass"||class.is_empty()||!usable_window(h)
}
fn maybe_new_caption(h:H)->bool{
 if h==0{return false;}
 if maybe_new_explorer(h){return true;}
 let class=class_name(h);
 matches!(class.as_str(),"TaskManagerWindow"|"MMCMainFrame"|"RegEdit_RegEdit"|"SunAwtFrame"|"ApplicationFrameWindow"|"Windows.UI.Core.CoreWindow"|"ConsoleWindowClass"|"Notepad"|"WordPadClass"|"MSPaintApp"|"Chrome_WidgetWin_1"|"OpusApp"|"XLMAIN"|"PPTFrameClass")
}
// Separate startup retries for provisional generic HWNDs. Do not change Explorer's policy.
fn caption_retry_delay(h:H,attempt:&mut usize)->Option<Duration>{
 if maybe_new_explorer(h){return explorer_retry_delay(attempt);}
 const MS:[u64;13]=[40,80,120,160,250,400,600,800,1000,1500,2000,2500,3000];
 let delay=*MS.get(*attempt)?;*attempt+=1;Some(Duration::from_millis(delay))
}
#[cfg(test)]
fn arm_explorer_retry(h:H,app:App,attempt:&mut usize,reason:&str)->Option<Instant>{
 if app!=App::Explorer{return None;}
 match explorer_retry_delay(attempt){
  Some(delay)=>{log(&format!("explorer_startup_retry hwnd={h:#x} attempt={} delay_ms={} reason={reason}",*attempt,delay.as_millis()));Some(Instant::now()+delay)},
  None=>{log(&format!("explorer_startup_retry_exhausted hwnd={h:#x} reason={reason}"));None}
 }
}
fn transient_native_geometry(reason:&str)->bool{matches!(reason,
 "Not an active supported window"|"No window rectangle"|"No visible frame"|"No original caption bounds"|"Original caption bounds not usable"|"Invalid initial slot"|"Focus changed during attach"|"Caption slot became invalid"|"Native bounds unavailable after state change"|"Native caption obscured by work area; leave stock"|"Native caption obscured; restore stock"|"Focus changed before present"|"Target no longer active"|"Stock buttons returned after state change"|"Caption popup remained covered; restoring stock")}
// Keep geometry-only failures recoverable while foreground. No permanent
// block after a temporary geometry failure, and no tight retry for access/style errors.
fn recovery_delay(attempt:&mut usize)->(Duration,bool){
 let n=*attempt;
 let ms=[30,80,160,320,640,1000].get(n).copied().unwrap_or(1500);
 *attempt=n.saturating_add(1);
 (Duration::from_millis(ms),n<=5)
}
fn arm_geometry_retry(h:H,app:App,attempt:&mut usize,reason:&str)->Option<Instant>{
 if !app.supported(false)||!transient_native_geometry(reason){return None;}
 let (delay,report)=recovery_delay(attempt);
 if report{log(&format!("native_geometry_retry hwnd={h:#x} attempt={} delay_ms={} reason={reason}",*attempt,delay.as_millis()));}
 Some(Instant::now()+delay)
}
#[cfg(test)] mod recovery_tests{use super::*;
 #[test]fn geometry_retry_recovers_after_budget(){let mut n=0;for ms in [30,80,160,320,640,1000,1500,1500]{assert_eq!(recovery_delay(&mut n).0,Duration::from_millis(ms));}assert!(transient_native_geometry("Caption popup remained covered; restoring stock"));assert!(!transient_native_geometry("Style change failed"));}
 #[test]fn retry_only_for_admitted_native(){let mut n=0;assert!(arm_geometry_retry(0,App::Other,&mut n,"Invalid initial slot").is_none());assert!(arm_geometry_retry(0,App::Explorer,&mut n,"Style change failed").is_none());assert_eq!(n,0);}
}
#[cfg(test)] mod explorer_startup_tests{
 use super::*;
 #[test] fn bounded_backoff(){let mut n=0;for ms in EXPLORER_RETRY_MS{assert_eq!(explorer_retry_delay(&mut n),Some(Duration::from_millis(ms)));}assert_eq!(explorer_retry_delay(&mut n),None);assert_eq!(n,5);}
 #[test] fn native_policy_unchanged(){let mut n=0;assert!(arm_explorer_retry(0,App::Console,&mut n,"test").is_none());assert_eq!(n,0);}
}

// One set of owned, non-topmost popup surfaces for each admitted HWND.
// A session survives deactivation and minimization. The manager never samples
// background pixels; only a foreground session can verify a new layout.
const MAX_SESSIONS:usize=24;
fn overlaps(a:Rect,b:Rect)->bool{a.left<b.right&&b.left<a.right&&a.top<b.bottom&&b.top<a.bottom}
// Bounded check of real desktop stacking; no action on incomplete evidence.
fn above(a:H,b:H)->bool{unsafe{
 if a==0||b==0||a==b{return false;}
 let mut h=b;
 for _ in 0..256{h=window_neighbor(h,3);if h==0||h==b{return false;}if h==a{return true;}}
 false
}}
struct Session{
 h:H,id:(u32,u32),app:App,active:Option<Target>,custom:custom_caption::Surface,
 sprite:Sprite,title:Sprite,cover:Sprite,frame:Option<Rect>,environment:Option<WindowEnvironment>,
 blocked:bool,blocked_zoom:Option<bool>,retry_at:Instant,explorer_attempt:usize,close_cooldown:Option<Instant>,visibility_suspended:bool,
}
// Only already-admitted GenericProbe sessions reach this policy. Settings
// ApplicationFrameWindow is admitted only after checking its Settings child.
fn centered_generic_class(class:&str)->bool{
 matches!(class,"SunAwtFrame"|"RegEdit_RegEdit"|"MMCMainFrame"|"TaskManagerWindow"|"ApplicationFrameWindow"|"Windows.UI.Core.CoreWindow")
}
#[cfg(test)] mod centered_policy_tests{use super::*;
 #[test] fn settings_and_classic_only(){for c in ["ApplicationFrameWindow","Windows.UI.Core.CoreWindow","SunAwtFrame","MMCMainFrame","TaskManagerWindow","RegEdit_RegEdit"]{assert!(centered_generic_class(c));}assert!(!centered_generic_class("Chrome_WidgetWin_1"));assert!(!centered_generic_class("#32770"));}
}
impl Session{
 fn new(h:H,app:App,name:&[u16],inst:H,office_exact:bool,browser_left:bool,browser_spacer:bool)->Option<Self>{unsafe{
  let make=|ex|Sprite(CreateWindowExW(ex,name.as_ptr(),name.as_ptr(),0x80000000,0,0,1,1,h,0,inst,null()));
  let need_title=app.needs_title_surface(office_exact)||(app==App::BrowserProbe&&read_title(h).as_deref().is_some_and(custom_caption::browser_plain_title));
  let need_cover=app.probe();
  let cover=if need_cover{make(0x08080080)}else{Sprite(0)};
  let title=if need_title{make(0x080800a0)}else{Sprite(0)};
  let sprite=make(0x08080080);
  if sprite.0==0||(need_title&&title.0==0)||(need_cover&&cover.0==0){log(&format!("session_surface_failed hwnd={h:#x} app={app:?} need_title={need_title} need_cover={need_cover}"));return None;}
  let class=class_name(h);
  let centered=app==App::GenericProbe&&centered_generic_class(&class);
  let settings=centered&&matches!(class.as_str(),"ApplicationFrameWindow"|"Windows.UI.Core.CoreWindow");
  let custom=custom_caption::Surface::new(title.0,office_exact,browser_left,browser_spacer,centered,settings);
  log(&format!("session_create hwnd={h:#x} app={app:?} surfaces={}",1+need_title as u32+need_cover as u32));
  Some(Self{h,id:identity(h),app,active:None,custom,sprite,title,cover,frame:None,environment:None,blocked:false,blocked_zoom:None,retry_at:Instant::now(),explorer_attempt:0,close_cooldown:None,visibility_suspended:false})
 }}
 fn alive(&self)->bool{unsafe{IsWindow(self.h)!=0&&identity(self.h)==self.id&&IsWindow(self.sprite.0)!=0&&[self.title.0,self.cover.0].into_iter().all(|w|w==0||IsWindow(w)!=0)}}
 fn abandon_destroyed(&mut self){
  // A destroy event newer than this session's admission invalidates its HWND.
  // Never restore old caption flags onto a subsequently reused HWND.
  if let Some(t)=self.active.as_mut(){t.guard.h=0;if let Some(g)=t.theme.as_mut(){g.h=0;}}
 }
 fn needs_fast_tick(&self,allow_left:bool)->bool{
  if unsafe{IsWindowEnabled(self.h)==0||IsIconic(self.h)!=0}||self.close_cooldown.is_some_and(|t|Instant::now()<t){return false;}
  if self.app.probe(){self.custom.needs_fast_tick(allow_left)}else{self.active.as_ref().map_or(!self.blocked&&Instant::now()>=self.retry_at,Target::needs_fast_tick)}
 }
 fn recheck_environment(&mut self){
  let current=window_environment(self.h);
  if environment_changed(self.environment,current){log(&format!("session_environment_changed hwnd={:#x} old={:?} new={current:?}",self.h,self.environment));self.clear("monitor_dpi_workarea_changed");self.rearm();}
  self.environment=current;
 }
 fn clear(&mut self,reason:&str){
  self.custom.reset(self.sprite.0,self.cover.0);
  detach(&mut self.active,self.sprite.0,self.title.0,reason);
  self.frame=None;
 }
 fn rearm(&mut self){self.blocked=false;self.blocked_zoom=None;self.retry_at=Instant::now();self.explorer_attempt=0;}
 // Only correct a proven overlap, never raise background sprites above another app.
 fn audit_stack(&mut self,fg:H,fg_rect:Option<Rect>){unsafe{
  let Some(fr)=fg_rect else{return;};
  if fg==self.h||GetForegroundWindow()!=fg||self.frame.is_none()||IsIconic(self.h)!=0{return;}
  for surface in [self.sprite.0,self.title.0,self.cover.0]{
   if surface==0||IsWindowVisible(surface)==0||window_neighbor(surface,4)!=self.h{continue;}
   if !window_rect(surface).is_some_and(|r|overlaps(r,fr)){continue;}
   if !above(fg,self.h){return;}
   if !above(surface,fg){continue;}
   if GetForegroundWindow()!=fg{return;}
   let ok=SetWindowPos(surface,fg,0,0,0,0,0x0213)!=0;
   log(&format!("session_zorder_correct hwnd={:#x} popup={surface:#x} foreground={fg:#x} ok={ok}",self.h));
   if GetForegroundWindow()==fg&&(!ok||above(surface,fg)){
    self.clear("background_zorder_conflict");self.rearm();return;
   }
  }
 }}
 // Explicitly hide all three surfaces, even if the renderer's cached flags
 // disagree with the actual popup visibility. Do not rely on owner auto-hide.
 fn suspend_surfaces(&mut self,reason:&str){
  self.visibility_suspended=true;
  let visible=[self.sprite.0,self.title.0,self.cover.0].into_iter().any(|w|w!=0&&unsafe{IsWindowVisible(w)!=0});
  if visible||self.frame.is_some()||self.active.is_some()||self.custom.saved_dpi().is_some(){
   for w in [self.sprite.0,self.title.0,self.cover.0]{if w!=0{unsafe{ShowWindow(w,0);}}}
   log(&format!("owner_visibility_suspend hwnd={:#x} reason={reason}",self.h));
   self.clear(reason);self.rearm();
  }
 }
 fn recover_visibility(&mut self){
  if self.visibility_suspended{self.visibility_suspended=false;self.rearm();self.custom.resume();log(&format!("caption_visibility_recover hwnd={:#x}",self.h));}
 }
 fn background(&mut self,d:&Dwm){unsafe{
  if !owner_presentable(d,self.h){self.suspend_surfaces("owner_not_presentable");return;}
  self.recheck_environment();
  // Do not sample a covered window or keep masks at stale screen positions.
  // A pure translation is safe; a background resize is not, so restore stock.
  if let (Some(old),Some(new))=(self.frame,visible_rect(d,self.h)){
   if old!=new{
    if old.width()==new.width()&&old.height()==new.height()&&self.active.as_ref().map(|t|t.dpi).or(self.custom.saved_dpi()).map_or(true,|dpi|dpi==GetDpiForWindow(self.h).max(96))&&monitor_info(self.h).map_or(false,|m|new.top>=m.work.top&&new.left>=m.work.left&&new.right<=m.work.right){
     let dx=new.left-old.left;let dy=new.top-old.top;
     let mut ok=true;
     for surface in [self.sprite.0,self.title.0,self.cover.0]{if surface!=0&&IsWindowVisible(surface)!=0{if let Some(r)=window_rect(surface){ok &= SetWindowPos(surface,0,r.left+dx,r.top+dy,0,0,0x0215)!=0;}}}
     if ok{
      if let Some(t)=self.active.as_mut(){t.last.pos.x+=dx;t.last.pos.y+=dy;if let Some((l,_))=t.title_cache.as_mut(){l.pos.x+=dx;l.pos.y+=dy;}if let Some(l)=t.ribbon_alt.as_mut(){l.pos.x+=dx;l.pos.y+=dy;}}
      self.custom.translate(dx,dy,new);self.frame=Some(new);
     }else{self.clear("background_move_failed");self.rearm();}
    }else{self.clear("background_geometry_changed");self.rearm();}
   }
  }
 }}
}
impl Drop for Session{fn drop(&mut self){self.clear("session_drop");}}
fn run()->Result<(),String>{unsafe{
 let cfg=release::config();
 let explicit_mac=std::env::args().any(|a|a=="--mac");
 let explicit_confirmed=std::env::args().any(|a|a=="--mac-confirmed");
 let (mut mode,mut mac_confirmed)=startup_mode(explicit_mac,explicit_confirmed||!explicit_mac);
 let alt_keys=cfg.alt_keys||std::env::args().any(|a|a=="--alt-keys");
 let console_left=!std::env::args().any(|a|a=="--console-right");
 let native_left=!std::env::args().any(|a|a=="--native-right");
 let custom_left=!std::env::args().any(|a|a=="--custom-right");
 let office_exact=cfg.office_exact_left||std::env::args().any(|a|a=="--office-exact-left");
 let browser_left=std::env::args().any(|a|a=="--browser-left");
 let browser_spacer=cfg.browser_spacer||std::env::args().any(|a|a=="--browser-spacer");
 let explorer_only=std::env::args().any(|a|a=="--explorer-only");
 let browser_enabled=cfg.browser_enabled&&!explorer_only&&!std::env::args().any(|a|a=="--no-browser");
 let office_enabled=cfg.office_enabled&&!explorer_only&&!std::env::args().any(|a|a=="--no-office");
 let trace_focus=std::env::args().any(|a|a=="--trace-focus");
 let use_workzoom=!cfg.native_maximize&&!std::env::args().any(|a|a=="--native-max");
 let safe_zoom=std::env::args().any(|a|a=="--safe-zoom");
 let legacy_copy=std::env::args().any(|a|a=="--legacy-copy");
 let mut zooms:HashMap<H,WorkZoom>=HashMap::new();
 let dark_text=std::env::args().any(|a|a=="--light");
 SetLastError(0);
 let mutex=CreateMutexW(null(),0,wide("Local\\MacTrafficCaptionLab0803").as_ptr());
 let mutex_error=GetLastError();
 if mutex==0{return Err(format!("Mutex error {mutex_error}"));}
 let _instance=Instance(mutex);
 if mutex_error==183{return Err("Another MacTraffic instance is already running. Exit it with Ctrl+Alt+F12.".into());}
 SetProcessDpiAwarenessContext(-4);
 let d=Dwm::load().ok_or("Cannot load DWM API")?;
 let name=wide("MacTrafficCaption100");let inst=GetModuleHandleW(null());
 let wc=Class{style:0,proc:Some(wndproc),cls_extra:0,wnd_extra:0,instance:inst,icon:0,cursor:LoadCursorW(0,32512usize as *const u16),background:0,menu:null(),name:name.as_ptr()};
 if RegisterClassW(&wc)==0{return Err(format!("RegisterClassW error {}",GetLastError()));}

 let manager=Sprite(CreateWindowExW(0x08000080,name.as_ptr(),name.as_ptr(),0x80000000,0,0,1,1,0,0,inst,null()));
 if manager.0==0{return Err("Manager window creation failed".into());}
 if !register_key(manager.0,1,0x7b,"F12"){return Err("Ctrl+Alt+F12 unavailable; no styles changed.".into());}
 let right=register_key(manager.0,2,0x7a,"F11");
 let left=register_key(manager.0,3,0x79,"F10");
 let left_alt=alt_keys&&register_key(manager.0,4,0x4c,"L");
 let right_alt=alt_keys&&register_key(manager.0,5,0x52,"R");
 if !right&&!right_alt{return Err("No right-layout shortcut available; no styles changed.".into());}
 if !left&&!left_alt{return Err("No left-layout shortcut available; no styles changed.".into());}
 let _toggle_key=register_key(manager.0,release::TOGGLE_ID,0x50,"P");
 release::PAUSED.store(cfg.start_paused,std::sync::atomic::Ordering::Release);
 release::prepare_spacer();
 let mut tray=release::Tray::new(manager.0);
 if SetTimer(manager.0,1,if cfg.start_paused{1000}else{40},0)==0{return Err("Timer unavailable; no styles changed.".into());}
 // No foreign-process code or injection. Keep a watchdog if events fail.
 MANAGER_WAKE.store(manager.0,std::sync::atomic::Ordering::Release);
 let hooks=EventHooks::install();let hooks_complete=hooks.complete();
 let background_period=if hooks_complete{BACKGROUND_PERIOD}else{BACKGROUND_FALLBACK};
 log(&format!("event_engine_ready hooks={}/6 fallback={} fast_ms=40 stable_ms=100 idle_ms=500 background_ms={}",hooks.0.len(),!hooks_complete,background_period.as_millis()));
 wake_manager();
 let mut sessions:HashMap<H,Session>=HashMap::new();
 let mut foreground=0;let mut foreground_app=App::Other;let mut foreground_visible=false;
 let mut inspect_attempt=0usize;let mut inspect_due:Option<Instant>=None;
 let mut tick_ms=if cfg.start_paused{1000u32}else{40u32};
 let mut background_due=Instant::now();let mut background_last=Instant::now();let mut background_dirty=false;let mut fast_until=Instant::now();
 let mut timer_ticks=0u64;let mut event_ticks=0u64;let mut background_passes=0u64;let stats_start=Instant::now();let mut stats_due=Instant::now()+Duration::from_secs(60);
 log(&format!("manager_ready mode=event_driven_owned lifetime=continuous tick_ms={tick_ms} mac_mode={mode} browser_spacer={browser_spacer} max_sessions={MAX_SESSIONS} background_ms={} native_margin_dip={LEFT_MARGIN_DIP}",background_period.as_millis()));
 let result=loop{
  let mut m:Msg=zeroed();let status=GetMessageW(&mut m,0,0,0);
  if status<0{break Err("GetMessageW failed".into());}if status==0{break Ok(());}
  if (m.message==0x0312&&m.wparam==1)||(m.message==0x0010&&m.hwnd==manager.0){log("stop hotkey");break Ok(());}
  // Release tray and toggle controls run only on the manager thread. No
  // cross-process command receiver or background installer is introduced.
  if m.hwnd==manager.0&&tray.taskbar_created!=0&&m.message==tray.taskbar_created{tray.readd();continue;}
  let tray_cmd=if m.hwnd==manager.0&&m.message==release::TRAY_MESSAGE&&matches!(m.lparam as u32,0x0202|0x0205|0x007b){
   let cmd=tray.menu();WAKE_QUEUED.store(false,std::sync::atomic::Ordering::Release);wake_manager();cmd
  }else{0};
  if tray_cmd==release::MENU_EXIT{log("stop tray_exit");break Ok(());}
  if tray_cmd==release::MENU_TOGGLE||(m.message==0x0312&&m.wparam==release::TOGGLE_ID as usize){
   use std::sync::atomic::Ordering;
   if release::PAUSED.load(Ordering::Acquire){
    if SetTimer(manager.0,1,40,0)!=0{tick_ms=40;foreground=0;foreground_app=App::Other;inspect_due=None;background_due=Instant::now();fast_until=Instant::now();release::PAUSED.store(false,Ordering::Release);log("release_resumed");}
    else{log("resume_timer_failed");}
   }else{
    release::PAUSED.store(true,Ordering::Release);
    for h in take_destroyed(){if let Some(mut s)=sessions.remove(&h){s.abandon_destroyed();}zooms.remove(&h);}
    sessions.clear();reconcile_tracked(&sessions);
    for (h,z) in zooms.drain(){if zoom_matches(h,z)&&zoom_restore_visible(z.before){position_normal(h,z.before,legacy_copy);}}
    foreground=0;foreground_app=App::Other;inspect_due=None;background_dirty=false;
    if SetTimer(manager.0,1,1000,0)!=0{tick_ms=1000;}
    log("release_paused cleanup_requested=true");
   }
   tray.refresh();wake_manager();continue;
  }
  if tray_cmd!=0{if let Err(e)=release::menu_action(tray_cmd){log(&format!("tray_action_error {e}"));MessageBoxW(0,wide(&e).as_ptr(),wide("MacTraffic 1.0.3").as_ptr(),0x30);}continue;}
  if m.message==release::TRAY_MESSAGE&&m.hwnd==manager.0{continue;}
  if m.message==0x0312&&release::PAUSED.load(std::sync::atomic::Ordering::Relaxed){continue;}
  if let Some((key,label))=if m.message==0x0312{layout_key(m.wparam)}else{None}{
   let ready=sessions.get(&GetForegroundWindow()).and_then(|s|s.active.as_ref()).map_or(false,|t|t.theme.is_some());
   let (next,confirmed)=layout_hotkey(mode,mac_confirmed,key,ready);
   log(&format!("layout_hotkey key={label} old_mode={mode} next_mode={next} confirmed={confirmed} probe_ready={ready}"));
   mac_confirmed=confirmed;
   if next!=mode{mode=next;for s in sessions.values_mut(){s.clear("layout_hotkey");s.rearm();}}
   else if key==3&&mode==2{if let Some(s)=sessions.get_mut(&GetForegroundWindow()){s.rearm();if let Some(t)=s.active.as_mut(){t.theme_failed=false;t.refresh=true;if t.theme.is_some(){t.theme_due=Some(Instant::now());}}}}
   wake_manager();continue;
  }
  if ((m.message==0x0113&&m.wparam==1)||m.message==WM_MANAGER_WAKE)&&m.hwnd==manager.0{
   if m.message==WM_MANAGER_WAKE{WAKE_QUEUED.store(false,std::sync::atomic::Ordering::Release);event_ticks+=1;}else{timer_ticks+=1;}
   let flags=EVENT_FLAGS.swap(0,std::sync::atomic::Ordering::AcqRel);
   if flags&DIRTY_SHUTDOWN!=0{log("stop session_end");break Ok(());}
   tray.retry();
   if release::PAUSED.load(std::sync::atomic::Ordering::Relaxed){continue;}
   if flags&DIRTY_MOTION!=0{fast_until=Instant::now()+Duration::from_millis(180);}
   for h in take_destroyed(){if let Some(mut s)=sessions.remove(&h){s.abandon_destroyed();log(&format!("session_destroyed hwnd={h:#x}"));}zooms.remove(&h);if foreground==h{foreground=0;foreground_app=App::Other;}}
   let fg=GetForegroundWindow();
   if sessions.get(&fg).is_some_and(|s|!s.alive()){sessions.remove(&fg);foreground=0;}
   let focus_changed=fg!=foreground;
   let visible_now=owner_presentable(&d,fg);
   let visibility_restored=!focus_changed&&!foreground_visible&&visible_now;foreground_visible=visible_now;
   background_dirty|=flags&DIRTY_BACKGROUND!=0;
   let background_pass=focus_changed||flags&(DIRTY_ENVIRONMENT|DIRTY_VISIBILITY)!=0||Instant::now()>=background_due||(background_dirty&&background_last.elapsed()>=Duration::from_millis(80));
   if background_pass{
    background_dirty=false;background_last=Instant::now();background_passes+=1;background_due=background_last+background_period;
   }
   if fg!=foreground{
    foreground=fg;foreground_app=sessions.get(&fg).map(|s|s.app).unwrap_or_else(||inspect_app(&d,fg,explorer_only,trace_focus));
    if let Some(s)=sessions.get_mut(&fg){s.rearm();s.custom.resume();if s.app==App::OfficeProbe&&custom_left&&mode==2{s.custom.rearm_office();}if let Some(t)=s.active.as_mut(){t.misses=0;t.miss_since=None;t.refresh=true;}
     if let Some(t)=s.active.as_mut(){if t.app==App::Explorer&&owner_presentable(&d,fg){
      let fill=explorer_caption::choose_fill(fg);
      if t.explorer_fill!=fill{t.explorer_fill=fill;t.band_attempt=None;t.title_cache=None;t.refresh=true;}
      else if t.band_attempt.is_some_and(|(_,ok)|!ok){t.band_attempt=None;}
     }}log(&format!("session_resume hwnd={fg:#x}"));}
    inspect_attempt=0;inspect_due=None;
    if foreground_app==App::Other&&maybe_new_caption(fg){inspect_due=caption_retry_delay(fg,&mut inspect_attempt).map(|delay|Instant::now()+delay);}
   }
   if !focus_changed&&(flags&DIRTY_INSPECT!=0||visibility_restored)&&foreground_app==App::Other&&maybe_new_caption(fg){if visibility_restored{inspect_attempt=0;}inspect_due=Some(Instant::now());}
   if inspect_due.map_or(false,|due|Instant::now()>=due){
    inspect_due=None;
    if foreground_app==App::Other&&maybe_new_caption(fg){foreground_app=inspect_app(&d,fg,explorer_only,trace_focus);if foreground_app==App::GenericProbe{log(&format!("generic_startup_admitted hwnd={fg:#x} attempts={inspect_attempt}"));}if foreground_app==App::Other{inspect_due=caption_retry_delay(fg,&mut inspect_attempt).map(|delay|Instant::now()+delay);}}
   }
   let admitted=foreground_app.supported(explorer_only)||(foreground_app==App::BrowserProbe&&browser_enabled)||(foreground_app==App::OfficeProbe&&office_enabled)||(foreground_app==App::GenericProbe&&!explorer_only);
   if admitted&&owner_presentable(&d,fg)&&!sessions.contains_key(&fg)&&sessions.len()>=MAX_SESSIONS{
    sessions.retain(|_,s|s.alive());
    if sessions.len()>=MAX_SESSIONS{if let Some(old)=sessions.keys().copied().find(|&h|h!=fg&&!owner_presentable(&d,h)){sessions.remove(&old);log(&format!("session_capacity_reclaim hwnd={old:#x}"));}}
    reconcile_tracked(&sessions);
   }
   if admitted&&owner_presentable(&d,fg)&&!sessions.contains_key(&fg)&&sessions.len()<MAX_SESSIONS{
    if let Some(s)=Session::new(fg,foreground_app,&name,inst,office_exact,browser_left,browser_spacer){sessions.insert(fg,s);track_window(fg);}
   }
   if let Some(s)=sessions.get_mut(&fg){
    if owner_presentable(&d,fg){
     s.recover_visibility();
     s.recheck_environment();
     let disabled=IsWindowEnabled(fg)==0;
     let handing_off=s.close_cooldown.is_some_and(|t|Instant::now()<t);
     // Do not interrogate modal owners; preserve a verified, unchanged frame.
     if disabled||handing_off{
      if s.frame.is_some()&&s.frame!=visible_rect(&d,fg){s.clear("close_owner_geometry_changed");s.rearm();}
     }else{
     s.close_cooldown=None;
     if s.app.supported(explorer_only){
      debug_assert!(s.title.0!=0, "native caption session requires title surface");
      if s.blocked&&s.blocked_zoom.map_or(false,|old|old!=(IsZoomed(fg)!=0)){s.rearm();s.retry_at=Instant::now()+FRAME_SETTLE;}
      if s.active.is_none()&&!s.blocked&&Instant::now()>=s.retry_at{
       match Target::attach(&d,fg,s.sprite.0,s.app){Ok(t)=>s.active=Some(t),Err(e)=>{
        log(&format!("attach_skipped hwnd={fg:#x} app={:?} reason={e}",s.app));s.blocked=true;s.blocked_zoom=Some(IsZoomed(fg)!=0);
        if let Some(due)=arm_geometry_retry(fg,s.app,&mut s.explorer_attempt,&e){s.blocked=false;s.retry_at=due;}
       }}
      }
      if let Some(t)=s.active.as_mut(){let fallback=safe_zoom&&zooms.contains_key(&fg);if let Err(e)=t.tick(&d,s.sprite.0,s.title.0,mode,dark_text,fallback,console_left,native_left){
       if GetForegroundWindow()==fg&&owner_presentable(&d,fg){
        log(&format!("target_error hwnd={fg:#x} reason={e}"));detach(&mut s.active,s.sprite.0,s.title.0,"render_error");s.blocked=true;s.blocked_zoom=Some(IsZoomed(fg)!=0);
        if let Some(due)=arm_geometry_retry(fg,s.app,&mut s.explorer_attempt,&e){s.blocked=false;s.retry_at=due;}
       }
      }}
      if s.active.as_ref().is_some_and(|t|t.shown&&t.misses==0)&&s.explorer_attempt!=0{log(&format!("caption_retry_recovered hwnd={fg:#x} attempts={}",s.explorer_attempt));s.explorer_attempt=0;}
     }else{s.custom.tick(&d,fg,s.app,s.sprite.0,s.cover.0,custom_left&&mode==2);}
     if owner_presentable(&d,fg){s.frame=visible_rect(&d,fg);}else{s.suspend_surfaces("owner_disappeared_during_present");}
     }
    }else{s.suspend_surfaces("foreground_not_presentable");}
   }
   if background_pass&&GetForegroundWindow()==fg{
    zooms.retain(|&h,z|zoom_alive(h,*z)&&(IsIconic(h)!=0||zoom_matches(h,*z)));
    sessions.retain(|_,s|s.alive());reconcile_tracked(&sessions);
    let fr=if owner_presentable(&d,fg){visible_rect(&d,fg)}else{None};
    for (&h,s) in sessions.iter_mut(){if h!=fg{s.background(&d);s.audit_stack(fg,fr);}}
   }
   let settling=focus_changed||background_dirty||Instant::now()<fast_until||inspect_due.is_some()||sessions.get(&fg).is_some_and(|s|s.needs_fast_tick(custom_left&&mode==2));
   let wanted=interval_ms(hooks_complete,admitted,settling,!sessions.is_empty());
   if wanted!=tick_ms&&SetTimer(manager.0,1,wanted,0)!=0{tick_ms=wanted;}
   if Instant::now()>=stats_due{log(&format!("engine_stats elapsed_ms={} timer_ticks={timer_ticks} event_ticks={event_ticks} background_passes={background_passes} interval_ms={tick_ms} sessions={}",stats_start.elapsed().as_millis(),sessions.len()));stats_due=Instant::now()+Duration::from_secs(60);}
   continue;
  }
  if m.message==WM_LBUTTONUP{
   if let Some(s)=sessions.values_mut().find(|s|s.sprite.0==m.hwnd){
    let h=s.h;if !s.alive()||!owner_presentable(&d,h)||IsWindowEnabled(h)==0{continue;}
    // Do not activate an owner through its active modal popup. A short close
    // debounce also prevents accidental repeated WM_CLOSE during confirmation.
    let fg=GetForegroundWindow();
    if (fg!=h&&fg!=0&&GetAncestor(fg,3)==h)||s.close_cooldown.is_some_and(|t|Instant::now()<t){continue;}
    // Direct user click on a background caption may activate its owner.
    if GetForegroundWindow()!=h{SetForegroundWindow(h);if GetForegroundWindow()!=h{continue;}}
    if let Some((h,role))=s.custom.click(&d,m.lparam){
     log(&format!("custom_click hwnd={h:#x} role={role}"));
     match role{0=>{
      if s.app.retain_on_close(){
       // Keep the verified surfaces and zoom record until actual destruction.
       // A close request is not a close result: Chromium may show a dialog
       // without changing the foreground HWND. Normal geometry/ownership checks
       // still run; a cancelled request can never permanently block the session.
       let ok=PostMessageW(h,0x0010,0,0)!=0;
       if ok{s.close_cooldown=Some(Instant::now()+Duration::from_millis(750));}
       log(&format!("close_requested_keep_caption hwnd={h:#x} posted={ok}"));
      }else{zooms.remove(&h);s.custom.clear(s.sprite.0,s.cover.0);s.custom.block_close(h);PostMessageW(h,0x0010,0,0);}
     },1=>{s.suspend_surfaces("minimize_requested");s.close_cooldown=Some(Instant::now()+Duration::from_millis(250));ShowWindowAsync(h,6);},2=>{
      let managed=zooms.contains_key(&h);let office=s.app==App::OfficeProbe;
      let maximized=IsZoomed(h)!=0;let info=monitor_info(h);let reserved=info.as_ref().is_some_and(|i|i.work.top>i.monitor.top);
      if office{if let Some(i)=info{log(&format!("office_zoom_request hwnd={h:#x} native_maximized={maximized} monitor={:?} work={:?} reserved_top={reserved}",i.monitor,i.work));}}
      if !(managed||use_workzoom)||!toggle_work_zoom(&d,h,&mut zooms,legacy_copy,false){
       if custom_zoom_native_fallback(office,use_workzoom,reserved,maximized){ShowWindowAsync(h,if maximized{9}else{3});}
       else{log(&format!("office_zoom_native_fallback_blocked hwnd={h:#x} reason=reserved_top"));}
      }
     },_=>{}}
     signal(DIRTY_MOTION);continue;
    }
    let role=s.active.as_ref().and_then(|t|if t.shown&&t.guard.alive(){button_role(t.last,(m.lparam as u16 as i16) as i32,((m.lparam>>16) as u16 as i16) as i32)}else{None});
    if let Some(i)=role{
     let t=s.active.as_mut().unwrap();if t.ribbon_alt==Some(t.last)&&!custom_caption::caption_hits(h,t.last){continue;}
     log(&format!("click hwnd={h:#x} role={i}"));
     match i{
      0=>{let ok=PostMessageW(h,0x0010,0,0)!=0;if ok{s.close_cooldown=Some(Instant::now()+Duration::from_millis(750));}log(&format!("close_requested_keep_caption hwnd={h:#x} posted={ok}"));},
      1=>{s.suspend_surfaces("minimize_requested");s.close_cooldown=Some(Instant::now()+Duration::from_millis(250));ShowWindowAsync(h,6);},
      2=>{let managed=zooms.contains_key(&h);let console=t.app==App::Console;let eligible=use_workzoom&&!console&&((mode==2&&t.last.left_slot)||(native_left&&matches!(t.app,App::Notepad|App::Paint|App::WordPad)));
       if managed||eligible{if toggle_work_zoom(&d,h,&mut zooms,legacy_copy,console){t.refresh=true;t.misses=0;t.miss_since=None;if t.theme.is_some()&&!matches!(t.app,App::Paint|App::WordPad|App::Console){t.theme_due=Some(Instant::now()+FRAME_SETTLE);}if !legacy_copy{t.zoom_repaint_due=Some(Instant::now()+FRAME_SETTLE);}}else{ShowWindowAsync(h,if IsZoomed(h)!=0{9}else{3});}}
       else{ShowWindowAsync(h,if IsZoomed(h)!=0{9}else{3});if console{t.zoom_repaint_due=Some(Instant::now()+FRAME_SETTLE);}}
      },_=>{}
     }
    }
    signal(DIRTY_MOTION);continue;
   }
  }
  TranslateMessage(&m);DispatchMessageW(&m);
 };
 // Stop callbacks first, restore stock styles and destroy all owned surfaces.
 drop(hooks);
 for h in take_destroyed(){if let Some(mut s)=sessions.remove(&h){s.abandon_destroyed();}zooms.remove(&h);}
 let restored_sessions=sessions.len();sessions.clear();
 for (h,z) in zooms.drain(){if zoom_matches(h,z)&&zoom_restore_visible(z.before){let ok=position_normal(h,z.before,legacy_copy);log(&format!("workzoom_exit_restore hwnd={h:#x} ok={ok}"));}}
 log(&format!("engine_stats final=true elapsed_ms={} timer_ticks={timer_ticks} event_ticks={event_ticks} background_passes={background_passes} interval_ms={tick_ms}",stats_start.elapsed().as_millis()));
 log(&format!("engine_exit_cleanup sessions={restored_sessions}"));
 custom_caption::shutdown_workers();
 drop(tray);drop(manager);result
}}
fn main(){let result=release::init().and_then(|_|{log(&format!("=== MacTraffic v1.0.3 pid={} ===",std::process::id()));run()});if let Err(e)=result{log(&format!("stop error: {e}"));unsafe{MessageBoxW(0,wide(&e).as_ptr(),wide("MacTraffic v1.0.3").as_ptr(),0x30);}}}

#[cfg(test)] mod performance_tests{use super::*;
 #[test] fn overlap_is_strict(){let a=Rect{left:0,top:0,right:80,bottom:30};assert!(overlaps(a,Rect{left:79,..a}));assert!(!overlaps(a,Rect{left:80,right:100,..a}));assert!(!overlaps(a,Rect{top:30,bottom:60,..a}));}
 #[test] fn native_margin_scales(){for dpi in [96,120,144,192]{let frame=Rect{left:100,top:50,right:2100,bottom:900};let l=Layout{pos:Point{x:1800,y:54},w:scale(66,dpi),h:scale(22,dpi),dpi,work_top:None,fallback_row:false,left_slot:false};let (b,t)=mac_slots(frame,l).unwrap();assert_eq!(b.pos.x,100+scale(8,dpi));assert!(b.pos.x+b.w<t.pos.x);}}
}

#[cfg(test)] mod surface_allocation_tests{use super::*;
 #[test] fn explorer_requires_title_surface(){assert!(!App::Explorer.native());assert!(App::Explorer.needs_title_surface(false));assert!(App::Explorer.needs_title_surface(true));}
 #[test] fn title_surface_matrix(){for app in [App::Explorer,App::Console,App::Notepad,App::WordPad,App::Paint]{assert!(app.needs_title_surface(false));}assert!(!App::BrowserProbe.needs_title_surface(true));assert!(!App::OfficeProbe.needs_title_surface(false));assert!(App::OfficeProbe.needs_title_surface(true));assert!(!App::Other.needs_title_surface(true));}
}

#[cfg(test)] mod ribbon_margin_tests{use super::*;
 #[test] fn old_paint_geometry_preserved(){for dpi in [96,120,144,192]{assert_eq!(ribbon_left_x(198,dpi,false),198+scale(3,dpi)+scale(5,dpi));assert_eq!(ribbon_left_x(198,dpi,true),198+scale(8,dpi)+scale(5,dpi));assert!(ribbon_left_x(198,dpi,true)>ribbon_left_x(198,dpi,false));}}
}

#[cfg(test)] mod stage2_tests{use super::*;
 #[test]fn event_timer_fast_and_stable(){assert_eq!(interval_ms(true,true,true,true),40);assert_eq!(interval_ms(true,true,false,true),100);}
 #[test]fn event_timer_idle_and_fallback(){assert_eq!(interval_ms(true,false,false,true),250);assert_eq!(interval_ms(true,false,false,false),500);assert_eq!(interval_ms(false,true,false,true),40);assert_eq!(interval_ms(false,false,false,false),160);}
 #[test]fn native_retry_is_bounded_to_geometry(){assert!(transient_native_geometry("Caption slot became invalid"));assert!(!transient_native_geometry("Style change failed"));assert!(!transient_native_geometry("Application changed caption style"));}
 #[test]fn event_timestamp_wrap(){assert!(event_is_current(2,u32::MAX-2));assert!(!event_is_current(99,100));assert!(event_is_current(100,100));}
 #[test]fn environment_tracks_monitor_dpi_and_workarea(){let a=WindowEnvironment{monitor:1,bounds:Rect{left:-1920,top:0,right:0,bottom:1080},work:Rect{left:-1920,top:24,right:0,bottom:1080},dpi:96};assert!(!environment_changed(None,Some(a)));assert!(!environment_changed(Some(a),Some(a)));assert!(environment_changed(Some(a),Some(WindowEnvironment{dpi:144,..a})));assert!(environment_changed(Some(a),Some(WindowEnvironment{monitor:2,..a})));assert!(environment_changed(Some(a),Some(WindowEnvironment{work:Rect{top:0,..a.work},..a})));assert!(environment_changed(Some(a),None));}
}

#[cfg(test)] mod visibility_hotfix_tests{use super::*;
 #[test] fn hidden_or_minimized_owner_is_not_presentable(){assert!(!visibility_allowed(false,false));assert!(!visibility_allowed(false,true));}
 #[test] fn cloaked_owner_is_not_presentable(){assert!(!visibility_allowed(true,true));assert!(visibility_allowed(true,false));}
}

#[cfg(test)] mod hotfix103_tests{use super::*;
 #[test]fn event_bursts_do_not_expire_coverage(){let now=Instant::now();let mut since=None;for _ in 0..100{assert!(!coverage_expired(&mut since,now));}assert!(!coverage_expired(&mut since,now+Duration::from_millis(649)));assert!(coverage_expired(&mut since,now+Duration::from_millis(650)));}
 #[test]fn settings_ownership_stays_identity_gated(){assert!(settings_owner_allowed(true,true,true));assert!(!settings_owner_allowed(false,true,true));assert!(!settings_owner_allowed(true,false,false));assert!(settings_owner_allowed(false,true,false));}
}
