use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use std::env;
use std::net::SocketAddr;
use tokio::sync::mpsc::Sender;
use dropshot::{endpoint, HttpError, HttpResponseUpdatedNoContent, HttpServerStarter, RequestContext, TypedBody};
use std::sync::{Arc, Mutex};

const DEFAULT_WIDTH: usize = 48;
const DEFAULT_HEIGHT: usize = 24;
const RGB_SIZE: usize = 3;

lazy_static::lazy_static! {
    static ref FORWARD_ADDR: SocketAddr = "127.0.0.1:9090".parse().expect("Invalid forward address");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (tx, mut rx) = mpsc::channel::<Stuff>(1000); // Buffer size of 1000

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <start_port>", args[0]);
        return Ok(());
    }

    let start_port: u16 = args[1].parse().expect("Invalid port number");

    let height: usize = args
        .get(2)
        .and_then(|h| h.parse().ok())
        .unwrap_or(DEFAULT_HEIGHT);
    let width: usize = args
        .get(3)
        .and_then(|w| w.parse().ok())
        .unwrap_or(DEFAULT_WIDTH);

    let buffer_size = height * width * RGB_SIZE;

    tokio::spawn(async move {
        let forward_socket = UdpSocket::bind("0.0.0.0:0").await.expect("Failed to bind forward socket");
        while let Some(stuff) = rx.recv().await {
            if let Some(src) = "127.0.0.1:9000".parse::<SocketAddr>().ok() {
                if stuff.src == src {
                    if let Err(e) = forward_socket.send_to(&stuff.msg, *FORWARD_ADDR).await {
                        eprintln!("Error forwarding message to {}: {}", *FORWARD_ADDR, e);
                    } else {
                        println!("Forwarded message from {} to {}", stuff.src, *FORWARD_ADDR);
                    }
                } else {
                    println!("Message from {} ignored (not matching forward source)", stuff.src);
                }
            }
        }
    });

    let mut tasks = Vec::new();
    for offset in 0..10 {
        let port = start_port + offset;
        let addr = format!("0.0.0.0:{}", port);
        let socket = UdpSocket::bind(&addr).await?;
        println!("Listening on {}", addr);
        let tx_clone = tx.clone();
        tasks.push(tokio::spawn(udp_listener(socket, buffer_size, tx_clone)));
    }

    for task in tasks {
        task.await?;
    }

    Ok(())
}

async fn udp_listener(socket: UdpSocket, buffer_size: usize, tx: Sender<Stuff>) {
    let mut buf = vec![0u8; buffer_size];

    loop {
        match socket.recv_from(&mut buf).await {
            Ok((size, src)) => {
                println!("size: {}", size);
                if size < buffer_size {
                    println!(
                        "Error: Received {} bytes from {}. Expected {} bytes: too small.",
                        size, src, buffer_size
                    );
                    continue;
                } else if size > buffer_size {
                    println!(
                        "Error: Received {} bytes from {}. Expected {} bytes: too large.",
                        size, src, buffer_size
                    );
                    continue;
                }

                println!("Received {} bytes from {}", size, src);
                handle_message(&buf[..size], &socket, src, tx.clone()).await;
            }
            Err(e) => {
                eprintln!("Error receiving UDP packet: {}", e);
            }
        }
    }
}

struct Stuff {
    src: SocketAddr,
    msg: Vec<u8>,
}

async fn handle_message(msg: &[u8], socket: &UdpSocket, src: SocketAddr, tx: Sender<Stuff>) {
    println!("Message from {}:", src);
    let stuff = Stuff {
        src: src.clone(),
        msg: msg.to_vec(),
    };

    if tx.send(stuff).await.is_err() {
        eprintln!("Error sending message to channel");
    }

    if let Err(e) = socket.send_to(msg, &src).await {
        eprintln!("Error sending response to {}: {}", src, e);
    }
}