use crate::lexer::{Token, TokenKind};


// ─────────────────────────────────────────────────────────────────────────
// AST
// ─────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Import {
    Std(Vec<String>),
    File { path: String, alias: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Ident(String),
    Int(i64),
    Float(f64),
    Str(String),
    Neg(Box<Expr>),
    BinOp {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Call {
        path: Vec<String>,
        args: Vec<Expr>,
    },
    Range {
        start: Box<Expr>,
        end: Box<Expr>,
        step: Option<Box<Expr>>,
    },
    Pipe {
        base: Box<Expr>,
        stages: Vec<PipeStage>,
    },
    // NEW: Array literals for Kernel matrices like `[[1, 2, 1], [2, 4, 2], [1, 2, 1]]`
    Array(Vec<Expr>),
    Not(Box<Expr>),
    
    
}

#[derive(Debug, Clone, PartialEq)]
pub struct PipeStage {
    pub path: Vec<String>,
    pub args: Vec<Expr>,
    pub mask: Option<(Expr, Expr)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Channel {
    R,
    G,
    B,
    A,
    T,
    L,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChannelAssign {
    pub channel: Channel,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Channel(ChannelAssign),
    Let { name: String, value: Expr },
    

    IfElse {
        cond: Box<Expr>,
        true_branch: Vec<Statement>,
        false_branch: Vec<Statement>,
    },
    
    

}

#[derive(Debug, Clone, PartialEq)]
pub struct FilterDecl {
    pub name: String,
    pub params: Vec<String>,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioFilterDecl {
    pub name: String,
    pub params: Vec<String>,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EffectDecl{
    pub name: String,
    pub params: Vec<String>,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Import(Import),
    Print { args: Vec<Expr> },
    Assign { name: String, value: Expr },
    //EffectDecl(EffectDecl),
    FilterDecl(FilterDecl),
    AudioFilterDecl(AudioFilterDecl),
    KernelDecl { name: String, matrix: Expr },
    Export { value: Expr, path: Expr },
    ForLoop {
        variable: String,
        range: Box<Expr>,
        items : Vec<Item>,
    },
    IfElse {
        cond: Box<Expr>,
        true_branch: Vec<Item>,
        false_branch: Vec<Item>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub items: Vec<Item>,
}



#[derive(Debug, Clone, PartialEq)]
pub enum ParseError {
    UnexpectedToken {
        expected: String,
        found: TokenKind,
        line: usize,
    },
    UnexpectedEof {
        expected: String,
    },
    InvalidNumber {
        value: String,
        line: usize,
    },
    InvalidChannel {
        name: String,
        line: usize,
    },
    TimeError{
        message: String,
        line: usize,
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::UnexpectedToken {
                expected,
                found,
                line,
            } => write!(f, "line {line}: expected {expected}, found {found:?}"),
            ParseError::UnexpectedEof { expected } => {
                write!(f, "unexpected end of input, expected {expected}")
            }
            ParseError::InvalidNumber { value, line } => {
                write!(f, "line {line}: invalid number literal '{value}'")
            }
            ParseError::InvalidChannel { name, line } => {
                write!(
                    f,
                    "line {line}: '{name}' is not a valid channel (expected r, g, b, or a)"
                )
            }
            ParseError::TimeError { message, line } => {
                write!(f,"line {line}: {message}")
            }
        }
    }
}

impl std::error::Error for ParseError {}

// ─────────────────────────────────────────────────────────────────────────
// Parser
// ─────────────────────────────────────────────────────────────────────────

pub struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

type PResult<T> = Result<T, ParseError>;

impl<'a> Parser<'a> {
    pub fn new(tokens: &'a [Token]) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.peek().kind
    }

    fn advance(&mut self) -> Token {
        let tok = self.peek().clone();
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        tok
    }

    fn check(&self, kind: &TokenKind) -> bool {
        self.peek_kind() == kind
    }

    fn expect(&mut self, kind: TokenKind, expected: &str) -> PResult<Token> {
        if self.check(&kind) {
            Ok(self.advance())
        } else if matches!(self.peek_kind(), TokenKind::EOF) {
            Err(ParseError::UnexpectedEof {
                expected: expected.to_string(),
            })
        } else {
            Err(ParseError::UnexpectedToken {
                expected: expected.to_string(),
                found: self.peek_kind().clone(),
                line: self.peek().line,
            })
        }
    }

    fn expect_identifier(&mut self, expected: &str) -> PResult<String> {
        match self.peek_kind() {
            TokenKind::Identifier => Ok(self.advance().value),
            TokenKind::EOF => Err(ParseError::UnexpectedEof {
                expected: expected.to_string(),
            }),
            other => Err(ParseError::UnexpectedToken {
                expected: expected.to_string(),
                found: other.clone(),
                line: self.peek().line,
            }),
        }
    }

    pub fn parse_program(&mut self) -> PResult<Program> {
        let mut items = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::EOF) {
            items.push(self.parse_item()?);
        }
        Ok(Program { items })
    }

    fn parse_item(&mut self) -> PResult<Item> {
        match self.peek_kind() {
            TokenKind::Import => self.parse_import(),
            TokenKind::Filter => self.parse_filter_decl(),
            TokenKind::AudioFilter => self.parse_audiofilter_decl(),
            TokenKind::Kernel => self.parse_kernel_decl(),
            TokenKind::Export => self.parse_export(),
            TokenKind::Identifier => self.parse_assignment(),
            TokenKind::Print => self.parse_print(),
            TokenKind::For => {
                self.advance();
                let variable = self.expect_identifier("loop variable name")?;
                self.expect(TokenKind::In, "in")?;
                let range = Box::new(self.parse_expr()?);
                self.expect(TokenKind::LeftBrace, "{")?;
                
                let mut items = Vec::new();
                while !self.check(&TokenKind::RightBrace) {
                    items.push(self.parse_item()?);
                }
                self.expect(TokenKind::RightBrace, "}")?;
                
                Ok(Item::ForLoop { variable, range, items })
            }
            TokenKind::If => {
                // (Very similar to statement if/else, just for Items!)
                self.advance();
                let cond = Box::new(self.parse_expr()?);
                self.expect(TokenKind::LeftBrace, "{")?;
                let mut true_branch = Vec::new();
                while !self.check(&TokenKind::RightBrace) {
                    true_branch.push(self.parse_item()?);
                }
                self.expect(TokenKind::RightBrace, "}")?;

                let mut false_branch = Vec::new();
                if self.check(&TokenKind::Else) {
                    self.advance();
                    self.expect(TokenKind::LeftBrace, "{")?;
                    while !self.check(&TokenKind::RightBrace) {
                        false_branch.push(self.parse_item()?);
                    }
                    self.expect(TokenKind::RightBrace, "}")?;
                }
                Ok(Item::IfElse { cond, true_branch, false_branch })
            }
            other => Err(ParseError::UnexpectedToken {
                expected: "import, filter, kernel, export, or assignment".to_string(),
                found: other.clone(),
                line: self.peek().line,
            }),
        }
    }
    fn parse_print(&mut self) -> PResult<Item> {
        self.advance();
        self.expect(TokenKind::LeftParen, "'(' after 'print'")?;
        let args = self.parse_arg_list()?;
        self.expect(TokenKind::RightParen, "')'")?;
        self.expect(TokenKind::SemiColon, "';' after ')'");
        Ok(Item::Print { args })
    }

    fn parse_import(&mut self) -> PResult<Item> {
        self.expect(TokenKind::Import, "'import'")?;

        let import = match self.peek_kind() {
            TokenKind::String => {
                let path = self.advance().value;
                let alias = "file".to_string();
                Import::File { path, alias }
            }
            TokenKind::Identifier => {
                let mut segments = vec![self.advance().value];
                while self.check(&TokenKind::DoubleColon) {
                    self.advance();
                    segments.push(self.expect_identifier("a path segment")?);
                }
                Import::Std(segments)
            }
            other => {
                return Err(ParseError::UnexpectedToken {
                    expected: "a module path or a quoted file path".to_string(),
                    found: other.clone(),
                    line: self.peek().line,
                });
            }
        };

        self.expect(TokenKind::SemiColon, "';'")?;
        Ok(Item::Import(import))
    }

    fn parse_filter_decl(&mut self) -> PResult<Item> {
        self.expect(TokenKind::Filter, "'filter'")?;
        let name = self.expect_identifier("a filter name")?;

        self.expect(TokenKind::LeftParen, "'('")?;
        let mut params = Vec::new();
        if !self.check(&TokenKind::RightParen) {
            loop {
                params.push(self.expect_identifier("a parameter name")?);
                if self.check(&TokenKind::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
        }
        self.expect(TokenKind::RightParen, "')'")?;

        self.expect(TokenKind::LeftBrace, "'{'")?;
        let mut body = Vec::new();
        while !self.check(&TokenKind::RightBrace) {
           body.push(self.parse_statement("filter")?);
        }
        self.expect(TokenKind::RightBrace, "'}'")?;

        Ok(Item::FilterDecl(FilterDecl { name, params, body }))
    }


    fn parse_audiofilter_decl(&mut self) -> PResult<Item> {
        self.expect(TokenKind::AudioFilter, "'af'")?;
        let name = self.expect_identifier("an AudioFilter name")?;

        self.expect(TokenKind::LeftParen, "'('")?;
        let mut params = Vec::new();
        if !self.check(&TokenKind::RightParen) {
            loop {
                params.push(self.expect_identifier("a parameter name")?);
                if self.check(&TokenKind::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
        }
        self.expect(TokenKind::RightParen, "')'")?;

        self.expect(TokenKind::LeftBrace, "'{'")?;
        let mut body = Vec::new();
        while !self.check(&TokenKind::RightBrace) {
           body.push(self.parse_statement("af")?);
        }
        self.expect(TokenKind::RightBrace, "'}'")?;

        Ok(Item::AudioFilterDecl(AudioFilterDecl { name, params, body }))
    }


    // Effect Declarations...
    // fn parse_effect_decl(&mut self) -> PResult<Item> {
    //     self.expect(TokenKind::Effect, "'effect'")?;
    //     let name = self.expect_identifier("an effect name")?;

    //     self.expect(TokenKind::LeftParen, "'('")?;
    //     let mut params = Vec::new();
    //     if !self.check(&TokenKind::RightParen) {
    //         loop {
    //             params.push(self.expect_identifier("a parameter name")?);
    //             if self.check(&TokenKind::Comma) {
    //                 self.advance();
    //             } else {
    //                 break;
    //             }
    //         }
    //     }

    //     self.expect(TokenKind::RightParen, "')'")?;

    //     self.expect(TokenKind::LeftBrace, "'{'")?;
    //     let mut body = Vec::new();
    //     while !self.check(&TokenKind::RightBrace) {
    //         body.push(self.parse_statement("effect")?);
    //     }
    //     Ok(Item::EffectDecl(EffectDecl { name, params, body }))
    // }

    // NEW: Parse Kernel block -> kernel blur = [[1, 2, 1], [2, 4, 2], [1, 2, 1]];
    fn parse_kernel_decl(&mut self) -> PResult<Item> {
        self.expect(TokenKind::Kernel, "'kernel'")?;
        let name = self.expect_identifier("a kernel name")?;
        self.expect(TokenKind::Equal, "'='")?;
        let matrix = self.parse_expr()?;
        self.expect(TokenKind::SemiColon, "';'")?;

        Ok(Item::KernelDecl { name, matrix })
    }
    
        


    

    fn parse_channel_assign(&mut self, caller: &str) -> PResult<ChannelAssign> {
        let tok = self.peek().clone();
        let name = self.expect_identifier("a channel name")?;
        
        // 🚨 STRICT CHANNEL BOUNCER 🚨
        let channel = match (caller, name.as_str()) {
            ("af", "l") => Channel::L,
            ("af", "r") => Channel::R, // 'r' plays double-agent: Right channel
            ("af", illegal) => {
                return Err(ParseError::InvalidChannel {
                    name: format!("'{illegal}' is not allowed inside audio filters..."),
                    line: tok.line,
                });
            }
            (_, "r") => Channel::R,
            (_, "g") => Channel::G,
            (_, "b") => Channel::B,
            (_, "a") => Channel::A,
            ("effect", "t") => Channel::T,
            ("filter", "t") => {
                return Err(ParseError::TimeError {
                    message: "Time-based operations ('t') are not allowed in 'filter'. Try using 'effect'.".to_string(),
                    line: tok.line,
                });
            }
            (_, invalid) => {
                return Err(ParseError::InvalidChannel {
                    name: invalid.to_string(),
                    line: tok.line,
                });
            }
        };

        self.expect(TokenKind::Equal, "'='")?;
        let value = self.parse_expr()?;
        self.expect(TokenKind::SemiColon, "';'")?;

        Ok(ChannelAssign { channel, value })
    }

    fn parse_export(&mut self) -> PResult<Item> {
        self.expect(TokenKind::Export, "'export'")?;
        self.expect(TokenKind::LeftParen, "'('")?;
        let value = self.parse_expr()?;
        self.expect(TokenKind::Comma, "','")?;
        let path = self.parse_expr()?;
        self.expect(TokenKind::RightParen, "')'")?;
        self.expect(TokenKind::SemiColon, "';'")?;
        Ok(Item::Export { value, path })
    }

    fn parse_assignment(&mut self) -> PResult<Item> {
        let name = self.expect_identifier("an identifier")?;
        self.expect(TokenKind::Equal, "'='")?;
        let value = self.parse_expr()?;
        self.expect(TokenKind::SemiColon, "';'")?;
        Ok(Item::Assign { name, value })
    }
    pub fn parse_statement(&mut self, caller: &str) -> PResult<Statement> {
        match self.peek_kind() {
            TokenKind::Let => {
                self.advance();
                let name = self.expect_identifier("Variable name expected")?;
                self.expect(TokenKind::Equal, "=")?;
                let value = self.parse_expr()?;
                self.expect(TokenKind::SemiColon, ";")?;
                Ok(Statement::Let { name, value })
            }
            TokenKind::If => self.parse_statement_if_else(caller),
            _ => {
                let assign = self.parse_channel_assign(caller)?;
                Ok(Statement::Channel(assign))
            }
        }
    }
    fn parse_statement_if_else(&mut self, caller: &str) -> PResult<Statement> {
        self.advance(); // consume 'if' or 'elif'
        let cond = Box::new(self.parse_expr()?);
        
        self.expect(TokenKind::LeftBrace, "{")?;
        let mut true_branch = Vec::new();
        while !self.check(&TokenKind::RightBrace) {
            true_branch.push(self.parse_statement(caller)?);
        }
        self.expect(TokenKind::RightBrace, "}")?;

        let mut false_branch = Vec::new();
        if self.check(&TokenKind::Else) {
            self.advance(); // consume else
            self.expect(TokenKind::LeftBrace, "{")?;
            while !self.check(&TokenKind::RightBrace) {
                false_branch.push(self.parse_statement(caller)?);
            }
            self.expect(TokenKind::RightBrace, "}")?;
        } else if self.check(&TokenKind::Elif) {
            // MAGIC: Recursively parse elif as the false_branch!
            false_branch.push(self.parse_statement_if_else(caller)?);
        }

        Ok(Statement::IfElse { cond, true_branch, false_branch })
    }

    pub fn parse_expr(&mut self) -> PResult<Expr> {
        self.parse_pipe()
    }

    fn parse_pipe(&mut self) -> PResult<Expr> {
        let base = self.parse_logical()?;

        if !self.check(&TokenKind::Arrow) {
            return Ok(base);
        }

        let mut stages = Vec::new();
        while self.check(&TokenKind::Arrow) {
            self.advance();
            stages.push(self.parse_pipe_stage()?);
        }

        Ok(Expr::Pipe {
            base: Box::new(base),
            stages,
        })
    }

    fn parse_pipe_stage(&mut self) -> PResult<PipeStage> {
        let path = self.parse_path()?;

        self.expect(TokenKind::LeftParen, "'(' after filter name")?;
        let args = self.parse_arg_list()?;
        self.expect(TokenKind::RightParen, "')'")?;

        let mask = if self.check(&TokenKind::LeftBracket) {
            self.advance();
            let x_range = self.parse_range()?;
            self.expect(TokenKind::Comma, "',' between x and y ranges in mask")?;
            let y_range = self.parse_range()?;
            self.expect(TokenKind::RightBracket, "']'")?;
            Some((x_range, y_range))
        } else {
            None
        };

        Ok(PipeStage { path, args, mask })
    }

    fn parse_range(&mut self) -> PResult<Expr> {
        let start = self.parse_additive()?;

        if !self.check(&TokenKind::DotDot) {
            return Ok(start);
        }
        self.advance();

        let end = self.parse_additive()?;

        let step = if self.check(&TokenKind::DotDot) {
            self.advance();
            Some(Box::new(self.parse_additive()?))
        } else {
            None
        };

        Ok(Expr::Range {
            start: Box::new(start),
            end: Box::new(end),
            step,
        })
    }

    fn parse_additive(&mut self) -> PResult<Expr> {
        let mut lhs = self.parse_multiplicative()?;

        loop {
            let op = match self.peek_kind() {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_multiplicative()?;
            lhs = Expr::BinOp {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }

        Ok(lhs)
    }

    fn parse_multiplicative(&mut self) -> PResult<Expr> {
        let mut lhs = self.parse_unary()?;

        loop {
            let op = match self.peek_kind() {
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_unary()?;
            lhs = Expr::BinOp {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }

        Ok(lhs)
    }
    fn parse_logical(&mut self) -> PResult<Expr> {
        let mut lhs = self.parse_comparison()?;

        loop {
            let op = match self.peek_kind() {
                TokenKind::And => BinOp::And,
                TokenKind::Or => BinOp::Or,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_comparison()?;
            lhs = Expr::BinOp {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }

        Ok(lhs)
    }

    fn parse_comparison(&mut self) -> PResult<Expr> {
        let mut lhs = self.parse_range()?;

        loop {
            let op = match self.peek_kind() {
                TokenKind::EqualEqual => BinOp::Eq,
                TokenKind::NotEqual => BinOp::Ne,
                TokenKind::GreaterThan => BinOp::Gt,
                TokenKind::GreaterEqual => BinOp::Ge,
                TokenKind::LessThan => BinOp::Lt,
                TokenKind::LessEqual => BinOp::Le,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_range()?;
            lhs = Expr::BinOp {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }

        Ok(lhs)
    }

    fn parse_unary(&mut self) -> PResult<Expr> {
        if self.check(&TokenKind::Not) {
            self.advance();
            let expr = self.parse_unary()?;
            return Ok(Expr::Not(Box::new(expr)));
        }
        if self.check(&TokenKind::Minus) {
            self.advance();
            let expr = self.parse_unary()?;
            return Ok(Expr::Neg(Box::new(expr)));
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> PResult<Expr> {
        let tok = self.peek().clone();

        match &tok.kind {
            TokenKind::Int => {
                self.advance();
                tok.value
                    .parse::<i64>()
                    .map(Expr::Int)
                    .map_err(|_| ParseError::InvalidNumber {
                        value: tok.value,
                        line: tok.line,
                    })
            }
            TokenKind::Float => {
                self.advance();
                tok.value
                    .parse::<f64>()
                    .map(Expr::Float)
                    .map_err(|_| ParseError::InvalidNumber {
                        value: tok.value,
                        line: tok.line,
                    })
            }
            TokenKind::String => {
                self.advance();
                Ok(Expr::Str(tok.value))
            }
            TokenKind::LeftParen => {
                self.advance();
                let inner = self.parse_expr()?;
                self.expect(TokenKind::RightParen, "')'")?;
                Ok(inner)
            }
            TokenKind::LeftBracket => {
                // NEW: Parse array literals (for kernel matrices)
                self.advance();
                let mut elements = Vec::new();
                if !self.check(&TokenKind::RightBracket) {
                    loop {
                        elements.push(self.parse_expr()?);
                        if self.check(&TokenKind::Comma) {
                            self.advance();
                        } else {
                            break;
                        }
                    }
                }
                self.expect(TokenKind::RightBracket, "']'")?;
                Ok(Expr::Array(elements))
            }
            TokenKind::LoadFrame => {
                self.advance();
                self.expect(TokenKind::LeftParen, "'(' after 'frame'")?;
                let args = self.parse_arg_list()?;
                self.expect(TokenKind::RightParen, "')'")?;
                Ok(Expr::Call {
                    path: vec!["frame".to_string()],
                    args,
                })
            }
            TokenKind::LoadTrack => {
                self.advance();
                self.expect(TokenKind::LeftParen, "'(' after 'track'")?;
                let args = self.parse_arg_list()?;
                self.expect(TokenKind::RightParen, "')'")?;
                Ok(Expr::Call {
                    path: vec!["track".to_string()],
                    args,
                })
            }

            TokenKind::Blank => {
                self.advance();

                self.expect(TokenKind::LeftParen, "'(' after 'blank'")?;

                let args = self.parse_arg_list()?;

                self.expect(TokenKind::RightParen, "')'")?;

                Ok(Expr::Call {
                    path: vec!["blank".to_string()],
                    args,
                })
            }
            TokenKind::Silence => {
                self.advance();

                self.expect(TokenKind::LeftParen, "'(' after 'silence'")?;

                let args = self.parse_arg_list()?;

                self.expect(TokenKind::RightParen, "')'")?;

                Ok(Expr::Call {
                    path: vec!["silence".to_string()],
                    args,
                })
            }
            TokenKind::Identifier => {
                let path = self.parse_path()?;

                if self.check(&TokenKind::LeftParen) {
                    self.advance();
                    let args = self.parse_arg_list()?;
                    self.expect(TokenKind::RightParen, "')'")?;
                    Ok(Expr::Call { path, args })
                } else if path.len() == 1 {
                    Ok(Expr::Ident(path.into_iter().next().unwrap()))
                } else {
                    Ok(Expr::Call { path, args: vec![] })
                }
            }
            TokenKind::EOF => Err(ParseError::UnexpectedEof {
                expected: "an expression".to_string(),
            }),
            other => Err(ParseError::UnexpectedToken {
                expected: "an expression".to_string(),
                found: other.clone(),
                line: tok.line,
            }),
        }
    }

    fn parse_path(&mut self) -> PResult<Vec<String>> {
        let mut segments = vec![self.expect_identifier("an identifier")?];
        while self.check(&TokenKind::DoubleColon) {
            self.advance();
            segments.push(self.expect_identifier("a path segment")?);
        }
        Ok(segments)
    }

    fn parse_arg_list(&mut self) -> PResult<Vec<Expr>> {
        let mut args = Vec::new();
        if self.check(&TokenKind::RightParen) {
            return Ok(args);
        }
        loop {
            args.push(self.parse_expr()?);
            if self.check(&TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        Ok(args)
    }
}

pub fn parse(source: &str) -> Result<Program, ParseOrLexError> {
    let tokens = crate::lexer::lexer(source).map_err(ParseOrLexError::Lex)?;
    let mut parser = Parser::new(&tokens);
    parser.parse_program().map_err(ParseOrLexError::Parse)
}

#[derive(Debug)]
pub enum ParseOrLexError {
    Lex(crate::lexer::LexError),
    Parse(ParseError),
}


impl std::fmt::Display for ParseOrLexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseOrLexError::Lex(e) => write!(f, "lex error: {e:?}"),
            ParseOrLexError::Parse(e) => write!(f, "parse error: {e}"),
        }
    }
}
