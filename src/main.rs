use drive::io::io::{decode_audio,encode_wav};
use drive::media::track::Track;
use std::path::Path;
use drive::media::video::TimeStamp;

fn main(){
    let track1= decode_audio(&Path::new("test_inputs/janmastmi_song2.wav")).expect("Failed to decode audio 1!");
    let mut audio2= decode_audio(&Path::new("test_inputs/janmastmi_song1.mp3")).expect("Failed to decode audio 2!");
    let mut audio1 = track1.slice(TimeStamp::from_seconds(0.0, 1 ,48000), TimeStamp::from_seconds(31.98, 1 ,48000));

    audio1.fade_in(TimeStamp::from_seconds(2.5, 1,48000));
    audio2.fade_in(TimeStamp::from_seconds(1.0, 1,48000));
    audio1.fade_out(TimeStamp::from_seconds(0.3, 1,48000));
    audio2.fade_out(TimeStamp::from_seconds(2.5, 1,48000));

    let merged = Track::merge_many(&[audio1,audio2]).expect("Merge Failed!");
    encode_wav(&merged, Path::new("test_inputs/janmastmi_song_output.wav")).expect("Failed to encode audio to wav");

}
