use serde_json::Value;
use std::ffi::{c_char, CStr, CString};
unsafe extern "C" {
    fn computer_native_screen_diagnostics() -> *mut c_char;
    fn computer_native_windows() -> *mut c_char;
    fn computer_native_capture(input: *const c_char) -> *mut c_char;
    fn computer_native_free(pointer: *mut c_char);
    fn computer_native_frontmost() -> *mut c_char;
    fn computer_native_available_apps() -> *mut c_char;
    fn computer_native_ui_tree(expected_bundle: *const c_char) -> *mut c_char;
    fn computer_native_action(input: *const c_char) -> *mut c_char;
    fn computer_native_accessibility_granted() -> bool;
    fn computer_native_request_accessibility() -> bool;
}
struct NativeOwnedString(*mut c_char);

impl Drop for NativeOwnedString {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { computer_native_free(self.0) };
        }
    }
}

fn decode(pointer: *mut c_char) -> Result<Value, String> {
    if pointer.is_null() {
        return Err("COMPUTER_NATIVE_ERROR: native bridge returned null".into());
    }
    // RAII guard ensures `computer_native_free` always runs even on parse error,
    // while allowing zero-copy deserialization directly from the C buffer.
    let owned = NativeOwnedString(pointer);
    let bytes = unsafe { CStr::from_ptr(owned.0) }.to_bytes();
    let value: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    drop(owned);
    if value["ok"] != true {
        return Err(format!(
            "{}: {}",
            value["error"].as_str().unwrap_or("NATIVE_ERROR"),
            value["message"]
                .as_str()
                .unwrap_or("Native operation failed")
        ));
    }
    Ok(value)
}
pub(super) fn windows() -> Result<Value, String> {
    decode(unsafe { computer_native_windows() })
}
pub(super) fn screen_diagnostics() -> Result<Value, String> {
    decode(unsafe { computer_native_screen_diagnostics() })
}
pub(super) fn capture(args: &Value) -> Result<Value, String> {
    let input = CString::new(args.to_string()).map_err(|e| e.to_string())?;
    decode(unsafe { computer_native_capture(input.as_ptr()) })
}
pub(super) fn frontmost() -> Result<Value, String> {
    decode(unsafe { computer_native_frontmost() })
}
pub(super) fn apps() -> Result<Value, String> {
    decode(unsafe { computer_native_available_apps() })
}
pub(super) fn ui_tree(expected: Option<&str>) -> Result<Value, String> {
    let input = expected
        .map(CString::new)
        .transpose()
        .map_err(|e| e.to_string())?;
    decode(unsafe {
        computer_native_ui_tree(input.as_ref().map_or(std::ptr::null(), |x| x.as_ptr()))
    })
}
pub(super) fn action(args: &Value) -> Result<Value, String> {
    let input = CString::new(args.to_string()).map_err(|e| e.to_string())?;
    decode(unsafe { computer_native_action(input.as_ptr()) })
}
pub(super) fn accessibility_granted() -> bool {
    unsafe { computer_native_accessibility_granted() }
}
pub(super) fn request_accessibility() {
    unsafe {
        computer_native_request_accessibility();
    }
}
