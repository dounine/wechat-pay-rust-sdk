use strum_macros::{Display};

#[derive(Debug, Display, Clone, PartialEq, Eq)]
#[strum(serialize_all = "UPPERCASE")]
pub enum HttpMethod {
    GET,
    POST,
    PUT,
    DELETE,
    PATCH,
    HEAD,
    OPTIONS,
    TRACE,
}

unsafe impl Send for HttpMethod {}

unsafe impl Sync for HttpMethod {}