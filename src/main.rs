use editron_v1::{
    io::io::{decode_audio, encode_wav},
    media::{track::Track, video::TimeStamp},
};
use std::path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path2 = path::Path::new("test_inputs/illegal_song.mp3");
    let path1 = path::Path::new("test_inputs/ready_song.mp3");
    let output = path::Path::new("Outputs/mixed.wav");
    let track1 = decode_audio(&path1).expect("failed to decode!");
    let track2 = decode_audio(&path2).expect("failed to decode!");
    let pause = Track::silence(TimeStamp::from_seconds(2.0, 1, 1000), 44100, 2);
    let slice1 = track1.slice(
        TimeStamp::from_seconds(0.0, 1, 44100),
        TimeStamp::from_seconds(60.0, 1, 44100),
    );
    let slice2 = track2.slice(
        TimeStamp::from_seconds(0.0, 1, 44100),
        TimeStamp::from_seconds(53.0, 1, 44100),
    );
    let mashup = Track::merge_many(&[slice1, pause, slice2]).expect("Merge failed!");
    encode_wav(&mashup, output).expect("Encoding failed!");
    Ok(())
}
