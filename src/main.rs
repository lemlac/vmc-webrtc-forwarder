use anyhow::Result;
use std::net::UdpSocket;
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use webrtc::data_channel::data_channel_init::RTCDataChannelInit;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::api::APIBuilder;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Setup local native UDP Socket to catch VMC data
    // Setting non-blocking allows Tokio to poll it efficiently
    let udp_socket = UdpSocket::bind("127.0.0.1:39539")?;
    udp_socket.set_non_blocking(true)?;
    let async_udp = tokio::net::UdpSocket::from_std(udp_socket)?;
    println!("📡 Listening for local VMC UDP data on port 39539...");

    // 2. Initialize WebRTC API Layer
    let api = APIBuilder::new().build();
    let config = RTCConfiguration::default(); // Add STUN/TURN servers here in production
    let peer_connection = Arc::new(api.new_peer_connection(config).await?));

    // 3. Configure the DataChannel for Ultra-Low Latency VMC Streaming
    let mut data_channel_init = RTCDataChannelInit::default();
    data_channel_init.ordered = Some(false);           // Do not wait for missed packets
    data_channel_init.max_retransmits = Some(0);       // Real-time prioritization

    let data_channel = peer_connection
        .create_data_channel("vmc-stream", Some(data_channel_init))
        .await?;

    println!("🔒 WebRTC DataChannel 'vmc-stream' initialized.");

    // Track connection state
    peer_connection.on_peer_connection_state_change(Box::new(move |s: RTCPeerConnectionState| {
        println!("Network State Change: {}", s);
        Box::pin(async {})
    }));

    // ------------------------------------------------------------------------
    // TODO: Handshake / Signaling Phase
    // In production, your app needs to send its local SDP Offer to your cloud server
    // via a quick HTTP POST or WebSocket, and receive the server's SDP Answer back.
    // ------------------------------------------------------------------------

    // 4. Run the Forwarding Loop once the channel opens
    let dc_clone = Arc::clone(&data_channel);
    tokio::spawn(async move {
        let mut buf = [0u8; 2048]; // VMC tracking packets are usually well under 2KB

        loop {
            // Only stream if the tunnel is ready to receive data
            if dc_clone.ready_state() == webrtc::data_channel::RTCDataChannelState::Open {
                match async_udp.recv_from(&mut buf).await {
                    Ok((len, _src)) => {
                        let raw_bytes = &buf[..len];
                        
                        // Push binary packet straight through WebRTC DataChannel over WAN
                        if let Err(e) = dc_clone.send(&bytes::Bytes::copy_from_slice(raw_bytes)).await {
                            eprintln!("Failed to forward VMC packet: {:?}", e);
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        sleep(Duration::from_millis(1)).await;
                    }
                    Err(e) => eprintln!("UDP Read error: {:?}", e),
                }
            } else {
                // Chill for a brief second if network is negotiating/connecting
                sleep(Duration::from_millis(100)).await;
            }
        }
    });

    // Keep the main thread alive running the async event loop
    tokio::signal::ctrl_c().await?;
    println!("Shutting down VMC forwarder.");
    Ok(())
}
