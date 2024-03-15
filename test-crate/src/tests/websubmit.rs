use chrono::naive::NaiveDateTime;
use chrono::Local;
use mysql::{from_value, Value};
// use rocket::State;

use std::collections::HashMap;
use std::hash::Hash;

use mysql::prelude::FromValue;

pub enum JoinIdx {
    Left(usize),
    Right(usize),
}

pub fn left_join(
    left: Vec<Vec<Value>>,
    right: Vec<Vec<Value>>,
    lid: usize,
    rid: usize,
    idx: Vec<JoinIdx>,
) -> Vec<Vec<Value>> {
    let mut rmap = HashMap::new();
    for (i, r) in right.iter().enumerate() {
        let id: u64 = from_value(r[rid].clone());
        rmap.insert(id, i);
    }

    left.into_iter()
        .map(|r| {
            let id: u64 = from_value(r[lid].clone());
            let other = rmap.get(&id);

            let mut vec = Vec::new();
            for i in idx.iter() {
                match i {
                    JoinIdx::Left(i) => vec.push(r[*i].clone()),
                    JoinIdx::Right(i) => match other {
                        None => vec.push(Value::NULL),
                        Some(oidx) => vec.push(right[*oidx][*i].clone()),
                    },
                }
            }
            vec
        })
        .collect()
}

pub type AvgIdx = usize;

// Compute the average of the given column grouped by the value of the group_by column.
// Result is on the form:
// [
//    [<group1>, <avg1>],
//    [<group2>, <avg2>],
//    ...
// ]
pub fn average<GroupType>(
    column: AvgIdx,
    group_by: AvgIdx,
    data: Vec<Vec<Value>>,
) -> Vec<Vec<Value>>
where
    GroupType: Eq + Hash + FromValue + Into<Value>,
{
    let map: HashMap<GroupType, (u64, u64)> = HashMap::new();
    data.into_iter()
        .fold(map, |mut map, row| {
            let group: GroupType = from_value(row[group_by].clone());
            let value: u64 = from_value(row[column].clone());
            let tup: &mut (u64, u64) = map.entry(group).or_default();
            tup.0 += value;
            tup.1 += 1;
            map
        })
        .into_iter()
        .map(|(group, (sum, count))| vec![group.into(), (sum / count).into()])
        .collect()
}

#[derive(Debug, Clone, Default)]
pub struct Config {
    /// Textual identifier for class
    pub class: String,
    /// Database user
    pub db_user: String,
    /// Database password
    pub db_password: String,
    /// System admin addresses
    pub admins: Vec<String>,
    /// System manager addresses
    pub managers: Vec<String>,
    /// Staff email addresses
    pub staff: Vec<String>,
    /// Web template directory
    pub template_dir: String,
    /// Web resource root directory
    pub resource_dir: String,
    /// Secret (for API key generation)
    pub secret: String,
    /// Whether to send emails
    pub send_emails: bool,
    /// Whether to reset and prime db
    pub prime: bool,
}

pub struct Admin;

pub struct Manager;

// static config: &State<Config>;
static config: &Config = &Config {
    class: String::new(),
    db_user: String::new(),
    db_password: String::new(),
    admins: vec![],
    managers: vec![],
    staff: vec![],
    template_dir: String::new(),
    resource_dir: String::new(),
    secret: String::new(),
    send_emails: true,
    prime: true,
};

// admin.rs:38-44
#[doc = "pure"]
fn ppr_1(user: &String) -> Option<Admin> {
    if config.admins.contains(&user) {
        Some(Admin)
    } else {
        None
    }
}

// admin.rs:187-188
#[doc = "pure"]
fn ppr_2(num: u8) -> String { format!("{}", num) }

// admin.rs:245
#[doc = "pure"]
fn ppr_3(id: String) -> bool { config.admins.contains(&id) }

// grades.rs:39
#[doc = "pure"]
fn ppr_4(v: NaiveDateTime) -> String { v.format("%Y-%m-%d %H:%M:%S").to_string() }

// manage.rs:40-46
#[doc = "pure"]
fn ppr_5(user: &String) -> Option<Manager> {
    if config.managers.contains(&user) {
        Some(Manager)
    } else {
        None
    }
}

// manage.rs:93
#[doc = "pure"]
fn ppr_6(grades: Vec<Vec<Value>>) -> Vec<Vec<Value>> { average::<String>(3, 0, grades) }

// manage.rs:94
#[doc = "pure"]
fn ppr_7(grades: Vec<Vec<Value>>) -> Vec<Vec<Value>> { average::<String>(3, 1, grades) }

// manage.rs:95
#[doc = "pure"]
fn ppr_8(grades: Vec<Vec<Value>>) -> Vec<Vec<Value>> { average::<bool>(3, 2, grades) }

// questions.rs:107
#[doc = "pure"]
fn ppr_9(email: String) -> bool { config.admins.contains(&email) }

// questions.rs:115-119
#[doc = "pure"]
fn ppr_10(v: Value) -> u64 {
    match v {
        Value::NULL => 0u64,
        v => from_value(v),
    }
}

// questions.rs:229-239
#[doc = "pure"]
fn ppr_11(tup: (Vec<Vec<Value>>, Vec<Vec<Value>>)) -> Vec<Vec<Value>>{
    let (questions, answers) = tup;
    let mut questions = left_join(
        questions,
        answers,
        1,
        2,
        vec![JoinIdx::Left(1), JoinIdx::Left(2), JoinIdx::Right(3)],
    );
    questions.sort_by(|a, b| a[0].partial_cmp(&b[0]).unwrap());
    questions
}
