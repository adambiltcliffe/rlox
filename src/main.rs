use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::collections::{HashMap, HashSet};
use std::convert::{TryFrom, TryInto};
use std::fmt;
use std::io::{BufRead, Write};
use std::iter::Peekable;
use std::slice::Iter;
use value::{HeapEntry, InternedString, ObjectRoot, Value, ValueType};

mod compiler;
mod dis;
mod parser;
mod scanner;
mod value;

#[derive(IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
pub enum OpCode {
    Constant,
    Nil,
    True,
    False,
    Equal,
    Greater,
    Less,
    Negate,
    Add,
    Subtract,
    Multiply,
    Divide,
    Not,
    Print,
    Jump,
    JumpIfFalse,
    Pop,
    GetLocal,
    SetLocal,
    GetGlobal,
    DefineGlobal,
    SetGlobal,
    Return,
}

type LineNo = u32;

pub struct Chunk {
    code: Vec<u8>,
    constants: Vec<Value>,
    lines: Vec<(usize, LineNo)>,
}

impl Chunk {
    fn new() -> Self {
        Self {
            code: Vec::new(),
            constants: Vec::new(),
            lines: Vec::new(),
        }
    }

    fn write(&mut self, byte: u8, line: LineNo) {
        self.code.push(byte);
        match self.lines.last() {
            Some(&(_, l)) if l == line => (),
            _ => self.lines.push((self.code.len() - 1, line)),
        }
    }

    fn add_constant(&mut self, value: Value) -> Result<u8, CompileError> {
        if self.constants.len() > (u8::MAX as usize) {
            return Err(CompileError::TooManyConstants);
        }
        self.constants.push(value);
        Ok((self.constants.len() - 1) as u8)
    }
}

#[derive(Clone)]
struct TracingIP<'a> {
    chunk: &'a Chunk,
    offset: usize,
    line: Option<LineNo>,
    is_line_start: bool,
    new_lines: Peekable<Iter<'a, (usize, LineNo)>>,
}

#[allow(dead_code)]
impl<'a> TracingIP<'a> {
    fn new(chunk: &'a Chunk, offset: usize) -> Self {
        let new_lines = chunk.lines.iter().peekable();
        let mut me = Self {
            chunk,
            offset,
            line: None,
            is_line_start: false,
            new_lines,
        };
        me.advance();
        me
    }

    fn advance(&mut self) {
        let old_line = self.line;
        self.line = match self.new_lines.peek() {
            Some(&&(offs, l)) if offs == self.offset => {
                self.new_lines.next();
                Some(l)
            }
            _ => self.line,
        };
        self.is_line_start = self.line != old_line;
    }

    fn valid(&self) -> bool {
        self.offset < self.chunk.code.len()
    }

    fn read(&mut self) -> u8 {
        let result = self.chunk.code[self.offset];
        self.offset += 1;
        self.advance();
        result
    }

    fn read_short(&mut self) -> u16 {
        let high = self.read() as u16;
        let low = self.read() as u16;
        (high << 8) | low
    }

    fn read_constant(&mut self) -> Value {
        let index = self.read();
        self.chunk.constants[index as usize].clone()
    }

    fn get_line(&self) -> Option<LineNo> {
        self.line
    }
}

#[cfg(feature = "trace")]
type IP<'a> = TracingIP<'a>;

// A fast IP to use when we don't need up-to-date line number info
#[cfg(not(feature = "trace"))]
struct IP<'a> {
    chunk: &'a Chunk,
    offset: usize,
}

#[cfg(not(feature = "trace"))]
impl<'a> IP<'a> {
    fn new(chunk: &'a Chunk, offset: usize) -> Self {
        Self { chunk, offset }
    }

    fn valid(&self) -> bool {
        self.offset < self.chunk.code.len()
    }

    fn read(&mut self) -> u8 {
        let result = self.chunk.code[self.offset];
        self.offset += 1;
        result
    }

    fn read_short(&mut self) -> u16 {
        let high = self.read() as u16;
        let low = self.read() as u16;
        (high << 8) | low
    }

    fn read_constant(&mut self) -> Value {
        let index = self.read();
        self.chunk.constants[index as usize].clone()
    }

    // This is much more expensive than with TracingIP because this is the
    // uncommon case we didn't optimise for
    fn get_line(&self) -> Option<LineNo> {
        let mut line: Option<LineNo> = None;
        for &(offs, n) in self.chunk.lines.iter() {
            if offs >= self.offset {
                break;
            }
            line = Some(n)
        }
        line
    }
}

#[derive(Debug, Clone, Copy)]
pub enum CompileError {
    ParseError,
    TooManyConstants,
    TooManyLocals,
    DuplicateName,
    UninitializedLocal,
    TooFarToJump,
}

