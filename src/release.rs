//! Release shell. No external crates; settings beside the portable executable.
use super::*;
use std::{io::Write, os::windows::ffi::OsStrExt, path::{Path,PathBuf}, sync::{OnceLock,Mutex,atomic::{AtomicBool,AtomicIsize,AtomicU32,Ordering}}};
const DEFAULT_INI:&str=include_str!("../MacTraffic.ini");
const LOG_NAME:&str="MacTraffic-v101.log";
const MAX_LOG:u64=1024*1024;
#[derive(Clone,Copy,Debug,PartialEq,Eq)] enum LogLevel{Normal,Debug,Off}
#[derive(Debug)] pub(super) struct Config{
 pub start_paused:bool,pub browser_spacer:bool,pub office_exact_left:bool,pub alt_keys:bool,pub native_maximize:bool,
 pub browser_enabled:bool,pub office_enabled:bool,excluded:Vec<String>,level:LogLevel,
}
impl Default for Config{fn default()->Self{Self{start_paused:false,browser_spacer:true,office_exact_left:true,alt_keys:true,native_maximize:false,browser_enabled:true,office_enabled:true,excluded:vec![],level:LogLevel::Normal}}}
static CONFIG:OnceLock<Config>=OnceLock::new();
static LOG_LOCK:Mutex<()>=Mutex::new(());
static LOG_OVERRIDE:OnceLock<bool>=OnceLock::new();
static TRAY_OWNER:AtomicIsize=AtomicIsize::new(0);
static TASKBAR_MESSAGE:AtomicU32=AtomicU32::new(0);
static TRAY_READD:AtomicBool=AtomicBool::new(false);
pub(super) fn taskbar_notification(h:H,m:u32)->bool{let id=TASKBAR_MESSAGE.load(Ordering::Relaxed);if id!=0&&m==id&&h==TRAY_OWNER.load(Ordering::Relaxed){TRAY_READD.store(true,Ordering::Release);wake_manager();true}else{false}}
pub(super) static PAUSED:AtomicBool=AtomicBool::new(false);
fn boolean(s:&str)->Result<bool,String>{match s.trim().to_ascii_lowercase().as_str(){"true"|"1"|"yes"=>Ok(true),"false"|"0"|"no"=>Ok(false),_=>Err(format!("Invalid boolean: {s}"))}}
fn parse(text:&str)->Result<Config,String>{
 let mut c=Config::default();
 for (i,line) in text.trim_start_matches('\u{feff}').lines().enumerate(){
  let line=line.trim();if line.is_empty()||line.starts_with('#')||line.starts_with(';')||line=="[MacTraffic]"{continue;}
  let (key,val)=line.split_once('=').ok_or_else(||format!("Invalid INI line {}",i+1))?;
  let val=val.trim();match key.trim().to_ascii_lowercase().as_str(){
   "start_paused"=>c.start_paused=boolean(val)?,"browser_spacer"=>c.browser_spacer=boolean(val)?,"office_exact_left"=>c.office_exact_left=boolean(val)?,"alt_keys"=>c.alt_keys=boolean(val)?,"native_maximize"=>c.native_maximize=boolean(val)?,"browser_enabled"=>c.browser_enabled=boolean(val)?,"office_enabled"=>c.office_enabled=boolean(val)?,
   "exclude"=>{c.excluded.clear();for item in val.split(',').map(|s|s.trim().to_ascii_lowercase()).filter(|s|!s.is_empty()){if !item.ends_with(".exe")||item.chars().any(|c|matches!(c, '\\'|'/'|':'|'*'|'?')){return Err(format!("Invalid executable exclusion: {item}"));}if !c.excluded.contains(&item){c.excluded.push(item);}}},
   "log_level"=>c.level=match val.to_ascii_lowercase().as_str(){"normal"=>LogLevel::Normal,"debug"=>LogLevel::Debug,"off"=>LogLevel::Off,_=>return Err("log_level must be normal, debug or off".into())},
   _=>return Err(format!("Unknown INI key on line {}: {key}",i+1)),
  }
 }Ok(c)
}
fn exe_path()->Result<PathBuf,String>{std::env::current_exe().map_err(|e|e.to_string())}
fn config_path()->Result<PathBuf,String>{Ok(exe_path()?.with_file_name("MacTraffic.ini"))}
pub(super) fn init()->Result<(),String>{let path=config_path()?;let contents=match std::fs::read_to_string(&path){Ok(t)=>t,Err(e) if e.kind()==std::io::ErrorKind::NotFound=>{let mut f=std::fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|e|format!("Cannot create {}: {e}",path.display()))?;f.write_all(DEFAULT_INI.as_bytes()).map_err(|e|e.to_string())?;DEFAULT_INI.to_owned()},Err(e)=>return Err(format!("Cannot read {}: {e}",path.display()))};let c=parse(&contents)?;CONFIG.set(c).map_err(|_|"Configuration already loaded".to_owned())}
pub(super) fn config()->&'static Config{CONFIG.get_or_init(Config::default)}
fn excluded(c:&Config,exe:&str,app:App,class:&str)->bool{
 c.excluded.iter().any(|e| e.eq_ignore_ascii_case(exe)||(app==App::GenericProbe&&matches!(class,"ApplicationFrameWindow"|"Windows.UI.Core.CoreWindow")&&e=="systemsettings.exe")||(app==App::GenericProbe&&class=="SunAwtFrame"&&matches!(e.as_str(),"simpmusic.exe"|"simp-music.exe")))
}
pub(super) fn is_excluded(exe:&str,app:App,class:&str)->bool{excluded(config(),exe,app,class)}
fn log_path()->PathBuf{std::env::temp_dir().join(LOG_NAME)}
fn normal_event(s:&str)->bool{
 let prefix=s.split_whitespace().next().unwrap_or("");
 s.starts_with("===")||s.starts_with("stop")||s.starts_with("restored=false")||prefix.contains("error")||prefix.contains("failed")||prefix.contains("panic")||matches!(prefix,"manager_ready"|"event_engine_ready"|"engine_stats"|"engine_exit_cleanup"|"release_paused"|"release_resumed"|"startup_changed"|"config_excluded"|"uia_context_created"|"uia_context_unavailable"|"owner_visibility_suspend"|"caption_visibility_recover"|"caption_retry_recovered"|"caption_auxiliary_recover"|"native_geometry_retry"|"generic_centered_wait"|"tray_unavailable")
}
pub(super) fn write_log(s:&str){
 let level=if *LOG_OVERRIDE.get_or_init(||std::env::args().any(|a|a=="--trace-focus"||a=="--debug-log")){LogLevel::Debug}else{CONFIG.get().map_or(LogLevel::Normal,|c|c.level)};
 if level==LogLevel::Off||(level==LogLevel::Normal&&!normal_event(s)){return;}
 let Ok(_guard)=LOG_LOCK.lock() else{return;};let path=log_path();
 if std::fs::metadata(&path).is_ok_and(|m|m.len()>=MAX_LOG){let previous=path.with_extension("previous.log");let _=std::fs::remove_file(&previous);if std::fs::rename(&path,&previous).is_err(){return;}}
 if let Ok(mut f)=std::fs::OpenOptions::new().create(true).append(true).open(path){let _=writeln!(f,"{s}");}
}
// The notification icon uses the documented x64 NOTIFYICONDATAW layout.
#[repr(C)] struct NotifyData{size:u32,hwnd:H,id:u32,flags:u32,callback:u32,icon:H,tip:[u16;128],state:u32,state_mask:u32,info:[u16;256],timeout:u32,info_title:[u16;64],info_flags:u32,guid:Guid,balloon_icon:H}
#[repr(C)] #[derive(Clone,Copy)] struct Guid{a:u32,b:u16,c:u16,d:[u8;8]}
#[link(name="shell32")] extern "system"{fn Shell_NotifyIconW(message:u32,data:*mut NotifyData)->i32;fn SHGetFolderPathW(hwnd:H,folder:i32,token:H,flags:u32,path:*mut u16)->i32;}
#[link(name="user32")] extern "system"{fn LoadIconW(instance:H,name:*const u16)->H;fn CreatePopupMenu()->H;fn AppendMenuW(menu:H,flags:u32,id:usize,text:*const u16)->i32;fn TrackPopupMenuEx(menu:H,flags:u32,x:i32,y:i32,hwnd:H,params:*const c_void)->u32;fn DestroyMenu(menu:H)->i32;fn RegisterWindowMessageW(name:*const u16)->u32;}
#[link(name="ole32")] extern "system"{fn CoInitializeEx(reserved:*mut c_void,coinit:u32)->i32;fn CoUninitialize();fn CoCreateInstance(clsid:*const Guid,outer:*mut c_void,context:u32,iid:*const Guid,result:*mut *mut c_void)->i32;}
pub(super) const TRAY_MESSAGE:u32=0x8002;
pub(super) const TOGGLE_ID:i32=6;
pub(super) const MENU_TOGGLE:u32=1;pub(super) const MENU_EXIT:u32=2;const MENU_CONFIG:u32=3;const MENU_LOG:u32=4;const MENU_STARTUP:u32=5;
pub(super) struct Tray{hwnd:H,added:bool,pub taskbar_created:u32,last_attempt:Instant}
impl Tray{
 pub(super) unsafe fn new(hwnd:H)->Self{let mut s=Self{hwnd,added:false,taskbar_created:RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()),last_attempt:Instant::now()};TRAY_OWNER.store(hwnd,Ordering::Release);TASKBAR_MESSAGE.store(s.taskbar_created,Ordering::Release);s.readd();s}
 unsafe fn data(&self)->NotifyData{let mut n:NotifyData=zeroed();n.size=size_of::<NotifyData>() as u32;n.hwnd=self.hwnd;n.id=1;n.flags=1|2|4;n.callback=TRAY_MESSAGE;n.icon=LoadIconW(0,32512usize as *const u16);let tip=if PAUSED.load(Ordering::Relaxed){"MacTraffic 1.0.1 - Paused"}else{"MacTraffic 1.0.1 - Running"};for (a,b) in n.tip.iter_mut().zip(tip.encode_utf16()){*a=b;}n}
 pub(super) unsafe fn readd(&mut self){self.last_attempt=Instant::now();let mut n=self.data();self.added=Shell_NotifyIconW(0,&mut n)!=0;if !self.added{log("tray_unavailable Ctrl+Alt+P / F12 still available");}}
 pub(super) unsafe fn refresh(&mut self){if self.added{let mut n=self.data();if Shell_NotifyIconW(1,&mut n)==0{self.added=false;}}}
 pub(super) unsafe fn retry(&mut self){if TRAY_READD.swap(false,Ordering::AcqRel)||(!self.added&&self.last_attempt.elapsed()>=Duration::from_secs(5)){self.readd();}}
 pub(super) unsafe fn menu(&self)->u32{
  let menu=CreatePopupMenu();if menu==0{return 0;}
  let paused=PAUSED.load(Ordering::Relaxed);let startup=startup_path().ok();let startup_on=startup.as_ref().is_some_and(|p|p.is_file());
  let add=|id:u32,label:&str,flags:u32|{AppendMenuW(menu,flags,id as usize,wide(label).as_ptr());};
  add(MENU_TOGGLE,if paused{"Resume MacTraffic (Ctrl+Alt+P)"}else{"Pause MacTraffic (Ctrl+Alt+P)"},0);
  add(MENU_CONFIG,"Edit configuration (restart to apply)",0);add(MENU_LOG,"Open log",0);
  add(MENU_STARTUP,if startup_on{"Disable start with Windows"}else{"Enable start with Windows"},if startup.is_none(){1}else{0});
  AppendMenuW(menu,0x800,0,null());add(MENU_EXIT,"Exit and restore (Ctrl+Alt+F12)",0);
  let mut p=Point::default();GetCursorPos(&mut p);SetForegroundWindow(self.hwnd);
  let cmd=TrackPopupMenuEx(menu,0x100|0x80|2,p.x,p.y,self.hwnd,null());DestroyMenu(menu);PostMessageW(self.hwnd,0,0,0);cmd
 }
}
impl Drop for Tray{fn drop(&mut self){TRAY_OWNER.store(0,Ordering::Release);TASKBAR_MESSAGE.store(0,Ordering::Release);if self.added{unsafe{let mut n=self.data();Shell_NotifyIconW(2,&mut n);}}}}
fn startup_path()->Result<PathBuf,String>{unsafe{let mut path=[0u16;260];let hr=SHGetFolderPathW(0,7,0,0,path.as_mut_ptr());if hr<0{return Err(format!("Startup folder unavailable: {hr:#x}"));}let end=path.iter().position(|&u|u==0).unwrap_or(path.len());use std::os::windows::ffi::OsStringExt;Ok(PathBuf::from(std::ffi::OsString::from_wide(&path[..end])).join("MacTraffic-v1.lnk"))}}
fn path_wide(path:&Path)->Vec<u16>{path.as_os_str().encode_wide().chain(Some(0)).collect()}
// IShellLinkW + IPersistFile, documented COM vtables. The only persistence
// operation is an explicit user-selected per-user Startup shortcut.
struct ComPtr(*mut c_void);impl Drop for ComPtr{fn drop(&mut self){if !self.0.is_null(){unsafe{let f:unsafe extern "system" fn(*mut c_void)->u32=std::mem::transmute(self.slot(2));f(self.0);}}}}
impl ComPtr{unsafe fn slot(&self,index:usize)->*const c_void{*(*(self.0 as *const *const *const c_void)).add(index)}}
struct Apartment(bool);impl Drop for Apartment{fn drop(&mut self){if self.0{unsafe{CoUninitialize();}}}}
fn check_hr(hr:i32,action:&str)->Result<(),String>{if hr<0{Err(format!("{action} failed: {hr:#x}"))}else{Ok(())}}
fn create_startup(path:&Path)->Result<(),String>{unsafe{
 let hr=CoInitializeEx(null_mut(),2);check_hr(hr,"COM initialization")?;let _apt=Apartment(true);
 let suffix=[0xc0,0,0,0,0,0,0,0x46];let cls=Guid{a:0x00021401,b:0,c:0,d:suffix};let iid=Guid{a:0x000214f9,b:0,c:0,d:suffix};let persist=Guid{a:0x0000010b,b:0,c:0,d:suffix};
 let mut ptr=null_mut();check_hr(CoCreateInstance(&cls,null_mut(),1,&iid,&mut ptr),"Create ShellLink")?;if ptr.is_null(){return Err("ShellLink unavailable".into());}let link=ComPtr(ptr);
 let exe=exe_path()?;let parent=exe.parent().ok_or("Executable folder unavailable")?;
 type SetString=unsafe extern "system" fn(*mut c_void,*const u16)->i32;
 let set_path:SetString=std::mem::transmute(link.slot(20));check_hr(set_path(link.0,path_wide(&exe).as_ptr()),"Set shortcut target")?;
 let set_dir:SetString=std::mem::transmute(link.slot(9));check_hr(set_dir(link.0,path_wide(parent).as_ptr()),"Set working directory")?;
 let set_desc:SetString=std::mem::transmute(link.slot(7));check_hr(set_desc(link.0,wide("MacTraffic 1.0.1").as_ptr()),"Set description")?;
 let qi:unsafe extern "system" fn(*mut c_void,*const Guid,*mut *mut c_void)->i32=std::mem::transmute(link.slot(0));let mut p=null_mut();check_hr(qi(link.0,&persist,&mut p),"Query IPersistFile")?;if p.is_null(){return Err("IPersistFile unavailable".into());}let file=ComPtr(p);
 let save:unsafe extern "system" fn(*mut c_void,*const u16,i32)->i32=std::mem::transmute(file.slot(6));check_hr(save(file.0,path_wide(path).as_ptr(),1),"Save Startup shortcut")
}}
#[link(name="kernel32")] extern "system"{fn GetSystemDirectoryW(buffer:*mut u16,size:u32)->u32;}
fn open_text(path:&Path)->Result<(),String>{let mut buf=[0u16;260];let n=unsafe{GetSystemDirectoryW(buf.as_mut_ptr(),buf.len() as u32)} as usize;if n==0||n>=buf.len(){return Err("System directory unavailable".into());}use std::os::windows::ffi::OsStringExt;let notepad=PathBuf::from(std::ffi::OsString::from_wide(&buf[..n])).join("notepad.exe");std::process::Command::new(notepad).arg(path).spawn().map(|_|()).map_err(|e|e.to_string())}
pub(super) fn menu_action(cmd:u32)->Result<(),String>{match cmd{
 MENU_CONFIG=>open_text(&config_path()?),MENU_LOG=>{let p=log_path();if !p.exists(){return Err("Log does not exist; set log_level=normal or debug and restart".into());}open_text(&p)},
 MENU_STARTUP=>{let p=startup_path()?;if p.exists(){std::fs::remove_file(p).map_err(|e|e.to_string())?;log("startup_changed enabled=false");}else{create_startup(&p)?;log("startup_changed enabled=true");}Ok(())},_=>Ok(())}}
