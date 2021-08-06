use crate::parser::{get_rule, Precedence};
use crate::scanner::{Scanner, Token, TokenType};
use crate::value::{create_string, format_function_name, manage, Function, FunctionType, Value};
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
    cc: ChunkCompiler<'src>,
}

pub struct ChunkCompiler<'src> {
    function: Function,
    function_type: FunctionType,
    locals: Vec<Local<'src>>,
    scope_depth: usize,
    enclosing: Option<Box<ChunkCompiler<'src>>>,
}

impl<'src> ChunkCompiler<'src> {
    pub fn new(vm: &mut VM, function_type: FunctionType) -> Self {
        let function = Function::new_in_vm(vm, None, 0);
        let mut locals = Vec::new();
        locals.push(Local {
            name: "",
            depth: Some(0),
        });
        Self {
            function,
            function_type,
            locals,
            scope_depth: 0,
            enclosing: None,
        }
    }
}

impl<'src, 'vm> Compiler<'src, 'vm> {
    fn new(scanner: Scanner<'src>, vm: &'vm mut VM) -> Self {
        let cc = ChunkCompiler::new(vm, FunctionType::Script);
        Self {
            scanner,
            vm,
            current: None,
            previous: None,
            first_error: None,
            panic_mode: false,
            cc,
        }
    }

    fn begin_scope(&mut self) {
        self.cc.scope_depth += 1;
    }

