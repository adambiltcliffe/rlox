use crate::scanner::{Scanner, Token, TokenType};
use crate::{Chunk, CompileError, CompilerResult, OpCode};

fn report_error(message: &str, token: &Token) {
    eprint!("[line {}] Error", token.line);
    match token.ttype {
        TokenType::EOF => eprint!(" at end"),
        tt if TokenType::error_message(tt).is_some() => (),
        _ => eprint!(" at '{}'", token.content.unwrap()),
    }
    eprintln!(": {}", message)
}

struct Compiler<'a> {
    scanner: Scanner<'a>,
    previous: Option<Token<'a>>,
    current: Option<Token<'a>>,
    first_error: Option<CompileError>,
    panic_mode: bool,
    chunk: Chunk,
}

impl<'a> Compiler<'a> {
    fn new(scanner: Scanner<'a>) -> Self {
        Self {
            scanner,
            current: None,
            previous: None,
            first_error: None,
            panic_mode: false,
            chunk: Chunk::new(),
        }
    }

    fn advance(&mut self) {
        self.previous = self.current.take();
        loop {
            let token = self.scanner.scan_token();
            let error = TokenType::error_message(token.ttype);
            self.current = Some(token);
            match error {
                None => break,
                Some(e) => self.error_at_current(e, CompileError::ParseError),
            }
        }
    }

    fn expression(&mut self) {}

    fn consume(&mut self, ttype: TokenType, message: &str) {
        if let Some(t) = &self.current {
            if t.ttype == ttype {
                self.advance();
                return;
            }
        }
        self.error_at_current(message, CompileError::ParseError)
    }

    fn error_at_current(&mut self, message: &str, ce: CompileError) {
        if self.panic_mode {
            return;
        }
        report_error(message, self.current.as_ref().unwrap());
        self.first_error = self.first_error.or(Some(ce));
        self.panic_mode = true
    }

    fn error(&mut self, message: &str, ce: CompileError) {
        if self.panic_mode {
            return;
        }
        report_error(message, self.previous.as_ref().unwrap());
        self.first_error = self.first_error.or(Some(ce));
        self.panic_mode = true
    }

    fn get_current_chunk(&mut self) -> &mut Chunk {
        return &mut self.chunk;
    }

    fn emit_byte(&mut self, byte: u8) {
        let line = self.previous.as_ref().unwrap().line;
        self.get_current_chunk().write(byte, line)
    }

    fn emit_bytes(&mut self, byte1: u8, byte2: u8) {
        self.emit_byte(byte1);
        self.emit_byte(byte2);
    }

    fn end(&mut self) {
        self.emit_byte(OpCode::Return.into())
    }
}

pub fn compile(source: &str) -> CompilerResult {
    let scanner = Scanner::new(source);
    let mut compiler = Compiler::new(scanner);
    compiler.advance();
    compiler.expression();
    compiler.consume(TokenType::EOF, "Expect end of expression.");
    compiler.end();
    match compiler.first_error {
        Some(e) => Err(e),
        None => Ok(Chunk::new()),
    }
}
