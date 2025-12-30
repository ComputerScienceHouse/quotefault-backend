use std::collections::BTreeSet;

use self::user::LdapUser;
use crate::ldap::client::LdapClient;
use crate::ldap::search::SearchAttrs;
use ldap3::{ResultEntry, SearchEntry};
use log::{log, Level};

pub mod client;
pub mod search;
pub mod user;

pub async fn get_intro_members(client: &LdapClient) -> Result<Vec<LdapUser>, anyhow::Error> {
    get_group_members(client, "intromembers").await
}

pub async fn get_active_upperclassmen(client: &LdapClient) -> Result<Vec<LdapUser>, anyhow::Error> {
    let res = ldap_search(
        client,
        "cn=users,cn=accounts,dc=csh,dc=rit,dc=edu",
        "(&(memberOf=*active*)(!(memberOf=*intromember*)))",
        None,
    )
    .await?;

    Ok(res
        .iter()
        .map(|r| {
            let user = SearchEntry::construct(r.to_owned());
            LdapUser::from_entry(&user)
        })
        .collect())
}

pub async fn get_group_members(
    client: &LdapClient,
    group: &str,
) -> Result<Vec<LdapUser>, anyhow::Error> {
    let res = ldap_search(
        client,
        "cn=users,cn=accounts,dc=csh,dc=rit,dc=edu",
        format!("memberOf=*{}*", group).as_str(),
        None,
    )
    .await?;

    Ok(res
        .iter()
        .map(|r| {
            let user = SearchEntry::construct(r.to_owned());
            LdapUser::from_entry(&user)
        })
        .collect())
}

pub async fn get_user(client: &LdapClient, user: &str) -> Result<Vec<LdapUser>, anyhow::Error> {
    let res = ldap_search(
        client,
        "cn=users,cn=accounts,dc=csh,dc=rit,dc=edu",
        format!("(uid={})", user).as_str(),
        None,
    )
    .await?;

    Ok(res
        .iter()
        .map(|r| {
            let user = SearchEntry::construct(r.to_owned());
            LdapUser::from_entry(&user)
        })
        .collect())
}

pub async fn get_users(
    client: &LdapClient,
    users: &[String],
) -> Result<Vec<LdapUser>, anyhow::Error> {
    if users.is_empty() {
        return Ok(Vec::new());
    }
    let res = ldap_search(
        client,
        "cn=users,cn=accounts,dc=csh,dc=rit,dc=edu",
        format!(
            "(|{})",
            users
                .iter()
                .filter_map(|x| if !x.contains(' ') {
                    Some(format!("(uid={x})"))
                } else {
                    None
                })
                .collect::<Vec<String>>()
                .join("")
        )
        .as_str(),
        None,
    )
    .await?;

    Ok(res
        .iter()
        .map(|r| {
            let user = SearchEntry::construct(r.to_owned());
            LdapUser::from_entry(&user)
        })
        .collect())
}

pub async fn users_exist(
    client: &LdapClient,
    users: BTreeSet<String>,
) -> Result<bool, anyhow::Error> {
    Ok(users.len()
        == get_users(client, Vec::from_iter(users).as_slice())
            .await?
            .len())
}

pub async fn search_users(
    client: &LdapClient,
    query: &str,
) -> Result<Vec<LdapUser>, anyhow::Error> {
    let res = ldap_search(
        client,
        "cn=users,cn=accounts,dc=csh,dc=rit,dc=edu",
        format!("(|(uid=*{query}*)(cn=*{query}*))").as_str(),
        None,
    )
    .await?;

    Ok(res
        .iter()
        .map(|r| {
            let user = SearchEntry::construct(r.to_owned());
            LdapUser::from_entry(&user)
        })
        .collect())
}

async fn ldap_search(
    client: &LdapClient,
    ou: &str,
    query: &str,
    attrs: Option<SearchAttrs>,
) -> Result<Vec<ResultEntry>, anyhow::Error> {
    log!(Level::Debug, "LDAP Search with query {query} from {ou}");
    let attrs = attrs.unwrap_or_default().finalize();
    let mut ldap = client.ldap.get().await.unwrap();
    ldap.with_timeout(std::time::Duration::from_secs(5));
    let (results, _result) = ldap
        .search(ou, ldap3::Scope::Subtree, query, attrs)
        .await?
        .success()?;

    Ok(results)
}
