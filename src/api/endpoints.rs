use std::collections::{BTreeSet, HashMap};
use std::fmt::{self, Display};

use actix_web::body::MessageBody;
use actix_web::{
    delete, get,
    http::StatusCode,
    post, put,
    web::{self, Data, Json, Path},
    HttpResponse, Responder, ResponseError,
};
use log::{log, Level};
use sha3::{Digest, Sha3_256};
use sqlx::{query, query_as, query_file_as, Connection, Postgres, Transaction};

use crate::auth::{any_user_has_kevlar, clear_kevlar_cache, get_kevlar_users, toggle_kevlar_cache};
use crate::{
    api::{
        db::{log_query, log_query_as, open_transaction},
        pings::send_ping,
    },
    app::AppState,
    auth::{CSHAuth, User, SECURITY_ENABLED},
    ldap,
    schema::{
        api::{
            FetchParams, Hidden, NewQuote, QuoteResponse, QuoteShardResponse, Reason,
            ReportResponse, ReportedQuoteResponse, ResolveParams, UserResponse, VersionResponse,
            VoteParams,
        },
        db::{QuoteShard, ReportedQuoteShard, Vote, ID},
    },
    utils::is_valid_username,
};

async fn shards_to_quotes(
    shards: &[QuoteShard],
    ldap: &ldap::client::LdapClient,
) -> Result<Vec<QuoteResponse>, HttpResponse> {
    let mut uid_map: HashMap<String, Option<String>> = HashMap::new();
    shards.iter().for_each(|x| {
        uid_map.insert(x.submitter.clone(), None);
        uid_map.insert(x.speaker.clone(), None);
        if let Some(hidden_actor) = &x.hidden_actor {
            uid_map.insert(hidden_actor.clone(), None);
        }
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
            let hidden_actor = shard.hidden_actor.as_ref().and_then(|hidden_actor| {
                uid_map
                    .get(hidden_actor)
                    .cloned()
                    .unwrap()
                    .map(|cn| UserResponse {
                        uid: hidden_actor.clone(),
                        cn,
                    })
            });
            quotes.push(QuoteResponse {
                id: shard.id,
                shards: vec![QuoteShardResponse {
                    body: shard.body.clone(),
                    speaker,
                }],
                timestamp: shard.timestamp,
                score: shard.score,
                vote: shard.vote.clone(),
                submitter,
                hidden: hidden_actor.clone().and_then(|actor| {
                    Some(Hidden {
                        actor,
                        reason: shard.hidden_reason.clone()?,
                    })
                }),
                favorited: shard.favorited,
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

fn format_reports(quotes: &[ReportedQuoteShard]) -> Vec<ReportedQuoteResponse> {
    let mut reported_quotes: HashMap<i32, ReportedQuoteResponse> = HashMap::new();
    for quote in quotes {
        match reported_quotes.get_mut(&quote.quote_id) {
            Some(reported_quote) => reported_quote.reports.push(ReportResponse {
                reason: quote.report_reason.clone(),
                timestamp: quote.report_timestamp,
                id: quote.report_id,
            }),
            None => {
                let _ = reported_quotes.insert(
                    quote.quote_id,
                    ReportedQuoteResponse {
                        quote_id: quote.quote_id,
                        reports: vec![ReportResponse {
                            timestamp: quote.report_timestamp,
                            reason: quote.report_reason.clone(),
                            id: quote.report_id,
                        }],
                    },
                );
            }
        }
    }
    reported_quotes.into_values().collect()
}

impl ResponseError for SqlxErrorOrResponse<'_> {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::SqlxError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::ResponseOwned(status_code, _) | Self::Response(status_code, _) => *status_code,
        }
    }
    fn error_response(&self) -> HttpResponse {
        match self {
            Self::SqlxError(error) => {
                HttpResponse::InternalServerError().body(format!("SQLX Error: {error}"))
            }
            Self::Response(status_code, body) => {
                HttpResponse::with_body(*status_code, body.to_string().boxed())
            }
            Self::ResponseOwned(status_code, body) => {
                HttpResponse::with_body(*status_code, body.clone().boxed())
            }
        }
    }
}

impl Display for SqlxErrorOrResponse<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::SqlxError(error) => write!(f, "{error}"),
            Self::Response(status_code, error_message) => {
                write!(f, "{status_code}: {error_message}")
            }
            Self::ResponseOwned(status_code, error_message) => {
                write!(f, "{status_code}: {error_message}")
            }
        }
    }
}

