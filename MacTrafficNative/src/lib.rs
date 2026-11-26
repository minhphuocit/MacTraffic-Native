//! Diagnostic sink and opt-in native NC renderer for existing ExplorerBlurMica callbacks.
use std::sync::{Mutex, OnceLock};
use std::io::Write;
static SEEN: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
fn describe(kind:i32,a:i32,b:i32,c:i32,d:i32,theme:&str)->String {
    match kind {
        0 => "bridge_ready mode=frame_lab_v038".into(),
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
        10 => format!("frame_geometry delta_top={a} delta_left={b} delta_width={c} delta_height={d}"),
        11 => format!("frame_geometry_accepted delta_top={a} expected_band={b} tolerance={c} delta_bottom={d}"),
        12 => format!("frame_client_slot left={a} top={b} right={c} bottom={d}"),
        13 => format!("frame_repaint reason={a} left={b} right={c} maximized={d} host={theme:?}"),
        14 => format!("frame_ribbon_title_fill left={a} top={b} right={c} bottom={d}"),
        15 => format!("frame_ribbon_buffer result={a} left={b} top={c} detail={d}"),
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
    if let Ok(mut f)=std::fs::OpenOptions::new().create(true).append(true).open(std::env::temp_dir().join("MacTrafficNative-v038.log")) {
        let _=writeln!(f,"pid={} {}",std::process::id(),line);
    }
}
/// # Safety
/// If non-null, theme must point to len readable UTF-16 code units for this call.
#[no_mangle]
pub unsafe extern "system" fn mt_probe_event(kind:i32,a:i32,b:i32,c:i32,d:i32,theme:*const u16,len:u32) {
    let name=if (kind==3 || kind==13) && !theme.is_null() && len<=255 {
        String::from_utf16_lossy(std::slice::from_raw_parts(theme,len as usize))
    } else {String::new()};
    record(kind,describe(kind,a,b,c,d,&name));
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn ready(){assert_eq!(describe(0,0,0,0,0,""),"bridge_ready mode=frame_lab_v038");}
    #[test] fn ribbon_buffer_log(){assert!(describe(15,0,987,0,1133,"").contains("result=0"));}
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
    if r.left<0 || r.top<0 || r.right>32768 || r.bottom>32768 || r.right-r.left<scale(94,dpi) || r.bottom-r.top<scale(12,dpi){return None;}
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
// Explicit ARGB color, including alpha, for the DWM glass surface.
const DOT_COLORS:[u32;3]=[0xffff5f57,0xfffebc2e,0xff28c840];
#[cfg(windows)]
mod native_paint {
    use super::*;
    use std::ffi::c_void;
    type H=isize;
    #[repr(C)] struct BitmapInfo {size:u32,width:i32,height:i32,planes:u16,bits:u16,compression:u32,image_size:u32,xppm:i32,yppm:i32,used:u32,important:u32,colors:u32}
    #[link(name="gdi32")] extern "system" {
        fn CreateCompatibleDC(dc:H)->H;
        fn DeleteDC(dc:H)->i32;
        fn CreateDIBSection(dc:H,info:*const BitmapInfo,usage:u32,bits:*mut *mut c_void,section:H,offset:u32)->H;
        fn SelectObject(dc:H,obj:H)->H;
        fn DeleteObject(obj:H)->i32;
        fn BitBlt(dc:H,x:i32,y:i32,w:i32,h:i32,src:H,sx:i32,sy:i32,rop:u32)->i32;
        fn GdiFlush()->i32;
    }
    /// # Safety: caller owns a valid clipped HDC. No title or background is painted.
    #[no_mangle]
    pub unsafe extern "system" fn mt_frame_draw(dc:H,left:i32,top:i32,right:i32,bottom:i32,dpi:u32,_title:*const u16,_len:u32,_active:i32)->i32{
        if dc==0{return 0;}
        let Some(l)=layout(FrameRect{left,top,right,bottom},dpi) else{return 0;};
        let d=2*l.radius+1;
        if d<1||d>128{return 0;}
        let mem=CreateCompatibleDC(dc);if mem==0{return 0;}
        let info=BitmapInfo{size:40,width:d,height:-d,planes:1,bits:32,compression:0,image_size:0,xppm:0,yppm:0,used:0,important:0,colors:0};
        let mut bits=std::ptr::null_mut();
        let bitmap=CreateDIBSection(mem,&info,0,&mut bits,0,0);
        if bitmap==0||bits.is_null(){if bitmap!=0{DeleteObject(bitmap);}DeleteDC(mem);return 0;}
        let old=SelectObject(mem,bitmap);if old==0||old == -1{DeleteObject(bitmap);DeleteDC(mem);return 0;}
        let mut ok=true;
        for (i,x) in l.centers.iter().enumerate(){
            // Flush batched GDI before reusing the DIB memory for another color.
            GdiFlush();
            std::slice::from_raw_parts_mut(bits.cast::<u32>(),(d*d) as usize).fill(DOT_COLORS[i]);
            // Copy only the disk's horizontal spans. Never overwrite the surrounding glass.
            for yy in -l.radius..=l.radius{
                let mut half=l.radius;
                while half*half+yy*yy>l.radius*l.radius{half-=1;}
                ok &= BitBlt(dc,*x-half,l.y+yy,2*half+1,1,mem,l.radius-half,l.radius+yy,0x00cc0020)!=0;
            }
        }
        GdiFlush();SelectObject(mem,old);DeleteObject(bitmap);DeleteDC(mem);
        if ok{1}else{0}
    }
}
#[cfg(test)] mod frame_tests {
    use super::*;
    #[test] fn hit_order(){let r=FrameRect{left:8,top:8,right:712,bottom:31};let l=layout(r,96).unwrap();for(i,x)in l.centers.iter().enumerate(){assert_eq!(frame_hit(*x,l.y,r,96),i as i32);}assert_eq!(frame_hit(50,12,r,96),-1);assert_eq!(frame_hit(l.centers[0],31,r,96),-1);}
    #[test] fn scaled(){let r=FrameRect{left:12,top:12,right:1000,bottom:48};let l=layout(r,144).unwrap();assert_eq!(l.centers,[895,928,961]);assert_eq!(l.radius,9);}
    #[test] fn opaque_colors(){assert_eq!(DOT_COLORS,[0xffff5f57,0xfffebc2e,0xff28c840]);assert!(DOT_COLORS.iter().all(|v|v>>24==255));}
    #[test] fn original_slot(){let r=FrameRect{left:900,top:0,right:1046,bottom:23};let l=layout(r,96).unwrap();assert_eq!(l.centers,[976,998,1020]);}
    #[test] fn narrow(){assert!(layout(FrameRect{left:0,top:0,right:80,bottom:26},96).is_none());}
}
