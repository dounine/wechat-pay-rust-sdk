use std::fmt::{Display, Formatter};

#[derive(Debug, PartialEq, Eq)]
pub enum PayType {
    Micro,
    Jsapi,
    Native,
    App,
    H5,
    Qrcode,
}

impl Display for PayType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            PayType::Micro => write!(f, "MICRO"),
            PayType::Jsapi => write!(f, "JSAPI"),
            PayType::Native => write!(f, "NATIVE"),
            PayType::App => write!(f, "APP"),
            PayType::H5 => write!(f, "H5"),
            PayType::Qrcode => write!(f, "QRCODE"),
        }
    }
}