#[derive(Debug)]
pub enum SqlxErrorOrResponse<'a> {
    SqlxError(sqlx::Error),
    Response(StatusCode, &'a str),
    ResponseOwned(StatusCode, String),
}

pub async fn hide_quote_by_id(
    id: i32,
    user: User,
    reason: String,
    transaction: &mut Transaction<'_, Postgres>,
) -> Result<(), SqlxErrorOrResponse<'static>> {
    let result = query!(
        "INSERT INTO public.hidden(quote_id, reason, actor)
            SELECT $1, $2, $3::varchar
            WHERE $1 IN (SELECT id FROM quotes)
                AND ($4 OR $1 IN (
                    SELECT quote_id FROM shards s
                    WHERE s.speaker = $3
                ))",
        id,
        reason,
        user.preferred_username,
        user.admin() || !*SECURITY_ENABLED,
    )
    .execute(&mut **transaction)
    .await?;
    if result.rows_affected() == 0 {
        Err(SqlxErrorOrResponse::Response(
            StatusCode::BAD_REQUEST,
            "Either you are not quoted in this quote or this quote does not exist.",
        ))
    } else {
        log!(Level::Trace, "hid quote");
        Ok(())
    }
}

/// Creates a quote
#[utoipa::path(
    post,
    path = "/api/quote",
    request_body=NewQuote,
    responses(
        (status = OK, description = "Quote created successfully"),
        (status = BAD_REQUEST, description = "Malformed request"),
        (status = UNAUTHORIZED, description = "Not authenticated"),
        (status = INTERNAL_SERVER_ERROR, description = "Internal Server Error")
    ),
)]
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
    if body.shards.len() > 6 {
        return HttpResponse::BadRequest().body("Maximum of 6 shards exceeded.");
    }
    for shard in &body.shards {
        if !is_valid_username(shard.speaker.as_str()) {
            return HttpResponse::BadRequest().body("Invalid speaker username format specified.");
        }
        if user.preferred_username == shard.speaker {
            return HttpResponse::BadRequest().body("Erm... maybe don't quote yourself?");
        }
    }
    if !is_valid_username(user.preferred_username.as_str()) {
        return HttpResponse::BadRequest()
            .body("Invalid submitter username specified. SHOULD NEVER HAPPEN!");
    }
    let mut users: Vec<String> = body.shards.iter().map(|x| x.speaker.clone()).collect();
    let kevlar = match any_user_has_kevlar(&state.db, users.as_slice()).await {
        Ok(b) => b,
        Err(e) => return HttpResponse::InternalServerError().body(e.to_string()),
    };
    if kevlar {
        return HttpResponse::BadRequest().body("One or more speakers cannot be quoted");
    }
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
        .fetch_all(&mut *transaction)
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
        query!(
            "INSERT INTO Shards (quote_id, index, body, speaker)
            SELECT quote_id, index, body, speaker
            FROM UNNEST($1::int4[], $2::int2[], $3::text[], $4::varchar[]) as a(quote_id, index, body, speaker)",
            ids.as_slice(),
            indices.as_slice(),
            bodies.as_slice(),
            speakers.as_slice()
        )
        .execute(&mut *transaction)
        .await, Some(transaction)).await {
        Ok((tx, _)) => transaction = tx.unwrap(),
        Err(res) => return res,
    }

    log!(Level::Trace, "created quote shards");

    match transaction.commit().await {
        Ok(_) => {
            for shard in &body.shards {
                if let Err(err) = send_ping(
                    shard.speaker.clone(),
                    format!(
                        "You were quoted by {}. Check it out at Quotefault!",
                        user.preferred_username
                    ),
                ) {
                    log!(Level::Error, "Failed to ping: {}", err);
                }
            }
            HttpResponse::Ok().body("")
        }
        Err(e) => {
            log!(Level::Error, "Transaction failed to commit");
            HttpResponse::InternalServerError().body(e.to_string())
        }
    }
}

