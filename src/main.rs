use editron_v1::{
    io::video_io::{Video, VideoEncoder},
    media::frame::Pos,
};

use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();
    let mut source = Video::open("test_inputs/input2.mp4").expect("Failed to open the file!");
    println!("Video Opening time: {:?}", start.elapsed());
    let start2 = Instant::now();
    let audio_track = source.decode_audio().expect("failed to decode audio!");
    println!("Audio Decoding Time: {:?}", start2.elapsed());
    let mut encoder = VideoEncoder::open("Outputs/output2.mp4", &source, Some(&audio_track))
        .expect("Failed to open the encoder");
    encoder
        .encode_audio(&audio_track)
        .expect("Audio Encoding failed");
    let start3 = Instant::now();
    while let Some(mut frame) = source
        .decode_next()
        .expect("You have reached the End of this file")
    {
        for i in (0..frame.width()).step_by(3) {
            for j in (0..frame.height()).step_by(4) {
                let pos = Pos(i, j);
                frame.brightness(&pos, 100);
            }
        }
        encoder.encode_frame(&frame).expect("Encoding Failed");
    }
    println!("Video frame by frame Editing time: {:?}", start3.elapsed());

    // Mux the original audio back in unchanged

    encoder.finish().expect("Video encoding failed");
    println!("Total time: {:?}", start.elapsed());
    Ok(())
}
