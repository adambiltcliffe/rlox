use crate::{Chunk, LineNo, OpCode, TracingIP};
use std::convert::TryFrom;

pub(crate) fn disassemble_instruction(chunk: &Chunk, offset: usize, line: Option<LineNo>) -> usize {
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

pub(crate) fn disassemble_chunk(chunk: &Chunk, name: &str) {
    println!("== {} ==", name);
    let mut ip = TracingIP::new(chunk, 0);
    while ip.valid() {
        // This is a hairy mess
        let _instruction = ip.read();
        let new_offset = disassemble_instruction(chunk, ip.offset - 1, ip.line);
        while ip.offset < new_offset {
            let _ = ip.read();
        }
    }
}
