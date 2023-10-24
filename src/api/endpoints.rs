use actix_web::{
    get, post,
    web::{self, Data, Json},
    HttpResponse, Responder,
};
use log::{log, Level};
use sqlx::{query, query_as};

use crate::{
    api::db::{log_query, log_query_as, open_transaction},
    app::AppState,
    auth::{CSHAuth, User},
    ldap,
    schema::api::{FetchParams, NewQuote, QuoteResponse, QuoteShardResponse},
    schema::{
        api::UserResponse,
        db::{QuoteShard, ID},
    },
    utils::is_valid_username,
};

#[post("/quote", wrap = "CSHAuth::enabled()")]
pub async fn create_quote(
    state: Data<AppState>,
    body: Json<NewQuote>,
    user: User,
) -> impl Responder {
    log!(Level::Info, "POST /api/quote");

    if body.shards.is_empty() {
        return HttpResponse::BadRequest().body("No quote shards specified");
    }
    if body.shards.len() > 50 {
        return HttpResponse::BadRequest().body("Maximum of 50 shards exceeded.");
    }
    if body
        .shards
        .iter()
        .any(|x| !is_valid_username(x.speaker.as_str()))
    {
        return HttpResponse::BadRequest().body("Invalid speaker username format specified.");
    }
    if !is_valid_username(user.preferred_username.as_str()) {
        return HttpResponse::BadRequest()
            .body("Invalid submitter username specified. SHOULD NEVER HAPPEN!");
    }
    let mut users: Vec<String> = body.shards.iter().map(|x| x.speaker.clone()).collect();
    users.push(user.preferred_username.clone());
    match ldap::users_exist(&state.ldap, users).await {
        Ok(exists) => {
            if !exists {
                return HttpResponse::BadRequest().body("Some users submitted do not exist.");
            }
        }
        Err(err) => return HttpResponse::InternalServerError().body(err.to_string()),
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
            user.preferred_username
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

    log!(Level::Trace, "created quote shards");

    match transaction.commit().await {
        Ok(_) => HttpResponse::Ok().body(""),
        Err(e) => {
            log!(Level::Error, "Transaction failed to commit");
            HttpResponse::InternalServerError().body(e.to_string())
        }
    }
}

#[get("/quotes", wrap = "CSHAuth::enabled()")]
pub async fn get_quotes(state: Data<AppState>, params: web::Query<FetchParams>) -> impl Responder {
    let limit: i64 = params.limit.unwrap_or(10).into();
    let offset: i64 = params.offset.unwrap_or(0).into();
    let query: String = format!(
        "%{}%",
        params.q.clone().unwrap_or(String::new()).to_lowercase()
    );
    let speaker = params.speaker.clone().unwrap_or("%".to_string());
    let submitter = params.submitter.clone().unwrap_or("%".to_string());
    match log_query_as(
        query_as!(
            QuoteShard,
            "SELECT pq.id as \"id!\", s.index as \"index!\", pq.submitter as \"submitter!\",
            pq.timestamp as \"timestamp!\", s.body as \"body!\", s.speaker as \"speaker!\"
            FROM (
                SELECT * FROM quotes q
                WHERE NOT hidden
                AND submitter LIKE $5
                AND q.id IN (
                    SELECT quote_id FROM shards s
                    WHERE LOWER(body) LIKE $3
                    AND speaker LIKE $4
                )
                ORDER BY q.id DESC
                LIMIT $1
                OFFSET $2
            ) AS pq
            LEFT JOIN shards s ON s.quote_id = pq.id
            ORDER BY pq.id DESC, s.index",
            limit,
            offset,
            query,
            speaker,
            submitter,
        )
        .fetch_all(&state.db)
        .await,
        None,
    )
    .await
    {
        Ok((_, shards)) => {
            let mut quotes: Vec<QuoteResponse> = Vec::new();
            for shard in shards {
                if shard.index == 1 {
                    quotes.push(QuoteResponse {
                        id: shard.id,
                        shards: vec![QuoteShardResponse {
                            body: shard.body,
                            speaker: shard.speaker,
                        }],
                        submitter: shard.submitter,
                        timestamp: shard.timestamp,
                    });
                } else {
                    quotes.last_mut().unwrap().shards.push(QuoteShardResponse {
                        body: shard.body,
                        speaker: shard.speaker,
                    });
                }
            }
            HttpResponse::Ok().json(quotes)
        }
        Err(res) => res,
    }
}

#[get("/users", wrap = "CSHAuth::enabled()")]
pub async fn get_users(state: Data<AppState>) -> impl Responder {
    match ldap::get_group_members(&state.ldap, "member").await {
        Ok(users) => HttpResponse::Ok().json(
            users
                .into_iter()
                .map(|x| UserResponse {
                    uid: x.uid,
                    cn: x.cn,
                })
                .collect::<Vec<_>>(),
        ),
        Err(err) => HttpResponse::InternalServerError().body(err.to_string()),
    }
}
