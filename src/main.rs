mod adpcm;


use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use clap::Parser;
use swf::{AudioCompression, Tag, SoundStreamHead};

use crate::adpcm::AdpcmDecoder;


#[derive(Parser)]
struct Opts {
    swf_path: PathBuf,
}


fn process_tags(filename_prefix: &str, tags: &[Tag]) {
    let mut stream: Vec<u8> = Vec::new();
    let mut stream_head: Option<Box<SoundStreamHead>> = None;
    for tag in tags {
        match tag {
            Tag::DefineSound(snd) => {
                if let AudioCompression::Mp3 = snd.format.compression {
                    let file_name = format!("{}{}.mp3", filename_prefix, snd.id);
                    let mut mp3 = File::create(file_name)
                        .expect("failed to open MP3 file");
                    mp3.write_all(snd.data)
                        .expect("failed to write MP3 file");
                } else {
                    println!("unexpected compression {:?}", snd.format.compression);
                }
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
            Tag::DefineEditText(_) => {},
            Tag::DefineFont(_) => {},
            Tag::DefineFont2(_) => {},
            Tag::DefineFontInfo(_) => {},
            Tag::DefineMorphShape(_) => {},
            Tag::DefineShape(_) => {},
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
                if let Some(sh) = &stream_head {
                    match sh.stream_format.compression {
                        AudioCompression::Mp3 => {
                            // the data can be decoded by MP3 playback software
                            stream.extend(*ssb);
                        },
                        AudioCompression::Adpcm => {
                            // this needs decoding first
                            let adpcm_reader = AdpcmDecoder::new(*ssb, sh.stream_format.is_stereo)
                                .expect("failed to create ADPCM reader");
                            for samples in adpcm_reader {
                                stream.extend(samples[0].to_le_bytes());
                                if sh.stream_format.is_stereo {
                                    stream.extend(samples[1].to_le_bytes());
                                }
                            }
                        },
                        other => {
                            panic!("cannot deal with {:?} sound stream compression", other);
                        },
                    }
                }
            },
            Tag::SoundStreamHead(ssh) => {
                println!("{}stream: {:?}", filename_prefix, ssh);
                stream_head = Some(ssh.clone());
            },
            Tag::SoundStreamHead2(ssh) => {
                println!("{}stream {:?}", filename_prefix, ssh);
                stream_head = Some(ssh.clone());
            },
            Tag::StartSound(_) => {},
            other => {
                panic!("unhandled block: {:?}", other);
            },
        }
    }
    if stream.len() > 0 {
        let compression = stream_head
            .as_ref()
            .map(|sh| sh.stream_format.compression);
        match compression {
            Some(AudioCompression::Mp3) => {
                // output straight out as MP3
                let file_name = format!("{}stream.mp3", filename_prefix);
                let mut f = File::create(&file_name)
                    .expect("failed to open stream file");
                f.write_all(&stream)
                    .expect("failed to write stream file");
            },
            Some(AudioCompression::Uncompressed)|Some(AudioCompression::Adpcm) => {
                // we have reconstructed PCM from ADPCM, so treat them the same
                // note: RIFF is little-endian
                let stream_format = &stream_head.as_ref().unwrap().stream_format;
                let sample_rate_bytes = u32::from(stream_format.sample_rate).to_le_bytes();
                // sample rate * bytes per sample * channels
                let bytes_per_sec_bytes = (
                    u32::from(stream_format.sample_rate)
                    * if stream_format.is_16_bit { 2 } else { 1 }
                    * if stream_format.is_stereo { 2 } else { 1 }
                ).to_le_bytes();
                let sample_alignment_bytes = (
                    1u16
                    * if stream_format.is_16_bit { 2 } else { 1 }
                    * if stream_format.is_stereo { 2 } else { 1 }
                ).to_le_bytes();
                let bits_per_sample_bytes = match stream_format.compression {
                    AudioCompression::Uncompressed => {
                        if stream_format.is_16_bit { 16u16 } else { 8 }
                    },
                    AudioCompression::Adpcm => 16, // always decodes to signed-16 PCM
                    _ => unreachable!(),
                }.to_le_bytes();

                let fmt_data = [
                    // general information
                    0x01, 0x00, // format tag = PCM (0x0001)
                    if stream_format.is_stereo { 0x02 } else { 0x01 }, 0x00, // channels = stereo (0x0002) or mono (0x0001)
                    sample_rate_bytes[0], sample_rate_bytes[1], sample_rate_bytes[2], sample_rate_bytes[3], // sampling rate (u32)
                    bytes_per_sec_bytes[0], bytes_per_sec_bytes[1], bytes_per_sec_bytes[2], bytes_per_sec_bytes[3], // (average) bytes per second (u32)
                    sample_alignment_bytes[0], sample_alignment_bytes[1], // sample byte alignment (u16)

                    // format-specific information (PCM)
                    bits_per_sample_bytes[0], bits_per_sample_bytes[1], // bits per sample (u16)
                ];

                let riff_data_len =
                    4 // "WAVE" type identifier
                    + 4 // "fmt " chunk tag
                    + 4 // "fmt " chunk length value
                    + fmt_data.len() // "fmt " chunk data
                    + 4 // "data" chunk tag
                    + 4 // "data" chunk length value
                    + stream.len() // "data" chunk data
                ;
                let riff_data_len_u32: u32 = riff_data_len.try_into().expect("wave data too long for 32 bits");

                {
                    let file_name = format!("{}stream.wav", filename_prefix);
                    let f = File::create(&file_name)
                        .expect("failed to open stream file");
                    let mut writer = BufWriter::new(f);

                    writer.write_all(b"RIFF").unwrap();
                    writer.write_all(&riff_data_len_u32.to_le_bytes()).unwrap();
                    writer.write_all(b"WAVE").unwrap();
                    writer.write_all(b"fmt ").unwrap();
                    writer.write_all(&u32::try_from(fmt_data.len()).unwrap().to_le_bytes()).unwrap();
                    writer.write_all(&fmt_data).unwrap();
                    writer.write_all(b"data").unwrap();
                    writer.write_all(&u32::try_from(stream.len()).unwrap().to_le_bytes()).unwrap();
                    writer.write_all(&stream).unwrap();
                }
            },
            _ => {
                let file_name = format!("{}stream.bin", filename_prefix);
                let mut f = File::create(&file_name)
                    .expect("failed to open stream file");
                f.write_all(&stream)
                    .expect("failed to write stream file");
            },
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