/// Deletes a quote
#[utoipa::path(
    delete,
    path = "/api/quote/{id}",
    params(
        ("id" = i32, Path, description = "ID of quote to delete")
    ),
    responses(
        (status = OK, description = "Quote deleted successfully"),
        (status = BAD_REQUEST, description = "Either requester does not own the quote or the quote does not exist"),
        (status = UNAUTHORIZED, description = "Not authenticated"),
        (status = INTERNAL_SERVER_ERROR, description = "Internal Server Error")
    ),
)]
#[delete("/quote/{id}", wrap = "CSHAuth::enabled()")]
pub async fn delete_quote(state: Data<AppState>, path: Path<(i32,)>, user: User) -> impl Responder {
    let (id,) = path.into_inner();

    let mut transaction = match open_transaction(&state.db).await {
        Ok(t) => t,
        Err(res) => return res,
    };

    match log_query(
        query!(
            "DELETE FROM quotes WHERE id = $1 AND submitter = $2",
            id,
            user.preferred_username
        )
        .execute(&mut *transaction)
        .await,
        Some(transaction),
    )
    .await
    {
        Ok((tx, result)) => {
            if result.rows_affected() == 0 {
                return HttpResponse::BadRequest()
                    .body("Either this is not your quote or this quote does not exist.");
            }
            transaction = tx.unwrap()
        }
        Err(res) => return res,
    }

    log!(Level::Trace, "deleted quote and all shards");

    match transaction.commit().await {
        Ok(_) => HttpResponse::Ok().body(""),
        Err(e) => {
            log!(Level::Error, "Transaction failed to commit");
            HttpResponse::InternalServerError().body(e.to_string())
        }
    }
}

/// Hides a quote
#[utoipa::path(
    put,
    path = "/api/quote/{id}/hide",
    params(
        ("id" = i32, Path, description = "ID of quote to hide"),
    ),
    request_body = Reason,
    responses(
        (status = OK, description = "Quote hidden successfully"),
        (status = BAD_REQUEST, description = "Either requester was not quoted or the quote does not exist"),
        (status = UNAUTHORIZED, description = "Not authenticated"),
        (status = INTERNAL_SERVER_ERROR, description = "Internal Server Error")
    ),
)]
#[put("/quote/{id}/hide", wrap = "CSHAuth::enabled()")]
pub async fn hide_quote(
    state: Data<AppState>,
    path: Path<(i32,)>,
    user: User,
    Json(reason): Json<Reason>,
) -> Result<HttpResponse, SqlxErrorOrResponse<'static>> {
    let (id,) = path.into_inner();

    let reason = if reason.reason.is_empty() {
        "No reason given".to_string()
    } else {
        reason.reason
    };

    state
        .db
        .acquire()
        .await?
        .transaction(|transaction| {
            Box::pin(async move { hide_quote_by_id(id, user, reason, transaction).await })
        })
        .await?;
    Ok(HttpResponse::Ok().body(""))
}

