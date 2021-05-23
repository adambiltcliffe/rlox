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
    EOF,
    UnexpectedCharacterError,
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

struct Scanner<'a> {
    source: &'a str,
    source_offset: usize,
    token_start: usize,
    chars: Peekable<CharIndices<'a>>,
    line: usize,
}

impl<'a> Scanner<'a> {
    pub fn new(source: &'a str) -> Self {
        let mut chars = source.char_indices().peekable();
        Self {
            source,
            source_offset: 0,
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
        let end_offset = byte + strlen + self.source_offset;
        if end_offset > self.source.len() {
            return false;
        }
        if expected == &self.source[byte + self.source_offset..end_offset] {
            self.chars = self.source[end_offset..].char_indices().peekable();
            self.source_offset = end_offset;
            return true;
        }
        false
    }

    fn current(&mut self) -> usize {
        self.chars
            .peek()
            .map(|(index, _c)| *index)
            .unwrap_or(self.source.len() - self.source_offset)
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
        let span =
            Some(&self.source[self.token_start + self.source_offset..current + self.source_offset]);
        Token::new(ttype, span, self.line)
    }

    pub fn scan_token(&mut self) -> Token<'a> {
        self.skip_whitespace();
        self.token_start = self.current();
        match self.advance() {
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
            print!("{:>4} ", line);
            line = token.line;
        } else {
            print!("   | ");
        }
        println!("{:?}", token);
        if let TokenType::EOF = token.ttype {
            break ();
        }
    }
}
