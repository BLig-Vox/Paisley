//! CEF owns the network transfer; GTK owns destination choice and publication.
use super::{Event, Events, ipc};
use cef::*;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
#[derive(Default)]
pub struct Transfers {
    pending: HashMap<u32, BeforeDownloadCallback>,
    owners: HashMap<u32, i32>,
    controls: HashMap<u32, DownloadItemCallback>,
    emitted: HashMap<u32, Instant>,
    methods: HashMap<String, String>,
}
wrap_download_handler! {
 pub struct Handler { transfers:Arc<Mutex<Transfers>>, events:Events }
 impl DownloadHandler {
  fn can_download(&self,_browser:Option<&mut Browser>,url:Option<&CefString>,method:Option<&CefString>)->i32{
   let Some(url)=url.map(|v|super::strings::borrowed(Some(v),8192))else{return 0;};
   if !url::Url::parse(&url).is_ok_and(|u|matches!(u.scheme(),"http"|"https"|"blob"|"data")){return 0;}
   let mut state=self.transfers.lock().unwrap();if state.pending.len()+state.controls.len()>=16{return 0;}
   if state.methods.len()>=32{state.methods.clear();}state.methods.insert(url,method.map(|v|super::strings::borrowed(Some(v),32)).unwrap_or_default());1
  }
  fn on_before_download(&self,browser:Option<&mut Browser>,item:Option<&mut DownloadItem>,name:Option<&CefString>,callback:Option<&mut BeforeDownloadCallback>)->i32{
   let(Some(item),Some(callback))=(item,callback)else{return 0;};let mut info=metadata(item);
   if let Some(name)=name {info.name=super::strings::borrowed(Some(name),255);}
   {let mut state=self.transfers.lock().unwrap();if state.pending.len()>=16{return 0;}state.pending.insert(item.id(),callback.clone());if let Some(browser)=browser{state.owners.insert(item.id(),browser.identifier());}info.method=state.methods.remove(&info.url).unwrap_or_default();}
   self.events.send(Event::DownloadStarted(info));1
  }
  fn on_download_updated(&self,_browser:Option<&mut Browser>,item:Option<&mut DownloadItem>,callback:Option<&mut DownloadItemCallback>){
   let Some(item)=item else{return;};let info=metadata(item);let terminal=info.complete||info.cancelled||info.interrupted;
   if cfg!(feature="test-support")&&std::env::var_os("PAISLEY_ENGINE_DIAGNOSTICS").is_some(){eprintln!("TRANSFER {} bytes={} paused={} complete={} cancelled={} interrupted={} reason={}",info.id,info.received,info.paused,info.complete,info.cancelled,info.interrupted,info.reason);}
   let send={let mut state=self.transfers.lock().unwrap();if let Some(callback)=callback{state.controls.insert(item.id(),callback.clone());}
    let send=terminal||info.paused||state.emitted.get(&item.id()).is_none_or(|at|at.elapsed()>=Duration::from_millis(250));
    if terminal{state.pending.remove(&item.id());state.owners.remove(&item.id());state.controls.remove(&item.id());state.emitted.remove(&item.id());}else if send{state.emitted.insert(item.id(),Instant::now());}send};
   if send{self.events.send(Event::DownloadProgress(info));}
  }
 }
}
fn metadata(item: &DownloadItem) -> ipc::NativeDownload {
    ipc::NativeDownload {
        id: item.id(),
        url: super::strings::owned(&item.original_url(), 8192),
        name: super::strings::owned(&item.suggested_file_name(), 255),
        path: super::strings::owned(&item.full_path(), 4096),
        received: item.received_bytes().max(0) as u64,
        total: item.total_bytes(),
        speed: item.current_speed().max(0) as u64,
        complete: item.is_complete() != 0,
        cancelled: item.is_canceled() != 0,
        paused: item.is_paused() != 0,
        interrupted: item.is_interrupted() != 0,
        reason: item.interrupt_reason().get_raw() as i32,
        method: String::new(),
    }
}
pub fn dispatch(state: &Arc<Mutex<Transfers>>, action: &ipc::servo_action::Action) -> bool {
    match action {
        ipc::servo_action::Action::DownloadDestination(choice) => {
            let callback = state.lock().unwrap().pending.remove(&choice.id);
            if let Some(callback) = callback {
                if !choice.path.is_empty() && std::path::Path::new(&choice.path).is_absolute() {
                    callback.cont(Some(&choice.path.as_str().into()), 0);
                }
            }
            true
        }
        ipc::servo_action::Action::DownloadControl(control) => {
            let callback = state.lock().unwrap().controls.get(&control.id).cloned();
            if let Some(callback) = callback {
                match control.operation.as_str() {
                    "pause" => callback.pause(),
                    "resume" => callback.resume(),
                    "cancel" => callback.cancel(),
                    _ => {}
                }
            }
            true
        }
        _ => false,
    }
}
pub fn cancel_all(state: &Arc<Mutex<Transfers>>) {
    let callbacks = {
        let mut state = state.lock().unwrap();
        state.pending.clear();
        state.controls.drain().map(|(_, c)| c).collect::<Vec<_>>()
    };
    for callback in callbacks {
        callback.cancel();
    }
}

impl Transfers {
    pub fn owns_browser(&self, id: i32) -> bool {
        self.owners.values().any(|owner| *owner == id)
    }
}