/// Reports a quote
#[utoipa::path(
    post,
    path = "/api/quote/{id}/report",
    params(
        ("id" = i32, Path, description = "ID of quote to report")
    ),
    responses(
        (status = OK, description = "Quote reported successfully"),
        (status = BAD_REQUEST, description = "Quote already reported by the same user or the quote doesn't exist"),
        (status = UNAUTHORIZED, description = "Not authenticated"),
        (status = INTERNAL_SERVER_ERROR, description = "Internal Server Error")
    ),
)]
#[post("/quote/{id}/report", wrap = "CSHAuth::enabled()")]
pub async fn report_quote(
    state: Data<AppState>,
    path: Path<(i32,)>,
    body: Json<Reason>,
    user: User,
) -> impl Responder {
    let (id,) = path.into_inner();

    let mut transaction = match open_transaction(&state.db).await {
        Ok(t) => t,
        Err(res) => return res,
    };

    let mut hasher = Sha3_256::new();
    hasher.update(format!("{}coleandethanwerehere", user.preferred_username).as_str()); // >:)
    let result = hasher.finalize();

    match log_query(
        query!(
            "INSERT INTO reports (quote_id, reason, submitter_hash)
            SELECT $1, $2, $3
            WHERE $1 IN (
                SELECT id FROM quotes
                WHERE id NOT IN (SELECT quote_id FROM hidden)
            )
            ON CONFLICT DO NOTHING",
            id,
            body.reason,
            result.as_slice()
        )
        .execute(&mut *transaction)
        .await,
        Some(transaction),
    )
    .await
    {
        Ok((tx, result)) => {
            transaction = tx.unwrap();
            if result.rows_affected() == 0 {
                return HttpResponse::BadRequest()
                    .body("You have already reported this quote or quote does not exist");
            }
        }
        Err(res) => return res,
    };
    log!(Level::Trace, "created a new report");

    match transaction.commit().await {
        Ok(_) => HttpResponse::Ok().body(""),
        Err(e) => {
            log!(Level::Error, "Transaction failed to commit");
            HttpResponse::InternalServerError().body(e.to_string())
        }
    }
}

/// Gets a quote by ID
#[utoipa::path(
    get,
    path = "/api/quote/{id}",
    params(
        ("id" = i32, Path, description = "ID of quote")
    ),
    responses(
        (status = OK, description = "Quote reported successfully", body = QuoteResponse),
        (status = NOT_FOUND, description = "Quote not found"),
        (status = UNAUTHORIZED, description = "Not authenticated"),
        (status = INTERNAL_SERVER_ERROR, description = "Internal Server Error")
    ),
)]
#[get("/quote/{id}", wrap = "CSHAuth::enabled()")]
pub async fn get_quote(state: Data<AppState>, path: Path<(i32,)>, user: User) -> impl Responder {
    let (id,) = path.into_inner();

    match log_query_as(
        query_as!(
            QuoteShard,
            "SELECT pq.id as \"id!\", s.index as \"index!\", pq.submitter as \"submitter!\",
            pq.timestamp as \"timestamp!\", s.body as \"body!\", s.speaker as \"speaker!\",
            hidden.reason as \"hidden_reason: Option<String>\", hidden.actor as \"hidden_actor: Option<String>\", 
            v.vote as \"vote: Option<Vote>\",
            (CASE WHEN t.score IS NULL THEN 0 ELSE t.score END) AS \"score!\",
            (CASE WHEN f.username IS NULL THEN FALSE ELSE TRUE END) AS \"favorited!\"
            FROM (
                SELECT * FROM quotes q
                WHERE q.id = $1
                AND CASE
                    WHEN $3 THEN TRUE
                    ELSE (CASE
                        WHEN q.id IN (SELECT quote_id FROM hidden) AND
                        (q.submitter=$2 OR $2 IN (
                            SELECT speaker FROM shards
                            WHERE quote_id=q.id))
                        THEN TRUE
                        ELSE q.id NOT IN (SELECT quote_id FROM hidden)
                    END)
                END
                ORDER BY q.id DESC
            ) AS pq
            LEFT JOIN hidden ON hidden.quote_id = pq.id
            LEFT JOIN shards s ON s.quote_id = pq.id
            LEFT JOIN (
                SELECT quote_id, vote FROM votes
                WHERE submitter=$2
            ) v ON v.quote_id = pq.id
            LEFT JOIN (
                SELECT
                    quote_id,
                    SUM(
                        CASE
                            WHEN vote='upvote' THEN 1 
                            WHEN vote='downvote' THEN -1
                            ELSE 0
                        END
                    ) AS score
                FROM votes
                GROUP BY quote_id
            ) t ON t.quote_id = pq.id
            LEFT JOIN (
                SELECT quote_id, username FROM favorites
                WHERE username=$2
            ) f ON f.quote_id = pq.id
            ORDER BY timestamp DESC, pq.id DESC, s.index",
            id,
            user.preferred_username,
            user.admin() || !*SECURITY_ENABLED,
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
                    Ok(quotes) => HttpResponse::Ok().json(quotes.first().unwrap()),
                    Err(res) => res,
                }
            }
        }
        Err(res) => res,
    }
}

