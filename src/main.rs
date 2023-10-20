use actix_web::{self, middleware::Logger, App, HttpServer};
use dotenv::dotenv;
use quotefault_backend::app::{configure_app, get_app_data};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();
    println!("Hello, world!");
    let app_data = get_app_data().await;
    HttpServer::new(move || {
        App::new()
            .wrap(Logger::new(
                "%a \"%r\" %s %b \"%{Referer}i\" \"%{User-Agent}i\" %T",
            ))
            .configure(configure_app)
            .app_data(app_data.clone())
    })
    .bind(("0.0.0.0", 3000))?
    .run()
    .await
}