#[derive(Debug, Clone)]
pub enum RuntimeError {
    UnknownOpcode,
    EndOfChunk,
    StackUnderflow,
    TypeError(ValueType, String),
    InvalidAddition(String, String),
    UndefinedVariable(String),
}

#[derive(Debug, Clone)]
pub enum VMError {
    CompileError(CompileError),
    RuntimeError(RuntimeError),
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CompileError::ParseError => write!(f, "Parse error."),
            CompileError::TooManyConstants => write!(f, "Too many constants in one chunk."),
            CompileError::TooManyLocals => write!(f, "Too many local variables in function."),
            CompileError::DuplicateName => {
                write!(f, "Already a variable with this name in this scope.")
            }
            CompileError::UninitializedLocal => {
                write!(f, "Can't read local variable in its own initializer.")
            }
            CompileError::TooFarToJump => write!(f, "Too much code to jump over."),
        }
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RuntimeError::UnknownOpcode => write!(f, "Unknown opcode."),
            RuntimeError::EndOfChunk => write!(f, "Unexpected end of chunk."),
            RuntimeError::StackUnderflow => write!(f, "Stack underflow."),
            RuntimeError::TypeError(t, v) => {
                #[cfg(not(feature = "lox_errors"))]
                {
                    return write!(f, "Expected a {} value but found: {}.", t, v);
                }
                #[cfg(feature = "lox_errors")]
                {
                    return write!(f, "Operands must be {}s.", t);
                }
            }

            RuntimeError::InvalidAddition(v1, v2) => {
                #[cfg(not(feature = "lox_errors"))]
                {
                    return write!(f, "Invalid types for + operator: {}, {}.", v1, v2);
                }
                #[cfg(feature = "lox_errors")]
                {
                    return write!(f, "Operands must be two numbers or two strings.");
                }
            }
            RuntimeError::UndefinedVariable(name) => write!(f, "Undefined variable '{}'.", name),
        }
    }
}

type CompilerResult = Result<Chunk, CompileError>;
type ValueResult = Result<Value, VMError>;
type InterpretResult = Result<(), VMError>;

pub struct VM {
    stack: Vec<Value>,
    objects: Vec<ObjectRoot>,
    strings: HashSet<value::InternedString>,
    globals: HashMap<value::InternedString, Value>,
}

impl VM {
    fn new() -> Self {
        Self {
            stack: Vec::new(),
            objects: Vec::new(),
            strings: HashSet::new(),
            globals: HashMap::new(),
        }
    }

    fn interpret_source(&mut self, source: &str) -> InterpretResult {
        let chunk = compiler::compile(source, self).map_err(VMError::CompileError)?;
        let mut ip = IP::new(&chunk, 0);
        let result = self.run(&mut ip);
        if let Err(VMError::RuntimeError(ref e)) = result {
            if let Some(n) = ip.get_line() {
                eprint!("[line {}] ", n);
            } else {
                eprint!("[unknown line] ");
            }
            eprintln!("Runtime error: {}", e);
            self.stack.clear();
        }
        result
    }

    fn peek_stack(&self, distance: usize) -> Value {
        self.stack[self.stack.len() - 1 - distance].clone()
    }

    fn pop_stack(&mut self) -> ValueResult {
        match self.stack.pop() {
            Some(v) => Ok(v),
            None => Err(VMError::RuntimeError(RuntimeError::StackUnderflow)),
        }
    }

