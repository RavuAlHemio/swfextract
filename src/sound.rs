use std::io::Write;

use swf::{AudioCompression, SoundFormat};

use crate::adpcm::AdpcmDecoder;


pub(crate) struct Sound {
    pub format: SoundFormat,
    pub data: Vec<u8>,
}
impl Sound {
    pub fn extension(&self) -> &'static str {
        match self.format.compression {
            AudioCompression::Adpcm => "wav",
            AudioCompression::Uncompressed => "wav",
            AudioCompression::UncompressedUnknownEndian => "wav",
            AudioCompression::Mp3 => "mp3",
            _other => "bin",
        }
    }

    pub fn append_data(&mut self, data: &[u8]) {
        if let AudioCompression::Adpcm = self.format.compression {
            // this needs decoding first
            let adpcm_reader = AdpcmDecoder::new(data, self.format.is_stereo)
                .expect("failed to create ADPCM reader");
            for samples in adpcm_reader {
                self.data.extend(samples[0].to_le_bytes());
                if self.format.is_stereo {
                    self.data.extend(samples[1].to_le_bytes());
                }
            }
        } else {
            self.data.extend(data);
        }
    }

    pub fn write<W: Write>(&self, mut writer: W) -> Result<(), std::io::Error> {
        match self.format.compression {
            AudioCompression::Mp3 => {
                // data already contains all necessary headers
                writer.write_all(&self.data)
            },
            AudioCompression::Adpcm|AudioCompression::Uncompressed|AudioCompression::UncompressedUnknownEndian => {
                self.write_wav(writer)
            },
            _ => {
                // we do not yet decode these formats
                writer.write_all(&self.data)
            },
        }
    }

    fn write_wav<W: Write>(&self, mut writer: W) -> Result<(), std::io::Error> {
        let sample_rate_bytes = u32::from(self.format.sample_rate).to_le_bytes();
        // sample rate * bytes per sample * channels
        let bytes_per_sec_bytes = (
            u32::from(self.format.sample_rate)
            * if self.format.is_16_bit { 2 } else { 1 }
            * if self.format.is_stereo { 2 } else { 1 }
        ).to_le_bytes();
        let sample_alignment_bytes = (
            1u16
            * if self.format.is_16_bit { 2 } else { 1 }
            * if self.format.is_stereo { 2 } else { 1 }
        ).to_le_bytes();
        let bits_per_sample_bytes = match self.format.compression {
            AudioCompression::Uncompressed => {
                if self.format.is_16_bit { 16u16 } else { 8 }
            },
            AudioCompression::Adpcm => 16, // always decodes to signed-16 PCM
            _ => unreachable!(),
        }.to_le_bytes();

        let fmt_data = [
            // general information
            0x01, 0x00, // format tag = PCM (0x0001)
            if self.format.is_stereo { 0x02 } else { 0x01 }, 0x00, // channels = stereo (0x0002) or mono (0x0001)
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
            + self.data.len() // "data" chunk data
        ;
        let riff_data_len_u32: u32 = riff_data_len.try_into().expect("wave data too long for 32 bits");

        writer.write_all(b"RIFF")?;
        writer.write_all(&riff_data_len_u32.to_le_bytes())?;
        writer.write_all(b"WAVE")?;
        writer.write_all(b"fmt ")?;
        writer.write_all(&u32::try_from(fmt_data.len()).unwrap().to_le_bytes())?;
        writer.write_all(&fmt_data)?;
        writer.write_all(b"data")?;
        writer.write_all(&u32::try_from(self.data.len()).unwrap().to_le_bytes())?;
        writer.write_all(&self.data)?;
        Ok(())
    }
}
