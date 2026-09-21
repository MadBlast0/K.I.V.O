//! End-to-end tests over the real transport (named pipes on Windows, sockets elsewhere).

use crate::frame::{codec, frames};
use crate::protocol::{Message, Outcome, Request, method};
use crate::transport::{self, unique_endpoint};
use crate::*;
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use kivo_core::event::{EventKind, UiEvent};
use kivo_core::{Event, EventBus, SessionState};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio_util::codec::Framed;
use tokio_util::sync::CancellationToken;

struct TestHandler {
    state: Mutex<StateSnapshot>,
}

impl TestHandler {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(StateSnapshot {
                session: SessionState::Idle,
                revision: 1,
            }),
        })
    }
}

impl Handler for TestHandler {
    fn snapshot(&self) -> StateSnapshot {
        self.state.lock().unwrap().clone()
    }

    fn call(&self, method: String, params: Value) -> BoxFuture<Result<Value, RpcError>> {
        Box::pin(async move {
            match method.as_str() {
                "echo" => Ok(params),
                "slow" => {
                    tokio::time::sleep(Duration::from_millis(300)).await;
                    Ok(json!("slow"))
                }
                "fail" => Err(RpcError::new(-1, "nope")),
                other => Err(RpcError::method_not_found(other)),
            }
        })
    }
}

struct Running {
    endpoint: String,
    token: String,
    bus: EventBus,
    handler: Arc<TestHandler>,
    shutdown: CancellationToken,
    task: JoinHandle<std::io::Result<()>>,
}

fn start_on(endpoint: String, hello_timeout: Duration) -> Running {
    let token = SessionToken::generate().unwrap();
    let presented = token.as_str().to_owned();
    let mut config = ServerConfig::new(endpoint.clone(), token, "test 1.0".into());
    config.hello_timeout = hello_timeout;
    let server = Server::bind(config).unwrap();
    let (bus, handler, shutdown) = (
        EventBus::new(),
        TestHandler::new(),
        CancellationToken::new(),
    );
    let task = tokio::spawn(server.run(handler.clone(), bus.clone(), shutdown.clone()));
    Running {
        endpoint,
        token: presented,
        bus,
        handler,
        shutdown,
        task,
    }
}

fn start() -> Running {
    start_on(unique_endpoint(), Duration::from_secs(3))
}

async fn raw(
    endpoint: &str,
) -> Framed<transport::ClientStream, tokio_util::codec::LengthDelimitedCodec> {
    frames(transport::connect(endpoint).await.unwrap())
}