    fn end_scope(&mut self) {
        self.cc.scope_depth -= 1;
        while !self.cc.locals.is_empty()
            && self.cc.locals.last().unwrap().depth.unwrap() > self.cc.scope_depth
        {
            self.emit_byte(OpCode::Pop.into());
            self.cc.locals.pop();
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
        if self.cc.scope_depth > 0 {
            return Ok(None);
        }
        let v = self.previous_identifier();
        self.identifier_constant(v).map(Some)
    }

    pub fn previous_identifier(&mut self) -> Value {
        let name = &self.previous.as_ref().unwrap().content.unwrap();
        let vm = &mut self.vm;
        create_string(vm, name).into()
    }

    pub fn identifier_constant(&mut self, name: Value) -> Result<u8, CompileError> {
        self.get_current_chunk().add_constant(name)
    }

    pub fn declare_variable(&mut self) {
        if self.cc.scope_depth == 0 {
            return;
        }
        let name = self.previous.as_ref().unwrap().content.unwrap();
        let mut is_duplicate = false;
        for local in self.cc.locals.iter().rev() {
            if let Some(d) = local.depth {
                if d < self.cc.scope_depth {
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
        if self.cc.locals.len() == u8::MAX as usize + 1 {
            self.short_error(CompileError::TooManyLocals);
            return;
        }
        let local = Local {
            name: name,
            depth: None,
        };
        self.cc.locals.push(local);
    }

    pub fn resolve_local(&mut self, name: &str) -> Option<u8> {
        for (i, local) in self.cc.locals.iter().enumerate().rev() {
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
        if self.cc.scope_depth == 0 {
            self.emit_bytes(OpCode::DefineGlobal.into(), global.unwrap());
        } else {
            // mark initialized, it's already sitting on the stack in the right place
            self.mark_initialized();
        }
    }

    fn mark_initialized(&mut self) {
        if self.cc.scope_depth == 0 {
            return;
        };
        self.cc.locals.last_mut().unwrap().depth = Some(self.cc.scope_depth);
    }

    pub fn argument_list(&mut self) -> usize {
        let mut arg_count: usize = 0;
        if !self.check(TokenType::RightParen) {
            loop {
                self.expression();
                if arg_count == 255 {
                    self.short_error(CompileError::TooManyArguments);
                }
                arg_count += 1;
                if !self.match_token(TokenType::Comma) {
                    break;
                }
            }
        }
        self.consume(TokenType::RightParen, "Expect ')' after arguments.");
        arg_count
    }

    pub fn block(&mut self) {
        while !self.check(TokenType::RightBrace) && !self.check(TokenType::EOF) {
            self.declaration();
        }
        self.consume(TokenType::RightBrace, "Expect '}' after block.");
    }

    pub fn function(&mut self, function_type: FunctionType) {
        self.begin_cc(function_type);
        self.begin_scope();
        self.consume(TokenType::LeftParen, "Expect '(' after function name.");
        if !self.check(TokenType::RightParen) {
            loop {
                self.cc.function.arity += 1;
                if self.cc.function.arity > 255 {
                    self.short_error_at_current(CompileError::TooManyParameters);
                }
                match self.parse_variable("Expect parameter name.") {
                    Err(e) => {
                        self.error(&format!("{}", e), e);
                        break;
                    }
                    Ok(constant) => {
                        self.define_variable(constant);
                    }
                }
                if !self.match_token(TokenType::Comma) {
                    break;
                }
            }
        }
        self.consume(TokenType::RightParen, "Expect ')' after parameters.");
        self.consume(TokenType::LeftBrace, "Expect '{' before function body.");
        self.block();
        let func = self.end_cc();
        let val = Value::Function(manage(self.vm, func));
        self.emit_constant(val);
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

    pub fn if_statement(&mut self) {
        self.consume(TokenType::LeftParen, "Expect '(' after 'if'.");
        self.expression();
        self.consume(TokenType::RightParen, "Expect ')' after condition.");
        let then_jump = self.emit_jump(OpCode::JumpIfFalse);
        self.emit_byte(OpCode::Pop.into());
        self.statement();
        let else_jump = self.emit_jump(OpCode::Jump);
        self.patch_jump(then_jump);
        self.emit_byte(OpCode::Pop.into());
        if self.match_token(TokenType::Else) {
            self.statement();
        }
        self.patch_jump(else_jump);
    }

    pub fn while_statement(&mut self) {
        let loop_start = self.get_current_chunk().code.len();
        self.consume(TokenType::LeftParen, "Expect '(' after 'while'.");
        self.expression();
        self.consume(TokenType::RightParen, "Expect ')' after condition.");
        let exit_jump = self.emit_jump(OpCode::JumpIfFalse);
        self.emit_byte(OpCode::Pop.into());
        self.statement();
        self.emit_loop(loop_start);
        self.patch_jump(exit_jump);
        self.emit_byte(OpCode::Pop.into());
    }

    pub fn for_statement(&mut self) {
        self.begin_scope();
        self.consume(TokenType::LeftParen, "Expect '(' after 'for'.");
        if self.match_token(TokenType::Semicolon) {
        } else if self.match_token(TokenType::Var) {
            self.var_declaration();
        } else {
            self.expression_statement();
        }
        let mut loop_start = self.get_current_chunk().code.len();
        let mut exit_jump: Option<usize> = None;
        if !self.match_token(TokenType::Semicolon) {
            self.expression();
            self.consume(TokenType::Semicolon, "Expect ';' after loop condition.");
            exit_jump = Some(self.emit_jump(OpCode::JumpIfFalse));
            self.emit_byte(OpCode::Pop.into());
        }
        if !self.match_token(TokenType::RightParen) {
            let body_jump = self.emit_jump(OpCode::Jump);
            let increment_start = self.get_current_chunk().code.len();
            self.expression();
            self.emit_byte(OpCode::Pop.into());
            self.consume(TokenType::RightParen, "Expect ')' after for clauses.");
            self.emit_loop(loop_start);
            loop_start = increment_start;
            self.patch_jump(body_jump);
        }
        self.statement();
        self.emit_loop(loop_start);
        if let Some(exit_jump) = exit_jump {
            self.patch_jump(exit_jump);
            self.emit_byte(OpCode::Pop.into());
        }
        self.end_scope();
    }

    pub fn declaration(&mut self) {
        if self.match_token(TokenType::Fun) {
            self.fun_declaration();
        } else if self.match_token(TokenType::Var) {
            self.var_declaration();
        } else {
            self.statement();
        }
        if self.panic_mode {
            self.synchronize();
        }
    }

    pub fn fun_declaration(&mut self) {
        match self.parse_variable("Expect variable name.") {
            Err(e) => self.error(&format!("{}", e), e),
            Ok(global) => {
                self.mark_initialized();
                self.function(FunctionType::Function);
                self.define_variable(global);
            }
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
        } else if self.match_token(TokenType::If) {
            self.if_statement();
        } else if self.match_token(TokenType::While) {
            self.while_statement();
        } else if self.match_token(TokenType::For) {
            self.for_statement();
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

    pub(crate) fn short_error_at_current(&mut self, ce: CompileError) {
        self.error_at_current(&ce.to_string(), ce);
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
        return &mut self.cc.function.chunk;
    }

    pub fn emit_byte(&mut self, byte: u8) {
        let line = self.previous.as_ref().unwrap().line;
        self.get_current_chunk().write(byte, line);
    }

    pub fn emit_bytes(&mut self, byte1: u8, byte2: u8) {
        self.emit_byte(byte1);
        self.emit_byte(byte2);
    }

    pub fn emit_jump(&mut self, instruction: OpCode) -> usize {
        self.emit_byte(instruction.into());
        self.emit_byte(0xff_u8);
        self.emit_byte(0xff_u8);
        self.get_current_chunk().code.len() - 2
    }

    pub fn patch_jump(&mut self, offset: usize) {
        let code = &mut self.get_current_chunk().code;
        let jump = code.len() - offset - 2;
        if jump > u16::MAX as usize {
            self.short_error(CompileError::TooFarToJump)
        } else {
            code[offset] = ((jump >> 8) & 0xff) as u8;
            code[offset + 1] = (jump & 0xff) as u8;
        }
    }

    pub fn emit_loop(&mut self, loop_start: usize) {
        self.emit_byte(OpCode::Loop.into());
        let jump = self.get_current_chunk().code.len() - loop_start + 2;
        if jump > u16::MAX as usize {
            self.short_error(CompileError::TooFarToLoop)
        } else {
            self.emit_byte(((jump >> 8) & 0xff) as u8);
            self.emit_byte((jump & 0xff) as u8);
        }
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

    fn begin_cc(&mut self, function_type: FunctionType) {
        let new_cc = ChunkCompiler::new(self.vm, function_type);
        let old_cc = std::mem::replace(&mut self.cc, new_cc);
        self.cc.enclosing = Some(Box::new(old_cc));

        let name = self.previous.as_ref().unwrap().content.unwrap().to_owned();
        self.cc.function.name = Some(create_string(self.vm, &name));
    }

    fn end_cc(&mut self) -> Function {
        // This is inconsistent with end() regarding how it handles errors
        self.emit_byte(OpCode::Return.into());
        #[cfg(feature = "dump")]
        {
            if let None = self.first_error {
                let s = format_function_name(&self.cc.function);
                crate::dis::disassemble_chunk(&self.get_current_chunk(), &s)
            }
        }
        let new_cc = *self.cc.enclosing.take().unwrap();
        let old_cc = std::mem::replace(&mut self.cc, new_cc);
        old_cc.function
    }

    fn end(mut self) -> CompilerResult {
        self.emit_byte(OpCode::Return.into());
        #[cfg(feature = "dump")]
        {
            if let None = self.first_error {
                let s = format_function_name(&self.cc.function);
                crate::dis::disassemble_chunk(&self.get_current_chunk(), &s)
            }
        }
        match self.first_error {
            Some(e) => Err(e),
            None => Ok(self.cc.function),
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
    compiler.end()
}