/// Adds a vote (upvote or downvote) to a quote
#[utoipa::path(
    post,
    path = "/api/quote/{id}/vote",
    params(
        ("id" = i32, Path, description = "ID of quote to vote on"),
        VoteParams
    ),
    responses(
        (status = OK, description = "Quote voted on successfully"),
        (status = NOT_FOUND, description = "Quote not found"),
        (status = UNAUTHORIZED, description = "Not authenticated"),
        (status = INTERNAL_SERVER_ERROR, description = "Internal Server Error")
    ),
)]
#[post("/quote/{id}/vote", wrap = "CSHAuth::enabled()")]
pub async fn vote_quote(
    state: Data<AppState>,
    path: Path<(i32,)>,
    params: web::Query<VoteParams>,
    user: User,
) -> impl Responder {
    let (id,) = path.into_inner();
    let vote = params.vote.clone();

    let mut transaction = match open_transaction(&state.db).await {
        Ok(t) => t,
        Err(res) => return res,
    };

    match log_query(
        query!(
            "INSERT INTO votes (quote_id, vote, submitter)
            SELECT $1, $2, $3
            WHERE $1 IN (
                SELECT id FROM quotes
                WHERE CASE WHEN $4 THEN true ELSE id NOT IN (SELECT quote_id FROM hidden) END
            )
            ON CONFLICT (quote_id, submitter)
            DO UPDATE SET vote=$2",
            id,
            vote as Vote,
            user.preferred_username,
            user.admin() || !*SECURITY_ENABLED
        )
        .execute(&mut *transaction)
        .await,
        Some(transaction),
    )
    .await
    {
        Ok((tx, result)) => {
            transaction = tx.unwrap();
            if result.rows_affected() == 0 {
                return HttpResponse::NotFound().body("Quote does not exist");
            }
        }
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

/// Removes a vote from a quote
#[utoipa::path(
    delete,
    path = "/api/quote/{id}/vote",
    params(
        ("id" = i32, Path, description = "ID of quote to unvote")
    ),
    responses(
        (status = OK, description = "Removed vote from quote successfully"),
        (status = NOT_FOUND, description = "Quote not found"),
        (status = UNAUTHORIZED, description = "Not authenticated"),
        (status = INTERNAL_SERVER_ERROR, description = "Internal Server Error")
    ),
)]
#[delete("/quote/{id}/vote", wrap = "CSHAuth::enabled()")]
pub async fn unvote_quote(state: Data<AppState>, path: Path<(i32,)>, user: User) -> impl Responder {
    let (id,) = path.into_inner();

    let mut transaction = match open_transaction(&state.db).await {
        Ok(t) => t,
        Err(res) => return res,
    };

    match log_query(
        query!(
            "DELETE FROM votes 
            WHERE quote_id=$1 AND submitter=$2
            AND $1 IN (
                SELECT id FROM quotes
                WHERE CASE WHEN $3 THEN true ELSE id NOT IN (SELECT quote_id FROM hidden) END
            )",
            id,
            user.preferred_username,
            user.admin() || !*SECURITY_ENABLED
        )
        .execute(&mut *transaction)
        .await,
        Some(transaction),
    )
    .await
    {
        Ok((tx, result)) => {
            transaction = tx.unwrap();
            if result.rows_affected() == 0 {
                return HttpResponse::NotFound().body("Quote does not exist");
            }
        }
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

/// Gets the list of users from LDAP
#[utoipa::path(
    get,
    path = "/api/quotes",
    params(FetchParams),
    responses(
        (status = OK, description = "Successfully searched quotes", body = Vec<QuoteResponse>),
        (status = UNAUTHORIZED, description = "Not authenticated"),
        (status = INTERNAL_SERVER_ERROR, description = "Internal Server Error")
    ),
)]
#[get("/quotes", wrap = "CSHAuth::enabled()")]
pub async fn get_quotes(
    state: Data<AppState>,
    params: web::Query<FetchParams>,
    user: User,
) -> impl Responder {
    let limit: i64 = params
        .limit
        .map(|x| if x == -1 { i64::MAX } else { x })
        .unwrap_or(10);
    let lt_qid: i32 = params.lt.unwrap_or(0);
    let query = params
        .q
        .clone()
        .map_or("%".to_string(), |q| format!("%{q}%"));
    let speaker = params.speaker.clone().unwrap_or("%".to_string());
    let submitter = params.submitter.clone().unwrap_or("%".to_string());
    let involved = params.involved.clone().unwrap_or("%".to_string());
    let hidden = params.hidden.unwrap_or(false);
    let filter_by_hidden = params.hidden.is_some();
    let favorited = params.favorited.unwrap_or(false);
    let sort = params.sort.as_ref().is_some_and(|s| s == "votes");
    let sort_direction = params.sort_direction.is_some_and(|d| d);
    match log_query_as(
        query_file_as!(
            QuoteShard,
            "queries/get_quotes.sql",
            limit,                              // $1
            lt_qid,                             // $2
            query,                              // $3
            speaker,                            // $4
            submitter,                          // $5
            hidden,                             // $6
            filter_by_hidden,                   // $7
            user.preferred_username,            // $8
            user.admin() || !*SECURITY_ENABLED, // $9
            involved,                           // $10
            favorited,                          // $11
            sort,                               // $12
            sort_direction,                     // $13
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

/// Gets the list of users
#[utoipa::path(
    get,
    path = "/api/users",
    responses(
        (status = OK, description = "Successfully fetched all users", body = Vec<QuoteResponse>),
        (status = UNAUTHORIZED, description = "Not authenticated"),
        (status = INTERNAL_SERVER_ERROR, description = "Internal Server Error")
    ),
)]
#[get("/users", wrap = "CSHAuth::enabled()")]
pub async fn get_users(state: Data<AppState>) -> impl Responder {
    let kevlar_users = match get_kevlar_users(&state.db).await {
        Ok(users) => users,
        Err(e) => return HttpResponse::InternalServerError().body(e.to_string()),
    };
    match ldap::get_group_members(&state.ldap, "member").await {
        Ok(users) => HttpResponse::Ok().json(
            users
                .into_iter()
                .filter(|user| kevlar_users.contains(&user.uid))
                .map(|x| UserResponse {
                    uid: x.uid,
                    cn: x.cn,
                })
                .collect::<Vec<_>>(),
        ),
        Err(err) => HttpResponse::InternalServerError().body(err.to_string()),
    }
}

/// Gets the list of reports
#[utoipa::path(
    get,
    path = "/api/reports",
    responses(
        (status = OK, description = "Successfully fetched reports", body = Vec<ReportResponse>),
        (status = UNAUTHORIZED, description = "Not an admin"),
        (status = INTERNAL_SERVER_ERROR, description = "Internal Server Error")
    ),
)]
#[get("/reports", wrap = "CSHAuth::admin_only()")]
pub async fn get_reports(state: Data<AppState>) -> impl Responder {
    match log_query_as(
        query_as!(
            ReportedQuoteShard,
            "SELECT pq.id AS \"quote_id!\", pq.submitter AS \"quote_submitter!\",
            pq.timestamp AS \"quote_timestamp!\", pq.hidden AS \"quote_hidden!\", 
            r.timestamp AS \"report_timestamp!\", r.id AS \"report_id!\",
            r.reason AS \"report_reason!\", r.resolver AS \"report_resolver\"
            FROM (
                SELECT * FROM (
                    SELECT id, submitter, timestamp,
                        (CASE WHEN quote_id IS NOT NULL THEN TRUE ELSE FALSE END) AS hidden
                    FROM quotes as _q
                    LEFT JOIN (SELECT quote_id FROM hidden) _h ON _q.id = _h.quote_id
                ) as q
                WHERE q.id IN (
                    SELECT quote_id FROM reports r
                    WHERE r.resolver IS NULL
                )
            ) AS pq
            LEFT JOIN reports r ON r.quote_id = pq.id WHERE r.resolver IS NULL
            ORDER BY pq.id, r.id"
        )
        .fetch_all(&state.db)
        .await,
        None,
    )
    .await
    {
        Ok((_, reports)) => HttpResponse::Ok().json(format_reports(reports.as_slice())),
        Err(res) => res,
    }
}

