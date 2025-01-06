use bytes::Bytes;
use axum::{
    extract::ws::{self, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use axum::extract::ws::{ Utf8Bytes};
use futures_util::{SinkExt, StreamExt};
use reqwest::Client;
use tokio::time::{sleep, Duration, Instant};

#[derive(Serialize, Deserialize, Debug)]
struct Request {
    action: String,
    data: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct Response {
    message: String,
}

#[tokio::main]
async fn main() {
    // initialize tracing
    tracing_subscriber::fmt::init();

    // build our application with a route
    let app = Router::new()
        // `GET /` goes to `root`
        .route("/ws", get(ws_handler));


    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// Handler untuk WebSocket
async fn ws_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_connection)
}

// Fungsi untuk menangani koneksi WebSocket
async fn handle_connection(ws: WebSocket) {
    let (mut sender, mut receiver) = ws.split();
    let client = Client::new(); // Membuat klien HTTP

    while let Some(message) = receiver.next().await {
        match message {
            Ok(ws::Message::Text(text)) => {
                match serde_json::from_str::<Request>(&text) {
                    Ok(request) => {
                        println!("Received request: {:?}", request);

                        // Melakukan request ke layanan lain selama 3 menit
                        match await_service_response(&client, &mut sender).await {
                            Ok(response_message) => {
                                if let Err(e) = sender.send(ws::Message::Text(Utf8Bytes::from(response_message))).await {
                                    eprintln!("Failed to send response: {}", e);
                                    break;
                                }
                            }
                            Err(e) => {
                                eprintln!("Error during service response loop: {}", e);
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Invalid request format: {}", e);
                    }
                }
            }
            Ok(ws::Message::Close(_)) => {
                println!("Client disconnected");
                break;
            }
            Err(e) => {
                eprintln!("Error while receiving message: {}", e);
                break;
            }
            _ => {}
        }
    }
    println!("Connection closed.");
}

// Fungsi untuk melakukan request ke layanan lain hingga mendapatkan flag success
async fn await_service_response(client: &Client, sender: &mut futures_util::stream::SplitSink<WebSocket, ws::Message>) -> Result<String, String> {
    let start_time = Instant::now();
    let duration = Duration::from_secs(180); // 3 menit

    while Instant::now().duration_since(start_time) < duration {
        // Melakukan permintaan ke layanan lain (ganti URL dengan yang sesuai)
        let response = client.get("https://api.example.com/data") // Ganti dengan URL yang sesuai
            .send()
            .await;

        match response {
            Ok(resp) => {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    // Cek apakah respons memiliki flag success
                    if json.get("flag").map_or(false, |v| v == "success") {
                        return Ok(format!("Received success response: {:?}", json));
                    } else {
                        println!("Received non-success response: {:?}", json);
                    }
                } else {
                    eprintln!("Failed to parse response");
                }
            }
            Err(e) => {
                eprintln!("Error while making request: {}", e);
            }
        }

        if let Err(e) = sender.send(ws::Message::Ping(Bytes::from(Vec::new()))).await {
            eprintln!("WebSocket connection has been closed. {}", e);
            break; // Keluar dari loop jika pengiriman pesan gagal (WebSocket terputus)
        }


        // Tunggu sebelum mencoba lagi (misalnya, 5 detik)
        sleep(Duration::from_secs(5)).await;
    }

    Err("Request timed out without success".to_string())
}
