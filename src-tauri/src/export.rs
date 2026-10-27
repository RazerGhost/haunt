//! Exports: a picture of the canvas or the layer map, saved where the save dialog said. The window draws it (SVG, or
//! PNG passed as base64) and Rust only writes it, refusing anything but a .png or .svg in a folder that exists.

use std::fs;
use std::path::Path;

/// Writes `data` to `path`: base64-decoded when `base64` (a PNG), else as text (an SVG).
pub fn save(path: &str, data: &str, base64: bool) -> Result<(), String> {
    let target = Path::new(path);
    let extension = target.extension().and_then(|ext| ext.to_str()).map(str::to_ascii_lowercase);
    if !matches!(extension.as_deref(), Some("png" | "svg")) {
        return Err(said!("export.notImage", "Only a .png or .svg file can be saved."));
    }
    if !target.is_absolute() {
        return Err(said!("export.notFull", "The file's place is not a full path."));
    }
    if !target.parent().is_some_and(Path::is_dir) {
        return Err(said!("export.noFolder", "The folder to save in does not exist."));
    }
    let bytes = if base64 {
        decode(data).ok_or_else(|| said!("export.unreadable", "The image could not be read."))?
    } else {
        data.as_bytes().to_vec()
    };
    fs::write(target, bytes)
        .map_err(|error| said!("export.notSaved", "The file could not be saved: {error}", error = error))
}

/// Standard base64 (with or without its `=` padding) to bytes; None for anything else. Small enough to keep here
/// rather than add a crate for one image.
fn decode(text: &str) -> Option<Vec<u8>> {
    let value = |byte: u8| match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    let text = text.trim().trim_end_matches('=');
    if text.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    for chunk in text.as_bytes().chunks(4) {
        let mut bits = 0u32;
        for (at, &byte) in chunk.iter().enumerate() {
            bits |= u32::from(value(byte)?) << (18 - 6 * at);
        }
        out.extend_from_slice(&bits.to_be_bytes()[1..chunk.len()]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_base64() {
        assert_eq!(decode("aGF1bnQ=").unwrap(), b"haunt");
        assert_eq!(decode("aGF1bnQ").unwrap(), b"haunt");
        assert_eq!(decode("aGF1bnRz").unwrap(), b"haunts");
        assert_eq!(decode("aGE=").unwrap(), b"ha");
        assert_eq!(decode("").unwrap(), b"");
        assert!(decode("aGF1b!Q=").is_none());
        assert!(decode("aGF1b").is_none());
    }

    #[test]
    fn writes_an_svg_and_a_decoded_png() {
        let dir = tempfile::tempdir().unwrap();
        let svg = dir.path().join("map.SVG");
        save(svg.to_str().unwrap(), "<svg/>", false).unwrap();
        assert_eq!(fs::read_to_string(&svg).unwrap(), "<svg/>");
        let png = dir.path().join("map.png");
        save(png.to_str().unwrap(), "iVBORw0KGgo=", true).unwrap();
        assert_eq!(fs::read(&png).unwrap(), b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn refuses_other_files_places_and_data() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("map.exe");
        assert!(save(exe.to_str().unwrap(), "x", false).is_err());
        assert!(!exe.exists());
        assert!(save("map.svg", "<svg/>", false).is_err());
        let missing = dir.path().join("nowhere").join("map.svg");
        assert!(save(missing.to_str().unwrap(), "<svg/>", false).is_err());
        let png = dir.path().join("map.png");
        assert!(save(png.to_str().unwrap(), "not base64!", true).is_err());
        assert!(!png.exists());
    }
}
