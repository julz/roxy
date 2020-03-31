use assert_cmd::prelude::*;
use std::process::Command;

use portpicker::pick_unused_port;

use httptest::{mappers::*, responders::*, Expectation, Server};
use std::sync::atomic::{AtomicUsize, Ordering};

use std::sync::Arc;
use tokio::runtime::Runtime;

use serde::Deserialize;

use std::error::Error;
type BoxResult<T> = Result<T, Box<dyn Error>>;

#[test]
fn respects_concurrency_one() -> BoxResult<()> {
    let server = Server::run();
    let thread_count = Arc::new(AtomicUsize::new(0));
    server.expect(
        Expectation::matching(request::method_path("GET", "/hello"))
            .times(2)
            .respond_with(move || {
                let thread_count = Arc::clone(&thread_count);
                let current_threads = thread_count.fetch_add(1, Ordering::Relaxed);

                println!("current {}", current_threads.to_string());

                if current_threads > 0 {
                    panic!("more than one request at once came through!");
                }

                futures::executor::block_on(tokio::time::delay_for(
                    std::time::Duration::from_millis(2000),
                ));

                thread_count.fetch_sub(1, Ordering::Relaxed);

                status_code(200).body("proxy me")
            }),
    );

    let queue_port = pick_unused_port().expect("No ports free").to_string();
    let metrics_port = pick_unused_port().expect("No ports free").to_string();

    let _result = Command::cargo_bin("roxy")?
        .arg("foobar")
        .env("CONTAINER_CONCURRENCY", "1")
        .env("QUEUE_SERVING_PORT", &queue_port)
        .env("METRICS_PORT", &metrics_port)
        .env("USER_PORT", server.addr().port().to_string())
        .env("REVISION_TIMEOUT_SECONDS", "60")
        .spawn()?;

    std::thread::sleep(std::time::Duration::from_millis(250)); // yuck :/

    let request_url = format!("http://localhost:{port}/hello", port = queue_port);
    let first_req = reqwest::get(&request_url);
    let second_req = reqwest::get(&request_url);

    let mut rt = Runtime::new().unwrap();
    rt.block_on(async {
        let (res1, res2) = futures::join!(first_req, second_req);
        assert_eq!(res1.unwrap().status(), 200);
        assert_eq!(res2.unwrap().status(), 200);
    });

    Ok(())
}

#[test]
fn reports_prometheus_metrics() -> BoxResult<()> {
    let user_container = Server::run();
    // user_container.expect(
    //     Expectation::matching(request::method_path("GET", "/hello"))
    //         .respond_with(status_code(200).body("proxy me")),
    // );

    let queue_port = pick_unused_port().expect("No ports free").to_string();
    let metrics_port = pick_unused_port().expect("No ports free").to_string();
    let _result = Command::cargo_bin("roxy")?
        .arg("foobar")
        .env("CONTAINER_CONCURRENCY", "1")
        .env("QUEUE_SERVING_PORT", &queue_port)
        .env("METRICS_PORT", &metrics_port)
        .env("USER_PORT", user_container.addr().port().to_string())
        .env("REVISION_TIMEOUT_SECONDS", "60")
        .spawn()?;

    std::thread::sleep(std::time::Duration::from_millis(250)); // yuck :/

    let json: PromMetrics =
        reqwest::blocking::get(&(format!("http://127.0.0.1:{}/metrics", metrics_port)))
            .expect("couldn't get metrics")
            .json()?;

    println!("{:?}", json);

    Ok(())
}

#[derive(Deserialize, Debug)]
struct PromMetrics {}
