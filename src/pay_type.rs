use strum_macros::Display;

#[derive(Debug, Display, PartialEq, Eq)]
#[strum(serialize_all = "UPPERCASE")]
pub enum PayType {
    Micro,
    Jsapi,
    Native,
    App,
    H5,
    Qrcode,
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::*;

    #[test]
    fn test_pay_type() {
        let pay_type = PayType::Micro;
        assert_eq!(format!("{}", pay_type), "MICRO");
    }
}
