use actix_web::{middleware, web, App, Error, HttpResponse, HttpServer};
use failure::Fallible;
use futures::{future::ok, Future};
use log::*;
use pg_queue::Producer;
use r2d2::Pool;
use r2d2_postgres::{PostgresConnectionManager, TlsMode};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "hook-listener", about = "JSON webhook listener")]
struct Opt {
    #[structopt(short = "b", long = "bind")]
    bind: std::net::SocketAddr,
    #[structopt(short = "p", long = "pgsql")]
    postgresql: String,
}

/// async handler
fn ingest(
    (path, body, pool): (
        web::Path<String>,
        web::Json<serde_json::Value>,
        web::Data<r2d2::Pool<r2d2_postgres::PostgresConnectionManager>>,
    ),
) -> impl Future<Item = HttpResponse, Error = Error> {
    let path = path.into_inner();
    let body = body.into_inner();
    ok(())
        .and_then(|()| {
            web::block(move || -> Fallible<()> {
                let mut producer = Producer::new(pool.get_ref().clone())?;
                let content = serde_json::to_vec(&body)?;
                let ver = producer.produce(path.as_bytes(), &content)?;
                debug!("Path: {} → {:?}", path, ver);
                Ok(())
            })
            .map_err(|e: actix_threadpool::BlockingError<_>| e.into())
        })
        .and_then(|()| Ok(HttpResponse::Ok().content_type("text/plain").body("ok\n")))
}

fn main() -> Fallible<()> {
    env_logger::init();
    let opt = Opt::from_args();
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
