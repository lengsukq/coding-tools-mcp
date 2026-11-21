use std::io::Cursor;

use base64::{engine::general_purpose::STANDARD, Engine};
use image::codecs::{jpeg::JpegEncoder, png::PngEncoder};
use image::{DynamicImage, ImageFormat, ImageReader};
use serde_json::{json, Value};

use crate::tools::workspace::{tool_ok, Workspace, WorkspaceError};

const DEFAULT_MAX_BYTES: usize = 5_242_880;
const DEFAULT_MAX_DIMENSION: u32 = 2_000;
/// Hard cap on raw image bytes read into memory even when `auto_resize` is enabled.
const MAX_RAW_IMAGE_BYTES: u64 = 50 * 1024 * 1024;
const JPEG_QUALITY_STEPS: [u8; 4] = [85, 70, 55, 40];

pub fn view_image(ws: &Workspace, args: &Value) -> Result<Value, WorkspaceError> {
    let path = args
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| WorkspaceError::invalid_argument("path is required"))?;
    let max_bytes = args
        .get("max_bytes")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_MAX_BYTES as u64) as usize;
    let max_width = args
        .get("max_width")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_MAX_DIMENSION as u64) as u32;
    let max_height = args
        .get("max_height")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_MAX_DIMENSION as u64) as u32;
    let auto_resize = args
        .get("auto_resize")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let output_mode = args
        .get("output")
        .and_then(Value::as_str)
        .unwrap_or("mcp_image");

    let resolved = ws.resolve_read_path(path)?;
    let meta = std::fs::metadata(&resolved.path).map_err(|e| WorkspaceError::Tool {
        code: "IO_ERROR",
        message: format!("Failed to read image metadata: {e}"),
        category: "runtime",
        retryable: false,
    })?;
    if meta.len() > MAX_RAW_IMAGE_BYTES || (!auto_resize && meta.len() > max_bytes as u64) {
        return Err(output_too_large_error());
    }

    let mut data = std::fs::read(&resolved.path).map_err(|e| WorkspaceError::Tool {
        code: "IO_ERROR",
        message: format!("Failed to read image: {e}"),
        category: "runtime",
        retryable: false,
    })?;

    let (mut mime_type, mut width, mut height) = identify_image_header(&data)?;
    let original = json!({
        "bytes": data.len(),
        "width": width,
        "height": height,
        "mime_type": mime_type
    });

    let mut resized = false;
    let mut warnings: Vec<String> = Vec::new();

    if auto_resize && should_resize(data.len(), width, height, max_bytes, max_width, max_height) {
        match resize_image(&data, mime_type, max_width, max_height, max_bytes) {
            Ok(Some(resized_img)) => {
                data = resized_img.bytes;
                mime_type = resized_img.mime_type;
                width = resized_img.width;
                height = resized_img.height;
                resized = true;
            }
            Ok(None) => warnings
                .push("auto_resize requested but image resize failed or format unsupported".into()),
            Err(err) => warnings.push(format!("auto_resize failed: {err}")),
        }
    }

    if data.len() > max_bytes {
        return Err(output_too_large_error());
    }

    let encoded = STANDARD.encode(&data);
    let mut payload = json!({
        "path": resolved.display,
        "mime_type": mime_type,
        "bytes": data.len(),
        "width": width,
        "height": height,
        "resized": resized,
        "original": original,
        "warnings": warnings
    });
    if let Some(object) = payload.as_object_mut() {
        if output_mode == "data_url" {
            object.insert(
                "data_url".into(),
                Value::String(format!("data:{mime_type};base64,{encoded}")),
            );
        } else {
            object.insert("base64".into(), Value::String(encoded));
        }
    }
    Ok(tool_ok(payload))
}

fn identify_image_header(data: &[u8]) -> Result<(&'static str, u32, u32), WorkspaceError> {
    let reader = ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .map_err(|_| binary_file_error())?;
    let format = reader.format().ok_or_else(binary_file_error)?;
    let (width, height) = reader.into_dimensions().map_err(|_| binary_file_error())?;
    Ok((mime_for_format(format), width, height))
}

