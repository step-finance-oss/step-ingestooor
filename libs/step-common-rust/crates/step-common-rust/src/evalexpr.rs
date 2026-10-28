use chrono::NaiveDateTime;
use rust_decimal::Decimal;
use std::ops::Deref;
use std::str::FromStr;

use evalexpr::{EvalexprResult, Value};

pub fn add_common_functions(
    context: &mut impl evalexpr::ContextWithMutableFunctions,
) -> EvalexprResult<()> {
    //add a startsWith function
    evalexpr::ContextWithMutableFunctions::set_function(
        context,
        "step::startsWith".to_string(),
        evalexpr::Function::new(|args| {
            let args = evalexpr::Value::as_fixed_len_tuple(args, 2)?;
            if args[0].is_empty() && args[1].is_empty() {
                return Ok(evalexpr::Value::Boolean(true));
            } else if args[0].is_empty() || args[1].is_empty() {
                return Ok(evalexpr::Value::Boolean(false));
            }
            let arg1 = args[0].as_string()?.to_uppercase();
            let arg2 = args[1].as_string()?.to_uppercase();
            Ok(evalexpr::Value::Boolean(arg1.starts_with(&arg2)))
        }),
    )?;

    //add a endsWith function
    evalexpr::ContextWithMutableFunctions::set_function(
        context,
        "step::endsWith".to_string(),
        evalexpr::Function::new(|args| {
            let args = evalexpr::Value::as_fixed_len_tuple(args, 2)?;
            if args[0].is_empty() && args[1].is_empty() {
                return Ok(evalexpr::Value::Boolean(true));
            } else if args[0].is_empty() || args[1].is_empty() {
                return Ok(evalexpr::Value::Boolean(false));
            }
            let arg1 = args[0].as_string()?.to_uppercase();
            let arg2 = args[1].as_string()?.to_uppercase();
            Ok(evalexpr::Value::Boolean(arg1.ends_with(&arg2)))
        }),
    )?;

    //add a string_contains function
    evalexpr::ContextWithMutableFunctions::set_function(
        context,
        "step::contains".to_string(),
        evalexpr::Function::new(|args| {
            let args = evalexpr::Value::as_fixed_len_tuple(args, 2)?;
            if args[0].is_empty() && args[1].is_empty() {
                return Ok(evalexpr::Value::Boolean(true));
            } else if args[0].is_empty() || args[1].is_empty() {
                return Ok(evalexpr::Value::Boolean(false));
            }
            let arg1 = args[0].as_string()?.to_uppercase();
            let arg2 = args[1].as_string()?.to_uppercase();
            Ok(evalexpr::Value::Boolean(arg1.contains(&arg2)))
        }),
    )?;
    Ok(())
}

pub struct ValueWrapper(Value);

impl ValueWrapper {
    pub fn unwrap(self) -> Value {
        self.0
    }

    pub fn new(value: Value) -> Self {
        ValueWrapper(value)
    }
}

impl Deref for ValueWrapper {
    type Target = Value;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<String> for ValueWrapper {
    fn from(s: String) -> Self {
        ValueWrapper(Value::String(s))
    }
}

impl From<&str> for ValueWrapper {
    fn from(s: &str) -> Self {
        ValueWrapper(Value::String(s.to_string()))
    }
}

impl From<i8> for ValueWrapper {
    fn from(i: i8) -> Self {
        ValueWrapper(Value::Int(i.into()))
    }
}

impl From<i16> for ValueWrapper {
    fn from(i: i16) -> Self {
        ValueWrapper(Value::Int(i.into()))
    }
}

impl From<i32> for ValueWrapper {
    fn from(i: i32) -> Self {
        ValueWrapper(Value::Int(i.into()))
    }
}

impl From<i64> for ValueWrapper {
    fn from(i: i64) -> Self {
        ValueWrapper(i.into())
    }
}

impl From<u8> for ValueWrapper {
    fn from(i: u8) -> Self {
        ValueWrapper(Value::Int(i.into()))
    }
}

impl From<u16> for ValueWrapper {
    fn from(i: u16) -> Self {
        ValueWrapper(Value::Int(i.into()))
    }
}

impl From<u32> for ValueWrapper {
    fn from(i: u32) -> Self {
        ValueWrapper(Value::Int(i.into()))
    }
}

impl From<u64> for ValueWrapper {
    fn from(i: u64) -> Self {
        if i <= i64::MAX as u64 {
            ValueWrapper(evalexpr::Value::Int(i as i64))
        } else {
            ValueWrapper(evalexpr::Value::Int(i64::MAX))
        }
    }
}

impl From<u128> for ValueWrapper {
    fn from(i: u128) -> Self {
        ValueWrapper(evalexpr::Value::String(i.to_string()))
    }
}

impl From<f32> for ValueWrapper {
    fn from(f: f32) -> Self {
        ValueWrapper(Value::Float(f.into()))
    }
}

impl From<f64> for ValueWrapper {
    fn from(f: f64) -> Self {
        ValueWrapper(f.into())
    }
}

impl From<bool> for ValueWrapper {
    fn from(b: bool) -> Self {
        ValueWrapper(Value::Boolean(b))
    }
}

impl<T> From<Option<T>> for ValueWrapper
where
    T: Into<ValueWrapper>,
{
    fn from(o: Option<T>) -> Self {
        match o {
            Some(v) => v.into(),
            None => ValueWrapper(Value::Empty),
        }
    }
}

impl<T> From<&T> for ValueWrapper
where
    T: Into<ValueWrapper> + Clone,
{
    fn from(t: &T) -> Self {
        t.clone().into()
    }
}

impl<T> From<Vec<T>> for ValueWrapper
where
    T: Into<ValueWrapper>,
{
    fn from(v: Vec<T>) -> Self {
        ValueWrapper(Value::Tuple(v.into_iter().map(|i| i.into().0).collect()))
    }
}

impl From<Decimal> for ValueWrapper {
    fn from(d: Decimal) -> Self {
        let s = d.to_string();
        //should be safe to unwrap here
        let f: f64 = f64::from_str(&s).unwrap_or_default();
        ValueWrapper(f.into())
    }
}

impl From<NaiveDateTime> for ValueWrapper {
    fn from(d: NaiveDateTime) -> Self {
        ValueWrapper(d.and_utc().timestamp().into())
    }
}
