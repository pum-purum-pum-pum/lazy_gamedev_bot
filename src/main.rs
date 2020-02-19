use std::env;
use hyper::{Client, body::HttpBody as _};
use hyper_tls::{HttpsConnector};
use tokio::io::{self, AsyncWriteExt as _};
use once_cell::sync::Lazy;

static TOKEN: Lazy<String> = Lazy::new(|| {
	env::var("LAZY_TOKEN").expect("token not set")
});

const BASE_URL: &'static str = "https://api.telegram.org/bot";

fn request_url(request: &str) -> String {
	format!("{}{}/{}", BASE_URL, *TOKEN, request)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	// let token = env::var("LAZY_TOKEN").expect("TELEGRAM_BOT_TOKEN not set");
	let get_me = request_url("getMe");
	let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);
    let mut res = client.get(get_me.parse()?).await?;
    println!("Status: {}", res.status());
    println!("Headers:\n{:#?}", res.headers());
    while let Some(chunk) = res.body_mut().data().await {
        let chunk = chunk?;
        io::stdout()
            .write_all(&chunk)
            .await?
    }
    Ok(())
}