#[cfg(test)] mod tests{use super::*;
 #[test] fn default_profile(){let c=parse(DEFAULT_INI).unwrap();assert!(c.browser_spacer&&c.office_exact_left&&c.alt_keys&&!c.native_maximize&&!c.start_paused);assert_eq!(c.level,LogLevel::Normal);}
 #[test] fn ini_bom_and_exclude(){let c=parse("\u{feff}[MacTraffic]\nexclude= BRAVE.EXE, systemsettings.exe, brave.exe\nstart_paused=yes").unwrap();assert!(c.start_paused);assert_eq!(c.excluded.len(),2);assert!(excluded(&c,"brave.exe",App::BrowserProbe,"Chrome_WidgetWin_1"));assert!(excluded(&c,"applicationframehost.exe",App::GenericProbe,"ApplicationFrameWindow"));assert!(!excluded(&c,"applicationframehost.exe",App::Other,"ApplicationFrameWindow"));}
 #[test] fn invalid_ini_fails_closed(){for x in ["exclude=C:\\x.exe","exclude=*.exe","browser_spacer=maybe","unexpected=1","garbage","log_level=trace"]{assert!(parse(x).is_err(),"{x}");}}
 #[test] fn log_policy(){assert!(normal_event("target_error hwnd=1"));assert!(normal_event("engine_exit_cleanup sessions=3"));assert!(!normal_event("generic_centered_verified hwnd=1"));assert!(!normal_event("browser_spacer_probe hwnd=1"));}
 #[test] fn notify_abi(){if size_of::<usize>()==8{assert_eq!(size_of::<NotifyData>(),976);}}
}
