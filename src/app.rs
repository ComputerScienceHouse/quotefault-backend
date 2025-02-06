use std::env;

use actix_web::web::{self, scope, Data};
use sqlx::{postgres::PgPoolOptions, Pool, Postgres};
use utoipa::{
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
    Modify, OpenApi,
};
use utoipa_swagger_ui::SwaggerUi;

use crate::{api::endpoints::*, auth::SECURITY_ENABLED, ldap::client::LdapClient};

pub struct AppState {
    pub db: Pool<Postgres>,
    pub ldap: LdapClient,
}

pub fn configure_app(cfg: &mut web::ServiceConfig) {
    let cors = if *SECURITY_ENABLED {
        actix_cors::Cors::default()
            .allowed_headers(vec!["Authorization", "Content-Type", "Accept"])
            .allow_any_method()
            .max_age(3600)
    } else {
        actix_cors::Cors::permissive()
    };

    #[derive(OpenApi)]
    #[openapi(
        paths(
            create_quote,
            delete_quote,
            favorite_quote,
            get_quote,
            get_quotes,
            get_reports,
            get_users,
            get_version,
            hide_quote,
            report_quote,
            resolve_report,
            unfavorite_quote,
            unvote_quote,
            vote_quote,
            toggle_kevlar,
            get_kevlar
        ),
        modifiers(&SecurityAddon),
        tags(
            (name = "Quotefault", description = "Quotefault API")
        ),
    )]
    struct ApiDoc;

    struct SecurityAddon;

    impl Modify for SecurityAddon {
        fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
            let components = openapi.components.as_mut().unwrap();
            components.add_security_scheme(
                "csh",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .bearer_format("JWT")
                        .build(),
                ),
            );
        }
    }

    let openapi = ApiDoc::openapi();

    cfg.service(SwaggerUi::new("/api/docs/{_:.*}").url("/api/openapi.json", openapi))
        .service(
            scope("/api")
                .wrap(cors)
                .service(create_quote)
                .service(get_quotes)
                .service(get_users)
                .service(get_quote)
                .service(get_reports)
                .service(delete_quote)
                .service(hide_quote)
                .service(report_quote)
                .service(resolve_report)
                .service(vote_quote)
                .service(unvote_quote)
                .service(get_version)
                .service(favorite_quote)
                .service(unfavorite_quote)
                .service(toggle_kevlar)
                .service(get_kevlar),
        );
}

pub async fn get_app_data() -> Data<AppState> {
    let db = PgPoolOptions::new()
        .connect(&env::var("DATABASE_URL").expect("DATABASE_URL not set"))
        .await
        .expect("Could not connect to database");
    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .expect("Failed to run migrations");
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
