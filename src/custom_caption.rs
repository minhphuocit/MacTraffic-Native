//! Custom caption geometry: read-only UI Automation on a persistent MTA worker.
//! COM interfaces use the documented Windows x64 ABI. No external crates.
use super::*;
#[path="browser_spacer.rs"] mod browser_spacer;
use std::{ffi::c_void, ptr::null_mut, cell::RefCell, sync::mpsc, time::{Duration, Instant}};
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct Request { pub hwnd:H, pub pid:u32, pub frame:Rect, pub dpi:u32, pub browser:bool, pub native_probe:bool, pub hint:Option<Rect> }
fn probe_cache_ttl(verified:bool,_browser:bool,native_probe:bool)->Duration{
 if verified{Duration::from_secs(5)}else if native_probe{Duration::from_millis(150)}else{Duration::from_millis(500)}
}
#[cfg(test)] mod recovery_cache_tests{use super::*;
 #[test]fn failure_cache_expires_quickly(){assert_eq!(probe_cache_ttl(false,true,false),Duration::from_millis(500));assert_eq!(probe_cache_ttl(false,false,false),Duration::from_millis(500));assert_eq!(probe_cache_ttl(false,false,true),Duration::from_millis(150));assert_eq!(probe_cache_ttl(true,true,false),Duration::from_secs(5));}
}
struct Worker { tx:mpsc::SyncSender<(Request,bool)>, rx:mpsc::Receiver<(Request,Option<Rect>)>, pending:Option<Request>, results:Vec<(Request,Option<Rect>,Instant)> }
thread_local! {static WORKER: RefCell<Option<Worker>>=RefCell::new(None);}
pub(super) fn poll(req:Request,trace:bool)->Option<Rect>{
    WORKER.with(|cell|{
        let mut slot=cell.borrow_mut();
        if slot.is_none(){
            let (tx,rx)=mpsc::sync_channel::<(Request,bool)>(1);
            let (out,ret)=mpsc::channel();
            if std::thread::Builder::new().name("caption-uia".into()).spawn(move||{
                let mut uia=UiaWorker::new("caption");
                while let Ok((r,t))=rx.recv(){
                    let started=Instant::now();
                    let value=unsafe{if r.browser||r.native_probe { quick_generic_hint(r).or_else(||verified_native(r)).or_else(||verified_dwm_hint(r)).or_else(||verified_hit_strip(r,t)).or_else(||query(r,t,&mut uia)) } else {query(r,t,&mut uia)}};
                    if t{log(&format!("custom_probe_complete hwnd={:#x} browser={} generic={} elapsed_ms={} verified={}",r.hwnd,r.browser,r.native_probe,started.elapsed().as_millis(),value.is_some()));}
                    if out.send((r,value)).is_err(){break;}
                    wake_manager();
                }
            }).is_err(){return None;}
            *slot=Some(Worker{tx,rx:ret,pending:None,results:Vec::new()});
        }
        let w=slot.as_mut()?;
        while let Ok((r,v))=w.rx.try_recv(){
            w.pending=None;
            w.results.retain(|(key,_,_)|*key!=r);
            w.results.push((r,v,Instant::now()));
        }
        w.results.retain(|(r,v,t)|t.elapsed()<probe_cache_ttl(v.is_some(),r.browser,r.native_probe));
        if w.results.len()>8 {let n=w.results.len()-8;w.results.drain(..n);}
        if let Some((_,v,_))=w.results.iter().find(|(r,_,_)|*r==req){return *v;}
        if w.pending.is_none()&&w.tx.try_send((req,trace)).is_ok(){w.pending=Some(req);}
        None
    })
}
pub(super) fn shutdown_workers(){
    WORKER.with(|cell|{drop(cell.borrow_mut().take());});
    browser_spacer::shutdown();
}
#[repr(C)] struct Guid { a:u32,b:u16,c:u16,d:[u8;8] }
const CLSID:Guid=Guid{a:0xff48dba4,b:0x60ef,c:0x4201,d:[0xaa,0x87,0x54,0x10,0x3e,0xef,0x59,0x4e]};
const IID:Guid=Guid{a:0x30cbe57d,b:0xd9d0,c:0x452a,d:[0xab,0x13,0x7a,0xc5,0xac,0x48,0x25,0xee]};
type P=*mut c_void;
unsafe fn slot(p:P,index:usize)->*const c_void { *(*(p as *const *const *const c_void)).add(index) }
struct Com(P);
impl Drop for Com{fn drop(&mut self){unsafe{if !self.0.is_null(){let release:unsafe extern "system" fn(P)->u32=std::mem::transmute(slot(self.0,2));release(self.0);}}}}
type Init=unsafe extern "system" fn(P,u32)->i32;
type Uninit=unsafe extern "system" fn();
type Create=unsafe extern "system" fn(*const Guid,P,u32,*const Guid,*mut P)->i32;
type Free=unsafe extern "system" fn(*mut u16);
type Len=unsafe extern "system" fn(*const u16)->u32;
struct Apartment(Uninit);
impl Drop for Apartment{fn drop(&mut self){unsafe{(self.0)();}}}
unsafe fn symbol(module:H,name:&[u8])->Option<*const c_void>{let p=GetProcAddress(module,name.as_ptr());if p.is_null(){None}else{Some(p)}}
fn role(name:&str)->Option<usize>{match name.trim().to_lowercase().as_str(){
    "minimize"|"minimize window"|"thu nhỏ"=>Some(0),
    "maximize"|"maximize window"|"restore"|"restore down"|"restore window"|"maximize/restore"|"phóng to"|"khôi phục"=>Some(1),
    "close"|"close window"|"đóng"=>Some(2),_=>None}}
