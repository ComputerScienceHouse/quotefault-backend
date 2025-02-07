use crate::app::AppState;
use actix_web::{
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
    web::Data,
    FromRequest, HttpMessage, HttpResponse,
};
use anyhow::{anyhow, Result};
use base64::{engine::general_purpose, Engine as _};
use futures::{executor::block_on, future::LocalBoxFuture, lock::Mutex};
use isahc::ReadResponseExt;
use lazy_static::lazy_static;
use log::{log, Level};
use openssl::{
    bn::BigNum,
    hash::MessageDigest,
    pkey::{PKey, Public},
    rsa::Rsa,
    sign::Verifier,
};
use serde::{Deserialize, Serialize};
use sqlx::{query, Pool, Postgres};
use std::{
    borrow::Borrow,
    collections::{HashMap, HashSet},
    env,
    future::{ready, Ready},
    hash::Hash,
    sync::{Arc, RwLock},
    task::{Context, Poll},
};

lazy_static! {
    static ref JWT_CACHE: Arc<Mutex<HashMap<String, PKey<Public>>>> =
        Arc::new(Mutex::new(HashMap::new()));
}

static KEVLAR_USERS: RwLock<Option<HashSet<String>>> = RwLock::new(None);

async fn get_kevlar_users_inner(db: &Pool<Postgres>) -> Result<HashSet<String>, sqlx::Error> {
    Ok(query!("select uid from kevlar where enabled")
        .fetch_all(db)
        .await?
        .into_iter()
        .map(|x| x.uid)
        .collect())
}

pub async fn get_kevlar_users(db: &Pool<Postgres>) -> Result<HashSet<String>, sqlx::Error> {
    if KEVLAR_USERS.read().unwrap().is_none() {
        *KEVLAR_USERS.write().unwrap() = Some(get_kevlar_users_inner(db).await?);
    }

    Ok(KEVLAR_USERS.read().unwrap().clone().unwrap())
}

pub async fn user_has_kevlar(db: &Pool<Postgres>, uid: &str) -> Result<bool, sqlx::Error> {
    if KEVLAR_USERS.read().unwrap().is_none() {
        *KEVLAR_USERS.write().unwrap() = Some(get_kevlar_users_inner(db).await?);
    }

    Ok(KEVLAR_USERS.read().unwrap().as_ref().unwrap().contains(uid))
}

pub async fn any_user_has_kevlar<T>(db: &Pool<Postgres>, uid: &[T]) -> Result<bool, sqlx::Error>
where
    String: Borrow<T>,
    T: Eq + Hash,
{
    if KEVLAR_USERS.read().unwrap().is_none() {
        *KEVLAR_USERS.write().unwrap() = Some(get_kevlar_users_inner(db).await?);
    }

    if let Some(users) = KEVLAR_USERS.read().unwrap().as_ref() {
        Ok(uid.iter().any(|u| users.contains(u)))
    } else {
        unreachable!()
    }
}

pub async fn edit_kevlar_cache(
    db: &Pool<Postgres>,
    mut f: impl FnMut(&mut HashSet<String>),
) -> Result<(), sqlx::Error> {
    if KEVLAR_USERS.read().unwrap().is_none() {
        *KEVLAR_USERS.write().unwrap() = Some(get_kevlar_users_inner(db).await?);
    }

    f(KEVLAR_USERS.write().unwrap().as_mut().unwrap());

    Ok(())
}

