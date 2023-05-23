use std::fmt;
use std::io::{Read, Write};

use gif;
use jpeg_decoder::PixelFormat;
use png::{BitDepth, ColorType};


const GIF_MAGIC: &[u8] = b"\x47\x49\x46\x38\x39\x61";
const JPEG_MAGIC: &[u8] = b"\xFF\xD8";
const PNG_MAGIC: &[u8] = b"\x89\x50\x4E\x47\x0D\x0A\x1A\x0A";


#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}


#[derive(Debug)]
pub(crate) enum Error {
    Io(std::io::Error),
    JpegDecoding(jpeg_decoder::Error),
    PngDecoding(png::DecodingError),
    PngEncoding(png::EncodingError),
    GifDecoding(gif::DecodingError),
    ZlibDecoding(std::io::Error),
    ShortRead,
    Cmyk,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {}", e),
            Self::JpegDecoding(e) => write!(f, "JPEG decoding error: {}", e),
            Self::PngDecoding(e) => write!(f, "PNG decoding error: {}", e),
            Self::PngEncoding(e) => write!(f, "PNG encoding error: {}", e),
            Self::GifDecoding(e) => write!(f, "GIF decoding error: {}", e),
            Self::ZlibDecoding(e) => write!(f, "zlib encoding error: {}", e),
            Self::ShortRead => write!(f, "not enough bytes available"),
            Self::Cmyk => write!(f, "images in CMYK color are unsupported"),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::JpegDecoding(e) => Some(e),
            Self::PngDecoding(e) => Some(e),
            Self::PngEncoding(e) => Some(e),
            Self::GifDecoding(e) => Some(e),
            Self::ZlibDecoding(e) => Some(e),
            Self::ShortRead => None,
            Self::Cmyk => None,
        }
    }
}
impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self { Self::Io(value) }
}
impl From<jpeg_decoder::Error> for Error {
    fn from(value: jpeg_decoder::Error) -> Self { Self::JpegDecoding(value) }
}
impl From<png::DecodingError> for Error {
    fn from(value: png::DecodingError) -> Self { Self::PngDecoding(value) }
}
impl From<png::EncodingError> for Error {
    fn from(value: png::EncodingError) -> Self { Self::PngEncoding(value) }
}
impl From<gif::DecodingError> for Error {
    fn from(value: gif::DecodingError) -> Self { Self::GifDecoding(value) }
}


/// Scales a 5-bit value to an 8-bit value.
fn scale_5_to_8(value: u16) -> u8 {
    const SCALE_FACTOR: f64 = (0xFF as f64) / (0b11111 as f64);
    (((value & 0b11111) as f64) * SCALE_FACTOR) as u8
}


#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub data: BitmapData,
}
impl Bitmap {
    pub fn new(width: u32, height: u32, data: BitmapData) -> Self {
        Self {
            width,
            height,
            data,
        }
    }

    pub fn extension(&self) -> &str {
        match &self.data {
            BitmapData::Gif { .. } => "gif",
            BitmapData::Jpeg { alpha_data, .. } => {
                if alpha_data.is_some() {
                    "png"
                } else {
                    "jpeg"
                }
            },
            BitmapData::Png { .. } => "png",
            BitmapData::ColorMapped { .. } => "png",
            BitmapData::Rgb15 { .. } => "png",
            BitmapData::Rgb24 { .. } => "png",
        }
    }

