use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use std::env;
use std::net::SocketAddr;
use std::os::unix::raw::time_t;
use chrono::Utc;
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicBool;
use std::thread::yield_now;
use std::time::{Duration, SystemTime};
use tokio::sync::mpsc::Sender;
use dashmap::DashMap;
use dashmap::mapref::one::{Ref, RefMut};

const DEFAULT_WIDTH: usize = 48;
const DEFAULT_HEIGHT: usize = 24;
const RGB_SIZE: usize = 3;
const RGB_SIZE_WITH_ALPHA: usize = 4;

lazy_static::lazy_static! {
    static ref FORWARD_ADDR: SocketAddr = "127.0.0.1:54321".parse().expect("Invalid forward address");
}

type RGBA = [u8; RGB_SIZE_WITH_ALPHA];
type RGB = [u8; RGB_SIZE_WITH_ALPHA];

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

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

    let buffer_size = height * width * RGB_SIZE_WITH_ALPHA;

    let mut tasks = Vec::new();
    let framebuffer_dash = Arc::new(DashMap::<u16, FrameBufferState>::new());

    for offset in 0..10 {

        let port = start_port + offset;
        let addr = format!("0.0.0.0:{}", port);
        let socket = UdpSocket::bind(&addr).await?;
        println!("Listening on {}", addr);

        let arc = framebuffer_dash.clone();
        let arc1 = arc.clone();
        arc1.insert(port, FrameBufferState {
            src: addr.parse().unwrap(),
            msg: vec![0u8; buffer_size],
            ready : false,
            last_active : SystemTime::UNIX_EPOCH
        });

        tasks.push(tokio::spawn(udp_listener(socket, buffer_size, port , arc1)));
    }

    tasks.push(
        tokio::spawn(composer(framebuffer_dash.clone(), width, height, RGB_SIZE_WITH_ALPHA))
    );

    for task in tasks {
        task.await?;
    }

    Ok(())
}

async fn composer(framebuffer_map: Arc<DashMap<u16, FrameBufferState>>, width : usize, height : usize, pixel_size : usize) {
    // Determine the resolution of the final composed framebuffer

    let buffer_size = width * height * pixel_size;

    let send_socket = UdpSocket::bind("0.0.0.0:0").await.unwrap();

    loop {
        let mut composed_framebuffer = vec![0u8; buffer_size];
        let now = SystemTime::now();

        let mut sorted_ports: Vec<u16> = framebuffer_map.iter().map(|entry| *entry.key()).collect();
        sorted_ports.sort_unstable();

        for port in sorted_ports {
            if let Some(mut entry) = framebuffer_map.get_mut(&port) {

                if now.duration_since(entry.last_active).unwrap_or(Duration::new(0, 0)) > Duration::new(5, 0) {
                    entry.ready = false;
                    entry.msg.fill(0);
                }
                let framebuffer = entry.value();

                if framebuffer.ready {
                    println!("port {}", port);
                    blend_framebuffer(&mut composed_framebuffer, &framebuffer.msg);
                }
            }
        }

        // Extract only RGB bytes from the composed framebuffer
        let rgb_framebuffer: Vec<u8> = composed_framebuffer
            .chunks(RGB_SIZE_WITH_ALPHA)
            .flat_map(|pixel| &pixel[..RGB_SIZE]) // Take the first 3 bytes (RGB)
            .copied()
            .collect();

        send_socket.send_to(&rgb_framebuffer, *FORWARD_ADDR).await.unwrap();

        // Simulate a frame delay (e.g., 16ms for ~60FPS)
        tokio::time::sleep(tokio::time::Duration::from_millis(32)).await; /*16*10*/
    }
}

fn blend_framebuffer(composed: &mut [u8], source: &[u8]) {
    assert_eq!(composed.len(), source.len());

    composed
        .chunks_mut(RGB_SIZE_WITH_ALPHA)
        .zip(source.chunks(RGB_SIZE_WITH_ALPHA))
        .for_each(|(composed_pixel, source_pixel)| {
            let src_alpha = source_pixel[3] as f32 / 255.0; // Source alpha normalized
            let dst_alpha = 1.0 - src_alpha; // Destination contribution

            for i in 0..3 {
                // Blend RGB channels
                composed_pixel[i] = ((source_pixel[i] as f32 * src_alpha)
                    + (composed_pixel[i] as f32 * dst_alpha))
                    .round() as u8;
            }

            // Update alpha to maximum of both (optional: depends on blending logic)
            composed_pixel[3] = 255;
        });
}

async fn udp_listener(socket: UdpSocket, buffer_size: usize, port: u16, arc1: Arc<DashMap<u16, FrameBufferState>>) {

    let mut receive_buffer = vec![0u8; buffer_size];

    loop {
        match socket.recv_from(&mut receive_buffer).await {
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
                println!("Received {} bytes from {} on port {}", size, src, port);
                handle_message(&receive_buffer[..size], &socket, src, arc1.get_mut(&port).unwrap()).await;
            }
            Err(e) => {
                eprintln!("Error receiving UDP packet: {}", e);
            }
        }
    }
}

struct FrameBufferState {
    src: SocketAddr,
    msg: Vec<u8>,
    ready : bool,
    last_active : SystemTime
}

async fn handle_message(msg: &[u8], socket: &UdpSocket, src: SocketAddr, mut ref_mut: RefMut<'_, u16, FrameBufferState>) {

    write_into_frame_buffer(ref_mut.value_mut(), msg);
    ref_mut.value_mut().ready = true;
    ref_mut.value_mut().last_active = SystemTime::now();

    if let Err(e) = socket.send_to(msg, &src).await {
        eprintln!("Error sending response to {}: {}", src, e);
    }
}

fn write_into_frame_buffer(framebuffer: &mut FrameBufferState, changes: &[u8]) {
    //print_framebuffer_as_ascii(&framebuffer.msg, 48, 24);
    if framebuffer.msg.len() != changes.len() {
        panic!("Framebuffer and changes must have the same size!");
    }
    if framebuffer.msg.len() % 4 != 0 {
        panic!("Framebuffer size must be a multiple of 4 (RGBA format)!");
    }
    for (frame_pixel, &change_pixel) in framebuffer.msg.iter_mut().zip(changes.iter()) {
        *frame_pixel = change_pixel;
    }
}

fn print_framebuffer_as_ascii(framebuffer: &[u8], width: usize, height: usize) {
    use colored::*;

    for row in framebuffer.chunks(width * RGB_SIZE_WITH_ALPHA) {
        for pixel in row.chunks(RGB_SIZE_WITH_ALPHA) {
            let r = pixel[0];
            let g = pixel[1];
            let b = pixel[2];

            let brightness = (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32) / 255.0;

            let ascii_char = match brightness {
                b if b < 0.2 => '#',
                b if b < 0.5 => '*',
                b if b < 0.8 => '.',
                _ => ' ',
            };

            print!("{}", ascii_char.to_string().truecolor(r, g, b));
        }
        println!();
    }
}