impl From<sqlx::Error> for SqlxErrorOrResponse<'_> {
    fn from(error: sqlx::Error) -> Self {
        Self::SqlxError(error)
    }
}

/// Resolves a report
#[utoipa::path(
    put,
    path = "/api/quote/{id}/resolve",
    params(
        ("id" = i32, Path, description = "ID of report to resolve"),
        ResolveParams
    ),
    responses(
        (status = OK, description = "Successfully resolved a report"),
        (status = BAD_REQUEST, description = "Report is either already resolved or doesn't exist"),
        (status = UNAUTHORIZED, description = "Not an admin"),
        (status = INTERNAL_SERVER_ERROR, description = "Internal Server Error")
    ),
)]
#[put("/quote/{id}/resolve", wrap = "CSHAuth::admin_only()")]
pub async fn resolve_report(
    state: Data<AppState>,
    path: Path<(i32,)>,
    user: User,
    params: web::Query<ResolveParams>,
) -> Result<HttpResponse, SqlxErrorOrResponse<'static>> {
    let (id,) = path.into_inner();

    state.db.acquire().await?.transaction(|transaction| Box::pin(async move {

        let result = match query!(
            "UPDATE reports SET resolver=$1 WHERE quote_id=$2 AND resolver IS NULL RETURNING reason",
            user.preferred_username,
            id,
        )
            .fetch_one(&mut **transaction)
            .await {
                Ok(result) => result,
                Err(sqlx::Error::RowNotFound) =>
                {
                    return Err(SqlxErrorOrResponse::Response(StatusCode::BAD_REQUEST, "Report is either already resolved or doesn't exist."));
                },
                Err(err) => return Err(err.into()),
            };

        log!(Level::Trace, "resolved all quote's reports");

        if let Some(true) = params.hide {
            hide_quote_by_id(id, user, result.reason, &mut *transaction).await?;
        }

        Ok(())

    })).await?;

    Ok(HttpResponse::Ok().body(""))
}

