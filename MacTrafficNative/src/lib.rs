//! Diagnostic sink and opt-in native NC renderer for existing ExplorerBlurMica callbacks.
use std::sync::{Mutex, OnceLock};
use std::io::Write;
static SEEN: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
fn describe(kind:i32,a:i32,b:i32,c:i32,d:i32,theme:&str)->String {
    match kind {
        0 => "bridge_ready mode=frame_lab_v034".into(),
        1 => format!("config effect={a} small_border_requested={b} elapsed_ms={c} global_border_change=skipped"),
        2 => format!("blur_init effect={a} elapsed_ms={b} build={c}"),
        3 => format!("theme_candidate class={theme:?} part={a} state={b} tracked={c} path={d}"),
        4 => format!("nc_message msg=0x{a:04x} dwm_nc_enabled={b} enabled_hr=0x{:08x} bounds_hr=0x{:08x}",c as u32,d as u32),
        5 => format!("dwm_bounds msg=0x{a:04x} width={b} height={c}"),
        6 => match a {
            0 => "frame_menu_ready".into(),
            1 => format!("frame_enabled client_top_before={b} client_top_after={c} band_height={d}"),
            2 => "frame_restored".into(),
            _ => format!("frame_rejected reason={b} detail1={c} detail2={d}"),
        },
        7 => format!("frame_draw ok={a} width={b} height={c}"),
        8 => format!("frame_click role={a}"),
        9 => format!("frame_dwm bounds_hr=0x{:08x} width={b} height={c} caption_style={d}",a as u32),
        _ => String::new(),
    }
}
fn record(kind:i32,line:String) {
    if line.is_empty() {return;}
    let seen=SEEN.get_or_init(||Mutex::new(Vec::new()));
    let Ok(mut lines)=seen.try_lock() else {return;};
    // Reserve room for startup timing and NC messages independently of theme volume.
    let prefix=if kind==3 {"theme_candidate"} else {""};
    let used=lines.iter().filter(|s|s.starts_with("theme_candidate")==(!prefix.is_empty())).count();
    let limit=if kind==3 {64} else {48};
    if used>=limit || lines.contains(&line) {return;}
    lines.push(line.clone());
    if let Ok(mut f)=std::fs::OpenOptions::new().create(true).append(true).open(std::env::temp_dir().join("MacTrafficNative-v034.log")) {
        let _=writeln!(f,"pid={} {}",std::process::id(),line);
    }
}
/// # Safety
/// If non-null, theme must point to len readable UTF-16 code units for this call.
#[no_mangle]
pub unsafe extern "system" fn mt_probe_event(kind:i32,a:i32,b:i32,c:i32,d:i32,theme:*const u16,len:u32) {
    let name=if kind==3 && !theme.is_null() && len<=255 {
        String::from_utf16_lossy(std::slice::from_raw_parts(theme,len as usize))
    } else {String::new()};
    record(kind,describe(kind,a,b,c,d,&name));
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn ready(){assert_eq!(describe(0,0,0,0,0,""),"bridge_ready mode=frame_lab_v034");}
    #[test] fn timing(){assert!(describe(2,2,123,18363,0,"").contains("elapsed_ms=123"));}
    #[test] fn names(){assert!(describe(3,18,1,0,2,"Window").contains("class=\"Window\""));}
    #[test] fn dwm(){assert!(describe(4,0x85,1,0,-1,"").contains("bounds_hr=0xffffffff"));}
}


