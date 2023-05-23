mod adpcm;
mod shape;
mod sound;


use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use clap::Parser;
use swf::Tag;

use crate::shape::shape_to_svg;
use crate::sound::Sound;


#[derive(Parser)]
struct Opts {
    swf_path: PathBuf,
}


fn process_tags(filename_prefix: &str, tags: &[Tag]) {
    let mut stream_sound: Option<Sound> = None;
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
            Tag::DefineBits { .. } => {},
            Tag::DefineBitsJpeg2 { .. } => {},
            Tag::DefineBitsLossless(_) => {},
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