    fn run(&mut self, ip: &mut IP) -> InterpretResult {
        macro_rules! binary_op {
            ($op:tt) => {{
                let b: f64 = self.pop_stack()?.try_into()?;
                let a: f64= self.pop_stack()?.try_into()?;
                self.stack.push((a $op b).into());
         } };
        }

        #[cfg(feature = "trace")]
        {
            println!("Execution trace:")
        }

        loop {
            // Performance-wise, we may want to delete this eventually
            if !ip.valid() {
                return rt(RuntimeError::EndOfChunk);
            }

            #[cfg(feature = "trace")]
            {
                print!("          ");
                if self.stack.len() == 0 {
                    print!("<empty>");
                } else {
                    for v in &self.stack {
                        print!("[ {} ]", v);
                    }
                }
                print!(
                    " (heap: {}, strings: {})",
                    self.objects.len(),
                    self.strings.len()
                );
                for (k, v) in &self.globals {
                    print!(" {}={}", k, v);
                }
                println!("");
                dis::disassemble_instruction(&mut ip.clone());
            }

            match OpCode::try_from(ip.read()) {
                Ok(instruction) => match instruction {
                    OpCode::Constant => {
                        let val = ip.read_constant();
                        self.stack.push(val);
                    }
                    OpCode::Nil => self.stack.push(Value::Nil),
                    OpCode::True => self.stack.push(Value::Bool(true)),
                    OpCode::False => self.stack.push(Value::Bool(false)),
                    OpCode::Equal => {
                        let a = self.pop_stack()?;
                        let b = self.pop_stack()?;
                        self.stack.push((a == b).into());
                    }
                    OpCode::Greater => binary_op!(>),
                    OpCode::Less => binary_op!(<),
                    OpCode::Negate => {
                        let n: f64 = self.pop_stack()?.try_into()?;
                        self.stack.push((-n).into());
                    }
                    OpCode::Add => {
                        let a = self.pop_stack()?;
                        let b = self.pop_stack()?;
                        match (a.get_type(), b.get_type()) {
                            (ValueType::Number, ValueType::Number) => {
                                let a: f64 = a.try_into()?;
                                let b: f64 = b.try_into()?;
                                self.stack.push((a + b).into())
                            }
                            (ValueType::String, ValueType::String) => {
                                let a: String = a.try_into()?;
                                let b: String = b.try_into()?;
                                let w = HeapEntry::create_string(self, &(b + &a));
                                self.stack.push(w.into())
                            }
                            _ => {
                                return rt(RuntimeError::InvalidAddition(
                                    b.to_string(),
                                    a.to_string(),
                                ))
                            }
                        }
                    }
                    OpCode::Subtract => binary_op!(-),
                    OpCode::Multiply => binary_op!(*),
                    OpCode::Divide => binary_op!(/),
                    OpCode::Not => {
                        let b = self.pop_stack()?.is_falsey();
                        self.stack.push(b.into());
                    }
                    OpCode::Print => {
                        println!("{}", value::printable_value(self.pop_stack()?));
                    }
                    OpCode::Jump => {
                        let offset = ip.read_short() as usize;
                        ip.offset += offset;
                    }
                    OpCode::JumpIfFalse => {
                        let offset = ip.read_short() as usize;
                        if self.peek_stack(0).is_falsey() {
                            ip.offset += offset;
                        }
                    }
                    OpCode::Pop => {
                        self.pop_stack()?;
                    }
                    OpCode::GetLocal => {
                        let slot = ip.read();
                        self.stack.push(self.stack[slot as usize].clone());
                    }
                    OpCode::SetLocal => {
                        let slot = ip.read();
                        self.stack[slot as usize] = self.peek_stack(0).clone();
                    }
                    OpCode::GetGlobal => {
                        let val = ip.read_constant();
                        let interned: InternedString = val.clone().try_into()?;
                        match self.globals.get(&interned) {
                            Some(v) => {
                                self.stack.push(v.clone());
                            }
                            None => return rt(RuntimeError::UndefinedVariable(val.try_into()?)),
                        }
                    }
                    OpCode::DefineGlobal => {
                        let val = ip.read_constant();
                        let interned: InternedString = val.try_into()?;
                        self.globals.insert(interned, self.peek_stack(0));
                        self.pop_stack()?;
                    }
                    OpCode::SetGlobal => {
                        let val = ip.read_constant();
                        let interned: InternedString = val.clone().try_into()?;
                        if self.globals.contains_key(&interned) {
                            self.globals.insert(interned, self.peek_stack(0));
                        } else {
                            return rt(RuntimeError::UndefinedVariable(val.try_into()?));
                        }
                    }
                    OpCode::Return => {
                        return Ok(());
                    }
                },
                Err(_) => return rt(RuntimeError::UnknownOpcode),
            }
        }
    }
}

fn main() {
    let mut vm = VM::new();
    let args: Vec<String> = std::env::args().collect();
    let argc = args.len();
    if argc == 1 {
        repl(&mut vm);
    } else if argc == 2 {
        run_file(&mut vm, &args[1])
    } else {
        eprintln!("usage: rlox [path]");
        std::process::exit(64);
    }
}

fn repl(vm: &mut VM) {
    print!("> ");
    std::io::stdout().flush().expect("Error writing to stdout.");
    for line in std::io::stdin().lock().lines() {
        // Following line silences the error since we already handled it
        vm.interpret_source(&line.unwrap()).unwrap_or(());
        print!("> ");
        std::io::stdout().flush().expect("Error writing to stdout.");
    }
}

fn run_file(vm: &mut VM, path: &str) -> ! {
    let source = std::fs::read_to_string(path).unwrap_or_else(|_| {
        eprintln!("Could not read input file: {}", path);
        std::process::exit(74)
    });
    let exitcode = match vm.interpret_source(&source) {
        Ok(()) => 0,
        Err(VMError::CompileError(_)) => 65,
        Err(VMError::RuntimeError(_)) => 70,
    };
    std::process::exit(exitcode);
}

fn rt(e: RuntimeError) -> InterpretResult {
    Err(VMError::RuntimeError(e))
}
