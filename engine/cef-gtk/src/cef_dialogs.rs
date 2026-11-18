//! One bounded, asynchronous script dialog per native page.
use super::{Event, ViewState, ipc, strings};
use cef::*;
#[derive(Default)]
pub struct Dialogs {
    next: u64,
    pending: Option<(u64, JsdialogCallback)>,
    auth: Option<(u64, AuthCallback)>,
    file: Option<(u64, FileDialogCallback)>,
    screen: Option<(u64, MediaAccessCallback)>,
}
pub fn cancel_screen(state: &ViewState) {
    let pending = state.dialogs.lock().unwrap().screen.take();
    if let Some((id, callback)) = pending {
        state.events.send(Event::CancelDialog(id));
        callback.cancel();
    }
}
wrap_permission_handler! {
 pub struct MediaHandler {state:ViewState}
 impl PermissionHandler {
  fn on_request_media_access_permission(&self,_browser:Option<&mut Browser>,frame:Option<&mut Frame>,origin:Option<&CefString>,permissions:u32,callback:Option<&mut MediaAccessCallback>)->i32 {
   let Some(callback)=callback else{return 0;};
   let origin=strings::borrowed(origin,2048);
   let allowed=permissions==MediaAccessPermissionTypes::DESKTOP_VIDEO_CAPTURE.get_raw()
    && frame.as_ref().is_some_and(|f|f.is_main()!=0)
    && url::Url::parse(&origin).is_ok_and(|url| matches!(url.scheme(),"https"|"http") && url.host_str().is_some());
   if !allowed {callback.cancel();return 1;}
   let id={let mut dialogs=self.state.dialogs.lock().unwrap();
    if dialogs.screen.is_some()||dialogs.next==u64::MAX {drop(dialogs);callback.cancel();return 1;}
    dialogs.next+=1;let id=dialogs.next;dialogs.screen=Some((id,callback.clone()));id};
   self.state.events.send(Event::PermissionRequest(ipc::PermissionRequest{request_id:id,feature_name:format!("sharing your entire desktop (all visible screens) with {origin}. Other open windows may be visible.")}));
   1
  }
 }
}
fn begin(
    state: &ViewState,
    kind: i32,
    message: String,
    default: String,
    callback: &JsdialogCallback,
) -> bool {
    let request_id = {
        let mut dialogs = state.dialogs.lock().unwrap();
        if dialogs.pending.is_some() || dialogs.next == u64::MAX {
            return false;
        }
        dialogs.next += 1;
        let id = dialogs.next;
        dialogs.pending = Some((id, callback.clone()));
        id
    };
    state
        .events
        .send(Event::ScriptDialogRequest(ipc::ScriptDialogRequest {
            request_id,
            kind,
            message,
            default_value: default,
        }));
    true
}
pub fn auth(state: &ViewState, origin: String, proxy: bool, callback: &AuthCallback) -> bool {
    let request_id = {
        let mut dialogs = state.dialogs.lock().unwrap();
        if dialogs.auth.is_some() || dialogs.next == u64::MAX {
            return false;
        }
        dialogs.next += 1;
        let id = dialogs.next;
        dialogs.auth = Some((id, callback.clone()));
        id
    };
    state.events.send(Event::AuthRequest(ipc::AuthRequest {
        request_id,
        url: origin,
        for_proxy: proxy,
    }));
    true
}
pub fn cancel_auth(state: &ViewState) {
    let auth = state.dialogs.lock().unwrap().auth.take();
    if let Some((id, callback)) = auth {
        state.events.send(Event::CancelDialog(id));
        callback.cancel();
    }
}
pub fn cancel(state: &ViewState) {
    cancel_screen(state);
    cancel_auth(state);
    cancel_file(state);
    cancel_script(state);
}
fn cancel_script(state: &ViewState) {
    let pending = state.dialogs.lock().unwrap().pending.take();
    if let Some((id, callback)) = pending {
        state.events.send(Event::CancelDialog(id));
        callback.cont(0, None);
    }
}
pub fn cancel_file(state: &ViewState) {
    let pending = state.dialogs.lock().unwrap().file.take();
    if let Some((id, callback)) = pending {
        state.events.send(Event::CancelDialog(id));
        callback.cancel();
    }
}
pub fn response(state: &ViewState, response: &ipc::ServoResponse) {
    if let Some(ipc::servo_response::Payload::Permission(answer)) = &response.payload {
        let callback = {
            let mut dialogs = state.dialogs.lock().unwrap();
            if dialogs.screen.as_ref().is_some_and(|(id, _)| *id == response.request_id) {
                dialogs.screen.take().map(|(_, callback)| callback)
            } else { None }
        };
        if let Some(callback) = callback {
            if answer.allow { callback.cont(MediaAccessPermissionTypes::DESKTOP_VIDEO_CAPTURE.get_raw()); }
            else { callback.cancel(); }
        }
        return;
    }
    if let Some(ipc::servo_response::Payload::FileChooser(answer)) = &response.payload {
        let callback = {
            let mut dialogs = state.dialogs.lock().unwrap();
            if dialogs
                .file
                .as_ref()
                .is_some_and(|(id, _)| *id == response.request_id)
            {
                dialogs.file.take().map(|(_, callback)| callback)
            } else {
                None
            }
        };
        if let Some(callback) = callback {
            if answer.paths.is_empty()
                || answer.paths.len() > 16
                || answer
                    .paths
                    .iter()
                    .any(|p| p.len() > 4096 || !std::path::Path::new(p).is_absolute())
            {
                callback.cancel();
            } else {
                let mut paths = CefStringList::new();
                for path in &answer.paths {
                    paths.append(path);
                }
                callback.cont(Some(&mut paths));
            }
        }
        return;
    }
    if let Some(ipc::servo_response::Payload::Auth(answer)) = &response.payload {
        let callback = {
            let mut dialogs = state.dialogs.lock().unwrap();
            if dialogs
                .auth
                .as_ref()
                .is_some_and(|(id, _)| *id == response.request_id)
            {
                dialogs.auth.take().map(|(_, callback)| callback)
            } else {
                None
            }
        };
        if let Some(callback) = callback {
            if answer.confirmed && answer.username.len() <= 4096 && answer.password.len() <= 4096 {
                callback.cont(
                    Some(&answer.username.as_str().into()),
                    Some(&answer.password.as_str().into()),
                );
            } else {
                callback.cancel();
            }
        }
        return;
    }

    let Some(ipc::servo_response::Payload::ScriptDialog(answer)) = &response.payload else {
        return;
    };
    let callback = {
        let mut dialogs = state.dialogs.lock().unwrap();
        if dialogs
            .pending
            .as_ref()
            .is_some_and(|(id, _)| *id == response.request_id)
        {
            dialogs.pending.take().map(|(_, callback)| callback)
        } else {
            None
        }
    };
    if let Some(callback) = callback {
        let text: String = answer.value.chars().take(4096).collect();
        callback.cont(answer.confirmed as i32, Some(&text.as_str().into()));
    }
}
wrap_jsdialog_handler! {
 pub struct Handler { state:ViewState }
 impl JsdialogHandler {
  fn on_jsdialog(&self,_browser:Option<&mut Browser>,_origin:Option<&CefString>,kind:JsdialogType,message:Option<&CefString>,default:Option<&CefString>,callback:Option<&mut JsdialogCallback>,suppress:Option<&mut i32>)->i32 {
   let kind=if kind==JsdialogType::ALERT {0}else if kind==JsdialogType::CONFIRM {1}else{2};
   if let Some(callback)=callback {if begin(&self.state,kind,strings::borrowed(message,4096),strings::borrowed(default,4096),callback){return 1;}}
   if let Some(suppress)=suppress{*suppress=1;}0
  }
  fn on_before_unload_dialog(&self,_browser:Option<&mut Browser>,message:Option<&CefString>,_reload:i32,callback:Option<&mut JsdialogCallback>)->i32 {
   if let Some(callback)=callback {if !begin(&self.state,1,strings::borrowed(message,4096),String::new(),callback){callback.cont(0,None);}}1
  }
  fn on_reset_dialog_state(&self,_browser:Option<&mut Browser>){cancel_script(&self.state);}
 }
}

