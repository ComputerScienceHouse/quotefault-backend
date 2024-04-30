use crate::schema::db::Vote;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

#[derive(Deserialize, Debug, ToSchema)]
pub struct NewQuote {
    pub shards: Vec<NewQuoteShard>,
}

#[derive(Deserialize, Debug, ToSchema)]
pub struct NewQuoteShard {
    pub body: String,
    pub speaker: String,
}

#[derive(Deserialize, Debug, ToSchema)]
pub struct Reason {
    pub reason: String,
}

#[derive(Deserialize, Debug, IntoParams)]
pub struct FetchParams {
    pub q: Option<String>,
    pub lt: Option<i32>,
    pub limit: Option<i64>,
    pub submitter: Option<String>,
    pub speaker: Option<String>,
    pub involved: Option<String>,
    pub hidden: Option<bool>,
    pub favorited: Option<bool>,
    pub sort: Option<String>,
    pub sort_direction: Option<bool>,
}

#[derive(Serialize, Debug, ToSchema)]
pub struct Hidden {
    pub reason: String,
    pub actor: UserResponse,
}

#[derive(Serialize, Debug, ToSchema)]
pub struct QuoteResponse {
    pub submitter: UserResponse,
    pub timestamp: chrono::NaiveDateTime,
    pub shards: Vec<QuoteShardResponse>,
    pub id: i32,
    pub vote: Option<Vote>,
    pub score: i64,
    pub hidden: Option<Hidden>,
    pub favorited: bool,
}

#[derive(Serialize, Debug, ToSchema)]
pub struct QuoteShardResponse {
    pub body: String,
    pub speaker: UserResponse,
}

#[derive(Serialize, Clone, Debug, ToSchema)]
pub struct UserResponse {
    pub cn: String,
    pub uid: String,
}

#[derive(Serialize, Debug)]
pub struct ReportedQuoteResponse {
    pub quote_id: i32,
    pub reports: Vec<ReportResponse>,
}

#[derive(Serialize, Debug, ToSchema)]
pub struct ReportResponse {
    pub reason: String,
    pub timestamp: chrono::NaiveDateTime,
    pub id: i32,
}

#[derive(Deserialize, Debug, IntoParams)]
pub struct ResolveParams {
    pub hide: Option<bool>,
}

#[derive(Deserialize, Debug, IntoParams)]
pub struct VoteParams {
    pub vote: Vote,
}

#[derive(Serialize, Debug, ToSchema)]
pub struct VersionResponse {
    pub revision: String,
    pub date: String,
    pub build_date: String,
    pub url: String,
}
