use crate::media::frame::Color;

#[derive(Debug, Clone)]
pub struct Filter {
    pub name: String,

    // Parameter names in declaration order
    pub params: Vec<String>,

    // Compiled programs
    pub r_program: Vec<Instruction>,
    pub g_program: Vec<Instruction>,
    pub b_program: Vec<Instruction>,
    pub a_program: Vec<Instruction>,
}

pub struct Effect {
    pub name: String,

    // Parameter names in declaration order
    pub params: Vec<String>,

    // Compiled programs
    pub r_program: Vec<Instruction>,
    pub g_program: Vec<Instruction>,
    pub b_program: Vec<Instruction>,
    pub a_program: Vec<Instruction>,
    pub t_program : Vec<Instruction>,
}
#[derive(Debug, Clone, PartialEq)]
pub enum Instruction {
    // ─────────────────────────────
    // Channel Access
    // ─────────────────────────────
    LoadR,
    LoadG,
    LoadB,
    LoadA,
    LoadT,

    // ─────────────────────────────
    // Filter Parameters
    // brightness(amount)
    // amount -> LoadParam(0)
    // ─────────────────────────────
    LoadParam(usize),

    // ─────────────────────────────
    // Frame Metadata
    // ─────────────────────────────
    LoadX,
    LoadY,
    LoadWidth,
    LoadHeight,

    // ─────────────────────────────
    // Constants
    // ─────────────────────────────
    PushInt(i64),
    PushFloat(f32),

    // ─────────────────────────────
    // Arithmetic
    // ─────────────────────────────
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,

    // ─────────────────────────────
    // Unary
    // ─────────────────────────────
    Neg,

    // ─────────────────────────────
    // Comparison
    // Result: 1.0 or 0.0
    // ─────────────────────────────
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,

    // ─────────────────────────────
    // Logic
    // ─────────────────────────────
    And,
    Or,
    Not,

    // ─────────────────────────────
    // Math
    // ─────────────────────────────
    Abs,
    Min,
    Max,
    Clamp,
    Lerp,
    SmoothLerp,

    Sin,
    Cos,
    Tan,

    Asin,
    Acos,
    Atan,

    Sqrt,
    Exp,
    Log,
    Log10,
    
    

    Floor,
    Jump(usize),
    JumpIfFalse(usize),
    Ceil,
    Round,
    StoreLocal(usize),
    LoadLocal(usize),
}

#[derive(Debug, Clone, Copy)]
pub struct PixelContext {
    pub color: Color,

    pub x: u32,
    pub y: u32,

    pub width: u32,
    pub height: u32,
}

pub struct FilterVM {
    stack: Vec<f32>,
    locals: Vec<f32>,
}

impl FilterVM {
    pub fn new() -> Self {
        Self {
            stack: Vec::with_capacity(64),
            locals: vec![0.0; 16],
        }
    }

    fn pop(&mut self) -> f32 {
        self.stack.pop().unwrap_or(0.0)
    }