wrap_dialog_handler! {
 pub struct FileHandler {state:ViewState}
 impl DialogHandler {
  fn on_file_dialog(&self,_browser:Option<&mut Browser>,mode:FileDialogMode,_title:Option<&CefString>,default:Option<&CefString>,filters:Option<&mut CefStringList>,_extensions:Option<&mut CefStringList>,_descriptions:Option<&mut CefStringList>,callback:Option<&mut FileDialogCallback>)->i32 {
   let Some(callback)=callback else{return 0;};
   let id={let mut dialogs=self.state.dialogs.lock().unwrap();
    if dialogs.file.is_some()||dialogs.next==u64::MAX {drop(dialogs);callback.cancel();return 1;}
    dialogs.next+=1;let id=dialogs.next;dialogs.file=Some((id,callback.clone()));id};
   let mut patterns=Vec::new();
   if let Some(filters)=filters {
    let raw:*mut sys::_cef_string_list_t=filters.into();
    let count=unsafe{sys::cef_string_list_size(raw)};
    if count<=32 {for index in 0..count {
     let mut value:sys::_cef_string_utf16_t=unsafe{std::mem::zeroed()};
     if unsafe{sys::cef_string_list_value(raw,index,&mut value)}>0 {
      let text=CefString::from(std::ptr::from_ref(&value));
      patterns.push(strings::borrowed(Some(&text),128));drop(text);
      unsafe{sys::cef_string_utf16_clear(&mut value)};
     }
    }}
   }
   self.state.events.send(Event::FileChooserRequest(ipc::FileChooserRequest{request_id:id,allow_multiple:mode==FileDialogMode::OPEN_MULTIPLE,filter_patterns:patterns,mode:mode.get_raw(),default_name:strings::borrowed(default,255)}));1
  }
 }
}
