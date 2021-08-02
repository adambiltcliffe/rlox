use crate::{RuntimeError, VMError, VM};
use std::convert::TryFrom;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc;

pub type ObjectRoot<T> = rc::Rc<HeapEntry<T>>;
pub type ObjectRef<T> = rc::Weak<HeapEntry<T>>;

#[derive(Clone)]
pub enum Value {
    Bool(bool),
    Nil,
    Number(f64),
    String(ObjectRef<String>),
}

impl Value {
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

impl From<ObjectRef<String>> for Value {
    fn from(w: rc::Weak<HeapEntry<String>>) -> Self {
        Value::String(w)
    }
}

impl TryFrom<Value> for bool {
    type Error = VMError;
    fn try_from(v: Value) -> Result<Self, Self::Error> {
        match v {
            Value::Bool(b) => Ok(b),
            _ => Err(VMError::RuntimeError(RuntimeError::TypeError(
                "bool",
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
                "number",
                v.to_string(),
            ))),
        }
    }
}

impl TryFrom<Value> for String {
    type Error = VMError;
    fn try_from(v: Value) -> Result<Self, Self::Error> {
        if let Value::String(ref obj) = v {
            let s = &obj.upgrade().unwrap().content;
            return Ok(s.clone());
        }
        Err(VMError::RuntimeError(RuntimeError::TypeError(
            "string",
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
            Self::String(obj) => write!(f, "{}", format_string(obj)),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Bool(a), Value::Bool(b)) => (a == b),
            (Value::Nil, Value::Nil) => true,
            (Value::Number(a), Value::Number(b)) => (a == b),
            // Value equality is pointer equality for interned strings
            (Value::String(a), Value::String(b)) => rc::Weak::ptr_eq(a, b),
            _ => false,
        }
    }
}

pub struct HeapEntry<T> {
    pub content: T,
}

pub fn create_string(vm: &mut VM, s: &str) -> ObjectRef<String> {
    use rc::Rc;
    match vm.strings.get(s) {
        Some(InternedString(oroot)) => Rc::downgrade(oroot),
        None => {
            let entry = HeapEntry::<String> {
                content: s.to_owned(),
            };
            let oroot = Rc::new(entry);
            let oref = Rc::downgrade(&oroot);
            let interned = InternedString(Rc::clone(&oroot));
            vm.strings.insert(interned);
            vm.objects.push(Box::new(oroot));
            oref
        }
    }
}

pub fn format_string(w: &ObjectRef<String>) -> String {
    let c = &w.upgrade().unwrap().content;
    format!("\"{}\"", c).to_owned()
}

pub fn printable_value(v: Value) -> String {
    if let Value::String(oref) = &v {
        let s = &oref.upgrade().unwrap().content;
        return format!("{}", s).to_owned();
    }
    format!("{}", v)
}

pub struct InternedString(ObjectRoot<String>);

impl Hash for InternedString {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.0.content.hash(h);
    }
}

impl PartialEq for InternedString {
    fn eq(&self, other: &Self) -> bool {
        &self.0.content == &other.0.content
    }
}

impl Eq for InternedString {}

impl std::borrow::Borrow<str> for InternedString {
    fn borrow(&self) -> &str {
        self.0.content.borrow()
    }
}

impl TryFrom<Value> for InternedString {
    type Error = VMError;
    fn try_from(v: Value) -> Result<Self, Self::Error> {
        match v {
            Value::String(oref) => Ok(Self(oref.upgrade().unwrap())),
            _ => Err(VMError::RuntimeError(RuntimeError::TypeError(
                "string",
                v.to_string(),
            ))),
        }
    }
}

impl fmt::Display for InternedString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.content)
    }
}

pub trait Trace {}

impl<T> Trace for ObjectRoot<T> {}
