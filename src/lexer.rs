#[derive(Debug, Clone)]
pub struct Token {
    /// The Token Struct has Three values
    /// kind which decides token kind
    /// value which stores values
    /// line which stores the line numbers
    pub kind: TokenKind,
    pub value: String,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    /// TokenKind enum stores the kind of tokens implimented in Drive
    Identifier,
    Let,
    Import,
    As,
    Int,
    Float,
    String,
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    SemiColon,
    Equal,
    EqualEqual,
    NotEqual,
    GreaterEqual,
    In,
    AudioFilter,
    LessEqual,
    
    And,
    Or,
    Not,
    For,
    LoadTrack,
    LoadFrame,
    Filter,
    Export,
    LeftBracket,
    RightBracket,
    Print,
    Dot,
    Comma,
    DotDot,
    DoubleColon,
    Arrow,
    Plus,
    Minus,
    Star,
    Slash,
    LessThan,
    If,
    Else,
    Elif,

    GreaterThan,

    Blank,
    Silence,
    Kernel,
    EOF,
}

#[derive(Debug, Clone)]
/// State Enum stores the state of the the data as the control moves through each character
pub enum State {
    Default,
    Identifier,
    String,
    Number,
    Comment,
}
#[derive(Debug, Clone)]
pub enum LexError {
    /// LexError generates the the Errors at lexer level
    InvalidCharacter {
        ch: char,
        line: usize,
        message: String,
    }, // The InvalidCharacter Error tells the user the character is invalid

    UnterminatedString {
        line: usize,
        message: String,
    }, // The UnterminatedString Error tells the user the string was not terminated

    InvalidNumber {
        value: String,
        line: usize,
        message: String,
    }, // The Invalid Number Error Tells the user the number is invalid
}

pub fn char_to_token(c: char) -> Option<TokenKind> {
    match c {
        '(' => Some(TokenKind::LeftParen),
        ')' => Some(TokenKind::RightParen),
        '{' => Some(TokenKind::LeftBrace),
        '}' => Some(TokenKind::RightBrace),

        ';' => Some(TokenKind::SemiColon),
        '[' => Some(TokenKind::LeftBracket),
        ']' => Some(TokenKind::RightBracket),
        ',' => Some(TokenKind::Comma),
        '+' => Some(TokenKind::Plus),
        '-' => Some(TokenKind::Minus),
        '*' => Some(TokenKind::Star),

        _ => None,
    }
}

fn identify_token(s: &str) -> TokenKind {
    match s {
        "frame" => TokenKind::LoadFrame,
        "track" => TokenKind::LoadTrack,
        "filter" => TokenKind::Filter,
        "export" => TokenKind::Export,
        "import" => TokenKind::Import,
        "as" => TokenKind::As,
        "kernel" => TokenKind::Kernel,
        "for" => TokenKind::For,
        "in" => TokenKind::In,
        "af" => TokenKind::AudioFilter,
        "if" => TokenKind::If,
        "else" => TokenKind::Else,
        "elif" => TokenKind::Elif,
        "blank" => TokenKind::Blank,
        "silence" => TokenKind::Silence,
        "and" => TokenKind::And,
        "or" => TokenKind::Or,
        "not" => TokenKind::Not,
        "let" => TokenKind::Let,
        "print" => TokenKind::Print,
       // "effect" => TokenKind::Effect,
        _ => TokenKind::Identifier,
    }
}

