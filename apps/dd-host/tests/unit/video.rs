use super::*;
#[test]
fn encoded_pipe_bursts_preserve_frame_order_with_one_pending_slot() {
    let (sender, receiver) = mpsc::sync_channel(1);
    sender.try_send(Ok(vec![1])).unwrap();
    let stopped = Arc::new(AtomicBool::new(false));
    let stop = Arc::clone(&stopped);
    let worker = thread::spawn(move || {
        send_frame(&sender, vec![2], &stop).unwrap();
        send_frame(&sender, vec![3], &stop).unwrap();
    });
    thread::sleep(Duration::from_millis(150));
    for expected in [1, 2, 3] {
        assert_eq!(
            receiver
                .recv_timeout(Duration::from_secs(1))
                .unwrap()
                .unwrap(),
            vec![expected]
        );
    }
    worker.join().unwrap();
}

#[test]
fn stalled_video_handoff_times_out_and_stop_never_waits_for_a_consumer() {
    let (sender, _receiver) = mpsc::sync_channel(1);
    sender.try_send(Ok(vec![1])).unwrap();
    assert!(
        send_frame(&sender, vec![2], &AtomicBool::new(false))
            .unwrap_err()
            .contains("500 ms")
    );
    assert!(
        send_frame(&sender, vec![2], &AtomicBool::new(true))
            .unwrap_err()
            .contains("stopped")
    );
}

#[test]
#[ignore = "Captures two Windows displays with x264 and NVIDIA NVENC; run explicitly on this desktop"]
fn native_tls_video_authenticates_and_switches_monitors() {
    use rustls::{
        ClientConfig, ClientConnection, RootCertStore,
        pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer, ServerName},
    };
    crate::windows_overlay::initialize_dpi().unwrap();
    let outputs = crate::windows_overlay::Output::enumerate().unwrap();
    assert!(
        outputs.len() >= 2,
        "Two real outputs are needed for this test"
    );
    let certificate = rcgen::generate_simple_self_signed(vec!["drawdock".into()]).unwrap();
    let server_config = Arc::new(
        ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![certificate.cert.der().clone()],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
                    certificate.key_pair.serialize_der(),
                )),
            )
            .unwrap(),
    );
    let mut roots = RootCertStore::empty();
    roots.add(certificate.cert.der().clone()).unwrap();
    let client_config = Arc::new(
        ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth(),
    );
    for encoder in [VideoEncoder::Software, VideoEncoder::Nvenc] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let source = Arc::new(std::sync::Mutex::new(SelectedSource::new(
            outputs[0].clone(),
        )));
        let service = VideoService::start(
            listener,
            Arc::clone(&server_config),
            "test-only-video-token".into(),
            Arc::clone(&source),
            encoder,
        )
        .unwrap();
        let connect = |token: &str| {
            let tcp = TcpStream::connect(address).unwrap();
            tcp.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
            let tls = ClientConnection::new(
                Arc::clone(&client_config),
                ServerName::try_from("drawdock").unwrap(),
            )
            .unwrap();
            let mut stream = StreamOwned::new(tls, tcp);
            writeln!(
                stream,
                "{}",
                json!({"type": "video", "version": 2, "token": token})
            )
            .unwrap();
            stream.flush().unwrap();
            stream
        };
        let mut denied = connect("wrong-token");
        match denied.read(&mut [0]) {
            Ok(0) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::ConnectionReset
                ) => {}
            other => panic!("Unauthorized video was not rejected: {other:?}"),
        }
        drop(denied);
        let mut stream = connect("test-only-video-token");
        let packet = |stream: &mut StreamOwned<ClientConnection, TcpStream>| {
            let mut header = [0; 13];
            stream.read_exact(&mut header).unwrap();
            let epoch = u64::from_be_bytes(header[1..9].try_into().unwrap());
            let size = u32::from_be_bytes(header[9..13].try_into().unwrap()) as usize;
            assert!((1..=MAX_ACCESS_UNIT).contains(&size));
            let mut payload = vec![0; size];
            stream.read_exact(&mut payload).unwrap();
            assert_ne!(
                header[0],
                3,
                "Video error: {}",
                String::from_utf8_lossy(&payload)
            );
            (header[0], epoch, payload)
        };
        let config = packet(&mut stream);
        assert_eq!((config.0, config.1), (1, 1));
        let metadata: serde_json::Value = serde_json::from_slice(&config.2).unwrap();
        assert_eq!(metadata["encoder"], encoder.name());
        let expected = encoded_size(outputs[0].width(), outputs[0].height());
        assert_eq!(metadata["width"], expected.0);
        assert_eq!(metadata["height"], expected.1);
        for index in 0..30 {
            assert_eq!(packet(&mut stream).0, 4);
            let frame = packet(&mut stream);
            assert_eq!(frame.0, 2);
            if index == 0 {
                for nal_type in [7, 8, 5] {
                    assert!(
                        frame
                            .2
                            .windows(4)
                            .any(|bytes| bytes[..3] == [0, 0, 1] && bytes[3] & 31 == nal_type),
                        "First H.264 frame needs SPS, PPS and IDR for {encoder:?}"
                    );
                }
            }
        }
        {
            let mut selected = source.lock().unwrap();
            selected.epoch = 2;
        }
        for _ in 0..15 {
            assert_eq!(
                packet(&mut stream).0,
                4,
                "Zoom epoch must not restart capture"
            );
            assert_eq!(packet(&mut stream).0, 2);
        }
        {
            let mut selected = source.lock().unwrap();
            selected.output = outputs[1].clone();
            selected.epoch = 3;
        }
        let mut switched = false;
        for _ in 0..90 {
            let frame = packet(&mut stream);
            if frame.0 == 1 && frame.1 == 3 {
                switched = true;
                break;
            }
        }
        assert!(switched, "No new source configuration arrived");
        for _ in 0..30 {
            assert_eq!(packet(&mut stream).0, 4);
            let frame = packet(&mut stream);
            assert_eq!(frame.0, 2);
        }
        drop(service);
        println!(
            "Authenticated 1440p TLS H.264 ({encoder:?}) received from two real monitors; wrong token rejected."
        );
    }
}

