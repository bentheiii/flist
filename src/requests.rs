use serde::{Deserialize, Serialize};

use crate::args::AddArgs;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum RemoteRequest {
    Insert(InsertRequest),
}

#[derive(Debug, Deserialize, Serialize)]
pub struct InsertRequest {
    pub name: String,
    pub link: String,
    pub metadata: Vec<String>,
}

impl From<AddArgs> for InsertRequest {
    fn from(args: AddArgs) -> Self {
        Self {
            name: args.name,
            link: args.link,
            metadata: args.metadata,
        }
    }
}
