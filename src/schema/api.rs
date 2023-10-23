use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct NewQuote {
    pub submitter: String,
    pub shards: Vec<NewQuoteShard>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct NewQuoteShard {
    pub body: String,
    pub speaker: String,
}

#[derive(Deserialize, Debug)]
pub struct FetchParams {
    pub q: Option<String>,
    pub offset: Option<u32>,
    pub limit: Option<u32>,
    pub submitter: Option<String>,
    pub speaker: Option<String>,
}

#[derive(Serialize, Debug)]
pub struct QuoteResponse {
    pub submitter: String,
    pub timestamp: chrono::NaiveDateTime,
    pub shards: Vec<QuoteShardResponse>,
    pub id: i32,
}

#[derive(Serialize, Debug)]
pub struct QuoteShardResponse {
    pub body: String,
    pub speaker: String,
}
