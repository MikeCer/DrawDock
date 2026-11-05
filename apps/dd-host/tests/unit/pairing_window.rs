use super::*;

#[test]
fn qr_bitmap_preserves_modules_and_four_module_quiet_zone() {
    let payload = format!(
        "drawdock://pair?v=1&host=127.0.0.1&port=47831&fingerprint={}&token={}",
        "0123456789abcdef".repeat(4),
        "abcdef0123456789".repeat(2),
    );
    let code = QrCode::new(payload.as_bytes()).unwrap();
    let (size, pixels) = qr_pixels(&payload).unwrap();
    let size = size as usize;
    for y in 0..size {
        for x in 0..size {
            let expected = if x < 20 || y < 20 || x >= size - 20 || y >= size - 20 {
                0x00ff_ffff
            } else if code[(x / 5 - 4, y / 5 - 4)] == Color::Dark {
                0
            } else {
                0x00ff_ffff
            };
            assert_eq!(pixels[y * size + x], expected);
        }
    }
}
