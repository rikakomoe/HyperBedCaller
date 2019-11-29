use std::{env, io, sync::Arc, thread, time};
extern crate uname;
use hyper_bed_caller::*;
use rtdlib::{tdjson::Tdlib, types::*};

fn main() {
    let tdlib = Arc::new(Tdlib::new());
    let set_online = SetOption::builder()
        .name("online")
        .value(OptionValue::Boolean(
            OptionValueBoolean::builder().value(true).build(),
        ))
        .build();
    tdlib.send(&set_online.to_json().expect("Bad JSON"));
    let handler = start_handler(tdlib.clone());
    let cron = start_cron(tdlib);
    handler.join().expect("Handler thread failed");
    cron.join().expect("Cron thread failed");
}

fn start_handler(tdlib: Arc<Tdlib>) -> thread::JoinHandle<()> {
    thread::spawn(move || loop {
        let json = tdlib.receive(60.0);
        if let None = json {
            continue;
        }
        let json = json.unwrap();
        let td_type = detect_td_type(json.as_str());
        if let None = td_type {
            eprintln!("data failed with json");
            continue;
        };
        let td_type = td_type.unwrap();
        match td_type.as_str() {
            "updateAuthorizationState" => {
                let state: UpdateAuthorizationState =
                    serde_json::from_str(json.as_str()).unwrap_or_default();
                let req: Option<Box<dyn RObject>> = match state.authorization_state() {
                    AuthorizationState::WaitTdlibParameters(_) => Some(Box::new(
                        SetTdlibParameters::builder()
                            .parameters(
                                TdlibParameters::builder()
                                    .database_directory("tdlib")
                                    .use_message_database(true)
                                    .use_secret_chats(true)
                                    .api_id(env!("API_ID").parse::<i64>().expect("Bad API ID"))
                                    .api_hash(env!("API_HASH"))
                                    .system_language_code("en")
                                    .device_model("Desktop")
                                    .system_version(uname::uname().expect("Bad uname").sysname)
                                    .application_version(env!("CARGO_PKG_VERSION"))
                                    .enable_storage_optimizer(true)
                                    .build(),
                            )
                            .build(),
                    )),
                    AuthorizationState::WaitEncryptionKey(_) => {
                        Some(Box::new(SetDatabaseEncryptionKey::builder().build()))
                    }
                    AuthorizationState::WaitPhoneNumber(_) => Some(Box::new(
                        SetAuthenticationPhoneNumber::builder()
                            .phone_number(env::var("PHONE").expect("Unknown env phone number"))
                            .build(),
                    )),
                    AuthorizationState::WaitCode(code) => {
                        let prompt = match code.code_info().type_() {
                            AuthenticationCodeType::TelegramMessage(_) => {
                                String::from(
                                    "Telegram has sent the code to the Telegram app on your other device."
                                )
                            }
                            AuthenticationCodeType::Sms(_) => {
                                format!(
                                    "Telegram has sent an SMS with an activation code to your phone {}.",
                                     code.code_info().phone_number()
                                )
                            }
                            _ => {
                                String::from("Telegram is calling you.")
                            }
                        };
                        println!("{}", prompt);
                        println!("Please type authentication code:");
                        let mut input = String::new();
                        io::stdin().read_line(&mut input).expect("Bad input");
                        Some(Box::new(
                            CheckAuthenticationCode::builder().code(input).build(),
                        ))
                    }
                    _ => {
                        println!("{}\t{}", td_type, json);
                        None
                    }
                };
                if let Some(req) = req {
                    tdlib.send(&req.to_json().expect("Bad JSON"))
                }
            }
            "updateNewMessage" => {
                let update_new_message: UpdateNewMessage =
                    serde_json::from_str(json.as_str()).unwrap_or_default();
                let message = update_new_message.message();
                if message.is_outgoing() {
                    continue;
                }
                println!("{}", json);
                match message.content() {
                    MessageContent::MessageText(message_text) => {
                        let text = message_text.text().text();
                        if text.starts_with("#") {
                            let text = message_text.text().text();
                            let cmd = parse_params(text);
                            let to_send = format!("cmd: {}\nargs: {}", cmd.cmd(), cmd.arg());
                            let req = SendMessage::builder()
                                .chat_id(message.chat_id())
                                .input_message_content(InputMessageContent::InputMessageText(
                                    InputMessageText::builder()
                                        .text(FormattedText::builder().text(to_send).build())
                                        .clear_draft(true)
                                        .build(),
                                ))
                                .reply_to_message_id(message.id())
                                .build();
                            tdlib.send(&req.to_json().expect("Bad JSON"));
                        }
                    }
                    _ => (),
                }
            }
            _ => {
                println!("{}\t{}", td_type, json);
            }
        };
    })
}

fn start_cron(tdlib: Arc<Tdlib>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        thread::sleep(time::Duration::from_secs(5));
        let get_me = GetMe::builder().build();
        tdlib.send(&get_me.to_json().expect("Bad JSON"));
    })
}