unsafe fn active(r:Request)->bool{let mut pid=0;GetWindowThreadProcessId(r.hwnd,&mut pid);GetForegroundWindow()==r.hwnd&&pid==r.pid}
unsafe fn element_at(a:&Com,p:Point,req:Request,free:Free,len:Len,trace:bool)->Option<(usize,Rect)>{
    if !active(req){return None;}
    // IUIAutomation::ElementFromPoint, IUIAutomationElement property getters.
    let from:unsafe extern "system" fn(P,Point,*mut P)->i32=std::mem::transmute(slot(a.0,7));
    let mut raw=null_mut();let hr=from(a.0,p,&mut raw);let el=Com(raw);if hr<0||raw.is_null(){return None;}
    let process:unsafe extern "system" fn(P,*mut i32)->i32=std::mem::transmute(slot(el.0,20));
    let control:unsafe extern "system" fn(P,*mut i32)->i32=std::mem::transmute(slot(el.0,21));
    let name:unsafe extern "system" fn(P,*mut *mut u16)->i32=std::mem::transmute(slot(el.0,23));
    let enabled:unsafe extern "system" fn(P,*mut i32)->i32=std::mem::transmute(slot(el.0,28));
    let mut is_enabled=0;if enabled(el.0,&mut is_enabled)<0||is_enabled==0{return None;}
    let bounds:unsafe extern "system" fn(P,*mut Rect)->i32=std::mem::transmute(slot(el.0,43));
    let(mut pid,mut kind)=(0,0);if process(el.0,&mut pid)<0||pid as u32!=req.pid||control(el.0,&mut kind)<0||kind!=50000{return None;}
    let mut text=null_mut();let hr=name(el.0,&mut text);if text.is_null(){return None;}
    let n=len(text) as usize;let label=if hr>=0&&n<=128{String::from_utf16_lossy(std::slice::from_raw_parts(text,n))}else{String::new()};free(text);
    let mut r=Rect::default();if bounds(el.0,&mut r)<0{return None;}
    let id=role(&label)?;
    if trace{log(&format!("custom_uia role={} rect={:?}",id,r));}
    if !inside(r,p)||r.width()<scale(22,req.dpi)||r.width()>scale(90,req.dpi)||r.height()<scale(18,req.dpi)||r.height()>scale(56,req.dpi)||r.left<req.frame.right-scale(240,req.dpi)||r.right>req.frame.right||r.top<req.frame.top||r.bottom>req.frame.top+scale(64,req.dpi){return None;}
    Some((id,r))
}
fn verified_group(parts:[Rect;3],frame:Rect,dpi:u32)->Option<Rect>{
    let [a,b,c]=parts;
    if a.left>=b.left||b.left>=c.left||a.right.abs_diff(b.left)>scale(4,dpi) as u32||b.right.abs_diff(c.left)>scale(4,dpi) as u32{return None;}
    for r in [a,b]{if r.top.abs_diff(c.top)>scale(3,dpi) as u32||r.bottom.abs_diff(c.bottom)>scale(3,dpi) as u32{return None;}}
    let all=union(union(a,b),c);
    if all.width()<scale(90,dpi)||all.width()>scale(220,dpi)||frame.right-all.right>scale(12,dpi)||all.top<frame.top||all.bottom>frame.top+scale(64,dpi)||all.height()>scale(56,dpi){None}else{Some(all)}
}
// Keep the MTA and IUIAutomation client alive on the same worker between
// requests. COM objects returned by individual queries remain local RAII values.
// Field declaration order is intentional: release automation BEFORE apartment.
struct UiaContext{automation:Com,free:Free,len:Len,_apartment:Apartment}
impl UiaContext{
 unsafe fn new()->Option<Self>{
    static LIBS:std::sync::OnceLock<(H,H)>=std::sync::OnceLock::new();
    let &(ole,aut)=LIBS.get_or_init(||(LoadLibraryExW(wide("ole32.dll").as_ptr(),0,0x800),LoadLibraryExW(wide("oleaut32.dll").as_ptr(),0,0x800)));
    if ole==0||aut==0{return None;}
    let init:Init=std::mem::transmute(symbol(ole,b"CoInitializeEx\0")?);
    let uninit:Uninit=std::mem::transmute(symbol(ole,b"CoUninitialize\0")?);
    let create:Create=std::mem::transmute(symbol(ole,b"CoCreateInstance\0")?);
    let free:Free=std::mem::transmute(symbol(aut,b"SysFreeString\0")?);
    let len:Len=std::mem::transmute(symbol(aut,b"SysStringLen\0")?);
    if init(null_mut(),0)<0{return None;}
    let apartment=Apartment(uninit); // S_OK and S_FALSE both need one uninitialize.
    let mut raw=null_mut();let hr=create(&CLSID,null_mut(),1,&IID,&mut raw);
    let automation=Com(raw);if hr<0||raw.is_null(){return None;}
    Some(Self{automation,free,len,_apartment:apartment})
 }
}
// Construct inside each long-lived worker, never on the UI manager thread.
// No per-request client recreation, no COM pointers sent across channels.
struct UiaWorker{label:&'static str,context:Option<UiaContext>,retry_at:Instant,calls:u64}
impl UiaWorker{
 fn new(label:&'static str)->Self{Self{label,context:None,retry_at:Instant::now(),calls:0}}
 unsafe fn get(&mut self)->Option<&UiaContext>{
    if self.context.is_none(){
     if Instant::now()<self.retry_at{return None;}
     self.context=UiaContext::new();
     if self.context.is_none(){self.retry_at=Instant::now()+Duration::from_secs(15);log(&format!("uia_context_unavailable worker={} retry_ms=15000",self.label));return None;}
     log(&format!("uia_context_created worker={} lifetime=worker",self.label));
    }
    self.calls=self.calls.saturating_add(1);
    if self.calls%20==0{log(&format!("uia_context_reused worker={} requests={}",self.label,self.calls));}
    self.context.as_ref()
 }
}
unsafe fn query(req:Request,trace:bool,uia:&mut UiaWorker)->Option<Rect>{
    if !active(req){return None;}
    let ctx=uia.get()?;let a=&ctx.automation;let free=ctx.free;let len=ctx.len;
    let mut parts=[None;3];let mut budget=36;
    let f=req.frame;let d=req.dpi;
    // The samples are search seeds, never overlay coordinates. Every final
    // rectangle must be supplied by a named UIA button and validated.
    for dy in [24,32,16,40,48,12]{
        for dx in [24,72,120,18,54,90]{
            if budget==0||!active(req){return None;}budget-=1;
            let point=Point{x:f.right-scale(dx,d),y:f.top+scale(dy,d)};
            if parts.iter().flatten().any(|r|inside(*r,point)){continue;}
            if let Some((id,r))=element_at(a,point,req,free,len,trace){parts[id]=Some(r);}
            if let [Some(x),Some(y),Some(z)]=parts{
                let found=verified_group([x,y,z],f,d);
                if found.is_some()&&active(req){return found;}
            }
        }
    }
    None
}
#[cfg(test)] mod tests{use super::*;#[test]fn office_valid_group(){let f=Rect{left:0,top:24,right:1920,bottom:1080};let parts=[Rect{left:1776,top:24,right:1824,bottom:72},Rect{left:1824,top:24,right:1872,bottom:72},Rect{left:1872,top:24,right:1920,bottom:72}];assert_eq!(verified_group(parts,f,96),Some(Rect{left:1776,top:24,right:1920,bottom:72}));}#[test]fn reject_wrong_strip(){let f=Rect{left:0,top:24,right:1920,bottom:1080};let p=[Rect{left:1853,top:23,right:1875,bottom:47},Rect{left:1875,top:23,right:1898,bottom:47},Rect{left:1898,top:23,right:1921,bottom:47}];assert_eq!(verified_group(p,f,96),None);}}

// Browser-native accessibility rectangles plus independent WM_NCHITTEST verification.
// Use the worker only: a busy browser cannot stall the shared window manager.
#[repr(C)] struct TitleBarInfoEx{size:u32,rect:Rect,state:[u32;6],elements:[Rect;6]}
#[link(name="user32")] extern "system"{
 fn SendMessageTimeoutW(h:H,m:u32,w:W,l:L,flags:u32,ms:u32,result:*mut W)->L;
 fn GetWindow(h:H,cmd:u32)->H;
 fn GetSystemMenu(h:H,revert:i32)->H;
 fn GetMenuState(menu:H,item:u32,flags:u32)->u32;
 fn GetGUIThreadInfo(thread:u32,info:*mut GuiInfo)->i32;
}
#[repr(C)] struct GuiInfo{size:u32,flags:u32,active:H,focus:H,capture:H,menu_owner:H,move_size:H,caret:H,caret_rect:Rect}
unsafe fn hit_code(h:H,p:Point)->Option<W>{hit_code_with_timeout(h,p,25)}
unsafe fn hit_code_with_timeout(h:H,p:Point,timeout:u32)->Option<W>{
 if p.x<i16::MIN as i32||p.x>i16::MAX as i32||p.y<i16::MIN as i32||p.y>i16::MAX as i32{return None;}
 let packed=(((p.y as u16 as u32)<<16)|(p.x as u16 as u32)) as L;
 let mut result=0;
 if SendMessageTimeoutW(h,0x0084,0,packed,3,timeout,&mut result)==0{None}else{Some(result)}
}
unsafe fn verified_native(req:Request)->Option<Rect>{
 if !active(req){return None;}
 let mut info:TitleBarInfoEx=zeroed();info.size=size_of::<TitleBarInfoEx>() as u32;
 let mut result=0;
 if SendMessageTimeoutW(req.hwnd,0x033f,0,(&mut info as *mut TitleBarInfoEx) as L,3,40,&mut result)==0{return None;}
 let mut parts=[Rect::default();3];
 for (i,index) in [2usize,3,5].into_iter().enumerate(){
  let r=info.elements[index];
  if info.state[index]&0x18001!=0||r.width()<scale(22,req.dpi)||r.width()>scale(90,req.dpi)||r.height()<scale(18,req.dpi)||r.height()>scale(56,req.dpi){return None;}
  if !inside(req.frame,Point{x:r.left,y:r.top})||!inside(req.frame,Point{x:r.right-1,y:r.bottom-1}){return None;}
  let p=Point{x:r.left+r.width()/2,y:r.top+r.height()/2};
  if hit_code(req.hwnd,p)!=Some([8,9,20][i])||!active(req){return None;}
  parts[i]=r;
 }
 let rect=verified_group(parts,req.frame,req.dpi)?;
 log(&format!("custom_browser_native_verified hwnd={:#x} bounds={rect:?}",req.hwnd));Some(rect)
}
// Chromium can expose native hit codes even when UIA/TitleBarInfoEx are incomplete.
// Search only the top-right caption band, with a fixed time/call budget.
fn hit_role(code:W)->Option<usize>{match code{8=>Some(0),9=>Some(1),20=>Some(2),_=>None}}
struct HitBudget{deadline:Instant,remaining:u32}
impl HitBudget{
 unsafe fn get(&mut self,r:Request,p:Point)->Option<Option<usize>>{
  if self.remaining==0||Instant::now()>=self.deadline{log(&format!("custom_browser_hit_budget hwnd={:#x}",r.hwnd));return None;}
  if !active(r)||!inside(r.frame,p){return None;}
  self.remaining-=1;Some(hit_role(hit_code(r.hwnd,p)?))
 }
}
// Find an exact contiguous role interval around a sampled center using binary
// search within the bounded coarse brackets. No guessed rectangle is accepted.
unsafe fn edge(b:&mut HitBudget,r:Request,y:i32,role:usize,mut good:i32,mut bad:i32)->Option<i32>{
 while good.abs_diff(bad)>1{let mid=good+(bad-good)/2;
  if b.get(r,Point{x:mid,y})?==Some(role){good=mid;}else{bad=mid;}
 }Some(good)
}
unsafe fn verified_hit_strip(r:Request,trace:bool)->Option<Rect>{
 if !active(r){return None;}
 let f=r.frame;let d=r.dpi;
 let mut b=HitBudget{deadline:Instant::now()+Duration::from_millis(500),remaining:300};
 let step=scale(6,d).max(2);let start=(f.right-scale(220,d)).max(f.left+scale(100,d));
 let mut observed=0u8;
 for dy in [15,24,8,32,40,48,4,20]{
  let y=f.top+scale(dy,d);if y>=f.bottom{continue;}
  let mut first=[None;3];let mut last=[None;3];
  let mut x=start;let mut samples=Vec::new();
  while x<f.right{
   let hit=b.get(r,Point{x,y})?;samples.push((x,hit));
   if let Some(role)=hit{observed|=1<<role;first[role].get_or_insert(x);last[role]=Some(x);}
   x+=step;
  }
  let [Some(a),Some(c),Some(e)]=first else{continue;};
  let [Some(aa),Some(cc),Some(ee)]=last else{continue;};
  if !(a<=aa&&aa<c&&c<=cc&&cc<e&&e<=ee){continue;}
  let lo=[a,c,e];let hi=[aa,cc,ee];
  if samples.iter().any(|(x,hit)|(0..3).any(|i|*x>=lo[i]&&*x<=hi[i]&&*hit!=Some(i))){continue;}
  let mut parts=[Rect::default();3];let mut centers=[0;3];
  for i in 0..3{
   let l=if lo[i]-step<f.left{lo[i]}else{edge(&mut b,r,y,i,lo[i],lo[i]-step)?};
   let rr=if hi[i]+step>=f.right{let x=f.right-1;if b.get(r,Point{x,y})?==Some(i){x}else{edge(&mut b,r,y,i,hi[i],x)?}}else{edge(&mut b,r,y,i,hi[i],hi[i]+step)?};
   if rr<l||rr-l+1<scale(22,d)||rr-l+1>scale(90,d){return None;}
   centers[i]=l+(rr-l)/2;parts[i]=Rect{left:l,top:y,right:rr+1,bottom:y+1};
  }
  // Sample vertical band using all three role centers. Refine each edge to a pixel.
  let same=|b:&mut HitBudget,yy:i32|->Option<bool>{
   for i in 0..3{if b.get(r,Point{x:centers[i],y:yy})?!=Some(i){return Some(false);}}Some(true)
  };
  let mut top=y;let mut bottom=y;
  for yy in (f.top..y).rev().step_by(step as usize){if !same(&mut b,yy)?{break;}top=yy;}
  for yy in ((y+1)..(f.top+scale(64,d)).min(f.bottom)).step_by(step as usize){if !same(&mut b,yy)?{break;}bottom=yy;}
  for _ in 0..step{if top<=f.top||!same(&mut b,top-1)?{break;}top-=1;}
  for _ in 0..step{if bottom+1>=f.bottom||bottom+1>=f.top+scale(64,d)||!same(&mut b,bottom+1)?{break;}bottom+=1;}
  if bottom-top+1<scale(18,d)||bottom-top+1>scale(56,d){continue;}
  for part in &mut parts{part.top=top;part.bottom=bottom+1;}
  let Some(rect)=verified_group(parts,f,d)else{continue;};
  if !active(r){return None;}
  log(&format!("custom_browser_hit_verified hwnd={:#x} bounds={rect:?} calls={}",r.hwnd,300-b.remaining));return Some(rect);
 }
 if trace{log(&format!("custom_browser_hit_unverified hwnd={:#x} roles_mask={observed:#x} calls={}",r.hwnd,300-b.remaining));}None
}
#[cfg(test)]mod browser_hit_tests{use super::*;
 #[test]fn hit_roles(){assert_eq!(hit_role(8),Some(0));assert_eq!(hit_role(9),Some(1));assert_eq!(hit_role(20),Some(2));assert_eq!(hit_role(1),None);assert_eq!(hit_role(2),None);}
}
// Fast path adapted from the v0.2.12 browser geometry probe. DWM bounds
// provide search seeds; the actual row and horizontal edges must return
// the correct role codes. In particular, never use DWM alone as a cover.
unsafe fn verified_dwm_hint(r:Request)->Option<Rect>{
 let raw=r.hint?;let f=r.frame;let d=r.dpi;
 let base=Rect{left:raw.left.max(f.left),top:raw.top.max(f.top),right:raw.right.min(f.right),bottom:raw.bottom.min(f.bottom)};
 if base.width()<scale(90,d)||base.width()>scale(220,d)||raw.height()<scale(18,d)||raw.height()>scale(56,d)||base.top>=f.bottom||f.right-base.right>scale(12,d){return None;}
 let xs=[base.left+base.width()/6,base.left+base.width()/2,base.left+base.width()*5/6];
 let lower=(base.top+raw.height()+scale(12,d)).min(f.bottom).min(f.top+scale(64,d));
 if lower<=base.top{return None;}
 let mut b=HitBudget{deadline:Instant::now()+Duration::from_millis(350),remaining:240};
 let row=|b:&mut HitBudget,y:i32|->Option<bool>{for (i,x) in xs.iter().enumerate(){if b.get(r,Point{x:*x,y})?!=Some(i){return Some(false);}}Some(true)};
 let step=scale(2,d).max(1);let mut start=None;let mut end=base.top;
 for y in (base.top..lower).step_by(step as usize){if row(&mut b,y)?{if start.is_none(){start=Some(y);}end=y;}else if start.is_some(){break;}}
 let mut top=start?;let mut bottom=end+1;
 while top>base.top&&row(&mut b,top-1)?{top-=1;}
 while bottom<lower&&row(&mut b,bottom)?{bottom+=1;}
 if bottom-top<scale(18,d)||bottom-top>scale(56,d){return None;}
 let mid=(top+bottom)/2;
 // Every sample must be a caption role, in minimize-maximize-close order.
 let left_limit=(base.left-scale(8,d)).max(f.left);let right_limit=(base.right+scale(8,d)).min(f.right);
 let mut l=xs[0];while l>left_limit&&b.get(r,Point{x:l-1,y:mid})?==Some(0){l-=1;}
 let mut rr=xs[2]+1;while rr<right_limit&&b.get(r,Point{x:rr,y:mid})?==Some(2){rr+=1;}
 let mut roles=0u8;let mut last=0usize;
 for x in (l..rr).step_by(scale(3,d).max(1) as usize){let role=b.get(r,Point{x,y:mid})??;if role<last{return None;}last=role;roles|=1<<role;}
 let rect=Rect{left:l,top,right:rr,bottom};
 if roles!=7||rect.width()<scale(90,d)||rect.width()>scale(220,d)||f.right-rect.right>scale(12,d)||!active(r){return None;}
 log(&format!("custom_browser_dwm_verified hwnd={:#x} bounds={rect:?}",r.hwnd));Some(rect)
}
// A DWM caption-button rectangle is the full visual envelope, whereas a
// hit-test group can be narrower than the visible stock glyphs. Only generic
// allowlisted windows use this fast path, and only with three ordered role hits.
// Keep the visual DWM envelope distinct from the actual hit row. On Win10
// classic frames DWM reports 146x30 while the live role strip can be 103x22.
fn generic_hint_candidates(b:Rect,dpi:u32)->[Rect;2]{
 let compact=Rect{left:(b.right-scale(103,dpi)).max(b.left),top:(b.bottom-scale(22,dpi)).max(b.top),..b};
 [compact,b]
}
unsafe fn quick_generic_hint(r:Request)->Option<Rect>{
 if !r.native_probe||!active(r){return None;}
 let raw=r.hint?;let f=r.frame;
 let b=Rect{left:raw.left.max(f.left),top:raw.top.max(f.top),right:raw.right.min(f.right),bottom:raw.bottom.min(f.bottom)};
 if candidate_layout(b,f,f,r.dpi).is_none()||b.width()<scale(90,r.dpi)||f.right-b.right>scale(12,r.dpi){return None;}
 let started=Instant::now();
 for row in generic_hint_candidates(b,r.dpi){
  if row.width()<scale(90,r.dpi)||row.height()<scale(18,r.dpi){continue;}
  let mut valid=true;
  for (i,role) in [8,9,20].into_iter().enumerate(){
   if started.elapsed()>=Duration::from_millis(18)||!active(r){return None;}
   let pt=Point{x:row.left+row.width()*(2*i as i32+1)/6,y:row.top+row.height()/2};
   if hit_code_with_timeout(r.hwnd,pt,5)!=Some(role){valid=false;break;}
  }
  if valid&&active(r){log(&format!("generic_fast_hint hwnd={:#x} row={row:?} envelope={b:?}",r.hwnd));return Some(row);}
 }
 None
}
// Align the left sprite to the independently verified role strip, not the
// visual envelope (which may include the top resize border). Keep the full
// envelope's left edge as the exclusion boundary for the stock cover.
fn generic_left_reference(cover:Layout,verified:Rect)->Layout{
 let h=scale(22,cover.dpi).max(verified.height()).min(cover.h);
 let y=(verified.top+(verified.height()-h)/2).clamp(cover.pos.y,cover.pos.y+cover.h-h);
 Layout{pos:Point{x:cover.pos.x,y},h,..cover}
}
fn generic_visual_envelope(verified:Rect,hint:Option<Rect>,frame:Rect,work:Rect,dpi:u32)->Option<Rect>{
 // A maximized DWM envelope can include invisible resize borders.
 // Clip at most 8 DIP of verified edges, never a role center or a reserved bar.
 let clipped=Rect{left:verified.left.max(frame.left).max(work.left),top:verified.top.max(frame.top).max(work.top),right:verified.right.min(frame.right).min(work.right),bottom:verified.bottom.min(frame.bottom).min(work.bottom)};
 if [clipped.left-verified.left,clipped.top-verified.top,verified.right-clipped.right,verified.bottom-clipped.bottom].into_iter().any(|n|n>scale(8,dpi)){return None;}
 if !(0..3).all(|i|inside(clipped,Point{x:verified.left+verified.width()*(2*i+1)/6,y:verified.top+verified.height()/2})){return None;}
 let verified=clipped;
 candidate_layout(verified,frame,work,dpi)?;
 let Some(raw)=hint else{return Some(verified);};
 let b=Rect{left:raw.left.max(frame.left).max(work.left),top:raw.top.max(frame.top).max(work.top),right:raw.right.min(frame.right).min(work.right),bottom:raw.bottom.min(frame.bottom).min(work.bottom)};
 if candidate_layout(b,frame,work,dpi).is_none()||b.width()<scale(90,dpi)||frame.right-b.right>scale(12,dpi){return Some(verified);}
 // A verified role group must overlap the OS-provided envelope and agree on
 // its right edge and vertical band. Fail closed on contradictory geometry.
 if !overlaps(b,verified)||b.right.abs_diff(verified.right)>scale(12,dpi) as u32||b.top.abs_diff(verified.top)>scale(12,dpi) as u32||b.bottom.abs_diff(verified.bottom)>scale(12,dpi) as u32{return None;}
 let full=union(b,verified);
 candidate_layout(full,frame,work,dpi)?;Some(full)
}
fn candidate_layout(r:Rect,frame:Rect,work:Rect,dpi:u32)->Option<Layout>{
 let w=r.width();let h=r.height();
 if dpi>480||w<scale(66,dpi)||w>scale(220,dpi)||h<scale(18,dpi)||h>scale(56,dpi)||r.left<frame.left+scale(100,dpi)||r.right>frame.right||r.top<frame.top||r.bottom>frame.bottom||r.bottom>frame.top+scale(64,dpi){return None;}
 // Never paint over the reserved MyFinder bar or across a monitor edge.
 if r.top<work.top||r.left<work.left||r.right>work.right||r.bottom>work.bottom{return None;}
 Some(Layout{pos:Point{x:r.left,y:r.top},w,h,dpi,work_top:Some(work.top),fallback_row:false,left_slot:false})
}
#[cfg(test)]
fn left_candidate(right:Layout,frame:Rect,work:Rect)->Option<Layout>{
 let w=scale(66,right.dpi);let h=scale(22,right.dpi);
 let pos=Point{x:frame.left+scale(3,right.dpi),y:right.pos.y+(right.h-h)/2};
 if pos.x<work.left||pos.y<work.top||pos.x+w>work.right||pos.y+h>work.bottom||pos.x+w+scale(12,right.dpi)>right.pos.x||pos.y+h>frame.bottom{return None;}
 Some(Layout{pos,w,h,left_slot:true,..right})
}
// Test each candidate independently of the exact-left slot. A maximized
// Office frame can include a non-visible border outside the work area.
fn ribbon_candidate(right:Layout,frame:Rect,work:Rect,offset:i32)->Option<Layout>{
 let w=scale(66,right.dpi);let h=scale(22,right.dpi);
 let start=frame.left.max(work.left);
 let end=frame.right.min(work.right);
 let pos=Point{x:start+scale(offset,right.dpi),y:right.pos.y+(right.h-h)/2};
 let limit=(start+(end-start)*45/100).min(right.pos.x-scale(12,right.dpi)).min(end);
 if pos.x<start||pos.x+w>limit||pos.y<frame.top.max(work.top)||pos.y+h>frame.bottom.min(work.bottom){return None;}
 Some(Layout{pos,w,h,left_slot:true,..right})
}
// Search close to the edge first, with denser offsets to avoid skipping a
// small gap between the title and Search. Never cover actual controls.
// Coarse horizontal sweep first; only then fill gaps and try alternate rows.
// Excel/PPT title areas can extend well beyond Word's 80-DIP slot.
const OFFICE_OFFSETS:[i32;35]=[3,8,80,112,144,176,208,240,272,304,336,368,400,448,512,576,640,24,40,56,72,88,104,120,136,152,168,184,200,216,232,248,264,280,296];
fn office_scan_point(i:usize)->Option<(i32,i32)>{
 let offsets=OFFICE_OFFSETS.len();
 let dy=*[0,2,-2].get(i/offsets)?;
 Some((OFFICE_OFFSETS[i%offsets],dy))
}
const NATIVE_OFFSETS:[i32;30]=[3,8,24,40,56,72,80,88,104,112,120,136,144,152,168,176,184,200,208,216,232,248,264,280,296,312,336,368,400,448];
#[derive(Clone,Copy,Debug,Default)]
pub(super) struct RibbonScan{index:usize}
impl RibbonScan{
 pub(super) fn step(&mut self,h:H,right:Layout,frame:Rect,work:Rect,native:bool)->(Option<Layout>,bool){
  let total=if native{NATIVE_OFFSETS.len()*3}else{OFFICE_OFFSETS.len()*3};
  if self.index>=total{return (None,true);}
  if unsafe{GetForegroundWindow()}!=h{return (None,false);}
  let i=self.index;self.index+=1;
  let (offset,dy)=if native{(NATIVE_OFFSETS[i/3],[4,0,2][i%3])}else{office_scan_point(i).expect("bounded scan index")};
  let candidate=ribbon_candidate(right,frame,work,offset).and_then(|l|shift_ribbon(l,frame,work,dy));
  if let Some(l)=candidate{
   // Keep the sprite within the title row, including when checking
   // a shifted candidate. Native row may need its 4-DIP resize clearance.
   let row_ok=native||(l.pos.y>=right.pos.y&&l.pos.y+l.h<=right.pos.y+right.h);
   if row_ok{
    let p=Point{x:l.pos.x+l.w/2,y:l.pos.y+l.h/2};
    let code=unsafe{hit_code(h,p)};
    let result=if code!=Some(2){Err("center_hit")}else if native{blank_caption_result(h,l)}else{office_blank_caption(h,l)};
    match result{
     Ok(())=>{log(&format!("ribbon_slot_verified hwnd={h:#x} offset_dip={offset} shift_dip={dy} rect={l:?}"));return (Some(l),true);}
     Err(reason)=>log(&format!("ribbon_slot_reject hwnd={h:#x} offset_dip={offset} shift_dip={dy} center_hit={code:?} reason={reason}")),
    }
   }else{log(&format!("ribbon_slot_reject hwnd={h:#x} offset_dip={offset} shift_dip={dy} reason=row_bounds"));}
  }else{log(&format!("ribbon_slot_reject hwnd={h:#x} offset_dip={offset} shift_dip={dy} reason=geometry frame={frame:?} work={work:?} right={right:?}"));}
  let done=self.index>=total;
  if done{log(&format!("ribbon_slot_unavailable hwnd={h:#x} fallback_right=true"));}
  (None,done)
 }
}
const GENERIC_OFFSETS:[i32;32]=[80,96,112,128,144,160,176,192,208,224,240,8,24,40,56,64,72,88,104,120,136,152,168,184,200,216,232,256,272,304,336,368];
impl RibbonScan{
 fn step_generic(&mut self,h:H,right:Layout,cover:Layout,frame:Rect,work:Rect)->(Option<Layout>,bool){
  let start=Instant::now();
  let total=GENERIC_OFFSETS.len()*3;
  for _ in 0..12{
   if self.index>=total||unsafe{GetForegroundWindow()}!=h||start.elapsed()>=Duration::from_millis(30){break;}
   let i=self.index;self.index+=1;
   let off=GENERIC_OFFSETS[i%GENERIC_OFFSETS.len()];
   let dy=[0,4,-2][i/GENERIC_OFFSETS.len()];
   let Some(l)=ribbon_candidate(right,frame,work,off).and_then(|l|shift_ribbon(l,frame,work,dy))else{continue;};
   if l.pos.y<cover.pos.y||l.pos.y+l.h>cover.pos.y+cover.h{continue;}
   // Check pixels before any cross-process hit testing: text and icons fail fast.
   if office_blank_caption(h,l).is_ok(){log(&format!("generic_left_fast hwnd={h:#x} offset_dip={off} rect={l:?}"));return (Some(l),true);}
  }
  (None,self.index>=total)
 }
}
fn shift_ribbon(mut l:Layout,frame:Rect,work:Rect,dy:i32)->Option<Layout>{
 l.pos.y+=scale(dy,l.dpi);
 if l.pos.y<frame.top||l.pos.y<work.top||l.pos.y+l.h>frame.bottom||l.pos.y+l.h>work.bottom{return None;}
 Some(l)
}
// Probe the target directly, even after our alpha-only dots are visible.
// HTCAPTION (2) means a drag area; HTCLIENT, menus, tabs and caption buttons fail.
pub(super) fn caption_hits(h:H,l:Layout)->bool{unsafe{
 let started=Instant::now();
 for yy in [1,3,5]{for xx in [1,2,3,4,5,6,7]{
  if started.elapsed()>Duration::from_millis(65)||GetForegroundWindow()!=h{return false;}
  let p=Point{x:l.pos.x+(l.w-1)*xx/8,y:l.pos.y+(l.h-1)*yy/6};
  if hit_code(h,p)!=Some(2){return false;}
 }}true
}}
// One bounded, local GDI copy replaces dozens of synchronous GetPixel calls.
// Never capture a full screen, save a bitmap or sample Explorer/Console/Notepad.
// DIB memory is released immediately. Ignore the unused alpha byte.
fn flat_ribbon_pixels(pixels:&[u32],w:usize,h:usize)->bool{
 if w<3||h<3||pixels.len()!=w*h{return false;}
 for row in pixels.chunks_exact(w){
  let base=row[1];
  for &color in &row[1..w-1]{
   if [0,8,16].into_iter().any(|shift|((color>>shift)&255).abs_diff((base>>shift)&255)>16){return false;}
  }
 }
 true
}
fn owned_ribbon(h:H,l:Layout)->bool{unsafe{
 if GetForegroundWindow()!=h{return false;}
 // Preserve the previous ownership check at every sampled grid point.
 for yy in [1,2,3,4,5]{
  let y=l.pos.y+(l.h-1)*yy/6;
  for x in (l.pos.x+1..l.pos.x+l.w-1).step_by(scale(4,l.dpi).max(1) as usize){
   if GetAncestor(WindowFromPoint(Point{x,y}),2)!=h{return false;}
  }
 }
 GetForegroundWindow()==h
}}
fn read_local_pixels(l:Layout)->Result<Vec<u32>,&'static str>{unsafe{
 if l.w<3||l.h<3||l.w>1600||l.h>280||l.w*l.h>240000{return Err("pixel_bounds");}
 let screen=GetDC(0);if screen==0{return Err("no_dc");}
 let mem=CreateCompatibleDC(screen);
 if mem==0{ReleaseDC(0,screen);return Err("no_mem_dc");}
 let info=BitmapInfo{size:40,width:l.w,height:-l.h,planes:1,bits:32,compression:0,image_size:0,xppm:0,yppm:0,used:0,important:0,colors:0};
 let mut bits=null_mut();let bmp=CreateDIBSection(screen,&info,0,&mut bits,0,0);
 let result=if bmp==0||bits.is_null(){Err("no_local_dib")}else{
  let old=SelectObject(mem,bmp);
  if old==0||old == -1{Err("select_dib")}else{
   let copied=BitBlt(mem,0,0,l.w,l.h,screen,l.pos.x,l.pos.y,0x00cc0020)!=0;
   let flushed=GdiFlush()!=0;
   let r=if !copied||!flushed{Err("local_copy_failed")}else{Ok(std::slice::from_raw_parts(bits.cast::<u32>(),(l.w*l.h) as usize).to_vec())};
   SelectObject(mem,old);r
  }
 };
 if bmp!=0{DeleteObject(bmp);}DeleteDC(mem);ReleaseDC(0,screen);result
}}
fn local_ribbon_flat(l:Layout)->Result<(),&'static str>{
 if l.w>330||l.h>110{return Err("pixel_bounds");}
 let pixels=read_local_pixels(l)?;
 if flat_ribbon_pixels(&pixels,l.w as usize,l.h as usize){Ok(())}else{Err("nonflat_color")}
}
// Optional visual relocation, only within the already verified caption row.
// No application style changes, injection, input forwarding, or disk images.
struct OfficeShift{layout:Layout,pixels:Vec<u32>,original_slot:Layout}
fn prefix_hits(h:H,slot:Layout,first:i32)->bool{unsafe{
 let started=Instant::now();
 for y in [slot.pos.y+slot.h/4,slot.pos.y+slot.h/2,slot.pos.y+slot.h*3/4]{
  for x in (first+scale(4,slot.dpi)..slot.pos.x+slot.w).step_by(scale(8,slot.dpi).max(1) as usize){
   if GetForegroundWindow()!=h||started.elapsed()>Duration::from_millis(220){return false;}
   // Only title drag and system icon; reject client, resize, buttons and timeout.
   if !matches!(hit_code(h,Point{x,y}),Some(2)|Some(3)){return false;}
  }
 }
 true
}}
fn exact_button_hits(h:H,l:Layout)->bool{unsafe{
 (0..3).all(|i|matches!(hit_code(h,Point{x:l.pos.x+scale(11+i*22,l.dpi),y:l.pos.y+l.h/2}),Some(2)|Some(3)))
}}
fn shifted_pixels(source:&[u32],width:usize,height:usize,delta:usize)->Option<Vec<u32>>{
 if delta<3||width<=delta||height<3||source.len()!=width*height{return None;}
 let prefix=width-delta;
 for r in source.chunks_exact(width){let row=&r[prefix..];let base=row[1];
  if row[1..delta-1].iter().any(|&color|[0,8,16].into_iter().any(|shift|((color>>shift)&255).abs_diff((base>>shift)&255)>16)){return None;}
 }
 let mut out=vec![0;width*height];
 for (src,dst) in source.chunks_exact(width).zip(out.chunks_exact_mut(width)){
  // Use the actual row background and move the original title/icon pixels.
  let bg=src[prefix+delta/2]|0xff000000;
  dst[..delta].fill(bg);
  for (d,s) in dst[delta..].iter_mut().zip(&src[..prefix]){*d=*s|0xff000000;}
 }
 Some(out)
}
fn make_office_shift(h:H,slot:Layout,right:Layout,frame:Rect,work:Rect)->Result<(Layout,OfficeShift),&'static str>{
 // Prefer 8 DIP. Only extend into additional pixels if their hit region
 // and color are verified; otherwise retain the user-tested 3 DIP layout.
 let pad=scale(LEFT_MARGIN_DIP,slot.dpi)-scale(3,slot.dpi);
 make_office_shift_inner(h,slot,right,frame,work,pad).or_else(|reason|{log(&format!("office_margin_fallback hwnd={h:#x} reason={reason} keep_margin_dip=3"));make_office_shift_inner(h,slot,right,frame,work,0)})
}
fn make_office_shift_inner(h:H,slot:Layout,right:Layout,frame:Rect,work:Rect,pad:i32)->Result<(Layout,OfficeShift),&'static str>{
 let start=frame.left.max(work.left)+scale(3,slot.dpi);
 let prefix=slot.pos.x-start;
 let extended=Layout{w:slot.w+pad,..slot};
 if prefix<scale(12,slot.dpi)||prefix>scale(300,slot.dpi){return Err("prefix_bounds");}
 let buttons=Layout{pos:Point{x:start+pad,y:slot.pos.y},..slot};
 // Keep away from the outer resize border, but include the full visible glyphs.
 let top=right.pos.y+scale(4,slot.dpi);
 let bottom=right.pos.y+right.h-scale(4,slot.dpi);
 let strip=Layout{pos:Point{x:start,y:top},w:prefix+extended.w,h:bottom-top,..slot};
 if top<frame.top.max(work.top)||bottom>frame.bottom.min(work.bottom)||top>slot.pos.y||bottom<slot.pos.y+slot.h||strip.pos.x+strip.w>right.pos.x-scale(12,slot.dpi){return Err("strip_geometry");}
 if (pad>0&&!caption_hits(h,extended))||!exact_button_hits(h,buttons)||!prefix_hits(h,extended,start)||!owned_ribbon(h,strip){return Err("prefix_hit_or_owner");}
 let pixels=read_local_pixels(strip)?;
 if !owned_ribbon(h,strip){return Err("prefix_focus_changed");}
 let pixels=shifted_pixels(&pixels,strip.w as usize,strip.h as usize,extended.w as usize).ok_or("expanded_slot_nonflat")?;
 Ok((buttons,OfficeShift{layout:strip,pixels,original_slot:extended}))
}
pub(super) fn blank_caption(h:H,l:Layout)->bool{blank_caption_result(h,l).is_ok()}
fn blank_caption_result(h:H,l:Layout)->Result<(),&'static str>{
 if !caption_hits(h,l){return Err("hit_grid");}
 if !owned_ribbon(h,l){return Err("foreign_or_focus_before");}
 let started=Instant::now();
 let result=local_ribbon_flat(l);
 if !owned_ribbon(h,l){return Err("foreign_or_focus_after");}
 log(&format!("ribbon_slot_color hwnd={h:#x} rect={l:?} elapsed_ms={} result={result:?}",started.elapsed().as_millis()));
 result
}
// Reject visibly occupied regions before spending 21 cross-process hit tests.
// The same checks as the native path are required before accepting a slot.
fn office_blank_caption(h:H,l:Layout)->Result<(),&'static str>{
 if !owned_ribbon(h,l){return Err("foreign_or_focus_before");}
 let started=Instant::now();
 let result=local_ribbon_flat(l);
 log(&format!("ribbon_slot_color hwnd={h:#x} rect={l:?} elapsed_ms={} result={result:?}",started.elapsed().as_millis()));
 result?;
 if !caption_hits(h,l){return Err("hit_grid");}
 if !owned_ribbon(h,l){return Err("foreign_or_focus_after");}
 Ok(())
}
// Exact/near-edge browser slots only. No scanning through tabs or controls.
fn browser_slot_geometry(right:Layout,frame:Rect,work:Rect,offset:i32)->Option<Layout>{
 if ![3,8].contains(&offset){return None;}
 let slot=ribbon_candidate(right,frame,work,offset)?;
 if slot.pos.y<right.pos.y||slot.pos.y+slot.h>right.pos.y+right.h{return None;}
 Some(slot)
}
fn browser_left_slot(h:H,right:Layout,frame:Rect,work:Rect)->Option<Layout>{
 for offset in [3,8]{
  if unsafe{GetForegroundWindow()}!=h{return None;}
  if let Some(slot)=browser_slot_geometry(right,frame,work,offset){
   match office_blank_caption(h,slot){
    Ok(())=>return Some(slot),
    Err(reason)=>log(&format!("browser_left_reject hwnd={h:#x} offset_dip={offset} reason={reason}")),
   }
  }
 }
 None
}
// Only already-admitted generic frames use this surface. The client
// boundary (or the verified Settings drag band) and hit roles protect controls.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
struct CenteredBand{layout:Layout,text:Rect,buttons:Layout,edge_y:i32}
#[cfg(test)]
fn centered_band_geometry(frame:Rect,work:Rect,cover:Layout,client_top:i32)->Option<CenteredBand>{centered_band_geometry_for(frame,work,cover,client_top,false)}
fn centered_band_geometry_for(frame:Rect,work:Rect,cover:Layout,client_top:i32,settings:bool)->Option<CenteredBand>{
 let d=cover.dpi;
 let start=frame.left.max(work.left)+if settings{0}else{scale(3,d)};
 let bottom=cover.pos.y+cover.h;
 let base_top=cover.pos.y.max(frame.top).max(work.top);
 let top=base_top+if settings||bottom-base_top<scale(24,d){0}else{scale(2,d)};
 // Settings samples an unobstructed client-body strip, so it needs no gap.
 let end=cover.pos.x-if settings{0}else{scale(12,d)};
 if bottom>client_top||bottom>frame.bottom.min(work.bottom)||end>work.right||bottom-top<scale(22,d)||end-start<scale(160,d)||end-start>16384||(end-start) as i64*(bottom-top) as i64>1_000_000{return None;}
 let layout=Layout{pos:Point{x:start,y:top},w:end-start,h:bottom-top,..cover};
 let buttons=Layout{pos:Point{x:frame.left.max(work.left)+scale(LEFT_MARGIN_DIP,d),y:top+(layout.h-scale(22,d))/2},w:scale(66,d),h:scale(22,d),left_slot:true,..cover};
 let center=frame.left.max(work.left)+(frame.right.min(work.right)-frame.left.max(work.left))/2;
 let half=(center-(buttons.pos.x+buttons.w+scale(12,d))).min(end-center-scale(4,d)).min(scale(320,d));
 if half<scale(35,d)||buttons.pos.x<start||buttons.pos.x+buttons.w>end{return None;}
 // Leave the original top resize/frame strip visible in both overlays.
 // Limit the transparent strip to five DIP from the actual visible frame.
 let edge_y=if settings{top}else{(frame.top+scale(5,d)).max(work.top).min(buttons.pos.y+scale(2,d)).max(cover.pos.y)};
 Some(CenteredBand{layout,text:Rect{left:center-half-start,top:0,right:center+half-start,bottom:layout.h},buttons,edge_y})
}
fn centered_button_hits(h:H,l:Layout)->bool{unsafe{
 let start=Instant::now();
 (0..3).all(|i|GetForegroundWindow()==h&&start.elapsed()<Duration::from_millis(20)&&matches!(hit_code_with_timeout(h,Point{x:l.pos.x+scale(11+i*22,l.dpi),y:l.pos.y+l.h/2},5),Some(2)|Some(3)))
}}
// Settings uses an extended client frame. The DWM button envelope is still
// verified before this function is called. Do not assume arbitrary client
// pixels are safe: only allow the top 40 DIP of a positively admitted
// Settings frame, and require the same live title drag hit roles.
fn centered_client_limit(frame:Rect,cover:Layout,client_top:i32,settings:bool)->Option<i32>{
 let bottom=cover.pos.y+cover.h;
 if bottom<=client_top{return Some(client_top);}
 if settings&&cover.pos.y>=frame.top&&bottom<=frame.top+scale(40,cover.dpi){Some(bottom)}else{None}
}
fn settings_frame(h:H)->bool{
 match class_name(h).as_str(){
  "ApplicationFrameWindow"=>has_settings_child(h)||query_name(identity(h).1).is_ok_and(|n|n=="systemsettings.exe"),
  "Windows.UI.Core.CoreWindow"=>query_name(identity(h).1).is_ok_and(|n|n=="systemsettings.exe"),
  _=>false,
 }
}
fn centered_band(h:H,frame:Rect,work:Rect,cover:Layout,settings:bool)->Option<CenteredBand>{unsafe{
 if style(h)&WS_CAPTION!=WS_CAPTION||GetForegroundWindow()!=h{return None;}
 let mut r=Rect::default();let mut origin=Point::default();
 if GetClientRect(h,&mut r)==0||r.width()<=0||r.height()<=0||ClientToScreen(h,&mut origin)==0{return None;}
 // Only the admitted Settings identity may use the extended client title.
 if settings&&!settings_frame(h){return None;}
 let limit=centered_client_limit(frame,cover,origin.y,settings)?;
 let band=centered_band_geometry_for(frame,work,cover,limit,settings)?;
 if !centered_button_hits(h,band.buttons){return None;}
 // Check drag area at middle and far right; client controls fail closed.
 let y=band.buttons.pos.y+band.buttons.h/2;
 for x in [band.layout.pos.x+(band.text.left+band.text.right)/2,band.layout.pos.x+band.layout.w-scale(8,cover.dpi)]{
  if hit_code_with_timeout(h,Point{x,y},5)!=Some(2)||GetAncestor(WindowFromPoint(Point{x,y}),2)!=h{return None;}
 }
 if settings&&limit!=origin.y{
  // Check more of the extended title row so the cover cannot overlap a
  // client toolbar. Do not accept HTCLIENT or unknown roles.
  for i in [1,3,5,7]{let x=band.layout.pos.x+band.layout.w*i/8;
   if hit_code_with_timeout(h,Point{x,y},5)!=Some(2)||GetAncestor(WindowFromPoint(Point{x,y}),2)!=h{return None;}
  }
  log(&format!("generic_settings_title_verified hwnd={h:#x} client_top={} bottom={limit}",origin.y));
 }
 if GetForegroundWindow()!=h{return None;}
 Some(band)
}}
// The classic caption body is a flat surface. Interpolating the dark top
// resize/shadow edge across the whole title produced the visible streak.
// Use the median of the middle half of the sampled rows, not the edge.
fn generic_body_color(rows:&[u32])->u32{
 if rows.is_empty(){return 0;}
 let start=rows.len()/4;let end=(rows.len()-start).max(start+1);
 let mut rgb=0;
 for shift in [0,8,16]{let mut v:Vec<u32>=rows[start..end].iter().map(|p|(p>>shift)&255).collect();v.sort_unstable();rgb|=v[v.len()/2]<<shift;}
 rgb
}
fn generic_edge_rows(l:Layout,edge_y:i32)->usize{(edge_y-l.pos.y).clamp(0,l.h) as usize}
fn generic_blank_pixels(l:Layout,rows:&[u32],edge_y:i32)->Option<Vec<u32>>{
 if rows.len()!=l.h as usize{return None;}
 let mut pixels=vec![0xff000000|generic_body_color(rows);l.w as usize*l.h as usize];
 pixels[..generic_edge_rows(l,edge_y)*l.w as usize].fill(0);
 Some(pixels)
}
fn present_centered_band(sprite:H,b:CenteredBand,cover:Layout,rows:&[u32],title:&[u16])->bool{unsafe{
 let l=b.layout;let dy=l.pos.y-cover.pos.y;
 if dy<0||dy+l.h>cover.h||rows.len()!=cover.h as usize||title.is_empty(){return false;}
 let dc=CreateCompatibleDC(0);if dc==0{return false;}
 let info=BitmapInfo{size:40,width:l.w,height:-l.h,planes:1,bits:32,compression:0,image_size:0,xppm:0,yppm:0,used:0,important:0,colors:0};
 let mut bits=null_mut();let bmp=CreateDIBSection(dc,&info,0,&mut bits,0,0);let mut ok=false;
 if bmp!=0&&!bits.is_null(){let old=SelectObject(dc,bmp);if old!=0&&old != -1{
  let pixels=std::slice::from_raw_parts_mut(bits.cast::<u32>(),(l.w*l.h) as usize);
  let color=generic_body_color(rows);
  pixels.fill(color);
  let face=wide("Segoe UI");let font=CreateFontW(-scale(12,l.dpi),0,0,0,500,0,0,0,1,0,0,4,0,face.as_ptr());
  if font!=0{let old_font=SelectObject(dc,font);if old_font!=0&&old_font != -1{
   let lum=((color>>16&255)*299+(color>>8&255)*587+(color&255)*114)/1000;
   SetTextColor(dc,if lum>=145{0x181818}else{0xf0f0f0});SetBkMode(dc,1);let mut text_rect=b.text;
   if DrawTextW(dc,title.as_ptr(),title.len() as i32,&mut text_rect,0x8825)>0&&GdiFlush()!=0{
    for px in pixels.iter_mut(){*px|=0xff000000;}
    pixels[..generic_edge_rows(l,b.edge_y)*l.w as usize].fill(0);
    let size=Point{x:l.w,y:l.h};let origin=Point::default();let blend=Blend{operation:0,flags:0,alpha:255,format:1};
    ok=UpdateLayeredWindow(sprite,0,&l.pos,&size,dc,&origin,0,&blend,2)!=0;
   }SelectObject(dc,old_font);
  }DeleteObject(font);}SelectObject(dc,old);
 }}if bmp!=0{DeleteObject(bmp);}DeleteDC(dc);ok
}}
fn present_blank(sprite:H,l:Layout,rows:&[u32])->bool{
 if rows.len()!=l.h as usize{return false;}
 let mut pixels=vec![0;l.w as usize*l.h as usize];
 for (y,row) in pixels.chunks_exact_mut(l.w as usize).enumerate(){row.fill(0xff000000|rows[y]);}
 present_pixels(sprite,l,&pixels)
}
fn idle_target(h:H)->bool{unsafe{
 let (thread,_)=identity(h);if thread==0||GetAncestor(h,2)!=h||GetWindow(h,4)!=0{return false;}
 let mut info:GuiInfo=zeroed();info.size=size_of::<GuiInfo>() as u32;
 if GetGUIThreadInfo(thread,&mut info)==0||info.flags&0x1e!=0||info.capture!=0{return false;}
 let menu=GetSystemMenu(h,0);if menu!=0{let s=GetMenuState(menu,0xf060,0);if s!=0xffffffff&&s&3!=0{return false;}}
 true
}}
fn rgb(c:u32)->u32{((c&255)<<16)|(c&0xff00)|((c>>16)&255)}
// Read only a handful of colors immediately beside the verified caption group.
// No screen bitmap capture, file or pixel log; never used for Explorer or consoles.
fn sample_rows(h:H,l:Layout)->Option<Vec<u32>>{unsafe{
 let owned=||[4,10].into_iter().all(|dx|GetAncestor(WindowFromPoint(Point{x:l.pos.x-scale(dx,l.dpi),y:l.pos.y+l.h/2}),2)==h)&&GetForegroundWindow()==h;
 if !owned(){return None;}
 // The DIB's native pixels are 0x00RRGGBB, unlike GetPixel's COLORREF.
 let x=l.pos.x-scale(9,l.dpi);
 let strip=Layout{pos:Point{x,y:l.pos.y},w:scale(9,l.dpi)-scale(3,l.dpi)+1,..l};
 let batch=read_local_pixels(strip).ok();
 // Fall back to the previous sampling path only if a local copy fails.
 let dc=if batch.is_none(){GetDC(0)}else{0};
 if batch.is_none()&&dc==0{return None;}
 let result=(||{
  let mut anchors=[0u32;5];
  for (i,anchor) in anchors.iter_mut().enumerate(){
   let y=l.pos.y+(l.h-1)*i as i32/4;
   let mut colors=[0;3];
   for (j,c) in colors.iter_mut().enumerate(){
    let sx=l.pos.x-scale(3+j as i32*3,l.dpi);
    if let Some(p)=batch.as_ref(){*c=*p.get(((y-l.pos.y)*strip.w+sx-x) as usize)?&0xffffff;}
    else{let v=GetPixel(dc,sx,y);if v==0xffffffff{return None;}*c=rgb(v);}
   }
   let mut channels=[0u32;3];
   for (k,shift) in [16,8,0].into_iter().enumerate(){let mut vals=colors.map(|v|(v>>shift)&255);vals.sort_unstable();channels[k]=vals[1];}
   *anchor=(channels[0]<<16)|(channels[1]<<8)|channels[2];
  }
  Some((0..l.h).map(|y|{let p=y*4;let q=(l.h-1).max(1);let i=(p/q).min(3) as usize;let n=p-i as i32*q;let a=anchors[i];let b=anchors[i+1];let mix=|shift:u32|(((a>>shift&255)*(q-n) as u32+(b>>shift&255)*n as u32)/q as u32)<<shift;mix(16)|mix(8)|mix(0)}).collect())
 })();
 if dc!=0{ReleaseDC(0,dc);}
 if owned(){result}else{None}
}}

