//! Drives the public async API. These will `todo!()`-panic until the matching
//! step is done — implement the function, then the test goes green.
//!
//! Everything here binds real localhost sockets on OS-assigned ports (`:0`),
//! so the tests are hermetic — no fixed ports, no external network, no flakes
//! from a port already in use. `#[tokio::test]` spins up a runtime per test.
//!
//! Four things to prove:
//!   1. the target parser expands/validates specs (pure, also unit-tested)
//!   2. `scan` reports a listening port as Open and a dead one as Closed
//!   3. a probe to a black hole times out as Filtered (timeout = cancellation)
//!   4. the proxy forwards bytes end-to-end and drains on shutdown

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use aprobe::scanner::{scan, ScanConfig};
use aprobe::shutdown::Shutdown;
use aprobe::target::{parse_target, PortState, Target};

/// Bind an ephemeral listener and return (listener, its port).
async fn ephemeral() -> (TcpListener, u16) {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    (l, port)
}

#[test]
fn parser_expands_and_dedups() {
    let t = parse_target("127.0.0.1:80-82,81,443").unwrap();
    assert_eq!(t.host, "127.0.0.1");
    assert_eq!(t.ports, vec![80, 81, 82, 443]);
}

#[tokio::test]
async fn scan_finds_an_open_port() {
    // A live listener on a known port → Open. A neighbor with nothing on it
    // → Closed (localhost RSTs immediately).
    let (listener, open_port) = ephemeral().await;
    // Keep accepting so the connect completes rather than racing a backlog drop.
    tokio::spawn(async move {
        loop {
            if listener.accept().await.is_err() {
                break;
            }
        }
    });

    // open_port is live. For a guaranteed-dead port, bind a second ephemeral
    // listener and immediately drop it: the OS just released that port, nothing
    // listens there, so a connect RSTs → Closed. Robust under parallel tests,
    // unlike guessing open_port+1 (which can land on another test's listener).
    let (dead, dead_port) = ephemeral().await;
    drop(dead);

    let target = Target {
        host: "127.0.0.1".to_string(),
        ports: vec![open_port, dead_port],
    };
    let cfg = ScanConfig {
        concurrency: 8,
        timeout: Duration::from_millis(300),
    };

    let out = scan(&target, &cfg).await;
    let state = |p: u16| out.iter().find(|o| o.port == p).map(|o| o.state);

    assert_eq!(state(open_port), Some(PortState::Open), "live port should be Open");
    assert_eq!(
        state(dead_port),
        Some(PortState::Closed),
        "dead localhost port should RST → Closed"
    );
}

#[tokio::test]
async fn unreachable_host_times_out_as_filtered() {
    // 10.255.255.1 is non-routable on most networks — the SYN black-holes, so
    // the connect neither succeeds nor is refused before the (short) timeout.
    // That dropped connect future is the cancellation; the verdict is Filtered.
    let target = Target {
        host: "10.255.255.1".to_string(),
        ports: vec![80],
    };
    let cfg = ScanConfig {
        concurrency: 1,
        timeout: Duration::from_millis(150),
    };

    let out = scan(&target, &cfg).await;
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].state, PortState::Filtered);
}

#[tokio::test]
async fn proxy_forwards_bytes_then_drains() {
    // Stand up an echo upstream, point the proxy at it, connect a client
    // through the proxy, and assert the bytes round-trip. Then trigger
    // shutdown and confirm run_proxy returns (graceful drain).
    let (up_listener, _up_port) = ephemeral().await;
    let up_addr = up_listener.local_addr().unwrap().to_string();
    tokio::spawn(async move {
        if let Ok((mut sock, _)) = up_listener.accept().await {
            let mut buf = [0u8; 64];
            if let Ok(n) = sock.read(&mut buf).await {
                let _ = sock.write_all(&buf[..n]).await;
            }
        }
    });

    // Pick a proxy port, then hand the *address* to run_proxy (it binds).
    let (probe, proxy_port) = ephemeral().await;
    drop(probe); // free it so run_proxy can bind the same port
    let listen = format!("127.0.0.1:{proxy_port}");

    let shutdown = Shutdown::new();
    let sd = shutdown.clone();
    let up = up_addr.clone();
    let listen_for_task = listen.clone();
    let proxy_task =
        tokio::spawn(async move { aprobe::proxy::run_proxy(&listen_for_task, &up, sd).await });

    // Give the listener a moment to bind, then drive a request through it.
    tokio::time::sleep(Duration::from_millis(50)).await;
    let mut client = TcpStream::connect(&listen).await.unwrap();
    client.write_all(b"ping").await.unwrap();
    let mut buf = [0u8; 4];
    client.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"ping", "bytes should round-trip through the proxy");

    // Graceful shutdown: trigger, and run_proxy should return.
    shutdown.trigger();
    let joined = tokio::time::timeout(Duration::from_secs(2), proxy_task).await;
    assert!(joined.is_ok(), "proxy should drain and return after shutdown");
}
