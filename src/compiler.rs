use std::iter::Peekable;
use std::str::CharIndices;

#[derive(Debug)]
enum TokenType {
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    Comma,
    Dot,
    Minus,
    Plus,
    Semicolon,
    Slash,
    Star,
    Bang,
    BangEqual,
    Equal,
    EqualEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    NumberLiteral,
    StringLiteral,
    EOF,
    UnexpectedCharacterError,
    UnterminatedStringError,
}

#[derive(Debug)]
struct Token<'a> {
    ttype: TokenType,
    content: Option<&'a str>,
    line: usize,
}

impl<'a> Token<'a> {
    pub fn new(ttype: TokenType, content: Option<&'a str>, line: usize) -> Self {
        Self {
            ttype,
            content,
            line,
        }
    }
}

fn is_digit(c: Option<char>) -> bool {
    if let Some(c) = c {
        return c >= '0' && c <= '9';
    }
    false
}

struct Scanner<'a> {
    source: &'a str,
    token_start: usize,
    chars: Peekable<CharIndices<'a>>,
    line: usize,
}

impl<'a> Scanner<'a> {
    pub fn new(source: &'a str) -> Self {
        let mut chars = source.char_indices().peekable();
        Self {
            source,
            token_start: chars.peek().map(|(index, _c)| *index).unwrap_or(0),
            chars,
            line: 1,
        }
    }

    fn advance(&mut self) -> Option<char> {
        match self.chars.next() {
            None => None,
            Some((_index, c)) => Some(c),
        }
    }

    fn maybe_match(&mut self, expected: char) -> bool {
        match self.chars.peek() {
            None => false,
            Some((_index, c)) => {
                if *c != expected {
                    return false;
                }
                let _ = self.advance();
                true
            }
        }
    }

    fn maybe_match_str(&mut self, expected: &str) -> bool {
        let strlen = expected.len();
        let byte: usize = match self.chars.peek() {
            None => return false,
            Some((index, _c)) => *index,
        };
        let end_offset = byte + strlen;
        if end_offset > self.source.len() {
            return false;
        }
        if expected == &self.source[byte..end_offset] {
            for _ in 0..expected.chars().count() {
                self.chars.next();
            }
            return true;
        }
        false
    }

    fn current(&mut self) -> usize {
        self.chars
            .peek()
            .map(|(index, _c)| *index)
            .unwrap_or(self.source.len())
    }

    fn skip_whitespace(&mut self) {
        loop {
            match self.chars.peek() {
                Some((_, ' ')) | Some((_, '\r')) | Some((_, '\t')) => {
                    self.advance();
                }
                Some((_, '\n')) => {
                    self.line += 1;
                    self.advance();
                }
                Some((_, '/')) => {
                    if self.maybe_match_str("//") {
                        while let Some((_, c)) = self.chars.peek() {
                            if *c == '\n' {
                                break;
                            }
                            self.advance();
                        }
                    } else {
                        return;
                    }
                }
                _ => return,
            };
        }
    }

    fn make_token(&mut self, ttype: TokenType) -> Token<'a> {
        let current = self.current();
        let span = Some(&self.source[self.token_start..current]);
        Token::new(ttype, span, self.line)
    }

    fn string_literal(&mut self) -> Token<'a> {
        loop {
            match self.chars.peek() {
                Some((_, '"')) => {
                    self.advance();
                    return self.make_token(TokenType::StringLiteral);
                }
                Some((_, c)) => {
                    if *c == '\n' {
                        self.line += 1;
                    }
                    self.advance();
                }
                None => return self.make_token(TokenType::UnterminatedStringError),
            }
        }
    }

    fn consume_integers(&mut self) {
        while match self.chars.peek() {
            Some((_, c)) => is_digit(Some(*c)),
            None => false,
        } {
            self.advance();
        }
    }

    fn number_literal(&mut self) -> Token<'a> {
        self.consume_integers();
        let mut ch = self.chars.clone();
        if let Some((_, '.')) = ch.next() {
            if let Some((_, c)) = ch.next() {
                if is_digit(Some(c)) {
                    self.advance();
                    self.consume_integers();
                }
            }
        }
        self.make_token(TokenType::NumberLiteral)
    }

    pub fn scan_token(&mut self) -> Token<'a> {
        self.skip_whitespace();
        self.token_start = self.current();
        let c = self.advance();
        if is_digit(c) {
            return self.number_literal();
        }
        match c {
            None => Token::new(TokenType::EOF, None, self.line),
            Some(c) => match c {
                '(' => self.make_token(TokenType::LeftParen),
                ')' => self.make_token(TokenType::RightParen),
                '{' => self.make_token(TokenType::LeftBrace),
                '}' => self.make_token(TokenType::RightBrace),
                ',' => self.make_token(TokenType::Comma),
                '.' => self.make_token(TokenType::Dot),
                '-' => self.make_token(TokenType::Minus),
                '+' => self.make_token(TokenType::Plus),
                ';' => self.make_token(TokenType::Semicolon),
                '/' => self.make_token(TokenType::Slash),
                '*' => self.make_token(TokenType::Star),
                '!' => {
                    if self.maybe_match('=') {
                        self.make_token(TokenType::BangEqual)
                    } else {
                        self.make_token(TokenType::Bang)
                    }
                }
                '=' => {
                    if self.maybe_match('=') {
                        self.make_token(TokenType::EqualEqual)
                    } else {
                        self.make_token(TokenType::Equal)
                    }
                }
                '<' => {
                    if self.maybe_match('=') {
                        self.make_token(TokenType::LessEqual)
                    } else {
                        self.make_token(TokenType::Less)
                    }
                }
                '>' => {
                    if self.maybe_match('=') {
                        self.make_token(TokenType::GreaterEqual)
                    } else {
                        self.make_token(TokenType::Greater)
                    }
                }
                '"' => self.string_literal(),
                _ => self.make_token(TokenType::UnexpectedCharacterError),
            },
        }
    }
}

pub fn compile(source: &str) {
    let mut line: usize = 0;
    let mut scanner = Scanner::new(source);
    loop {
        let token = scanner.scan_token();
        if token.line != line {
            line = token.line;
            print!("{:>4} ", line);
        } else {
            print!("   | ");
        }
        println!("{:?}", token);
        if let TokenType::EOF = token.ttype {
            break ();
        }
    }
}
