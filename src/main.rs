use actix_web::{get, web, App, HttpServer, Responder, middleware::Logger};

#[get("/hello/{name}")]
async fn greet(name: web::Path<String>) -> impl Responder {
    format!("Hello {name}!")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv::dotenv().ok();
    env_logger::init();

    HttpServer::new(|| {
        App::new()
            .wrap(Logger::default())
            .service(greet)
    })
    .bind(("127.0.0.1", 8286))?
    .run()
    .await
}
