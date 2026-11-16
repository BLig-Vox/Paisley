//! Private, token-gated loopback relay for the bundled DevTools frontend.
//! CEF's ShowDevTools creates a native Chrome window even for OSR clients.
use cef::*;
use std::{
    io::Read,
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
use tungstenite::{
    Error, Message,
    handshake::server::{ErrorResponse, Request, Response},
    protocol::WebSocketConfig,
};
#[path = "cef_inspector_queue.rs"]
mod queue;
use queue::{BUDGET, Queue};
wrap_dev_tools_message_observer! {
 struct Observer{output:Arc<Mutex<Queue>>,stop:Arc<AtomicBool>}
 impl DevToolsMessageObserver{
  fn on_dev_tools_message(&self,_browser:Option<&mut Browser>,message:Option<&[u8]>)->i32{
   if let Some(message)=message{if !self.output.lock().unwrap().push(message){eprintln!("Paisley DevTools relay output exceeded its bounded queue");self.stop.store(true,Ordering::Release);}}1
  }
  fn on_dev_tools_agent_detached(&self,_browser:Option<&mut Browser>){self.stop.store(true,Ordering::Release);}
 }
}
pub struct Bridge {
    pub url: String,
    input: Arc<Mutex<Queue>>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
    _registration: Registration,
}
impl Bridge {
    pub fn new(host: &BrowserHost) -> Result<Self, String> {
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let address = listener.local_addr().map_err(|e| e.to_string())?;
        let mut random = [0u8; 32];
        std::fs::File::open("/dev/urandom")
            .and_then(|mut f| f.read_exact(&mut random))
            .map_err(|e| e.to_string())?;
        let token: String = random.iter().map(|b| format!("{b:02x}")).collect();
        let path = format!("/{token}");
        let input = Arc::new(Mutex::new(Queue::default()));
        let output = Arc::new(Mutex::new(Queue::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let registration = host
            .add_dev_tools_message_observer(Some(&mut Observer::new(output.clone(), stop.clone())))
            .ok_or("DevTools observer unavailable")?;
        let worker_input = input.clone();
        let worker_stop = stop.clone();
        let worker = thread::spawn(move || {
            while !worker_stop.load(Ordering::Acquire) {
                let (stream, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(_) => {
                        thread::sleep(Duration::from_millis(10));
                        continue;
                    }
                };
                let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
                let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
                let check =
                    |request: &Request, response: Response| -> Result<Response, ErrorResponse> {
                        let origin = request
                            .headers()
                            .get("origin")
                            .and_then(|h| h.to_str().ok());
                        if request.uri().path() != path
                            || !matches!(origin, Some("devtools://devtools" | "null"))
                        {
                            return Err(tungstenite::http::Response::builder()
                                .status(403)
                                .body(Some("Forbidden".into()))
                                .unwrap());
                        }
                        Ok(response)
                    };
                let config = WebSocketConfig::default()
                    .read_buffer_size(8192)
                    .write_buffer_size(0)
                    .max_write_buffer_size(BUDGET + 1024)
                    .max_message_size(Some(BUDGET))
                    .max_frame_size(Some(BUDGET));
                let Ok(mut socket) =
                    tungstenite::accept_hdr_with_config(stream, check, Some(config))
                else {
                    continue;
                };
                let _ = socket
                    .get_mut()
                    .set_read_timeout(Some(Duration::from_millis(10)));
                while !worker_stop.load(Ordering::Acquire) {
                    for _ in 0..64 {
                        let next = output.lock().unwrap().pop();
                        let Some(next) = next else {
                            break;
                        };
                        let Ok(text) = String::from_utf8(next) else {
                            worker_stop.store(true, Ordering::Release);
                            break;
                        };
                        if socket.send(Message::Text(text.into())).is_err() {
                            worker_stop.store(true, Ordering::Release);
                            break;
                        }
                    }
                    match socket.read() {
                        Ok(Message::Text(text)) => {
                            if !worker_input.lock().unwrap().push(text.as_bytes()) {
                                eprintln!("Paisley DevTools relay input exceeded its bounded queue");
                                worker_stop.store(true, Ordering::Release);
                            }
                        }
                        Ok(Message::Close(_))
                        | Err(Error::ConnectionClosed | Error::AlreadyClosed) => break,
                        Err(Error::Io(e))
                            if matches!(
                                e.kind(),
                                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                            ) => {}
                        Err(_) => break,
                        _ => {}
                    }
                }
                worker_stop.store(true, Ordering::Release);
                break;
            }
        });
        #[cfg(feature = "test-support")]
        if std::env::var_os("PAISLEY_ENGINE_DIAGNOSTICS").is_some() {
            use std::io::Write;
            let mut socket = std::net::TcpStream::connect(address).expect("test relay connection");
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            socket.write_all(format!("GET /wrong-token HTTP/1.1\r\nHost: {address}\r\nOrigin: devtools://devtools\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n").as_bytes()).unwrap();
            let mut reply = [0; 128];
            let count = socket.read(&mut reply).unwrap();
            assert!(
                std::str::from_utf8(&reply[..count])
                    .unwrap()
                    .starts_with("HTTP/1.1 403")
            );
        }
        Ok(Self {
            url: format!("devtools://devtools/bundled/devtools_app.html?ws={address}/{token}"),
            input,
            stop,
            worker: Some(worker),
            _registration: registration,
        })
    }
    pub fn message(&self) -> Option<Vec<u8>> {
        self.input.lock().unwrap().pop()
    }
    pub fn stopped(&self) -> bool {
        self.stop.load(Ordering::Acquire)
    }
}
impl Drop for Bridge {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
