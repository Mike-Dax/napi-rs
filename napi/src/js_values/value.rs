use crate::sys;

use super::ValueType;

#[derive(Clone, Copy, Debug)]
pub struct Value {
  pub env: sys::napi_env,
  pub value: sys::napi_value,
  pub value_type: ValueType,
}