// Native NC renderer experiment: shared geometry for paint and hit test.
// No background fill: existing DWM/Explorer blur must remain visible.
#[derive(Clone, Copy)]
#[repr(C)]
struct FrameRect { left:i32, top:i32, right:i32, bottom:i32 }
#[derive(Clone, Copy)]
struct Layout { centers:[i32;3], y:i32, radius:i32, hit:i32 }
fn scale(v:i32,dpi:u32)->i32 {(v*dpi.clamp(96,480) as i32+48)/96}
fn layout(r:FrameRect,dpi:u32)->Option<Layout>{
    if r.left<0 || r.top<0 || r.right>32768 || r.bottom>32768 || r.right-r.left<scale(160,dpi) || r.bottom-r.top<scale(12,dpi){return None;}
    let radius=scale(6,dpi).min((r.bottom-r.top-2)/2);
    Some(Layout{centers:[r.right-scale(70,dpi),r.right-scale(48,dpi),r.right-scale(26,dpi)],y:r.top+(r.bottom-r.top)/2,radius,hit:scale(10,dpi).min((r.bottom-r.top)/2)})
}
fn frame_hit(x:i32,y:i32,r:FrameRect,dpi:u32)->i32{
    if x<r.left||x>=r.right||y<r.top||y>=r.bottom{return -1;}
    let Some(l)=layout(r,dpi) else{return -1;};
    l.centers.iter().position(|cx|(x-*cx).abs()<=l.hit&&(y-l.y).abs()<=l.hit).map(|i|i as i32).unwrap_or(-1)
}
#[no_mangle]
pub extern "system" fn mt_frame_hit(x:i32,y:i32,left:i32,top:i32,right:i32,bottom:i32,dpi:u32)->i32{
    frame_hit(x,y,FrameRect{left,top,right,bottom},dpi)
}
#[cfg(windows)]
mod native_paint {
    use super::*;
    type H=isize;
    #[link(name="gdi32")] extern "system" {
        fn CreateSolidBrush(color:u32)->H;
        fn SelectObject(dc:H,obj:H)->H;
        fn DeleteObject(obj:H)->i32;
        fn GetStockObject(obj:i32)->H;
        fn Ellipse(dc:H,left:i32,top:i32,right:i32,bottom:i32)->i32;
        fn SetBkMode(dc:H,mode:i32)->i32;
        fn SetTextColor(dc:H,color:u32)->u32;
    }
    #[link(name="user32")] extern "system" {
        fn DrawTextW(dc:H,text:*const u16,len:i32,r:*mut FrameRect,flags:u32)->i32;
        fn GetSysColor(index:i32)->u32;
    }
    /// # Safety: DC belongs to the caller and has a valid clip; title points to len UTF-16 units.
    #[no_mangle]
    pub unsafe extern "system" fn mt_frame_draw(dc:H,left:i32,top:i32,right:i32,bottom:i32,dpi:u32,title:*const u16,len:u32,active:i32)->i32{
        if dc==0{return 0;}
        let Some(l)=layout(FrameRect{left,top,right,bottom},dpi) else{return 0;};
        let pen=SelectObject(dc,GetStockObject(8)); // NULL_PEN
        if pen==0||pen == -1{return 0;}
        let mut all=true;
        for (i,x) in l.centers.iter().enumerate(){
            let brush=CreateSolidBrush([0x00575fff,0x002ebcfe,0x0040c828][i]); // COLORREF = BGR
            if brush==0{all=false;continue;}
            let old=SelectObject(dc,brush);
            if old!=0&&old != -1 {
                all &= Ellipse(dc,*x-l.radius,l.y-l.radius,*x+l.radius+1,l.y+l.radius+1)!=0;
                SelectObject(dc,old);
            }else{all=false;}
            DeleteObject(brush);
        }
        SelectObject(dc,pen);
        if !title.is_null()&&len>0&&len<=255{
            let mut r=FrameRect{left:left+scale(12,dpi),top,right:right-scale(94,dpi),bottom};
            if r.right>r.left{
                let old=SelectObject(dc,GetStockObject(17));
                SetBkMode(dc,1);
                SetTextColor(dc,GetSysColor(if active!=0{9}else{19}));
                DrawTextW(dc,title,len as i32,&mut r,0x20|0x4|0x8000|0x800);
                if old!=0&&old != -1{SelectObject(dc,old);}
            }
        }
        if all {1}else{0}
    }
}
#[cfg(test)] mod frame_tests {
    use super::*;
    #[test] fn hit_order(){let r=FrameRect{left:8,top:8,right:712,bottom:31};let l=layout(r,96).unwrap();for(i,x)in l.centers.iter().enumerate(){assert_eq!(frame_hit(*x,l.y,r,96),i as i32);}assert_eq!(frame_hit(50,12,r,96),-1);assert_eq!(frame_hit(l.centers[0],31,r,96),-1);}
    #[test] fn scaled(){let r=FrameRect{left:12,top:12,right:1000,bottom:48};let l=layout(r,144).unwrap();assert_eq!(l.centers,[895,928,961]);assert_eq!(l.radius,9);}
    #[test] fn narrow(){assert!(layout(FrameRect{left:0,top:0,right:100,bottom:26},96).is_none());}
}
