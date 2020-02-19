use std::env;
use std::collections::HashMap;
use std::fs::File;
use std::io::prelude::*;


use ron::ser::{to_string_pretty, PrettyConfig};
use ron::de::from_str;
use serde::{Serialize, Deserialize};
use futures::StreamExt;
use telegram_bot::*;
use telegram_bot::types::refs::{SupergroupId, UserId};
use chrono::{DateTime, Datelike, Timelike, Utc, NaiveDate};

const MOSCOW_OFFSET: usize = 3;
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum ChatId {
	SG(SupergroupId),
	U(UserId)
}


#[derive(Debug, Serialize, Deserialize)]
struct Timer {
	name: String,
	msg: String,
	time: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Chats {
	chats_reminders: HashMap<ChatId, Timer>
}

#[tokio::main]
async fn main() -> Result<()> {
	let now = Utc::now();
	let (is_pm, hour) = now.hour12();
    println!(
        "The current UTC time is {:02}:{:02}:{:02} {}",
        hour,
        now.minute(),
        now.second(),
        if is_pm { "PM" } else { "AM" }
    );

    let data_path = "chats.ron";
    let chats = if let Ok(mut file) = File::open(data_path) {
    	let mut data = String::new();
    	file.read_to_string(&mut data)?;
    	from_str(&data)?
    } else {
    	let mut file = File::create(data_path)?;
    	let chats = Chats::default();
        let pretty = PrettyConfig {
            depth_limit: 2,
            separate_tuple_members: true,
            enumerate_arrays: true,
            ..PrettyConfig::default()
        };
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
                println!("{:?}", &message);
                println!("<{}>: {}", &message.from.first_name, data);

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