use std::collections::{BTreeSet, HashMap};

use actix_web::{
    delete, get, post, put,
    web::{self, Data, Json, Path},
    HttpResponse, Responder,
};
use log::{log, Level};
use sha3::{Digest, Sha3_256};
use sqlx::{query, query_as, Postgres, Transaction};

use crate::{
    api::{
        db::{log_query, log_query_as, open_transaction},
        pings::send_ping,
    },
    app::AppState,
    auth::{CSHAuth, User},
    ldap,
    schema::{
        api::{
            FetchParams, NewQuote, NewReport, QuoteResponse, QuoteShardResponse, ReportResponse,
            ReportedQuoteResponse, ResolveParams, UserResponse, VersionResponse, VoteParams,
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
                score: shard.score,
                vote: shard.vote.clone(),
                submitter,
                hidden: shard.hidden,
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

pub async fn hide_quote_by_id(
    id: i32,
    user: User,
    mut transaction: Transaction<'_, Postgres>,
) -> Result<Transaction<'_, Postgres>, HttpResponse> {
    match log_query(
        query!(
            "UPDATE quotes SET hidden=true WHERE id=$1
            AND ($3 OR id IN (
                SELECT quote_id FROM shards s
                WHERE s.speaker = $2
            ))",
            id,
            user.preferred_username,
            user.admin(),
        )
        .execute(&mut *transaction)
        .await,
        Some(transaction),
    )
    .await
    {
        Ok((tx, result)) => {
            if result.rows_affected() == 0 {
                Err(HttpResponse::BadRequest()
                    .body("Either you are not quoted in this quote or this quote does not exist."))
            } else {
                log!(Level::Trace, "hid quote");
                Ok(tx.unwrap())
            }
        }
        Err(res) => Err(res),
    }
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

#[put("/quote/{id}/hide", wrap = "CSHAuth::enabled()")]
pub async fn hide_quote(state: Data<AppState>, path: Path<(i32,)>, user: User) -> impl Responder {
    let (id,) = path.into_inner();

    let mut transaction = match open_transaction(&state.db).await {
        Ok(t) => t,
        Err(res) => return res,
    };

    match hide_quote_by_id(id, user, transaction).await {
        Ok(tx) => transaction = tx,
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

#[post("/quote/{id}/report", wrap = "CSHAuth::enabled()")]
pub async fn report_quote(
    state: Data<AppState>,
    path: Path<(i32,)>,
    body: Json<NewReport>,
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
                WHERE NOT hidden
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

#[get("/quote/{id}", wrap = "CSHAuth::enabled()")]
pub async fn get_quote(state: Data<AppState>, path: Path<(i32,)>, user: User) -> impl Responder {
    let (id,) = path.into_inner();

    match log_query_as(
        query_as!(
            QuoteShard,
            "SELECT pq.id as \"id!\", s.index as \"index!\", pq.submitter as \"submitter!\",
            pq.timestamp as \"timestamp!\", s.body as \"body!\", s.speaker as \"speaker!\",
            pq.hidden as \"hidden!\", v.vote as \"vote: Option<Vote>\",
            (CASE WHEN t.score IS NULL THEN 0 ELSE t.score END) AS \"score!\"
            FROM (
                SELECT * FROM quotes q
                WHERE q.id = $1
                AND CASE WHEN $3 THEN true ELSE NOT q.hidden END
            ) AS pq
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
            ) t ON t.quote_id = pq.id",
            id,
            user.preferred_username,
            user.admin(),
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
                    Ok(quotes) => HttpResponse::Ok().json(quotes.get(0).unwrap()),
                    Err(res) => res,
                }
            }
        }
        Err(res) => res,
    }
}

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
                WHERE CASE WHEN $4 THEN true ELSE NOT hidden END
            )
            ON CONFLICT (quote_id, submitter)
            DO UPDATE SET vote=$2",
            id,
            vote as Vote,
            user.preferred_username,
            user.admin()
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
                return HttpResponse::BadRequest().body("Quote does not exist");
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
                WHERE CASE WHEN $3 THEN true ELSE NOT hidden END
            )",
            id,
            user.preferred_username,
            user.admin()
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
                return HttpResponse::BadRequest().body("Quote does not exist");
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

#[get("/quotes", wrap = "CSHAuth::enabled()")]
pub async fn get_quotes(
    state: Data<AppState>,
    params: web::Query<FetchParams>,
    user: User,
) -> impl Responder {
    let limit: i64 = params.limit.unwrap_or(10).into();
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
    match log_query_as(
        query_as!(
            QuoteShard,
            "SELECT pq.id as \"id!\", s.index as \"index!\", pq.submitter as \"submitter!\",
            pq.timestamp as \"timestamp!\", s.body as \"body!\", s.speaker as \"speaker!\",
            pq.hidden as \"hidden!\", v.vote as \"vote: Option<Vote>\",
            (CASE WHEN t.score IS NULL THEN 0 ELSE t.score END) AS \"score!\"
            FROM (
                SELECT * FROM quotes q
                WHERE CASE
                    WHEN $7 AND $6 AND $9 THEN hidden=TRUE
                    WHEN $7 AND $6 THEN CASE
                        WHEN (q.submitter=$8 
                            OR $8 IN (SELECT speaker FROM shards WHERE quote_id=q.id))
                            THEN hidden=TRUE
                        ELSE FALSE
                    END
                    WHEN $7 AND NOT $6 THEN hidden=FALSE
                    ELSE hidden=(q.hidden AND
                        (q.submitter=$8 OR $8 IN (
                            SELECT speaker FROM shards
                            WHERE quote_id=q.id)))
                END
                AND CASE WHEN $2::int4 > 0 THEN q.id < $2::int4 ELSE true END
                AND submitter LIKE $5
                AND (submitter LIKE $10 OR q.id IN (SELECT quote_id FROM shards s WHERE speaker LIKE $10))
                AND q.id IN (
                    SELECT quote_id FROM shards s
                    WHERE body ILIKE $3
                    AND speaker LIKE $4
                )
                ORDER BY q.id DESC
                LIMIT $1
            ) AS pq
            LEFT JOIN shards s ON s.quote_id = pq.id
            LEFT JOIN (
                SELECT quote_id, vote FROM votes
                WHERE submitter=$8
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
            ORDER BY timestamp DESC, pq.id DESC, s.index",
            limit, // $1
            lt_qid, // $2
            query, // $3
            speaker, // $4
            submitter, // $5
            hidden, // $6
            filter_by_hidden, // $7
            user.preferred_username, // $8
            user.admin(), // $9
            involved, // $10
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
                SELECT * FROM quotes q
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

#[put("/quote/{id}/resolve", wrap = "CSHAuth::admin_only()")]
pub async fn resolve_report(
    state: Data<AppState>,
    path: Path<(i32,)>,
    user: User,
    params: web::Query<ResolveParams>,
) -> impl Responder {
    let (id,) = path.into_inner();

    let mut transaction = match open_transaction(&state.db).await {
        Ok(t) => t,
        Err(res) => return res,
    };

    match log_query(
        query!(
            "UPDATE reports SET resolver=$1 WHERE quote_id=$2 AND resolver IS NULL",
            user.preferred_username,
            id,
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
                    .body("Report is either already resolved or doesn't exist.");
            }
        }
        Err(res) => return res,
    }

    log!(Level::Trace, "resolved all quote's reports");

    if let Some(true) = params.hide {
        match hide_quote_by_id(id, user, transaction).await {
            Ok(tx) => transaction = tx,
            Err(res) => return res,
        }
    }

    match transaction.commit().await {
        Ok(_) => HttpResponse::Ok().body(""),
        Err(e) => {
            log!(Level::Error, "Transaction failed to commit");
            HttpResponse::InternalServerError().body(e.to_string())
        }
    }
}

#[get("/version", wrap = "CSHAuth::enabled()")]
pub async fn get_version() -> impl Responder {
    HttpResponse::Ok().json(VersionResponse {
        build_date: env!("VERGEN_BUILD_TIMESTAMP").to_string(),
        date: env!("VERGEN_GIT_COMMIT_TIMESTAMP").to_string(),
        revision: env!("VERGEN_GIT_SHA").to_string(),
        url: env!("REPO_URL").to_string(),
    })
}
