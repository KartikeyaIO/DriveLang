use std::io::BufReader;
use std::fs::File;
use openh264::decoder::Decoder as H264Decoder;

use mp4::Mp4Reader;

use crate::media::video::VideoFrame;


pub struct Video {
    reader: Mp4Reader<BufReader<File>>,
    track_id: u32,

    decoder: H264Decoder,
    sps_pps: Vec<u8>,
    nal_length_size: usize,

    current_frame: VideoFrame,
    bitstream: Vec<u8>,

    current_sample: u32,
    key_frames: Vec<u32>,

    fps: f64,
    frame_count: u64,
}

impl Video {
    
}