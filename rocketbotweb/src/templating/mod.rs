pub(crate) mod filters;


use askama::Template;
use serde::{Deserialize, Serialize};


#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize, Template)]
#[template(path = "400.html")]
pub(crate) struct Error400Template {
    pub reason: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize, Template)]
#[template(path = "404.html")]
pub(crate) struct Error404Template;

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize, Template)]
#[template(path = "405.html")]
pub(crate) struct Error405Template {
    pub allowed_methods: Vec<String>,
}
