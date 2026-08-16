use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use images::encoding::rotate_180_to_png::rotate_180_to_png;

#[tauri::command]
pub fn flip_image(image: &str) -> Result<String, String> {
  let bytes = BASE64_STANDARD.decode(image).map_err(|err| format!("Base64 decode error: {}", err))?;
  let output = rotate_180_to_png(&bytes).map_err(|err| format!("Image processing error: {}", err))?;
  Ok(BASE64_STANDARD.encode(output))
}
