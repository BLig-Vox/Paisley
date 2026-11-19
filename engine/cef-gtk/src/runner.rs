// CEF backend for the bounded Paisley GTK transport. Wire names stay compatible.
use crate::proto_ipc::{self as ipc, servo_action::Action, servo_event::Event};
use cef::{args::Args, *};
use prost::Message;
#[path = "cef_dialogs.rs"]
mod dialogs;
#[path = "cef_inspector.rs"]
mod inspector;
#[path = "cef_services.rs"]
mod services;
#[path = "cef_keyboard.rs"]
mod keyboard;
#[path = "cef_strings.rs"]
mod strings;
use std::{
    collections::HashMap,
    io::{Read, Write},
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};
#[path = "cef_activity.rs"]
mod activity;
#[path = "downloads.rs"]
mod downloads;
pub const RUNNER_ARG: &str = "--paisley-cef-runner";
#[derive(Clone)]
struct Events {
    id: u64,
    output: Arc<Mutex<std::io::Stdout>>,
}
impl Events {
    fn send(&self, event: Event) {
        let packet = ipc::ServoEvent {
            webview_id: self.id,
            event: Some(event),
        }
        .encode_to_vec();
        if packet.len() > paisley_engine_transport::MAX_PACKET_BYTES {
            return;
        }
        if let Ok(mut out) = self.output.lock() {
            if out
                .write_all(&(packet.len() as u32).to_le_bytes())
                .and_then(|_| out.write_all(&packet))
                .and_then(|_| out.flush())
                .is_err()
            {
                std::process::exit(0);
            }
        }
    }
}
struct CreatedPopup {
    browser: Browser,
    state: ViewState,
    opener: Events,
}