// Use only the admitted Settings window's exposed client pixels immediately
// below the verified title. Take a median of tiny patches, rejecting foreign
// windows, text/icons and mixed colors. No hardcoded dark-mode color.
fn settings_sample_rows(h:H,l:Layout,frame:Rect,work:Rect)->Option<Vec<u32>>{unsafe{
 let d=l.dpi;let left=frame.left.max(work.left);let right=frame.right.min(work.right);
 let mut colors=Vec::new();
 for off in [8,16]{for fraction in [1,2,8,9]{
  let x=left+(right-left)*fraction/10;let y=l.pos.y+l.h+scale(off,d);
  let patch=Layout{pos:Point{x,y},w:3,h:3,..l};
  if x<left||x+3>right||y<work.top||y+3>frame.bottom.min(work.bottom){continue;}
  let owned=||GetForegroundWindow()==h&&[Point{x,y},Point{x:x+2,y:y+2}].into_iter().all(|pt|GetAncestor(WindowFromPoint(pt),2)==h);
  if !owned(){continue;}
  if let Ok(px)=read_local_pixels(patch){if owned()&&flat_color_patch(&px){colors.push(px[4]&0xffffff);}}
 }}
 if GetForegroundWindow()!=h||colors.len()<3{return None;}
 let rgb=dominant_body_color(&colors)?;
 Some(vec![rgb;l.h as usize])
}}
fn dominant_body_color(colors:&[u32])->Option<u32>{
 let similar=|a:u32,b:u32|[0,8,16].into_iter().all(|shift|((a>>shift)&255).abs_diff((b>>shift)&255)<=8);
 let (&color,count)=colors.iter().map(|c|(c,colors.iter().filter(|&&other|similar(*c,other)).count())).max_by_key(|(_,n)|*n)?;
 if count<3||count*2<=colors.len(){None}else{Some(color)}
}
fn flat_color_patch(px:&[u32])->bool{
 px.len()==9&&px.iter().all(|p|[0,8,16].into_iter().all(|shift|((p>>shift)&255).abs_diff((px[4]>>shift)&255)<=8))
}
fn caption_sample_rows(h:H,l:Layout,frame:Rect,work:Rect,settings:bool)->Option<Vec<u32>>{
 if settings{settings_sample_rows(h,l,frame,work)}else{sample_rows(h,l)}
}
fn composite(dot:u32,bg:u32)->u32{
 let a=dot>>24;let c=|shift:u32|((dot>>shift&255)+((bg>>shift&255)*(255-a)+127)/255).min(255)<<shift;
 0xff000000|c(16)|c(8)|c(0)
}
fn present_cover(sprite:H,l:Layout,hover:bool,rows:&[u32])->bool{
 if rows.len()!=l.h as usize{return false;}
 let mut pixels=pixel_buffer(l,hover);
 for (y,row) in pixels.chunks_exact_mut(l.w as usize).enumerate(){for v in row{*v=composite(*v,rows[y]);}}
 present_pixels(sprite,l,&pixels)
}
fn choose_spacer(previous:Option<Layout>,fresh:bool,result:Option<browser_spacer::Verdict>)->Option<Layout>{
 match result{Some(browser_spacer::Verdict::Verified(m))=>Some(m),Some(browser_spacer::Verdict::Rejected)=>None,_ if fresh=>previous,_=>None}
}
fn update_title_cache(cache:&mut Option<Vec<u16>>,title:Option<&[u16]>){
 if cache.as_deref()==title{return;}
 match title{
  Some(t)=>{let v=cache.get_or_insert_with(Vec::new);v.clear();v.extend_from_slice(t);},
  None=>{*cache=None;}
 }
}
pub(super) struct Surface{
 request:Option<Request>,layout:Option<Layout>,generic_row:Option<Layout>,generic_fast_at:Instant,left:Option<Layout>,rows:Vec<u32>,hover:bool,shown:bool,cover_shown:bool,left_check:Instant,
 retry_at:Instant,last_sample:Instant,request_started:Instant,generic_retries:u8,blocked:Option<H>,misses:u32,office_title:Option<Vec<u16>>,ribbon_scan:Option<RibbonScan>,
 prefix_sprite:H,office_exact:bool,browser_left:bool,office_shift:Option<OfficeShift>,centered_generic:bool,settings_profile:bool,generic_band:Option<CenteredBand>,generic_center_at:Instant,generic_center_tries:u8,first_seen:Instant,first_reported:bool,
 spacer_enabled:bool,spacer:Option<Layout>,spacer_key:Option<browser_spacer::Key>,spacer_checked:Option<Instant>,spacer_grace:Option<Instant>,spacer_rebased:bool,spacer_wait_until:Option<Instant>,
}
impl Surface{
 pub(super) fn new(prefix_sprite:H,office_exact:bool,browser_left:bool,spacer_enabled:bool,centered_generic:bool,settings_profile:bool)->Self{Self{prefix_sprite,office_exact,browser_left,office_shift:None,centered_generic,settings_profile,generic_band:None,generic_center_at:Instant::now(),generic_center_tries:0,first_seen:Instant::now(),first_reported:false,spacer_enabled,spacer:None,spacer_key:None,spacer_checked:None,spacer_grace:None,spacer_rebased:false,spacer_wait_until:None,request:None,layout:None,generic_row:None,generic_fast_at:Instant::now(),left:None,rows:Vec::new(),hover:false,shown:false,cover_shown:false,left_check:Instant::now(),retry_at:Instant::now(),last_sample:Instant::now(),request_started:Instant::now(),generic_retries:0,blocked:None,misses:0,office_title:None,ribbon_scan:None}}
 pub(super) fn clear(&mut self,sprite:H,cover:H){
  if self.shown{unsafe{ShowWindow(sprite,0);SetWindowPos(sprite,0,0,0,0,0,0x0217);}self.shown=false;}
  if let (Some(req),Some(scan))=(self.request,self.ribbon_scan){log(&format!("ribbon_slot_scan_reset hwnd={:#x} next_index={}",req.hwnd,scan.index));}
  self.hide_prefix();self.hide_cover(cover);self.layout=None;self.generic_row=None;self.generic_fast_at=Instant::now();self.generic_center_at=Instant::now();self.generic_center_tries=0;self.left=None;self.request=None;self.rows=Vec::new();self.misses=0;self.request_started=Instant::now();self.generic_retries=0;self.office_title=None;self.ribbon_scan=None;self.spacer=None;self.spacer_key=None;self.spacer_checked=None;self.spacer_grace=None;self.spacer_rebased=false;self.spacer_wait_until=None;
 }
 fn hide_prefix(&mut self){let shifted=self.office_shift.take().is_some();let centered=self.generic_band.take().is_some();if shifted||centered{unsafe{ShowWindow(self.prefix_sprite,0);SetWindowPos(self.prefix_sprite,0,0,0,0,0,0x0217);}}}
 fn hide_cover(&mut self,cover:H){if self.cover_shown{unsafe{ShowWindow(cover,0);SetWindowPos(cover,0,0,0,0,0,0x0217);}self.cover_shown=false;}}
 pub(super) fn reset(&mut self,sprite:H,cover:H){self.clear(sprite,cover);self.blocked=None;self.retry_at=Instant::now();}
 pub(super) fn block_close(&mut self,h:H){self.blocked=Some(h);}
 pub(super) fn resume(&mut self){
  self.blocked=None;self.generic_center_tries=0;self.generic_center_at=Instant::now();
  if self.spacer.is_some(){
   self.spacer_grace=Some(Instant::now()+Duration::from_secs(4));
   // A very recent matching proof is still valid after a quick Alt+Tab.
   // Otherwise force a new key, as in the baseline.
   if !self.spacer_checked.is_some_and(|t|t.elapsed()<browser_spacer::PERIOD){if let Some(k)=self.spacer_key.as_mut(){k.epoch=Instant::now();}}
  }
  if !self.shown{self.retry_at=Instant::now();}
  self.last_sample=Instant::now();self.misses=0;
 }
 // Keep the fast timer for initial layout, incomplete scans, stale hits and
 // browser handoff. Steady-state verification retains its original deadlines.
 pub(super) fn needs_fast_tick(&self,allow_left:bool)->bool{
  if allow_left&&self.centered_generic&&self.layout.is_some()&&self.generic_band.is_none(){return self.generic_center_tries<8;}
  !self.shown||self.misses>0||self.ribbon_scan.is_some()||self.spacer_wait_until.is_some()||
  (allow_left&&self.centered_generic&&self.generic_band.is_none()&&self.generic_center_tries<8)
 }
 pub(super) fn saved_dpi(&self)->Option<u32>{self.request.map(|r|r.dpi)}
 pub(super) fn translate(&mut self,dx:i32,dy:i32,frame:Rect){
  let move_l=|l:&mut Layout|{l.pos.x+=dx;l.pos.y+=dy;};
  if let Some(l)=self.layout.as_mut(){move_l(l);}
  if let Some(l)=self.generic_row.as_mut(){move_l(l);}
  if let Some(l)=self.left.as_mut(){move_l(l);}
  if let Some(l)=self.spacer.as_mut(){move_l(l);}
  if let Some(s)=self.office_shift.as_mut(){move_l(&mut s.layout);move_l(&mut s.original_slot);}
  if let Some(b)=self.generic_band.as_mut(){move_l(&mut b.layout);move_l(&mut b.buttons);b.edge_y+=dy;}
  if let Some(r)=self.request.as_mut(){r.frame=frame;if let Some(h)=r.hint.as_mut(){h.left+=dx;h.right+=dx;h.top+=dy;h.bottom+=dy;}}
  if let Some(k)=self.spacer_key.as_mut(){if let Some(r)=self.request{k.req=r;}move_l(&mut k.right);k.epoch=Instant::now();}
 }
 fn reject_spacer(&mut self,cover:H){self.spacer=None;self.left=None;self.spacer_checked=None;self.spacer_grace=None;self.spacer_rebased=false;self.hide_cover(cover);self.misses=0;}
 fn spacer_fresh(&self)->bool{self.spacer_checked.is_some_and(|t|t.elapsed()<=browser_spacer::VALID)||self.spacer_grace.is_some_and(|t|Instant::now()<t)}
 // Rebase only a previously verified prefix, at unchanged DPI. Check real
 // right caption hit roles. Fresh UIA must confirm the new geometry.
 fn rebase_spacer(&mut self,req:Request,work:Rect)->bool{
  let (Some(old),Some(right),Some(mask),Some(key))=(self.request,self.layout,self.spacer,self.spacer_key)else{return false;};
  if old.hwnd!=req.hwnd||old.pid!=req.pid||old.dpi!=req.dpi||!self.spacer_fresh(){return false;}
  let mut r=right;r.pos.x+=req.frame.right-old.frame.right;r.pos.y+=req.frame.top-old.frame.top;r.work_top=Some(work.top);
  let rr=Rect{left:r.pos.x,top:r.pos.y,right:r.pos.x+r.w,bottom:r.pos.y+r.h};
  if candidate_layout(rr,req.frame,work,req.dpi).is_none(){return false;}
  for (i,role) in [8,9,20].into_iter().enumerate(){let pt=Point{x:r.pos.x+r.w*(2*i as i32+1)/6,y:r.pos.y+r.h/2};if unsafe{hit_code(req.hwnd,pt)}!=Some(role){return false;}}
  let mut m=mask;m.pos.x+=req.frame.left-old.frame.left;m.pos.y=r.pos.y;m.h=r.h;m.work_top=Some(work.top);
  if m.pos.x<work.left||m.pos.y<work.top||m.pos.x+m.w>=r.pos.x-scale(12,r.dpi)||m.pos.y+m.h>work.bottom||m.pos.x<req.frame.left{return false;}
  let Some(b)=browser_spacer::button_layout(m)else{return false;};
  self.layout=Some(r);self.left=Some(b);self.spacer=Some(m);self.request=Some(req);
  self.spacer_key=Some(browser_spacer::Key{req,right:r,work,count:key.count,epoch:Instant::now()});
  if !self.spacer_rebased{self.spacer_grace=Some(Instant::now()+Duration::from_secs(2));}
  self.spacer_rebased=true;self.last_sample=Instant::now();self.misses=0;
  log(&format!("browser_spacer_rebase hwnd={:#x} mask={m:?}",req.hwnd));true
 }

