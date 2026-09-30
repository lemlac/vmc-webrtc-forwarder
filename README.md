# VMC WebRTC Forwarder
Tool that streams VMC to a server through WebRTC.

## How to Receive and Handle an Incoming Stream

To handle the incoming real-time motion data on your cloud rendering server, the server must run a native or high-performance WebRTC service. This service acts as a WebRTC-to-UDP Gateway. It terminates the WAN WebRTC connection from the creator, extracts the raw binary data, and instantly pipes it into your 3D engine via a local loopback port (127.0.0.1:39539).

Using Node.js with the highly efficient node-datachannel library (which wraps native C++ libdatachannel) provides a perfect balance of fast networking and easy integration.

```
[ Creator App ] ──( WebRTC via WAN )──> [ Your Cloud Server / Node.js App ]
                                                   │
                                        (Extracts Binary Payload)
                                                   │
                                                   ▼
[ 3D Engine (Unity/Unreal) ] <──( Local UDP Loopback )─┘
```

Run this in your server project folder:

```sh
npm install node-datachannel dgram
```

This Node.js script initializes the WebRTC listener, exposes a mechanism to handle the handshake (SDP), and pipes the incoming stream out to a local UDP port.

```js
const nodeDatachannel = require('node-datachannel');const dgram = require('dgram');
// 1. Setup local UDP Client to pipe data straight into the 3D Engineconst udpClient = dgram.createSocket('udp4');const ENGINE_UDP_PORT = 39539; const ENGINE_IP = '127.0.0.1'; // Assuming the 3D Engine runs on the same server instance

console.log(`🚀 Gateway ready to forward VMC packets to ${ENGINE_IP}:${ENGINE_UDP_PORT}`);
/**
 * Call this function whenever your Signaling Server (WebSocket or HTTP API) 
 * receives a connection request (SDP Offer) from a creator's desktop app.
 */function handleCreatorConnection(creatorSdpOffer) {
    // 2. Initialize the WebRTC Peer Connection on the server
    const pc = new nodeDatachannel.PeerConnection("VmcReceiver", {
        iceServers: ["stun:://google.com"] // Use your own TURN servers in production
    });

    // 3. Listen for incoming data channels created by the client app
    pc.onDataChannel((dc) => {
        console.log(`🔒 DataChannel '${dc.getLabel()}' successfully opened by creator!`);

        // Set binary type to handle raw ArrayBuffers/Buffers
        dc.setBinaryType('buffer');

        // 4. Ultra-fast routing loop
        dc.onMessage((msg) => {
            // 'msg' is a Node.js Buffer containing the raw binary VMC/OSC bundle
            udpClient.send(msg, 0, msg.length, ENGINE_UDP_PORT, ENGINE_IP, (err) => {
                if (err) console.error("Error forwarding packet to 3D engine:", err);
            });
        });

        dc.onClosed(() => {
            console.log("🛑 Creator disconnected DataChannel.");
        });
    });

    // 5. Execute the WebRTC Handshake (Signaling)
    pc.setRemoteDescription(creatorSdpOffer, "offer");
    
    // Generate the answer to send back to the creator's app
    const sdpAnswer = pc.getLocalDescription();
    
    // Return this SDP Answer back through your signaling server to the creator
    return sdpAnswer; 
}
```

Because the server script forwards packets to 127.0.0.1:39539, your 3D engine running on the cloud server does not need to know anything about WebRTC or the internet.

* In Unity (EVMC4U or VMC Receiver plugins): Set the receiver settings to bind to port 39539 on IP 127.0.0.1.
* In Unreal Engine (Live Link VMC plugins): Set your Live Link VMC source port to 39539.

The engine will see the packets exactly as if the virtual actor were running the motion capture software locally on that cloud machine.

__⚡ Critical Considerations for Production Cloud Servers:__

* Port Availability & Firewalls: Your cloud provider (AWS, GCP, DigitalOcean) must have UDP ports wide open for WebRTC's interactive connectivity establishment (ICE). Usually, this means opening a range (e.g., UDP ports 30000-40000) for media traffic.
* Co-locating Server & Engine: Running the Node.js script and the 3D Engine (like Unity/Unreal) on the same virtual machine instance ensures that the final UDP leg has exactly 0ms of latency and zero packet loss.
* Process Management: Use a process manager like PM2 (pm2 start server.js) to ensure this background router restarts automatically if an unexpected networking loop error happens.

## How to implement the SDP connection handshake (Signaling) between VMC Forwarder and server


Here is the step-by-step implementation for the handshake using an HTTP API gateway.

```
[ Local Rust App ] ─────── 1. POST /session (SDP Offer) ──────> [ Cloud Node.js Server ]
                                                                       │
                                                               2. Generates Answer
                                                                       │
[ Local Rust App ] <────── 3. 200 OK (SDP Answer) ─────────────────────┘
       │
(WebRTC P2P Tunnel Established via WAN)
       │
       ▼
[ Cloud Node.js Server ]
```

We will update our server code using a lightweight framework like Express to host a REST API endpoint. The server will receive the creator's SDP Offer, register it, and respond directly with an SDP Answer.

First, run `npm install express` on your server, then deploy this code:

