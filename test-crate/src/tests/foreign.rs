use chrono::naive::NaiveDateTime;
use uuid::Uuid;

#[doc = "pure"]
pub fn uuid(left: usize, right: usize) -> usize {
    let _id = Uuid::new_v4();
    left + right
}

#[doc = "pure"]
pub fn date_format(v: NaiveDateTime) -> String {
    v.format("%Y-%m-%d %H:%M:%S").to_string()
}
