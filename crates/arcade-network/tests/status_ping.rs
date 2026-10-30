//! End-to-end status ping against a real TCP listener.

use arcade_network::{Server, ServerConfig};
use arcade_protocol::{Codec, PacketReader, PacketWriter, State};
use arcade_protocol_data::GameData;
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[test]
fn status_ping_reports_26_3() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/versions/26.3");
    let data = GameData::load(&dir).unwrap();
    let config = ServerConfig {
        bind: "127.0.0.1:25599".into(),
        online_mode: false,
        view_distance: 2,
        ..ServerConfig::default()
    };
    let server = Server::new(config, data).unwrap();
    let ids = server.data.packet_ids.clone();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async move {
        let task = tokio::spawn(Server::run(server));
        tokio::time::sleep(Duration::from_millis(800)).await;

        let mut stream = TcpStream::connect("127.0.0.1:25599").await.unwrap();
        let mut codec = Codec::new();
        let mut payload = PacketWriter::new();
        payload.write_varint(0);
        payload.write_varint(777);
        payload.write_string("127.0.0.1");
        payload.write_u16(25599);
        payload.write_varint(1);
        let frame = codec.encode(&payload.into_inner());
        stream.write_all(&frame).await.unwrap();

        let mut request = PacketWriter::new();
        request.write_varint(ids.serverbound_id(State::Status, "status_request").unwrap());
        let frame = codec.encode(&request.into_inner());
        stream.write_all(&frame).await.unwrap();

        let mut buf = vec![0u8; 8192];
        let packet = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if let Some(p) = codec.next_packet().unwrap() {
                    return p;
                }
                let n = stream.read(&mut buf).await.unwrap();
                assert!(n > 0, "connection closed");
                codec.push_received(&buf[..n]);
            }
        })
        .await
        .expect("status response timed out");

        let mut r = PacketReader::new(&packet);
        let id = r.read_varint().unwrap();
        assert_eq!(id, 0, "status response id");
        let json: serde_json::Value = serde_json::from_str(&r.read_string().unwrap()).unwrap();
        assert_eq!(json["version"]["protocol"], 777);
        task.abort();
    });
}
