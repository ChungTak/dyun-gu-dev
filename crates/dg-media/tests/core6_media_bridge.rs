use dg_core::{Buffer, BufferDesc, DataFormat, DataType, DeviceKind, Shape, Tensor, TensorDesc};
use dg_graph::{Packet, PacketMeta};
use dg_media::graph_packet_to_media_frame;

fn tensor_packet() -> Packet {
    let tensor = Tensor::from_buffer(
        TensorDesc::new(
            Shape::new(vec![2, 2]),
            DataType::U8,
            DataFormat::Auto,
            DeviceKind::Cpu,
        ),
        Buffer::from_host_bytes(DeviceKind::Cpu, BufferDesc::new(4, 1), vec![1, 2, 3, 4]).unwrap(),
    )
    .unwrap();
    Packet::tensor(tensor).with_meta(PacketMeta {
        sequence: 0,
        stream_id: Some("0".to_string()),
        tags: [("media".to_string(), "video".to_string())].into(),
        media_info: None,
    })
}

#[test]
fn graph_packet_to_media_frame_rejects_non_tensor_payload() {
    let detections = vec![dg_core::Detection::new(
        dg_core::BBox::new(0.0, 0.0, 1.0, 1.0),
        0.0,
        0,
    )];
    let packet = Packet::detections(detections).with_meta(PacketMeta {
        sequence: 0,
        stream_id: Some("0".to_string()),
        tags: Default::default(),
        media_info: None,
    });
    assert!(graph_packet_to_media_frame(packet).is_err());
}

#[test]
fn graph_packet_to_media_frame_preserves_eos() {
    let frame = graph_packet_to_media_frame(Packet::eos()).expect("eos");
    assert!(frame.is_end_of_stream());
}

#[test]
fn graph_packet_to_media_frame_preserves_shared_tensor_and_metadata() {
    let packet = tensor_packet();
    let cloned = packet.clone();
    let frame = graph_packet_to_media_frame(packet).expect("bridge");
    assert!(!frame.is_end_of_stream());
    assert_eq!(frame.buffer.read_bytes().unwrap(), vec![1, 2, 3, 4]);

    let frame2 = graph_packet_to_media_frame(cloned).expect("clone bridge");
    assert_eq!(frame2.buffer.read_bytes().unwrap(), vec![1, 2, 3, 4]);
}
