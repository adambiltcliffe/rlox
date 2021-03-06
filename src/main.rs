use gc::Trace;
use memory::get_allocated_bytes;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::collections::{HashMap, HashSet};
use std::convert::{TryFrom, TryInto};
use std::fmt;
use std::io::{BufRead, Write};
use std::iter::Peekable;
use std::slice::Iter;
use value::{
    create_string, manage, Closure, Function, InternedString, Native, NativeFn, ObjectRef,
    ObjectRoot, Upvalue, UpvalueLocation, Value,
};

mod compiler;
mod dis;
mod gc;
mod memory;
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
    Loop,
    Call,
    Closure,
    CloseUpvalue,
    Pop,
    GetLocal,
    SetLocal,
    GetGlobal,
    DefineGlobal,
    SetGlobal,
    GetUpvalue,
    SetUpvalue,
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
        loop {
            match self.new_lines.peek() {
                Some(&&(offs, _)) if offs < self.offset => self.new_lines.next(),
                Some(&&(offs, l)) if offs == self.offset => {
                    self.line = Some(l);
                    self.new_lines.next();
                    break;
                }
                _ => break,
            };
        }
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
            if offs > self.offset {
                break;
            }
            line = Some(n)
        }
        line
    }
}

pub struct CallFrame {
    closure: ObjectRoot<Closure>,
    ip_offset: usize,
    base: usize,
}

#[derive(Debug, Clone, Copy)]
pub enum CompileError {
    ParseError,
    TooManyConstants,
    TooManyLocals,
    DuplicateName,
    UninitializedLocal,
    TooFarToJump,
    TooFarToLoop,
    TooManyParameters,
    TooManyArguments,
    TooManyUpvalues,
    ReturnAtTopLevel,
}

#[derive(Debug, Clone)]
pub enum RuntimeError {
    UnknownOpcode,
    EndOfChunk,
    StackUnderflow,
    StackOverflow,
    TypeError(&'static str, String, bool),
    InvalidAddition(String, String),
    UndefinedVariable(String),
    NotCallable,
    WrongArity(usize, usize),
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
            CompileError::TooFarToLoop => write!(f, "Loop body too large."),
            CompileError::TooManyParameters => write!(f, "Can't have more than 255 parameters."),
            CompileError::TooManyArguments => write!(f, "Can't have more than 255 arguments."),
            CompileError::TooManyUpvalues => write!(f, "Too many closure variables in function."),
            CompileError::ReturnAtTopLevel => write!(f, "Can't return from top-level code."),
        }
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RuntimeError::UnknownOpcode => write!(f, "Unknown opcode."),
            RuntimeError::EndOfChunk => write!(f, "Unexpected end of chunk."),
            RuntimeError::StackUnderflow => write!(f, "Stack underflow."),
            RuntimeError::StackOverflow => write!(f, "Stack overflow."),
            RuntimeError::TypeError(t, v, _plural) => {
                #[cfg(not(feature = "lox_errors"))]
                {
                    return write!(f, "Expected a {} value but found: {}.", t, v);
                }
                #[cfg(feature = "lox_errors")]
                {
                    if *plural {
                        return write!(f, "Operands must be {}s.", t);
                    } else {
                        return write!(f, "Operand must be a {}.", t);
                    }
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
            RuntimeError::NotCallable => write!(f, "Can only call functions and classes."),
            RuntimeError::WrongArity(expect, actual) => {
                write!(f, "Expected {} arguments but got {}.", expect, actual)
            }
        }
    }
}

type CompilerResult = Result<Function, CompileError>;
type ValueResult = Result<Value, VMError>;
type InterpretResult = Result<(), VMError>;

pub struct VM {
    stack: Vec<Value>,
    objects: Vec<Box<dyn Trace>>,
    strings: HashSet<value::InternedString>,
    globals: HashMap<value::InternedString, Value>,
    frames: Vec<CallFrame>,
    open_upvalues: Vec<ObjectRef<Upvalue>>,
    next_gc: usize,
}

impl VM {
    fn new() -> Self {
        Self {
            stack: Vec::new(),
            objects: Vec::new(),
            strings: HashSet::new(),
            globals: HashMap::new(),
            frames: Vec::new(),
            open_upvalues: Vec::new(),
            next_gc: get_allocated_bytes() * 2,
        }
    }

