//use serde::Deserialize;
//
#[macro_use]
extern crate lazy_static;

use core::convert::Infallible;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Client, Response, Server};
use serde::Deserialize;

use futures_intrusive::sync::Semaphore;

use futures::try_join;

lazy_static! {
    static ref CONFIG: Config = get_config();
    static ref SEM: Semaphore = Semaphore::new(false, CONFIG.container_concurrency);
}

#[tokio::main]
async fn main() {
    let make_prometheus_svc = make_service_fn(|_| {
        async {
            Ok::<_, Infallible>(service_fn(|_req| {
                async {
                    let resp = Response::builder()
                        .status(200)
                        .body(Body::from("metrics"))
                        .unwrap();

                    Ok::<_, Infallible>(resp)
                }
            }))
        }
    });

    let make_proxy_svc = make_service_fn(|_| {
        async {
            Ok::<_, Infallible>(service_fn(|mut req| {
                async {
                    let _permit = SEM.acquire(1).await;

                    let user_addr = CONFIG.user_port;
                    let target = format!(
                        "http://127.0.0.1:{:?}{:?}",
                        &user_addr,
                        req.uri().path_and_query().unwrap()
                    );

                    let client = Client::new();
                    *req.uri_mut() = target.parse().unwrap();
                    client.request(req).await
                }
            }))
        }
    });

    let proxy_addr = ([127, 0, 0, 1], CONFIG.queue_serving_port).into();

    println!("Starting on {}", CONFIG.queue_serving_port);
    let proxy = Server::bind(&proxy_addr).serve(make_proxy_svc);

    let metrics_addr = ([127, 0, 0, 1], CONFIG.metrics_port).into();
    let metrics = Server::bind(&metrics_addr).serve(make_prometheus_svc);

    if let Err(err) = try_join!(proxy, metrics) {
        eprintln!("server error: {}", err);
    }
}

#[derive(Deserialize, Debug)]
struct Config {
    container_concurrency: usize,
    queue_serving_port: u16,

    #[serde(default = "default_metrics_port")]
    metrics_port: u16,

    user_port: u16,
}

fn get_config() -> Config {
    return match envy::from_env::<Config>() {
        Ok(config) => config,
        Err(error) => panic!("{:#?}", error),
    };
}

fn default_metrics_port() -> u16 {
    9090
}
