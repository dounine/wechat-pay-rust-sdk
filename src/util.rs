use uuid::Uuid;

pub fn random_trade_no() -> String {
    Uuid::new_v4().to_string().replace("-", "")
}