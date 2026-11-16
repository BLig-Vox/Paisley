//! Bounded profile operations on the CEF UI thread.
use super::{Event, Events, ipc};
use cef::*;
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
wrap_dev_tools_message_observer! {
 struct ClearObserver{events:Events,request:u64,message:i32,done:Arc<AtomicBool>}
 impl DevToolsMessageObserver{
  fn on_dev_tools_method_result(&self,_browser:Option<&mut Browser>,message:i32,success:i32,_result:Option<&[u8]>){
   if message==self.message&&!self.done.swap(true,Ordering::AcqRel){self.events.send(Event::OperationResult(ipc::OperationResult{request_id:self.request,success:success!=0,message:if success!=0{"Site data cleared. Reload the site; open tabs may recreate data."}else{"The browser engine could not clear this site’s data."}.into()}));}
  }
 }
}
struct Pending {
    _registration: Registration,
    done: Arc<AtomicBool>,
    at: Instant,
    request: u64,
}
pub struct Services {
    pending: HashMap<i32, Pending>,
    next: i32,
    events: Events,
}
impl Services {
    pub fn new(events: Events) -> Self {
        Self {
            pending: HashMap::new(),
            next: 1_000_000_000,
            events,
        }
    }
    pub fn clear(&mut self, host: &BrowserHost, request: &ipc::ClearSiteData) {
        if self.pending.len() >= 8 || self.next == i32::MAX {
            self.failure(request.request_id, "Site data service is busy. Try again.");
            return;
        }
        let Some(origin) = url::Url::parse(&request.origin).ok().filter(|u| {
            matches!(u.scheme(), "http" | "https")
                && u.origin().ascii_serialization() == request.origin
        }) else {
            self.failure(request.request_id, "A website origin is required.");
            return;
        };
        let id = self.next;
        self.next += 1;
        let done = Arc::new(AtomicBool::new(false));
        let Some(registration) = host.add_dev_tools_message_observer(Some(
            &mut ClearObserver::new(self.events.clone(), request.request_id, id, done.clone()),
        )) else {
            self.failure(request.request_id, "Site data service is unavailable.");
            return;
        };
        let Some(mut params) = dictionary_value_create() else {
            self.failure(request.request_id, "Site data service is unavailable.");
            return;
        };
        params.set_string(
            Some(&"origin".into()),
            Some(&origin.origin().ascii_serialization().as_str().into()),
        );
        params.set_string(Some(&"storageTypes".into()), Some(&"all".into()));
        self.pending.insert(
            id,
            Pending {
                _registration: registration,
                done,
                at: Instant::now(),
                request: request.request_id,
            },
        );
        if host.execute_dev_tools_method(
            id,
            Some(&"Storage.clearDataForOrigin".into()),
            Some(&mut params),
        ) == 0
        {
            self.pending.remove(&id);
            self.failure(
                request.request_id,
                "The browser engine rejected the site data request.",
            );
        }
    }
    pub fn failure(&self, request: u64, message: &str) {
        self.events
            .send(Event::OperationResult(ipc::OperationResult {
                request_id: request,
                success: false,
                message: message.into(),
            }));
    }
    pub fn reap(&mut self) {
        let mut timed_out = Vec::new();
        self.pending.retain(|_, p| {
            if p.done.load(Ordering::Acquire) {
                false
            } else if p.at.elapsed() > Duration::from_secs(15) {
                timed_out.push(p.request);
                false
            } else {
                true
            }
        });
        for request in timed_out {
            self.failure(request, "Site data clearing timed out. Try again.");
        }
    }
}
