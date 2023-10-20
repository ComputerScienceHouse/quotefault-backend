#![allow(unused)]

use async_trait::async_trait;
use deadpool::managed::{self, Metrics};
use ldap3::{drive, Ldap, LdapConnAsync, LdapError};
use rand::prelude::SliceRandom;
use rand::SeedableRng;
use std::sync::Arc;
use trust_dns_resolver::{
    config::{ResolverConfig, ResolverOpts},
    AsyncResolver,
};

type Pool = managed::Pool<LdapManager>;

#[derive(Clone)]
pub struct LdapClient {
    pub(super) ldap: Arc<Pool>,
}

#[derive(Clone)]
pub(super) struct LdapManager {
    ldap_servers: Vec<String>,
    bind_dn: String,
    bind_pw: String,
}

impl LdapManager {
    pub async fn new(bind_dn: &str, bind_pw: &str) -> Self {
        let ldap_servers = get_ldap_servers().await;

        LdapManager {
            ldap_servers,
            bind_dn: bind_dn.to_string(),
            bind_pw: bind_pw.to_string(),
        }
    }
}

#[async_trait]
impl managed::Manager for LdapManager {
    type Type = Ldap;
    type Error = LdapError;

    async fn create(&self) -> Result<Self::Type, Self::Error> {
        let (conn, mut ldap) = LdapConnAsync::new(
            self.ldap_servers
                .choose(&mut rand::rngs::StdRng::from_entropy())
                .unwrap(),
        )
        .await
        .unwrap();

        drive!(conn);

        ldap.simple_bind(&self.bind_dn, &self.bind_pw)
            .await
            .unwrap();

        Ok(ldap)
    }

    async fn recycle(
        &self,
        ldap: &mut Self::Type,
        _: &Metrics,
    ) -> managed::RecycleResult<Self::Error> {
        ldap.extended(ldap3::exop::WhoAmI).await?;
        Ok(())
    }
}

impl LdapClient {
    pub async fn new(bind_dn: &str, bind_pw: &str) -> Self {
        let ldap_manager = LdapManager::new(bind_dn, bind_pw).await;
        let ldap_pool = Pool::builder(ldap_manager).max_size(5).build().unwrap();

        LdapClient {
            ldap: Arc::new(ldap_pool),
        }
    }
}

async fn get_ldap_servers() -> Vec<String> {
    let resolver = AsyncResolver::tokio(ResolverConfig::default(), ResolverOpts::default());
    let response = resolver.srv_lookup("_ldap._tcp.csh.rit.edu").await.unwrap();

    // TODO: Make sure servers are working
    response
        .iter()
        .map(|record| {
            format!(
                "ldaps://{}",
                record.target().to_string().trim_end_matches('.')
            )
        })
        .collect()
}
