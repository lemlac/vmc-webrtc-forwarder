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