    pub fn write<W: Write>(&self, mut write: W) -> Result<(), Error> {
        match &self.data {
            BitmapData::Gif { gif_data } => write.write_all(&gif_data)?,
            BitmapData::Png { png_data } => write.write_all(&png_data)?,
            BitmapData::Jpeg { jpeg_data, alpha_data } => {
                if let Some(ad) = alpha_data {
                    // decode alpha data
                    let mut alpha_pixels = Vec::new();
                    {
                        let mut decoder = flate2::read::ZlibDecoder::new(ad.as_slice());
                        decoder.read_to_end(&mut alpha_pixels)
                            .map_err(|e| Error::ZlibDecoding(e))?;
                    }

                    // we don't have JPEG-with-transparency; convert to PNG
                    let (image_info, pixels) = {
                        let mut decoder = jpeg_decoder::Decoder::new(jpeg_data.as_slice());
                        let image_info = decoder.info().unwrap();
                        (image_info, decoder.decode()?)
                    };

                    let mut pixels_iterator = pixels.iter();
                    let mut alpha_iterator = alpha_pixels.iter();

                    let mut png = png::Encoder::new(
                        write,
                        image_info.width.into(),
                        image_info.height.into(),
                    );
                    match image_info.pixel_format {
                        PixelFormat::L8 => {
                            png.set_color(ColorType::GrayscaleAlpha);
                            png.set_depth(BitDepth::Eight);
                            let mut writer = png.write_header()?;

                            let mut row = Vec::new();
                            for _ in 0..image_info.height {
                                row.clear();
                                for _ in 0..image_info.width {
                                    let grayscale_value = pixels_iterator.next()
                                        .ok_or(Error::ShortRead)?;
                                    let alpha_value = alpha_iterator.next()
                                        .ok_or(Error::ShortRead)?;
                                    row.push(*grayscale_value);
                                    row.push(*alpha_value);
                                }
                                writer.write_image_data(&row)?;
                            }
                        },
                        PixelFormat::L16 => {
                            png.set_color(ColorType::GrayscaleAlpha);
                            png.set_depth(BitDepth::Sixteen);
                            let mut writer = png.write_header()?;

                            let mut row = Vec::new();
                            for _ in 0..image_info.height {
                                row.clear();
                                for _ in 0..image_info.width {
                                    let grayscale_value_msb = pixels_iterator.next()
                                        .ok_or(Error::ShortRead)?;
                                    let grayscale_value_lsb = pixels_iterator.next()
                                        .ok_or(Error::ShortRead)?;
                                    let alpha_value = alpha_iterator.next()
                                        .ok_or(Error::ShortRead)?;
                                    row.push(*grayscale_value_msb);
                                    row.push(*grayscale_value_lsb);

                                    // 8-bit values can be scaled to 16 bits via duplication
                                    row.push(*alpha_value);
                                    row.push(*alpha_value);
                                }
                                writer.write_image_data(&row)?;
                            }
                        },
                        PixelFormat::RGB24 => {
                            png.set_color(ColorType::Rgba);
                            png.set_depth(BitDepth::Eight);
                            let mut writer = png.write_header()?;

                            let mut row = Vec::new();
                            for _ in 0..image_info.height {
                                row.clear();
                                for _ in 0..image_info.width {
                                    let r = pixels_iterator.next()
                                        .ok_or(Error::ShortRead)?;
                                    let g = pixels_iterator.next()
                                        .ok_or(Error::ShortRead)?;
                                    let b = pixels_iterator.next()
                                        .ok_or(Error::ShortRead)?;
                                    let alpha_value = alpha_iterator.next()
                                        .ok_or(Error::ShortRead)?;
                                    row.push(*r);
                                    row.push(*g);
                                    row.push(*b);
                                    row.push(*alpha_value);
                                }
                                writer.write_image_data(&row)?;
                            }
                        },
                        PixelFormat::CMYK32 => return Err(Error::Cmyk),
                    }
                } else {
                    write.write_all(jpeg_data)?;
                }
            },
            BitmapData::ColorMapped { palette, image_data } => {
                let mut palette_bytes = Vec::new();
                for color in palette {
                    palette_bytes.push(color.r);
                    palette_bytes.push(color.g);
                    palette_bytes.push(color.b);
                }

                let mut png = png::Encoder::new(
                    write,
                    self.width,
                    self.height,
                );
                png.set_color(ColorType::Indexed);
                png.set_depth(BitDepth::Eight);
                png.set_palette(&palette_bytes);
                let mut writer = png.write_header()?;
                writer.write_image_data(&image_data)?;
            },
            BitmapData::Rgb15 { zlib_data } => {
                let mut image_data = Vec::new();
                {
                    let mut decoder = flate2::read::ZlibDecoder::new(zlib_data.as_slice());
                    decoder.read_to_end(&mut image_data)
                        .map_err(|e| Error::ZlibDecoding(e))?;
                }

                let mut data_iter = image_data.iter();

                let mut png = png::Encoder::new(
                    write,
                    self.width,
                    self.height,
                );
                png.set_color(ColorType::Rgb);
                png.set_depth(BitDepth::Eight);
                let mut writer = png.write_header()?;
                let mut row = Vec::new();
                for _ in 0..self.height {
                    row.clear();
                    for _ in 0..self.width {
                        let top_byte = data_iter.next()
                            .ok_or(Error::ShortRead)?;
                        let bottom_byte = data_iter.next()
                            .ok_or(Error::ShortRead)?;
                        let word =
                            (u16::from(*top_byte) << 8)
                            | u16::from(*bottom_byte);
                        let r = scale_5_to_8(word >> 10);
                        let g = scale_5_to_8(word >>  5);
                        let b = scale_5_to_8(word >>  0);
                        row.push(r);
                        row.push(g);
                        row.push(b);
                    }
                    writer.write_image_data(&row)?;
                }
            },
            BitmapData::Rgb24 { zlib_data } => {
                let mut image_data = Vec::new();
                {
                    let mut decoder = flate2::read::ZlibDecoder::new(zlib_data.as_slice());
                    decoder.read_to_end(&mut image_data)
                        .map_err(|e| Error::ZlibDecoding(e))?;
                }

                let mut data_iter = image_data.iter();

                let mut png = png::Encoder::new(
                    write,
                    self.width,
                    self.height,
                );
                png.set_color(ColorType::Rgb);
                png.set_depth(BitDepth::Eight);
                let mut writer = png.write_header()?;
                let mut row = Vec::new();
                for _ in 0..self.height {
                    row.clear();
                    for _ in 0..self.width {
                        let r = data_iter.next()
                            .ok_or(Error::ShortRead)?;
                        let g = data_iter.next()
                            .ok_or(Error::ShortRead)?;
                        let b = data_iter.next()
                            .ok_or(Error::ShortRead)?;
                        row.push(*r);
                        row.push(*g);
                        row.push(*b);
                    }
                    writer.write_image_data(&row)?;
                }
            },
        }
        Ok(())
    }

