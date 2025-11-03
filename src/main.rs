use actix_web::{get, App, HttpServer, Responder, middleware::Logger};
use std::env;

#[get("/")]
async fn index() -> impl Responder {
    let feed_url = format!(
        "https://isthereanydeal.com/feeds/waitlist.rss?token={}",
        env::var("WAITLIST_RSS_TOKEN").unwrap()
    );
    format!("Waitlist RSS Feed URL: {feed_url}")
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
