use crate::{RuntimeError, VMError};
use std::convert::TryFrom;
use std::fmt;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ValueType {
    Bool,
    Nil,
    Number,
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
            }
        )
    }
}

// As Value gets more complicated, need to check whether it can still be Copy
#[derive(Copy, Clone)]
pub enum Value {
    Bool(bool),
    Nil,
    Number(f64),
}

impl Value {
    fn get_type(&self) -> ValueType {
        match self {
            Value::Bool(_) => ValueType::Bool,
            Value::Nil => ValueType::Nil,
            Value::Number(_) => ValueType::Number,
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

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool(b) => write!(f, "{}", b),
            Self::Nil => write!(f, "nil"),
            Self::Number(n) => write!(f, "{}", n),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Bool(a), Value::Bool(b)) => (a == b),
            (Value::Nil, Value::Nil) => true,
            (Value::Number(a), Value::Number(b)) => (a == b),
            _ => false,
        }
    }
}