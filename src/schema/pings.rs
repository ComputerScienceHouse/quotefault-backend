use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct PingsBody {
    pub username: String,
    pub body: String,
}