pub(crate) fn clear_kevlar_cache() {
    *KEVLAR_USERS.write().unwrap() = None;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TokenHeader {
    alg: String,
    kid: String,
    typ: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    exp: u32,
    iat: u32,
    auth_time: Option<u32>,
    jti: String,
    iss: String,
    aud: String,
    sub: String,
    typ: String,
    azp: String,
    nonce: Option<String>,
    session_state: Option<String>,
    scope: String,
    sid: Option<String>,
    email_verified: bool,
    pub name: Option<String>,
    pub groups: Vec<String>,
    pub preferred_username: String,
    pub given_name: Option<String>,
    pub family_name: Option<String>,
    pub email: Option<String>,
}

impl FromRequest for User {
    type Error = actix_web::error::Error;
    type Future = LocalBoxFuture<'static, Result<Self, Self::Error>>;

    fn from_request(
        req: &actix_web::HttpRequest,
        _payload: &mut actix_web::dev::Payload,
    ) -> Self::Future {
        let unauthorized = || {
            Box::pin(async {
                <Result<Self, Self::Error>>::Err(actix_web::error::ErrorUnauthorized(""))
            })
        };

        let h = match req.headers().get("Authorization").map(|h| {
            h.to_str()
                .unwrap_or("")
                .to_string()
                .trim_start_matches("Bearer ")
                .to_string()
        }) {
            Some(h) => h,
            None => return unauthorized(),
        };

        let (head, head_64, user, user_64, sig) = match get_token_pieces(h) {
            Ok(vals) => vals,
            Err(_) => return unauthorized(),
        };

        if verify_token(&head, &head_64, &user, &user_64, &sig) {
            Box::pin(async { Ok(user) })
        } else {
            unauthorized()
        }
    }
}

impl User {
    pub fn admin(&self) -> bool {
        self.groups.contains(&String::from("eboard")) || self.groups.contains(&String::from("rtp"))
    }

    pub fn eboard(&self) -> bool {
        self.groups.contains(&String::from("eboard"))
    }
}

#[doc(hidden)]
pub struct CSHAuthService<S> {
    service: S,
    enabled: bool,
    admin_only: bool,
    eboard_only: bool,
}

fn get_token_pieces(token: String) -> Result<(TokenHeader, String, User, String, Vec<u8>)> {
    let mut it = token.split('.');
    let token_header_base64 = it.next().ok_or(anyhow!("!header"))?;
    let token_header = general_purpose::URL_SAFE_NO_PAD.decode(token_header_base64)?;
    let token_header: TokenHeader = serde_json::from_slice(&token_header)?;
    let token_payload_base64 = it.next().ok_or(anyhow!("!body"))?;
    let token_payload = general_purpose::URL_SAFE_NO_PAD.decode(token_payload_base64)?;
    let token_payload: User = serde_json::from_slice(&token_payload)?;
    let token_signature = it.next().ok_or(anyhow!("signature"))?;
    let token_signature = general_purpose::URL_SAFE_NO_PAD.decode(token_signature)?;
    Ok((
        token_header,
        token_header_base64.to_owned(),
        token_payload,
        token_payload_base64.to_owned(),
        token_signature,
    ))
}

#[allow(unused_must_use)]
fn verify_token(
    header: &TokenHeader,
    header_64: &String,
    payload: &User,
    payload_64: &String,
    key: &[u8],
) -> bool {
    if payload.exp < (chrono::Utc::now().timestamp() as u32) {
        return false;
    }
    if header.alg != "RS256" {
        return false;
    }

    let data_cache = JWT_CACHE.clone();
    let mut cache = block_on(data_cache.lock());
    let pkey = match cache.get(header.kid.as_str()) {
        Some(x) => Some(x),
        None => {
            update_cache(&mut cache).unwrap();
            cache.get(header.kid.as_str())
        }
    };

    let pkey = match pkey {
        Some(p) => p,
        None => return false,
    };

    let mut verifier = Verifier::new(MessageDigest::sha256(), pkey).unwrap();
    verifier.update(header_64.as_bytes());
    verifier.update(b".");
    verifier.update(payload_64.as_bytes());
    verifier.verify(key).unwrap_or(false)
}

impl<S> Service<ServiceRequest> for CSHAuthService<S>
where
    S: Service<
        ServiceRequest,
        Response = ServiceResponse<actix_web::body::BoxBody>,
        Error = actix_web::Error,
    >,
    S::Future: 'static,
{
    type Response = ServiceResponse<actix_web::body::BoxBody>;
    type Error = actix_web::Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&self, ctx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(ctx)
    }

    #[allow(unused_must_use)]
    fn call(&self, req: ServiceRequest) -> Self::Future {
        let _app_data: &Data<AppState> = req.app_data().unwrap();
        if self.enabled {
            let unauthorized = |req: ServiceRequest| -> Self::Future {
                Box::pin(async { Ok(req.into_response(HttpResponse::Unauthorized().finish())) })
            };

            let internal_error = |req: ServiceRequest| -> Self::Future {
                Box::pin(async {
                    Ok(req.into_response(HttpResponse::InternalServerError().finish()))
                })
            };

            let token = match req.headers().get("Authorization").map(|x| x.to_str()) {
                Some(Ok(x)) => x.trim_start_matches("Bearer ").to_string(),
                _ => return unauthorized(req),
            };

            let (
                token_header,
                token_header_base64,
                token_payload,
                token_payload_base64,
                token_signature,
            ) = match get_token_pieces(token) {
                Ok(x) => x,
                Err(e) => {
                    log!(Level::Debug, "Token is formated incorrectly: {e}");
                    return unauthorized(req);
                }
            };

            let verified = verify_token(
                &token_header,
                &token_header_base64,
                &token_payload,
                &token_payload_base64,
                &token_signature,
            );

            if verified {
                req.extensions_mut().insert(token_payload.clone());
            } else {
                return unauthorized(req);
            }

            let kevlar_users = match futures::executor::block_on(get_kevlar_users(&_app_data.db)) {
                Ok(users) => users,
                Err(_) => return internal_error(req),
            };

            if kevlar_users.contains(&token_payload.preferred_username) {
                log!(
                    Level::Debug,
                    "Unauthorized due to Kevlar: {}",
                    token_payload.preferred_username
                );
                return unauthorized(req);
            }

            if self.admin_only && !token_payload.admin() {
                return unauthorized(req);
            }

            if self.eboard_only && !token_payload.eboard() {
                return unauthorized(req);
            }

            let future = self.service.call(req);
            return Box::pin(async move {
                let response = future.await?;
                Ok(response)
            });
        }
        let future = self.service.call(req);
        Box::pin(async move {
            let response = future.await?;
            Ok(response)
        })
    }
}

