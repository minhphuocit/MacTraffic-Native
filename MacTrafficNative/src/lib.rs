//! Bounded read-only diagnostic sink for the upstream ExplorerBlurMica callbacks.
//! Does not paint, alter windows, capture the desktop or add hooks.
use std::sync::{Mutex, OnceLock};
use std::io::Write;
static SEEN: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
fn describe(kind:i32,a:i32,b:i32,c:i32,d:i32,theme:&str)->String {
    match kind {
        0 => "bridge_ready mode=probe_v031".into(),
        1 => format!("config effect={a} small_border_requested={b} elapsed_ms={c} global_border_change=skipped"),
        2 => format!("blur_init effect={a} elapsed_ms={b} build={c}"),
        3 => format!("theme_candidate class={theme:?} part={a} state={b} tracked={c} path={d}"),
        4 => format!("nc_message msg=0x{a:04x} dwm_nc_enabled={b} enabled_hr=0x{:08x} bounds_hr=0x{:08x}",c as u32,d as u32),
        5 => format!("dwm_bounds msg=0x{a:04x} width={b} height={c}"),
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
    if let Ok(mut f)=std::fs::OpenOptions::new().create(true).append(true).open(std::env::temp_dir().join("MacTrafficNative-v031.log")) {
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
    #[test] fn ready(){assert_eq!(describe(0,0,0,0,0,""),"bridge_ready mode=probe_v031");}
    #[test] fn timing(){assert!(describe(2,2,123,18363,0,"").contains("elapsed_ms=123"));}
    #[test] fn names(){assert!(describe(3,18,1,0,2,"Window").contains("class=\"Window\""));}
    #[test] fn dwm(){assert!(describe(4,0x85,1,0,-1,"").contains("bounds_hr=0xffffffff"));}
}
