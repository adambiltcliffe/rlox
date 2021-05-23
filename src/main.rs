use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::convert::TryFrom;
use std::io::{BufRead, Write};
use std::iter::Peekable;
use std::slice::Iter;

mod compiler;
mod dis;

#[derive(IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
enum OpCode {
    Constant,
    Negate,
    Add,
    Subtract,
    Multiply,
    Divide,
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

    /*fn interpret(&mut self, chunk: &Chunk) -> InterpretResult {
        let mut ip = IP::new(chunk, 0);
        self.run(&mut ip)
    }*/

    fn interpret_source(&mut self, source: &str) -> InterpretResult {
        compiler::compile(source);
        Ok(())
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
                let b = self.pop_stack()?;
                let a = self.pop_stack()?;
                self.stack.push(a $op b);
         } };
        }

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
                    OpCode::Add => binary_op!(+),
                    OpCode::Subtract => binary_op!(-),
                    OpCode::Multiply => binary_op!(*),
                    OpCode::Divide => binary_op!(/),
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

    /*let mut chunk = Chunk::new();

    let constant_index = chunk.add_constant(1.2);
    chunk.write(OpCode::Constant.into(), 122);
    chunk.write(constant_index, 122);

    let constant_index = chunk.add_constant(3.4);
    chunk.write(OpCode::Constant.into(), 123);
    chunk.write(constant_index, 123);

    chunk.write(OpCode::Add.into(), 124);

    let constant_index = chunk.add_constant(5.6);
    chunk.write(OpCode::Constant.into(), 125);
    chunk.write(constant_index, 125);

    chunk.write(OpCode::Divide.into(), 123);
    chunk.write(OpCode::Negate.into(), 123);
    chunk.write(OpCode::Return.into(), 123);
    chunk.write(OpCode::Return.into(), 124);

    println!("disassembler output:");
    dis::disassemble_chunk(&chunk, "test chunk");
    println!("interpreter output:");
    match vm.interpret(&chunk) {
        Ok(_) => println!("execution terminated successfully"),
        Err(e) => println!("execution terminated with error: {:?}", e),
    }*/

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
    std::io::stdout().flush();
    for line in std::io::stdin().lock().lines() {
        vm.interpret_source(&line.unwrap());
        print!("> ");
        std::io::stdout().flush();
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