 pub(super) fn click(&self,d:&Dwm,lparam:L)->Option<(H,usize)>{
  if !self.shown{return None;}let req=self.request?;let l=self.left.or(self.layout)?;
  if !unsafe{active(req)}||!owner_presentable(d,req.hwnd)||unsafe{IsWindowEnabled(req.hwnd)}==0||visible_rect(d,req.hwnd)!=Some(req.frame){return None;}
  let mut x=(lparam as u16 as i16) as i32;let mut y=((lparam>>16) as u16 as i16) as i32;
  if let Some(mask)=self.spacer{
   let mut title_buf=[0u16;1024];
   if !self.spacer_fresh()||read_title_buf(req.hwnd,&mut title_buf).is_none_or(browser_spacer::marker_utf16){return None;}
   x+=mask.pos.x-l.pos.x;y+=mask.pos.y-l.pos.y;
   return button_role(l,x,y).map(|role|(req.hwnd,role));
  }
  let role=button_role(l,x,y)?;
  if self.left.is_some(){
   let mut title_buf=[0u16;1024];
   if read_title_buf(req.hwnd,&mut title_buf)!=self.office_title.as_deref(){return None;}
   let hit=unsafe{hit_code(req.hwnd,Point{x:l.pos.x+x,y:l.pos.y+y})};
   if let Some(shift)=self.office_shift.as_ref(){if !matches!(hit,Some(2)|Some(3))||!caption_hits(req.hwnd,shift.original_slot){return None;}}
   else if self.generic_band.is_some(){if !matches!(hit,Some(2)|Some(3)){return None;}}
   else if hit!=Some(2){return None;}
  }
  Some((req.hwnd,role))
 }
 pub(super) fn tick(&mut self,d:&Dwm,h:H,app:App,sprite:H,cover:H,allow_left:bool){
  if unsafe{GetForegroundWindow()}!=h{return;}
  if !owner_presentable(d,h){self.clear(sprite,cover);return;}
  let generic=app==App::GenericProbe;
  if self.blocked==Some(h)||(!generic&&style(h)&(WS_CAPTION|BUTTON_BITS)!=(WS_CAPTION|BUTTON_BITS)){self.clear(sprite,cover);return;}
  if generic&&unsafe{GetAncestor(h,2)!=h||GetWindow(h,4)!=0}{self.clear(sprite,cover);return;}
  if !idle_target(h){if self.request.is_some_and(|r|visible_rect(d,h)==Some(r.frame)){return;}self.clear(sprite,cover);return;}
  let Some(frame)=visible_rect(d,h)else{self.clear(sprite,cover);return;};
  let Some(info)=monitor_info(h)else{self.clear(sprite,cover);return;};
  let dpi=unsafe{GetDpiForWindow(h)}.max(96);if dpi>480||frame.width()<scale(340,dpi){self.clear(sprite,cover);return;}
  let browser=app==App::BrowserProbe;
  let settings=generic&&self.settings_profile;
  let hint=if browser||generic{window_rect(h).zip(d.rect(h,5)).map(|(w,b)|Rect{left:w.left+b.left,top:w.top+b.top,right:w.left+b.right,bottom:w.top+b.bottom})}else{None};
  let req=Request{hwnd:h,pid:identity(h).1,frame,dpi,browser,native_probe:generic,hint};
  let mut title_buf=[0u16;1024];
  let title=if allow_left&&(generic||app==App::OfficeProbe||(browser&&(self.browser_left||self.spacer_enabled))){read_title_buf(h,&mut title_buf)}else{None};
  let stable_browser=browser&&self.spacer_enabled&&allow_left;
  let selected_marker=stable_browser&&title.is_some_and(browser_spacer::marker_utf16);
  let mut rebased=false;
  if self.request!=Some(req){
   if stable_browser&&!selected_marker&&self.rebase_spacer(req,info.work){rebased=true;}
   else{let was_left=stable_browser&&self.spacer.is_some()&&!selected_marker;self.clear(sprite,cover);self.request=Some(req);self.retry_at=Instant::now();if was_left{self.spacer_wait_until=Some(Instant::now()+Duration::from_secs(4));}}
  }else if !stable_browser&&(self.office_title.is_some()||(generic&&allow_left&&self.layout.is_some()))&&self.office_title.as_deref()!=title{self.clear(sprite,cover);self.request=Some(req);self.retry_at=Instant::now();}
  if stable_browser{
   if selected_marker{if self.spacer.is_some(){self.reject_spacer(cover);unsafe{ShowWindow(sprite,0);}self.shown=false;log("browser_spacer_selected_marker");}self.spacer_key=None;}
   else if self.spacer_key.is_none(){if let (Some(l),Some(_))=(self.layout,title.as_ref()){if let Ok(exe)=query_name(req.pid){let count=match exe.as_str(){"brave.exe"=>3,"msedge.exe"=>2,_=>0};if count>0{self.spacer_key=Some(browser_spacer::Key{req,right:l,work:info.work,count,epoch:Instant::now()});}}}}
   update_title_cache(&mut self.office_title,title);
  }
  if Instant::now()<self.retry_at{return;}
  if self.layout.is_none(){
   // A bounded direct DWM/role check bypasses the worker queue on fresh
   // allowlisted generic frames. If it cannot prove the geometry, use the
   // existing asynchronous verifier; never paint guessed bounds.
   let fast=if generic&&Instant::now()>=self.generic_fast_at{self.generic_fast_at=Instant::now()+Duration::from_millis(100);unsafe{quick_generic_hint(req)}}else{None};
   let Some(verified)=fast.or_else(||poll(req,true))else{return;};
   let r=verified;
   let r=if generic{let Some(full)=generic_visual_envelope(r,req.hint,frame,info.work,dpi)else{log(&format!("generic_cover_conflict hwnd={h:#x} verified={r:?} hint={:?}",req.hint));self.retry_at=Instant::now()+Duration::from_millis(250);return;};if full!=r{log(&format!("generic_cover_expanded hwnd={h:#x} verified={r:?} full={full:?}"));}full}else{r};
   let Some(l)=candidate_layout(r,frame,info.work,dpi)else{log(&format!("custom_skip hwnd={h:#x} app={app:?} reason=caption_outside_workarea rect={r:?}"));self.retry_at=Instant::now()+Duration::from_millis(if generic{100}else{500});return;};
   let Some(rows)=caption_sample_rows(h,l,frame,info.work,settings)else{log(&format!("custom_skip hwnd={h:#x} app={app:?} reason=background_unavailable"));self.retry_at=Instant::now()+Duration::from_millis(if generic{80}else{500});return;};
   if !unsafe{active(req)}||visible_rect(d,h)!=Some(frame){return;}
   if settings{log(&format!("settings_body_color hwnd={h:#x} rgb={:06x}",rows[0]));}
   self.layout=Some(l);self.generic_row=if generic{Some(generic_left_reference(l,verified))}else{None};self.rows=rows;self.last_sample=Instant::now();
   // Chromium has no external tab-strip inset setter. Only accept an actual
   // empty drag region, never copy or visually displace browser tabs.
   // Test once per attachment/geometry; no background retries on occupied tabs.
   if browser&&allow_left&&self.browser_left&&!self.spacer_enabled&&title.is_some(){
    if let Some(slot)=browser_left_slot(h,l,frame,info.work){
     self.left=Some(slot);update_title_cache(&mut self.office_title,title);self.left_check=Instant::now();
     log(&format!("browser_left_verified hwnd={h:#x} rect={slot:?} tab_layout_unchanged=true"));
    }else{log(&format!("browser_left_fallback hwnd={h:#x} reason=no_verified_empty_caption keep_right=true"));}
   }
   if browser&&allow_left&&self.spacer_enabled{
    if let Some(t)=title{update_title_cache(&mut self.office_title,Some(t));if !browser_spacer::marker_utf16(t){
     if let Ok(exe)=query_name(req.pid){let count=match exe.as_str(){"brave.exe"=>3,"msedge.exe"=>2,_=>0};if count>0{self.spacer_key=Some(browser_spacer::Key{req,right:l,work:info.work,count,epoch:Instant::now()});}}
    }}
   }
   if allow_left&&generic&&self.centered_generic&&self.prefix_sprite!=0&&title.is_some(){
    self.generic_center_at=Instant::now()+Duration::from_millis(250);
    if let Some(band)=centered_band(h,frame,info.work,l,settings){
     self.left=Some(band.buttons);self.generic_band=Some(band);self.left_check=Instant::now();
     update_title_cache(&mut self.office_title,title);
     log(&format!("generic_centered_verified hwnd={h:#x} margin_dip={LEFT_MARGIN_DIP} band={band:?}"));
    }else{update_title_cache(&mut self.office_title,title);log(&format!("generic_centered_wait hwnd={h:#x} reason=nonclient_or_hit_validation"));}
   }else if allow_left&&(generic||(app==App::OfficeProbe&&title.is_some())){
    self.ribbon_scan=Some(RibbonScan::default());
    log(&format!("ribbon_slot_scan_start hwnd={h:#x} candidates={} order=horizontal_first",OFFICE_OFFSETS.len()*3));
    update_title_cache(&mut self.office_title,title);
   }else if allow_left&&app==App::OfficeProbe{log(&format!("ribbon_slot_not_started hwnd={h:#x} reason=no_window_title"));}
  }
  let Some(l)=self.layout else{return;};
  // Recheck the left drag slot while active: tab/QAT changes need not move the HWND.
  // If it stops being a drag region, immediately return to the verified right layout.
  let mut switched=rebased;
  // A startup frame can be incomplete even though the previous role group
  // was valid. Upgrade a fallback once the real nonclient title becomes ready.
  if allow_left&&generic&&self.centered_generic&&self.prefix_sprite!=0&&title.is_some()&&self.generic_band.is_none()&&Instant::now()>=self.generic_center_at{
   self.generic_center_tries=self.generic_center_tries.saturating_add(1);
   self.generic_center_at=Instant::now()+Duration::from_millis(if self.generic_center_tries<8{150}else{1000});
   if let Some(band)=centered_band(h,frame,info.work,l,settings){
    self.left=Some(band.buttons);self.generic_band=Some(band);self.ribbon_scan=None;self.left_check=Instant::now();update_title_cache(&mut self.office_title,title);switched=true;
    log(&format!("generic_centered_verified hwnd={h:#x} margin_dip={LEFT_MARGIN_DIP} retry=true band={band:?}"));
   }
  }
  if let Some(key)=self.spacer_key{
   use browser_spacer::Verdict;
   let result=browser_spacer::poll(key);
   if let Some((Verdict::Verified(_),t))=result{self.spacer_checked=Some(t);self.spacer_grace=None;self.spacer_rebased=false;}
   let mask=choose_spacer(self.spacer,self.spacer_fresh(),result.map(|(v,_)|v));
   if self.spacer_wait_until.is_some(){
    if mask.is_some()||matches!(result,Some((Verdict::Rejected,_)))||self.spacer_wait_until.is_some_and(|t|Instant::now()>=t){self.spacer_wait_until=None;}
    else{return;}
   }
   if mask!=self.spacer{
    if let Some(m)=mask{self.spacer=Some(m);self.left=browser_spacer::button_layout(m);}else{self.reject_spacer(cover);}
    switched=true;self.misses=0;
    log(&format!("browser_spacer_layout hwnd={h:#x} mask={mask:?} fallback_right={}",mask.is_none()));
   }
  }
  if let Some(scan)=self.ribbon_scan.as_mut(){
   let (slot,done)=if generic{scan.step_generic(h,self.generic_row.unwrap_or(l),l,frame,info.work)}else{
    let start=Instant::now();let mut result=(None,false);
    for _ in 0..4{result=scan.step(h,l,frame,info.work,false);if result.1||start.elapsed()>=Duration::from_millis(25){break;}}
    result
   };
   if done{self.ribbon_scan=None;log(&format!("custom_left_probe hwnd={h:#x} app={app:?} verified={} fallback_right={}",slot.is_some(),slot.is_none()));}
   if let Some(slot)=slot{
    self.left=Some(slot);
    if self.office_exact&&app==App::OfficeProbe{
     match make_office_shift(h,slot,l,frame,info.work){
      Ok((exact,shift))=>{self.left=Some(exact);self.office_shift=Some(shift);log(&format!("office_exact_left_verified hwnd={h:#x} rect={exact:?} visual_only=true"));}
      Err(reason)=>log(&format!("office_exact_left_fallback hwnd={h:#x} reason={reason} near_left=true")),
     }
    }
    self.left_check=Instant::now();switched=true;
   }
  }
  if let Some(left)=self.left{if self.spacer.is_none()&&self.left_check.elapsed()>=Duration::from_millis(if browser{200}else{300}){
   self.left_check=Instant::now();
   let verify_slot=self.office_shift.as_ref().map_or(left,|s|s.original_slot);
   if !(if self.generic_band.is_some(){centered_button_hits(h,left)}else{caption_hits(h,verify_slot)})||(self.office_shift.is_some()&&!exact_button_hits(h,left)){if unsafe{GetForegroundWindow()}!=h{return;}self.left=None;if generic&&self.centered_generic{self.generic_center_tries=0;self.generic_center_at=Instant::now();}else if generic&&self.generic_retries<1{self.generic_retries+=1;self.ribbon_scan=Some(RibbonScan::default());}self.hide_prefix();self.hide_cover(cover);unsafe{ShowWindow(sprite,0);}self.shown=false;self.misses=0;switched=true;log(&format!("custom_left_revert hwnd={h:#x} reason=slot_changed"));}
  }}
  // A known full-title profile must not jump to the right or to a slot
  // after its old title. Keep the stock caption until the left/center proof
  // is available; retry at a bounded cadence instead of masking controls.
  if allow_left&&generic&&self.centered_generic&&self.generic_band.is_none(){return;}
  if !self.shown&&self.left.is_none(){
   let age=self.last_sample.elapsed();
   if self.ribbon_scan.is_some()&&(generic||age<Duration::from_millis(180)){return;}
   if self.spacer_key.is_some()&&self.spacer.is_none()&&age<Duration::from_millis(250){return;}
  }
  if !unsafe{active(req)}{return;}if visible_rect(d,h)!=Some(frame){return;}
  let buttons=self.left.unwrap_or(l);
  let hover=cursor_over(buttons);
  let surface=self.spacer.unwrap_or(buttons);
  let mut changed=switched;
  if self.ribbon_scan.is_none()&&self.last_sample.elapsed()>=Duration::from_secs(if stable_browser{3}else{2}){
   self.last_sample=Instant::now();
   if let Some(rows)=caption_sample_rows(h,l,frame,info.work,settings){if rows!=self.rows{
    if settings{log(&format!("settings_body_color hwnd={h:#x} rgb={:06x}",rows[0]));}
    if self.office_shift.is_some(){self.clear(sprite,cover);self.request=Some(req);self.retry_at=Instant::now()+Duration::from_millis(160);log("office_exact_left_theme_refresh");return;}
    self.rows=rows;changed=true;
   }}else if !stable_browser&&!settings{self.clear(sprite,cover);self.retry_at=Instant::now()+Duration::from_millis(if generic{100}else{500});return;}
  }
  let auxiliary_hidden=unsafe{(self.left.is_some()&&(!self.cover_shown||IsWindowVisible(cover)==0))||((self.generic_band.is_some()||self.office_shift.is_some())&&IsWindowVisible(self.prefix_sprite)==0)};
  if auxiliary_hidden&&self.shown{log(&format!("caption_auxiliary_recover hwnd={h:#x}"));}
  if !self.shown||unsafe{IsWindowVisible(sprite)}==0||auxiliary_hidden||hover!=self.hover||changed||self.misses==3||self.misses==8{
   let rendered=if self.left.is_some(){
    let mask_ok=if !self.cover_shown||unsafe{IsWindowVisible(cover)}==0||changed||self.misses==3||self.misses==8{
     let painted=if let Some(band)=self.generic_band{generic_blank_pixels(l,&self.rows,band.edge_y).is_some_and(|p|present_pixels(cover,l,&p))}else{present_blank(cover,l,&self.rows)};
     if painted&&unsafe{active(req)}&&visible_rect(d,h)==Some(frame)&&unsafe{SetWindowPos(cover,0,l.pos.x,l.pos.y,0,0,0x0251)}!=0{self.cover_shown=true;true}else{false}
    }else{true};
    let prefix_ok=if let Some(band)=self.generic_band{
     if !self.shown||switched||changed||self.misses==3||self.misses==8||unsafe{IsWindowVisible(self.prefix_sprite)}==0{
      self.office_title.as_deref().is_some_and(|t|present_centered_band(self.prefix_sprite,band,l,&self.rows,t))&&unsafe{active(req)}&&visible_rect(d,h)==Some(frame)&&unsafe{SetWindowPos(self.prefix_sprite,0,band.layout.pos.x,band.layout.pos.y,0,0,0x0251)}!=0
     }else{true}
    }else if let Some(shift)=self.office_shift.as_ref(){
     if !self.shown||switched||changed||self.misses==3||self.misses==8||unsafe{IsWindowVisible(self.prefix_sprite)}==0{
      present_pixels(self.prefix_sprite,shift.layout,&shift.pixels)&&unsafe{active(req)}&&unsafe{SetWindowPos(self.prefix_sprite,0,shift.layout.pos.x,shift.layout.pos.y,0,0,0x0251)}!=0
     }else{true}
    }else{true};
    mask_ok&&prefix_ok&&if let Some(mask)=self.spacer{browser_spacer::render(sprite,mask,buttons,hover,&self.rows)}else{present(sprite,buttons,hover)}
   }else{present_cover(sprite,l,hover,&self.rows)};
   if !unsafe{active(req)}{return;}
   if !rendered||visible_rect(d,h)!=Some(frame)||unsafe{SetWindowPos(sprite,0,surface.pos.x,surface.pos.y,0,0,0x0251)}==0{
    unsafe{ShowWindow(sprite,0);}self.clear(sprite,cover);self.request=Some(req);self.retry_at=Instant::now()+Duration::from_millis(if generic{100}else{500});log("custom_present_failed; stock unchanged");return;
   }
   if generic&&!self.first_reported{self.first_reported=true;log(&format!("generic_session_first_present hwnd={h:#x} elapsed_ms={} centered={}",self.first_seen.elapsed().as_millis(),self.generic_band.is_some()));}
   if !self.shown{log(&format!("custom_first_present hwnd={h:#x} app={app:?} elapsed_ms={} left={}",self.request_started.elapsed().as_millis(),self.left.is_some()));}
   if !self.shown||switched{log(&format!("custom_present hwnd={h:#x} app={app:?} rect={buttons:?} left={} mode=verified_cover stock_style_unchanged=true",self.left.is_some()));}
   self.shown=true;self.hover=hover;
  }
  if unsafe{GetForegroundWindow()}!=h{return;}
  let mid=Point{x:buttons.pos.x+buttons.w/2,y:buttons.pos.y+buttons.h/2};
  let cover_ok=!self.cover_shown||unsafe{WindowFromPoint(Point{x:l.pos.x+l.w/2,y:l.pos.y+l.h/2})}==cover;
  if cover_ok&&unsafe{WindowFromPoint(mid)}==sprite{self.misses=0;}else{self.misses+=1;if self.misses>=15{log("custom_covered; stock unchanged");self.clear(sprite,cover);self.request=Some(req);self.retry_at=Instant::now()+Duration::from_millis(if generic{100}else{500});}}
 }
}
#[cfg(test)] mod surface_tests{use super::*;
 #[test]fn unchanged_title_reuses_cache(){let mut c=None;let a=[65u16,66,67];update_title_cache(&mut c,Some(&a));let ptr=c.as_ref().unwrap().as_ptr();update_title_cache(&mut c,Some(&a));assert_eq!(c.as_ref().unwrap().as_ptr(),ptr);update_title_cache(&mut c,Some(&[68,69]));assert_eq!(c.as_deref(),Some(&[68u16,69][..]));assert_eq!(c.as_ref().unwrap().as_ptr(),ptr);update_title_cache(&mut c,None);assert!(c.is_none());}
 #[test]fn spacer_pending_is_not_rejection(){let l=Layout{pos:Point{x:0,y:24},w:108,h:40,dpi:96,work_top:Some(24),fallback_row:false,left_slot:true};use browser_spacer::Verdict;assert_eq!(choose_spacer(Some(l),true,None),Some(l));assert_eq!(choose_spacer(Some(l),true,Some(Verdict::Unavailable)),Some(l));assert_eq!(choose_spacer(Some(l),true,Some(Verdict::Rejected)),None);assert_eq!(choose_spacer(Some(l),false,None),None);assert_eq!(choose_spacer(None,false,Some(Verdict::Verified(l))),Some(l));}

 #[test] fn browser_slot_near_edge_only(){let f=Rect{left:0,top:24,right:1920,bottom:1080};let r=Layout{pos:Point{x:1783,y:24},w:137,h:40,dpi:96,work_top:Some(24),fallback_row:false,left_slot:false};let l=browser_slot_geometry(r,f,f,3).unwrap();assert_eq!(l.pos,Point{x:3,y:33});assert!(browser_slot_geometry(r,f,f,80).is_none());assert!(browser_slot_geometry(Layout{h:18,..r},f,f,3).is_none());}
 #[test] fn browser_slot_respects_workarea(){let f=Rect{left:-8,top:16,right:1928,bottom:1088};let work=Rect{left:0,top:24,right:1920,bottom:1080};let r=Layout{pos:Point{x:1783,y:24},w:137,h:40,dpi:96,work_top:Some(24),fallback_row:false,left_slot:false};assert_eq!(browser_slot_geometry(r,f,work,8).unwrap().pos,Point{x:8,y:33});assert!(browser_slot_geometry(r,f,Rect{top:40,..work},3).is_none());}

 #[test] fn shift_prefix_keeps_title_pixels(){let row=[0x00112233,0x00ffffff,0x00112233,0x00112233,0x00112233,0x00112233];let p=row.repeat(3);let out=shifted_pixels(&p,6,3,3).unwrap();assert_eq!(&out[..6],&[0xff112233,0xff112233,0xff112233,0xff112233,0xffffffff,0xff112233]);}
 #[test] fn padded_prefix_keeps_gap_and_title(){let p=[0x00112233,0x00ffffff,0x00112233,0x00112233,0x00112233,0x00112233,0x00112233,0x00112233].repeat(3);let out=shifted_pixels(&p,8,3,5).unwrap();assert_eq!(&out[..8],&[0xff112233,0xff112233,0xff112233,0xff112233,0xff112233,0xff112233,0xffffffff,0xff112233]);}
 #[test] fn shift_prefix_reject_occupied_destination(){let mut p=vec![0x00112233;24];p[6]=0x00ffffff;assert!(shifted_pixels(&p,8,3,4).is_none());assert!(shifted_pixels(&p,8,3,8).is_none());}

 #[test] fn office_horizontal_first(){assert_eq!(office_scan_point(2),Some((80,0)));assert_eq!(office_scan_point(5),Some((176,0)));assert_eq!(office_scan_point(10),Some((336,0)));assert_eq!(office_scan_point(35),Some((3,2)));assert_eq!(office_scan_point(70),Some((3,-2)));assert_eq!(office_scan_point(105),None);}
 #[test] fn office_scan_unique(){let mut points=std::collections::HashSet::new();for i in 0..OFFICE_OFFSETS.len()*3{assert!(points.insert(office_scan_point(i).unwrap()));}}
 #[test] fn local_pixels_preserve_row_gradient(){let mut p=vec![0x00185abc;66*22];for y in 0..22{for x in 0..66{p[y*66+x]+=y as u32;}}assert!(flat_ribbon_pixels(&p,66,22));p[10*66+31]=0x00ffffff;assert!(!flat_ribbon_pixels(&p,66,22));assert!(!flat_ribbon_pixels(&p[..20],66,22));}
 #[test] fn local_pixels_ignore_unused_alpha(){let mut p=vec![0x00185abc;66*22];p[100]|=0xff000000;assert!(flat_ribbon_pixels(&p,66,22));}

 #[test] fn maximized_ribbon_left_clipped(){let f=Rect{left:-8,top:16,right:1928,bottom:1088};let work=Rect{left:0,top:24,right:1920,bottom:1080};let r=Layout{pos:Point{x:1776,y:24},w:144,h:48,dpi:96,work_top:Some(24),fallback_row:false,left_slot:false};assert!(left_candidate(r,f,work).is_none());assert_eq!(ribbon_candidate(r,f,work,80).unwrap().pos,Point{x:80,y:37});}
 #[test] fn no_ribbon_overlap(){let f=Rect{left:0,top:24,right:1920,bottom:1080};let r=Layout{pos:Point{x:130,y:24},w:144,h:48,dpi:96,work_top:Some(24),fallback_row:false,left_slot:false};assert!(ribbon_candidate(r,f,f,80).is_none());}
 #[test] fn shifted_slot_workarea(){let f=Rect{left:0,top:24,right:1920,bottom:1080};let r=Layout{pos:Point{x:1846,y:28},w:66,h:22,dpi:96,work_top:None,fallback_row:false,left_slot:false};let l=ribbon_candidate(r,f,f,112).unwrap();assert_eq!(shift_ribbon(l,f,f,4).unwrap().pos,Point{x:112,y:32});assert!(shift_ribbon(l,f,Rect{top:40,..f},4).is_none());}
 #[test] fn ribbon_slot_bounds(){let f=Rect{left:0,top:24,right:1920,bottom:1080};let r=Layout{pos:Point{x:1776,y:24},w:144,h:48,dpi:96,work_top:Some(24),fallback_row:false,left_slot:false};assert_eq!(ribbon_candidate(r,f,f,176).unwrap().pos,Point{x:176,y:37});assert!(ribbon_candidate(r,Rect{right:500,..f},f,176).is_none());}
 #[test] fn left_slot_workarea(){let frame=Rect{left:0,top:24,right:1920,bottom:1080};let right=Layout{pos:Point{x:1780,y:24},w:140,h:40,dpi:96,work_top:Some(24),fallback_row:false,left_slot:false};let l=left_candidate(right,frame,frame).unwrap();assert_eq!(l.pos,Point{x:3,y:33});assert!(l.left_slot);assert!(left_candidate(right,frame,Rect{top:44,..frame}).is_none());}
 #[test] fn alpha_composite(){assert_eq!(composite(0,0x123456),0xff123456);assert_eq!(composite(0xffff5f57,0),0xffff5f57);}
 #[test] fn centered_roles(){let l=Layout{pos:Point::default(),w:146,h:30,dpi:96,work_top:None,fallback_row:false,left_slot:false};assert_eq!(button_role(l,51,15),Some(0));assert_eq!(button_role(l,73,15),Some(1));assert_eq!(button_role(l,95,15),Some(2));assert_eq!(button_role(l,5,15),None);}
 #[test] fn reject_finder_overlap(){let f=Rect{left:0,top:0,right:1920,bottom:1080};let work=Rect{top:24,..f};assert!(candidate_layout(Rect{left:1774,top:0,right:1920,bottom:30},f,work,96).is_none());assert!(candidate_layout(Rect{left:1774,top:24,right:1920,bottom:54},f,work,96).is_some());}
}

