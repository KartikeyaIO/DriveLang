use editron::engine::Engine;
use editron::parser;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = std::fs::read_to_string("input.edt").expect("Failed to read file");

    let program = parser::parse(&source).expect("Parsing Failed!");

    let mut engine = Engine::new();

    engine.run(&program).expect("Execution Error!");
    println!("Execution was successfull!!!");

    Ok(())
}