/// Favorites a quote
#[utoipa::path(
    post,
    path = "/api/quote/{id}/favorite",
    params(
        ("id" = i32, Path, description = "ID of quote to favorite")
    ),
    responses(
        (status = OK, description = "Successfully favorited quote"),
        (status = BAD_REQUEST, description = "Quote is either already favorited or doesn't exist"),
        (status = UNAUTHORIZED, description = "Not authenticated"),
        (status = INTERNAL_SERVER_ERROR, description = "Internal Server Error")
    ),
)]
#[post("/quote/{id}/favorite", wrap = "CSHAuth::enabled()")]
pub async fn favorite_quote(
    state: Data<AppState>,
    user: User,
    path: Path<(i32,)>,
) -> impl Responder {
    let (id,) = path.into_inner();

    let mut transaction = match open_transaction(&state.db).await {
        Ok(t) => t,
        Err(res) => return res,
    };

    match log_query(
        query!(
            "INSERT INTO favorites (quote_id, username)
            VALUES ($1, $2)",
            id,
            user.preferred_username,
        )
        .execute(&mut *transaction)
        .await,
        Some(transaction),
    )
    .await
    {
        Ok((tx, result)) => {
            transaction = tx.unwrap();
            if result.rows_affected() == 0 {
                return HttpResponse::BadRequest()
                    .body("Quote is either already favorited or doesn't exist.");
            }
        }
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

/// Unfavorites a quote
#[utoipa::path(
    delete,
    path = "/api/quote/{id}/favorite",
    params(
        ("id" = i32, Path, description = "ID of quote to unfavorite")
    ),
    responses(
        (status = OK, description = "Successfully unfavorited quote"),
        (status = BAD_REQUEST, description = "Quote is either not favorited or doesn't exist"),
        (status = UNAUTHORIZED, description = "Not authenticated"),
        (status = INTERNAL_SERVER_ERROR, description = "Internal Server Error")
    ),
)]
#[delete("/quote/{id}/favorite", wrap = "CSHAuth::enabled()")]
pub async fn unfavorite_quote(
    state: Data<AppState>,
    user: User,
    path: Path<(i32,)>,
) -> impl Responder {
    let (id,) = path.into_inner();

    let mut transaction = match open_transaction(&state.db).await {
        Ok(t) => t,
        Err(res) => return res,
    };

    match log_query(
        query!(
            "DELETE FROM favorites WHERE quote_id=$1 AND username=$2",
            id,
            user.preferred_username,
        )
        .execute(&mut *transaction)
        .await,
        Some(transaction),
    )
    .await
    {
        Ok((tx, result)) => {
            transaction = tx.unwrap();
            if result.rows_affected() == 0 {
                return HttpResponse::BadRequest().body("Quote is not favorited.");
            }
        }
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

/// Get the current version of the backend
#[utoipa::path(
    get,
    path = "/api/version",
    responses(
        (status = OK, description = "Version information", body = VersionResponse),
    ),
)]
#[get("/version", wrap = "CSHAuth::enabled()")]
pub async fn get_version() -> impl Responder {
    HttpResponse::Ok().json(VersionResponse {
        build_date: env!("VERGEN_BUILD_TIMESTAMP").to_string(),
        date: env!("VERGEN_GIT_COMMIT_TIMESTAMP").to_string(),
        revision: env!("VERGEN_GIT_SHA").to_string(),
        url: env!("REPO_URL").to_string(),
    })
}

