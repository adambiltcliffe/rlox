use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::convert::TryFrom;
use std::iter::Peekable;
use std::slice::Iter;

mod dis;

#[derive(IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
enum OpCode {
    Constant,
    Negate,
    Return,
}

type Value = f64;
type LineNo = u32;

struct Chunk {
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

    fn add_constant(&mut self, value: Value) -> u8 {
        self.constants.push(value);
        (self.constants.len() - 1) as u8
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

    fn read_constant(&mut self) -> Value {
        let index = self.read();
        self.chunk.constants[index as usize]
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

    //fn valid(&self) -> bool {
    //    self.offset < self.chunk.code.len()
    //}

    fn read(&mut self) -> u8 {
        let result = self.chunk.code[self.offset];
        self.offset += 1;
        result
    }

    fn read_constant(&mut self) -> Value {
        let index = self.read();
        self.chunk.constants[index as usize]
    }
}

#[derive(Debug)]
enum CompileError {}

#[derive(Debug)]
enum RuntimeError {
    StackUnderflow,
}

#[derive(Debug)]
enum VMError {
    CompileError(CompileError),
    RuntimeError(RuntimeError),
}

type ValueResult = Result<Value, VMError>;
type InterpretResult = Result<(), VMError>;

struct VM {
    stack: Vec<Value>,
}

impl VM {
    fn new() -> Self {
        Self { stack: Vec::new() }
    }

    fn interpret(&mut self, chunk: &Chunk) -> InterpretResult {
        let mut ip = IP::new(chunk, 0);
        self.run(&mut ip)
    }

    fn pop_stack(&mut self) -> ValueResult {
        match self.stack.pop() {
            Some(v) => Ok(v),
            None => Err(VMError::RuntimeError(RuntimeError::StackUnderflow)),
        }
    }

    fn run(&mut self, ip: &mut IP) -> InterpretResult {
        loop {
            #[cfg(feature = "trace")]
            {
                print!("          ");
                if self.stack.len() == 0 {
                    println!("<empty>");
                } else {
                    for v in &self.stack {
                        print!("[ {} ]", v);
                    }
                    println!("");
                }
                dis::disassemble_instruction(&mut ip.clone());
            }

            match OpCode::try_from(ip.read()) {
                Ok(instruction) => match instruction {
                    OpCode::Constant => {
                        let val = ip.read_constant();
                        self.stack.push(val);
                    }
                    OpCode::Negate => {
                        let val = self.pop_stack()?;
                        self.stack.push(-val);
                    }
                    OpCode::Return => {
                        println!("{}", self.pop_stack()?);
                        return Ok(());
                    }
                },
                Err(_) => println!("(ignoring unknown opcode)"),
            }
        }
    }
}

fn main() {
    let mut vm = VM::new();
    let mut chunk = Chunk::new();
    let constant_index = chunk.add_constant(1.2);
    chunk.write(OpCode::Constant.into(), 122);
    chunk.write(constant_index, 122);
    chunk.write(OpCode::Negate.into(), 123);
    chunk.write(OpCode::Return.into(), 123);
    chunk.write(OpCode::Return.into(), 124);
    println!("disassembler output:");
    dis::disassemble_chunk(&chunk, "test chunk");
    println!("interpreter output:");
    match vm.interpret(&chunk) {
        Ok(_) => println!("execution terminated successfully"),
        Err(e) => println!("execution terminated with error: {:?}", e),
    }
}
