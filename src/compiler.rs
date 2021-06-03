use crate::scanner::{Scanner, Token, TokenType};
use crate::{Chunk, CompileError, CompilerResult, LineNo, OpCode, Value};

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

    fn consume(&mut self, ttype: TokenType, message: &str) {
        if let Some(t) = &self.current {
            if t.ttype == ttype {
                self.advance();
                return;
            }
        }
        self.error_at_current(message, CompileError::ParseError)
    }

    fn number(&mut self) {
        let value: Value = self
            .previous
            .as_ref()
            .unwrap()
            .content
            .unwrap()
            .parse()
            .unwrap();
        self.emit_constant(value);
    }

    fn grouping(&mut self) {
        self.expression();
        self.consume(TokenType::RightParen, "Expect ')' after expression.")
    }

    fn unary(&mut self) {
        let token = self.previous.as_ref().unwrap();
        let op_type = token.ttype;
        let line = token.line;
        self.expression();
        match op_type {
            TokenType::Minus => self.emit_byte_with_line(OpCode::Negate.into(), line),
            _ => unreachable!(),
        }
    }

    fn expression(&mut self) {}

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

    fn emit_byte_with_line(&mut self, byte: u8, line: LineNo) {
        self.get_current_chunk().write(byte, line)
    }

    fn emit_constant(&mut self, value: Value) {
        if let Some(constant) = self.get_current_chunk().add_constant(value) {
            self.emit_bytes(OpCode::Constant.into(), constant)
        } else {
            self.error(
                "Too many constants in one chunk.",
                CompileError::TooManyConstants,
            )
        }
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