    fn push(&mut self, value: f32) {
        self.stack.push(value);
    }
    fn run_program(&mut self, program: &[Instruction], ctx: &PixelContext, params: &[f32]) {
        let mut ip = 0;
        while ip< program.len() {
            let instruction = &program[ip];
            match instruction {
                Instruction::Jump(target) => {
                    ip = *target;
                    continue; // Skip the standard ip += 1
                }
                Instruction::JumpIfFalse(target) => {
                    let val = self.pop();
                    if val == 0.0 { // Assuming 0.0 is false
                        ip = *target;
                        continue;
                    }
                }
                
                Instruction::LoadR => {
                    self.push(ctx.color.r() as f32);
                }

                Instruction::LoadG => {
                    self.push(ctx.color.g() as f32);
                }

                Instruction::LoadB => {
                    self.push(ctx.color.b() as f32);
                }

                Instruction::LoadA => {
                    let alpha = match ctx.color {
                        Color::RGBA(_, _, _, a) => a,
                        _ => 255,
                    };

                    self.push(alpha as f32);
                }
                Instruction::LoadT => {}

                
                Instruction::LoadParam(index) => {
                    self.push(params.get(*index).copied().unwrap_or(0.0));
                }

                
                Instruction::LoadX => {
                    self.push(ctx.x as f32);
                }

                Instruction::LoadY => {
                    self.push(ctx.y as f32);
                }

                Instruction::LoadWidth => {
                    self.push(ctx.width as f32);
                }

                Instruction::LoadHeight => {
                    self.push(ctx.height as f32);
                }

                
                Instruction::PushInt(v) => {
                    self.push(*v as f32);
                }

                Instruction::PushFloat(v) => {
                    self.push(*v);
                }

                
                Instruction::Add => {
                    let b = self.pop();
                    let a = self.pop();
                    self.push(a + b);
                }

                Instruction::Sub => {
                    let b = self.pop();
                    let a = self.pop();
                    self.push(a - b);
                }

                Instruction::Mul => {
                    let b = self.pop();
                    let a = self.pop();
                    self.push(a * b);
                }

                Instruction::Div => {
                    let b = self.pop();

                    if b == 0.0 {
                        self.push(0.0);
                    } else {
                        let a = self.pop();
                        self.push(a / b);
                    }
                }

                Instruction::Mod => {
                    let b = self.pop();

                    if b == 0.0 {
                        self.push(0.0);
                    } else {
                        let a = self.pop();
                        self.push(a % b);
                    }
                }

                Instruction::Pow => {
                    let b = self.pop();
                    let a = self.pop();

                    self.push(a.powf(b));
                }

               
                Instruction::Neg => {
                    let a = self.pop();
                    self.push(-a);
                }

                Instruction::Abs => {
                    let a = self.pop();
                    self.push(a.abs());
                }

                
                Instruction::Eq => {
                    let b = self.pop();
                    let a = self.pop();
                    self.push((a == b) as u8 as f32);
                }

                Instruction::Ne => {
                    let b = self.pop();
                    let a = self.pop();
                    self.push((a != b) as u8 as f32);
                }

                Instruction::Gt => {
                    let b = self.pop();
                    let a = self.pop();
                    self.push((a > b) as u8 as f32);
                }

                Instruction::Ge => {
                    let b = self.pop();
                    let a = self.pop();
                    self.push((a >= b) as u8 as f32);
                }

                Instruction::Lt => {
                    let b = self.pop();
                    let a = self.pop();
                    self.push((a < b) as u8 as f32);
                }

                Instruction::Le => {
                    let b = self.pop();
                    let a = self.pop();
                    self.push((a <= b) as u8 as f32);
                }

                
                Instruction::And => {
                    let b = self.pop();
                    let a = self.pop();

                    self.push(((a != 0.0) && (b != 0.0)) as u8 as f32);
                }

                Instruction::Or => {
                    let b = self.pop();
                    let a = self.pop();

                    self.push(((a != 0.0) || (b != 0.0)) as u8 as f32);
                }

                Instruction::Not => {
                    let a = self.pop();

                    self.push((a == 0.0) as u8 as f32);
                }

                
                Instruction::Min => {
                    let b = self.pop();
                    let a = self.pop();

                    self.push(a.min(b));
                }

                Instruction::Max => {
                    let b = self.pop();
                    let a = self.pop();

                    self.push(a.max(b));
                }

                Instruction::Clamp => {
                    let max = self.pop();
                    let min = self.pop();
                    let value = self.pop();

                    self.push(value.clamp(min, max));
                }
                Instruction::Lerp => {
                     let t = self.pop();
                     let b = self.pop();
                     let a = self.pop();

    
                     self.push(a + t * (b - a));
                    }

                    Instruction::SmoothLerp => {
                        let t_raw = self.pop();
                        let b = self.pop();
                        let a = self.pop();

                       
                        let t = t_raw.clamp(0.0, 1.0);

                        
                        let smooth_t = t * t * (3.0 - 2.0 * t);

                        
                        self.push(a + smooth_t * (b - a));
                    }

                
                Instruction::Sin => {
                    let x = { self.pop().sin() };
                    self.push(x);
                }

                Instruction::Cos => {
                    let x = { self.pop().cos() };
                    self.push(x);
                }

                Instruction::Tan => {
                    let x = { self.pop().tan() };
                    self.push(x);
                }

                Instruction::Asin => {
                    let x = { self.pop().asin() };
                    self.push(x);
                }

                Instruction::Acos => {
                    let x = { self.pop().acos() };
                    self.push(x);
                }

                Instruction::Atan => {
                    let x = { self.pop().atan() };
                    self.push(x);
                }

                // -------------------------
                // EXPONENTIAL
                // -------------------------
                Instruction::Sqrt => {
                    let x = { self.pop().sqrt() };
                    self.push(x);
                }

                Instruction::Exp => {
                    let x = { self.pop().exp() };
                    self.push(x);
                }

                Instruction::Log => {
                    let x = { self.pop().ln() };
                    self.push(x);
                }

                Instruction::Log10 => {
                    let x = { self.pop().log10() };
                    self.push(x);
                }

                
                Instruction::Floor => {
                    let x = { self.pop().floor() };
                    self.push(x);
                }

                Instruction::Ceil => {
                    let x = { self.pop().ceil() };
                    self.push(x);
                }

                Instruction::Round => {
                    let x = { self.pop().round() };
                    self.push(x);
                }
                Instruction::StoreLocal(index) => {
                    let val = self.pop();
                    if *index >= self.locals.len() {
                        self.locals.resize(*index + 1, 0.0);
                    }
                    self.locals[*index] = val;
                }
                Instruction::LoadLocal(index) => {
                    let val = self.locals.get(*index).copied().unwrap_or(0.0);
                    self.push(val);
                }
            }
            ip +=1;
        }
    }

    pub fn execute(&mut self, program: &[Instruction], ctx: &PixelContext, params: &[f32]) -> f32 {
        self.stack.clear();
        self.run_program(program, ctx, params);

        

        self.pop()
    }
}

impl Filter {
    pub fn apply(
        &self,
        color: Color,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        params: &[f32],
        vm: &mut FilterVM,
    ) -> Color {
        let ctx = PixelContext {
            color,
            x,
            y,
            width,
            height,
        };

        let r = vm.execute(&self.r_program, &ctx, params).clamp(0.0, 255.0) as u8;

        let g = vm.execute(&self.g_program, &ctx, params).clamp(0.0, 255.0) as u8;

        let b = vm.execute(&self.b_program, &ctx, params).clamp(0.0, 255.0) as u8;

        match color {
            Color::RGB(_, _, _) => Color::RGB(r, g, b).to_rgba(),

            Color::RGBA(_, _, _, _) => {
                let a = vm.execute(&self.a_program, &ctx, params).clamp(0.0, 255.0) as u8;

                Color::RGBA(r, g, b, a)
            }

            Color::Gray(_) => Color::Gray(r).to_rgba(),
        }
    }
}
