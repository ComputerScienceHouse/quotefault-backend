use actix_web::{
    post,
    web::{Data, Json},
    HttpResponse, Responder,
};
use log::{log, Level};
use sqlx::{query, query_as};

use crate::{
    api::db::{log_query, log_query_as, open_transaction},
    app::AppState,
    schema::api::NewQuote,
    schema::db::ID,
};

#[post("/quote")]
pub async fn create_quote(state: Data<AppState>, body: Json<NewQuote>) -> impl Responder {
    log!(Level::Info, "POST /api/quote");

    if body.shards.is_empty() {
        return HttpResponse::BadRequest().body("No quote shards specified");
    }
    if body.shards.len() > 50 {
        return HttpResponse::BadRequest().body("Maximum of 50 shards exceeded.");
    }
    if body.shards.iter().any(|s| s.speaker.len() > 32) {
        return HttpResponse::BadRequest().body("Maximum speaker name length exceeded.");
    }
    if body.submitter.len() > 32 {
        return HttpResponse::BadRequest().body("Maximum submitter name length exceeded.");
    }

    let mut transaction = match open_transaction(&state.db).await {
        Ok(t) => t,
        Err(res) => return res,
    };

    let id: i32;
    match log_query_as(
        query_as!(
            ID,
            "INSERT INTO quotes(submitter) VALUES ($1) RETURNING id",
            body.submitter
        )
        .fetch_all(&state.db)
        .await,
        Some(transaction),
    )
    .await
    {
        Ok((tx, i)) => {
            transaction = tx.unwrap();
            id = i[0].id;
        }
        Err(res) => return res,
    }
    log!(Level::Trace, "created a new entry in quote table");

    let ids: Vec<i32> = vec![id; body.shards.len()];
    let indices: Vec<i16> = (1..=body.shards.len()).map(|a| a as i16).collect();
    let bodies: Vec<String> = body.shards.iter().map(|s| s.body.clone()).collect();
    let speakers: Vec<String> = body.shards.iter().map(|s| s.speaker.clone()).collect();

    match log_query(
        query!("INSERT INTO Shards (quote_id, index, body, speaker) SELECT quote_id, index, body, speaker FROM UNNEST($1::int4[], $2::int2[], $3::text[], $4::varchar[]) as a(quote_id, index, body, speaker)", ids.as_slice(), indices.as_slice(), bodies.as_slice(), speakers.as_slice())
        .execute(&state.db)
        .await
        .map(|_| ()), Some(transaction)).await {
        Ok(tx) => transaction = tx.unwrap(),
        Err(res) => return res,
    }

    match transaction.commit().await {
        Ok(_) => HttpResponse::Ok().body(""),
        Err(e) => {
            log!(Level::Error, "Transaction failed to commit");
            HttpResponse::InternalServerError().body(e.to_string())
        }
    }
}
