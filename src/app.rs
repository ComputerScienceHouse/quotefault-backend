use std::env;

use actix_web::web::{self, scope, Data};
use sqlx::{postgres::PgPoolOptions, Pool, Postgres};

use crate::api::create_quote;

#[derive(Debug)]
pub struct AppState {
    pub db: Pool<Postgres>,
}

pub fn configure_app(cfg: &mut web::ServiceConfig) {
    cfg.service(scope("/api").service(create_quote));
}

pub async fn get_app_data() -> Data<AppState> {
    let pool = PgPoolOptions::new()
        .connect(&env::var("DATABASE_URL").expect("DATABASE_URL not set"))
        .await
        .expect("Could not connect to database");
    println!("Successfully connected to database! :)");
    Data::new(AppState { db: pool })
}