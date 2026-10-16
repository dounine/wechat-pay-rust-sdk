use std::fmt::{Display, Formatter};

/// 支付方式。
///
/// ⚠ crate 内部并未使用它：`Display` 输出的大写字符串与微信文档里的 `trade_type`
/// 取值一致，仅供调用方做分类与展示。微信文档给出的取值集合是
/// `JSAPI` / `NATIVE` / `APP` / `MICROPAY` / `MWEB` / `FACEPAY`，本枚举只覆盖前五种。
///
/// [`PayType::Qrcode`] 是历史遗留命名：扫码支付在微信文档里的取值是 `NATIVE`，
/// 所以它的输出与 [`PayType::Native`] 相同。
#[derive(Debug, PartialEq, Eq)]
pub enum PayType {
    /// 付款码支付
    Micro,
    /// 小程序 / 公众号支付
    Jsapi,
    /// 扫码支付（返回 `code_url`，由商户自行生成二维码）
    Native,
    /// APP 支付
    App,
    /// H5 支付（外部浏览器）
    H5,
    /// 二维码支付
    Qrcode,
}

impl Display for PayType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            PayType::Micro => write!(f, "MICROPAY"),
            PayType::Jsapi => write!(f, "JSAPI"),
            PayType::Native => write!(f, "NATIVE"),
            PayType::App => write!(f, "APP"),
            PayType::H5 => write!(f, "MWEB"),
            PayType::Qrcode => write!(f, "NATIVE"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PayType;

    #[test]
    fn display_matches_official_trade_type() {
        let cases = [
            (PayType::Micro, "MICROPAY"),
            (PayType::Jsapi, "JSAPI"),
            (PayType::Native, "NATIVE"),
            (PayType::App, "APP"),
            (PayType::H5, "MWEB"),
            (PayType::Qrcode, "NATIVE"),
        ];
        for (pay_type, trade_type) in cases {
            assert_eq!(pay_type.to_string(), trade_type, "{pay_type:?}");
        }
    }
}
