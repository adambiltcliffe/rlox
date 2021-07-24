use crate::{VM, RuntimeError, VMError};
use std::convert::TryFrom;
use std::fmt;
use std::rc;

pub type ObjectRoot = rc::Rc<HeapEntry>;
pub type ObjectRef = rc::Weak<HeapEntry>;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ValueType {
    Bool,
    Nil,
    Number,
    String,
}

impl fmt::Display for ValueType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Bool => "bool",
                Self::Nil => "nil",
                Self::Number => "number",
                Self::String => "string",
            }
        )
    }
}

#[derive(Clone)]
pub enum Value {
    Bool(bool),
    Nil,
    Number(f64),
    Object(ObjectRef),
}

impl Value {
    pub fn get_type(&self) -> ValueType {
        match self {
            Value::Bool(_) => ValueType::Bool,
            Value::Nil => ValueType::Nil,
            Value::Number(_) => ValueType::Number,
            Value::Object(entry) => entry.upgrade().unwrap().get_type(),
        }
    }
    fn is_bool(&self) -> bool {
        match self {
            Value::Bool(_) => true,
            _ => false,
        }
    }
    fn is_nil(&self) -> bool {
        match self {
            Value::Nil => true,
            _ => false,
        }
    }
    fn is_number(&self) -> bool {
        match self {
            Value::Number(_) => true,
            _ => false,
        }
    }
    // We only want to do this explicitly which is why it's not a From impl
    pub fn is_falsey(&self) -> bool {
        match self {
            Value::Nil => true,
            Value::Bool(b) => !b,
            _ => false,
        }
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}

impl From<f64> for Value {
    fn from(n: f64) -> Self {
        Value::Number(n)
    }
}

impl From<ObjectRef> for Value {
    fn from(w: rc::Weak<HeapEntry>) -> Self {
        Value::Object(w)
    }
}

impl TryFrom<Value> for bool {
    type Error = VMError;
    fn try_from(v: Value) -> Result<Self, Self::Error> {
        match v {
            Value::Bool(b) => Ok(b),
            _ => Err(VMError::RuntimeError(RuntimeError::TypeError(
                ValueType::Bool,
                v.to_string(),
            ))),
        }
    }
}

impl TryFrom<Value> for f64 {
    type Error = VMError;
    fn try_from(v: Value) -> Result<Self, Self::Error> {
        match v {
            Value::Number(n) => Ok(n),
            _ => Err(VMError::RuntimeError(RuntimeError::TypeError(
                ValueType::Number,
                v.to_string(),
            ))),
        }
    }
}

impl TryFrom<Value> for String {
    type Error = VMError;
    fn try_from(v: Value) -> Result<Self, Self::Error> {
        if let Value::Object(ref obj) = v {
            #[allow(irrefutable_let_patterns)]
            if let Object::String(s) = &obj.upgrade().unwrap().content {
                return Ok(s.clone());
            }
        }
        Err(VMError::RuntimeError(RuntimeError::TypeError(
            ValueType::String,
            v.to_string(),
        )))
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool(b) => write!(f, "{}", b),
            Self::Nil => write!(f, "nil"),
            Self::Number(n) => write!(f, "{}", n),
            Self::Object(obj) => write!(f, "{}", format_obj(obj)),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Bool(a), Value::Bool(b)) => (a == b),
            (Value::Nil, Value::Nil) => true,
            (Value::Number(a), Value::Number(b)) => (a == b),
            (Value::Object(a), Value::Object(b)) => {
                let a = &a.upgrade().unwrap().content;
                let b = &b.upgrade().unwrap().content;
                match (a, b) {
                    (Object::String(x), Object::String(y)) => x == y,
                }
            }
            _ => false,
        }
    }
}

pub struct HeapEntry {
    content: Object,
}

impl HeapEntry {
    pub fn get_type(&self) -> ValueType {
        match self.content {
            Object::String(_) => ValueType::String,
        }
    }

    pub fn create_string(vm: &mut VM, s: &str) -> ObjectRef {
        let entry = Self {
            content: Object::String(s.to_owned()),
        };
        let oroot = rc::Rc::new(entry);
        let oref = rc::Rc::downgrade(&oroot);
        vm.objects.push(oroot);
        oref
    }
}

enum Object {
    String(String),
}

fn format_obj(w: &ObjectRef) -> String {
    match &w.upgrade().unwrap().content {
        Object::String(s) => s.clone(),
    }
}