fn binary_file_error() -> WorkspaceError {
    WorkspaceError::Tool {
        code: "BINARY_FILE",
        message: "File is not a supported image.".into(),
        category: "validation",
        retryable: false,
    }
}

fn output_too_large_error() -> WorkspaceError {
    WorkspaceError::Tool {
        code: "OUTPUT_TOO_LARGE",
        message: "Image exceeds max_bytes.".into(),
        category: "validation",
        retryable: false,
    }
}

fn mime_for_format(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Png => "image/png",
        ImageFormat::Jpeg => "image/jpeg",
        ImageFormat::Gif => "image/gif",
        ImageFormat::WebP => "image/webp",
        _ => "application/octet-stream",
    }
}

fn should_resize(
    bytes: usize,
    width: u32,
    height: u32,
    max_bytes: usize,
    max_width: u32,
    max_height: u32,
) -> bool {
    bytes > max_bytes || width > max_width || height > max_height
}

struct ResizedImage {
    bytes: Vec<u8>,
    mime_type: &'static str,
    width: u32,
    height: u32,
}

fn resize_image(
    data: &[u8],
    mime_type: &str,
    max_width: u32,
    max_height: u32,
    max_bytes: usize,
) -> Result<Option<ResizedImage>, String> {
    let img = image::load_from_memory(data).map_err(|e| e.to_string())?;
    let thumb = img.thumbnail(max_width, max_height);
    let width = thumb.width();
    let height = thumb.height();
    let mut out = Vec::new();
    match mime_type {
        "image/png" => {
            let enc = PngEncoder::new(&mut out);
            thumb.write_with_encoder(enc).map_err(|e| e.to_string())?;
            if out.len() > max_bytes {
                return encode_jpeg(&thumb, max_bytes);
            }
            Ok(Some(ResizedImage {
                bytes: out,
                mime_type: "image/png",
                width,
                height,
            }))
        }
        _ => encode_jpeg(&thumb, max_bytes),
    }
}

fn encode_jpeg(img: &DynamicImage, max_bytes: usize) -> Result<Option<ResizedImage>, String> {
    let width = img.width();
    let height = img.height();
    for quality in JPEG_QUALITY_STEPS {
        let mut out = Vec::new();
        let enc = JpegEncoder::new_with_quality(&mut out, quality);
        img.write_with_encoder(enc).map_err(|e| e.to_string())?;
        if out.len() <= max_bytes {
            return Ok(Some(ResizedImage {
                bytes: out,
                mime_type: "image/jpeg",
                width,
                height,
            }));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};
    use tempfile::tempdir;

    fn sample_png_bytes(width: u32, height: u32) -> Vec<u8> {
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_pixel(width, height, Rgba([12, 34, 56, 255]));
        let mut out = Vec::new();
        img.write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
            .expect("encode png");
        out
    }

    #[test]
    fn view_image_emits_only_requested_output_encoding() {
        let dir = tempdir().expect("dir");
        std::fs::write(dir.path().join("pixel.png"), sample_png_bytes(4, 4)).expect("write");
        let ws = Workspace::new(dir.path().to_path_buf()).expect("ws");

        let mcp = view_image(&ws, &json!({"path": "pixel.png"})).expect("mcp image");
        assert!(mcp.get("base64").is_some());
        assert!(mcp.get("data_url").is_none());
        assert_eq!(mcp["width"], 4);
        assert_eq!(mcp["height"], 4);

        let data_url =
            view_image(&ws, &json!({"path": "pixel.png", "output": "data_url"})).expect("data_url");
        assert!(data_url.get("data_url").is_some());
        assert!(data_url.get("base64").is_none());
    }

    #[test]
    fn view_image_rejects_oversized_file_before_decoding_when_auto_resize_disabled() {
        let dir = tempdir().expect("dir");
        std::fs::write(dir.path().join("big.png"), vec![0u8; 4096]).expect("write");
        let ws = Workspace::new(dir.path().to_path_buf()).expect("ws");

        let err = view_image(
            &ws,
            &json!({"path": "big.png", "max_bytes": 1024, "auto_resize": false}),
        )
        .expect_err("oversized");
        assert_eq!(err.to_error_value()["code"], "OUTPUT_TOO_LARGE");
    }
}
