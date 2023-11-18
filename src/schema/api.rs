use crate::schema::db::Vote;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Debug)]
pub struct NewQuote {
    pub shards: Vec<NewQuoteShard>,
}

#[derive(Deserialize, Debug)]
pub struct NewQuoteShard {
    pub body: String,
    pub speaker: String,
}

#[derive(Deserialize, Debug)]
pub struct NewReport {
    pub reason: String,
}

#[derive(Deserialize, Debug)]
pub struct FetchParams {
    pub q: Option<String>,
    pub lt: Option<i32>,
    pub limit: Option<i64>,
    pub submitter: Option<String>,
    pub speaker: Option<String>,
    pub involved: Option<String>,
    pub hidden: Option<bool>,
}

#[derive(Serialize, Debug)]
pub struct QuoteResponse {
    pub submitter: UserResponse,
    pub timestamp: chrono::NaiveDateTime,
    pub shards: Vec<QuoteShardResponse>,
    pub id: i32,
    pub vote: Option<Vote>,
    pub score: i64,
    pub hidden: bool,
}

#[derive(Serialize, Debug)]
pub struct QuoteShardResponse {
    pub body: String,
    pub speaker: UserResponse,
}

#[derive(Serialize, Debug)]
pub struct UserResponse {
    pub cn: String,
    pub uid: String,
}

#[derive(Serialize, Debug)]
pub struct ReportedQuoteResponse {
    pub quote_id: i32,
    pub reports: Vec<ReportResponse>,
}

#[derive(Serialize, Debug)]
pub struct ReportResponse {
    pub reason: String,
    pub timestamp: chrono::NaiveDateTime,
    pub id: i32,
}

#[derive(Deserialize, Debug)]
pub struct ResolveParams {
    pub hide: Option<bool>,
}

#[derive(Deserialize, Debug)]
pub struct VoteParams {
    pub vote: Vote,
}

#[derive(Serialize, Debug)]
pub struct VersionResponse {
    pub revision: String,
    pub date: String,
    pub build_date: String,
    pub url: String,
}
