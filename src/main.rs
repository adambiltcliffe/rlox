use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::convert::TryFrom;

#[derive(IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
enum OpCode {
    Return,
}

type Chunk = Vec<u8>;

fn disassemble_instruction(chunk: &Chunk, offset: usize) -> usize {
    print!("{:04} ", offset);
    let byte = chunk[offset];
    match OpCode::try_from(byte) {
        Ok(instruction) => match instruction {
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

fn disassemble_chunk(chunk: &Chunk, name: &str) {
    println!("== {} ==", name);
    let mut offset = 0;
    while offset < chunk.len() {
        offset = disassemble_instruction(&chunk, offset);
    }
}

fn main() {
    let mut chunk: Chunk = Vec::new();
    chunk.push(OpCode::Return.into());
    disassemble_chunk(&chunk, "test chunk");
}
