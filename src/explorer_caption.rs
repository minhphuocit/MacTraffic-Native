//! Explorer caption fallback. This does not create Explorer client-area blur.
use super::*;
use release::ExplorerCaption;
#[repr(C)] struct ModuleEntry{size:u32,module_id:u32,pid:u32,global_usage:u32,process_usage:u32,base:*mut u8,base_size:u32,module:H,name:[u16;256],path:[u16;260]}
#[link(name="kernel32")] extern "system"{fn Module32FirstW(snapshot:H,entry:*mut ModuleEntry)->i32;fn Module32NextW(snapshot:H,entry:*mut ModuleEntry)->i32;}
fn known_provider(name:&str)->bool{name.eq_ignore_ascii_case("ExplorerBlurMica.dll")}
fn provider_loaded(h:H)->Option<bool>{unsafe{
 let (_,pid)=identity(h);if pid==0{return None;}
 let snapshot=CreateToolhelp32Snapshot(0x18,pid);if snapshot==0||snapshot==-1{return None;}
 let _guard=Instance(snapshot);
 let mut m:ModuleEntry=zeroed();m.size=size_of::<ModuleEntry>() as u32;
 if Module32FirstW(snapshot,&mut m)==0{return None;}
 loop{let n=m.name.iter().position(|&c|c==0).unwrap_or(m.name.len());if known_provider(&String::from_utf16_lossy(&m.name[..n])){return Some(true);}m.size=size_of::<ModuleEntry>() as u32;if Module32NextW(snapshot,&mut m)==0{return if GetLastError()==18{Some(false)}else{None};}}
}}
pub(super) fn choose_fill(h:H)->Option<u32>{
 let mode=release::config().explorer_caption;
 let fill=match mode{ExplorerCaption::Native=>None,ExplorerCaption::SolidDark=>Some(0x202020),ExplorerCaption::SolidLight=>Some(0xf0f0f0),ExplorerCaption::Auto=>match provider_loaded(h){Some(false)=>Some(if std::env::args().any(|s|s=="--light"){0xf0f0f0}else{0x202020}),_=>None}};
 log(&format!("explorer_caption_policy hwnd={h:#x} mode={mode:?} solid={}",fill.is_some()));fill
}
fn band_geometry(frame:Rect,client_top:i32,center:Layout,left:Layout)->Option<Layout>{
 let d=center.dpi;let x=frame.left+scale(2,d);let right=frame.right-scale(2,d);let top=(frame.top+scale(2,d)).min(center.pos.y).max(frame.top);
 if center.fallback_row||client_top>frame.top+scale(40,d)||client_top<=top||right-x<scale(220,d)||right-x>16384{return None;}
 let h=client_top-top;let w=right-x;
 if h<scale(22,d)||(w as i64)*(h as i64)>1_000_000{return None;}
 if left.pos.x<x||left.pos.x+left.w>right||center.pos.x<x||center.pos.x+center.w>right||center.pos.y<top||center.pos.y+center.h>client_top||left.pos.y<top||left.pos.y+left.h>client_top{return None;}
 Some(Layout{pos:Point{x,y:top},w,h,..center})
}
pub(super) fn candidate(h:H,frame:Rect,center:Layout,left:Layout)->Option<Layout>{unsafe{
 if GetForegroundWindow()!=h{return None;}
 let mut c=Point::default();if ClientToScreen(h,&mut c)==0{return None;}
 band_geometry(frame,c.y,center,left)
}}
#[cfg(test)] mod tests{use super::*;
 #[test] fn provider_name_exact(){assert!(known_provider("EXPLORERBLURMICA.DLL"));assert!(!known_provider("other.dll"));}
 #[test] fn verified_geometry_stays_outside_client(){for d in [96,120,144,192]{let frame=Rect{left:0,top:0,right:scale(1000,d),bottom:scale(600,d)};let left=Layout{pos:Point{x:scale(8,d),y:scale(4,d)},w:scale(66,d),h:scale(22,d),dpi:d,work_top:None,fallback_row:false,left_slot:true};let center=Layout{pos:Point{x:scale(300,d),y:left.pos.y},w:scale(400,d),..left};let b=band_geometry(frame,scale(30,d),center,left).unwrap();assert_eq!(b.pos.y+b.h,scale(30,d));assert!(band_geometry(frame,scale(20,d),center,left).is_none());assert!(band_geometry(frame,scale(60,d),center,left).is_none());}}
}
