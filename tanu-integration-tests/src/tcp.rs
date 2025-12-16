use std::net::SocketAddr;

use tanu::{check, check_eq, eyre};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::OnceCell,
    task::JoinSet,
    time::{timeout, Duration},
};

static TCP_ADDR: OnceCell<SocketAddr> = OnceCell::const_new();

async fn tcp_addr() -> SocketAddr {
    *TCP_ADDR
        .get_or_init(|| async {
            let listener = TcpListener::bind("127.0.0.1:0")
                .await
                .expect("failed to bind TCP test server");
            let addr = listener
                .local_addr()
                .expect("failed to get TCP test server address");

            tokio::spawn(async move {
                loop {
                    let (stream, _) = match listener.accept().await {
                        Ok(v) => v,
                        Err(_) => break,
                    };
                    tokio::spawn(async move {
                        let _ = handle_conn(stream).await;
                    });
                }
            });

            addr
        })
        .await
}

async fn handle_conn(stream: TcpStream) -> eyre::Result<()> {
    let (read, mut write) = stream.into_split();
    let mut reader = BufReader::new(read);
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            return Ok(());
        }

        let line = line.trim_end_matches(['\n', '\r']);

        if line == "PING" {
            write.write_all(b"PONG\n").await?;
            continue;
        }

        if let Some(msg) = line.strip_prefix("ECHO ") {
            write.write_all(msg.as_bytes()).await?;
            write.write_all(b"\n").await?;
            continue;
        }

        if line == "CLOSE" {
            return Ok(());
        }

        if line == "NOOP" {
            continue;
        }

        write.write_all(b"ERR\n").await?;
    }
}

async fn send_line_and_read(addr: SocketAddr, request: &str) -> eyre::Result<String> {
    let stream = TcpStream::connect(addr).await?;
    let (read, mut write) = stream.into_split();
    let mut reader = BufReader::new(read);

    write.write_all(request.as_bytes()).await?;
    write.write_all(b"\n").await?;

    let mut line = String::new();
    reader.read_line(&mut line).await?;
    Ok(line.trim_end_matches(['\n', '\r']).to_string())
}

#[tanu::test]
async fn tcp_ping() -> eyre::Result<()> {
    let addr = tcp_addr().await;
    let response = send_line_and_read(addr, "PING").await?;
    check_eq!("PONG", response);
    Ok(())
}

#[tanu::test]
async fn tcp_echo_roundtrip() -> eyre::Result<()> {
    let addr = tcp_addr().await;
    let response = send_line_and_read(addr, "ECHO hello").await?;
    check_eq!("hello", response);
    Ok(())
}

#[tanu::test]
async fn tcp_unknown_command_returns_error() -> eyre::Result<()> {
    let addr = tcp_addr().await;
    let response = send_line_and_read(addr, "WHAT").await?;
    check_eq!("ERR", response);
    Ok(())
}

#[tanu::test]
async fn tcp_multiple_clients_concurrent() -> eyre::Result<()> {
    let addr = tcp_addr().await;

    let mut set = JoinSet::new();
    for _ in 0..10 {
        set.spawn(tanu::scope_current(async move {
            let response = send_line_and_read(addr, "PING").await?;
            check_eq!("PONG", response);
            eyre::Ok(())
        }));
    }

    while let Some(result) = set.join_next().await {
        result??;
    }

    Ok(())
}

#[tanu::test]
async fn tcp_timeout_waiting_for_response() -> eyre::Result<()> {
    let addr = tcp_addr().await;

    let stream = TcpStream::connect(addr).await?;
    let (read, mut write) = stream.into_split();
    let mut reader = BufReader::new(read);

    write.write_all(b"NOOP\n").await?;

    let mut line = String::new();
    let res = timeout(Duration::from_millis(100), reader.read_line(&mut line)).await;
    check!(res.is_err(), "expected read timeout");

    Ok(())
}

#[tanu::test]
async fn tcp_server_closes_connection() -> eyre::Result<()> {
    let addr = tcp_addr().await;

    let stream = TcpStream::connect(addr).await?;
    let (read, mut write) = stream.into_split();
    let mut reader = BufReader::new(read);

    write.write_all(b"CLOSE\n").await?;

    let mut buf = String::new();
    let n = reader.read_line(&mut buf).await?;
    check_eq!(0usize, n);

    Ok(())
}
