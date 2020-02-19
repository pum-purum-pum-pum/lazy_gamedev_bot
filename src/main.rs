#[macro_use]
extern crate log;

use std::env;
use std::collections::HashMap;
use std::fs::File;
use std::io::prelude::*;
use std::str::Split;

use env_logger::Builder;
use log::LevelFilter;
use ron::ser::{to_string_pretty, PrettyConfig};
use ron::de::from_str;
use serde::{Serialize, Deserialize};
use futures::StreamExt;
use telegram_bot::*;
use telegram_bot::types::refs::{GroupId, SupergroupId, UserId};
use telegram_bot::types::chat::MessageChat;
use chrono::{DateTime, Datelike, Timelike, Utc, NaiveDate, NaiveTime, Weekday};

const MOSCOW_OFFSET: usize = 3;
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum ChatId {
	SG(SupergroupId),
	U(UserId),
	G(GroupId)
}


#[derive(Debug, Serialize, Deserialize)]
struct Timer {
	pub name: String,
	pub msg: String,
	pub week_day: Weekday,
	pub time: NaiveTime,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Chats {
	pub reminders: HashMap<ChatId, Vec<Timer>>
}

fn parse_request(mut tokens: Split<'_, &str>) -> Result<(Weekday, NaiveTime)> {
    // let mut tokens = input.split(" ");
    let token = tokens.next().ok_or("no dayweek token")?;
    info!("{}", &token);
    let day_week: Weekday = from_str(&format!("\"{}\"", token))?;
    info!("{:?}", day_week);
    let token = tokens.next().ok_or("no time token")?;
	info!("{}", &token);
    let time = NaiveTime::parse_from_str(token, "%H:%M:%S")?;
    Ok((day_week, time))
}

fn update_file(data_path: &str, chats: &Chats) -> Result<()> {
    let pretty = PrettyConfig {
        depth_limit: 2,
        separate_tuple_members: true,
        enumerate_arrays: true,
        ..PrettyConfig::default()
    };
	let mut file = File::open(data_path)?;
	file.write_all(to_string_pretty(chats, pretty)?.as_bytes())?;
	Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
	let mut builder = Builder::new();
    builder.filter_level(LevelFilter::Info).init();
    let pretty = PrettyConfig {
        depth_limit: 2,
        separate_tuple_members: true,
        enumerate_arrays: true,
        ..PrettyConfig::default()
    };
    // dbg!(to_string_pretty(&Weekday::Mon, pretty));
	// return Ok(());
	// let now = Utc::now();
	// let (is_pm, hour) = now.hour12();
    // println!(
    //     "The current UTC time is {:02}:{:02}:{:02} {}",
    //     hour,
    //     now.minute(),
    //     now.second(),
    //     if is_pm { "PM" } else { "AM" }
    // );

    let data_path = "chats.ron";
    let mut chats = if let Ok(mut file) = File::open(data_path) {
    	let mut data = String::new();
    	file.read_to_string(&mut data)?;
    	from_str(&data)?
    } else {
    	let mut file = File::create(data_path)?;
    	let chats = Chats::default();
    	file.write_all(to_string_pretty(&chats, pretty)?.as_bytes())?;
    	chats
    };
    let token = env::var("LAZY_TOKEN").expect("token not set");
    let api = Api::new(token);

    // Fetch new updates via long poll method
    let mut stream = api.stream();
    // api.send(message.text_reply("hello".to_string()));
    while let Some(update) = stream.next().await {
        // If the received update contains a new message...
        let update = update?;
        if let UpdateKind::Message(message) = update.kind {
            if let MessageKind::Text { ref data, .. } = message.kind {
                // Print received text message to stdout.
                let chat_id = match &message.chat {
                	MessageChat::Private(user) => {
                		ChatId::U(user.id)
                	}
                	MessageChat::Supergroup(group) => {
                		ChatId::SG(group.id)
                	}
                	MessageChat::Group(group) => {
                		ChatId::G(group.id)
                	}
                	_ => continue
                };
                // println!("<{}>: {}", &message.from.first_name, data);
                let mut tokens = data.split(" ");
                if let Some(cmd) = tokens.next() { // first token
                	dbg!(&cmd);
                	if cmd == "/remind" { // set reminder
		                match parse_request(tokens) {
		                	Ok((week_day, time)) => {
			                	let chat_reminds = chats.reminders.entry(chat_id).or_insert(vec![]);
			                	chat_reminds.push(Timer {
			                		name: "".to_string(),
			                		msg: "".to_string(),
			                		week_day,
			                		time
			                	});
			                	// dbg!(&chats);
			                	let _err = update_file(data_path, &chats);
		                	}
		                	Err(err) => {
		                		dbg!(err);
		                	}
		                }
                	}
                } else {continue;}
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