use actix_web::{get, App, HttpServer, Responder, middleware::Logger};
use std::env;
use std::error::Error;
use rss::Channel;
use scraper::{Html, Selector};
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct Deal {
    price: String,
    discount: String,
    store: String,
    link: String,
}

#[derive(Debug, Clone)]
struct GameDeals {
    name: String,
    historical_low: String,
    deals: Vec<Deal>,
}

async fn get_rss_feed() -> Result<Channel, Box<dyn Error>> {
    let feed_url = format!(
        "https://isthereanydeal.com/feeds/waitlist.rss?token={}",
        env::var("WAITLIST_RSS_TOKEN").unwrap()
    );
    let bytes = reqwest::get(&feed_url)
        .await?
        .bytes()
        .await?;

    let channel = Channel::read_from(&bytes[..])?;
    Ok(channel)
}

#[get("/")]
async fn index() -> impl Responder {
    match get_rss_feed().await {
        Ok(channel) => {
            let mut html = String::new();
            html.push_str("<html><head><title>Waitlist RSS Feed</title></head><body>");
            html.push_str("<h1>Waitlist RSS Feed</h1><ul>");
            for item in channel.items() {
                let title = item.title().unwrap_or("No title");
                let description = item.description().unwrap_or("No description");
                html.push_str(&format!("<li><strong>{}</strong><br/>{}</li>", title, description));
            }
            html.push_str("</ul></body></html>");
            
            actix_web::HttpResponse::Ok()
                .content_type("text/html; charset=utf-8")
                .body(html)
        },
        Err(e) => {
            eprintln!("Error fetching RSS feed: {}", e);
            actix_web::HttpResponse::InternalServerError()
                .content_type("text/plain; charset=utf-8")
                .body("Failed to fetch RSS feed")
        }
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv::dotenv().ok();
    if let Err(_) = env::var("WAITLIST_RSS_TOKEN") {
        panic!("WAITLIST_RSS_TOKEN must be set in the environment");
    }
    
    env_logger::init();
    HttpServer::new(|| {
        App::new()
            .wrap(Logger::default())
            .service(index)
    })
    .bind(("127.0.0.1", 8286))?
    .run()
    .await
}
