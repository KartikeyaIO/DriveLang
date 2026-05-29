use media::frame::{Frame, PixelData};

// use crate::io::export_frame_to_png;

mod engine;
pub mod experiments;
pub mod filter;
pub mod filters;
pub mod io;
pub mod lexer;
pub mod media;
pub mod parser;
pub mod range;
pub mod text;

// #[test]
// fn test_image_operations() {
//     let path = "test_inputs/image.jpg";
//     for fmt in ["rgb24", "rgba", "gray", "yuv420p"] {
//         let mut frame = io::load_image(path, fmt).expect("Load failed! Check image or functions!");

//         if let PixelData::RGBA(..) = frame.data() {
//             frame.opacity(50).expect("Opacity failed!");
//         }

//         let output_path = format!("Outputs/output_{}.png", fmt);
//         match io::export_frame_to_png(&frame, &output_path) {
//             Ok(_) => println!("Success! Output at: {output_path}"),
//             Err(e) => panic!("Export failed: {e}"),
//         }
//     }
// }

// #[test]
// fn test_text_to_image() {
//     let path = include_bytes!("../test_inputs/georgia.ttf") as &[u8];
//     let font = Font::from_bytes(path, fontdue::FontSettings::default()).unwrap();
//     let pos = Pos(50, 100);
//     let color = Color::RGBA(250, 0, 50, 255);
//     let text = Text::new("Hello World", font, 120.0, pos, color);
//     let frame = text.picturize().unwrap();
//     export_frame_to_png(&frame, "Outputs/text.png").unwrap();
// }

// #[test]
// fn test_blur_effect() {
//     let path = "test_inputs/image.jpg";
//     let blur = GaussianBlur::new(6.0);
//     for fmt in ["rgb24", "rgba", "gray", "yuv420p"] {
//         let frame = io::load_image(path, fmt).expect("Load failed! Check image or functions!");

//         let frame = blur.apply(frame);

//         let output_path = format!("Outputs/output_blur_{}.png", fmt);
//         match io::export_frame_to_png(&frame, &output_path) {
//             Ok(_) => println!("Blur success! Output at: {output_path}"),
//             Err(e) => panic!("Export failed: {e}"),
//         }
//     }
// }

// #[test]
// fn test_brightness() {
//     let path = "test_inputs/image.jpg";
//     for fmt in ["rgb24", "rgba", "gray", "yuv420p"] {
//         let mut frame = io::load_image(path, fmt).expect("Load failed!");

//         frame.brightness(50);
//         io::export_frame_to_png(&frame, &format!("Outputs/brightness_plus_{}.png", fmt))
//             .expect("Export failed!");

//         frame.brightness(-100);
//         io::export_frame_to_png(&frame, &format!("Outputs/brightness_minus_{}.png", fmt))
//             .expect("Export failed!");
//     }
// }

// #[test]
// fn test_contrast() {
//     let path = "test_inputs/image.jpg";
//     for fmt in ["rgb24", "rgba", "gray", "yuv420p"] {
//         let mut frame = io::load_image(path, fmt).expect("Load failed!");

//         frame.contrast().expect("Contrast failed!");

//         let output_path = format!("Outputs/contrast_{}.png", fmt);
//         io::export_frame_to_png(&frame, &output_path).expect("Export failed!");
//     }
// }

// #[test]
// fn test_pixel_ops() {
//     let mut frame = io::load_image("test_inputs/image.jpg", "rgb24").expect("Load failed!");

//     let pos = Pos(10, 10);
//     let original = frame.get_pixel(&pos).expect("get_pixel failed!");
//     println!("Original pixel: {:?}", original);

//     let new_color = Color::RGB(255, 0, 0);
//     frame
//         .set_pixel(&pos, &new_color)
//         .expect("set_pixel failed!");

//     let updated = frame.get_pixel(&pos).expect("get_pixel failed!");
//     println!("{updated:?} \n {new_color:?}")
// }
// #[test]
// fn test_cwd() {
//     println!("{}", std::env::current_dir().unwrap().display());
// }
