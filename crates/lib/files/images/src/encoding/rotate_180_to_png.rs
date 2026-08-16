use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::ImageReader;
use std::io::Cursor;

pub fn rotate_180_to_png(bytes: &[u8]) -> Result<Vec<u8>, image::ImageError> {
  let image = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?.decode()?;
  let mut output = Vec::new();
  let encoder = PngEncoder::new_with_quality(&mut output, CompressionType::Fast, FilterType::Adaptive);
  image.rotate180().write_with_encoder(encoder)?;
  Ok(output)
}

#[cfg(test)]
mod tests {
  use super::*;
  use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};

  #[test]
  fn flips_real_image_pixels_and_returns_png() {
    let mut source = RgbaImage::new(2, 1);
    source.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
    source.put_pixel(1, 0, Rgba([0, 0, 255, 255]));
    let mut input = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(source).write_to(&mut input, ImageFormat::Png).unwrap();

    let output = rotate_180_to_png(&input.into_inner()).unwrap();
    let decoded = image::load_from_memory_with_format(&output, ImageFormat::Png).unwrap().to_rgba8();

    assert_eq!(decoded.dimensions(), (2, 1));
    assert_eq!(*decoded.get_pixel(0, 0), Rgba([0, 0, 255, 255]));
    assert_eq!(*decoded.get_pixel(1, 0), Rgba([255, 0, 0, 255]));
  }
}
