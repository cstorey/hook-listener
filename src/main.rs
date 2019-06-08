use actix_web::{middleware, web, App, Error, HttpResponse, HttpServer};
use failure::Fallible;
use futures::{future::ok, Future};
use log::*;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "hook-listener", about = "JSON webhook listener")]
struct Opt {
    #[structopt(short = "b", long = "bind")]
    bind: std::net::SocketAddr,
}

/// async handler
fn ingest(
    (path, body): (web::Path<String>, web::Json<serde_json::Value>),
) -> impl Future<Item = HttpResponse, Error = Error> {
    let path = path.into_inner();
    let body = body.into_inner();
    ok(()).and_then(move |()| {
        info!(
            "Path: {}; body: {}",
            path,
            serde_json::to_string_pretty(&body)?
        );
        Ok(HttpResponse::Ok()
            .content_type("text/plain")
            .body(format!("Hello {:?}!\n", path)))
    })
}

fn main() -> Fallible<()> {
    env_logger::init();
    let opt = Opt::from_args();

    let sys = actix_rt::System::new("basic-example");
    let serv = HttpServer::new(|| {
        App::new()
            // enable logger - always register actix-web Logger middleware last
            .wrap(middleware::Logger::default())
            // with path parameters
            .service(web::resource("/gh/{path:.*}").route(web::post().to_async(ingest)))
    })
    .bind(&opt.bind)?;

    info!("Starting http server: {:?}", serv.addrs());
    serv.start();

    sys.run()?;
    Ok(())
}
