//! Experimental opt-in: identify dedicated, locally named spacer tabs using UIA.
//! No browser process writes, extension, input forwarding, or tab creation.
use super::*;
pub(super) const VALID:Duration=Duration::from_secs(4);
pub(super) const PERIOD:Duration=Duration::from_millis(1400);
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(super) struct Key{pub req:Request,pub right:Layout,pub work:Rect,pub count:usize,pub epoch:Instant}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(super) enum Verdict{Verified(Layout),Rejected,Unavailable}
struct Probe{tx:mpsc::SyncSender<Key>,rx:mpsc::Receiver<(Key,Verdict,Instant)>,pending:bool,cache:Vec<(Key,Verdict,Instant,Instant)>}
thread_local!{static PROBE:RefCell<Option<Probe>>=RefCell::new(None);}
#[cfg(test)]
fn marker(name:&str)->bool{
 // Chromium can append localized accessibility status to a tab's title.
 // Never accept generic Untitled, New Tab, or about:blank labels.
 name.match_indices("MacTraffic Spacer 072").any(|(i,m)|{
  let before=name[..i].chars().next_back();let after=name[i+m.len()..].chars().next();
  !before.is_some_and(|c|c.is_alphanumeric())&&!after.is_some_and(|c|c.is_alphanumeric())
 })
}
// Match the same marker and Unicode word boundaries without allocating a
// UTF-8 String. Invalid UTF-16 is treated like from_utf16_lossy (U+FFFD).
pub(super) fn marker_utf16(name:&[u16])->bool{
 const M:&[u8]=b"MacTraffic Spacer 072";
 fn first_alnum(s:&[u16])->bool{
  char::decode_utf16(s.iter().copied()).next().and_then(Result::ok).is_some_and(char::is_alphanumeric)
 }
 fn last_alnum(s:&[u16])->bool{
  let Some(&last)=s.last()else{return false;};
  let start=if (0xdc00..=0xdfff).contains(&last)&&s.len()>=2&&(0xd800..=0xdbff).contains(&s[s.len()-2]){s.len()-2}else{s.len()-1};
  first_alnum(&s[start..])
 }
 name.windows(M.len()).enumerate().any(|(i,w)|{
  w.iter().zip(M).all(|(&a,&b)|a==u16::from(b))&&!last_alnum(&name[..i])&&!first_alnum(&name[i+M.len()..])
 })
}
pub(super) fn poll(k:Key)->Option<(Verdict,Instant)>{PROBE.with(|cell|{
 let mut slot=cell.borrow_mut();
 if slot.is_none(){
  let (tx,rx)=mpsc::sync_channel::<Key>(1);let (out,ret)=mpsc::channel();
  std::thread::Builder::new().name("spacer-uia".into()).spawn(move||{
   let mut uia=UiaWorker::new("spacer");
   while let Ok(key)=rx.recv(){let start=Instant::now();let mut definite=false;let result=match unsafe{query_tabs(key,&mut definite,&mut uia)}{Some(mask)=>Verdict::Verified(mask),None if definite=>Verdict::Rejected,None=>Verdict::Unavailable};
    if out.send((key,result,start)).is_err(){break;}
    wake_manager();
   }
  }).ok()?;
  *slot=Some(Probe{tx,rx:ret,pending:false,cache:Vec::new()});
 }
 let p=slot.as_mut()?;
 while let Ok((key,result,t))=p.rx.try_recv(){
  p.pending=false;
  let changed=p.cache.iter().find(|(old,_,_,_)|*old==key).map(|(_,r,_,_)|*r)!=Some(result);
  if changed||t.elapsed()>=Duration::from_secs(1){log(&format!("browser_spacer_probe hwnd={:#x} expected={} verdict={result:?} elapsed_ms={}",key.req.hwnd,key.count,t.elapsed().as_millis()));}
  let next=Instant::now()+if matches!(result,Verdict::Verified(_)){PERIOD}else{Duration::from_millis(1200)};
  p.cache.retain(|(old,_,_,_)|old.req.hwnd!=key.req.hwnd);
  p.cache.push((key,result,t,next));
  if p.cache.len()>8{p.cache.remove(0);}
 }
 let entry=p.cache.iter().find(|(key,_,_,_)|*key==k).copied();
 if !p.pending&&entry.map_or(true,|(_,_,_,next)|Instant::now()>=next)&&p.tx.try_send(k).is_ok(){p.pending=true;}
 entry.filter(|(_,_,t,_)|t.elapsed()<=VALID).map(|(_,r,t,_)|(r,t))
})}
pub(super) fn shutdown(){PROBE.with(|cell|{drop(cell.borrow_mut().take());});}
#[derive(Clone,Copy,Debug)]struct Tab{rect:Rect,marked:bool}
// Edge's UIA TabItem rectangles include overlapping rounded tab margins.
// Only allow bounded overlap in the dedicated two-marker Edge adapter;
// gaps and the boundary of the first genuine tab remain independently checked.
fn adjacent(a:Rect,b:Rect,d:u32,max_gap:i32,max_overlap:i32)->bool{
 let gap=b.left-a.right;
 b.left>a.left&&b.right>a.right&&gap>=-scale(max_overlap,d)&&gap<=scale(max_gap,d)
}
pub(super) fn button_layout(mask:Layout)->Option<Layout>{
 let w=scale(66,mask.dpi);let h=scale(22,mask.dpi);
 // Preserve the previous minimum width; divide its spare pixels evenly.
 if mask.w<w+scale(8,mask.dpi)||mask.h<h{return None;}
 let inset=(mask.w-w)/2;
 Some(Layout{pos:Point{x:mask.pos.x+inset,y:mask.pos.y+(mask.h-h)/2},w,h,..mask})
}
fn geometry_in_place(k:Key,tabs:&mut [Tab])->Option<Layout>{
 let d=k.right.dpi;let row=k.right;let f=k.req.frame;
 if ![2,3].contains(&k.count)||tabs.len()<=k.count{return None;}
 tabs.sort_by_key(|t|t.rect.left);
 let start=f.left;let first=tabs[0].rect;
 if start<k.work.left||row.pos.y<k.work.top||row.pos.y+row.h>k.work.bottom{return None;}
 if first.left<start-scale(3,d)||first.left>start+scale(30,d){return None;}
 // First N entries must be explicitly named markers, followed by a genuine tab.
 if !tabs[..k.count].iter().all(|t|t.marked)||tabs[k.count..].iter().any(|t|t.marked){return None;}
 for (i,t) in tabs[..k.count].iter().enumerate(){
  let r=t.rect;
  if r.width()<scale(24,d)||r.width()>scale(85,d)||r.height()<scale(18,d)||r.top<row.pos.y-scale(3,d)||r.bottom>row.pos.y+row.h+scale(3,d){return None;}
  if i>0&&!adjacent(tabs[i-1].rect,r,d,12,if k.count==2{20}else{12}){return None;}
 }
 let last=tabs[k.count-1].rect;let next=tabs[k.count].rect;
 if !adjacent(last,next,d,14,if k.count==2{20}else{14}){return None;}
 let end=next.left.min(last.right);let w=end-start;
 if w<scale(72,d)||w>scale(180,d)||end>k.work.right||end>=row.pos.x-scale(12,d){return None;}
 // Never cover a real tab even if reported rectangles overlap.
 let mask=Layout{pos:Point{x:start,y:row.pos.y},w,h:row.h,left_slot:true,..row};
 button_layout(mask)?;
 Some(mask)
}
// Windows x64 VARIANT: VT_I4 plus its padded 16-byte payload union.
// This module is only used with the existing Windows GNU x64 build.
#[repr(C,align(8))]struct VariantI4{vt:u16,r1:u16,r2:u16,r3:u16,value:i32,pad:[u8;12]}
unsafe fn query_tabs(k:Key,definite:&mut bool,uia:&mut UiaWorker)->Option<Layout>{
 let r=k.req;if !active(r)||size_of::<usize>()!=8{return None;}
 let mut title_buf=[0u16;1024];
 if read_title_buf(r.hwnd,&mut title_buf).is_none_or(marker_utf16){return None;}
 let ctx=uia.get()?;let a=&ctx.automation;let free=ctx.free;let len=ctx.len;
 let from:unsafe extern "system" fn(P,H,*mut P)->i32=std::mem::transmute(slot(a.0,6));
 let mut raw=null_mut();let hr=from(a.0,r.hwnd,&mut raw);let root=Com(raw);if hr<0||raw.is_null(){return None;}
 let cond_fn:unsafe extern "system" fn(P,i32,VariantI4,*mut P)->i32=std::mem::transmute(slot(a.0,23));
 let mut raw=null_mut();let hr=cond_fn(a.0,30003,VariantI4{vt:3,r1:0,r2:0,r3:0,value:50019,pad:[0;12]},&mut raw);let cond=Com(raw);if hr<0||raw.is_null(){return None;}
 // Search only within the target browser HWND; never enumerate the desktop.
 let find:unsafe extern "system" fn(P,i32,P,*mut P)->i32=std::mem::transmute(slot(root.0,6));
 let mut raw=null_mut();let hr=find(root.0,4,cond.0,&mut raw);let arr=Com(raw);if hr<0||raw.is_null(){return None;}
 let length:unsafe extern "system" fn(P,*mut i32)->i32=std::mem::transmute(slot(arr.0,3));
 let item:unsafe extern "system" fn(P,i32,*mut P)->i32=std::mem::transmute(slot(arr.0,4));
 let mut n=0;if length(arr.0,&mut n)<0||!(1..=128).contains(&n){return None;}
 let mut tabs=Vec::with_capacity(n as usize);
 for i in 0..n{
  if !active(r){return None;}
  let mut raw=null_mut();let hr=item(arr.0,i,&mut raw);let el=Com(raw);if hr<0||raw.is_null(){return None;}
  let bounds:unsafe extern "system" fn(P,*mut Rect)->i32=std::mem::transmute(slot(el.0,43));
  let process:unsafe extern "system" fn(P,*mut i32)->i32=std::mem::transmute(slot(el.0,20));
  let name:unsafe extern "system" fn(P,*mut *mut u16)->i32=std::mem::transmute(slot(el.0,23));
  let mut rect=Rect::default();let mut pid=0;
  if bounds(el.0,&mut rect)<0||process(el.0,&mut pid)<0||pid as u32!=r.pid{continue;}
  if rect.width()<=0||rect.height()<scale(18,r.dpi)||rect.left<r.frame.left-scale(3,r.dpi)||rect.right>r.frame.right||rect.top<k.right.pos.y-scale(3,r.dpi)||rect.bottom>k.right.pos.y+k.right.h+scale(3,r.dpi){continue;}
  let mut txt=null_mut();let hr=name(el.0,&mut txt);if txt.is_null(){return None;}
  let count=len(txt) as usize;let marked=hr>=0&&count<=512&&marker_utf16(std::slice::from_raw_parts(txt,count));free(txt);
  if hr<0{return None;}tabs.push(Tab{rect,marked});
 }
 if !active(r)||read_title_buf(r.hwnd,&mut title_buf).is_none_or(marker_utf16){return None;}
 if tabs.is_empty(){return None;}
 *definite=true;let result=geometry_in_place(k,&mut tabs);
 if result.is_none(){log(&format!("browser_spacer_geometry_rejected hwnd={:#x} tabs={tabs:?}",r.hwnd));}
 result
}
pub(super) fn render(sprite:H,mask:Layout,buttons:Layout,hover:bool,rows:&[u32])->bool{
 if mask.h as usize!=rows.len()||mask.w<1||buttons.pos.x<mask.pos.x||buttons.pos.y<mask.pos.y||buttons.pos.x+buttons.w>mask.pos.x+mask.w||buttons.pos.y+buttons.h>mask.pos.y+mask.h{return false;}
 let mut pixels=vec![0;mask.w as usize*mask.h as usize];
 for (y,row) in pixels.chunks_exact_mut(mask.w as usize).enumerate(){row.fill(0xff000000|rows[y]);}
 let dots=pixel_buffer(buttons,hover);let dx=(buttons.pos.x-mask.pos.x) as usize;let dy=(buttons.pos.y-mask.pos.y) as usize;
 for y in 0..buttons.h as usize{for x in 0..buttons.w as usize{let idx=(y+dy)*mask.w as usize+x+dx;pixels[idx]=composite(dots[y*buttons.w as usize+x],pixels[idx]);}}
 present_pixels(sprite,mask,&pixels)
}
#[cfg(test)]mod tests{use super::*;
 fn geometry(k:Key,mut tabs:Vec<Tab>)->Option<Layout>{geometry_in_place(k,&mut tabs)}
 #[test]fn utf16_marker_matches_previous(){
  for a in ["","a","é","𐐀","😀","_"," ","-","9"]{for b in ["","a","é","𐐀","😀","_"," ","-","9"]{
   let s=format!("{a}MacTraffic Spacer 072{b}");assert_eq!(marker(&s),marker_utf16(&s.encode_utf16().collect::<Vec<_>>()),"{s}");
  }}
  for s in ["about:blank","New Tab","MacTraffic Spacer 0729","MacTraffic Spacer 072 - Pinned tab","MacTraffic Spacer 072x MacTraffic Spacer 072"]{assert_eq!(marker(s),marker_utf16(&s.encode_utf16().collect::<Vec<_>>()));}
  let m:Vec<u16>="MacTraffic Spacer 072".encode_utf16().collect();for bad in [0xd800u16,0xdc00]{let mut v=vec![bad];v.extend_from_slice(&m);v.push(bad);assert_eq!(marker(&String::from_utf16_lossy(&v)),marker_utf16(&v));}
 }
 fn key(count:usize)->Key{let frame=Rect{left:0,top:24,right:1920,bottom:1080};Key{req:Request{hwnd:0,pid:0,frame,dpi:96,browser:true,native_probe:false,hint:None},right:Layout{pos:Point{x:1783,y:24},w:137,h:40,dpi:96,work_top:Some(24),fallback_row:false,left_slot:false},work:frame,count,epoch:Instant::now()}}
 fn tab(left:i32,right:i32,marked:bool)->Tab{Tab{rect:Rect{left,top:26,right,bottom:62},marked}}
 #[test]fn brave_three(){assert_eq!(geometry(key(3),vec![tab(3,37,true),tab(39,73,true),tab(75,109,true),tab(110,340,false)]).unwrap().w,109);}
 #[test]fn edge_two(){assert_eq!(geometry(key(2),vec![tab(3,73,true),tab(74,129,true),tab(130,340,false)]).unwrap().w,129);}
 #[test]fn edge_observed_overlap(){let mut k=key(2);k.right=Layout{pos:Point{x:1774,y:25},w:138,h:39,..k.right};let t=|left,right,marked|Tab{rect:Rect{left,top:24,right,bottom:65},marked};let m=geometry(k,vec![t(10,82,true),t(65,137,true),t(120,376,false)]).unwrap();assert_eq!(m.w,120);assert_eq!(m.pos.x+m.w,120);let b=button_layout(m).unwrap();assert_eq!(b.pos.x,27);assert!(b.pos.x+b.w<=120);}
 #[test]fn edge_reject_excessive_overlap(){assert!(geometry(key(2),vec![tab(10,82,true),tab(60,132,true),tab(115,376,false)]).is_none());assert!(geometry(key(2),vec![tab(10,82,true),tab(65,137,true),tab(110,376,false)]).is_none());}
 #[test]fn brave_offset_and_fit(){let m=geometry(key(3),vec![tab(3,37,true),tab(39,73,true),tab(75,109,true),tab(110,340,false)]).unwrap();let b=button_layout(m).unwrap();assert_eq!(b.pos,Point{x:21,y:33});assert!(button_layout(Layout{w:73,..m}).is_none());}
 #[test]fn observed_balanced_masks(){
  let r=key(3).right;
  let brave=Layout{pos:Point{x:0,y:24},w:108,h:40,left_slot:true,..r};
  let edge=Layout{pos:Point{x:7,y:25},w:113,h:39,left_slot:true,..r};
  assert_eq!(button_layout(brave).unwrap().pos,Point{x:21,y:33});
  assert_eq!(button_layout(edge).unwrap().pos,Point{x:30,y:33});
  for dpi in [96,120,144,192]{for width in [74,108,113,180]{
   let mask=Layout{pos:Point{x:-200,y:70},w:scale(width,dpi),h:scale(40,dpi),dpi,..r};
   let b=button_layout(mask).unwrap();
   let left=b.pos.x-mask.pos.x;let right=mask.pos.x+mask.w-b.pos.x-b.w;
   assert!(left>=0&&right>=0&&(left-right).abs()<=1);
   assert!(b.pos.y>=mask.pos.y&&b.pos.y+b.h<=mask.pos.y+mask.h);
  }}
 }
 #[test]fn no_real_tab_mask(){assert!(geometry(key(2),vec![tab(3,40,true),tab(40,80,false),tab(80,340,false)]).is_none());assert!(geometry(key(2),vec![tab(3,40,true),tab(40,80,true)]).is_none());}
 #[test]fn no_generic_blanks(){assert!(!marker("about:blank"));assert!(!marker("New Tab"));assert!(marker("MacTraffic Spacer 072 - Pinned tab"));assert!(!marker("MacTraffic Spacer 0729"));assert_eq!(size_of::<VariantI4>(),24);}
}
