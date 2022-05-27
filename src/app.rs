use futures_util::{pin_mut, StreamExt, SinkExt};
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, WebSocketStream, MaybeTlsStream, tungstenite::protocol::Message};
use std::sync::{Arc, Mutex};

type SocketReader = Arc<Mutex<Option<futures_util::stream::SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>>>;

pub struct ChatApp {
    message: String,
    socket: SocketReader,
    messages: Arc<Mutex<Vec<String>>>
}

impl ChatApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {

        let websocket = Arc::new(Mutex::new(None));
        let thread_ws_link = websocket.clone();

        let messages = Arc::new(Mutex::new(vec![]));
        let messages_link = messages.clone();

        std::thread::spawn(move || {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    let url = url::Url::parse("ws://127.0.0.1:8080").unwrap();
                    let (ws_stream, _) = connect_async(url).await.expect("Failed to connect");
                    let (write, read) = ws_stream.split();
                    let mut ws_writes = thread_ws_link.lock().unwrap();
                    *ws_writes = Some(write);
                    drop(ws_writes);
                    let ws_to_stdout = {
                        read.for_each(|message| async {
                            let data = message.unwrap().into_text().unwrap();
                            let mut message_lock = messages_link.lock().unwrap();
                            message_lock.push(data);
                        })
                    };
                    pin_mut!(ws_to_stdout);
                    ws_to_stdout.await;
                });
        });
        Self {
            message: String::new(),
            socket: websocket,
            messages
        }
    }
}

impl eframe::App for ChatApp {

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {

        let Self { message, messages, socket } = self;

        egui::CentralPanel::default().show(ctx, |ui| {

            ui.text_edit_multiline(message);
            if ui.button("send").clicked() {
                let mut writer = socket.lock().unwrap();
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .unwrap()
                    .block_on(async {
                        writer.as_mut().unwrap().send(Message::from(message.clone())).await.unwrap();
                    });
            }

            if let Ok(lock) = messages.try_lock() {
                for message in lock.iter() {
                    ui.label(message);
                }
            }

        });

    }
}
