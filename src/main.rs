use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::convert::TryFrom;

#[derive(IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
enum OpCode {
    Constant,
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

fn disassemble_instruction(chunk: &Chunk, offset: usize, line: Option<LineNo>) -> usize {
    match line {
        None => print!("    | {:04} ", offset),
        Some(l) => print!("{:5} {:04} ", l, offset),
    }
    let byte = chunk.code[offset];
    match OpCode::try_from(byte) {
        Ok(instruction) => match instruction {
            OpCode::Constant => constant_instruction("CONSTANT", chunk, offset),
            OpCode::Return => simple_instruction("RETURN", offset),
        },
        Err(_) => {
            println!("Unknown opcode {}", byte);
            offset + 1
        }
    }
}

fn simple_instruction(name: &str, offset: usize) -> usize {
    println!("{}", name);
    offset + 1
}

fn constant_instruction(name: &str, chunk: &Chunk, offset: usize) -> usize {
    let constant_index = chunk.code[offset + 1];
    print!("{:<16} {:<4} ", name, constant_index);
    println!("{}", chunk.constants[constant_index as usize]);
    offset + 2
}

fn disassemble_chunk(chunk: &Chunk, name: &str) {
    println!("== {} ==", name);
    let mut new_lines = chunk.lines.iter().peekable();
    let mut offset = 0;
    while offset < chunk.code.len() {
        let line = match new_lines.peek() {
            Some(&&(offs, l)) if offs == offset => {
                new_lines.next();
                Some(l)
            }
            _ => None,
        };
        offset = disassemble_instruction(&chunk, offset, line);
    }
}

fn main() {
    let mut chunk = Chunk::new();
    let constant_index = chunk.add_constant(1.2);
    chunk.write(OpCode::Constant.into(), 122);
    chunk.write(constant_index, 122);
    chunk.write(OpCode::Return.into(), 122);
    chunk.write(OpCode::Return.into(), 123);
    disassemble_chunk(&chunk, "test chunk");
}
