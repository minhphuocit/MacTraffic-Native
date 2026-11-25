//! Experimental in-surface theme painter. Probe-only by default.
//! No hooks, process injection, desktop capture, registry or system file changes.
#![cfg_attr(not(windows), allow(dead_code))]
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Rect { left: i32, top: i32, right: i32, bottom: i32 }
fn role(part: i32) -> Option<u32> { match part { 15 => Some(0), 17 | 21 => Some(1), 18 => Some(2), _ => None } }
fn color(role: u32, state: i32) -> u32 {
    if state == 4 { return 0x008b8b8b; }
    // COLORREF = 0x00BBGGRR.
    match role { 0 => 0x002ebcfe, 1 => 0x0040c828, _ => 0x00575fff }
}
fn circle(r: Rect, dpi: u32) -> Option<Rect> {
    let w = r.right.checked_sub(r.left)?;
    let h = r.bottom.checked_sub(r.top)?;
    if !(72..=480).contains(&dpi) || w < 12 || h < 12 || w > 400 || h > 200 { return None; }
    let d = ((12 * dpi as i32 + 48) / 96).min(w - 4).min(h - 4);
    if d < 8 { return None; }
    let x = r.left.checked_add((w-d)/2)?;
    let y = r.top.checked_add((h-d)/2)?;
    Some(Rect {left:x,top:y,right:x.checked_add(d)?,bottom:y.checked_add(d)?})
}
#[cfg(windows)]
mod win {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::io::Write;
    type H = isize;
    static SEEN: AtomicU32 = AtomicU32::new(0);
    #[link(name="gdi32")]
    extern "system" {
        fn SaveDC(dc:H)->i32;
        fn RestoreDC(dc:H,saved:i32)->i32;
        fn CreateSolidBrush(c:u32)->H;
        fn GetStockObject(i:i32)->H;
        fn SelectObject(dc:H,obj:H)->H;
        fn DeleteObject(obj:H)->i32;
        fn Ellipse(dc:H,l:i32,t:i32,r:i32,b:i32)->i32;
    }
    fn note(bit: u32, message: &str) {
        if SEEN.fetch_or(bit, Ordering::Relaxed) & bit != 0 {return;}
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true)
            .open(std::env::temp_dir().join("MacTrafficNative-v030.log")) {
            let _ = writeln!(f,"pid={} {}",std::process::id(),message);
        }
    }
    #[no_mangle]
    pub extern "system" fn mt_caption_probe_start() {
        note(1<<9, if cfg!(feature="paint") {"bridge_ready mode=paint"} else {"bridge_ready mode=probe"});
    }
    // part and state from the upstream theme callback. 'direct' means the
    // upstream caller verified this DC belongs to the exact Explorer window.
    // A zero return always leaves the original paint path in charge.
    #[no_mangle]
    pub extern "system" fn mt_caption_draw(dc:H,part:i32,state:i32,r:Rect,dpi:u32,direct:i32,clip_ok:i32)->i32 {
        let Some(k) = role(part) else {return 0};
        let valid = circle(r,dpi);
        let safe = direct == 1 && clip_ok == 1 && dc != 0 && (1..=4).contains(&state) && valid.is_some();
        let shift = k + if safe { 0 } else { 3 };
        note(1<<shift,&format!("caption_candidate role={} part={} state={} rect={:?} dpi={} direct={} clip_ok={} eligible={} paint_feature={}",k,part,state,r,dpi,direct,clip_ok,safe,cfg!(feature="paint")));
        if !cfg!(feature="paint") || !safe {return 0;}
        let Some(c) = valid else {return 0;};
        unsafe {
            let saved=SaveDC(dc); if saved==0 {return 0;}
            let brush=CreateSolidBrush(color(k,state));
            if brush==0 {RestoreDC(dc,saved);return 0;}
            let pen=GetStockObject(8); // NULL_PEN
            let old_b=SelectObject(dc,brush);
            let old_p=if pen!=0 {SelectObject(dc,pen)} else {0};
            let ok=old_b!=0&&old_b != -1&&old_p!=0&&old_p != -1&&Ellipse(dc,c.left,c.top,c.right,c.bottom)!=0;
            RestoreDC(dc,saved); DeleteObject(brush);
            if ok {note(1<<(k+6),&format!("caption_painted role={}",k));1} else {0}
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn exact_roles(){assert_eq!(role(15),Some(0));assert_eq!(role(17),Some(1));assert_eq!(role(21),Some(1));assert_eq!(role(18),Some(2));assert_eq!(role(19),None);}
    #[test] fn centers(){let r=circle(Rect{left:100,top:20,right:148,bottom:50},96).unwrap();assert_eq!((r.left,r.top,r.right,r.bottom),(118,29,130,41));}
    #[test] fn reject_bad(){assert!(circle(Rect{left:0,top:0,right:10,bottom:48},96).is_none());assert!(circle(Rect{left:0,top:0,right:48,bottom:48},0).is_none());}
    #[test] fn correct_color(){assert_eq!(color(2,1),0x00575fff);assert_eq!(color(0,4),0x008b8b8b);}
}
