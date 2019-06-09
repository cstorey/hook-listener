use std::collections::BTreeMap;

use actix_web::{error, http, middleware, web, App, Error, HttpResponse, HttpServer};
use failure::Fallible;
use futures::stream::Stream;
use futures::Future;
use hex::FromHex;
use log::*;
use pg_queue::Producer;
use r2d2::Pool;
use r2d2_postgres::{PostgresConnectionManager, TlsMode};
use ring::hmac;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "hook-listener", about = "JSON webhook listener")]
struct Opt {
    #[structopt(short = "b", long = "bind")]
    bind: std::net::SocketAddr,
    #[structopt(short = "p", long = "pgsql")]
    postgresql: String,
    #[structopt(short = "s", long = "secret-key")]
    secret: String,
}

#[derive(Debug, Clone)]
struct Verifier {
    key: Vec<u8>,
}

impl Verifier {
    fn check(&self, body: &[u8], value: &http::HeaderValue) -> Result<(), Error> {
        let mut it = value.as_bytes().split(|c| *c == b'=');
        let algo_name = it
            .next()
            .ok_or_else(|| error::ErrorBadRequest("No algorithm found"))?;
        debug!(
            "hub signature algorithm: {:?}",
            String::from_utf8_lossy(&algo_name)
        );
        let sig = it
            .next()
            .ok_or_else(|| error::ErrorBadRequest("No signature found"))
            .and_then(|s| {
                Vec::from_hex(s)
                    .map_err(|e| error::ErrorBadRequest(format!("Parsing signature: {}", e)))
            })?;
        debug!("hub signature value: {:?}", String::from_utf8_lossy(&sig));
        if it.next().is_some() {
            return Err(error::ErrorBadRequest("More than one '='"));
        }

        let algo = match algo_name {
            b"sha1" => &ring::digest::SHA1,
            other => {
                return Err(error::ErrorBadRequest(format!(
                    "Invalid hmac method {}",
                    String::from_utf8_lossy(other)
                )))
            }
        };
        let key = hmac::VerificationKey::new(algo, &self.key);

        hmac::verify(&key, body, &sig).map_err(|e| {
            error!("Verifying signature: {}", e);
            error::ErrorBadRequest("Invalid signature")
        })?;

        Ok(())
    }
}

/// async handler
fn ingest(
    (path, body, pool, verifier, req): (
        web::Path<String>,
        web::Payload,
        web::Data<r2d2::Pool<r2d2_postgres::PostgresConnectionManager>>,
        web::Data<Verifier>,
        web::HttpRequest,
    ),
) -> impl Future<Item = HttpResponse, Error = Error> {
    let path = path.into_inner();
    debug!(
        "H: {:?}",
        req.headers()
            .into_iter()
            .map(|(k, v)| {
                (
                    k.as_str().to_string(),
                    String::from_utf8_lossy(v.as_bytes()).into_owned(),
                )
            })
            .collect::<BTreeMap<String, String>>()
    );
    body.map_err(Error::from)
        .fold(web::BytesMut::new(), move |mut body, chunk| {
            body.extend_from_slice(&chunk);
            Ok::<_, Error>(body)
        })
        .and_then(move |bytes| {
            let sig = req
                .headers()
                .get("x-hub-signature")
                .ok_or_else(|| error::ErrorBadRequest("missing x-hub-signature"))?;
            debug!("x-hub-signature: {:?}", sig);
            verifier.check(&bytes, sig)?;
            Ok(bytes)
        })
        .and_then(|bytes| {
            let json = serde_json::from_slice::<serde_json::Value>(&bytes)?;
            Ok(json)
        })
        .and_then(|body| {
            web::block(move || -> Fallible<()> {
                let mut producer = Producer::new(pool.get_ref().clone())?;
                let content = serde_json::to_vec(&body)?;
                let ver = producer.produce(path.as_bytes(), &content)?;
                info!("Path: {} → {:?}", path, ver);
                Ok(())
            })
            .map_err(|e: actix_threadpool::BlockingError<_>| e.into())
        })
        .and_then(|()| Ok(HttpResponse::Ok().content_type("text/plain").body("ok\n")))
}

fn main() -> Fallible<()> {
    env_logger::init();
    let opt = Opt::from_args();

    let verifier = Verifier {
        key: opt.secret.as_bytes().to_vec(),
    };

    let pool = Pool::builder()
        .max_size(1)
        .build(PostgresConnectionManager::new(
            opt.postgresql,
            TlsMode::None,
        )?)?;
    pg_queue::setup(&*pool.get().expect("borrow from pool"))?;

    let sys = actix_rt::System::new("basic-example");
    let serv = HttpServer::new(move || {
        App::new()
            // enable logger - always register actix-web Logger middleware last
            .wrap(middleware::Logger::default())
            .data(verifier.clone())
            .data(pool.clone())
            // with path parameters
            .service(web::resource("/gh/{path:.*}").route(web::post().to_async(ingest)))
    })
    .bind(&opt.bind)?;

    info!("Starting http server: {:?}", serv.addrs());
    serv.start();

    sys.run()?;
    Ok(())
}
