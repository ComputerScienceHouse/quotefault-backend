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