#[test]
fn bounded_packets_include_kind_epoch_and_network_order_length() {
    let mut bytes = Vec::new();
    write_packet(&mut bytes, 2, 7, &[1, 2, 3]).unwrap();
    assert_eq!(
        bytes,
        [
            vec![2],
            7u64.to_be_bytes().to_vec(),
            3u32.to_be_bytes().to_vec(),
            vec![1, 2, 3]
        ]
        .concat()
    );
    assert!(write_packet(&mut bytes, 2, 7, &[]).is_err());
    assert!(write_packet(&mut bytes, 2, 7, &vec![0; MAX_ACCESS_UNIT + 1]).is_err());
}
#[test]
fn timestamped_packets_reject_reordering_invalid_sizes_and_encoder_errors() {
    assert_eq!(
        parse_packet_metadata("0, 1770000000000000, 1770000000000000, 33333, 2413, 0x0").unwrap(),
        (1770000000000000, 2413)
    );
    assert_eq!(
        parse_packet_metadata(
            "0, 1770000000000000, 1770000000000000, 33333, 2413, 0x0, F=0x0, S=1, 8"
        )
        .unwrap(),
        (1770000000000000, 2413)
    );
    for line in [
        "0, 1, 2, 33333, 10, 0x0",
        "0, 0, 0, 33333, 10, 0x0",
        "0, -1, -1, 33333, 10, 0x0",
        "0, 1, 1, 33333, 0, 0x0",
        "0, 1, 1, 33333, 8388609, 0x0",
        "encoder failed",
    ] {
        assert!(parse_packet_metadata(line).is_err(), "{line}");
    }
}
#[test]
fn video_size_preserves_landscape_portrait_and_even_dimensions() {
    assert_eq!(encoded_size(2560, 1440), (2560, 1440));
    assert_eq!(encoded_size(3840, 2160), (2560, 1440));
    assert_eq!(encoded_size(1080, 1920), (810, 1440));
    assert_eq!(encoded_size(640, 480), (640, 480));
}
