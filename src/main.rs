use std::{
    collections::HashMap,
    env,
    fs::File,
    io::{Read, Write},
    str::Split,
    time::Duration,
};

use chrono::{DateTime, Datelike, NaiveDateTime, NaiveTime, Utc, Weekday};
use env_logger::Builder;
use futures::StreamExt;
use log::{info, LevelFilter};
use once_cell::sync::Lazy;
use ron::{
    de::from_str,
    ser::{to_string_pretty, PrettyConfig},
};
use serde::{Deserialize, Serialize};
use telegram_bot::{
    types::{
        refs::{ChatId, ChatRef},
        requests::send_message::{CanReplySendMessage, SendMessage},
    },
    Api, Message, MessageKind, UpdateKind,
};
use tokio::{self, sync::Mutex, time::delay_for};

const MOSCOW_OFFSET: i64 = 3;
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, Serialize, Deserialize)]
struct Timer {
    pub name: String,
    pub msg: String,
    pub week_day: Weekday,
    pub time: NaiveTime,
    pub last_time: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Chats {
    pub reminders: HashMap<ChatId, Vec<Timer>>,
}

static DATA_PATH: &'static str = "chats.ron";

static CHATS: Lazy<Mutex<Chats>> = Lazy::new(|| {
    let chats = if let Ok(mut file) = File::open(DATA_PATH) {
        let mut data = String::new();
        file.read_to_string(&mut data)
            .expect("failed to read saved data");
        from_str(&data).expect("failed to parse saved data")
    } else {
        let chats = Chats::default();
        update_file(DATA_PATH, &chats).expect("Failed to save data");
        chats
    };
    Mutex::new(chats)
});

fn update_file(data_path: &str, chats: &Chats) -> Result<()> {
    let pretty = PrettyConfig {
        depth_limit: 2,
        separate_tuple_members: true,
        enumerate_arrays: true,
        ..PrettyConfig::default()
    };
    let mut file = File::create(data_path)?;
    file.write_all(to_string_pretty(chats, pretty)?.as_bytes())?;
    Ok(())
}

fn parse_request(mut tokens: Split<'_, &str>) -> Result<(Weekday, NaiveTime, String)> {
    let token = tokens.next().ok_or("no dayweek token")?;
    info!("{}", &token);
    let day_week: Weekday = from_str(&format!("\"{}\"", token))?;
    info!("{:?}", day_week);
    let token = tokens.next().ok_or("no time token")?;
    info!("{}", &token);
    let time = NaiveTime::parse_from_str(token, "%H:%M:%S")?;
    let token = tokens.next().ok_or("no msg token")?;

    Ok((day_week, time, token.to_string()))
}

async fn reminder(api: Api) {
    let mut last_time_log: DateTime<Utc> = Utc::now() + chrono::Duration::hours(MOSCOW_OFFSET);
    loop {
        {
            for (id, timers) in CHATS.lock().await.reminders.iter_mut() {
                let now: DateTime<Utc> = Utc::now() + chrono::Duration::hours(MOSCOW_OFFSET);
                for timer in timers.iter_mut() {
                    let week_day = now.weekday();
                    if week_day != timer.week_day {
                        continue;
                    }
                    let naive_dt = NaiveDateTime::new(now.date().naive_utc(), timer.time);
                    let dt = DateTime::<Utc>::from_utc(naive_dt, Utc);
                    let d_sec = (now - dt).num_seconds();
                    let need_remind = timer
                        .last_time
                        .map(|t| (now - t).num_seconds() > 100)
                        .unwrap_or(true);
                    if d_sec.abs() < 1 && need_remind {
                        timer.last_time = Some(now);
                        let chat_ref = ChatRef::from_chat_id(*id);
                        let msg =
                            SendMessage::new(chat_ref, format!("{}: {}", &timer.name, &timer.msg));
                        let _err = api.send(msg).await;
                    }
                }
            }
            // unlock mutex
        }
        delay_for(Duration::from_millis(100)).await;
        let now: DateTime<Utc> = Utc::now() + chrono::Duration::hours(MOSCOW_OFFSET);
        if (now - last_time_log).num_seconds() > 10 {
            info!("{:?}", now);
            last_time_log = now;
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut builder = Builder::new();
    builder.filter_level(LevelFilter::Info).init();

    let token = env::var("LAZY_TOKEN").expect("token not set");
    let api = Api::new(token);
    let a = api.clone();
    tokio::spawn(async move { reminder(a).await });
    // Fetch new updates via long poll method
    let mut stream = api.stream();
    // api.send(message.text_reply("hello".to_string()));
    while let Some(update) = stream.next().await {
        // If the received update contains a new message...
        let update = update?;
        if let UpdateKind::Message(message) = update.kind {
            if let MessageKind::Text { ref data, .. } = message.kind {
                let chat_id = message.chat.id();
                let mut tokens = data.split(" ");
                if let Some(cmd) = tokens.next() {
                    dispatch_command(&api, &message, &chat_id, tokens, cmd.to_string()).await?
                } else {
                    continue;
                }
                // Answer message with "Hi".
                api.send(message.text_reply(format!(
                    "Hi, {}! You just wrote '{}'",
                    &message.from.first_name, data
                )))
                .await?;
            }
        }
    }
    Ok(())
}

async fn dispatch_command(
    api: &Api,
    message: &Message,
    chat_id: &ChatId,
    tokens: Split<'_, &str>,
    cmd: String,
) -> Result<()> {
    info!("{:?}", cmd);
    match cmd.as_ref() {
        "/remind" => set_reminder(&chat_id, tokens).await,
        "/remind_state" => {
            api.send(message.text_reply(format!(
                "Current state is: ```{:?}```",
                &CHATS.lock().await.reminders.get(&chat_id)
            )))
            .await?;
        }
        "/remind_help" => {
            api.send(
                message.text_reply("/remind <Weekday: 3 letters Mon, ...> HH:MM:SS".to_string()),
            )
            .await?;
        }
        _ => (),
    };
    Ok(())
}

async fn set_reminder(chat_id: &ChatId, tokens: Split<'_, &str>) {
    match parse_request(tokens) {
        Ok((week_day, time, msg)) => {
            info!("Updating");
            let mut chats = CHATS.lock().await;

            let chat_reminds = chats.reminders.entry(*chat_id).or_insert(vec![]);
            chat_reminds.push(Timer {
                name: "".to_string(),
                msg,
                week_day,
                time,
                last_time: None,
            });
            let _err = update_file(DATA_PATH, &*chats);
            info!("{:?}", _err);
            info!("Updated chats {:?}", chats.reminders.get(&chat_id));
        }
        Err(err) => {
            dbg!(err);
        }
    }
}
