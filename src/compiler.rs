use crate::parser::{get_rule, Precedence};
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

pub struct Compiler<'a> {
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

    pub fn unwrap_previous(&self) -> &Token {
        self.previous.as_ref().unwrap()
    }

    pub fn unwrap_current(&self) -> &Token {
        self.current.as_ref().unwrap()
    }

    pub fn advance(&mut self) {
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

    pub fn consume(&mut self, ttype: TokenType, message: &str) {
        if let Some(t) = &self.current {
            if t.ttype == ttype {
                self.advance();
                return;
            }
        }
        self.error_at_current(message, CompileError::ParseError)
    }

    pub fn parse_precedence(&mut self, prec: Precedence) {
        self.advance();
        match get_rule(self.unwrap_previous().ttype).prefix {
            Some(rule) => rule(self),
            None => {
                self.error("Expect expression.", CompileError::ParseError);
                return;
            }
        }
        while prec <= get_rule(self.unwrap_current().ttype).precedence {
            self.advance();
            get_rule(self.unwrap_previous().ttype).infix.unwrap()(self);
        }
    }

    pub fn expression(&mut self) {
        self.parse_precedence(Precedence::Assignment)
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

    pub fn emit_byte(&mut self, byte: u8) {
        let line = self.previous.as_ref().unwrap().line;
        self.get_current_chunk().write(byte, line);
    }

    pub fn emit_bytes(&mut self, byte1: u8, byte2: u8) {
        self.emit_byte(byte1);
        self.emit_byte(byte2);
    }

    pub fn emit_byte_with_line(&mut self, byte: u8, line: LineNo) {
        self.get_current_chunk().write(byte, line)
    }

    pub fn emit_constant(&mut self, value: Value) {
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
        self.emit_byte(OpCode::Return.into());
        #[cfg(feature = "dump")]
        {
            if let None = self.first_error {
                crate::dis::disassemble_chunk(&self.chunk, "code")
            }
        }
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
        None => Ok(compiler.chunk),
    }
}
