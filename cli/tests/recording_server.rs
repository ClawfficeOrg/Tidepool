//! One test for the recorded server itself.
//!
//! The fixture both the SDK and the CLI tests use has to wait for a request
//! that arrives just after the connection is accepted. On macOS and BSD a
//! non-blocking listener hands its flag to the accepted socket, so an early
//! read fails with `WouldBlock`; the fixture used to drop that connection under
//! the client, which surfaced as a rare `Connection reset by peer`.

mod support;

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use support::{Reply, ScriptedServer};

#[test]
fn the_recorded_server_waits_for_a_request_that_arrives_after_the_accept() {
    let server = ScriptedServer::start(vec![Reply::json(200, "{}")]);
    let address = server.url().strip_prefix("http://").expect("http URL");
    let mut stream = TcpStream::connect(address).expect("connect");
    // Let the server accept before the request exists, so a non-blocking read
    // would see `WouldBlock` rather than the request.
    std::thread::sleep(Duration::from_millis(50));
    stream
        .write_all(b"GET /v1/hosts HTTP/1.1\r\nhost: test\r\n\r\n")
        .expect("write request");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read timeout");
    let mut response = String::new();
    stream.read_to_string(&mut response).expect("read response");

    assert!(response.starts_with("HTTP/1.1 200 OK"), "got {response:?}");
    let requests = server.join();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, "/v1/hosts");
}