#[cfg(test)] mod first_paint_tests{use super::*;
 #[test] fn compact_generic_hint_matches_observed_role_strip(){let b=Rect{left:1421,top:192,right:1567,bottom:222};let [compact,full]=generic_hint_candidates(b,96);assert_eq!(compact,Rect{left:1464,top:200,right:1567,bottom:222});assert_eq!(full,b);}
 #[test] fn generic_left_row_preserves_verified_y_and_cover_boundary(){let cover=Layout{pos:Point{x:1421,y:192},w:146,h:30,dpi:96,work_top:Some(0),fallback_row:false,left_slot:false};let verified=Rect{left:1463,top:200,right:1566,bottom:222};let row=generic_left_reference(cover,verified);assert_eq!(row.pos,Point{x:1421,y:200});assert_eq!(row.h,22);assert_eq!(row.w,146);let f=Rect{left:100,top:192,right:1567,bottom:900};let l=ribbon_candidate(row,f,f,80).unwrap();assert_eq!(l.pos,Point{x:180,y:200});}
 #[test] fn generic_left_row_scales_and_stays_inside_cover(){for dpi in [96,120,144,192]{let cover=Layout{pos:Point{x:1000,y:100},w:scale(146,dpi),h:scale(30,dpi),dpi,work_top:Some(0),fallback_row:false,left_slot:false};let b=Rect{left:1000,top:100,right:1000+cover.w,bottom:100+cover.h};let v=generic_hint_candidates(b,dpi)[0];let l=generic_left_reference(cover,v);assert_eq!(l.pos.y+l.h,b.bottom);assert_eq!(l.h,scale(22,dpi));}}
 #[test] fn full_visual_cover_includes_minimize(){let f=Rect{left:100,top:100,right:900,bottom:700};let h=Rect{left:754,top:100,right:900,bottom:130};let v=Rect{left:797,top:104,right:900,bottom:126};assert_eq!(generic_visual_envelope(v,Some(h),f,f,96),Some(h));}
 #[test] fn contradictory_hint_rejected(){let f=Rect{left:100,top:100,right:900,bottom:700};let h=Rect{left:754,top:100,right:900,bottom:130};let v=Rect{left:797,top:120,right:900,bottom:142};assert_eq!(generic_visual_envelope(v,Some(h),f,f,96),None);}
 #[test] fn missing_hint_keeps_verified_group(){let f=Rect{left:0,top:0,right:900,bottom:700};let v=Rect{left:780,top:0,right:900,bottom:30};assert_eq!(generic_visual_envelope(v,None,f,f,96),Some(v));}
}

