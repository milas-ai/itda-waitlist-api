use actix_web::{get, App, HttpServer, Responder, middleware::Logger};
use scraper::{Html, Selector};
use std::collections::HashMap;
use std::error::Error;
use rss::Channel;
use std::env;

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

fn parse_game_deals(html_content: &str) -> Result<Vec<GameDeals>, Box<dyn Error>> {
    let document = Html::parse_document(html_content);
    let mut game_deals = Vec::new();

    let game_block_selector = Selector::parse("div[style*='margin-bottom:30px']").expect("Failed to create selector");
    let game_name_selector = Selector::parse("a[style*='font-size:1.2em']").expect("Failed to create selector");
    let historical_low_selector = Selector::parse("div[style*='font-size: 0.9em']").expect("Failed to create selector");
    let deal_row_selector = Selector::parse("div[style*='padding-left:15px'] > div").expect("Failed to create selector");
    let price_link_selector = Selector::parse("a[style*='font-size:1.1em']").expect("Failed to create selector");
    let discount_selector = Selector::parse("span[style*='min-width:2.8em']").expect("Failed to create selector");

    for game_block in document.select(&game_block_selector) {
        let name = game_block.select(&game_name_selector)
            .next()
            .map(|e| e.text().collect::<String>())
            .unwrap_or_else(|| "Unknown Game".to_string());

        let historical_low = game_block.select(&historical_low_selector)
            .next()
            .map(|e| e.text().collect::<String>())
            .unwrap_or_else(|| "".to_string())
            .replace("Historical low: ", "")
            .trim()
            .to_string();

        let mut deals = Vec::new();
        for deal_element in game_block.select(&deal_row_selector) {
            let price_link_element = deal_element.select(&price_link_selector).next();

            let price = price_link_element
                .map(|e| e.text().collect::<String>().trim().to_string())
                .unwrap_or_else(|| "N/A".to_string());

            let link = price_link_element
                .map(|e| e.value().attr("href").unwrap_or("#").to_string())
                .unwrap_or_else(|| "#".to_string());

            let discount = deal_element.select(&discount_selector)
                .next()
                .map(|e| e.text().collect::<String>().trim().to_string())
                .unwrap_or_else(|| "N/A".to_string());

            let store_raw: Vec<String> = deal_element.text()
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect();

            let mut store = "Unknown Store".to_string();
            if let Some(on_index) = store_raw.iter().position(|s| s == "on") {
                if let Some(store_name) = store_raw.get(on_index + 1) {
                    store = store_name.clone();
                }
            }

            deals.push(Deal { price, discount, store, link });
        }

        game_deals.push(GameDeals { name, historical_low, deals });
    }
    Ok(game_deals)
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
            let mut games_map: HashMap<String, GameDeals> = HashMap::new();

            for item in channel.items().iter().take(2) {
                if let Some(description) = item.description() {
                    if let Ok(parsed_games) = parse_game_deals(description) {
                        for game in parsed_games {
                            games_map
                                .entry(game.name.clone())
                                .or_insert_with(|| GameDeals {
                                    name: game.name.clone(),
                                    historical_low: game.historical_low.clone(),
                                    deals: Vec::new(),
                                })
                                .deals.extend(game.deals);
                        }
                    }
                }
            }

            let mut html = String::new();
            html.push_str("<html><head><title>Waitlist RSS Feed</title>");
            html.push_str("<style>
                body { font-family: sans-serif; line-height: 1.6; }
                .game { border: 1px solid #ccc; border-radius: 8px; margin: 15px; padding: 15px; }
                .game h2 { margin-top: 0; }
                ul { padding-left: 20px; }
                li { margin-bottom: 5px; }
            </style>");
            html.push_str("</head><body>");
            html.push_str("<h1>Waitlist RSS Feed</h1>");
            for game in games_map.values() {
                html.push_str("<div class='game'>");
                html.push_str(&format!("<h2>{}</h2>", game.name));
                html.push_str(&format!("<p><strong>Historical Low:</strong> {}</p>", game.historical_low));
                
                if game.deals.is_empty() {
                    html.push_str("<p>No current deals available.</p>");
                } else {
                    html.push_str("<ul>");
                    for deal in &game.deals {
                        html.push_str(&format!(
                            "<li><a href='{}'>{}</a> ({}) - <strong>{}</strong></li>",
                            deal.link, deal.price, deal.discount, deal.store
                        ));
                    }
                    html.push_str("</ul>");
                }
                html.push_str("</div>");
            }
            html.push_str("</body></html>");

            actix_web::HttpResponse::Ok()
                .content_type("text/html; charset=utf-8")
                .body(html)
        },
        Err(e) => {
            eprintln!("Error fetching RSS feed: {}", e);
            actix_web::HttpResponse::InternalServerError().body("Failed to fetch RSS feed")
        }
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv::dotenv().ok();
    if let Err(_) = env::var("WAITLIST_RSS_TOKEN") {
        panic!("WAITLIST_RSS_TOKEN must be set in the environment");
    }

    let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = env::var("PORT").unwrap_or_else(|_| "8080".to_string())
        .parse::<u16>()
        .expect("PORT must be a valid u16 number");
    
    env_logger::init();
    println!("Starting server at http://{}:{}", host, port);
    HttpServer::new(|| {
        App::new()
            .wrap(Logger::default())
            .service(index)
    })
    .bind((host, port))?
    .run()
    .await
}
