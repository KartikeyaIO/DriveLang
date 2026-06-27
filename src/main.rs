use drive::engine::Engine;
use drive::parser;
use std::time::Instant;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = std::fs::read_to_string("input2.drive").expect("Failed to read file");

    let program = parser::parse(&source).expect("Parsing Failed!");

    let mut engine = Engine::new();
    let start = Instant::now();

    engine.run(&program).expect("Execution Error!");
    println!("Execution Completed in {:?}",start.elapsed());

    Ok(())
}