pub fn lexer(source: &str) -> Result<Vec<Token>, LexError> {
    let bytes = source.as_bytes();
    let len = bytes.len();

    let mut tokens = Vec::new();
    let mut line = 1;
    let mut i = 0;

    let mut state = State::Default;
    let mut buffer = String::new();

    while i < len {
        let c = bytes[i] as char;

        match state {
            State::Default => match c {
                '\n' => {
                    line += 1;
                    i += 1;
                }

                ' ' | '\t' | '\r' => {
                    i += 1;
                }

                '"' => {
                    buffer.clear();
                    state = State::String;
                    i += 1;
                }
                c if c.is_ascii_alphabetic() || c == '_' => {
                    buffer.clear();
                    buffer.push(c);
                    state = State::Identifier;
                    i += 1;
                }
                c if c.is_ascii_digit() => {
                    buffer.clear();
                    buffer.push(c);
                    state = State::Number;
                    i += 1;
                }
                '.' => {
                    if i + 1 < len && bytes[i + 1] as char == '.' {
                        tokens.push(Token {
                            kind: TokenKind::DotDot,
                            value: "..".to_string(),
                            line,
                        });
                        i += 2;
                    } else {
                        tokens.push(Token {
                            kind: TokenKind::Dot,
                            value: ".".to_string(),
                            line,
                        });
                        i += 1;
                    }
                }

                ':' => {
                    if i + 1 < len && bytes[i + 1] as char == ':' {
                        tokens.push(Token {
                            kind: TokenKind::DoubleColon,
                            value: "::".to_string(),
                            line,
                        });
                        i += 2;
                    } else {
                        return Err(LexError::InvalidCharacter {
                            ch: ':',
                            line,
                            message: "Unexpected ':' — did you mean '::'?".to_string(),
                        });
                    }
                }
                '=' => {
                    if i + 1 < len && bytes[i + 1] as char == '=' {
                        tokens.push(Token {
                            kind: TokenKind::EqualEqual,
                            value: "==".to_string(),
                            line,
                        });
                        i += 2;
                    } else {
                        tokens.push(Token {
                            kind: TokenKind::Equal,
                            value: "=".to_string(),
                            line,
                        });
                        i += 1;
                    }
                }
                '>' => {
                    if i + 1 < len && bytes[i + 1] as char == '=' {
                        tokens.push(Token {
                            kind: TokenKind::GreaterEqual,
                            value: ">=".to_string(),
                            line,
                        });
                        i += 2;
                    } else {
                        tokens.push(Token {
                            kind: TokenKind::GreaterThan,
                            value: ">".to_string(),
                            line,
                        });
                        i += 1;
                    }
                }
                '<' => {
                    if i + 1 < len && bytes[i + 1] as char == '=' {
                        tokens.push(Token {
                            kind: TokenKind::LessEqual,
                            value: "<=".to_string(),
                            line,
                        });
                        i += 2;
                    } else {
                        tokens.push(Token {
                            kind: TokenKind::LessThan,
                            value: "<".to_string(),
                            line,
                        });
                        i += 1;
                    }
                }
                '!' => {
                    if i + 1 < len && bytes[i + 1] as char == '=' {
                        tokens.push(Token {
                            kind: TokenKind::NotEqual,
                            value: "!=".to_string(),
                            line,
                        });
                        i += 2;
                    } else {
                        return Err(LexError::InvalidCharacter {
                            ch: '!',
                            line,
                            message: "Unexpected '!' — did you mean '!='?".to_string(),
                        });
                    }
                }
                '/' => {
                    if i + 1 < len && bytes[i + 1] as char == '/' {
                        state = State::Comment;
                        i += 2;
                    } else {
                        tokens.push(Token {
                            kind: TokenKind::Slash,
                            value: "/".to_string(),
                            line,
                        });
                        i += 1;
                    }
                }

                '-' => {
                    if i + 1 < len && bytes[i + 1] as char == '>' {
                        tokens.push(Token {
                            kind: TokenKind::Arrow,
                            value: "->".to_string(),
                            line,
                        });
                        i += 2;
                    } else {
                        tokens.push(Token {
                            kind: TokenKind::Minus,
                            value: "-".to_string(),
                            line,
                        });
                        i += 1;
                    }
                }
                c => {
                    if let Some(kind) = char_to_token(c) {
                        tokens.push(Token {
                            kind,
                            value: c.to_string(),
                            line,
                        });
                    } else {
                        return Err(LexError::InvalidCharacter {
                            ch: c,
                            line,
                            message: "The Character is invalid".to_string(),
                        });
                    }
                    i += 1;
                }
            },

            State::Identifier => {
                if c.is_ascii_alphanumeric() || c == '_' {
                    buffer.push(c);
                    i += 1;
                } else {
                    let kind = identify_token(&buffer);
                    tokens.push(Token {
                        kind,
                        value: buffer.clone(),
                        line,
                    });
                    buffer.clear();
                    state = State::Default;
                }
            }

            State::String => {
                if c == '"' {
                    tokens.push(Token {
                        kind: TokenKind::String,
                        value: buffer.clone(),
                        line,
                    });
                    buffer.clear();
                    state = State::Default;
                    i += 1;
                } else {
                    if c == '\n' {
                        line += 1;
                    }
                    buffer.push(c);
                    i += 1;
                }
            }
            State::Number => {
                if c.is_ascii_digit() {
                    buffer.push(c);
                    i += 1;
                } else if c == '.' && !buffer.contains('.') {
                    if i + 1 < len && bytes[i + 1] as char == '.' {
                        let kind = if buffer.contains('.') {
                            TokenKind::Float
                        } else {
                            TokenKind::Int
                        };
                        tokens.push(Token {
                            kind,
                            value: buffer.clone(),
                            line,
                        });
                        buffer.clear();
                        state = State::Default;
                        // don't advance i — let Default state handle the '..'
                    } else {
                        buffer.push(c);
                        i += 1;
                    }
                } else {
                    let kind = if buffer.contains('.') {
                        TokenKind::Float
                    } else {
                        TokenKind::Int
                    };

                    tokens.push(Token {
                        kind,
                        value: buffer.clone(),
                        line,
                    });

                    buffer.clear();
                    state = State::Default;
                }
            }
            State::Comment => {
                if c == '\n' {
                    line += 1;
                    state = State::Default;
                }
                i += 1;
            }
        }
    }
    match state {
        State::String => {
            return Err(LexError::UnterminatedString {
                line,
                message: "The string was never terminated!".to_string(),
            });
        }
        State::Identifier => {
            let kind = identify_token(&buffer);
            tokens.push(Token {
                kind,
                value: buffer.clone(),
                line,
            });
        }
        State::Number => {
            let kind = if buffer.contains('.') {
                TokenKind::Float
            } else {
                TokenKind::Int
            };
            tokens.push(Token {
                kind,
                value: buffer.clone(),
                line,
            });
        }
        State::Comment => {}
        State::Default => {}
    }
    tokens.push(Token {
        kind: TokenKind::EOF,
        value: String::new(),
        line,
    });
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lex_minus_is_binary_op() {
        let toks = lexer("r - 1").unwrap();
        let kinds: Vec<_> = toks.iter().map(|t| t.kind.clone()).collect();
        assert_eq!(
            kinds,
            vec![
                TokenKind::Identifier,
                TokenKind::Minus,
                TokenKind::Int,
                TokenKind::EOF
            ]
        );
    }

    #[test]
    fn lex_arrow_and_double_colon() {
        let toks = lexer("a -> b::c").unwrap();
        let kinds: Vec<_> = toks.iter().map(|t| t.kind.clone()).collect();
        assert_eq!(
            kinds,
            vec![
                TokenKind::Identifier,
                TokenKind::Arrow,
                TokenKind::Identifier,
                TokenKind::DoubleColon,
                TokenKind::Identifier,
                TokenKind::EOF
            ]
        );
    }

    #[test]
    fn lex_range_dotdot() {
        let toks = lexer("0..10").unwrap();
        let kinds: Vec<_> = toks.iter().map(|t| t.kind.clone()).collect();
        assert_eq!(
            kinds,
            vec![
                TokenKind::Int,
                TokenKind::DotDot,
                TokenKind::Int,
                TokenKind::EOF
            ]
        );
    }

    #[test]
    fn lex_as_keyword() {
        let toks = lexer("import \"f.drive\" as filt;").unwrap();
        let kinds: Vec<_> = toks.iter().map(|t| t.kind.clone()).collect();
        assert_eq!(
            kinds,
            vec![
                TokenKind::Import,
                TokenKind::String,
                TokenKind::As,
                TokenKind::Identifier,
                TokenKind::SemiColon,
                TokenKind::EOF
            ]
        );
    }
}