#[derive(Clone, Debug)]
pub struct CSHAuth {
    enabled: bool,
    admin: bool,
    eboard: bool,
}

lazy_static! {
    pub static ref SECURITY_ENABLED: bool = env::var("SECURITY_ENABLED")
        .map(|x| x.parse::<bool>().unwrap_or(true))
        .unwrap_or(true);
}

impl CSHAuth {
    pub fn admin_only() -> Self {
        Self {
            enabled: *SECURITY_ENABLED,
            admin: true,
            eboard: false,
        }
    }

    pub fn eboard_only() -> Self {
        Self {
            enabled: *SECURITY_ENABLED,
            admin: false,
            eboard: true,
        }
    }

    pub fn enabled() -> Self {
        Self {
            enabled: *SECURITY_ENABLED,
            admin: false,
            eboard: false,
        }
    }

    pub fn disabled() -> Self {
        Self {
            enabled: false,
            admin: false,
            eboard: false,
        }
    }
}

impl<S> Transform<S, ServiceRequest> for CSHAuth
where
    S: Service<
        ServiceRequest,
        Response = ServiceResponse<actix_web::body::BoxBody>,
        Error = actix_web::Error,
    >,
    S::Future: 'static,
{
    type Response = ServiceResponse<actix_web::body::BoxBody>;
    type Error = actix_web::Error;
    type Transform = CSHAuthService<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(CSHAuthService {
            service,
            enabled: self.enabled,
            admin_only: self.admin,
            eboard_only: self.eboard,
        }))
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct CertKey {
    kid: String,
    kty: String,
    alg: String,
    r#use: String,
    n: String,
    e: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct CertData {
    keys: Vec<CertKey>,
}

pub fn update_cache(cache: &mut HashMap<String, PKey<Public>>) -> Result<()> {
    let cert_data: CertData =
        isahc::get("https://sso.csh.rit.edu/auth/realms/csh/protocol/openid-connect/certs")
            .unwrap()
            .json()
            .unwrap();

    for key in cert_data.keys {
        if cache.contains_key(key.kid.as_str()) {
            continue;
        }
        let n: Vec<String> = general_purpose::URL_SAFE_NO_PAD
            .decode(key.n.as_bytes())?
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect();
        let e: Vec<String> = general_purpose::URL_SAFE_NO_PAD
            .decode(key.e.as_bytes())?
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect();
        let n = BigNum::from_hex_str(&n.join(""))?;
        let e = BigNum::from_hex_str(&e.join(""))?;
        let rsa = Rsa::from_public_components(n, e)?;
        cache.insert(key.kid, PKey::from_rsa(rsa)?);
    }
    Ok(())
}
