use std::borrow::Cow;
use std::io::{Error, Read};

use libdeflater::{CompressionLvl, Compressor, Decompressor};

// CompressionType is an enum that represents different compression types that this code can handle
pub(crate) enum CompressionType {
    Gzip,
    Zlib,
    Uncompressed,
}

// Implementations of various methods for the CompressionType enum
impl CompressionType {
    // Converts a u8 value to the corresponding CompressionType variant
    pub(crate) fn from_u8(int: u8) -> Option<CompressionType> {
        match int {
            1 => Some(CompressionType::Gzip),
            2 => Some(CompressionType::Zlib),
            3 => Some(CompressionType::Uncompressed),
            _ => None,
        }
    }

    // Converts a CompressionType variant to the corresponding u8 value
    pub(crate) fn to_u8(&self) -> u8 {
        match self {
            CompressionType::Gzip => 1,
            CompressionType::Zlib => 2,
            CompressionType::Uncompressed => 3,
        }
    }

    // Decompresses a given slice of bytes using the decompression method corresponding to the CompressionType variant
    pub(crate) fn decompress(&self, data: Cow<[u8]>) -> Result<Vec<u8>, Error> {
        match self {
            // For gzip compression, use libdeflate to decompress the data
            CompressionType::Gzip => {
                // gzip RFC1952: a valid gzip file has an ISIZE field in the
                // footer, which is a little-endian u32 number representing the
                // decompressed size. This is ideal for libdeflate, which needs
                // pre-allocating the decompressed buffer.
                let isize = {
                    let isize_start = data.len() - 4;
                    let isize_bytes = &data[isize_start..];
                    let mut ret: u32 = isize_bytes[0] as u32;
                    ret |= (isize_bytes[1] as u32) << 8;
                    ret |= (isize_bytes[2] as u32) << 16;
                    ret |= (isize_bytes[3] as u32) << 24;
                    ret as usize
                };

                let mut decompressor = Decompressor::new();
                let mut outbuf = Vec::new();
                outbuf.resize(isize, 0);
                decompressor
                    .gzip_decompress(data.as_ref(), &mut outbuf)
                    .unwrap();
                Ok(outbuf)
            }
            // For zlib compression, use the system zlib implementation provided by the `flate2` crate to decompress the data
            CompressionType::Zlib => {
                //we don't know the decompressed size, so we have to use system zlib here
                let mut decoder = flate2::read::ZlibDecoder::new(data.as_ref());
                let mut buffer = Vec::new();
                decoder.read_to_end(&mut buffer)?;
                Ok(buffer)
            }
            // For uncompressed data, return a copy of the input data
            CompressionType::Uncompressed => Ok(data.to_vec()),
        }
    }

    // Compresses a given slice of bytes using the compression method corresponding to the CompressionType variant
    pub(crate) fn compress(
        &self,
        data: &[u8],
        compression: CompressionLvl,
    ) -> Result<Vec<u8>, Error> {
        match self {
            // For gzip compression, use libdeflate to compress the data
            CompressionType::Gzip => {
                let mut compressor = Compressor::new(compression);
                let max_sz = compressor.gzip_compress_bound(data.len());
                let mut compressed_data = Vec::new();
                compressed_data.resize(max_sz, 0);
                let actual_sz = compressor
                    .gzip_compress(data, &mut compressed_data)
                    .unwrap();
                compressed_data.resize(actual_sz, 0);
                Ok(compressed_data)
            }
            // For zlib compression, use libdeflate to compress the data
            CompressionType::Zlib => {
                let mut compressor = Compressor::new(compression);
                let max_sz = compressor.zlib_compress_bound(data.len());
                let mut compressed_data = Vec::new();
                compressed_data.resize(max_sz, 0);
                let actual_sz = compressor
                    .zlib_compress(data, &mut compressed_data)
                    .unwrap();
                compressed_data.resize(actual_sz, 0);
                Ok(compressed_data)
            }
            // For uncompressed data, return a copy of the input data
            CompressionType::Uncompressed => Ok(data.to_vec()),
        }
    }
}
