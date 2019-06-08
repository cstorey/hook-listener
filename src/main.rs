use actix_web;
use log::*;

use actix_web::{middleware, web, App, Error, HttpResponse, HttpServer};
use failure::Fallible;
use futures::{future::ok, Future};

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
    let sys = actix_rt::System::new("basic-example");
    HttpServer::new(|| {
        App::new()
            // enable logger - always register actix-web Logger middleware last
            .wrap(middleware::Logger::default())
            // with path parameters
            .service(web::resource("/gh/{path:.*}").route(web::post().to_async(ingest)))
    })
    .bind("127.0.0.1:8080")?
    .start();

    info!("Starting http server: 127.0.0.1:8080");
    sys.run()?;
    Ok(())
}
