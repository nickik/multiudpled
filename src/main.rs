use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use std::env;
use std::net::SocketAddr;

const DEFAULT_WIDTH: usize = 48;
const DEFAULT_HEIGHT: usize = 24;
const RGB_SIZE: usize = 3;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Get the starting port from command line arguments
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

    let buffer_size = height * width * RGB_SIZE; // Calculate the buffer size dynamically


    // Create a vector of tasks to listen on 255 ports
    let mut tasks = Vec::new();
    for offset in 0..10 {
        let port = start_port + offset;
        let addr = format!("0.0.0.0:{}", port); //0.0.0.0
        let socket = UdpSocket::bind(&addr).await?;
        println!("Listening on {}", addr);

        tasks.push(tokio::spawn(udp_listener(socket, buffer_size)));
    }

    // Wait for all tasks to finish
    for task in tasks {
        task.await?;
    }

    Ok(())
}

async fn udp_listener(socket: UdpSocket, buffer_size: usize) {
    let mut buf = vec![0u8; buffer_size];

    loop {
        match socket.recv_from(&mut buf).await {
            Ok((size, src)) => {
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
                handle_message(&buf[..size], &socket, src).await;
            }
            Err(e) => {
                eprintln!("Error receiving UDP packet: {}", e);
            }
        }
    }
}

async fn handle_message(msg: &[u8], socket: &UdpSocket, src: SocketAddr) {
    println!("Message from {}: {:?}", src, msg);
    // Echo the message back to the sender

    if let Err(e) = socket.send_to(msg, &src).await {
        eprintln!("Error sending response to {}: {}", src, e);
    }
}
