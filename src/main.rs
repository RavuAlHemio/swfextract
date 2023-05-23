mod adpcm;
mod bitmap;
mod shape;
mod sound;


use std::collections::HashMap;
use std::fs::File;
use std::io::{Write, Read};
use std::path::PathBuf;

use clap::Parser;
use swf::{BitmapFormat, Tag};

use crate::bitmap::{Bitmap, BitmapData, RgbColor};
use crate::shape::shape_to_svg;
use crate::sound::Sound;


#[derive(Parser)]
struct Opts {
    swf_path: PathBuf,
}


fn process_tags(filename_prefix: &str, tags: &[Tag]) {
    let mut stream_sound: Option<Sound> = None;
    let mut id_to_bitmap: HashMap<u16, Bitmap> = HashMap::new();
    for tag in tags {
        match tag {
            Tag::DefineSound(snd) => {
                let sound = Sound {
                    format: snd.format.clone(),
                    data: Vec::from(snd.data),
                };
                let file_name = format!("{}{}.{}", filename_prefix, snd.id, sound.extension());
                let output = File::create(file_name)
                    .expect("failed to open sound file");
                sound.write(output)
                    .expect("failed to write sound file");
            },
            Tag::DefineBinaryData(bd) => {
                let file_name = format!("{}{}.bin", filename_prefix, bd.id);
                let mut bin = File::create(file_name)
                    .expect("failed to open binary file");
                bin.write_all(bd.data)
                    .expect("failed to write binary data");
            },
            Tag::DefineSprite(ds) => {
                // process subtags
                let filename_prefix = format!("{}-", ds.id);
                process_tags(&filename_prefix, &ds.tags);
            },
            Tag::ExportAssets(ass) => {
                println!("exporting assets: {:?}", ass);
            },
            Tag::DefineBits { id, jpeg_data } => {
                id_to_bitmap.insert(
                    *id,
                    Bitmap::from_jpeg(jpeg_data, None),
                );
            },
            Tag::DefineBitsJpeg2 { id, jpeg_data } => {
                // Jpeg2 may also be PNG or GIF
                id_to_bitmap.insert(
                    *id,
                    Bitmap::from_bytes(jpeg_data, None).unwrap(),
                );
            },
            Tag::DefineBitsJpeg3(j3) => {
                // Jpeg3 may also be PNG or GIF
                let alpha_data = if j3.alpha_data.len() > 0 {
                    Some(j3.alpha_data)
                } else {
                    None
                };
                id_to_bitmap.insert(
                    j3.id,
                    Bitmap::from_bytes(j3.data, alpha_data).unwrap(),
                );
            },
            Tag::DefineBitsLossless(bmap) => {
                // TODO: handle alpha if bmap.version == 2
                match &bmap.format {
                    BitmapFormat::ColorMap8 { num_colors } => {
                        let actual_num_colors = usize::from(*num_colors) + 1;
                        let mut palette_bytes = vec![0u8; 3*actual_num_colors];
                        let mut image_data = Vec::new();
                        let mut decoder = flate2::read::ZlibDecoder::new(bmap.data);
                        decoder.read_exact(&mut palette_bytes)
                            .expect("failed to read palette");
                        decoder.read_to_end(&mut image_data)
                            .expect("failed to read image data");

                        let mut palette = Vec::with_capacity(actual_num_colors);
                        let mut palette_iter = palette_bytes.iter();
                        for _ in 0..actual_num_colors {
                            let r = *palette_iter.next().unwrap();
                            let g = *palette_iter.next().unwrap();
                            let b = *palette_iter.next().unwrap();
                            palette.push(RgbColor { r, g, b });
                        }

                        id_to_bitmap.insert(
                            bmap.id,
                            Bitmap::new(
                                bmap.width.into(),
                                bmap.height.into(),
                                BitmapData::ColorMapped {
                                    palette,
                                    image_data,
                                },
                            )
                        );
                    },
                    BitmapFormat::Rgb15 => {
                        id_to_bitmap.insert(
                            bmap.id,
                            Bitmap::new(
                                bmap.width.into(),
                                bmap.height.into(),
                                BitmapData::Rgb15 {
                                    zlib_data: Vec::from(bmap.data),
                                },
                            )
                        );
                    },
                    BitmapFormat::Rgb32 => {
                        id_to_bitmap.insert(
                            bmap.id,
                            Bitmap::new(
                                bmap.width.into(),
                                bmap.height.into(),
                                BitmapData::Rgb24 {
                                    zlib_data: Vec::from(bmap.data),
                                },
                            )
                        );
                    },
                }
            },
            Tag::DefineButton2(_) => {},
            Tag::DefineButtonSound(_) => {},
            Tag::DefineEditText(et) => {
                if let Some(it) = et.initial_text {
                    let filename = format!("{}{}.txt", filename_prefix, et.id);
                    let mut f = File::create(&filename)
                        .expect("failed to open text file");
                    f.write_all(it.as_bytes())
                        .expect("failed to write text file");
                }
            },
            Tag::DefineFont(_) => {},
            Tag::DefineFont2(_) => {},
            Tag::DefineFontInfo(_) => {},
            Tag::DefineMorphShape(_) => {},
            Tag::DefineShape(sh) => {
                let shape_data = shape_to_svg(sh);
                let filename = format!("{}{}.svg", filename_prefix, sh.id);
                let mut f = File::create(&filename)
                    .expect("failed to open SVG file");
                f.write_all(shape_data.as_bytes())
                    .expect("failed to write SVG file");
            },
            Tag::DefineText(_) => {},
            Tag::DoAction(_) => {},
            Tag::FrameLabel(_) => {},
            Tag::JpegTables(_) => {},
            Tag::PlaceObject(_) => {},
            Tag::Protect(_) => {},
            Tag::RemoveObject(_) => {},
            Tag::SetBackgroundColor(_) => {},
            Tag::ShowFrame => {},
            Tag::SoundStreamBlock(ssb) => {
                if let Some(snd) = &mut stream_sound {
                    snd.append_data(ssb);
                }
            },
            Tag::SoundStreamHead(ssh) => {
                stream_sound = Some(Sound {
                    format: ssh.stream_format.clone(),
                    data: Vec::new(),
                });
            },
            Tag::SoundStreamHead2(ssh) => {
                stream_sound = Some(Sound {
                    format: ssh.stream_format.clone(),
                    data: Vec::new(),
                });
            },
            Tag::StartSound(_) => {},
            other => {
                panic!("unhandled block: {:?}", other);
            },
        }
    }
    if let Some(ssnd) = stream_sound {
        if ssnd.data.len() > 0 {
            let file_name = format!("{}stream.{}", filename_prefix, ssnd.extension());
            let f = File::create(&file_name)
                .expect("failed to open stream file");
            ssnd.write(f)
                .expect("failed to write stream file");
        }
    }
}


fn main() {
    let opts = Opts::parse();

    let swf_buf = {
        let f = File::open(&opts.swf_path)
            .expect("failed to open SWF file");
        swf::decompress_swf(f)
            .expect("failed to decompress SWF file")
    };
    let swf = swf::parse_swf(&swf_buf)
        .expect("failed to parse SWF file");

    process_tags("", &swf.tags);
}
