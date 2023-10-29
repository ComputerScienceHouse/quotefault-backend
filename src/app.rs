use std::env;

use actix_web::web::{self, scope, Data};
use sqlx::{postgres::PgPoolOptions, Pool, Postgres};

use crate::{
    api::endpoints::{
        create_quote, delete_quote, get_hidden, get_quote, get_quotes, get_reports, get_users,
        hide_quote, report_quote, resolve_report, unvote_quote, vote_quote,
    },
    ldap::client::LdapClient,
};

pub struct AppState {
    pub db: Pool<Postgres>,
    pub ldap: LdapClient,
}

pub fn configure_app(cfg: &mut web::ServiceConfig) {
    cfg.service(
        scope("/api")
            .service(create_quote)
            .service(get_quotes)
            .service(get_users)
            .service(get_quote)
            .service(get_reports)
            .service(delete_quote)
            .service(hide_quote)
            .service(report_quote)
            .service(get_hidden)
            .service(resolve_report)
            .service(vote_quote)
            .service(unvote_quote),
    );
}

pub async fn get_app_data() -> Data<AppState> {
    let db = PgPoolOptions::new()
        .connect(&env::var("DATABASE_URL").expect("DATABASE_URL not set"))
        .await
        .expect("Could not connect to database");
    println!("Successfully connected to database! :)");
    let ldap = LdapClient::new(
        env::var("QUOTEFAULT_LDAP_BIND_DN")
            .expect("QUOTEFAULT_LDAP_BIND_DN not set")
            .as_str(),
        env::var("QUOTEFAULT_LDAP_BIND_PW")
            .expect("QUOTEFAULT_LDAP_BIND_PW not set")
            .as_str(),
    )
    .await;
    Data::new(AppState { db, ldap })
}