#[derive(Clone)]
struct ViewState {
    events: Events,
    popup_sender: mpsc::Sender<CreatedPopup>,
    opener: Option<Events>,
    size: Arc<Mutex<(i32, i32)>>,
    closed: Arc<std::sync::atomic::AtomicBool>,
    blocking: Arc<Mutex<ipc::Blocking>>,
    top: Arc<Mutex<String>>,
    activity: Arc<Mutex<activity::Activity>>,
    transfers: Arc<Mutex<downloads::Transfers>>,
    strict: Arc<std::sync::atomic::AtomicBool>,
    dialogs: Arc<Mutex<dialogs::Dialogs>>,
    popup: Arc<Mutex<ipc::PopupState>>,
    drag: Arc<Mutex<Option<LocalDrag>>>,
}
struct LocalDrag {
    data: DragData,
    allowed: DragOperationsMask,
    operation: DragOperationsMask,
    entered: bool,
}
wrap_render_handler! {
    struct Paint { state:ViewState }
    impl RenderHandler {
        fn start_dragging(
            &self,
            _browser: Option<&mut Browser>,
            data: Option<&mut DragData>,
            allowed: DragOperationsMask,
            _x: i32,
            _y: i32,
        ) -> i32 {
            let Some(data) = data.and_then(|data| ImplDragData::clone(data)) else {
                return 0;
            };
            data.reset_file_contents();
            *self.state.drag.lock().unwrap() = Some(LocalDrag {
                data,
                allowed,
                operation: DragOperationsMask::default(),
                entered: false,
            });
            1
        }

        fn update_drag_cursor(&self, _browser: Option<&mut Browser>, operation: DragOperationsMask) {
            if let Some(drag) = self.state.drag.lock().unwrap().as_mut() {
                drag.operation = operation;
            }
        }
        fn on_popup_show(&self,_browser:Option<&mut Browser>,show:i32){
            if cfg!(feature="test-support") && std::env::var_os("PAISLEY_NATIVE_POPUP").is_some(){eprintln!("POPUP native show {show}");}
            let state={let mut popup=self.state.popup.lock().unwrap();popup.serial=popup.serial.wrapping_add(1);popup.visible=show!=0;popup.clone()};
            self.state.events.send(Event::PopupState(state));
        }
        fn on_popup_size(&self,_browser:Option<&mut Browser>,rect:Option<&Rect>){
            if cfg!(feature="test-support") && std::env::var_os("PAISLEY_NATIVE_POPUP").is_some(){eprintln!("POPUP native size {rect:?}");}
            if let Some(rect)=rect {let state={let mut popup=self.state.popup.lock().unwrap();popup.x=rect.x;popup.y=rect.y;popup.clone()};self.state.events.send(Event::PopupState(state));}
        }

        fn view_rect(&self,_browser:Option<&mut Browser>,rect:Option<&mut Rect>){if let Some(rect)=rect{let(width,height)=*self.state.size.lock().unwrap();*rect=Rect{x:0,y:0,width,height};}}
        fn on_paint(&self,_browser:Option<&mut Browser>,kind:PaintElementType,_dirty:Option<&[Rect]>,buffer:*const u8,width:i32,height:i32){
            if (kind!=PaintElementType::VIEW&&kind!=PaintElementType::POPUP)||buffer.is_null()||width<=0||height<=0{return;}
            let Some(len)=(width as usize).checked_mul(height as usize).and_then(|n|n.checked_mul(4))else{return;};
            if !paisley_engine_transport::valid_rgba(width as u32,height as u32,len){return;}
            let mut rgba=unsafe{std::slice::from_raw_parts(buffer,len)}.to_vec();
            for pixel in rgba.chunks_exact_mut(4){pixel.swap(0,2);}
            let frame=ipc::FrameReady{rgba_data:rgba,width:width as u32,height:height as u32};
            if kind==PaintElementType::POPUP {if cfg!(feature="test-support") && std::env::var_os("PAISLEY_NATIVE_POPUP").is_some(){eprintln!("POPUP native paint {width}x{height}");}let state=self.state.popup.lock().unwrap().clone();if state.visible{self.state.events.send(Event::PopupFrame(ipc::PopupFrame{serial:state.serial,frame:Some(frame)}));}}
            else {self.state.events.send(Event::FrameReady(frame));}
        }
    }
}
wrap_display_handler! {
    struct Display { state:ViewState }
    impl DisplayHandler {
        fn on_media_access_change(&self,_browser:Option<&mut Browser>,video:i32,audio:i32){self.state.activity.lock().unwrap().capture=video!=0||audio!=0;}
        fn on_favicon_urlchange(&self,browser:Option<&mut Browser>,urls:Option<&mut CefStringList>){
            let (Some(browser),Some(urls))=(browser,urls)else{return;};
            let raw:*mut sys::_cef_string_list_t=(&mut *urls).into();
            if unsafe{sys::cef_string_list_size(raw)}>16{return;}
            let url=std::mem::take(urls).into_iter().find(|s|s.len()<=8192&&url::Url::parse(s).is_ok_and(|u|matches!(u.scheme(),"http"|"https"|"data")));
            if let(Some(url),Some(host))=(url,browser.host()){let document=self.state.top.lock().unwrap().clone();host.download_image(Some(&url.as_str().into()),1,64,0,Some(&mut FaviconImage::new(self.state.clone(),document)));}
        }
        fn on_title_change(&self,_browser:Option<&mut Browser>,title:Option<&CefString>){self.state.events.send(Event::TitleChanged(ipc::TitleChanged{title:strings::borrowed(title,1024)}));}
        fn on_address_change(&self,_browser:Option<&mut Browser>,frame:Option<&mut Frame>,url:Option<&CefString>){if frame.is_some_and(|f|f.is_main()!=0){let url=strings::borrowed(url,8192);*self.state.top.lock().unwrap()=url.clone();self.state.events.send(Event::UrlChanged(ipc::UrlChanged{url}));}}
    }
}
wrap_download_image_callback! {
    struct FaviconImage { state:ViewState, document:String }
    impl DownloadImageCallback {
        fn on_download_image_finished(&self,_url:Option<&CefString>,_status:i32,image:Option<&mut Image>){
            if *self.state.top.lock().unwrap()!=self.document{return;}
            let Some(image)=image else{return;};let(mut width,mut height)=(0,0);
            let Some(bitmap)=image.as_bitmap(1.0,ColorType::RGBA_8888,AlphaType::POSTMULTIPLIED,Some(&mut width),Some(&mut height))else{return;};
            if width<=0||height<=0||width>128||height>128{return;}
            let len=width as usize*height as usize*4;if bitmap.size()!=len{return;}
            let mut pixels=vec![0;len];if bitmap.data(Some(&mut pixels),0)!=len{return;}
            self.state.events.send(Event::FaviconChanged(ipc::FrameReady{rgba_data:pixels,width:width as u32,height:height as u32}));
        }
    }
}
wrap_load_handler! {
    struct Loading { state:ViewState }
    impl LoadHandler {
        fn on_loading_state_change(&self,browser:Option<&mut Browser>,loading:i32,back:i32,forward:i32){
            if loading!=0 {dialogs::cancel_screen(&self.state);}
            self.state.events.send(Event::HistoryChanged(ipc::HistoryState{can_go_back:back!=0,can_go_forward:forward!=0}));
            if loading==0 {if let Some(browser)=browser.as_deref(){publish_tls(browser,&self.state.events);}}
            let url=browser.and_then(|b|b.main_frame()).map(|f|CefString::from(&f.url()).to_string()).unwrap_or_default();
            self.state.events.send(if loading!=0{Event::LoadStart(ipc::LoadStart{url})}else{Event::LoadEnd(ipc::LoadEnd{url})});
        }
        fn on_load_error(&self,_browser:Option<&mut Browser>,frame:Option<&mut Frame>,code:Errorcode,text:Option<&CefString>,_url:Option<&CefString>){
            if code.get_raw() as i32 != -3 && frame.is_some_and(|f|f.is_main()!=0){self.state.events.send(Event::PageError(ipc::PageError{message:format!("Couldn’t load this page: {}. Reload to try again.",strings::borrowed(text,512)),unavailable:false}));}
        }
        fn on_load_start(&self,_browser:Option<&mut Browser>,frame:Option<&mut Frame>,_transition:TransitionType){if let Some(frame)=frame.filter(|f|f.is_main()!=0){dialogs::cancel_file(&self.state);self.state.events.send(Event::LoadCommitted(ipc::LoadEnd{url:CefString::from(&frame.url()).to_string()}));}}
    }
}
wrap_life_span_handler! {
    struct Lifetime { state:ViewState }
    impl LifeSpanHandler {
        fn on_before_popup(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _popup_id: i32,
            url: Option<&CefString>,
            _target: Option<&CefString>,
            _disposition: WindowOpenDisposition,
            gesture: i32,
            _features: Option<&PopupFeatures>,
            info: Option<&mut WindowInfo>,
            client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra: Option<&mut Option<DictionaryValue>>,
            _access: Option<&mut i32>,
        ) -> i32 {
            let url = url.map(ToString::to_string).unwrap_or_default();
            if gesture == 0 || !(url.is_empty() || url == "about:blank"
                || url::Url::parse(&url).is_ok_and(|url| matches!(url.scheme(), "http" | "https"))) {
                return 1;
            }
            let (Some(info), Some(client)) = (info, client) else { return 1 };
            static NEXT_POPUP: std::sync::atomic::AtomicU64 =
                std::sync::atomic::AtomicU64::new(1 << 63);
            let id = NEXT_POPUP.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            *info = WindowInfo::default().set_as_windowless(0);
            let state = ViewState {
                events: Events { id, output: self.state.events.output.clone() },
                popup_sender: self.state.popup_sender.clone(),
                opener: Some(self.state.events.clone()),
                size: Arc::new(Mutex::new((800, 600))),
                closed: Default::default(),
                blocking: Arc::new(Mutex::new(self.state.blocking.lock().unwrap().clone())),
                top: Default::default(),
                activity: Default::default(),
                transfers: self.state.transfers.clone(),
                strict: self.state.strict.clone(),
                dialogs: Default::default(),
                popup: Default::default(),
                drag: Default::default(),
            };
            *client = Some(PageClient::new(state));
            0
        }

        fn on_after_created(&self, browser: Option<&mut Browser>) {
            if let (Some(browser), Some(opener)) = (browser, self.state.opener.as_ref()) {
                let _ = self.state.popup_sender.send(CreatedPopup {
                    browser: browser.clone(),
                    state: self.state.clone(),
                    opener: opener.clone(),
                });
            }
        }
        fn on_before_close(&self,_browser:Option<&mut Browser>){dialogs::cancel(&self.state);self.state.closed.store(true,std::sync::atomic::Ordering::Release);self.state.events.send(Event::ViewClosed(true));}
    }
}
wrap_client! {
    struct PageClient { state:ViewState }
    impl Client {
        fn on_process_message_received(&self,_browser:Option<&mut Browser>,frame:Option<&mut Frame>,source:ProcessId,message:Option<&mut ProcessMessage>)->i32{{
            let mut activity=self.state.activity.lock().unwrap();let handled=activity.receive(frame,source,message);
            if handled!=0 {self.state.events.send(Event::PageActivity(ipc::PageActivity{protected:activity.protected()}));}handled
        }}
        fn dialog_handler(&self)->Option<DialogHandler>{Some(dialogs::FileHandler::new(self.state.clone()))}
        fn jsdialog_handler(&self)->Option<JsdialogHandler>{Some(dialogs::Handler::new(self.state.clone()))}
        fn permission_handler(&self)->Option<PermissionHandler>{Some(dialogs::MediaHandler::new(self.state.clone()))}
        fn download_handler(&self)->Option<DownloadHandler>{Some(downloads::Handler::new(self.state.transfers.clone(),Events{id:0,output:self.state.events.output.clone()}))}
        fn render_handler(&self)->Option<RenderHandler>{Some(Paint::new(self.state.clone()))}
        fn display_handler(&self)->Option<DisplayHandler>{Some(Display::new(self.state.clone()))}
        fn load_handler(&self)->Option<LoadHandler>{Some(Loading::new(self.state.clone()))}
        fn request_handler(&self)->Option<RequestHandler>{Some(RequestPolicy::new(self.state.clone()))}
        fn life_span_handler(&self)->Option<LifeSpanHandler>{Some(Lifetime::new(self.state.clone()))}
    }
}
wrap_app! {
    struct EngineApp { renderer:RenderProcessHandler }
    impl App {
        fn render_process_handler(&self)->Option<RenderProcessHandler>{Some(self.renderer.clone())}
        fn on_before_command_line_processing(&self,_kind:Option<&CefStringUtf16>,command:Option<&mut CommandLine>){if let Some(command)=command{
            let mut disabled=CefString::from(&command.switch_value(Some(&"disable-features".into()))).to_string();
            if !disabled.split(',').any(|s|s=="BackForwardCache"){if !disabled.is_empty(){disabled.push(',');}disabled.push_str("BackForwardCache");}
            command.append_switch_with_value(Some(&"disable-features".into()),Some(&disabled.as_str().into()));
            // Route HTTP/proxy challenges to our GTK handler instead of a detached Chrome window.
            command.append_switch(Some(&"disable-chrome-login-prompt".into()));
            command.append_switch(Some(&"no-first-run".into()));
            if std::env::var("PAISLEY_CEF_COMPONENT_UPDATES").as_deref() != Ok("0") {
                command.append_switch_with_value(Some(&"component-updater".into()), Some(&"fast-update".into()));
            } else {
                command.append_switch(Some(&"disable-component-update".into()));
            }
            command.append_switch_with_value(Some(&"disk-cache-size".into()),Some(&"67108864".into()));
            command.append_switch_with_value(Some(&"media-cache-size".into()),Some(&"16777216".into()));
        }}
    }
}
wrap_request_handler! {
    struct RequestPolicy { state:ViewState }
    impl RequestHandler {
        fn auth_credentials(&self,_browser:Option<&mut Browser>,origin:Option<&CefString>,proxy:i32,host:Option<&CefString>,port:i32,_realm:Option<&CefString>,_scheme:Option<&CefString>,callback:Option<&mut AuthCallback>)->i32 {
            let origin=if proxy!=0 {format!("{}:{port}",strings::borrowed(host,255))}else{strings::borrowed(origin,8192)};
            callback.is_some_and(|callback|dialogs::auth(&self.state,origin,proxy!=0,callback)) as i32
        }

        fn resource_request_handler(&self,_browser:Option<&mut Browser>,_frame:Option<&mut Frame>,_request:Option<&mut Request>,_navigation:i32,_download:i32,_initiator:Option<&CefString>,_disable:Option<&mut i32>)->Option<ResourceRequestHandler>{Some(ResourcePolicy::new(self.state.clone()))}
    }
}
wrap_resource_request_handler! {
    struct ResourcePolicy { state:ViewState }
    impl ResourceRequestHandler {
        fn cookie_access_filter(&self,_browser:Option<&mut Browser>,_frame:Option<&mut Frame>,_request:Option<&mut Request>)->Option<CookieAccessFilter>{Some(CookiePolicy::new(self.state.strict.clone()))}
        fn on_before_resource_load(&self,_browser:Option<&mut Browser>,_frame:Option<&mut Frame>,request:Option<&mut Request>,_callback:Option<&mut Callback>)->ReturnValue{
            let Some(request)=request else{return ReturnValue::CONTINUE;};
            if request.resource_type()==ResourceType::MAIN_FRAME{return ReturnValue::CONTINUE;}
            let policy=self.state.blocking.lock().unwrap();
            if !policy.enabled{return ReturnValue::CONTINUE;}
            let first_party=strings::owned(&request.first_party_for_cookies(),8192);
            let top=url::Url::parse(&first_party).ok().or_else(||url::Url::parse(&self.state.top.lock().unwrap()).ok());
            let target=url::Url::parse(&strings::owned(&request.url(),8192)).ok();
            if let (Some(top),Some(target))=(top,target){if !policy.exceptions.iter().any(|e|*e==top.origin().ascii_serialization())&&paisley_engine_transport::tracker_blocked(target.host_str().unwrap_or(""),top.host_str().unwrap_or("")){return ReturnValue::CANCEL;}}
            ReturnValue::CONTINUE
        }
    }
}
// Chromium computes first_party_for_cookies (including opaque cross-site contexts).
// Unknown contexts are denied in strict mode; top-level navigations remain usable.
wrap_cookie_access_filter! {
 struct CookiePolicy{strict:Arc<std::sync::atomic::AtomicBool>}
 impl CookieAccessFilter{
  fn can_send_cookie(&self,_browser:Option<&mut Browser>,_frame:Option<&mut Frame>,request:Option<&mut Request>,_cookie:Option<&Cookie>)->i32{cookie_allowed(self.strict.load(std::sync::atomic::Ordering::Acquire),request) as i32}
  fn can_save_cookie(&self,_browser:Option<&mut Browser>,_frame:Option<&mut Frame>,request:Option<&mut Request>,_response:Option<&mut Response>,_cookie:Option<&Cookie>)->i32{cookie_allowed(self.strict.load(std::sync::atomic::Ordering::Acquire),request) as i32}
 }
}
fn cookie_allowed(strict: bool, request: Option<&mut Request>) -> bool {
    if !strict {
        return true;
    }
    let Some(request) = request else {
        return false;
    };
    if request.resource_type() == ResourceType::MAIN_FRAME {
        return true;
    }
    let Some(first) =
        url::Url::parse(&strings::owned(&request.first_party_for_cookies(), 8192)).ok()
    else {
        return false;
    };
    let Some(target) = url::Url::parse(&strings::owned(&request.url(), 8192)).ok() else {
        return false;
    };
    let same_scheme = first.scheme() == target.scheme();
    match (first.host_str(), target.host_str()) {
        (Some(first), Some(target)) => {
            same_scheme && (target == first || target.ends_with(&format!(".{first}")))
        }
        _ => false,
    }
}
struct Page {
    browser: Browser,
    state: ViewState,
    closing: bool,
    retired: bool,
    inspector: Option<(u64, inspector::Bridge)>,
    pointer: (i32, i32),
    mouse_buttons: u32,
    external_file_drag: bool,
}
pub fn run_if_requested() {
    let args: Vec<String> = std::env::args().collect();
    let child = args.iter().any(|s| s.starts_with("--type="));
    if !child && !args.iter().any(|s| s == RUNNER_ARG) {
        return;
    }
    if child {
        unsafe {
            libc::dup2(libc::STDERR_FILENO, libc::STDOUT_FILENO);
        }
    }
    #[cfg(target_os = "linux")]
    if !child {
        let parent = args
            .windows(2)
            .find(|p| p[0] == "--servo-parent-pid")
            .and_then(|p| p[1].parse::<libc::pid_t>().ok());
        unsafe {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
        }
        if parent.is_some_and(|pid| unsafe { libc::getppid() } != pid) {
            std::process::exit(0);
        }
    }
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);
    let main_args = Args::new();
    let mut app = EngineApp::new(activity::RendererProbe::new(Default::default()));
    let status = execute_process(
        Some(main_args.as_main_args()),
        Some(&mut app),
        std::ptr::null_mut(),
    );
    if status >= 0 {
        std::process::exit(status);
    }
    if child {
        std::process::exit(1);
    }
    run(args, main_args, app);
    std::process::exit(0);
}
fn run(args: Vec<String>, main_args: Args, mut app: App) {
    let temporary = args.iter().any(|s| s == "--servo-temporary-storage");
    let directory = args
        .windows(2)
        .find(|p| p[0] == "--servo-config-dir")
        .map(|p| p[1].clone())
        .unwrap_or_default();
    if !temporary && !directory.is_empty() {
        std::fs::create_dir_all(&directory).expect("CEF profile directory");
    }
    let scratch = if temporary {
        use std::os::unix::fs::DirBuilderExt;
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "paisley-cef-private-{}-{unique}",
            std::process::id()
        ));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&path)
            .expect("private CEF storage");
        Some(path)
    } else {
        None
    };
    let root = scratch
        .clone()
        .unwrap_or_else(|| std::path::PathBuf::from(directory));
    let cache = if temporary {
        String::new()
    } else {
        root.join("cache").to_string_lossy().into_owned()
    };
    let runtime = std::env::var_os("PAISLEY_CEF_RUNTIME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_exe()
                .unwrap()
                .parent()
                .unwrap()
                .to_path_buf()
        });
    let settings = Settings {
        no_sandbox: (cfg!(all(feature = "test-support", debug_assertions))
            && std::env::var("PAISLEY_CEF_TEST_NO_SANDBOX").as_deref() == Ok("1"))
            as i32,
        windowless_rendering_enabled: 1,
        external_message_pump: 1,
        cache_path: cache.as_str().into(),
        root_cache_path: root.to_string_lossy().as_ref().into(),
        resources_dir_path: runtime.to_string_lossy().as_ref().into(),
        locales_dir_path: runtime.join("locales").to_string_lossy().as_ref().into(),
        log_severity: LogSeverity::WARNING,
        ..Default::default()
    };
    if initialize(
        Some(main_args.as_main_args()),
        Some(&settings),
        Some(&mut app),
        std::ptr::null_mut(),
    ) != 1
    {
        std::process::exit(1);
    }
    let (sender, receiver) =
        mpsc::sync_channel::<ipc::ServoAction>(paisley_engine_transport::ACTION_CAPACITY);
    std::thread::spawn(move || {
        let mut input = std::io::stdin().lock();
        loop {
            let mut header = [0; 4];
            if input.read_exact(&mut header).is_err() {
                break;
            }
            let Ok(len) = paisley_engine_transport::packet_length(header) else {
                break;
            };
            if len > paisley_engine_transport::MAX_ACTION_BYTES {
                break;
            }
            let mut body = vec![0; len];
            if input.read_exact(&mut body).is_err() {
                break;
            }
            let Ok(action) = ipc::ServoAction::decode(body.as_slice()) else {
                break;
            };
            if sender.send(action).is_err() {
                return;
            }
        }
    });
    let output = Arc::new(Mutex::new(std::io::stdout()));
    let mut pages: HashMap<u64, Page> = HashMap::new();
    let (popup_sender, popups) = mpsc::channel::<CreatedPopup>();
    let mut stopping = None;
    let mut strict_privacy = false;
    let strict_policy = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut closed_pages = 0_u64;
    let transfers = Arc::new(Mutex::new(downloads::Transfers::default()));
    let mut services = services::Services::new(Events {
        id: 0,
        output: output.clone(),
    });
    let mut last_probe = Instant::now() - Duration::from_secs(5);
    loop {
        do_message_loop_work();
        for CreatedPopup { browser, state, opener } in popups.try_iter() {
            if state.closed.load(std::sync::atomic::Ordering::Acquire) {
                continue;
            }
            if stopping.is_some() || pages.len() >= 32
                || !pages.get(&opener.id).is_some_and(|page| !page.closing)
            {
                if let Some(host) = browser.host() {
                    host.close_browser(1);
                }
                continue;
            }
            let id = state.events.id;
            pages.insert(id, Page {
                browser,
                state,
                closing: false,
                retired: false,
                inspector: None,
                pointer: (0, 0),
                mouse_buttons: 0,
                external_file_drag: false,
            });
            opener.send(Event::CreateWebView(ipc::CreateWebView { popup_id: id }));
        }
        services.reap();
        let frontend_ids: Vec<_> = pages
            .iter()
            .filter(|(_, p)| p.inspector.is_some())
            .map(|(id, _)| *id)
            .collect();
        for id in frontend_ids {
            let (target, messages, stopped) = {
                let page = &pages[&id];
                let (target, bridge) = page.inspector.as_ref().unwrap();
                let messages: Vec<_> = (0..64).map_while(|_| bridge.message()).collect();
                (*target, messages, bridge.stopped())
            };
            if let Some(host) = pages
                .get(&target)
                .filter(|p| !p.closing)
                .and_then(|p| p.browser.host())
            {
                for message in messages {
                    host.send_dev_tools_message(Some(&message));
                }
            } else {
                if let Some(page) = pages.get_mut(&id) {
                    dialogs::cancel(&page.state);
                    if let Some(host) = page.browser.host() {
                        host.close_browser(1);
                    }
                    page.closing = true;
                }
            }
            if stopped {
                if let Some(page) = pages.get_mut(&id) {
                    if !page.closing {
                        if let Some(host) = page.browser.host() {
                            host.close_browser(1);
                        }
                        page.closing = true;
                    }
                }
            }
        }
        if last_probe.elapsed() >= Duration::from_secs(5) {
            for page in pages.values().filter(|p| !p.closing && !p.retired) {
                page.state.activity.lock().unwrap().begin(&page.browser);
                page.state
                    .events
                    .send(Event::PageActivity(ipc::PageActivity { protected: true }));
            }
            last_probe = Instant::now();
        }
        for page in pages
            .values_mut()
            .filter(|page| page.retired && !page.closing)
        {
            if !transfers
                .lock()
                .unwrap()
                .owns_browser(page.browser.identifier())
            {
                if let Some(host) = page.browser.host() {
                    host.close_browser(1);
                }
                page.closing = true;
            }
        }
        pages.retain(|_, page| {
            let closed = page.state.closed.load(std::sync::atomic::Ordering::Acquire);
            if closed {
                closed_pages += 1;
            }
            !closed
        });
        if stopping.is_some() && pages.is_empty() {
            break;
        }
        if stopping.is_some_and(|at: Instant| at.elapsed() > Duration::from_secs(3)) {
            break;
        }
        for _ in 0..paisley_engine_transport::ACTION_CAPACITY {
            let action = match receiver.try_recv() {
                Ok(a) => a,
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    stopping.get_or_insert_with(Instant::now);
                    break;
                }
            };
            let id = action.webview_id;
            if matches!(action.action, Some(Action::AttachPopup(_))) && !pages.contains_key(&id) {
                Events { id, output: output.clone() }.send(Event::ViewClosed(true));
                continue;
            }
            if action
                .action
                .as_ref()
                .is_some_and(|action| downloads::dispatch(&transfers, action))
            {
                continue;
            }
            if let Some(Action::StartDownload(start)) = action.action.as_ref() {
                if url::Url::parse(&start.url).is_ok_and(|u| matches!(u.scheme(), "http" | "https"))
                {
                    if let Some(page) = pages
                        .get(&start.page_id)
                        .or_else(|| pages.values().find(|p| !p.closing))
                    {
                        if let Some(host) = page.browser.host() {
                            host.start_download(Some(&start.url.as_str().into()));
                        }
                    }
                }
                continue;
            }
            if matches!(action.action, Some(Action::QueryStats(_))) {
                Events {
                    id: 0,
                    output: output.clone(),
                }
                .send(Event::EngineStats(ipc::EngineStats {
                    pages: pages.len() as u32,
                    closed_pages,
                }));
                continue;
            }
            if matches!(action.action, Some(Action::Shutdown(_))) {
                stopping.get_or_insert_with(Instant::now);
                downloads::cancel_all(&transfers);
                break;
            }
            if let Some(Action::StrictPrivacy(strict)) = action.action.as_ref() {
                strict_privacy = *strict;
                strict_policy.store(*strict, std::sync::atomic::Ordering::Release);
                Events {
                    id: 0,
                    output: output.clone(),
                }
                .send(Event::OperationResult(ipc::OperationResult {
                    request_id: u64::MAX,
                    success: true,
                    message: String::new(),
                }));
                continue;
            }
            if let Some(Action::ClearSiteData(request)) = action.action.as_ref() {
                if let Some(host) = pages
                    .get(&request.page_id)
                    .filter(|p| {
                        !p.closing
                            && url::Url::parse(&p.state.top.lock().unwrap())
                                .is_ok_and(|u| u.origin().ascii_serialization() == request.origin)
                    })
                    .and_then(|p| p.browser.host())
                {
                    services.clear(&host, request);
                } else {
                    services.failure(
                        request.request_id,
                        "The page changed or closed. Reopen site information and try again.",
                    );
                }
                continue;
            }
            if let Some(Action::InspectPage(target)) = action.action.as_ref() {
                if stopping.is_some()
                    || pages.contains_key(&id)
                    || pages.values().filter(|p| p.inspector.is_some()).count() >= 8
                {
                    continue;
                }
                let bridge = pages
                    .get(target)
                    .filter(|p| !p.closing)
                    .and_then(|p| p.browser.host())
                    .and_then(|h| inspector::Bridge::new(&h).ok());
                if let Some(bridge) = bridge {
                    let state = ViewState {
                        events: Events {
                            id,
                            output: output.clone(),
                        },
                        popup_sender: popup_sender.clone(),
                        opener: None,
                        size: Arc::new(Mutex::new((640, 600))),
                        closed: Default::default(),
                        blocking: Default::default(),
                        top: Default::default(),
                        activity: Default::default(),
                        transfers: transfers.clone(),
                        strict: strict_policy.clone(),
                        dialogs: Default::default(),
                        popup: Default::default(),
                        drag: Default::default(),
                    };
                    let info = WindowInfo::default().set_as_windowless(0);
                    let settings = BrowserSettings {
                        windowless_frame_rate: 30,
                        ..Default::default()
                    };
                    if let Some(browser) = browser_host_create_browser_sync(
                        Some(&info),
                        Some(&mut PageClient::new(state.clone())),
                        Some(&bridge.url.as_str().into()),
                        Some(&settings),
                        None,
                        None,
                    ) {
                        pages.insert(
                            id,
                            Page {
                                browser,
                                state,
                                closing: false,
                                retired: false,
                                inspector: Some((*target, bridge)),
                                pointer: (0, 0),
                                mouse_buttons: 0,
                                external_file_drag: false,
                            },
                        );
                    } else {
                        Events {
                            id,
                            output: output.clone(),
                        }
                        .send(Event::ViewClosed(true));
                    }
                } else {
                    Events {
                        id,
                        output: output.clone(),
                    }
                    .send(Event::ViewClosed(true));
                }
                continue;
            }
            if matches!(action.action, Some(Action::CreateView(_))) {
                if stopping.is_some() || pages.contains_key(&id) {
                    continue;
                }
                if pages.values().filter(|p| !p.retired).count() >= 32 {
                    Events{id,output:output.clone()}.send(Event::PageError(ipc::PageError{message:"The limit of 32 live pages has been reached. Close a tab, then reload this page.".into(),unavailable:true}));
                    continue;
                }
                let state = ViewState {
                    events: Events {
                        id,
                        output: output.clone(),
                    },
                    popup_sender: popup_sender.clone(),
                    opener: None,
                    size: Arc::new(Mutex::new((800, 600))),
                    closed: Default::default(),
                    blocking: Default::default(),
                    top: Default::default(),
                    activity: Default::default(),
                    transfers: transfers.clone(),
                    strict: strict_policy.clone(),
                    dialogs: Default::default(),
                    popup: Default::default(),
                    drag: Default::default(),
                };
                let info = WindowInfo::default().set_as_windowless(0);
                let settings = BrowserSettings {
                    windowless_frame_rate: 30,
                    ..Default::default()
                };
                let mut extra = dictionary_value_create();
                if let Some(extra) = extra.as_ref() {
                    extra.set_bool(Some(&"paisley.strict".into()), strict_privacy as i32);
                }
                if let Some(browser) = browser_host_create_browser_sync(
                    Some(&info),
                    Some(&mut PageClient::new(state.clone())),
                    Some(&"about:blank".into()),
                    Some(&settings),
                    extra.as_mut(),
                    None,
                ) {
                    pages.insert(
                        id,
                        Page {
                            browser,
                            state,
                            closing: false,
                            retired: false,
                            inspector: None,
                            pointer: (0, 0),
                            mouse_buttons: 0,
                            external_file_drag: false,
                        },
                    );
                } else {
                    Events {
                        id,
                        output: output.clone(),
                    }
                    .send(Event::PageError(ipc::PageError {
                        message:
                            "The browser engine could not create this page. Reload to try again."
                                .into(),
                        unavailable: true,
                    }));
                }
                continue;
            }
            let Some(page) = pages.get_mut(&id) else {
                continue;
            };
            if !page.closing {
                handle(page, action.action);
            }
        }
        if stopping.is_some() {
            for page in pages.values_mut() {
                if !page.closing {
                    dialogs::cancel(&page.state);
                    if let Some(host) = page.browser.host() {
                        host.close_browser(1);
                    }
                    page.closing = true;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    drop(pages);
    drop(services);
    shutdown();
    if let Some(path) = scratch {
        let _ = std::fs::remove_dir_all(path);
    }
}
fn handle(page: &mut Page, action: Option<Action>) {
    if matches!(action, Some(Action::KeyPress(_) | Action::ButtonPress(_))) {
        page.state.activity.lock().unwrap().interacted = true;
        page.state
            .events
            .send(Event::PageActivity(ipc::PageActivity { protected: true }));
    }
    if matches!(action, Some(Action::LoadUrl(_) | Action::LoadHtml(_))) {
        *page.state.activity.lock().unwrap() = activity::Activity::default();
    }
    match action {
        Some(Action::LoadUrl(load)) => {
            if let Some(frame) = page.browser.main_frame() {
                frame.load_url(Some(&load.url.as_str().into()));
            }
        }
        Some(Action::LoadHtml(load)) => {
            if let Some(frame) = page.browser.main_frame() {
                let url = format!(
                    "data:text/html;charset=utf-8,{}",
                    url::form_urlencoded::byte_serialize(load.html.as_bytes()).collect::<String>()
                );
                frame.load_url(Some(&url.as_str().into()));
            }
        }
        Some(Action::Blocking(policy)) => {
            *page.state.blocking.lock().unwrap() = policy;
        }
        Some(Action::KeyPress(key)) => {
            if key.key == "Escape" && finish_local_drag(page, false) {
                return;
            }
            key_event(page, &key.key, key.key_code, key.modifiers, false)
        }
        Some(Action::KeyRelease(key)) => {
            key_event(page, &key.key, key.key_code, key.modifiers, true)
        }
        Some(Action::StopLoading(_)) => {
            if !finish_local_drag(page, false) {
                page.browser.stop_load();
            }
        }
        Some(Action::Reload(_)) => page.browser.reload(),
        Some(Action::GoBack(_)) => page.browser.go_back(),
        Some(Action::GoForward(_)) => page.browser.go_forward(),
        Some(Action::Response(response)) => dialogs::response(&page.state, &response),
        Some(Action::CloseView(_)) => {
            dialogs::cancel(&page.state);
            if page
                .state
                .transfers
                .lock()
                .unwrap()
                .owns_browser(page.browser.identifier())
            {
                // CEF stops delivering transfer callbacks after browser destruction.
                // Release the DOM and surface; keep only a tiny transfer host until terminal.
                page.retired = true;
                *page.state.size.lock().unwrap() = (1, 1);
                if let Some(host) = page.browser.host() {
                    host.was_hidden(1);
                    host.was_resized();
                }
                if let Some(frame) = page.browser.main_frame() {
                    frame.load_url(Some(&"about:blank".into()));
                }
            } else {
                if let Some(host) = page.browser.host() {
                    host.close_browser(1);
                }
                page.closing = true;
            }
        }
        Some(Action::Resize(size)) => {
            let len = (size.width as usize)
                .checked_mul(size.height as usize)
                .and_then(|n| n.checked_mul(4));
            if len.is_some_and(|n| paisley_engine_transport::valid_rgba(size.width, size.height, n))
            {
                *page.state.size.lock().unwrap() = (size.width as i32, size.height as i32);
                if let Some(host) = page.browser.host() {
                    host.was_resized();
                }
            }
        }
        Some(Action::Visible(visible)) => {
            if let Some(host) = page.browser.host() {
                host.was_hidden((!visible) as i32);
            }
        }
        Some(Action::Focused(focused)) => {
            if !focused {
                finish_local_drag(page, false);
            }
            if let Some(host) = page.browser.host() {
                host.set_focus(focused as i32);
            }
        }
        Some(Action::EvaluateJavascript(js)) => {
            if let Some(frame) = page.browser.main_frame() {
                frame.execute_java_script(Some(&js.source.as_str().into()), None, 1);
            }
        }
        Some(Action::SetZoomLevel(zoom)) => {
            if zoom.level.is_finite() && zoom.level > 0.0 {
                if let Some(host) = page.browser.host() {
                    host.set_zoom_level(zoom.level.clamp(0.1, 10.0).ln() / 1.2_f64.ln());
                }
            }
        }
        Some(Action::AttachPopup(_)) => {
            if let Some(frame) = page.browser.main_frame() {
                let url = CefString::from(&frame.url()).to_string();
                page.state.events.send(Event::UrlChanged(ipc::UrlChanged { url }));
            }
            if let Some(host) = page.browser.host() {
                if let Some(entry) = host.visible_navigation_entry() {
                    let title = CefString::from(&entry.title()).to_string();
                    page.state.events.send(Event::TitleChanged(ipc::TitleChanged { title }));
                }
                host.invalidate(PaintElementType::VIEW);
            }
            publish_tls(&page.browser, &page.state.events);
        }
        Some(Action::FileDrag(drag)) => external_file_drag(page, drag),
        Some(Action::Motion(pos)) => {
            page.pointer = (pos.x as i32, pos.y as i32);
            if move_local_drag(page) {
                return;
            }
            if let Some(host) = page.browser.host() {
                host.send_mouse_move_event(
                    Some(&MouseEvent {
                        x: pos.x as i32,
                        y: pos.y as i32,
                        modifiers: page.mouse_buttons,
                        ..Default::default()
                    }),
                    0,
                );
            }
        }
        Some(Action::ButtonPress(pos)) => {
            if pos.button == 1 {
                finish_local_drag(page, false);
            }
            page.pointer = (pos.x as i32, pos.y as i32);
            page.mouse_buttons |= mouse_button_flag(pos.button);
            mouse(page, pos.button, pos.x, pos.y, false, pos.click_count)
        }
        Some(Action::ButtonRelease(pos)) => {
            page.pointer = (pos.x as i32, pos.y as i32);
            page.mouse_buttons &= !mouse_button_flag(pos.button);
            if pos.button != 1 || !finish_local_drag(page, true) {
                mouse(page, pos.button, pos.x, pos.y, true, pos.click_count);
            }
        }
        Some(Action::Scroll(scroll)) => {
            if let Some(host) = page.browser.host() {
                host.send_mouse_wheel_event(
                    Some(&MouseEvent { x: page.pointer.0, y: page.pointer.1, modifiers: page.mouse_buttons }),
                    (-scroll.dx * 40.0) as i32,
                    (-scroll.dy * 40.0) as i32,
                );
            }
        }
        _ => {}
    }
}
fn external_file_drag(page: &mut Page, drag: ipc::FileDrag) {
    use ipc::file_drag::Phase;
    let Ok(phase) = Phase::try_from(drag.phase) else { return };
    let Some(host) = page.browser.host() else { return };
    if phase == Phase::Leave {
        if std::mem::take(&mut page.external_file_drag) {
            host.drag_target_drag_leave();
        }
        return;
    }
    let event = MouseEvent { x: drag.x as i32, y: drag.y as i32, modifiers: 0 };
    let operations = cef::sys::cef_drag_operations_mask_t::DRAG_OPERATION_COPY.into();
    if !page.external_file_drag {
        if drag.paths.is_empty() || drag.paths.iter().any(|path| !std::path::Path::new(path).is_absolute()) {
            return;
        }
        let Some(mut data) = drag_data_create() else { return };
        for path in drag.paths {
            data.add_file(Some(&path.as_str().into()), None);
        }
        host.drag_target_drag_enter(Some(&mut data), Some(&event), operations);
        page.external_file_drag = true;
    }
    host.drag_target_drag_over(Some(&event), operations);
    if phase == Phase::Drop {
        host.drag_target_drop(Some(&event));
        page.external_file_drag = false;
    }
}
fn drag_pointer(page: &Page) -> (MouseEvent, bool) {
    let (x, y) = page.pointer;
    let (width, height) = *page.state.size.lock().unwrap();
    (
        MouseEvent {
            x,
            y,
            modifiers: page.mouse_buttons,
        },
        x >= 0 && y >= 0 && x < width && y < height,
    )
}
fn move_local_drag(page: &Page) -> bool {
    let Some(host) = page.browser.host() else {
        return false;
    };
    let (event, inside) = drag_pointer(page);
    let (allowed, was_inside, mut data) = {
        let mut state = page.state.drag.lock().unwrap();
        let Some(drag) = state.as_mut() else {
            return false;
        };
        let was_inside = drag.entered;
        drag.entered = inside;
        if !inside {
            drag.operation = DragOperationsMask::default();
        }
        (
            drag.allowed,
            was_inside,
            (inside && !was_inside).then(|| Clone::clone(&drag.data)),
        )
    };
    if inside {
        if let Some(data) = data.as_mut() {
            host.drag_target_drag_enter(Some(data), Some(&event), allowed);
        }
        host.drag_target_drag_over(Some(&event), allowed);
    } else if was_inside {
        host.drag_target_drag_leave();
    }
    true
}
fn finish_local_drag(page: &Page, released: bool) -> bool {
    let Some(drag) = page.state.drag.lock().unwrap().take() else {
        return false;
    };
    if let Some(host) = page.browser.host() {
        let (event, inside) = drag_pointer(page);
        let mut operation = DragOperationsMask::default();
        if drag.entered {
            if released && inside && drag.operation != operation {
                host.drag_target_drop(Some(&event));
                operation = drag.operation;
            } else {
                host.drag_target_drag_leave();
            }
        }
        host.drag_source_ended_at(event.x, event.y, operation);
        host.drag_source_system_drag_ended();
    }
    true
}
fn mouse_button_flag(button: u32) -> u32 {
    match button { 1 => 1 << 4, 2 => 1 << 5, 3 => 1 << 6, _ => 0 }
}
fn mouse(page: &Page, button: u32, x: f64, y: f64, up: bool, click_count: i32) {
    if let Some(host) = page.browser.host() {
        let kind = match button {
            2 => MouseButtonType::MIDDLE,
            3 => MouseButtonType::RIGHT,
            _ => MouseButtonType::LEFT,
        };
        host.send_mouse_click_event(
            Some(&MouseEvent {
                x: x as i32,
                y: y as i32,
                modifiers: page.mouse_buttons,
                ..Default::default()
            }),
            kind,
            up as i32,
            click_count.max(1),
        );
    }
}

fn publish_tls(browser: &Browser, events: &Events) {
    let Some(status) = browser
        .host()
        .and_then(|h| h.visible_navigation_entry())
        .and_then(|e| e.sslstatus())
    else {
        return;
    };
    if status.is_secure_connection() == 0 {
        return;
    }
    let Some(pem) = status.x509_certificate().and_then(|c| c.pemencoded()) else {
        return;
    };
    if pem.size() == 0 || pem.size() > 65536 {
        return;
    }
    let mut bytes = vec![0; pem.size()];
    if pem.data(Some(&mut bytes), 0) != bytes.len() {
        return;
    }
    while bytes.last() == Some(&0) {
        bytes.pop();
    }
    events.send(Event::TlsState(ipc::TlsState {
        pem: bytes,
        cert_status: status.cert_status().get_raw(),
        certificate_error: is_cert_status_error(status.cert_status()) != 0,
        insecure_content: status.content_status().get_raw() != 0,
    }));
}
fn key_event(page: &Page, key: &str, code: u32, modifiers: u32, release: bool) {
    let vk = keyboard::windows_key_code(key);
    let flags = (if modifiers & 1 != 0 { 2 } else { 0 })
        | (if modifiers & 2 != 0 { 1 } else { 0 })
        | (modifiers & 12);
    if !release && modifiers & 4 != 0 {
        if let Some(frame) = page.browser.focused_frame() {
            match key.to_ascii_lowercase().as_str() {
                "c" => {
                    frame.copy();
                    return;
                }
                "v" => {
                    frame.paste();
                    return;
                }
                "x" => {
                    frame.cut();
                    return;
                }
                "a" => {
                    frame.select_all();
                    return;
                }
                _ => {}
            }
        }
    }
    if let Some(host) = page.browser.host() {
        let event = KeyEvent {
            type_: if release {
                KeyEventType::KEYUP
            } else {
                KeyEventType::RAWKEYDOWN
            },
            modifiers: flags,
            windows_key_code: vk,
            native_key_code: code as i32,
            ..Default::default()
        };
        host.send_key_event(Some(&event));
        if !release && modifiers & 12 == 0 {
            let value = match key {
                "Enter" => "\r",
                "Tab" => "\t",
                _ => key,
            };
            if value.chars().count() == 1 {
                for character in value.encode_utf16() {
                    host.send_key_event(Some(&KeyEvent {
                        type_: KeyEventType::CHAR,
                        character,
                        unmodified_character: character,
                        ..event.clone()
                    }));
                }
            }
        }
    }
}