```js
const express = require('express');const nodeDatachannel = require('node-datachannel');const dgram = require('dgram');
const app = express();
app.use(express.json()); // Essential to parse JSON payloads
const udpClient = dgram.createSocket('udp4');const ENGINE_UDP_PORT = 39539;const ENGINE_IP = '127.0.0.1';
// HTTP POST endpoint that handles the WebRTC handshake
app.post('/api/connect-session', (req, res) => {
    const { sdp, type } = req.body;

    if (!sdp || type !== 'offer') {
        return res.status(400).json({ error: "Invalid WebRTC Offer payload." });
    }

    console.log("📥 Received SDP Offer from local creator app...");

    // 1. Initialize PeerConnection
    const pc = new nodeDatachannel.PeerConnection("VmcReceiver", {
        iceServers: ["stun:://google.com"] 
    });

    // 2. Setup WebRTC streaming pipe to local UDP loopback
    pc.onDataChannel((dc) => {
        console.log(`🚀 Stream DataChannel successfully active: [${dc.getLabel()}]`);
        dc.setBinaryType('buffer');
        
        dc.onMessage((msg) => {
            udpClient.send(msg, 0, msg.length, ENGINE_UDP_PORT, ENGINE_IP);
        });
    });

    // 3. Process the Offer and generate our local network Answer
    pc.setRemoteDescription(sdp, type);
    
    // WebRTC creates our local profile natively synchronously
    const localSdp = pc.getLocalDescription(); 
    const localType = pc.getLocalDescriptionType();

    console.log("📤 Sending SDP Answer back to creator app.");
    
    // 4. Send network profile right back through the active HTTP response lifecycle
    res.json({
        sdp: localSdp,
        type: localType
    });
});

app.listen(8080, () => console.log("🌐 Signaling Server listening on port 8080"));
```

We need to update our Rust application to gather its own connection data (SDP Offer), compile it to JSON, send it to the cloud server via an HTTP POST request, and read the response.

Add reqwest and serde_json to your Cargo.toml to make handling network payloads clean:

```toml
[dependencies]# (Keep previous dependencies)
reqwest = { version = "0.11", features = ["json"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
```

Then update your `main.rs` file to handle the automatic handshake connection pipeline:

```rs
use anyhow::Result;use serde::{Deserialize, Serialize};use std::sync::Arc;use webrtc::peer_connection::configuration::RTCConfiguration;use webrtc::api::APIBuilder;

#[derive(Serialize, Deserialize, Debug)]struct SdpPayload {
    sdp: String,
    #[serde(rename = "type")]
    sdp_type: String, // Maps to "offer" or "answer"
}

#[tokio::main]async fn main() -> Result<()> {
    // 1. Setup your WebRTC instance locally
    let api = APIBuilder::new().build();
    let config = RTCConfiguration {
        ice_servers: vec![webrtc::ice_transport::ice_server::RTCIceServer {
            urls: vec!["stun:://google.com".to_owned()],
            ..Default::default()
        }],
        ..Default::default()
    };
    let peer_connection = Arc::new(api.new_peer_connection(config).await?);

    // Create the DataChannel configuration (unreliable + unordered for pure real-time data)
    let mut data_channel_init = webrtc::data_channel::data_channel_init::RTCDataChannelInit::default();
    data_channel_init.ordered = Some(false);
    data_channel_init.max_retransmits = Some(0);
    let _data_channel = peer_connection.create_data_channel("vmc-stream", Some(data_channel_init)).await?;

    // 2. Generate local SDP Offer to send to the server
    let offer = peer_connection.create_offer(None).await?;
    
    // We must commit our own local description internally before sending it out
    let mut gather_complete = peer_connection.gathering_complete_promise().await;
    peer_connection.set_local_description(offer).await?;
    
    // Wait for the ICE gathering phase to compile all available network profiles
    let _ = gather_complete.recv().await;

    let local_desc = peer_connection.local_description().await.unwrap();
    let offer_payload = SdpPayload {
        sdp: local_desc.sdp,
        sdp_type: "offer".to_string(),
    };

    // 3. Blast the SDP Offer payload out to your cloud server via HTTP POST
    println!("📡 Sending network offer payload to cloud rendering server...");
    let server_url = "http://YOUR_CLOUD_SERVER_IP:8080/api/connect-session";
    let http_client = reqwest::Client::new();
    
    let http_response = http_client
        .post(server_url)
        .json(&offer_payload)
        .send()
        .await?;

    if !http_response.status().is_success() {
        panic!("Cloud server failed to acknowledge signaling request!");
    }

    // 4. Grab the server's matching SDP Answer payload
    let answer_payload: SdpPayload = http_response.json().await?;
    println!("📥 Received target SDP Answer from cloud infrastructure.");

    // 5. Apply the server's answer. This tells WebRTC to build the P2P WAN connection!
    let session_desc = webrtc::peer_connection::sdp::session_description::RTCSessionDescription {
        sdp: answer_payload.sdp,
        sdp_type: webrtc::peer_connection::sdp::sdp_type::RTCSdpType::Answer,
    };
    peer_connection.set_remote_description(session_desc).await?;

    println!("⚡ Handshake complete! WebRTC optimization tunnels are initiating...");

    // (Keep the previous UDP loopback listener forwarding loop code here)

    tokio::signal::ctrl_c().await?;
    Ok(())
}
```

__💡 Infrastructure Tips for Success:__

* **ICE Trickle vs. Vanilla SDP:** WebRTC takes a few moments to find out its public router configuration (ICE candidates). By waiting for `gathering_complete_promise()` to finish before firing off the HTTP request, we package all connection paths into a single payload. This eliminates the need for complex, real-time message swapping.
* **Production Authentication:** For production security, append an API Token parameter or header into your client request (`/api/connect-session?token=YOUR_CREATOR_KEY`) to prevent random users from hitting your rendering servers.
