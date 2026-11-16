//! Session notification service; every call is asynchronous and bounded.
use gtk::{gio, glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
};

pub struct DesktopNotifications {
    proxy: RefCell<Option<gio::DBusProxy>>,
    enabled: Cell<bool>,
    active: RefCell<HashMap<u64, u32>>,
    pending: RefCell<HashSet<u64>>,
    cancelled: RefCell<HashSet<u64>>,
    status: RefCell<String>,
}
impl DesktopNotifications {
    pub fn new(enabled: bool, activate: impl Fn(u64) + 'static) -> Rc<Self> {
        let this = Rc::new(Self {
            proxy: RefCell::new(None),
            enabled: Cell::new(enabled),
            active: RefCell::new(HashMap::new()),
            pending: RefCell::new(HashSet::new()),
            cancelled: RefCell::new(HashSet::new()),
            status: RefCell::new("Connecting to the desktop notification service…".into()),
        });
        let weak = Rc::downgrade(&this);
        glib::MainContext::default().spawn_local(async move {
            let result=gio::DBusProxy::for_bus_future(gio::BusType::Session,gio::DBusProxyFlags::NONE,None,"org.freedesktop.Notifications","/org/freedesktop/Notifications","org.freedesktop.Notifications").await;
            let Some(this)=weak.upgrade()else{return;};
            match result {
                Ok(proxy)=>{
                    let caps=proxy.call_future("GetCapabilities",None,gio::DBusCallFlags::NONE,2000).await;
                    let _ = caps;
                    let weak=Rc::downgrade(&this);
                    proxy.connect_local("g-signal",false,move |args|{
                        let Some(this)=weak.upgrade()else{return None;};
                        let name=args[2].get::<String>().ok()?;
                        let params=args[3].get::<glib::Variant>().ok()?;
                        if name=="ActionInvoked" {
                            if let Some((os_id,key))=params.get::<(u32,String)>() && key=="default" && this.enabled.get(){
                                let event=this.active.borrow().iter().find(|(_,id)|**id==os_id).map(|(event,_)|*event);
                                if let Some(event)=event{activate(event);this.close(event);}
                            }
                        }else if name=="NotificationClosed" && let Some((os_id,_))=params.get::<(u32,u32)>(){this.active.borrow_mut().retain(|_,id|*id!=os_id);}
                        None
                    });
                    let weak=Rc::downgrade(&this);
                    proxy.connect_notify_local(Some("g-name-owner"),move |_,_|{if let Some(this)=weak.upgrade(){this.active.borrow_mut().clear();}});
                    *this.status.borrow_mut()=if proxy.name_owner().is_some(){"Uses your desktop’s notification settings and Do Not Disturb."}else{"Desktop notification service is unavailable; in-app alerts still work."}.into();
                    *this.proxy.borrow_mut()=Some(proxy);
                }
                Err(_)=>*this.status.borrow_mut()="Desktop notifications are unavailable in this session; in-app alerts still work.".into(),
            }
        });
        this
    }
    pub fn status(&self) -> String {
        if self
            .proxy
            .borrow()
            .as_ref()
            .is_some_and(|p| p.name_owner().is_some())
        {
            "Uses your desktop’s notification settings and Do Not Disturb.".into()
        } else {
            self.status.borrow().clone()
        }
    }
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.set(enabled);
        if !enabled {
            self.clear();
        }
    }
    pub fn send(self: &Rc<Self>, event: u64, title: &str, body: &str) {
        if !self.enabled.get() || self.pending.borrow().len() >= 4 {
            return;
        }
        let Some(proxy) = self.proxy.borrow().clone() else {
            return;
        };
        let body = glib::markup_escape_text(body).to_string();
        let hints = HashMap::from([
            ("desktop-entry", "site.blig.Paisley".to_variant()),
            ("urgency", 1u8.to_variant()),
        ]);
        let params = (
            "Paisley",
            0u32,
            crate::desktop_install::ICON_NAME,
            title,
            body,
            vec!["default", "Open Paisley"],
            hints,
            -1i32,
        )
            .to_variant();
        self.pending.borrow_mut().insert(event);
        let weak = Rc::downgrade(self);
        glib::MainContext::default().spawn_local(async move {
            let result=proxy.call_future("Notify",Some(&params),gio::DBusCallFlags::NONE,2000).await;
            let Some(this)=weak.upgrade()else{
                if let Ok(value)=result && let Some((id,))=value.get::<(u32,)>() {Self::close_os(&proxy,id);}
                return;
            };
            this.pending.borrow_mut().remove(&event);
            let cancelled=this.cancelled.borrow_mut().remove(&event);
            match result {
                Ok(value)=>if let Some((os_id,))=value.get::<(u32,)>() {
                    if !this.enabled.get() || cancelled {Self::close_os(&proxy,os_id);}else{
                        if this.active.borrow().len()>=50 {let oldest=this.active.borrow().keys().min().copied();if let Some(oldest)=oldest{this.close(oldest);}}
                        if let Some(old)=this.active.borrow_mut().insert(event,os_id){Self::close_os(&proxy,old);}
                    }
                },
                Err(_)=>*this.status.borrow_mut()="Desktop delivery failed; check your session notification service. In-app alerts still work.".into(),
            }
        });
    }
    fn close_os(proxy: &gio::DBusProxy, id: u32) {
        proxy.call(
            "CloseNotification",
            Some(&(id,).to_variant()),
            gio::DBusCallFlags::NONE,
            1000,
            None::<&gio::Cancellable>,
            |_| {},
        );
    }
    pub fn close(&self, event: u64) {
        if self.pending.borrow().contains(&event) {
            self.cancelled.borrow_mut().insert(event);
        }
        if let Some(id) = self.active.borrow_mut().remove(&event)
            && let Some(proxy) = self.proxy.borrow().as_ref()
        {
            Self::close_os(proxy, id);
        }
    }
    pub fn clear(&self) {
        let active = self.active.replace(HashMap::new());
        if let Some(proxy) = self.proxy.borrow().as_ref() {
            for id in active.values() {
                Self::close_os(proxy, *id);
            }
        }
        self.cancelled
            .borrow_mut()
            .extend(self.pending.borrow().iter().copied());
    }
}
impl Drop for DesktopNotifications {
    fn drop(&mut self) {
        self.clear();
    }
}
