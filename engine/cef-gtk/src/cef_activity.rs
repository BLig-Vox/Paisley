//! Native-only CEF process messages probe every frame. No page-facing bridge.
use cef::*;
use std::{collections::HashMap, time::Instant};
const PROBE: &str = "paisley.native.activity.request.v1";
const REPLY: &str = "paisley.native.activity.reply.v1";
const SCRIPT: &str = r#"(()=>{try {
 if(document.readyState!=='complete'||!navigator.userActivation||navigator.userActivation.hasBeenActive)return true;
 if(document.querySelector('[contenteditable]:not([contenteditable="false"])'))return true;
 for(const e of document.querySelectorAll('input,textarea,select')) {
  if(e.tagName==='SELECT'){for(const o of e.options)if(o.selected!==o.defaultSelected)return true;}
  else if(e.type==='checkbox'||e.type==='radio'){if(e.checked!==e.defaultChecked)return true;}
  else if(e.value!==e.defaultValue)return true;
 }
 for(const m of document.querySelectorAll('audio,video'))if(!m.paused&&!m.ended)return true;
 return false;
 }catch(_){return true;}})()"#;
wrap_render_process_handler! {
 pub struct RendererProbe { strict:std::sync::Arc<std::sync::Mutex<HashMap<i32,bool>>> }
 impl RenderProcessHandler {
  fn on_browser_created(&self,browser:Option<&mut Browser>,extra:Option<&mut DictionaryValue>){if let Some(browser)=browser{let strict=extra.map(|e|if e.has_key(Some(&"paisley.strict".into()))!=0{e.bool(Some(&"paisley.strict".into()))!=0}else{true}).unwrap_or(true);self.strict.lock().unwrap().insert(browser.identifier(),strict);}}
  fn on_browser_destroyed(&self,browser:Option<&mut Browser>){if let Some(browser)=browser{self.strict.lock().unwrap().remove(&browser.identifier());}}
  fn on_context_created(&self,browser:Option<&mut Browser>,_frame:Option<&mut Frame>,context:Option<&mut V8Context>){
   let strict=browser.as_ref().map(|b|self.strict.lock().unwrap().get(&b.identifier()).copied().unwrap_or(true)).unwrap_or(true);
   if strict{
    if let Some(context)=context{if context.enter()!=0{let mut value=None;let mut error=None;
     let result=context.eval(Some(&"for(const key of ['RTCPeerConnection','webkitRTCPeerConnection']){Object.defineProperty(globalThis,key,{value:undefined,writable:false,configurable:false})}".into()),None,1,Some(&mut value),Some(&mut error));if result!=1{eprintln!("CEF strict privacy bootstrap failed");}context.exit();
    }}
   }
  }

  fn on_process_message_received(&self,_browser:Option<&mut Browser>,frame:Option<&mut Frame>,source:ProcessId,message:Option<&mut ProcessMessage>)->i32{
   let Some(message)=message else{return 0;};
   if source!=ProcessId::BROWSER||CefString::from(&message.name()).to_string()!=PROBE{return 0;}
   let Some(frame)=frame else{return 1;};let Some(args)=message.argument_list()else{return 1;};
   let mut protected=true;
   if let Some(context)=frame.v8_context(){if context.enter()!=0{let mut value=None;let mut error=None;
    if context.eval(Some(&SCRIPT.into()),None,1,Some(&mut value),Some(&mut error))!=0{if let Some(value)=value{if value.is_bool()!=0{protected=value.bool_value()!=0;}}}
    context.exit();
   }}
   if let Some(mut reply)=process_message_create(Some(&REPLY.into())){if let Some(values)=reply.argument_list(){values.set_int(0,args.int(0));values.set_bool(1,protected as i32);frame.send_process_message(ProcessId::BROWSER,Some(&mut reply));}}
   1
  }
 }
}
#[derive(Default)]
pub struct Activity {
    round: i32,
    pub at: Option<Instant>,
    frames: HashMap<String, Option<bool>>,
    pub capture: bool,
    pub interacted: bool,
}
impl Activity {
    pub fn begin(&mut self, browser: &Browser) {
        self.round = self.round.wrapping_add(1);
        self.at = Some(Instant::now());
        self.frames.clear();
        if browser.frame_count() > 256 {
            return;
        }
        let mut ids = CefStringList::new();
        browser.frame_identifiers(Some(&mut ids));
        for id in ids.into_iter().take(256) {
            self.frames.insert(id.clone(), None);
            if let Some(frame) = browser.frame_by_identifier(Some(&id.as_str().into())) {
                if let Some(mut message) = process_message_create(Some(&PROBE.into())) {
                    if let Some(args) = message.argument_list() {
                        args.set_int(0, self.round);
                        frame.send_process_message(ProcessId::RENDERER, Some(&mut message));
                    }
                }
            }
        }
    }
    pub fn receive(
        &mut self,
        frame: Option<&mut Frame>,
        source: ProcessId,
        message: Option<&mut ProcessMessage>,
    ) -> i32 {
        let Some(message) = message else {
            return 0;
        };
        if source != ProcessId::RENDERER || CefString::from(&message.name()).to_string() != REPLY {
            return 0;
        }
        if let (Some(frame), Some(args)) = (frame, message.argument_list()) {
            if args.size() == 2 && args.int(0) == self.round {
                let id = CefString::from(&frame.identifier()).to_string();
                if let Some(value) = self.frames.get_mut(&id) {
                    *value = Some(args.bool(1) != 0);
                }
            }
        }
        1
    }
    pub fn protected(&self) -> bool {
        self.capture
            || self.interacted
            || self.frames.is_empty()
            || self.frames.values().any(|state| *state != Some(false))
    }
}
