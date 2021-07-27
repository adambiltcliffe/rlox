use crate::parser::{get_rule, Precedence};
use crate::scanner::{Scanner, Token, TokenType};
use crate::value::{HeapEntry, Value};
use crate::VM;
use crate::{Chunk, CompileError, CompilerResult, LineNo, OpCode};
use std::convert::TryInto;

fn report_error(message: &str, token: &Token) {
    eprint!("[line {}] Error", token.line);
    match token.ttype {
        TokenType::EOF => eprint!(" at end"),
        tt if TokenType::error_message(tt).is_some() => (),
        _ => eprint!(" at '{}'", token.content.unwrap()),
    }
    eprintln!(": {}", message)
}

pub struct Local<'src> {
    name: &'src str,
    depth: Option<usize>,
}

pub struct Compiler<'src, 'vm> {
    pub vm: &'vm mut VM,
    pub scanner: Scanner<'src>,
    pub previous: Option<Token<'src>>,
    pub current: Option<Token<'src>>,
    first_error: Option<CompileError>,
    panic_mode: bool,
    chunk: Chunk,
    locals: Vec<Local<'src>>,
    local_count: usize,
    scope_depth: usize,
}

impl<'src, 'vm> Compiler<'src, 'vm> {
    fn new(scanner: Scanner<'src>, vm: &'vm mut VM) -> Self {
        Self {
            scanner,
            vm,
            current: None,
            previous: None,
            first_error: None,
            panic_mode: false,
            chunk: Chunk::new(),
            locals: Vec::new(),
            local_count: 0,
            scope_depth: 0,
        }
    }

    fn begin_scope(&mut self) {
        self.scope_depth += 1;
    }

