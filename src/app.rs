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
    host_server_public: String,
    socket: Option<SocketReader>,
    messages: Arc<Mutex<Vec<ChatMessage>>>,
    mode: i32,
    servers: Vec<ChatServer>,
    error_box: Arc<Mutex<bool>>,
    error: Arc<Mutex<String>>,
    error_message: Arc<Mutex<String>>,
    hosted_server: Arc<Mutex<Option<String>>>
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

macro_rules! show_error {

    ($flag:expr, $text:expr, $msg:expr, $message:expr, $description:expr) => {
        let mut flag_lock = $flag.lock().unwrap();
        *flag_lock = true;
        let mut text_lock = $text.lock().unwrap();
        *text_lock = $message.to_string();
        let mut msg_lock = $msg.lock().unwrap();
        *msg_lock = $description.to_string();
    }

}

macro_rules! connect_to_server {
    ($server:expr, $username:expr, $socket:expr, $messages:expr, $flag:expr, $text:expr, $msg:expr) => {
        let current_server = $server.url.clone();
        let my_username = $username.clone();
        let websocket = Arc::new(Mutex::new(None));
        let thread_ws_link = websocket.clone();
        let msg_list = Arc::new(Mutex::new(vec![]));
        let messages_link = msg_list.clone();

        let error_link_1 = $flag.clone();
        let error_link_2 = $text.clone();
        let error_link_3 = $msg.clone();

        std::thread::spawn(move || {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                    println!("Attempting to connect to url: {}", current_server);
                    let url = url::Url::parse(&current_server).unwrap();
                    match connect_async(url).await {
                        Ok((ws_stream, _)) => {
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
                        },
                        Err(_) => {
                            show_error!(error_link_1, error_link_2, error_link_3, "Couldn't connect to server", "Failed to reach remote host");
                        }
                    }
                });
        });
        *$socket = Some(websocket);
        *$messages = msg_list;
    }
}

macro_rules! refresh_servers {
    ($serv_list:expr) => {
        let mut servers = vec![];
        let body = reqwest::blocking::get("http://82.35.235.223:8000/get_servers").unwrap().text().unwrap();
        for (server_name, server_url) in serde_json::from_str::<std::collections::HashMap<String, String>>(&body).unwrap() {
            servers.push(ChatServer{
                name: server_name,
                url: server_url
            })
        }
        *$serv_list = servers;
    }
}

impl ChatApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {

        let mut servers = vec![];
        refresh_servers!(&mut servers);

        Self {
            message: String::new(),
            username: String::from("Username"),
            host_server_name: String::new(),
            host_server_url: String::from("192.168.0.36:8080"),
            host_server_public: String::from("82.35.235.223:8080"),
            socket: None,
            messages: Arc::new(Mutex::new(vec![])),
            mode: 0,
            servers,
            error_box: Arc::new(Mutex::new(false)),
            error: Arc::new(Mutex::new(String::new())),
            error_message: Arc::new(Mutex::new(String::new())),
            hosted_server: Arc::new(Mutex::new(None))
        }
    }
}

impl eframe::App for ChatApp {

    fn on_exit(&mut self, _ctx: &eframe::glow::Context) {
        if let Some(server_name) = &*self.hosted_server.lock().unwrap() {
            println!("Shutting down server");
            let client = reqwest::blocking::Client::new();
            let s_name = base64::encode_config(server_name, base64::URL_SAFE);
            client.delete(format!("http://82.35.235.223:8000/remove_server/{}", s_name)).send().unwrap();
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {

        let Self { message, username, host_server_name, host_server_url, host_server_public, messages, socket, mode, servers, error_box, error, error_message, hosted_server } = self;
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
                if *(error_box.lock().unwrap()) {
                    egui::Window::new("Error").show(ctx, |ui| {
                        ui.heading(error.lock().unwrap().clone());
                        ui.label(error_message.lock().unwrap().clone());
                        if ui.button("ok").clicked() {
                            *error_box.lock().unwrap() = false;
                        }
                    });
                } else {

                    egui::SidePanel::left("server_list").show(ctx, |ui| {
                        ui.heading("Server list");

                        if ui.button("Refresh servers").clicked() {
                            refresh_servers!(servers);
                        }

                        for server in servers.iter() {
                            ui.horizontal(|ui| {
                                ui.label(format!("{} ({})", server.name, server.url));
                                if ui.button("Join").clicked() {
                                    connect_to_server!(server, username, socket, messages, error_box, error, error_message);
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
                        ui.horizontal(|ui| {
                            ui.label("Server public url: ");
                            ui.text_edit_singleline(host_server_public);
                        });
                        if ui.button("Host").clicked() {
                            if host_server_name.to_string() == String::new() {
                                show_error!(
                                    error_box, error, error_message,
                                    "Couldn't start server",
                                    "Please choose a name for the server."
                                );
                            } else if hosted_server.lock().unwrap().is_some() {
                                show_error!(
                                    error_box, error, error_message,
                                    "Couldn't start server",
                                    "You are already hosting a server."
                                );
                            } else {
                                let hs_name = host_server_name.to_string().clone();
                                let hs_url = host_server_url.to_string().clone();
                                let hs_name_2 = hs_name.clone();
                                let hs_public = host_server_public.to_string().clone();
                                let hs_public_2 = hs_public.clone();

                                let err_box_2 = error_box.clone();
                                let err_2 = error.clone();
                                let err_message_2 = error_message.clone();

                                let hs_link = hosted_server.clone();
                                std::thread::spawn(move || {
                                    tokio::runtime::Builder::new_multi_thread()
                                        .enable_all()
                                        .build()
                                        .unwrap()
                                        .block_on(async {
                                            match chatroom::host_room(hs_name, hs_url, hs_public, hs_link).await {
                                                Ok(_) => {},
                                                Err(_) => {
                                                    show_error!(
                                                        err_box_2, err_2, err_message_2,
                                                        "Couldn't bind to local IP",
                                                        "Please use a valid local IP."
                                                    );
                                                }
                                            }
                                        });
                                });
                                if !*(error_box.lock().unwrap()) {
                                    let mut url_string = String::from("ws://");
                                    url_string.push_str(&hs_public_2);
                                    connect_to_server!(ChatServer {
                                        name: hs_name_2,
                                        url: url_string,
                                    }, username, socket, messages, error_box, error, error_message);
                                }
                            }
                        }

                    });
                    egui::TopBottomPanel::bottom("input").show(ctx, |ui| {
                        ui.text_edit_multiline(message);
                        if ui.button("send").clicked() {
                            match socket {
                                Some(_) => {
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
                                },
                                None => {
                                    show_error!(
                                        error_box, error, error_message,
                                        "Couldn't send message",
                                        "You are not connected to a server."
                                    );
                                }
                            }
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

                }
            },
            _ => panic!("Unknown mode: {}", mode)
        }
    }
}