#[cfg(test)] mod centered_generic_tests{use super::*;
 #[test] fn exact_margin_and_center(){for dpi in [96,120,144,192]{let f=Rect{left:-900,top:100,right:900,bottom:900};let c=Layout{pos:Point{x:900-scale(146,dpi),y:100},w:scale(146,dpi),h:scale(30,dpi),dpi,work_top:Some(100),fallback_row:false,left_slot:false};let b=centered_band_geometry(f,f,c,100+c.h).unwrap();assert_eq!(b.buttons.pos.x,f.left+scale(8,dpi));assert_eq!(b.layout.pos.x+(b.text.left+b.text.right)/2,0);assert_eq!(b.layout.pos.y+b.layout.h,100+c.h);assert!(b.layout.pos.x+b.layout.w<=c.pos.x-scale(12,dpi));}}
 #[test] fn rejects_client_overlap(){let f=Rect{left:0,top:0,right:900,bottom:700};let c=Layout{pos:Point{x:754,y:0},w:146,h:30,dpi:96,work_top:Some(0),fallback_row:false,left_slot:false};assert!(centered_band_geometry(f,f,c,29).is_none());}
 #[test] fn rejects_narrow_title(){let f=Rect{left:0,top:0,right:340,bottom:700};let c=Layout{pos:Point{x:194,y:0},w:146,h:30,dpi:96,work_top:Some(0),fallback_row:false,left_slot:false};assert!(centered_band_geometry(f,f,c,30).is_none());}
}

