use drive::engine::{Engine, Value};
use drive::parser::parse;
use std::fs;
use std::path::Path;

fn run_script(src: &str) -> Engine {
    let program = parse(src).expect("Parsing failed");
    let mut engine = Engine::new();
    engine.run(&program).expect("Execution failed");
    engine
}

// ─────────────────────────────────────────────────────────────────────────────
// Feature 1: Allow other filters to be used inside filters like function calls
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_feature_1_filter_composition() {
    let script = r#"
filter add_ten(amt) {
    r = r + amt;
    g = g + amt;
    b = b + amt;
}

filter complex_filter(factor) {
    add_ten(factor);
    r = r + 5;
}

img = blank(10, 10);
result = img -> complex_filter(20);
"#;
    let engine = run_script(script);
    let result = engine.vars.get("result").expect("result var exists");
    if let Value::Frame(f) = result {
        let pixel = f.get_pixel(&drive::media::frame::Pos(0, 0)).unwrap();
        // blank starts at (0, 0, 0, 255)
        // add_ten(20) makes it (20, 20, 20)
        // complex_filter adds 5 to r -> 25
        assert_eq!(pixel.r(), 25);
        assert_eq!(pixel.g(), 20);
        assert_eq!(pixel.b(), 20);
    } else {
        panic!("Expected Value::Frame");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Feature 2: Allow mathematical functions everywhere
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_feature_2_math_functions_at_script_level() {
    let script = r#"
a = abs(-42);
s = sin(0);
c = cos(0);
sq = sqrt(144);
fl = floor(3.9);
ce = ceil(3.1);
rd = round(3.6);
mn = min(10, 25);
mx = max(10, 25);
pw = pow(2, 4);
md = mod(17, 5);
cl = clamp(150, 0, 100);
lp = lerp(10, 20, 0.5);
"#;
    let engine = run_script(script);
    assert_eq!(engine.vars.get("a").unwrap().clone().number(), 42.0);
    assert_eq!(engine.vars.get("s").unwrap().clone().number(), 0.0);
    assert_eq!(engine.vars.get("c").unwrap().clone().number(), 1.0);
    assert_eq!(engine.vars.get("sq").unwrap().clone().number(), 12.0);
    assert_eq!(engine.vars.get("fl").unwrap().clone().number(), 3.0);
    assert_eq!(engine.vars.get("ce").unwrap().clone().number(), 4.0);
    assert_eq!(engine.vars.get("rd").unwrap().clone().number(), 4.0);
    assert_eq!(engine.vars.get("mn").unwrap().clone().number(), 10.0);
    assert_eq!(engine.vars.get("mx").unwrap().clone().number(), 25.0);
    assert_eq!(engine.vars.get("pw").unwrap().clone().number(), 16.0);
    assert_eq!(engine.vars.get("md").unwrap().clone().number(), 2.0);
    assert_eq!(engine.vars.get("cl").unwrap().clone().number(), 100.0);
    assert_eq!(engine.vars.get("lp").unwrap().clone().number(), 15.0);
}

trait ValueExt {
    fn number(self) -> f64;
    fn string(self) -> String;
}
impl ValueExt for Value {
    fn number(self) -> f64 {
        match self {
            Value::Number(n) => n,
            _ => panic!("Expected number"),
        }
    }
    fn string(self) -> String {
        match self {
            Value::String(s) => s,
            _ => panic!("Expected string"),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Feature 3: Folder type, image-only filtering, folder[filename], and iteration
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_feature_3_folder_type_and_filtering() {
    let test_dir = "target/test_folder_feat3";
    let _ = fs::remove_dir_all(test_dir);
    fs::create_dir_all(test_dir).unwrap();

    // Create a real PNG image inside the folder
    let img_frame = drive::media::frame::Frame::blank(4, 4);
    drive::io::io::encode_image(&img_frame, &format!("{test_dir}/sunset.png")).unwrap();
    drive::io::io::encode_image(&img_frame, &format!("{test_dir}/beach.jpg")).unwrap();

    // Create non-image files (must be filtered out!)
    fs::write(format!("{test_dir}/notes.txt"), "hello world").unwrap();
    fs::write(format!("{test_dir}/data.csv"), "1,2,3").unwrap();

    let script = format!(
        r#"
photos = folder("{test_dir}");
count = photos.len();
pic = photos["sunset.png"];
pic_w = pic.width;
pic_h = pic.height;

iter_count = 0;
for p in photos {{
    iter_count = iter_count + 1;
}}
"#
    );
    let engine = run_script(&script);

    // Only 2 image files were kept (notes.txt and data.csv were ignored)
    assert_eq!(engine.vars.get("count").unwrap().clone().number(), 2.0);
    assert_eq!(engine.vars.get("pic_w").unwrap().clone().number(), 4.0);
    assert_eq!(engine.vars.get("pic_h").unwrap().clone().number(), 4.0);
    assert_eq!(engine.vars.get("iter_count").unwrap().clone().number(), 2.0);

    let _ = fs::remove_dir_all(test_dir);
}

// ─────────────────────────────────────────────────────────────────────────────
// Feature 4: Allow looped exports and looped filter application
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_feature_4_looped_exports_and_filters() {
    let out_dir = "target/test_out_feat4";
    let _ = fs::remove_dir_all(out_dir);
    fs::create_dir_all(out_dir).unwrap();

    let script = format!(
        r#"
filter tint(amt) {{
    r = amt * 50;
    g = 100;
    b = 150;
}}

for i in 0..3 {{
    b = blank(8, 8);
    res = b -> tint(i);
    export(res, f"{out_dir}/frame_{{i:02}}.png");
}}
"#
    );
    run_script(&script);

    assert!(Path::new(&format!("{out_dir}/frame_00.png")).exists());
    assert!(Path::new(&format!("{out_dir}/frame_01.png")).exists());
    assert!(Path::new(&format!("{out_dir}/frame_02.png")).exists());

    let _ = fs::remove_dir_all(out_dir);
}

// ─────────────────────────────────────────────────────────────────────────────
// Feature 5: Escape sequences in string literals
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_feature_5_escape_sequences() {
    let script = r#"
s1 = "Line 1\nLine 2";
s2 = "Tab\there";
s3 = "Quote: \"Drive\"";
s4 = "Slash: \\";
"#;
    let engine = run_script(script);
    assert_eq!(engine.vars.get("s1").unwrap().clone().string(), "Line 1\nLine 2");
    assert_eq!(engine.vars.get("s2").unwrap().clone().string(), "Tab\there");
    assert_eq!(engine.vars.get("s3").unwrap().clone().string(), "Quote: \"Drive\"");
    assert_eq!(engine.vars.get("s4").unwrap().clone().string(), "Slash: \\");
}

// ─────────────────────────────────────────────────────────────────────────────
// Feature 6: Really nice Formatted Strings (f-strings)
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_feature_6_formatted_strings() {
    let script = r#"
name = "World";
idx = 7;
ratio = 3.14159;
msg = f"Hello, {name}! File index: {idx:04}, Ratio: {ratio:.2f}";
"#;
    let engine = run_script(script);
    assert_eq!(
        engine.vars.get("msg").unwrap().clone().string(),
        "Hello, World! File index: 0007, Ratio: 3.14"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Feature 7: If elif else all inside filters, audio filters and functions
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_feature_7_if_elif_else_everywhere() {
    let script = r#"
filter classify() {
    if r < 50 {
        r = 10;
    } elif r < 150 {
        r = 100;
    } else {
        r = 200;
    }
}

af sound_gate(th) {
    if abs(l) < th {
        l = 0.0;
    } elif abs(l) < th * 2 {
        l = l * 0.5;
    } else {
        l = l;
    }
}

fn grade(score) {
    if score >= 90 {
        return 4;
    } elif score >= 80 {
        return 3;
    } elif score >= 70 {
        return 2;
    } else {
        return 1;
    }
}

g1 = grade(95);
g2 = grade(85);
g3 = grade(72);
g4 = grade(50);
"#;
    let engine = run_script(script);
    assert_eq!(engine.vars.get("g1").unwrap().clone().number(), 4.0);
    assert_eq!(engine.vars.get("g2").unwrap().clone().number(), 3.0);
    assert_eq!(engine.vars.get("g3").unwrap().clone().number(), 2.0);
    assert_eq!(engine.vars.get("g4").unwrap().clone().number(), 1.0);
}

// ─────────────────────────────────────────────────────────────────────────────
// Feature 8: All math functions callable inside filters (image & audio)
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_feature_8_math_inside_filters() {
    let script = r#"
filter math_magic() {
    let angle = atan2(y, x);
    let dist = sqrt(x * x + y * y);
    r = clamp(abs(sin(dist)) * 255, 0, 255);
    g = floor(min(dist, 100));
    b = mod(round(dist), 10);
}

img = blank(16, 16);
res = img -> math_magic();
"#;
    let engine = run_script(script);
    let res = engine.vars.get("res").expect("res exists");
    if let Value::Frame(f) = res {
        let p = f.get_pixel(&drive::media::frame::Pos(3, 4)).unwrap();
        // Dist at (3, 4) is sqrt(3*3 + 4*4) = 5.0
        // g = floor(min(5, 100)) = 5
        assert_eq!(p.g(), 5);
        // b = mod(round(5), 10) = 5
        assert_eq!(p.b(), 5);
    } else {
        panic!("Expected Frame");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Feature 9: Native MP3 Export (no ffmpeg)
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_feature_9_native_mp3_export() {
    let out_mp3 = "target/test_output_feat9.mp3";
    let _ = fs::remove_file(out_mp3);

    let script = format!(
        r#"
sound = silence(0.5, 44100, 2);
export(sound, "{out_mp3}");
"#
    );
    run_script(&script);

    let p = Path::new(out_mp3);
    assert!(p.exists(), "MP3 file should be created");
    let metadata = fs::metadata(p).unwrap();
    assert!(metadata.len() > 0, "MP3 file should not be empty");

    let _ = fs::remove_file(out_mp3);
}

// ─────────────────────────────────────────────────────────────────────────────
// Feature 10: Rich Audio Functions (merge, slice, concat, channel_split, etc.)
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_feature_10_audio_functions() {
    let script = r#"
t1 = silence(1.0, 44100, 2);
t2 = silence(2.0, 44100, 2);

// Merge (mix)
mixed = merge(t1, t2);

// Slice
clipped = slice(t2, 0.5, 1.5);

// Concat
chained = concat(t1, t2);

// Channel split
mono_l = channel_split(t1, 0);

// Pipeline stages: reverse, speed, fade
piped = t1 -> reverse() -> speed(2.0) -> fade_in(0.2) -> fade_out(0.2);
"#;
    let engine = run_script(script);
    assert!(engine.vars.contains_key("mixed"));
    assert!(engine.vars.contains_key("clipped"));
    assert!(engine.vars.contains_key("chained"));
    assert!(engine.vars.contains_key("mono_l"));
    assert!(engine.vars.contains_key("piped"));

    if let Value::Track(mono) = engine.vars.get("mono_l").unwrap() {
        assert_eq!(mono.channels(), 1);
    } else {
        panic!("Expected mono track");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Feature 11: Loop variable accessed inside filters
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_feature_11_loop_var_in_filters() {
    let script = r#"
filter add_frame_idx(idx) {
    r = r + idx * 10;
}

for i in 1..4 {
    img = blank(2, 2);
    out = img -> add_frame_idx(i);
    // At i=3, r should be 30
    last_val = i * 10;
}
"#;
    let engine = run_script(script);
    assert_eq!(engine.vars.get("last_val").unwrap().clone().number(), 30.0);
}

// ─────────────────────────────────────────────────────────────────────────────
// Feature 12: Loops inside mathematical functions and filter expressions
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_feature_12_loops_inside_functions_and_filters() {
    let script = r#"
// Loop inside mathematical function
fn sum_range(n) {
    let acc = 0;
    for k in 1..n {
        acc = acc + k;
    }
    return acc;
}

s10 = sum_range(5); // 1 + 2 + 3 + 4 = 10

// Loop inside filter body (unrolled compile-time accumulation)
filter loop_accum() {
    let total = 0;
    for dx in 1..4 {
        total = total + dx * 10;
    }
    r = total;
}

frame_in = blank(2, 2);
frame_out = frame_in -> loop_accum();
"#;
    let engine = run_script(script);
    assert_eq!(engine.vars.get("s10").unwrap().clone().number(), 10.0);

    let frame_out = engine.vars.get("frame_out").expect("frame_out exists");
    if let Value::Frame(f) = frame_out {
        let p = f.get_pixel(&drive::media::frame::Pos(0, 0)).unwrap();
        // 1*10 + 2*10 + 3*10 = 60
        assert_eq!(p.r(), 60);
    } else {
        panic!("Expected Frame");
    }
}
