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
    pub limit: Option<u32>,
    pub submitter: Option<String>,
    pub speaker: Option<String>,
}

#[derive(Serialize, Debug)]
pub struct QuoteResponse {
    pub submitter: UserResponse,
    pub timestamp: chrono::NaiveDateTime,
    pub shards: Vec<QuoteShardResponse>,
    pub id: i32,
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