#[utoipa::path(
    put,
    path = "/api/kevlar",
    responses(
        (status = OK, description = "Kevlar updated"),
    )
)]
#[put("/kevlar", wrap = "CSHAuth::disabled()")]
pub async fn toggle_kevlar(state: Data<AppState>, user: User) -> impl Responder {
    let result = match query!("insert into kevlar(uid, enabled) values($1, true) on conflict on constraint pkey do update set enabled = not kevlar.enabled, last_modified = now() where kevlar.uid = $1 and kevlar.last_modified + '24 hours' < now()", user.preferred_username).execute(&state.db).await {
        Ok(r) => r,
        Err(e) => return HttpResponse::InternalServerError().body(e.to_string()),
    };
    match result.rows_affected() {
        0 => HttpResponse::BadRequest().body("Kevlar has been toggled too recently"),
        _ => {
            toggle_kevlar_cache(&user.preferred_username);
            log!(Level::Info, "{:?}", get_kevlar_users(&state.db).await);
            HttpResponse::NoContent().finish()
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/kevlar",
    responses(
        (status = OK, description = "Kevlar status"),
    )
)]
#[get("/kevlar", wrap = "CSHAuth::disabled()")]
pub async fn get_kevlar(state: Data<AppState>, user: User) -> impl Responder {
    let Ok(result) = query!(
        "select count(*) from kevlar where uid = $1 and enabled",
        user.preferred_username
    )
    .fetch_one(&state.db)
    .await
    else {
        return HttpResponse::InternalServerError().body("Failed to get kevlar status");
    };

    HttpResponse::Ok().json(result.count.is_some_and(|x| x > 0))
}

#[utoipa::path(
    delete,
    path = "/api/kevlar",
    responses(
        (status = OK, description = "Cache cleared"),
    )
)]
#[delete("/kevlar", wrap = "CSHAuth::disabled()")]
pub async fn delete_kevlar_cache() -> impl Responder {
    clear_kevlar_cache();
    HttpResponse::NoContent().finish()
}
