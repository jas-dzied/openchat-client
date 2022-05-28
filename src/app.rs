use futures_util::{pin_mut, StreamExt, SinkExt};
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, WebSocketStream, MaybeTlsStream, tungstenite::protocol::Message};
use std::sync::{Arc, Mutex};

type SocketReader = Arc<Mutex<Option<futures_util::stream::SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>>>;

pub struct ChatApp {
    message: String,
    username: String,
    host_server_name: String,
    host_server_url: String,
    socket: Option<SocketReader>,
    messages: Arc<Mutex<Vec<ChatMessage>>>,
    mode: i32,
    servers: Vec<ChatServer>
}

struct ChatMessage {
    sender: String,
    text: String
}
#[derive(Clone)]
struct ChatServer {
    name: String,
    url: String
}

macro_rules! connect_to_server {
    ($server:expr, $username:expr, $socket:expr, $messages:expr) => {
        let current_server = $server.url.clone();
        let my_username = $username.clone();
        let websocket = Arc::new(Mutex::new(None));
        let thread_ws_link = websocket.clone();
        let msg_list = Arc::new(Mutex::new(vec![]));
        let messages_link = msg_list.clone();
        std::thread::spawn(move || {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                    let url = url::Url::parse(&current_server).unwrap();
                    let (ws_stream, _) = connect_async(url).await.expect("Failed to connect");
                    let (mut write, read) = ws_stream.split();
                    let mut join_message = std::collections::HashMap::new();
                    join_message.insert("event", "join");
                    join_message.insert("username", &my_username);
                    write.send(Message::from(serde_json::to_string(&join_message).unwrap())).await.unwrap();
                    let mut ws_writes = thread_ws_link.lock().unwrap();
                    *ws_writes = Some(write);
                    drop(ws_writes);
                    let ws_to_stdout = {
                        read.for_each(|message| async {
                            let data = message.unwrap().into_text().unwrap();
                            let event: serde_json::Value = serde_json::from_str(&data).unwrap();
                            let mut message_lock = messages_link.lock().unwrap();
                            message_lock.push(ChatMessage {
                                sender: event["sender"].as_str().unwrap().to_string(),
                                text: event["message"].as_str().unwrap().to_string()
                            });
                        })
                    };
                    pin_mut!(ws_to_stdout);
                    ws_to_stdout.await;
                });
        });
        *$socket = Some(websocket);
        *$messages = msg_list;
    }
}

impl ChatApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {

        let mut servers = vec![];
        let body = reqwest::blocking::get("http://127.0.0.1:8000/get_servers").unwrap().text().unwrap();
        for (server_name, server_url) in serde_json::from_str::<std::collections::HashMap<String, String>>(&body).unwrap() {
            servers.push(ChatServer{
                name: server_name,
                url: server_url
            })
        }

        Self {
            message: String::new(),
            username: String::from("Username"),
            host_server_name: String::new(),
            host_server_url: String::from("127.0.0.1:8080"),
            socket: None,
            messages: Arc::new(Mutex::new(vec![])),
            mode: 0,
            servers
        }
    }
    }

impl eframe::App for ChatApp {

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {

        let Self { message, username, host_server_name, host_server_url, messages, socket, mode, servers } = self;
        match mode {
            0 => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.label("Choose a username:");
                    ui.text_edit_singleline(username);
                    if ui.button("Login").clicked() {
                        *mode += 1;
                    }
                });
            },
            1 => {
                egui::SidePanel::left("server_list").show(ctx, |ui| {
                    ui.heading("Server list");

                    for server in servers.iter() {
                        ui.horizontal(|ui| {
                            ui.label(format!("{} ({})", server.name, server.url));
                            if ui.button("Join").clicked() {
                                connect_to_server!(server, username, socket, messages);
                            }
                        });
                    }
                    ui.label("Host your own server");
                    ui.horizontal(|ui| {
                        ui.label("Server name: ");
                        ui.text_edit_singleline(host_server_name);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Server url: ");
                        ui.text_edit_singleline(host_server_url);
                    });
                    if ui.button("Host").clicked() {
                        let hs_name = host_server_name.to_string().clone();
                        let hs_url = host_server_url.to_string().clone();
                        let hs_name_2 = hs_name.clone();
                        let hs_url_2 = hs_url.clone();
                        std::thread::spawn(move || {
                            tokio::runtime::Builder::new_multi_thread()
                                .enable_all()
                                .build()
                                .unwrap()
                                .block_on(async {
                                    chatroom::host_room(hs_name, hs_url).await.unwrap();
                                });
                        });
                        let mut url_string = String::from("ws//");
                        url_string.push_str(&hs_url_2);
                        connect_to_server!(ChatServer {
                            name: hs_name_2,
                            url: url_string
                        }, username, socket, messages);
                    }

                });
                egui::TopBottomPanel::bottom("input").show(ctx, |ui| {
                    ui.text_edit_multiline(message);
                    if ui.button("send").clicked() {
                        let mut writer = socket.as_mut().unwrap().lock().unwrap();
                        tokio::runtime::Builder::new_multi_thread()
                            .enable_all()
                            .build()
                            .unwrap()
                            .block_on(async {
                                let mut event = std::collections::HashMap::new();
                                event.insert("event", "message");
                                event.insert("sender", username);
                                event.insert("message", message);
                                let msg = serde_json::to_string(&event).unwrap();
                                writer.as_mut().unwrap().send(Message::from(msg)).await.unwrap();
                            });
                    }
                });
                egui::CentralPanel::default().show(ctx, |ui| {
                    if socket.is_some() {
                        if let Ok(lock) = messages.try_lock() {
                            for message in lock.iter() {
                                ui.label(format!("<{}> {}", message.sender, message.text));
                            }
                        }
                    } else {
                        ui.label("Connect to a server to receive messages.");
                    }
                });
            },
            _ => panic!("Unknown mode: {}", mode)
        }
    }
}