    pub fn from_gif(gif_data: &[u8]) -> Result<Self, Error> {
        let decoder = gif::Decoder::new(gif_data)?;
        let width = decoder.width().into();
        let height = decoder.height().into();
        Ok(Self::new(
            width,
            height,
            BitmapData::Gif {
                gif_data: Vec::from(gif_data),
            },
        ))
    }

    pub fn from_png(png_data: &[u8]) -> Result<Self, Error> {
        let decoder = png::Decoder::new(png_data);
        let info = decoder.read_info()?;
        let width = info.info().width;
        let height = info.info().height;
        Ok(Self::new(
            width,
            height,
            BitmapData::Png {
                png_data: Vec::from(png_data),
            },
        ))
    }

    pub fn from_jpeg(jpeg_data: &[u8], alpha_data: Option<&[u8]>) -> Self {
        let decoder = jpeg_decoder::Decoder::new(jpeg_data);
        let image_info = decoder.info().unwrap();
        let width = image_info.width.into();
        let height = image_info.height.into();
        Self::new(
            width,
            height,
            BitmapData::Jpeg {
                jpeg_data: Vec::from(jpeg_data),
                alpha_data: alpha_data.map(|ad| Vec::from(ad)),
            },
        )
    }

    pub fn from_bytes(bytes: &[u8], alpha_bytes: Option<&[u8]>) -> Option<Self> {
        if bytes.starts_with(GIF_MAGIC) {
            Some(Bitmap::from_gif(bytes).ok()?)
        } else if bytes.starts_with(PNG_MAGIC) {
            Some(Bitmap::from_png(bytes).ok()?)
        } else if bytes.starts_with(JPEG_MAGIC) {
            Some(Bitmap::from_jpeg(bytes, alpha_bytes))
        } else {
            None
        }
    }
}


#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum BitmapData {
    Gif { gif_data: Vec<u8>, },
    Jpeg {
        jpeg_data: Vec<u8>,
        alpha_data: Option<Vec<u8>>,
    },
    Png { png_data: Vec<u8> },
    ColorMapped {
        palette: Vec<RgbColor>,
        image_data: Vec<u8>,
    },
    Rgb15 { zlib_data: Vec<u8> },
    Rgb24 { zlib_data: Vec<u8> },
}
impl BitmapData {
    pub fn is_gif(&self) -> bool {
        match self {
            Self::Gif { .. } => true,
            _ => false,
        }
    }

    pub fn is_jpeg(&self) -> bool {
        match self {
            Self::Jpeg { .. } => true,
            _ => false,
        }
    }

    pub fn is_png(&self) -> bool {
        match self {
            Self::Png { .. } => true,
            _ => false,
        }
    }

    pub fn is_lossless(&self) -> bool {
        match self {
            Self::ColorMapped { .. } => true,
            Self::Rgb15 { .. } => true,
            Self::Rgb24 { .. } => true,
            _ => false,
        }
    }
}