    fn interpret_source(&mut self, source: &str) -> InterpretResult {
        let func = compiler::compile(source, self).map_err(VMError::CompileError)?;
        let oref = manage(self, func);
        let closure_ref = manage(self, Closure::new(oref));
        let closure_root = closure_ref.upgrade().unwrap();
        self.stack.push(Value::Function(closure_ref));
        self.call(closure_root, 0)?;
        let result = self.run();
        if let Err(VMError::RuntimeError(ref e)) = result {
            eprintln!("Runtime error: {}", e);
            for frame in self.frames.iter().rev() {
                let func_root = frame.closure.content.function.upgrade().unwrap().clone();
                // don't subtract 1 from the offset because if we hit an error, the offset
                // probably hasn't been updated anyway
                let ip = IP::new(&func_root.content.chunk, frame.ip_offset);
                if let Some(n) = ip.get_line() {
                    eprint!("[line {}] in ", n);
                } else {
                    eprint!("[unknown line] in ");
                }
                match &frame
                    .closure
                    .content
                    .function
                    .upgrade()
                    .unwrap()
                    .content
                    .name
                {
                    None => eprintln!("script"),
                    Some(oref) => eprintln!("{}()", oref.upgrade().unwrap().content),
                }
            }
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

    fn capture_upvalue(&mut self, slot: usize) -> ObjectRef<Upvalue> {
        let mut insertion_index = self.open_upvalues.len();
        for (i, uv) in self.open_upvalues.iter().enumerate().rev() {
            match *uv.upgrade().unwrap().content.location.borrow() {
                UpvalueLocation::Stack(index) => {
                    if index == slot {
                        return uv.clone();
                    } else if index < slot {
                        break;
                    }
                    insertion_index = i;
                }
                _ => unreachable!(),
            }
        }
        let new_uv = manage(self, Upvalue::new(UpvalueLocation::Stack(slot)));
        self.open_upvalues.insert(insertion_index, new_uv.clone());
        new_uv
    }

    fn close_upvalues(&mut self, last: usize) {
        loop {
            match self.open_upvalues.last() {
                None => {
                    return;
                }
                Some(uv_ref) => {
                    let uv_root = uv_ref.upgrade().unwrap();
                    let mut loc = uv_root.content.location.borrow_mut();
                    if let UpvalueLocation::Stack(index) = *loc {
                        if index < last {
                            return;
                        }
                        *loc = UpvalueLocation::Heap(self.stack[index].clone());
                        self.open_upvalues.pop();
                    }
                }
            }
        }
    }

    fn run(&mut self) -> InterpretResult {
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

        let mut func_root = self
            .frames
            .last()
            .unwrap()
            .closure
            .content
            .function
            .upgrade()
            .unwrap()
            .clone();
        let mut ip = IP::new(&func_root.content.chunk, 0);

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
                    " (heap: {}, strings: {}, bytes: {})",
                    self.objects.len(),
                    self.strings.len(),
                    crate::memory::get_allocated_bytes()
                );
                #[cfg(feature = "trace_globals")]
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
                        // this is a lot of effort to make one test pass
                        #[cfg(not(feature = "lox_errors"))]
                        {
                            let n: f64 = self.pop_stack()?.try_into()?;
                            self.stack.push((-n).into());
                        }
                        #[cfg(feature = "lox_errors")]
                        {
                            let n: f64 = self.pop_stack()?.try_into().map_err(|vme| match vme {
                                VMError::RuntimeError(RuntimeError::TypeError(ex, act, true)) => {
                                    VMError::RuntimeError(RuntimeError::TypeError(ex, act, false))
                                }
                                _ => vme,
                            })?;
                            self.stack.push((-n).into());
                        }
                    }
                    OpCode::Add => {
                        let a = self.pop_stack()?;
                        let b = self.pop_stack()?;
                        match (&a, &b) {
                            (Value::Number(a), Value::Number(b)) => self.stack.push((a + b).into()),
                            (Value::String(a), Value::String(b)) => {
                                let a = &a.upgrade().unwrap().content;
                                let b = &b.upgrade().unwrap().content;
                                let w = create_string(self, &format!("{}{}", b, a));
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
                    OpCode::Loop => {
                        let offset = ip.read_short() as usize;
                        ip.offset -= offset;
                    }
                    OpCode::Call => {
                        let arg_count = ip.read() as usize;
                        self.frames.last_mut().unwrap().ip_offset = ip.offset;
                        let old_frames = self.frames.len();
                        self.call_value(self.peek_stack(arg_count), arg_count)?;
                        if self.frames.len() > old_frames {
                            func_root = self
                                .frames
                                .last()
                                .unwrap()
                                .closure
                                .content
                                .function
                                .upgrade()
                                .unwrap()
                                .clone();
                            ip = IP::new(&func_root.content.chunk, 0);
                        }
                    }
                    OpCode::Return => {
                        let result = self.pop_stack()?;
                        let top = self.frames.last().unwrap().base;
                        self.close_upvalues(top);
                        self.frames.pop();
                        match self.frames.last() {
                            None => {
                                self.pop_stack()?;
                                return Ok(());
                            }
                            Some(frame) => {
                                self.stack.truncate(top);
                                self.stack.push(result);
                                func_root =
                                    frame.closure.content.function.upgrade().unwrap().clone();
                                ip = IP::new(&func_root.content.chunk, frame.ip_offset);
                            }
                        }
                    }
                    OpCode::Closure => {
                        let val = ip.read_constant();
                        if let Value::FunctionProto(function) = val {
                            let upvalue_count = function.upgrade().unwrap().content.upvalue_count;
                            let mut closure = Closure::new(function);
                            for _ in 0..upvalue_count {
                                let is_local = ip.read() != 0;
                                let index = ip.read() as usize;
                                if is_local {
                                    let frame_base = self.frames.last().unwrap().base;
                                    let uv = self.capture_upvalue(frame_base + index);
                                    closure.upvalues.push(uv);
                                } else {
                                    let frame = &self.frames.last().unwrap();
                                    let uv = frame.closure.content.upvalues[index].clone();
                                    closure.upvalues.push(uv);
                                }
                            }
                            let closure_val = Value::Function(manage(self, closure));
                            self.stack.push(closure_val);
                        }
                    }
                    OpCode::CloseUpvalue => {
                        self.close_upvalues(self.stack.len() - 1);
                        self.pop_stack()?;
                    }
                    OpCode::Pop => {
                        self.pop_stack()?;
                    }
                    OpCode::GetLocal => {
                        let slot = ip.read();
                        let frame = self.frames.last().unwrap();
                        self.stack
                            .push(self.stack[slot as usize + frame.base].clone());
                    }
                    OpCode::SetLocal => {
                        let slot = ip.read();
                        let frame = self.frames.last().unwrap();
                        self.stack[slot as usize + frame.base] = self.peek_stack(0).clone();
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
                    OpCode::GetUpvalue => {
                        let slot = ip.read() as usize;
                        let frame = &self.frames.last().unwrap();
                        match &*frame.closure.content.upvalues[slot]
                            .upgrade()
                            .unwrap()
                            .content
                            .location
                            .borrow()
                        {
                            UpvalueLocation::Stack(index) => {
                                self.stack.push(self.stack[*index].clone())
                            }
                            UpvalueLocation::Heap(value) => self.stack.push(value.clone()),
                        }
                    }
                    OpCode::SetUpvalue => {
                        let slot = ip.read() as usize;
                        let frame = &self.frames.last().unwrap();
                        let uv_root = frame.closure.content.upvalues[slot].upgrade().unwrap();
                        let mut loc = uv_root.content.location.borrow_mut();
                        match *loc {
                            UpvalueLocation::Stack(index) => self.stack[index] = self.peek_stack(0),
                            UpvalueLocation::Heap(_) => {
                                *loc = UpvalueLocation::Heap(self.peek_stack(0))
                            }
                        }
                    }
                },
                Err(_) => return rt(RuntimeError::UnknownOpcode),
            }
            self.frames.last_mut().unwrap().ip_offset = ip.offset;
            let current_bytes;
            #[cfg(not(feature = "stress_gc"))]
            {
                current_bytes = get_allocated_bytes();
            }
            #[cfg(feature = "stress_gc")]
            {
                current_bytes = self.next_gc;
            }
            if current_bytes >= self.next_gc {
                self.collect_garbage();
                self.next_gc = get_allocated_bytes() * 2;
            }
        }
    }

    fn call_value(&mut self, callee: Value, arg_count: usize) -> Result<(), VMError> {
        match callee {
            Value::Function(oref) => return self.call(oref.upgrade().unwrap(), arg_count),
            Value::Native(oref) => {
                let args: &[Value] = &self.stack[self.stack.len() - arg_count..];
                let result = (oref.upgrade().unwrap().content.function)(arg_count, args);
                self.stack.truncate(self.stack.len() - arg_count - 1);
                self.stack.push(result);
                Ok(())
            }
            _ => rt(RuntimeError::NotCallable),
        }
    }

    fn call(&mut self, closure: ObjectRoot<Closure>, arg_count: usize) -> Result<(), VMError> {
        let function = closure.content.function.upgrade().unwrap();
        if arg_count != function.content.arity {
            return rt(RuntimeError::WrongArity(function.content.arity, arg_count));
        }
        if self.frames.len() == 64 {
            return rt(RuntimeError::StackOverflow);
        }
        let frame = CallFrame {
            closure,
            ip_offset: 0,
            base: self.stack.len() - arg_count - 1,
        };
        self.frames.push(frame);
        Ok(())
    }

    fn define_native(&mut self, name: &str, function: NativeFn) {
        let interned = InternedString(create_string(self, name).upgrade().unwrap());
        let value = Value::Native(manage::<Native>(self, Native::new(function)));
        self.globals.insert(interned, value);
    }
}

fn clock() -> u128 {
    use std::time;
    time::SystemTime::now()
        .duration_since(time::UNIX_EPOCH)
        .unwrap()
        .as_millis()
}

fn clock_native(_arg_count: usize, _args: &[Value]) -> Value {
    Value::Number(clock() as f64)
}

fn main() {
    let mut vm = VM::new();
    vm.define_native("clock", clock_native);
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
