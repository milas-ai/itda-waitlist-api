use actix_web::{get, web, App, HttpServer, Responder, middleware::Logger};
use scraper::{Html, Selector};
use std::collections::HashMap;
use serde::Deserialize;
use std::error::Error;
use rss::Channel;
use std::env;
use std::f32;

#[derive(Deserialize)]
struct QueryParams {
    #[serde(rename = "collapse-after")]
    collapse_after: Option<String>,
    #[serde(rename = "show-thumbnails")]
    show_thumbnails: Option<String>,
}

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
    image_url: String,
    historical_low: String,
    deals: Vec<Deal>,
}

fn parse_price_to_f32(price_str: &str) -> Option<f32> {
    price_str
        .replace("R$", "")
        .replace(".", "")
        .replace(",", ".")
        .trim()
        .parse::<f32>()
        .ok()
}

async fn get_image_url_from_metadata(info_url: &str) -> Option<String> {
    let html_content = reqwest::get(info_url).await.ok()?.bytes().await.ok()?;
    let document = Html::parse_document(std::str::from_utf8(&html_content).ok()?);
    let meta_selector = Selector::parse("meta[property='og:image']").ok()?;

    let image_url = document.select(&meta_selector)
        .next()?
        .value()
        .attr("content")?;

    Some(image_url.to_string())
}

async fn parse_game_deals(html_content: &str) -> Result<Vec<GameDeals>, Box<dyn Error>> {
    let document = Html::parse_document(html_content);
    let mut game_deals = Vec::new();

    let game_block_selector = Selector::parse("div[style*='margin-bottom:30px']").expect("Failed to create selector");
    let name_selector = Selector::parse("a[style*='font-size:1.2em']").expect("Failed to create selector");
    let historical_low_selector = Selector::parse("div[style*='font-size: 0.9em']").expect("Failed to create selector");
    let deal_row_selector = Selector::parse("div[style*='padding-left:15px'] > div").expect("Failed to create selector");
    let price_link_selector = Selector::parse("a[style*='font-size:1.1em']").expect("Failed to create selector");
    let discount_selector = Selector::parse("span[style*='min-width:2.8em']").expect("Failed to create selector");

    for game_block in document.select(&game_block_selector) {
        let name_element = game_block.select(&name_selector).next();

        let name = name_element
            .map(|e| e.text().collect::<String>())
            .unwrap_or_else(|| "Unknown Game".to_string());

        let info_url = name_element
            .and_then(|e| e.value().attr("href"))
            .unwrap_or("");

        let image_url = if info_url.is_empty() {
            None
        } else {
            get_image_url_from_metadata(info_url).await
        };

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

        game_deals.push(GameDeals {
            name,
            image_url: image_url.unwrap_or_else(|| "https://i.imgur.com/v4ChE6O.jpeg".to_string()),
            historical_low,
            deals,
        });
    }
    Ok(game_deals)
}

async fn get_rss_feed() -> Result<(Channel, String), Box<dyn Error>> {
    let feed_url = format!(
        "https://isthereanydeal.com/feeds/waitlist.rss?token={}",
        env::var("WAITLIST_RSS_TOKEN").unwrap()
    );
    let bytes = reqwest::get(&feed_url)
        .await?
        .bytes()
        .await?;

    let channel = Channel::read_from(&bytes[..])?;

    let title_url = channel.items()
        .first()
        .and_then(|item| item.link())
        .unwrap_or(&feed_url)
        .to_string();

    Ok((channel, title_url))
}

#[get("/")]
async fn index(query: web::Query<QueryParams>) -> impl Responder {
    match get_rss_feed().await {
        Ok((channel, title_url)) => {
            // Parameter handling
            let collapse_after = query.collapse_after.as_deref().unwrap_or("5");
            let show_thumbnails = query.show_thumbnails.as_deref().map(|s| s == "true").unwrap_or(true);

            // Parse game deals from the RSS feed
            let mut games_map: HashMap<String, GameDeals> = HashMap::new();
            for item in channel.items().iter().take(1) {
                if let Some(description) = item.description() {
                    if let Ok(parsed_games) = parse_game_deals(description).await {
                        for game in parsed_games {
                            games_map
                                .entry(game.name.clone())
                                .or_insert_with(|| GameDeals {
                                    name: game.name.clone(),
                                    image_url: game.image_url.clone(),
                                    historical_low: game.historical_low.clone(),
                                    deals: Vec::new(),
                                })
                                .deals.extend(game.deals);
                        }
                    }
                }
            }

            let mut games: Vec<GameDeals> = games_map.into_values().collect();

            games.sort_by(|a, b| {
                let min_price_a = a.deals.iter()
                    .filter_map(|d| parse_price_to_f32(&d.price))
                    .fold(f32::MAX, f32::min);

                let min_price_b = b.deals.iter()
                    .filter_map(|d| parse_price_to_f32(&d.price))
                    .fold(f32::MAX, f32::min);

                min_price_a.partial_cmp(&min_price_b).unwrap_or(std::cmp::Ordering::Equal)
            });

            let mut html = String::new();
            html.push_str("<html><head><title>Is There Any Deal? - Waitlist Notification</title>");
            html.push_str("<style>
                .game-container { display: flex; align-items: center; gap: 10px; }
                .game-container img { height: 80px; aspect-ratio: 16/9; border-radius: 4px; object-fit: cover; flex-shrink: 0; }
                .game-content { flex: 1; }
            </style>");
            html.push_str("</head><body>");

            html.push_str(&format!("<ul class='list list-gap-10 list-with-separator collapsible-container' data-collapse-after='{}'>", collapse_after));
            for game in games {
                html.push_str("<li class='game-container'>");
                if show_thumbnails {
                    html.push_str(&format!("<img src='{}' alt='{}' />", game.image_url, game.name));
                }
                html.push_str(&format!("<div class='game-content'><h2 class='color-highlight size-h2'>{}</h2>", game.name));
                html.push_str(&format!("<p class='color-subdued'><strong>Historical Low:</strong> {}</p>", game.historical_low));
                
                if game.deals.is_empty() {
                    html.push_str("<p class='size-h4 color-negative'>No current deals available.</p>");
                } else {
                    html.push_str("<ul>");
                    for deal in &game.deals {
                        html.push_str(&format!(
                            "<li class='size-h4'><a class='color-primary' href='{}'>{}</a> ({}) - {}</li>",
                            deal.link, deal.price, deal.discount, deal.store
                        ));
                    }
                    html.push_str("</ul>");
                }
                html.push_str("</div></li>");
            }
            html.push_str("</ul></body></html>");

            actix_web::HttpResponse::Ok()
                .content_type("text/html; charset=utf-8")
                .insert_header(("Widget-Title", "Is There Any Deal?"))
                .insert_header(("Widget-Title-URL", title_url))
                .insert_header(("Widget-Content-Type", "html"))
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
