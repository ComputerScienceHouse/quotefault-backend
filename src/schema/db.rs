use serde::Serialize;
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
