use serde::{Deserialize, Serialize};
pub struct ID {
    pub id: i32, // SERIAL value
}

#[derive(Serialize, Debug)]
pub struct QuoteShard {
    pub id: i32,
    pub index: i32,
    pub body: String,
    pub submitter: String,
    pub speaker: String,
    pub timestamp: chrono::NaiveDateTime,
    pub vote: Option<Vote>,
    pub score: i64,
    pub hidden: bool,
}

#[derive(Serialize, Debug)]
pub struct ReportedQuoteShard {
    pub quote_id: i32,
    pub quote_submitter: String,
    pub quote_timestamp: chrono::NaiveDateTime,
    pub quote_hidden: bool,
    pub report_id: i32,
    pub report_reason: String,
    pub report_timestamp: chrono::NaiveDateTime,
    pub report_resolver: Option<String>,
}

#[derive(Clone, Debug, PartialEq, PartialOrd, sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "vote", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum Vote {
    Upvote,
    Downvote,
}
