//! Pure coordinate mapping for a single captured macOS image.
//!
//! ScreenCaptureKit returns dimensions in pixels and a global display/window
//! rectangle in logical screen points. Never assume Retina scale is 2.0.
use serde_json::{json, Value};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CaptureFrame {
    pub origin_x: f64,
    pub origin_y: f64,
    pub point_width: f64,
    pub point_height: f64,
    pub pixel_width: f64,
    pub pixel_height: f64,
}

impl CaptureFrame {
    pub fn from_metadata(meta: &Value) -> Result<Self, String> {
        fn number(meta: &Value, key: &str) -> Result<f64, String> {
            meta.get(key)
                .and_then(Value::as_f64)
                .filter(|value| value.is_finite())
                .ok_or_else(|| format!("INVALID_CAPTURE_GEOMETRY: missing or non-finite {key}"))
        }
        let frame = Self {
            origin_x: number(meta, "origin_x")?,
            origin_y: number(meta, "origin_y")?,
            point_width: number(meta, "point_width")?,
            point_height: number(meta, "point_height")?,
            pixel_width: number(meta, "pixel_width")?,
            pixel_height: number(meta, "pixel_height")?,
        };
        if frame.point_width <= 0.0
            || frame.point_height <= 0.0
            || frame.pixel_width < 1.0
            || frame.pixel_height < 1.0
            || frame.pixel_width > 8192.0
            || frame.pixel_height > 8192.0
        {
            return Err("INVALID_CAPTURE_GEOMETRY: unsupported capture dimensions".into());
        }
        Ok(frame)
    }

    pub fn point_for(&self, x: f64, y: f64, space: &str) -> Result<(f64, f64), String> {
        if !x.is_finite() || !y.is_finite() {
            return Err("INVALID_COORDINATE: coordinates must be finite".into());
        }
        match space {
            "image_pixels" => {
                if x < 0.0 || y < 0.0 || x >= self.pixel_width || y >= self.pixel_height {
                    return Err(
                        "STALE_SNAPSHOT: image pixel falls outside the captured frame".into(),
                    );
                }
                let point_x = self.origin_x + (x + 0.5) * self.point_width / self.pixel_width;
                let point_y = self.origin_y + (y + 0.5) * self.point_height / self.pixel_height;
                Ok((point_x, point_y))
            }
            "screen_points" => {
                if x < self.origin_x
                    || y < self.origin_y
                    || x >= self.origin_x + self.point_width
                    || y >= self.origin_y + self.point_height
                {
                    return Err(
                        "STALE_SNAPSHOT: screen point falls outside the captured frame".into(),
                    );
                }
                Ok((x, y))
            }
            _ => Err("INVALID_COORDINATE_SPACE: use image_pixels or screen_points".into()),
        }
    }

    pub fn map_action(&self, arguments: &Value) -> Result<Value, String> {
        let mut result = arguments.clone();
        let space = result
            .get("coordinate_space")
            .and_then(Value::as_str)
            .unwrap_or("screen_points")
            .to_owned();
        for (x_key, y_key) in [("x", "y"), ("end_x", "end_y")] {
            if let (Some(x), Some(y)) = (arguments.get(x_key), arguments.get(y_key)) {
                let (point_x, point_y) = self.point_for(
                    x.as_f64().ok_or("INVALID_COORDINATE: x must be numeric")?,
                    y.as_f64().ok_or("INVALID_COORDINATE: y must be numeric")?,
                    &space,
                )?;
                result[x_key] = json!(point_x);
                result[y_key] = json!(point_y);
            }
        }
        if let Some(obj) = result.as_object_mut() {
            obj.remove("coordinate_space");
            obj.remove("snapshot_id");
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame() -> CaptureFrame {
        CaptureFrame::from_metadata(&json!({
            "origin_x": -1440.0, "origin_y": -100.0,
            "point_width": 1440.0, "point_height": 900.0,
            "pixel_width": 1920, "pixel_height": 1200
        }))
        .unwrap()
    }

    #[test]
    fn maps_retina_downsampled_pixels_into_negative_global_desktop_points() {
        assert_eq!(
            frame().point_for(959.5, 599.5, "image_pixels").unwrap(),
            (-720.0, 350.0)
        );
        assert_eq!(
            frame().point_for(-720.0, 350.0, "screen_points").unwrap(),
            (-720.0, 350.0)
        );
    }

    #[test]
    fn requires_finite_and_within_capture_geometry() {
        for (x, y) in [(1920.0, 100.0), (-1.0, 0.0), (2.0, 1200.0), (f64::NAN, 3.0)] {
            assert!(frame().point_for(x, y, "image_pixels").is_err());
        }
        assert!(frame().point_for(1.0, 2.0, "screen_points").is_err());
        assert!(frame().point_for(0.0, 0.0, "weird").is_err());
        assert!(CaptureFrame::from_metadata(&json!({"pixel_width": 0})).is_err());
    }

    #[test]
    fn converts_both_ends_of_a_drag() {
        let action = frame()
            .map_action(&json!({
                "action": "drag", "coordinate_space": "image_pixels", "snapshot_id": "test",
                "x": 0, "y": 0, "end_x": 1919, "end_y": 1199
            }))
            .unwrap();
        assert!(action["x"].as_f64().unwrap() < -1439.0);
        assert!(action["end_x"].as_f64().unwrap() < 0.0);
        assert!(action.get("snapshot_id").is_none());
    }
}
