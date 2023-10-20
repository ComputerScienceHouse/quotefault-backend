use actix_web::{
    post,
    web::{Data, Json},
    HttpResponse, Responder,
};

use crate::{app::AppState, schema::api::NewQuote};

#[post("/quote")]
pub async fn create_quote(state: Data<AppState>, body: Json<NewQuote>) -> impl Responder {
    println!("{:?}", body);
    println!("{:?}", state);
    HttpResponse::Ok()
}