#[cfg(test)] mod stage1_final_tests{use super::*;
 #[test] fn settings_extended_client_is_bounded(){let f=Rect{left:0,top:100,right:900,bottom:700};let c=Layout{pos:Point{x:754,y:100},w:146,h:30,dpi:96,work_top:Some(100),fallback_row:false,left_slot:false};assert_eq!(centered_client_limit(f,c,100,true),Some(130));assert_eq!(centered_client_limit(f,c,100,false),None);assert_eq!(centered_client_limit(f,Layout{h:41,..c},100,true),None);}
 #[test] fn edge_does_not_tint_title(){let mut rows=vec![0xffffff;30];rows[..7].fill(0x181818);rows[29]=0x555555;assert_eq!(generic_body_color(&rows),0xffffff);let l=Layout{pos:Point{x:0,y:100},w:10,h:30,dpi:96,work_top:None,fallback_row:false,left_slot:false};let p=generic_blank_pixels(l,&rows,105).unwrap();assert!(p[..50].iter().all(|&v|v==0));assert!(p[50..].iter().all(|&v|v==0xffffffff));}
 #[test] fn edge_matches_both_surfaces_at_dpi(){for dpi in [96,120,144,192]{let f=Rect{left:0,top:100,right:1200,bottom:800};let c=Layout{pos:Point{x:1200-scale(146,dpi),y:100},w:scale(146,dpi),h:scale(30,dpi),dpi,work_top:Some(100),fallback_row:false,left_slot:false};let b=centered_band_geometry(f,f,c,100+c.h).unwrap();assert_eq!(c.pos.y+generic_edge_rows(c,b.edge_y) as i32,b.edge_y);assert_eq!(b.layout.pos.y+generic_edge_rows(b.layout,b.edge_y) as i32,b.edge_y);assert!(b.edge_y<=b.buttons.pos.y+scale(2,dpi));}}
}

