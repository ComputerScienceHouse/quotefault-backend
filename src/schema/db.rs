use serde::{Deserialize, Serialize};
pub struct ID {
    pub id: i32, // SERIAL value
}

#[derive(Serialize, Deserialize, Debug)]
pub struct QuoteShard {
    pub id: i32,
    pub index: i32,
    pub body: String,
    pub submitter: String,
    pub speaker: String,
    pub timestamp: chrono::NaiveDateTime,
}
