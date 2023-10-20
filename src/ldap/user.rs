use lazy_static::lazy_static;
use ldap3::SearchEntry;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Debug;
use std::str::FromStr;
use utoipa::ToSchema;

#[allow(non_snake_case)]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LdapUser {
    pub dn: String,
    pub cn: String,
    pub uid: String,
    pub rit_username: String,
    pub groups: Vec<String>,
    pub krb_principal_name: String,
    pub mail: Vec<String>,
    pub mobile: Vec<String>,
    pub drink_balance: Option<i64>,
    pub ibutton: Vec<String>,
}

impl LdapUser {
    #[must_use]
    pub fn from_entry(entry: &SearchEntry) -> Self {
        let user_attrs = &entry.attrs;
        LdapUser {
            dn: entry.dn.clone(),
            cn: get_one(user_attrs, "cn").unwrap(),
            uid: get_one(user_attrs, "uid").unwrap(),
            rit_username: get_one(user_attrs, "ritDn").unwrap_or_default(),
            groups: get_groups(get_vec(user_attrs, "memberOf")),
            krb_principal_name: get_one(user_attrs, "krbPrincipalName").unwrap(),
            mail: get_vec(user_attrs, "mail"),
            mobile: get_vec(user_attrs, "mobile"),
            ibutton: get_vec(user_attrs, "ibutton"),
            drink_balance: get_one(user_attrs, "drinkBalance"),
        }
    }
}

fn get_one<T>(entry: &HashMap<String, Vec<String>>, field: &str) -> Option<T>
where
    T: FromStr,
    <T as FromStr>::Err: Debug,
{
    match entry.get(field).map(|f| f.get(0).unwrap().parse::<T>()) {
        Some(Ok(r)) => Some(r),
        _ => None,
    }
}

fn get_vec<T>(entry: &HashMap<String, Vec<String>>, field: &str) -> Vec<T>
where
    T: FromStr,
    <T as FromStr>::Err: Debug,
{
    entry
        .get(field)
        .map(|v| {
            v.iter()
                .filter_map(|f| f.parse::<T>().ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or(vec![])
}

pub fn get_groups(member_of: Vec<String>) -> Vec<String> {
    lazy_static! {
        static ref GROUP_REGEX: Regex =
            Regex::new(r"cn=(?P<name>\w+),cn=groups,cn=accounts,dc=csh,dc=rit,dc=edu").unwrap();
    }
    member_of
        .iter()
        .filter_map(|group| {
            GROUP_REGEX
                .captures(group)
                .map(|cap| cap["name"].to_owned())
        })
        .collect()
}
