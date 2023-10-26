use std::collections::{BTreeSet, HashMap};

use actix_web::{
    get, post,
    web::{self, Data, Json, Path},
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

async fn shards_to_quotes(
    shards: &[QuoteShard],
    ldap: &ldap::client::LdapClient,
) -> Result<Vec<QuoteResponse>, HttpResponse> {
    let mut uid_map: HashMap<String, Option<String>> = HashMap::new();
    shards.iter().for_each(|x| {
        let _ = uid_map.insert(x.submitter.clone(), None);
        let _ = uid_map.insert(x.speaker.clone(), None);
    });
    match ldap::get_users(
        ldap,
        uid_map.keys().cloned().collect::<Vec<String>>().as_slice(),
    )
    .await
    {
        Ok(users) => users.into_iter().for_each(|x| {
            let _ = uid_map.insert(x.uid, Some(x.cn));
        }),
        Err(err) => return Err(HttpResponse::InternalServerError().body(err.to_string())),
    }

    let mut quotes: Vec<QuoteResponse> = Vec::new();
    for shard in shards {
        let speaker = match uid_map.get(&shard.speaker).cloned().unwrap() {
            Some(cn) => UserResponse {
                uid: shard.speaker.clone(),
                cn,
            },
            None => continue,
        };
        if shard.index == 1 {
            let submitter = match uid_map.get(&shard.submitter).cloned().unwrap() {
                Some(cn) => UserResponse {
                    uid: shard.submitter.clone(),
                    cn,
                },
                None => continue,
            };
            quotes.push(QuoteResponse {
                id: shard.id,
                shards: vec![QuoteShardResponse {
                    body: shard.body.clone(),
                    speaker,
                }],
                timestamp: shard.timestamp,
                submitter,
            });
        } else {
            quotes.last_mut().unwrap().shards.push(QuoteShardResponse {
                body: shard.body.clone(),
                speaker,
            });
        }
    }
    Ok(quotes)
}

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
    match ldap::users_exist(&state.ldap, BTreeSet::from_iter(users.into_iter())).await {
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

#[get("/quotes/{id}", wrap = "CSHAuth::enabled()")]
pub async fn get_quote(state: Data<AppState>, path: Path<(String,)>) -> impl Responder {
    let (id,) = path.into_inner();
    let id: i32 = match id.parse() {
        Ok(id) => id,
        Err(_e) => {
            log!(Level::Warn, "Invalid id");
            return HttpResponse::BadRequest().body("Invalid id");
        }
    };

    match log_query_as(
        query_as!(
            QuoteShard,
            "SELECT pq.id as \"id!\", s.index as \"index!\", pq.submitter as \"submitter!\",
            pq.timestamp as \"timestamp!\", s.body as \"body!\", s.speaker as \"speaker!\"
            FROM (
                SELECT * FROM quotes q WHERE q.id = $1
            ) AS pq
            LEFT JOIN shards s ON s.quote_id = pq.id",
            id,
        )
        .fetch_all(&state.db)
        .await,
        None,
    )
    .await
    {
        Ok((_, shards)) => {
            if shards.is_empty() {
                HttpResponse::NotFound().body("Quote could not be found")
            } else {
                match shards_to_quotes(shards.as_slice(), &state.ldap).await {
                    Ok(quotes) => HttpResponse::Ok().json(quotes),
                    Err(res) => res,
                }
            }
        }
        Err(res) => res,
    }
}

#[get("/quotes", wrap = "CSHAuth::enabled()")]
pub async fn get_quotes(state: Data<AppState>, params: web::Query<FetchParams>) -> impl Responder {
    let limit: i64 = params.limit.unwrap_or(10).into();
    let lt_qid: i32 = params.lt.unwrap_or(0);
    let query: String = params
        .q
        .clone()
        .map(|x| format!("%({})%", (x.replace(' ', "|"))))
        .unwrap_or("%%".into());
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
                AND CASE WHEN $2::int4 > 0 THEN q.id < $2::int4 ELSE true END
                AND submitter LIKE $5
                AND q.id IN (
                    SELECT quote_id FROM shards s
                    WHERE LOWER(body) SIMILAR TO LOWER($3)
                    AND speaker LIKE $4
                )
                ORDER BY q.id DESC
                LIMIT $1
            ) AS pq
            LEFT JOIN shards s ON s.quote_id = pq.id
            ORDER BY timestamp DESC, pq.id DESC, s.index",
            limit,
            lt_qid,
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
        Ok((_, shards)) => match shards_to_quotes(shards.as_slice(), &state.ldap).await {
            Ok(quotes) => HttpResponse::Ok().json(quotes),
            Err(response) => response,
        },
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
