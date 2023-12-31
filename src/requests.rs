use serde::{Deserialize, Serialize};

use crate::{args::AddArgs, link::Link};

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum RemoteRequest {
    Insert(InsertRequest),
}

#[derive(Debug, Deserialize, Serialize)]
pub struct InsertRequest {
    pub name: String,
    pub link: Link,
    pub metadata: Vec<String>,
}

impl From<AddArgs> for InsertRequest {
    fn from(args: AddArgs) -> Self {
        Self {
            name: args.name,
            link: args.link.as_str().into(),
            metadata: args.metadata,
        }
    }
}