#[cfg(test)] mod stage2_caption_tests{use super::*;
 #[test] fn settings_band_is_contiguous_at_all_dpi(){for d in [96,120,144,192]{let f=Rect{left:-1920,top:0,right:0,bottom:1080};let c=Layout{pos:Point{x:-scale(146,d),y:0},w:scale(146,d),h:scale(30,d),dpi:d,work_top:Some(0),fallback_row:false,left_slot:false};let b=centered_band_geometry_for(f,f,c,c.h,true).unwrap();assert_eq!(b.layout.pos.x,f.left);assert_eq!(b.layout.pos.x+b.layout.w,c.pos.x);assert_eq!(b.layout.pos.y,c.pos.y);assert_eq!(b.edge_y,b.layout.pos.y);assert_eq!(b.buttons.pos.x,f.left+scale(8,d));assert_eq!(b.layout.pos.x+(b.text.left+b.text.right)/2,-960);}}
 #[test] fn settings_body_rejects_mixed_and_uses_majority(){assert_eq!(dominant_body_color(&[0x181818,0x181818,0x181818,0x00aabb]),Some(0x181818));assert_eq!(dominant_body_color(&[0x181818,0x181818,0xffffff,0xffffff]),None);}
 #[test] fn maximized_envelope_clips_hidden_borders(){let f=Rect{left:-8,top:-8,right:1928,bottom:1088};let w=Rect{left:0,top:0,right:1920,bottom:1080};let v=Rect{left:1817,top:4,right:1920,bottom:26};let raw=Rect{left:1774,top:-4,right:1924,bottom:26};assert_eq!(generic_visual_envelope(v,Some(raw),f,w,96),Some(Rect{left:1774,top:0,right:1920,bottom:26}));assert!(generic_visual_envelope(v,Some(raw),f,Rect{top:24,..w},96).is_none());}
 #[test] fn compact_row_still_fits_centered_buttons(){let f=Rect{left:0,top:0,right:900,bottom:700};let l=Layout{pos:Point{x:797,y:8},w:103,h:22,dpi:96,work_top:Some(0),fallback_row:false,left_slot:false};let b=centered_band_geometry(f,f,l,30).unwrap();assert_eq!(b.buttons.pos,Point{x:8,y:8});}
 #[test] fn settings_sample_rejects_icons_and_accepts_flat(){assert!(flat_color_patch(&[0x181818;9]));let mut p=[0x181818;9];p[2]=0xffffff;assert!(!flat_color_patch(&p));assert!(!flat_color_patch(&p[..8]));}
}
