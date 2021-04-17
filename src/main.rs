use num_enum::{IntoPrimitive, TryFromPrimitive};

mod dis;

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

fn main() {
    let mut chunk = Chunk::new();
    let constant_index = chunk.add_constant(1.2);
    chunk.write(OpCode::Constant.into(), 122);
    chunk.write(constant_index, 122);
    chunk.write(OpCode::Return.into(), 122);
    chunk.write(OpCode::Return.into(), 123);
    dis::disassemble_chunk(&chunk, "test chunk");
}
