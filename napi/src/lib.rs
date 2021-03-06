//! High level NodeJS [N-API](https://nodejs.org/api/n-api.html) binding
//!
//! **napi-rs** provides minimal overhead to write N-API modules in `Rust`.
//! ## Feature flags
//! ### libuv
//! With `libuv` feature, you can execute a rust future in `libuv` in NodeJS, and return a `promise` object.
//! ```
//! use std::thread;
//! use std::fs;
//!
//! use futures::prelude::*;
//! use futures::channel::oneshot;
//! use napi::{CallContext, Result, JsString, JsObject, Status, Error};
//!
//! #[js_function(1)]
//! pub fn uv_read_file(ctx: CallContext) -> Result<JsObject> {
//!   let path = ctx.get::<JsString>(0)?;
//!   let (sender, receiver) = oneshot::channel();
//!   let p = path.as_str()?.to_owned();
//!   thread::spawn(|| {
//!     let res = fs::read(p).map_err(|e| Error::new(Status::Unknown, format!("{}", e)));
//!     sender.send(res).expect("Send data failed");
//!   });
//!   ctx.env.execute(receiver.map_err(|e| Error::new(Status::Unknown, format!("{}", e))).map(|x| x.and_then(|x| x)), |&mut env, data| {
//!     env.create_buffer_with_data(data)
//!   })
//! }
//! ```
//! ### tokio_rt
//! With `tokio_rt` feature, `napi-rs` provides a ***tokio runtime*** in an additional thread.
//! And you can easily run tokio `future` in it and return `promise`.
//!
//! ```
//! use futures::prelude::*;
//! use napi::{CallContext, Error, JsObject, JsString, Result, Status};
//! use tokio;
//!
//! #[js_function(1)]
//! pub fn tokio_readfile(ctx: CallContext) -> Result<JsObject> {
//!   let js_filepath = ctx.get::<JsString>(0)?;
//!   let path_str = js_filepath.as_str()?;
//!   ctx.env.execute_tokio_future(
//!     tokio::fs::read(path_str.to_owned())
//!       .map(|v| v.map_err(|e| Error::new(Status::Unknown, format!("failed to read file, {}", e)))),
//!     |&mut env, data| env.create_buffer_with_data(data),
//!   )
//! }
//! ```
//!
//! ***Tokio channel in `napi-rs` buffer size is default `100`.***
//!
//! ***You can adjust it via `NAPI_RS_TOKIO_CHANNEL_BUFFER_SIZE` environment variable***
//!
//! ```
//! NAPI_RS_TOKIO_CHANNEL_BUFFER_SIZE=1000 node ./app.js
//! ```
//!

mod async_work;
mod call_context;
mod env;
mod error;
mod js_values;
mod module;
#[cfg(all(feature = "libuv", napi4))]
mod promise;
mod status;
mod task;
#[cfg(napi4)]
pub mod threadsafe_function;
#[cfg(all(feature = "tokio_rt", napi4))]
mod tokio_rt;
#[cfg(all(feature = "libuv", napi4))]
mod uv;
mod version;

pub use napi_sys as sys;

pub use call_context::CallContext;
pub use env::*;
pub use error::{Error, Result};
pub use js_values::*;
pub use module::Module;
pub use status::Status;
pub use task::Task;
pub use version::NodeVersion;

#[cfg(all(feature = "tokio_rt", napi4))]
pub use tokio_rt::shutdown as shutdown_tokio_rt;

/// register nodejs module
///
/// ## Example
/// ```
/// register_module!(test_module, init);
///
/// fn init(module: &mut Module) -> Result<()> {
///   module.create_named_method("nativeFunction", native_function)?;
/// }
/// ```
#[macro_export]
macro_rules! register_module {
  ($module_name:ident, $init:ident) => {
    #[inline]
    #[cfg(all(feature = "tokio_rt", napi4))]
    fn check_status(code: $crate::sys::napi_status) -> Result<()> {
      use $crate::{Error, Status};
      let status = Status::from(code);
      match status {
        Status::Ok => Ok(()),
        _ => Err(Error::from_status(status)),
      }
    }
    #[no_mangle]
    #[cfg_attr(target_os = "linux", link_section = ".ctors")]
    #[cfg_attr(target_os = "macos", link_section = "__DATA,__mod_init_func")]
    #[cfg_attr(target_os = "windows", link_section = ".CRT$XCU")]
    pub static __REGISTER_MODULE: extern "C" fn() = {
      use std::ffi::CString;
      use std::io::Write;
      use std::os::raw::c_char;
      use std::ptr;
      use $crate::{sys, Env, JsObject, Module, NapiValue};

      #[cfg(all(feature = "tokio_rt", napi4))]
      use $crate::shutdown_tokio_rt;

      extern "C" fn register_module() {
        static mut MODULE_DESCRIPTOR: Option<sys::napi_module> = None;
        unsafe {
          MODULE_DESCRIPTOR = Some(sys::napi_module {
            nm_version: 1,
            nm_flags: 0,
            nm_filename: concat!(file!(), "\0").as_ptr() as *const c_char,
            nm_register_func: Some(init_module),
            nm_modname: concat!(stringify!($module_name), "\0").as_ptr() as *const c_char,
            nm_priv: 0 as *mut _,
            reserved: [0 as *mut _; 4],
          });

          sys::napi_module_register(MODULE_DESCRIPTOR.as_mut().unwrap() as *mut sys::napi_module);
        }

        extern "C" fn init_module(
          raw_env: sys::napi_env,
          raw_exports: sys::napi_value,
        ) -> sys::napi_value {
          let env = Env::from_raw(raw_env);
          let mut exports: JsObject = JsObject::from_raw_unchecked(raw_env, raw_exports);
          let mut cjs_module = Module { env, exports };
          let result = $init(&mut cjs_module);

          #[cfg(all(feature = "tokio_rt", napi4))]
          let hook_result = check_status(unsafe {
            sys::napi_add_env_cleanup_hook(raw_env, Some(shutdown_tokio_rt), ptr::null_mut())
          });

          #[cfg(not(all(feature = "tokio_rt", napi4)))]
          let hook_result = Ok(());

          match hook_result.and_then(move |_| result) {
            Ok(_) => exports.raw_value(),
            Err(e) => {
              unsafe {
                sys::napi_throw_error(
                  raw_env,
                  ptr::null(),
                  CString::from_vec_unchecked(format!("Error initializing module: {}", e).into())
                    .as_ptr() as *const _,
                )
              };
              ptr::null_mut()
            }
          }
        }
      }

      register_module
    };
  };
}