    fn end_scope(&mut self) {
        self.scope_depth -= 1;
        while !self.locals.is_empty()
            && self.locals.last().unwrap().depth.unwrap() > self.scope_depth
        {
            self.emit_byte(OpCode::Pop.into());
            self.locals.pop();
        }
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

    pub fn check(&mut self, ttype: TokenType) -> bool {
        if let Some(t) = &self.current {
            return t.ttype == ttype;
        }
        false
    }

    pub fn match_token(&mut self, ttype: TokenType) -> bool {
        if !self.check(ttype) {
            return false;
        }
        self.advance();
        true
    }

    pub fn parse_precedence(&mut self, prec: Precedence) {
        self.advance();
        let can_assign = prec <= Precedence::Assignment;
        match get_rule(self.previous.as_ref().unwrap().ttype).prefix {
            Some(rule) => rule(self, can_assign),
            None => {
                self.error("Expect expression.", CompileError::ParseError);
                return;
            }
        }
        while prec <= get_rule(self.current.as_ref().unwrap().ttype).precedence {
            self.advance();
            get_rule(self.previous.as_ref().unwrap().ttype)
                .infix
                .unwrap()(self, can_assign);
        }
        if can_assign && self.match_token(TokenType::Equal) {
            self.error("Invalid assignment target.", CompileError::ParseError);
        }
    }

    pub fn parse_variable(&mut self, message: &str) -> Result<Option<u8>, CompileError> {
        self.consume(TokenType::Identifier, message);
        self.declare_variable();
        if self.scope_depth > 0 {
            return Ok(None);
        }
        let v = self.previous_identifier();
        self.identifier_constant(v).map(Some)
    }

    pub fn previous_identifier(&mut self) -> Value {
        let name = &self.previous.as_ref().unwrap().content.unwrap();
        let vm = &mut self.vm;
        HeapEntry::create_string(vm, name).into()
    }

    pub fn identifier_constant(&mut self, name: Value) -> Result<u8, CompileError> {
        self.get_current_chunk().add_constant(name)
    }

    pub fn declare_variable(&mut self) {
        if self.scope_depth == 0 {
            return;
        }
        let name = self.previous.as_ref().unwrap().content.unwrap();
        let mut is_duplicate = false;
        for local in self.locals.iter().rev() {
            if let Some(d) = local.depth {
                if d < self.scope_depth {
                    break;
                }
            }
            if name == local.name {
                is_duplicate = true;
                break;
            }
        }
        if is_duplicate {
            self.short_error(CompileError::DuplicateName)
        } else {
            self.add_local(name);
        }
    }

    pub fn add_local(&mut self, name: &'src str) {
        if self.local_count == u8::MAX as usize + 1 {
            self.short_error(CompileError::TooManyLocals);
            return;
        }
        let local = Local {
            name: name,
            depth: None,
        };
        self.locals.push(local);
    }

    pub fn resolve_local(&mut self, name: &str) -> Option<u8> {
        for (i, local) in self.locals.iter().enumerate().rev() {
            if local.name == name {
                if local.depth.is_none() {
                    self.short_error(CompileError::UninitializedLocal)
                }
                return Some(i.try_into().unwrap());
            }
        }
        return None;
    }

    pub fn define_variable(&mut self, global: Option<u8>) {
        if self.scope_depth == 0 {
            self.emit_bytes(OpCode::DefineGlobal.into(), global.unwrap());
        } else {
            self.locals.last_mut().unwrap().depth = Some(self.scope_depth);
        }
    }

    pub fn block(&mut self) {
        while !self.check(TokenType::RightBrace) && !self.check(TokenType::EOF) {
            self.declaration();
        }
        self.consume(TokenType::RightBrace, "Expect '}' after block.");
    }

    pub fn expression(&mut self) {
        self.parse_precedence(Precedence::Assignment)
    }

    pub fn expression_statement(&mut self) {
        self.expression();
        self.consume(TokenType::Semicolon, "Expect ';' after expression.");
        self.emit_byte(OpCode::Pop.into());
    }

    pub fn print_statement(&mut self) {
        self.expression();
        self.consume(TokenType::Semicolon, "Expect ';' after value.");
        self.emit_byte(OpCode::Print.into());
    }

    pub fn declaration(&mut self) {
        if self.match_token(TokenType::Var) {
            self.var_declaration();
        } else {
            self.statement();
        }
        if self.panic_mode {
            self.synchronize();
        }
    }

    pub fn var_declaration(&mut self) {
        match self.parse_variable("Expect variable name.") {
            Err(e) => self.error(&format!("{}", e), e),
            Ok(global) => {
                if self.match_token(TokenType::Equal) {
                    self.expression();
                } else {
                    self.emit_byte(OpCode::Nil.into());
                }
                self.consume(
                    TokenType::Semicolon,
                    "Expect ';' after variable declaration.",
                );
                self.define_variable(global);
            }
        }
    }

    pub fn synchronize(&mut self) {
        self.panic_mode = false;
        while self.current.as_ref().unwrap().ttype != TokenType::EOF {
            if self.previous.as_ref().unwrap().ttype == TokenType::Semicolon {
                return;
            }
            match self.current.as_ref().unwrap().ttype {
                TokenType::Class
                | TokenType::Fun
                | TokenType::Var
                | TokenType::For
                | TokenType::If
                | TokenType::While
                | TokenType::Print
                | TokenType::Return => return,
                _ => (),
            }
            self.advance();
        }
    }

    pub fn statement(&mut self) {
        if self.match_token(TokenType::Print) {
            self.print_statement();
        } else if self.match_token(TokenType::LeftBrace) {
            self.begin_scope();
            self.block();
            self.end_scope();
        } else {
            self.expression_statement();
        }
    }

    fn error_at_current(&mut self, message: &str, ce: CompileError) {
        if self.panic_mode {
            return;
        }
        report_error(message, self.current.as_ref().unwrap());
        self.first_error = self.first_error.or(Some(ce));
        self.panic_mode = true
    }

    pub(crate) fn error(&mut self, message: &str, ce: CompileError) {
        if self.panic_mode {
            return;
        }
        report_error(message, self.previous.as_ref().unwrap());
        self.first_error = self.first_error.or(Some(ce));
        self.panic_mode = true
    }

    pub(crate) fn short_error(&mut self, ce: CompileError) {
        self.error(&ce.to_string(), ce);
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
        if let Ok(constant) = self.get_current_chunk().add_constant(value) {
            self.emit_bytes(OpCode::Constant.into(), constant)
        } else {
            let m: &str = &format!("{}", CompileError::TooManyConstants);
            self.error(m, CompileError::TooManyConstants)
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

pub(crate) fn compile(source: &str, vm: &mut VM) -> CompilerResult {
    let scanner = Scanner::new(source);
    let mut compiler = Compiler::new(scanner, vm);
    compiler.advance();
    while !compiler.match_token(TokenType::EOF) {
        compiler.declaration();
    }
    compiler.consume(TokenType::EOF, "Expect end of expression.");
    compiler.end();
    match compiler.first_error {
        Some(e) => Err(e),
        None => Ok(compiler.chunk),
    }
}
