//! Test-only process DATA_DIR scope. Callers retain it through all filesystem assertions.
use std::{ffi::OsString,path::PathBuf,sync::{Mutex,MutexGuard}};
static SELECTED:Mutex<()>=Mutex::new(());
pub(crate) struct FixtureRoot { previous:Option<OsString>,path:PathBuf,_selected:MutexGuard<'static,()> }
impl FixtureRoot {
 pub(crate) fn private()->Self {
  let selected=SELECTED.lock().unwrap_or_else(|e|e.into_inner());
  let path=std::env::temp_dir().join(format!("starlane-private-test-{}",uuid::Uuid::new_v4().simple()));
  std::fs::create_dir(&path).expect("new disposable test directory");
  #[cfg(unix)] {use std::os::unix::fs::PermissionsExt;std::fs::set_permissions(&path,std::fs::Permissions::from_mode(0o700)).expect("private test directory");}
  let previous=std::env::var_os("DATA_DIR");std::env::set_var("DATA_DIR",&path);
  Self{previous,path,_selected:selected}
 }
}
impl Drop for FixtureRoot {
 fn drop(&mut self){match &self.previous {Some(old)=>std::env::set_var("DATA_DIR",old),None=>std::env::remove_var("DATA_DIR")};let _=std::fs::remove_dir_all(&self.path);}
}