async fn send_json(
    io: &mut Framed<transport::ClientStream, tokio_util::codec::LengthDelimitedCodec>,
    v: Value,
) {
    io.send(Bytes::from(serde_json::to_vec(&v).unwrap()))
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_client_gets_the_snapshot_answers_and_events() {
    let rt = start();
    let conn = connect(&rt.endpoint, &rt.token, "tests").await.unwrap();
    assert_eq!(conn.welcome.protocol_version, PROTOCOL_VERSION);
    assert_eq!(conn.welcome.runtime_version, "test 1.0");
    assert_eq!(conn.welcome.snapshot.session, SessionState::Idle);

    let c = &conn.client;
    assert_eq!(
        c.request(method::PING, Value::Null).await.unwrap(),
        json!("pong")
    );
    assert_eq!(
        c.request(method::STATE, Value::Null).await.unwrap(),
        json!({"session":"idle","revision":1})
    );
    assert_eq!(
        c.request("echo", json!({"a": [1, 2]})).await.unwrap(),
        json!({"a": [1, 2]})
    );
    let err = c.request("nope", Value::Null).await.unwrap_err();
    assert!(matches!(err, ClientError::Rejected(e) if e.code == RpcError::METHOD_NOT_FOUND));

    let mut notes = conn.notifications;
    let first = Event::new(EventKind::Ui(UiEvent::OverlayShown));
    let second = Event::new(EventKind::Ui(UiEvent::OverlayHidden));
    rt.bus.publish(first.clone());
    rt.bus.publish(second.clone());
    for expected in [first, second] {
        let n = notes.recv().await.unwrap();
        assert_eq!(n.method, method::EVENT);
        assert_eq!(serde_json::from_value::<Event>(n.params).unwrap(), expected);
    }
    rt.shutdown.cancel();
    rt.task.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn requests_are_answered_independently() {
    let rt = start();
    let conn = connect(&rt.endpoint, &rt.token, "tests").await.unwrap();
    let (slow, fast) = (conn.client.clone(), conn.client.clone());
    let slow = tokio::spawn(async move { slow.request("slow", Value::Null).await.unwrap() });
    tokio::time::sleep(Duration::from_millis(30)).await;
    let started = std::time::Instant::now();
    assert_eq!(fast.request("echo", json!(1)).await.unwrap(), json!(1));
    assert!(
        started.elapsed() < Duration::from_millis(250),
        "the fast call didn't wait for the slow one"
    );
    assert_eq!(slow.await.unwrap(), json!("slow"));
    rt.shutdown.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_wrong_token_is_refused_and_others_still_connect() {
    let rt = start();
    let err = connect(&rt.endpoint, "0".repeat(64).as_str(), "intruder")
        .await
        .err()
        .unwrap();
    assert!(
        matches!(err, ClientError::Rejected(ref e) if e.code == RpcError::UNAUTHORIZED),
        "{err}"
    );
    assert!(connect(&rt.endpoint, &rt.token, "tests").await.is_ok());
    rt.shutdown.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn another_protocol_major_is_refused_with_a_clear_message() {
    let rt = start();
    let mut io = raw(&rt.endpoint).await;
    let hello =
        json!({"protocolVersion": {"major": 2, "minor": 0}, "client": "future", "token": rt.token});
    send_json(
        &mut io,
        json!({"jsonrpc": "2.0", "id": 5, "method": "hello", "params": hello}),
    )
    .await;
    let reply: Message = serde_json::from_slice(&io.next().await.unwrap().unwrap()).unwrap();
    let Message::Response(r) = reply else {
        panic!("expected a response")
    };
    let Outcome::Error(e) = r.outcome else {
        panic!("expected an error")
    };
    assert_eq!((r.id, e.code), (5, RpcError::INCOMPATIBLE));
    assert!(
        e.message.contains("1.x") && e.message.contains("2.x"),
        "{}",
        e.message
    );
    assert!(io.next().await.is_none(), "the connection is closed");
    rt.shutdown.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn the_first_message_must_be_hello() {
    let rt = start();
    let mut io = raw(&rt.endpoint).await;
    send_json(
        &mut io,
        serde_json::to_value(Request::new(9, method::PING, Value::Null)).unwrap(),
    )
    .await;
    let reply: Message = serde_json::from_slice(&io.next().await.unwrap().unwrap()).unwrap();
    assert!(
        matches!(reply, Message::Response(r) if matches!(r.outcome, Outcome::Error(ref e) if e.code == RpcError::INVALID_REQUEST))
    );
    assert!(io.next().await.is_none());
    rt.shutdown.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn silent_clients_are_dropped_after_the_hello_timeout() {
    let rt = start_on(unique_endpoint(), Duration::from_millis(150));
    let mut io = raw(&rt.endpoint).await;
    let closed = tokio::time::timeout(Duration::from_secs(2), io.next())
        .await
        .expect("closed by the server");
    assert!(closed.is_none());
    rt.shutdown.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn oversized_frames_close_the_connection() {
    let rt = start();
    let conn = connect(&rt.endpoint, &rt.token, "tests").await.unwrap();
    // A second, raw connection that says hello properly and then sends 2 MiB.
    let mut io = Framed::new(
        transport::connect(&rt.endpoint).await.unwrap(),
        codec(8 * MAX_FRAME),
    );
    let hello = json!({"protocolVersion": PROTOCOL_VERSION, "client": "big", "token": rt.token});
    io.send(Bytes::from(
        serde_json::to_vec(&json!({"jsonrpc":"2.0","id":0,"method":"hello","params":hello}))
            .unwrap(),
    ))
    .await
    .unwrap();
    let _welcome = io.next().await.unwrap().unwrap();
    let _ = io.send(Bytes::from(vec![b' '; 2 * MAX_FRAME])).await;
    let rest: Vec<_> = io.collect().await;
    assert!(
        rest.iter().all(|f| f.is_ok()),
        "the server just closes; nothing malformed comes back"
    );
    assert_eq!(
        conn.client
            .request(method::PING, Value::Null)
            .await
            .unwrap(),
        json!("pong"),
        "others unaffected"
    );
    rt.shutdown.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_client_that_falls_behind_gets_a_snapshot() {
    let rt = start();
    let mut conn = connect(&rt.endpoint, &rt.token, "tests").await.unwrap();
    *rt.handler.state.lock().unwrap() = StateSnapshot {
        session: SessionState::Listening,
        revision: 42,
    };
    // Don't read notifications while the bus overflows the channel, the pipe and the server.
    for _ in 0..20_000 {
        rt.bus
            .publish(Event::new(EventKind::Ui(UiEvent::OverlayShown)));
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    let mut saw_snapshot = false;
    while let Ok(Some(n)) =
        tokio::time::timeout(Duration::from_millis(500), conn.notifications.recv()).await
    {
        if n.method == method::SNAPSHOT {
            assert_eq!(
                serde_json::from_value::<StateSnapshot>(n.params)
                    .unwrap()
                    .revision,
                42
            );
            saw_snapshot = true;
            break;
        }
    }
    assert!(
        saw_snapshot,
        "a lagging client is resynchronized with a snapshot"
    );
    rt.shutdown.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn clients_reconnect_after_the_runtime_restarts_with_a_new_token() {
    let dir = tempfile::tempdir().unwrap();
    let token_file = dir.path().join("session.token");
    let endpoint = unique_endpoint();

    let first = start_on(endpoint.clone(), Duration::from_secs(3));
    std::fs::write(&token_file, &first.token).unwrap();
    let read = || std::fs::read_to_string(&token_file);
    let cancel = CancellationToken::new();
    let mut conn = connect_with_backoff(&endpoint, read, "tests", &cancel)
        .await
        .unwrap();

    // The runtime goes away: in-flight and new requests fail, notifications end.
    first.shutdown.cancel();
    first.task.await.unwrap().unwrap();
    assert!(conn.notifications.recv().await.is_none());
    assert!(matches!(
        conn.client.request(method::PING, Value::Null).await,
        Err(ClientError::Closed)
    ));

    // It comes back later with a new token; the client keeps trying and then gets a fresh snapshot.
    let endpoint2 = endpoint.clone();
    let token_file2 = token_file.clone();
    let restarted = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(400)).await;
        let rt = start_on(endpoint2, Duration::from_secs(3));
        std::fs::write(&token_file2, &rt.token).unwrap();
        rt
    });
    let conn = connect_with_backoff(&endpoint, read, "tests", &cancel)
        .await
        .unwrap();
    assert_eq!(conn.welcome.snapshot.session, SessionState::Idle);
    assert_eq!(
        conn.client
            .request(method::PING, Value::Null)
            .await
            .unwrap(),
        json!("pong")
    );
    restarted.await.unwrap().shutdown.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn backoff_stops_when_cancelled() {
    let cancel = CancellationToken::new();
    let stop = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        stop.cancel();
    });
    let endpoint = unique_endpoint();
    let result = connect_with_backoff(&endpoint, || Ok(String::new()), "tests", &cancel).await;
    assert!(matches!(result, Err(ClientError::Cancelled)));
}
